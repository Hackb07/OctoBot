use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use axum::{
    Router,
    extract::{
        Path as AxumPath,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
    routing::get,
};
use color_eyre::eyre::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpListener,
    process::Command,
    sync::mpsc,
    time,
};

use crate::utils::now_ts;
use crate::{models::OpsEvent, utils::next_id};

const DEFAULT_RUNTIME_ADDR: &str = "127.0.0.1:7879";

#[derive(Debug, Clone, Copy)]
pub(crate) enum RuntimeSmokeKind {
    ListDirectory,
    DockerPolicy,
    Cancellation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeToolEnvelope {
    #[serde(default)]
    id: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    tool: Option<ToolRequest>,
    #[serde(default)]
    workspace_root: String,
    #[serde(default)]
    requires_approval: bool,
    #[serde(default)]
    resource_limits: ResourceLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolRequest {
    #[serde(default)]
    id: String,
    #[serde(default)]
    task_id: String,
    name: String,
    #[serde(default)]
    arguments: Value,
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
    #[serde(default = "default_dry_run")]
    dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ResourceLimits {
    #[serde(default = "default_timeout")]
    timeout_seconds: u64,
    #[serde(default = "default_memory_mb")]
    memory_mb: u64,
    #[serde(default = "default_cpu_count")]
    cpu_count: f32,
    #[serde(default)]
    network_enabled: bool,
    #[serde(default)]
    docker_image: Option<String>,
    #[serde(default)]
    use_docker: bool,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            timeout_seconds: default_timeout(),
            memory_mb: default_memory_mb(),
            cpu_count: default_cpu_count(),
            network_enabled: false,
            docker_image: None,
            use_docker: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RuntimeEvent {
    id: String,
    task_id: String,
    #[serde(rename = "type")]
    event_type: String,
    payload: Value,
    timestamp: String,
}

pub(crate) async fn serve_runtime_service() -> Result<()> {
    let app = Router::new()
        .route("/runtime/tools/{tool}", get(tool_socket))
        .route("/runtime/health", get(runtime_health));
    let addr =
        std::env::var("OCTOBOT_RUNTIME_ADDR").unwrap_or_else(|_| DEFAULT_RUNTIME_ADDR.into());
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("binding runtime service on {addr}"))?;
    tracing::info!("runtime service listening on {addr}");
    axum::serve(listener, app)
        .await
        .context("serving runtime service")?;
    Ok(())
}

pub(crate) fn spawn_runtime_smoke(
    kind: RuntimeSmokeKind,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) {
    tokio::spawn(async move {
        run_runtime_smoke(kind, event_tx).await;
    });
}

async fn run_runtime_smoke(kind: RuntimeSmokeKind, event_tx: mpsc::UnboundedSender<OpsEvent>) {
    let id = next_id("runtime-smoke");
    let (name, request) = match kind {
        RuntimeSmokeKind::ListDirectory => (
            "runtime smoke list_directory",
            ToolRequest {
                id: id.clone(),
                task_id: "octobot-runtime-smoke".into(),
                name: "list_directory".into(),
                arguments: json!({ "root": ".", "path": "." }),
                timeout_seconds: 10,
                dry_run: true,
            },
        ),
        RuntimeSmokeKind::DockerPolicy => (
            "runtime smoke docker policy",
            ToolRequest {
                id: id.clone(),
                task_id: "octobot-runtime-smoke".into(),
                name: "execute_terminal".into(),
                arguments: json!({
                    "root": ".",
                    "command": "cargo test",
                    "use_docker": true,
                    "docker_image": "rust:1-bookworm",
                    "network_enabled": false,
                    "memory_mb": 512,
                    "cpu_count": "1.0"
                }),
                timeout_seconds: 10,
                dry_run: true,
            },
        ),
        RuntimeSmokeKind::Cancellation => (
            "runtime smoke cancellation",
            ToolRequest {
                id: id.clone(),
                task_id: "octobot-runtime-smoke".into(),
                name: "execute_terminal".into(),
                arguments: json!({
                    "root": ".",
                    "command": "python -m timeit -n 100000000 pass",
                    "allow_dry_run_execution": true
                }),
                timeout_seconds: 30,
                dry_run: false,
            },
        ),
    };
    let _ = event_tx.send(OpsEvent::ToolCallRequested {
        id: id.clone(),
        tool: name.into(),
        arguments: request.arguments.clone(),
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::NotificationRaised {
        level: "info".into(),
        message: format!("{name} started"),
        timestamp: now_ts(),
    });

    let result = match kind {
        RuntimeSmokeKind::Cancellation => runtime_cancel_smoke(&request).await,
        _ => runtime_collect_smoke(request).await,
    };
    let success = result
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let _ = event_tx.send(OpsEvent::ToolCallCompleted {
        id,
        tool: name.into(),
        success,
        output: result.clone(),
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::NotificationRaised {
        level: if success { "info" } else { "warn" }.into(),
        message: format!(
            "{name} {}",
            if success {
                "completed"
            } else {
                "failed or cancelled"
            }
        ),
        timestamp: now_ts(),
    });
}

async fn runtime_collect_smoke(request: ToolRequest) -> Value {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<RuntimeEvent>();
    let (_cancel_tx, cancel_rx) = mpsc::unbounded_channel::<()>();
    tokio::spawn(execute_tool(request, event_tx, cancel_rx));
    let mut events = Vec::new();
    while let Some(event) = event_rx.recv().await {
        let done = matches!(event.event_type.as_str(), "tool.completed" | "tool.failed");
        events.push(json!({
            "type": event.event_type,
            "payload": event.payload
        }));
        if done {
            break;
        }
    }
    let success = events
        .last()
        .and_then(|event| event.get("type"))
        .and_then(Value::as_str)
        == Some("tool.completed");
    json!({ "success": success, "events": events })
}

async fn runtime_cancel_smoke(request: &ToolRequest) -> Value {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<RuntimeEvent>();
    let (cancel_tx, mut cancel_rx) = mpsc::unbounded_channel::<()>();
    let request = request.clone();
    tokio::spawn(async move {
        time::sleep(Duration::from_millis(100)).await;
        let _ = cancel_tx.send(());
    });
    tokio::spawn(async move {
        let result = execute_command_tool(&request, event_tx.clone(), &mut cancel_rx).await;
        match result {
            Ok(payload) => {
                let _ = event_tx.send(event(&request.task_id, "tool.completed", payload));
            }
            Err(error) => {
                let _ = event_tx.send(event(
                    &request.task_id,
                    "tool.failed",
                    json!({ "error": error }),
                ));
            }
        }
    });
    let mut events = Vec::new();
    while let Some(event) = event_rx.recv().await {
        let done = matches!(event.event_type.as_str(), "tool.completed" | "tool.failed");
        events.push(json!({
            "type": event.event_type,
            "payload": event.payload
        }));
        if done {
            break;
        }
    }
    let cancelled = events
        .last()
        .and_then(|event| event.get("payload"))
        .and_then(|payload| payload.get("error"))
        .and_then(Value::as_str)
        .map(|error| error.contains("cancelled"))
        .unwrap_or(false);
    json!({ "success": cancelled, "cancelled": cancelled, "events": events })
}

async fn runtime_health() -> impl IntoResponse {
    axum::Json(json!({ "status": "ok", "service": "octobot-runtime-service" }))
}

async fn tool_socket(ws: WebSocketUpgrade, AxumPath(tool): AxumPath<String>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_tool_socket(socket, tool))
}

async fn handle_tool_socket(socket: WebSocket, tool_name: String) {
    let (mut sender, mut receiver) = socket.split();
    let Some(Ok(Message::Text(initial))) = receiver.next().await else {
        return;
    };
    let request = match parse_initial_request(&initial, &tool_name) {
        Ok(request) => request,
        Err(error) => {
            let _ = sender
                .send(Message::Text(
                    serde_json::to_string(&event("", "runtime.error", json!({ "error": error })))
                        .unwrap_or_else(|_| "{}".into())
                        .into(),
                ))
                .await;
            return;
        }
    };
    let task_id = request.task_id.clone();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<RuntimeEvent>();
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<()>();
    let cancel_task = tokio::spawn(async move {
        while let Some(Ok(message)) = receiver.next().await {
            if let Message::Text(text) = message
                && text.contains("\"cancel\"")
            {
                let _ = cancel_tx.send(());
                break;
            }
        }
    });
    let run_task = tokio::spawn(execute_tool(request, event_tx, cancel_rx));
    while let Some(runtime_event) = event_rx.recv().await {
        if sender
            .send(Message::Text(
                serde_json::to_string(&runtime_event)
                    .unwrap_or_else(|_| "{}".into())
                    .into(),
            ))
            .await
            .is_err()
        {
            break;
        }
    }
    let _ = run_task.await;
    cancel_task.abort();
    let _ = sender
        .send(Message::Text(
            serde_json::to_string(&event(&task_id, "runtime.closed", json!({})))
                .unwrap_or_else(|_| "{}".into())
                .into(),
        ))
        .await;
}

fn parse_initial_request(input: &str, path_tool: &str) -> std::result::Result<ToolRequest, String> {
    let value: Value = serde_json::from_str(input).map_err(|error| error.to_string())?;
    if value.get("tool").is_some() {
        let envelope: RuntimeToolEnvelope =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        let mut tool = envelope
            .tool
            .ok_or_else(|| "missing tool request".to_string())?;
        if tool.name.is_empty() {
            tool.name = path_tool.into();
        }
        if tool.timeout_seconds == 0 {
            tool.timeout_seconds = envelope.resource_limits.timeout_seconds;
        }
        if tool.arguments.get("root").is_none() && !envelope.workspace_root.is_empty() {
            tool.arguments["root"] = Value::String(envelope.workspace_root);
        }
        if envelope.resource_limits.use_docker {
            tool.arguments["use_docker"] = Value::Bool(true);
        }
        if let Some(image) = envelope.resource_limits.docker_image {
            tool.arguments["docker_image"] = Value::String(image);
        }
        tool.arguments["network_enabled"] = Value::Bool(envelope.resource_limits.network_enabled);
        tool.arguments["memory_mb"] = Value::Number(envelope.resource_limits.memory_mb.into());
        return Ok(tool);
    }
    let mut request: ToolRequest =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    if request.name.is_empty() {
        request.name = path_tool.into();
    }
    Ok(request)
}

async fn execute_tool(
    request: ToolRequest,
    event_tx: mpsc::UnboundedSender<RuntimeEvent>,
    mut cancel_rx: mpsc::UnboundedReceiver<()>,
) {
    let _ = event_tx.send(event(
        &request.task_id,
        "tool.started",
        json!({ "tool": request.name, "id": request.id }),
    ));
    let result = match request.name.as_str() {
        "execute_terminal" | "run_tests" | "lint_project" => {
            execute_command_tool(&request, event_tx.clone(), &mut cancel_rx).await
        }
        "read_file" => read_file_tool(&request),
        "write_file" => write_file_tool(&request),
        "list_directory" => list_directory_tool(&request),
        _ => Err(format!("unsupported runtime tool `{}`", request.name)),
    };
    match result {
        Ok(payload) => {
            let _ = event_tx.send(event(&request.task_id, "tool.completed", payload));
        }
        Err(error) => {
            let _ = event_tx.send(event(
                &request.task_id,
                "tool.failed",
                json!({ "tool": request.name, "error": error }),
            ));
        }
    }
}

async fn execute_command_tool(
    request: &ToolRequest,
    event_tx: mpsc::UnboundedSender<RuntimeEvent>,
    cancel_rx: &mut mpsc::UnboundedReceiver<()>,
) -> std::result::Result<Value, String> {
    let root = workspace_root(request)?;
    let command = command_for_request(request)?;
    RuntimeCommandPolicy::validate(&command)?;
    if request.dry_run && !arg_bool(request, "allow_dry_run_execution") {
        return Ok(json!({ "tool": request.name, "dry_run": true, "command": command }));
    }
    let mut cmd = if arg_bool(request, "use_docker") {
        docker_command(request, &root, &command)?
    } else {
        local_command(&root, &command)?
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|error| error.to_string())?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    if let Some(stdout) = stdout {
        spawn_stream_reader(request.task_id.clone(), "stdout", stdout, event_tx.clone());
    }
    if let Some(stderr) = stderr {
        spawn_stream_reader(request.task_id.clone(), "stderr", stderr, event_tx.clone());
    }
    let timeout = Duration::from_secs(request.timeout_seconds.max(1));
    let exit_status = tokio::select! {
        status = child.wait() => status.map_err(|error| error.to_string())?,
        _ = time::sleep(timeout) => {
            let _ = child.kill().await;
            return Err(format!("command timed out after {}s", timeout.as_secs()));
        }
        _ = cancel_rx.recv() => {
            let _ = child.kill().await;
            return Err("command cancelled".into());
        }
    };
    Ok(json!({
        "tool": request.name,
        "command": command,
        "exit_code": exit_status.code(),
        "success": exit_status.success()
    }))
}

fn spawn_stream_reader<R>(
    task_id: String,
    stream: &'static str,
    reader: R,
    event_tx: mpsc::UnboundedSender<RuntimeEvent>,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        let mut sequence = 0_u64;
        while let Ok(Some(line)) = lines.next_line().await {
            sequence += 1;
            let _ = event_tx.send(event(
                &task_id,
                "tool.output",
                json!({ "stream": stream, "sequence": sequence, "data": line }),
            ));
        }
    });
}

fn local_command(root: &Path, command: &str) -> std::result::Result<Command, String> {
    let parts = split_command(command)?;
    let mut cmd = Command::new(&parts[0]);
    cmd.args(&parts[1..]).current_dir(root);
    Ok(cmd)
}

fn docker_command(
    request: &ToolRequest,
    root: &Path,
    command: &str,
) -> std::result::Result<Command, String> {
    let image = arg_string(request, "docker_image").unwrap_or_else(|| "rust:1-bookworm".into());
    let mut cmd = Command::new("docker");
    cmd.arg("run")
        .arg("--rm")
        .arg("--network")
        .arg(if arg_bool(request, "network_enabled") {
            "bridge"
        } else {
            "none"
        })
        .arg("--cap-drop")
        .arg("ALL")
        .arg("--security-opt")
        .arg("no-new-privileges")
        .arg("--memory")
        .arg(format!("{}m", arg_u64(request, "memory_mb").unwrap_or(512)))
        .arg("--cpus")
        .arg(arg_string(request, "cpu_count").unwrap_or_else(|| "1.0".into()))
        .arg("-v")
        .arg(format!("{}:/workspace:rw", root.display()))
        .arg("-w")
        .arg("/workspace")
        .arg(image);
    cmd.args(split_command(command)?);
    Ok(cmd)
}

fn read_file_tool(request: &ToolRequest) -> std::result::Result<Value, String> {
    let root = workspace_root(request)?;
    let path = safe_path(&root, &required_arg(request, "path")?)?;
    let content = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    Ok(json!({ "path": path, "content": content }))
}

fn write_file_tool(request: &ToolRequest) -> std::result::Result<Value, String> {
    let root = workspace_root(request)?;
    let path = safe_path(&root, &required_arg(request, "path")?)?;
    let content = required_arg(request, "content")?;
    if request.dry_run {
        return Ok(json!({ "path": path, "dry_run": true, "bytes": content.len() }));
    }
    std::fs::write(&path, content).map_err(|error| error.to_string())?;
    Ok(json!({ "path": path, "bytes": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) }))
}

fn list_directory_tool(request: &ToolRequest) -> std::result::Result<Value, String> {
    let root = workspace_root(request)?;
    let rel = arg_string(request, "path").unwrap_or_else(|| ".".into());
    let path = safe_path(&root, &rel)?;
    let entries = fs_list(&path)?;
    Ok(json!({ "path": path, "entries": entries }))
}

fn fs_list(path: &Path) -> std::result::Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().to_string();
        if matches!(name.as_str(), ".git" | "target" | ".venv" | "__pycache__") {
            continue;
        }
        entries.push(json!({
            "name": name,
            "type": if entry.path().is_dir() { "dir" } else { "file" }
        }));
    }
    Ok(entries)
}

fn command_for_request(request: &ToolRequest) -> std::result::Result<String, String> {
    if let Some(command) = arg_string(request, "command") {
        return Ok(command);
    }
    let root = workspace_root(request)?;
    match request.name.as_str() {
        "run_tests" if root.join("Cargo.toml").exists() => Ok("cargo test".into()),
        "run_tests" if root.join("pyproject.toml").exists() => Ok("python -m pytest tests".into()),
        "lint_project" if root.join("Cargo.toml").exists() => Ok("cargo check".into()),
        "lint_project" if root.join("pyproject.toml").exists() => Ok("ruff check .".into()),
        _ => Err("no command provided or detected".into()),
    }
}

fn workspace_root(request: &ToolRequest) -> std::result::Result<PathBuf, String> {
    let root = required_arg(request, "root")?;
    let path = PathBuf::from(root)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.is_dir() {
        return Err("workspace root is not a directory".into());
    }
    Ok(path)
}

fn safe_path(root: &Path, rel: &str) -> std::result::Result<PathBuf, String> {
    if rel == "." || rel.is_empty() {
        return Ok(root.to_path_buf());
    }
    if rel.contains("..") {
        return Err("path traversal blocked".into());
    }
    let path = root.join(rel);
    let parent = path.parent().unwrap_or(root);
    let canonical_parent = parent.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_parent.starts_with(root) {
        return Err("path escapes workspace".into());
    }
    Ok(path)
}

struct RuntimeCommandPolicy;

impl RuntimeCommandPolicy {
    fn validate(command: &str) -> std::result::Result<(), String> {
        let lower = command.to_ascii_lowercase();
        for blocked in [
            "sudo",
            " su ",
            "rm -rf /",
            "mkfs",
            "shutdown",
            "reboot",
            "poweroff",
            "chmod -r 777 /",
            "chown -r",
        ] {
            if lower.contains(blocked) {
                return Err(format!("blocked dangerous command pattern `{blocked}`"));
            }
        }
        let parts = split_command(command)?;
        match parts.as_slice() {
            [bin, sub, ..]
                if matches!(
                    (bin.as_str(), sub.as_str()),
                    ("cargo", "test")
                        | ("cargo", "check")
                        | ("cargo", "fmt")
                        | ("cargo", "clippy")
                        | ("python", "-m")
                        | ("python3", "-m")
                        | ("ruff", "check")
                        | ("ruff", "format")
                        | ("npm", "test")
                        | ("npm", "run")
                        | ("pnpm", "test")
                        | ("pnpm", "run")
                        | ("yarn", "test")
                        | ("yarn", "run")
                        | ("go", "test")
                        | ("go", "vet")
                ) =>
            {
                Ok(())
            }
            [bin] if bin == "pytest" => Ok(()),
            _ => Err(format!("command is not allowlisted: `{command}`")),
        }
    }
}

fn split_command(command: &str) -> std::result::Result<Vec<String>, String> {
    let parts = command
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return Err("empty command".into());
    }
    Ok(parts)
}

fn event(task_id: &str, event_type: &str, payload: Value) -> RuntimeEvent {
    RuntimeEvent {
        id: format!("runtime-{}", now_ts().replace([':', '.', '-'], "")),
        task_id: task_id.into(),
        event_type: event_type.into(),
        payload,
        timestamp: now_ts(),
    }
}

fn required_arg(request: &ToolRequest, name: &str) -> std::result::Result<String, String> {
    arg_string(request, name).ok_or_else(|| format!("missing `{name}` argument"))
}

fn arg_string(request: &ToolRequest, name: &str) -> Option<String> {
    request
        .arguments
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn arg_bool(request: &ToolRequest, name: &str) -> bool {
    request
        .arguments
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn arg_u64(request: &ToolRequest, name: &str) -> Option<u64> {
    request.arguments.get(name).and_then(Value::as_u64)
}

fn default_timeout() -> u64 {
    120
}

fn default_memory_mb() -> u64 {
    512
}

fn default_cpu_count() -> f32 {
    1.0
}

fn default_dry_run() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{RuntimeCommandPolicy, ToolRequest, parse_initial_request};

    #[test]
    fn runtime_policy_blocks_dangerous_commands() {
        assert!(RuntimeCommandPolicy::validate("sudo rm -rf /").is_err());
        assert!(RuntimeCommandPolicy::validate("cargo test").is_ok());
    }

    #[test]
    fn parses_python_tool_request() {
        let request = parse_initial_request(
            r#"{"task_id":"task-1","name":"run_tests","arguments":{"root":"."},"dry_run":true}"#,
            "run_tests",
        )
        .unwrap();
        assert_eq!(request.name, "run_tests");
        assert_eq!(request.task_id, "task-1");
    }

    #[test]
    fn parses_runtime_envelope() {
        let request = parse_initial_request(
            r#"{"workspace_root":".","resource_limits":{"use_docker":true,"docker_image":"rust:1-bookworm"},"tool":{"task_id":"task-1","name":"execute_terminal","arguments":{"command":"cargo test"},"dry_run":false}}"#,
            "execute_terminal",
        )
        .unwrap();
        assert_eq!(request.name, "execute_terminal");
        assert_eq!(request.arguments["use_docker"], true);
    }

    #[test]
    fn tool_request_defaults_timeout_and_dry_run() {
        let request: ToolRequest = serde_json::from_str(
            r#"{"task_id":"task-1","name":"list_directory","arguments":{"root":"."}}"#,
        )
        .unwrap();
        assert_eq!(request.timeout_seconds, 120);
        assert!(request.dry_run);
    }
}
