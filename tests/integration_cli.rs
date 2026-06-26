use std::process::Command;
use std::str;

fn run_macmop(args: &[&str]) -> (i32, String, String) {
    let output = Command::new("cargo")
        .args(["run", "--bin", "macmop", "--"])
        .args(args)
        .output()
        .expect("failed to execute macmop binary via cargo run");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}

#[test]
fn test_cli_flag_conflict_validation() {
    // 1. Conflicting formats: --json + --ndjson
    let (code1, _stdout, _stderr) = run_macmop(&["--json", "--ndjson", "cleanup"]);
    assert_ne!(code1, 0, "conflicting formats must exit with non-zero");

    // 2. --permanent without --force
    let (code2, _stdout, _stderr) = run_macmop(&["--permanent", "cleanup"]);
    assert_ne!(code2, 0, "permanent without force must exit with non-zero");

    // 3. --apply with --dry-run
    let (code3, _stdout, _stderr) = run_macmop(&["--apply", "--dry-run", "cleanup"]);
    assert_ne!(code3, 0, "apply + dry-run must exit with non-zero");
}

#[test]
fn test_cli_scan_always_dry_run_warning_on_stderr() {
    let (code, stdout, stderr) = run_macmop(&["--apply", "scan"]);
    assert_eq!(code, 0, "scan with apply should succeed but print warning");

    // Warning must be on stderr
    assert!(
        stderr.contains("macmop scan is always dry-run. Ignoring --apply."),
        "warning must be present on stderr"
    );

    // Output envelope must go to stdout and parse as valid JSON
    let envelope: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert_eq!(envelope["schema_version"], "1.0");
    assert_eq!(envelope["command"], "scan");
    assert!(envelope.get("payload").is_some());
}

#[test]
fn test_ndjson_line_by_line_validation() {
    let (code, stdout, _stderr) = run_macmop(&["--ndjson", "scan"]);
    assert_eq!(code, 0);

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let envelope: serde_json::Value =
            serde_json::from_str(line).expect("ndjson line must be valid JSON");
        assert_eq!(envelope["schema_version"], "1.0");
        assert_eq!(envelope["command"], "scan");
        assert!(envelope.get("payload").is_some());
    }
}

#[test]
fn test_readme_command_smoke_tests() {
    // macmop cleanup --dry-run
    let (code1, _stdout, _stderr) = run_macmop(&["cleanup", "--dry-run"]);
    assert_eq!(code1, 0);

    // macmop scan
    let (code2, _stdout, _stderr) = run_macmop(&["scan"]);
    assert_eq!(code2, 0);

    // macmop report last
    let (code3, _stdout, _stderr) = run_macmop(&["report", "last"]);
    assert_eq!(code3, 0);

    // macmop rollback list
    let (code4, _stdout, _stderr) = run_macmop(&["rollback", "list"]);
    assert_eq!(code4, 0);

    // macmop maintenance list
    let (code5, _stdout, _stderr) = run_macmop(&["maintenance", "list"]);
    assert_eq!(code5, 0);

    // macmop maintenance check
    let (code6, _stdout, _stderr) = run_macmop(&["maintenance", "check"]);
    assert_eq!(code6, 0);

    // macmop status --json
    let (code7, stdout, _stderr) = run_macmop(&["status", "--json"]);
    assert_eq!(code7, 0);
    let envelope: serde_json::Value =
        serde_json::from_str(&stdout).expect("status stdout must be valid JSON");
    assert_eq!(envelope["command"], "status");
}
