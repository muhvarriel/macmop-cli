use crate::core::{PlannedActionKind, RiskLevel, ScanFinding};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Policy {
    home: PathBuf,
    protected: Vec<PathBuf>,
}

impl Policy {
    pub fn new(home: PathBuf, custom: Vec<PathBuf>) -> Self {
        let mut protected: Vec<PathBuf> = [
            ".ssh",
            "Documents",
            "Desktop",
            "Pictures/Photos Library.photoslibrary",
            "Library/Keychains",
            "Library/Mobile Documents",
            "Library/Application Support/1Password",
            "Library/Application Support/Bitwarden",
            "Library/Application Support/iCloud",
        ]
        .into_iter()
        .map(|p| home.join(p))
        .chain(
            ["/System", "/bin", "/sbin", "/usr/bin", "/usr/sbin"]
                .into_iter()
                .map(PathBuf::from),
        )
        .collect();

        protected.extend(custom);
        Self { home, protected }
    }

    pub fn cleanup_roots(&self, categories: &[String]) -> Vec<(String, PathBuf, RiskLevel)> {
        let requested = if categories.is_empty() {
            vec!["cache".to_string(), "logs".to_string()]
        } else {
            categories.to_vec()
        };
        requested
            .into_iter()
            .filter_map(|category| match category.as_str() {
                "cache" | "user_cache" => Some((
                    "user_cache".to_string(),
                    self.home.join("Library/Caches"),
                    RiskLevel::Low,
                )),
                "logs" => Some((
                    "logs".to_string(),
                    self.home.join("Library/Logs"),
                    RiskLevel::Low,
                )),
                "temp" => Some(("temp".to_string(), std::env::temp_dir(), RiskLevel::Low)),
                "xcode" => Some((
                    "xcode".to_string(),
                    self.home.join("Library/Developer/Xcode/DerivedData"),
                    RiskLevel::Medium,
                )),
                _ => None,
            })
            .collect()
    }

    pub fn allowed_cleanup_path(
        &self,
        path: &Path,
        roots: &[(String, PathBuf, RiskLevel)],
    ) -> bool {
        roots.iter().any(|(_, root, _)| path.starts_with(root)) && !self.is_protected(path)
    }

    pub fn is_protected(&self, path: &Path) -> bool {
        self.protected
            .iter()
            .any(|protected| path.starts_with(protected))
    }

    pub fn enforce_finding(&self, finding: &mut ScanFinding) {
        if self.is_protected(&finding.path) {
            finding.risk = RiskLevel::Critical;
            finding.action = PlannedActionKind::ReportOnly;
            finding.reason = format!("protected path: {}", finding.reason);
        }
        if finding.requires_sudo {
            finding.action = PlannedActionKind::ReportOnly;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_path_wins() {
        let home = PathBuf::from("/Users/alex");
        let policy = Policy::new(home.clone(), vec![]);
        assert!(policy.is_protected(&home.join(".ssh/id_rsa")));
    }

    #[test]
    fn cleanup_roots_are_allowlisted() {
        let home = PathBuf::from("/Users/alex");
        let policy = Policy::new(home.clone(), vec![]);
        let roots = policy.cleanup_roots(&["cache".to_string()]);
        assert!(policy.allowed_cleanup_path(&home.join("Library/Caches/a"), &roots));
        assert!(!policy.allowed_cleanup_path(&home.join("Library/Application Support/a"), &roots));
    }
}
