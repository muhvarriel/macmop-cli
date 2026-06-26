use anyhow::Result;
use macmop::core::{AppContext, AppPaths, ExecutionMode, OutputFormat};
use macmop::modules::privacy;
use std::fs;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

struct TestEnv {
    home: PathBuf,
    quicklook_dir: PathBuf,
    no_read_dir: PathBuf,
    _base: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let unique_id = macmop::core::unix_now();
        let base = std::env::temp_dir().join(format!(
            "macmop-test-privacy-{}-{}-{}",
            test_name,
            unique_id,
            std::process::id()
        ));
        let home = base.join("home");
        let quicklook_dir = base.join("quicklook");
        let no_read_dir = base.join("no_read_dir");

        fs::create_dir_all(home.join("Library/Caches/com.apple.Safari")).unwrap();
        fs::create_dir_all(home.join("Library/Application Support/com.apple.sharedfilelist"))
            .unwrap();
        fs::create_dir_all(&quicklook_dir).unwrap();
        fs::create_dir_all(&no_read_dir).unwrap();

        Self {
            home,
            quicklook_dir,
            no_read_dir,
            _base: base,
        }
    }

    fn ctx(&self) -> AppContext {
        let mut ctx = AppContext::load(
            None,
            ExecutionMode::DryRun,
            OutputFormat::Json,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        ctx.paths = AppPaths {
            home: self.home.clone(),
            data_dir: self._base.join("data"),
            trash: self._base.join("trash"),
            audit_file: self._base.join("data/audit.json"),
            rollback_file: self._base.join("data/rollback.json"),
            apps_dirs: vec![],
            startup_dirs: vec![],
            quicklook_dirs: vec![self.quicklook_dir.clone(), self.no_read_dir.clone()],
        };
        ctx
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(&self.no_read_dir, fs::Permissions::from_mode(0o755));
        }
        let _ = fs::remove_dir_all(&self._base);
    }
}

#[test]
fn test_privacy_scan_all_artifacts() -> Result<()> {
    let env = TestEnv::new("scan_all");

    // Create Safari cache contents
    fs::write(
        env.home.join("Library/Caches/com.apple.Safari/Cache.db"),
        "safari-data",
    )
    .unwrap();
    // Create Shell history
    fs::write(env.home.join(".zsh_history"), "sensitive-command-here").unwrap();

    let ctx = env.ctx();
    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Scan,
        },
    )?;

    let payload = res.payload;
    let findings = payload.get("findings").unwrap().as_array().unwrap();

    // Verify browser cache & shell history detected
    let has_safari = findings.iter().any(|f| {
        f.get("category").unwrap().as_str().unwrap() == "browser_cache"
            && f.get("detail")
                .unwrap()
                .as_str()
                .unwrap()
                .contains("Safari")
    });
    let has_shell = findings
        .iter()
        .any(|f| f.get("category").unwrap().as_str().unwrap() == "shell_history");

    assert!(has_safari);
    assert!(has_shell);

    Ok(())
}

#[test]
fn test_privacy_browsers_subcommand() -> Result<()> {
    let env = TestEnv::new("browsers_sub");
    fs::write(
        env.home.join("Library/Caches/com.apple.Safari/Cache.db"),
        "safari-data",
    )
    .unwrap();
    fs::write(env.home.join(".zsh_history"), "sensitive-command-here").unwrap();

    let ctx = env.ctx();
    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Browsers,
        },
    )?;

    let findings = res.payload.get("findings").unwrap().as_array().unwrap();
    let has_safari = findings
        .iter()
        .any(|f| f.get("category").unwrap().as_str().unwrap() == "browser_cache");
    let has_shell = findings
        .iter()
        .any(|f| f.get("category").unwrap().as_str().unwrap() == "shell_history");

    assert!(has_safari);
    assert!(!has_shell);

    Ok(())
}

#[test]
fn test_privacy_recent_subcommand() -> Result<()> {
    let env = TestEnv::new("recent_sub");
    fs::write(
        env.home.join("Library/Caches/com.apple.Safari/Cache.db"),
        "safari-data",
    )
    .unwrap();
    fs::write(env.home.join(".zsh_history"), "sensitive-command-here").unwrap();

    let ctx = env.ctx();
    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Recent,
        },
    )?;

    let findings = res.payload.get("findings").unwrap().as_array().unwrap();
    let has_safari = findings
        .iter()
        .any(|f| f.get("category").unwrap().as_str().unwrap() == "browser_cache");
    let has_shell = findings
        .iter()
        .any(|f| f.get("category").unwrap().as_str().unwrap() == "shell_history");
    let has_recent = findings
        .iter()
        .any(|f| f.get("category").unwrap().as_str().unwrap() == "recent_items");

    assert!(!has_safari);
    assert!(has_shell);
    assert!(has_recent);

    Ok(())
}

#[test]
fn test_privacy_all_findings_are_report_only() -> Result<()> {
    let env = TestEnv::new("report_only");
    fs::write(env.home.join(".zsh_history"), "content").unwrap();

    let ctx = env.ctx();
    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Scan,
        },
    )?;

    let findings = res.payload.get("findings").unwrap().as_array().unwrap();
    for finding in findings {
        assert_eq!(
            finding.get("action").unwrap().as_str().unwrap(),
            "report_only"
        );
    }

    Ok(())
}

#[test]
fn test_privacy_json_schema_version_is_stable() -> Result<()> {
    let env = TestEnv::new("schema");
    let ctx = env.ctx();
    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Scan,
        },
    )?;

    assert_eq!(res.schema_version, "1.0");
    assert_eq!(res.command, "privacy scan");
    assert_eq!(res.mode, "dry_run");

    Ok(())
}

#[test]
fn test_shell_history_content_is_not_included_in_output() -> Result<()> {
    let env = TestEnv::new("no_content_leak");
    fs::write(env.home.join(".zsh_history"), "super-secret-password-12345").unwrap();

    let ctx = env.ctx();
    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Scan,
        },
    )?;

    let serialized = serde_json::to_string(&res)?;
    assert!(!serialized.contains("super-secret-password-12345"));

    Ok(())
}

#[test]
fn test_permission_denied_privacy_path_becomes_warning() -> Result<()> {
    let env = TestEnv::new("perms");

    // Create a file in no_read_dir, then lock it down
    let locked_file = env.no_read_dir.join("locked.txt");
    fs::write(&locked_file, "secrets").unwrap();

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&env.no_read_dir).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&env.no_read_dir, perms).unwrap();
    }

    let ctx = env.ctx();
    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Scan,
        },
    )?;

    #[cfg(unix)]
    {
        let warnings = res.payload.get("warnings").unwrap().as_array().unwrap();
        assert!(warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("permission denied")));
    }

    Ok(())
}
