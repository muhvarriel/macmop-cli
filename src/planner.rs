use crate::core::{
    new_id, unix_now, ActionPlan, ExecutionMode, PlanId, PlannedAction, PlannedActionKind,
    ScanFinding,
};

pub fn build_action_plan(findings: &[ScanFinding], mode: &ExecutionMode) -> ActionPlan {
    let actions: Vec<PlannedAction> = findings
        .iter()
        .filter_map(|finding| {
            let action = match mode {
                ExecutionMode::DryRun => PlannedActionKind::ReportOnly,
                ExecutionMode::Apply => {
                    if finding.action == PlannedActionKind::MoveToTrash {
                        PlannedActionKind::MoveToTrash
                    } else {
                        PlannedActionKind::ReportOnly
                    }
                }
                ExecutionMode::Permanent { force: true } => {
                    if finding.action == PlannedActionKind::MoveToTrash {
                        PlannedActionKind::PermanentDelete
                    } else {
                        PlannedActionKind::ReportOnly
                    }
                }
                ExecutionMode::Permanent { force: false } => PlannedActionKind::ReportOnly,
            };
            if action == PlannedActionKind::ReportOnly && mode.is_destructive() {
                return None;
            }
            Some(PlannedAction {
                finding_id: finding.id.clone(),
                action,
                path: finding.path.clone(),
                rollback_supported: action == PlannedActionKind::MoveToTrash,
            })
        })
        .collect();

    ActionPlan {
        id: PlanId(new_id("plan")),
        created_at: unix_now(),
        total_items: actions.len(),
        total_size_bytes: findings.iter().map(|f| f.size_bytes).sum(),
        actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{FindingId, RiskLevel};
    use std::path::PathBuf;

    #[test]
    fn dry_run_plan_never_deletes() {
        let findings = vec![ScanFinding {
            id: FindingId("f1".into()),
            module: "cleanup".into(),
            category: "cache".into(),
            path: PathBuf::from("/tmp/a"),
            size_bytes: 1,
            risk: RiskLevel::Low,
            confidence: 1.0,
            action: PlannedActionKind::MoveToTrash,
            reason: "test".into(),
            requires_sudo: false,
        }];
        let plan = build_action_plan(&findings, &ExecutionMode::DryRun);
        assert_eq!(plan.actions[0].action, PlannedActionKind::ReportOnly);
    }
}
