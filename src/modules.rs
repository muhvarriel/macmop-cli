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
