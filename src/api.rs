use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use color_eyre::eyre::{Context, Result};
use serde::Deserialize;
use tokio::{
    net::TcpListener,
    sync::{mpsc, watch},
};
use tracing::info;

use crate::{
    models::{AgentRole, AgentStatus, OpsEvent, OpsState},
    persistence::PersistenceRuntime,
    platform::engineering_os_blueprint,
    utils::{next_agent_name, now_ts},
};

#[derive(Clone)]
struct ApiState {
    state_rx: watch::Receiver<OpsState>,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
    persistence: Arc<PersistenceRuntime>,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
}

pub(crate) async fn serve_api(
    rx: watch::Receiver<OpsState>,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) -> Result<()> {
    let state = ApiState {
        state_rx: rx,
        event_tx,
        persistence: Arc::new(PersistenceRuntime::from_env().await),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/state", get(snapshot))
        .route("/api/platform/capabilities", get(platform_capabilities))
        .route("/api/plugins", get(plugins))
        .route("/api/processes", get(processes))
        .route("/api/syscalls", get(syscalls))
        .route("/api/policy", get(policy))
        .route("/api/apps", get(apps))
        .route("/api/sessions", get(sessions))
        .route("/api/conversation", get(conversation))
        .route("/api/services", get(services))
        .route("/api/workspace", get(workspace))
        .route("/api/kernel/tasks", get(kernel_tasks))
        .route("/api/quotas", get(quotas))
        .route("/api/ipc", get(ipc))
        .route("/api/grants", get(grants))
        .route("/api/packages", get(packages))
        .route("/api/supervisor", get(supervisor))
        .route("/api/boot", get(boot))
        .route("/api/agents/spawn/{role}", post(spawn_agent))
        .route("/api/processes/{agent}/kill", post(kill_agent))
        .route("/api/processes/{agent}/pause", post(pause_agent))
        .route("/api/processes/{agent}/resume", post(resume_agent))
        .route("/api/replay/events", get(replay_events))
        .route("/api/replay/reconstruct", get(reconstruct_state))
        .route("/api/memory/search", get(memory_search))
        .route("/api/incidents/similar", get(incident_similarity_search))
        .with_state(state);
    let addr = std::env::var("OCTOBOT_API_ADDR").unwrap_or_else(|_| "127.0.0.1:7878".into());
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("binding local Axum API on {addr}"))?;
    info!("local control API listening on {addr}");
    axum::serve(listener, app)
        .await
        .context("serving local Axum API")?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "octobot-control-plane" }))
}

async fn snapshot(State(state): State<ApiState>) -> impl IntoResponse {
    Json(state.state_rx.borrow().clone())
}

async fn platform_capabilities() -> impl IntoResponse {
    Json(engineering_os_blueprint())
}

async fn plugins(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "plugins": state.state_rx.borrow().plugins }))
}

async fn processes(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "processes": state.state_rx.borrow().process_table }))
}

async fn syscalls(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "syscalls": state.state_rx.borrow().syscalls }))
}

async fn policy(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "policy": state.state_rx.borrow().sandbox_policy }))
}

async fn apps(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "apps": state.state_rx.borrow().plugins }))
}

async fn sessions(State(state): State<ApiState>) -> impl IntoResponse {
    let replay = state.state_rx.borrow().replay.clone();
    Json(serde_json::json!({ "sessions": [], "current": replay }))
}

async fn conversation(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "messages": state.state_rx.borrow().conversation }))
}

async fn services(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "services": state.state_rx.borrow().system_services }))
}

async fn workspace(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "artifacts": state.state_rx.borrow().workspace_artifacts }))
}

async fn kernel_tasks(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "tasks": state.state_rx.borrow().kernel_tasks }))
}

async fn quotas(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "quotas": state.state_rx.borrow().resource_quotas }))
}

async fn ipc(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "messages": state.state_rx.borrow().ipc_messages }))
}

async fn grants(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "grants": state.state_rx.borrow().policy_grants }))
}

async fn packages(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "packages": state.state_rx.borrow().app_packages }))
}

async fn supervisor(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "events": state.state_rx.borrow().supervisor_events }))
}

async fn boot(State(state): State<ApiState>) -> impl IntoResponse {
    Json(serde_json::json!({ "boot": state.state_rx.borrow().boot_config }))
}

async fn spawn_agent(State(state): State<ApiState>, Path(role): Path<String>) -> impl IntoResponse {
    let role = parse_agent_role(&role);
    let name = next_agent_name();
    let _ = state.event_tx.send(OpsEvent::AgentSpawned {
        name: name.clone(),
        role: role.clone(),
        timestamp: now_ts(),
    });
    Json(serde_json::json!({ "agent": name, "role": format!("{role:?}") }))
}

async fn kill_agent(State(state): State<ApiState>, Path(agent): Path<String>) -> impl IntoResponse {
    lifecycle(&state, agent, AgentStatus::Failed, "killed by API")
}

async fn pause_agent(
    State(state): State<ApiState>,
    Path(agent): Path<String>,
) -> impl IntoResponse {
    lifecycle(&state, agent, AgentStatus::Waiting, "paused by API")
}

async fn resume_agent(
    State(state): State<ApiState>,
    Path(agent): Path<String>,
) -> impl IntoResponse {
    lifecycle(&state, agent, AgentStatus::Running, "resumed by API")
}

fn lifecycle(
    state: &ApiState,
    agent: String,
    status: AgentStatus,
    task: &str,
) -> Json<serde_json::Value> {
    let _ = state.event_tx.send(OpsEvent::AgentLifecycleChanged {
        agent: agent.clone(),
        status: status.clone(),
        task: task.into(),
        timestamp: now_ts(),
    });
    Json(serde_json::json!({ "agent": agent, "status": format!("{status:?}") }))
}

async fn replay_events(State(state): State<ApiState>) -> impl IntoResponse {
    Json(match state.persistence.replay_events().await {
        Ok(events) => serde_json::json!({ "events": events }),
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    })
}

fn parse_agent_role(role: &str) -> AgentRole {
    match role.to_ascii_lowercase().as_str() {
        "planner" => AgentRole::Planner,
        "executor" => AgentRole::Executor,
        "logs" => AgentRole::Logs,
        "triage" => AgentRole::Triage,
        "workflow" => AgentRole::Workflow,
        "report" => AgentRole::Report,
        _ => AgentRole::Research,
    }
}

async fn reconstruct_state(State(state): State<ApiState>) -> impl IntoResponse {
    Json(match state.persistence.reconstruct_state().await {
        Ok(reconstructed) => serde_json::json!({ "state": reconstructed }),
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    })
}

async fn memory_search(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    Json(match state.persistence.semantic_search(&query.q).await {
        Ok(results) => serde_json::json!({ "results": results }),
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    })
}

async fn incident_similarity_search(
    State(state): State<ApiState>,
    Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
    Json(
        match state.persistence.incident_similarity_search(&query.q).await {
            Ok(results) => serde_json::json!({ "results": results }),
            Err(error) => serde_json::json!({ "error": error.to_string() }),
        },
    )
}
