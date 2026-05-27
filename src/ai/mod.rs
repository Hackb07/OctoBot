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
                model: "deepseek-r1:8b".into(),
                purpose: "security analysis, log analysis, anomaly detection, reasoning-heavy tasks, infrastructure diagnostics".into(),
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
        Self {
            base_url: env::var("OCTOBOT_OLLAMA_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.into()),
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
        let body = json!({
            "model": model,
            "stream": true,
            "messages": messages,
            "tools": tools.iter().map(openai_tool).collect::<Vec<_>>(),
        });
        let response = self.send_with_retry("/api/chat", body).await?;
        self.consume_chat_stream(response, model, agent_name, event_tx)
            .await
    }

    pub(crate) async fn generate(
        &self,
        prompt: &str,
        agent_name: &str,
        event_tx: Option<&mpsc::UnboundedSender<OpsEvent>>,
    ) -> Result<AiResponse> {
        let body = json!({
            "model": self.profile.model,
            "stream": true,
            "prompt": prompt,
        });
        let response = self.send_with_retry("/api/generate", body).await?;
        self.consume_generate_stream(response, agent_name, event_tx)
            .await
    }

    pub(crate) async fn embeddings(&self, model: Option<&str>, input: &str) -> Result<Vec<f32>> {
        let body = json!({
            "model": model.unwrap_or(&self.profile.model),
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
        self.consume_json_stream(
            response,
            &self.profile.model,
            agent_name,
            event_tx,
            StreamKind::Generate,
        )
        .await
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

fn parse_tool_call(value: &Value) -> Option<ToolCall> {
    let function = value.get("function")?;
    let name = function.get("name")?.as_str()?.to_string();
    let raw_args = function
        .get("arguments")
        .and_then(Value::as_str)
        .unwrap_or("{}");
    let arguments = serde_json::from_str(raw_args).unwrap_or_else(|_| json!({}));
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
