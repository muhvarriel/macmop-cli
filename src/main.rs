use anyhow::Result;
use clap::Parser;
use macmop::cli::{Cli, Command};
use macmop::core::{AppContext, JsonEnvelope, OutputFormat};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .without_time()
        .init();

    let cancelled = Arc::new(AtomicBool::new(false));
    let signal_flag = Arc::clone(&cancelled);
    ctrlc::set_handler(move || {
        signal_flag.store(true, Ordering::SeqCst);
    })?;

    let cli = Cli::parse();
    let output = cli.output_format()?;
    let mode = cli.execution_mode()?;
    let ctx = AppContext::load(cli.config.clone(), mode, output, cancelled)?;

    let result = match cli.command {
        Command::Cleanup(args) => macmop::modules::cleanup::run(&ctx, args),
        Command::Disk(args) => macmop::modules::disk::run(&ctx, args),
        Command::Clutter(args) => macmop::modules::clutter::run(&ctx, args),
        Command::Duplicates(args) => macmop::modules::duplicates::run(&ctx, args),
        Command::Report(args) => macmop::modules::report::run(&ctx, args),
        Command::Rollback(args) => macmop::modules::rollback::run(&ctx, args),
        Command::Scan(args) => macmop::modules::scan::run(&ctx, args),
        Command::Apps(args) => macmop::modules::apps::run(&ctx, args),
        Command::Startup(args) => macmop::modules::startup::run(&ctx, args),
        Command::Protect(args) => macmop::modules::protect::run(&ctx, args),
        Command::Privacy(args) => macmop::modules::privacy::run(&ctx, args),
    }?;

    match ctx.output {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&result)?),
        OutputFormat::Ndjson => println!("{}", serde_json::to_string(&result)?),
        OutputFormat::Table => print_table(&result)?,
    }

    Ok(())
}

fn print_table(value: &JsonEnvelope<serde_json::Value>) -> Result<()> {
    if let Some(items) = value.payload.get("items").and_then(|v| v.as_array()) {
        if let Some(summary) = value.payload.get("summary") {
            println!("{summary}");
        }
        for item in items {
            println!("{item}");
        }
    } else {
        println!("{}", serde_json::to_string_pretty(value)?);
    }
    Ok(())
}
