use std::env;
use std::time::Duration;

use color_eyre::eyre::{Context, Result, eyre};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::{debug, info, warn};

use crate::{
    models::{OpsEvent, OpsState},
    security::PersistenceProtector,
    utils::next_id,
};

#[derive(Debug, Clone)]
pub(crate) struct PersistenceConfig {
    pub(crate) database_url: Option<String>,
    pub(crate) qdrant_url: Option<String>,
    pub(crate) qdrant_collection: String,
    pub(crate) embedding_url: Option<String>,
}

impl PersistenceConfig {
    pub(crate) fn from_env() -> Self {
        Self {
            database_url: env::var("OCTOBOT_DATABASE_URL").ok(),
            qdrant_url: env::var("OCTOBOT_QDRANT_URL").ok(),
            qdrant_collection: env::var("OCTOBOT_QDRANT_COLLECTION")
                .unwrap_or_else(|_| "octobot_operational_memory".into()),
            embedding_url: env::var("OCTOBOT_EMBEDDING_URL").ok(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PersistenceRuntime {
    postgres: Option<PostgresStore>,
    memory: Option<SemanticMemory>,
}

impl PersistenceRuntime {
    pub(crate) async fn from_env() -> Self {
        let config = PersistenceConfig::from_env();
        match Self::connect(config).await {
            Ok(runtime) => runtime,
            Err(error) => {
                warn!(%error, "persistent intelligence layer disabled");
                Self {
                    postgres: None,
                    memory: None,
                }
            }
        }
    }

    pub(crate) async fn connect(config: PersistenceConfig) -> Result<Self> {
        let postgres = if let Some(database_url) = config.database_url.as_deref() {
            Some(PostgresStore::connect(database_url).await?)
        } else {
            info!("OCTOBOT_DATABASE_URL is unset; PostgreSQL persistence disabled");
            None
        };

        let memory = match (
            config.qdrant_url.as_deref(),
            config.embedding_url.as_deref(),
        ) {
            (Some(qdrant_url), Some(embedding_url)) => Some(
                SemanticMemory::connect(qdrant_url, &config.qdrant_collection, embedding_url)
                    .await?,
            ),
            (Some(_), None) => {
                warn!(
                    "OCTOBOT_QDRANT_URL is set but OCTOBOT_EMBEDDING_URL is unset; semantic memory indexing disabled"
                );
                None
            }
            _ => {
                info!(
                    "Qdrant semantic memory disabled; set OCTOBOT_QDRANT_URL and OCTOBOT_EMBEDDING_URL to enable it"
                );
                None
            }
        };

        Ok(Self { postgres, memory })
    }

    pub(crate) async fn persist_event(&self, event: &OpsEvent, state: &OpsState) {
        if let Some(postgres) = &self.postgres
            && let Err(error) = postgres.persist_event(event, state).await
        {
            warn!(%error, "failed to persist OpsEvent");
        }
        if let Some(memory) = &self.memory
            && let Err(error) = memory.index_event(event).await
        {
            warn!(%error, "failed to index semantic operational memory");
        }
    }

    pub(crate) async fn replay_events(&self) -> Result<Vec<OpsEvent>> {
        let Some(postgres) = &self.postgres else {
            return Ok(Vec::new());
        };
        postgres.load_events().await
    }

    pub(crate) async fn reconstruct_state(&self) -> Result<OpsState> {
        let events = self.replay_events().await?;
        Ok(reconstruct_state(events))
    }

    pub(crate) async fn semantic_search(&self, query: &str) -> Result<Vec<MemorySearchResult>> {
        let Some(memory) = &self.memory else {
            return Ok(Vec::new());
        };
        memory.search(query, None).await
    }

    pub(crate) async fn incident_similarity_search(
        &self,
        query: &str,
    ) -> Result<Vec<MemorySearchResult>> {
        let Some(memory) = &self.memory else {
            return Ok(Vec::new());
        };
        memory.search(query, Some("incident")).await
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .context("connecting to PostgreSQL")?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("running SQLx migrations")?;
        info!("PostgreSQL persistence enabled");
        Ok(Self { pool })
    }

    async fn persist_event(&self, event: &OpsEvent, state: &OpsState) -> Result<()> {
        let event_json = serde_json::to_value(event).context("serializing OpsEvent")?;
        let mut state_json = serde_json::to_value(state).context("serializing OpsState")?;
        PersistenceProtector::protect_json(&mut state_json);
        let event_type = event_type(event);

        let mut tx = self.pool.begin().await.context("starting persistence tx")?;
        sqlx::query("INSERT INTO ops_events (event_type, event, state) VALUES ($1, $2, $3)")
            .bind(event_type)
            .bind(event_json)
            .bind(state_json)
            .execute(&mut *tx)
            .await
            .context("inserting append-only event")?;

        for incident in &state.incidents {
            sqlx::query(
                "INSERT INTO incidents (id, document, updated_at) VALUES ($1, $2, now())
                 ON CONFLICT (id) DO UPDATE SET document = EXCLUDED.document, updated_at = now()",
            )
            .bind(&incident.id)
            .bind(serde_json::to_value(incident).context("serializing incident")?)
            .execute(&mut *tx)
            .await
            .context("upserting incident")?;
        }

        for workflow in &state.workflows {
            sqlx::query(
                "INSERT INTO workflows (id, document, updated_at) VALUES ($1, $2, now())
                 ON CONFLICT (id) DO UPDATE SET document = EXCLUDED.document, updated_at = now()",
            )
            .bind(&workflow.id)
            .bind(serde_json::to_value(workflow).context("serializing workflow")?)
            .execute(&mut *tx)
            .await
            .context("upserting workflow")?;
        }

        for record in &state.explainability {
            sqlx::query(
                "INSERT INTO explainability_records (id, document, updated_at) VALUES ($1, $2, now())
                 ON CONFLICT (id) DO UPDATE SET document = EXCLUDED.document, updated_at = now()",
            )
            .bind(&record.id)
            .bind(serde_json::to_value(record).context("serializing explainability record")?)
            .execute(&mut *tx)
            .await
            .context("upserting explainability record")?;
        }

        for agent in &state.agents {
            sqlx::query(
                "INSERT INTO agent_state (name, document, updated_at) VALUES ($1, $2, now())
                 ON CONFLICT (name) DO UPDATE SET document = EXCLUDED.document, updated_at = now()",
            )
            .bind(&agent.name)
            .bind(serde_json::to_value(agent).context("serializing agent state")?)
            .execute(&mut *tx)
            .await
            .context("upserting agent state")?;
        }

        tx.commit().await.context("committing persistence tx")?;
        debug!(event_type, "persisted OpsEvent");
        Ok(())
    }

    async fn load_events(&self) -> Result<Vec<OpsEvent>> {
        let rows = sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT event FROM ops_events ORDER BY id ASC",
        )
        .fetch_all(&self.pool)
        .await
        .context("loading replay events")?;

        rows.into_iter()
            .map(|value| serde_json::from_value(value).context("deserializing replay event"))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SemanticMemory {
    qdrant: QdrantClient,
    embeddings: EmbeddingClient,
}

impl SemanticMemory {
    async fn connect(qdrant_url: &str, collection: &str, embedding_url: &str) -> Result<Self> {
        let qdrant = QdrantClient::new(qdrant_url, collection)?;
        qdrant.ensure_collection().await?;
        let embeddings = EmbeddingClient::new(embedding_url)?;
        info!(collection, "Qdrant semantic memory enabled");
        Ok(Self { qdrant, embeddings })
    }

    async fn index_event(&self, event: &OpsEvent) -> Result<()> {
        let Some(document) = memory_document(event) else {
            return Ok(());
        };
        let vector = self.embeddings.embed(&document.text).await?;
        self.qdrant.upsert(document, vector).await
    }

    async fn search(&self, query: &str, source: Option<&str>) -> Result<Vec<MemorySearchResult>> {
        let vector = self.embeddings.embed(query).await?;
        self.qdrant.search(vector, source).await
    }
}

#[derive(Debug, Clone)]
struct QdrantClient {
    base_url: String,
    collection: String,
    client: reqwest::Client,
}

impl QdrantClient {
    fn new(base_url: &str, collection: &str) -> Result<Self> {
        if collection.trim().is_empty() {
            return Err(eyre!("Qdrant collection name cannot be empty"));
        }
        Ok(Self {
            base_url: base_url.trim_end_matches('/').into(),
            collection: collection.into(),
            client: reqwest::Client::new(),
        })
    }

    async fn ensure_collection(&self) -> Result<()> {
        let url = format!("{}/collections/{}", self.base_url, self.collection);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .context("checking Qdrant collection")?;
        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::NOT_FOUND => {
                info!(
                    collection = %self.collection,
                    "Qdrant collection not found; creating with default vector size 384"
                );
                let create_url = format!("{}/collections/{}", self.base_url, self.collection);
                let create_body = json!({
                    "vectors": {
                        "size": 384,
                        "distance": "Cosine"
                    }
                });
                let create_resp = self
                    .client
                    .put(&create_url)
                    .json(&create_body)
                    .send()
                    .await
                    .context("creating Qdrant collection")?;
                if create_resp.status().is_success() {
                    info!(collection = %self.collection, "Qdrant collection created automatically");
                    Ok(())
                } else {
                    let status = create_resp.status();
                    let body = create_resp.text().await.unwrap_or_default();
                    Err(eyre!(
                        "Qdrant collection creation failed with {status}: {body}"
                    ))
                }
            }
            status => Err(eyre!("Qdrant collection check failed with {status}")),
        }
    }

    async fn upsert(&self, document: MemoryDocument, vector: Vec<f32>) -> Result<()> {
        let url = format!(
            "{}/collections/{}/points?wait=true",
            self.base_url, self.collection
        );
        let body = json!({
            "points": [{
                "id": document.id,
                "vector": vector,
                "payload": {
                    "source": document.source,
                    "text": document.text,
                    "metadata": document.metadata
                }
            }]
        });
        retry(|| async {
            let response = self
                .client
                .put(&url)
                .json(&body)
                .send()
                .await
                .context("upserting Qdrant point")?;
            if !response.status().is_success() {
                return Err(eyre!("Qdrant upsert failed with {}", response.status()));
            }
            Ok(())
        })
        .await
    }

    async fn search(
        &self,
        vector: Vec<f32>,
        source: Option<&str>,
    ) -> Result<Vec<MemorySearchResult>> {
        let url = format!(
            "{}/collections/{}/points/search",
            self.base_url, self.collection
        );
        let mut body = json!({
            "vector": vector,
            "limit": 10,
            "with_payload": true
        });
        if let Some(source) = source {
            body["filter"] = json!({
                "must": [{
                    "key": "source",
                    "match": { "value": source }
                }]
            });
        }

        retry(|| async {
            let response = self
                .client
                .post(&url)
                .json(&body)
                .send()
                .await
                .context("searching Qdrant semantic memory")?;
            if !response.status().is_success() {
                return Err(eyre!("Qdrant search failed with {}", response.status()));
            }
            let body: QdrantSearchResponse = response
                .json()
                .await
                .context("decoding Qdrant search response")?;
            Ok(body
                .result
                .into_iter()
                .map(|point| MemorySearchResult {
                    id: point.id,
                    score: point.score,
                    source: point
                        .payload
                        .get("source")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .into(),
                    text: point
                        .payload
                        .get("text")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .into(),
                    metadata: point
                        .payload
                        .get("metadata")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                })
                .collect())
        })
        .await
    }
}

#[derive(Debug, Clone)]
struct EmbeddingClient {
    url: String,
    client: reqwest::Client,
}

impl EmbeddingClient {
    fn new(url: &str) -> Result<Self> {
        if url.trim().is_empty() {
            return Err(eyre!("embedding endpoint cannot be empty"));
        }
        Ok(Self {
            url: url.into(),
            client: reqwest::Client::new(),
        })
    }

    async fn embed(&self, input: &str) -> Result<Vec<f32>> {
        let response = self
            .client
            .post(&self.url)
            .json(&json!({ "input": input }))
            .send()
            .await
            .context("requesting embedding")?;
        if !response.status().is_success() {
            return Err(eyre!(
                "embedding endpoint failed with {}",
                response.status()
            ));
        }
        let body: EmbeddingResponse = response
            .json()
            .await
            .context("decoding embedding response")?;
        if body.embedding.is_empty() {
            return Err(eyre!("embedding endpoint returned an empty vector"));
        }
        Ok(body.embedding)
    }
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct QdrantSearchResponse {
    result: Vec<QdrantSearchPoint>,
}

#[derive(Debug, Deserialize)]
struct QdrantSearchPoint {
    id: serde_json::Value,
    score: f32,
    payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MemorySearchResult {
    id: serde_json::Value,
    score: f32,
    source: String,
    text: String,
    metadata: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct MemoryDocument {
    id: String,
    source: String,
    text: String,
    metadata: serde_json::Value,
}

fn memory_document(event: &OpsEvent) -> Option<MemoryDocument> {
    match event {
        OpsEvent::IncidentDetected {
            incident_id,
            service,
            severity,
            timestamp,
        } => Some(MemoryDocument {
            id: next_id("memory"),
            source: "incident".into(),
            text: format!("{severity} incident {incident_id} detected for {service}"),
            metadata: json!({ "incident_id": incident_id, "service": service, "timestamp": timestamp }),
        }),
        OpsEvent::ExplainabilityRecorded { record } => Some(MemoryDocument {
            id: next_id("memory"),
            source: "explainability".into(),
            text: format!(
                "{}. {} Evidence: {}",
                record.action,
                record.why,
                record.evidence.join("; ")
            ),
            metadata: json!({ "record_id": record.id, "confidence": record.confidence, "timestamp": record.timestamp }),
        }),
        OpsEvent::ResearchCompleted {
            topic,
            conclusion,
            confidence,
            timestamp,
        } => Some(MemoryDocument {
            id: next_id("memory"),
            source: "research".into(),
            text: format!("{topic}: {conclusion}"),
            metadata: json!({ "topic": topic, "confidence": confidence, "timestamp": timestamp }),
        }),
        OpsEvent::CommandExecuted {
            id,
            command,
            success,
            stdout,
            stderr,
            timestamp,
            ..
        } => {
            let text = format!(
                "{} {} {}",
                command,
                if *success { "succeeded" } else { "failed" },
                if stdout.is_empty() { stderr } else { stdout }
            );
            Some(MemoryDocument {
                id: next_id("memory"),
                source: "execution".into(),
                text: crate::security::redact_sensitive(&text),
                metadata: json!({ "execution_id": id, "command": command, "success": success, "timestamp": timestamp }),
            })
        }
        OpsEvent::InfrastructureSnapshotRecorded {
            source,
            nodes,
            timestamp,
        } => Some(MemoryDocument {
            id: next_id("memory"),
            source: "infrastructure".into(),
            text: format!(
                "{} infrastructure snapshot: {}",
                source,
                nodes
                    .iter()
                    .map(|node| format!("{} {} health={}", node.kind, node.name, node.health))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            metadata: json!({ "source": source, "node_count": nodes.len(), "timestamp": timestamp }),
        }),
        OpsEvent::WorkflowDefinitionLoaded { definition } => Some(MemoryDocument {
            id: next_id("memory"),
            source: "workflow".into(),
            text: format!(
                "workflow {} loaded with {} nodes and entrypoint {}",
                definition.name, definition.node_count, definition.entrypoint
            ),
            metadata: json!({ "workflow_id": definition.id, "timestamp": definition.timestamp }),
        }),
        OpsEvent::ToolCallCompleted {
            id,
            tool,
            success,
            output,
            timestamp,
        } => Some(MemoryDocument {
            id: next_id("memory"),
            source: "tool-call".into(),
            text: crate::security::redact_sensitive(&format!(
                "tool {tool} completed success={success}: {output}"
            )),
            metadata: json!({ "tool_call_id": id, "tool": tool, "success": success, "timestamp": timestamp }),
        }),
        _ => None,
    }
}

pub(crate) fn reconstruct_state(events: Vec<OpsEvent>) -> OpsState {
    let mut state = OpsState::empty();
    for event in events {
        state.apply_event(event);
    }
    state
}

async fn retry<F, Fut, T>(mut f: F) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let max_attempts: u32 = std::env::var("OCTOBOT_QDRANT_RETRY_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let mut last_error = None;
    for attempt in 1..=max_attempts {
        match f().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                warn!(attempt, max = max_attempts, %error, "Qdrant operation failed, retrying");
                last_error = Some(error);
                if attempt < max_attempts {
                    tokio::time::sleep(Duration::from_millis(100 * attempt as u64)).await;
                }
            }
        }
    }
    Err(last_error
        .unwrap_or_else(|| eyre!("Qdrant operation failed after {max_attempts} attempts")))
}

pub(crate) fn event_type(event: &OpsEvent) -> &'static str {
    match event {
        OpsEvent::IncidentDetected { .. } => "IncidentDetected",
        OpsEvent::AgentSpawned { .. } => "AgentSpawned",
        OpsEvent::AgentLifecycleChanged { .. } => "AgentLifecycleChanged",
        OpsEvent::AgentTelemetryRecorded { .. } => "AgentTelemetryRecorded",
        OpsEvent::AiProviderRegistered { .. } => "AiProviderRegistered",
        OpsEvent::ToolCallRequested { .. } => "ToolCallRequested",
        OpsEvent::ToolCallCompleted { .. } => "ToolCallCompleted",
        OpsEvent::ReasoningChunkRecorded { .. } => "ReasoningChunkRecorded",
        OpsEvent::TokenUsageRecorded { .. } => "TokenUsageRecorded",
        OpsEvent::ModelHealthUpdated { .. } => "ModelHealthUpdated",
        OpsEvent::NotificationRaised { .. } => "NotificationRaised",
        OpsEvent::TaskAssigned { .. } => "TaskAssigned",
        OpsEvent::CommandRequested { .. } => "CommandRequested",
        OpsEvent::CommandOutput { .. } => "CommandOutput",
        OpsEvent::CommandExecuted { .. } => "CommandExecuted",
        OpsEvent::ResearchCompleted { .. } => "ResearchCompleted",
        OpsEvent::WorkflowAdvanced { .. } => "WorkflowAdvanced",
        OpsEvent::ExplainabilityRecorded { .. } => "ExplainabilityRecorded",
        OpsEvent::AgentMessageRecorded { .. } => "AgentMessageRecorded",
        OpsEvent::TimelineRecorded { .. } => "TimelineRecorded",
        OpsEvent::RecoveryProposed { .. } => "RecoveryProposed",
        OpsEvent::RecoveryApproved { .. } => "RecoveryApproved",
        OpsEvent::ResearchConfidenceUpdated { .. } => "ResearchConfidenceUpdated",
        OpsEvent::PluginRegistered { .. } => "PluginRegistered",
        OpsEvent::PluginStatusChanged { .. } => "PluginStatusChanged",
        OpsEvent::RuntimeUpdated { .. } => "RuntimeUpdated",
        OpsEvent::KnowledgeNodeEnsured { .. } => "KnowledgeNodeEnsured",
        OpsEvent::KnowledgeEdgeAdded { .. } => "KnowledgeEdgeAdded",
        OpsEvent::SandboxPolicyUpdated { .. } => "SandboxPolicyUpdated",
        OpsEvent::RoleChanged { .. } => "RoleChanged",
        OpsEvent::ReplayStarted { .. } => "ReplayStarted",
        OpsEvent::ReplayStepped { .. } => "ReplayStepped",
        OpsEvent::UserCommandEntered { .. } => "UserCommandEntered",
        OpsEvent::MetricsSampled { .. } => "MetricsSampled",
        OpsEvent::InfrastructureSnapshotRecorded { .. } => "InfrastructureSnapshotRecorded",
        OpsEvent::WorkflowDefinitionLoaded { .. } => "WorkflowDefinitionLoaded",
        OpsEvent::AgentProcessUpdated { .. } => "AgentProcessUpdated",
        OpsEvent::SyscallRecorded { .. } => "SyscallRecorded",
        OpsEvent::ConversationMessageRecorded { .. } => "ConversationMessageRecorded",
        OpsEvent::KernelTaskScheduled { .. } => "KernelTaskScheduled",
        OpsEvent::WorkspaceArtifactRecorded { .. } => "WorkspaceArtifactRecorded",
        OpsEvent::SystemServiceUpdated { .. } => "SystemServiceUpdated",
        OpsEvent::AgenticAppInstalled { .. } => "AgenticAppInstalled",
        OpsEvent::ResourceQuotaUpdated { .. } => "ResourceQuotaUpdated",
        OpsEvent::IpcMessageRecorded { .. } => "IpcMessageRecorded",
        OpsEvent::PolicyGrantUpdated { .. } => "PolicyGrantUpdated",
        OpsEvent::AgentMemoryEntryRecorded { .. } => "AgentMemoryEntryRecorded",
        OpsEvent::AppPackageImported { .. } => "AppPackageImported",
        OpsEvent::SupervisorEventRecorded { .. } => "SupervisorEventRecorded",
        OpsEvent::BootCompleted { .. } => "BootCompleted",
        OpsEvent::WorkflowNodeCompleted { .. } => "WorkflowNodeCompleted",
        OpsEvent::AiProviderLogin { .. } => "AiProviderLogin",
        OpsEvent::AgentMemoryStored { .. } => "AgentMemoryStored",
        OpsEvent::PlanCreated { .. } => "PlanCreated",
        OpsEvent::SubTaskCompleted { .. } => "SubTaskCompleted",
    }
}
