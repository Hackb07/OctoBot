# OctoBot Quickstart Guide

This guide gets OctoBot running locally and walks through a small test path. For detailed examples for every feature, see [User Guide](user-guide.md). For production-style setup, see [Deployment Reference](deployment.md).

## Prerequisites

- Rust stable toolchain.
- Linux terminal, ideally 120 columns or wider.
- Optional: Ollama for local AI agent tasks.
- Optional: Ollama, Docker, kubectl, PostgreSQL, Qdrant, Prometheus, Loki, OpenSearch, Node.js, or npm for integrations and frontend work.

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

## One-Command Linux Install

On a fresh Linux machine, install dependencies, clone OctoBot from GitHub, set up Rust/Python/frontend tooling, pull default Ollama models, and launch the app with:

```bash
curl -fsSL https://raw.githubusercontent.com/Hackb07/OctoBot/main/scripts/install-linux.sh | bash
```

Or clone first and run the installer from the repository:

```bash
git clone https://github.com/Hackb07/OctoBot "$HOME/openAi/OctoBot"
cd "$HOME/openAi/OctoBot"
scripts/install-linux.sh
```

Useful installer options:

```bash
scripts/install-linux.sh --no-run
scripts/install-linux.sh --skip-ollama
scripts/install-linux.sh --dir "$HOME/apps/OctoBot"
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
49 passed
```

Press `/` for commands, `1`-`9` for views, `Tab` to cycle views, `?` for help, and `q` to quit.

`cargo run` is the normal local entry point. It starts or reuses these services before opening the terminal UI:

- Ollama on `127.0.0.1:11434`
- Python orchestrator on `127.0.0.1:8787`
- Frontend dev server on `127.0.0.1:5173`
- Rust control API on `127.0.0.1:7878`
- Rust runtime service on `127.0.0.1:7879`

Service logs are written under `.octobot/logs/`. To disable service autostart, run with `OCTOBOT_NO_AUTOSTART=1`.

For a service-only local developer workflow, start or reuse the background services with:

```bash
scripts/start-dev.sh
scripts/healthcheck.sh
```

`scripts/start-dev.sh` reuses already-running services instead of failing with duplicate-port errors.

## Optional Ollama Setup

OctoBot uses Ollama for local Rust-side agent tasks. The Python orchestrator also supports Ollama, OpenAI, Anthropic, and Groq-compatible provider routing.

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

## Autonomous Orchestrator Smoke Test

Start all local services through the Rust entry point:

```bash
cargo run
```

Create and run a dry-run coding task:

```bash
curl -s http://127.0.0.1:8787/api/tasks \
  -H 'content-type: application/json' \
  -d '{"goal":"summarize repository","repository":{"path":"."},"dry_run":true}'
```

Use the returned `task_id`:

```bash
curl -s -X POST http://127.0.0.1:8787/api/tasks/<task_id>/run
curl -s http://127.0.0.1:8787/api/tasks/<task_id>/observability
curl -s http://127.0.0.1:8787/metrics
```

## Production Verification

Run the full local production check set:

```bash
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
PYTHONPATH=. .venv/bin/pytest
PYTHONPATH=. .venv/bin/ruff check backend tests
cd octobot-web && npm ci && npm run build && npm audit
cd octobot-web/src-tauri && cargo check
```

## Next Steps

- Read [User Guide](user-guide.md) for detailed examples for each feature and use case.
- Read [Deployment Reference](deployment.md) for optional backends, environment variables, plugins, workflows, and troubleshooting.
