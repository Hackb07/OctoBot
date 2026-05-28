# OctoBot: A Local-First Agentic OS for DevOps and Security Operations

Modern operations teams need fast incident response, reliable automation, explainable AI assistance, and security controls that do not depend on sending sensitive infrastructure context to cloud services. OctoBot is a Rust-based, terminal-native agentic OS built for that workflow.

OctoBot brings together an agent kernel, local AI agents, process management, capability-based syscalls, infrastructure diagnostics, workflow automation, replayable audit trails, agentic apps, and security hardening in one keyboard-first operating layer.

## What OctoBot Does

OctoBot is designed for SRE, platform, DevOps, and security teams that want a local-first agentic operating layer. It helps operators investigate incidents, run safe infrastructure checks, coordinate AI agents, manage workflows, review security posture, and generate reports from a single terminal UI.

At a high level, OctoBot provides:

- A terminal dashboard for health, agents, incidents, logs, infrastructure, workflows, reports, and settings.
- An Agentic OS kernel with process table, syscall audit, service model, boot state, and supervisor events.
- Local AI agent orchestration through Ollama.
- Agentic shell commands such as `/ps`, `/kill`, `/pause`, `/resume`, `/apps`, `/syscalls`, `/policy show`, and `/memory search`.
- Conversation AI tab/API for natural-language questions, project guidance, and operational summaries.
- Safe, allowlisted infrastructure command execution.
- Incident investigation workflows with replay and explainability.
- Plugin and runtime management.
- Agentic app packages, workspace artifacts, IPC messages, policy grants, and scoped memory entries.
- Security dashboards, audit trails, and runtime hardening.
- Optional persistence and semantic memory integrations.

## Key Features

### Agentic OS Kernel

OctoBot models agents as OS-style processes with PID-like IDs, lifecycle state, parent/child relationships, runtime endpoint, memory scope, tool usage, token usage, and event accounting.

The agentic shell exposes OS-like commands:

```text
/ps
/agent spawn planner
/pause agent-001
/resume agent-001
/kill agent-001
/syscalls
/policy show
/apps
/memory search checkout
```

Agents and apps interact through a capability-based syscall layer covering file, shell, memory, workflow, plugin, network, and event operations.

### Conversation AI

OctoBot adds a dedicated Chat tab on `0` for conversation while keeping the existing 1-9 operational views. Operators can ask natural-language questions through the shell, and OctoBot routes each question to a local agent role for the answer:

```text
/chat summarize the current system state
/chat what is unhealthy?
/chat explain blocked commands
/chat plan the next task
```

The conversation is recorded in state, appears in replayable events, and can be queried through the local API.

### Local AI Agent Runtime

OctoBot uses local Ollama models instead of cloud AI providers. It includes dedicated agent profiles for common operations tasks:

- Coding agent: `qwen2.5-coder:7b`
- Planning agent: `llama3.1:8b`
- Security reasoning agent: `deepseek-r1:8b`
- Utility agent: `phi4`

The runtime routes tasks to the right model, streams responses, tracks token usage, checks model health, and unloads completed task models to free memory.

### Multi-Agent Operations

OctoBot supports planner and executor agents. A planner can break a complex task into subtasks, spawn executor agents, and track completion across the task lifecycle.

Example:

```text
/multi-agent Check local model health, disk pressure, and failed systemd services
```

### Safe Infrastructure Commands

OctoBot does not run arbitrary shell commands. Infrastructure commands go through a security policy and allowlist.

Examples:

```text
/exec uptime
/exec df -h
/exec ps aux
/exec systemctl --no-pager --failed
/exec journalctl -n 40 --no-pager
```

Dangerous commands such as shell pipelines, destructive file operations, and unsafe remediations are blocked or routed through approval paths.

### Incident Investigation

Operators can create incidents directly from the TUI:

```text
/investigate checkout_latency
```

OctoBot records the incident, starts investigation state, updates the timeline, and links related evidence into the knowledge graph.

### Workflow Automation

OctoBot includes a DAG workflow runtime with:

- YAML workflow definitions.
- Command, agent task, approval, and condition nodes.
- Retry and backoff support.
- Conditional branching.
- Rollback support.
- Approval checkpoints.
- Workflow risk scoring.

### Security Dashboard

The Settings view includes a security dashboard for:

- Active threats.
- Suspicious activity.
- Blocked attacks.
- Runtime integrity.
- Permission violations.
- Vulnerability findings.
- Plugin security status.
- Sandbox and resource protection.
- Security replay and AI reasoning traces.

### Security Tooling

OctoBot includes local security tooling for:

- Dependency metadata review.
- Local listening port inspection.
- Configuration analysis.
- Log anomaly detection.
- Plugin behavior analysis.
- Workflow validation.
- Sandbox inspection.

### Production-Grade Hardening

The application includes defense-in-depth controls across the runtime:

- Centralized security policy enforcement.
- Hardened event bus validation.
- Event integrity hashing.
- Plugin runtime boundaries.
- Sensitive persistence protection.
- Redacted memory indexing.
- Async runtime backpressure checks.
- Rate limiting for commands and AI tasks.

### Plugin System

OctoBot supports native and script-backed plugins. Plugins have manifests, lifecycle hooks, status transitions, scoped permissions, and security validation.

Example:

```text
/plugin add runbook-index integration
/plugin enable runbook-index
/plugin disable runbook-index
```

### Replay, Reports, and Explainability

OctoBot records operational events and supports replay:

```text
/replay start
/replay step
```

Reports can be generated from the TUI:

```text
/tasks-report
/generate-report checkout_latency
```

The explainability layer records why actions happened, which tools were used, what evidence was considered, and how confident the system was.

## How To Use OctoBot

### 1. Run The App

```bash
cargo run
```

The terminal UI opens with the main dashboard. Use number keys to switch views:

- `1` Dashboard
- `2` Agents
- `3` Incidents
- `4` Research
- `5` Logs
- `6` Infrastructure
- `7` Workflows
- `8` Reports
- `9` Settings

Press `/` to enter command mode, `?` for help, and `q` to quit.

### 2. Run Basic Checks

```text
/exec uptime
/exec df -h
/exec systemctl --no-pager --failed
```

### 3. Create An Incident

```text
/investigate nginx_latency
```

### 4. Spawn An Agent

```text
/spawn-agent research
```

### 5. Run A Multi-Agent Task

```text
/multi-agent Summarize local machine health and identify urgent risks
```

### 6. Review Security

Run a blocked command:

```text
/exec rm -rf /tmp/test
```

Then press `9` to open Settings and inspect the security dashboard.

### 7. Test The API

While OctoBot is running:

```bash
curl http://127.0.0.1:7878/health
curl http://127.0.0.1:7878/api/state
curl http://127.0.0.1:7878/api/plugins
curl http://127.0.0.1:7878/api/conversation
curl http://127.0.0.1:7878/api/processes
curl http://127.0.0.1:7878/api/syscalls
curl http://127.0.0.1:7878/api/services
curl http://127.0.0.1:7878/api/boot
curl http://127.0.0.1:7878/api/replay/events
curl http://127.0.0.1:7878/api/replay/reconstruct
```

## Tech Stack

OctoBot is built with a local-first Rust, Python, and TypeScript stack:

- Language: Rust 2024 edition
- Terminal UI: Ratatui and Crossterm
- Async runtime: Tokio
- HTTP API: Axum
- Python orchestration: FastAPI and Pydantic
- Agent graph: LangGraph-compatible graph with deterministic fallback
- Frontend: React, TypeScript, Vite, and Tauri
- Production frontend serving: nginx static image
- HTTP client: Reqwest with Rustls
- Serialization: Serde, Serde JSON, Serde YAML
- Persistence: SQLx with PostgreSQL
- Semantic memory: Qdrant-compatible vector storage
- Local AI runtime: Ollama
- Observability and tracing: Tracing and Tracing Subscriber
- Plugin runtime: Native Rust plugins and external script plugins
- Workflow engine: YAML-defined DAG runtime
- LLM providers: Ollama, OpenAI, Anthropic, and Groq-compatible APIs
- Container deployment: Docker Compose profiles with healthchecks and TLS proxy config

## Why Local-First Matters

Operations data often contains sensitive infrastructure details, incident context, credentials, service names, logs, and internal topology. OctoBot is designed around local inference and local control paths so teams can keep sensitive operational context close to their own systems.

The AI runtime only accepts local Ollama endpoints, and the security layer blocks non-local AI provider configuration. This makes OctoBot suitable for teams that want AI-assisted operations without defaulting to cloud-hosted reasoning.

## Final Thoughts

OctoBot is more than a terminal dashboard. It is a local-first agentic OS with process management, syscalls, services, memory, apps, security, workflow automation, observability, explainability, and replay built into the core.

For teams that live in terminals and care about operational safety, OctoBot provides a production-oriented base for AI-assisted incident response, infrastructure control, and autonomous coding workflows.
