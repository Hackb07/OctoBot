use crate::{
    ai::{AgentKind, AgentProfile, AgentPrompt, AiClient, ToolSpec},
    infra::InfraIntegrations,
    models::{
        AgentRole, AgentRuntime, AgentRuntimeKind, AgentStatus, ExplainabilityRecord,
        KnowledgeEdge, OpsEvent, OpsState, PluginDescriptor, PluginKind, PluginStatus,
        RecoveryAction, RecoveryStatus, RuntimeStatus, SandboxPolicy, TimelineCategory,
        TimelineEvent, UserRole,
    },
    persistence::{event_type, reconstruct_state},
    plugins::host::NativePlugin,
    plugins::registry::PluginRegistry,
    remediation::{RemediationEngine, RiskLevel},
    runtime::parse_allowlisted_command,
    trace::TraceEngine,
    ui::command_completion,
    utils::now_ts,
    workflows::{DagWorkflowRuntime, WorkflowNodeKind},
};

use serde_json::json;

#[test]
fn allowlist_accepts_read_only_infrastructure_commands() {
    for command in [
        "docker ps",
        "kubectl get pods",
        "journalctl -n 20 --no-pager",
        "journalctl -f -n 20 --no-pager",
        "systemctl --no-pager --failed",
        "ps aux",
        "df -h",
        "uptime",
        "ssh ops@example-host uptime",
    ] {
        assert!(
            parse_allowlisted_command(command).is_ok(),
            "{command} should be allowed"
        );
    }
}

#[test]
fn allowlist_blocks_write_or_shell_commands() {
    for command in [
        "kubectl delete pod auth",
        "systemctl restart nginx",
        "rm -rf /tmp/example",
        "sh -c uptime",
        "ssh host reboot",
    ] {
        assert!(
            parse_allowlisted_command(command).is_err(),
            "{command} should be blocked"
        );
    }
}

#[test]
fn reducer_records_explainability_events() {
    let mut state = OpsState::seed();
    let record = ExplainabilityRecord {
        id: "exp-test".into(),
        action: "Validate incident".into(),
        why: "Evidence threshold reached".into(),
        evidence: vec!["journalctl sample".into()],
        confidence: 88,
        tools_used: vec!["journalctl".into()],
        timestamp: now_ts(),
    };

    state.apply_event(OpsEvent::ExplainabilityRecorded {
        record: record.clone(),
    });

    assert!(state.explainability.iter().any(|item| item.id == record.id));
    assert!(
        state
            .events
            .iter()
            .any(|event| matches!(event, OpsEvent::ExplainabilityRecorded { .. }))
    );
}

#[test]
fn reducer_tracks_command_lifecycle() {
    let mut state = OpsState::seed();
    state.apply_event(OpsEvent::CommandRequested {
        id: "cmd-test".into(),
        command: "uptime".into(),
        reason: "unit test".into(),
        dry_run: false,
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::CommandExecuted {
        id: "cmd-test".into(),
        command: "uptime".into(),
        success: true,
        exit_code: Some(0),
        stdout: "up 1 day".into(),
        stderr: String::new(),
        timestamp: now_ts(),
    });

    let execution = state
        .executions
        .iter()
        .find(|item| item.id == "cmd-test")
        .expect("execution should be recorded");
    assert_eq!(execution.status, "completed");
    assert_eq!(execution.exit_code, Some(0));
}

#[test]
fn command_completion_uses_default_commands() {
    assert_eq!(
        command_completion("exec j"),
        Some("exec journalctl -n 40 --no-pager")
    );
    assert_eq!(
        command_completion("invest"),
        Some("investigate local_model_health")
    );
    assert_eq!(
        command_completion(""),
        Some("multi-agent Assess Ollama readiness and report findings")
    );
    assert_eq!(command_completion("unknown"), None);
}

#[test]
fn initial_state_has_no_seeded_agents() {
    let state = OpsState::seed();

    assert!(state.agents.is_empty());
    assert!(state.runtimes.is_empty());
    assert!(state.coordination_links.is_empty());
    assert_eq!(state.active_agents, 0);
    assert!(state.incidents.is_empty());
    assert!(state.infra.is_empty());
}

#[test]
fn reducer_tracks_dynamic_agent_lifecycle() {
    let mut state = OpsState::seed();
    state.apply_event(OpsEvent::AgentSpawned {
        name: "agent-test".into(),
        role: AgentRole::Research,
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::RuntimeUpdated {
        runtime: AgentRuntime {
            agent: "agent-test".into(),
            kind: AgentRuntimeKind::LocalProcess,
            endpoint: "local://agent/agent-test".into(),
            status: RuntimeStatus::Local,
            heartbeat: now_ts(),
            notes: "registered".into(),
        },
    });
    state.apply_event(OpsEvent::AgentLifecycleChanged {
        agent: "agent-test".into(),
        status: AgentStatus::Running,
        task: "investigate incident".into(),
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::AgentTelemetryRecorded {
        agent: "agent-test".into(),
        metric: "task_started".into(),
        value: 1,
        timestamp: now_ts(),
    });

    let agent = state
        .agents
        .iter()
        .find(|agent| agent.name == "agent-test")
        .expect("dynamic agent should be registered");
    assert_eq!(agent.status, AgentStatus::Running);
    assert_eq!(agent.task, "investigate incident");
    assert_eq!(state.active_agents, 1);
    assert!(state.runtimes.iter().any(|runtime| {
        runtime.agent == "agent-test" && runtime.status == RuntimeStatus::Active
    }));
    assert!(
        state.timeline.iter().any(|event| {
            event.source == "agent-test" && event.summary.contains("task_started=1")
        })
    );
}

#[test]
fn reducer_keeps_completed_agent_visible_as_completed() {
    let mut state = OpsState::seed();
    state.apply_event(OpsEvent::AgentSpawned {
        name: "agent-test".into(),
        role: AgentRole::Research,
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::RuntimeUpdated {
        runtime: AgentRuntime {
            agent: "agent-test".into(),
            kind: AgentRuntimeKind::LocalProcess,
            endpoint: "local://agent/agent-test".into(),
            status: RuntimeStatus::Active,
            heartbeat: now_ts(),
            notes: "running".into(),
        },
    });
    state.apply_event(OpsEvent::AgentLifecycleChanged {
        agent: "agent-test".into(),
        status: AgentStatus::Completed,
        task: "AI task completed after reasoning loop".into(),
        timestamp: now_ts(),
    });

    let agent = state
        .agents
        .iter()
        .find(|agent| agent.name == "agent-test")
        .expect("dynamic agent should be registered");
    assert_eq!(agent.status, AgentStatus::Completed);
    assert_eq!(state.active_agents, 0);
    assert!(state.runtimes.iter().any(|runtime| {
        runtime.agent == "agent-test" && runtime.status == RuntimeStatus::Local
    }));
}

#[test]
fn reducer_records_agent_coordination_graph_edges() {
    let mut state = OpsState::seed();
    state.apply_event(OpsEvent::AgentMessageRecorded {
        from: "triage-01".into(),
        to: "research-01".into(),
        protocol: "evidence-handoff".into(),
        message: "validate rollback runbook".into(),
        confidence: 84,
        timestamp: now_ts(),
    });

    assert!(state.coordination_links.iter().any(|link| {
        link.from == "triage-01" && link.to == "research-01" && link.protocol == "evidence-handoff"
    }));
}

#[test]
fn reducer_records_time_travel_timeline_events() {
    let mut state = OpsState::seed();
    let event = TimelineEvent {
        id: "time-test".into(),
        timestamp: now_ts(),
        category: TimelineCategory::Commit,
        source: "git".into(),
        summary: "commit abc123 changed ingress timeout".into(),
        cpu: Some(73),
        memory: Some(61),
        related_incident: Some("inc-042".into()),
    };

    state.apply_event(OpsEvent::TimelineRecorded {
        event: event.clone(),
    });

    assert!(state.timeline.iter().any(|item| item.id == event.id));
}

#[test]
fn rbac_controls_recovery_approval() {
    let mut state = OpsState::seed();
    let action = RecoveryAction {
        id: "rec-test".into(),
        name: "Restart test service".into(),
        command: "systemctl restart test".into(),
        target: "test".into(),
        status: RecoveryStatus::AwaitingApproval,
        risk: "unit test".into(),
        requires_role: UserRole::Operator,
        evidence: vec!["test evidence".into()],
        requested_by: "unit-test".into(),
        approved_by: None,
        dry_run_only: true,
        timestamp: now_ts(),
    };

    state.apply_event(OpsEvent::RecoveryProposed { action });
    state.apply_event(OpsEvent::RecoveryApproved {
        action_id: "rec-test".into(),
        role: UserRole::ReadOnly,
        timestamp: now_ts(),
    });
    assert_eq!(
        state
            .recovery_actions
            .iter()
            .find(|item| item.id == "rec-test")
            .expect("recovery action should exist")
            .status,
        RecoveryStatus::Rejected
    );

    state.apply_event(OpsEvent::RecoveryApproved {
        action_id: "rec-test".into(),
        role: UserRole::Operator,
        timestamp: now_ts(),
    });
    assert_eq!(
        state
            .recovery_actions
            .iter()
            .find(|item| item.id == "rec-test")
            .expect("recovery action should exist")
            .status,
        RecoveryStatus::Approved
    );
}

#[test]
fn replay_cursor_walks_recorded_events() {
    let mut state = OpsState::seed();
    state.apply_event(OpsEvent::UserCommandEntered {
        command: "investigate nginx_latency".into(),
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::ReplayStarted {
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::ReplayStepped {
        position: 1,
        timestamp: now_ts(),
    });

    assert!(state.replay.active);
    assert_eq!(state.replay.position, 1);
    assert!(state.replay.last_event.is_some());
}

#[test]
fn event_store_reconstruction_rebuilds_state_from_events() {
    let incident_event = OpsEvent::IncidentDetected {
        incident_id: "inc-replay".into(),
        service: "checkout".into(),
        severity: "SEV2".into(),
        timestamp: now_ts(),
    };
    let workflow_event = OpsEvent::WorkflowAdvanced {
        id: "wf-replay".into(),
        stage: "collect evidence".into(),
        progress: 25,
        timestamp: now_ts(),
    };
    let explainability_event = OpsEvent::ExplainabilityRecorded {
        record: ExplainabilityRecord {
            id: "exp-replay".into(),
            action: "Replay persisted reasoning".into(),
            why: "Event sourcing must reconstruct explainability state".into(),
            evidence: vec!["append-only event".into()],
            confidence: 100,
            tools_used: vec!["event-store".into()],
            timestamp: now_ts(),
        },
    };

    let state = reconstruct_state(vec![incident_event, workflow_event, explainability_event]);

    assert!(
        state
            .incidents
            .iter()
            .any(|incident| incident.id == "inc-replay")
    );
    assert!(
        state
            .workflows
            .iter()
            .any(|workflow| workflow.id == "wf-replay")
    );
    assert!(
        state
            .explainability
            .iter()
            .any(|record| record.id == "exp-replay")
    );
    assert_eq!(state.events.len(), 3);
}

#[test]
fn event_type_names_are_stable_for_storage() {
    let event = OpsEvent::IncidentDetected {
        incident_id: "inc-type".into(),
        service: "api".into(),
        severity: "SEV3".into(),
        timestamp: now_ts(),
    };

    assert_eq!(event_type(&event), "IncidentDetected");
}

#[test]
fn ai_client_builds_tool_call_request_payload() {
    let client = AiClient::new(AgentProfile {
        kind: AgentKind::Coding,
        name: "test-agent".into(),
        model: "test-model".into(),
        purpose: "test".into(),
    });
    let body = client.request_body(&AgentPrompt {
        system: "You are an operations agent".into(),
        user: "Inspect disk pressure".into(),
        tools: vec![ToolSpec {
            name: "exec".into(),
            description: "Run an allowlisted infrastructure command".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        }],
    });

    assert_eq!(body["model"], "test-model");
    assert_eq!(body["tools"][0]["function"]["name"], "exec");
}

#[test]
fn ai_client_builds_ollama_unload_request_payload() {
    let client = AiClient::new(AgentProfile {
        kind: AgentKind::Coding,
        name: "test-agent".into(),
        model: "test-model".into(),
        purpose: "test".into(),
    });
    let body = client.unload_request_body();

    assert_eq!(body["model"], "test-model");
    assert_eq!(body["prompt"], "");
    assert_eq!(body["stream"], false);
    assert_eq!(body["keep_alive"], 0);
}

#[test]
fn reducer_records_new_phase_events() {
    let mut state = OpsState::empty();
    state.apply_event(OpsEvent::ModelHealthUpdated {
        models: vec![crate::models::ModelHealthSnapshot {
            model: "qwen2.5-coder:7b".into(),
            installed: true,
            online: true,
            size_bytes: Some(123),
            digest: Some("digest".into()),
            modified_at: Some(now_ts()),
            last_checked: now_ts(),
            notes: "installed".into(),
        }],
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::InfrastructureSnapshotRecorded {
        source: "unit-test".into(),
        nodes: vec![crate::models::InfraNode {
            name: "container-a".into(),
            kind: "docker-container".into(),
            health: 100,
            cpu: 0,
            memory: 0,
        }],
        timestamp: now_ts(),
    });

    assert_eq!(state.infra.len(), 1);
    assert_eq!(state.health, 100);
    assert_eq!(state.model_health.len(), 1);
}

#[test]
fn dag_workflow_runtime_parses_yaml_and_tracks_ready_nodes() {
    let yaml = r#"
id: wf-test
name: Test Workflow
entrypoint: collect
nodes:
  - id: collect
    kind: command
    command: uptime
  - id: approve
    kind: approval
    depends_on: [collect]
    approval_required: true
"#;
    let mut runtime = DagWorkflowRuntime::from_yaml(yaml).expect("workflow should parse");
    let ready = runtime.ready_nodes();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "collect");
    assert_eq!(ready[0].kind, WorkflowNodeKind::Command);

    runtime
        .mark_succeeded("collect")
        .expect("node should exist");
    let ready = runtime.ready_nodes();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, "approve");
    runtime
        .mark_succeeded("approve")
        .expect("node should exist");
    assert!(runtime.is_complete());
}

#[test]
fn infra_integrations_can_exist_without_fake_nodes() {
    let integrations = InfraIntegrations {
        docker_socket: None,
        kubernetes_url: None,
        prometheus_url: None,
        loki_url: None,
        opensearch_url: None,
        postgres_url: None,
    };

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    let nodes = rt
        .block_on(integrations.discover())
        .expect("empty discovery should succeed");
    assert!(nodes.is_empty());
}

#[test]
fn reducer_tracks_research_and_knowledge_features() {
    let mut state = OpsState::seed();
    state.apply_event(OpsEvent::ResearchCompleted {
        topic: "nginx_latency".into(),
        conclusion: "confidence engine refreshed".into(),
        confidence: 91,
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::KnowledgeEdgeAdded {
        edge: KnowledgeEdge {
            from: "deploy-1188".into(),
            relation: "correlates-with".into(),
            to: "inc-042".into(),
            weight: 93,
            timestamp: now_ts(),
        },
    });

    assert_eq!(state.research_profile.subject, "nginx_latency");
    assert!(state.research_profile.ranking > 0);
    assert!(
        state
            .knowledge_edges
            .iter()
            .any(|edge| edge.from == "deploy-1188" && edge.to == "inc-042")
    );
}

#[test]
fn reducer_tracks_plugins_runtimes_and_sandbox_policy() {
    let mut state = OpsState::seed();
    state.apply_event(OpsEvent::PluginRegistered {
        plugin: PluginDescriptor {
            name: "local-plugin".into(),
            kind: PluginKind::Tool,
            description: "local plugin".into(),
            version: "1.0.0".into(),
            status: PluginStatus::Registered,
            owner: "operator".into(),
        },
    });
    state.apply_event(OpsEvent::PluginStatusChanged {
        name: "local-plugin".into(),
        status: PluginStatus::Enabled,
        timestamp: now_ts(),
    });
    state.apply_event(OpsEvent::RuntimeUpdated {
        runtime: AgentRuntime {
            agent: "triage-01".into(),
            kind: AgentRuntimeKind::RemoteServer,
            endpoint: "ssh://triage-node".into(),
            status: RuntimeStatus::Active,
            heartbeat: now_ts(),
            notes: "remote runtime".into(),
        },
    });
    state.apply_event(OpsEvent::SandboxPolicyUpdated {
        policy: SandboxPolicy {
            mode: "role-aware sandbox approval".into(),
            persisted: true,
            approved_roles: vec![UserRole::Admin],
            review_required_for: vec!["restart".into()],
            updated_at: now_ts(),
        },
    });

    assert!(
        state
            .plugins
            .iter()
            .any(|plugin| plugin.name == "local-plugin" && plugin.status == PluginStatus::Enabled)
    );
    assert!(
        state
            .runtimes
            .iter()
            .any(|runtime| runtime.agent == "triage-01"
                && runtime.kind == AgentRuntimeKind::RemoteServer)
    );
    assert!(state.sandbox_policy.persisted);
    assert!(
        state
            .sandbox_policy
            .approved_roles
            .contains(&UserRole::Admin)
    );
}

#[test]
fn remediation_risk_level_assesses_commands_correctly() {
    let engine = RemediationEngine;
    assert_eq!(
        engine.risk_level("systemctl restart nginx", "nginx"),
        RiskLevel::High
    );
    assert_eq!(
        engine.risk_level("kubectl rollout undo deployment/foo", "foo"),
        RiskLevel::High
    );
    assert_eq!(
        engine.risk_level("docker restart my-container", "my-container"),
        RiskLevel::High
    );
    assert_eq!(
        engine.risk_level("kubectl scale deployment/foo --replicas=3", "foo"),
        RiskLevel::Medium
    );
    assert_eq!(engine.risk_level("uptime", ""), RiskLevel::Low);
    assert_eq!(engine.risk_level("df -h", ""), RiskLevel::Low);
}

#[test]
fn trace_engine_span_lifecycle() {
    let (event_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut engine = TraceEngine::default();

    assert_eq!(engine.active_span_count(), 0);

    let span_id = engine.start_span(None, "test-op".into(), "test-target".into(), &event_tx);
    assert!(!span_id.is_empty());
    assert_eq!(engine.active_span_count(), 1);

    engine.end_span(&span_id, true, &event_tx);
    assert_eq!(engine.active_span_count(), 0);

    let summary = engine.active_span_summary();
    assert!(summary.is_empty());
}

#[test]
fn trace_engine_nested_spans() {
    let (event_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut engine = TraceEngine::default();

    let parent = engine.start_span(None, "root".into(), "system".into(), &event_tx);
    let child = engine.start_span(
        Some(parent.clone()),
        "child".into(),
        "subsystem".into(),
        &event_tx,
    );

    assert_eq!(engine.active_span_count(), 2);

    engine.end_span(&child, true, &event_tx);
    assert_eq!(engine.active_span_count(), 1);

    engine.end_span(&parent, false, &event_tx);
    assert_eq!(engine.active_span_count(), 0);
}

#[test]
fn plugin_registry_crud() {
    let (event_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut registry = PluginRegistry::new(None, event_tx);

    assert_eq!(registry.count(), 0);

    let native = NativePlugin::new(
        "test-tool",
        PluginKind::Tool,
        "A test plugin",
        "0.1.0",
        "tester",
    );
    assert!(registry.register(Box::new(native)).is_ok());
    assert_eq!(registry.count(), 1);

    assert!(registry.enable("test-tool").is_ok());
    let desc = registry.get("test-tool").unwrap();
    assert_eq!(desc.status, PluginStatus::Enabled);
    assert_eq!(registry.enabled_count(), 1);

    assert!(registry.disable("test-tool").is_ok());
    let desc = registry.get("test-tool").unwrap();
    assert_eq!(desc.status, PluginStatus::Disabled);
    assert_eq!(registry.enabled_count(), 0);

    let result = registry.execute("test-tool", "hello");
    assert!(result.is_err(), "disabled plugin should reject execution");

    assert!(registry.enable("test-tool").is_ok());
    let result = registry.execute("test-tool", "hello");
    assert!(result.is_ok(), "enabled plugin should accept execution");

    let removed = registry.unregister("test-tool");
    assert!(removed.is_ok());
    assert_eq!(registry.count(), 0);
}

#[test]
fn plugin_registry_duplicate_rejected() {
    let (event_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut registry = PluginRegistry::new(None, event_tx);

    let native = NativePlugin::new("dup", PluginKind::Tool, "", "1.0", "op");
    assert!(registry.register(Box::new(native)).is_ok());

    let native2 = NativePlugin::new("dup", PluginKind::Integration, "", "2.0", "op");
    assert!(registry.register(Box::new(native2)).is_err());
}

#[test]
fn plugin_registry_missing_ops() {
    let (event_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut registry = PluginRegistry::new(None, event_tx);

    assert!(registry.enable("nonexistent").is_err());
    assert!(registry.disable("nonexistent").is_err());
    assert!(registry.unregister("nonexistent").is_err());
    assert!(registry.execute("nonexistent", "").is_err());
    assert!(registry.get("nonexistent").is_none());
}
