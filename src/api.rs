use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
};
use color_eyre::eyre::{Context, Result};
use serde::Deserialize;
use tokio::{net::TcpListener, sync::watch};
use tracing::info;

use crate::{models::OpsState, persistence::PersistenceRuntime};

#[derive(Clone)]
struct ApiState {
    state_rx: watch::Receiver<OpsState>,
    persistence: Arc<PersistenceRuntime>,
}

#[derive(Debug, Deserialize)]
struct SearchQuery {
    q: String,
}

pub(crate) async fn serve_api(rx: watch::Receiver<OpsState>) -> Result<()> {
    let state = ApiState {
        state_rx: rx,
        persistence: Arc::new(PersistenceRuntime::from_env().await),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/state", get(snapshot))
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

async fn replay_events(State(state): State<ApiState>) -> impl IntoResponse {
    Json(match state.persistence.replay_events().await {
        Ok(events) => serde_json::json!({ "events": events }),
        Err(error) => serde_json::json!({ "error": error.to_string() }),
    })
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
