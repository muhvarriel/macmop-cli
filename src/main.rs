use anyhow::Result;
use clap::{CommandFactory, Parser};
use macmop::cli::{Cli, Command};
use macmop::core::{AppContext, JsonEnvelope, OutputFormat};
use std::io::IsTerminal;
use std::path::PathBuf;
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

    // Resolve config path
    let is_test = std::env::var("MACMOP_TEST_MODE")
        .map(|v| v == "1")
        .unwrap_or(false);
    let home = if is_test {
        std::env::var("MACMOP_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                directories::BaseDirs::new()
                    .unwrap()
                    .home_dir()
                    .to_path_buf()
            })
    } else {
        directories::BaseDirs::new()
            .unwrap()
            .home_dir()
            .to_path_buf()
    };
    let config_path = cli
        .config
        .clone()
        .unwrap_or_else(|| home.join(".config/macmop/config.toml"));

    let is_validate_cmd = matches!(
        cli.command,
        Some(Command::Config(ref args)) if matches!(args.command, macmop::cli::ConfigCommand::Validate { .. })
    );

    let config = if is_validate_cmd {
        macmop::core::Config::default()
    } else {
        match macmop::core::Config::load_from_path(&config_path) {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    };

    let output = cli.output_format(&config)?;
    let mode = cli.execution_mode()?;
    let ctx = AppContext::load(cli.config.clone(), mode, output, cancelled)?;

    let run_tui = match &cli.command {
        Some(Command::Tui) => true,
        None => {
            if std::io::stdout().is_terminal() {
                true
            } else {
                let mut cmd = Cli::command();
                cmd.print_help()?;
                println!();
                return Ok(());
            }
        }
        _ => false,
    };

    if run_tui {
        return macmop::modules::tui::run(&ctx);
    }

    let result = match cli.command.unwrap() {
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
        Command::Maintenance(args) => macmop::modules::maintenance::run(&ctx, args),
        Command::Status => macmop::modules::status::run(&ctx),
        Command::Tui => unreachable!(),
        Command::Config(args) => macmop::modules::config::run(&ctx, args),
        Command::Cloud(args) => macmop::modules::cloud::run(&ctx, args),
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
