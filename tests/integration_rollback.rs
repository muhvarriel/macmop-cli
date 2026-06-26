use anyhow::Result;
use macmop::cli::{CleanupArgs, RollbackArgs, RollbackCommand};
use macmop::core::{AppContext, ExecutionMode, OutputFormat};
use std::fs;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

#[allow(dead_code)]
struct TestEnv {
    home: PathBuf,
    data_dir: PathBuf,
    trash_dir: PathBuf,
    audit_file: PathBuf,
    rollback_file: PathBuf,
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
        let data_dir = base.join("data");
        let trash_dir = base.join("trash");
        let audit_file = data_dir.join("audit/last.json");
        let rollback_file = data_dir.join("rollback/entries.json");

        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&trash_dir).unwrap();

        Self {
            home,
            data_dir,
            trash_dir,
            audit_file,
            rollback_file,
        }
    }

    fn context(&self, mode: ExecutionMode) -> AppContext {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut ctx = AppContext::load(None, mode, OutputFormat::Json, cancelled).unwrap();
        ctx.paths = macmop::core::AppPaths {
            home: self.home.clone(),
            data_dir: self.data_dir.clone(),
            trash: self.trash_dir.clone(),
            audit_file: self.audit_file.clone(),
            rollback_file: self.rollback_file.clone(),
            apps_dirs: vec![],
            startup_dirs: vec![],
        };
        ctx
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let parent = self.home.parent().unwrap();
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn test_rollback_restores_original_path() -> Result<()> {
    let env = TestEnv::new("rollback");
    let cache_dir = env.home.join("Library/Caches");
    fs::create_dir_all(&cache_dir)?;
    let file1 = cache_dir.join("test_file.txt");
    fs::write(&file1, "rollback test content")?;

    // 1. Run cleanup with --apply
    let ctx = env.context(ExecutionMode::Apply);
    let cleanup_args = CleanupArgs {
        category: vec!["cache".to_string()],
        older_than_days: 0,
    };
    macmop::modules::cleanup::run(&ctx, cleanup_args)?;
    assert!(!file1.exists(), "original file must be moved");

    // Get rollback ID
    let rollbacks = macmop::audit::read_rollbacks(&env.rollback_file)?;
    assert_eq!(rollbacks.len(), 1);
    let rollback_id = rollbacks[0].id.0.clone();

    // 2. Apply rollback
    let rollback_args = RollbackArgs {
        command: RollbackCommand::Apply { id: rollback_id },
    };
    macmop::modules::rollback::run(&ctx, rollback_args)?;

    assert!(file1.exists(), "original file must be restored");
    assert_eq!(
        fs::read_to_string(&file1)?,
        "rollback test content",
        "restored contents must match original"
    );

    // Rollback entries should be empty after application
    let remaining = macmop::audit::read_rollbacks(&env.rollback_file)?;
    assert_eq!(remaining.len(), 0);

    Ok(())
}
