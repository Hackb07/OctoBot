use std::env;

use color_eyre::eyre::{Context, Result, eyre};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiProviderKind {
    OpenAi,
    Ollama,
    OpenRouter,
}

impl AiProviderKind {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Ollama => "ollama",
            Self::OpenRouter => "openrouter",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AiProviderConfig {
    pub(crate) kind: AiProviderKind,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) api_key: Option<String>,
}

impl AiProviderConfig {
    pub(crate) fn configured_from_env() -> Vec<Self> {
        let mut providers = Vec::new();
        if let Ok(api_key) = env::var("OCTOBOT_OPENAI_API_KEY") {
            providers.push(Self {
                kind: AiProviderKind::OpenAi,
                endpoint: env::var("OCTOBOT_OPENAI_BASE_URL")
                    .unwrap_or_else(|_| "https://api.openai.com/v1/chat/completions".into()),
                model: env::var("OCTOBOT_OPENAI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".into()),
                api_key: Some(api_key),
            });
        }
        if let Ok(endpoint) = env::var("OCTOBOT_OLLAMA_URL") {
            providers.push(Self {
                kind: AiProviderKind::Ollama,
                endpoint: format!("{}/api/chat", endpoint.trim_end_matches('/')),
                model: env::var("OCTOBOT_OLLAMA_MODEL").unwrap_or_else(|_| "llama3.1".into()),
                api_key: None,
            });
        }
        if let Ok(api_key) = env::var("OCTOBOT_OPENROUTER_API_KEY") {
            providers.push(Self {
                kind: AiProviderKind::OpenRouter,
                endpoint: env::var("OCTOBOT_OPENROUTER_BASE_URL")
                    .unwrap_or_else(|_| "https://openrouter.ai/api/v1/chat/completions".into()),
                model: env::var("OCTOBOT_OPENROUTER_MODEL")
                    .unwrap_or_else(|_| "openrouter/free".into()),
                api_key: Some(api_key),
            });
        }
        providers
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
pub(crate) struct AiResponse {
    pub(crate) content: String,
    pub(crate) tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone)]
pub(crate) struct AiClient {
    config: AiProviderConfig,
    http: reqwest::Client,
}

impl AiClient {
    pub(crate) fn new(config: AiProviderConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
        }
    }

    pub(crate) async fn run_agent_turn(&self, prompt: AgentPrompt) -> Result<AiResponse> {
        let body = self.request_body(&prompt);
        send_chat_request(&self.http, &self.config, body).await
    }

    pub(crate) async fn run_agent_turn_with_messages(
        &self,
        messages: Vec<Value>,
        tools: &[ToolSpec],
    ) -> Result<AiResponse> {
        let body = match self.config.kind {
            AiProviderKind::OpenAi | AiProviderKind::OpenRouter => json!({
                "model": self.config.model,
                "messages": messages,
                "tools": tools.iter().map(openai_tool).collect::<Vec<_>>()
            }),
            AiProviderKind::Ollama => json!({
                "model": self.config.model,
                "stream": false,
                "messages": messages,
                "tools": tools.iter().map(openai_tool).collect::<Vec<_>>()
            }),
        };
        send_chat_request(&self.http, &self.config, body).await
    }

    pub(crate) fn request_body(&self, prompt: &AgentPrompt) -> Value {
        match self.config.kind {
            AiProviderKind::OpenAi | AiProviderKind::OpenRouter => json!({
                "model": self.config.model,
                "messages": [
                    { "role": "system", "content": prompt.system },
                    { "role": "user", "content": prompt.user }
                ],
                "tools": prompt.tools.iter().map(openai_tool).collect::<Vec<_>>()
            }),
            AiProviderKind::Ollama => json!({
                "model": self.config.model,
                "stream": false,
                "messages": [
                    { "role": "system", "content": prompt.system },
                    { "role": "user", "content": prompt.user }
                ],
                "tools": prompt.tools.iter().map(openai_tool).collect::<Vec<_>>()
            }),
        }
    }

    pub(crate) fn provider_name(&self) -> &'static str {
        self.config.kind.as_str()
    }

    pub(crate) fn endpoint(&self) -> &str {
        &self.config.endpoint
    }
}

pub(crate) fn build_messages(
    system: &str,
    user: &str,
    history: &[ToolCallResult],
) -> Vec<Value> {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ToolCallResult {
    pub(crate) call: ToolCall,
    pub(crate) output: Value,
    pub(crate) success: bool,
}

async fn send_chat_request(
    http: &reqwest::Client,
    config: &AiProviderConfig,
    body: Value,
) -> Result<AiResponse> {
    let mut request = http.post(&config.endpoint).json(&body);
    if let Some(api_key) = &config.api_key {
        request = request.bearer_auth(api_key);
    }
    if config.kind == AiProviderKind::OpenRouter {
        request = request.header("HTTP-Referer", "https://localhost/octobot");
    }
    let response = request
        .send()
        .await
        .with_context(|| format!("calling {} provider", config.kind.as_str()))?;
    if !response.status().is_success() {
        return Err(eyre!(
            "{} provider failed with {}",
            config.kind.as_str(),
            response.status()
        ));
    }
    let value: Value = response.json().await.context("decoding AI response")?;
    parse_response(&config.kind, value)
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

fn parse_response(kind: &AiProviderKind, value: Value) -> Result<AiResponse> {
    match kind {
        AiProviderKind::OpenAi | AiProviderKind::OpenRouter => {
            let message = value
                .get("choices")
                .and_then(|choices| choices.get(0))
                .and_then(|choice| choice.get("message"))
                .ok_or_else(|| eyre!("missing chat completion message"))?;
            Ok(AiResponse {
                content: message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                tool_calls: parse_openai_tool_calls(message.get("tool_calls")),
            })
        }
        AiProviderKind::Ollama => {
            let message = value
                .get("message")
                .ok_or_else(|| eyre!("missing Ollama message"))?;
            Ok(AiResponse {
                content: message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .into(),
                tool_calls: parse_openai_tool_calls(message.get("tool_calls")),
            })
        }
    }
}

fn parse_openai_tool_calls(value: Option<&Value>) -> Vec<ToolCall> {
    value
        .and_then(Value::as_array)
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    let function = call.get("function")?;
                    let name = function.get("name")?.as_str()?.to_string();
                    let raw_args = function
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("{}");
                    let arguments = serde_json::from_str(raw_args).unwrap_or_else(|_| json!({}));
                    Some(ToolCall {
                        id: call
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("tool-call")
                            .into(),
                        name,
                        arguments,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}
