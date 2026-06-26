use super::*;
use crate::core::{ProtectFinding, RiskLevel, StartupItem};

pub fn run(
    ctx: &crate::core::AppContext,
    args: crate::cli::ProtectArgs,
) -> Result<JsonEnvelope<Value>> {
    match args.command {
        crate::cli::ProtectCommand::Scan => scan(ctx, "protect scan"),
        crate::cli::ProtectCommand::Startup => scan(ctx, "protect startup"),
        crate::cli::ProtectCommand::Inspect { id } => inspect(ctx, &id),
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
