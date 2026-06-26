use super::*;
use crate::core::{RiskLevel, StartupItem};

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::StartupArgs,
) -> Result<JsonEnvelope<Value>> {
    match args.command {
        crate::cli::StartupCommand::List => list(ctx),
        crate::cli::StartupCommand::Inspect { id } => inspect(ctx, &id),
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
