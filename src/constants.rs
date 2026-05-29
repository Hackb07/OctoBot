pub(crate) const NAV_ITEMS: [(&str, char); 10] = [
    ("Dashboard", '1'),
    ("Agents", '2'),
    ("Incidents", '3'),
    ("Research", '4'),
    ("Logs", '5'),
    ("Infrastructure", '6'),
    ("Workflows", '7'),
    ("Reports", '8'),
    ("Settings", '9'),
    ("Chat", '0'),
];

pub(crate) const DEFAULT_LOG_LIMIT: usize = 120;
pub(crate) const DEFAULT_EVENT_LIMIT: usize = 120;
pub(crate) const DEFAULT_EXECUTION_LIMIT: usize = 40;
pub(crate) const DEFAULT_EXPLAINABILITY_LIMIT: usize = 80;
pub(crate) const DEFAULT_COORDINATION_LIMIT: usize = 80;
pub(crate) const DEFAULT_TIMELINE_LIMIT: usize = 160;
pub(crate) const DEFAULT_RECOVERY_LIMIT: usize = 40;
pub(crate) const DEFAULT_SYSCALL_LIMIT: usize = 160;

pub(crate) fn log_limit() -> usize {
    std::env::var("OCTOBOT_LOG_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_LOG_LIMIT)
}
pub(crate) fn event_limit() -> usize {
    std::env::var("OCTOBOT_EVENT_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_EVENT_LIMIT)
}
pub(crate) fn execution_limit() -> usize {
    std::env::var("OCTOBOT_EXECUTION_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_EXECUTION_LIMIT)
}
pub(crate) fn explainability_limit() -> usize {
    std::env::var("OCTOBOT_EXPLAINABILITY_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_EXPLAINABILITY_LIMIT)
}
pub(crate) fn coordination_limit() -> usize {
    std::env::var("OCTOBOT_COORDINATION_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_COORDINATION_LIMIT)
}
pub(crate) fn timeline_limit() -> usize {
    std::env::var("OCTOBOT_TIMELINE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TIMELINE_LIMIT)
}
pub(crate) fn recovery_limit() -> usize {
    std::env::var("OCTOBOT_RECOVERY_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_RECOVERY_LIMIT)
}
pub(crate) fn syscall_limit() -> usize {
    std::env::var("OCTOBOT_SYSCALL_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SYSCALL_LIMIT)
}

pub(crate) const COMMAND_SUGGESTIONS: [&str; 65] = [
    "multi-agent Assess Ollama readiness and report findings",
    "investigate local_model_health",
    "spawn-agent planner",
    "tasks-report",
    "analyze-logs auth-service",
    "generate-report incident_042",
    "recover edge-nginx",
    "approve rec-0001",
    "role operator",
    "role admin",
    "role readonly",
    "chat summarize the current system state",
    "replay start",
    "replay step",
    "exec uptime",
    "exec df -h",
    "exec ps aux",
    "exec docker ps",
    "exec kubectl get pods",
    "exec systemctl --no-pager --failed",
    "exec journalctl -n 40 --no-pager",
    "research confidence",
    "plugin enable local-research",
    "plugin disable workflow-rca",
    "plugin add runbook-index integration",
    "runtime set agent-remote remote ssh://research-node-01",
    "runtime smoke",
    "runtime docker-smoke",
    "runtime cancel-smoke",
    "graph link deploy-1188 correlates-with inc-042",
    "sandbox policy operator restart",
    "sandbox policy admin rollback",
    "assign <agent_id> <task>",
    "ps",
    "agent spawn planner",
    "pause <agent>",
    "resume <agent>",
    "kill <agent>",
    "syscalls",
    "policy show",
    "policy grant agent-001 cmd:readonly",
    "apps",
    "run runbook-index",
    "memory search checkout",
    "workspace write operator agent://workspace/note.md note",
    "marketplace import local-tool",
    "services",
    "supervisor",
    "boot status",
    "ipc send agent-001 topic payload",
    "agent spawn executor",
    "login ollama <url>",
    "login ollama http://localhost:11434",
    "code plan fix failing test",
    "code index .",
    "code review current patch",
    "infra generate terraform private api",
    "infra generate k8s api service",
    "cost analyze cluster",
    "cost forecast monthly",
    "security scan workspace",
    "soc report",
    "research swarm rust async runtimes",
    "model route coding",
    "topology refresh",
];
