use crate::core::{new_id, FindingId, PlannedActionKind, RiskLevel, ScanFinding};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use walkdir::WalkDir;

#[derive(Clone, Debug, serde::Serialize)]
pub struct ScanWarning {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct ScanResult {
    pub findings: Vec<ScanFinding>,
    pub warnings: Vec<ScanWarning>,
}

pub fn cleanup_candidates(
    root: &Path,
    category: &str,
    risk: RiskLevel,
    older_than_days: u64,
    is_cancelled: impl Fn() -> bool,
) -> ScanResult {
    let mut result = ScanResult::default();
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(older_than_days.saturating_mul(86_400)))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    for entry in WalkDir::new(root).follow_links(false).into_iter() {
        if is_cancelled() {
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                result.warnings.push(ScanWarning {
                    path: error
                        .path()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| root.to_path_buf()),
                    message: error.to_string(),
                });
                continue;
            }
        };
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                result.warnings.push(ScanWarning {
                    path: entry.path().to_path_buf(),
                    message: error.to_string(),
                });
                continue;
            }
        };
        if metadata.modified().map(|m| m > cutoff).unwrap_or(true) {
            continue;
        }
        result.findings.push(ScanFinding {
            id: FindingId(new_id("finding")),
            module: "cleanup".to_string(),
            category: category.to_string(),
            path: entry.path().to_path_buf(),
            size_bytes: metadata.len(),
            risk,
            confidence: 0.95,
            action: PlannedActionKind::MoveToTrash,
            reason: format!("{category} file older than {older_than_days} days"),
            requires_sudo: false,
        });
    }
    result
}

pub fn file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
