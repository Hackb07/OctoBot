use std::{
    collections::{HashMap, VecDeque},
    hash::{Hash, Hasher},
    time::{Duration, Instant},
};

use crate::models::{
    AgentRole, ExplainabilityRecord, OpsEvent, OpsState, PluginDescriptor, PluginKind, UserRole,
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
        {
            if let Some(reason) = Self::detect_prompt_attack(text) {
                return Err(format!("blocked malicious tool arguments: {reason}"));
            }
        }
        Ok(())
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
