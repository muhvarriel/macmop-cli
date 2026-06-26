use super::*;
use crate::cli::{CloudArgs, CloudCommand};
use crate::core::{CloudProvider, CloudScanSummary};

pub fn load_cloud_scan(
    ctx: &crate::core::AppContext,
) -> Result<(CloudScanSummary, Vec<CloudProvider>)> {
    let sync_warning = "Warning: Deleting synchronized files may immediately affect cloud copies and remote sync state.".to_string();
    let mut providers_detected = 0;
    let mut total_sampled_size_bytes: u64 = 0;
    let mut items = Vec::new();

    for (name, path) in &ctx.paths.cloud_dirs {
        let exists = path.exists();
        if !exists {
            continue;
        }
        providers_detected += 1;

        let mut sampled_file_count = 0;
        let mut sampled_dir_count = 0;
        let mut sampled_size_bytes: u64 = 0;
        let mut scan_limited = false;
        let mut warnings = Vec::new();

        for (visited_entries, entry) in WalkDir::new(path)
            .max_depth(3)
            .follow_links(false)
            .into_iter()
            .enumerate()
        {
            if visited_entries >= 10_000 {
                scan_limited = true;
                warnings.push(
                    "Scan was bounded; sampled values may not represent the full cloud folder."
                        .to_string(),
                );
                break;
            }
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_symlink() {
                        continue;
                    }
                    if entry.file_type().is_dir() {
                        sampled_dir_count += 1;
                    } else if entry.file_type().is_file() {
                        sampled_file_count += 1;
                        match entry.metadata() {
                            Ok(metadata) => {
                                sampled_size_bytes =
                                    sampled_size_bytes.saturating_add(metadata.len());
                            }
                            Err(error) => {
                                warnings.push(format!(
                                    "could not read metadata for {}: {error}",
                                    entry.path().display()
                                ));
                            }
                        }
                    }
                }
                Err(error) => {
                    let path_err = error
                        .path()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| path.clone());
                    warnings.push(format!("could not scan {}: {error}", path_err.display()));
                }
            }
        }

        total_sampled_size_bytes = total_sampled_size_bytes.saturating_add(sampled_size_bytes);

        let provider_info = CloudProvider {
            provider: name.clone(),
            path: path.clone(),
            exists: true,
            sampled_file_count,
            sampled_dir_count,
            sampled_size_bytes,
            scan_limited,
            warnings,
            action: PlannedActionKind::ReportOnly,
        };
        items.push(provider_info);
    }
    let mut total_sampled_file_count = 0;
    let mut total_sampled_dir_count = 0;
    let mut scan_limited = false;
    for item in &items {
        total_sampled_file_count += item.sampled_file_count;
        total_sampled_dir_count += item.sampled_dir_count;
        if item.scan_limited {
            scan_limited = true;
        }
    }

    let summary = CloudScanSummary {
        total_providers: ctx.paths.cloud_dirs.len(),
        providers_detected,
        total_sampled_file_count,
        total_sampled_dir_count,
        total_sampled_size_bytes,
        scan_limited,
        sync_warning,
    };

    Ok((summary, items))
}

pub fn run(ctx: &crate::core::AppContext, args: CloudArgs) -> Result<JsonEnvelope<Value>> {
    let sync_warning = "Warning: Deleting synchronized files may immediately affect cloud copies and remote sync state.".to_string();

    match args.command {
        CloudCommand::Providers => {
            let mut providers = Vec::new();
            for (name, path) in &ctx.paths.cloud_dirs {
                let exists = path.exists();
                providers.push(json!({
                    "provider": name,
                    "path": path,
                    "exists": exists,
                }));
            }
            Ok(JsonEnvelope::new(
                "cloud providers",
                ctx.mode.clone(),
                json!({
                    "providers": providers,
                    "sync_warning": sync_warning,
                }),
            ))
        }
        CloudCommand::Scan => {
            let (summary, items) = load_cloud_scan(ctx)?;
            let mut findings = Vec::new();
            for item in &items {
                findings.push(json!({
                    "id": format!("cloud_{}_{}", item.provider.to_lowercase().replace(' ', "_"), crate::core::unix_now()),
                    "module": "cloud",
                    "category": "sync",
                    "path": item.path,
                    "size_bytes": item.sampled_size_bytes,
                    "risk": "low",
                    "confidence": 1.0,
                    "action": "report_only",
                    "reason": format!("Cloud sync path detected: {} (contains warning)", item.provider),
                    "requires_sudo": false,
                }));
            }

            Ok(JsonEnvelope::new(
                "cloud scan",
                ctx.mode.clone(),
                json!({
                    "summary": summary,
                    "items": items,
                    "findings": findings,
                }),
            ))
        }
    }
}
