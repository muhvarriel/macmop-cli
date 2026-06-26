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
