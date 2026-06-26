use crate::{
    audit,
    cli::*,
    core::{JsonEnvelope, PlannedActionKind, ScanFinding},
    executor, planner,
    policy::Policy,
    scanner,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub mod cleanup {
    use super::*;

    pub fn run(ctx: &crate::core::AppContext, args: CleanupArgs) -> Result<JsonEnvelope<Value>> {
        let policy = Policy::new(ctx.paths.home.clone());
        let roots = policy.cleanup_roots(&args.category);
        if !args.category.is_empty() && roots.len() != args.category.len() {
            bail!("invalid cleanup category; supported: cache, user_cache, logs, temp, xcode");
        }
        let mut findings = Vec::new();
        let mut warnings = Vec::new();

        for (category, root, risk) in &roots {
            if !root.exists() {
                continue;
            }
            let scan =
                scanner::cleanup_candidates(root, category, *risk, args.older_than_days, || {
                    ctx.is_cancelled()
                });
            warnings.extend(scan.warnings);
            for mut finding in scan.findings {
                if policy.allowed_cleanup_path(&finding.path, &roots) {
                    policy.enforce_finding(&mut finding);
                    findings.push(finding);
                }
            }
        }

        let plan = planner::build_action_plan(&findings, &ctx.mode);
        let audits = if ctx.mode.is_destructive() {
            executor::execute_plan(ctx, "cleanup", &plan)?
        } else {
            Vec::new()
        };
        let summary = format!(
            "cleanup: {} findings, {} bytes",
            findings.len(),
            plan.total_size_bytes
        );
        Ok(JsonEnvelope::new(
            "cleanup",
            ctx.mode.clone(),
            json!({
                "summary": summary,
                "findings": findings,
                "action_plan": plan,
                "audit": audits,
                "warnings": warnings
            }),
        ))
    }
}

pub mod disk {
    use super::*;

    pub fn run(ctx: &crate::core::AppContext, args: DiskArgs) -> Result<JsonEnvelope<Value>> {
        let root = args.path.unwrap_or_else(|| ctx.paths.home.clone());
        let items = top_entries(&root, args.depth, args.top, 0)?;
        Ok(JsonEnvelope::new(
            "disk",
            ctx.mode.clone(),
            json!({
                "summary": format!("disk: {} entries under {}", items.len(), root.display()),
                "items": items
            }),
        ))
    }
}

pub mod clutter {
    use super::*;

    pub fn run(ctx: &crate::core::AppContext, args: ClutterArgs) -> Result<JsonEnvelope<Value>> {
        let root = args
            .path
            .unwrap_or_else(|| ctx.paths.home.join("Downloads"));
        let items = top_entries(&root, usize::MAX, args.top, args.min_size)?;
        let policy = Policy::new(ctx.paths.home.clone());
        let mut findings: Vec<ScanFinding> = items
            .iter()
            .map(|item| ScanFinding {
                id: crate::core::FindingId(crate::core::new_id("finding")),
                module: "clutter".into(),
                category: "large_file".into(),
                path: PathBuf::from(item["path"].as_str().unwrap_or_default()),
                size_bytes: item["size_bytes"].as_u64().unwrap_or_default(),
                risk: crate::core::RiskLevel::Medium,
                confidence: 0.75,
                action: PlannedActionKind::MoveToTrash,
                reason: "large file candidate".into(),
                requires_sudo: false,
            })
            .collect();
        for finding in &mut findings {
            policy.enforce_finding(finding);
        }
        let plan = planner::build_action_plan(&findings, &ctx.mode);
        let audits = if ctx.mode.is_destructive() {
            executor::execute_plan(ctx, "clutter", &plan)?
        } else {
            Vec::new()
        };
        Ok(JsonEnvelope::new(
            "clutter",
            ctx.mode.clone(),
            json!({
                "summary": format!("clutter: {} large files under {}", findings.len(), root.display()),
                "items": items,
                "findings": findings,
                "action_plan": plan,
                "audit": audits
            }),
        ))
    }
}

pub mod duplicates {
    use super::*;

    pub fn run(ctx: &crate::core::AppContext, args: DuplicatesArgs) -> Result<JsonEnvelope<Value>> {
        let roots = if args.paths.is_empty() {
            vec![ctx.paths.home.join("Downloads")]
        } else {
            args.paths
        };
        let groups = duplicate_groups(&roots, args.min_size)?;
        Ok(JsonEnvelope::new(
            "duplicates",
            ctx.mode.clone(),
            json!({
                "summary": format!("duplicates: {} groups", groups.len()),
                "groups": groups
            }),
        ))
    }
}

pub mod scan {
    use super::*;

    pub fn run(ctx: &crate::core::AppContext, args: ScanArgs) -> Result<JsonEnvelope<Value>> {
        if ctx.mode.is_destructive() {
            eprintln!(
                "macmop scan is always dry-run. Ignoring --{}.",
                ctx.mode.as_str()
            );
        }
        let dry_ctx = ctx.with_mode(crate::core::ExecutionMode::DryRun);
        let cleanup = crate::modules::cleanup::run(
            &dry_ctx,
            CleanupArgs {
                category: Vec::new(),
                older_than_days: 30,
            },
        )?;
        let disk = crate::modules::disk::run(
            &dry_ctx,
            DiskArgs {
                path: Some(ctx.paths.home.clone()),
                depth: if args.profile == "deep" { 4 } else { 2 },
                top: 20,
            },
        )?;
        Ok(JsonEnvelope::new(
            "scan",
            ctx.mode.clone(),
            json!({
                "summary": format!("scan profile {}", args.profile),
                "modules": {
                    "cleanup": cleanup.payload,
                    "disk": disk.payload
                }
            }),
        ))
    }
}

pub mod report {
    use super::*;

    pub fn run(ctx: &crate::core::AppContext, args: ReportArgs) -> Result<JsonEnvelope<Value>> {
        match args.command {
            ReportCommand::Last => {
                let entries = audit::read_last_audit(&ctx.paths.audit_file)?;
                Ok(JsonEnvelope::new(
                    "report",
                    ctx.mode.clone(),
                    json!({
                        "summary": format!("report last: {} audit entries", entries.len()),
                        "items": entries
                    }),
                ))
            }
        }
    }
}

pub mod rollback {
    use super::*;

    pub fn run(ctx: &crate::core::AppContext, args: RollbackArgs) -> Result<JsonEnvelope<Value>> {
        match args.command {
            RollbackCommand::List => {
                let entries = audit::read_rollbacks(&ctx.paths.rollback_file)?;
                Ok(JsonEnvelope::new(
                    "rollback",
                    ctx.mode.clone(),
                    json!({
                        "summary": format!("rollback: {} entries", entries.len()),
                        "items": entries
                    }),
                ))
            }
            RollbackCommand::Apply { id } => {
                let mut entries = audit::read_rollbacks(&ctx.paths.rollback_file)?;
                let index = entries
                    .iter()
                    .position(|entry| entry.id.0 == id)
                    .context("rollback id not found")?;
                let entry = entries.remove(index);
                if ctx.mode.is_destructive() {
                    if let Some(parent) = entry.original_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&entry.current_path, &entry.original_path).with_context(|| {
                        format!(
                            "cannot restore {} to {}",
                            entry.current_path.display(),
                            entry.original_path.display()
                        )
                    })?;
                    audit::write_rollbacks(&ctx.paths.rollback_file, &entries)?;
                }
                Ok(JsonEnvelope::new(
                    "rollback",
                    ctx.mode.clone(),
                    json!({
                        "summary": format!("rollback apply {}", id),
                        "restored": entry,
                        "applied": ctx.mode.is_destructive()
                    }),
                ))
            }
        }
    }
}

fn top_entries(root: &Path, max_depth: usize, top: usize, min_size: u64) -> Result<Vec<Value>> {
    let mut heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    for entry in WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size < min_size {
            continue;
        }
        heap.push(Reverse((size, entry.path().to_path_buf())));
        if heap.len() > top {
            heap.pop();
        }
    }
    let mut items: Vec<_> = heap
        .into_iter()
        .map(|Reverse((size, path))| json!({ "path": path, "size_bytes": size }))
        .collect();
    items.sort_by(|a, b| b["size_bytes"].as_u64().cmp(&a["size_bytes"].as_u64()));
    Ok(items)
}

fn duplicate_groups(roots: &[PathBuf], min_size: u64) -> Result<Vec<Value>> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for root in roots {
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if entry.file_type().is_symlink() || !entry.file_type().is_file() {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size >= min_size {
                by_size
                    .entry(size)
                    .or_default()
                    .push(entry.path().to_path_buf());
            }
        }
    }

    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter().filter(|(_, paths)| paths.len() > 1) {
        let mut by_partial: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if let Ok(hash) = partial_hash(&path) {
                by_partial.entry(hash).or_default().push(path);
            }
        }
        for paths in by_partial.into_values().filter(|paths| paths.len() > 1) {
            let mut by_full: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
            for path in paths {
                if let Ok(hash) = full_hash(&path) {
                    by_full.entry(hash).or_default().push(path);
                }
            }
            for duplicates in by_full.into_values().filter(|paths| paths.len() > 1) {
                let keep = duplicates[0].clone();
                let delete_candidates: Vec<_> = duplicates.iter().skip(1).cloned().collect();
                groups.push(json!({
                    "size_bytes": size,
                    "keep": keep,
                    "delete_candidates": delete_candidates,
                    "count": duplicates.len()
                }));
            }
        }
    }
    Ok(groups)
}

fn partial_hash(path: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0; 4096];
    let read = file.read(&mut buf)?;
    hasher.update(&buf[..read]);
    let len = file.metadata()?.len();
    if len > 4096 {
        file.seek(SeekFrom::Start(len / 2))?;
        let read = file.read(&mut buf)?;
        hasher.update(&buf[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn full_hash(path: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

pub mod apps {
    use super::*;
    use crate::core::{AppAssociation, AppBundle, AppLeftover, LeftoverConfidence, RiskLevel};
    use std::collections::HashSet;

    pub fn run(
        ctx: &crate::core::AppContext,
        args: crate::cli::AppsArgs,
    ) -> Result<JsonEnvelope<Value>> {
        match args.command {
            crate::cli::AppsCommand::List => list(ctx),
            crate::cli::AppsCommand::Inspect { app } => inspect(ctx, &app),
            crate::cli::AppsCommand::Leftovers => leftovers(ctx),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_group_never_selects_all_files() {
        let group = json!({
            "keep": "/tmp/a",
            "delete_candidates": ["/tmp/b"],
            "count": 2
        });
        assert!(
            group["delete_candidates"].as_array().unwrap().len()
                < group["count"].as_u64().unwrap() as usize
        );
    }
}
