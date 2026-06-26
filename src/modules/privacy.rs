use super::*;
use crate::core::{PlannedActionKind, PrivacyFinding};

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::PrivacyArgs,
) -> Result<JsonEnvelope<Value>> {
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

    let home = &ctx.paths.home;

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

    Ok(JsonEnvelope::new(
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
