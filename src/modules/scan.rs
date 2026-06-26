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
