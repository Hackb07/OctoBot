# OctoBot User Guide

This guide explains how to install OctoBot, run it locally, test each major feature, and adapt it to common operations use cases.

## What OctoBot Does

OctoBot is a terminal operations console. It combines a Ratatui dashboard, an event-driven runtime, Ollama-backed AI agents, allowlisted command execution, YAML workflows, recovery approvals, reporting, replay, and optional persistence.

The default local setup runs without external services. Optional services such as Ollama, PostgreSQL, Qdrant, Prometheus, Loki, OpenSearch, Docker, and Kubernetes unlock deeper automation and observability.

## Install

### 1. Install Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup update stable
```

Verify:

```bash
rustc --version
cargo --version
```

### 2. Install Linux Build Packages

Debian or Ubuntu:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev
```

Fedora:

```bash
sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config openssl-devel
```

### 3. Clone and Build

```bash
git clone <repo-url>
cd OctoBot
cargo build
cargo test
```

Expected test result:

```text
37 passed
```

### 4. Run

Development run:

```bash
cargo run
```

Release run:

```bash
cargo build --release
./target/release/OctoBot
```

Controls:

| Key | Action |
|-----|--------|
| `/` | Enter command mode |
| `Enter` | Run command |
| `Tab` | Next view or autocomplete while typing a command |
| `Shift-Tab` | Previous view |
| `1`-`9` | Jump to a view |
| `?` or `h` | Help overlay |
| `q` | Quit |

## Optional Local AI Setup

OctoBot currently uses Ollama for local agent tasks.

Install Ollama, then pull useful models:

```bash
ollama pull llama3.1:8b
ollama pull qwen2.5-coder:7b
ollama pull deepseek-r1:8b
ollama pull phi4
```

Run OctoBot with Ollama:

```bash
export OCTOBOT_OLLAMA_URL="http://localhost:11434"
export OCTOBOT_OLLAMA_MODEL="llama3.1:8b"
cargo run
```

Or configure Ollama from inside the TUI:

```text
/login ollama http://localhost:11434
```

When an AI task completes, OctoBot marks the agent `Completed` and unloads the Ollama model with `keep_alive=0` to release local memory.

## Command Examples

All commands are typed after pressing `/`.

| Command | Example | What Happens |
|---------|---------|--------------|
| `multi-agent` | `/multi-agent Investigate disk pressure and summarize risk` | Spawns a planner agent, decomposes the task, creates executor agents, and tracks completion. |
| `spawn-agent` | `/spawn-agent research` | Creates a research agent and assigns it a startup analysis task. |
| `assign` | `/assign agent-123 Check uptime and disk pressure` | Assigns a task to an existing agent. |
| `tasks-report` | `/tasks-report` | Generates a report of recent agent task events. |
| `investigate` | `/investigate checkout_latency` | Creates an incident and starts investigation state. |
| `exec` | `/exec uptime` | Runs an allowlisted infrastructure command. |
| `analyze-logs` | `/analyze-logs auth-service` | Requests recent journal logs for analysis. |
| `recover` | `/recover nginx` | Proposes a dry-run recovery action that requires approval. |
| `role` | `/role operator` | Switches current role for approval checks. |
| `approve` | `/approve rec-123` | Approves a recovery if the current role can approve it. |
| `research confidence` | `/research confidence` | Refreshes the research confidence profile. |
| `graph link` | `/graph link deploy-1188 correlates-with inc-checkout` | Adds a knowledge graph edge. |
| `generate-report` | `/generate-report checkout_incident` | Writes a JSON report under `reports/`. |
| `plugin add` | `/plugin add runbook-index integration` | Registers a plugin in state. |
| `plugin enable` | `/plugin enable runbook-index` | Marks a plugin enabled. |
| `plugin disable` | `/plugin disable runbook-index` | Marks a plugin disabled. |
| `runtime set` | `/runtime set agent-remote remote ssh://node-01` | Registers a local, remote, container, or cluster runtime endpoint. |
| `sandbox policy` | `/sandbox policy operator restart` | Adds an approval role and review keyword. |
| `replay start` | `/replay start` | Starts replay cursor state. |
| `replay step` | `/replay step` | Advances replay by one event. |
| `login` | `/login ollama http://localhost:11434` | Reconfigures the Ollama endpoint at runtime. |

Allowlisted `/exec` examples:

```text
/exec uptime
/exec df -h
/exec ps aux
/exec docker ps
/exec kubectl get pods
/exec systemctl --no-pager --failed
/exec journalctl -n 40 --no-pager
/exec ssh node-01 uptime
```

Commands outside the allowlist are rejected by the sandbox.

## Feature Walkthroughs

### Dashboard

Use view `1`.

Purpose: See top-level health, active agents, event volume, recovery queue, workflow progress, and infrastructure health.

Try:

```text
/exec uptime
/investigate api_latency
```

Expected: event preview updates, health panels stay visible, and incident/workflow counters change.

### Agent Orchestration

Use view `2`.

Single-agent test:

```text
/spawn-agent research
```

Multi-agent test:

```text
/multi-agent Check local model health, disk pressure, and failed systemd services
```

Expected: agents move through `Waiting`, `Running`, and `Completed`. Completed agents show `[OK]`. If Ollama is available, task reasoning and tool usage events are recorded. If Ollama is not available, you can still test registration and lifecycle events.

### Agent Assignment Testing

1. Spawn an agent:

```text
/spawn-agent research
```

2. Copy the generated agent name from the Agents view.

3. Assign a task:

```text
/assign agent-123 Run an operational summary using uptime and disk information
```

4. Generate the task report:

```text
/tasks-report
```

Expected: Reports view includes spawn, assign, status, memory, plan, and completion events where applicable.

### Incidents

Use view `3`.

```text
/investigate checkout_503
```

Expected: an incident with id `inc-checkout_503` appears with service `operator-request` and severity `SEV3`.

Use case: Create a tracked investigation for a real alert name, then attach evidence with graph links and reports.

### Logs

Use view `5`.

```text
/exec journalctl -n 40 --no-pager
/analyze-logs auth-service
```

Expected: log command output streams into the event/log views. On Linux with journal access, the live journal stream also starts at launch.

### Infrastructure

Use view `6`.

Run local checks:

```text
/exec uptime
/exec df -h
/exec ps aux
```

Enable optional discovery:

```bash
export OCTOBOT_DOCKER_SOCKET="/var/run/docker.sock"
export OCTOBOT_PROMETHEUS_URL="http://localhost:9090"
cargo run
```

Expected: discovery runs every 30 seconds and emits infrastructure snapshots when configured services respond.

### Workflows

Use view `7`.

Create a workflow directory:

```bash
mkdir -p /tmp/octobot-workflows
```

Create `/tmp/octobot-workflows/disk-check.yaml`:

```yaml
id: wf-disk-check
name: Disk Check
entrypoint: collect-disk
nodes:
  - id: collect-disk
    kind: command
    command: df -h
    retry:
      attempts: 2
      backoff_ms: 500
  - id: approve-cleanup
    kind: approval
    depends_on: [collect-disk]
    approval_required: true
```

Run:

```bash
export OCTOBOT_WORKFLOW_DIR="/tmp/octobot-workflows"
cargo run
```

Expected: the workflow loads, executes ready nodes, tracks progress, and creates approval items for approval nodes.

### Recovery and Approval

Use view `7`.

```text
/recover nginx
/role readonly
/approve rec-123
/role operator
/approve rec-123
```

Expected: readonly cannot approve recovery. Operator can approve, which triggers a dry-run command request. Recovery actions remain dry-run unless external policy and command execution are explicitly configured.

### Reports

Use view `8`.

```text
/generate-report incident_checkout
/tasks-report
```

Expected: report output appears in the Reports view and JSON files are written under `reports/`.

### Knowledge Graph and Research

Use view `4`.

```text
/graph link checkout-api depends-on postgres-primary
/graph link deploy-1188 correlates-with inc-checkout_503
/research confidence
```

Expected: knowledge edges and confidence records update. Use this to build an evidence chain while investigating.

### Plugins

Use view `9`.

State-only plugin registration:

```text
/plugin add runbook-index integration
/plugin enable runbook-index
/plugin disable runbook-index
```

Filesystem plugin discovery:

```bash
mkdir -p /tmp/octobot-plugins
```

Create `/tmp/octobot-plugins/runbook-search.json`:

```json
{
  "name": "runbook-search",
  "kind": "Tool",
  "description": "Search operational runbooks",
  "version": "0.1.0",
  "owner": "platform"
}
```

Run:

```bash
export OCTOBOT_PLUGIN_DIR="/tmp/octobot-plugins"
cargo run
```

Expected: plugin manifests are discovered by the registry and can be enabled or disabled.

### Replay and Audit

Use view `6`.

```text
/replay start
/replay step
/replay step
```

Expected: replay cursor advances through recorded events. With PostgreSQL enabled, replay can reconstruct historical state from stored events.

### REST API

Run OctoBot, then query:

```bash
curl http://127.0.0.1:7878/health
curl http://127.0.0.1:7878/api/state
curl http://127.0.0.1:7878/api/plugins
curl http://127.0.0.1:7878/api/replay/events
curl http://127.0.0.1:7878/api/replay/reconstruct
```

With Qdrant and embeddings configured:

```bash
curl "http://127.0.0.1:7878/api/memory/search?q=checkout"
curl "http://127.0.0.1:7878/api/incidents/similar?q=latency"
```

## Use-Case Recipes

### Local Machine Health Check

Use this when you want a quick workstation or server status review.

```text
/exec uptime
/exec df -h
/exec systemctl --no-pager --failed
/multi-agent Summarize local machine health and identify urgent risks
/tasks-report
```

### Incident Triage

Use this when an alert fires for a service.

```text
/investigate checkout_latency
/exec journalctl -n 40 --no-pager
/graph link checkout-api depends-on postgres-primary
/research confidence
/generate-report checkout_latency
```

### AI-Assisted Investigation

Use this when Ollama is running locally and you want agents to decompose work.

```bash
export OCTOBOT_OLLAMA_URL="http://localhost:11434"
export OCTOBOT_OLLAMA_MODEL="llama3.1:8b"
cargo run
```

Then:

```text
/multi-agent Investigate why auth-service latency increased and list likely causes
```

Expected: planner and executor agents appear, task events are recorded, successful agents become `Completed`, and the model unloads after completion.

### Controlled Recovery

Use this when a service needs an approved remediation action.

```text
/recover edge-nginx
/role operator
/approve rec-123
/generate-report recovery_edge_nginx
```

Expected: recovery proposal is tracked, approval is recorded, and execution remains dry-run by default.

### Workflow-Based Runbook

Use this when a repeated operational procedure should run as a DAG.

```bash
export OCTOBOT_WORKFLOW_DIR="./workflows"
cargo run
```

Then keep YAML workflows in `./workflows`. Use command nodes for safe checks, agent nodes for analysis, condition nodes for branching, and approval nodes for human gates.

### Persistent Audit Trail

Use this when you need state reconstruction across sessions.

```bash
export OCTOBOT_DATABASE_URL="postgres://postgres:octobot@localhost:5432/octobot"
cargo run
```

Verify:

```bash
curl http://127.0.0.1:7878/api/replay/events
curl http://127.0.0.1:7878/api/replay/reconstruct
```

## Environment Variables

| Variable | Purpose | Default |
|----------|---------|---------|
| `OCTOBOT_OLLAMA_URL` | Ollama server URL | `http://localhost:11434` |
| `OCTOBOT_OLLAMA_MODEL` | Default Ollama model | `llama3.1:8b` in runtime login path |
| `OCTOBOT_OLLAMA_RETRIES` | Ollama request retry count | `2` |
| `OCTOBOT_AI_MAX_TURNS` | Max AI reasoning turns per task | `5` |
| `OCTOBOT_DATABASE_URL` | PostgreSQL persistence URL | unset |
| `OCTOBOT_QDRANT_URL` | Qdrant URL | unset |
| `OCTOBOT_QDRANT_COLLECTION` | Qdrant collection name | `octobot_operational_memory` |
| `OCTOBOT_EMBEDDING_URL` | Embedding service endpoint | unset |
| `OCTOBOT_DOCKER_SOCKET` | Docker socket path | `/var/run/docker.sock` |
| `OCTOBOT_KUBERNETES_URL` | Kubernetes API URL | unset |
| `OCTOBOT_PROMETHEUS_URL` | Prometheus URL | unset |
| `OCTOBOT_LOKI_URL` | Loki URL | unset |
| `OCTOBOT_OPENSEARCH_URL` | OpenSearch URL | unset |
| `OCTOBOT_API_ADDR` | API bind address | `127.0.0.1:7878` |
| `OCTOBOT_WORKFLOW_DIR` | Workflow YAML directory | unset |
| `OCTOBOT_PLUGIN_DIR` | Plugin manifest directory | unset |
| `OCTOBOT_LOG_LIMIT` | Max in-memory logs | `120` |
| `OCTOBOT_EVENT_LIMIT` | Max in-memory events | `120` |
| `OCTOBOT_STREAM_CAPTURE_LINES` | Max captured command output lines | `100` |
| `OCTOBOT_TRACE` | Enable tracing output | unset |

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `cargo build` fails on Rust edition errors | Run `rustup update stable`. |
| TUI layout looks broken | Use a terminal at least 120 columns wide. |
| Logs are empty | Add your user to `systemd-journal`, then start a new shell. |
| Ollama calls fail | Confirm `ollama serve` is running and `OCTOBOT_OLLAMA_URL` points to it. |
| Agent never completes | Increase `OCTOBOT_AI_MAX_TURNS` or simplify the task. |
| Model memory stays high | Check Ollama logs; OctoBot sends `keep_alive=0` after successful completion. |
| `/exec` command is rejected | Use only allowlisted commands. |
| Workflow does not load | Check YAML syntax, duplicate node ids, missing dependencies, or cycles. |
| API calls fail | Confirm OctoBot is running and `OCTOBOT_API_ADDR` matches the curl URL. |
