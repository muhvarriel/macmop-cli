use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub const SCHEMA_VERSION: &str = "1.0";

#[derive(Clone)]
pub struct AppContext {
    pub paths: AppPaths,
    pub mode: ExecutionMode,
    pub output: OutputFormat,
    cancelled: Arc<AtomicBool>,
}

impl AppContext {
    pub fn load(
        config_path: Option<PathBuf>,
        mode: ExecutionMode,
        output: OutputFormat,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Self> {
        let is_test = std::env::var("MACMOP_TEST_MODE")
            .map(|v| v == "1")
            .unwrap_or(false);

        let home = if is_test {
            if let Ok(val) = std::env::var("MACMOP_HOME") {
                PathBuf::from(val)
            } else {
                BaseDirs::new()
                    .context("cannot locate home directory")?
                    .home_dir()
                    .to_path_buf()
            }
        } else {
            BaseDirs::new()
                .context("cannot locate home directory")?
                .home_dir()
                .to_path_buf()
        };

        let data_dir = if is_test {
            if let Ok(val) = std::env::var("MACMOP_DATA_DIR") {
                PathBuf::from(val)
            } else {
                config_path
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_else(|| home.join(".local/share/macmop"))
            }
        } else {
            config_path
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_else(|| home.join(".local/share/macmop"))
        };

        let trash = if is_test {
            if let Ok(val) = std::env::var("MACMOP_TRASH_DIR") {
                PathBuf::from(val)
            } else {
                home.join(".Trash")
            }
        } else {
            home.join(".Trash")
        };

        let audit_file = if is_test {
            if let Ok(val) = std::env::var("MACMOP_AUDIT_FILE") {
                PathBuf::from(val)
            } else {
                data_dir.join("audit/last.json")
            }
        } else {
            data_dir.join("audit/last.json")
        };

        let rollback_file = if is_test {
            if let Ok(val) = std::env::var("MACMOP_ROLLBACK_FILE") {
                PathBuf::from(val)
            } else {
                data_dir.join("rollback/entries.json")
            }
        } else {
            data_dir.join("rollback/entries.json")
        };

        let apps_dirs = if is_test {
            if let Ok(val) = std::env::var("MACMOP_APPS_DIRS") {
                val.split(':').map(PathBuf::from).collect()
            } else {
                default_apps_dirs(&home)
            }
        } else {
            default_apps_dirs(&home)
        };

        let startup_dirs = default_startup_dirs(&home);

        Ok(Self {
            paths: AppPaths {
                home,
                data_dir,
                trash,
                audit_file,
                rollback_file,
                apps_dirs,
                startup_dirs,
            },
            mode,
            output,
            cancelled,
        })
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn with_mode(&self, mode: ExecutionMode) -> Self {
        Self {
            paths: self.paths.clone(),
            mode,
            output: self.output,
            cancelled: Arc::clone(&self.cancelled),
        }
    }
}

#[derive(Clone)]
pub struct AppPaths {
    pub home: PathBuf,
    pub data_dir: PathBuf,
    pub trash: PathBuf,
    pub audit_file: PathBuf,
    pub rollback_file: PathBuf,
    pub apps_dirs: Vec<PathBuf>,
    /// (directory, source_label) — e.g. ("~/Library/LaunchAgents", "user_launch_agents")
    pub startup_dirs: Vec<(PathBuf, String)>,
}

fn default_apps_dirs(home: &std::path::Path) -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications"), home.join("Applications")]
}

fn default_startup_dirs(home: &std::path::Path) -> Vec<(PathBuf, String)> {
    vec![
        (
            home.join("Library/LaunchAgents"),
            "user_launch_agents".to_string(),
        ),
        (
            PathBuf::from("/Library/LaunchAgents"),
            "system_launch_agents".to_string(),
        ),
        (
            PathBuf::from("/Library/LaunchDaemons"),
            "system_launch_daemons".to_string(),
        ),
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    Table,
    Json,
    Ndjson,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    DryRun,
    Apply,
    Permanent { force: bool },
}

impl ExecutionMode {
    pub fn is_destructive(&self) -> bool {
        !matches!(self, Self::DryRun)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DryRun => "dry_run",
            Self::Apply => "apply",
            Self::Permanent { .. } => "permanent",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlannedActionKind {
    ReportOnly,
    MoveToTrash,
    PermanentDelete,
    Quarantine,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct FindingId(pub String);
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct PlanId(pub String);
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct AuditId(pub String);
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct RollbackId(pub String);

impl fmt::Display for FindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl fmt::Display for PlanId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl fmt::Display for RollbackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn new_id(prefix: &str) -> String {
    let seq = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{}_{}", unix_now(), seq)
}

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScanFinding {
    pub id: FindingId,
    pub module: String,
    pub category: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub risk: RiskLevel,
    pub confidence: f32,
    pub action: PlannedActionKind,
    pub reason: String,
    pub requires_sudo: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedAction {
    pub finding_id: FindingId,
    pub action: PlannedActionKind,
    pub path: PathBuf,
    pub rollback_supported: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActionPlan {
    pub id: PlanId,
    pub created_at: u64,
    pub total_items: usize,
    pub total_size_bytes: u64,
    pub actions: Vec<PlannedAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: AuditId,
    pub timestamp: u64,
    pub command: String,
    pub action: PlannedActionKind,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub status: String,
    pub rollback_id: Option<RollbackId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RollbackEntry {
    pub id: RollbackId,
    pub original_path: PathBuf,
    pub current_path: PathBuf,
    pub created_at: u64,
    pub action: PlannedActionKind,
}

#[derive(Clone, Debug, Serialize)]
pub struct JsonEnvelope<T: Serialize> {
    pub schema_version: &'static str,
    pub command: String,
    pub mode: String,
    pub payload: T,
}

impl<T: Serialize> JsonEnvelope<T> {
    pub fn new(command: impl Into<String>, mode: ExecutionMode, payload: T) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            command: command.into(),
            mode: mode.as_str().to_string(),
            payload,
        }
    }
}

/// Confidence level for leftover association inference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeftoverConfidence {
    Low,
    Medium,
    High,
}

impl LeftoverConfidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

/// Metadata for a discovered .app bundle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppBundle {
    pub name: String,
    pub path: PathBuf,
    pub bundle_id: String,
    pub version: String,
    pub size_bytes: u64,
    pub is_system_app: bool,
    pub risk: RiskLevel,
}

/// A file/directory associated with a known app bundle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppAssociation {
    pub path: PathBuf,
    pub kind: String,
    pub size_bytes: u64,
    pub exists: bool,
}

/// An orphaned file likely belonging to an uninstalled app.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppLeftover {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub confidence: LeftoverConfidence,
    pub associated_bundle_id: String,
    pub action: PlannedActionKind,
}

/// A LaunchAgent or LaunchDaemon startup item.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartupItem {
    /// Unique id used for `startup inspect` (same as label).
    pub id: String,
    pub label: String,
    pub path: PathBuf,
    /// Resolved executable (Program or first ProgramArguments entry).
    pub program: Option<String>,
    pub program_arguments: Vec<String>,
    pub run_at_load: bool,
    pub keep_alive: bool,
    /// Source directory category: user_launch_agents | system_launch_agents | system_launch_daemons
    pub source: String,
    pub is_system_item: bool,
    pub risk: RiskLevel,
    /// Non-fatal parse issues (e.g. missing Label, unrecognised KeepAlive shape).
    pub warnings: Vec<String>,
    pub action: PlannedActionKind,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProtectFinding {
    pub id: String,
    pub source: String,
    pub label: String,
    pub path: PathBuf,
    pub severity: RiskLevel,
    pub is_system_item: bool,
    pub is_protected: bool,
    pub evidence: Vec<String>,
    pub recommendation: String,
    pub action: PlannedActionKind,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_schema_version_is_stable() {
        let env = JsonEnvelope::new("cleanup", ExecutionMode::DryRun, serde_json::json!({}));
        assert_eq!(env.schema_version, "1.0");
        assert_eq!(env.mode, "dry_run");
    }
}
