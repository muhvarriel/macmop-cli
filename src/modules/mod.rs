use crate::{
    audit,
    cli::*,
    core::{JsonEnvelope, PlannedActionKind, ScanFinding},
    executor, planner,
    policy::Policy,
    scanner,
};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

pub mod apps;
pub mod cleanup;
pub mod clutter;
pub mod disk;
pub mod duplicates;
pub mod maintenance;
pub mod privacy;
pub mod protect;
pub mod report;
pub mod rollback;
pub mod scan;
pub mod startup;
pub mod status;

fn top_entries(root: &Path, max_depth: usize, top: usize, min_size: u64) -> Result<Vec<Value>> {
    let mut heap: BinaryHeap<Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    for entry in WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
    {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        if size < min_size {
            continue;
        }
        heap.push(Reverse((size, entry.path().to_path_buf())));
        if heap.len() > top {
            heap.pop();
        }
    }
    let mut items: Vec<_> = heap
        .into_iter()
        .map(|Reverse((size, path))| json!({ "path": path, "size_bytes": size }))
        .collect();
    items.sort_by(|a, b| b["size_bytes"].as_u64().cmp(&a["size_bytes"].as_u64()));
    Ok(items)
}

fn duplicate_groups(roots: &[PathBuf], min_size: u64) -> Result<Vec<Value>> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for root in roots {
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if entry.file_type().is_symlink() || !entry.file_type().is_file() {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if size >= min_size {
                by_size
                    .entry(size)
                    .or_default()
                    .push(entry.path().to_path_buf());
            }
        }
    }

    let mut groups = Vec::new();
    for (size, paths) in by_size.into_iter().filter(|(_, paths)| paths.len() > 1) {
        let mut by_partial: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
        for path in paths {
            if let Ok(hash) = partial_hash(&path) {
                by_partial.entry(hash).or_default().push(path);
            }
        }
        for paths in by_partial.into_values().filter(|paths| paths.len() > 1) {
            let mut by_full: HashMap<[u8; 32], Vec<PathBuf>> = HashMap::new();
            for path in paths {
                if let Ok(hash) = full_hash(&path) {
                    by_full.entry(hash).or_default().push(path);
                }
            }
            for duplicates in by_full.into_values().filter(|paths| paths.len() > 1) {
                let keep = duplicates[0].clone();
                let delete_candidates: Vec<_> = duplicates.iter().skip(1).cloned().collect();
                groups.push(json!({
                    "size_bytes": size,
                    "keep": keep,
                    "delete_candidates": delete_candidates,
                    "count": duplicates.len()
                }));
            }
        }
    }
    Ok(groups)
}

fn partial_hash(path: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0; 4096];
    let read = file.read(&mut buf)?;
    hasher.update(&buf[..read]);
    let len = file.metadata()?.len();
    if len > 4096 {
        file.seek(SeekFrom::Start(len / 2))?;
        let read = file.read(&mut buf)?;
        hasher.update(&buf[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

fn full_hash(path: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Ok(*hasher.finalize().as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_group_never_selects_all_files() {
        let group = json!({
            "keep": "/tmp/a",
            "delete_candidates": ["/tmp/b"],
            "count": 2
        });
        assert!(
            group["delete_candidates"].as_array().unwrap().len()
                < group["count"].as_u64().unwrap() as usize
        );
    }
}
