use anyhow::Result;
use macmop::core::{AppContext, AppPaths, ExecutionMode, OutputFormat};
use macmop::modules::protect;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};

struct TestEnv {
    user_agents: PathBuf,
    system_agents: PathBuf,
    _base: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let unique_id = macmop::core::unix_now();
        let base = std::env::temp_dir().join(format!(
            "macmop-test-protect-{}-{}-{}",
            test_name,
            unique_id,
            std::process::id()
        ));
        let user_agents = base.join("user_agents");
        let system_agents = base.join("system_agents");
        fs::create_dir_all(&user_agents).unwrap();
        fs::create_dir_all(&system_agents).unwrap();
        Self {
            user_agents,
            system_agents,
            _base: base,
        }
    }

    fn write_plist(&self, dir: &Path, filename: &str, content: &str) {
        fs::write(dir.join(filename), content).unwrap();
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
            home: self._base.join("home"),
            data_dir: self._base.join("data"),
            trash: self._base.join("trash"),
            audit_file: self._base.join("data/audit.json"),
            rollback_file: self._base.join("data/rollback.json"),
            apps_dirs: vec![],
            startup_dirs: vec![
                (self.user_agents.clone(), "user_launch_agents".to_string()),
                (
                    self.system_agents.clone(),
                    "system_launch_agents".to_string(),
                ),
            ],
            quicklook_dirs: vec![],
        };
        ctx
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self._base);
    }
}

#[test]
fn test_protect_benign_startup_item_low_or_no_finding() -> Result<()> {
    let env = TestEnv::new("benign");
    let current_exe = std::env::current_exe()?.to_string_lossy().to_string();

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.benign</string>
    <key>Program</key>
    <string>{}</string>
</dict>
</plist>"#,
        current_exe
    );
    env.write_plist(&env.user_agents, "com.example.benign.plist", &plist);

    let ctx = env.ctx();
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;

    let payload = res.payload;
    let summary = payload.get("summary").unwrap();
    let scanned_items = summary.get("scanned_items").unwrap().as_u64().unwrap();
    let finding_count = summary.get("finding_count").unwrap().as_u64().unwrap();

    assert_eq!(scanned_items, 1);
    assert_eq!(finding_count, 0);

    let findings = payload.get("findings").unwrap().as_array().unwrap();
    assert!(findings.is_empty());

    Ok(())
}

#[test]
fn test_protect_shell_based_args_produces_finding() -> Result<()> {
    let env = TestEnv::new("shell");
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.shell</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/sh</string>
        <string>-c</string>
        <string>echo hello</string>
    </array>
</dict>
</plist>"#;
    env.write_plist(&env.user_agents, "com.example.shell.plist", plist);

    let ctx = env.ctx();
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;

    let findings_val = res.payload.get("findings").unwrap().as_array().unwrap();
    assert_eq!(findings_val.len(), 1);

    let finding = findings_val.first().unwrap();
    assert_eq!(
        finding.get("label").unwrap().as_str().unwrap(),
        "com.example.shell"
    );
    assert_eq!(finding.get("severity").unwrap().as_str().unwrap(), "medium");
    assert!(!finding
        .get("evidence")
        .unwrap()
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        finding.get("action").unwrap().as_str().unwrap(),
        "report_only"
    );

    Ok(())
}

#[test]
fn test_protect_missing_executable_produces_finding() -> Result<()> {
    let env = TestEnv::new("missing");
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.missing</string>
    <key>Program</key>
    <string>/nonexistent/executable/path</string>
</dict>
</plist>"#;
    env.write_plist(&env.user_agents, "com.example.missing.plist", plist);

    let ctx = env.ctx();
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;

    let findings_val = res.payload.get("findings").unwrap().as_array().unwrap();
    assert_eq!(findings_val.len(), 1);

    let finding = findings_val.first().unwrap();
    assert_eq!(finding.get("severity").unwrap().as_str().unwrap(), "high");
    let evidence = finding.get("evidence").unwrap().as_array().unwrap();
    assert!(evidence
        .iter()
        .any(|e| e.as_str().unwrap().contains("does not exist")));
    assert_eq!(
        finding.get("recommendation").unwrap().as_str().unwrap(),
        "Verify this item path and arguments for suspicious behavior."
    );

    Ok(())
}

#[test]
fn test_protect_system_item_is_report_only() -> Result<()> {
    let env = TestEnv::new("system");
    let current_exe = std::env::current_exe()?.to_string_lossy().to_string();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.apple.system.service</string>
    <key>Program</key>
    <string>{}</string>
</dict>
</plist>"#,
        current_exe
    );
    env.write_plist(&env.system_agents, "com.apple.system.service.plist", &plist);

    let ctx = env.ctx();
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;

    let findings_val = res.payload.get("findings").unwrap().as_array().unwrap();
    assert_eq!(findings_val.len(), 1);

    let finding = findings_val.first().unwrap();
    assert_eq!(finding.get("severity").unwrap().as_str().unwrap(), "low");
    assert!(finding.get("is_system_item").unwrap().as_bool().unwrap());
    assert!(finding.get("is_protected").unwrap().as_bool().unwrap());
    assert_eq!(
        finding.get("action").unwrap().as_str().unwrap(),
        "report_only"
    );

    Ok(())
}

#[test]
fn test_protect_malformed_plist_warning_no_crash() -> Result<()> {
    let env = TestEnv::new("malformed");
    env.write_plist(
        &env.user_agents,
        "com.example.malformed.plist",
        "not a valid plist",
    );

    let ctx = env.ctx();
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;

    let warnings = res.payload.get("warnings").unwrap().as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0]
        .as_str()
        .unwrap()
        .contains("skipped com.example.malformed.plist"));

    Ok(())
}

#[test]
fn test_protect_json_schema_version_is_stable() -> Result<()> {
    let env = TestEnv::new("schema");
    let ctx = env.ctx();
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;

    assert_eq!(res.schema_version, "1.0");
    assert_eq!(res.command, "protect scan");
    assert_eq!(res.mode, "dry_run");

    Ok(())
}

#[test]
fn test_protect_no_destructive_action_plan_actions() -> Result<()> {
    let env = TestEnv::new("no_destructive");
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.shell</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/sh</string>
        <string>-c</string>
        <string>echo hello</string>
    </array>
</dict>
</plist>"#;
    env.write_plist(&env.user_agents, "com.example.shell.plist", plist);

    let ctx = env.ctx();
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;

    let findings_val = res.payload.get("findings").unwrap().as_array().unwrap();
    for finding in findings_val {
        let action = finding.get("action").unwrap().as_str().unwrap();
        assert_eq!(action, "report_only");
    }

    Ok(())
}
