use anyhow::Result;
use macmop::cli::DuplicatesArgs;
use macmop::core::{AppContext, ExecutionMode, OutputFormat};
use std::fs;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

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
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[test]
fn test_duplicate_groups_preserves_original() -> Result<()> {
    let env = TestEnv::new("duplicates");
    let scan_dir = env.home.join("Downloads");
    fs::create_dir_all(&scan_dir)?;

    let file1 = scan_dir.join("a.txt");
    let file2 = scan_dir.join("b.txt");
    let file3 = scan_dir.join("c.txt");

    fs::write(&file1, "binary identical data")?;
    fs::write(&file2, "binary identical data")?;
    fs::write(&file3, "binary identical data")?;

    let ctx = env.context(ExecutionMode::DryRun);
    let args = DuplicatesArgs {
        paths: vec![scan_dir],
        min_size: 0,
    };

    let result = macmop::modules::duplicates::run(&ctx, args)?;
    let payload = &result.payload;

    let groups = payload.get("groups").unwrap().as_array().unwrap();
    assert_eq!(groups.len(), 1, "exactly one duplicate group found");

    let group = &groups[0];
    let count = group.get("count").unwrap().as_u64().unwrap() as usize;
    assert_eq!(count, 3, "group should contain 3 files");

    let delete_candidates = group.get("delete_candidates").unwrap().as_array().unwrap();
    assert_eq!(
        delete_candidates.len(),
        2,
        "max delete candidates should be group_size - 1"
    );

    let keep_path = group.get("keep").unwrap().as_str().unwrap();
    assert!(!keep_path.is_empty(), "original to keep must be defined");
    assert!(
        !delete_candidates
            .iter()
            .any(|c| c.as_str().unwrap() == keep_path),
        "keep file must not be in delete candidates"
    );

    Ok(())
}
