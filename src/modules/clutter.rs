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
