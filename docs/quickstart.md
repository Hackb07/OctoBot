# OctoBot Quickstart Guide

Run the application and test all features with sample data.

## Prerequisites

- Rust toolchain (`rustup` recommended)
- Linux with `journalctl` and `systemctl` (best experience)
- Optional: Docker, kubectl, ssh for richer commands

## Quick Run

```bash
git clone <repo-url>
cd OctoBot
cargo run
```

Press **1-9** to switch views, **Tab** to cycle, **q** to quit.

## Command Reference

All commands use the **`/`** prefix. Press **`/`** to enter command mode, type a command, then press **Enter**.

| Command | Description |
|---------|-------------|
| `/investigate nginx_latency` | Create an incident and start investigation workflow |
| `/spawn-agent research` | Register a dynamic AI agent with tool-call execution |
| `/exec uptime` | Run a real allowlisted infrastructure command |
| `/exec df -h` | Check disk space |
| `/exec journalctl -n 40 --no-pager` | Tail recent system logs |
| `/analyze-logs auth-service` | Request log analysis |
| `/generate-report incident_042` | Export a JSON report to `reports/` |
| `/login openai sk-xxx` | Configure OpenAI API key at runtime (no restart) |
| `/login ollama http://localhost:11434` | Configure Ollama at runtime |
| `/login openrouter sk-xxx` | Configure OpenRouter at runtime |
| `/recover edge-nginx` | Propose a recovery action |
| `/approve rec-0001` | Approve a recovery (requires Admin/Operator role) |
| `/role operator` | Switch to Operator role |
| `/research confidence` | Refresh the research confidence profile |
| `/plugin add my-tool tool` | Register a new plugin |
| `/plugin enable openrouter-research` | Enable a plugin |
| `/graph link deploy-1188 correlates-with inc-042` | Add a knowledge graph edge |
| `/sandbox policy operator restart` | Update sandbox approval policy |
| `/runtime set agent-remote remote ssh://node-01` | Register a distributed runtime |
| `/replay start` | Start replaying recorded events |
| `/replay step` | Step through events |
| /multi-agent Investigate why auth-service latency spiked after last deploy ||
## Testing All Features Step-by-Step

### 1. Dashboard (Tab 1)

Start the app. You'll see:
- Prometheus SLO burn, Agent throughput, Evidence coverage gauges
- Workflow Monitor and Infrastructure panels
- Top bar showing workspace, env, health, alerts, uptime

### 2. Agent Orchestration (Tab 2)

```text
/spawn-agent research
```

Expected: A new agent row appears with `[~]` status. The Coordination Graph and Distributed Execution panels update. If an AI provider is configured, `ToolCallRequested` and `ToolCallCompleted` events fire.

```text
/runtime set my-agent remote ssh://worker-01
```

Expected: A runtime entry appears in the Distributed Execution table.

### 3. Incidents (Tab 3)

```text
/investigate test_incident
```

Expected: A new incident `inc-test_incident` appears. The hardcoded incident workflow starts (detect → investigate → validate → report → remediate). Watch progress advance in the Workflow view.

### 4. Research (Tab 4)

```text
/research confidence
```

Expected: The research tree updates with live data from infrastructure, incidents, workflows, and knowledge graph edges — no more hardcoded lines.

```text
/graph link edge-nginx depends-on postgres-primary
```

Expected: A knowledge graph edge appears under "knowledge graph:" in the research view.

### 5. Logs (Tab 5)

```text
/exec journalctl -n 30 --no-pager
```

Expected: Real system log lines stream into the Logs view. Try also:
```text
/exec uptime
/exec df -h
/exec ps aux
```

The continuous `journalctl -f` stream starts automatically at launch.

### 6. Infrastructure (Tab 6)

Infrastructure nodes appear here (seeded demo nodes + real discovered nodes from Docker/K8s/Prometheus if configured). The timeline table correlates deployments, incidents, commands, and metrics.

### 7. Workflows (Tab 7)

Expected: Active workflows appear here. The incident workflow from `/investigate` advances through stages. Recovery actions appear in the Autonomous Recovery Queue.

```text
/recover edge-nginx
/role operator
/approve rec-0001
```

Expected: A recovery is proposed, then approved, triggering a dry-run command.

### 8. Reports (Tab 8)

```text
/generate-report my_test_report
```

Expected: A JSON report is written to `reports/report-<id>.json`. It appears in the Reports view with the explainability records.

### 9. Settings (Tab 9)

Shows plugin registry, integration settings, sandbox policy, and platform completion status.

```text
/plugin add my-indexer integration
/plugin enable my-indexer
/sandbox policy admin rollback
```

Expected: Plugin table updates. Sandbox policy updates.

## Testing with AI Providers

Set one of these before launching:

### OpenAI
```bash
export OCTOBOT_OPENAI_API_KEY="sk-your-key-here"
cargo run
```

### Ollama (local)
```bash
export OCTOBOT_OLLAMA_URL="http://localhost:11434"
export OCTOBOT_OLLAMA_MODEL="llama3.1"
cargo run
```

### OpenRouter
```bash
export OCTOBOT_OPENROUTER_API_KEY="sk-your-key-here"
cargo run
```

When AI is configured, `/spawn-agent` triggers `AiClient::run_agent_turn()` and emits `ToolCallRequested` / `ToolCallCompleted` events visible in the timeline.

### Runtime Login (no restart)

You can also configure an AI provider while the app is running:

```text
/login openai sk-your-key-here           # uses OCTOBOT_OPENAI_MODEL / OCTOBOT_OPENAI_BASE_URL
/login ollama http://localhost:11434     # appends /api/chat, uses OCTOBOT_OLLAMA_MODEL
/login openrouter sk-your-key-here       # uses OCTOBOT_OPENROUTER_MODEL
```

No restart needed. The provider is immediately available for `/spawn-agent`. API keys are never persisted to the event store.

## Testing Persistence

### PostgreSQL
```bash
export OCTOBOT_DATABASE_URL="postgres://user:pass@localhost:5432/octobot"
cargo run
# Events, incidents, workflows, agents are persisted.
# curl http://127.0.0.1:7878/api/replay/events
# curl http://127.0.0.1:7878/api/replay/reconstruct
```

### Qdrant Vector Memory
```bash
export OCTOBOT_QDRANT_URL="http://localhost:6333"
export OCTOBOT_QDRANT_COLLECTION="octobot_operational_memory"
export OCTOBOT_EMBEDDING_URL="http://localhost:8080/embed"
cargo run
# Collection auto-creates if missing.
# curl http://127.0.0.1:7878/api/memory/search?q=incident
# curl http://127.0.0.1:7878/api/incidents/similar?q=nginx
```

## Testing Real Infrastructure Integrations

```bash
export OCTOBOT_DOCKER_SOCKET="/var/run/docker.sock"
export OCTOBOT_KUBERNETES_URL="https://127.0.0.1:6443"
export OCTOBOT_PROMETHEUS_URL="http://127.0.0.1:9090"
export OCTOBOT_LOKI_URL="http://127.0.0.1:3100"
export OCTOBOT_OPENSEARCH_URL="http://127.0.0.1:9200"
cargo run
# Every 30s, infra discovery runs and enriches nodes with Loki/OpenSearch data.
```

## Testing YAML DAG Workflows

Create a workflow definition:

```bash
mkdir -p /tmp/octobot-workflows
cat > /tmp/octobot-workflows/disk-check.yaml << 'EOF'
id: wf-disk-pressure
name: Disk Pressure Investigation
entrypoint: collect-disk
nodes:
  - id: collect-disk
    kind: command
    command: df -h
    retry:
      attempts: 2
      backoff_ms: 500
  - id: approve-recovery
    kind: approval
    depends_on: [collect-disk]
    approval_required: true
EOF
export OCTOBOT_WORKFLOW_DIR="/tmp/octobot-workflows"
cargo run
```

Expected: The workflow definition loads, appears in the Workflows view, and its progress advances via the DAG scheduler.

## Environment Variables Reference

| Variable | Default | Description |
|----------|---------|-------------|
| `OCTOBOT_OPENAI_API_KEY` | — | OpenAI API key |
| `OCTOBOT_OLLAMA_URL` | — | Ollama endpoint URL |
| `OCTOBOT_OPENROUTER_API_KEY` | — | OpenRouter API key |
| `OCTOBOT_DATABASE_URL` | — | PostgreSQL connection string |
| `OCTOBOT_QDRANT_URL` | — | Qdrant vector DB URL |
| `OCTOBOT_QDRANT_COLLECTION` | `octobot_operational_memory` | Qdrant collection name |
| `OCTOBOT_EMBEDDING_URL` | — | Embedding endpoint URL |
| `OCTOBOT_DOCKER_SOCKET` | `/var/run/docker.sock` | Docker socket path |
| `OCTOBOT_KUBERNETES_URL` | — | Kubernetes API URL |
| `OCTOBOT_PROMETHEUS_URL` | — | Prometheus URL |
| `OCTOBOT_LOKI_URL` | — | Loki URL |
| `OCTOBOT_OPENSEARCH_URL` | — | OpenSearch URL |
| `OCTOBOT_API_ADDR` | `127.0.0.1:7878` | Control API bind address |
| `OCTOBOT_WORKFLOW_DIR` | — | YAML workflow directory |
| `OCTOBOT_LOG_LIMIT` | `120` | Max in-memory log lines |
| `OCTOBOT_EVENT_LIMIT` | `120` | Max in-memory events |
| `OCTOBOT_STREAM_CAPTURE_LINES` | `100` | Max captured output lines |
| `OCTOBOT_PREVIEW_LINES` | `12` | Max preview lines |
| `OCTOBOT_QDRANT_RETRY_ATTEMPTS` | `3` | Qdrant retry count |
| `OCTOBOT_TRACE` | — | Set to enable tracing output |
| `OCTOBOT_OPENAI_BASE_URL` | `https://api.openai.com/v1/chat/completions` | OpenAI API endpoint (used by `/login`) |
| `OCTOBOT_OPENAI_MODEL` | `gpt-4.1-mini` | OpenAI model (used by `/login`) |
| `OCTOBOT_OLLAMA_MODEL` | `llama3.1` | Ollama model (used by `/login`) |
| `OCTOBOT_OPENROUTER_MODEL` | `openrouter/free` | OpenRouter model (used by `/login`) |

## Verifying the API

```bash
curl http://127.0.0.1:7878/health
curl http://127.0.0.1:7878/api/state | jq .health
curl http://127.0.0.1:7878/api/memory/search?q=incident
```

## Running Tests

```bash
cargo test  # 19 unit tests covering all features
```
