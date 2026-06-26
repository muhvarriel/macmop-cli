use crate::core::{AuditLog, RollbackEntry};
use anyhow::{Context, Result};
use std::{fs, path::Path};

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    Ok(())
}

pub fn write_last_audit(path: &Path, entries: &[AuditLog]) -> Result<()> {
    ensure_parent(path)?;
    fs::write(path, serde_json::to_vec_pretty(entries)?)
        .with_context(|| format!("cannot write {}", path.display()))
}

pub fn read_last_audit(path: &Path) -> Result<Vec<AuditLog>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn append_rollback(path: &Path, entry: RollbackEntry) -> Result<()> {
    let mut entries = read_rollbacks(path)?;
    entries.push(entry);
    ensure_parent(path)?;
    fs::write(path, serde_json::to_vec_pretty(&entries)?)
        .with_context(|| format!("cannot write {}", path.display()))
}

pub fn read_rollbacks(path: &Path) -> Result<Vec<RollbackEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn write_rollbacks(path: &Path, entries: &[RollbackEntry]) -> Result<()> {
    ensure_parent(path)?;
    fs::write(path, serde_json::to_vec_pretty(entries)?)
        .with_context(|| format!("cannot write {}", path.display()))
}
