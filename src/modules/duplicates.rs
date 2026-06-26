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
