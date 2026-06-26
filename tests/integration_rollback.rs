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
            quicklook_dirs: vec![],
            cloud_dirs: vec![],
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

#[test]
fn test_executor_shared_rollback_id_multiple_files() -> Result<()> {
    let env = TestEnv::new("shared_rollback");
    let cache_dir = env.home.join("Library/Caches");
    fs::create_dir_all(&cache_dir)?;

    let f1 = cache_dir.join("f1.txt");
    let f2 = cache_dir.join("f2.txt");
    fs::write(&f1, "content1")?;
    fs::write(&f2, "content2")?;

    let ctx = env.context(ExecutionMode::Apply);
    let cleanup_args = CleanupArgs {
        category: vec!["cache".to_string()],
        older_than_days: 0,
    };
    macmop::modules::cleanup::run(&ctx, cleanup_args)?;

    assert!(!f1.exists());
    assert!(!f2.exists());

    let rollbacks = macmop::audit::read_rollbacks(&env.rollback_file)?;
    assert_eq!(rollbacks.len(), 2);
    assert_eq!(
        rollbacks[0].id.0, rollbacks[1].id.0,
        "both entries must share the same rollback ID"
    );

    // Rollback both using that ID
    let rollback_id = rollbacks[0].id.0.clone();
    macmop::modules::rollback::run(
        &ctx,
        RollbackArgs {
            command: RollbackCommand::Apply { id: rollback_id },
        },
    )?;

    assert!(f1.exists());
    assert!(f2.exists());
    assert_eq!(fs::read_to_string(&f1)?, "content1");
    assert_eq!(fs::read_to_string(&f2)?, "content2");

    Ok(())
}

#[test]
fn test_executor_cancellation_simulation() -> Result<()> {
    let env = TestEnv::new("cancel_sim");
    let cache_dir = env.home.join("Library/Caches");
    fs::create_dir_all(&cache_dir)?;

    // Create 15 files to ensure the thread can interrupt it mid-run
    let mut files = Vec::new();
    for i in 0..15 {
        let f = cache_dir.join(format!("file_{}.txt", i));
        fs::write(&f, "content")?;
        files.push(f);
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    let mut ctx = AppContext::load(
        None,
        ExecutionMode::Apply,
        OutputFormat::Json,
        Arc::clone(&cancelled),
    )
    .unwrap();
    ctx.paths = macmop::core::AppPaths {
        home: env.home.clone(),
        data_dir: env.data_dir.clone(),
        trash: env.trash_dir.clone(),
        audit_file: env.audit_file.clone(),
        rollback_file: env.rollback_file.clone(),
        apps_dirs: vec![],
        startup_dirs: vec![],
        quicklook_dirs: vec![],
        cloud_dirs: vec![],
    };

    // Trigger cancellation in a separate thread after a small delay
    let cancelled_clone = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1));
        cancelled_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let cleanup_args = CleanupArgs {
        category: vec!["cache".to_string()],
        older_than_days: 0,
    };
    macmop::modules::cleanup::run(&ctx, cleanup_args)?;

    // Read audit log to check statuses
    let audit_entries = macmop::audit::read_last_audit(&env.audit_file)?;
    assert_eq!(
        audit_entries.len(),
        15,
        "all 15 planned actions must be accounted for in audit"
    );

    let mut success_count = 0;
    let mut cancelled_count = 0;
    for entry in &audit_entries {
        match entry.status.as_str() {
            "success" => success_count += 1,
            "cancelled" => cancelled_count += 1,
            other => panic!("unexpected audit status: {}", other),
        }
    }

    assert!(
        success_count > 0,
        "at least some files must succeed before cancel"
    );
    assert!(cancelled_count > 0, "at least some files must be cancelled");

    // Rollback database should only contain entries for successfully moved files
    let rollbacks = macmop::audit::read_rollbacks(&env.rollback_file)?;
    assert_eq!(
        rollbacks.len(),
        success_count,
        "rollback entries count must match success count"
    );

    // All success rollbacks must share the same rollback ID
    if success_count > 0 {
        let r_id = &rollbacks[0].id.0;
        for r in &rollbacks {
            assert_eq!(&r.id.0, r_id);
        }
    }

    Ok(())
}

#[test]
fn test_executor_failed_and_cancelled() -> Result<()> {
    let env = TestEnv::new("fail_cancel");
    let cache_dir = env.home.join("Library/Caches");
    fs::create_dir_all(&cache_dir)?;

    let f_fail = cache_dir.join("fail.txt"); // Will not exist, so move_to_trash fails

    let mut actions = vec![macmop::core::PlannedAction {
        finding_id: macmop::core::FindingId("f_fail".to_string()),
        path: f_fail.clone(),
        action: macmop::core::PlannedActionKind::MoveToTrash,
        rollback_supported: true,
    }];

    for i in 0..50 {
        let f = cache_dir.join(format!("cancel_{}.txt", i));
        fs::write(&f, "content")?;
        actions.push(macmop::core::PlannedAction {
            finding_id: macmop::core::FindingId(format!("f_cancel_{}", i)),
            path: f,
            action: macmop::core::PlannedActionKind::MoveToTrash,
            rollback_supported: true,
        });
    }

    let total_items = actions.len();
    let plan = macmop::core::ActionPlan {
        id: macmop::core::PlanId("test_plan".to_string()),
        created_at: macmop::core::unix_now(),
        total_items,
        total_size_bytes: 0,
        actions,
    };

    let cancelled = Arc::new(AtomicBool::new(false));
    let ctx = env.context(ExecutionMode::Apply);

    let mut ctx_custom = AppContext::load(
        None,
        ExecutionMode::Apply,
        OutputFormat::Json,
        Arc::clone(&cancelled),
    )
    .unwrap();
    ctx_custom.paths = ctx.paths.clone();

    // Trigger cancel in a separate thread after a small delay
    let cancelled_clone = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1));
        cancelled_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    });

    let logs = macmop::executor::execute_plan(&ctx_custom, "cleanup", &plan)?;
    assert_eq!(logs.len(), total_items);
    assert!(
        logs[0].status.contains("failed"),
        "first action must be failed"
    );

    // There must be at least one cancelled action at the end
    let last_status = &logs[total_items - 1].status;
    assert_eq!(last_status, "cancelled", "last actions must be cancelled");

    Ok(())
}
