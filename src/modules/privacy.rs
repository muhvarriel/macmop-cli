use super::*;
use crate::core::{
    new_id, unix_now, AuditId, AuditLog, FindingId, PlannedActionKind, PrivacyFinding, RiskLevel,
    RollbackEntry, RollbackId, ScanFinding,
};
use std::fs;
use std::path::{Path, PathBuf};

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::PrivacyArgs,
) -> Result<JsonEnvelope<Value>> {
    if matches!(ctx.mode, crate::core::ExecutionMode::Permanent { .. }) {
        anyhow::bail!("Privacy module does not support permanent delete yet.");
    }

    if ctx.mode.is_destructive() && matches!(args.command, crate::cli::PrivacyCommand::Scan) {
        anyhow::bail!("privacy scan --apply is not supported. Use privacy browsers --apply or privacy recent --apply instead.");
    }

    match args.command {
        crate::cli::PrivacyCommand::Scan => scan(ctx, "privacy scan", true, true, true, true),
        crate::cli::PrivacyCommand::Browsers => {
            scan(ctx, "privacy browsers", true, false, false, false)
        }
        crate::cli::PrivacyCommand::Recent => scan(ctx, "privacy recent", false, true, false, true),
    }
}

fn scan(
    ctx: &crate::core::AppContext,
    command_name: &str,
    include_browsers: bool,
    include_recent: bool,
    include_quicklook: bool,
    include_shell: bool,
) -> Result<JsonEnvelope<Value>> {
    let mut findings = Vec::new();
    let mut warnings = Vec::new();

    let home = canonicalize_path(&ctx.paths.home);

    // Helper to add finding
    let mut add_finding = |category: &str, path: PathBuf, detail: &str, is_dir: bool| {
        if !path.exists() {
            return;
        }
        let (size, count, warns) = if is_dir {
            get_dir_metadata(&path)
        } else {
            get_file_metadata(&path)
        };
        warnings.extend(warns);

        let id = format!(
            "privacy_{}_{}",
            category,
            &blake3::hash(path.to_string_lossy().as_bytes()).to_hex()[..16]
        );

        findings.push(PrivacyFinding {
            id,
            category: category.to_string(),
            path,
            size_bytes: size,
            count: Some(count),
            detail: detail.to_string(),
            action: PlannedActionKind::ReportOnly,
        });
    };

    // 1. Browsers
    if include_browsers {
        add_finding(
            "browser_cache",
            home.join("Library/Caches/com.apple.Safari"),
            "Safari cache directory detected",
            true,
        );
        add_finding(
            "browser_cache",
            home.join("Library/Caches/Google/Chrome"),
            "Chrome cache directory detected",
            true,
        );
        add_finding(
            "browser_cache",
            home.join("Library/Caches/Firefox"),
            "Firefox cache directory detected",
            true,
        );
        let ff_support = home.join("Library/Application Support/Firefox/Profiles");
        if ff_support.exists() {
            if let Ok(entries) = fs::read_dir(&ff_support) {
                for entry in entries.flatten() {
                    let cache2 = entry.path().join("cache2");
                    if cache2.exists() {
                        add_finding(
                            "browser_cache",
                            cache2,
                            "Firefox profile cache2 directory detected",
                            true,
                        );
                    }
                }
            }
        }
    }

    // 2. Recent items
    if include_recent {
        add_finding(
            "recent_items",
            home.join("Library/Application Support/com.apple.sharedfilelist"),
            "Recent items list folder detected",
            true,
        );
        add_finding(
            "recent_items",
            home.join("Library/Preferences/com.apple.finder.plist"),
            "Finder preferences file detected",
            false,
        );
    }

    // 3. QuickLook cache
    if include_quicklook {
        for ql_dir in &ctx.paths.quicklook_dirs {
            add_finding(
                "quicklook_cache",
                ql_dir.clone(),
                "QuickLook thumbnail cache directory detected",
                true,
            );
        }
    }

    // 4. Shell history
    if include_shell {
        add_finding(
            "shell_history",
            home.join(".zsh_history"),
            "Zsh shell history file detected",
            false,
        );
        add_finding(
            "shell_history",
            home.join(".bash_history"),
            "Bash shell history file detected",
            false,
        );
        add_finding(
            "shell_history",
            home.join(".fish_history"),
            "Fish shell history file detected",
            false,
        );
    }

    // Process running warning (non-blocking)
    if include_browsers {
        if is_app_running("Safari") {
            warnings.push(
                "Safari is currently running. Cache cleanup might not take effect or cause issues."
                    .to_string(),
            );
        }
        if is_app_running("Google Chrome") || is_app_running("chrome") {
            warnings.push("Google Chrome is currently running. Cache cleanup might not take effect or cause issues.".to_string());
        }
        if is_app_running("Firefox") {
            warnings.push("Firefox is currently running. Cache cleanup might not take effect or cause issues.".to_string());
        }
    }

    // Backwards compatibility: return simple scan payload for "privacy scan" in dry-run
    if command_name == "privacy scan" && !ctx.mode.is_destructive() {
        return Ok(JsonEnvelope::new(
            command_name,
            ctx.mode.clone(),
            json!({
                "summary": {
                    "scanned_categories": {
                        "browser_cache": include_browsers,
                        "recent_items": include_recent,
                        "quicklook_cache": include_quicklook,
                        "shell_history": include_shell,
                    },
                    "finding_count": findings.len(),
                },
                "findings": findings,
                "warnings": warnings,
            }),
        ));
    }

    // Otherwise, build action plan and return structured output
    let mut scan_findings = Vec::new();
    for finding in &findings {
        if (finding.category == "browser_cache" && command_name.contains("browsers"))
            || (finding.category == "recent_items" && command_name.contains("recent"))
        {
            scan_findings.push(ScanFinding {
                id: FindingId(finding.id.clone()),
                module: "privacy".to_string(),
                category: finding.category.clone(),
                path: finding.path.clone(),
                size_bytes: finding.size_bytes,
                risk: RiskLevel::Low,
                confidence: 1.0,
                action: PlannedActionKind::MoveToTrash,
                reason: finding.detail.clone(),
                requires_sudo: false,
            });
        }
    }

    let plan = crate::planner::build_action_plan(&scan_findings, &ctx.mode);
    let mut audits = Vec::new();
    let mut moved_count = 0;
    let mut failed_count = 0;
    let mut rollback_id = None;

    if ctx.mode.is_destructive() {
        let custom_protected: Vec<PathBuf> = ctx
            .custom_protected_paths
            .iter()
            .map(|p| canonicalize_path(p))
            .collect();
        let policy = crate::policy::Policy::new(home.clone(), custom_protected);
        let mut successfully_moved = Vec::new();

        for action in &plan.actions {
            let canon_path = canonicalize_path(&action.path);
            let mut allowed_final = false;

            let finding = findings.iter().find(|f| f.id == action.finding_id.0);
            let category = finding.map(|f| f.category.as_str()).unwrap_or("");

            // Revalidate: must be inside home
            if canon_path.starts_with(&home) && is_allowed_privacy_path(ctx, &canon_path, category)
            {
                allowed_final = true;
            }

            if allowed_final && !policy.is_protected(&canon_path) {
                match move_to_trash(ctx, &canon_path) {
                    Ok(rollback_entry) => {
                        moved_count += 1;
                        successfully_moved.push(rollback_entry.clone());

                        audits.push(AuditLog {
                            id: AuditId(new_id("audit")),
                            timestamp: unix_now(),
                            command: format!("privacy {}", category),
                            action: PlannedActionKind::MoveToTrash,
                            path: canon_path,
                            size_bytes: action.path.metadata().map(|m| m.len()).unwrap_or(0),
                            status: "success".to_string(),
                            rollback_id: None,
                        });
                    }
                    Err(e) => {
                        failed_count += 1;
                        audits.push(AuditLog {
                            id: AuditId(new_id("audit")),
                            timestamp: unix_now(),
                            command: format!("privacy {}", category),
                            action: PlannedActionKind::MoveToTrash,
                            path: canon_path,
                            size_bytes: action.path.metadata().map(|m| m.len()).unwrap_or(0),
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
                    command: format!("privacy {}", category),
                    action: PlannedActionKind::MoveToTrash,
                    path: canon_path,
                    size_bytes: action.path.metadata().map(|m| m.len()).unwrap_or(0),
                    status: "failed: policy protection or validation violation".to_string(),
                    rollback_id: None,
                });
            }
        }

        if !successfully_moved.is_empty() {
            let r_id = RollbackId(new_id("rollback"));
            rollback_id = Some(r_id.0.clone());

            for mut entry in successfully_moved {
                entry.id = r_id.clone();
                crate::audit::append_rollback(&ctx.paths.rollback_file, entry)?;
            }

            for log in &mut audits {
                if log.status == "success" {
                    log.rollback_id = Some(r_id.clone());
                }
            }
        }

        crate::audit::write_last_audit(&ctx.paths.audit_file, &audits)?;
    }

    let audit_id = audits.first().map(|log| log.id.0.clone());

    Ok(JsonEnvelope::new(
        command_name,
        ctx.mode.clone(),
        json!({
            "summary": format!("privacy cleanup: {} findings", findings.len()),
            "plan_kind": "privacy_cleanup_dry_run",
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

fn get_dir_metadata(dir: &Path) -> (u64, usize, Vec<String>) {
    let mut total_size = 0;
    let mut file_count = 0;
    let mut warnings = Vec::new();

    if !dir.exists() {
        return (0, 0, warnings);
    }

    if let Ok(meta) = dir.metadata() {
        total_size += meta.len();
    }

    let walker = WalkDir::new(dir).into_iter();
    for entry in walker {
        match entry {
            Ok(e) => {
                if e.file_type().is_file() {
                    match e.metadata() {
                        Ok(m) => {
                            total_size += m.len();
                            file_count += 1;
                        }
                        Err(err) => {
                            if let Some(io_err) = err.io_error() {
                                if io_err.kind() == std::io::ErrorKind::PermissionDenied {
                                    warnings
                                        .push(format!("permission denied: {}", e.path().display()));
                                }
                            }
                        }
                    }
                }
            }
            Err(err) => {
                if let Some(io_err) = err.io_error() {
                    if io_err.kind() == std::io::ErrorKind::PermissionDenied {
                        let path_str = err
                            .path()
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "unknown".to_string());
                        warnings.push(format!("permission denied: {}", path_str));
                    }
                }
            }
        }
    }
    (total_size, file_count, warnings)
}

fn get_file_metadata(path: &Path) -> (u64, usize, Vec<String>) {
    let mut warnings = Vec::new();
    if !path.exists() {
        return (0, 0, warnings);
    }
    match path.metadata() {
        Ok(meta) => (meta.len(), 1, warnings),
        Err(err) => {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                warnings.push(format!("permission denied: {}", path.display()));
            }
            (0, 0, warnings)
        }
    }
}

fn canonicalize_path(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => path.to_path_buf(),
    }
}

fn is_allowed_privacy_path(ctx: &crate::core::AppContext, path: &Path, category: &str) -> bool {
    let home = canonicalize_path(&ctx.paths.home);
    let canon_path = canonicalize_path(path);

    if category == "browser_cache" {
        let safari_cache = canonicalize_path(&home.join("Library/Caches/com.apple.Safari"));
        let chrome_cache = canonicalize_path(&home.join("Library/Caches/Google/Chrome"));
        let firefox_cache = canonicalize_path(&home.join("Library/Caches/Firefox"));

        if canon_path == safari_cache || canon_path == chrome_cache || canon_path == firefox_cache {
            return true;
        }

        // Firefox profile cache2: home/Library/Application Support/Firefox/Profiles/<profile>/cache2
        let ff_support = home.join("Library/Application Support/Firefox/Profiles");
        if canon_path.starts_with(&ff_support)
            && canon_path.file_name().and_then(|n| n.to_str()) == Some("cache2")
        {
            return true;
        }
    } else if category == "recent_items" {
        let sharedfilelist =
            canonicalize_path(&home.join("Library/Application Support/com.apple.sharedfilelist"));
        let finder_plist =
            canonicalize_path(&home.join("Library/Preferences/com.apple.finder.plist"));

        if canon_path == sharedfilelist || canon_path == finder_plist {
            return true;
        }
    }
    false
}

fn is_app_running(bundle_name: &str) -> bool {
    if let Ok(output) = std::process::Command::new("pgrep")
        .arg("-f")
        .arg(bundle_name)
        .output()
    {
        !output.stdout.is_empty()
    } else {
        false
    }
}

fn move_to_trash(ctx: &crate::core::AppContext, path: &Path) -> Result<RollbackEntry> {
    if !path.exists() {
        anyhow::bail!("path does not exist: {}", path.display());
    }
    let trash = &ctx.paths.trash;
    fs::create_dir_all(trash)?;
    let file_name = path.file_name().context("path has no file name")?;

    let mut target = trash.join(file_name);
    if target.exists() {
        for i in 1.. {
            let candidate = trash.join(format!("{}.{}", file_name.to_string_lossy(), i));
            if !candidate.exists() {
                target = candidate;
                break;
            }
        }
    }

    fs::rename(path, &target)?;

    Ok(RollbackEntry {
        id: RollbackId(new_id("rollback")),
        original_path: path.to_path_buf(),
        current_path: target,
        created_at: unix_now(),
        action: PlannedActionKind::MoveToTrash,
    })
}
