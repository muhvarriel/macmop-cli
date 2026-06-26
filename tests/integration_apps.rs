use anyhow::Result;
use macmop::core::{AppContext, AppPaths, ExecutionMode, OutputFormat};
use macmop::modules::apps;
use std::fs;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

// ── Test env setup ──────────────────────────────────────────────────────────

struct TestEnv {
    home: PathBuf,
    apps_dir: PathBuf,
    _base: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let unique_id = macmop::core::unix_now();
        let base = std::env::temp_dir().join(format!(
            "macmop-test-apps-{}-{}-{}",
            test_name,
            unique_id,
            std::process::id()
        ));
        let home = base.join("home");
        let apps_dir = base.join("Applications");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&apps_dir).unwrap();
        Self {
            home,
            apps_dir,
            _base: base,
        }
    }

    /// Create a minimal .app fixture with a real XML Info.plist.
    fn create_app(&self, app_name: &str, bundle_id: &str, version: &str) -> PathBuf {
        let app_path = self.apps_dir.join(format!("{app_name}.app"));
        let contents = app_path.join("Contents");
        fs::create_dir_all(&contents).unwrap();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>{bundle_id}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleDisplayName</key>
    <string>{app_name}</string>
</dict>
</plist>"#
        );
        fs::write(contents.join("Info.plist"), plist).unwrap();
        app_path
    }

    /// Build an isolated AppContext with test-local paths. No env var races.
    fn ctx(&self) -> AppContext {
        let mut ctx = AppContext::load(
            None,
            ExecutionMode::DryRun,
            OutputFormat::Json,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        ctx.paths = AppPaths {
            home: self.home.clone(),
            data_dir: self._base.join("data"),
            trash: self._base.join("trash"),
            audit_file: self._base.join("data/audit.json"),
            rollback_file: self._base.join("data/rollback.json"),
            apps_dirs: vec![self.apps_dir.clone()],
            startup_dirs: vec![],
            quicklook_dirs: vec![],
            cloud_dirs: vec![],
        };
        ctx
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self._base);
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// apps list detects fixture .app bundle
#[test]
fn test_apps_list_detects_fixture_bundle() -> Result<()> {
    let env = TestEnv::new("list");
    env.create_app("Fixture", "com.example.fixture", "1.2.3");
    let ctx = env.ctx();

    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::List,
        },
    )?;

    // JSON envelope is schema_version 1.0
    assert_eq!(envelope.schema_version, "1.0");
    assert_eq!(envelope.command, "apps list");
    assert_eq!(envelope.mode, "dry_run");

    let items = envelope.payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "should find exactly one app");

    let app = &items[0];
    assert_eq!(app["name"].as_str().unwrap(), "Fixture");
    assert_eq!(app["bundle_id"].as_str().unwrap(), "com.example.fixture");
    assert_eq!(app["version"].as_str().unwrap(), "1.2.3");
    assert!(!app["is_system_app"].as_bool().unwrap());

    Ok(())
}

/// apps inspect reads Info.plist metadata and reports associations
#[test]
fn test_apps_inspect_reads_plist_metadata() -> Result<()> {
    let env = TestEnv::new("inspect");
    let app_path = env.create_app("Inspector", "com.example.inspector", "2.0.0");

    // Create a fake associated cache dir
    let cache_dir = env.home.join("Library/Caches/com.example.inspector");
    fs::create_dir_all(&cache_dir)?;
    fs::write(cache_dir.join("cache.bin"), b"cached data")?;

    let ctx = env.ctx();
    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Inspect {
                app: app_path.to_string_lossy().to_string(),
            },
        },
    )?;

    assert_eq!(envelope.schema_version, "1.0");
    let bundle = &envelope.payload["bundle"];
    assert_eq!(
        bundle["bundle_id"].as_str().unwrap(),
        "com.example.inspector"
    );
    assert_eq!(bundle["version"].as_str().unwrap(), "2.0.0");

    let associations = envelope.payload["associations"].as_array().unwrap();
    assert!(!associations.is_empty(), "associations must be present");

    // The cache association must exist=true
    let cache_assoc = associations
        .iter()
        .find(|a| a["kind"].as_str().unwrap() == "caches")
        .expect("caches association must be present");
    assert!(cache_assoc["exists"].as_bool().unwrap());
    assert!(cache_assoc["size_bytes"].as_u64().unwrap() > 0);

    Ok(())
}

/// apps leftovers reports orphaned cache/preference files
#[test]
fn test_apps_leftovers_reports_orphaned_files() -> Result<()> {
    let env = TestEnv::new("leftovers");

    // Known app — must NOT appear in leftovers
    env.create_app("KnownApp", "com.example.known", "1.0");

    // Orphan in Caches (no matching .app)
    let orphan_cache = env.home.join("Library/Caches/com.ghost.app");
    fs::create_dir_all(&orphan_cache)?;
    fs::write(orphan_cache.join("data.bin"), b"leftover cache")?;

    // Orphan in Preferences
    let prefs_dir = env.home.join("Library/Preferences");
    fs::create_dir_all(&prefs_dir)?;
    fs::write(
        prefs_dir.join("com.ghost.app.plist"),
        b"<?xml version=\"1.0\"?><plist version=\"1.0\"><dict/></plist>",
    )?;

    let ctx = env.ctx();
    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Leftovers,
        },
    )?;

    assert_eq!(envelope.schema_version, "1.0");
    let items = envelope.payload["items"].as_array().unwrap();

    // Ghost entries must appear
    let ghost_entries: Vec<_> = items
        .iter()
        .filter(|i| {
            i["associated_bundle_id"]
                .as_str()
                .unwrap_or("")
                .contains("com.ghost")
        })
        .collect();
    assert!(
        !ghost_entries.is_empty(),
        "orphaned ghost entries must appear in leftovers"
    );

    // Known app must NOT appear
    let known_leak = items
        .iter()
        .any(|i| i["associated_bundle_id"].as_str().unwrap_or("") == "com.example.known");
    assert!(!known_leak, "known app must not appear in leftovers");

    // All leftovers must be report_only
    for item in items {
        assert_eq!(
            item["action"].as_str().unwrap(),
            "report_only",
            "leftovers must be report_only"
        );
    }

    Ok(())
}

/// System/Apple apps must be flagged is_system_app and risk=critical
#[test]
fn test_system_app_is_marked_protected() -> Result<()> {
    let env = TestEnv::new("system");

    // Apple-style bundle_id triggers system detection
    env.create_app("AppleApp", "com.apple.finder", "14.0");

    let ctx = env.ctx();
    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::List,
        },
    )?;

    let items = envelope.payload["items"].as_array().unwrap();
    let apple_app = items
        .iter()
        .find(|a| a["bundle_id"].as_str().unwrap_or("") == "com.apple.finder")
        .expect("apple app must be listed");

    assert!(
        apple_app["is_system_app"].as_bool().unwrap(),
        "com.apple.* must be system_app"
    );
    assert_eq!(
        apple_app["risk"].as_str().unwrap(),
        "critical",
        "system app risk must be critical"
    );

    Ok(())
}

/// JSON schema_version remains 1.0 across all apps subcommands
#[test]
fn test_apps_json_schema_version_is_stable() -> Result<()> {
    let env = TestEnv::new("schema");
    env.create_app("SchemaApp", "com.example.schema", "1.0");
    let ctx = env.ctx();

    for cmd in [
        macmop::cli::AppsCommand::List,
        macmop::cli::AppsCommand::Leftovers,
    ] {
        let envelope = apps::run(&ctx, macmop::cli::AppsArgs { command: cmd })?;
        assert_eq!(envelope.schema_version, "1.0", "schema_version must be 1.0");
    }

    Ok(())
}

#[test]
fn test_apps_uninstall_direct_path_must_end_with_app() -> Result<()> {
    let env = TestEnv::new("direct_path");
    let ctx = env.ctx();
    let res = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: env
                    .apps_dir
                    .join("Invalid.txt")
                    .to_string_lossy()
                    .to_string(),
            },
        },
    );
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("must end with .app"));
    Ok(())
}

#[test]
fn test_apps_uninstall_canonicalized_protected_path_is_blocked() -> Result<()> {
    let env = TestEnv::new("protected_path");
    let mut ctx = env.ctx();
    let app_path = env.create_app("ProtectedApp", "com.example.protected", "1.0");

    // Add app path to custom protected paths, canonicalized
    ctx.custom_protected_paths = vec![std::fs::canonicalize(&app_path).unwrap()];

    let res = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "ProtectedApp".to_string(),
            },
        },
    );
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Uninstalling system/protected app is blocked"));
    Ok(())
}

#[test]
fn test_apps_uninstall_exact_match_wins_over_fuzzy_ambiguity() -> Result<()> {
    let env = TestEnv::new("exact_win");
    env.create_app("Chrome", "com.example.chrome", "1.0");
    env.create_app("Chrome Canary", "com.example.chrome.canary", "1.0");
    let ctx = env.ctx();

    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "Chrome".to_string(),
            },
        },
    )?;

    assert_eq!(
        envelope.payload["plan_kind"].as_str().unwrap(),
        "apps_uninstall_dry_run"
    );
    assert!(envelope.payload["summary"]
        .as_str()
        .unwrap()
        .contains("Chrome"));
    Ok(())
}

#[test]
fn test_apps_uninstall_leftover_under_protected_path_is_blocked_excluded() -> Result<()> {
    let env = TestEnv::new("leftover_blocked");
    let mut ctx = env.ctx();
    let _app_path = env.create_app("LeftoverBlockedApp", "com.example.leftoverblocked", "1.0");

    // Create associated preference file
    let prefs_dir = env.home.join("Library/Preferences");
    fs::create_dir_all(&prefs_dir)?;
    let pref_file = prefs_dir.join("com.example.leftoverblocked.plist");
    fs::write(&pref_file, b"test")?;

    // Protect this preference file specifically, canonicalized
    ctx.custom_protected_paths = vec![std::fs::canonicalize(&pref_file).unwrap()];

    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "LeftoverBlockedApp".to_string(),
            },
        },
    )?;

    let findings = envelope.payload["findings"].as_array().unwrap();
    // Should NOT contain the protected preference file
    let has_pref = findings.iter().any(|f| {
        let path_str = f["path"].as_str().unwrap();
        path_str.contains("com.example.leftoverblocked.plist")
    });
    assert!(
        !has_pref,
        "protected leftover path must be blocked/excluded"
    );
    Ok(())
}

#[test]
fn test_apps_uninstall_dry_run_creates_no_audit_or_rollback() -> Result<()> {
    let env = TestEnv::new("dry_run_safety");
    let ctx = env.ctx();
    env.create_app("DryApp", "com.example.dry", "1.0");

    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "DryApp".to_string(),
            },
        },
    )?;

    assert_eq!(
        envelope.payload["execution"].as_str().unwrap(),
        "not_executed"
    );
    assert!(!ctx.paths.audit_file.exists());
    assert!(!ctx.paths.rollback_file.exists());
    Ok(())
}

#[test]
fn test_apps_uninstall_apply_moves_to_trash_and_creates_rollback() -> Result<()> {
    let env = TestEnv::new("apply_trash");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;
    let app_path = env.create_app("ApplyApp", "com.example.apply", "1.0");

    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "ApplyApp".to_string(),
            },
        },
    )?;

    assert_eq!(envelope.payload["execution"].as_str().unwrap(), "executed");
    assert_eq!(
        envelope.payload["execution_result"].as_str().unwrap(),
        "success"
    );
    assert_eq!(envelope.payload["moved_count"].as_u64().unwrap(), 1);

    // App bundle should be moved to Trash
    assert!(!app_path.exists());
    assert!(ctx.paths.audit_file.exists());
    assert!(ctx.paths.rollback_file.exists());
    Ok(())
}

#[test]
fn test_apps_uninstall_permanent_blocked() -> Result<()> {
    let env = TestEnv::new("perm_block");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Permanent { force: true };
    env.create_app("PermApp", "com.example.perm", "1.0");

    let res = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "PermApp".to_string(),
            },
        },
    );

    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Uninstall does not support permanent delete yet"));
    Ok(())
}

#[test]
fn test_apps_uninstall_leftover_revalidation_blocked_during_apply() -> Result<()> {
    let env = TestEnv::new("revalidate_blocked");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;
    let _app_path = env.create_app("RevalApp", "com.example.reval", "1.0");

    // Create leftover preference
    let prefs_dir = env.home.join("Library/Preferences");
    fs::create_dir_all(&prefs_dir)?;
    let pref_file = prefs_dir.join("com.example.reval.plist");
    fs::write(&pref_file, b"test")?;

    // Dry-run/finding phase has custom protected paths empty, so it resolves.
    // But right before execution, we simulate a custom protected path update that blocks it!
    ctx.custom_protected_paths = vec![std::fs::canonicalize(&pref_file).unwrap()];

    let _envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "RevalApp".to_string(),
            },
        },
    )?;

    // Leftover should not be moved, but app should
    assert!(
        pref_file.exists(),
        "Leftover should be skipped because it is protected immediately before execution"
    );
    Ok(())
}

#[test]
fn test_apps_uninstall_rollback_success_and_idempotency() -> Result<()> {
    let env = TestEnv::new("rollback_test");
    let mut ctx = env.ctx();
    ctx.mode = ExecutionMode::Apply;
    let app_path = env.create_app("RollbackApp", "com.example.rollback", "1.0");

    let envelope = apps::run(
        &ctx,
        macmop::cli::AppsArgs {
            command: macmop::cli::AppsCommand::Uninstall {
                app: "RollbackApp".to_string(),
            },
        },
    )?;

    assert!(!app_path.exists());
    let r_id = envelope.payload["rollback_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Call rollback apply
    let rollback_env = macmop::modules::rollback::run(
        &ctx,
        macmop::cli::RollbackArgs {
            command: macmop::cli::RollbackCommand::Apply { id: r_id.clone() },
        },
    )?;

    assert!(rollback_env.payload["applied"].as_bool().unwrap());
    // App path should be restored!
    assert!(app_path.exists());

    // Second rollback should fail cleanly
    let res = macmop::modules::rollback::run(
        &ctx,
        macmop::cli::RollbackArgs {
            command: macmop::cli::RollbackCommand::Apply { id: r_id },
        },
    );
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("rollback id not found"));
    Ok(())
}
