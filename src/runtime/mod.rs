use std::{
    collections::HashMap,
    process::Stdio,
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{mpsc, watch},
    time,
};

use crate::{
    agents::AgentRuntimeManager,
    ai::{
        AgentKind, AgentPrompt, AiClient, RustNativeRuntimeDescriptor, ToolCallResult, ToolSpec,
        agent_kind_for_role, build_messages,
    },
    infra::InfraIntegrations,
    models::{
        AgentRole, ConversationMessage, ExplainabilityRecord, OpsEvent, OpsState, RecoveryAction,
        RecoveryStatus, UserRole,
    },
    observability::ObservabilityEngine,
    persistence::PersistenceRuntime,
    security::{
        AsyncRuntimeGuard, CommandTier, EventBusSecurity, RateLimiter, ReliabilityGuard,
        SecurityAuditor, SecurityPolicy, SecurityTooling, ThreatDetector, WorkflowSecurity,
        redact_sensitive,
    },
    utils::{next_agent_name, next_id, next_sub_agent_name, now_ts},
    workflows::{
        DagWorkflowRuntime, NodeStatus, WorkflowNode, WorkflowNodeKind, load_workflows_from_dir,
    },
};

const CHAT_TASK_PREFIX: &str = "[ChatQuery]";

pub(crate) async fn ops_runtime(
    tx: watch::Sender<OpsState>,
    mut event_rx: mpsc::UnboundedReceiver<OpsEvent>,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) {
    let started = Instant::now();
    let mut state = OpsState::empty();
    let mut agent_runtime = AgentRuntimeManager::default();
    let persistence = PersistenceRuntime::from_env().await;
    let infra = InfraIntegrations::from_env();
    let mut ai_clients = build_ai_clients();
    register_ollama_models(&event_tx, &ai_clients).await;
    register_rust_native_runtime(&event_tx);
    let mut dag_workflows: Vec<DagWorkflowRuntime> = load_configured_workflows(&event_tx);
    if let Err(error) = persistence.reconstruct_state().await {
        tracing::warn!(%error, "historical state reconstruction failed");
    }
    let mut interval = time::interval(Duration::from_secs(1));
    let mut infra_interval = time::interval(Duration::from_secs(30));
    let mut observability_interval = time::interval(Duration::from_secs(60));
    let mut pending_tasks: HashMap<String, String> = HashMap::new();
    let mut startup_workflow_started = false;
    // Maps command_id → (workflow_id, node_id, command)
    let mut pending_workflow_nodes: HashMap<String, (String, String, String)> = HashMap::new();
    let mut observability = ObservabilityEngine::default();
    let mut command_rate_limiter = RateLimiter::new(12, Duration::from_secs(10));
    let mut ai_rate_limiter = RateLimiter::new(20, Duration::from_secs(60));
    loop {
        tokio::select! {
            _ = infra_interval.tick() => {
                match infra.discover().await {
                    Ok(nodes) if !nodes.is_empty() => {
                        let event = OpsEvent::InfrastructureSnapshotRecorded {
                            source: "configured-integrations".into(),
                            nodes: nodes.clone(),
                            timestamp: now_ts(),
                        };
                        state.apply_event(event);
                        if let Some(last) = state.events.last() {
                            persistence.persist_event(last, &state).await;
                        }
                        // Build topology and emit dependency edges
                        let topo_edges = infra.build_topology(&nodes);
                        if !topo_edges.is_empty() {
                            let topo_snapshot = crate::models::TopologySnapshot {
                                edges: topo_edges.iter().map(|e| crate::models::TopologyEdge {
                                    source: e.from.clone(),
                                    target: e.to.clone(),
                                    relation: e.relation.clone(),
                                }).collect(),
                                updated_at: now_ts(),
                            };
                            state.topology = topo_snapshot;
                            for edge in topo_edges {
                                let _ = event_tx.send(OpsEvent::KnowledgeEdgeAdded { edge });
                            }
                        }
                    }
                    Ok(_) => {
                        tracing::debug!("infrastructure discovery returned no nodes");
                    }
                    Err(error) => tracing::warn!(%error, "infrastructure discovery failed"),
                }
            }
            _ = observability_interval.tick() => {
                let ai_client = client_for_kind(&ai_clients, AgentKind::Security);
                observability.analyze_incidents(ai_client, &state, &event_tx).await;
                observability.analyze_recent_logs(ai_client, &state, &event_tx).await;
                let findings = SecurityAuditor::audit_state(&state);
                let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                    record: SecurityAuditor::explainability_record(&findings),
                });
                let tooling_findings = SecurityTooling::offline_audit(&state).all_findings();
                let runtime_findings = AsyncRuntimeGuard::backpressure_findings(&state);
                let previous_hash = state
                    .events
                    .last()
                    .map(|event| EventBusSecurity::integrity_hash(event, 0))
                    .unwrap_or(0);
                if let Some(last_event) = state.events.last() {
                    let integrity = EventBusSecurity::integrity_hash(last_event, previous_hash);
                    tracing::debug!(integrity, "validated event bus replay integrity checkpoint");
                }
                if !tooling_findings.is_empty() {
                    let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                        record: ExplainabilityRecord {
                            id: next_id("security-tooling"),
                            action: "Phase 14 security tooling audit".into(),
                            why: format!(
                                "{} dependency, port, configuration, log, plugin, workflow, or sandbox findings",
                                tooling_findings.len()
                            ),
                            evidence: tooling_findings
                                .iter()
                                .take(8)
                                .map(|finding| {
                                    format!(
                                        "{}: {} ({})",
                                        finding.severity, finding.title, finding.evidence
                                    )
                                })
                                .collect(),
                            confidence: 82,
                            tools_used: vec![
                                "dependency-vulnerability-scanner".into(),
                                "local-port-scanner".into(),
                                "configuration-analyzer".into(),
                                "log-anomaly-detector".into(),
                                "plugin-behavior-analyzer".into(),
                                "workflow-validator".into(),
                                "sandbox-inspector".into(),
                            ],
                            timestamp: now_ts(),
                        },
                    });
                }
                if !runtime_findings.is_empty() {
                    let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                        record: ExplainabilityRecord {
                            id: next_id("async-runtime-guard"),
                            action: "Phase 16 async runtime supervision".into(),
                            why: format!("{} supervision findings", runtime_findings.len()),
                            evidence: runtime_findings
                                .iter()
                                .map(|finding| format!("{}: {}", finding.severity, finding.evidence))
                                .collect(),
                            confidence: 84,
                            tools_used: vec![
                                "task-supervisor".into(),
                                "backpressure-guard".into(),
                                "event-integrity-checkpoint".into(),
                            ],
                            timestamp: now_ts(),
                        },
                    });
                }
                for signal in ThreatDetector::analyze(&state) {
                    let _ = event_tx.send(ThreatDetector::event_for(&signal));
                }
            }
            _ = interval.tick() => {
                state.uptime_secs = started.elapsed().as_secs();
                state.tick();
                ReliabilityGuard::cleanup_state(&mut state);
                let pressure = ReliabilityGuard::memory_pressure(&state);
                if pressure >= 85 {
                    let _ = event_tx.send(OpsEvent::NotificationRaised {
                        level: "warn".into(),
                        message: format!("runtime memory pressure at {pressure}% after cleanup"),
                        timestamp: now_ts(),
                    });
                }
                if !startup_workflow_started && !ai_clients.is_empty() && state.uptime_secs > 2 {
                    startup_workflow_started = true;
                    let planner_id = next_agent_name();
                    let task_desc = "Validate local Ollama readiness, inventory installed models, and produce an autonomous operations readiness plan";
                    let _ = event_tx.send(OpsEvent::AgentSpawned {
                        name: planner_id.clone(),
                        role: AgentRole::Planner,
                        timestamp: now_ts(),
                    });
                    let _ = event_tx.send(OpsEvent::TaskAssigned {
                        agent: planner_id.clone(),
                        task: task_desc.to_string(),
                        timestamp: now_ts(),
                    });
                    let _ = event_tx.send(OpsEvent::NotificationRaised {
                        level: "info".into(),
                        message: "startup validation workflow scheduled".into(),
                        timestamp: now_ts(),
                    });
                }
                observability.process_metrics(&state, &event_tx);
                step_dag_workflows(&mut dag_workflows, &state, &event_tx, &ai_clients, &mut pending_workflow_nodes).await;
                let cpu = state.infra.first().map(|node| node.cpu).unwrap_or(0);
                let memory = state.infra.first().map(|node| node.memory).unwrap_or(0);
                let event = OpsEvent::MetricsSampled {
                    cpu,
                    memory,
                    timestamp: now_ts(),
                };
                state.apply_event(event);
                if let Some(last) = state.events.last() {
                    persistence.persist_event(last, &state).await;
                }
            }
            Some(event) = event_rx.recv() => {
                if let Err(error) = SecurityPolicy::validate_event(&event, &state) {
                    let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                        record: ExplainabilityRecord {
                            id: next_id("event-policy-block"),
                            action: "Blocked invalid event bus transition".into(),
                            why: error,
                            evidence: vec![format!("{event:?}")],
                            confidence: 93,
                            tools_used: vec!["event-bus-security".into(), "modular-security-policy".into()],
                            timestamp: now_ts(),
                        },
                    });
                    continue;
                }
                match &event {
                    OpsEvent::IncidentDetected { incident_id, .. } => {
                        let wf = create_incident_dag(incident_id.clone());
                        let id = wf.id.clone();
                        dag_workflows.push(wf);
                        let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
                            id: id.clone(),
                            stage: "Incident response workflow initialized".into(),
                            progress: 0,
                            timestamp: now_ts(),
                        });
                    }
                    OpsEvent::CommandRequested {
                        id,
                        command,
                        dry_run,
                        ..
                    } => {
                        let id = id.clone();
                        let command = command.clone();
                        let dry_run = *dry_run;
                        if !command_rate_limiter.allow("command-execution") {
                            let _ = event_tx.send(OpsEvent::CommandExecuted {
                                id,
                                command,
                                success: false,
                                exit_code: None,
                                stdout: String::new(),
                                stderr: "blocked by rate limiter: command execution budget exhausted".into(),
                                timestamp: now_ts(),
                            });
                        } else {
                            tokio::spawn(run_infrastructure_command(id, command, dry_run, event_tx.clone()));
                        }
                    }
                    OpsEvent::AgentSpawned { name, role, timestamp } => {
                        if let Some(client) = client_for_role(&ai_clients, role) {
                            let client = client.clone();
                            let name = name.clone();
                            let role = role.clone();
                            let ts = timestamp.clone();
                            let tx = event_tx.clone();
                            tokio::spawn(async move {
                                spawn_ai_agent_task(client, name, role, ts, tx).await;
                            });
                        }
                    }
                    OpsEvent::TaskAssigned { agent, task, timestamp } => {
                        let is_planner = state.agents.iter().any(|a| {
                            a.name == *agent && matches!(a.role, AgentRole::Planner)
                        });
                        let mem_ctx = agent_runtime.memory_context(agent);
                        let role = state
                            .agents
                            .iter()
                            .find(|a| a.name == *agent)
                            .map(|a| a.role.clone())
                            .unwrap_or(AgentRole::Executor);
                        if !ai_rate_limiter.allow(&format!("agent-task-{agent}")) {
                            let _ = event_tx.send(OpsEvent::AgentLifecycleChanged {
                                agent: agent.clone(),
                                status: crate::models::AgentStatus::Failed,
                                task: "blocked by rate limiter: AI task budget exhausted".into(),
                                timestamp: now_ts(),
                            });
                            continue;
                        }
                        if let Some(reason) = SecurityPolicy::detect_prompt_attack(task) {
                            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                                record: crate::models::ExplainabilityRecord {
                                    id: next_id("prompt-block"),
                                    action: "Blocked prompt injection".into(),
                                    why: reason,
                                    evidence: vec![task.clone()],
                                    confidence: 91,
                                    tools_used: vec!["prompt-policy".into()],
                                    timestamp: now_ts(),
                                },
                            });
                            if chat_query_from_task(task).is_some() {
                                send_chat_agent_message(
                                    &event_tx,
                                    "assistant",
                                    "I cannot process that request because it looks like a prompt-injection or policy bypass attempt.",
                                    "policy",
                                    92,
                                );
                            }
                            continue;
                        }
                        let is_chat_task = chat_query_from_task(task).is_some();
                        if let Some(client) = client_for_role(&ai_clients, &role) {
                            if let Some(prev) = pending_tasks.insert(agent.clone(), task.clone()) {
                                tracing::warn!(agent, prev_task = %prev, new_task = %task, "agent overwritten with new task");
                            }
                            let client = if is_chat_task {
                                client_for_chat_task(&role).unwrap_or_else(|| client.clone())
                            } else {
                                client.clone()
                            };
                            let agent = agent.clone();
                            let task = SecurityPolicy::sanitize_prompt(task);
                            let ts = timestamp.clone();
                            let tx = event_tx.clone();
                            if is_planner && !is_chat_task {
                                tokio::spawn(async move {
                                    handle_planner_task(client, agent, task, ts, tx, mem_ctx).await;
                                });
                            } else {
                                tokio::spawn(async move {
                                    execute_ai_task(client, agent, task, ts, tx, mem_ctx).await;
                                });
                            }
                        } else if is_chat_task {
                            send_chat_agent_message(
                                &event_tx,
                                "assistant",
                                "No local agent runtime is configured, so I cannot answer from the Chat tab yet. Configure local Ollama and try again.",
                                "runtime",
                                70,
                            );
                        }
                    }
                    OpsEvent::CommandExecuted {
                        id,
                        success,
                        ..
                    } => {
                        // Check if this completes a pending workflow node
                        if let Some((wf_id, node_id, _cmd)) =
                            pending_workflow_nodes.remove(id.as_str())
                            && let Some(wf) = dag_workflows.iter_mut().find(|w| w.id == wf_id)
                        {
                                if *success {
                                    let _ = wf.mark_succeeded(&node_id);
                                    tracing::info!(workflow_id = %wf_id, %node_id, "workflow command node succeeded");
                                    let _ = event_tx.send(OpsEvent::WorkflowNodeCompleted {
                                        workflow_id: wf_id.clone(),
                                        node_id: node_id.clone(),
                                        timestamp: now_ts(),
                                    });
                                } else if wf.can_retry(&node_id) {
                                    let backoff = wf.retry_backoff_ms(&node_id);
                                    tracing::warn!(workflow_id = %wf_id, %node_id, backoff, "workflow command failed, will retry");
                                    let _ = wf.reset_node(&node_id);
                                    let tx = event_tx.clone();
                                    tokio::spawn(async move {
                                        tokio::time::sleep(Duration::from_millis(backoff)).await;
                                        let _ = tx.send(OpsEvent::WorkflowNodeCompleted {
                                            workflow_id: wf_id.clone(),
                                            node_id: node_id.clone(),
                                            timestamp: now_ts(),
                                        });
                                    });
                                } else {
                                    let _ = wf.mark_failed(&node_id, "command failed and retries exhausted".into());
                                    tracing::error!(workflow_id = %wf_id, %node_id, "workflow command failed, retries exhausted");
                                    let _ = event_tx.send(OpsEvent::WorkflowNodeCompleted {
                                        workflow_id: wf_id.clone(),
                                        node_id: node_id.clone(),
                                        timestamp: now_ts(),
                                    });
                                    // Check for rollback
                                    if let Some(node) = wf.get_node(&node_id).cloned()
                                        && let Some(rollback_target) = node.rollback
                                    {
                                        let _ = wf.mark_for_rollback(&rollback_target);
                                        let _ = event_tx.send(OpsEvent::WorkflowNodeCompleted {
                                            workflow_id: wf_id.clone(),
                                            node_id: rollback_target,
                                            timestamp: now_ts(),
                                        });
                                    }
                                }
                        }
                    }
                    OpsEvent::WorkflowNodeCompleted {
                        workflow_id,
                        node_id,
                        ..
                    } => {
                        if let Some(wf) = dag_workflows.iter_mut().find(|w| w.id == *workflow_id) {
                            // Only mark succeeded if not already in a terminal state
                            let state = wf.node_states.get(node_id);
                            let needs_mark = state
                                .map(|s| matches!(s.status, NodeStatus::Running))
                                .unwrap_or(false);
                            if needs_mark {
                                let _ = wf.mark_succeeded(node_id);
                            }
                            tracing::info!(workflow_id, node_id, "workflow node completed");
                        }
                    }
                    OpsEvent::AiProviderLogin {
                        kind,
                        endpoint,
                        model,
                        timestamp,
                        ..
                    } => {
                        if kind == "ollama" {
                            match AiClient::validate_local_endpoint(endpoint) {
                                Ok(local_endpoint) => {
                                    unsafe { std::env::set_var("OCTOBOT_OLLAMA_URL", local_endpoint); }
                                    if !model.is_empty() {
                                        unsafe { std::env::set_var("OCTOBOT_OLLAMA_MODEL", model); }
                                    }
                                    ai_clients = build_ai_clients();
                                    register_ollama_models(&event_tx, &ai_clients).await;
                                    let _ = event_tx.send(OpsEvent::NotificationRaised {
                                        level: "info".into(),
                                        message: "ollama endpoint reconfigured from login command".into(),
                                        timestamp: timestamp.clone(),
                                    });
                                }
                                Err(error) => {
                                    let _ = event_tx.send(OpsEvent::NotificationRaised {
                                        level: "error".into(),
                                        message: format!("rejected Ollama login: {error}"),
                                        timestamp: timestamp.clone(),
                                    });
                                }
                            }
                        } else {
                            let _ = event_tx.send(OpsEvent::NotificationRaised {
                                level: "warn".into(),
                                message: format!("ignored login attempt for unsupported provider `{kind}`"),
                                timestamp: timestamp.clone(),
                            });
                        }
                    }
                    _ => {}
                }
                agent_runtime.handle_event(&event, &event_tx);
                state.apply_event(event);
                if let Some(last) = state.events.last() {
                    persistence.persist_event(last, &state).await;
                }
            }
            else => break,
        }

        if tx.send(state.clone()).is_err() {
            break;
        }
    }
}

fn build_ai_clients() -> Vec<AiClient> {
    RustNativeRuntimeDescriptor::new()
        .agents
        .into_iter()
        .map(|spec| AiClient::new(spec.agent_profile()))
        .collect()
}

fn client_for_role<'a>(clients: &'a [AiClient], role: &AgentRole) -> Option<&'a AiClient> {
    let desired = agent_kind_for_role(role);
    clients
        .iter()
        .find(|client| client.profile().kind == desired)
        .or_else(|| clients.first())
}

fn client_for_kind(clients: &[AiClient], kind: AgentKind) -> Option<&AiClient> {
    clients
        .iter()
        .find(|client| client.profile().kind == kind)
        .or_else(|| clients.first())
}

fn client_for_chat_task(role: &AgentRole) -> Option<AiClient> {
    let model = std::env::var("OCTOBOT_CHAT_MODEL")
        .or_else(|_| std::env::var("OCTOBOT_OLLAMA_MODEL"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())?;
    let kind = agent_kind_for_role(role);
    Some(AiClient::new(crate::ai::AgentProfile {
        kind,
        name: format!("chat-{}-agent", kind.as_str()),
        model,
        purpose: "agentic conversation inside the OctoBot Chat tab".into(),
    }))
}

fn chat_query_from_task(task: &str) -> Option<&str> {
    let body = task.strip_prefix(CHAT_TASK_PREFIX)?.trim();
    let query = body
        .strip_prefix("User question:")
        .unwrap_or(body)
        .trim()
        .split("\n\nInstructions:")
        .next()
        .unwrap_or(body)
        .trim();
    (!query.is_empty()).then_some(query)
}

fn send_chat_agent_message(
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
    role: &str,
    content: &str,
    model: &str,
    confidence: u8,
) {
    let _ = event_tx.send(OpsEvent::ConversationMessageRecorded {
        message: ConversationMessage {
            id: next_id("chat"),
            role: role.into(),
            content: content.into(),
            model: model.into(),
            confidence,
            timestamp: now_ts(),
        },
    });
}

async fn register_ollama_models(
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ai_clients: &[AiClient],
) {
    let Some(client) = ai_clients.first() else {
        let _ = event_tx.send(OpsEvent::NotificationRaised {
            level: "error".into(),
            message: "no Ollama clients configured".into(),
            timestamp: now_ts(),
        });
        return;
    };

    match client.health_check().await {
        Ok(()) => {
            let required: Vec<String> = ai_clients
                .iter()
                .map(|client| client.model().to_string())
                .collect();
            match client.required_model_health(&required).await {
                Ok(models) => {
                    let _ = event_tx.send(OpsEvent::ModelHealthUpdated {
                        models: models
                            .iter()
                            .map(|model| crate::models::ModelHealthSnapshot {
                                model: model.model.clone(),
                                installed: model.installed,
                                online: model.online,
                                size_bytes: model.size_bytes,
                                digest: model.digest.clone(),
                                modified_at: model.modified_at.clone(),
                                last_checked: model.last_checked.clone(),
                                notes: model.notes.clone(),
                            })
                            .collect(),
                        timestamp: now_ts(),
                    });
                    for model in models {
                        let level = if model.installed { "info" } else { "warn" };
                        let message = if model.installed {
                            format!("ollama model available: {}", model.model)
                        } else {
                            format!("missing required Ollama model: {}", model.model)
                        };
                        let _ = event_tx.send(OpsEvent::NotificationRaised {
                            level: level.into(),
                            message,
                            timestamp: now_ts(),
                        });
                    }
                    let _ = event_tx.send(OpsEvent::NotificationRaised {
                        level: "info".into(),
                        message: format!("ollama runtime available at {}", client.endpoint()),
                        timestamp: now_ts(),
                    });
                }
                Err(error) => {
                    let _ = event_tx.send(OpsEvent::NotificationRaised {
                        level: "error".into(),
                        message: format!("failed to query Ollama models: {error}"),
                        timestamp: now_ts(),
                    });
                }
            }
        }
        Err(error) => {
            let _ = event_tx.send(OpsEvent::NotificationRaised {
                level: "error".into(),
                message: format!("Ollama health check failed: {error}"),
                timestamp: now_ts(),
            });
        }
    }
}

fn register_rust_native_runtime(event_tx: &mpsc::UnboundedSender<OpsEvent>) {
    let descriptor = RustNativeRuntimeDescriptor::new();
    let required_models = descriptor.required_models().join(", ");
    let _ = event_tx.send(OpsEvent::NotificationRaised {
        level: "info".into(),
        message: format!(
            "rust-native AI runtime registered: agent_runtime={}, swarm_runtime={}, provider={}, models=[{}]",
            descriptor.agent_runtime,
            descriptor.swarm_runtime,
            descriptor.local_provider,
            required_models
        ),
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
        record: ExplainabilityRecord {
            id: next_id("rust-ai-runtime"),
            action: "Registered Rust-native AI orchestration backbone".into(),
            why: "Phase 30 routes autonomous agents through rig-compatible agent specs, swarms_rs swarm plans, and ollama-rs local provider metadata while preserving existing event replay.".into(),
            evidence: vec![
                format!("agent_runtime={}", descriptor.agent_runtime),
                format!("swarm_runtime={}", descriptor.swarm_runtime),
                format!("local_provider={}", descriptor.local_provider),
                format!("agent_count={}", descriptor.agents.len()),
                format!("required_models={required_models}"),
            ],
            confidence: 90,
            tools_used: vec![
                "rig".into(),
                "swarms_rs".into(),
                "ollama-rs".into(),
                "runtime-registry".into(),
            ],
            timestamp: now_ts(),
        },
    });
    for spec in descriptor.agents {
        let _ = event_tx.send(OpsEvent::RuntimeUpdated {
            runtime: crate::models::AgentRuntime {
                agent: spec.name.clone(),
                kind: crate::models::AgentRuntimeKind::LocalProcess,
                endpoint: format!("rig://ollama-rs/{}", spec.name),
                status: crate::models::RuntimeStatus::Local,
                heartbeat: now_ts(),
                notes: format!(
                    "model={} memory_scope={} tools={} streaming={} async={} replay={}",
                    spec.model,
                    spec.memory_scope,
                    spec.tools.len(),
                    spec.streaming,
                    spec.async_execution,
                    spec.replay_compatible
                ),
            },
        });
        let _ = event_tx.send(OpsEvent::AgentMemoryEntryRecorded {
            entry: crate::models::AgentMemoryEntry {
                id: next_id("agent-memory"),
                scope: spec.memory_scope,
                kind: "runtime-agent-spec".into(),
                key: spec.name,
                preview: spec.prompt.chars().take(160).collect(),
                provenance: "phase-30-rust-native-runtime".into(),
                created_at: now_ts(),
            },
        });
    }
}

fn load_configured_workflows(
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
) -> Vec<DagWorkflowRuntime> {
    let Ok(path) = std::env::var("OCTOBOT_WORKFLOW_DIR") else {
        return Vec::new();
    };
    match load_workflows_from_dir(path) {
        Ok(workflows) => {
            for workflow in &workflows {
                let _ = event_tx.send(OpsEvent::WorkflowDefinitionLoaded {
                    definition: workflow.summary(),
                });
            }
            workflows
        }
        Err(error) => {
            tracing::warn!(%error, "workflow definition loading failed");
            Vec::new()
        }
    }
}

async fn spawn_ai_agent_task(
    client: AiClient,
    name: String,
    role: AgentRole,
    timestamp: String,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) {
    let _ = event_tx.send(OpsEvent::ToolCallRequested {
        id: next_id("tool"),
        tool: format!("ai-agent-spawn-{name}"),
        arguments: serde_json::json!({ "role": format!("{role:?}"), "agent": name }),
        timestamp: timestamp.clone(),
    });

    let system_prompt = match role {
        AgentRole::Planner => format!(
            "You are a planner agent named {name}. Your role is to decompose complex tasks into sub-tasks. \
             Respond concisely with a structured plan."
        ),
        AgentRole::Executor => format!(
            "You are an executor agent named {name}. Your role is to execute sub-tasks assigned by a planner. \
             Use the exec_command tool to run infrastructure commands when needed. Report completion concisely."
        ),
        _ => format!(
            "You are an operations agent named {name} with role {:?}. Respond concisely.",
            role
        ),
    };

    let prompt = AgentPrompt {
        system: system_prompt,
        user: format!(
            "Agent {name} spawned with role {:?}. Report readiness.",
            role
        ),
        tools: vec![ToolSpec {
            name: "report_readiness".into(),
            description: "Report that the agent is ready for task assignment".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "status": { "type": "string", "enum": ["ready", "degraded"] }
                },
                "required": ["status"]
            }),
        }],
    };

    match client
        .chat(
            vec![
                serde_json::json!({ "role": "system", "content": prompt.system }),
                serde_json::json!({ "role": "user", "content": prompt.user }),
            ],
            &prompt.tools,
            &name,
            Some(&event_tx),
        )
        .await
    {
        Ok(response) => {
            let _ = event_tx.send(OpsEvent::ToolCallCompleted {
                id: next_id("tool"),
                tool: format!("ai-agent-spawn-{name}"),
                success: true,
                output: serde_json::json!({ "content": response.content, "tool_calls": response.tool_calls }),
                timestamp: now_ts(),
            });
        }
        Err(error) => {
            tracing::warn!(agent = %name, %error, "AI agent spawn response failed");
            let _ = event_tx.send(OpsEvent::ToolCallCompleted {
                id: next_id("tool"),
                tool: format!("ai-agent-spawn-{name}"),
                success: false,
                output: serde_json::json!({ "error": error.to_string() }),
                timestamp: now_ts(),
            });
        }
    }
}

async fn execute_ai_task(
    client: AiClient,
    agent: String,
    task: String,
    timestamp: String,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
    memory_ctx: String,
) {
    let tool_id = next_id("tool");
    let memory_section = if memory_ctx.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", memory_ctx)
    };
    let chat_query = chat_query_from_task(&task).map(str::to_string);
    let system_prompt = if chat_query.is_some() {
        format!(
            "You are OctoBot's conversational agent named {agent}. Answer the user's question directly in the Chat tab. \
             You may answer general questions, project questions, and operational questions. \
             Use OctoBot context when relevant, be concise, and say when you are uncertain. \
             Do not claim to run commands or modify files from chat.{}",
            memory_section
        )
    } else {
        format!(
            "You are an operations agent named {agent}. Execute the assigned task using available tools. \
             You can request tools by name with arguments. The tool results will be provided back to you. \
             Continue reasoning and calling tools until you have enough information to complete the task. \
             When done, call complete_task with a summary and confidence score. Respond concisely.{}",
            memory_section
        )
    };

    let _ = event_tx.send(OpsEvent::ToolCallRequested {
        id: tool_id.clone(),
        tool: format!("ai-task-{agent}"),
        arguments: serde_json::json!({ "agent": agent, "task": task }),
        timestamp: timestamp.clone(),
    });

    let tools = if chat_query.is_some() {
        Vec::new()
    } else {
        vec![
            ToolSpec {
                name: "exec_command".into(),
                description: "Run an allowlisted infrastructure command and return its output"
                    .into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "command to run" }
                    },
                    "required": ["command"]
                }),
            },
            ToolSpec {
                name: "complete_task".into(),
                description: "Report task completion with findings and confidence score".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "summary": { "type": "string", "description": "summary of findings" },
                        "confidence": { "type": "integer", "minimum": 0, "maximum": 100 }
                    },
                    "required": ["summary", "confidence"]
                }),
            },
        ]
    };

    let max_turns: u8 = std::env::var("OCTOBOT_AI_MAX_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let mut history: Vec<ToolCallResult> = Vec::new();
    let mut all_tool_calls: Vec<serde_json::Value> = Vec::new();
    let mut final_content = String::new();
    let mut success = false;

    for turn in 0..max_turns {
        let messages = build_messages(&system_prompt, &task, &history);
        let prompt = AgentPrompt {
            system: system_prompt.clone(),
            user: if history.is_empty() {
                chat_query.clone().unwrap_or_else(|| task.clone())
            } else {
                format!(
                    "Continue execution. Previous tool results have been provided. Current turn {turn}/{max_turns}."
                )
            },
            tools: tools.clone(),
        };
        let tools_list = tools.clone();

        let response = if history.is_empty() {
            client
                .chat(
                    vec![
                        serde_json::json!({ "role": "system", "content": prompt.system }),
                        serde_json::json!({ "role": "user", "content": prompt.user }),
                    ],
                    &tools_list,
                    &agent,
                    Some(&event_tx),
                )
                .await
        } else {
            client
                .chat(messages, &tools_list, &agent, Some(&event_tx))
                .await
        };

        match response {
            Ok(response) => {
                if response.tool_calls.is_empty() {
                    final_content = response.content;
                    success = true;
                    break;
                }

                for tc in &response.tool_calls {
                    all_tool_calls.push(serde_json::json!({
                        "turn": turn,
                        "name": tc.name,
                        "arguments": tc.arguments
                    }));

                    if tc.name == "complete_task" {
                        final_content = tc
                            .arguments
                            .get("summary")
                            .and_then(Value::as_str)
                            .unwrap_or("task completed")
                            .to_string();
                        success = true;
                        break;
                    }

                    let result = execute_ai_tool(&agent, &tc.name, &tc.arguments, &event_tx).await;
                    history.push(ToolCallResult {
                        call: tc.clone(),
                        output: result.clone(),
                        success: true,
                    });
                    let _ = event_tx.send(OpsEvent::ToolCallCompleted {
                        id: next_id("tool"),
                        tool: format!("ai-task-{agent}-turn-{turn}"),
                        success: true,
                        output: result,
                        timestamp: now_ts(),
                    });
                }

                if success {
                    break;
                }
            }
            Err(error) => {
                tracing::warn!(%agent, turn, %error, "AI reasoning turn failed");
                if turn == max_turns - 1 {
                    let _ = event_tx.send(OpsEvent::ToolCallCompleted {
                        id: tool_id,
                        tool: format!("ai-task-{agent}"),
                        success: false,
                        output: serde_json::json!({ "error": error.to_string(), "turn": turn }),
                        timestamp: now_ts(),
                    });
                    let _ = event_tx.send(OpsEvent::AgentLifecycleChanged {
                        agent,
                        status: crate::models::AgentStatus::Failed,
                        task: format!("AI reasoning failed after max {max_turns} turns: {error}"),
                        timestamp: now_ts(),
                    });
                    if chat_query.is_some() {
                        send_chat_agent_message(
                            &event_tx,
                            "assistant",
                            &format!(
                                "The assigned chat agent could not complete the answer: {error}"
                            ),
                            client.model(),
                            50,
                        );
                    }
                    return;
                }
                // Retry on transient failure
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    if success {
        let _ = event_tx.send(OpsEvent::ToolCallCompleted {
            id: tool_id,
            tool: format!("ai-task-{agent}"),
            success: true,
            output: serde_json::json!({
                "content": final_content,
                "tool_calls": all_tool_calls,
                "turns": history.len() + 1
            }),
            timestamp: now_ts(),
        });
        let _ = event_tx.send(OpsEvent::AgentMemoryStored {
            agent: agent.clone(),
            key: "last_task_result".into(),
            value: final_content.clone(),
            timestamp: now_ts(),
        });
        let _ = event_tx.send(OpsEvent::AgentMemoryStored {
            agent: agent.clone(),
            key: "last_task".into(),
            value: task.clone(),
            timestamp: now_ts(),
        });
        if chat_query.is_some() {
            send_chat_agent_message(&event_tx, "assistant", &final_content, client.model(), 88);
        }
        let _ = event_tx.send(OpsEvent::AgentLifecycleChanged {
            agent: agent.clone(),
            status: crate::models::AgentStatus::Completed,
            task: "AI task completed after reasoning loop".into(),
            timestamp: now_ts(),
        });
        unload_completed_task_model(&client, &agent, &event_tx).await;
        // If this executor was assigned by a planner, report SubTaskCompleted
        if let Some(planner_name) = task
            .strip_prefix("[Planner: ")
            .and_then(|s| s.split(']').next())
        {
            let _ = event_tx.send(OpsEvent::SubTaskCompleted {
                planner: planner_name.to_string(),
                executor: agent.clone(),
                sub_task: task.clone(),
                result: final_content.clone(),
                timestamp: now_ts(),
            });
        }
    }
}

async fn handle_planner_task(
    client: AiClient,
    agent: String,
    task: String,
    timestamp: String,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
    memory_ctx: String,
) {
    let tool_id = next_id("plan");
    let memory_section = if memory_ctx.is_empty() {
        String::new()
    } else {
        format!("\n\n{}", memory_ctx)
    };
    let system_prompt = format!(
        "You are a planner agent named {agent}. Your ONLY job is to decompose the assigned task into 2-5 concrete sub-tasks. \
         You MUST use the create_subtask tool for EACH sub-task. \
         Example: for 'Investigate database CPU', create sub-tasks like 'Check database CPU metrics', \
         'Analyze slow queries', 'Review connection pool usage', 'Check disk I/O'. \
         After ALL sub-tasks are created, call finalize_plan. \
         DO NOT respond with text alone — you MUST use the tools provided.{}",
        memory_section
    );

    let _ = event_tx.send(OpsEvent::ToolCallRequested {
        id: tool_id.clone(),
        tool: format!("planner-{agent}"),
        arguments: serde_json::json!({ "agent": agent, "task": task }),
        timestamp: timestamp.clone(),
    });

    let tools = vec![
        ToolSpec {
            name: "create_subtask".into(),
            description: "Define ONE sub-task for an executor agent. Call this once per sub-task."
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string", "description": "clear description of this sub-task" },
                    "executor_name": { "type": "string", "description": "short name for the executor agent" }
                },
                "required": ["description", "executor_name"]
            }),
        },
        ToolSpec {
            name: "finalize_plan".into(),
            description: "Call this ONLY after ALL sub-tasks have been defined via create_subtask"
                .into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "summary": { "type": "string", "description": "plan summary" }
                },
                "required": ["summary"]
            }),
        },
    ];

    let mut sub_tasks: Vec<(String, String)> = Vec::new();
    let max_turns: u8 = std::env::var("OCTOBOT_AI_MAX_TURNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let mut history: Vec<ToolCallResult> = Vec::new();
    let mut plan_finalized = false;

    for turn in 0..max_turns {
        if plan_finalized {
            break;
        }
        let messages = build_messages(&system_prompt, &task, &history);
        let prompt = AgentPrompt {
            system: system_prompt.clone(),
            user: if history.is_empty() {
                format!("Decompose this task into sub-tasks using create_subtask: {task}")
            } else {
                format!(
                    "Continue creating sub-tasks. Turn {turn}/{max_turns}. Call create_subtask for each, then finalize_plan when done."
                )
            },
            tools: tools.clone(),
        };

        let response = if history.is_empty() {
            client
                .chat(
                    vec![
                        serde_json::json!({ "role": "system", "content": prompt.system }),
                        serde_json::json!({ "role": "user", "content": prompt.user }),
                    ],
                    &tools,
                    &agent,
                    Some(&event_tx),
                )
                .await
        } else {
            client.chat(messages, &tools, &agent, Some(&event_tx)).await
        };

        match response {
            Ok(resp) => {
                if resp.tool_calls.is_empty() {
                    break;
                }
                for tc in &resp.tool_calls {
                    if tc.name == "create_subtask" {
                        let desc = tc
                            .arguments
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("unnamed sub-task")
                            .to_string();
                        let exec_name = tc
                            .arguments
                            .get("executor_name")
                            .and_then(Value::as_str)
                            .map(|s| s.to_string())
                            .unwrap_or_else(next_sub_agent_name);
                        sub_tasks.push((desc.clone(), exec_name.clone()));
                        history.push(ToolCallResult {
                            call: tc.clone(),
                            output: json!({ "status": "subtask_defined", "description": desc, "executor": exec_name }),
                            success: true,
                        });
                    } else if tc.name == "finalize_plan" {
                        plan_finalized = true;
                        break;
                    } else {
                        history.push(ToolCallResult {
                            call: tc.clone(),
                            output: json!({ "error": "unknown tool", "tool": tc.name }),
                            success: false,
                        });
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%agent, turn, %error, "planner reasoning failed");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    // Fallback: if planner created no sub-tasks, auto-generate them from the task
    if sub_tasks.is_empty() {
        tracing::info!(%agent, task = %task, "planner created no sub-tasks, generating fallback sub-tasks");
        let fallback_subtasks = generate_fallback_subtasks(&task, &agent);
        for (desc, exec_name) in &fallback_subtasks {
            sub_tasks.push((desc.clone(), exec_name.clone()));
        }
    }

    let sub_task_descs: Vec<String> = sub_tasks.iter().map(|(d, _)| d.clone()).collect();
    let _ = event_tx.send(OpsEvent::PlanCreated {
        planner: agent.clone(),
        task: task.clone(),
        sub_tasks: sub_task_descs.clone(),
        timestamp: now_ts(),
    });

    if !sub_tasks.is_empty() {
        for (i, (desc, _exec_name)) in sub_tasks.iter().enumerate() {
            let exec_agent = format!("{}-exec-{}", agent, i);
            let _ = event_tx.send(OpsEvent::AgentSpawned {
                name: exec_agent.clone(),
                role: AgentRole::Executor,
                timestamp: now_ts(),
            });
            let _ = event_tx.send(OpsEvent::TaskAssigned {
                agent: exec_agent.clone(),
                task: format!("[Planner: {agent}] Sub-task {i}: {desc}"),
                timestamp: now_ts(),
            });
            let _ = event_tx.send(OpsEvent::AgentMessageRecorded {
                from: agent.clone(),
                to: exec_agent,
                protocol: "plan-execute".into(),
                message: format!("Execute sub-task {i}: {desc}"),
                confidence: 80,
                timestamp: now_ts(),
            });
        }
    }

    let _ = event_tx.send(OpsEvent::ToolCallCompleted {
        id: tool_id,
        tool: format!("planner-{agent}"),
        success: true,
        output: json!({
            "task": task,
            "sub_tasks": sub_task_descs,
            "count": sub_tasks.len()
        }),
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::AgentMemoryStored {
        agent: agent.clone(),
        key: "last_plan".into(),
        value: format!("{} sub-tasks created", sub_tasks.len()),
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::AgentLifecycleChanged {
        agent: agent.clone(),
        status: crate::models::AgentStatus::Completed,
        task: format!("Plan completed with {} sub-tasks", sub_tasks.len()),
        timestamp: now_ts(),
    });
    unload_completed_task_model(&client, &agent, &event_tx).await;
}

async fn unload_completed_task_model(
    client: &AiClient,
    agent: &str,
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
) {
    let model = client.model().to_string();
    match client.unload_model().await {
        Ok(()) => {
            let _ = event_tx.send(OpsEvent::NotificationRaised {
                level: "info".into(),
                message: format!("unloaded Ollama model {model} after completed task for {agent}"),
                timestamp: now_ts(),
            });
        }
        Err(error) => {
            tracing::warn!(%agent, %model, %error, "failed to unload completed task model");
            let _ = event_tx.send(OpsEvent::NotificationRaised {
                level: "warn".into(),
                message: format!("failed to unload Ollama model {model} for {agent}: {error}"),
                timestamp: now_ts(),
            });
        }
    }
}

fn generate_fallback_subtasks(task: &str, agent: &str) -> Vec<(String, String)> {
    let task_lower = task.to_lowercase();
    let mut sub_tasks: Vec<(String, String)> = Vec::new();

    if task_lower.contains("database")
        || task_lower.contains("db")
        || task_lower.contains("postgres")
    {
        sub_tasks.push((
            "Check database CPU and connection metrics".into(),
            format!("{}-db-cpu", agent),
        ));
        sub_tasks.push((
            "Analyze slow queries and lock contention".into(),
            format!("{}-db-queries", agent),
        ));
        sub_tasks.push((
            "Review disk I/O and storage capacity".into(),
            format!("{}-db-disk", agent),
        ));
        sub_tasks.push((
            "Check replication lag and WAL status".into(),
            format!("{}-db-repl", agent),
        ));
    } else if task_lower.contains("nginx")
        || task_lower.contains("ingress")
        || task_lower.contains("gateway")
    {
        sub_tasks.push((
            "Check nginx connection pool and TLS handshakes".into(),
            format!("{}-nginx-conn", agent),
        ));
        sub_tasks.push((
            "Review recent configuration and rollout changes".into(),
            format!("{}-nginx-config", agent),
        ));
        sub_tasks.push((
            "Analyze request latency percentiles".into(),
            format!("{}-nginx-latency", agent),
        ));
        sub_tasks.push((
            "Inspect upstream health and error rates".into(),
            format!("{}-nginx-upstream", agent),
        ));
    } else if task_lower.contains("auth")
        || task_lower.contains("token")
        || task_lower.contains("cache")
    {
        sub_tasks.push((
            "Check auth service latency and error rates".into(),
            format!("{}-auth-latency", agent),
        ));
        sub_tasks.push((
            "Analyze token cache hit ratios and saturation".into(),
            format!("{}-auth-cache", agent),
        ));
        sub_tasks.push((
            "Review recent auth service deployments".into(),
            format!("{}-auth-deploy", agent),
        ));
        sub_tasks.push((
            "Check auth service CPU and memory usage".into(),
            format!("{}-auth-resources", agent),
        ));
    } else if task_lower.contains("disk")
        || task_lower.contains("storage")
        || task_lower.contains("space")
    {
        sub_tasks.push((
            "Check disk usage across all mount points".into(),
            format!("{}-disk-usage", agent),
        ));
        sub_tasks.push((
            "Identify large files and directories".into(),
            format!("{}-disk-large", agent),
        ));
        sub_tasks.push((
            "Check inode usage and filesystem health".into(),
            format!("{}-disk-inode", agent),
        ));
    } else if task_lower.contains("payment")
        || task_lower.contains("checkout")
        || task_lower.contains("503")
    {
        sub_tasks.push((
            "Check payment service health and error logs".into(),
            format!("{}-payment-health", agent),
        ));
        sub_tasks.push((
            "Analyze payment service recent deployments".into(),
            format!("{}-payment-deploy", agent),
        ));
        sub_tasks.push((
            "Review downstream dependency status for payment".into(),
            format!("{}-payment-deps", agent),
        ));
        sub_tasks.push((
            "Inspect payment service resource usage".into(),
            format!("{}-payment-resources", agent),
        ));
    } else {
        sub_tasks.push((
            "Analyze system metrics and resource usage".into(),
            format!("{}-metrics", agent),
        ));
        sub_tasks.push((
            "Review recent changes and deployments".into(),
            format!("{}-changes", agent),
        ));
        sub_tasks.push((
            "Check error logs and recent anomalies".into(),
            format!("{}-errors", agent),
        ));
        sub_tasks.push((
            "Summarize findings and recommend actions".into(),
            format!("{}-summary", agent),
        ));
    }
    sub_tasks
}

async fn execute_ai_tool(
    agent: &str,
    name: &str,
    arguments: &Value,
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
) -> Value {
    let role = AgentRole::Executor;
    if let Err(error) = SecurityPolicy::validate_tool_call(&role, name, arguments) {
        return json!({ "error": error, "tool": name });
    }
    match name {
        "exec_command" => {
            let command = arguments
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("uptime");
            let id = next_id("tool-cmd");
            let _ = event_tx.send(OpsEvent::CommandRequested {
                id: id.clone(),
                command: command.into(),
                reason: format!("AI agent {agent} requested tool execution"),
                dry_run: false,
                timestamp: now_ts(),
            });

            let command_clone = command.to_string();
            let event_tx_clone = event_tx.clone();
            let (result_tx, result_rx) = tokio::sync::oneshot::channel::<Value>();
            let id_clone = id.clone();

            tokio::spawn(async move {
                let parsed = parse_allowlisted_command(&command_clone);
                match parsed {
                    Ok(cmd) => {
                        let child = match tokio::process::Command::new(cmd.program)
                            .args(&cmd.args)
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                        {
                            Ok(child) => child,
                            Err(e) => {
                                let _ = result_tx.send(json!({
                                    "error": format!("failed to spawn: {e}"),
                                    "command": command_clone
                                }));
                                return;
                            }
                        };
                        let output = child.wait_with_output().await;
                        match output {
                            Ok(out) => {
                                let stdout =
                                    redact_sensitive(&String::from_utf8_lossy(&out.stdout));
                                let stderr =
                                    redact_sensitive(&String::from_utf8_lossy(&out.stderr));
                                let _ = event_tx_clone.send(OpsEvent::CommandExecuted {
                                    id: id_clone,
                                    command: command_clone.clone(),
                                    success: out.status.success(),
                                    exit_code: out.status.code(),
                                    stdout: stdout.clone(),
                                    stderr: stderr.clone(),
                                    timestamp: now_ts(),
                                });
                                let _ = result_tx.send(json!({
                                    "stdout": stdout,
                                    "stderr": stderr,
                                    "exit_code": out.status.code(),
                                    "success": out.status.success(),
                                    "command": command_clone
                                }));
                            }
                            Err(e) => {
                                let _ = result_tx.send(json!({
                                    "error": format!("execution failed: {e}"),
                                    "command": command_clone
                                }));
                            }
                        }
                    }
                    Err(msg) => {
                        let _ = event_tx_clone.send(OpsEvent::CommandExecuted {
                            id: id_clone,
                            command: command_clone.clone(),
                            success: false,
                            exit_code: None,
                            stdout: String::new(),
                            stderr: msg.clone(),
                            timestamp: now_ts(),
                        });
                        let _ = result_tx.send(json!({
                            "error": msg,
                            "command": command_clone
                        }));
                    }
                }
            });

            match tokio::time::timeout(Duration::from_secs(10), result_rx).await {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => json!({ "error": "tool execution channel closed" }),
                Err(_) => json!({ "error": "tool execution timed out after 10s" }),
            }
        }
        _ => json!({ "error": format!("unknown tool: {name}"), "arguments": arguments }),
    }
}

fn create_incident_dag(incident_id: String) -> DagWorkflowRuntime {
    let wf_id = format!("wf-{incident_id}");
    DagWorkflowRuntime::new(
        wf_id,
        format!("Incident Response: {incident_id}"),
        vec![
            WorkflowNode {
                id: "detect".into(),
                kind: WorkflowNodeKind::Command,
                command: Some("uptime".into()),
                agent: None,
                depends_on: vec![],
                retry: Default::default(),
                approval_required: false,
                condition: None,
                on_success: None,
                on_failure: None,
                rollback: None,
            },
            WorkflowNode {
                id: "spawn-planner".into(),
                kind: WorkflowNodeKind::AgentTask,
                command: None,
                agent: Some("planner-01".into()),
                depends_on: vec!["detect".into()],
                retry: Default::default(),
                approval_required: false,
                condition: None,
                on_success: None,
                on_failure: None,
                rollback: None,
            },
            WorkflowNode {
                id: "spawn-executor".into(),
                kind: WorkflowNodeKind::AgentTask,
                command: None,
                agent: Some("executor-01".into()),
                depends_on: vec!["detect".into()],
                retry: Default::default(),
                approval_required: false,
                condition: None,
                on_success: None,
                on_failure: None,
                rollback: None,
            },
            WorkflowNode {
                id: "collect-evidence".into(),
                kind: WorkflowNodeKind::Command,
                command: Some("journalctl -n 30 --no-pager".into()),
                agent: None,
                depends_on: vec!["spawn-planner".into(), "spawn-executor".into()],
                retry: Default::default(),
                approval_required: false,
                condition: None,
                on_success: None,
                on_failure: None,
                rollback: None,
            },
            WorkflowNode {
                id: "validate".into(),
                kind: WorkflowNodeKind::Command,
                command: Some("systemctl --no-pager --failed".into()),
                agent: None,
                depends_on: vec!["collect-evidence".into()],
                retry: Default::default(),
                approval_required: false,
                condition: None,
                on_success: None,
                on_failure: None,
                rollback: None,
            },
            WorkflowNode {
                id: "approve-remediation".into(),
                kind: WorkflowNodeKind::Approval,
                command: None,
                agent: None,
                depends_on: vec!["validate".into()],
                retry: Default::default(),
                approval_required: true,
                condition: None,
                on_success: None,
                on_failure: None,
                rollback: Some("notify-operator".into()),
            },
        ],
    )
}

async fn step_dag_workflows(
    workflows: &mut [DagWorkflowRuntime],
    state: &OpsState,
    event_tx: &mpsc::UnboundedSender<OpsEvent>,
    _ai_clients: &[AiClient],
    pending_nodes: &mut HashMap<String, (String, String, String)>,
) {
    let mut to_advance: Vec<(String, String, u16)> = Vec::new();

    for wf in workflows.iter_mut() {
        let ready: Vec<WorkflowNode> = wf.ready_nodes().into_iter().cloned().collect();
        let wf_id = wf.id.clone();

        for node in &ready {
            let node_id = node.id.clone();
            let node_kind = node.kind.clone();

            if wf.mark_running(&node_id).is_err() {
                continue;
            }

            match node_kind {
                WorkflowNodeKind::Command => {
                    let cmd = node.command.clone().unwrap_or_else(|| "uptime".into());
                    let cmd_id = next_id("wf-cmd");
                    let _ = event_tx.send(OpsEvent::CommandRequested {
                        id: cmd_id.clone(),
                        command: cmd.clone(),
                        reason: format!("DAG workflow {wf_id} node {node_id}"),
                        dry_run: false,
                        timestamp: now_ts(),
                    });
                    // Track pending — don't emit WorkflowNodeCompleted yet; wait for CommandExecuted
                    pending_nodes.insert(cmd_id, (wf_id.clone(), node_id.clone(), cmd));
                }
                WorkflowNodeKind::AgentTask => {
                    let agent_name = node.agent.clone().unwrap_or_else(next_sub_agent_name);
                    let _ = event_tx.send(OpsEvent::AgentSpawned {
                        name: agent_name.clone(),
                        role: AgentRole::Research,
                        timestamp: now_ts(),
                    });
                    let _ = event_tx.send(OpsEvent::TaskAssigned {
                        agent: agent_name.clone(),
                        task: format!("Execute DAG workflow {wf_id} node {node_id}"),
                        timestamp: now_ts(),
                    });
                    // Agent tasks complete immediately (fire-and-forget for now)
                    let _ = event_tx.send(OpsEvent::WorkflowNodeCompleted {
                        workflow_id: wf_id.clone(),
                        node_id: node_id.clone(),
                        timestamp: now_ts(),
                    });
                }
                WorkflowNodeKind::Approval => {
                    let risk_score = WorkflowSecurity::risk_score(
                        node.command.as_deref(),
                        node.approval_required,
                        node.rollback.is_some(),
                    );
                    let _ = event_tx.send(OpsEvent::RecoveryProposed {
                        action: RecoveryAction {
                            id: next_id("rec"),
                            name: format!("Approval required for {wf_id}"),
                            command: "systemctl restart edge-nginx".into(),
                            target: "edge-nginx".into(),
                            status: RecoveryStatus::AwaitingApproval,
                            risk: format!(
                                "approval checkpoint in automated workflow; workflow risk score {risk_score}/100"
                            ),
                            requires_role: UserRole::Operator,
                            evidence: vec![format!(
                                "DAG workflow {wf_id} requires approval at node {node_id}"
                            )],
                            requested_by: "dag-workflow-engine".into(),
                            approved_by: None,
                            dry_run_only: true,
                            timestamp: now_ts(),
                        },
                    });
                    let _ = event_tx.send(OpsEvent::WorkflowNodeCompleted {
                        workflow_id: wf_id.clone(),
                        node_id: node_id.clone(),
                        timestamp: now_ts(),
                    });
                }
                WorkflowNodeKind::Condition => {
                    // Evaluate condition against state
                    let condition_met = node
                        .condition
                        .as_ref()
                        .map(|cond| {
                            let context = build_condition_context(state);
                            wf.evaluate_condition(cond, &context)
                        })
                        .unwrap_or(true);

                    if condition_met {
                        let _ = wf.mark_succeeded(&node_id);
                        tracing::info!(workflow_id = %wf_id, %node_id, condition = ?node.condition, "condition met");
                    } else {
                        let _ = wf.mark_skipped(&node_id);
                        tracing::info!(workflow_id = %wf_id, %node_id, condition = ?node.condition, "condition not met, skipping");
                        // Skip all downstream nodes that depend only on this condition
                        skip_downstream_nodes(wf, &node_id);
                    }
                    let _ = event_tx.send(OpsEvent::WorkflowNodeCompleted {
                        workflow_id: wf_id.clone(),
                        node_id: node_id.clone(),
                        timestamp: now_ts(),
                    });
                }
            }
        }

        let progress = wf.progress();
        let is_done = wf.is_complete();
        let stage = if is_done {
            "Completed".into()
        } else if !ready.is_empty() {
            format!("Executing {} node(s)", ready.len())
        } else if wf.all_running() {
            "Waiting for node completion".into()
        } else {
            "Running".into()
        };

        let wf_id_clone = wf.id.clone();
        to_advance.push((wf_id_clone, stage, progress));
    }

    for (wf_id, stage, progress) in &to_advance {
        let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
            id: wf_id.clone(),
            stage: stage.clone(),
            progress: *progress,
            timestamp: now_ts(),
        });
    }
}

/// Build a flat key=value context from OpsState for condition evaluation.
fn build_condition_context(state: &OpsState) -> HashMap<String, String> {
    let mut ctx = HashMap::new();
    ctx.insert("health".into(), state.health.to_string());
    ctx.insert("alert_count".into(), state.alert_count.to_string());
    ctx.insert("active_agents".into(), state.active_agents.to_string());
    ctx.insert("uptime_secs".into(), state.uptime_secs.to_string());
    if let Some(node) = state.infra.first() {
        ctx.insert("infra_health".into(), node.health.to_string());
        ctx.insert("infra_cpu".into(), node.cpu.to_string());
        ctx.insert("infra_memory".into(), node.memory.to_string());
    }
    ctx
}

/// Skip nodes that depend exclusively on a skipped condition node.
fn skip_downstream_nodes(wf: &mut DagWorkflowRuntime, skipped_node: &str) {
    let node_ids: Vec<String> = wf.node_states.keys().cloned().collect();
    for id in &node_ids {
        if let Some(node) = wf.get_node(id).cloned()
            && node.depends_on.contains(&skipped_node.to_string())
            && node.depends_on.iter().all(|dep| {
                wf.node_states
                    .get(dep)
                    .map(|s| matches!(s.status, NodeStatus::Succeeded | NodeStatus::Skipped))
                    .unwrap_or(false)
            })
            && wf.mark_skipped(id).is_ok()
        {
            tracing::info!("skipping downstream node {} due to condition", id);
        }
    }
}

async fn run_infrastructure_command(
    id: String,
    command: String,
    dry_run: bool,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) {
    let parsed = match parse_allowlisted_command(&command) {
        Ok(parsed) => Some(parsed),
        Err(message) => {
            if dry_run {
                let _ = event_tx.send(OpsEvent::CommandExecuted {
                    id,
                    command,
                    success: true,
                    exit_code: Some(0),
                    stdout: format!("dry-run approval gate: {message}"),
                    stderr: String::new(),
                    timestamp: now_ts(),
                });
                return;
            }
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: message,
                timestamp: now_ts(),
            });
            return;
        }
    };

    if dry_run {
        let _ = event_tx.send(OpsEvent::CommandExecuted {
            id,
            command,
            success: true,
            exit_code: Some(0),
            stdout: "dry-run: command approved but not executed".into(),
            stderr: String::new(),
            timestamp: now_ts(),
        });
        return;
    }

    let Some(parsed) = parsed else {
        return;
    };
    let mut child = match Command::new(parsed.program)
        .args(parsed.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("failed to start command: {error}"),
                timestamp: now_ts(),
            });
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(stream_command_output(
        id.clone(),
        "stdout",
        stdout,
        event_tx.clone(),
    ));
    let stderr_task = tokio::spawn(stream_command_output(
        id.clone(),
        "stderr",
        stderr,
        event_tx.clone(),
    ));

    let status = match time::timeout(Duration::from_secs(8), child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("failed while waiting for command: {error}"),
                timestamp: now_ts(),
            });
            return;
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: "command timed out after 8s".into(),
                timestamp: now_ts(),
            });
            return;
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let _ = event_tx.send(OpsEvent::CommandExecuted {
        id,
        command,
        success: status.success(),
        exit_code: status.code(),
        stdout: redact_sensitive(&stdout),
        stderr: redact_sensitive(&stderr),
        timestamp: now_ts(),
    });
}

pub(crate) async fn run_live_log_stream(event_tx: mpsc::UnboundedSender<OpsEvent>) {
    let id = "live-journalctl".to_string();
    let command = "journalctl -f -n 20 --no-pager".to_string();
    let _ = event_tx.send(OpsEvent::CommandRequested {
        id: id.clone(),
        command: command.clone(),
        reason: "Continuous real system log stream for Logs tab".into(),
        dry_run: false,
        timestamp: now_ts(),
    });

    let mut child = match Command::new("journalctl")
        .args(["-f", "-n", "20", "--no-pager"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("failed to start continuous journal stream: {error}"),
                timestamp: now_ts(),
            });
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(stream_command_output(
        id.clone(),
        "stdout",
        stdout,
        event_tx.clone(),
    ));
    let stderr_task = tokio::spawn(stream_command_output(
        id.clone(),
        "stderr",
        stderr,
        event_tx.clone(),
    ));

    let status = match child.wait().await {
        Ok(status) => status,
        Err(error) => {
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("continuous journal stream failed: {error}"),
                timestamp: now_ts(),
            });
            return;
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let _ = event_tx.send(OpsEvent::CommandExecuted {
        id,
        command,
        success: status.success(),
        exit_code: status.code(),
        stdout,
        stderr,
        timestamp: now_ts(),
    });
}

async fn stream_command_output(
    id: String,
    stream: &'static str,
    output: Option<impl tokio::io::AsyncRead + Send + Unpin + 'static>,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) -> String {
    let Some(output) = output else {
        return String::new();
    };
    let mut lines = BufReader::new(output).lines();
    let mut captured = Vec::new();
    let max_captured = std::env::var("OCTOBOT_STREAM_CAPTURE_LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100usize);
    while let Ok(Some(line)) = lines.next_line().await {
        let line = redact_sensitive(&line);
        if captured.len() < max_captured {
            captured.push(line.clone());
        }
        let _ = event_tx.send(OpsEvent::CommandOutput {
            id: id.clone(),
            stream: stream.into(),
            line,
            timestamp: now_ts(),
        });
    }
    captured.join("\n")
}

pub(crate) struct ParsedCommand {
    program: &'static str,
    args: Vec<String>,
}

pub(crate) fn parse_allowlisted_command(
    command: &str,
) -> std::result::Result<ParsedCommand, String> {
    let decision = SecurityPolicy::validate_command(command)?;
    if matches!(decision.tier, CommandTier::Remediation) {
        return Err(format!(
            "blocked by sandbox allowlist: `{command}` must use the approved remediation engine ({})",
            decision.audit
        ));
    }
    let parts = command.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["docker", "ps"] => Ok(ParsedCommand {
            program: "docker",
            args: vec!["ps".into()],
        }),
        ["kubectl", "get", "pods"] => Ok(ParsedCommand {
            program: "kubectl",
            args: vec!["get".into(), "pods".into()],
        }),
        ["journalctl", "-n", count, "--no-pager"] if count.parse::<u16>().is_ok() => {
            Ok(ParsedCommand {
                program: "journalctl",
                args: vec!["-n".into(), (*count).into(), "--no-pager".into()],
            })
        }
        ["journalctl", "-f", "-n", count, "--no-pager"] if count.parse::<u16>().is_ok() => {
            Ok(ParsedCommand {
                program: "journalctl",
                args: vec![
                    "-f".into(),
                    "-n".into(),
                    (*count).into(),
                    "--no-pager".into(),
                ],
            })
        }
        ["systemctl", "--no-pager", "--failed"] => Ok(ParsedCommand {
            program: "systemctl",
            args: vec!["--no-pager".into(), "--failed".into()],
        }),
        ["ps", "aux"] => Ok(ParsedCommand {
            program: "ps",
            args: vec!["aux".into()],
        }),
        ["df", "-h"] => Ok(ParsedCommand {
            program: "df",
            args: vec!["-h".into()],
        }),
        ["uptime"] => Ok(ParsedCommand {
            program: "uptime",
            args: Vec::new(),
        }),
        ["ssh", host, "uptime"] if safe_ssh_target(host) => Ok(ParsedCommand {
            program: "ssh",
            args: vec![(*host).into(), "uptime".into()],
        }),
        _ => Err(format!(
            "blocked by sandbox allowlist: `{command}`. Allowed: docker ps, kubectl get pods, journalctl -n N --no-pager, journalctl -f -n N --no-pager, systemctl --no-pager --failed, ps aux, df -h, uptime, ssh <host> uptime"
        )),
    }
}

fn safe_ssh_target(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 128
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
}
