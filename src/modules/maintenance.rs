use super::*;
use crate::core::new_id;
use crate::core::unix_now;
use crate::core::{AuditId, AuditLog, MaintenanceTask, PlannedActionKind, RiskLevel};
use std::path::PathBuf;

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::MaintenanceArgs,
) -> Result<JsonEnvelope<Value>> {
    if matches!(ctx.mode, crate::core::ExecutionMode::Permanent { .. }) {
        anyhow::bail!("Maintenance module does not support permanent delete.");
    }

    match args.command {
        crate::cli::MaintenanceCommand::List => list(ctx),
        crate::cli::MaintenanceCommand::Check => check(ctx),
        crate::cli::MaintenanceCommand::Run { task } => run_task(ctx, &task),
    }
}

fn list(ctx: &crate::core::AppContext) -> Result<JsonEnvelope<Value>> {
    let tasks = catalog();
    Ok(JsonEnvelope::new(
        "maintenance list",
        ctx.mode.clone(),
        json!({
            "summary": {
                "task_count": tasks.len(),
                "execution_supported": false,
            },
            "items": tasks,
        }),
    ))
}

fn check(ctx: &crate::core::AppContext) -> Result<JsonEnvelope<Value>> {
    let mut tasks = catalog();
    for task in &mut tasks {
        match task.id.as_str() {
            "flush_dns" => {
                task.available = cfg!(target_os = "macos")
                    || std::env::var("MACMOP_MAINTENANCE_DSCACHEUTIL").is_ok();
                task.reason = if task.available {
                    "Available on macOS."
                } else {
                    "DNS cache flush preflight is only available on macOS."
                }
                .to_string();
            }
            "rebuild_spotlight" => {
                task.available = tool_exists("mdutil");
                task.reason = if task.available {
                    "mdutil is discoverable; alpha.6 reports only and does not rebuild indexes."
                        .to_string()
                } else {
                    "mdutil not found in PATH.".to_string()
                };
            }
            "thin_time_machine_snapshots" => {
                task.available = tool_exists("tmutil");
                task.reason = if task.available {
                    "tmutil is discoverable; future execution would require explicit sudo handling."
                        .to_string()
                } else {
                    "tmutil not found in PATH.".to_string()
                };
            }
            "rotate_logs" => {
                task.available = true;
                task.reason = "Listed as future report-only maintenance capability; no log rotation is run in alpha.6.".to_string();
            }
            _ => {}
        }
    }

    Ok(JsonEnvelope::new(
        "maintenance check",
        ctx.mode.clone(),
        json!({
            "summary": {
                "task_count": tasks.len(),
                "execution_supported": false,
                "note": "maintenance check does not run maintenance tasks",
            },
            "items": tasks,
        }),
    ))
}

fn run_task(ctx: &crate::core::AppContext, task: &str) -> Result<JsonEnvelope<Value>> {
    if task != "flush_dns" {
        anyhow::bail!(
            "maintenance task '{}' is not supported for execution yet.",
            task
        );
    }

    let is_macos = cfg!(target_os = "macos");
    let is_test_override = std::env::var("MACMOP_MAINTENANCE_DSCACHEUTIL").is_ok();
    if !is_macos && !is_test_override {
        anyhow::bail!("maintenance task 'flush_dns' is only supported on macOS");
    }

    let program = std::env::var("MACMOP_MAINTENANCE_DSCACHEUTIL")
        .unwrap_or_else(|_| "/usr/bin/dscacheutil".to_string());
    let command_str = format!("{} -flushcache", program);

    let mut exit_code = None;
    let mut stdout_capped = String::new();
    let mut stderr_capped = String::new();
    let stdout_len;
    let stderr_len;
    let is_success;

    if ctx.mode.is_destructive() {
        let output = std::process::Command::new(&program)
            .arg("-flushcache")
            .output();

        match output {
            Ok(out) => {
                let code = out.status.code().unwrap_or(-1);
                exit_code = Some(code);
                is_success = out.status.success();

                let stdout_str = String::from_utf8_lossy(&out.stdout);
                let stderr_str = String::from_utf8_lossy(&out.stderr);
                stdout_len = stdout_str.len();
                stderr_len = stderr_str.len();

                stdout_capped = if stdout_str.len() > 500 {
                    format!("{}... [truncated]", &stdout_str[..500])
                } else {
                    stdout_str.into_owned()
                };

                stderr_capped = if stderr_str.len() > 500 {
                    format!("{}... [truncated]", &stderr_str[..500])
                } else {
                    stderr_str.into_owned()
                };
            }
            Err(e) => {
                exit_code = Some(-2);
                is_success = false;
                stdout_len = 0;
                stderr_capped = format!("Failed to run command: {}", e);
                stderr_len = stderr_capped.len();
            }
        }

        let audit_status = json!({
            "operation": "maintenance_run",
            "task": "flush_dns",
            "command": "/usr/bin/dscacheutil -flushcache",
            "rollback": "not_reversible",
            "exit_code": exit_code.unwrap_or(-1),
            "stdout_len": stdout_len,
            "stderr_len": stderr_len,
            "status": if is_success { "success" } else { "failed" },
            "stdout": stdout_capped,
            "stderr": stderr_capped,
        })
        .to_string();

        let audit = AuditLog {
            id: AuditId(new_id("audit")),
            timestamp: unix_now(),
            command: format!("maintenance run {}", task),
            action: PlannedActionKind::ReportOnly,
            path: PathBuf::from(&program),
            size_bytes: 0,
            status: audit_status,
            rollback_id: None,
        };

        crate::audit::write_last_audit(&ctx.paths.audit_file, &[audit])?;
    }

    Ok(JsonEnvelope::new(
        "maintenance run",
        ctx.mode.clone(),
        json!({
            "summary": format!("maintenance run: {}", task),
            "task": task,
            "command": command_str,
            "execution": if ctx.mode.is_destructive() { "executed" } else { "not_executed" },
            "rollback": "not_reversible",
            "exit_code": exit_code,
            "stdout": if ctx.mode.is_destructive() { Some(stdout_capped) } else { None },
            "stderr": if ctx.mode.is_destructive() { Some(stderr_capped) } else { None },
        }),
    ))
}

fn catalog() -> Vec<MaintenanceTask> {
    vec![
        MaintenanceTask {
            id: "flush_dns".to_string(),
            category: "network".to_string(),
            name: "Flush DNS Cache".to_string(),
            description: "Flush system DNS cache resolver.".to_string(),
            risk: RiskLevel::Low,
            requires_sudo: false,
            available: true,
            reason: "Available on macOS.".to_string(),
            future_action: "Flush DNS cache using macOS DNS cache utilities.".to_string(),
            execution_supported: cfg!(target_os = "macos") || std::env::var("MACMOP_MAINTENANCE_DSCACHEUTIL").is_ok(),
            action: PlannedActionKind::ReportOnly,
        },
        MaintenanceTask {
            id: "rebuild_spotlight".to_string(),
            category: "indexing".to_string(),
            name: "Rebuild Spotlight Index".to_string(),
            description: "Report whether Spotlight index rebuild support could be available later.".to_string(),
            risk: RiskLevel::Medium,
            requires_sudo: false,
            available: true,
            reason: "Static catalog entry; run maintenance check for availability.".to_string(),
            future_action:
                "Would request Spotlight to rebuild an explicitly selected path in a future guarded execution slice."
                    .to_string(),
            execution_supported: false,
            action: PlannedActionKind::ReportOnly,
        },
        MaintenanceTask {
            id: "thin_time_machine_snapshots".to_string(),
            category: "backup".to_string(),
            name: "Thin Time Machine Snapshots".to_string(),
            description: "Report whether local Time Machine snapshot thinning could be supported later.".to_string(),
            risk: RiskLevel::Medium,
            requires_sudo: true,
            available: true,
            reason: "Static catalog entry; run maintenance check for availability.".to_string(),
            future_action:
                "Would thin local Time Machine snapshots only in a future guarded execution slice with explicit administrator consent."
                    .to_string(),
            execution_supported: false,
            action: PlannedActionKind::ReportOnly,
        },
        MaintenanceTask {
            id: "rotate_logs".to_string(),
            category: "logs".to_string(),
            name: "Rotate Logs".to_string(),
            description: "Report whether safe log rotation support could be available later.".to_string(),
            risk: RiskLevel::Low,
            requires_sudo: false,
            available: true,
            reason: "Static catalog entry; run maintenance check for availability.".to_string(),
            future_action:
                "Would request safe user-level log rotation in a future guarded execution slice."
                    .to_string(),
            execution_supported: false,
            action: PlannedActionKind::ReportOnly,
        },
    ]
}

fn tool_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|dir| dir.join(name).is_file())
}
