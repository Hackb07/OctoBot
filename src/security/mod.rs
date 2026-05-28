use std::{
    collections::{HashMap, VecDeque},
    fs,
    hash::{Hash, Hasher},
    path::Path,
    time::{Duration, Instant},
};

use crate::models::{
    AgentRole, AgentRuntimeKind, ExplainabilityRecord, OpsEvent, OpsState, PluginDescriptor,
    PluginKind, RuntimeStatus, TimelineCategory, UserRole,
};
use crate::utils::now_ts;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CommandTier {
    ReadOnly,
    Remediation,
}

#[derive(Debug, Clone)]
pub(crate) struct CommandSecurityDecision {
    pub(crate) tier: CommandTier,
    pub(crate) audit: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SecurityFinding {
    pub(crate) id: String,
    pub(crate) severity: String,
    pub(crate) title: String,
    pub(crate) evidence: String,
    pub(crate) recommendation: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ThreatSignal {
    pub(crate) category: String,
    pub(crate) score: u8,
    pub(crate) evidence: String,
}

#[derive(Debug, Clone)]
pub(crate) struct SecureLogRecord {
    pub(crate) sequence: u64,
    pub(crate) message: String,
    pub(crate) previous_hash: u64,
    pub(crate) hash: u64,
    pub(crate) timestamp: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AuthSession {
    pub(crate) token_hash: u64,
    pub(crate) role: UserRole,
    pub(crate) expires_at: Instant,
}

#[derive(Debug, Clone)]
struct RateBucket {
    seen: VecDeque<Instant>,
}

#[derive(Debug, Clone)]
pub(crate) struct RateLimiter {
    limit: usize,
    window: Duration,
    buckets: HashMap<String, RateBucket>,
}

impl RateLimiter {
    pub(crate) fn new(limit: usize, window: Duration) -> Self {
        Self {
            limit,
            window,
            buckets: HashMap::new(),
        }
    }

    pub(crate) fn allow(&mut self, key: &str) -> bool {
        let now = Instant::now();
        let bucket = self
            .buckets
            .entry(key.into())
            .or_insert_with(|| RateBucket {
                seen: VecDeque::new(),
            });
        while bucket
            .seen
            .front()
            .map(|older| now.duration_since(*older) > self.window)
            .unwrap_or(false)
        {
            bucket.seen.pop_front();
        }
        if bucket.seen.len() >= self.limit {
            return false;
        }
        bucket.seen.push_back(now);
        true
    }
}

pub(crate) struct SecurityPolicy;

impl SecurityPolicy {
    pub(crate) fn validate_command(command: &str) -> Result<CommandSecurityDecision, String> {
        let trimmed = command.trim();
        if trimmed.is_empty() || trimmed.len() > 512 {
            return Err("blocked by sandbox: command is empty or too long".into());
        }
        if trimmed.chars().any(|c| c.is_control()) {
            return Err("blocked by sandbox: control characters are not allowed".into());
        }
        let dangerous = [
            ";", "&&", "||", "|", "`", "$(", ">", "<", "\n", "\r", "*", "?", "..", "${",
        ];
        if dangerous.iter().any(|pattern| trimmed.contains(pattern)) {
            return Err(format!(
                "blocked by sandbox: dangerous shell pattern in `{trimmed}`"
            ));
        }
        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        if parts.iter().any(|part| part.len() > 160) {
            return Err("blocked by sandbox: argument length exceeds policy".into());
        }
        match parts.as_slice() {
            ["docker", "ps"]
            | ["kubectl", "get", "pods"]
            | ["systemctl", "--no-pager", "--failed"]
            | ["ps", "aux"]
            | ["df", "-h"]
            | ["uptime"] => Ok(CommandSecurityDecision {
                tier: CommandTier::ReadOnly,
                audit: "read-only allowlist command".into(),
            }),
            ["journalctl", "-n", count, "--no-pager"]
            | ["journalctl", "-f", "-n", count, "--no-pager"]
                if bounded_count(count, 1, 500) =>
            {
                Ok(CommandSecurityDecision {
                    tier: CommandTier::ReadOnly,
                    audit: "journal access capped by line-count policy".into(),
                })
            }
            ["ssh", host, "uptime"] if safe_target(host) => Ok(CommandSecurityDecision {
                tier: CommandTier::ReadOnly,
                audit: "ssh command restricted to uptime probe".into(),
            }),
            ["systemctl", "restart", service] if safe_target(service) => {
                Ok(CommandSecurityDecision {
                    tier: CommandTier::Remediation,
                    audit: "restart requires remediation approval path".into(),
                })
            }
            ["kubectl", "rollout", "undo", target] if safe_target(target) => {
                Ok(CommandSecurityDecision {
                    tier: CommandTier::Remediation,
                    audit: "rollback requires remediation approval path".into(),
                })
            }
            _ => Err(format!("blocked by sandbox allowlist: `{trimmed}`")),
        }
    }

    pub(crate) fn role_can_use_tool(role: &AgentRole, tool: &str) -> bool {
        match tool {
            "complete_task" | "create_subtask" | "finalize_plan" | "report_readiness" => true,
            "exec_command" => matches!(
                role,
                AgentRole::Executor
                    | AgentRole::Logs
                    | AgentRole::Research
                    | AgentRole::Triage
                    | AgentRole::Planner
            ),
            _ => false,
        }
    }

    pub(crate) fn can_administer(role: &UserRole) -> bool {
        matches!(role, UserRole::Admin)
    }

    pub(crate) fn sanitize_prompt(input: &str) -> String {
        let mut out = redact_sensitive(input);
        for marker in [
            "<!--",
            "-->",
            "<script",
            "</script",
            "```system",
            "BEGIN_SYSTEM",
            "END_SYSTEM",
        ] {
            out = out.replace(marker, "[filtered]");
        }
        out.chars().filter(|c| !c.is_control()).collect()
    }

    pub(crate) fn detect_prompt_attack(input: &str) -> Option<String> {
        let lower = input.to_ascii_lowercase();
        let patterns = [
            "ignore previous instructions",
            "developer message",
            "system prompt",
            "jailbreak",
            "reveal your instructions",
            "call exec_command",
            "tool hijack",
            "write this to memory",
            "recursive agent",
        ];
        patterns
            .iter()
            .find(|pattern| lower.contains(**pattern))
            .map(|pattern| format!("prompt-injection indicator `{pattern}`"))
    }

    pub(crate) fn validate_tool_call(
        role: &AgentRole,
        tool: &str,
        arguments: &serde_json::Value,
    ) -> Result<(), String> {
        if !Self::role_can_use_tool(role, tool) {
            return Err(format!("tool `{tool}` is not permitted for role {role:?}"));
        }
        if let Some(text) = arguments
            .get("command")
            .and_then(serde_json::Value::as_str)
            .or_else(|| arguments.get("summary").and_then(serde_json::Value::as_str))
            && let Some(reason) = Self::detect_prompt_attack(text)
        {
            return Err(format!("blocked malicious tool arguments: {reason}"));
        }
        Ok(())
    }

    pub(crate) fn validate_event(event: &OpsEvent, state: &OpsState) -> Result<(), String> {
        EventBusSecurity::validate_event(event, state)
    }

    pub(crate) fn validate_syscall(role: &AgentRole, call: &str) -> Result<&'static str, String> {
        let capability = match call {
            "fs.read" => "fs:read",
            "fs.write" if matches!(role, AgentRole::Executor | AgentRole::Report) => "fs:write",
            "shell.exec"
                if matches!(
                    role,
                    AgentRole::Executor
                        | AgentRole::Logs
                        | AgentRole::Research
                        | AgentRole::Triage
                        | AgentRole::Planner
                ) =>
            {
                "cmd:readonly"
            }
            "memory.search" => "memory:read",
            "memory.write" => "memory:write",
            "workflow.start" if matches!(role, AgentRole::Planner | AgentRole::Workflow) => {
                "workflow:execute"
            }
            "plugin.call" => "plugin:call",
            "network.request" => {
                return Err("syscall denied: external network requires explicit app grant".into());
            }
            "event.emit" => "event:emit",
            _ => {
                return Err(format!(
                    "syscall `{call}` is not permitted for role {role:?}"
                ));
            }
        };
        Ok(capability)
    }
}

pub(crate) struct PluginSecurity;

impl PluginSecurity {
    pub(crate) fn validate_descriptor(descriptor: &PluginDescriptor) -> Result<(), String> {
        if descriptor.name.is_empty()
            || descriptor.name.len() > 80
            || !descriptor
                .name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err("plugin manifest rejected: invalid plugin name".into());
        }
        if descriptor.owner.trim().is_empty() || descriptor.version.trim().is_empty() {
            return Err("plugin manifest rejected: owner and version are required".into());
        }
        if descriptor
            .description
            .to_ascii_lowercase()
            .contains("curl | sh")
        {
            return Err("plugin manifest rejected: suspicious installer pattern".into());
        }
        Ok(())
    }

    pub(crate) fn permissions_for_kind(kind: &PluginKind) -> &'static [&'static str] {
        match kind {
            PluginKind::Tool => &["execute:scoped", "fs:deny", "net:deny"],
            PluginKind::Workflow => &["workflow:define", "execute:deny", "net:deny"],
            PluginKind::Integration => &["net:configured", "fs:deny"],
            PluginKind::Agent => &["agent:spawn", "memory:scoped", "execute:deny"],
        }
    }

    pub(crate) fn validate_script_path(path: &std::path::Path) -> Result<(), String> {
        let text = path.to_string_lossy();
        if text.contains("..") || !path.extension().map(|e| e == "sh").unwrap_or(false) {
            return Err("plugin script rejected: invalid script path".into());
        }
        Ok(())
    }

    pub(crate) fn manifest_integrity(descriptor: &PluginDescriptor) -> u64 {
        stable_hash(&(
            descriptor.name.as_str(),
            format!("{:?}", descriptor.kind),
            descriptor.description.as_str(),
            descriptor.version.as_str(),
            descriptor.owner.as_str(),
        ))
    }

    pub(crate) fn verify_manifest_integrity(
        descriptor: &PluginDescriptor,
        expected: u64,
    ) -> Result<(), String> {
        let actual = Self::manifest_integrity(descriptor);
        if actual == expected {
            Ok(())
        } else {
            Err("plugin manifest rejected: integrity signature mismatch".into())
        }
    }

    pub(crate) fn enforce_runtime_boundaries(
        descriptor: &PluginDescriptor,
        input: &str,
    ) -> Result<(), String> {
        Self::validate_descriptor(descriptor)?;
        if input.len() > 2_048 {
            return Err("plugin input rejected: quota exceeded".into());
        }
        if SecurityPolicy::detect_prompt_attack(input).is_some() {
            return Err("plugin input rejected: prompt manipulation".into());
        }
        let scopes = Self::permissions_for_kind(&descriptor.kind);
        let lower = input.to_ascii_lowercase();
        if scopes.contains(&"net:deny")
            && ["http://", "https://", "curl ", "wget ", "nc "]
                .iter()
                .any(|needle| lower.contains(needle))
        {
            return Err(
                "plugin input rejected: network access denied by scoped permissions".into(),
            );
        }
        if scopes.contains(&"fs:deny")
            && ["../", "/etc/", "/var/", "read_to_string", "write "]
                .iter()
                .any(|needle| lower.contains(needle))
        {
            return Err(
                "plugin input rejected: filesystem access denied by scoped permissions".into(),
            );
        }
        Ok(())
    }
}

pub(crate) struct AuthManager {
    sessions: HashMap<String, AuthSession>,
}

impl AuthManager {
    pub(crate) fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    pub(crate) fn issue_session(
        &mut self,
        subject: &str,
        token: &str,
        role: UserRole,
        ttl: Duration,
    ) -> Result<(), String> {
        if subject.trim().is_empty() || token.len() < 16 {
            return Err("auth rejected: subject is empty or token is too short".into());
        }
        self.sessions.insert(
            subject.into(),
            AuthSession {
                token_hash: stable_hash(&token),
                role,
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(())
    }

    pub(crate) fn authorize(&self, subject: &str, token: &str, required: &UserRole) -> bool {
        let Some(session) = self.sessions.get(subject) else {
            return false;
        };
        if Instant::now() >= session.expires_at || session.token_hash != stable_hash(&token) {
            return false;
        }
        match required {
            UserRole::ReadOnly => true,
            UserRole::SecurityReviewer => matches!(
                session.role,
                UserRole::SecurityReviewer | UserRole::Operator | UserRole::Admin
            ),
            UserRole::Operator => matches!(session.role, UserRole::Operator | UserRole::Admin),
            UserRole::Admin => matches!(session.role, UserRole::Admin),
        }
    }
}

pub(crate) struct SecurityAuditor;

impl SecurityAuditor {
    pub(crate) fn audit_state(state: &OpsState) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        for execution in &state.executions {
            if let Err(error) = SecurityPolicy::validate_command(&execution.command) {
                findings.push(SecurityFinding {
                    id: format!("cmd-{}", execution.id),
                    severity: "high".into(),
                    title: "Unsafe command recorded".into(),
                    evidence: error,
                    recommendation: "route commands through the allowlisted sandbox".into(),
                });
            }
        }
        for plugin in &state.plugins {
            if let Err(error) = PluginSecurity::validate_descriptor(plugin) {
                findings.push(SecurityFinding {
                    id: format!("plugin-{}", plugin.name),
                    severity: "medium".into(),
                    title: "Unsafe plugin manifest".into(),
                    evidence: error,
                    recommendation: "require signed, scoped plugin manifests".into(),
                });
            }
        }
        if state.current_role == UserRole::Admin && state.sandbox_policy.approved_roles.is_empty() {
            findings.push(SecurityFinding {
                id: "policy-admin-empty".into(),
                severity: "low".into(),
                title: "Admin role active without persisted sandbox policy".into(),
                evidence: "approval policy has no approved roles".into(),
                recommendation: "persist role-aware sandbox approval policy".into(),
            });
        }
        findings
    }

    pub(crate) fn scan_source(name: &str, source: &str) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        for (idx, line) in source.lines().enumerate() {
            let lower = line.to_ascii_lowercase();
            let finding = if lower.contains("command::new(\"sh\")") || lower.contains("sh -c") {
                Some(("high", "Unsafe shell execution"))
            } else if lower.contains("unwrap()") && lower.contains("mutex") {
                Some((
                    "medium",
                    "Potential panic while holding synchronization primitive",
                ))
            } else if lower.contains("password") || lower.contains("api_key") {
                Some(("medium", "Potential secret handling"))
            } else if lower.contains("read_to_string") && lower.contains("..") {
                Some(("medium", "Potential path traversal"))
            } else {
                None
            };
            if let Some((severity, title)) = finding {
                findings.push(SecurityFinding {
                    id: format!("{name}:{}", idx + 1),
                    severity: severity.into(),
                    title: title.into(),
                    evidence: line.trim().into(),
                    recommendation: "review and enforce OctoBot security policy".into(),
                });
            }
        }
        findings
    }

    pub(crate) fn ai_security_prompt(findings: &[SecurityFinding]) -> String {
        let evidence = findings
            .iter()
            .map(|f| format!("{} {}: {}", f.severity, f.title, f.evidence))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Use local deepseek-r1:8b only. Analyze these OctoBot vulnerability findings, score exploitability 0-100, identify attack paths, root cause, and concrete mitigations:\n{evidence}"
        )
    }

    pub(crate) fn explainability_record(findings: &[SecurityFinding]) -> ExplainabilityRecord {
        ExplainabilityRecord {
            id: format!("sec-audit-{}", now_ts()),
            action: "Automatic security audit".into(),
            why: format!(
                "{} findings produced by built-in self-audit",
                findings.len()
            ),
            evidence: findings
                .iter()
                .take(8)
                .map(|f| format!("{}: {} ({})", f.severity, f.title, f.evidence))
                .collect(),
            confidence: if findings.is_empty() { 92 } else { 78 },
            tools_used: vec!["security-auditor".into(), "deepseek-r1:8b-prompt".into()],
            timestamp: now_ts(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SecurityToolReport {
    pub(crate) dependency_findings: Vec<SecurityFinding>,
    pub(crate) port_findings: Vec<SecurityFinding>,
    pub(crate) configuration_findings: Vec<SecurityFinding>,
    pub(crate) log_findings: Vec<SecurityFinding>,
    pub(crate) plugin_findings: Vec<SecurityFinding>,
    pub(crate) workflow_findings: Vec<SecurityFinding>,
    pub(crate) sandbox_findings: Vec<SecurityFinding>,
}

impl SecurityToolReport {
    pub(crate) fn all_findings(&self) -> Vec<SecurityFinding> {
        [
            self.dependency_findings.as_slice(),
            self.port_findings.as_slice(),
            self.configuration_findings.as_slice(),
            self.log_findings.as_slice(),
            self.plugin_findings.as_slice(),
            self.workflow_findings.as_slice(),
            self.sandbox_findings.as_slice(),
        ]
        .concat()
    }
}

pub(crate) struct SecurityTooling;

impl SecurityTooling {
    pub(crate) fn offline_audit(state: &OpsState) -> SecurityToolReport {
        SecurityToolReport {
            dependency_findings: Self::scan_dependency_metadata(
                "Cargo.lock",
                include_str!("../../Cargo.lock"),
            ),
            port_findings: Self::scan_proc_net_tcp("/proc/net/tcp")
                .into_iter()
                .chain(Self::scan_proc_net_tcp("/proc/net/tcp6"))
                .collect(),
            configuration_findings: Self::analyze_configuration(state),
            log_findings: Self::detect_log_anomalies(&state.logs),
            plugin_findings: Self::analyze_plugins(&state.plugins),
            workflow_findings: Self::validate_workflow_summaries(state),
            sandbox_findings: Self::inspect_sandbox(state),
        }
    }

    pub(crate) fn scan_dependency_metadata(name: &str, contents: &str) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        let mut current_package = String::new();
        for line in contents.lines() {
            let trimmed = line.trim();
            if let Some(value) = trimmed.strip_prefix("name = ") {
                current_package = value.trim_matches('"').to_string();
            } else if let Some(value) = trimmed.strip_prefix("version = ") {
                let current_version = value.trim_matches('"').to_string();
                if is_risky_dependency(&current_package, &current_version) {
                    findings.push(SecurityFinding {
                        id: format!("dep-{current_package}-{current_version}"),
                        severity: "medium".into(),
                        title: "Dependency requires offline vulnerability review".into(),
                        evidence: format!("{current_package} {current_version} in {name}"),
                        recommendation:
                            "review local advisory data, changelog, and patched package versions"
                                .into(),
                    });
                }
            }
        }
        if contents.contains("default-features = true") && name.ends_with("Cargo.toml") {
            findings.push(SecurityFinding {
                id: "dep-default-features".into(),
                severity: "low".into(),
                title: "Dependency enables default features".into(),
                evidence: "default features may expand attack surface".into(),
                recommendation: "disable unused dependency features where possible".into(),
            });
        }
        findings
    }

    pub(crate) fn scan_proc_net_tcp(path: impl AsRef<Path>) -> Vec<SecurityFinding> {
        let Ok(contents) = fs::read_to_string(path.as_ref()) else {
            return Vec::new();
        };
        Self::scan_proc_net_tcp_contents(&path.as_ref().display().to_string(), &contents)
    }

    pub(crate) fn scan_proc_net_tcp_contents(name: &str, contents: &str) -> Vec<SecurityFinding> {
        contents
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts = line.split_whitespace().collect::<Vec<_>>();
                let local = parts.get(1)?;
                let state = parts.get(3)?;
                if *state != "0A" {
                    return None;
                }
                let (_, port_hex) = local.rsplit_once(':')?;
                let port = u16::from_str_radix(port_hex, 16).ok()?;
                risky_listening_port(port).map(|(severity, reason)| SecurityFinding {
                    id: format!("port-{port}"),
                    severity: severity.into(),
                    title: "Risky local listening port".into(),
                    evidence: format!("{name} reports TCP/{port} listening ({reason})"),
                    recommendation: "bind admin services to localhost and require authentication"
                        .into(),
                })
            })
            .collect()
    }

    pub(crate) fn analyze_configuration(state: &OpsState) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        for (key, value) in [
            (
                "OCTOBOT_DATABASE_URL",
                std::env::var("OCTOBOT_DATABASE_URL").ok(),
            ),
            (
                "OCTOBOT_QDRANT_URL",
                std::env::var("OCTOBOT_QDRANT_URL").ok(),
            ),
            (
                "OCTOBOT_EMBEDDING_URL",
                std::env::var("OCTOBOT_EMBEDDING_URL").ok(),
            ),
            (
                "OCTOBOT_KUBERNETES_URL",
                std::env::var("OCTOBOT_KUBERNETES_URL").ok(),
            ),
        ] {
            if let Some(value) = value
                && is_remote_url(&value)
            {
                findings.push(SecurityFinding {
                    id: format!("config-{key}"),
                    severity: "medium".into(),
                    title: "External service endpoint configured".into(),
                    evidence: format!("{key}={}", redact_sensitive(&value)),
                    recommendation: "use localhost endpoints for offline deployments".into(),
                });
            }
        }
        if state.current_role == UserRole::Admin {
            findings.push(SecurityFinding {
                id: "config-active-admin-role".into(),
                severity: "low".into(),
                title: "Admin role is currently active".into(),
                evidence: "current runtime role is Admin".into(),
                recommendation: "drop to operator or read-only outside approval windows".into(),
            });
        }
        findings
    }

    pub(crate) fn detect_log_anomalies(logs: &[String]) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        let failed = logs
            .iter()
            .filter(|line| {
                let lower = line.to_ascii_lowercase();
                lower.contains("failed")
                    || lower.contains("denied")
                    || lower.contains("unauthorized")
                    || lower.contains("blocked")
            })
            .count();
        if failed >= 3 {
            findings.push(SecurityFinding {
                id: "log-repeated-failures".into(),
                severity: "medium".into(),
                title: "Repeated suspicious log events".into(),
                evidence: format!("{failed} log lines contain failure, denial, or block markers"),
                recommendation: "review recent command, auth, and plugin activity".into(),
            });
        }
        for (idx, line) in logs.iter().enumerate() {
            if SecurityPolicy::detect_prompt_attack(line).is_some() {
                findings.push(SecurityFinding {
                    id: format!("log-prompt-attack-{}", idx + 1),
                    severity: "high".into(),
                    title: "Prompt manipulation in logs".into(),
                    evidence: line.clone(),
                    recommendation: "quarantine the source and preserve audit records".into(),
                });
            }
        }
        findings
    }

    pub(crate) fn analyze_plugins(plugins: &[PluginDescriptor]) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        for plugin in plugins {
            if let Err(error) = PluginSecurity::validate_descriptor(plugin) {
                findings.push(SecurityFinding {
                    id: format!("plugin-{}", plugin.name),
                    severity: "medium".into(),
                    title: "Plugin manifest failed security validation".into(),
                    evidence: error,
                    recommendation: "fix manifest identity, owner, version, and installer text"
                        .into(),
                });
            }
            if matches!(plugin.kind, PluginKind::Integration)
                && plugin.description.to_ascii_lowercase().contains("token")
            {
                findings.push(SecurityFinding {
                    id: format!("plugin-secret-{}", plugin.name),
                    severity: "medium".into(),
                    title: "Plugin description may expose credential handling".into(),
                    evidence: plugin.description.clone(),
                    recommendation: "move credentials to scoped secret storage".into(),
                });
            }
        }
        findings
    }

    pub(crate) fn validate_workflow_yaml(name: &str, yaml: &str) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        let value = match serde_yaml::from_str::<serde_yaml::Value>(yaml) {
            Ok(value) => value,
            Err(error) => {
                return vec![SecurityFinding {
                    id: format!("workflow-{name}-parse"),
                    severity: "high".into(),
                    title: "Workflow YAML is invalid".into(),
                    evidence: error.to_string(),
                    recommendation: "fix YAML syntax before loading the workflow".into(),
                }];
            }
        };
        let Some(nodes) = value.get("nodes").and_then(|nodes| nodes.as_sequence()) else {
            findings.push(SecurityFinding {
                id: format!("workflow-{name}-nodes"),
                severity: "high".into(),
                title: "Workflow has no node list".into(),
                evidence: "nodes field is missing or not a sequence".into(),
                recommendation: "define an explicit DAG node list".into(),
            });
            return findings;
        };
        let mut ids = Vec::new();
        for node in nodes {
            if let Some(id) = node.get("id").and_then(|id| id.as_str()) {
                ids.push(id.to_string());
            }
            if let Some(command) = node.get("command").and_then(|cmd| cmd.as_str())
                && let Err(error) = SecurityPolicy::validate_command(command)
            {
                findings.push(SecurityFinding {
                    id: format!("workflow-{name}-command"),
                    severity: "high".into(),
                    title: "Workflow command violates sandbox policy".into(),
                    evidence: error,
                    recommendation:
                        "replace with an allowlisted read-only command or approval gate".into(),
                });
            }
            let kind = node
                .get("kind")
                .and_then(|kind| kind.as_str())
                .unwrap_or("");
            if kind == "command"
                && !node
                    .get("approval_required")
                    .and_then(|approval| approval.as_bool())
                    .unwrap_or(false)
                && node.get("rollback").is_none()
            {
                findings.push(SecurityFinding {
                    id: format!("workflow-{name}-rollback"),
                    severity: "low".into(),
                    title: "Command workflow node has no rollback gap coverage".into(),
                    evidence: "command node lacks rollback and explicit approval".into(),
                    recommendation: "add approval_required or rollback for risky command nodes"
                        .into(),
                });
            }
        }
        ids.sort();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            findings.push(SecurityFinding {
                id: format!("workflow-{name}-duplicate-node"),
                severity: "high".into(),
                title: "Workflow contains duplicate node IDs".into(),
                evidence: "duplicate DAG node identity detected".into(),
                recommendation: "make every node id unique".into(),
            });
        }
        findings
    }

    fn validate_workflow_summaries(state: &OpsState) -> Vec<SecurityFinding> {
        state
            .workflows
            .iter()
            .filter(|workflow| workflow.progress < 100 && workflow.stage.contains("Approval"))
            .map(|workflow| SecurityFinding {
                id: format!("workflow-runtime-{}", workflow.id),
                severity: "low".into(),
                title: "Workflow waiting on approval".into(),
                evidence: format!("{} is at {}", workflow.name, workflow.stage),
                recommendation: "ensure approval is tied to an authorized operator role".into(),
            })
            .collect()
    }

    pub(crate) fn inspect_sandbox(state: &OpsState) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        if !state.sandbox_policy.persisted {
            findings.push(SecurityFinding {
                id: "sandbox-policy-not-persisted".into(),
                severity: "low".into(),
                title: "Sandbox policy is not persisted".into(),
                evidence: format!("mode={}", state.sandbox_policy.mode),
                recommendation: "persist sandbox approvals and review requirements".into(),
            });
        }
        for runtime in &state.runtimes {
            if matches!(
                runtime.kind,
                AgentRuntimeKind::RemoteServer | AgentRuntimeKind::Cluster
            ) && matches!(runtime.status, RuntimeStatus::Active)
            {
                findings.push(SecurityFinding {
                    id: format!("sandbox-runtime-{}", runtime.agent),
                    severity: "medium".into(),
                    title: "Active non-local runtime boundary".into(),
                    evidence: format!("{} uses {}", runtime.agent, runtime.endpoint),
                    recommendation: "verify network, filesystem, and quota boundaries".into(),
                });
            }
        }
        findings
    }
}

pub(crate) struct EventBusSecurity;

impl EventBusSecurity {
    pub(crate) fn validate_event(event: &OpsEvent, state: &OpsState) -> Result<(), String> {
        if state.events.len() >= crate::constants::event_limit().saturating_mul(2) {
            return Err("event bus rejected event: backlog exceeds hard limit".into());
        }
        match event {
            OpsEvent::CommandRequested { command, .. } => {
                SecurityPolicy::validate_command(command).map(|_| ())
            }
            OpsEvent::ToolCallRequested {
                tool, arguments, ..
            } => {
                if arguments.to_string().len() > 8_192 {
                    return Err("event bus rejected tool call: arguments too large".into());
                }
                if tool.trim().is_empty() {
                    return Err("event bus rejected tool call: empty tool name".into());
                }
                Ok(())
            }
            OpsEvent::AiProviderLogin { kind, endpoint, .. } => {
                if kind != "ollama" {
                    return Err(
                        "event bus rejected AI provider: only local Ollama is allowed".into(),
                    );
                }
                if local_loopback_http(endpoint) {
                    Ok(())
                } else {
                    Err("event bus rejected AI provider: endpoint must be localhost".into())
                }
            }
            OpsEvent::PluginRegistered { plugin } => PluginSecurity::validate_descriptor(plugin),
            OpsEvent::TimelineRecorded { event } => {
                if matches!(event.category, TimelineCategory::Recovery)
                    && event.summary.to_ascii_lowercase().contains("approved")
                    && state.current_role == UserRole::ReadOnly
                {
                    Err("event bus rejected recovery transition for read-only role".into())
                } else {
                    Ok(())
                }
            }
            OpsEvent::RecoveryApproved { role, .. } => {
                if role.can_approve_recovery() {
                    Ok(())
                } else {
                    Err("event bus rejected recovery approval from unauthorized role".into())
                }
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn integrity_hash(event: &OpsEvent, previous_hash: u64) -> u64 {
        stable_hash(&(previous_hash, format!("{event:?}")))
    }
}

pub(crate) struct PersistenceProtector;

impl PersistenceProtector {
    pub(crate) fn protect_json(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                for (key, value) in map.iter_mut() {
                    let lower = key.to_ascii_lowercase();
                    if lower.contains("password")
                        || lower.contains("token")
                        || lower.contains("api_key")
                        || lower.contains("authorization")
                        || lower.contains("secret")
                    {
                        let fingerprint = stable_hash(&value.to_string());
                        *value = serde_json::Value::String(format!("protected:{fingerprint:x}"));
                    } else {
                        Self::protect_json(value);
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    Self::protect_json(value);
                }
            }
            serde_json::Value::String(text) => {
                *text = redact_sensitive(text);
            }
            _ => {}
        }
    }
}

pub(crate) struct WorkflowSecurity;

impl WorkflowSecurity {
    pub(crate) fn risk_score(command: Option<&str>, approval_required: bool, rollback: bool) -> u8 {
        let mut score = 10;
        if let Some(command) = command {
            match SecurityPolicy::validate_command(command) {
                Ok(decision) if decision.tier == CommandTier::Remediation => score += 45,
                Ok(_) => score += 10,
                Err(_) => score += 80,
            }
        }
        if !approval_required {
            score += 15;
        }
        if !rollback {
            score += 15;
        }
        score.min(100)
    }
}

pub(crate) struct AsyncRuntimeGuard;

impl AsyncRuntimeGuard {
    pub(crate) fn backpressure_findings(state: &OpsState) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        let event_limit = crate::constants::event_limit();
        if state.events.len() >= event_limit.saturating_mul(8) / 10 {
            findings.push(SecurityFinding {
                id: "runtime-event-backpressure".into(),
                severity: "medium".into(),
                title: "Event bus approaching capacity".into(),
                evidence: format!(
                    "{} events buffered; limit is {event_limit}",
                    state.events.len()
                ),
                recommendation: "increase event processing throughput or lower producer rate"
                    .into(),
            });
        }
        let active = state
            .agents
            .iter()
            .filter(|agent| {
                !matches!(
                    agent.status,
                    crate::models::AgentStatus::Completed | crate::models::AgentStatus::Failed
                )
            })
            .count();
        if active > 24 {
            findings.push(SecurityFinding {
                id: "runtime-agent-supervision".into(),
                severity: "medium".into(),
                title: "Too many active async agents".into(),
                evidence: format!("{active} agents are active"),
                recommendation: "throttle new work and wait for supervised tasks to finish".into(),
            });
        }
        findings
    }
}

pub(crate) struct ThreatDetector;

impl ThreatDetector {
    pub(crate) fn analyze(state: &OpsState) -> Vec<ThreatSignal> {
        let mut signals = Vec::new();
        let failed_commands = state
            .executions
            .iter()
            .filter(|e| e.status == "failed")
            .count();
        if failed_commands >= 3 {
            signals.push(ThreatSignal {
                category: "repeated-failed-commands".into(),
                score: (failed_commands as u8).saturating_mul(15).min(95),
                evidence: format!("{failed_commands} failed command executions"),
            });
        }
        let active_agents = state
            .agents
            .iter()
            .filter(|a| !matches!(a.status, crate::models::AgentStatus::Completed))
            .count();
        if active_agents > 12 {
            signals.push(ThreatSignal {
                category: "resource-exhaustion".into(),
                score: 80,
                evidence: format!("{active_agents} non-completed agents active"),
            });
        }
        for link in &state.coordination_links {
            if SecurityPolicy::detect_prompt_attack(&link.message).is_some() {
                signals.push(ThreatSignal {
                    category: "prompt-manipulation".into(),
                    score: 88,
                    evidence: link.message.clone(),
                });
            }
        }
        signals
    }

    pub(crate) fn event_for(signal: &ThreatSignal) -> OpsEvent {
        OpsEvent::ExplainabilityRecorded {
            record: ExplainabilityRecord {
                id: format!("threat-{}-{}", signal.category, now_ts()),
                action: format!("Threat detected: {}", signal.category),
                why: "runtime threat detector matched abnormal behavior".into(),
                evidence: vec![signal.evidence.clone()],
                confidence: signal.score,
                tools_used: vec!["threat-detector".into()],
                timestamp: now_ts(),
            },
        }
    }
}

pub(crate) struct ReliabilityGuard;

impl ReliabilityGuard {
    pub(crate) fn cleanup_state(state: &mut OpsState) {
        let max_agents = env_usize("OCTOBOT_MAX_AGENTS", 64);
        if state.agents.len() > max_agents {
            state
                .agents
                .sort_by_key(|a| matches!(a.status, crate::models::AgentStatus::Completed));
            state.agents.truncate(max_agents);
        }
        let max_workflows = env_usize("OCTOBOT_MAX_WORKFLOWS", 80);
        if state.workflows.len() > max_workflows {
            let drop_count = state.workflows.len() - max_workflows;
            state.workflows.drain(0..drop_count);
        }
        state.reasoning_stream.retain(|line| line.len() <= 4_096);
        state.notifications.retain(|line| line.len() <= 2_048);
    }

    pub(crate) fn memory_pressure(state: &OpsState) -> u8 {
        let units = state.events.len()
            + state.logs.len()
            + state.executions.len()
            + state.explainability.len()
            + state.reasoning_stream.len();
        ((units as u16 * 100) / 600).min(100) as u8
    }
}

pub(crate) struct SecureLogger {
    records: Vec<SecureLogRecord>,
}

impl SecureLogger {
    pub(crate) fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub(crate) fn push(&mut self, message: &str) -> SecureLogRecord {
        let sequence = self.records.len() as u64 + 1;
        let previous_hash = self.records.last().map(|r| r.hash).unwrap_or(0);
        let redacted = redact_sensitive(message);
        let timestamp = now_ts();
        let hash = stable_hash(&(
            sequence,
            previous_hash,
            redacted.as_str(),
            timestamp.as_str(),
        ));
        let record = SecureLogRecord {
            sequence,
            message: redacted,
            previous_hash,
            hash,
            timestamp,
        };
        self.records.push(record.clone());
        record
    }
}

pub(crate) fn redact_sensitive(input: &str) -> String {
    input
        .split_whitespace()
        .map(|part| {
            let lower = part.to_ascii_lowercase();
            if lower.contains("password=")
                || lower.contains("token=")
                || lower.contains("api_key=")
                || lower.contains("authorization:")
            {
                "[redacted]"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn bounded_count(value: &str, min: u16, max: u16) -> bool {
    value
        .parse::<u16>()
        .map(|parsed| parsed >= min && parsed <= max)
        .unwrap_or(false)
}

fn safe_target(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('-')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@' | '/'))
}

fn stable_hash<T: Hash>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn is_risky_dependency(name: &str, version: &str) -> bool {
    let old_major_zero = version
        .split('.')
        .next()
        .and_then(|major| major.parse::<u16>().ok())
        .map(|major| major == 0)
        .unwrap_or(false);
    matches!(
        name,
        "openssl" | "native-tls" | "ring" | "hyper" | "h2" | "tokio" | "reqwest"
    ) && old_major_zero
}

fn risky_listening_port(port: u16) -> Option<(&'static str, &'static str)> {
    match port {
        22 | 2375 | 2376 | 5432 | 6379 | 9200 | 9300 => Some(("high", "admin or data service")),
        6333 | 7878 | 8080 | 9090 | 11434 => Some(("medium", "operations service")),
        _ => None,
    }
}

fn is_remote_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && !lower.contains("://localhost")
        && !lower.contains("://127.0.0.1")
        && !lower.contains("://[::1]")
}

fn local_loopback_http(value: &str) -> bool {
    let lower = value.trim().trim_end_matches('/').to_ascii_lowercase();
    lower.starts_with("http://localhost:")
        || lower.starts_with("http://127.0.0.1:")
        || lower.starts_with("http://[::1]:")
}
