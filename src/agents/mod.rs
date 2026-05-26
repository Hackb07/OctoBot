use std::collections::{HashMap, HashSet};

use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::{
    models::{
        AgentRole, AgentRuntime, AgentRuntimeKind, AgentStatus, ExplainabilityRecord, OpsEvent,
        RuntimeStatus,
    },
    utils::{next_id, now_ts},
};

#[derive(Debug, Clone)]
pub(crate) struct MemoryEntry {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) timestamp: String,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct AgentMemory {
    store: HashMap<String, Vec<MemoryEntry>>,
}

impl AgentMemory {
    pub(crate) fn store(&mut self, agent: &str, key: &str, value: &str) {
        self.store
            .entry(agent.into())
            .or_default()
            .push(MemoryEntry {
                key: key.into(),
                value: value.into(),
                timestamp: now_ts(),
            });
        info!(agent, key, "agent memory stored");
    }

    pub(crate) fn recall(&self, agent: &str) -> Vec<MemoryEntry> {
        self.store.get(agent).cloned().unwrap_or_default()
    }

    pub(crate) fn recall_key(&self, agent: &str, key: &str) -> Option<MemoryEntry> {
        self.store
            .get(agent)?
            .iter()
            .rev()
            .find(|entry| entry.key == key)
            .cloned()
    }

    pub(crate) fn all_agents(&self) -> Vec<String> {
        self.store.keys().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentDescriptor {
    pub(crate) name: String,
    pub(crate) role: AgentRole,
    pub(crate) runtime_kind: AgentRuntimeKind,
    pub(crate) endpoint: String,
}

#[derive(Debug, Default)]
pub(crate) struct AgentRegistry {
    agents: HashMap<String, AgentDescriptor>,
}

impl AgentRegistry {
    pub(crate) fn register(&mut self, descriptor: AgentDescriptor) -> bool {
        let is_new = !self.agents.contains_key(&descriptor.name);
        self.agents.insert(descriptor.name.clone(), descriptor);
        is_new
    }

    pub(crate) fn get(&self, name: &str) -> Option<&AgentDescriptor> {
        self.agents.get(name)
    }
}

#[derive(Debug, Default)]
pub(crate) struct AgentRuntimeManager {
    registry: AgentRegistry,
    running_tasks: HashSet<String>,
    pub(crate) memory: AgentMemory,
}

impl AgentRuntimeManager {
    pub(crate) fn handle_event(
        &mut self,
        event: &OpsEvent,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        match event {
            OpsEvent::AgentSpawned {
                name,
                role,
                timestamp,
            } => self.register_local_agent(name, role, timestamp, event_tx),
            OpsEvent::TaskAssigned {
                agent,
                task,
                timestamp,
            } => self.start_agent_task(agent, task, timestamp, event_tx),
            OpsEvent::AgentLifecycleChanged {
                agent,
                status,
                timestamp: _,
                ..
            } if *status != AgentStatus::Running => {
                self.running_tasks.remove(agent);
            }
            OpsEvent::AgentMemoryStored {
                agent,
                key,
                value,
                ..
            } => {
                self.memory.store(agent, key, value);
            }
            _ => {}
        }
    }

    pub(crate) fn memory_context(&self, agent: &str) -> String {
        let entries = self.memory.recall(agent);
        if entries.is_empty() {
            return String::new();
        }
        let mut ctx = String::from("Previous task memory:\n");
        for entry in entries.iter().rev().take(5) {
            ctx.push_str(&format!("  [{key}] {value}\n", key = entry.key, value = entry.value));
        }
        ctx
    }

    fn register_local_agent(
        &mut self,
        name: &str,
        role: &AgentRole,
        timestamp: &str,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        let descriptor = AgentDescriptor {
            name: name.into(),
            role: role.clone(),
            runtime_kind: AgentRuntimeKind::LocalProcess,
            endpoint: format!("local://agent/{name}"),
        };
        let is_new = self.registry.register(descriptor.clone());
        if !is_new {
            warn!(
                agent = name,
                "agent registration replaced existing descriptor"
            );
        }

        let _ = event_tx.send(OpsEvent::RuntimeUpdated {
            runtime: AgentRuntime {
                agent: descriptor.name.clone(),
                kind: descriptor.runtime_kind.clone(),
                endpoint: descriptor.endpoint.clone(),
                status: RuntimeStatus::Local,
                heartbeat: timestamp.into(),
                notes: "registered by dynamic agent runtime manager".into(),
            },
        });
        let _ = event_tx.send(OpsEvent::AgentTelemetryRecorded {
            agent: name.into(),
            metric: "registration".into(),
            value: 1,
            timestamp: timestamp.into(),
        });
        let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
            record: ExplainabilityRecord {
                id: next_id("exp"),
                action: format!("Register agent runtime {name}"),
                why: "An operator or workflow requested a runtime-managed agent instead of using a seeded agent fixture.".into(),
                evidence: vec![
                    format!("role={:?}", descriptor.role),
                    "runtime_kind=LocalProcess".into(),
                    "agent_state=Waiting".into(),
                ],
                confidence: 100,
                tools_used: vec!["agent-runtime-manager".into()],
                timestamp: timestamp.into(),
            },
        });
        info!(agent = name, role = ?role, "registered dynamic agent");
    }

    fn start_agent_task(
        &mut self,
        agent: &str,
        task: &str,
        timestamp: &str,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        if self.registry.get(agent).is_none() {
            let _ = event_tx.send(OpsEvent::AgentLifecycleChanged {
                agent: agent.into(),
                status: AgentStatus::Failed,
                task: format!("task rejected; agent {agent} is not registered"),
                timestamp: timestamp.into(),
            });
            let _ = event_tx.send(OpsEvent::AgentTelemetryRecorded {
                agent: agent.into(),
                metric: "task_rejected_unregistered_agent".into(),
                value: 1,
                timestamp: timestamp.into(),
            });
            warn!(agent, task, "rejected task for unregistered agent");
            return;
        }

        self.running_tasks.insert(agent.into());
        let _ = event_tx.send(OpsEvent::AgentLifecycleChanged {
            agent: agent.into(),
            status: AgentStatus::Running,
            task: task.into(),
            timestamp: timestamp.into(),
        });
        let _ = event_tx.send(OpsEvent::AgentTelemetryRecorded {
            agent: agent.into(),
            metric: "task_started".into(),
            value: 1,
            timestamp: timestamp.into(),
        });
        let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
            record: ExplainabilityRecord {
                id: next_id("exp"),
                action: format!("Start agent task for {agent}"),
                why: "A registered runtime agent received a task through the event bus.".into(),
                evidence: vec![
                    format!("agent={agent}"),
                    format!("task={task}"),
                    "transport=OpsEvent::TaskAssigned".into(),
                ],
                confidence: 100,
                tools_used: vec!["agent-runtime-manager".into()],
                timestamp: now_ts(),
            },
        });
        info!(agent, task, "started dynamic agent task");
    }
}
