use super::*;
use crate::cli::{ConfigArgs, ConfigCommand};
use crate::core::Config;

pub fn run(ctx: &crate::core::AppContext, args: ConfigArgs) -> Result<JsonEnvelope<Value>> {
    match args.command {
        ConfigCommand::Show => {
            let path = ctx.config_path.as_ref().cloned().unwrap();
            let config = if ctx.config_loaded {
                Config::load_from_path(&path)?
            } else {
                Config::default()
            };
            Ok(JsonEnvelope::new(
                "config show",
                ctx.mode.clone(),
                json!({
                    "config_path": path,
                    "loaded": ctx.config_loaded,
                    "config": config,
                }),
            ))
        }
        ConfigCommand::Validate { path } => {
            let target_path = path.unwrap_or_else(|| ctx.config_path.as_ref().cloned().unwrap());
            let mut valid = true;
            let mut error = None;
            if target_path.exists() {
                match Config::load_from_path(&target_path) {
                    Ok(_) => {}
                    Err(e) => {
                        valid = false;
                        error = Some(e.to_string());
                    }
                }
            } else {
                valid = false;
                error = Some(format!(
                    "Configuration file does not exist at {}",
                    target_path.display()
                ));
            }

            Ok(JsonEnvelope::new(
                "config validate",
                ctx.mode.clone(),
                json!({
                    "config_path": target_path,
                    "valid": valid,
                    "error": error,
                }),
            ))
        }
    }
}
