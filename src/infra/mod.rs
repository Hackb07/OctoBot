use std::{collections::HashMap, env, path::Path};

use color_eyre::eyre::{Context, Result, eyre};
use serde_json::Value;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

use crate::models::{InfraNode, KnowledgeEdge};

#[derive(Debug, Clone)]
pub(crate) struct InfraIntegrations {
    pub(crate) docker_socket: Option<String>,
    pub(crate) kubernetes_url: Option<String>,
    pub(crate) prometheus_url: Option<String>,
    pub(crate) loki_url: Option<String>,
    pub(crate) opensearch_url: Option<String>,
    pub(crate) postgres_url: Option<String>,
}

impl InfraIntegrations {
    pub(crate) fn from_env() -> Self {
        let kubernetes_url = match (
            env::var("OCTOBOT_KUBERNETES_URL").ok(),
            env::var("KUBERNETES_SERVICE_HOST").ok(),
            env::var("KUBERNETES_SERVICE_PORT").ok(),
        ) {
            (Some(url), _, _) => Some(url),
            (None, Some(host), port) => Some(format!(
                "https://{}:{}",
                host,
                port.unwrap_or_else(|| "443".into())
            )),
            _ => None,
        };
        Self {
            docker_socket: env::var("OCTOBOT_DOCKER_SOCKET").ok().or_else(|| {
                Path::new("/var/run/docker.sock")
                    .exists()
                    .then(|| "/var/run/docker.sock".into())
            }),
            kubernetes_url,
            prometheus_url: env::var("OCTOBOT_PROMETHEUS_URL").ok(),
            loki_url: env::var("OCTOBOT_LOKI_URL").ok(),
            opensearch_url: env::var("OCTOBOT_OPENSEARCH_URL").ok(),
            postgres_url: env::var("OCTOBOT_DATABASE_URL").ok(),
        }
    }

    pub(crate) async fn discover(&self) -> Result<Vec<InfraNode>> {
        let mut nodes = Vec::new();
        if let Some(socket) = &self.docker_socket {
            nodes.extend(discover_docker(socket).await?);
        }
        if let Some(url) = &self.kubernetes_url {
            nodes.extend(discover_kubernetes(url).await?);
        }
        if let Some(url) = &self.prometheus_url {
            nodes.extend(discover_prometheus(url).await?);
        }
        if let Some(url) = &self.postgres_url {
            nodes.extend(discover_postgres(url).await?);
        }
        self.enrich_from_loki_and_opensearch(&mut nodes).await;
        Ok(nodes)
    }

    /// Build a topology map from the current infra node list.
    /// Returns knowledge edges discovered from container → pod, pod → service,
    /// and service → database patterns based on node names and kinds.
    pub(crate) fn build_topology(&self, nodes: &[InfraNode]) -> Vec<KnowledgeEdge> {
        let mut edges = Vec::new();
        let by_kind: HashMap<&str, Vec<&InfraNode>> = nodes.iter().fold(HashMap::new(), |mut acc, n| {
            acc.entry(n.kind.as_str()).or_default().push(n);
            acc
        });

        // Container → pod: link docker containers to kubernetes pods by name prefix
        if let Some(containers) = by_kind.get("docker-container") {
            if let Some(pods) = by_kind.get("kubernetes-pod") {
                for container in containers {
                    let container_base = container.name.split('-').next().unwrap_or(&container.name);
                    for pod in pods {
                        if pod.name.starts_with(container_base) {
                            edges.push(KnowledgeEdge {
                                from: container.name.clone(),
                                relation: "runs-on".into(),
                                to: pod.name.clone(),
                                weight: 85,
                                timestamp: crate::utils::now_ts(),
                            });
                        }
                    }
                }
            }
        }

        // Pod → service: link pods to services by matching name prefix
        if let Some(pods) = by_kind.get("kubernetes-pod") {
            for pod in pods {
                let pod_base = pod.name.split('-').next().unwrap_or(&pod.name);
                for node in nodes {
                    if node.kind == "service" && node.name.starts_with(pod_base) {
                        edges.push(KnowledgeEdge {
                            from: pod.name.clone(),
                            relation: "belongs-to".into(),
                            to: node.name.clone(),
                            weight: 90,
                            timestamp: crate::utils::now_ts(),
                        });
                    }
                }
            }
        }

        // Service → database: link services to databases by name substring
        for node in nodes {
            if node.kind == "service" || node.kind == "deployment" {
                let svc_name = node.name.split('-').next().unwrap_or(&node.name);
                for db_node in nodes {
                    if db_node.kind == "database" || db_node.kind == "vector-db" {
                        if db_node.name.contains(svc_name)
                            || svc_name.contains(&db_node.name.trim_end_matches("-primary").trim_end_matches("-vector"))
                        {
                            edges.push(KnowledgeEdge {
                                from: node.name.clone(),
                                relation: "connects-to".into(),
                                to: db_node.name.clone(),
                                weight: 80,
                                timestamp: crate::utils::now_ts(),
                            });
                        }
                    }
                }
            }
        }

        edges
    }

    pub(crate) async fn query_loki(&self, query: &str) -> Result<Value> {
        let Some(base_url) = &self.loki_url else {
            return Err(eyre!("OCTOBOT_LOKI_URL is not configured"));
        };
        let url = format!(
            "{}/loki/api/v1/query?query={}",
            base_url.trim_end_matches('/'),
            query
        );
        get_json(&url).await
    }

    pub(crate) async fn query_opensearch(&self, index: &str, body: Value) -> Result<Value> {
        let Some(base_url) = &self.opensearch_url else {
            return Err(eyre!("OCTOBOT_OPENSEARCH_URL is not configured"));
        };
        let url = format!("{}/{}/_search", base_url.trim_end_matches('/'), index);
        let response = reqwest::Client::new()
            .post(url)
            .json(&body)
            .send()
            .await
            .context("querying OpenSearch")?;
        if !response.status().is_success() {
            return Err(eyre!("OpenSearch query failed with {}", response.status()));
        }
        response
            .json()
            .await
            .context("decoding OpenSearch response")
    }

    pub(crate) async fn enrich_from_loki_and_opensearch(&self, nodes: &mut Vec<InfraNode>) {
        if let Err(error) = self.enrich_from_loki(nodes).await {
            tracing::warn!(%error, "Loki enrichment failed");
        }
        if let Err(error) = self.enrich_from_opensearch(nodes).await {
            tracing::warn!(%error, "OpenSearch enrichment failed");
        }
    }

    async fn enrich_from_loki(&self, nodes: &mut Vec<InfraNode>) -> Result<()> {
        let Some(base_url) = &self.loki_url else {
            return Ok(());
        };
        for node in nodes.iter_mut() {
            let query = format!(r#"{{job="{name}"}}"#, name = node.name);
            let url = format!(
                "{}/loki/api/v1/query_range?query={}&limit=1",
                base_url.trim_end_matches('/'),
                urlencoding(&query)
            );
            if let Ok(value) = get_json(&url).await {
                if let Some(data) = value.get("data") {
                    if let Some(results) = data.get("result") {
                        if let Some(results_arr) = results.as_array() {
                            if !results_arr.is_empty() {
                                tracing::debug!(node = %node.name, "found Loki logs");
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn enrich_from_opensearch(&self, nodes: &mut Vec<InfraNode>) -> Result<()> {
        let Some(base_url) = &self.opensearch_url else {
            return Ok(());
        };
        for node in nodes.iter_mut() {
            let body = serde_json::json!({
                "query": {
                    "match": { "service": node.name }
                },
                "size": 1
            });
            let url = format!("{}/_search", base_url.trim_end_matches('/'));
            if let Ok(response) = reqwest::Client::new()
                .post(&url)
                .json(&body)
                .send()
                .await
            {
                if response.status().is_success() {
                    tracing::debug!(node = %node.name, "found OpenSearch data");
                }
            }
        }
        Ok(())
    }
}

fn urlencoding(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".into(),
            '{' => "%7B".into(),
            '}' => "%7D".into(),
            '"' => "%22".into(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

async fn discover_docker(socket: &str) -> Result<Vec<InfraNode>> {
    let response = docker_get(socket, "/containers/json").await?;
    let containers =
        serde_json::from_str::<Value>(&response).context("decoding Docker API response")?;
    let nodes = containers
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|container| {
            let id = container
                .get("Id")?
                .as_str()?
                .chars()
                .take(12)
                .collect::<String>();
            let name = container
                .get("Names")
                .and_then(Value::as_array)
                .and_then(|names| names.first())
                .and_then(Value::as_str)
                .map(|name| name.trim_start_matches('/').to_string())
                .unwrap_or(id);
            let state = container
                .get("State")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Some(InfraNode {
                name,
                kind: "docker-container".into(),
                health: if state == "running" { 100 } else { 0 },
                cpu: 0,
                memory: 0,
            })
        })
        .collect();
    Ok(nodes)
}

async fn docker_get(socket: &str, path: &str) -> Result<String> {
    let mut stream = UnixStream::connect(socket)
        .await
        .with_context(|| format!("connecting Docker socket {socket}"))?;
    let request = format!("GET {path} HTTP/1.1\r\nHost: docker\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .context("writing Docker API request")?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .await
        .context("reading Docker API response")?;
    response
        .split("\r\n\r\n")
        .nth(1)
        .map(str::to_string)
        .ok_or_else(|| eyre!("Docker API returned malformed HTTP response"))
}

async fn discover_kubernetes(base_url: &str) -> Result<Vec<InfraNode>> {
    let token = std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/token").ok();
    let url = format!("{}/api/v1/pods", base_url.trim_end_matches('/'));
    let mut request = reqwest::Client::new().get(url);
    if let Some(token) = token {
        request = request.bearer_auth(token.trim());
    }
    let response = request
        .send()
        .await
        .context("querying Kubernetes API pods")?;
    if !response.status().is_success() {
        return Err(eyre!(
            "Kubernetes API query failed with {}",
            response.status()
        ));
    }
    let value: Value = response.json().await.context("decoding Kubernetes pods")?;
    Ok(value
        .get("items")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|pod| {
            let metadata = pod.get("metadata")?;
            let status = pod.get("status")?;
            let name = metadata.get("name")?.as_str()?.to_string();
            let phase = status
                .get("phase")
                .and_then(Value::as_str)
                .unwrap_or("Unknown");
            Some(InfraNode {
                name,
                kind: "kubernetes-pod".into(),
                health: if phase == "Running" { 100 } else { 0 },
                cpu: 0,
                memory: 0,
            })
        })
        .collect())
}

async fn discover_prometheus(base_url: &str) -> Result<Vec<InfraNode>> {
    let url = format!("{}/api/v1/query?query=up", base_url.trim_end_matches('/'));
    let value = get_json(&url).await?;
    Ok(value
        .pointer("/data/result")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|sample| {
            let metric = sample.get("metric")?;
            let name = metric
                .get("instance")
                .or_else(|| metric.get("job"))?
                .as_str()?
                .to_string();
            let up = sample
                .get("value")
                .and_then(Value::as_array)
                .and_then(|value| value.get(1))
                .and_then(Value::as_str)
                .unwrap_or("0");
            Some(InfraNode {
                name,
                kind: "prometheus-target".into(),
                health: if up == "1" { 100 } else { 0 },
                cpu: 0,
                memory: 0,
            })
        })
        .collect())
}

async fn discover_postgres(database_url: &str) -> Result<Vec<InfraNode>> {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .context("connecting to PostgreSQL for discovery")?;

    let mut nodes = Vec::new();

    // Query active connections
    match sqlx::query_as::<_, (String, String, String, i32)>(
        "SELECT datname, state, query, COALESCE(pid, 0)::int4 FROM pg_stat_activity WHERE datname IS NOT NULL LIMIT 20"
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => {
            for (datname, state, _query, _pid) in &rows {
                nodes.push(InfraNode {
                    name: format!("pg-{datname}"),
                    kind: "postgres-database".into(),
                    health: if state == "active" { 100 } else { 80 },
                    cpu: 0,
                    memory: 0,
                });
            }
        }
        Err(error) => {
            tracing::warn!(%error, "pg_stat_activity query failed");
            // Fall back to a single node if activity query fails
            nodes.push(InfraNode {
                name: "postgres".into(),
                kind: "postgres-database".into(),
                health: 80,
                cpu: 0,
                memory: 0,
            });
        }
    }

    // Query database-level stats
    match sqlx::query_as::<_, (String, f64, f64)>(
        "SELECT datname, COALESCE(blks_hit::float8 / NULLIF(blks_hit + blks_read, 0), 0) * 100, \
         COALESCE(xact_commit::float8 / NULLIF(xact_commit + xact_rollback, 0), 0) * 100 \
         FROM pg_stat_database WHERE datname NOT IN ('template0', 'template1', 'postgres') LIMIT 10"
    )
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => {
            for (datname, cache_hit_ratio, _commit_ratio) in &rows {
                // Update health based on cache hit ratio
                if let Some(node) = nodes.iter_mut().find(|n| n.name == format!("pg-{datname}")) {
                    node.health = cache_hit_ratio.round().min(100.0) as u8;
                } else {
                    nodes.push(InfraNode {
                        name: format!("pg-{datname}"),
                        kind: "postgres-database".into(),
                        health: cache_hit_ratio.round().min(100.0) as u8,
                        cpu: 0,
                        memory: 0,
                    });
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, "pg_stat_database query failed");
        }
    }

    let _ = pool.close().await;
    Ok(nodes)
}

async fn get_json(url: &str) -> Result<Value> {
    let response = reqwest::get(url)
        .await
        .with_context(|| format!("GET {url}"))?;
    if !response.status().is_success() {
        return Err(eyre!("GET {url} failed with {}", response.status()));
    }
    response.json().await.context("decoding JSON response")
}
