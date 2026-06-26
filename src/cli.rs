use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::core::{ExecutionMode, OutputFormat};

#[derive(Debug, Parser, Clone)]
#[command(name = "macmop", version, about = "Safety-first macOS cleanup CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[arg(long, global = true)]
    pub dry_run: bool,
    #[arg(long, global = true)]
    pub apply: bool,
    #[arg(long, global = true)]
    pub permanent: bool,
    #[arg(long, global = true)]
    pub force: bool,
    #[arg(long, global = true)]
    pub yes: bool,

    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub ndjson: bool,
    #[arg(long, global = true)]
    pub table: bool,
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
}

impl Cli {
    pub fn execution_mode(&self) -> Result<ExecutionMode> {
        if self.permanent && !self.force {
            bail!("--permanent requires --force");
        }
        if self.permanent && self.dry_run {
            bail!("--permanent conflicts with --dry-run");
        }
        if self.apply && self.dry_run {
            bail!("--apply conflicts with --dry-run");
        }
        if self.permanent {
            Ok(ExecutionMode::Permanent { force: true })
        } else if self.apply {
            Ok(ExecutionMode::Apply)
        } else {
            Ok(ExecutionMode::DryRun)
        }
    }

    pub fn output_format(&self) -> Result<OutputFormat> {
        let count = [self.json, self.ndjson, self.table]
            .into_iter()
            .filter(|v| *v)
            .count();
        if count > 1 {
            bail!("choose only one output format: --json, --ndjson, or --table");
        }
        if self.json {
            Ok(OutputFormat::Json)
        } else if self.ndjson {
            Ok(OutputFormat::Ndjson)
        } else {
            Ok(OutputFormat::Table)
        }
    }
}

#[derive(Debug, Subcommand, Clone)]
pub enum Command {
    /// Safe junk cleanup for caches, logs, and derived data
    Cleanup(CleanupArgs),
    /// Terminal-based storage map and tree visualization
    Disk(DiskArgs),
    /// Scan for large files, installers, and download clutter
    Clutter(ClutterArgs),
    /// Find binary-identical duplicate files using staged hashing
    Duplicates(DuplicatesArgs),
    /// View audit logs and reports from previous runs
    Report(ReportArgs),
    /// List or apply rollback entries to restore trashed files
    Rollback(RollbackArgs),
    /// Run dry-run scan across multiple modules
    Scan(ScanArgs),
    /// App inventory, inspection, and leftover detection (report-only)
    Apps(AppsArgs),
    /// Startup item report: LaunchAgents and LaunchDaemons (report-only)
    Startup(StartupArgs),
    /// Scan for protection/persistence risks and suspicious items (report-only)
    Protect(ProtectArgs),
    /// Scan for privacy-related artifacts and metadata (report-only)
    Privacy(PrivacyArgs),
    /// Maintenance task catalog and preflight checks (report-only)
    Maintenance(MaintenanceArgs),
    /// Read-only support/debug status summary
    Status,
    /// Interactive terminal dashboard (TUI)
    Tui,
}

#[derive(Debug, Args, Clone)]
pub struct CleanupArgs {
    /// Categories to clean (comma-separated): cache, user_cache, logs, temp, xcode
    #[arg(long, value_delimiter = ',')]
    pub category: Vec<String>,
    /// Only clean files older than this threshold in days
    #[arg(long, default_value = "30")]
    pub older_than_days: u64,
}

#[derive(Debug, Args, Clone)]
pub struct DiskArgs {
    /// Target path to map (defaults to home directory)
    pub path: Option<PathBuf>,
    /// Max directory traversal depth
    #[arg(long, default_value = "3")]
    pub depth: usize,
    /// Number of top large folders/files to display
    #[arg(long, default_value = "50")]
    pub top: usize,
}

#[derive(Debug, Args, Clone)]
pub struct ClutterArgs {
    /// Target path to scan (defaults to Downloads directory)
    pub path: Option<PathBuf>,
    /// Minimum size threshold in bytes for large files
    #[arg(long, default_value = "104857600")]
    pub min_size: u64,
    /// Number of top clutter files to display
    #[arg(long, default_value = "50")]
    pub top: usize,
}

#[derive(Debug, Args, Clone)]
pub struct DuplicatesArgs {
    /// Target paths to scan for duplicates (defaults to Downloads directory)
    pub paths: Vec<PathBuf>,
    /// Minimum size in bytes to include files in duplicate scan
    #[arg(long, default_value = "10485760")]
    pub min_size: u64,
}

#[derive(Debug, Args, Clone)]
pub struct ReportArgs {
    #[command(subcommand)]
    pub command: ReportCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum ReportCommand {
    /// Display details of the last executed cleanup action plan
    Last,
}

#[derive(Debug, Args, Clone)]
pub struct RollbackArgs {
    #[command(subcommand)]
    pub command: RollbackCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum RollbackCommand {
    /// List all available rollback entries
    List,
    /// Apply rollback by ID to restore original files
    Apply {
        /// The rollback entry ID to restore
        id: String,
    },
}

#[derive(Debug, Args, Clone)]
pub struct ScanArgs {
    /// Scan profile to run: safe, developer, creator, privacy, deep
    #[arg(long, default_value = "safe")]
    pub profile: String,
}

#[derive(Debug, Args, Clone)]
pub struct AppsArgs {
    #[command(subcommand)]
    pub command: AppsCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum AppsCommand {
    /// List all installed apps with metadata
    List,
    /// Inspect a single app bundle and its associated files
    Inspect {
        /// App name or path (e.g. "Safari.app" or "/Applications/Safari.app")
        app: String,
    },
    /// Report likely orphaned files from uninstalled apps
    Leftovers,
}

#[derive(Debug, Args, Clone)]
pub struct StartupArgs {
    #[command(subcommand)]
    pub command: StartupCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum StartupCommand {
    /// List all LaunchAgents and LaunchDaemons with parsed metadata
    List,
    /// Inspect a single startup item by its Label
    Inspect {
        /// The Label of the startup item (e.g. com.example.helper)
        id: String,
    },
}

#[derive(Debug, Args, Clone)]
pub struct ProtectArgs {
    #[command(subcommand)]
    pub command: ProtectCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum ProtectCommand {
    /// Scan for protection/persistence risks and suspicious items
    Scan,
    /// Scan and display startup persistence findings specifically
    Startup,
    /// Inspect details of a specific protect finding
    Inspect {
        /// The finding ID (e.g. protect_startup_...)
        id: String,
    },
}

#[derive(Debug, Args, Clone)]
pub struct PrivacyArgs {
    #[command(subcommand)]
    pub command: PrivacyCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum PrivacyCommand {
    /// Scan all privacy artifacts (browsers, recent, quicklook, shell history)
    Scan,
    /// Scan and display browser cache/history privacy artifacts specifically
    Browsers,
    /// Scan and display recent items and shell history specifically
    Recent,
}

#[derive(Debug, Args, Clone)]
pub struct MaintenanceArgs {
    #[command(subcommand)]
    pub command: MaintenanceCommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum MaintenanceCommand {
    /// List supported maintenance tasks without checking local availability
    List,
    /// Run read-only preflight checks for supported maintenance tasks
    Check,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn permanent_requires_force() {
        let cli = Cli::try_parse_from(["macmop", "--permanent", "cleanup"]).unwrap();
        assert!(cli.execution_mode().is_err());
    }

    #[test]
    fn output_flags_conflict() {
        let cli = Cli::try_parse_from(["macmop", "--json", "--ndjson", "cleanup"]).unwrap();
        assert!(cli.output_format().is_err());
    }
}
