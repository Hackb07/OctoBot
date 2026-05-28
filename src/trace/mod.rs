use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::{
    models::{ExplainabilityRecord, OpsEvent, OpsState},
    persistence::PersistenceRuntime,
    utils::{next_id, now_ts},
};

#[derive(Debug, Clone)]
pub(crate) struct ExecutionSpan {
    pub(crate) span_id: String,
    pub(crate) parent_span_id: Option<String>,
    pub(crate) operation: String,
    pub(crate) target: String,
    pub(crate) start_time: String,
    pub(crate) end_time: Option<String>,
    pub(crate) status: SpanStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SpanStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplaySession {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) created_at: String,
    pub(crate) event_count: usize,
}

#[derive(Default)]
pub(crate) struct TraceEngine {
    active_spans: HashMap<String, ExecutionSpan>,
    span_counter: u64,
}

impl TraceEngine {
    /// Start a new execution span.
    pub(crate) fn start_span(
        &mut self,
        parent_span_id: Option<String>,
        operation: String,
        target: String,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) -> String {
        self.span_counter += 1;
        let span_id = format!("span-{}-{}", target, self.span_counter);
        let span = ExecutionSpan {
            span_id: span_id.clone(),
            parent_span_id,
            operation: operation.clone(),
            target: target.clone(),
            start_time: now_ts(),
            end_time: None,
            status: SpanStatus::Running,
        };
        self.active_spans.insert(span_id.clone(), span);

        let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
            record: ExplainabilityRecord {
                id: next_id("exp-trace"),
                action: format!("trace-span-start-{span_id}"),
                why: format!("Started {operation} on {target}"),
                evidence: vec![format!("span_id={span_id}")],
                confidence: 100,
                tools_used: vec!["trace-engine".into()],
                timestamp: now_ts(),
            },
        });

        span_id
    }

    /// Complete an execution span.
    pub(crate) fn end_span(
        &mut self,
        span_id: &str,
        success: bool,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        if let Some(span) = self.active_spans.get_mut(span_id) {
            span.end_time = Some(now_ts());
            span.status = if success {
                SpanStatus::Completed
            } else {
                SpanStatus::Failed
            };

            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: ExplainabilityRecord {
                    id: next_id("exp-trace"),
                    action: format!("trace-span-end-{span_id}"),
                    why: format!(
                        "Completed {} on {}: {:?}",
                        span.operation, span.target, span.status
                    ),
                    evidence: vec![
                        format!("span_id={span_id}"),
                        format!("status={:?}", span.status),
                    ],
                    confidence: 100,
                    tools_used: vec!["trace-engine".into()],
                    timestamp: now_ts(),
                },
            });
        }
    }

    pub(crate) fn active_span_count(&self) -> usize {
        self.active_spans
            .values()
            .filter(|s| s.status == SpanStatus::Running)
            .count()
    }

    pub(crate) fn active_span_summary(&self) -> Vec<String> {
        self.active_spans
            .values()
            .filter(|s| s.status == SpanStatus::Running)
            .map(|s| {
                format!(
                    "[{}] {} → {} (started {})",
                    s.span_id, s.operation, s.target, s.start_time
                )
            })
            .collect()
    }
}

/// Manage persistent replay sessions with PostgreSQL.
pub(crate) struct ReplayManager;

impl ReplayManager {
    /// Start a new replay session and persist it.
    pub(crate) async fn start_session(
        _persistence: &PersistenceRuntime,
        name: &str,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) -> ReplaySession {
        let session = ReplaySession {
            id: next_id("session"),
            name: name.into(),
            created_at: now_ts(),
            event_count: 0,
        };

        let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
            record: ExplainabilityRecord {
                id: next_id("exp-replay"),
                action: format!("replay-session-start-{}", session.id),
                why: format!("Started replay session: {name}"),
                evidence: vec![
                    format!("session_id={}", session.id),
                    "replay_session_created".into(),
                ],
                confidence: 100,
                tools_used: vec!["replay-manager".into()],
                timestamp: now_ts(),
            },
        });

        let _ = event_tx.send(OpsEvent::ReplayStarted {
            timestamp: now_ts(),
        });

        session
    }

    /// Step through events with optional filtering.
    pub(crate) async fn step_session(
        persistence: &PersistenceRuntime,
        state: &OpsState,
        filter: Option<EventFilter>,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        let all_events = persistence.replay_events().await.unwrap_or_default();
        let filtered: Vec<_> = if let Some(filter) = filter {
            all_events
                .iter()
                .enumerate()
                .filter(|(_, e)| {
                    let type_name = crate::persistence::event_type(e);
                    match &filter {
                        EventFilter::All => true,
                        EventFilter::Type(name) => type_name == *name,
                        EventFilter::Types(names) => names.contains(&type_name),
                    }
                })
                .collect()
        } else {
            all_events.iter().enumerate().collect()
        };

        let total = filtered.len();
        let position = state.replay.position.min(total);

        if position < total {
            let _ = event_tx.send(OpsEvent::ReplayStepped {
                position: position + 1,
                timestamp: now_ts(),
            });
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: ExplainabilityRecord {
                    id: next_id("exp-replay"),
                    action: "replay-step".into(),
                    why: format!(
                        "Replay step {}/{}: {:?}",
                        position + 1,
                        total,
                        filtered
                            .get(position)
                            .map(|(_, e)| crate::persistence::event_type(e))
                    ),
                    evidence: vec![
                        format!("position={}", position + 1),
                        format!("total={total}"),
                    ],
                    confidence: 100,
                    tools_used: vec!["replay-manager".into()],
                    timestamp: now_ts(),
                },
            });
        }
    }

    /// Build a structured evidence chain from explainability records.
    pub(crate) fn build_evidence_chain(state: &OpsState) -> Vec<String> {
        let mut chain = Vec::new();
        for record in state.explainability.iter().rev().take(20) {
            chain.push(format!(
                "[{confidence}%] {action}: {why}",
                confidence = record.confidence,
                action = record.action,
                why = record.why
            ));
            for evidence in &record.evidence {
                chain.push(format!("  ├─ {evidence}"));
            }
        }
        chain
    }

    /// Reconstruct a decision chain for a specific incident.
    pub(crate) fn reconstruct_decision_chain(state: &OpsState, incident_id: &str) -> Vec<String> {
        let mut chain = Vec::new();

        // Collect timeline events for this incident
        for event in &state.timeline {
            if event.related_incident.as_deref() == Some(incident_id) {
                chain.push(format!(
                    "[{time}] {source}: {summary}",
                    time = event.timestamp,
                    source = event.source,
                    summary = event.summary
                ));
            }
        }

        // Collect explainability records mentioning this incident
        for record in &state.explainability {
            if record.action.contains(incident_id)
                || record.evidence.iter().any(|e| e.contains(incident_id))
            {
                chain.push(format!(
                    "  → [{confidence}%] {action}: {why}",
                    confidence = record.confidence,
                    action = record.action,
                    why = record.why
                ));
            }
        }

        if chain.is_empty() {
            chain.push(format!("No decision chain data for incident {incident_id}"));
        }

        chain
    }
}

#[derive(Debug, Clone)]
pub(crate) enum EventFilter {
    All,
    Type(&'static str),
    Types(Vec<&'static str>),
}
