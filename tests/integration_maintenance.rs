use std::fs;
use std::process::Command;

fn run_macmop_isolated(
    test_name: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> (i32, serde_json::Value) {
    let base = format!("/private/tmp/macmop-test-maint-{}", test_name);
    let home = format!("{}/home", base);
    let data_dir = format!("{}/data", base);
    let trash = format!("{}/trash", base);
    let audit_file = format!("{}/data/audit/last.json", base);
    let rollback_file = format!("{}/data/rollback/entries.json", base);

    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data_dir).unwrap();

    let mut cmd = Command::new("cargo");
    cmd.args(["run", "--bin", "macmop", "--", "--json"])
        .args(args)
        .env("MACMOP_TEST_MODE", "1")
        .env("MACMOP_HOME", &home)
        .env("MACMOP_DATA_DIR", &data_dir)
        .env("MACMOP_TRASH_DIR", &trash)
        .env("MACMOP_AUDIT_FILE", &audit_file)
        .env("MACMOP_ROLLBACK_FILE", &rollback_file);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .expect("failed to execute macmop binary via cargo run");
    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let json = if exit_code == 0 {
        serde_json::from_str(&stdout).expect("stdout must be valid JSON")
    } else {
        serde_json::Value::Null
    };

    (exit_code, json)
}

#[test]
fn test_maintenance_list_returns_catalog() {
    let (code, envelope) = run_macmop_isolated("list", &["maintenance", "list"], &[]);
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

    let _ = fs::remove_dir_all("/private/tmp/macmop-test-maint-list");
}

#[test]
fn test_maintenance_check_is_report_only_and_not_executable() {
    let (code, envelope) = run_macmop_isolated("check", &["maintenance", "check"], &[]);
    assert_eq!(code, 0);
    assert_eq!(envelope["command"], "maintenance check");

    let items = envelope["payload"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    for item in items {
        assert_eq!(item["action"], "report_only");
        if item["id"] == "flush_dns" {
            assert_eq!(item["execution_supported"], true);
        } else {
            assert_eq!(item["execution_supported"], false);
        }
        assert!(!item["future_action"].as_str().unwrap().contains("sudo "));
        assert!(!item["future_action"].as_str().unwrap().contains(";"));
    }

    let snapshot_task = items
        .iter()
        .find(|item| item["id"] == "thin_time_machine_snapshots")
        .unwrap();
    assert_eq!(snapshot_task["requires_sudo"], true);

    let _ = fs::remove_dir_all("/private/tmp/macmop-test-maint-check");
}

#[test]
fn test_maintenance_check_creates_no_audit_or_rollback_files() {
    let base = "/private/tmp/macmop-test-maint-check-files";
    let audit_file = format!("{}/data/audit/last.json", base);
    let rollback_file = format!("{}/data/rollback/entries.json", base);

    let (code, _envelope) = run_macmop_isolated("check-files", &["maintenance", "check"], &[]);
    assert_eq!(code, 0);

    assert!(!std::path::Path::new(&audit_file).exists());
    assert!(!std::path::Path::new(&rollback_file).exists());

    let _ = fs::remove_dir_all(base);
}

#[test]
fn test_maintenance_run_flush_dns_dry_run() {
    let base = "/private/tmp/macmop-test-maint-dryrun";
    let audit_file = format!("{}/data/audit/last.json", base);

    let (code, envelope) = run_macmop_isolated("dryrun", &["maintenance", "run", "flush_dns"], &[]);
    assert_eq!(code, 0);
    assert_eq!(envelope["command"], "maintenance run");
    assert_eq!(envelope["payload"]["execution"], "not_executed");
    assert_eq!(envelope["payload"]["rollback"], "not_reversible");

    assert!(!std::path::Path::new(&audit_file).exists());

    let _ = fs::remove_dir_all(base);
}

#[test]
fn test_maintenance_run_flush_dns_apply_success() {
    let base = "/private/tmp/macmop-test-maint-success";
    let script_base = "/private/tmp/macmop-test-maint-success-script";
    let script_path = format!("{}/fake-dscacheutil", script_base);
    let audit_file = format!("{}/data/audit/last.json", base);
    let rollback_file = format!("{}/data/rollback/entries.json", base);

    let _ = fs::remove_dir_all(base);
    let _ = fs::remove_dir_all(script_base);
    fs::create_dir_all(script_base).unwrap();

    fs::write(&script_path, "#!/bin/sh\necho \"fake stdout\"\nexit 0").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let (code, envelope) = run_macmop_isolated(
        "success",
        &["--apply", "maintenance", "run", "flush_dns"],
        &[("MACMOP_MAINTENANCE_DSCACHEUTIL", &script_path)],
    );

    assert_eq!(code, 0);
    assert_eq!(envelope["payload"]["execution"], "executed");
    assert_eq!(envelope["payload"]["rollback"], "not_reversible");
    assert_eq!(envelope["payload"]["exit_code"], 0);
    assert!(envelope["payload"]["stdout"]
        .as_str()
        .unwrap()
        .contains("fake stdout"));

    // Check audit file log
    assert!(std::path::Path::new(&audit_file).exists());
    let audit_data = fs::read_to_string(&audit_file).unwrap();
    let audit_json: serde_json::Value = serde_json::from_str(&audit_data).unwrap();
    assert_eq!(audit_json[0]["command"], "maintenance run flush_dns");

    let status_str = audit_json[0]["status"].as_str().unwrap();
    let status_json: serde_json::Value = serde_json::from_str(status_str).unwrap();
    assert_eq!(status_json["operation"], "maintenance_run");
    assert_eq!(status_json["task"], "flush_dns");
    assert_eq!(status_json["rollback"], "not_reversible");
    assert_eq!(status_json["status"], "success");
    assert_eq!(status_json["exit_code"], 0);

    // Rollback file must not exist
    assert!(!std::path::Path::new(&rollback_file).exists());

    let _ = fs::remove_dir_all(base);
    let _ = fs::remove_dir_all(script_base);
}

#[test]
fn test_maintenance_run_flush_dns_apply_failure() {
    let base = "/private/tmp/macmop-test-maint-fail";
    let script_base = "/private/tmp/macmop-test-maint-fail-script";
    let script_path = format!("{}/fake-dscacheutil", script_base);
    let audit_file = format!("{}/data/audit/last.json", base);

    let _ = fs::remove_dir_all(base);
    let _ = fs::remove_dir_all(script_base);
    fs::create_dir_all(script_base).unwrap();

    fs::write(&script_path, "#!/bin/sh\necho \"error logs\" >&2\nexit 5").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
    }

    let (code, envelope) = run_macmop_isolated(
        "fail",
        &["--apply", "maintenance", "run", "flush_dns"],
        &[("MACMOP_MAINTENANCE_DSCACHEUTIL", &script_path)],
    );

    assert_eq!(code, 0);
    assert_eq!(envelope["payload"]["execution"], "executed");
    assert_eq!(envelope["payload"]["exit_code"], 5);
    assert!(envelope["payload"]["stderr"]
        .as_str()
        .unwrap()
        .contains("error logs"));

    // Check audit file log
    assert!(std::path::Path::new(&audit_file).exists());
    let audit_data = fs::read_to_string(&audit_file).unwrap();
    let audit_json: serde_json::Value = serde_json::from_str(&audit_data).unwrap();
    let status_str = audit_json[0]["status"].as_str().unwrap();
    let status_json: serde_json::Value = serde_json::from_str(status_str).unwrap();
    assert_eq!(status_json["status"], "failed");
    assert_eq!(status_json["exit_code"], 5);
    assert!(status_json["stderr"]
        .as_str()
        .unwrap()
        .contains("error logs"));

    let _ = fs::remove_dir_all(base);
    let _ = fs::remove_dir_all(script_base);
}

#[test]
fn test_maintenance_run_unsupported_tasks() {
    let (code, _envelope) = run_macmop_isolated(
        "unsupported",
        &["--apply", "maintenance", "run", "rebuild_spotlight"],
        &[],
    );
    assert_ne!(code, 0);
}

#[test]
fn test_maintenance_run_permanent_blocked() {
    let (code, _envelope) = run_macmop_isolated(
        "perm-blocked",
        &["--permanent", "maintenance", "run", "flush_dns"],
        &[],
    );
    assert_ne!(code, 0);
}
