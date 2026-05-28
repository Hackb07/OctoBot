use tokio::sync::mpsc;

use crate::{
    models::{ExplainabilityRecord, OpsEvent, OpsState, RecoveryAction, RecoveryStatus, UserRole},
    runtime::parse_allowlisted_command,
    utils::{next_id, now_ts},
};

pub(crate) struct RemediationEngine;

impl RemediationEngine {
    /// Safely execute a remediation command.
    /// Returns (approved, command_to_run, error_message).
    pub(crate) fn evaluate(
        &self,
        command: &str,
        target: &str,
        reason: &str,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) -> RemediationDecision {
        // 1. Check if command is syntactically valid
        if parse_allowlisted_command(command).is_err() {
            // Check if this is a remediation-specific pattern
            if !is_remediation_command(command) {
                return RemediationDecision::Rejected(format!(
                    "Command `{command}` is not in the allowlist or remediation patterns"
                ));
            }
        }

        // 2. Evaluate risk level
        let risk_level = self.risk_level(command, target);
        let requires_approval = risk_level == RiskLevel::High
            || state
                .sandbox_policy
                .review_required_for
                .iter()
                .any(|pattern| command.contains(pattern) || target.contains(pattern));

        // 3. Check RBAC
        let role = &state.current_role;
        if requires_approval && !role.can_approve_recovery() {
            return RemediationDecision::Rejected(format!(
                "Operation requires {:?} role or higher",
                UserRole::Operator
            ));
        }

        // 4. Build approval action if needed
        if requires_approval && !role.can_approve_recovery() {
            return RemediationDecision::NeedsApproval(format!(
                "Approval needed for {command} on {target}"
            ));
        }

        // 5. Auto-execute if safe
        if requires_approval {
            let action_id = next_id("rec");
            let _ = event_tx.send(OpsEvent::RecoveryProposed {
                action: RecoveryAction {
                    id: action_id.clone(),
                    name: format!("{reason}: {command} on {target}"),
                    command: command.into(),
                    target: target.into(),
                    status: RecoveryStatus::AwaitingApproval,
                    risk: format!("{:?} risk operation", risk_level),
                    requires_role: UserRole::Operator,
                    evidence: vec![
                        format!("command={command}"),
                        format!("target={target}"),
                        format!("reason={reason}"),
                    ],
                    requested_by: "remediation-engine".into(),
                    approved_by: None,
                    dry_run_only: false,
                    timestamp: now_ts(),
                },
            });
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: ExplainabilityRecord {
                    id: next_id("exp-remediation"),
                    action: format!("remediation-proposed-{action_id}"),
                    why: format!("{reason}: {command} on {target}"),
                    evidence: vec![
                        format!("risk={:?}", risk_level),
                        format!("requires_approval={requires_approval}"),
                    ],
                    confidence: 85,
                    tools_used: vec!["remediation-engine".into()],
                    timestamp: now_ts(),
                },
            });
            RemediationDecision::NeedsApproval(action_id)
        } else {
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: ExplainabilityRecord {
                    id: next_id("exp-remediation-auto"),
                    action: format!("remediation-auto-{target}"),
                    why: format!("Auto-approved {command} on {target}: {reason}"),
                    evidence: vec![
                        format!("risk={:?}", risk_level),
                        "auto-approved_by_policy".into(),
                    ],
                    confidence: 90,
                    tools_used: vec!["remediation-engine".into()],
                    timestamp: now_ts(),
                },
            });
            RemediationDecision::Approved(command.to_string())
        }
    }

    /// Determine risk level of a remediation operation.
    pub(crate) fn risk_level(&self, command: &str, _target: &str) -> RiskLevel {
        if command.starts_with("systemctl restart")
            || command.starts_with("kubectl rollout undo")
            || command.starts_with("docker restart")
        {
            RiskLevel::High
        } else if command.starts_with("kubectl scale")
            || command.starts_with("systemctl start")
            || command.starts_with("systemctl stop")
        {
            RiskLevel::Medium
        } else {
            RiskLevel::Low
        }
    }

    /// Execute a recovery and verify the result.
    pub(crate) async fn execute_and_verify(
        &self,
        action: &RecoveryAction,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) -> bool {
        // 1. Execute the command via event bus
        let cmd_id = next_id("remediation");
        let _ = event_tx.send(OpsEvent::CommandRequested {
            id: cmd_id.clone(),
            command: action.command.clone(),
            reason: format!("Remediation execution for {}", action.id),
            dry_run: false,
            timestamp: now_ts(),
        });

        // 2. Create a span for tracing
        let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
            record: ExplainabilityRecord {
                id: next_id("exp-verify"),
                action: format!("remediation-execute-{}", action.id),
                why: format!(
                    "Executing {cmd} on {target}",
                    cmd = action.command,
                    target = action.target
                ),
                evidence: vec![
                    format!("action_id={}", action.id),
                    format!("command={}", action.command),
                    format!("target={}", action.target),
                ],
                confidence: 80,
                tools_used: vec!["remediation-engine".into()],
                timestamp: now_ts(),
            },
        });

        // 3. Verification: check if the target infra node is healthy
        let successfully_recovered = state
            .infra
            .iter()
            .find(|n| n.name == action.target)
            .map(|n| n.health >= 80)
            .unwrap_or(false);

        if successfully_recovered {
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: ExplainabilityRecord {
                    id: next_id("exp-verified"),
                    action: format!("remediation-verified-{}", action.id),
                    why: format!(
                        "Recovery verified: {target} health >= 80",
                        target = action.target
                    ),
                    evidence: vec!["remediation_successful".into()],
                    confidence: 95,
                    tools_used: vec!["remediation-engine".into()],
                    timestamp: now_ts(),
                },
            });
        } else {
            let health = state
                .infra
                .iter()
                .find(|n| n.name == action.target)
                .map(|n| n.health)
                .unwrap_or(0);
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: ExplainabilityRecord {
                    id: next_id("exp-verify-failed"),
                    action: format!("remediation-verify-failed-{}", action.id),
                    why: format!(
                        "Recovery may not have succeeded: {target} health = {health}",
                        target = action.target
                    ),
                    evidence: vec![
                        "remediation_verification_failed".into(),
                        format!("target_health={health}"),
                    ],
                    confidence: 60,
                    tools_used: vec!["remediation-engine".into()],
                    timestamp: now_ts(),
                },
            });
        }

        successfully_recovered
    }

    /// Build a self-healing workflow from an incident failure pattern.
    pub(crate) fn create_self_healing_workflow(
        &self,
        failed_service: &str,
        incident_id: &str,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        use crate::workflows::{DagWorkflowRuntime, WorkflowNode, WorkflowNodeKind};

        let wf_id = format!("self-heal-{incident_id}");
        let wf = DagWorkflowRuntime::new(
            wf_id.clone(),
            format!("Self-heal: {failed_service}"),
            vec![
                WorkflowNode {
                    id: "diagnose".into(),
                    kind: WorkflowNodeKind::Command,
                    command: Some("systemctl --no-pager --failed".to_string()),
                    agent: None,
                    depends_on: vec![],
                    retry: Default::default(),
                    approval_required: false,
                    condition: None,
                    on_success: Some("restart-service".into()),
                    on_failure: Some("escalate".into()),
                    rollback: None,
                },
                WorkflowNode {
                    id: "restart-service".into(),
                    kind: WorkflowNodeKind::Command,
                    command: Some(format!("systemctl restart {failed_service}")),
                    agent: None,
                    depends_on: vec!["diagnose".into()],
                    retry: crate::workflows::RetryPolicy {
                        attempts: 2,
                        backoff_ms: 2000,
                    },
                    approval_required: true,
                    condition: None,
                    on_success: Some("verify".into()),
                    on_failure: Some("escalate".into()),
                    rollback: None,
                },
                WorkflowNode {
                    id: "verify".into(),
                    kind: WorkflowNodeKind::Command,
                    command: Some(format!("systemctl is-active {failed_service}")),
                    agent: None,
                    depends_on: vec!["restart-service".into()],
                    retry: Default::default(),
                    approval_required: false,
                    condition: None,
                    on_success: None,
                    on_failure: Some("escalate".into()),
                    rollback: None,
                },
                WorkflowNode {
                    id: "escalate".into(),
                    kind: WorkflowNodeKind::Approval,
                    command: None,
                    agent: None,
                    depends_on: vec!["restart-service".into(), "verify".into()],
                    retry: Default::default(),
                    approval_required: true,
                    condition: None,
                    on_success: None,
                    on_failure: None,
                    rollback: None,
                },
            ],
        );

        let _ = event_tx.send(OpsEvent::WorkflowDefinitionLoaded {
            definition: wf.summary(),
        });
    }
}

fn is_remediation_command(command: &str) -> bool {
    let parts: Vec<&str> = command.split_whitespace().collect();
    matches!(
        parts.as_slice(),
        ["systemctl", "restart", _]
            | ["systemctl", "start", _]
            | ["systemctl", "stop", _]
            | ["systemctl", "reload", _]
            | ["kubectl", "rollout", "undo", ..]
            | ["kubectl", "scale", ..]
            | ["docker", "restart", _]
    )
}

#[derive(Debug)]
pub(crate) enum RemediationDecision {
    Approved(String),
    NeedsApproval(String),
    Rejected(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RiskLevel {
    Low,
    Medium,
    High,
}
