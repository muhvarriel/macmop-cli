use std::fs;
use std::process::Command;

fn run_macmop(args: &[&str]) -> (i32, serde_json::Value) {
    let output = Command::new("cargo")
        .args(["run", "--bin", "macmop", "--", "--json"])
        .args(args)
        .env("MACMOP_TEST_MODE", "1")
        .env("MACMOP_HOME", "/private/tmp/macmop-maintenance-home")
        .env("MACMOP_DATA_DIR", "/private/tmp/macmop-maintenance-data")
        .env("MACMOP_TRASH_DIR", "/private/tmp/macmop-maintenance-trash")
        .env(
            "MACMOP_AUDIT_FILE",
            "/private/tmp/macmop-maintenance-data/audit/last.json",
        )
        .env(
            "MACMOP_ROLLBACK_FILE",
            "/private/tmp/macmop-maintenance-data/rollback/entries.json",
        )
        .output()
        .expect("failed to execute macmop binary via cargo run");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let json = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    (exit_code, json)
}

#[test]
fn test_maintenance_list_returns_catalog() {
    let (code, envelope) = run_macmop(&["maintenance", "list"]);
    assert_eq!(code, 0);
    assert_eq!(envelope["schema_version"], "1.0");
    assert_eq!(envelope["command"], "maintenance list");

    let items = envelope["payload"]["items"].as_array().unwrap();
    let ids: Vec<_> = items
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 4);
    assert!(ids.contains(&"flush_dns"));
    assert!(ids.contains(&"rebuild_spotlight"));
    assert!(ids.contains(&"thin_time_machine_snapshots"));
    assert!(ids.contains(&"rotate_logs"));
}

#[test]
fn test_maintenance_check_is_report_only_and_not_executable() {
    let (code, envelope) = run_macmop(&["maintenance", "check"]);
    assert_eq!(code, 0);
    assert_eq!(envelope["command"], "maintenance check");

    let items = envelope["payload"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    for item in items {
        assert_eq!(item["action"], "report_only");
        assert_eq!(item["execution_supported"], false);
        assert!(!item["future_action"].as_str().unwrap().contains("sudo "));
        assert!(!item["future_action"].as_str().unwrap().contains(";"));
    }

    let snapshot_task = items
        .iter()
        .find(|item| item["id"] == "thin_time_machine_snapshots")
        .unwrap();
    assert_eq!(snapshot_task["requires_sudo"], true);
}

#[test]
fn test_maintenance_check_creates_no_audit_or_rollback_files() {
    let _ = fs::remove_dir_all("/private/tmp/macmop-maintenance-data");

    let (code, _envelope) = run_macmop(&["maintenance", "check"]);
    assert_eq!(code, 0);

    assert!(!std::path::Path::new("/private/tmp/macmop-maintenance-data/audit/last.json").exists());
    assert!(
        !std::path::Path::new("/private/tmp/macmop-maintenance-data/rollback/entries.json")
            .exists()
    );
}
