use super::*;
use crate::core::{HomeSummary, StatusSummary, SCHEMA_VERSION};

const HOME_ENTRY_LIMIT: usize = 10_000;
const HOME_MAX_DEPTH: usize = 3;

pub fn run(ctx: &crate::core::AppContext) -> Result<JsonEnvelope<Value>> {
    let mut warnings = Vec::new();

    let last_audit_exists = ctx.paths.audit_file.exists();
    let last_audit_entry_count = if last_audit_exists {
        match audit::read_last_audit(&ctx.paths.audit_file) {
            Ok(entries) => entries.len(),
            Err(error) => {
                warnings.push(format!(
                    "could not read audit file {}: {error}",
                    ctx.paths.audit_file.display()
                ));
                0
            }
        }
    } else {
        0
    };

    let rollback_entry_count = if ctx.paths.rollback_file.exists() {
        match audit::read_rollbacks(&ctx.paths.rollback_file) {
            Ok(entries) => entries.len(),
            Err(error) => {
                warnings.push(format!(
                    "could not read rollback file {}: {error}",
                    ctx.paths.rollback_file.display()
                ));
                0
            }
        }
    } else {
        0
    };

    let home_summary = summarize_home(&ctx.paths.home);
    warnings.extend(home_summary.warnings.clone());

    let summary = StatusSummary {
        version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: SCHEMA_VERSION.to_string(),
        test_mode: std::env::var("MACMOP_TEST_MODE")
            .map(|value| value == "1")
            .unwrap_or(false),
        home: ctx.paths.home.clone(),
        data_dir: ctx.paths.data_dir.clone(),
        trash: ctx.paths.trash.clone(),
        audit_file: ctx.paths.audit_file.clone(),
        rollback_file: ctx.paths.rollback_file.clone(),
        last_audit_exists,
        last_audit_entry_count,
        rollback_entry_count,
        available_modules: available_modules(),
        home_summary,
    };

    Ok(JsonEnvelope::new(
        "status",
        ctx.mode.clone(),
        json!({
            "summary": summary,
            "warnings": warnings,
        }),
    ))
}

fn available_modules() -> Vec<String> {
    [
        "cleanup",
        "disk",
        "clutter",
        "duplicates",
        "scan",
        "apps",
        "startup",
        "protect",
        "privacy",
        "maintenance",
        "report",
        "rollback",
        "status",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn summarize_home(home: &Path) -> HomeSummary {
    let mut summary = HomeSummary {
        path: home.to_path_buf(),
        exists: home.exists(),
        sampled_file_count: 0,
        sampled_dir_count: 0,
        sampled_size_bytes: 0,
        entry_limit_reached: false,
        warnings: Vec::new(),
    };

    if !summary.exists {
        return summary;
    }

    for (visited_entries, entry) in WalkDir::new(home)
        .max_depth(HOME_MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        if visited_entries >= HOME_ENTRY_LIMIT {
            summary.entry_limit_reached = true;
            break;
        }
        match entry {
            Ok(entry) => {
                if entry.file_type().is_symlink() {
                    continue;
                }
                if entry.file_type().is_dir() {
                    summary.sampled_dir_count += 1;
                    continue;
                }
                if entry.file_type().is_file() {
                    summary.sampled_file_count += 1;
                    match entry.metadata() {
                        Ok(metadata) => {
                            summary.sampled_size_bytes =
                                summary.sampled_size_bytes.saturating_add(metadata.len());
                        }
                        Err(error) => {
                            summary.warnings.push(format!(
                                "could not read metadata for {}: {error}",
                                entry.path().display()
                            ));
                        }
                    }
                }
            }
            Err(error) => {
                let path = error
                    .path()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| home.to_path_buf());
                summary
                    .warnings
                    .push(format!("could not scan {}: {error}", path.display()));
            }
        }
    }

    summary
}
