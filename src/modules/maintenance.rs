use super::*;
use crate::core::{MaintenanceTask, PlannedActionKind, RiskLevel};

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::MaintenanceArgs,
) -> Result<JsonEnvelope<Value>> {
    match args.command {
        crate::cli::MaintenanceCommand::List => list(ctx),
        crate::cli::MaintenanceCommand::Check => check(ctx),
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
                task.available = cfg!(target_os = "macos");
                task.reason = if task.available {
                    "Available on macOS; alpha.6 reports only and does not flush DNS.".to_string()
                } else {
                    "DNS cache flush preflight is only available on macOS.".to_string()
                };
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

fn catalog() -> Vec<MaintenanceTask> {
    vec![
        MaintenanceTask {
            id: "flush_dns".to_string(),
            category: "network".to_string(),
            name: "Flush DNS Cache".to_string(),
            description: "Report whether DNS cache flushing could be supported later.".to_string(),
            risk: RiskLevel::Low,
            requires_sudo: false,
            available: true,
            reason: "Static catalog entry; run maintenance check for availability.".to_string(),
            future_action:
                "Would flush DNS cache using macOS DNS cache utilities in a future guarded execution slice."
                    .to_string(),
            execution_supported: false,
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
