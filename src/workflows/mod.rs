use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::Path,
};

use color_eyre::eyre::{Context, Result, eyre};
use serde::{Deserialize, Serialize};

use crate::{models::WorkflowDefinitionSummary, utils::now_ts};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkflowDefinition {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) entrypoint: String,
    pub(crate) nodes: Vec<WorkflowNode>,
}

pub(crate) fn load_workflows_from_dir(path: impl AsRef<Path>) -> Result<Vec<DagWorkflowRuntime>> {
    let mut workflows = Vec::new();
    let path = path.as_ref();
    if !path.exists() {
        return Ok(workflows);
    }
    for entry in fs::read_dir(path).with_context(|| format!("reading {}", path.display()))? {
        let entry = entry.context("reading workflow directory entry")?;
        let path = entry.path();
        let is_yaml = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| matches!(extension, "yaml" | "yml"))
            .unwrap_or(false);
        if !is_yaml {
            continue;
        }
        let contents =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        workflows.push(DagWorkflowRuntime::from_yaml(&contents)?);
    }
    Ok(workflows)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkflowNode {
    pub(crate) id: String,
    pub(crate) kind: WorkflowNodeKind,
    #[serde(default)]
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) agent: Option<String>,
    #[serde(default)]
    pub(crate) depends_on: Vec<String>,
    #[serde(default)]
    pub(crate) retry: RetryPolicy,
    #[serde(default)]
    pub(crate) approval_required: bool,
    #[serde(default)]
    pub(crate) condition: Option<String>,
    #[serde(default)]
    pub(crate) on_success: Option<String>,
    #[serde(default)]
    pub(crate) on_failure: Option<String>,
    #[serde(default)]
    pub(crate) rollback: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum WorkflowNodeKind {
    Command,
    AgentTask,
    Approval,
    Condition,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct RetryPolicy {
    pub(crate) attempts: u8,
    pub(crate) backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            attempts: 1,
            backoff_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum NodeStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct NodeExecutionState {
    pub(crate) status: NodeStatus,
    pub(crate) attempts: u8,
    pub(crate) error: Option<String>,
}

impl Default for NodeExecutionState {
    fn default() -> Self {
        Self {
            status: NodeStatus::Pending,
            attempts: 0,
            error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DagWorkflowRuntime {
    definition: WorkflowDefinition,
    pub(crate) node_states: HashMap<String, NodeExecutionState>,
    pub(crate) id: String,
}

impl DagWorkflowRuntime {
    pub(crate) fn new(id: String, name: String, nodes: Vec<WorkflowNode>) -> Self {
        let definition = WorkflowDefinition {
            id: id.clone(),
            name,
            entrypoint: nodes.first().map(|n| n.id.clone()).unwrap_or_default(),
            nodes,
        };
        let node_states = definition
            .nodes
            .iter()
            .map(|n| (n.id.clone(), NodeExecutionState::default()))
            .collect();
        Self {
            id,
            definition,
            node_states,
        }
    }

    pub(crate) fn from_yaml(input: &str) -> Result<Self> {
        let definition: WorkflowDefinition =
            serde_yaml::from_str(input).context("parsing workflow YAML")?;
        validate_definition(&definition)?;
        let node_states = definition
            .nodes
            .iter()
            .map(|n| (n.id.clone(), NodeExecutionState::default()))
            .collect();
        Ok(Self {
            id: definition.id.clone(),
            definition,
            node_states,
        })
    }

    pub(crate) fn summary(&self) -> WorkflowDefinitionSummary {
        WorkflowDefinitionSummary {
            id: self.definition.id.clone(),
            name: self.definition.name.clone(),
            node_count: self.definition.nodes.len(),
            entrypoint: self.definition.entrypoint.clone(),
            timestamp: now_ts(),
        }
    }

    pub(crate) fn ready_nodes(&self) -> Vec<&WorkflowNode> {
        self.definition
            .nodes
            .iter()
            .filter(|node| {
                let state = self
                    .node_states
                    .get(&node.id)
                    .map(|s| &s.status)
                    .unwrap_or(&NodeStatus::Pending);
                matches!(state, NodeStatus::Pending)
                    && node.depends_on.iter().all(|dependency| {
                        self.node_states
                            .get(dependency)
                            .map(|s| {
                                matches!(s.status, NodeStatus::Succeeded | NodeStatus::Skipped)
                            })
                            .unwrap_or(false)
                    })
            })
            .collect()
    }

    pub(crate) fn all_running(&self) -> bool {
        self.node_states.values().all(|s| {
            matches!(
                s.status,
                NodeStatus::Succeeded | NodeStatus::Failed | NodeStatus::Skipped
            )
        })
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.node_states.values().all(|s| {
            matches!(
                s.status,
                NodeStatus::Succeeded | NodeStatus::Failed | NodeStatus::Skipped
            )
        })
    }

    pub(crate) fn mark_running(&mut self, node_id: &str) -> Result<()> {
        let state = self
            .node_states
            .get_mut(node_id)
            .ok_or_else(|| eyre!("node `{node_id}` does not exist"))?;
        state.status = NodeStatus::Running;
        state.attempts += 1;
        Ok(())
    }

    pub(crate) fn mark_succeeded(&mut self, node_id: &str) -> Result<()> {
        let state = self
            .node_states
            .get_mut(node_id)
            .ok_or_else(|| eyre!("node `{node_id}` does not exist"))?;
        state.status = NodeStatus::Succeeded;
        state.error = None;
        Ok(())
    }

    pub(crate) fn mark_failed(&mut self, node_id: &str, error: String) -> Result<()> {
        let state = self
            .node_states
            .get_mut(node_id)
            .ok_or_else(|| eyre!("node `{node_id}` does not exist"))?;
        state.status = NodeStatus::Failed;
        state.error = Some(error);
        Ok(())
    }

    pub(crate) fn mark_skipped(&mut self, node_id: &str) -> Result<()> {
        let state = self
            .node_states
            .get_mut(node_id)
            .ok_or_else(|| eyre!("node `{node_id}` does not exist"))?;
        state.status = NodeStatus::Skipped;
        Ok(())
    }

    pub(crate) fn can_retry(&self, node_id: &str) -> bool {
        self.definition
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| {
                let attempts = self
                    .node_states
                    .get(node_id)
                    .map(|s| s.attempts)
                    .unwrap_or(0);
                attempts < n.retry.attempts
            })
            .unwrap_or(false)
    }

    pub(crate) fn retry_backoff_ms(&self, node_id: &str) -> u64 {
        self.definition
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.retry.backoff_ms)
            .unwrap_or(0)
    }

    pub(crate) fn progress(&self) -> u16 {
        let total = self.definition.nodes.len().max(1);
        let done = self
            .node_states
            .values()
            .filter(|s| {
                matches!(
                    s.status,
                    NodeStatus::Succeeded | NodeStatus::Failed | NodeStatus::Skipped
                )
            })
            .count();
        ((done as f64 / total as f64) * 100.0) as u16
    }

    pub(crate) fn get_node(&self, node_id: &str) -> Option<&WorkflowNode> {
        self.definition.nodes.iter().find(|n| n.id == node_id)
    }

    pub(crate) fn get_node_mut(&mut self, node_id: &str) -> Option<&mut WorkflowNode> {
        self.definition.nodes.iter_mut().find(|n| n.id == node_id)
    }

    /// Reset a failed node back to Pending so the scheduler retries it.
    pub(crate) fn reset_node(&mut self, node_id: &str) -> Result<()> {
        let state = self
            .node_states
            .get_mut(node_id)
            .ok_or_else(|| eyre!("node `{node_id}` does not exist"))?;
        state.status = NodeStatus::Pending;
        Ok(())
    }

    /// Reset a node so the scheduler picks it up for rollback execution.
    pub(crate) fn mark_for_rollback(&mut self, node_id: &str) -> Result<()> {
        let state = self
            .node_states
            .get_mut(node_id)
            .ok_or_else(|| eyre!("node `{node_id}` does not exist"))?;
        // Only reset if currently not in a terminal state
        if matches!(state.status, NodeStatus::Pending | NodeStatus::Running) {
            state.status = NodeStatus::Pending;
        }
        Ok(())
    }

    /// Evaluate a condition expression against a simple key=value context.
    pub(crate) fn evaluate_condition(
        &self,
        condition: &str,
        context: &HashMap<String, String>,
    ) -> bool {
        // Supports: "key=value", "key!=value", "key>number", "key<number"
        if let Some((key, expected)) = condition.split_once('=') {
            if key.ends_with('!') {
                let actual_key = key.trim_end_matches('!');
                context
                    .get(actual_key)
                    .map(|v| v != expected)
                    .unwrap_or(false)
            } else if key.ends_with('>') {
                let actual_key = key.trim_end_matches('>');
                let actual: f64 = context
                    .get(actual_key)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0);
                let expected_val: f64 = expected.parse().unwrap_or(0.0);
                actual > expected_val
            } else if key.ends_with('<') {
                let actual_key = key.trim_end_matches('<');
                let actual: f64 = context
                    .get(actual_key)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0);
                let expected_val: f64 = expected.parse().unwrap_or(0.0);
                actual < expected_val
            } else {
                context.get(key).map(|v| v == expected).unwrap_or(false)
            }
        } else {
            // Simple key presence check
            context.contains_key(condition)
        }
    }
}

fn validate_definition(definition: &WorkflowDefinition) -> Result<()> {
    if definition.id.trim().is_empty() {
        return Err(eyre!("workflow id cannot be empty"));
    }
    if definition.nodes.is_empty() {
        return Err(eyre!("workflow must contain at least one node"));
    }
    let mut ids = HashSet::new();
    for node in &definition.nodes {
        if !ids.insert(node.id.clone()) {
            return Err(eyre!("duplicate workflow node `{}`", node.id));
        }
    }
    if !ids.contains(&definition.entrypoint) {
        return Err(eyre!(
            "workflow entrypoint `{}` does not match a node",
            definition.entrypoint
        ));
    }
    for node in &definition.nodes {
        for dependency in &node.depends_on {
            if !ids.contains(dependency) {
                return Err(eyre!(
                    "workflow node `{}` depends on missing node `{dependency}`",
                    node.id
                ));
            }
        }
    }
    ensure_acyclic(definition)
}

fn ensure_acyclic(definition: &WorkflowDefinition) -> Result<()> {
    let mut indegree = HashMap::<String, usize>::new();
    let mut outgoing = HashMap::<String, Vec<String>>::new();
    for node in &definition.nodes {
        indegree.entry(node.id.clone()).or_insert(0);
        for dependency in &node.depends_on {
            *indegree.entry(node.id.clone()).or_insert(0) += 1;
            outgoing
                .entry(dependency.clone())
                .or_default()
                .push(node.id.clone());
        }
    }
    let mut queue = indegree
        .iter()
        .filter(|&(_node, count)| *count == 0)
        .map(|(node, _count)| node.clone())
        .collect::<VecDeque<_>>();
    let mut visited = 0;
    while let Some(node) = queue.pop_front() {
        visited += 1;
        for child in outgoing.get(&node).into_iter().flatten() {
            if let Some(count) = indegree.get_mut(child) {
                *count -= 1;
                if *count == 0 {
                    queue.push_back(child.clone());
                }
            }
        }
    }
    if visited != definition.nodes.len() {
        return Err(eyre!("workflow DAG contains a cycle"));
    }
    Ok(())
}
