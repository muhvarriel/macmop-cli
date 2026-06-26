use anyhow::Result;
use std::fs;
use std::path::PathBuf;

struct TestEnv {
    base: PathBuf,
    home: PathBuf,
}

impl TestEnv {
    fn new(test_name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "macmop-test-config-{}-{}-{}",
            test_name,
            macmop::core::unix_now(),
            std::process::id()
        ));
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        Self { base, home }
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

#[test]
fn test_config_invalid_output_and_profile_fail_validation() -> Result<()> {
    let env = TestEnv::new("invalid_vals");
    let config_path = env.home.join("config.toml");

    // Invalid output format
    fs::write(
        &config_path,
        r#"
[defaults]
output = "invalid_format"
"#,
    )?;
    let result = macmop::core::Config::load_from_path(&config_path);
    assert!(result.is_err());
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("invalid defaults.output"));

    // Invalid profile
    fs::write(
        &config_path,
        r#"
[defaults]
profile = "super_deep"
"#,
    )?;
    let result = macmop::core::Config::load_from_path(&config_path);
    assert!(result.is_err());
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("invalid defaults.profile"));

    Ok(())
}

#[test]
fn test_config_cannot_remove_builtin_protected_paths() -> Result<()> {
    let env = TestEnv::new("safety_additivity");
    let custom_path = env.home.join("custom_protected");
    fs::create_dir_all(&custom_path)?;

    // We configure a custom protected path
    let policy = macmop::policy::Policy::new(env.home.clone(), vec![custom_path.clone()]);

    // Built-in protected path must still be protected
    assert!(policy.is_protected(&env.home.join(".ssh/id_rsa")));
    // Custom protected path must be protected
    assert!(policy.is_protected(&custom_path));

    Ok(())
}

#[test]
fn test_missing_config_is_ok_for_normal_commands() -> Result<()> {
    let env = TestEnv::new("missing_config");
    let config_path = env.home.join("nonexistent_config.toml");
    let result = macmop::core::Config::load_from_path(&config_path);
    assert!(result.is_ok());

    Ok(())
}
