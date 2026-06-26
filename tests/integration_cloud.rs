use anyhow::Result;
use macmop::cli::{CloudArgs, CloudCommand};
use macmop::core::{AppContext, AppPaths, ExecutionMode, OutputFormat};
use std::fs;
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};

struct TestEnv {
    base: PathBuf,
    home: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "macmop-test-cloud-{}-{}-{}",
            test_name,
            macmop::core::unix_now(),
            std::process::id()
        ));
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        Self { base, home }
    }

    fn context(&self, cloud_dirs: Vec<(String, PathBuf)>) -> AppContext {
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut ctx =
            AppContext::load(None, ExecutionMode::DryRun, OutputFormat::Json, cancelled).unwrap();
        ctx.paths = AppPaths {
            home: self.home.clone(),
            data_dir: self.base.join("data"),
            trash: self.base.join("trash"),
            audit_file: self.base.join("data/audit.json"),
            rollback_file: self.base.join("data/rollback.json"),
            apps_dirs: vec![],
            startup_dirs: vec![],
            quicklook_dirs: vec![],
            cloud_dirs,
        };
        ctx
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn test_cloud_providers_detects_fixture_paths() -> Result<()> {
    let env = TestEnv::new("providers");
    let provider_path = env.home.join("MockiCloud");
    fs::create_dir_all(&provider_path)?;

    let ctx = env.context(vec![
        ("iCloud Drive".to_string(), provider_path.clone()),
        ("Dropbox".to_string(), env.home.join("DropboxNonExistent")),
    ]);

    let args = CloudArgs {
        command: CloudCommand::Providers,
    };
    let result = macmop::modules::cloud::run(&ctx, args)?;
    assert_eq!(result.schema_version, "1.0");

    let payload = &result.payload;
    assert!(payload["sync_warning"]
        .as_str()
        .unwrap()
        .contains("Deleting synchronized files"));

    let providers = payload["providers"].as_array().unwrap();
    assert_eq!(providers.len(), 2);

    let icloud = providers
        .iter()
        .find(|p| p["provider"] == "iCloud Drive")
        .unwrap();
    assert_eq!(icloud["exists"], true);

    let dropbox = providers
        .iter()
        .find(|p| p["provider"] == "Dropbox")
        .unwrap();
    assert_eq!(dropbox["exists"], false);

    Ok(())
}

#[test]
fn test_cloud_scan_reports_sampled_stats_and_warnings() -> Result<()> {
    let env = TestEnv::new("scan");
    let provider_path = env.home.join("MockDropbox");
    fs::create_dir_all(&provider_path)?;
    fs::write(provider_path.join("file1.txt"), b"hello")?;
    fs::write(provider_path.join("file2.txt"), b"world")?;

    let ctx = env.context(vec![("Dropbox".to_string(), provider_path.clone())]);

    let args = CloudArgs {
        command: CloudCommand::Scan,
    };
    let result = macmop::modules::cloud::run(&ctx, args)?;
    assert_eq!(result.schema_version, "1.0");

    let payload = &result.payload;
    let summary = &payload["summary"];
    assert_eq!(summary["providers_detected"], 1);
    assert_eq!(summary["total_sampled_size_bytes"], 10);
    assert!(summary["sync_warning"]
        .as_str()
        .unwrap()
        .contains("Deleting synchronized files"));

    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["sampled_file_count"], 2);
    assert_eq!(items[0]["sampled_size_bytes"], 10);
    assert_eq!(items[0]["scan_limited"], false);

    let findings = payload["findings"].as_array().unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0]["action"], "report_only");

    // Verify no audit or rollback files were created
    assert!(!ctx.paths.audit_file.exists());
    assert!(!ctx.paths.rollback_file.exists());

    Ok(())
}

#[test]
fn test_cloud_scan_limited_cap() -> Result<()> {
    let env = TestEnv::new("limit");
    let provider_path = env.home.join("MockLimit");
    fs::create_dir_all(&provider_path)?;

    // Generate 10005 files (fast empty files)
    for i in 0..10_005 {
        fs::write(provider_path.join(format!("f{}.txt", i)), b"")?;
    }

    let ctx = env.context(vec![("OneDrive".to_string(), provider_path.clone())]);

    let args = CloudArgs {
        command: CloudCommand::Scan,
    };
    let result = macmop::modules::cloud::run(&ctx, args)?;
    let payload = &result.payload;
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items[0]["scan_limited"], true);
    assert!(items[0]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w.as_str().unwrap().contains("Scan was bounded")));

    Ok(())
}
