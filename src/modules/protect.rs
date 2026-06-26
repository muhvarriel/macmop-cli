use super::*;
use crate::core::{
    new_id, unix_now, AuditId, AuditLog, FindingId, PlannedActionKind, ProtectFinding, RiskLevel,
    RollbackEntry, RollbackId, ScanFinding, StartupItem,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuarantineMetadata {
    pub quarantine_id: String,
    pub finding_id: String,
    pub original_path: PathBuf,
    pub quarantine_path: PathBuf,
    pub metadata_path: PathBuf,
    pub operation: String,
    pub created_at: u64,
}

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::ProtectArgs,
) -> Result<JsonEnvelope<Value>> {
    match args.command {
        crate::cli::ProtectCommand::Scan => scan(ctx, "protect scan"),
        crate::cli::ProtectCommand::Startup => scan(ctx, "protect startup"),
        crate::cli::ProtectCommand::Inspect { id } => inspect(ctx, &id),
        crate::cli::ProtectCommand::Quarantine { id } => quarantine(ctx, &id),
        crate::cli::ProtectCommand::Restore { id } => restore(ctx, &id),
    }
}

fn scan(ctx: &crate::core::AppContext, command_name: &str) -> Result<JsonEnvelope<Value>> {
    let (items, warnings) = startup::scan_startup_dirs(&ctx.paths.startup_dirs);
    let findings = analyze_items(&items);

    Ok(JsonEnvelope::new(
        command_name,
        ctx.mode.clone(),
        json!({
            "summary": {
                "scanned_items": items.len(),
                "finding_count": findings.len(),
            },
            "findings": findings,
            "warnings": warnings,
        }),
    ))
}

fn inspect(ctx: &crate::core::AppContext, id: &str) -> Result<JsonEnvelope<Value>> {
    let (items, _) = startup::scan_startup_dirs(&ctx.paths.startup_dirs);
    let findings = analyze_items(&items);
    let finding = findings
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| anyhow::anyhow!("protect finding not found: {id}"))?;

    Ok(JsonEnvelope::new(
        "protect inspect",
        ctx.mode.clone(),
        json!({
            "summary": format!("inspect: {} ({})", finding.label, finding.source),
            "finding": finding,
        }),
    ))
}

fn analyze_items(items: &[StartupItem]) -> Vec<ProtectFinding> {
    let mut findings = Vec::new();
    let shell_interpreters = [
        "sh",
        "bash",
        "zsh",
        "python",
        "python3",
        "ruby",
        "perl",
        "osascript",
        "node",
        "curl",
        "wget",
    ];

    for item in items {
        let mut evidence = Vec::new();
        let mut severity = RiskLevel::Low;
        let mut is_suspicious = false;

        // 1. Missing executable check
        if let Some(prog) = &item.program {
            if is_missing_executable(prog) {
                evidence.push(format!("Executable path does not exist: {}", prog));
                severity = RiskLevel::High;
                is_suspicious = true;
            }
        } else if let Some(first_arg) = item.program_arguments.first() {
            if is_missing_executable(first_arg) {
                evidence.push(format!("Executable path does not exist: {}", first_arg));
                severity = RiskLevel::High;
                is_suspicious = true;
            }
        }

        // 2. Shell interpreter check
        let launcher = item
            .program
            .as_deref()
            .or_else(|| item.program_arguments.first().map(|s| s.as_str()))
            .unwrap_or("");
        let launcher_name = Path::new(launcher)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if shell_interpreters.contains(&launcher_name) {
            is_suspicious = true;
            evidence.push(format!(
                "Uses shell interpreter as launcher: {}",
                launcher_name
            ));

            // Check if combined with temp paths, curl | sh, base64, etc.
            let mut combined_high = false;
            for arg in &item.program_arguments {
                let arg_lower = arg.to_lowercase();
                if arg.contains("/tmp")
                    || arg.contains("/var/tmp")
                    || arg.contains("Library/Caches")
                {
                    evidence.push(format!("References temp/cache path: {}", arg));
                    combined_high = true;
                }
                if arg_lower.contains("curl") && arg_lower.contains("sh") {
                    evidence
                        .push("Contains nested network download/execution (curl | sh)".to_string());
                    combined_high = true;
                }
                if arg_lower.contains("base64") {
                    evidence.push("Contains base64 decoding command".to_string());
                    combined_high = true;
                }
            }
            if item.run_at_load {
                evidence.push("Configured to run at load".to_string());
            }

            if combined_high {
                severity = RiskLevel::High;
            } else {
                severity = RiskLevel::Medium;
            }
        }

        // Determine if we should report this item
        if is_suspicious || item.is_system_item {
            let rule = if !evidence.is_empty() {
                if evidence.iter().any(|e| e.contains("does not exist")) {
                    "missing_executable"
                } else {
                    "shell_launcher"
                }
            } else {
                "system_item_baseline"
            };

            let id = compute_finding_id(&item.label, &item.path, rule);

            let recommendation = if !evidence.is_empty() {
                "Verify this item path and arguments for suspicious behavior.".to_string()
            } else {
                "No action required; system-provided baseline persistence.".to_string()
            };

            findings.push(ProtectFinding {
                id,
                source: item.source.clone(),
                label: item.label.clone(),
                path: item.path.clone(),
                severity,
                is_system_item: item.is_system_item,
                is_protected: item.is_system_item,
                evidence,
                recommendation,
                action: crate::core::PlannedActionKind::ReportOnly,
            });
        }
    }

    findings
}

fn is_missing_executable(path_str: &str) -> bool {
    if !path_str.starts_with('/') || path_str.contains('$') || path_str.contains('%') {
        return false;
    }
    !Path::new(path_str).exists()
}

fn compute_finding_id(label: &str, path: &Path, rule: &str) -> String {
    let path_str = path.to_string_lossy();
    let data = format!("{}:{}:{}", label, path_str, rule);
    let hash = blake3::hash(data.as_bytes()).to_hex();
    format!("protect_startup_{}", &hash[..16])
}

fn canonicalize_path(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    }
}

fn quarantined_filename(original_path: &Path) -> String {
    let stem = original_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = original_path.extension().and_then(|s| s.to_str());
    let path_str = original_path.to_string_lossy();
    let hash = blake3::hash(path_str.as_bytes()).to_hex().to_string();

    if let Some(e) = ext {
        format!("{}__{}.{}", stem, &hash[..16], e)
    } else {
        format!("{}__{}", stem, &hash[..16])
    }
}

fn quarantine(ctx: &crate::core::AppContext, id: &str) -> Result<JsonEnvelope<Value>> {
    if matches!(ctx.mode, crate::core::ExecutionMode::Permanent { .. }) {
        anyhow::bail!("Protect module does not support permanent delete yet.");
    }

    let (items, _) = startup::scan_startup_dirs(&ctx.paths.startup_dirs);
    let findings = analyze_items(&items);
    let finding = findings
        .into_iter()
        .find(|f| f.id == id)
        .ok_or_else(|| anyhow::anyhow!("protect finding not found: {id}"))?;

    let canon_path = canonicalize_path(&finding.path);

    // Revalidate immediately: must be within allowed user LaunchAgents
    let mut allowed = false;
    for (dir, source) in &ctx.paths.startup_dirs {
        if source == "user_launch_agents" {
            let canon_dir = canonicalize_path(dir);
            if canon_path.starts_with(&canon_dir) {
                allowed = true;
                break;
            }
        }
    }
    if !allowed {
        anyhow::bail!(
            "Suspicious file must be inside user LaunchAgents: {}",
            canon_path.display()
        );
    }

    // Safety checks
    let path_str = canon_path.to_string_lossy();
    if path_str.starts_with("/Library")
        || path_str.starts_with("/System")
        || path_str.starts_with("/Applications")
    {
        anyhow::bail!("System paths, /Library, /System, and /Applications are out of scope.");
    }

    let policy =
        crate::policy::Policy::new(ctx.paths.home.clone(), ctx.custom_protected_paths.clone());
    if policy.is_protected(&canon_path) {
        anyhow::bail!("Path is policy-protected: {}", canon_path.display());
    }

    let warnings = vec![
        "Changes may require logout/login or manual launchctl unload/load to take effect."
            .to_string(),
    ];

    let findings_list = vec![ScanFinding {
        id: FindingId(crate::core::new_id("finding")),
        module: "protect".to_string(),
        category: "user_launch_agents".to_string(),
        path: canon_path.clone(),
        size_bytes: canon_path.metadata().map(|m| m.len()).unwrap_or(0),
        risk: finding.severity,
        confidence: 1.0,
        action: PlannedActionKind::Quarantine,
        reason: format!("Quarantine suspicious file {}", finding.label),
        requires_sudo: false,
    }];

    let plan = crate::planner::build_action_plan(&findings_list, &ctx.mode);

    let mut audits = Vec::new();
    let mut moved_count = 0;
    let mut failed_count = 0;
    let mut rollback_id = None;

    if ctx.mode.is_destructive() {
        let quarantine_dir = ctx.paths.data_dir.join("quarantined_files");
        fs::create_dir_all(&quarantine_dir)?;

        let target_filename = quarantined_filename(&canon_path);
        let quarantine_path = quarantine_dir.join(&target_filename);
        let metadata_path = quarantine_path.with_extension("json");

        // Immediate Revalidation check again
        let final_canon = canonicalize_path(&canon_path);
        let mut allowed_final = false;
        for (dir, source) in &ctx.paths.startup_dirs {
            if source == "user_launch_agents" {
                let canon_dir = canonicalize_path(dir);
                if final_canon.starts_with(&canon_dir) {
                    allowed_final = true;
                    break;
                }
            }
        }

        if allowed_final
            && !policy.is_protected(&final_canon)
            && !final_canon.to_string_lossy().starts_with("/Library")
            && !final_canon.to_string_lossy().starts_with("/System")
            && !final_canon.to_string_lossy().starts_with("/Applications")
        {
            if quarantine_path.exists() {
                anyhow::bail!(
                    "Quarantine conflict: target quarantine path already exists: {}",
                    quarantine_path.display()
                );
            }

            match fs::rename(&final_canon, &quarantine_path) {
                Ok(()) => {
                    moved_count += 1;
                    let r_id = RollbackId(new_id("rollback"));
                    rollback_id = Some(r_id.0.clone());
                    let q_id = new_id("quarantine");

                    // Save sidecar metadata JSON
                    let meta = QuarantineMetadata {
                        quarantine_id: q_id,
                        finding_id: finding.id.clone(),
                        original_path: final_canon.clone(),
                        quarantine_path: quarantine_path.clone(),
                        metadata_path: metadata_path.clone(),
                        operation: "protect_quarantine".to_string(),
                        created_at: unix_now(),
                    };
                    fs::write(&metadata_path, serde_json::to_vec_pretty(&meta)?)?;

                    // Write RollbackEntry to rollback.json
                    let rollback_entry = RollbackEntry {
                        id: r_id,
                        original_path: final_canon.clone(),
                        current_path: quarantine_path.clone(),
                        created_at: unix_now(),
                        action: PlannedActionKind::MoveToTrash,
                    };
                    crate::audit::append_rollback(&ctx.paths.rollback_file, rollback_entry)?;

                    // Push audit log
                    audits.push(AuditLog {
                        id: AuditId(new_id("audit")),
                        timestamp: unix_now(),
                        command: "protect quarantine".to_string(),
                        action: PlannedActionKind::MoveToTrash,
                        path: final_canon,
                        size_bytes: plan.total_size_bytes,
                        status: "success".to_string(),
                        rollback_id: Some(RollbackId(rollback_id.clone().unwrap())),
                    });
                }
                Err(e) => {
                    failed_count += 1;
                    audits.push(AuditLog {
                        id: AuditId(new_id("audit")),
                        timestamp: unix_now(),
                        command: "protect quarantine".to_string(),
                        action: PlannedActionKind::MoveToTrash,
                        path: final_canon,
                        size_bytes: plan.total_size_bytes,
                        status: format!("failed: {e}"),
                        rollback_id: None,
                    });
                }
            }
        } else {
            failed_count += 1;
            audits.push(AuditLog {
                id: AuditId(new_id("audit")),
                timestamp: unix_now(),
                command: "protect quarantine".to_string(),
                action: PlannedActionKind::MoveToTrash,
                path: final_canon,
                size_bytes: plan.total_size_bytes,
                status: "failed: policy protection violation immediately before execution"
                    .to_string(),
                rollback_id: None,
            });
        }

        crate::audit::write_last_audit(&ctx.paths.audit_file, &audits)?;
    }

    let audit_id = audits.first().map(|log| log.id.0.clone());

    Ok(JsonEnvelope::new(
        "protect quarantine",
        ctx.mode.clone(),
        json!({
            "summary": format!("quarantine plan: {} ({} items)", finding.label, plan.total_items),
            "plan_kind": "protect_quarantine_dry_run",
            "execution": if ctx.mode.is_destructive() { "executed" } else { "not_executed" },
            "execution_result": if ctx.mode.is_destructive() {
                if failed_count > 0 { "partial_failure" } else { "success" }
            } else {
                "not_executed"
            },
            "audit_id": audit_id,
            "rollback_id": rollback_id,
            "moved_count": moved_count,
            "failed_count": failed_count,
            "plan": plan,
            "findings": findings_list,
            "warnings": warnings,
        }),
    ))
}

fn restore(ctx: &crate::core::AppContext, id: &str) -> Result<JsonEnvelope<Value>> {
    if matches!(ctx.mode, crate::core::ExecutionMode::Permanent { .. }) {
        anyhow::bail!("Protect module does not support permanent delete yet.");
    }

    let quarantine_dir = ctx.paths.data_dir.join("quarantined_files");
    if !quarantine_dir.exists() {
        anyhow::bail!("Quarantine directory does not exist.");
    }

    let entries = fs::read_dir(&quarantine_dir)?;
    let mut metadata_list = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        if let Ok(meta) = serde_json::from_str::<QuarantineMetadata>(&content) {
            if meta.operation == "protect_quarantine" {
                metadata_list.push(meta);
            }
        }
    }

    // Validate every loaded sidecar immediately
    let mut validated_metadata_list = Vec::new();
    for meta in metadata_list {
        let canon_quarantine = canonicalize_path(&meta.quarantine_path);
        let canon_quarantine_dir = canonicalize_path(&quarantine_dir);
        if !canon_quarantine.starts_with(&canon_quarantine_dir) {
            anyhow::bail!(
                "Malformed sidecar metadata: quarantine path outside quarantine directory"
            );
        }

        let canon_orig = canonicalize_path(&meta.original_path);
        let mut allowed = false;
        for (dir, source) in &ctx.paths.startup_dirs {
            if source == "user_launch_agents" {
                let canon_dir = canonicalize_path(dir);
                if canon_orig.starts_with(&canon_dir) {
                    allowed = true;
                    break;
                }
            }
        }
        if !allowed {
            anyhow::bail!("Malformed sidecar metadata: original path outside user LaunchAgents");
        }

        validated_metadata_list.push(meta);
    }

    // Lookup matches
    // 1. exact quarantine_id
    let mut quarantine_id_matches = Vec::new();
    for meta in &validated_metadata_list {
        if meta.quarantine_id == id {
            quarantine_id_matches.push(meta.clone());
        }
    }

    // 2. exact finding_id
    let mut finding_id_matches = Vec::new();
    if quarantine_id_matches.is_empty() {
        for meta in &validated_metadata_list {
            if meta.finding_id == id {
                finding_id_matches.push(meta.clone());
            }
        }
    }

    let resolved_meta = if quarantine_id_matches.len() == 1 {
        quarantine_id_matches.remove(0)
    } else if quarantine_id_matches.len() > 1 {
        anyhow::bail!(
            "Ambiguous restore query: multiple matches found for '{}'",
            id
        );
    } else if finding_id_matches.len() == 1 {
        finding_id_matches.remove(0)
    } else if finding_id_matches.len() > 1 {
        anyhow::bail!(
            "Ambiguous restore query: multiple matches found for '{}'",
            id
        );
    } else {
        anyhow::bail!("Quarantine record not found for query: '{}'", id);
    };

    if !resolved_meta.quarantine_path.exists() {
        anyhow::bail!(
            "Quarantined file not found: {}",
            resolved_meta.quarantine_path.display()
        );
    }

    // Handle existing destination conflict
    if resolved_meta.original_path.exists() {
        anyhow::bail!(
            "Restore conflict: target path already exists: {}",
            resolved_meta.original_path.display()
        );
    }

    // Revalidate target and original folders
    let canon_disabled_dir = canonicalize_path(&quarantine_dir);
    let canon_orig = canonicalize_path(&resolved_meta.original_path);
    let canon_disabled = canonicalize_path(&resolved_meta.quarantine_path);

    let mut allowed = false;
    for (dir, source) in &ctx.paths.startup_dirs {
        if source == "user_launch_agents" {
            let canon_dir = canonicalize_path(dir);
            if canon_orig.starts_with(&canon_dir) {
                allowed = true;
                break;
            }
        }
    }
    if !allowed {
        anyhow::bail!(
            "Restore item must be inside user LaunchAgents: {}",
            canon_orig.display()
        );
    }
    if !canon_disabled.starts_with(&canon_disabled_dir) {
        anyhow::bail!(
            "Quarantined item must be inside quarantine directory: {}",
            canon_disabled.display()
        );
    }

    let policy =
        crate::policy::Policy::new(ctx.paths.home.clone(), ctx.custom_protected_paths.clone());
    if policy.is_protected(&canon_orig) || policy.is_protected(&canon_disabled) {
        anyhow::bail!("Restore paths are policy-protected.");
    }

    let warnings = vec![
        "Changes may require logout/login or manual launchctl unload/load to take effect."
            .to_string(),
    ];

    let findings_list = vec![ScanFinding {
        id: FindingId(crate::core::new_id("finding")),
        module: "protect".to_string(),
        category: "user_launch_agents".to_string(),
        path: canon_disabled.clone(),
        size_bytes: canon_disabled.metadata().map(|m| m.len()).unwrap_or(0),
        risk: RiskLevel::Low,
        confidence: 1.0,
        action: PlannedActionKind::Quarantine,
        reason: format!("Restore quarantined file {}", resolved_meta.quarantine_id),
        requires_sudo: false,
    }];

    let plan = crate::planner::build_action_plan(&findings_list, &ctx.mode);

    let mut audits = Vec::new();
    let mut moved_count = 0;
    let mut failed_count = 0;
    let mut rollback_id = None;

    if ctx.mode.is_destructive() {
        let mut allowed_final = false;
        for (dir, source) in &ctx.paths.startup_dirs {
            if source == "user_launch_agents" {
                let canon_dir = canonicalize_path(dir);
                if canon_orig.starts_with(&canon_dir) {
                    allowed_final = true;
                    break;
                }
            }
        }

        if allowed_final
            && canon_disabled.starts_with(&canon_disabled_dir)
            && !policy.is_protected(&canon_orig)
            && !policy.is_protected(&canon_disabled)
        {
            if canon_orig.exists() {
                anyhow::bail!(
                    "Restore conflict: target path already exists: {}",
                    canon_orig.display()
                );
            }

            match fs::rename(&canon_disabled, &canon_orig) {
                Ok(()) => {
                    moved_count += 1;
                    let r_id = RollbackId(new_id("rollback"));
                    rollback_id = Some(r_id.0.clone());

                    // Clean up sidecar metadata JSON
                    if resolved_meta.metadata_path.exists() {
                        let _ = fs::remove_file(&resolved_meta.metadata_path);
                    }

                    // Write RollbackEntry to rollback.json
                    let rollback_entry = RollbackEntry {
                        id: r_id,
                        original_path: canon_disabled.clone(),
                        current_path: canon_orig.clone(),
                        created_at: unix_now(),
                        action: PlannedActionKind::MoveToTrash,
                    };
                    crate::audit::append_rollback(&ctx.paths.rollback_file, rollback_entry)?;

                    audits.push(AuditLog {
                        id: AuditId(new_id("audit")),
                        timestamp: unix_now(),
                        command: "protect restore".to_string(),
                        action: PlannedActionKind::MoveToTrash,
                        path: canon_disabled,
                        size_bytes: plan.total_size_bytes,
                        status: "success".to_string(),
                        rollback_id: Some(RollbackId(rollback_id.clone().unwrap())),
                    });
                }
                Err(e) => {
                    failed_count += 1;
                    audits.push(AuditLog {
                        id: AuditId(new_id("audit")),
                        timestamp: unix_now(),
                        command: "protect restore".to_string(),
                        action: PlannedActionKind::MoveToTrash,
                        path: canon_disabled,
                        size_bytes: plan.total_size_bytes,
                        status: format!("failed: {e}"),
                        rollback_id: None,
                    });
                }
            }
        } else {
            failed_count += 1;
            audits.push(AuditLog {
                id: AuditId(new_id("audit")),
                timestamp: unix_now(),
                command: "protect restore".to_string(),
                action: PlannedActionKind::MoveToTrash,
                path: canon_disabled,
                size_bytes: plan.total_size_bytes,
                status: "failed: policy protection violation immediately before execution"
                    .to_string(),
                rollback_id: None,
            });
        }

        crate::audit::write_last_audit(&ctx.paths.audit_file, &audits)?;
    }

    let audit_id = audits.first().map(|log| log.id.0.clone());

    Ok(JsonEnvelope::new(
        "protect restore",
        ctx.mode.clone(),
        json!({
            "summary": format!("restore plan: {} ({} items)", resolved_meta.quarantine_id, plan.total_items),
            "plan_kind": "protect_restore_dry_run",
            "execution": if ctx.mode.is_destructive() { "executed" } else { "not_executed" },
            "execution_result": if ctx.mode.is_destructive() {
                if failed_count > 0 { "partial_failure" } else { "success" }
            } else {
                "not_executed"
            },
            "audit_id": audit_id,
            "rollback_id": rollback_id,
            "moved_count": moved_count,
            "failed_count": failed_count,
            "plan": plan,
            "findings": findings_list,
            "warnings": warnings,
        }),
    ))
}
