use std::collections::{HashMap, VecDeque};

use tokio::sync::mpsc;

use crate::{
    ai::{AgentPrompt, AiClient},
    models::{
        Incident, KnowledgeEdge, OpsEvent, OpsState, ResearchConfidenceProfile, TimelineEvent,
    },
    utils::{next_id, now_ts},
};

const ANOMALY_ZSCORE_THRESHOLD: f64 = 2.0;
const SLO_TARGET: f64 = 99.9;

pub(crate) struct ObservabilityEngine {
    /// Rolling window of metric values for anomaly detection
    metric_window: VecDeque<f64>,
    /// Cache of the last anomaly detection result
    last_anomaly_score: Option<AnomalyScore>,
}

pub(crate) struct AnomalyScore {
    value: f64,
    zscore: f64,
    timestamp: String,
}

impl Default for ObservabilityEngine {
    fn default() -> Self {
        Self {
            metric_window: VecDeque::with_capacity(30),
            last_anomaly_score: None,
        }
    }
}

impl ObservabilityEngine {
    /// 1. AI log analysis — send log lines to AI for pattern/anomaly analysis
    pub(crate) async fn analyze_logs(
        &self,
        ai_client: Option<&AiClient>,
        logs: &[String],
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        let Some(client) = ai_client else { return };
        let sample: Vec<&str> = logs.iter().rev().take(20).map(|s| s.as_str()).collect();
        if sample.is_empty() {
            return;
        }
        let log_text = sample.join("\n");
        let prompt = AgentPrompt {
            system: "You are an observability AI analyzing system logs. Identify anomalies, error patterns, and root causes.".into(),
            user: format!("Analyze these log lines for anomalies and patterns:\n```\n{log_text}\n```"),
            tools: vec![],
        };
        match client.run_agent_turn(prompt).await {
            Ok(response) => {
                let _ = event_tx.send(OpsEvent::ResearchCompleted {
                    topic: "ai-log-analysis".into(),
                    conclusion: response.content,
                    confidence: 75,
                    timestamp: now_ts(),
                });
            }
            Err(error) => {
                tracing::warn!(%error, "AI log analysis failed");
            }
        }
    }

    /// 2. Anomaly detection using z-score on the metric window
    pub(crate) fn detect_anomalies(&mut self, metrics: &[u64]) -> Option<AnomalyScore> {
        for &m in metrics {
            if self.metric_window.len() >= 30 {
                self.metric_window.pop_front();
            }
            self.metric_window.push_back(m as f64);
        }
        if self.metric_window.len() < 5 {
            return None;
        }
        let n = self.metric_window.len() as f64;
        let mean: f64 = self.metric_window.iter().sum::<f64>() / n;
        let variance: f64 = self
            .metric_window
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>()
            / n;
        let stddev = variance.sqrt();
        if stddev < 1e-10 {
            return None;
        }
        let latest = *self.metric_window.back().unwrap_or(&0.0);
        let zscore = (latest - mean).abs() / stddev;
        let score = AnomalyScore {
            value: latest,
            zscore,
            timestamp: now_ts(),
        };
        self.last_anomaly_score = Some(AnomalyScore {
            value: latest,
            zscore,
            timestamp: now_ts(),
        });
        if zscore > ANOMALY_ZSCORE_THRESHOLD {
            Some(score)
        } else {
            None
        }
    }

    /// 3. Root-cause hypothesis engine — AI generates hypotheses from incident + topology + metrics
    pub(crate) async fn generate_hypothesis(
        &self,
        ai_client: Option<&AiClient>,
        incident: &Incident,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        let Some(client) = ai_client else { return };
        let infra_summary: String = state
            .infra
            .iter()
            .map(|n| {
                format!(
                    "  {} ({}): health={}, cpu={}, memory={}",
                    n.name, n.kind, n.health, n.cpu, n.memory
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let edge_summary: String = state
            .knowledge_edges
            .iter()
            .map(|e| format!("  {} --[{}]--> {}", e.from, e.relation, e.to))
            .collect::<Vec<_>>()
            .join("\n");
        let timeline_summary: String = state
            .timeline
            .iter()
            .rev()
            .take(10)
            .map(|t| format!("  [{}] {}: {}", t.timestamp, t.source, t.summary))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = AgentPrompt {
            system: "You are a root-cause analysis AI. Based on incident data, infrastructure state, \
                     knowledge graph edges, and timeline events, generate a concise root-cause hypothesis. \
                     Format: HYPOTHESIS: <one line>, CONFIDENCE: <0-100>, EVIDENCE: <list>".into(),
            user: format!(
                "Incident: {id} ({severity}) on {service}\nHypothesis: {hypothesis}\n\n\
                 Infrastructure:\n{infra}\n\nKnowledge edges:\n{edges}\n\nRecent timeline:\n{timeline}",
                id = incident.id, service = incident.service, severity = incident.severity,
                hypothesis = incident.hypothesis, infra = infra_summary,
                edges = edge_summary, timeline = timeline_summary
            ),
            tools: vec![],
        };
        match client.run_agent_turn(prompt).await {
            Ok(response) => {
                let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                    record: crate::models::ExplainabilityRecord {
                        id: next_id("exp-rca"),
                        action: format!("root-cause-hypothesis-{}", incident.id),
                        why: response.content.clone(),
                        evidence: vec![
                            format!("incident={}", incident.id),
                            format!("service={}", incident.service),
                        ],
                        confidence: 70,
                        tools_used: vec!["observability-engine".into()],
                        timestamp: now_ts(),
                    },
                });
                let _ = event_tx.send(OpsEvent::ResearchCompleted {
                    topic: format!("root-cause-{}", incident.id),
                    conclusion: response.content,
                    confidence: 70,
                    timestamp: now_ts(),
                });
            }
            Err(error) => {
                tracing::warn!(%error, "hypothesis generation failed");
            }
        }
    }

    /// 4. Evidence correlation — links related evidence by timestamp proximity and shared services
    pub(crate) fn correlate_evidence(&self, state: &OpsState) -> Vec<String> {
        let mut correlations = Vec::new();
        if state.incidents.len() < 2 {
            return correlations;
        }
        // Find incidents sharing a service
        let services: HashMap<&str, Vec<&Incident>> =
            state.incidents.iter().fold(HashMap::new(), |mut acc, inc| {
                acc.entry(inc.service.as_str()).or_default().push(inc);
                acc
            });
        for (service, incidents) in &services {
            if incidents.len() > 1 {
                correlations.push(format!(
                    "Service {} has {} related incidents",
                    service,
                    incidents.len()
                ));
            }
        }
        // Correlate knowledge edges with active incidents
        for edge in &state.knowledge_edges {
            for inc in &state.incidents {
                if edge.from.contains(&inc.id)
                    || edge.to.contains(&inc.id)
                    || edge.from.contains(&inc.service)
                {
                    correlations.push(format!(
                        "Edge {}--{}--{} correlates with incident {}",
                        edge.from, edge.relation, edge.to, inc.id
                    ));
                }
            }
        }
        correlations
    }

    /// 5. Dependency impact analysis — uses topology to compute blast radius
    pub(crate) fn analyze_impact(&self, failed_service: &str, state: &OpsState) -> Vec<String> {
        let mut impacted = Vec::new();
        for edge in &state.knowledge_edges {
            if edge.from == failed_service || edge.to == failed_service {
                impacted.push(format!(
                    "{} {} {}",
                    edge.from,
                    if edge.from == failed_service {
                        "impacts"
                    } else {
                        "depends-on"
                    },
                    edge.to
                ));
            }
            // Also check transitive dependency via matching name
            if edge.from.contains(failed_service) || edge.to.contains(failed_service) {
                impacted.push(format!(
                    "transitive: {} --{}--> {}",
                    edge.from, edge.relation, edge.to
                ));
            }
        }
        if impacted.is_empty() {
            impacted.push(format!("No dependency edges found for {failed_service}"));
        }
        impacted
    }

    /// 6. Incident summarization — AI-generated summary
    pub(crate) async fn summarize_incident(
        &self,
        ai_client: Option<&AiClient>,
        incident: &Incident,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        let Some(client) = ai_client else { return };
        let related_edges: Vec<&KnowledgeEdge> = state
            .knowledge_edges
            .iter()
            .filter(|e| {
                e.from.contains(&incident.id)
                    || e.to.contains(&incident.id)
                    || e.from.contains(&incident.service)
            })
            .collect();
        let related_timeline: Vec<&TimelineEvent> = state
            .timeline
            .iter()
            .filter(|t| t.related_incident.as_deref() == Some(&incident.id))
            .collect();

        let prompt = AgentPrompt {
            system: "Summarize the incident clearly and concisely. Include the service, severity, \
                     current hypothesis, related infrastructure dependencies, and key timeline events.".into(),
            user: format!(
                "Incident: {id} ({severity}) on {service}\nHypothesis: {hypothesis}\n\n\
                 Related knowledge edges ({edge_count}):\n{edges}\n\n\
                 Timeline events ({tl_count}):\n{timeline}",
                id = incident.id, severity = incident.severity, service = incident.service,
                hypothesis = incident.hypothesis,
                edge_count = related_edges.len(),
                edges = related_edges.iter().map(|e| format!("  {} --[{}]--> {}", e.from, e.relation, e.to)).collect::<Vec<_>>().join("\n"),
                tl_count = related_timeline.len(),
                timeline = related_timeline.iter().map(|t| format!("  {}: {}", t.source, t.summary)).collect::<Vec<_>>().join("\n")
            ),
            tools: vec![],
        };
        match client.run_agent_turn(prompt).await {
            Ok(response) => {
                let _ = event_tx.send(OpsEvent::ResearchCompleted {
                    topic: format!("incident-summary-{}", incident.id),
                    conclusion: response.content,
                    confidence: 80,
                    timestamp: now_ts(),
                });
            }
            Err(error) => {
                tracing::warn!(%error, "incident summarization failed");
            }
        }
    }

    /// 7. Predictive alerting — simple linear regression forecast
    pub(crate) fn predict_alert(&mut self, metrics: &[u64]) -> Option<PredictiveAlert> {
        if metrics.len() < 5 {
            return None;
        }
        let n = metrics.len() as f64;
        let indices: Vec<f64> = (0..metrics.len()).map(|i| i as f64).collect();
        let values: Vec<f64> = metrics.iter().map(|&v| v as f64).collect();
        let mean_x = indices.iter().sum::<f64>() / n;
        let mean_y = values.iter().sum::<f64>() / n;
        let mut num = 0.0;
        let mut den = 0.0;
        for (i, &v) in indices.iter().zip(values.iter()) {
            num += (i - mean_x) * (v - mean_y);
            den += (i - mean_x).powi(2);
        }
        if den.abs() < 1e-10 {
            return None;
        }
        let slope = num / den;
        let intercept = mean_y - slope * mean_x;
        // Predict next 3 values
        let mut forecasts = Vec::new();
        for step in 1..=3 {
            let pred = slope * (n + step as f64 - 1.0) + intercept;
            forecasts.push(pred.clamp(0.0, 100.0) as u8);
        }
        // Alert if forecast exceeds threshold
        let threshold = 85.0;
        let will_cross = forecasts.iter().any(|&v| v as f64 >= threshold);
        if will_cross {
            Some(PredictiveAlert {
                forecasts,
                slope,
                threshold,
                timestamp: now_ts(),
            })
        } else {
            None
        }
    }

    /// 8. SLO burn-rate analysis
    pub(crate) fn compute_slo_burn(&self, state: &OpsState) -> SloBurnReport {
        let total_seconds = state.uptime_secs.max(1);
        let good_seconds = state
            .infra
            .iter()
            .map(|n| (n.health as f64 / 100.0) * total_seconds as f64)
            .sum::<f64>()
            / state.infra.len().max(1) as f64;
        let availability = (good_seconds / total_seconds as f64) * 100.0;
        let error_budget_remaining = if availability >= SLO_TARGET {
            100.0
        } else {
            (availability / SLO_TARGET) * 100.0
        };
        let burn_rate = (SLO_TARGET - availability.min(SLO_TARGET)) / 100.0 * 3600.0; // per hour
        SloBurnReport {
            availability: availability.clamp(0.0, 100.0),
            slo_target: SLO_TARGET,
            error_budget_remaining: error_budget_remaining.clamp(0.0, 100.0),
            burn_rate: burn_rate.max(0.0),
        }
    }

    /// 9. Semantic log search — returns a query for the persistence layer
    pub(crate) fn semantic_log_query(&self, query: &str) -> String {
        format!("log-search:{}", query)
    }

    /// 10. Confidence scoring engine — weighted score from multiple signals
    pub(crate) fn compute_confidence(&self, state: &OpsState) -> u8 {
        let mut signals = Vec::new();
        // Evidence reliability from research profile
        signals.push(state.research_profile.evidence_reliability as f64 * 0.3);
        // Coordination link density (more links = more confidence)
        let link_score = (state.coordination_links.len() as f64 / 20.0).min(1.0) * 100.0;
        signals.push(link_score * 0.2);
        // Knowledge edge coverage
        let edge_score = (state.knowledge_edges.len() as f64 / 10.0).min(1.0) * 100.0;
        signals.push(edge_score * 0.2);
        // Infrastructure health
        let health_score = state.health as f64;
        signals.push(health_score * 0.15);
        // Contradiction penalty
        let contradiction_penalty = state.research_profile.contradiction_count as f64 * 5.0;
        let raw = signals.iter().sum::<f64>() - contradiction_penalty;
        raw.clamp(0.0, 100.0) as u8
    }

    /// Emit a metrics-sampled event with anomaly detection
    pub(crate) fn process_metrics(
        &mut self,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        if let Some(anomaly) = self.detect_anomalies(&state.metrics)
            && anomaly.zscore > ANOMALY_ZSCORE_THRESHOLD
        {
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: crate::models::ExplainabilityRecord {
                    id: next_id("exp-anomaly"),
                    action: "anomaly-detection".into(),
                    why: format!(
                        "Metric anomaly detected: value={:.1}, zscore={:.1}",
                        anomaly.value, anomaly.zscore
                    ),
                    evidence: vec![
                        format!("zscore={:.2}", anomaly.zscore),
                        format!("value={:.1}", anomaly.value),
                    ],
                    confidence: 80,
                    tools_used: vec!["observability-anomaly-detector".into()],
                    timestamp: now_ts(),
                },
            });
        }
    }

    /// Emit SLO burn report periodically
    pub(crate) fn emit_slo_report(
        &self,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        let report = self.compute_slo_burn(state);
        let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
            record: crate::models::ExplainabilityRecord {
                id: next_id("exp-slo"),
                action: "slo-burn-rate-analysis".into(),
                why: format!(
                    "Availability={:.1}% (SLO={}%), error budget={:.1}%, burn rate={:.4}/hr",
                    report.availability,
                    report.slo_target,
                    report.error_budget_remaining,
                    report.burn_rate
                ),
                evidence: vec![
                    format!("availability={:.1}", report.availability),
                    format!("error_budget={:.1}", report.error_budget_remaining),
                    format!("burn_rate={:.4}", report.burn_rate),
                ],
                confidence: 85,
                tools_used: vec!["observability-slo-engine".into()],
                timestamp: now_ts(),
            },
        });
    }

    /// Generate root-cause hypotheses for all active incidents
    pub(crate) async fn analyze_incidents(
        &mut self,
        ai_client: Option<&AiClient>,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        self.process_metrics(state, event_tx);
        self.emit_slo_report(state, event_tx);

        // Correlate evidence
        let correlations = self.correlate_evidence(state);
        for corr in correlations {
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: crate::models::ExplainabilityRecord {
                    id: next_id("exp-corr"),
                    action: "evidence-correlation".into(),
                    why: corr,
                    evidence: vec![],
                    confidence: 70,
                    tools_used: vec!["observability-correlation-engine".into()],
                    timestamp: now_ts(),
                },
            });
        }

        // Generate hypotheses for each incident
        for incident in &state.incidents {
            // Compute impact analysis
            let impact = self.analyze_impact(&incident.service, state);
            for line in impact {
                let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                    record: crate::models::ExplainabilityRecord {
                        id: next_id("exp-impact"),
                        action: format!("dependency-impact-{}", incident.id),
                        why: line,
                        evidence: vec![],
                        confidence: 75,
                        tools_used: vec!["observability-impact-analyzer".into()],
                        timestamp: now_ts(),
                    },
                });
            }

            // Generate hypothesis
            self.generate_hypothesis(ai_client, incident, state, event_tx)
                .await;

            // Summarize
            self.summarize_incident(ai_client, incident, state, event_tx)
                .await;
        }

        // Predictive alerting
        if let Some(alert) = self.predict_alert(&state.metrics) {
            let _ = event_tx.send(OpsEvent::ExplainabilityRecorded {
                record: crate::models::ExplainabilityRecord {
                    id: next_id("exp-predict"),
                    action: "predictive-alerting".into(),
                    why: format!(
                        "Metric trend forecast crosses threshold ({}): next values {:?}",
                        alert.threshold, alert.forecasts
                    ),
                    evidence: vec![
                        format!("forecasts={:?}", alert.forecasts),
                        format!("slope={:.4}", alert.slope),
                    ],
                    confidence: 70,
                    tools_used: vec!["observability-predictive-engine".into()],
                    timestamp: now_ts(),
                },
            });
        }

        // Update research profile with computed confidence
        let confidence = self.compute_confidence(state);
        let profile = ResearchConfidenceProfile {
            subject: "system-health".into(),
            evidence_reliability: confidence,
            contradiction_count: state.research_profile.contradiction_count,
            ranking: confidence,
            last_reviewed: now_ts(),
            signals: state.research_profile.signals.clone(),
        };
        let _ = event_tx.send(OpsEvent::ResearchConfidenceUpdated { profile });
    }

    /// AI log analysis on recent log entries
    pub(crate) async fn analyze_recent_logs(
        &self,
        ai_client: Option<&AiClient>,
        state: &OpsState,
        event_tx: &mpsc::UnboundedSender<OpsEvent>,
    ) {
        self.analyze_logs(ai_client, &state.logs, event_tx).await;
    }
}

pub(crate) struct PredictiveAlert {
    pub(crate) forecasts: Vec<u8>,
    pub(crate) slope: f64,
    pub(crate) threshold: f64,
    pub(crate) timestamp: String,
}

pub(crate) struct SloBurnReport {
    pub(crate) availability: f64,
    pub(crate) slo_target: f64,
    pub(crate) error_budget_remaining: f64,
    pub(crate) burn_rate: f64,
}
