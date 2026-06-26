use super::*;

pub fn run(ctx: &crate::core::AppContext, args: RollbackArgs) -> Result<JsonEnvelope<Value>> {
    match args.command {
        RollbackCommand::List => {
            let entries = audit::read_rollbacks(&ctx.paths.rollback_file)?;
            Ok(JsonEnvelope::new(
                "rollback",
                ctx.mode.clone(),
                json!({
                    "summary": format!("rollback: {} entries", entries.len()),
                    "items": entries
                }),
            ))
        }
        RollbackCommand::Apply { id } => {
            let entries = audit::read_rollbacks(&ctx.paths.rollback_file)?;
            let mut to_restore = Vec::new();
            let mut remaining = Vec::new();

            for entry in entries {
                if entry.id.0 == id {
                    to_restore.push(entry);
                } else {
                    remaining.push(entry);
                }
            }

            if to_restore.is_empty() {
                anyhow::bail!("rollback id not found");
            }

            if ctx.mode.is_destructive() {
                for entry in &to_restore {
                    if let Some(parent) = entry.original_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(&entry.current_path, &entry.original_path).with_context(|| {
                        format!(
                            "cannot restore {} to {}",
                            entry.current_path.display(),
                            entry.original_path.display()
                        )
                    })?;

                    // Clean up sidecar metadata JSON if this is a quarantined file
                    let meta_path = entry.current_path.with_extension("json");
                    if meta_path.exists() {
                        let _ = fs::remove_file(meta_path);
                    }
                }
                audit::write_rollbacks(&ctx.paths.rollback_file, &remaining)?;
            }
            Ok(JsonEnvelope::new(
                "rollback",
                ctx.mode.clone(),
                json!({
                    "summary": format!("rollback apply {}", id),
                    "restored": to_restore,
                    "applied": ctx.mode.is_destructive()
                }),
            ))
        }
    }
}
