use super::*;
use crate::core::{
    AppAssociation, AppBundle, AppLeftover, FindingId, LeftoverConfidence, RiskLevel,
};
use std::collections::HashSet;

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::AppsArgs,
) -> Result<JsonEnvelope<Value>> {
    match args.command {
        crate::cli::AppsCommand::List => list(ctx),
        crate::cli::AppsCommand::Inspect { app } => inspect(ctx, &app),
        crate::cli::AppsCommand::Leftovers => leftovers(ctx),
        crate::cli::AppsCommand::Uninstall { app } => uninstall(ctx, &app),
    }
}

// ── list ──────────────────────────────────────────────────────────────────

fn list(ctx: &crate::core::AppContext) -> Result<JsonEnvelope<Value>> {
    let bundles = discover_bundles(&ctx.paths.apps_dirs);
    Ok(JsonEnvelope::new(
        "apps list",
        ctx.mode.clone(),
        json!({
            "summary": format!("apps: {} installed", bundles.len()),
            "items": bundles,
        }),
    ))
}

// ── inspect ───────────────────────────────────────────────────────────────

fn inspect(ctx: &crate::core::AppContext, app: &str) -> Result<JsonEnvelope<Value>> {
    let path = resolve_app_path(app, &ctx.paths.apps_dirs)
        .ok_or_else(|| anyhow::anyhow!("app not found: {app}"))?;
    let bundle = read_bundle(&path);
    let associations = associated_files(&bundle, &ctx.paths.home);
    let total_assoc_bytes: u64 = associations.iter().map(|a| a.size_bytes).sum();
    Ok(JsonEnvelope::new(
        "apps inspect",
        ctx.mode.clone(),
        json!({
            "summary": format!("inspect: {} ({})", bundle.name, bundle.bundle_id),
            "bundle": bundle,
            "associations": associations,
            "total_associated_bytes": total_assoc_bytes,
        }),
    ))
}

// ── leftovers ─────────────────────────────────────────────────────────────

fn leftovers(ctx: &crate::core::AppContext) -> Result<JsonEnvelope<Value>> {
    // O(n) pass: build known bundle_id set first
    let bundles = discover_bundles(&ctx.paths.apps_dirs);
    let known_ids: HashSet<String> = bundles.iter().map(|b| b.bundle_id.clone()).collect();
    let known_names: HashSet<String> = bundles.iter().map(|b| b.name.clone()).collect();

    let items = scan_leftovers(&ctx.paths.home, &known_ids, &known_names);
    Ok(JsonEnvelope::new(
        "apps leftovers",
        ctx.mode.clone(),
        json!({
            "summary": format!("leftovers: {} orphaned entries", items.len()),
            "items": items,
        }),
    ))
}

// ── uninstall ─────────────────────────────────────────────────────────────

fn canonicalize_path(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    }
}

fn resolve_uninstall_app(app: &str, apps_dirs: &[PathBuf]) -> Result<PathBuf> {
    // 1. Direct path check
    let candidate = PathBuf::from(app);
    if candidate.is_absolute() || candidate.components().count() > 1 {
        if candidate.extension().and_then(|e| e.to_str()) != Some("app") {
            anyhow::bail!("Direct path must end with .app: {}", candidate.display());
        }
        if !candidate.exists() {
            anyhow::bail!("App path does not exist: {}", candidate.display());
        }
        return Ok(candidate);
    }
    if candidate.extension().and_then(|e| e.to_str()) == Some("app") && candidate.exists() {
        return Ok(candidate);
    }

    let bundles = discover_bundles(apps_dirs);
    let query_app = app.strip_suffix(".app").unwrap_or(app);
    let get_stem = |p: &Path| -> String {
        p.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    };

    // 2. Exact app bundle name match
    let mut exact_bundle_matches = Vec::new();
    for b in &bundles {
        let stem = get_stem(&b.path);
        if stem == query_app {
            exact_bundle_matches.push(b.path.clone());
        }
    }
    if exact_bundle_matches.len() == 1 {
        return Ok(exact_bundle_matches.remove(0));
    } else if exact_bundle_matches.len() > 1 {
        return return_ambiguous_error(app, exact_bundle_matches);
    }

    // Try case-insensitive exact bundle name match
    let mut exact_bundle_ci_matches = Vec::new();
    for b in &bundles {
        let stem = get_stem(&b.path);
        if stem.eq_ignore_ascii_case(query_app) {
            exact_bundle_ci_matches.push(b.path.clone());
        }
    }
    if exact_bundle_ci_matches.len() == 1 {
        return Ok(exact_bundle_ci_matches.remove(0));
    } else if exact_bundle_ci_matches.len() > 1 {
        return return_ambiguous_error(app, exact_bundle_ci_matches);
    }

    // 3. Exact display name / CFBundleName match
    let mut exact_display_matches = Vec::new();
    for b in &bundles {
        if b.name == query_app {
            exact_display_matches.push(b.path.clone());
        }
    }
    if exact_display_matches.len() == 1 {
        return Ok(exact_display_matches.remove(0));
    } else if exact_display_matches.len() > 1 {
        return return_ambiguous_error(app, exact_display_matches);
    }

    // Try case-insensitive exact display name match
    let mut exact_display_ci_matches = Vec::new();
    for b in &bundles {
        if b.name.eq_ignore_ascii_case(query_app) {
            exact_display_ci_matches.push(b.path.clone());
        }
    }
    if exact_display_ci_matches.len() == 1 {
        return Ok(exact_display_ci_matches.remove(0));
    } else if exact_display_ci_matches.len() > 1 {
        return return_ambiguous_error(app, exact_display_ci_matches);
    }

    // 4. Fuzzy substring match
    let mut fuzzy_matches = Vec::new();
    let query_lower = query_app.to_lowercase();
    for b in &bundles {
        let stem = get_stem(&b.path).to_lowercase();
        let name_lower = b.name.to_lowercase();
        let id_lower = b.bundle_id.to_lowercase();
        if stem.contains(&query_lower)
            || name_lower.contains(&query_lower)
            || id_lower.contains(&query_lower)
        {
            fuzzy_matches.push(b.path.clone());
        }
    }

    if fuzzy_matches.is_empty() {
        anyhow::bail!("app not found: {app}");
    } else if fuzzy_matches.len() == 1 {
        Ok(fuzzy_matches.remove(0))
    } else {
        return_ambiguous_error(app, fuzzy_matches)
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

fn uninstall(ctx: &crate::core::AppContext, app: &str) -> Result<JsonEnvelope<Value>> {
    let resolved_path = resolve_uninstall_app(app, &ctx.paths.apps_dirs)?;
    let app_canonical = canonicalize_path(&resolved_path);

    // Safety checks: block system/protected app bundles
    let policy =
        crate::policy::Policy::new(ctx.paths.home.clone(), ctx.custom_protected_paths.clone());
    if policy.is_protected(&app_canonical) {
        anyhow::bail!(
            "Uninstalling system/protected app is blocked: {}",
            app_canonical.display()
        );
    }

    let bundle = read_bundle(&app_canonical);
    if bundle.is_system_app || bundle.risk == RiskLevel::Critical {
        anyhow::bail!(
            "Uninstalling system/protected app is blocked: {}",
            app_canonical.display()
        );
    }

    let mut findings = Vec::new();

    // 1. Add app bundle itself as a finding
    findings.push(ScanFinding {
        id: FindingId(crate::core::new_id("finding")),
        module: "apps".to_string(),
        category: "app_bundle".to_string(),
        path: app_canonical.clone(),
        size_bytes: bundle.size_bytes,
        risk: RiskLevel::Low,
        confidence: 1.0,
        action: PlannedActionKind::MoveToTrash,
        reason: format!("App bundle for {}", bundle.name),
        requires_sudo: false,
    });

    // 2. Discover associated leftovers (conservative candidate locations only)
    let assoc = associated_files(&bundle, &ctx.paths.home);
    for item in assoc {
        if item.exists {
            let leftover_canonical = canonicalize_path(&item.path);
            if policy.is_protected(&leftover_canonical) {
                // leftover under protected path is blocked/excluded
                continue;
            }
            findings.push(ScanFinding {
                id: FindingId(crate::core::new_id("finding")),
                module: "apps".to_string(),
                category: item.kind.clone(),
                path: leftover_canonical,
                size_bytes: item.size_bytes,
                risk: RiskLevel::Low,
                confidence: 1.0,
                action: PlannedActionKind::MoveToTrash,
                reason: format!("Associated leftover for {}: {}", bundle.name, item.kind),
                requires_sudo: false,
            });
        }
    }

    // Build the ActionPlan
    let plan = crate::planner::build_action_plan(&findings, &crate::core::ExecutionMode::DryRun);

    Ok(JsonEnvelope::new(
        "apps uninstall",
        ctx.mode.clone(),
        json!({
            "summary": format!("uninstall plan: {} ({} items, {} bytes)", bundle.name, plan.total_items, plan.total_size_bytes),
            "plan_kind": "apps_uninstall_dry_run",
            "execution": "not_executed",
            "plan": plan,
            "findings": findings,
        }),
    ))
}

// ── internals ─────────────────────────────────────────────────────────────

/// Discover all .app bundles under each apps_dir at depth=1 only (O(n)).
fn discover_bundles(apps_dirs: &[PathBuf]) -> Vec<AppBundle> {
    let mut bundles = Vec::new();
    for dir in apps_dirs {
        if !dir.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("app") {
                continue;
            }
            bundles.push(read_bundle(&path));
        }
    }
    bundles
}

/// Read Info.plist from a .app bundle and build an AppBundle.
pub fn read_bundle(path: &Path) -> AppBundle {
    let plist_path = path.join("Contents/Info.plist");
    let (bundle_id, version, display_name) = parse_info_plist(&plist_path);
    let name = display_name
        .or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "Unknown".to_string());

    let size_bytes = dir_size(path);
    let is_system = is_system_bundle(path, &bundle_id);
    let risk = if is_system {
        RiskLevel::Critical
    } else {
        RiskLevel::Low
    };
    AppBundle {
        name,
        path: path.to_path_buf(),
        bundle_id,
        version,
        size_bytes,
        is_system_app: is_system,
        risk,
    }
}

/// Parse CFBundleIdentifier, CFBundleShortVersionString, CFBundleDisplayName from plist.
fn parse_info_plist(plist_path: &Path) -> (String, String, Option<String>) {
    let Ok(val) = plist::Value::from_file(plist_path) else {
        return ("unknown".to_string(), "unknown".to_string(), None);
    };
    let dict = match &val {
        plist::Value::Dictionary(d) => d,
        _ => return ("unknown".to_string(), "unknown".to_string(), None),
    };
    let bundle_id = dict
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .unwrap_or("unknown")
        .to_string();
    let version = dict
        .get("CFBundleShortVersionString")
        .or_else(|| dict.get("CFBundleVersion"))
        .and_then(|v| v.as_string())
        .unwrap_or("unknown")
        .to_string();
    let display_name = dict
        .get("CFBundleDisplayName")
        .or_else(|| dict.get("CFBundleName"))
        .and_then(|v| v.as_string())
        .map(|s| s.to_string());
    (bundle_id, version, display_name)
}

/// Resolve an app name/path to a concrete path by searching apps_dirs.
fn resolve_app_path(app: &str, apps_dirs: &[PathBuf]) -> Option<PathBuf> {
    let candidate = PathBuf::from(app);
    if candidate.exists() && candidate.extension().and_then(|e| e.to_str()) == Some("app") {
        return Some(candidate);
    }
    // Search apps_dirs for matching basename
    let search = if app.ends_with(".app") {
        app.to_string()
    } else {
        format!("{app}.app")
    };
    for dir in apps_dirs {
        let p = dir.join(&search);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Enumerate the ~8 standard associated paths for a given bundle.
pub fn associated_files(bundle: &AppBundle, home: &Path) -> Vec<AppAssociation> {
    let id = &bundle.bundle_id;
    let name = &bundle.name;
    let candidates: &[(&str, String)] = &[
        (
            "application_support_id",
            format!("Library/Application Support/{id}"),
        ),
        (
            "application_support_name",
            format!("Library/Application Support/{name}"),
        ),
        ("caches", format!("Library/Caches/{id}")),
        ("preferences", format!("Library/Preferences/{id}.plist")),
        ("logs_id", format!("Library/Logs/{id}")),
        ("logs_name", format!("Library/Logs/{name}")),
        (
            "saved_state",
            format!("Library/Saved Application State/{id}.savedState"),
        ),
        ("containers", format!("Library/Containers/{id}")),
    ];
    candidates
        .iter()
        .map(|(kind, rel)| {
            let path = home.join(rel);
            let exists = path.exists();
            let size_bytes = if exists { dir_size(&path) } else { 0 };
            AppAssociation {
                path,
                kind: kind.to_string(),
                size_bytes,
                exists,
            }
        })
        .collect()
}

/// Scan known leftover dirs in ~/Library for orphaned bundle entries (O(n)).
fn scan_leftovers(
    home: &Path,
    known_ids: &HashSet<String>,
    known_names: &HashSet<String>,
) -> Vec<AppLeftover> {
    // Dirs and their entry-kind labels
    let scan_dirs: &[(&str, &str)] = &[
        ("Library/Application Support", "application_support"),
        ("Library/Caches", "caches"),
        ("Library/Containers", "containers"),
        ("Library/Saved Application State", "saved_state"),
    ];
    let mut leftovers = Vec::new();
    for (rel, _kind) in scan_dirs {
        let dir = home.join(rel);
        if !dir.exists() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let entry_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .trim_end_matches(".savedState")
                .trim_end_matches(".plist")
                .to_string();

            // Skip if it matches a known bundle_id or app name
            if known_ids.contains(&entry_name) || known_names.contains(&entry_name) {
                continue;
            }

            // Confidence heuristic: bundle-id-like strings are medium/high
            let confidence = if looks_like_bundle_id(&entry_name) {
                LeftoverConfidence::Medium
            } else {
                LeftoverConfidence::Low
            };

            let size_bytes = dir_size(&path);
            leftovers.push(AppLeftover {
                path,
                size_bytes,
                confidence,
                associated_bundle_id: entry_name,
                action: PlannedActionKind::ReportOnly,
            });
        }
    }
    // Also scan Preferences for orphaned .plist files
    let prefs_dir = home.join("Library/Preferences");
    if prefs_dir.exists() {
        let Ok(entries) = fs::read_dir(&prefs_dir) else {
            return leftovers;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("plist") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if known_ids.contains(&stem) || known_names.contains(&stem) {
                continue;
            }
            if !looks_like_bundle_id(&stem) {
                continue;
            }
            let size_bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
            leftovers.push(AppLeftover {
                path,
                size_bytes,
                confidence: LeftoverConfidence::Medium,
                associated_bundle_id: stem,
                action: PlannedActionKind::ReportOnly,
            });
        }
    }
    leftovers
}

/// A name looks like a bundle ID if it contains dots and has 2+ segments.
fn looks_like_bundle_id(s: &str) -> bool {
    let parts: Vec<_> = s.split('.').collect();
    parts.len() >= 2 && parts.iter().all(|p| !p.is_empty())
}

fn is_system_bundle(path: &Path, bundle_id: &str) -> bool {
    path.starts_with("/System")
        || path.starts_with("/Library")
        || bundle_id.starts_with("com.apple.")
}

/// Recursively sum directory/file size (O(n) single pass).
fn dir_size(path: &Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}
