use serde::{Deserialize, Serialize};

use crate::{
    security::redact_sensitive,
    utils::{now_ts, trim_preview},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Agent {
    pub(crate) name: String,
    pub(crate) role: AgentRole,
    pub(crate) status: AgentStatus,
    pub(crate) task: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentProcess {
    pub(crate) pid: u32,
    pub(crate) agent: String,
    pub(crate) parent: Option<String>,
    pub(crate) role: AgentRole,
    pub(crate) status: AgentStatus,
    pub(crate) runtime: String,
    pub(crate) task: String,
    pub(crate) memory_scope: String,
    pub(crate) tool_calls: u64,
    pub(crate) model_tokens: u64,
    pub(crate) events_emitted: u64,
    pub(crate) started_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SyscallRecord {
    pub(crate) id: String,
    pub(crate) agent: String,
    pub(crate) call: String,
    pub(crate) capability: String,
    pub(crate) allowed: bool,
    pub(crate) reason: String,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ConversationMessage {
    pub(crate) id: String,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) model: String,
    pub(crate) confidence: u8,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct KernelTask {
    pub(crate) id: String,
    pub(crate) owner: String,
    pub(crate) description: String,
    pub(crate) priority: u8,
    pub(crate) status: String,
    pub(crate) attempts: u8,
    pub(crate) queued_at: String,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkspaceArtifact {
    pub(crate) id: String,
    pub(crate) owner: String,
    pub(crate) path: String,
    pub(crate) kind: String,
    pub(crate) bytes: usize,
    pub(crate) immutable: bool,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SystemService {
    pub(crate) name: String,
    pub(crate) status: String,
    pub(crate) health: u8,
    pub(crate) started_at: String,
    pub(crate) last_heartbeat: String,
    pub(crate) notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgenticApp {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) status: String,
    pub(crate) permissions: Vec<String>,
    pub(crate) commands: Vec<String>,
    pub(crate) installed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ResourceQuota {
    pub(crate) subject: String,
    pub(crate) tool_call_limit: u64,
    pub(crate) model_token_limit: u64,
    pub(crate) memory_write_limit: u64,
    pub(crate) event_limit: u64,
    pub(crate) tool_calls_used: u64,
    pub(crate) model_tokens_used: u64,
    pub(crate) memory_writes_used: u64,
    pub(crate) events_used: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IpcMessage {
    pub(crate) id: String,
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) topic: String,
    pub(crate) payload: String,
    pub(crate) delivered: bool,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PolicyGrant {
    pub(crate) id: String,
    pub(crate) subject: String,
    pub(crate) capability: String,
    pub(crate) active: bool,
    pub(crate) reason: String,
    pub(crate) granted_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentMemoryEntry {
    pub(crate) id: String,
    pub(crate) scope: String,
    pub(crate) kind: String,
    pub(crate) key: String,
    pub(crate) preview: String,
    pub(crate) provenance: String,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AppPackage {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) signed: bool,
    pub(crate) dependencies: Vec<String>,
    pub(crate) source: String,
    pub(crate) installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SupervisorEvent {
    pub(crate) id: String,
    pub(crate) subject: String,
    pub(crate) action: String,
    pub(crate) reason: String,
    pub(crate) restarts: u8,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct BootConfig {
    pub(crate) profile: String,
    pub(crate) services: Vec<String>,
    pub(crate) mounted_workspaces: Vec<String>,
    pub(crate) default_policy: String,
    pub(crate) initialized_at: String,
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
    Completed,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelHealthSnapshot {
    pub(crate) model: String,
    pub(crate) installed: bool,
    pub(crate) online: bool,
    pub(crate) size_bytes: Option<u64>,
    pub(crate) digest: Option<String>,
    pub(crate) modified_at: Option<String>,
    pub(crate) last_checked: String,
    pub(crate) notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct TokenUsageSnapshot {
    pub(crate) requests: u64,
    pub(crate) prompt_tokens: u64,
    pub(crate) completion_tokens: u64,
    pub(crate) total_tokens: u64,
    pub(crate) retries: u64,
    pub(crate) errors: u64,
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
    ReasoningChunkRecorded {
        agent: String,
        model: String,
        chunk: String,
        timestamp: String,
    },
    TokenUsageRecorded {
        agent: String,
        model: String,
        prompt_tokens: u64,
        completion_tokens: u64,
        total_tokens: u64,
        timestamp: String,
    },
    ModelHealthUpdated {
        models: Vec<ModelHealthSnapshot>,
        timestamp: String,
    },
    NotificationRaised {
        level: String,
        message: String,
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
    AgentProcessUpdated {
        process: AgentProcess,
    },
    SyscallRecorded {
        record: SyscallRecord,
    },
    ConversationMessageRecorded {
        message: ConversationMessage,
    },
    KernelTaskScheduled {
        task: KernelTask,
    },
    WorkspaceArtifactRecorded {
        artifact: WorkspaceArtifact,
    },
    SystemServiceUpdated {
        service: SystemService,
    },
    AgenticAppInstalled {
        app: AgenticApp,
    },
    ResourceQuotaUpdated {
        quota: ResourceQuota,
    },
    IpcMessageRecorded {
        message: IpcMessage,
    },
    PolicyGrantUpdated {
        grant: PolicyGrant,
    },
    AgentMemoryEntryRecorded {
        entry: AgentMemoryEntry,
    },
    AppPackageImported {
        package: AppPackage,
    },
    SupervisorEventRecorded {
        event: SupervisorEvent,
    },
    BootCompleted {
        config: BootConfig,
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
    pub(crate) model_health: Vec<ModelHealthSnapshot>,
    pub(crate) token_usage: TokenUsageSnapshot,
    pub(crate) process_table: Vec<AgentProcess>,
    pub(crate) syscalls: Vec<SyscallRecord>,
    pub(crate) conversation: Vec<ConversationMessage>,
    pub(crate) kernel_tasks: Vec<KernelTask>,
    pub(crate) workspace_artifacts: Vec<WorkspaceArtifact>,
    pub(crate) system_services: Vec<SystemService>,
    pub(crate) agentic_apps: Vec<AgenticApp>,
    pub(crate) resource_quotas: Vec<ResourceQuota>,
    pub(crate) ipc_messages: Vec<IpcMessage>,
    pub(crate) policy_grants: Vec<PolicyGrant>,
    pub(crate) agent_memory: Vec<AgentMemoryEntry>,
    pub(crate) app_packages: Vec<AppPackage>,
    pub(crate) supervisor_events: Vec<SupervisorEvent>,
    pub(crate) boot_config: BootConfig,
    pub(crate) reasoning_stream: Vec<String>,
    pub(crate) notifications: Vec<String>,
}

impl OpsState {
    pub(crate) fn empty() -> Self {
        Self {
            workspace: "octobot-ops".into(),
            environment: "prod / us-east".into(),
            uptime_secs: 0,
            health: 100,
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
            model_health: Vec::new(),
            token_usage: TokenUsageSnapshot::default(),
            process_table: Vec::new(),
            syscalls: Vec::new(),
            conversation: Vec::new(),
            kernel_tasks: Vec::new(),
            workspace_artifacts: Vec::new(),
            system_services: default_system_services(),
            agentic_apps: Vec::new(),
            resource_quotas: Vec::new(),
            ipc_messages: Vec::new(),
            policy_grants: Vec::new(),
            agent_memory: Vec::new(),
            app_packages: Vec::new(),
            supervisor_events: Vec::new(),
            boot_config: BootConfig {
                profile: "local-agentic-os".into(),
                services: vec![
                    "scheduler".into(),
                    "event-bus".into(),
                    "memory".into(),
                    "policy".into(),
                    "workflow".into(),
                    "apps".into(),
                    "observability".into(),
                    "security".into(),
                    "persistence".into(),
                ],
                mounted_workspaces: vec!["agent://workspace".into(), "agent://reports".into()],
                default_policy: "read-only allowlist".into(),
                initialized_at: now_ts(),
            },
            reasoning_stream: Vec::new(),
            notifications: Vec::new(),
        }
    }

    pub(crate) fn seed() -> Self {
        Self::empty()
    }

    pub(crate) fn tick(&mut self) {
        self.uptime_secs += 1;
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
                self.ensure_process(
                    name,
                    None,
                    role.clone(),
                    AgentStatus::Waiting,
                    "registered; waiting for task assignment",
                    timestamp,
                );
                self.ensure_quota(name);
                self.active_agents = self
                    .agents
                    .iter()
                    .filter(|agent| {
                        !matches!(
                            agent.status,
                            AgentStatus::Idle | AgentStatus::Completed | AgentStatus::Failed
                        )
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
                if let Some(process) = self
                    .process_table
                    .iter_mut()
                    .find(|item| item.agent == *agent)
                {
                    process.status = status.clone();
                    process.task = task.clone();
                    process.updated_at = timestamp.clone();
                }
                if matches!(status, AgentStatus::Failed) {
                    self.supervisor_events.push(SupervisorEvent {
                        id: format!("supervisor-{agent}-{timestamp}"),
                        subject: agent.clone(),
                        action: "isolate".into(),
                        reason: task.clone(),
                        restarts: 0,
                        timestamp: timestamp.clone(),
                    });
                }
                if let Some(runtime) = self.runtimes.iter_mut().find(|item| item.agent == *agent) {
                    runtime.heartbeat = timestamp.clone();
                    runtime.notes = task.clone();
                    runtime.status = match status {
                        AgentStatus::Running => RuntimeStatus::Active,
                        AgentStatus::Idle | AgentStatus::Waiting | AgentStatus::Completed => {
                            RuntimeStatus::Local
                        }
                        AgentStatus::Escalated | AgentStatus::Failed => RuntimeStatus::Suspended,
                    };
                }
                self.active_agents = self
                    .agents
                    .iter()
                    .filter(|item| {
                        !matches!(
                            item.status,
                            AgentStatus::Idle | AgentStatus::Completed | AgentStatus::Failed
                        )
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
                kind, timestamp, ..
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
            OpsEvent::ReasoningChunkRecorded {
                agent,
                model,
                chunk,
                timestamp,
            } => {
                self.reasoning_stream
                    .push(format!("{agent} [{model}]: {chunk}"));
                if self.reasoning_stream.len() > 120 {
                    let drop_count = self.reasoning_stream.len() - 120;
                    self.reasoning_stream.drain(0..drop_count);
                }
                self.record_timeline(TimelineEvent {
                    id: format!("time-reasoning-{agent}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: agent.clone(),
                    summary: chunk.clone(),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::TokenUsageRecorded {
                agent,
                model,
                prompt_tokens,
                completion_tokens,
                total_tokens,
                timestamp,
            } => {
                self.token_usage.requests += 1;
                self.token_usage.prompt_tokens += *prompt_tokens;
                self.token_usage.completion_tokens += *completion_tokens;
                self.token_usage.total_tokens += *total_tokens;
                if let Some(process) = self
                    .process_table
                    .iter_mut()
                    .find(|item| item.agent == *agent)
                {
                    process.model_tokens = process.model_tokens.saturating_add(*total_tokens);
                    process.updated_at = timestamp.clone();
                }
                self.record_timeline(TimelineEvent {
                    id: format!("time-usage-{agent}-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: agent.clone(),
                    summary: format!("{model}: {prompt_tokens}+{completion_tokens} tokens"),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::ModelHealthUpdated { models, timestamp } => {
                self.model_health = models.clone();
                self.record_timeline(TimelineEvent {
                    id: format!("time-model-health-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: "ollama".into(),
                    summary: format!("{} model health records updated", models.len()),
                    cpu: None,
                    memory: None,
                    related_incident: None,
                });
            }
            OpsEvent::NotificationRaised {
                level,
                message,
                timestamp,
            } => {
                self.notifications.push(format!("[{level}] {message}"));
                if self.notifications.len() > 80 {
                    let drop_count = self.notifications.len() - 80;
                    self.notifications.drain(0..drop_count);
                }
                self.record_timeline(TimelineEvent {
                    id: format!("time-notify-{timestamp}"),
                    timestamp: timestamp.clone(),
                    category: TimelineCategory::Agent,
                    source: level.clone(),
                    summary: message.clone(),
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
                if let Some(process) = self
                    .process_table
                    .iter_mut()
                    .find(|process| tool.contains(&process.agent))
                {
                    process.tool_calls = process.tool_calls.saturating_add(1);
                    process.updated_at = timestamp.clone();
                }
                self.record_syscall(SyscallRecord {
                    id: format!("sys-tool-{id}"),
                    agent: "agent-runtime".into(),
                    call: "plugin.call".into(),
                    capability: "plugin:call".into(),
                    allowed: true,
                    reason: tool.clone(),
                    timestamp: timestamp.clone(),
                });
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
                if let Some(process) = self
                    .process_table
                    .iter_mut()
                    .find(|item| item.agent == *agent)
                {
                    process.status = AgentStatus::Running;
                    process.task = task.clone();
                    process.events_emitted = process.events_emitted.saturating_add(1);
                    process.updated_at = timestamp.clone();
                }
                if let Some(quota) = self
                    .resource_quotas
                    .iter_mut()
                    .find(|q| q.subject == *agent)
                {
                    quota.events_used = quota.events_used.saturating_add(1);
                }
                self.kernel_tasks.push(KernelTask {
                    id: format!("ktask-{agent}-{timestamp}"),
                    owner: agent.clone(),
                    description: task.clone(),
                    priority: 50,
                    status: "running".into(),
                    attempts: 1,
                    queued_at: timestamp.clone(),
                    updated_at: timestamp.clone(),
                });
                if let Some(runtime) = self.runtimes.iter_mut().find(|item| item.agent == *agent) {
                    runtime.status = RuntimeStatus::Active;
                    runtime.heartbeat = timestamp.clone();
                    runtime.notes = task.clone();
                }
                let confidence = dynamic_assignment_confidence(self);
                self.record_agent_link(
                    "workflow-engine",
                    agent,
                    "task-assignment",
                    task,
                    confidence,
                    timestamp,
                );
            }
            OpsEvent::AgentMemoryStored {
                agent, key, value, ..
            } => {
                self.record_syscall(SyscallRecord {
                    id: format!("sys-memory-{agent}-{key}"),
                    agent: agent.clone(),
                    call: "memory.write".into(),
                    capability: "memory:write".into(),
                    allowed: true,
                    reason: "agent memory write recorded".into(),
                    timestamp: now_ts(),
                });
                self.agent_memory.push(AgentMemoryEntry {
                    id: format!("mem-{agent}-{key}"),
                    scope: format!("agent://{agent}/memory"),
                    kind: "episodic".into(),
                    key: key.clone(),
                    preview: trim_preview(value.clone()),
                    provenance: "AgentMemoryStored".into(),
                    created_at: now_ts(),
                });
                if let Some(quota) = self
                    .resource_quotas
                    .iter_mut()
                    .find(|q| q.subject == *agent)
                {
                    quota.memory_writes_used = quota.memory_writes_used.saturating_add(1);
                }
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
                    self.record_agent_link(
                        planner,
                        sub,
                        "plan-subtask",
                        task,
                        70 + i as u8,
                        timestamp,
                    );
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
                self.ipc_messages.push(IpcMessage {
                    id: format!("ipc-{executor}-{planner}"),
                    from: executor.clone(),
                    to: planner.clone(),
                    topic: "subtask-completed".into(),
                    payload: format!("{sub_task}: {result}"),
                    delivered: true,
                    timestamp: now_ts(),
                });
            }
            OpsEvent::CommandRequested {
                id,
                command,
                dry_run,
                timestamp,
                reason,
            } => {
                self.record_syscall(SyscallRecord {
                    id: format!("sys-{id}"),
                    agent: "operator".into(),
                    call: "shell.exec".into(),
                    capability: "cmd:readonly".into(),
                    allowed: true,
                    reason: command.clone(),
                    timestamp: timestamp.clone(),
                });
                if let Some(quota) = self
                    .resource_quotas
                    .iter_mut()
                    .find(|q| q.subject == "operator")
                {
                    quota.events_used = quota.events_used.saturating_add(1);
                }
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
                let line = redact_sensitive(line);
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
                self.workspace_artifacts.push(WorkspaceArtifact {
                    id: format!("artifact-{topic}-{timestamp}"),
                    owner: "reports".into(),
                    path: format!("agent://reports/{topic}.md"),
                    kind: "report".into(),
                    bytes: conclusion.len(),
                    immutable: false,
                    created_at: timestamp.clone(),
                });
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
                self.record_metric_sample(((*cpu as u64) + (*memory as u64)) / 2);
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
                if !self.infra.is_empty() {
                    let cpu_avg = self.infra.iter().map(|node| node.cpu as u64).sum::<u64>()
                        / self.infra.len() as u64;
                    let mem_avg = self
                        .infra
                        .iter()
                        .map(|node| node.memory as u64)
                        .sum::<u64>()
                        / self.infra.len() as u64;
                    self.record_metric_sample((cpu_avg + mem_avg) / 2);
                }
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
                let permissions =
                    crate::security::PluginSecurity::permissions_for_kind(&plugin.kind)
                        .iter()
                        .map(|scope| (*scope).to_string())
                        .collect::<Vec<_>>();
                self.upsert_agentic_app(AgenticApp {
                    id: format!("app-{}", plugin.name),
                    name: plugin.name.clone(),
                    version: plugin.version.clone(),
                    status: format!("{:?}", plugin.status),
                    permissions,
                    commands: vec![format!("plugin enable {}", plugin.name)],
                    installed_at: now_ts(),
                });
            }
            OpsEvent::PluginStatusChanged {
                name,
                status,
                timestamp: _,
            } => {
                if let Some(existing) = self.plugins.iter_mut().find(|item| item.name == *name) {
                    existing.status = status.clone();
                }
                if let Some(app) = self.agentic_apps.iter_mut().find(|item| item.name == *name) {
                    app.status = format!("{status:?}");
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
                if let Some(process) = self
                    .process_table
                    .iter_mut()
                    .find(|item| item.agent == runtime.agent)
                {
                    process.runtime = runtime.endpoint.clone();
                    process.updated_at = runtime.heartbeat.clone();
                }
            }
            OpsEvent::AgentProcessUpdated { process } => {
                if let Some(existing) = self
                    .process_table
                    .iter_mut()
                    .find(|item| item.agent == process.agent)
                {
                    *existing = process.clone();
                } else {
                    self.process_table.push(process.clone());
                }
            }
            OpsEvent::SyscallRecorded { record } => {
                self.record_syscall(record.clone());
            }
            OpsEvent::ConversationMessageRecorded { message } => {
                self.conversation.push(message.clone());
                if self.conversation.len() > 120 {
                    let drop_count = self.conversation.len() - 120;
                    self.conversation.drain(0..drop_count);
                }
            }
            OpsEvent::KernelTaskScheduled { task } => {
                if let Some(existing) = self.kernel_tasks.iter_mut().find(|item| item.id == task.id)
                {
                    *existing = task.clone();
                } else {
                    self.kernel_tasks.push(task.clone());
                }
            }
            OpsEvent::WorkspaceArtifactRecorded { artifact } => {
                self.workspace_artifacts.push(artifact.clone());
            }
            OpsEvent::SystemServiceUpdated { service } => {
                if let Some(existing) = self
                    .system_services
                    .iter_mut()
                    .find(|item| item.name == service.name)
                {
                    *existing = service.clone();
                } else {
                    self.system_services.push(service.clone());
                }
            }
            OpsEvent::AgenticAppInstalled { app } => {
                self.upsert_agentic_app(app.clone());
            }
            OpsEvent::ResourceQuotaUpdated { quota } => {
                if let Some(existing) = self
                    .resource_quotas
                    .iter_mut()
                    .find(|item| item.subject == quota.subject)
                {
                    *existing = quota.clone();
                } else {
                    self.resource_quotas.push(quota.clone());
                }
            }
            OpsEvent::IpcMessageRecorded { message } => {
                self.ipc_messages.push(message.clone());
            }
            OpsEvent::PolicyGrantUpdated { grant } => {
                if let Some(existing) = self
                    .policy_grants
                    .iter_mut()
                    .find(|item| item.id == grant.id)
                {
                    *existing = grant.clone();
                } else {
                    self.policy_grants.push(grant.clone());
                }
            }
            OpsEvent::AgentMemoryEntryRecorded { entry } => {
                self.agent_memory.push(entry.clone());
                self.research_profile.subject = entry.key.clone();
                self.research_profile.last_reviewed = entry.created_at.clone();
                self.research_profile.signals.push(ResearchSignal {
                    source: format!("agent-memory:{}", entry.scope),
                    evidence: entry.preview.clone(),
                    reliability: 82,
                    contradiction: false,
                });
                let reliability_total: u32 = self
                    .research_profile
                    .signals
                    .iter()
                    .map(|signal| signal.reliability as u32)
                    .sum();
                self.research_profile.evidence_reliability =
                    (reliability_total / self.research_profile.signals.len() as u32) as u8;
                self.research_profile.ranking = self
                    .research_profile
                    .evidence_reliability
                    .saturating_sub(self.research_profile.contradiction_count.saturating_mul(3));
                if self.research_profile.signals.len() > 12 {
                    let drop_count = self.research_profile.signals.len() - 12;
                    self.research_profile.signals.drain(0..drop_count);
                }
            }
            OpsEvent::AppPackageImported { package } => {
                if let Some(existing) = self
                    .app_packages
                    .iter_mut()
                    .find(|item| item.name == package.name && item.version == package.version)
                {
                    *existing = package.clone();
                } else {
                    self.app_packages.push(package.clone());
                }
            }
            OpsEvent::SupervisorEventRecorded { event } => {
                self.supervisor_events.push(event.clone());
            }
            OpsEvent::BootCompleted { config } => {
                self.boot_config = config.clone();
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
        let syscall_limit = crate::constants::syscall_limit();
        if self.syscalls.len() > syscall_limit {
            let drop_count = self.syscalls.len() - syscall_limit;
            self.syscalls.drain(0..drop_count);
        }
    }

    fn ensure_quota(&mut self, subject: &str) {
        if self
            .resource_quotas
            .iter()
            .any(|quota| quota.subject == subject)
        {
            return;
        }
        self.resource_quotas.push(ResourceQuota {
            subject: subject.into(),
            tool_call_limit: 100,
            model_token_limit: 100_000,
            memory_write_limit: 1_000,
            event_limit: 1_000,
            tool_calls_used: 0,
            model_tokens_used: 0,
            memory_writes_used: 0,
            events_used: 0,
        });
    }

    fn upsert_agentic_app(&mut self, app: AgenticApp) {
        if let Some(existing) = self
            .agentic_apps
            .iter_mut()
            .find(|item| item.name == app.name)
        {
            *existing = app;
        } else {
            self.agentic_apps.push(app);
        }
    }

    fn ensure_process(
        &mut self,
        agent: &str,
        parent: Option<String>,
        role: AgentRole,
        status: AgentStatus,
        task: &str,
        timestamp: &str,
    ) {
        if self
            .process_table
            .iter()
            .any(|process| process.agent == agent)
        {
            return;
        }
        let pid = self
            .process_table
            .iter()
            .map(|process| process.pid)
            .max()
            .unwrap_or(999)
            + 1;
        self.process_table.push(AgentProcess {
            pid,
            agent: agent.into(),
            parent,
            role,
            status,
            runtime: format!("local://agent/{agent}"),
            task: task.into(),
            memory_scope: format!("agent://{agent}/memory"),
            tool_calls: 0,
            model_tokens: 0,
            events_emitted: 1,
            started_at: timestamp.into(),
            updated_at: timestamp.into(),
        });
    }

    fn record_syscall(&mut self, record: SyscallRecord) {
        self.syscalls.push(record);
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
        if let Some(existing) = self
            .coordination_links
            .iter_mut()
            .find(|link| link.from == from && link.to == to && link.protocol == protocol)
        {
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

    fn record_metric_sample(&mut self, value: u64) {
        self.metrics.push(value.min(100));
        if self.metrics.len() > 30 {
            self.metrics.remove(0);
        }
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

/// Compute a dynamic assignment confidence score based on current state.
/// Higher values mean the system is more confident in task assignments.
pub(crate) fn dynamic_assignment_confidence(state: &OpsState) -> u8 {
    let health_factor = state.health as u16;
    let agent_factor = (state.active_agents.min(10) as u16).saturating_mul(3);
    let infra_health_avg = if state.infra.is_empty() {
        80u16
    } else {
        state.infra.iter().map(|n| n.health as u16).sum::<u16>() / state.infra.len() as u16
    };
    let alert_penalty = (state.alert_count as u16).saturating_mul(5);
    let base = 65u16;
    let score = base
        .saturating_add(health_factor / 4)
        .saturating_add(agent_factor)
        .saturating_add(infra_health_avg / 5)
        .saturating_sub(alert_penalty);
    score.clamp(20, 98) as u8
}

fn default_system_services() -> Vec<SystemService> {
    let now = now_ts();
    [
        ("scheduler", "agent task queue and dispatch"),
        ("event-bus", "append-only operational events"),
        ("memory", "agent memory and semantic recall"),
        ("policy", "capability and role enforcement"),
        ("workflow", "DAG workflow runtime"),
        ("apps", "agentic app/plugin runtime"),
        ("observability", "metrics, timelines, and reports"),
        ("security", "threat detection and audit"),
        ("persistence", "state, replay, and vector memory"),
    ]
    .into_iter()
    .map(|(name, notes)| SystemService {
        name: name.into(),
        status: "running".into(),
        health: 100,
        started_at: now.clone(),
        last_heartbeat: now.clone(),
        notes: notes.into(),
    })
    .collect()
}
