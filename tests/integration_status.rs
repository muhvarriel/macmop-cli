use anyhow::Result;
use macmop::audit;
use macmop::core::{
    AppContext, AuditId, AuditLog, ExecutionMode, OutputFormat, PlannedActionKind, RollbackEntry,
    RollbackId,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};

struct TestEnv {
    base: PathBuf,
    home: PathBuf,
    data_dir: PathBuf,
    trash_dir: PathBuf,
    audit_file: PathBuf,
    rollback_file: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "macmop-status-{}-{}-{}",
            test_name,
            macmop::core::unix_now(),
            std::process::id()
        ));
        let home = base.join("home");
        let data_dir = base.join("data");
        let trash_dir = base.join("trash");
        let audit_file = data_dir.join("audit/last.json");
        let rollback_file = data_dir.join("rollback/entries.json");

        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&trash_dir).unwrap();

        Self {
            base,
            home,
            data_dir,
            trash_dir,
            audit_file,
            rollback_file,
        }
    }

    fn context(&self) -> AppContext {
        std::env::set_var("MACMOP_TEST_MODE", "1");
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut ctx =
            AppContext::load(None, ExecutionMode::DryRun, OutputFormat::Json, cancelled).unwrap();
        ctx.paths = macmop::core::AppPaths {
            home: self.home.clone(),
            data_dir: self.data_dir.clone(),
            trash: self.trash_dir.clone(),
            audit_file: self.audit_file.clone(),
            rollback_file: self.rollback_file.clone(),
            apps_dirs: vec![],
            startup_dirs: vec![],
            quicklook_dirs: vec![],
        };
        ctx
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn test_status_reports_schema_version_paths_and_test_mode() -> Result<()> {
    let env = TestEnv::new("basic");
    let result = macmop::modules::status::run(&env.context())?;
    assert_eq!(result.schema_version, "1.0");
    assert_eq!(result.command, "status");

    let summary = &result.payload["summary"];
    assert_eq!(summary["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(summary["schema_version"], "1.0");
    assert_eq!(summary["test_mode"], true);
    assert_eq!(summary["home"], env.home.to_string_lossy().as_ref());
    assert_eq!(summary["data_dir"], env.data_dir.to_string_lossy().as_ref());
    assert_eq!(summary["trash"], env.trash_dir.to_string_lossy().as_ref());
    assert_eq!(
        summary["audit_file"],
        env.audit_file.to_string_lossy().as_ref()
    );
    assert_eq!(
        summary["rollback_file"],
        env.rollback_file.to_string_lossy().as_ref()
    );
    assert!(summary["available_modules"]
        .as_array()
        .unwrap()
        .iter()
        .any(|module| module == "status"));

    Ok(())
}

#[test]
fn test_status_counts_audit_and_rollback_fixtures() -> Result<()> {
    let env = TestEnv::new("counts");
    let audit_entries = vec![
        AuditLog {
            id: AuditId("audit_1".to_string()),
            timestamp: 1,
            command: "cleanup".to_string(),
            action: PlannedActionKind::MoveToTrash,
            path: env.home.join("a"),
            size_bytes: 1,
            status: "success".to_string(),
            rollback_id: None,
        },
        AuditLog {
            id: AuditId("audit_2".to_string()),
            timestamp: 2,
            command: "cleanup".to_string(),
            action: PlannedActionKind::MoveToTrash,
            path: env.home.join("b"),
            size_bytes: 2,
            status: "success".to_string(),
            rollback_id: None,
        },
    ];
    audit::write_last_audit(&env.audit_file, &audit_entries)?;
    audit::append_rollback(
        &env.rollback_file,
        RollbackEntry {
            id: RollbackId("rollback_1".to_string()),
            original_path: env.home.join("a"),
            current_path: env.trash_dir.join("a"),
            created_at: 1,
            action: PlannedActionKind::MoveToTrash,
        },
    )?;

    let result = macmop::modules::status::run(&env.context())?;
    let summary = &result.payload["summary"];
    assert_eq!(summary["last_audit_exists"], true);
    assert_eq!(summary["last_audit_entry_count"], 2);
    assert_eq!(summary["rollback_entry_count"], 1);
    assert!(result.payload["warnings"].as_array().unwrap().is_empty());

    Ok(())
}

#[test]
fn test_status_missing_audit_and_rollback_do_not_fail_or_write() -> Result<()> {
    let env = TestEnv::new("missing");
    let result = macmop::modules::status::run(&env.context())?;
    let summary = &result.payload["summary"];
    assert_eq!(summary["last_audit_exists"], false);
    assert_eq!(summary["last_audit_entry_count"], 0);
    assert_eq!(summary["rollback_entry_count"], 0);
    assert!(!env.audit_file.exists());
    assert!(!env.rollback_file.exists());

    Ok(())
}

#[test]
fn test_status_corrupt_audit_and_rollback_become_warnings() -> Result<()> {
    let env = TestEnv::new("corrupt");
    write_file(&env.audit_file, b"not-json")?;
    write_file(&env.rollback_file, b"not-json")?;

    let result = macmop::modules::status::run(&env.context())?;
    let summary = &result.payload["summary"];
    let warnings = result.payload["warnings"].as_array().unwrap();
    assert_eq!(summary["last_audit_entry_count"], 0);
    assert_eq!(summary["rollback_entry_count"], 0);
    assert!(warnings.len() >= 2);

    Ok(())
}

#[test]
fn test_status_bounded_home_traversal() -> Result<()> {
    let env = TestEnv::new("bounded");
    for i in 0..10_050 {
        fs::write(env.home.join(format!("file-{i}.txt")), b"x")?;
    }

    let result = macmop::modules::status::run(&env.context())?;
    let home_summary = &result.payload["summary"]["home_summary"];
    assert!(home_summary["sampled_file_count"].as_u64().unwrap() <= 10_000);
    assert_eq!(home_summary["entry_limit_reached"], true);

    Ok(())
}

#[test]
fn test_status_cli_json_envelope() {
    let output = std::process::Command::new("cargo")
        .args(["run", "--bin", "macmop", "--", "status", "--json"])
        .env("MACMOP_TEST_MODE", "1")
        .env("MACMOP_HOME", "/private/tmp/macmop-status-cli-home")
        .env("MACMOP_DATA_DIR", "/private/tmp/macmop-status-cli-data")
        .env("MACMOP_TRASH_DIR", "/private/tmp/macmop-status-cli-trash")
        .env(
            "MACMOP_AUDIT_FILE",
            "/private/tmp/macmop-status-cli-data/audit/last.json",
        )
        .env(
            "MACMOP_ROLLBACK_FILE",
            "/private/tmp/macmop-status-cli-data/rollback/entries.json",
        )
        .output()
        .expect("failed to run macmop status");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(envelope["schema_version"], "1.0");
    assert_eq!(envelope["command"], "status");
    assert_eq!(
        envelope["payload"]["summary"]["version"],
        env!("CARGO_PKG_VERSION")
    );
}

fn write_file(path: &Path, content: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}
