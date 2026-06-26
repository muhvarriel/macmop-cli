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
            cloud_dirs: vec![],
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

#[test]
fn test_privacy_scan_apply_is_blocked() -> Result<()> {
    let env = TestEnv::new("scan_apply_blocked");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Scan,
        },
    );

    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("privacy scan --apply is not supported"));
    Ok(())
}

#[test]
fn test_privacy_permanent_blocked() -> Result<()> {
    let env = TestEnv::new("perm_blocked");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Permanent { force: true };

    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Browsers,
        },
    );

    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Privacy module does not support permanent delete yet"));
    Ok(())
}

#[test]
fn test_privacy_cleanup_browsers_apply() -> Result<()> {
    let env = TestEnv::new("browsers_apply");

    let chrome_cache = env.home.join("Library/Caches/Google/Chrome");
    fs::create_dir_all(&chrome_cache)?;
    fs::write(chrome_cache.join("Cache.db"), "data")?;

    // Create a mock chrome Application Support cookie path to ensure it is NOT cleaned
    let chrome_support = env
        .home
        .join("Library/Application Support/Google/Chrome/Default");
    fs::create_dir_all(&chrome_support)?;
    let cookie_file = chrome_support.join("Cookies");
    fs::write(&cookie_file, "chrome-cookies")?;

    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Browsers,
        },
    )?;

    assert_eq!(res.payload["execution"].as_str().unwrap(), "executed");
    assert_eq!(res.payload["moved_count"].as_u64().unwrap(), 2);

    // Cache should be gone (moved to Trash)
    assert!(!chrome_cache.exists());

    // Cookies file must be completely untouched!
    assert!(cookie_file.exists());

    Ok(())
}

#[test]
fn test_privacy_cleanup_recent_apply_and_excludes_shell_history() -> Result<()> {
    let env = TestEnv::new("recent_apply");

    let finder_plist = env.home.join("Library/Preferences/com.apple.finder.plist");
    fs::create_dir_all(finder_plist.parent().unwrap())?;
    fs::write(&finder_plist, "data")?;

    let shell_hist = env.home.join(".zsh_history");
    fs::write(&shell_hist, "sensitive history")?;

    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Recent,
        },
    )?;

    assert_eq!(res.payload["execution"].as_str().unwrap(), "executed");
    assert_eq!(res.payload["moved_count"].as_u64().unwrap(), 2);

    // Finder plist should be gone (moved to Trash)
    assert!(!finder_plist.exists());

    // Shell history file must be completely untouched!
    assert!(shell_hist.exists());

    Ok(())
}

#[test]
fn test_privacy_cleanup_rollback() -> Result<()> {
    let env = TestEnv::new("rollback_test");

    let chrome_cache = env.home.join("Library/Caches/Google/Chrome");
    fs::create_dir_all(&chrome_cache)?;
    fs::write(chrome_cache.join("Cache.db"), "data")?;

    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Browsers,
        },
    )?;

    assert_eq!(res.payload["execution"].as_str().unwrap(), "executed");
    let rollback_id = res.payload["rollback_id"].as_str().unwrap().to_string();

    // Call rollback
    let roll_res = macmop::modules::rollback::run(
        &ctx,
        macmop::cli::RollbackArgs {
            command: macmop::cli::RollbackCommand::Apply {
                id: rollback_id.clone(),
            },
        },
    )?;
    assert!(roll_res.payload["applied"].as_bool().unwrap());

    // Restored!
    assert!(chrome_cache.exists());

    // Second rollback fails cleanly
    let roll_res2 = macmop::modules::rollback::run(
        &ctx,
        macmop::cli::RollbackArgs {
            command: macmop::cli::RollbackCommand::Apply { id: rollback_id },
        },
    );
    assert!(roll_res2.is_err());
    assert!(roll_res2
        .unwrap_err()
        .to_string()
        .contains("rollback id not found"));

    Ok(())
}

#[test]
fn test_privacy_cleanup_revalidation_policy_check() -> Result<()> {
    let env = TestEnv::new("policy_reval");

    let chrome_cache = env.home.join("Library/Caches/Google/Chrome");
    fs::create_dir_all(&chrome_cache)?;
    fs::write(chrome_cache.join("Cache.db"), "data")?;

    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;
    // Add chrome cache to custom protected paths so it gets blocked immediately before execution
    ctx.custom_protected_paths = vec![chrome_cache.clone()];

    let res = privacy::run(
        &ctx,
        macmop::cli::PrivacyArgs {
            command: macmop::cli::PrivacyCommand::Browsers,
        },
    )?;

    assert_eq!(res.payload["execution"].as_str().unwrap(), "executed");
    assert_eq!(res.payload["failed_count"].as_u64().unwrap(), 1);

    // Cache should NOT be moved
    assert!(chrome_cache.exists());

    Ok(())
}
