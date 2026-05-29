# OctoBot Engineering OS Blueprint

OctoBot now models the next-generation autonomous engineering platform as a typed Rust capability catalog in `src/platform/mod.rs`. The catalog is exposed through `GET /api/platform/capabilities` and covers all 15 requested capabilities with these required sections:

- System architecture
- Rust module structure
- Database schema
- Agent responsibilities
- Workflow design
- TUI integration
- API design
- Implementation phases
- Required crates
- Example commands
- Security considerations
- Scalability strategy

## Core Architecture

OctoBot stays local-first and terminal-native:

- Runtime: Tokio task supervisor with replayable `OpsEvent` flow.
- Event bus: `mpsc` input, `watch` state broadcast, policy validation before reduction.
- Persistence: SQLx/PostgreSQL append-only events plus graph/workspace/security tables.
- Memory: typed knowledge graph plus optional semantic vector recall.
- UI: Ratatui dashboard, agents, incidents, infrastructure, workflows, reports, settings, and chat.
- Models: Ollama-first routing with provider adapters for OpenAI, Anthropic, Gemini, DeepSeek, Mistral, and Qwen-class workloads.
- Security: role-aware approvals, sandboxed runtime tools, secret redaction, plugin signatures, and full replayability.

## Capability Map

| Capability | Primary Modules | Primary Tables | Main Agents | Example Commands |
|---|---|---|---|---|
| AI Project Memory Graph | `src/platform/mod.rs`, `src/persistence/mod.rs`, `src/models.rs` | `memory_nodes`, `memory_edges`, `memory_embeddings` | Memory, Research, Reviewer | `memory search checkout`, `graph link deploy-1188 fixes inc-042` |
| Autonomous Coding Workspace | `src/runtime_service.rs`, `src/workflows/mod.rs` | `repositories`, `repo_symbols`, `code_tasks`, `patch_sets` | Architect, Developer, Tester, Reviewer | `code plan fix failing test`, `workspace write operator agent://workspace/note.md note` |
| Multi-Agent Software Company | `src/agents/mod.rs`, `src/workflows/mod.rs`, `src/ai/mod.rs` | `agent_processes`, `agent_messages`, `consensus_votes` | Architect, Developer, Reviewer, Security, Tester, DevOps, PM | `agent spawn planner`, `ipc send agent-001 topic payload` |
| Infrastructure Architect | `src/infra/mod.rs`, `src/security/mod.rs` | `infra_plans`, `generated_artifacts` | Infra, Security, DevOps, Reviewer | `infra generate k8s api service` |
| Predictive Incident Engine | `src/observability/mod.rs`, `src/remediation/mod.rs` | `metric_samples`, `risk_scores` | Infra, Memory, Recovery | `investigate nginx_latency`, `analyze-logs auth-service` |
| Self-Healing Infrastructure | `src/remediation/mod.rs`, `src/runtime/mod.rs` | `remediation_policies`, `recovery_runs` | Recovery, Security, DevOps | `recover edge-nginx`, `approve rec-0001` |
| AI Security Operations Center | `src/security/mod.rs`, `src/plugins/mod.rs` | `security_findings`, `security_reports` | Security, Reviewer, DevOps | `security scan workspace`, `soc report` |
| Research Swarm | `src/ai/mod.rs`, `src/agents/mod.rs` | `research_runs`, `research_sources` | Research, Reviewer, Memory | `research swarm rust async runtimes` |
| Smart Model Router | `src/ai/mod.rs` | `model_providers`, `model_routing_events` | Planner, Execution, Validation | `login ollama http://localhost:11434`, `model route coding` |
| Plugin Marketplace | `src/plugins/registry.rs`, `src/plugins/host.rs` | `plugin_registry`, `plugin_permissions` | Security, Reviewer, Plugin | `marketplace import local-tool`, `plugin enable local-research` |
| Live Infrastructure Map | `src/infra/mod.rs`, `src/ui.rs` | `infra_nodes`, `topology_edges` | Infra, DevOps, Recovery | `topology refresh`, `exec kubectl get pods` |
| Cost Optimization Engine | `src/infra/mod.rs`, `src/observability/mod.rs` | `cost_samples`, `cost_recommendations` | Infra, PM, DevOps | `cost analyze cluster`, `cost forecast monthly` |
| Incident Replay Studio | `src/persistence/mod.rs`, `src/ui.rs` | `replay_sessions`, `replay_annotations` | Reviewer, Recovery, PM | `replay start`, `replay step` |
| AI Pair Programmer | `src/ai/mod.rs`, `src/runtime_service.rs`, `src/ui.rs` | `pair_sessions`, `pair_suggestions` | Developer, Reviewer, Tester | `chat summarize the current system state`, `assign agent-001 refactor api` |
| AI-Native Engineering OS | `src/main.rs`, `src/runtime/mod.rs`, `src/api.rs`, `src/ui.rs` | `ops_events`, `system_services`, `resource_quotas` | All agents under planner and policy | `boot status`, `services`, `tasks-report` |

## Implementation Phases

1. Contract and schema: maintain the typed capability catalog, API endpoint, migrations, and tests.
2. Event projectors: project runtime events into memory, workspace, security, cost, replay, and topology tables.
3. Agent workflows: implement DAG templates for coding, incident prediction, remediation, research, security scans, and infrastructure generation.
4. TUI surfaces: add memory graph, workspace, SOC, cost, replay, model router, and marketplace panels.
5. Distributed runtime: support remote agent runtimes with quota, health, policy, and replay-compatible execution.
6. Production hardening: add signed plugin feeds, model quality telemetry, snapshot compaction, and multi-project tenancy.

## Security And Scaling Baseline

All write-capable features must be policy-gated, audit-logged, replayable, and namespace-scoped. Default operation is local and read-only; destructive operations require explicit role approval.

Scale by partitioning data per project, batching embedding/indexing jobs, sharding agent execution by workflow, using stateless API replicas, and keeping the event log append-only with periodic snapshots.
