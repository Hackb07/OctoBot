# OctoBot Quickstart Guide

This guide gets OctoBot running locally and walks through a small test path. For detailed examples for every feature, see [User Guide](user-guide.md). For production-style setup, see [Deployment Reference](deployment.md).

## Prerequisites

- Rust stable toolchain.
- Linux terminal, ideally 120 columns or wider.
- Optional: Ollama for local AI agent tasks.
- Optional: Docker, kubectl, PostgreSQL, Qdrant, Prometheus, Loki, or OpenSearch for integrations.

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup update stable
```

Install Linux build dependencies:

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev
```

## Install and Run

```bash
git clone <repo-url>
cd OctoBot
cargo test
cargo run
```

Expected tests:

```text
37 passed
```

Press `/` for commands, `1`-`9` for views, `Tab` to cycle views, `?` for help, and `q` to quit.

## Optional Ollama Setup

OctoBot's current AI task runtime is Ollama-focused.

```bash
ollama pull llama3.1:8b
export OCTOBOT_OLLAMA_URL="http://localhost:11434"
export OCTOBOT_OLLAMA_MODEL="llama3.1:8b"
cargo run
```

You can also configure Ollama after launch:

```text
/login ollama http://localhost:11434
```

## First Test Path

Type these commands in the TUI after pressing `/`.

### 1. Run Safe Infrastructure Checks

```text
/exec uptime
/exec df -h
/exec systemctl --no-pager --failed
```

Expected: command events and output appear in the Logs, Infrastructure, and event preview panels.

### 2. Create an Incident

```text
/investigate checkout_latency
```

Expected: an incident named `inc-checkout_latency` appears in the Incidents view.

### 3. Spawn a Test Agent

```text
/spawn-agent research
```

Expected: a new agent appears in the Agents view. With Ollama running, it can execute an AI task and then move to `Completed`.

### 4. Run a Multi-Agent Task

```text
/multi-agent Check local model health, disk pressure, and failed systemd services
```

Expected: a planner agent creates executor work. Completed agents show `[OK]`, and the model unloads after successful completion.

### 5. Generate a Task Report

```text
/tasks-report
```

Expected: Reports view shows recent task lifecycle events.

### 6. Add Evidence

```text
/graph link checkout-api depends-on postgres-primary
/research confidence
```

Expected: the Research view includes the graph edge and refreshed confidence profile.

### 7. Propose and Approve a Recovery

```text
/recover checkout-api
/role operator
/approve rec-123
```

Replace `rec-123` with the recovery id shown in the Workflows view. Expected: approval is recorded and a dry-run recovery command is requested.

### 8. Export a Report

```text
/generate-report checkout_latency
```

Expected: a JSON file appears in `reports/`.

## Common Commands

| Command | Example |
|---------|---------|
| Multi-agent task | `/multi-agent Investigate auth latency` |
| Spawn agent | `/spawn-agent planner` |
| Assign task | `/assign agent-123 Summarize current incident` |
| Task report | `/tasks-report` |
| Incident | `/investigate nginx_latency` |
| Logs | `/exec journalctl -n 40 --no-pager` |
| Recovery | `/recover nginx` |
| Approval role | `/role operator` |
| Report | `/generate-report nginx_latency` |
| Plugin | `/plugin add runbook-index integration` |
| Runtime | `/runtime set agent-remote remote ssh://node-01` |
| Replay | `/replay start`, then `/replay step` |

## API Smoke Test

While OctoBot is running:

```bash
curl http://127.0.0.1:7878/health
curl http://127.0.0.1:7878/api/state
curl http://127.0.0.1:7878/api/plugins
```

## Next Steps

- Read [User Guide](user-guide.md) for detailed examples for each feature and use case.
- Read [Deployment Reference](deployment.md) for optional backends, environment variables, plugins, workflows, and troubleshooting.
