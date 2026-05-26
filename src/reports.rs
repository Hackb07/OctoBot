use std::fs;

use color_eyre::eyre::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{
    models::{
        AgentLink, AgentRuntime, ExecutionRecord, ExplainabilityRecord, Incident, KnowledgeEdge,
        KnowledgeNode, OpsState, PluginDescriptor, RecoveryAction, ResearchConfidenceProfile,
        SandboxPolicy, TimelineEvent, Workflow,
    },
    utils::next_id,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedReport {
    id: String,
    topic: String,
    conclusion: String,
    confidence: u8,
    generated_at: String,
    workspace: String,
    environment: String,
    incidents: Vec<Incident>,
    workflows: Vec<Workflow>,
    executions: Vec<ExecutionRecord>,
    explainability: Vec<ExplainabilityRecord>,
    coordination_links: Vec<AgentLink>,
    timeline: Vec<TimelineEvent>,
    recovery_actions: Vec<RecoveryAction>,
    research_profile: ResearchConfidenceProfile,
    plugins: Vec<PluginDescriptor>,
    runtimes: Vec<AgentRuntime>,
    knowledge_nodes: Vec<KnowledgeNode>,
    knowledge_edges: Vec<KnowledgeEdge>,
    sandbox_policy: SandboxPolicy,
}

pub(crate) fn write_report_json(
    state: &OpsState,
    topic: &str,
    conclusion: &str,
    confidence: u8,
    timestamp: &str,
) -> Result<String> {
    fs::create_dir_all("reports").context("creating reports directory")?;

    let report = GeneratedReport {
        id: next_id("report"),
        topic: topic.into(),
        conclusion: conclusion.into(),
        confidence,
        generated_at: timestamp.into(),
        workspace: state.workspace.clone(),
        environment: state.environment.clone(),
        incidents: state.incidents.clone(),
        workflows: state.workflows.clone(),
        executions: state.executions.clone(),
        explainability: state.explainability.clone(),
        coordination_links: state.coordination_links.clone(),
        timeline: state.timeline.clone(),
        recovery_actions: state.recovery_actions.clone(),
        research_profile: state.research_profile.clone(),
        plugins: state.plugins.clone(),
        runtimes: state.runtimes.clone(),
        knowledge_nodes: state.knowledge_nodes.clone(),
        knowledge_edges: state.knowledge_edges.clone(),
        sandbox_policy: state.sandbox_policy.clone(),
    };

    let path = format!("reports/{}.json", report.id);
    let json = serde_json::to_string_pretty(&report).context("serializing generated report")?;
    fs::write(&path, json).context("writing generated report JSON")?;

    Ok(path)
}
