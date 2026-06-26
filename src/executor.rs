use crate::{
    audit,
    core::{
        new_id, unix_now, ActionPlan, AppContext, AuditId, AuditLog, PlannedActionKind,
        RollbackEntry, RollbackId,
    },
    scanner,
};
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn execute_plan(ctx: &AppContext, command: &str, plan: &ActionPlan) -> Result<Vec<AuditLog>> {
    let mut logs = Vec::new();
    let mut cancelled = false;
    let rollback_id = RollbackId(new_id("rollback"));

    for action in &plan.actions {
        if cancelled || ctx.is_cancelled() {
            cancelled = true;
            logs.push(AuditLog {
                id: AuditId(new_id("audit")),
                timestamp: unix_now(),
                command: command.to_string(),
                action: action.action,
                path: action.path.clone(),
                size_bytes: scanner::file_size(&action.path),
                status: "cancelled".to_string(),
                rollback_id: None,
            });
            audit::write_last_audit(&ctx.paths.audit_file, &logs)?;
            continue;
        }

        let size = scanner::file_size(&action.path);
        let (status, action_rollback_id) = match action.action {
            PlannedActionKind::ReportOnly => ("reported".to_string(), None),
            PlannedActionKind::MoveToTrash => match move_to_trash(ctx, &action.path) {
                Ok(mut rollback) => {
                    rollback.id = rollback_id.clone();
                    audit::append_rollback(&ctx.paths.rollback_file, rollback)?;
                    ("success".to_string(), Some(rollback_id.clone()))
                }
                Err(error) => (format!("failed: {error}"), None),
            },
            PlannedActionKind::PermanentDelete => match fs::remove_file(&action.path) {
                Ok(()) => ("success".to_string(), None),
                Err(error) => (format!("failed: {error}"), None),
            },
            PlannedActionKind::Quarantine => ("skipped_blocked".to_string(), None),
        };

        logs.push(AuditLog {
            id: AuditId(new_id("audit")),
            timestamp: unix_now(),
            command: command.to_string(),
            action: action.action,
            path: action.path.clone(),
            size_bytes: size,
            status,
            rollback_id: action_rollback_id,
        });

        audit::write_last_audit(&ctx.paths.audit_file, &logs)?;

        if ctx.is_cancelled() {
            cancelled = true;
        }
    }
    Ok(logs)
}

fn move_to_trash(ctx: &AppContext, path: &Path) -> Result<RollbackEntry> {
    if !path.exists() {
        bail!("path does not exist: {}", path.display());
    }
    let trash = &ctx.paths.trash;
    fs::create_dir_all(trash).with_context(|| format!("cannot create {}", trash.display()))?;
    let file_name = path.file_name().context("path has no file name")?;
    let target = unique_trash_path(trash, file_name);
    fs::rename(path, &target)
        .with_context(|| format!("cannot move {} to {}", path.display(), target.display()))?;
    Ok(RollbackEntry {
        id: RollbackId(new_id("rollback")),
        original_path: path.to_path_buf(),
        current_path: target,
        created_at: unix_now(),
        action: PlannedActionKind::MoveToTrash,
    })
}

fn unique_trash_path(trash: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let base = trash.join(file_name);
    if !base.exists() {
        return base;
    }
    for i in 1.. {
        let candidate = trash.join(format!("{}.{}", file_name.to_string_lossy(), i));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}
