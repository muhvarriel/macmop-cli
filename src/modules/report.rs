use super::*;

pub fn run(ctx: &crate::core::AppContext, args: ReportArgs) -> Result<JsonEnvelope<Value>> {
    match args.command {
        ReportCommand::Last => {
            let entries = audit::read_last_audit(&ctx.paths.audit_file)?;
            Ok(JsonEnvelope::new(
                "report",
                ctx.mode.clone(),
                json!({
                    "summary": format!("report last: {} audit entries", entries.len()),
                    "items": entries
                }),
            ))
        }
    }
}
