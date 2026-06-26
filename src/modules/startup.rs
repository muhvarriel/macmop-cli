use super::*;
use crate::core::{
    new_id, unix_now, AuditId, AuditLog, FindingId, PlannedActionKind, RiskLevel, RollbackEntry,
    RollbackId, ScanFinding, StartupItem,
};

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::StartupArgs,
) -> Result<JsonEnvelope<Value>> {
    match args.command {
        crate::cli::StartupCommand::List => list(ctx),
        crate::cli::StartupCommand::Inspect { id } => inspect(ctx, &id),
        crate::cli::StartupCommand::Disable { id } => disable(ctx, &id),
        crate::cli::StartupCommand::Enable { id } => enable(ctx, &id),
    }
}

// ── list ──────────────────────────────────────────────────────────────────

fn list(ctx: &crate::core::AppContext) -> Result<JsonEnvelope<Value>> {
    let (items, warnings) = scan_startup_dirs(&ctx.paths.startup_dirs);
    Ok(JsonEnvelope::new(
        "startup list",
        ctx.mode.clone(),
        json!({
            "summary": format!("startup: {} items", items.len()),
            "items": items,
            "warnings": warnings,
        }),
    ))
}

// ── inspect ───────────────────────────────────────────────────────────────

fn inspect(ctx: &crate::core::AppContext, id: &str) -> Result<JsonEnvelope<Value>> {
    let (items, _) = scan_startup_dirs(&ctx.paths.startup_dirs);
    let item = items
        .into_iter()
        .find(|i| i.label == id || i.id == id)
        .ok_or_else(|| anyhow::anyhow!("startup item not found: {id}"))?;
    Ok(JsonEnvelope::new(
        "startup inspect",
        ctx.mode.clone(),
        json!({
            "summary": format!("inspect: {} ({})", item.label, item.source),
            "item": item,
        }),
    ))
}
// ── disable / enable ──────────────────────────────────────────────────────

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
struct StartupMetadata {
    original_path: PathBuf,
    disabled_path: PathBuf,
    label: String,
    operation: String,
}

fn canonicalize_path(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    }
}

fn disabled_filename(label: &str, original_path: &Path) -> String {
    let path_str = original_path.to_string_lossy();
    let hash = blake3::hash(path_str.as_bytes()).to_hex().to_string();
    format!("{}__{}.plist", label, &hash[..16])
}

fn resolve_disable_item(ctx: &crate::core::AppContext, id: &str) -> Result<PathBuf> {
    let candidate = PathBuf::from(id);
    if candidate.is_absolute() || candidate.components().count() > 1 {
        if candidate.extension().and_then(|e| e.to_str()) != Some("plist") {
            anyhow::bail!("Direct path must end with .plist: {}", candidate.display());
        }
        if !candidate.exists() {
            anyhow::bail!("Startup path does not exist: {}", candidate.display());
        }
        return Ok(candidate);
    }
    if candidate.extension().and_then(|e| e.to_str()) == Some("plist") && candidate.exists() {
        return Ok(candidate);
    }

    let mut user_dirs = Vec::new();
    for (dir, source) in &ctx.paths.startup_dirs {
        if source == "user_launch_agents" {
            user_dirs.push((dir.clone(), source.clone()));
        }
    }
    let (items, _) = scan_startup_dirs(&user_dirs);
    let query = id.strip_suffix(".plist").unwrap_or(id);

    // 1. Exact plist filename
    let mut exact_filename_matches = Vec::new();
    for item in &items {
        if let Some(filename) = item.path.file_name().and_then(|n| n.to_str()) {
            if filename == id || filename.strip_suffix(".plist") == Some(query) {
                exact_filename_matches.push(item.path.clone());
            }
        }
    }
    if exact_filename_matches.len() == 1 {
        return Ok(exact_filename_matches.remove(0));
    } else if exact_filename_matches.len() > 1 {
        return return_ambiguous_error(id, exact_filename_matches);
    }

    // 2. Exact Label
    let mut exact_label_matches = Vec::new();
    for item in &items {
        if item.label == query {
            exact_label_matches.push(item.path.clone());
        }
    }
    if exact_label_matches.len() == 1 {
        return Ok(exact_label_matches.remove(0));
    } else if exact_label_matches.len() > 1 {
        return return_ambiguous_error(id, exact_label_matches);
    }

    // 3. Fuzzy match
    let mut fuzzy_matches = Vec::new();
    let query_lower = query.to_lowercase();
    for item in &items {
        let label_lower = item.label.to_lowercase();
        let filename_lower = item
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if label_lower.contains(&query_lower) || filename_lower.contains(&query_lower) {
            fuzzy_matches.push(item.path.clone());
        }
    }

    if fuzzy_matches.is_empty() {
        anyhow::bail!("startup item not found: {id}");
    } else if fuzzy_matches.len() == 1 {
        Ok(fuzzy_matches.remove(0))
    } else {
        return_ambiguous_error(id, fuzzy_matches)
    }
}

fn return_ambiguous_error(query: &str, mut paths: Vec<PathBuf>) -> Result<PathBuf> {
    paths.sort();
    paths.dedup();
    let total = paths.len();
    let display_paths: Vec<String> = paths
        .iter()
        .take(10)
        .map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();

    let list_str = display_paths.join(", ");
    if total > 10 {
        anyhow::bail!(
            "Ambiguous query: multiple apps match '{}'. Please specify one of: {}, ...and {} more",
            query,
            list_str,
            total - 10
        );
    } else {
        anyhow::bail!(
            "Ambiguous query: multiple apps match '{}'. Please specify one of: {}",
            query,
            list_str
        );
    }
}

fn resolve_enable_item(ctx: &crate::core::AppContext, id: &str) -> Result<StartupMetadata> {
    let disabled_dir = ctx.paths.data_dir.join("disabled_launchagents");
    if !disabled_dir.exists() {
        anyhow::bail!("startup item not found in disabled directory: {id}");
    }

    let entries = fs::read_dir(&disabled_dir)?;
    let mut metadata_list = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        if let Ok(meta) = serde_json::from_str::<StartupMetadata>(&content) {
            if meta.operation == "startup_disable" {
                // Ensure disabled plist actually exists
                if meta.disabled_path.exists() {
                    metadata_list.push(meta);
                }
            }
        }
    }

    let query = id.strip_suffix(".plist").unwrap_or(id);

    // 1. Exact plist filename
    let mut exact_filename_matches = Vec::new();
    for meta in &metadata_list {
        if let Some(filename) = meta.original_path.file_name().and_then(|n| n.to_str()) {
            if filename == id || filename.strip_suffix(".plist") == Some(query) {
                exact_filename_matches.push(meta.clone());
            }
        }
    }
    if exact_filename_matches.len() == 1 {
        return Ok(exact_filename_matches.remove(0));
    } else if exact_filename_matches.len() > 1 {
        return return_ambiguous_enable_error(id, exact_filename_matches);
    }

    // 2. Exact Label
    let mut exact_label_matches = Vec::new();
    for meta in &metadata_list {
        if meta.label == query {
            exact_label_matches.push(meta.clone());
        }
    }
    if exact_label_matches.len() == 1 {
        return Ok(exact_label_matches.remove(0));
    } else if exact_label_matches.len() > 1 {
        return return_ambiguous_enable_error(id, exact_label_matches);
    }

    // 3. Fuzzy match
    let mut fuzzy_matches = Vec::new();
    let query_lower = query.to_lowercase();
    for meta in &metadata_list {
        let label_lower = meta.label.to_lowercase();
        let filename_lower = meta
            .original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        if label_lower.contains(&query_lower) || filename_lower.contains(&query_lower) {
            fuzzy_matches.push(meta.clone());
        }
    }

    if fuzzy_matches.is_empty() {
        anyhow::bail!("startup item not found in disabled directory: {id}");
    } else if fuzzy_matches.len() == 1 {
        Ok(fuzzy_matches.remove(0))
    } else {
        return_ambiguous_enable_error(id, fuzzy_matches)
    }
}

fn return_ambiguous_enable_error(
    query: &str,
    mut list: Vec<StartupMetadata>,
) -> Result<StartupMetadata> {
    list.sort_by(|a, b| a.label.cmp(&b.label));
    let total = list.len();
    let display_paths: Vec<String> = list
        .iter()
        .take(10)
        .map(|meta| meta.label.clone())
        .collect();

    let list_str = display_paths.join(", ");
    if total > 10 {
        anyhow::bail!(
            "Ambiguous query: multiple disabled apps match '{}'. Please specify one of: {}, ...and {} more",
            query,
            list_str,
            total - 10
        );
    } else {
        anyhow::bail!(
            "Ambiguous query: multiple disabled apps match '{}'. Please specify one of: {}",
            query,
            list_str
        );
    }
}

fn disable(ctx: &crate::core::AppContext, id: &str) -> Result<JsonEnvelope<Value>> {
    if matches!(ctx.mode, crate::core::ExecutionMode::Permanent { .. }) {
        anyhow::bail!("Startup module does not support permanent delete yet.");
    }

    let resolved_path = resolve_disable_item(ctx, id)?;
    let canon_path = canonicalize_path(&resolved_path);

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
            "Startup item must be inside user LaunchAgents: {}",
            canon_path.display()
        );
    }

    // Policy check
    let policy =
        crate::policy::Policy::new(ctx.paths.home.clone(), ctx.custom_protected_paths.clone());
    if policy.is_protected(&canon_path) {
        anyhow::bail!("Startup item is policy-protected: {}", canon_path.display());
    }

    let item = parse_startup_plist(&canon_path, "user_launch_agents")?;

    // Warnings
    let warnings = vec![
        "Changes may require logout/login or manual launchctl unload/load to take effect."
            .to_string(),
    ];

    let findings = vec![ScanFinding {
        id: FindingId(crate::core::new_id("finding")),
        module: "startup".to_string(),
        category: "user_launch_agents".to_string(),
        path: canon_path.clone(),
        size_bytes: canon_path.metadata().map(|m| m.len()).unwrap_or(0),
        risk: item.risk,
        confidence: 1.0,
        action: PlannedActionKind::MoveToTrash,
        reason: format!("Disable LaunchAgent {}", item.label),
        requires_sudo: false,
    }];

    let plan = crate::planner::build_action_plan(&findings, &ctx.mode);

    let mut audits = Vec::new();
    let mut moved_count = 0;
    let mut failed_count = 0;
    let mut rollback_id = None;

    if ctx.mode.is_destructive() {
        let disabled_dir = ctx.paths.data_dir.join("disabled_launchagents");
        fs::create_dir_all(&disabled_dir)?;

        let target_filename = disabled_filename(&item.label, &canon_path);
        let disabled_path = disabled_dir.join(&target_filename);

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

        if allowed_final && !policy.is_protected(&final_canon) {
            match fs::rename(&final_canon, &disabled_path) {
                Ok(()) => {
                    moved_count += 1;
                    let r_id = RollbackId(new_id("rollback"));
                    rollback_id = Some(r_id.0.clone());

                    // Save sidecar metadata JSON
                    let meta = StartupMetadata {
                        original_path: final_canon.clone(),
                        disabled_path: disabled_path.clone(),
                        label: item.label.clone(),
                        operation: "startup_disable".to_string(),
                    };
                    let meta_path = disabled_path.with_extension("plist.json");
                    fs::write(&meta_path, serde_json::to_vec_pretty(&meta)?)?;

                    // Write RollbackEntry to rollback.json
                    let rollback_entry = RollbackEntry {
                        id: r_id,
                        original_path: final_canon.clone(),
                        current_path: disabled_path.clone(),
                        created_at: unix_now(),
                        action: PlannedActionKind::MoveToTrash,
                    };
                    audit::append_rollback(&ctx.paths.rollback_file, rollback_entry)?;

                    // Push audit log
                    audits.push(AuditLog {
                        id: AuditId(new_id("audit")),
                        timestamp: unix_now(),
                        command: "startup disable".to_string(),
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
                        command: "startup disable".to_string(),
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
                command: "startup disable".to_string(),
                action: PlannedActionKind::MoveToTrash,
                path: final_canon,
                size_bytes: plan.total_size_bytes,
                status: "failed: policy protection violation immediately before execution"
                    .to_string(),
                rollback_id: None,
            });
        }

        audit::write_last_audit(&ctx.paths.audit_file, &audits)?;
    }

    let audit_id = audits.first().map(|log| log.id.0.clone());

    Ok(JsonEnvelope::new(
        "startup disable",
        ctx.mode.clone(),
        json!({
            "summary": format!("disable plan: {} ({} items)", item.label, plan.total_items),
            "plan_kind": "startup_disable_dry_run",
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
            "findings": findings,
            "warnings": warnings,
        }),
    ))
}

fn enable(ctx: &crate::core::AppContext, id: &str) -> Result<JsonEnvelope<Value>> {
    if matches!(ctx.mode, crate::core::ExecutionMode::Permanent { .. }) {
        anyhow::bail!("Startup module does not support permanent delete yet.");
    }

    let meta = resolve_enable_item(ctx, id)?;

    // Handle existing destination
    if meta.original_path.exists() {
        anyhow::bail!(
            "Enable conflict: target path already exists: {}",
            meta.original_path.display()
        );
    }

    // Revalidate target and original folders
    let disabled_dir = ctx.paths.data_dir.join("disabled_launchagents");
    let canon_disabled_dir = canonicalize_path(&disabled_dir);

    let canon_orig = canonicalize_path(&meta.original_path);
    let canon_disabled = canonicalize_path(&meta.disabled_path);

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
            "Startup item must be inside user LaunchAgents: {}",
            canon_orig.display()
        );
    }
    if !canon_disabled.starts_with(&canon_disabled_dir) {
        anyhow::bail!(
            "Disabled item must be inside disabled directory: {}",
            canon_disabled.display()
        );
    }

    let policy =
        crate::policy::Policy::new(ctx.paths.home.clone(), ctx.custom_protected_paths.clone());
    if policy.is_protected(&canon_orig) || policy.is_protected(&canon_disabled) {
        anyhow::bail!("Startup paths are policy-protected.");
    }

    // Warnings
    let warnings = vec![
        "Changes may require logout/login or manual launchctl unload/load to take effect."
            .to_string(),
    ];

    let findings = vec![ScanFinding {
        id: FindingId(crate::core::new_id("finding")),
        module: "startup".to_string(),
        category: "user_launch_agents".to_string(),
        path: canon_disabled.clone(),
        size_bytes: canon_disabled.metadata().map(|m| m.len()).unwrap_or(0),
        risk: RiskLevel::Low,
        confidence: 1.0,
        action: PlannedActionKind::MoveToTrash,
        reason: format!("Enable LaunchAgent {}", meta.label),
        requires_sudo: false,
    }];

    let plan = crate::planner::build_action_plan(&findings, &ctx.mode);

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
                    "Enable conflict: target path already exists: {}",
                    canon_orig.display()
                );
            }

            match fs::rename(&canon_disabled, &canon_orig) {
                Ok(()) => {
                    moved_count += 1;
                    let r_id = RollbackId(new_id("rollback"));
                    rollback_id = Some(r_id.0.clone());

                    // Write RollbackEntry to rollback.json
                    let rollback_entry = RollbackEntry {
                        id: r_id,
                        original_path: canon_disabled.clone(),
                        current_path: canon_orig.clone(),
                        created_at: unix_now(),
                        action: PlannedActionKind::MoveToTrash,
                    };
                    audit::append_rollback(&ctx.paths.rollback_file, rollback_entry)?;

                    audits.push(AuditLog {
                        id: AuditId(new_id("audit")),
                        timestamp: unix_now(),
                        command: "startup enable".to_string(),
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
                        command: "startup enable".to_string(),
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
                command: "startup enable".to_string(),
                action: PlannedActionKind::MoveToTrash,
                path: canon_disabled,
                size_bytes: plan.total_size_bytes,
                status: "failed: policy protection violation immediately before execution"
                    .to_string(),
                rollback_id: None,
            });
        }

        audit::write_last_audit(&ctx.paths.audit_file, &audits)?;
    }

    let audit_id = audits.first().map(|log| log.id.0.clone());

    Ok(JsonEnvelope::new(
        "startup enable",
        ctx.mode.clone(),
        json!({
            "summary": format!("enable plan: {} ({} items)", meta.label, plan.total_items),
            "plan_kind": "startup_enable_dry_run",
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
            "findings": findings,
            "warnings": warnings,
        }),
    ))
}

// ── internals ─────────────────────────────────────────────────────────────

/// Scan all startup_dirs, parse each .plist, collect items and scan-level warnings.
/// O(n) over all plist files across all dirs.
pub fn scan_startup_dirs(startup_dirs: &[(PathBuf, String)]) -> (Vec<StartupItem>, Vec<String>) {
    let mut items = Vec::new();
    let mut scan_warnings = Vec::new();

    for (dir, source) in startup_dirs {
        if !dir.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            scan_warnings.push(format!("cannot read directory: {}", dir.display()));
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("plist") {
                continue;
            }
            match parse_startup_plist(&path, source) {
                Ok(item) => items.push(item),
                Err(e) => scan_warnings.push(format!(
                    "skipped {}: {e}",
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                )),
            }
        }
    }
    (items, scan_warnings)
}

/// Parse a single LaunchAgent/Daemon plist into a StartupItem.
/// Malformed-but-readable plists produce warnings on the item rather than Err.
pub fn parse_startup_plist(path: &Path, source: &str) -> Result<StartupItem> {
    let val =
        plist::Value::from_file(path).map_err(|e| anyhow::anyhow!("plist parse error: {e}"))?;

    let dict = match &val {
        plist::Value::Dictionary(d) => d,
        _ => anyhow::bail!("plist root is not a dictionary"),
    };

    let mut warnings: Vec<String> = Vec::new();

    // Label — required; fall back to filename stem
    let label = dict
        .get("Label")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            warnings.push("missing Label key; using filename as id".to_string());
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    // Program + ProgramArguments
    let program_arguments: Vec<String> = dict
        .get("ProgramArguments")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let program = dict
        .get("Program")
        .and_then(|v| v.as_string())
        .map(|s| s.to_string())
        .or_else(|| program_arguments.first().cloned());

    // RunAtLoad
    let run_at_load = dict
        .get("RunAtLoad")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    // KeepAlive: can be bool or dict (treat any dict as true)
    let keep_alive = match dict.get("KeepAlive") {
        Some(plist::Value::Boolean(b)) => *b,
        Some(plist::Value::Dictionary(_)) => {
            warnings.push("KeepAlive is a condition dict; treated as true".to_string());
            true
        }
        _ => false,
    };

    let is_system = source == "system_launch_agents" || source == "system_launch_daemons";

    let risk = if is_system {
        RiskLevel::Critical
    } else if run_at_load {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    Ok(StartupItem {
        id: label.clone(),
        label,
        path: path.to_path_buf(),
        program,
        program_arguments,
        run_at_load,
        keep_alive,
        source: source.to_string(),
        is_system_item: is_system,
        risk,
        warnings,
        action: PlannedActionKind::ReportOnly,
    })
}
