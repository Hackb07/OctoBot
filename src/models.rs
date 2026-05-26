use serde::{Deserialize, Serialize};

use crate::utils::{now_ts, trim_preview};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Agent {
    pub(crate) name: String,
    pub(crate) role: AgentRole,
    pub(crate) status: AgentStatus,
    pub(crate) task: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentLink {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) protocol: String,
    pub(crate) message: String,
    pub(crate) confidence: u8,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum AgentRole {
    Triage,
    Logs,
    Research,
    Workflow,
    Report,
    Planner,
    Executor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum AgentStatus {
    Idle,
    Running,
    Waiting,
    Escalated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Workflow {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) owner: String,
    pub(crate) stage: String,
    pub(crate) progress: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecutionRecord {
    pub(crate) id: String,
    pub(crate) command: String,
    pub(crate) status: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) output_preview: String,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExplainabilityRecord {
    pub(crate) id: String,
    pub(crate) action: String,
    pub(crate) why: String,
    pub(crate) evidence: Vec<String>,
    pub(crate) confidence: u8,
    pub(crate) tools_used: Vec<String>,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TimelineEvent {
    pub(crate) id: String,
    pub(crate) timestamp: String,
    pub(crate) category: TimelineCategory,
    pub(crate) source: String,
    pub(crate) summary: String,
    pub(crate) cpu: Option<u8>,
    pub(crate) memory: Option<u8>,
    pub(crate) related_incident: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum TimelineCategory {
    Deployment,
    Log,
    Metric,
    Commit,
    Incident,
    Agent,
    Recovery,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum UserRole {
    Admin,
    Operator,
    ReadOnly,
    SecurityReviewer,
}

impl UserRole {
    pub(crate) fn can_approve_recovery(&self) -> bool {
        matches!(self, UserRole::Admin | UserRole::Operator)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RecoveryAction {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) command: String,
    pub(crate) target: String,
    pub(crate) status: RecoveryStatus,
    pub(crate) risk: String,
    pub(crate) requires_role: UserRole,
    pub(crate) evidence: Vec<String>,
    pub(crate) requested_by: String,
    pub(crate) approved_by: Option<UserRole>,
    pub(crate) dry_run_only: bool,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResearchSignal {
    pub(crate) source: String,
    pub(crate) evidence: String,
    pub(crate) reliability: u8,
    pub(crate) contradiction: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResearchConfidenceProfile {
    pub(crate) subject: String,
    pub(crate) evidence_reliability: u8,
    pub(crate) contradiction_count: u8,
    pub(crate) ranking: u8,
    pub(crate) last_reviewed: String,
    pub(crate) signals: Vec<ResearchSignal>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum PluginKind {
    Tool,
    Workflow,
    Integration,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum PluginStatus {
    Registered,
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PluginDescriptor {
    pub(crate) name: String,
    pub(crate) kind: PluginKind,
    pub(crate) description: String,
    pub(crate) version: String,
    pub(crate) status: PluginStatus,
    pub(crate) owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum AgentRuntimeKind {
    LocalProcess,
    RemoteServer,
    Container,
    Cluster,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RuntimeStatus {
    Local,
    Provisioned,
    Active,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentRuntime {
    pub(crate) agent: String,
    pub(crate) kind: AgentRuntimeKind,
    pub(crate) endpoint: String,
    pub(crate) status: RuntimeStatus,
    pub(crate) heartbeat: String,
    pub(crate) notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KnowledgeNode {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: String,
    pub(crate) confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KnowledgeEdge {
    pub(crate) from: String,
    pub(crate) relation: String,
    pub(crate) to: String,
    pub(crate) weight: u8,
    pub(crate) timestamp: String,
}

/// Describes a discovered dependency between two infrastructure components.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TopologyEdge {
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) relation: String,
}

/// Snapshot of the infrastructure topology at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct TopologySnapshot {
    pub(crate) edges: Vec<TopologyEdge>,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SandboxPolicy {
    pub(crate) mode: String,
    pub(crate) persisted: bool,
    pub(crate) approved_roles: Vec<UserRole>,
    pub(crate) review_required_for: Vec<String>,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum RecoveryStatus {
    Proposed,
    AwaitingApproval,
    Approved,
    DryRunQueued,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReplayCursor {
    pub(crate) active: bool,
    pub(crate) position: usize,
    pub(crate) total: usize,
    pub(crate) last_event: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) enum OpsEvent {
    IncidentDetected {
        incident_id: String,
        service: String,
        severity: String,
        timestamp: String,
    },
    AgentSpawned {
        name: String,
        role: AgentRole,
        timestamp: String,
    },
    AgentLifecycleChanged {
        agent: String,
        status: AgentStatus,
        task: String,
        timestamp: String,
    },
    AgentTelemetryRecorded {
        agent: String,
        metric: String,
        value: u64,
        timestamp: String,
    },
    AiProviderRegistered {
        provider: String,
        endpoint: String,
        timestamp: String,
    },
    ToolCallRequested {
        id: String,
        tool: String,
        arguments: serde_json::Value,
        timestamp: String,
    },
    ToolCallCompleted {
        id: String,
        tool: String,
        success: bool,
        output: serde_json::Value,
        timestamp: String,
    },
    TaskAssigned {
        agent: String,
        task: String,
        timestamp: String,
    },
    AgentMemoryStored {
        agent: String,
        key: String,
        value: String,
        timestamp: String,
    },
    PlanCreated {
        planner: String,
        task: String,
        sub_tasks: Vec<String>,
        timestamp: String,
    },
    SubTaskCompleted {
        planner: String,
        executor: String,
        sub_task: String,
        result: String,
        timestamp: String,
    },
    CommandRequested {
        id: String,
        command: String,
        reason: String,
        dry_run: bool,
        timestamp: String,
    },
    CommandOutput {
        id: String,
        stream: String,
        line: String,
        timestamp: String,
    },
    CommandExecuted {
        id: String,
        command: String,
        success: bool,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        timestamp: String,
    },
    ResearchCompleted {
        topic: String,
        conclusion: String,
        confidence: u8,
        timestamp: String,
    },
    WorkflowAdvanced {
        id: String,
        stage: String,
        progress: u16,
        timestamp: String,
    },
    ExplainabilityRecorded {
        record: ExplainabilityRecord,
    },
    AgentMessageRecorded {
        from: String,
        to: String,
        protocol: String,
        message: String,
        confidence: u8,
        timestamp: String,
    },
    TimelineRecorded {
        event: TimelineEvent,
    },
    RecoveryProposed {
        action: RecoveryAction,
    },
    RecoveryApproved {
        action_id: String,
        role: UserRole,
        timestamp: String,
    },
    ResearchConfidenceUpdated {
        profile: ResearchConfidenceProfile,
    },
    PluginRegistered {
        plugin: PluginDescriptor,
    },
    PluginStatusChanged {
        name: String,
        status: PluginStatus,
        timestamp: String,
    },
    RuntimeUpdated {
        runtime: AgentRuntime,
    },
    KnowledgeNodeEnsured {
        node: KnowledgeNode,
    },
    KnowledgeEdgeAdded {
        edge: KnowledgeEdge,
    },
    SandboxPolicyUpdated {
        policy: SandboxPolicy,
    },
    RoleChanged {
        role: UserRole,
        timestamp: String,
    },
    ReplayStarted {
        timestamp: String,
    },
    ReplayStepped {
        position: usize,
        timestamp: String,
    },
    UserCommandEntered {
        command: String,
        timestamp: String,
    },
    MetricsSampled {
        cpu: u8,
        memory: u8,
        timestamp: String,
    },
    InfrastructureSnapshotRecorded {
        source: String,
        nodes: Vec<InfraNode>,
        timestamp: String,
    },
    WorkflowDefinitionLoaded {
        definition: WorkflowDefinitionSummary,
    },
    WorkflowNodeCompleted {
        workflow_id: String,
        node_id: String,
        timestamp: String,
    },
    AiProviderLogin {
        kind: String,
        endpoint: String,
        model: String,
        #[serde(skip)]
        api_key: Option<String>,
        timestamp: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Incident {
    pub(crate) id: String,
    pub(crate) service: String,
    pub(crate) severity: String,
    pub(crate) hypothesis: String,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct InfraNode {
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) health: u8,
    pub(crate) cpu: u8,
    pub(crate) memory: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkflowDefinitionSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) node_count: usize,
    pub(crate) entrypoint: String,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OpsState {
    pub(crate) workspace: String,
    pub(crate) environment: String,
    pub(crate) uptime_secs: u64,
    pub(crate) health: u8,
    pub(crate) alert_count: u8,
    pub(crate) active_agents: usize,
    pub(crate) metrics: Vec<u64>,
    pub(crate) agents: Vec<Agent>,
    pub(crate) workflows: Vec<Workflow>,
    pub(crate) incidents: Vec<Incident>,
    pub(crate) infra: Vec<InfraNode>,
    pub(crate) executions: Vec<ExecutionRecord>,
    pub(crate) explainability: Vec<ExplainabilityRecord>,
    pub(crate) coordination_links: Vec<AgentLink>,
    pub(crate) timeline: Vec<TimelineEvent>,
    pub(crate) recovery_actions: Vec<RecoveryAction>,
    pub(crate) research_profile: ResearchConfidenceProfile,
    pub(crate) plugins: Vec<PluginDescriptor>,
    pub(crate) runtimes: Vec<AgentRuntime>,
    pub(crate) knowledge_nodes: Vec<KnowledgeNode>,
    pub(crate) knowledge_edges: Vec<KnowledgeEdge>,
    pub(crate) sandbox_policy: SandboxPolicy,
    pub(crate) current_role: UserRole,
    pub(crate) replay: ReplayCursor,
    pub(crate) events: Vec<OpsEvent>,
    pub(crate) logs: Vec<String>,
    pub(crate) reports: Vec<String>,
    pub(crate) topology: TopologySnapshot,
}

impl OpsState {
    pub(crate) fn empty() -> Self {
        Self {
            workspace: "octobot-ops".into(),
            environment: "prod / us-east".into(),
            uptime_secs: 0,
            health: 0,
            alert_count: 0,
            active_agents: 0,
            metrics: Vec::new(),
            agents: Vec::new(),
            workflows: Vec::new(),
            incidents: Vec::new(),
            infra: Vec::new(),
            executions: Vec::new(),
            explainability: Vec::new(),
            coordination_links: Vec::new(),
            timeline: Vec::new(),
            recovery_actions: Vec::new(),
            research_profile: ResearchConfidenceProfile {
                subject: "uninitialized".into(),
                evidence_reliability: 0,
                contradiction_count: 0,
                ranking: 0,
                last_reviewed: now_ts(),
                signals: Vec::new(),
            },
            plugins: Vec::new(),
            runtimes: Vec::new(),
            knowledge_nodes: Vec::new(),
            knowledge_edges: Vec::new(),
            sandbox_policy: SandboxPolicy {
                mode: "read-only allowlist".into(),
                persisted: false,
                approved_roles: Vec::new(),
                review_required_for: Vec::new(),
                updated_at: now_ts(),
            },
            current_role: UserRole::ReadOnly,
            replay: ReplayCursor {
                active: false,
                position: 0,
                total: 0,
                last_event: None,
            },
            events: Vec::new(),
            logs: Vec::new(),
            reports: Vec::new(),
            topology: TopologySnapshot::default(),
        }
    }

    pub(crate) fn seed() -> Self {
        Self {
            workspace: "octobot-ops".into(),
            environment: "prod / us-east".into(),
            uptime_secs: 0,
            health: 94,
            alert_count: 3,
            active_agents: 0,
            metrics: vec![36, 42, 40, 45, 51, 49, 58, 62, 59, 63, 68, 64],
            agents: Vec::new(),
            workflows: Vec::new(),
            incidents: vec![
                Incident {
                    id: "inc-042".into(),
                    service: "edge-nginx".into(),
                    severity: "SEV2".into(),
                    hypothesis: "TLS handshakes queueing after ingress rollout".into(),
                    status: "investigating".into(),
                },
                Incident {
                    id: "inc-039".into(),
                    service: "auth-service".into(),
                    severity: "SEV3".into(),
                    hypothesis: "Token cache saturation during deploy window".into(),
                    status: "monitoring".into(),
                },
            ],
            infra: vec![
                InfraNode {
                    name: "edge-nginx-7d9c".into(),
                    kind: "deployment".into(),
                    health: 78,
                    cpu: 72,
                    memory: 64,
                },
                InfraNode {
                    name: "auth-service".into(),
                    kind: "service".into(),
                    health: 86,
                    cpu: 48,
                    memory: 81,
                },
                InfraNode {
                    name: "postgres-primary".into(),
                    kind: "database".into(),
                    health: 93,
                    cpu: 38,
                    memory: 58,
                },
                InfraNode {
                    name: "qdrant-vector".into(),
                    kind: "vector-db".into(),
                    health: 97,
                    cpu: 29,
                    memory: 44,
                },
            ],
            executions: Vec::new(),
            explainability: vec![ExplainabilityRecord {
                id: "exp-0001".into(),
                action: "Open incident inc-042 investigation".into(),
                why: "Latency alert and ingress rollout timing overlap require triage.".into(),
                evidence: vec![
                    "edge-nginx p95 crossed 820ms for 4m".into(),
                    "deploy-1188 occurred inside the alert window".into(),
                ],
                confidence: 72,
                tools_used: vec!["prometheus".into(), "loki".into()],
                timestamp: now_ts(),
            }],
            coordination_links: Vec::new(),
            timeline: vec![
                TimelineEvent {
                    id: "time-0001".into(),
                    timestamp: now_ts(),
                    category: TimelineCategory::Deployment,
                    source: "deploy-1188".into(),
                    summary: "edge-nginx ingress rollout started inside alert window".into(),
                    cpu: None,
                    memory: None,
                    related_incident: Some("inc-042".into()),
                },
                TimelineEvent {
                    id: "time-0002".into(),
                    timestamp: now_ts(),
                    category: TimelineCategory::Incident,
                    source: "alertmanager".into(),
                    summary: "p95 latency crossed threshold after rollout".into(),
                    cpu: Some(72),
                    memory: Some(64),
                    related_incident: Some("inc-042".into()),
                },
            ],
            recovery_actions: vec![RecoveryAction {
                id: "rec-0001".into(),
                name: "Restart edge-nginx".into(),
                command: "systemctl restart edge-nginx".into(),
                target: "edge-nginx".into(),
                status: RecoveryStatus::AwaitingApproval,
                risk: "brief connection draining and reload churn".into(),
                requires_role: UserRole::Operator,
                evidence: vec![
                    "latency alert overlaps ingress rollout".into(),
                    "systemd status check is required before execution".into(),
                ],
                requested_by: "workflow-engine".into(),
                approved_by: None,
                dry_run_only: true,
                timestamp: now_ts(),
            }],
            research_profile: ResearchConfidenceProfile {
                subject: "nginx_latency".into(),
                evidence_reliability: 78,
                contradiction_count: 1,
                ranking: 82,
                last_reviewed: now_ts(),
                signals: vec![
                    ResearchSignal {
                        source: "prometheus".into(),
                        evidence: "p95 latency and request rate correlated with rollout".into(),
                        reliability: 83,
                        contradiction: false,
                    },
                    ResearchSignal {
                        source: "journald".into(),
                        evidence: "restart spikes and TLS handshake retries observed".into(),
                        reliability: 76,
                        contradiction: false,
                    },
                    ResearchSignal {
                        source: "runbook".into(),
                        evidence: "rollback is safe but requires operator approval".into(),
                        reliability: 71,
                        contradiction: true,
                    },
                ],
            },
            plugins: vec![
                PluginDescriptor {
                    name: "openrouter-research".into(),
                    kind: PluginKind::Integration,
                    description: "LLM-backed research route for runbook synthesis".into(),
                    version: "0.1.0".into(),
                    status: PluginStatus::Registered,
                    owner: "platform".into(),
                },
                PluginDescriptor {
                    name: "prometheus-triage".into(),
                    kind: PluginKind::Tool,
                    description: "Prometheus evidence collector for active incidents".into(),
                    version: "0.1.0".into(),
                    status: PluginStatus::Enabled,
                    owner: "triage-01".into(),
                },
                PluginDescriptor {
                    name: "workflow-rca".into(),
                    kind: PluginKind::Workflow,
                    description: "Auto-generates report-ready RCA outlines".into(),
                    version: "0.1.0".into(),
                    status: PluginStatus::Registered,
                    owner: "reporter-01".into(),
                },
            ],
            runtimes: Vec::new(),
            knowledge_nodes: vec![
                KnowledgeNode {
                    id: "svc-edge-nginx".into(),
                    label: "edge-nginx".into(),
                    kind: "service".into(),
                    confidence: 93,
                },
                KnowledgeNode {
                    id: "inc-042".into(),
                    label: "incident inc-042".into(),
                    kind: "incident".into(),
                    confidence: 89,
                },
                KnowledgeNode {
                    id: "deploy-1188".into(),
                    label: "deploy-1188".into(),
                    kind: "deployment".into(),
                    confidence: 84,
                },
                KnowledgeNode {
                    id: "metric-p95".into(),
                    label: "latency p95".into(),
                    kind: "metric".into(),
                    confidence: 90,
                },
            ],
            knowledge_edges: vec![
                KnowledgeEdge {
                    from: "deploy-1188".into(),
                    relation: "triggered".into(),
                    to: "inc-042".into(),
                    weight: 86,
                    timestamp: now_ts(),
                },
                KnowledgeEdge {
                    from: "metric-p95".into(),
                    relation: "correlates-with".into(),
                    to: "svc-edge-nginx".into(),
                    weight: 91,
                    timestamp: now_ts(),
                },
                KnowledgeEdge {
                    from: "inc-042".into(),
                    relation: "impacts".into(),
                    to: "svc-edge-nginx".into(),
                    weight: 88,
                    timestamp: now_ts(),
                },
            ],
            sandbox_policy: SandboxPolicy {
                mode: "read-only allowlist".into(),
                persisted: true,
                approved_roles: vec![UserRole::Admin, UserRole::Operator],
                review_required_for: vec!["restart".into(), "rollback".into(), "cleanup".into()],
                updated_at: now_ts(),
            },
            current_role: UserRole::ReadOnly,
            replay: ReplayCursor {
                active: false,
                position: 0,
                total: 0,
                last_event: None,
            },
            events: Vec::new(),
            logs: Vec::new(),
            reports: vec![
                "inc-042: evidence graph 63% complete, 7 validated claims".into(),
                "daily-sre: availability summary waiting on OpenSearch export".into(),
            ],
            topology: TopologySnapshot::default(),
        }
    }

    pub(crate) fn tick(&mut self) {
        self.uptime_secs += 1;
        let next = (self.metrics.last().copied().unwrap_or(50) + 7 + self.uptime_secs) % 100;
        self.metrics.push(next.max(18));
        if self.metrics.len() > 30 {
            self.metrics.remove(0);
        }

        for workflow in &mut self.workflows {
            workflow.progress = ((workflow.progress + 3) % 101).max(12);
        }

        for (idx, node) in self.infra.iter_mut().enumerate() {
            node.cpu = ((node.cpu as u16 + 5 + idx as u16) % 100) as u8;
            node.memory = ((node.memory as u16 + 2 + idx as u16) % 100) as u8;
            node.health = 100u8.saturating_sub(node.cpu.saturating_sub(76));
        }

        self.health = self
            .infra
            .iter()
            .map(|node| node.health as u16)
            .sum::<u16>()
            .checked_div(self.infra.len() as u16)
            .unwrap_or(100) as u8;
    }

    pub(crate) fn apply_event(&mut self, event: OpsEvent) {
        match &event {
            OpsEvent::IncidentDetected {
                incident_id,
                service,
                severity,
                timestamp,
            } => {
                self.alert_count = self.alert_count.saturating_add(1);
                self.ensure_knowledge_node(
                    incident_id,
                    format!("incident {incident_id}"),
                    "incident",
                    86,
                );
                self.ensure_knowledge_node(service, service.clone(), "service", 90);
                self.record_knowledge_edge(service, "has-incident", incident_id, 92, timestamp);
                if !self
                    .incidents
                    .iter()
                    .any(|incident| incident.id == *incident_id)
                {
                    self.incidents.push(Incident {
                        id: incident_id.clone(),
                        service: service.clone(),
                        severity: severity.clone(),
                        hypothesis: "Awaiting correlated evidence from workflow engine".into(),
                        status: "detected".into(),
                    });
                }
                self.record_timeline(TimelineEvent {
                    id: format!("time-{incident_id}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Incident,
                    source: service.clone(),
                    summary: format!("{severity} incident detected for {service}"),
                    cpu: self.infra.first().map(|node| node.cpu),
                    memory: self.infra.first().map(|node| node.memory),
                    related_incident: Some(incident_id.clone()),
                });
            }
            OpsEvent::AgentSpawned {
                name,
                role,
                timestamp,
            } => {
                if !self.agents.iter().any(|agent| agent.name == *name) {
                    self.agents.push(Agent {
                        name: name.clone(),
                        role: role.clone(),
                        status: AgentStatus::Waiting,
                        task: "registered; waiting for task assignment".into(),
                    });
                }
                self.active_agents = self
                    .agents
                    .iter()
                    .filter(|agent| {
                        agent.status != AgentStatus::Idle && agent.status != AgentStatus::Failed
                    })
                    .count();
                self.record_timeline(TimelineEvent {
                    id: format!("time-agent-{name}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: name.clone(),
                    summary: format!("registered {:?} agent runtime", role),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::AgentLifecycleChanged {
                agent,
                status,
                task,
                timestamp,
            } => {
                if let Some(existing) = self.agents.iter_mut().find(|item| item.name == *agent) {
                    existing.status = status.clone();
                    existing.task = task.clone();
                }
                if let Some(runtime) = self.runtimes.iter_mut().find(|item| item.agent == *agent) {
                    runtime.heartbeat = timestamp.clone();
                    runtime.notes = task.clone();
                    runtime.status = match status {
                        AgentStatus::Running => RuntimeStatus::Active,
                        AgentStatus::Idle | AgentStatus::Waiting => RuntimeStatus::Local,
                        AgentStatus::Escalated | AgentStatus::Failed => RuntimeStatus::Suspended,
                    };
                }
                self.active_agents = self
                    .agents
                    .iter()
                    .filter(|item| {
                        item.status != AgentStatus::Idle && item.status != AgentStatus::Failed
                    })
                    .count();
                self.record_timeline(TimelineEvent {
                    id: format!("time-agent-lifecycle-{agent}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: agent.clone(),
                    summary: format!("agent lifecycle changed to {:?}", status),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::AgentTelemetryRecorded {
                agent,
                metric,
                value,
                timestamp,
            } => {
                self.record_timeline(TimelineEvent {
                    id: format!("time-agent-telemetry-{agent}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: agent.clone(),
                    summary: format!("{metric}={value}"),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::AiProviderRegistered {
                provider,
                endpoint,
                timestamp,
            } => {
                self.record_timeline(TimelineEvent {
                    id: format!("time-ai-provider-{provider}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: provider.clone(),
                    summary: format!("AI provider registered at {endpoint}"),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::AiProviderLogin {
                kind,
                timestamp,
                ..
            } => {
                self.record_timeline(TimelineEvent {
                    id: format!("time-ai-login-{kind}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: kind.clone(),
                    summary: format!("AI provider {kind} configured via /login"),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::ToolCallRequested {
                id,
                tool,
                timestamp,
                ..
            } => {
                self.record_timeline(TimelineEvent {
                    id: format!("time-tool-request-{id}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: tool.clone(),
                    summary: "tool call requested".into(),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::ToolCallCompleted {
                id,
                tool,
                success,
                timestamp,
                ..
            } => {
                self.record_timeline(TimelineEvent {
                    id: format!("time-tool-complete-{id}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: tool.clone(),
                    summary: format!("tool call completed success={success}"),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::TaskAssigned {
                agent,
                task,
                timestamp,
            } => {
                if let Some(existing) = self.agents.iter_mut().find(|item| item.name == *agent) {
                    existing.status = AgentStatus::Running;
                    existing.task = task.clone();
                }
                if let Some(runtime) = self.runtimes.iter_mut().find(|item| item.agent == *agent) {
                    runtime.status = RuntimeStatus::Active;
                    runtime.heartbeat = timestamp.clone();
                    runtime.notes = task.clone();
                }
                self.record_agent_link(
                    "workflow-engine",
                    agent,
                    "task-assignment",
                    task,
                    74,
                    timestamp,
                );
            }
            OpsEvent::AgentMemoryStored {
                agent,
                key,
                value,
                ..
            } => {
                self.record_timeline(TimelineEvent {
                    id: format!("memory-{agent}-{key}"),
                    timestamp: now_ts(),
                    category: TimelineCategory::Agent,
                    source: agent.clone(),
                    summary: format!("agent memory [{key}]: {value}"),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::PlanCreated {
                planner,
                task,
                sub_tasks,
                timestamp,
            } => {
                for (i, sub) in sub_tasks.iter().enumerate() {
                    self.record_agent_link(planner, sub, "plan-subtask", task, 70 + i as u8, timestamp);
                }
                self.record_timeline(TimelineEvent {
                    id: format!("plan-{planner}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: planner.clone(),
                    summary: format!("plan created for {task} with {} sub-tasks", sub_tasks.len()),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::SubTaskCompleted {
                planner,
                executor,
                sub_task,
                result,
                ..
            } => {
                self.record_agent_link(
                    executor,
                    planner,
                    "subtask-completed",
                    &format!("{sub_task}: {result}"),
                    85,
                    &now_ts(),
                );
            }
            OpsEvent::CommandRequested {
                id,
                command,
                dry_run,
                timestamp,
                reason,
            } => {
                self.executions.push(ExecutionRecord {
                    id: id.clone(),
                    command: command.clone(),
                    status: if *dry_run {
                        "dry-run queued"
                    } else {
                        "running"
                    }
                    .into(),
                    exit_code: None,
                    output_preview: String::new(),
                    timestamp: timestamp.clone(),
                });
                self.record_timeline(TimelineEvent {
                    id: format!("time-{id}"),
                    timestamp: timestamp.clone(),
                    category: if *dry_run {
                        TimelineCategory::Recovery
                    } else {
                        TimelineCategory::Log
                    },
                    source: command.clone(),
                    summary: reason.clone(),
                    cpu: self.infra.first().map(|node| node.cpu),
                    memory: self.infra.first().map(|node| node.memory),
                    related_incident: self.incidents.first().map(|incident| incident.id.clone()),
                });
            }
            OpsEvent::CommandOutput {
                id, stream, line, ..
            } => {
                self.logs.push(format!("[{stream}] {line}"));
                if let Some(existing) = self.executions.iter_mut().find(|item| item.id == *id) {
                    existing.output_preview = trim_preview(format!(
                        "{}{}{}",
                        existing.output_preview,
                        if existing.output_preview.is_empty() {
                            ""
                        } else {
                            "\n"
                        },
                        line
                    ));
                }
                self.logs
                    .push(format!("{} {} {}", stream.to_uppercase(), id, line));
            }
            OpsEvent::CommandExecuted {
                id,
                command,
                success,
                exit_code,
                stdout,
                stderr,
                timestamp,
                ..
            } => {
                if let Some(existing) = self.executions.iter_mut().find(|item| item.id == *id) {
                    existing.status = if *success { "completed" } else { "failed" }.into();
                    existing.exit_code = *exit_code;
                    existing.output_preview = trim_preview(if stdout.is_empty() {
                        stderr.clone()
                    } else {
                        stdout.clone()
                    });
                } else {
                    self.executions.push(ExecutionRecord {
                        id: id.clone(),
                        command: command.clone(),
                        status: if *success { "completed" } else { "failed" }.into(),
                        exit_code: *exit_code,
                        output_preview: trim_preview(if stdout.is_empty() {
                            stderr.clone()
                        } else {
                            stdout.clone()
                        }),
                        timestamp: timestamp.clone(),
                    });
                }
            }
            OpsEvent::ResearchCompleted {
                topic,
                conclusion,
                confidence,
                timestamp,
            } => {
                self.reports.push(format!(
                    "{}: {} (confidence {}%)",
                    topic, conclusion, confidence
                ));
                self.refresh_research_profile(topic, conclusion, *confidence, timestamp);
                self.record_timeline(TimelineEvent {
                    id: format!("time-research-{topic}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: topic.clone(),
                    summary: conclusion.clone(),
                    cpu: None,
                    memory: None,
                    related_incident: Some(topic.clone()),
                });
            }
            OpsEvent::WorkflowAdvanced {
                id,
                stage,
                progress,
                ..
            } => {
                if let Some(workflow) = self.workflows.iter_mut().find(|item| item.id == *id) {
                    workflow.stage = stage.clone();
                    workflow.progress = *progress;
                } else {
                    self.workflows.push(Workflow {
                        id: id.clone(),
                        name: "tier1 incident response".into(),
                        owner: "workflow-engine".into(),
                        stage: stage.clone(),
                        progress: *progress,
                    });
                }
            }
            OpsEvent::ExplainabilityRecorded { record } => {
                self.explainability.push(record.clone());
                self.refresh_research_from_explainability(record);
            }
            OpsEvent::AgentMessageRecorded {
                from,
                to,
                protocol,
                message,
                confidence,
                timestamp,
            } => self.record_agent_link(from, to, protocol, message, *confidence, timestamp),
            OpsEvent::TimelineRecorded { event } => {
                self.record_timeline(event.clone());
            }
            OpsEvent::RecoveryProposed { action } => {
                self.recovery_actions.push(action.clone());
                self.ensure_knowledge_node(&action.target, action.target.clone(), "service", 88);
                self.ensure_knowledge_node(
                    &action.name,
                    action.name.clone(),
                    "recovery-action",
                    79,
                );
                self.record_knowledge_edge(
                    &action.target,
                    "proposed-remediation",
                    &action.name,
                    84,
                    &action.timestamp,
                );
                self.record_timeline(TimelineEvent {
                    id: format!("time-{}", action.id),
                    timestamp: action.timestamp.clone(),
                    category: TimelineCategory::Recovery,
                    source: action.target.clone(),
                    summary: format!("proposed recovery: {}", action.name),
                    cpu: self.infra.first().map(|node| node.cpu),
                    memory: self.infra.first().map(|node| node.memory),
                    related_incident: self.incidents.first().map(|incident| incident.id.clone()),
                });
            }
            OpsEvent::RecoveryApproved {
                action_id,
                role,
                timestamp,
            } => {
                if let Some(action) = self
                    .recovery_actions
                    .iter_mut()
                    .find(|item| item.id == *action_id)
                {
                    if role.can_approve_recovery() {
                        action.status = RecoveryStatus::Approved;
                        action.approved_by = Some(role.clone());
                    } else {
                        action.status = RecoveryStatus::Rejected;
                    }
                    action.timestamp = timestamp.clone();
                }
            }
            OpsEvent::RoleChanged { role, .. } => {
                self.current_role = role.clone();
            }
            OpsEvent::ReplayStarted { .. } => {
                self.replay.active = true;
                self.replay.position = 0;
                self.replay.total = self.events.len();
                self.replay.last_event = None;
            }
            OpsEvent::ReplayStepped { position, .. } => {
                self.replay.active = true;
                self.replay.total = self.events.len();
                self.replay.position = (*position).min(self.replay.total);
                self.replay.last_event = self
                    .events
                    .get(self.replay.position.saturating_sub(1))
                    .map(|event| format!("{event:?}"));
            }
            OpsEvent::UserCommandEntered { .. } => {}
            OpsEvent::MetricsSampled { cpu, memory, .. } => {
                if let Some(node) = self.infra.first_mut() {
                    node.cpu = *cpu;
                    node.memory = *memory;
                    node.health = 100u8.saturating_sub(cpu.saturating_sub(75));
                }
            }
            OpsEvent::InfrastructureSnapshotRecorded {
                source,
                nodes,
                timestamp,
            } => {
                self.infra = nodes.clone();
                self.health = self
                    .infra
                    .iter()
                    .map(|node| node.health as u16)
                    .sum::<u16>()
                    .checked_div(self.infra.len() as u16)
                    .unwrap_or(0) as u8;
                self.record_timeline(TimelineEvent {
                    id: format!("time-infra-snapshot-{source}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Metric,
                    source: source.clone(),
                    summary: format!("recorded {} infrastructure nodes", nodes.len()),
                    cpu: self.infra.first().map(|node| node.cpu),
                    memory: self.infra.first().map(|node| node.memory),
                    related_incident: None,
                });
            }
            OpsEvent::WorkflowDefinitionLoaded { definition } => {
                if !self.workflows.iter().any(|item| item.id == definition.id) {
                    self.workflows.push(Workflow {
                        id: definition.id.clone(),
                        name: definition.name.clone(),
                        owner: "dag-runtime".into(),
                        stage: format!("loaded entrypoint {}", definition.entrypoint),
                        progress: 0,
                    });
                }
                self.record_timeline(TimelineEvent {
                    id: format!("time-workflow-definition-{}", definition.id),
                    timestamp: definition.timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: definition.id.clone(),
                    summary: format!("loaded DAG workflow with {} nodes", definition.node_count),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::WorkflowNodeCompleted {
                workflow_id,
                node_id,
                timestamp,
            } => {
                if let Some(workflow) = self
                    .workflows
                    .iter_mut()
                    .find(|item| item.id == *workflow_id)
                {
                    workflow.stage = format!("completed node {node_id}");
                }
                self.record_timeline(TimelineEvent {
                    id: format!("time-workflow-node-{workflow_id}-{node_id}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: workflow_id.clone(),
                    summary: format!("workflow node completed: {node_id}"),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::ResearchConfidenceUpdated { profile } => {
                self.research_profile = profile.clone();
            }
            OpsEvent::PluginRegistered { plugin } => {
                if let Some(existing) = self
                    .plugins
                    .iter_mut()
                    .find(|item| item.name == plugin.name)
                {
                    *existing = plugin.clone();
                } else {
                    self.plugins.push(plugin.clone());
                }
            }
            OpsEvent::PluginStatusChanged {
                name,
                status,
                timestamp: _,
            } => {
                if let Some(existing) = self.plugins.iter_mut().find(|item| item.name == *name) {
                    existing.status = status.clone();
                }
            }
            OpsEvent::RuntimeUpdated { runtime } => {
                if let Some(existing) = self
                    .runtimes
                    .iter_mut()
                    .find(|item| item.agent == runtime.agent)
                {
                    *existing = runtime.clone();
                } else {
                    self.runtimes.push(runtime.clone());
                }
            }
            OpsEvent::KnowledgeNodeEnsured { node } => {
                self.ensure_knowledge_node(
                    &node.id,
                    node.label.clone(),
                    &node.kind,
                    node.confidence,
                );
            }
            OpsEvent::KnowledgeEdgeAdded { edge } => {
                self.record_knowledge_edge(
                    &edge.from,
                    &edge.relation,
                    &edge.to,
                    edge.weight,
                    &edge.timestamp,
                );
            }
            OpsEvent::SandboxPolicyUpdated { policy } => {
                self.sandbox_policy = policy.clone();
            }
        }

        let log_limit = crate::constants::log_limit();
        if self.logs.len() > log_limit {
            let drop_count = self.logs.len() - log_limit;
            self.logs.drain(0..drop_count);
        }
        self.events.push(event);
        let event_limit = crate::constants::event_limit();
        if self.events.len() > event_limit {
            let drop_count = self.events.len() - event_limit;
            self.events.drain(0..drop_count);
        }
        let execution_limit = crate::constants::execution_limit();
        if self.executions.len() > execution_limit {
            let drop_count = self.executions.len() - execution_limit;
            self.executions.drain(0..drop_count);
        }
        let explainability_limit = crate::constants::explainability_limit();
        if self.explainability.len() > explainability_limit {
            let drop_count = self.explainability.len() - explainability_limit;
            self.explainability.drain(0..drop_count);
        }
        let coordination_limit = crate::constants::coordination_limit();
        if self.coordination_links.len() > coordination_limit {
            let drop_count = self.coordination_links.len() - coordination_limit;
            self.coordination_links.drain(0..drop_count);
        }
        let timeline_limit = crate::constants::timeline_limit();
        if self.timeline.len() > timeline_limit {
            let drop_count = self.timeline.len() - timeline_limit;
            self.timeline.drain(0..drop_count);
        }
        let recovery_limit = crate::constants::recovery_limit();
        if self.recovery_actions.len() > recovery_limit {
            let drop_count = self.recovery_actions.len() - recovery_limit;
            self.recovery_actions.drain(0..drop_count);
        }
    }

    fn record_agent_link(
        &mut self,
        from: &str,
        to: &str,
        protocol: &str,
        message: &str,
        confidence: u8,
        timestamp: &str,
    ) {
        if let Some(existing) = self.coordination_links.iter_mut().find(|link| {
            link.from == from && link.to == to && link.protocol == protocol
        }) {
            existing.message = message.into();
            existing.confidence = confidence;
            existing.timestamp = timestamp.into();
        } else {
            self.coordination_links.push(AgentLink {
                from: from.into(),
                to: to.into(),
                protocol: protocol.into(),
                message: message.into(),
                confidence,
                timestamp: timestamp.into(),
            });
        }
    }

    fn record_timeline(&mut self, event: TimelineEvent) {
        self.timeline.push(event);
    }

    fn ensure_knowledge_node(&mut self, id: &str, label: String, kind: &str, confidence: u8) {
        if let Some(existing) = self.knowledge_nodes.iter_mut().find(|node| node.id == id) {
            existing.label = label;
            existing.kind = kind.into();
            existing.confidence = confidence;
        } else {
            self.knowledge_nodes.push(KnowledgeNode {
                id: id.into(),
                label,
                kind: kind.into(),
                confidence,
            });
        }
    }

    fn record_knowledge_edge(
        &mut self,
        from: &str,
        relation: &str,
        to: &str,
        weight: u8,
        timestamp: &str,
    ) {
        if self
            .knowledge_edges
            .iter()
            .any(|edge| edge.from == from && edge.relation == relation && edge.to == to)
        {
            return;
        }
        self.knowledge_edges.push(KnowledgeEdge {
            from: from.into(),
            relation: relation.into(),
            to: to.into(),
            weight,
            timestamp: timestamp.into(),
        });
    }

    fn refresh_research_profile(
        &mut self,
        topic: &str,
        conclusion: &str,
        confidence: u8,
        timestamp: &str,
    ) {
        let mut signals = vec![
            ResearchSignal {
                source: "explainability".into(),
                evidence: conclusion.into(),
                reliability: confidence,
                contradiction: false,
            },
            ResearchSignal {
                source: "incidents".into(),
                evidence: format!("{} active incidents", self.incidents.len()),
                reliability: 65,
                contradiction: self
                    .incidents
                    .iter()
                    .any(|incident| incident.status == "detected"),
            },
            ResearchSignal {
                source: "timeline".into(),
                evidence: format!("{} correlated timeline events", self.timeline.len()),
                reliability: 74,
                contradiction: false,
            },
        ];
        if self
            .knowledge_edges
            .iter()
            .any(|edge| edge.relation == "correlates-with" && edge.from == "metric-p95")
        {
            signals.push(ResearchSignal {
                source: "knowledge-graph".into(),
                evidence: "latency metric correlates with edge-nginx".into(),
                reliability: 88,
                contradiction: false,
            });
        }
        let contradiction_count =
            signals.iter().filter(|signal| signal.contradiction).count() as u8;
        let reliability_total: u32 = signals.iter().map(|signal| signal.reliability as u32).sum();
        let evidence_reliability = (reliability_total / signals.len() as u32) as u8;
        let mut ranking = confidence
            .saturating_add(evidence_reliability / 2)
            .saturating_sub(contradiction_count.saturating_mul(5));
        ranking = ranking.clamp(0, 100);
        self.research_profile = ResearchConfidenceProfile {
            subject: topic.into(),
            evidence_reliability,
            contradiction_count,
            ranking,
            last_reviewed: timestamp.into(),
            signals,
        };
    }

    fn refresh_research_from_explainability(&mut self, record: &ExplainabilityRecord) {
        self.research_profile.subject = record.action.clone();
        self.research_profile.last_reviewed = record.timestamp.clone();
        self.research_profile.evidence_reliability = record.confidence;
        self.research_profile.ranking = record
            .confidence
            .saturating_sub(self.research_profile.contradiction_count.saturating_mul(3));
        self.research_profile.signals.push(ResearchSignal {
            source: "explainability-ledger".into(),
            evidence: record.why.clone(),
            reliability: record.confidence,
            contradiction: false,
        });
        if self.research_profile.signals.len() > 12 {
            let drop_count = self.research_profile.signals.len() - 12;
            self.research_profile.signals.drain(0..drop_count);
        }
    }
}
