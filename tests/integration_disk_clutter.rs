use anyhow::Result;
use macmop::cli::DiskArgs;
use macmop::core::{AppContext, ExecutionMode, OutputFormat};
use std::fs;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

struct TestEnv {
    home: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let unique_id = macmop::core::unix_now();
        let base = std::env::temp_dir().join(format!(
            "macmop-test-{}-{}-{}",
            test_name,
            unique_id,
            std::process::id()
        ));
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();

        Self { home }
    }

    fn context(&self, mode: ExecutionMode) -> AppContext {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut ctx = AppContext::load(None, mode, OutputFormat::Json, cancelled).unwrap();
        ctx.paths.home = self.home.clone();
        ctx
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // Fix permissions just in case there was a perm-denied test dir
        let perm_denied = self.home.join("Downloads/unreadable");
        if perm_denied.exists() {
            #[cfg(unix)]
            let _ = fs::set_permissions(&perm_denied, std::fs::Permissions::from_mode(0o755));
        }
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[test]
fn test_disk_top_k_is_bounded_and_sorted() -> Result<()> {
    let env = TestEnv::new("disk_top_k");
    let scan_dir = env.home.join("Downloads");
    fs::create_dir_all(&scan_dir)?;

    fs::write(scan_dir.join("file_small.txt"), "a")?; // 1 byte
    fs::write(scan_dir.join("file_large.txt"), "bbbbbb")?; // 6 bytes
    fs::write(scan_dir.join("file_medium.txt"), "ccc")?; // 3 bytes
    fs::write(scan_dir.join("file_extra.txt"), "dddd")?; // 4 bytes

    let ctx = env.context(ExecutionMode::DryRun);
    let args = DiskArgs {
        path: Some(scan_dir),
        depth: 2,
        top: 3,
    };

    let result = macmop::modules::disk::run(&ctx, args)?;
    let payload = &result.payload;

    let items = payload.get("items").unwrap().as_array().unwrap();
    assert_eq!(items.len(), 3, "top k must restrict to 3 items");

    // Must be sorted by size descending (large: 6, extra: 4, medium: 3)
    assert_eq!(items[0]["size_bytes"].as_u64().unwrap(), 6);
    assert_eq!(items[1]["size_bytes"].as_u64().unwrap(), 4);
    assert_eq!(items[2]["size_bytes"].as_u64().unwrap(), 3);

    Ok(())
}

#[test]
fn test_symlinks_are_skipped() -> Result<()> {
    let env = TestEnv::new("symlinks");
    let scan_dir = env.home.join("Downloads");
    fs::create_dir_all(&scan_dir)?;

    let target_file = env.home.join("actual_file.txt");
    fs::write(&target_file, "actual contents")?;

    // Create a symlink pointing to target_file
    let link_file = scan_dir.join("link_file.txt");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_file, &link_file)?;
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(&target_file, &link_file)?;

    let ctx = env.context(ExecutionMode::DryRun);
    let args = DiskArgs {
        path: Some(scan_dir),
        depth: 2,
        top: 10,
    };

    let result = macmop::modules::disk::run(&ctx, args)?;
    let payload = &result.payload;
    let items = payload.get("items").unwrap().as_array().unwrap();

    let contains_link = items
        .iter()
        .any(|item| item["path"].as_str().unwrap().contains("link_file.txt"));
    assert!(!contains_link, "symlinks must be skipped in disk scans");

    Ok(())
}

#[test]
#[cfg(unix)]
fn test_unreadable_directory_becomes_warning_and_does_not_crash() -> Result<()> {
    let env = TestEnv::new("unreadable_dir");
    let scan_dir = env.home.join("Downloads");
    let unreadable = scan_dir.join("unreadable");
    fs::create_dir_all(&unreadable)?;

    // Create a file inside unreadable
    fs::write(unreadable.join("hidden.txt"), "secret data")?;

    // Set unreadable directory permissions to 000
    fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))?;

    // Run cleanup candidates (this usesWalkDir and should encounter permission error)
    // Note: We use env.home.join("Library/Caches") as simulated caches root but we override cleanup_roots to scan unreadable!
    // Since policy cleanup_roots is configured, let's just test that the scanner::cleanup_candidates handles this cleanly.
    let scan = macmop::scanner::cleanup_candidates(
        &unreadable,
        "cache",
        macmop::core::RiskLevel::Low,
        0,
        || false,
    );

    // Set permissions back so drop can cleanup
    let _ = fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o755));

    assert!(
        !scan.warnings.is_empty(),
        "must record warning for unreadable dir"
    );
    // Should not crash and should succeed with empty/partial findings
    assert!(
        scan.findings.is_empty(),
        "findings should be empty for unreadable path contents"
    );

    Ok(())
}
