use std::{collections::HashMap, env, time::Duration};

use color_eyre::eyre::{Context, Result, eyre};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    models::{AgentRole, OpsEvent},
    utils::now_ts,
};

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const STREAM_TIMEOUT_SECS: u64 = 120;
const REQUEST_TIMEOUT_SECS: u64 = 45;
const PLANNING_MODEL: &str = "llama3.1:8b";
const CODING_MODEL: &str = "qwen2.5-coder:7b";
const DEEPSEEK_CODER_MODEL: &str = "deepseek-coder";
const SECURITY_MODEL: &str = "llama3.1:8b";
const LIGHTWEIGHT_MODEL: &str = "mistral";
const TOOL_CAPABLE_MODEL: &str = "llama3.1:8b";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum AgentKind {
    Coding,
    Planning,
    Security,
    Utility,
}

impl AgentKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Coding => "coding",
            Self::Planning => "planning",
            Self::Security => "security",
            Self::Utility => "utility",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum RuntimeAgentKind {
    Planner,
    Coding,
    Security,
    Infra,
    Research,
    Recovery,
    Validation,
    Memory,
    Execution,
}

impl RuntimeAgentKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Planner => "planner",
            Self::Coding => "coding",
            Self::Security => "security",
            Self::Infra => "infra",
            Self::Research => "research",
            Self::Recovery => "recovery",
            Self::Validation => "validation",
            Self::Memory => "memory",
            Self::Execution => "execution",
        }
    }

    pub(crate) fn profile_kind(self) -> AgentKind {
        match self {
            Self::Planner => AgentKind::Planning,
            Self::Coding => AgentKind::Coding,
            Self::Security => AgentKind::Security,
            Self::Infra => AgentKind::Security,
            Self::Research => AgentKind::Security,
            Self::Recovery => AgentKind::Planning,
            Self::Validation => AgentKind::Utility,
            Self::Memory => AgentKind::Utility,
            Self::Execution => AgentKind::Coding,
        }
    }

    pub(crate) fn default_model(self) -> &'static str {
        match self {
            Self::Planner | Self::Recovery => PLANNING_MODEL,
            Self::Coding | Self::Execution => CODING_MODEL,
            Self::Security | Self::Infra | Self::Research => SECURITY_MODEL,
            Self::Validation | Self::Memory => LIGHTWEIGHT_MODEL,
        }
    }

    pub(crate) fn default_prompt(self) -> &'static str {
        match self {
            Self::Planner => {
                "Decompose the objective into auditable steps, delegate to specialized agents, and preserve replay context."
            }
            Self::Coding => {
                "Analyze repository context, generate minimal patches, run validation, and repair failures through bounded retries."
            }
            Self::Security => {
                "Validate commands, plugins, policies, prompts, and patches for security risk before execution."
            }
            Self::Infra => {
                "Reason about infrastructure state, incidents, service topology, and safe remediation paths."
            }
            Self::Research => {
                "Collect evidence, compare signals, identify contradictions, and return sourced operational context."
            }
            Self::Recovery => {
                "Classify failures, plan rollback or repair, checkpoint progress, and isolate broken execution branches."
            }
            Self::Validation => {
                "Run acceptance gates, review tool output, check policy constraints, and produce final pass or fail decisions."
            }
            Self::Memory => {
                "Compress context, retrieve scoped memory, persist snapshots, and prepare replay-compatible summaries."
            }
            Self::Execution => {
                "Execute approved tools through the sandbox, stream telemetry, and return structured execution results."
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeToolBinding {
    pub(crate) name: String,
    pub(crate) capability: String,
    pub(crate) risk_tier: String,
}

impl RuntimeToolBinding {
    fn new(name: &str, capability: &str, risk_tier: &str) -> Self {
        Self {
            name: name.into(),
            capability: capability.into(),
            risk_tier: risk_tier.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RuntimeAgentSpec {
    pub(crate) name: String,
    pub(crate) kind: RuntimeAgentKind,
    pub(crate) model: String,
    pub(crate) provider: String,
    pub(crate) prompt: String,
    pub(crate) memory_scope: String,
    pub(crate) tools: Vec<RuntimeToolBinding>,
    pub(crate) streaming: bool,
    pub(crate) async_execution: bool,
    pub(crate) replay_compatible: bool,
}

impl RuntimeAgentSpec {
    pub(crate) fn for_kind(kind: RuntimeAgentKind) -> Self {
        Self {
            name: format!("{}-agent", kind.as_str()),
            kind,
            model: kind.default_model().into(),
            provider: "ollama-rs".into(),
            prompt: kind.default_prompt().into(),
            memory_scope: format!("agent:{}:memory", kind.as_str()),
            tools: default_tools_for_agent(kind),
            streaming: true,
            async_execution: true,
            replay_compatible: true,
        }
    }

    pub(crate) fn agent_profile(&self) -> AgentProfile {
        AgentProfile {
            kind: self.kind.profile_kind(),
            name: self.name.clone(),
            model: self.model.clone(),
            purpose: self.prompt.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RustNativeRuntimeDescriptor {
    pub(crate) agent_runtime: String,
    pub(crate) swarm_runtime: String,
    pub(crate) local_provider: String,
    pub(crate) crate_anchors: Vec<String>,
    pub(crate) agents: Vec<RuntimeAgentSpec>,
}

impl RustNativeRuntimeDescriptor {
    pub(crate) fn new() -> Self {
        Self {
            agent_runtime: "rig".into(),
            swarm_runtime: "swarms_rs".into(),
            local_provider: "ollama-rs".into(),
            crate_anchors: rust_ai_crate_anchors(),
            agents: runtime_agent_specs(),
        }
    }

    pub(crate) fn required_models(&self) -> Vec<String> {
        let mut models = self
            .agents
            .iter()
            .map(|agent| agent.model.clone())
            .collect::<Vec<_>>();
        models.sort();
        models.dedup();
        models
    }
}

pub(crate) fn rust_ai_crate_anchors() -> Vec<String> {
    vec![
        std::any::type_name::<rig::providers::ollama::OllamaBuilder>().into(),
        std::any::type_name::<swarms_rs::structs::agent::AgentConfig>().into(),
        std::any::type_name::<ollama_rs::Ollama>().into(),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ModelWorkload {
    Lightweight,
    Planning,
    Coding,
    Security,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelRoutingDecision {
    pub(crate) workload: ModelWorkload,
    pub(crate) model: String,
    pub(crate) reason: String,
}

pub(crate) fn route_model(task: &str, agent: RuntimeAgentKind) -> ModelRoutingDecision {
    let lower = task.to_ascii_lowercase();
    let workload = if matches!(
        agent,
        RuntimeAgentKind::Coding | RuntimeAgentKind::Execution
    ) || lower.contains("repo")
        || lower.contains("patch")
        || lower.contains("code")
        || lower.contains("test")
    {
        ModelWorkload::Coding
    } else if matches!(agent, RuntimeAgentKind::Security | RuntimeAgentKind::Infra)
        || lower.contains("vulnerability")
        || lower.contains("sandbox")
        || lower.contains("policy")
        || lower.contains("incident")
    {
        ModelWorkload::Security
    } else if matches!(
        agent,
        RuntimeAgentKind::Planner | RuntimeAgentKind::Recovery
    ) || lower.contains("plan")
        || lower.contains("delegate")
        || lower.contains("workflow")
    {
        ModelWorkload::Planning
    } else {
        ModelWorkload::Lightweight
    };

    let model = match workload {
        ModelWorkload::Lightweight => LIGHTWEIGHT_MODEL,
        ModelWorkload::Planning => PLANNING_MODEL,
        ModelWorkload::Coding => {
            if lower.contains("deepseek") {
                DEEPSEEK_CODER_MODEL
            } else {
                CODING_MODEL
            }
        }
        ModelWorkload::Security => SECURITY_MODEL,
    };

    ModelRoutingDecision {
        workload,
        model: model.into(),
        reason: format!(
            "routed {} task for {} via Rust-native model router",
            match workload {
                ModelWorkload::Lightweight => "lightweight",
                ModelWorkload::Planning => "planning",
                ModelWorkload::Coding => "coding",
                ModelWorkload::Security => "security",
            },
            agent.as_str()
        ),
    }
}

pub(crate) fn runtime_agent_specs() -> Vec<RuntimeAgentSpec> {
    [
        RuntimeAgentKind::Planner,
        RuntimeAgentKind::Coding,
        RuntimeAgentKind::Security,
        RuntimeAgentKind::Infra,
        RuntimeAgentKind::Research,
        RuntimeAgentKind::Recovery,
        RuntimeAgentKind::Validation,
        RuntimeAgentKind::Memory,
        RuntimeAgentKind::Execution,
    ]
    .into_iter()
    .map(RuntimeAgentSpec::for_kind)
    .collect()
}

fn default_tools_for_agent(kind: RuntimeAgentKind) -> Vec<RuntimeToolBinding> {
    match kind {
        RuntimeAgentKind::Planner => vec![
            RuntimeToolBinding::new("workflow-delegate", "workflow:delegate", "low"),
            RuntimeToolBinding::new("memory-retrieve", "memory:read", "low"),
        ],
        RuntimeAgentKind::Coding => vec![
            RuntimeToolBinding::new("repository-index", "repo:index", "low"),
            RuntimeToolBinding::new("semantic-code-search", "repo:read", "low"),
            RuntimeToolBinding::new("patch-generate", "repo:write", "medium"),
            RuntimeToolBinding::new("validation-run", "tool:execute", "medium"),
        ],
        RuntimeAgentKind::Security => vec![
            RuntimeToolBinding::new("command-validator", "security:command", "low"),
            RuntimeToolBinding::new("plugin-inspector", "security:plugin", "medium"),
            RuntimeToolBinding::new("policy-gate", "security:policy", "low"),
        ],
        RuntimeAgentKind::Infra => vec![
            RuntimeToolBinding::new("infra-discover", "infra:read", "low"),
            RuntimeToolBinding::new("incident-analyze", "infra:incident", "medium"),
        ],
        RuntimeAgentKind::Research => vec![
            RuntimeToolBinding::new("evidence-search", "research:read", "low"),
            RuntimeToolBinding::new("source-rank", "research:rank", "low"),
        ],
        RuntimeAgentKind::Recovery => vec![
            RuntimeToolBinding::new("checkpoint-restore", "workflow:checkpoint", "medium"),
            RuntimeToolBinding::new("rollback-plan", "workflow:rollback", "high"),
        ],
        RuntimeAgentKind::Validation => vec![
            RuntimeToolBinding::new("test-review", "validation:test", "low"),
            RuntimeToolBinding::new("consensus-check", "validation:consensus", "low"),
        ],
        RuntimeAgentKind::Memory => vec![
            RuntimeToolBinding::new("context-compress", "memory:compress", "low"),
            RuntimeToolBinding::new("semantic-recall", "memory:read", "low"),
            RuntimeToolBinding::new("snapshot-store", "memory:write", "medium"),
        ],
        RuntimeAgentKind::Execution => vec![
            RuntimeToolBinding::new("sandbox-exec", "tool:execute", "high"),
            RuntimeToolBinding::new("telemetry-stream", "observability:write", "low"),
        ],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentProfile {
    pub(crate) kind: AgentKind,
    pub(crate) name: String,
    pub(crate) model: String,
    pub(crate) purpose: String,
}

impl AgentProfile {
    pub(crate) fn for_kind(kind: AgentKind) -> Self {
        match kind {
            AgentKind::Coding => Self {
                kind,
                name: "coding-agent".into(),
                model: "qwen2.5-coder:7b".into(),
                purpose: "code generation, debugging, refactoring, terminal command generation, workflow scripting".into(),
            },
            AgentKind::Planning => Self {
                kind,
                name: "planning-agent".into(),
                model: "llama3.1:8b".into(),
                purpose: "task decomposition, autonomous planning, orchestration, workflow execution planning, multi-step reasoning".into(),
            },
            AgentKind::Security => Self {
                kind,
                name: "security-agent".into(),
                model: "llama3.1:8b".into(),
                purpose: "tool-capable security analysis, log analysis, anomaly detection, reasoning-heavy tasks, infrastructure diagnostics".into(),
            },
            AgentKind::Utility => Self {
                kind,
                name: "utility-agent".into(),
                model: "phi4".into(),
                purpose: "quick responses, summaries, lightweight assistant tasks, routing, system notifications".into(),
            },
        }
    }
}

pub(crate) fn default_agent_profiles() -> Vec<AgentProfile> {
    [
        AgentKind::Coding,
        AgentKind::Planning,
        AgentKind::Security,
        AgentKind::Utility,
    ]
    .into_iter()
    .map(AgentProfile::for_kind)
    .collect()
}

pub(crate) fn agent_kind_for_role(role: &AgentRole) -> AgentKind {
    match role {
        AgentRole::Planner => AgentKind::Planning,
        AgentRole::Executor => AgentKind::Coding,
        AgentRole::Research | AgentRole::Logs | AgentRole::Triage => AgentKind::Security,
        AgentRole::Workflow | AgentRole::Report => AgentKind::Utility,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ToolSpec {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AgentPrompt {
    pub(crate) system: String,
    pub(crate) user: String,
    pub(crate) tools: Vec<ToolSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct ToolCall {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolCallResult {
    pub(crate) call: ToolCall,
    pub(crate) output: Value,
    pub(crate) success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct TokenUsage {
    pub(crate) requests: u64,
    pub(crate) prompt_tokens: u64,
    pub(crate) completion_tokens: u64,
    pub(crate) total_tokens: u64,
    pub(crate) retries: u64,
    pub(crate) errors: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelHealth {
    pub(crate) model: String,
    pub(crate) installed: bool,
    pub(crate) online: bool,
    pub(crate) size_bytes: Option<u64>,
    pub(crate) digest: Option<String>,
    pub(crate) modified_at: Option<String>,
    pub(crate) last_checked: String,
    pub(crate) notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AiResponse {
    pub(crate) content: String,
    pub(crate) tool_calls: Vec<ToolCall>,
    pub(crate) usage: Option<TokenUsage>,
}

#[derive(Debug, Clone)]
pub(crate) struct AiClient {
    base_url: String,
    profile: AgentProfile,
    http: reqwest::Client,
    timeout: Duration,
    retries: usize,
}

impl AiClient {
    pub(crate) fn new(profile: AgentProfile) -> Self {
        let configured_url =
            env::var("OCTOBOT_OLLAMA_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.into());
        Self {
            base_url: local_ollama_endpoint(&configured_url).unwrap_or_else(|| {
                tracing::warn!(
                    endpoint = %configured_url,
                    "rejected non-local Ollama endpoint; falling back to localhost"
                );
                DEFAULT_OLLAMA_URL.into()
            }),
            profile,
            http: reqwest::Client::new(),
            timeout: Duration::from_secs(REQUEST_TIMEOUT_SECS),
            retries: env::var("OCTOBOT_OLLAMA_RETRIES")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2),
        }
    }

    pub(crate) fn profile(&self) -> &AgentProfile {
        &self.profile
    }

    pub(crate) fn model(&self) -> &str {
        &self.profile.model
    }

    pub(crate) fn endpoint(&self) -> String {
        self.base_url.trim_end_matches('/').to_string()
    }

    pub(crate) async fn health_check(&self) -> Result<()> {
        let url = format!("{}/api/tags", self.endpoint());
        let response = self
            .http
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .context("checking Ollama health")?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(eyre!(
                "ollama health check failed with {}",
                response.status()
            ))
        }
    }

    pub(crate) async fn list_models(&self) -> Result<Vec<ModelHealth>> {
        let url = format!("{}/api/tags", self.endpoint());
        let response = self
            .http
            .get(url)
            .timeout(self.timeout)
            .send()
            .await
            .context("requesting Ollama model tags")?;
        if !response.status().is_success() {
            return Err(eyre!("ollama /api/tags failed with {}", response.status()));
        }
        let tags: OllamaTagsResponse = response.json().await.context("decoding Ollama tags")?;
        let now = now_ts();
        Ok(tags
            .models
            .into_iter()
            .map(|model| ModelHealth {
                installed: true,
                online: true,
                model: model.name,
                size_bytes: model.size,
                digest: model.digest,
                modified_at: model.modified_at,
                last_checked: now.clone(),
                notes: model
                    .details
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "installed".into()),
            })
            .collect())
    }

    pub(crate) async fn required_model_health(
        &self,
        required: &[String],
    ) -> Result<Vec<ModelHealth>> {
        let installed = self.list_models().await.unwrap_or_default();
        let mut map: HashMap<String, ModelHealth> = installed
            .into_iter()
            .map(|health| (health.model.clone(), health))
            .collect();
        let now = now_ts();
        for model in required {
            if map.contains_key(model) || resolve_installed_model_name(model, map.keys()).is_some()
            {
                continue;
            }
            map.entry(model.clone()).or_insert(ModelHealth {
                model: model.clone(),
                installed: false,
                online: false,
                size_bytes: None,
                digest: None,
                modified_at: None,
                last_checked: now.clone(),
                notes: "missing locally; pull it with `ollama pull <model>`".into(),
            });
        }
        Ok(map.into_values().collect())
    }

    pub(crate) async fn chat(
        &self,
        messages: Vec<Value>,
        tools: &[ToolSpec],
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
    ) -> Result<AiResponse> {
        self.chat_with_model(&self.profile.model, messages, tools, agent_name, event_tx)
            .await
    }

    pub(crate) async fn run_agent_turn(&self, prompt: AgentPrompt) -> Result<AiResponse> {
        self.chat(
            vec![
                json!({ "role": "system", "content": prompt.system }),
                json!({ "role": "user", "content": prompt.user }),
            ],
            &prompt.tools,
            self.profile.name.as_str(),
            None,
        )
        .await
    }

    pub(crate) async fn run_agent_turn_with_messages(
        &self,
        messages: Vec<Value>,
        tools: &[ToolSpec],
    ) -> Result<AiResponse> {
        self.chat(messages, tools, self.profile.name.as_str(), None)
            .await
    }

    pub(crate) fn request_body(&self, prompt: &AgentPrompt) -> Value {
        json!({
            "model": self.profile.model,
            "stream": true,
            "messages": [
                { "role": "system", "content": prompt.system },
                { "role": "user", "content": prompt.user }
            ],
            "tools": prompt.tools.iter().map(openai_tool).collect::<Vec<_>>(),
        })
    }

    pub(crate) fn unload_request_body(&self) -> Value {
        Self::unload_request_body_for(&self.profile.model)
    }

    pub(crate) fn unload_request_body_for(model: &str) -> Value {
        json!({
            "model": model,
            "prompt": "",
            "stream": false,
            "keep_alive": 0,
        })
    }

    pub(crate) fn provider_name(&self) -> &'static str {
        "ollama"
    }

    pub(crate) fn validate_local_endpoint(endpoint: &str) -> Result<String> {
        local_ollama_endpoint(endpoint).ok_or_else(|| {
            eyre!(
                "Ollama endpoint must be local-only (http://localhost:11434, 127.0.0.1, or [::1])"
            )
        })
    }

    pub(crate) async fn unload_model(&self) -> Result<()> {
        self.unload_model_named(&self.profile.model).await
    }

    pub(crate) async fn unload_model_named(&self, model: &str) -> Result<()> {
        let _ = self
            .send_with_retry("/api/generate", Self::unload_request_body_for(model))
            .await?;
        Ok(())
    }

    pub(crate) async fn chat_with_model(
        &self,
        model: &str,
        messages: Vec<Value>,
        tools: &[ToolSpec],
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
    ) -> Result<AiResponse> {
        let requested_model = if tools.is_empty() {
            model
        } else {
            tool_capable_model_for(model)
        };
        let resolved_model = self.resolve_model_name(requested_model).await?;
        if resolved_model != model
            && let Some(tx) = event_tx
        {
            let _ = tx.send(OpsEvent::NotificationRaised {
                level: "info".into(),
                message: format!(
                    "resolved Ollama model `{model}` to installed tool-capable `{resolved_model}`"
                ),
                timestamp: now_ts(),
            });
        }
        let body = json!({
            "model": resolved_model,
            "stream": true,
            "messages": messages,
            "tools": tools.iter().map(openai_tool).collect::<Vec<_>>(),
        });
        let response = self.send_with_retry("/api/chat", body).await?;
        self.consume_chat_stream(response, &resolved_model, agent_name, event_tx)
            .await
    }

    pub(crate) async fn generate(
        &self,
        prompt: &str,
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
    ) -> Result<AiResponse> {
        let resolved_model = self.resolve_model_name(&self.profile.model).await?;
        let body = json!({
            "model": resolved_model,
            "stream": true,
            "prompt": prompt,
        });
        let response = self.send_with_retry("/api/generate", body).await?;
        self.consume_generate_stream_with_model(response, &resolved_model, agent_name, event_tx)
            .await
    }

    pub(crate) async fn embeddings(&self, model: Option<&str>, input: &str) -> Result<Vec<f32>> {
        let requested_model = model.unwrap_or(&self.profile.model);
        let resolved_model = self.resolve_model_name(requested_model).await?;
        let body = json!({
            "model": resolved_model,
            "prompt": input,
        });
        let response = self
            .http
            .post(format!("{}/api/embeddings", self.endpoint()))
            .timeout(self.timeout)
            .json(&body)
            .send()
            .await
            .context("requesting Ollama embeddings")?;
        if !response.status().is_success() {
            return Err(eyre!(
                "ollama /api/embeddings failed with {}",
                response.status()
            ));
        }
        let payload: OllamaEmbeddingsResponse = response
            .json()
            .await
            .context("decoding Ollama embeddings response")?;
        Ok(payload.embedding)
    }

    pub(crate) fn all_profiles() -> Vec<AgentProfile> {
        default_agent_profiles()
    }

    async fn send_with_retry(&self, path: &str, body: Value) -> Result<reqwest::Response> {
        let mut last_error = None;
        for attempt in 0..=self.retries {
            let request = self
                .http
                .post(format!(
                    "{}/{}",
                    self.endpoint(),
                    path.trim_start_matches('/')
                ))
                .timeout(self.timeout)
                .json(&body);
            match request.send().await {
                Ok(response) if response.status().is_success() => return Ok(response),
                Ok(response) => {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_default();
                    last_error = Some(eyre!(
                        "ollama request failed with {}: {}",
                        status,
                        error_body.chars().take(240).collect::<String>()
                    ));
                }
                Err(error) => last_error = Some(error.into()),
            }
            if attempt < self.retries {
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
        }
        Err(last_error.unwrap_or_else(|| eyre!("ollama request failed")))
    }

    async fn consume_chat_stream(
        &self,
        response: reqwest::Response,
        model: &str,
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
    ) -> Result<AiResponse> {
        self.consume_json_stream(response, model, agent_name, event_tx, StreamKind::Chat)
            .await
    }

    async fn consume_generate_stream(
        &self,
        response: reqwest::Response,
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
    ) -> Result<AiResponse> {
        self.consume_generate_stream_with_model(response, &self.profile.model, agent_name, event_tx)
            .await
    }

    async fn consume_generate_stream_with_model(
        &self,
        response: reqwest::Response,
        model: &str,
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
    ) -> Result<AiResponse> {
        self.consume_json_stream(response, model, agent_name, event_tx, StreamKind::Generate)
            .await
    }

    async fn resolve_model_name(&self, requested: &str) -> Result<String> {
        let installed = self.list_models().await.unwrap_or_default();
        let installed_names = installed
            .iter()
            .map(|model| model.model.as_str())
            .collect::<Vec<_>>();
        if let Some(resolved) = resolve_installed_model_name(requested, installed_names.into_iter())
        {
            return Ok(resolved);
        }
        if let Ok(fallback) = env::var("OCTOBOT_OLLAMA_MODEL") {
            let fallback = fallback.trim();
            if !fallback.is_empty() && fallback != requested {
                let installed = installed
                    .iter()
                    .map(|model| model.model.as_str())
                    .collect::<Vec<_>>();
                if let Some(resolved) =
                    resolve_installed_model_name(fallback, installed.into_iter())
                {
                    return Ok(resolved);
                }
            }
        }
        Ok(requested.to_string())
    }

    async fn consume_json_stream(
        &self,
        response: reqwest::Response,
        model: &str,
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
        kind: StreamKind,
    ) -> Result<AiResponse> {
        let mut content = String::new();
        let mut tool_calls: HashMap<String, ToolCall> = HashMap::new();
        let mut usage = TokenUsage::default();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let timeout = tokio::time::sleep(Duration::from_secs(STREAM_TIMEOUT_SECS));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                _ = &mut timeout => {
                    return Err(eyre!("ollama streaming response timed out after {}s", STREAM_TIMEOUT_SECS));
                }
                chunk = stream.next() => {
                    let Some(chunk) = chunk else { break; };
                    let chunk = chunk.context("reading Ollama response chunk")?;
                    let text = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&text);

                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].trim().to_string();
                        buffer.drain(..=pos);
                        if line.is_empty() {
                            continue;
                        }
                        let value: Value = serde_json::from_str(&line).with_context(|| {
                            format!("parsing Ollama streamed JSON line: {line}")
                        })?;
                        if let Some(message) = value.get("message") {
                            if let Some(delta) = message.get("content").and_then(Value::as_str) {
                                content.push_str(delta);
                                if let Some(tx) = event_tx {
                                    let _ = tx.send(OpsEvent::CommandOutput {
                                        id: next_stream_id(agent_name),
                                        stream: format!("ollama:{model}"),
                                        line: delta.to_string(),
                                        timestamp: now_ts(),
                                    });
                                }
                            }
                            if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                                for call in calls {
                                    if let Some(tool) = parse_tool_call(call) {
                                        tool_calls.insert(tool.id.clone(), tool);
                                    }
                                }
                            }
                        }
                        if value.get("done").and_then(Value::as_bool).unwrap_or(false) {
                            usage.requests += 1;
                            usage.prompt_tokens += value.get("prompt_eval_count").and_then(Value::as_u64).unwrap_or(0);
                            usage.completion_tokens += value.get("eval_count").and_then(Value::as_u64).unwrap_or(0);
                            usage.total_tokens = usage.prompt_tokens + usage.completion_tokens;
                            if let Some(tx) = event_tx {
                                let _ = tx.send(OpsEvent::ToolCallCompleted {
                                    id: next_stream_id(agent_name),
                                    tool: format!("ollama-{kind:?}-{model}"),
                                    success: true,
                                    output: json!({
                                        "model": model,
                                        "prompt_tokens": usage.prompt_tokens,
                                        "completion_tokens": usage.completion_tokens,
                                        "total_tokens": usage.total_tokens,
                                    }),
                                    timestamp: now_ts(),
                                });
                                let _ = tx.send(OpsEvent::TokenUsageRecorded {
                                    agent: agent_name.into(),
                                    model: model.into(),
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                    total_tokens: usage.total_tokens,
                                    timestamp: now_ts(),
                                });
                                let _ = tx.send(OpsEvent::AgentMemoryStored {
                                    agent: agent_name.into(),
                                    key: "last_model".into(),
                                    value: model.into(),
                                    timestamp: now_ts(),
                                });
                            }
                        }
                    }
                }
            }
        }

        if !buffer.trim().is_empty() {
            let value: Value = serde_json::from_str(buffer.trim())
                .context("parsing trailing Ollama response JSON")?;
            if let Some(message) = value.get("message") {
                if let Some(delta) = message.get("content").and_then(Value::as_str) {
                    content.push_str(delta);
                }
                if let Some(calls) = message.get("tool_calls").and_then(Value::as_array) {
                    for call in calls {
                        if let Some(tool) = parse_tool_call(call) {
                            tool_calls.insert(tool.id.clone(), tool);
                        }
                    }
                }
            }
        }

        Ok(AiResponse {
            content,
            tool_calls: tool_calls.into_values().collect(),
            usage: Some(usage),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaTagModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaTagModel {
    name: String,
    size: Option<u64>,
    digest: Option<String>,
    modified_at: Option<String>,
    #[serde(default)]
    details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaEmbeddingsResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    Chat,
    Generate,
}

fn openai_tool(tool: &ToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters
        }
    })
}

fn local_ollama_endpoint(endpoint: &str) -> Option<String> {
    let trimmed = endpoint.trim().trim_end_matches('/').to_string();
    let lower = trimmed.to_ascii_lowercase();
    let local = lower == "http://localhost:11434"
        || lower == "http://127.0.0.1:11434"
        || lower == "http://[::1]:11434"
        || lower.starts_with("http://localhost:")
        || lower.starts_with("http://127.0.0.1:")
        || lower.starts_with("http://[::1]:");
    if local { Some(trimmed) } else { None }
}

pub(crate) fn resolve_installed_model_name<I, S>(requested: &str, installed: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let requested = requested.trim();
    if requested.is_empty() {
        return None;
    }
    let installed = installed
        .into_iter()
        .map(|model| model.as_ref().to_string())
        .collect::<Vec<_>>();
    if installed.iter().any(|model| model == requested) {
        return Some(requested.to_string());
    }
    for requested_base in compatible_model_bases(requested) {
        if let Some(model) = installed.iter().find(|model| {
            model
                .split(':')
                .next()
                .map(|base| base == requested_base)
                .unwrap_or(false)
        }) {
            return Some(model.clone());
        }
    }
    None
}

fn compatible_model_bases(requested: &str) -> Vec<&str> {
    let requested_base = requested.split(':').next().unwrap_or(requested);
    match requested_base {
        "llama3.3" => vec!["llama3.3", "llama3.1", "llama3"],
        "mistral" => vec!["mistral", "phi4", "phi"],
        "phi" => vec!["phi", "phi4"],
        "qwen2.5-coder" => vec!["qwen2.5-coder", "qwen2.5"],
        "deepseek-coder" => vec!["deepseek-coder", "qwen2.5-coder", "qwen2.5"],
        "deepseek-r1" => vec!["deepseek-r1"],
        _ => vec![requested_base],
    }
}

pub(crate) fn tool_capable_model_for(requested: &str) -> &str {
    let requested_base = requested.split(':').next().unwrap_or(requested).trim();
    match requested_base {
        "deepseek-r1" | "phi4" | "phi" | "mistral" => TOOL_CAPABLE_MODEL,
        _ => requested,
    }
}

fn parse_tool_call(value: &Value) -> Option<ToolCall> {
    let function = value.get("function")?;
    let name = function.get("name")?.as_str()?.to_string();
    let arguments = match function.get("arguments") {
        Some(Value::String(raw_args)) => {
            serde_json::from_str(raw_args).unwrap_or_else(|_| json!({}))
        }
        Some(Value::Object(map)) => Value::Object(map.clone()),
        Some(value) => value.clone(),
        None => json!({}),
    };
    Some(ToolCall {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("tool-call")
            .into(),
        name,
        arguments,
    })
}

fn next_stream_id(agent_name: &str) -> String {
    format!("stream-{}-{}", agent_name, now_ts())
}

pub(crate) fn build_messages(system: &str, user: &str, history: &[ToolCallResult]) -> Vec<Value> {
    let mut messages: Vec<Value> = vec![
        json!({ "role": "system", "content": system }),
        json!({ "role": "user", "content": user }),
    ];
    for result in history {
        messages.push(json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": result.call.id,
                "type": "function",
                "function": {
                    "name": result.call.name,
                    "arguments": result.call.arguments.to_string()
                }
            }]
        }));
        messages.push(json!({
            "role": "tool",
            "tool_call_id": result.call.id,
            "content": result.output.to_string()
        }));
    }
    messages
}
