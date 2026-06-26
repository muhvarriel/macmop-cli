use super::*;

pub fn run(ctx: &crate::core::AppContext, args: CleanupArgs) -> Result<JsonEnvelope<Value>> {
    let policy = Policy::new(ctx.paths.home.clone(), ctx.custom_protected_paths.clone());
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
        let scan = scanner::cleanup_candidates(root, category, *risk, args.older_than_days, || {
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
