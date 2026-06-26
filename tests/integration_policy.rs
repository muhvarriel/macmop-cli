use anyhow::Result;
use macmop::cli::ClutterArgs;
use macmop::core::{
    new_id, AppContext, ExecutionMode, FindingId, OutputFormat, PlannedActionKind, RiskLevel,
    ScanFinding,
};
use macmop::planner::build_action_plan;
use macmop::policy::Policy;
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
fn test_protected_paths_are_never_planned_for_deletion() -> Result<()> {
    let env = TestEnv::new("policy");

    // Create a protected file:
    let ssh_dir = env.home.join(".ssh");
    fs::create_dir_all(&ssh_dir)?;
    let protected_file = ssh_dir.join("id_rsa");
    fs::write(&protected_file, "ssh key contents")?;

    let policy = Policy::new(env.home.clone());

    let mut finding = ScanFinding {
        id: FindingId(new_id("finding")),
        module: "cleanup".to_string(),
        category: "cache".to_string(),
        path: protected_file,
        size_bytes: 100,
        risk: RiskLevel::Low,
        confidence: 0.95,
        action: PlannedActionKind::MoveToTrash,
        reason: "mock cache".to_string(),
        requires_sudo: false,
    };

    // Enforce policy
    policy.enforce_finding(&mut finding);

    // Verify finding for protected file is marked report-only/critical
    assert_eq!(finding.risk, RiskLevel::Critical);
    assert_eq!(finding.action, PlannedActionKind::ReportOnly);

    // Verify ActionPlan does not contain destructive action for the protected path
    let plan = build_action_plan(&[finding], &ExecutionMode::Apply);

    let plan_for_protected = plan.actions.iter().any(|act| {
        act.path.to_string_lossy().contains("id_rsa") && act.action != PlannedActionKind::ReportOnly
    });
    assert!(
        !plan_for_protected,
        "ActionPlan must not plan destructive action for protected path"
    );

    Ok(())
}

#[test]
fn test_protected_paths_e2e_clutter() -> Result<()> {
    let env = TestEnv::new("policy_e2e");

    // Create a protected file:
    let ssh_dir = env.home.join(".ssh");
    fs::create_dir_all(&ssh_dir)?;
    let protected_file = ssh_dir.join("id_rsa");
    fs::write(&protected_file, "ssh key contents")?;

    // Run clutter scan on home
    let ctx = env.context(ExecutionMode::Apply);
    let args = ClutterArgs {
        path: Some(env.home.clone()),
        min_size: 0,
        top: 10,
    };

    let result = macmop::modules::clutter::run(&ctx, args)?;
    let payload = &result.payload;

    let findings = payload.get("findings").unwrap().as_array().unwrap();
    let action_plan = payload.get("action_plan").unwrap();
    let actions = action_plan.get("actions").unwrap().as_array().unwrap();

    // The protected file must be reported
    let protected_finding = findings
        .iter()
        .find(|f| f.get("path").unwrap().as_str().unwrap().contains("id_rsa"));
    assert!(protected_finding.is_some());
    let finding = protected_finding.unwrap();
    assert_eq!(finding.get("risk").unwrap().as_str().unwrap(), "critical");
    assert_eq!(
        finding.get("action").unwrap().as_str().unwrap(),
        "report_only"
    );

    // Verify ActionPlan does not contain destructive action for the protected path
    let plan_for_protected = actions.iter().any(|act| {
        act.get("path")
            .unwrap()
            .as_str()
            .unwrap()
            .contains("id_rsa")
            && act.get("action").unwrap().as_str().unwrap() != "report_only"
    });
    assert!(
        !plan_for_protected,
        "E2E: ActionPlan must not plan destructive action for protected path"
    );

    Ok(())
}
