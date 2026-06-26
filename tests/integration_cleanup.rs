use anyhow::Result;
use clap::Parser;
use macmop::cli::CleanupArgs;
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
            quicklook_dirs: vec![],
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
fn test_cleanup_dry_run_does_not_mutate() -> Result<()> {
    let env = TestEnv::new("cleanup_dry_run");
    let cache_dir = env.home.join("Library/Caches");
    fs::create_dir_all(&cache_dir)?;
    let file1 = cache_dir.join("test_file.txt");
    fs::write(&file1, "cache contents")?;

    let ctx = env.context(ExecutionMode::DryRun);
    let args = CleanupArgs {
        category: vec!["cache".to_string()],
        older_than_days: 0,
    };

    let result = macmop::modules::cleanup::run(&ctx, args)?;

    assert!(file1.exists(), "file must not be deleted on dry-run");
    assert!(
        !env.audit_file.exists(),
        "audit file must not be created on dry-run"
    );
    assert!(
        !env.rollback_file.exists(),
        "rollback file must not be created on dry-run"
    );

    let payload = &result.payload;
    assert!(payload.get("findings").is_some());
    assert!(payload.get("action_plan").is_some());

    Ok(())
}

#[test]
fn test_cleanup_apply_moves_to_trash_and_audits() -> Result<()> {
    let env = TestEnv::new("cleanup_apply");
    let cache_dir = env.home.join("Library/Caches");
    fs::create_dir_all(&cache_dir)?;
    let file1 = cache_dir.join("test_file.txt");
    fs::write(&file1, "cache contents")?;

    let ctx = env.context(ExecutionMode::Apply);
    let args = CleanupArgs {
        category: vec!["cache".to_string()],
        older_than_days: 0,
    };

    let _result = macmop::modules::cleanup::run(&ctx, args)?;

    assert!(!file1.exists(), "original file must be moved");
    assert!(env.audit_file.exists(), "audit file must be created");
    assert!(env.rollback_file.exists(), "rollback file must be created");

    let trash_contents: Vec<_> = fs::read_dir(&env.trash_dir)?
        .map(|r| r.unwrap().path())
        .collect();
    assert_eq!(
        trash_contents.len(),
        1,
        "exactly one file should be in trash"
    );
    assert_eq!(
        fs::read_to_string(&trash_contents[0])?,
        "cache contents",
        "trash file contents must match original"
    );

    Ok(())
}

#[test]
fn test_cleanup_permanent_delete_without_force_fails() {
    let args = macmop::cli::Cli::try_parse_from(["macmop", "--permanent", "cleanup"]);
    assert!(args.is_ok());
    let cli = args.unwrap();
    let mode = cli.execution_mode();
    assert!(
        mode.is_err(),
        "permanent delete without force must fail validation"
    );
}
