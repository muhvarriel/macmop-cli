use anyhow::Result;
use macmop::core::{AppContext, AppPaths, ExecutionMode, OutputFormat, PlannedActionKind};
use macmop::modules::startup;
use std::fs;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

// ── Test env setup ──────────────────────────────────────────────────────────

struct TestEnv {
    agents_dir: PathBuf,
    _base: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let unique_id = macmop::core::unix_now();
        let base = std::env::temp_dir().join(format!(
            "macmop-test-startup-{}-{}-{}",
            test_name,
            unique_id,
            std::process::id()
        ));
        let agents_dir = base.join("LaunchAgents");
        fs::create_dir_all(&agents_dir).unwrap();
        Self {
            agents_dir,
            _base: base,
        }
    }

    /// Write a well-formed LaunchAgent plist with the given fields.
    fn create_plist(
        &self,
        filename: &str,
        label: &str,
        program_args: &[&str],
        run_at_load: bool,
        keep_alive: bool,
    ) -> PathBuf {
        let args_xml: String = program_args
            .iter()
            .map(|a| format!("    <string>{a}</string>\n"))
            .collect();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
{args_xml}    </array>
    <key>RunAtLoad</key>
    <{run_at_load}/>
    <key>KeepAlive</key>
    <{keep_alive}/>
</dict>
</plist>"#
        );
        let path = self.agents_dir.join(filename);
        fs::write(&path, plist).unwrap();
        path
    }

    /// Write a plist whose XML is intentionally invalid (binary gibberish).
    fn create_malformed_plist(&self, filename: &str) -> PathBuf {
        let path = self.agents_dir.join(filename);
        fs::write(&path, b"\x00\x01NOT_A_PLIST\xFF\xFE").unwrap();
        path
    }

    /// Write a well-formed plist but missing the Label key.
    fn create_plist_without_label(&self, filename: &str) -> PathBuf {
        let plist = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/some-helper</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>"#;
        let path = self.agents_dir.join(filename);
        fs::write(&path, plist).unwrap();
        path
    }

    /// Build an isolated AppContext with only startup_dirs pointing to test dir.
    fn ctx_user_agent(&self) -> AppContext {
        self.ctx_with_source("user_launch_agents")
    }

    fn ctx_system_daemon(&self) -> AppContext {
        self.ctx_with_source("system_launch_daemons")
    }

    fn ctx_with_source(&self, source: &str) -> AppContext {
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
            startup_dirs: vec![(self.agents_dir.clone(), source.to_string())],
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

// ── Tests ────────────────────────────────────────────────────────────────────

/// startup list detects a user LaunchAgent plist
#[test]
fn test_startup_list_detects_user_launch_agent() -> Result<()> {
    let env = TestEnv::new("list");
    env.create_plist(
        "com.example.helper.plist",
        "com.example.helper",
        &["/usr/bin/example-helper", "--daemon"],
        true,
        false,
    );
    let ctx = env.ctx_user_agent();

    let envelope = startup::run(
        &ctx,
        macmop::cli::StartupArgs {
            command: macmop::cli::StartupCommand::List,
        },
    )?;

    assert_eq!(envelope.schema_version, "1.0");
    assert_eq!(envelope.command, "startup list");
    assert_eq!(envelope.mode, "dry_run");

    let items = envelope.payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "should detect one startup item");

    let item = &items[0];
    assert_eq!(item["label"].as_str().unwrap(), "com.example.helper");
    assert_eq!(item["source"].as_str().unwrap(), "user_launch_agents");
    assert!(item["run_at_load"].as_bool().unwrap());
    assert!(!item["keep_alive"].as_bool().unwrap());
    assert!(!item["is_system_item"].as_bool().unwrap());

    Ok(())
}

/// parses Label and ProgramArguments correctly
#[test]
fn test_startup_parses_label_and_program_arguments() -> Result<()> {
    let env = TestEnv::new("parse");
    env.create_plist(
        "com.acme.worker.plist",
        "com.acme.worker",
        &["/usr/local/bin/worker", "--config", "/etc/worker.conf"],
        false,
        true,
    );
    let ctx = env.ctx_user_agent();

    let envelope = startup::run(
        &ctx,
        macmop::cli::StartupArgs {
            command: macmop::cli::StartupCommand::List,
        },
    )?;

    let items = envelope.payload["items"].as_array().unwrap();
    let item = &items[0];

    assert_eq!(item["label"].as_str().unwrap(), "com.acme.worker");
    assert_eq!(
        item["program"].as_str().unwrap(),
        "/usr/local/bin/worker",
        "program must be first ProgramArguments entry"
    );
    let args = item["program_arguments"].as_array().unwrap();
    assert_eq!(args.len(), 3);
    assert_eq!(args[0].as_str().unwrap(), "/usr/local/bin/worker");
    assert_eq!(args[1].as_str().unwrap(), "--config");
    assert!(item["keep_alive"].as_bool().unwrap());

    Ok(())
}

/// system LaunchDaemon gets is_system_item=true and risk=critical
#[test]
fn test_system_daemon_is_marked_critical() -> Result<()> {
    let env = TestEnv::new("system");
    env.create_plist(
        "com.vendor.daemon.plist",
        "com.vendor.daemon",
        &["/usr/sbin/vendord"],
        true,
        true,
    );
    let ctx = env.ctx_system_daemon();

    let envelope = startup::run(
        &ctx,
        macmop::cli::StartupArgs {
            command: macmop::cli::StartupCommand::List,
        },
    )?;

    let items = envelope.payload["items"].as_array().unwrap();
    let item = &items[0];

    assert!(
        item["is_system_item"].as_bool().unwrap(),
        "system_launch_daemons must be is_system_item=true"
    );
    assert_eq!(
        item["risk"].as_str().unwrap(),
        "critical",
        "system daemon risk must be critical"
    );
    assert_eq!(
        item["action"].as_str().unwrap(),
        "report_only",
        "system daemon action must be report_only"
    );

    Ok(())
}

/// malformed plist becomes a scan warning, not a crash
#[test]
fn test_malformed_plist_becomes_warning_not_crash() -> Result<()> {
    let env = TestEnv::new("malformed");
    env.create_malformed_plist("garbage.plist");

    // Add one valid agent too
    env.create_plist(
        "com.ok.agent.plist",
        "com.ok.agent",
        &["/usr/bin/ok"],
        false,
        false,
    );

    let ctx = env.ctx_user_agent();
    let envelope = startup::run(
        &ctx,
        macmop::cli::StartupArgs {
            command: macmop::cli::StartupCommand::List,
        },
    )?;

    // Must not panic; valid item still parsed
    let items = envelope.payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "only valid plist should be in items");
    assert_eq!(items[0]["label"].as_str().unwrap(), "com.ok.agent");

    // Malformed file must appear as a warning
    let warnings = envelope.payload["warnings"].as_array().unwrap();
    assert!(
        !warnings.is_empty(),
        "malformed plist must produce a scan warning"
    );
    let has_garbage_warning = warnings
        .iter()
        .any(|w| w.as_str().unwrap_or("").contains("garbage.plist"));
    assert!(
        has_garbage_warning,
        "warning must reference the bad filename"
    );

    Ok(())
}

/// plist missing Label key: uses filename stem as id and adds item warning
#[test]
fn test_plist_missing_label_uses_filename_fallback() -> Result<()> {
    let env = TestEnv::new("nolabel");
    env.create_plist_without_label("no-label-agent.plist");
    let ctx = env.ctx_user_agent();

    let envelope = startup::run(
        &ctx,
        macmop::cli::StartupArgs {
            command: macmop::cli::StartupCommand::List,
        },
    )?;

    let items = envelope.payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "no-label plist still produces an item");

    let item = &items[0];
    // Label falls back to filename stem
    assert_eq!(
        item["label"].as_str().unwrap(),
        "no-label-agent",
        "label should fall back to filename stem"
    );
    // Item-level warnings array must mention missing Label
    let item_warnings = item["warnings"].as_array().unwrap();
    assert!(
        item_warnings
            .iter()
            .any(|w| w.as_str().unwrap_or("").contains("Label")),
        "item must warn about missing Label key"
    );

    Ok(())
}

/// JSON envelope schema_version is always 1.0 for both startup commands
#[test]
fn test_startup_json_schema_version_is_stable() -> Result<()> {
    let env = TestEnv::new("schema");
    env.create_plist(
        "com.schema.test.plist",
        "com.schema.test",
        &["/bin/sh"],
        false,
        false,
    );
    let ctx = env.ctx_user_agent();

    for cmd in [
        macmop::cli::StartupCommand::List,
        macmop::cli::StartupCommand::Inspect {
            id: "com.schema.test".to_string(),
        },
    ] {
        let envelope = startup::run(&ctx, macmop::cli::StartupArgs { command: cmd })?;
        assert_eq!(envelope.schema_version, "1.0", "schema_version must be 1.0");
    }

    Ok(())
}

/// No ActionPlan destructive actions — all startup items are report_only
#[test]
fn test_startup_items_are_always_report_only() -> Result<()> {
    let env = TestEnv::new("safety");
    // High-risk-looking items: run_at_load=true, keep_alive=true
    env.create_plist(
        "com.risky.agent.plist",
        "com.risky.agent",
        &["/usr/bin/risky"],
        true,
        true,
    );
    let ctx = env.ctx_user_agent();

    let envelope = startup::run(
        &ctx,
        macmop::cli::StartupArgs {
            command: macmop::cli::StartupCommand::List,
        },
    )?;

    let items = envelope.payload["items"].as_array().unwrap();
    for item in items {
        assert_eq!(
            item["action"].as_str().unwrap(),
            "report_only",
            "startup items must never have destructive action"
        );
        // Verify via strongly-typed parse too
        let action: PlannedActionKind = serde_json::from_value(item["action"].clone()).unwrap();
        assert_eq!(action, PlannedActionKind::ReportOnly);
    }

    Ok(())
}
