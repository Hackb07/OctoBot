# OctoBot — Deployment & Task Reference

This reference covers production-style setup and optional backends. For hands-on feature examples and use-case recipes, see [User Guide](user-guide.md). For a short first run, see [Quickstart Guide](quickstart.md).

## Prerequisites

| Requirement | Minimum | Recommended |
|-------------|---------|-------------|
| Rust | 1.80+ | 1.85+ (2024 edition) |
| OS | Linux | Linux (systemd journal access) |
| Terminal | 120×30 | 160×40+ (256 color) |
| RAM | 128 MB (no AI) | 2 GB+ (with AI) |

## Quick Install

```bash
git clone https://github.com/your-org/OctoBot.git
cd OctoBot
cargo build --release
./target/release/OctoBot
```

That's it. OctoBot runs immediately with zero configuration — all optional features degrade gracefully when their dependencies are missing.

## Task Checklist — First-time Setup

### 1. System Dependencies

- [ ] **Rust toolchain** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- [ ] **Build tools** — `pkg-config`, `libssl-dev` (Debian) / `openssl-devel` (Fedora)
- [ ] **systemd-journald** — OctoBot streams journalctl logs; ensure your user is in the `systemd-journal` group: `sudo usermod -aG systemd-journal $USER`
- [ ] **Docker socket** (optional) — infra integration: `sudo usermod -aG docker $USER`
- [ ] **kubectl + kubeconfig** (optional) — Kubernetes discovery

### 2. Clone & Build

- [ ] `git clone https://github.com/your-org/OctoBot.git`
- [ ] `cd OctoBot`
- [ ] `cargo build --release` (first build: ~3 min)
- [ ] `cargo test` — verify 37 tests pass
- [ ] `cargo check` — verify zero compilation errors

### 3. Run Without Any Backend

```bash
./target/release/OctoBot
```

Press **`?`** to see the help overlay. OctoBot starts with empty runtime state and populates views from real commands, events, workflows, integrations, and user actions.

### 4. Optional Backends

Each backend is independently optional. OctoBot runs without any of them.

**PostgreSQL** (persistence + replay):

- [ ] Install: `sudo apt install postgresql` or `docker run -d --name pg -e POSTGRES_PASSWORD=octobot -p 5432:5432 postgres:17`
- [ ] Create database: `createdb octobot`
- [ ] Set env: `export OCTOBOT_DATABASE_URL=postgres://postgres:octobot@localhost:5432/octobot`
- [ ] Verify: startup logs "PostgreSQL persistence enabled"

**Qdrant** (semantic memory / vector search):

- [ ] Run: `docker run -d --name qdrant -p 6333:6333 qdrant/qdrant`
- [ ] Set env: `export OCTOBOT_QDRANT_URL=http://localhost:6333`

**Embedding endpoint** (required for Qdrant):

- [ ] Run: e.g. `docker run -d -p 1234:1234 ghcr.io/huggingface/text-embeddings-inference:v1.5 --model-id BAAI/bge-small-en-v1.5`
- [ ] Set env: `export OCTOBOT_EMBEDDING_URL=http://localhost:1234/embed`
- [ ] Verify: Qdrant collection auto-created on first event

**Ollama** (local AI agent reasoning):

- [ ] Install Ollama and start the server.
- [ ] Pull models: `ollama pull llama3.1:8b`, `ollama pull qwen2.5-coder:7b`, `ollama pull deepseek-r1:8b`, `ollama pull phi4`
- [ ] Set env: `export OCTOBOT_OLLAMA_URL=http://localhost:11434`
- [ ] Optional model override: `export OCTOBOT_OLLAMA_MODEL=llama3.1:8b`
- [ ] Or configure at runtime with `/login ollama http://localhost:11434`
- [ ] Verify: completed agent tasks show `Completed`; OctoBot unloads the completed task model with `keep_alive=0`

**Prometheus** (metrics):

- [ ] `export OCTOBOT_PROMETHEUS_URL=http://localhost:9090`

**Loki** (log aggregation):

- [ ] `export OCTOBOT_LOKI_URL=http://localhost:3100`

**OpenSearch** (log storage):

- [ ] `export OCTOBOT_OPENSEARCH_URL=http://localhost:9200`

**Workflow directory** (auto-load DAGs):

- [ ] `mkdir -p workflows && export OCTOBOT_WORKFLOW_DIR=./workflows`
- [ ] Add `.yaml` workflow files (see YAML format below)

**Plugin directory** (auto-load plugins):

- [ ] `mkdir -p plugins && export OCTOBOT_PLUGIN_DIR=./plugins`
- [ ] Add `.json` manifest files (see Plugin Manifest below)

### 5. Environment Variables — Complete Reference

| Variable | Purpose | Default |
|----------|---------|---------|
| `OCTOBOT_OLLAMA_URL` | Ollama server URL | `http://localhost:11434` |
| `OCTOBOT_OLLAMA_MODEL` | Ollama model for runtime login path | `llama3.1:8b` |
| `OCTOBOT_OLLAMA_RETRIES` | Ollama request retry count | `2` |
| `OCTOBOT_DATABASE_URL` | PostgreSQL connection | — |
| `OCTOBOT_QDRANT_URL` | Qdrant vector DB URL | — |
| `OCTOBOT_QDRANT_COLLECTION` | Qdrant collection name | `octobot_operational_memory` |
| `OCTOBOT_EMBEDDING_URL` | Embedding endpoint URL | — |
| `OCTOBOT_DOCKER_SOCKET` | Docker socket path | `/var/run/docker.sock` |
| `OCTOBOT_KUBERNETES_URL` | Kubernetes API URL | — |
| `OCTOBOT_PROMETHEUS_URL` | Prometheus URL | — |
| `OCTOBOT_LOKI_URL` | Loki URL | — |
| `OCTOBOT_OPENSEARCH_URL` | OpenSearch URL | — |
| `OCTOBOT_API_ADDR` | Control API bind | `127.0.0.1:7878` |
| `OCTOBOT_WORKFLOW_DIR` | YAML DAG workflow dir | — |
| `OCTOBOT_PLUGIN_DIR` | Plugin manifest dir | — |
| `OCTOBOT_LOG_LIMIT` | Max in-memory log lines | `120` |
| `OCTOBOT_EVENT_LIMIT` | Max in-memory events | `120` |
| `OCTOBOT_STREAM_CAPTURE_LINES` | Max captured output | `100` |
| `OCTOBOT_QDRANT_RETRY_ATTEMPTS` | Qdrant retry count | `3` |
| `OCTOBOT_AI_MAX_TURNS` | Max AI reasoning turns | `5` |
| `OCTOBOT_TRACE` | Enable tracing output | — |

### 6. Verify Everything Works

- [ ] `cargo test` → 37 passed
- [ ] `cargo check` → zero errors (warnings are expected for unused pub items)
- [ ] Launch with `./target/release/OctoBot` → TUI shows dashboard
- [ ] Press `?` → help overlay appears
- [ ] `/exec uptime` → command runs
- [ ] `/investigate nginx_latency` → incident created
- [ ] `/spawn-agent research` → agent registered and task assigned
- [ ] `/multi-agent Assess Ollama readiness and report findings` → planner/executor flow starts
- [ ] `/tasks-report` → task lifecycle report appears in Reports
- [ ] `/replay start` then `/replay step` → timeline advances
- [ ] `curl http://localhost:7878/health` → API online

## All Tasks — Complete Command Reference

### Incident Management

| Command | What It Does |
|---------|-------------|
| `/investigate <name>` | Creates an incident, triggers investigation DAG, switches to Incidents view |
| `/assign <agent> <task>` | Assigns a natural-language task to a specific agent |
| `/analyze-logs <service>` | Fetches journalctl logs for analysis |
| `/research confidence` | Refreshes the research confidence profile from all evidence signals |

### Infrastructure Commands

| Command | What It Does |
|---------|-------------|
| `/exec uptime` | System uptime |
| `/exec df -h` | Disk usage |
| `/exec ps aux` | Process list |
| `/exec docker ps` | Running containers |
| `/exec kubectl get pods` | Kubernetes pods |
| `/exec systemctl --no-pager --failed` | Failed systemd units |
| `/exec journalctl -n 40 --no-pager` | Recent journal logs |
| `/exec ssh <host> uptime` | Remote host uptime (allowlisted) |

### Remediation

| Command | What It Does |
|---------|-------------|
| `/recover <service>` | Proposes a recovery action (e.g. `systemctl restart <service>`) — requires approval |
| `/approve <action_id>` | Approves a pending recovery (requires Operator or Admin role) |

### Agent Orchestration

| Command | What It Does |
|---------|-------------|
| `/spawn-agent research` | Registers a new AI Research agent |
| `/assign <agent_id> <task>` | Gives an agent a task to execute |
| `/runtime set <name> <kind> <endpoint>` | Registers a distributed runtime (remote/container/cluster) |

### Plugin Management

| Command | What It Does |
|---------|-------------|
| `/plugin add <name> <kind>` | Registers a new plugin (kind: tool/workflow/integration/agent) |
| `/plugin enable <name>` | Enables a registered plugin |
| `/plugin disable <name>` | Disables a plugin |
| `OCTOBOT_PLUGIN_DIR=... cargo run` | Loads plugin manifests from a directory at startup |

Example:
```
/plugin add runbook-search tool
/plugin enable runbook-search
```

### Ollama Login

| Command | What It Does |
|---------|-------------|
| `/login ollama <url>` | Connects to local Ollama (env fallback: `OCTOBOT_OLLAMA_URL`) |

### Replay & Audit

| Command | What It Does |
|---------|-------------|
| `/replay start` | Begins timeline playback from stored events |
| `/replay step` | Advances replay by one event |
| `/replay start` then repeated `/replay step` | Walk through incident timeline step by step |

### Role-Based Access Control

| Command | What It Does |
|---------|-------------|
| `/role admin` | Elevates to Admin (can approve all recoveries) |
| `/role operator` | Sets to Operator (can approve recoveries) |
| `/role readonly` | Read-only (cannot approve) |
| `/role security` | Security Reviewer (cannot approve) |
| `/sandbox policy <role> <keyword>` | Adds a role+keyword to the sandbox approval policy |

### Knowledge Graph

| Command | What It Does |
|---------|-------------|
| `/graph link <from> <rel> <to>` | Adds a knowledge edge (e.g. `graph link deploy-1188 correlates-with inc-042`) |

### Reports

| Command | What It Does |
|---------|-------------|
| `/generate-report <name>` | Exports a JSON report of current state to `reports/` directory |

## What Each Tab Shows

| # | Tab | Live Data Source |
|---|-----|-----------------|
| 1 | **Dashboard** | System health gauge, agent throughput, event count, recovery queue, per-node CPU/mem bars, last 5 events, workflow progress, infra health |
| 2 | **Agents** | Agent table (role/status/confidence/task), coordination graph edges, distributed runtime table |
| 3 | **Incidents** | All incidents with severity, service, status, hypothesis |
| 4 | **Research** | Confidence profile, evidence signals, knowledge graph nodes/edges, explainability records |
| 5 | **Logs** | Live `journalctl -f` streaming (real system logs) |
| 6 | **Infrastructure** | Infra node table, command execution records, time-travel timeline |
| 7 | **Workflows** | DAG workflow progress, autonomous recovery queue with approval status |
| 8 | **Reports** | Generated report text, explainability ledger |
| 9 | **Settings** | Provider config, plugin registry, sandbox policy, distributed runtimes |

## YAML Workflow Format

Place in `OCTOBOT_WORKFLOW_DIR`:

```yaml
id: "incident-response"
name: "Incident Response"
entrypoint: "detect"
nodes:
  - id: "detect"
    kind: "Command"
    command: "journalctl -n 20 --no-pager"
    on_success: "analyze"
    retry:
      attempts: 2
      backoff_ms: 1000
  - id: "analyze"
    kind: "AgentTask"
    agent: "auto"
    command: "Analyze logs for anomalies"
    depends_on: ["detect"]
    on_success: "remediate"
    on_failure: "escalate"
  - id: "remediate"
    kind: "Approval"
    depends_on: ["analyze"]
  - id: "escalate"
    kind: "Command"
    command: "echo Escalating incident"
    depends_on: ["analyze"]
```

## Plugin Manifest Format

Place `.json` files in `OCTOBOT_PLUGIN_DIR`. Pair with an optional `.sh` script for external plugins:

```json
{
  "name": "runbook-search",
  "kind": "Tool",
  "description": "Search operational runbooks",
  "version": "0.1.0",
  "owner": "operator"
}
```

Enable it: `/plugin enable runbook-search`

## REST API

| Endpoint | Method | Returns |
|----------|--------|---------|
| `/health` | GET | `{"ok": true}` |
| `/api/state` | GET | Full JSON state snapshot |
| `/api/replay/events` | GET | All stored OpsEvents |
| `/api/replay/reconstruct` | GET | Full state reconstructed from events |
| `/api/memory/search?q=<query>` | GET | Qdrant semantic search results |
| `/api/incidents/similar?q=<query>` | GET | Incident similarity search |
| `/api/plugins` | GET | All registered plugins |
| `/api/sessions` | GET | All replay sessions |

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                       TUI (ratatui)                      │
│  Dashboard | Agents | Incidents | Research | Logs | ... │
└─────────────────────────┬───────────────────────────────┘
                          │ watch channel
                          ▼
┌─────────────────────────────────────────────────────────┐
│                    Event Loop (runtime)                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐ │
│  │ AI Agent │  │ Workflow │  │ Remediat.│  │ Observab.│ │
│  │ Executor │  │ Engine   │  │ Engine   │  │ Engine   │ │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘ │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐ │
│  │ Infra    │  │ Trace    │  │ Plugin   │  │ Command  │ │
│  │ Discovery│  │ Replay   │  │ Registry │  │ Executor │ │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘ │
└──────────┬───────────────────────────────────────────────┘
           │ mpsc / watch
           ▼
┌─────────────────────────────────────────────────────────┐
│                  Persistence Layer                        │
│  PostgreSQL (events)    Qdrant (vectors)   Axum API      │
└─────────────────────────────────────────────────────────┘

All events flow through a single mpsc channel.
State is snapshotted via watch channel every tick.
PostgreSQL and Qdrant are optional — everything works without them.
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| Blank TUI / garbage characters | Terminal too small | Resize to ≥120×30 |
| TUI shows no logs | systemd-journal group not set | `sudo usermod -aG systemd-journal $USER && newgrp systemd-journal` |
| AI commands return empty | Ollama is not configured or unavailable | Start Ollama and run `/login ollama http://localhost:11434` |
| PostgreSQL not connecting | DB not running or wrong URL | Check `OCTOBOT_DATABASE_URL`, run `docker start pg` |
| Qdrant not connecting | Qdrant not running | Run `docker start qdrant` |
| `/exec uptime` returns nothing | Command timeout (>8s) or not allowlisted | Check command is in allowlist (`docker ps`, `kubectl get pods`, `journalctl`, `systemctl`, `ps aux`, `df -h`, `uptime`, `ssh`) |
| Plugin not found | Wrong directory or broken manifest | Check `OCTOBOT_PLUGIN_DIR`, validate JSON |
| YAML workflow not loading | Syntax error or cycle | Run `cargo test` to catch cycle detection errors |
| `cargo build` fails on nightly | Edition 2024 requires Rust 1.85+ | `rustup update stable` |
