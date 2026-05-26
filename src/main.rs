use std::{
    io,
    process::Stdio,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{Json, Router, extract::State, response::IntoResponse, routing::get};
use color_eyre::eyre::{Context, Result};
use crossterm::event::{self, Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::TcpListener,
    process::Command,
    sync::{mpsc, watch},
    time,
};
use tracing::{debug, info};

const NAV_ITEMS: [(&str, char); 9] = [
    ("Dashboard", '1'),
    ("Agents", '2'),
    ("Incidents", '3'),
    ("Research", '4'),
    ("Logs", '5'),
    ("Infrastructure", '6'),
    ("Workflows", '7'),
    ("Reports", '8'),
    ("Settings", '9'),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Agent {
    name: String,
    role: AgentRole,
    status: AgentStatus,
    task: String,
    confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AgentRole {
    Triage,
    Logs,
    Research,
    Workflow,
    Report,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AgentStatus {
    Idle,
    Running,
    Waiting,
    Escalated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Workflow {
    id: String,
    name: String,
    owner: String,
    stage: String,
    progress: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExecutionRecord {
    id: String,
    command: String,
    status: String,
    exit_code: Option<i32>,
    output_preview: String,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ExplainabilityRecord {
    id: String,
    action: String,
    why: String,
    evidence: Vec<String>,
    confidence: u8,
    tools_used: Vec<String>,
    timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum OpsEvent {
    IncidentDetected {
        incident_id: String,
        service: String,
        severity: String,
        timestamp: String,
    },
    AgentSpawned {
        name: String,
        role: AgentRole,
        timestamp: String,
    },
    TaskAssigned {
        agent: String,
        task: String,
        timestamp: String,
    },
    CommandRequested {
        id: String,
        command: String,
        reason: String,
        dry_run: bool,
        timestamp: String,
    },
    CommandOutput {
        id: String,
        stream: String,
        line: String,
        timestamp: String,
    },
    CommandExecuted {
        id: String,
        command: String,
        success: bool,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        timestamp: String,
    },
    ResearchCompleted {
        topic: String,
        conclusion: String,
        confidence: u8,
        timestamp: String,
    },
    WorkflowAdvanced {
        id: String,
        stage: String,
        progress: u16,
        timestamp: String,
    },
    ExplainabilityRecorded {
        record: ExplainabilityRecord,
    },
    UserCommandEntered {
        command: String,
        timestamp: String,
    },
    MetricsSampled {
        cpu: u8,
        memory: u8,
        timestamp: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Incident {
    id: String,
    service: String,
    severity: String,
    hypothesis: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InfraNode {
    name: String,
    kind: String,
    health: u8,
    cpu: u8,
    memory: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpsState {
    workspace: String,
    environment: String,
    uptime_secs: u64,
    health: u8,
    alert_count: u8,
    active_agents: usize,
    metrics: Vec<u64>,
    agents: Vec<Agent>,
    workflows: Vec<Workflow>,
    incidents: Vec<Incident>,
    infra: Vec<InfraNode>,
    executions: Vec<ExecutionRecord>,
    explainability: Vec<ExplainabilityRecord>,
    events: Vec<OpsEvent>,
    logs: Vec<String>,
    reports: Vec<String>,
}

impl OpsState {
    fn seed() -> Self {
        Self {
            workspace: "octobot-ops".into(),
            environment: "prod / us-east".into(),
            uptime_secs: 0,
            health: 94,
            alert_count: 3,
            active_agents: 4,
            metrics: vec![36, 42, 40, 45, 51, 49, 58, 62, 59, 63, 68, 64],
            agents: vec![
                Agent {
                    name: "triage-01".into(),
                    role: AgentRole::Triage,
                    status: AgentStatus::Running,
                    task: "Correlating nginx p95 latency with pod restarts".into(),
                    confidence: 82,
                },
                Agent {
                    name: "logscan-02".into(),
                    role: AgentRole::Logs,
                    status: AgentStatus::Running,
                    task: "Streaming auth-service Loki labels and error bursts".into(),
                    confidence: 76,
                },
                Agent {
                    name: "research-01".into(),
                    role: AgentRole::Research,
                    status: AgentStatus::Waiting,
                    task: "Building remediation tree from runbooks and vendor docs".into(),
                    confidence: 69,
                },
                Agent {
                    name: "reporter-01".into(),
                    role: AgentRole::Report,
                    status: AgentStatus::Idle,
                    task: "Incident 042 draft RCA queued".into(),
                    confidence: 91,
                },
            ],
            workflows: vec![
                Workflow {
                    id: "wf-2041".into(),
                    name: "nginx latency investigation".into(),
                    owner: "triage-01".into(),
                    stage: "Prometheus query fanout".into(),
                    progress: 46,
                },
                Workflow {
                    id: "wf-2042".into(),
                    name: "auth log anomaly scan".into(),
                    owner: "logscan-02".into(),
                    stage: "Loki histogram clustering".into(),
                    progress: 71,
                },
                Workflow {
                    id: "wf-2043".into(),
                    name: "incident report generation".into(),
                    owner: "reporter-01".into(),
                    stage: "Evidence graph assembly".into(),
                    progress: 28,
                },
            ],
            incidents: vec![
                Incident {
                    id: "inc-042".into(),
                    service: "edge-nginx".into(),
                    severity: "SEV2".into(),
                    hypothesis: "TLS handshakes queueing after ingress rollout".into(),
                    status: "investigating".into(),
                },
                Incident {
                    id: "inc-039".into(),
                    service: "auth-service".into(),
                    severity: "SEV3".into(),
                    hypothesis: "Token cache saturation during deploy window".into(),
                    status: "monitoring".into(),
                },
            ],
            infra: vec![
                InfraNode {
                    name: "edge-nginx-7d9c".into(),
                    kind: "deployment".into(),
                    health: 78,
                    cpu: 72,
                    memory: 64,
                },
                InfraNode {
                    name: "auth-service".into(),
                    kind: "service".into(),
                    health: 86,
                    cpu: 48,
                    memory: 81,
                },
                InfraNode {
                    name: "postgres-primary".into(),
                    kind: "database".into(),
                    health: 93,
                    cpu: 38,
                    memory: 58,
                },
                InfraNode {
                    name: "qdrant-vector".into(),
                    kind: "vector-db".into(),
                    health: 97,
                    cpu: 29,
                    memory: 44,
                },
            ],
            executions: Vec::new(),
            explainability: vec![ExplainabilityRecord {
                id: "exp-0001".into(),
                action: "Open incident inc-042 investigation".into(),
                why: "Latency alert and ingress rollout timing overlap require triage.".into(),
                evidence: vec![
                    "edge-nginx p95 crossed 820ms for 4m".into(),
                    "deploy-1188 occurred inside the alert window".into(),
                ],
                confidence: 72,
                tools_used: vec!["prometheus".into(), "loki".into()],
                timestamp: now_ts(),
            }],
            events: Vec::new(),
            logs: vec![
                "INFO workflow wf-2041 scheduled prometheus range query".into(),
                "WARN edge-nginx p95 latency crossed 820ms for 4m".into(),
                "INFO agent triage-01 linked rollout deploy-1188 to latency spike".into(),
                "INFO qdrant indexed 128 runbook fragments for retrieval".into(),
            ],
            reports: vec![
                "inc-042: evidence graph 63% complete, 7 validated claims".into(),
                "daily-sre: availability summary waiting on OpenSearch export".into(),
            ],
        }
    }

    fn tick(&mut self) {
        self.uptime_secs += 1;
        let next = (self.metrics.last().copied().unwrap_or(50) + 7 + self.uptime_secs) % 100;
        self.metrics.push(next.max(18));
        if self.metrics.len() > 30 {
            self.metrics.remove(0);
        }

        for workflow in &mut self.workflows {
            workflow.progress = ((workflow.progress + 3) % 101).max(12);
        }

        for (idx, node) in self.infra.iter_mut().enumerate() {
            node.cpu = ((node.cpu as u16 + 5 + idx as u16) % 100) as u8;
            node.memory = ((node.memory as u16 + 2 + idx as u16) % 100) as u8;
            node.health = 100u8.saturating_sub(node.cpu.saturating_sub(76));
        }

        self.health = self
            .infra
            .iter()
            .map(|node| node.health as u16)
            .sum::<u16>()
            .checked_div(self.infra.len() as u16)
            .unwrap_or(100) as u8;

        let event = match self.uptime_secs % 5 {
            0 => "INFO report engine generated explainability checkpoint",
            1 => "INFO websocket broadcast delivered orchestration snapshot",
            2 => "DEBUG reqwest provider probe completed for OpenRouter route",
            3 => "INFO postgres incident timeline transaction committed",
            _ => "WARN loki stream detected elevated 5xx sample density",
        };
        self.logs.push(event.into());
        if self.logs.len() > 80 {
            self.logs.remove(0);
        }
    }

    fn apply_event(&mut self, event: OpsEvent) {
        self.logs.push(format!("EVENT {}", event.summary()));
        match &event {
            OpsEvent::IncidentDetected {
                incident_id,
                service,
                severity,
                ..
            } => {
                self.alert_count = self.alert_count.saturating_add(1);
                if !self
                    .incidents
                    .iter()
                    .any(|incident| incident.id == *incident_id)
                {
                    self.incidents.push(Incident {
                        id: incident_id.clone(),
                        service: service.clone(),
                        severity: severity.clone(),
                        hypothesis: "Awaiting correlated evidence from workflow engine".into(),
                        status: "detected".into(),
                    });
                }
            }
            OpsEvent::AgentSpawned { name, role, .. } => {
                self.active_agents += 1;
                if !self.agents.iter().any(|agent| agent.name == *name) {
                    self.agents.push(Agent {
                        name: name.clone(),
                        role: role.clone(),
                        status: AgentStatus::Running,
                        task: "Waiting for task assignment".into(),
                        confidence: 60,
                    });
                }
            }
            OpsEvent::TaskAssigned { agent, task, .. } => {
                if let Some(existing) = self.agents.iter_mut().find(|item| item.name == *agent) {
                    existing.status = AgentStatus::Running;
                    existing.task = task.clone();
                }
            }
            OpsEvent::CommandRequested {
                id,
                command,
                dry_run,
                timestamp,
                ..
            } => {
                self.executions.push(ExecutionRecord {
                    id: id.clone(),
                    command: command.clone(),
                    status: if *dry_run {
                        "dry-run queued"
                    } else {
                        "running"
                    }
                    .into(),
                    exit_code: None,
                    output_preview: String::new(),
                    timestamp: timestamp.clone(),
                });
            }
            OpsEvent::CommandOutput {
                id, stream, line, ..
            } => {
                if let Some(existing) = self.executions.iter_mut().find(|item| item.id == *id) {
                    existing.output_preview = trim_preview(format!(
                        "{}{}{}",
                        existing.output_preview,
                        if existing.output_preview.is_empty() {
                            ""
                        } else {
                            "\n"
                        },
                        line
                    ));
                }
                self.logs
                    .push(format!("{} {} {}", stream.to_uppercase(), id, line));
            }
            OpsEvent::CommandExecuted {
                id,
                command,
                success,
                exit_code,
                stdout,
                stderr,
                timestamp,
                ..
            } => {
                if let Some(existing) = self.executions.iter_mut().find(|item| item.id == *id) {
                    existing.status = if *success { "completed" } else { "failed" }.into();
                    existing.exit_code = *exit_code;
                    existing.output_preview = trim_preview(if stdout.is_empty() {
                        stderr.clone()
                    } else {
                        stdout.clone()
                    });
                } else {
                    self.executions.push(ExecutionRecord {
                        id: id.clone(),
                        command: command.clone(),
                        status: if *success { "completed" } else { "failed" }.into(),
                        exit_code: *exit_code,
                        output_preview: trim_preview(if stdout.is_empty() {
                            stderr.clone()
                        } else {
                            stdout.clone()
                        }),
                        timestamp: timestamp.clone(),
                    });
                }
            }
            OpsEvent::ResearchCompleted {
                topic,
                conclusion,
                confidence,
                ..
            } => {
                self.reports.push(format!(
                    "{}: {} (confidence {}%)",
                    topic, conclusion, confidence
                ));
            }
            OpsEvent::WorkflowAdvanced {
                id,
                stage,
                progress,
                ..
            } => {
                if let Some(workflow) = self.workflows.iter_mut().find(|item| item.id == *id) {
                    workflow.stage = stage.clone();
                    workflow.progress = *progress;
                } else {
                    self.workflows.push(Workflow {
                        id: id.clone(),
                        name: "tier1 incident response".into(),
                        owner: "workflow-engine".into(),
                        stage: stage.clone(),
                        progress: *progress,
                    });
                }
            }
            OpsEvent::ExplainabilityRecorded { record } => {
                self.explainability.push(record.clone());
            }
            OpsEvent::UserCommandEntered { .. } => {}
            OpsEvent::MetricsSampled { cpu, memory, .. } => {
                if let Some(node) = self.infra.first_mut() {
                    node.cpu = *cpu;
                    node.memory = *memory;
                    node.health = 100u8.saturating_sub(cpu.saturating_sub(75));
                }
            }
        }

        if self.logs.len() > 120 {
            let drop_count = self.logs.len() - 120;
            self.logs.drain(0..drop_count);
        }
        self.events.push(event);
        if self.events.len() > 120 {
            let drop_count = self.events.len() - 120;
            self.events.drain(0..drop_count);
        }
        if self.executions.len() > 40 {
            let drop_count = self.executions.len() - 40;
            self.executions.drain(0..drop_count);
        }
        if self.explainability.len() > 80 {
            let drop_count = self.explainability.len() - 80;
            self.explainability.drain(0..drop_count);
        }
    }
}

impl OpsEvent {
    fn summary(&self) -> String {
        match self {
            OpsEvent::IncidentDetected {
                incident_id,
                service,
                severity,
                ..
            } => {
                format!("incident_detected id={incident_id} service={service} severity={severity}")
            }
            OpsEvent::AgentSpawned { name, role, .. } => {
                format!("agent_spawned name={name} role={role:?}")
            }
            OpsEvent::TaskAssigned { agent, task, .. } => {
                format!("task_assigned agent={agent} task={task}")
            }
            OpsEvent::CommandRequested {
                id,
                command,
                dry_run,
                ..
            } => format!("command_requested id={id} dry_run={dry_run} command={command}"),
            OpsEvent::CommandOutput {
                id, stream, line, ..
            } => format!("command_output id={id} stream={stream} line={line}"),
            OpsEvent::CommandExecuted {
                id,
                success,
                exit_code,
                ..
            } => format!("command_executed id={id} success={success} exit={exit_code:?}"),
            OpsEvent::ResearchCompleted {
                topic, confidence, ..
            } => format!("research_completed topic={topic} confidence={confidence}"),
            OpsEvent::WorkflowAdvanced {
                id,
                stage,
                progress,
                ..
            } => format!("workflow_advanced id={id} progress={progress} stage={stage}"),
            OpsEvent::ExplainabilityRecorded { record } => {
                format!(
                    "explainability_recorded id={} confidence={}",
                    record.id, record.confidence
                )
            }
            OpsEvent::UserCommandEntered { command, .. } => {
                format!("user_command command=:{command}")
            }
            OpsEvent::MetricsSampled { cpu, memory, .. } => {
                format!("metrics_sampled cpu={cpu} memory={memory}")
            }
        }
    }
}

#[derive(Debug)]
struct App {
    state: OpsState,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
    selected_nav: usize,
    command_mode: bool,
    command: String,
    activity: Vec<String>,
    exit: bool,
}

impl Default for OpsState {
    fn default() -> Self {
        Self::seed()
    }
}

impl App {
    fn new(event_tx: mpsc::UnboundedSender<OpsEvent>) -> Self {
        Self {
            state: OpsState::seed(),
            event_tx,
            selected_nav: 0,
            command_mode: false,
            command: String::new(),
            activity: vec![
                ":investigate nginx_latency".into(),
                ":spawn-agent research".into(),
            ],
            exit: false,
        }
    }

    async fn run(
        &mut self,
        terminal: &mut DefaultTerminal,
        mut state_rx: watch::Receiver<OpsState>,
    ) -> io::Result<()> {
        let mut refresh = time::interval(Duration::from_millis(120));
        while !self.exit {
            refresh.tick().await;
            while state_rx.has_changed().unwrap_or(false) {
                self.state = state_rx.borrow_and_update().clone();
            }
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&self, frame: &mut Frame) {
        let root = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(5),
        ])
        .split(frame.area());

        self.draw_top_bar(frame, root[0]);

        let body = Layout::horizontal([Constraint::Length(22), Constraint::Min(60)]).split(root[1]);
        self.draw_nav(frame, body[0]);
        self.draw_main(frame, body[1]);
        self.draw_console(frame, root[2]);
    }

    fn draw_top_bar(&self, frame: &mut Frame, area: Rect) {
        let uptime = format_duration(self.state.uptime_secs);
        let line = Line::from(vec![
            " OctoBot ".bold().fg(Color::Cyan),
            self.state.workspace.as_str().into(),
            "  env ".dark_gray(),
            self.state.environment.as_str().yellow(),
            "  agents ".dark_gray(),
            self.state.active_agents.to_string().green(),
            "  health ".dark_gray(),
            format!("{}%", self.state.health).fg(health_color(self.state.health)),
            "  alerts ".dark_gray(),
            self.state.alert_count.to_string().red(),
            "  uptime ".dark_gray(),
            uptime.into(),
        ]);
        frame.render_widget(Paragraph::new(line).block(Block::bordered()), area);
    }

    fn draw_nav(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = NAV_ITEMS
            .iter()
            .enumerate()
            .map(|(idx, (name, key))| {
                let marker = if idx == self.selected_nav { ">" } else { " " };
                let style = if idx == self.selected_nav {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(Line::from(vec![
                    Span::raw(marker),
                    Span::raw(" "),
                    Span::styled(format!("[{}] ", key), Style::default().fg(Color::DarkGray)),
                    Span::raw(*name),
                ]))
                .style(style)
            })
            .collect();

        frame.render_widget(
            List::new(items).block(Block::bordered().title(" Navigation ")),
            area,
        );
    }

    fn draw_main(&self, frame: &mut Frame, area: Rect) {
        match self.selected_nav {
            1 => self.draw_agents(frame, area),
            2 => self.draw_incidents(frame, area),
            3 => self.draw_research(frame, area),
            4 => self.draw_logs(frame, area),
            5 => self.draw_infra(frame, area),
            6 => self.draw_workflows(frame, area),
            7 => self.draw_reports(frame, area),
            8 => self.draw_settings(frame, area),
            _ => self.draw_dashboard(frame, area),
        }
    }

    fn draw_dashboard(&self, frame: &mut Frame, area: Rect) {
        let rows = Layout::vertical([Constraint::Length(8), Constraint::Min(10)]).split(area);
        let top = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[0]);

        self.draw_metric_card(
            frame,
            top[0],
            "Prometheus SLO burn",
            self.state.health,
            "multi-window burn proxy",
        );
        self.draw_metric_card(
            frame,
            top[1],
            "Agent throughput",
            72,
            "concurrent tasks/min",
        );
        self.draw_metric_card(
            frame,
            top[2],
            "Evidence coverage",
            63,
            "claims with cited telemetry",
        );

        let bottom = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);
        self.draw_workflows(frame, bottom[0]);
        self.draw_infra(frame, bottom[1]);
    }

    fn draw_metric_card(&self, frame: &mut Frame, area: Rect, title: &str, value: u8, label: &str) {
        let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(2)]).split(area);
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().title(format!(" {} ", title)))
                .gauge_style(Style::default().fg(health_color(value)).bg(Color::Black))
                .percent(value as u16),
            chunks[0],
        );
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .fg(Color::Gray),
            chunks[1],
        );
    }

    fn draw_agents(&self, frame: &mut Frame, area: Rect) {
        let rows = self.state.agents.iter().map(|agent| {
            Row::new(vec![
                agent.name.clone(),
                format!("{:?}", agent.role),
                format!("{:?}", agent.status),
                agent.confidence.to_string(),
                agent.task.clone(),
            ])
        });
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(14),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Length(6),
                    Constraint::Min(28),
                ],
            )
            .header(
                Row::new(["agent", "role", "status", "score", "current task"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Agent Orchestration ")),
            area,
        );
    }

    fn draw_incidents(&self, frame: &mut Frame, area: Rect) {
        let rows = self.state.incidents.iter().map(|incident| {
            Row::new(vec![
                incident.id.clone(),
                incident.severity.clone(),
                incident.service.clone(),
                incident.status.clone(),
                incident.hypothesis.clone(),
            ])
        });
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(9),
                    Constraint::Length(7),
                    Constraint::Length(16),
                    Constraint::Length(14),
                    Constraint::Min(28),
                ],
            )
            .header(
                Row::new(["id", "sev", "service", "status", "active hypothesis"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Incident Investigations ")),
            area,
        );
    }

    fn draw_research(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from("research-root: nginx_latency"),
            Line::from("  ├─ telemetry: prometheus p95, pod restarts, deploy markers"),
            Line::from("  ├─ retrieval: runbooks, qdrant fragments, incident memory"),
            Line::from("  ├─ external: provider docs via OpenRouter research agent"),
            Line::from("  └─ claims: each action requires cited evidence before execution"),
            Line::from(""),
            Line::from("latest explainability records:").fg(Color::Cyan),
        ];
        lines.extend(
            self.state
                .explainability
                .iter()
                .rev()
                .take(5)
                .map(|record| {
                    Line::from(format!(
                        "- {} | confidence {}% | tools {}",
                        record.action,
                        record.confidence,
                        record.tools_used.join(",")
                    ))
                }),
        );
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(" Deep Research Tree "))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_logs(&self, frame: &mut Frame, area: Rect) {
        let visible = self
            .state
            .logs
            .iter()
            .rev()
            .take(area.height.saturating_sub(2) as usize);
        let items = visible
            .map(|line| ListItem::new(line.as_str()))
            .collect::<Vec<_>>();
        frame.render_widget(
            List::new(items).block(Block::bordered().title(" Live Logs ")),
            area,
        );
    }

    fn draw_infra(&self, frame: &mut Frame, area: Rect) {
        let split =
            Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)]).split(area);
        let rows = self.state.infra.iter().map(|node| {
            Row::new(vec![
                node.name.clone(),
                node.kind.clone(),
                format!("{}%", node.health),
                format!("{}%", node.cpu),
                format!("{}%", node.memory),
            ])
            .style(Style::default().fg(health_color(node.health)))
        });
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(18),
                    Constraint::Length(12),
                    Constraint::Length(8),
                    Constraint::Length(8),
                    Constraint::Length(8),
                ],
            )
            .header(
                Row::new(["resource", "kind", "health", "cpu", "mem"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Infrastructure ")),
            split[0],
        );

        let execution_rows = self.state.executions.iter().rev().take(6).map(|record| {
            Row::new(vec![
                record.id.clone(),
                record.command.clone(),
                record.status.clone(),
                record
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or("-".into()),
            ])
        });
        frame.render_widget(
            Table::new(
                execution_rows,
                [
                    Constraint::Length(18),
                    Constraint::Min(24),
                    Constraint::Length(14),
                    Constraint::Length(6),
                ],
            )
            .header(
                Row::new(["id", "real command", "status", "exit"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Infrastructure Execution ")),
            split[1],
        );
    }

    fn draw_workflows(&self, frame: &mut Frame, area: Rect) {
        let rows = self.state.workflows.iter().map(|workflow| {
            Row::new(vec![
                workflow.id.clone(),
                workflow.name.clone(),
                workflow.owner.clone(),
                workflow.stage.clone(),
                format!("{}%", workflow.progress),
            ])
        });
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(9),
                    Constraint::Length(28),
                    Constraint::Length(12),
                    Constraint::Min(20),
                    Constraint::Length(8),
                ],
            )
            .header(
                Row::new(["id", "workflow", "owner", "stage", "done"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Workflow Monitor ")),
            area,
        );
    }

    fn draw_reports(&self, frame: &mut Frame, area: Rect) {
        let mut items = self
            .state
            .reports
            .iter()
            .map(|line| ListItem::new(line.as_str()))
            .collect::<Vec<_>>();
        items.extend(
            self.state
                .explainability
                .iter()
                .rev()
                .take(8)
                .map(|record| {
                    ListItem::new(format!(
                        "{} | why: {} | evidence: {} | confidence: {}% | tools: {}",
                        record.id,
                        record.why,
                        record.evidence.join("; "),
                        record.confidence,
                        record.tools_used.join(", ")
                    ))
                }),
        );
        frame.render_widget(
            List::new(items).block(Block::bordered().title(" Explainable Reports ")),
            area,
        );
    }

    fn draw_settings(&self, frame: &mut Frame, area: Rect) {
        let tabs = Tabs::new([
            "OpenRouter",
            "PostgreSQL",
            "Qdrant",
            "Prometheus",
            "Loki/OpenSearch",
        ])
        .block(Block::new().borders(Borders::BOTTOM))
        .highlight_style(Style::default().fg(Color::Cyan));
        let chunks = Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).split(area);
        frame.render_widget(tabs, chunks[0]);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(
                    "Provider defaults: OpenRouter LLM route with user-configurable adapters",
                ),
                Line::from(
                    "Persistence: PostgreSQL incident timelines and workflow execution state",
                ),
                Line::from("Retrieval: Qdrant operational memory and runbook embeddings"),
                Line::from("Telemetry: Prometheus metrics plus Loki/OpenSearch logs"),
                Line::from(
                    "Realtime: Tokio channels, watch snapshots, and Axum API/WebSocket boundary",
                ),
                Line::from("Tier 1: event bus, workflow engine, command sandbox, streaming execution, explainability ledger"),
            ])
            .block(Block::bordered().title(" Integration Settings "))
            .wrap(Wrap { trim: true }),
            chunks[1],
        );
    }

    fn draw_console(&self, frame: &mut Frame, area: Rect) {
        let prompt = if self.command_mode { ":" } else { ">" };
        let text = if self.command_mode {
            format!("{prompt}{}", self.command)
        } else {
            "press : | exec uptime | investigate nginx_latency | analyze-logs auth-service | j/k | 1-9 | q".into()
        };
        let recent = self
            .activity
            .iter()
            .rev()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join("  ");
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(text),
                Line::from(recent.fg(Color::DarkGray)),
            ])
            .block(Block::bordered().title(" Command Console ")),
            area,
        );
    }

    fn handle_events(&mut self) -> io::Result<()> {
        while event::poll(Duration::from_millis(0))? {
            match event::read()? {
                TerminalEvent::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.command_mode {
            match key.code {
                KeyCode::Esc => {
                    self.command_mode = false;
                    self.command.clear();
                }
                KeyCode::Enter => self.execute_command(),
                KeyCode::Backspace => {
                    self.command.pop();
                }
                KeyCode::Char(c) => self.command.push(c),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Char('q') => self.exit = true,
            KeyCode::Char(':') => self.command_mode = true,
            KeyCode::Char('j') | KeyCode::Down => self.next_nav(),
            KeyCode::Char('k') | KeyCode::Up => self.prev_nav(),
            KeyCode::Char(c @ '1'..='9') => {
                self.selected_nav = (c as usize - '1' as usize).min(NAV_ITEMS.len() - 1)
            }
            _ => {}
        }
    }

    fn next_nav(&mut self) {
        self.selected_nav = (self.selected_nav + 1) % NAV_ITEMS.len();
    }

    fn prev_nav(&mut self) {
        self.selected_nav = self
            .selected_nav
            .checked_sub(1)
            .unwrap_or(NAV_ITEMS.len() - 1);
    }

    fn execute_command(&mut self) {
        let command = self.command.trim().to_string();
        if command.is_empty() {
            self.command_mode = false;
            return;
        }

        self.activity.push(format!(":{command}"));
        let _ = self.event_tx.send(OpsEvent::UserCommandEntered {
            command: command.clone(),
            timestamp: now_ts(),
        });

        if command.starts_with("investigate") {
            self.selected_nav = 2;
            let incident_id = command
                .split_whitespace()
                .nth(1)
                .unwrap_or("manual_incident")
                .replace('-', "_");
            let _ = self.event_tx.send(OpsEvent::IncidentDetected {
                incident_id: format!("inc-{incident_id}"),
                service: "operator-request".into(),
                severity: "SEV3".into(),
                timestamp: now_ts(),
            });
        } else if command.starts_with("spawn-agent") {
            self.selected_nav = 1;
            let _ = self.event_tx.send(OpsEvent::AgentSpawned {
                name: format!("agent-{}", self.state.active_agents + 1),
                role: AgentRole::Research,
                timestamp: now_ts(),
            });
        } else if command.starts_with("analyze-logs") {
            self.selected_nav = 4;
            let request_id = next_id("cmd");
            let _ = self.event_tx.send(OpsEvent::CommandRequested {
                id: request_id,
                command: "journalctl -n 40 --no-pager".into(),
                reason: "Analyze recent system logs from operator command".into(),
                dry_run: false,
                timestamp: now_ts(),
            });
        } else if command.starts_with("generate-report") {
            self.selected_nav = 7;
            let _ = self.event_tx.send(OpsEvent::ResearchCompleted {
                topic: command.clone(),
                conclusion: "Report generated from incident timeline and explainability ledger"
                    .into(),
                confidence: 82,
                timestamp: now_ts(),
            });
        } else if let Some(raw) = command.strip_prefix("exec ") {
            self.selected_nav = 4;
            let _ = self.event_tx.send(OpsEvent::CommandRequested {
                id: next_id("cmd"),
                command: raw.trim().into(),
                reason: "Operator requested allowlisted infrastructure command".into(),
                dry_run: false,
                timestamp: now_ts(),
            });
        }

        self.command.clear();
        self.command_mode = false;
    }
}

async fn ops_runtime(
    tx: watch::Sender<OpsState>,
    mut event_rx: mpsc::UnboundedReceiver<OpsEvent>,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) {
    let started = Instant::now();
    let mut state = OpsState::seed();
    let mut interval = time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                state.uptime_secs = started.elapsed().as_secs();
                state.tick();
                let cpu = state.infra.first().map(|node| node.cpu).unwrap_or(0);
                let memory = state.infra.first().map(|node| node.memory).unwrap_or(0);
                state.apply_event(OpsEvent::MetricsSampled {
                    cpu,
                    memory,
                    timestamp: now_ts(),
                });
            }
            Some(event) = event_rx.recv() => {
                match &event {
                    OpsEvent::IncidentDetected { incident_id, .. } => {
                        tokio::spawn(run_incident_workflow(incident_id.clone(), event_tx.clone()));
                    }
                    OpsEvent::CommandRequested {
                        id,
                        command,
                        dry_run,
                        ..
                    } => {
                        let id = id.clone();
                        let command = command.clone();
                        let dry_run = *dry_run;
                        tokio::spawn(run_infrastructure_command(id, command, dry_run, event_tx.clone()));
                    }
                    _ => {}
                }
                state.apply_event(event);
            }
            else => break,
        }

        if tx.send(state.clone()).is_err() {
            break;
        }
    }
}

async fn run_incident_workflow(incident_id: String, event_tx: mpsc::UnboundedSender<OpsEvent>) {
    let workflow_id = format!("wf-{incident_id}");
    let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
        id: workflow_id.clone(),
        stage: "Detect issue".into(),
        progress: 10,
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::AgentSpawned {
        name: "planner-01".into(),
        role: AgentRole::Workflow,
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::TaskAssigned {
        agent: "planner-01".into(),
        task: format!("Coordinate investigation for {incident_id}"),
        timestamp: now_ts(),
    });

    time::sleep(Duration::from_millis(500)).await;
    let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
        id: workflow_id.clone(),
        stage: "Investigate with real read-only infrastructure commands".into(),
        progress: 30,
        timestamp: now_ts(),
    });
    for command in [
        "docker ps",
        "kubectl get pods",
        "systemctl --no-pager --failed",
        "journalctl -n 30 --no-pager",
    ] {
        let _ = event_tx.send(OpsEvent::CommandRequested {
            id: next_id("cmd"),
            command: command.into(),
            reason: format!("Collect evidence for {incident_id}"),
            dry_run: false,
            timestamp: now_ts(),
        });
    }

    time::sleep(Duration::from_millis(1000)).await;
    let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
        id: workflow_id.clone(),
        stage: "Validate evidence and confidence".into(),
        progress: 62,
        timestamp: now_ts(),
    });
    let record = ExplainabilityRecord {
        id: next_id("exp"),
        action: format!("Validate root-cause hypothesis for {incident_id}"),
        why: "The workflow collected process, container, Kubernetes, systemd, and journald evidence before recommending action.".into(),
        evidence: vec![
            "Allowlisted infrastructure commands were executed or reported unavailable.".into(),
            "Workflow state reached validation after execution events completed.".into(),
            "No destructive remediation ran without approval.".into(),
        ],
        confidence: 78,
        tools_used: vec![
            "docker".into(),
            "kubectl".into(),
            "systemctl".into(),
            "journalctl".into(),
        ],
        timestamp: now_ts(),
    };
    let _ = event_tx.send(OpsEvent::ExplainabilityRecorded { record });

    time::sleep(Duration::from_millis(500)).await;
    let _ = event_tx.send(OpsEvent::ResearchCompleted {
        topic: incident_id.clone(),
        conclusion: "Evidence bundle ready; remediation requires operator approval".into(),
        confidence: 78,
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
        id: workflow_id.clone(),
        stage: "Generate report".into(),
        progress: 82,
        timestamp: now_ts(),
    });

    time::sleep(Duration::from_millis(300)).await;
    let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
        id: workflow_id.clone(),
        stage: "Execute remediation through dry-run approval gate".into(),
        progress: 92,
        timestamp: now_ts(),
    });
    let _ = event_tx.send(OpsEvent::CommandRequested {
        id: next_id("remediate"),
        command: "systemctl restart edge-nginx".into(),
        reason: format!("Proposed remediation for {incident_id}; dry-run records approval gate"),
        dry_run: true,
        timestamp: now_ts(),
    });

    time::sleep(Duration::from_millis(300)).await;
    let _ = event_tx.send(OpsEvent::WorkflowAdvanced {
        id: workflow_id,
        stage: "Remediation proposal recorded; operator approval required for write action".into(),
        progress: 100,
        timestamp: now_ts(),
    });
}

async fn run_infrastructure_command(
    id: String,
    command: String,
    dry_run: bool,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) {
    let parsed = match parse_allowlisted_command(&command) {
        Ok(parsed) => Some(parsed),
        Err(message) => {
            if dry_run {
                let _ = event_tx.send(OpsEvent::CommandExecuted {
                    id,
                    command,
                    success: true,
                    exit_code: Some(0),
                    stdout: format!("dry-run approval gate: {message}"),
                    stderr: String::new(),
                    timestamp: now_ts(),
                });
                return;
            }
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: message,
                timestamp: now_ts(),
            });
            return;
        }
    };

    if dry_run {
        let _ = event_tx.send(OpsEvent::CommandExecuted {
            id,
            command,
            success: true,
            exit_code: Some(0),
            stdout: "dry-run: command approved but not executed".into(),
            stderr: String::new(),
            timestamp: now_ts(),
        });
        return;
    }

    let Some(parsed) = parsed else {
        return;
    };
    let mut child = match Command::new(parsed.program)
        .args(parsed.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("failed to start command: {error}"),
                timestamp: now_ts(),
            });
            return;
        }
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_task = tokio::spawn(stream_command_output(
        id.clone(),
        "stdout",
        stdout,
        event_tx.clone(),
    ));
    let stderr_task = tokio::spawn(stream_command_output(
        id.clone(),
        "stderr",
        stderr,
        event_tx.clone(),
    ));

    let status = match time::timeout(Duration::from_secs(8), child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => {
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("failed while waiting for command: {error}"),
                timestamp: now_ts(),
            });
            return;
        }
        Err(_) => {
            let _ = child.kill().await;
            let _ = event_tx.send(OpsEvent::CommandExecuted {
                id,
                command,
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: "command timed out after 8s".into(),
                timestamp: now_ts(),
            });
            return;
        }
    };

    let stdout = stdout_task.await.unwrap_or_default();
    let stderr = stderr_task.await.unwrap_or_default();
    let _ = event_tx.send(OpsEvent::CommandExecuted {
        id,
        command,
        success: status.success(),
        exit_code: status.code(),
        stdout,
        stderr,
        timestamp: now_ts(),
    });
}

async fn stream_command_output(
    id: String,
    stream: &'static str,
    output: Option<impl tokio::io::AsyncRead + Send + Unpin + 'static>,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
) -> String {
    let Some(output) = output else {
        return String::new();
    };
    let mut lines = BufReader::new(output).lines();
    let mut captured = Vec::new();
    while let Ok(Some(line)) = lines.next_line().await {
        if captured.len() < 20 {
            captured.push(line.clone());
        }
        let _ = event_tx.send(OpsEvent::CommandOutput {
            id: id.clone(),
            stream: stream.into(),
            line,
            timestamp: now_ts(),
        });
    }
    captured.join("\n")
}

struct ParsedCommand {
    program: &'static str,
    args: Vec<String>,
}

fn parse_allowlisted_command(command: &str) -> std::result::Result<ParsedCommand, String> {
    let parts = command.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["docker", "ps"] => Ok(ParsedCommand {
            program: "docker",
            args: vec!["ps".into()],
        }),
        ["kubectl", "get", "pods"] => Ok(ParsedCommand {
            program: "kubectl",
            args: vec!["get".into(), "pods".into()],
        }),
        ["journalctl", "-n", count, "--no-pager"] if count.parse::<u16>().is_ok() => {
            Ok(ParsedCommand {
                program: "journalctl",
                args: vec!["-n".into(), (*count).into(), "--no-pager".into()],
            })
        }
        ["systemctl", "--no-pager", "--failed"] => Ok(ParsedCommand {
            program: "systemctl",
            args: vec!["--no-pager".into(), "--failed".into()],
        }),
        ["ps", "aux"] => Ok(ParsedCommand {
            program: "ps",
            args: vec!["aux".into()],
        }),
        ["df", "-h"] => Ok(ParsedCommand {
            program: "df",
            args: vec!["-h".into()],
        }),
        ["uptime"] => Ok(ParsedCommand {
            program: "uptime",
            args: Vec::new(),
        }),
        ["ssh", host, "uptime"] if safe_ssh_target(host) => Ok(ParsedCommand {
            program: "ssh",
            args: vec![(*host).into(), "uptime".into()],
        }),
        _ => Err(format!(
            "blocked by sandbox allowlist: `{command}`. Allowed: docker ps, kubectl get pods, journalctl -n N --no-pager, systemctl --no-pager --failed, ps aux, df -h, uptime, ssh <host> uptime"
        )),
    }
}

fn safe_ssh_target(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 128
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@'))
}

async fn serve_api(rx: watch::Receiver<OpsState>) -> Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/state", get(snapshot))
        .with_state(rx);
    let listener = TcpListener::bind("127.0.0.1:7878")
        .await
        .context("binding local Axum API on 127.0.0.1:7878")?;
    info!("local control API listening on 127.0.0.1:7878");
    axum::serve(listener, app)
        .await
        .context("serving local Axum API")?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok", "service": "octobot-control-plane" }))
}

async fn snapshot(State(rx): State<watch::Receiver<OpsState>>) -> impl IntoResponse {
    Json(rx.borrow().clone())
}

fn now_ts() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    secs.to_string()
}

fn next_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{nanos}")
}

fn trim_preview(input: String) -> String {
    input
        .lines()
        .take(12)
        .collect::<Vec<_>>()
        .join("\n")
        .chars()
        .take(1200)
        .collect()
}

fn health_color(value: u8) -> Color {
    match value {
        90..=100 => Color::Green,
        70..=89 => Color::Yellow,
        _ => Color::Red,
    }
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_accepts_read_only_infrastructure_commands() {
        for command in [
            "docker ps",
            "kubectl get pods",
            "journalctl -n 20 --no-pager",
            "systemctl --no-pager --failed",
            "ps aux",
            "df -h",
            "uptime",
            "ssh ops@example-host uptime",
        ] {
            assert!(
                parse_allowlisted_command(command).is_ok(),
                "{command} should be allowed"
            );
        }
    }

    #[test]
    fn allowlist_blocks_write_or_shell_commands() {
        for command in [
            "kubectl delete pod auth",
            "systemctl restart nginx",
            "rm -rf /tmp/example",
            "sh -c uptime",
            "ssh host reboot",
        ] {
            assert!(
                parse_allowlisted_command(command).is_err(),
                "{command} should be blocked"
            );
        }
    }

    #[test]
    fn reducer_records_explainability_events() {
        let mut state = OpsState::seed();
        let record = ExplainabilityRecord {
            id: "exp-test".into(),
            action: "Validate incident".into(),
            why: "Evidence threshold reached".into(),
            evidence: vec!["journalctl sample".into()],
            confidence: 88,
            tools_used: vec!["journalctl".into()],
            timestamp: now_ts(),
        };

        state.apply_event(OpsEvent::ExplainabilityRecorded {
            record: record.clone(),
        });

        assert!(state.explainability.iter().any(|item| item.id == record.id));
        assert!(
            state
                .events
                .iter()
                .any(|event| matches!(event, OpsEvent::ExplainabilityRecorded { .. }))
        );
    }

    #[test]
    fn reducer_tracks_command_lifecycle() {
        let mut state = OpsState::seed();
        state.apply_event(OpsEvent::CommandRequested {
            id: "cmd-test".into(),
            command: "uptime".into(),
            reason: "unit test".into(),
            dry_run: false,
            timestamp: now_ts(),
        });
        state.apply_event(OpsEvent::CommandExecuted {
            id: "cmd-test".into(),
            command: "uptime".into(),
            success: true,
            exit_code: Some(0),
            stdout: "up 1 day".into(),
            stderr: String::new(),
            timestamp: now_ts(),
        });

        let execution = state
            .executions
            .iter()
            .find(|item| item.id == "cmd-test")
            .expect("execution should be recorded");
        assert_eq!(execution.status, "completed");
        assert_eq!(execution.exit_code, Some(0));
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt().with_writer(io::sink).init();

    let (tx, rx) = watch::channel(OpsState::seed());
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    tokio::spawn(ops_runtime(tx, event_rx, event_tx.clone()));
    tokio::spawn({
        let rx = rx.clone();
        async move {
            if let Err(error) = serve_api(rx).await {
                debug!(%error, "control API exited");
            }
        }
    });

    let mut terminal = ratatui::init();
    let result = App::new(event_tx).run(&mut terminal, rx).await;
    ratatui::restore();
    result.context("running OctoBot terminal UI")
}
