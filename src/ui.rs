use std::{io, time::Duration};

use crossterm::event::{self, Event as TerminalEvent, KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
};
use tokio::{
    sync::{mpsc, watch},
    time,
};

use crate::{
    constants::{COMMAND_SUGGESTIONS, NAV_ITEMS},
    models::{
        AgentRole, AgentRuntime, AgentRuntimeKind, AgentStatus, ExecutionRecord, KnowledgeEdge,
        OpsEvent, OpsState, PluginDescriptor, PluginKind, PluginStatus, RecoveryAction,
        RecoveryStatus, RuntimeStatus, UserRole,
    },
    reports::write_report_json,
    utils::{next_agent_name, next_id, now_ts},
};

#[derive(Debug)]
pub(crate) struct App {
    state: OpsState,
    event_tx: mpsc::UnboundedSender<OpsEvent>,
    selected_nav: usize,
    command_mode: bool,
    command: String,
    activity: Vec<String>,
    exit: bool,
    help_mode: bool,
    event_scroll: usize,
}

impl Default for OpsState {
    fn default() -> Self {
        Self::seed()
    }
}

impl App {
    pub(crate) fn new(event_tx: mpsc::UnboundedSender<OpsEvent>) -> Self {
        Self {
            state: OpsState::seed(),
            event_tx,
            selected_nav: 0,
            command_mode: false,
            command: String::new(),
            activity: vec![
                "/investigate nginx_latency".into(),
                "/spawn-agent research".into(),
            ],
            exit: false,
            help_mode: false,
            event_scroll: 0,
        }
    }

    pub(crate) async fn run(
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

        if self.help_mode {
            self.draw_help_overlay(frame, frame.area());
        }
    }

    fn draw_top_bar(&self, frame: &mut Frame, area: Rect) {
        let uptime = format_duration(self.state.uptime_secs);
        let info_line = Line::from(vec![
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
            "  role ".dark_gray(),
            format!("{:?}", self.state.current_role).magenta(),
            "  uptime ".dark_gray(),
            uptime.into(),
        ]);
        frame.render_widget(
            Paragraph::new(info_line.clone()).block(Block::bordered()),
            area,
        );
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
        let rows = Layout::vertical([
            Constraint::Length(6),
            Constraint::Min(8),
            Constraint::Length(8),
        ])
        .split(area);

        let top = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(rows[0]);

        self.draw_metric_card(frame, top[0], "Health", self.state.health);
        self.draw_metric_card(
            frame,
            top[1],
            "Agents",
            (self.state.active_agents as u8).saturating_mul(20).min(100),
        );
        self.draw_metric_card(
            frame,
            top[2],
            "Events",
            (self.state.events.len() as u8).min(100),
        );
        self.draw_metric_card(
            frame,
            top[3],
            "Pending",
            (self
                .state
                .recovery_actions
                .iter()
                .filter(|a| a.status == RecoveryStatus::AwaitingApproval)
                .count() as u8)
                .saturating_mul(33)
                .min(100),
        );

        let mid = Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(rows[1]);
        self.draw_system_metrics(frame, mid[0]);
        self.draw_event_preview(frame, mid[1]);

        let bottom = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[2]);
        self.draw_workflows(frame, bottom[0]);
        self.draw_infra_compact(frame, bottom[1]);
    }

    fn draw_system_metrics(&self, frame: &mut Frame, area: Rect) {
        if self.state.infra.is_empty() {
            frame.render_widget(
                Paragraph::new(" No infrastructure nodes")
                    .block(Block::bordered().title(" Node Metrics ")),
                area,
            );
            return;
        }
        let rows: Vec<Row> = self
            .state
            .infra
            .iter()
            .take(6)
            .map(|node| {
                let health_style = Style::default().fg(health_color(node.health));
                Row::new(vec![
                    Cell::from(octopus_health(&node.health)),
                    Cell::from(node.name.clone()),
                    Cell::from(format!("{:>3}%", node.health)).style(health_style),
                    Cell::from(format!("{:>3}%", node.cpu)),
                    Cell::from(format!("{:>3}%", node.memory)),
                ])
            })
            .collect();
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(3),
                    Constraint::Length(18),
                    Constraint::Length(6),
                    Constraint::Length(6),
                    Constraint::Length(6),
                ],
            )
            .header(
                Row::new(vec!["", "node", "health", "cpu", "mem"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Node Metrics ")),
            area,
        );
    }

    fn draw_event_preview(&self, frame: &mut Frame, area: Rect) {
        if self.state.events.is_empty() {
            frame.render_widget(
                Paragraph::new(" No events").block(Block::bordered().title(" Events ")),
                area,
            );
            return;
        }
        let rows: Vec<Row> = self
            .state
            .events
            .iter()
            .rev()
            .take(6)
            .map(|event| {
                let tag = event_type_tag(event);
                let ts = event_timestamp(event);
                let short = if ts.len() >= 8 {
                    &ts[ts.len().saturating_sub(8)..]
                } else {
                    ts
                };
                Row::new(vec![short, tag])
            })
            .collect();
        frame.render_widget(
            Table::new(rows, [Constraint::Length(10), Constraint::Min(24)])
                .header(Row::new(vec!["time", "event"]).style(Style::default().fg(Color::Cyan)))
                .block(Block::bordered().title(" Events ")),
            area,
        );
    }

    fn draw_metric_card(&self, frame: &mut Frame, area: Rect, title: &str, value: u8) {
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().title(format!(" {} ", title)))
                .gauge_style(Style::default().fg(health_color(value)).bg(Color::Black))
                .percent(value as u16)
                .label(format!("{}%", value)),
            area,
        );
    }

    fn draw_agents(&self, frame: &mut Frame, area: Rect) {
        let chunks =
            Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)]).split(area);
        let rows: Vec<Row> = self
            .state
            .agents
            .iter()
            .map(|agent| {
                Row::new(vec![
                    octopus_marker(&agent.status).into(),
                    agent.name.clone(),
                    format!("{:?}", agent.role),
                    format!("{:?}", agent.status),
                    agent.task.clone(),
                ])
            })
            .collect();
        let visible_rows: Vec<Row> = rows
            .into_iter()
            .rev()
            .take(area.height.saturating_sub(3) as usize)
            .collect();
        frame.render_widget(
            Table::new(
                visible_rows,
                [
                    Constraint::Length(5),
                    Constraint::Length(14),
                    Constraint::Length(10),
                    Constraint::Length(10),
                    Constraint::Min(28),
                ],
            )
            .header(
                Row::new(["st", "name", "role", "status", "current task"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Agent Orchestration ")),
            chunks[0],
        );

        let lower = Layout::vertical([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(chunks[1]);
        let graph_rows = self
            .state
            .coordination_links
            .iter()
            .rev()
            .take(5)
            .map(|link| {
                Row::new(vec![
                    format!("{} -> {}", link.from, link.to),
                    link.protocol.clone(),
                    format!("{}%", link.confidence),
                    link.message.clone(),
                ])
            });
        frame.render_widget(
            Table::new(
                graph_rows,
                [
                    Constraint::Length(24),
                    Constraint::Length(18),
                    Constraint::Length(8),
                    Constraint::Min(28),
                ],
            )
            .header(
                Row::new(["edge", "protocol", "score", "latest message"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Coordination Graph ")),
            lower[0],
        );

        let runtime_rows = self.state.runtimes.iter().rev().take(5).map(|runtime| {
            Row::new(vec![
                runtime.agent.clone(),
                format!("{:?}", runtime.kind),
                runtime.endpoint.clone(),
                format!("{:?}", runtime.status),
                runtime.notes.clone(),
            ])
        });
        frame.render_widget(
            Table::new(
                runtime_rows,
                [
                    Constraint::Length(14),
                    Constraint::Length(14),
                    Constraint::Min(24),
                    Constraint::Length(12),
                    Constraint::Min(20),
                ],
            )
            .header(
                Row::new(["agent", "runtime", "endpoint", "status", "notes"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Distributed Execution ")),
            lower[1],
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
        let profile = &self.state.research_profile;
        let infra_summary = self
            .state
            .infra
            .iter()
            .map(|node| format!("{} {} health={}%", node.kind, node.name, node.health))
            .collect::<Vec<_>>()
            .join(", ");
        let incident_summary = self
            .state
            .incidents
            .iter()
            .map(|inc| format!("{} ({})", inc.id, inc.status))
            .collect::<Vec<_>>()
            .join(", ");
        let workflow_summary = self
            .state
            .workflows
            .iter()
            .map(|w| format!("{} {}%", w.name, w.progress))
            .collect::<Vec<_>>()
            .join(", ");
        let mut lines = vec![
            Line::from(format!("research-root: {}", profile.subject)).fg(Color::Cyan),
            Line::from(format!(
                "confidence: {}%  reliability: {}%  contradictions: {}",
                profile.ranking, profile.evidence_reliability, profile.contradiction_count
            )),
            Line::from(format!(
                "  ├─ infrastructure: {}",
                if infra_summary.is_empty() {
                    "none"
                } else {
                    &infra_summary
                }
            )),
            Line::from(format!(
                "  ├─ incidents: {}",
                if incident_summary.is_empty() {
                    "none"
                } else {
                    &incident_summary
                }
            )),
            Line::from(format!(
                "  ├─ workflows: {}",
                if workflow_summary.is_empty() {
                    "none"
                } else {
                    &workflow_summary
                }
            )),
            Line::from(format!(
                "  └─ knowledge graph: {} nodes, {} edges",
                self.state.knowledge_nodes.len(),
                self.state.knowledge_edges.len()
            )),
            Line::from(""),
            Line::from("recent signals:").fg(Color::Cyan),
        ];
        lines.extend(profile.signals.iter().rev().take(8).map(|signal| {
            Line::from(format!(
                "- {} | {} | reliability {}%{}",
                signal.source,
                signal.evidence,
                signal.reliability,
                if signal.contradiction {
                    " | contradiction"
                } else {
                    ""
                }
            ))
        }));
        lines.push(Line::from(""));
        lines.push(Line::from("knowledge graph:").fg(Color::Cyan));
        lines.extend(self.state.knowledge_edges.iter().rev().take(6).map(|edge| {
            Line::from(format!(
                "- {} {} {} (weight {}%)",
                edge.from, edge.relation, edge.to, edge.weight
            ))
        }));
        lines.push(Line::from(""));
        lines.push(Line::from("latest explainability records:").fg(Color::Cyan));
        lines.extend(
            self.state
                .explainability
                .iter()
                .rev()
                .take(6)
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
        let items = if self.state.logs.is_empty() {
            vec![ListItem::new(
                "Streaming live logs from journalctl -f -n 20 --no-pager...",
            )]
        } else {
            self.state
                .logs
                .iter()
                .rev()
                .take(area.height.saturating_sub(2) as usize)
                .map(|line| ListItem::new(line.as_str()))
                .collect::<Vec<_>>()
        };
        frame.render_widget(
            List::new(items).block(Block::bordered().title(" Real Logs ")),
            area,
        );
    }

    fn draw_infra(&self, frame: &mut Frame, area: Rect) {
        let split = Layout::vertical([
            Constraint::Percentage(42),
            Constraint::Percentage(28),
            Constraint::Percentage(30),
        ])
        .split(area);
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

        let timeline_rows = self.state.timeline.iter().rev().take(7).map(|event| {
            Row::new(vec![
                event.timestamp.clone(),
                format!("{:?}", event.category),
                event.source.clone(),
                event
                    .cpu
                    .map(|value| format!("{value}%"))
                    .unwrap_or("-".into()),
                event
                    .memory
                    .map(|value| format!("{value}%"))
                    .unwrap_or("-".into()),
                event.summary.clone(),
            ])
        });
        frame.render_widget(
            Table::new(
                timeline_rows,
                [
                    Constraint::Length(12),
                    Constraint::Length(10),
                    Constraint::Length(18),
                    Constraint::Length(6),
                    Constraint::Length(6),
                    Constraint::Min(24),
                ],
            )
            .header(
                Row::new(["time", "kind", "source", "cpu", "mem", "correlation"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Time Travel Correlation ")),
            split[2],
        );
    }

    fn draw_infra_compact(&self, frame: &mut Frame, area: Rect) {
        if self.state.infra.is_empty() {
            frame.render_widget(
                Paragraph::new(" No infrastructure")
                    .block(Block::bordered().title(" Infrastructure ")),
                area,
            );
            return;
        }
        let rows: Vec<Row> = self
            .state
            .infra
            .iter()
            .map(|node| {
                let health_style = Style::default().fg(health_color(node.health));
                Row::new(vec![
                    Cell::from(octopus_health(&node.health)),
                    Cell::from(node.name.clone()),
                    Cell::from(format!("{:>3}%", node.health)).style(health_style),
                    Cell::from(format!("{:>3}%", node.cpu)),
                    Cell::from(format!("{:>3}%", node.memory)),
                ])
            })
            .collect();
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(3),
                    Constraint::Length(18),
                    Constraint::Length(6),
                    Constraint::Length(6),
                    Constraint::Length(6),
                ],
            )
            .header(
                Row::new(vec!["", "node", "health", "cpu", "mem"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Infrastructure ")),
            area,
        );
    }

    fn draw_workflows(&self, frame: &mut Frame, area: Rect) {
        let split =
            Layout::vertical([Constraint::Percentage(55), Constraint::Percentage(45)]).split(area);
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
            split[0],
        );

        let recovery_rows = self
            .state
            .recovery_actions
            .iter()
            .rev()
            .take(6)
            .map(|action| {
                Row::new(vec![
                    action.id.clone(),
                    action.target.clone(),
                    format!("{:?}", action.status),
                    format!("{:?}", action.requires_role),
                    action.risk.clone(),
                ])
            });
        frame.render_widget(
            Table::new(
                recovery_rows,
                [
                    Constraint::Length(16),
                    Constraint::Length(16),
                    Constraint::Length(17),
                    Constraint::Length(11),
                    Constraint::Min(24),
                ],
            )
            .header(
                Row::new(["id", "target", "status", "approval", "risk"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Autonomous Recovery Queue ")),
            split[1],
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

    fn draw_help_overlay(&self, frame: &mut Frame, area: Rect) {
        let overlay = Rect {
            x: area.x + 4,
            y: area.y + 2,
            width: area.width.saturating_sub(8).min(72),
            height: area.height.saturating_sub(4).min(30),
        };
        let lines = vec![
            Line::from(" ┌─────────────────────────────────────────────────────┐").fg(Color::Cyan),
            Line::from(" │                 Keyboard Shortcuts                  │").fg(Color::Cyan),
            Line::from(" ├─────────────────────────────────────────────────────┤").fg(Color::Cyan),
            Line::from(" │ q           Quit                                    │").into(),
            Line::from(" │ /           Enter command mode                      │").into(),
            Line::from(" │ ? or h      Toggle this help overlay                │").into(),
            Line::from(" │ Tab / j/k   Next / previous navigation tab          │").into(),
            Line::from(" │ 1-9         Jump to navigation tab                  │").into(),
            Line::from(" │ Esc         Exit command mode / close help          │").into(),
            Line::from(" │ Enter       Execute typed command                   │").into(),
            Line::from(" ├─────────────────────────────────────────────────────┤").fg(Color::Cyan),
            Line::from(" │                    Commands                         │").fg(Color::Cyan),
            Line::from(" ├─────────────────────────────────────────────────────┤").fg(Color::Cyan),
            Line::from(" │ /multi-agent <task>     Multi-agent investigation    │").into(),
            Line::from(" │ /investigate <service>  Create incident             │").into(),
            Line::from(" │ /spawn-agent <role>     Launch AI agent             │").into(),
            Line::from(" │ /exec <command>         Run infra command           │").into(),
            Line::from(" │ /recover <target>       Propose recovery action     │").into(),
            Line::from(" │ /approve <action_id>    Approve recovery            │").into(),
            Line::from(" │ /replay start|step      Replay event timeline       │").into(),
            Line::from(" │ /role <admin|operator>  Change user role            │").into(),
            Line::from(" │ /plugin add|enable|disable <name>  Plugin mgmt      │").into(),
            Line::from(" │ /research confidence    View research profile       │").into(),
            Line::from(" │ /login ollama <url>     Point to local Ollama       │").into(),
            Line::from(" │ /tasks-report           Report last 50 tasks        │").into(),
            Line::from(" └─────────────────────────────────────────────────────┘").fg(Color::Cyan),
        ];
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().style(Style::default().bg(Color::Black)))
                .alignment(Alignment::Center),
            overlay,
        );
    }

    fn draw_settings(&self, frame: &mut Frame, area: Rect) {
        let tabs = Tabs::new(["Security", "Ollama", "Plugins", "Sandbox", "Replay"])
            .block(Block::new().borders(Borders::BOTTOM))
            .highlight_style(Style::default().fg(Color::Cyan));
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Percentage(24),
            Constraint::Percentage(38),
            Constraint::Percentage(38),
        ])
        .split(area);
        frame.render_widget(tabs, chunks[0]);

        self.draw_security_dashboard(frame, chunks[1]);

        let middle = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(chunks[2]);
        self.draw_security_audit_panel(frame, middle[0]);
        self.draw_plugin_security_panel(frame, middle[1]);
        self.draw_runtime_protection_panel(frame, middle[2]);

        let bottom = Layout::horizontal([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(chunks[3]);
        self.draw_attack_timeline(frame, bottom[0]);
        self.draw_vulnerability_explorer(frame, bottom[1]);
        self.draw_security_replay_panel(frame, bottom[2]);
    }

    fn draw_security_dashboard(&self, frame: &mut Frame, area: Rect) {
        let summary = SecurityUiSummary::from_state(&self.state);
        let cards = Layout::horizontal([
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(17),
            Constraint::Percentage(16),
            Constraint::Percentage(16),
        ])
        .split(area);

        self.draw_count_card(
            frame,
            cards[0],
            "Threats",
            summary.active_threats,
            threat_color(summary.active_threats),
        );
        self.draw_count_card(
            frame,
            cards[1],
            "Suspicious",
            summary.suspicious_activity,
            threat_color(summary.suspicious_activity),
        );
        self.draw_count_card(
            frame,
            cards[2],
            "Blocked",
            summary.blocked_attacks,
            if summary.blocked_attacks == 0 {
                Color::DarkGray
            } else {
                Color::Yellow
            },
        );
        self.draw_count_card(
            frame,
            cards[3],
            "Violations",
            summary.permission_violations,
            threat_color(summary.permission_violations),
        );
        self.draw_count_card(
            frame,
            cards[4],
            "Vulns",
            summary.vulnerability_alerts,
            threat_color(summary.vulnerability_alerts),
        );
        frame.render_widget(
            Gauge::default()
                .block(Block::bordered().title(" Integrity "))
                .gauge_style(
                    Style::default()
                        .fg(health_color(summary.runtime_integrity))
                        .bg(Color::Black),
                )
                .percent(summary.runtime_integrity as u16)
                .label(format!("{}%", summary.runtime_integrity)),
            cards[5],
        );
    }

    fn draw_count_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        title: &str,
        value: usize,
        color: Color,
    ) {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(format!("{value}")).alignment(Alignment::Center),
                Line::from(title).alignment(Alignment::Center),
            ])
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .block(Block::bordered()),
            area,
        );
    }

    fn draw_security_audit_panel(&self, frame: &mut Frame, area: Rect) {
        let rows = self.state.executions.iter().rev().take(7).map(|record| {
            let status_style = if is_blocked_execution(record) {
                Style::default().fg(Color::Yellow)
            } else if record.status == "failed" {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };
            Row::new(vec![
                record.id.clone(),
                record.status.clone(),
                record.command.clone(),
                record.output_preview.clone(),
            ])
            .style(status_style)
        });
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(13),
                    Constraint::Length(10),
                    Constraint::Min(18),
                    Constraint::Min(18),
                ],
            )
            .header(
                Row::new(["id", "status", "command audit", "policy output"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Command Security Audit ")),
            area,
        );
    }

    fn draw_plugin_security_panel(&self, frame: &mut Frame, area: Rect) {
        let plugin_rows = self.state.plugins.iter().map(|plugin| {
            let risk = plugin_security_risk(plugin);
            Row::new(vec![
                plugin.name.clone(),
                format!("{:?}", plugin.kind),
                format!("{:?}", plugin.status),
                risk,
            ])
        });
        frame.render_widget(
            Table::new(
                plugin_rows,
                [
                    Constraint::Length(18),
                    Constraint::Length(12),
                    Constraint::Length(12),
                    Constraint::Min(12),
                ],
            )
            .header(
                Row::new(["plugin", "kind", "status", "security"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Plugin Security Monitor ")),
            area,
        );
    }

    fn draw_runtime_protection_panel(&self, frame: &mut Frame, area: Rect) {
        let summary = SecurityUiSummary::from_state(&self.state);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(format!(
                    "resource protection: memory pressure {}%",
                    estimated_memory_pressure(&self.state)
                )),
                Line::from(format!("runtime integrity: {}%", summary.runtime_integrity)),
                Line::from(format!(
                    "Ollama endpoint: {}",
                    std::env::var("OCTOBOT_OLLAMA_URL")
                        .unwrap_or_else(|_| "http://localhost:11434".into())
                )),
                Line::from(format!(
                    "Installed models: {}",
                    self.state.model_health.len()
                )),
                Line::from(format!(
                    "Token usage: {} requests, {} prompt / {} completion / {} total",
                    self.state.token_usage.requests,
                    self.state.token_usage.prompt_tokens,
                    self.state.token_usage.completion_tokens,
                    self.state.token_usage.total_tokens
                )),
                Line::from(format!(
                    "Reasoning stream entries: {}",
                    self.state.reasoning_stream.len()
                )),
                Line::from(format!("Notifications: {}", self.state.notifications.len())),
                Line::from(format!("sandbox: {}", self.state.sandbox_policy.mode)),
                Line::from(format!(
                    "approval roles: {:?}",
                    self.state.sandbox_policy.approved_roles
                )),
                Line::from(format!(
                    "review targets: {}",
                    self.state.sandbox_policy.review_required_for.join(", ")
                )),
                Line::from(format!(
                    "blocked attacks: {} | violations: {}",
                    summary.blocked_attacks, summary.permission_violations
                )),
                Line::from(format!("current role: {:?}", self.state.current_role)),
            ])
            .block(Block::bordered().title(" Resource & Sandbox Protection "))
            .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_attack_timeline(&self, frame: &mut Frame, area: Rect) {
        let rows = self
            .state
            .explainability
            .iter()
            .rev()
            .filter(|record| is_security_record(&record.action) || is_security_record(&record.why))
            .take(7)
            .map(|record| {
                Row::new(vec![
                    short_time(&record.timestamp),
                    record.action.clone(),
                    format!("{}%", record.confidence),
                    record.evidence.join("; "),
                ])
            });
        frame.render_widget(
            Table::new(
                rows,
                [
                    Constraint::Length(9),
                    Constraint::Min(18),
                    Constraint::Length(7),
                    Constraint::Min(18),
                ],
            )
            .header(
                Row::new(["time", "threat event", "score", "evidence"])
                    .style(Style::default().fg(Color::Cyan)),
            )
            .block(Block::bordered().title(" Threat Timeline ")),
            area,
        );
    }

    fn draw_vulnerability_explorer(&self, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::from(format!(
                "alerts: {}",
                SecurityUiSummary::from_state(&self.state).vulnerability_alerts
            ))
            .fg(Color::Cyan),
            Line::from(format!(
                "workflow risk: {} pending approvals",
                self.state
                    .recovery_actions
                    .iter()
                    .filter(|action| action.status == RecoveryStatus::AwaitingApproval)
                    .count()
            )),
            Line::from(""),
        ];
        lines.extend(
            self.state
                .explainability
                .iter()
                .rev()
                .filter(|record| {
                    is_security_record(&record.action) || is_security_record(&record.why)
                })
                .take(5)
                .map(|record| {
                    Line::from(format!(
                        "- {} | {}% | {}",
                        record.action,
                        record.confidence,
                        record.evidence.join("; ")
                    ))
                }),
        );
        lines.push(Line::from(""));
        lines.extend(
            self.state
                .recovery_actions
                .iter()
                .rev()
                .take(4)
                .map(|action| {
                    Line::from(format!(
                        "- workflow risk {}: {:?} {}",
                        action.id, action.status, action.risk
                    ))
                }),
        );
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(" Vulnerability & Workflow Risk "))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_security_replay_panel(&self, frame: &mut Frame, area: Rect) {
        let replay_status = if self.state.replay.active {
            format!(
                "replay {}/{}",
                self.state.replay.position, self.state.replay.total
            )
        } else {
            "replay idle".into()
        };
        let mut lines = vec![
            Line::from(replay_status).fg(Color::Cyan),
            Line::from(format!(
                "last: {}",
                self.state
                    .replay
                    .last_event
                    .clone()
                    .unwrap_or_else(|| "-".into())
            )),
            Line::from(""),
            Line::from("security event replay:").fg(Color::Cyan),
        ];
        lines.extend(
            self.state
                .events
                .iter()
                .rev()
                .filter(|event| is_security_event(event))
                .take(6)
                .map(|event| {
                    Line::from(format!(
                        "- {} {}",
                        short_time(event_timestamp(event)),
                        event_type_tag(event)
                    ))
                }),
        );
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "reasoning trace entries: {}",
            self.state.reasoning_stream.len()
        )));
        lines.extend(
            self.state
                .reasoning_stream
                .iter()
                .rev()
                .take(3)
                .map(|entry| Line::from(format!("- {entry}"))),
        );
        frame.render_widget(
            Paragraph::new(lines)
                .block(Block::bordered().title(" Security Replay & AI Trace "))
                .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_legacy_settings_summary(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(format!(
                    "Sandbox policy: {} | roles {:?} | review targets {}",
                    self.state.sandbox_policy.mode,
                    self.state.sandbox_policy.approved_roles,
                    self.state.sandbox_policy.review_required_for.join(", ")
                )),
                Line::from(format!(
                    "Research confidence: {}% reliability {}% contradictions {}",
                    self.state.research_profile.ranking,
                    self.state.research_profile.evidence_reliability,
                    self.state.research_profile.contradiction_count
                )),
                Line::from(format!(
                    "Knowledge graph: {} nodes / {} edges",
                    self.state.knowledge_nodes.len(),
                    self.state.knowledge_edges.len()
                )),
                Line::from(format!(
                    "Replay cursor: {}/{}",
                    self.state.replay.position, self.state.replay.total
                )),
                Line::from(format!("Current role: {:?}", self.state.current_role)),
            ])
            .block(Block::bordered().title(" Integration Settings "))
            .wrap(Wrap { trim: true }),
            area,
        );
    }

    fn draw_console(&self, frame: &mut Frame, area: Rect) {
        let prompt = if self.command_mode { "/" } else { ">" };
        let text = if self.command_mode {
            format!("{prompt}{}", self.command)
        } else {
            "press / | h/? help | Tab switch views | exec uptime | j/k | 1-9 | q".into()
        };
        let hint = if self.command_mode {
            command_completion(&self.command)
                .map(|completion| format!("Tab complete: /{completion}"))
                .unwrap_or_else(|| {
                    "commands: multi-agent | investigate | spawn-agent | tasks-report | analyze-logs | recover | approve | role | replay | exec | research confidence | plugin add|enable|disable | runtime set | graph link | sandbox policy".into()
                })
        } else {
            self.activity
                .iter()
                .rev()
                .take(2)
                .cloned()
                .collect::<Vec<_>>()
                .join("  ")
        };
        frame.render_widget(
            Paragraph::new(vec![Line::from(text), Line::from(hint.fg(Color::DarkGray))])
                .block(Block::bordered().title(" Command Console ")),
            area,
        );
        if self.command_mode {
            let cursor_x = area
                .x
                .saturating_add(2)
                .saturating_add(self.command.chars().count() as u16)
                .min(area.right().saturating_sub(2));
            frame.set_cursor_position(Position::new(cursor_x, area.y.saturating_add(1)));
        }
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
                    if self.help_mode {
                        self.help_mode = false;
                    } else {
                        self.command_mode = false;
                        self.command.clear();
                    }
                }
                KeyCode::Enter => self.execute_command(),
                KeyCode::Tab => self.autocomplete_command(),
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
            KeyCode::Char('/') => self.command_mode = true,
            KeyCode::Char('?') => self.help_mode = !self.help_mode,
            KeyCode::Char('h') => self.help_mode = !self.help_mode,
            KeyCode::Tab => self.next_nav(),
            KeyCode::BackTab => self.prev_nav(),
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

    fn autocomplete_command(&mut self) {
        if let Some(completion) = command_completion(&self.command) {
            self.command = completion.into();
        }
    }

    fn execute_command(&mut self) {
        let command = self.command.trim().to_string();
        if command.is_empty() {
            self.command_mode = false;
            return;
        }

        self.activity.push(format!("/{command}"));
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
        } else if command.starts_with("multi-agent") {
            self.selected_nav = 1;
            let task = command
                .strip_prefix("multi-agent ")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Investigate current system state and report findings".into());
            let planner_id = next_agent_name();
            let _ = self.event_tx.send(OpsEvent::AgentSpawned {
                name: planner_id.clone(),
                role: AgentRole::Planner,
                timestamp: now_ts(),
            });
            let _ = self.event_tx.send(OpsEvent::TaskAssigned {
                agent: planner_id.clone(),
                task: task.clone(),
                timestamp: now_ts(),
            });
            self.activity.push(
                format!(
                    "Multi-agent task '{}' delegated to planner {planner_id}",
                    &task[..task.len().min(60)]
                )
                .into(),
            );
        } else if command.starts_with("spawn-agent") {
            self.selected_nav = 1;
            let role = command
                .strip_prefix("spawn-agent ")
                .and_then(|r| match r.trim() {
                    "planner" => Some(AgentRole::Planner),
                    "executor" => Some(AgentRole::Executor),
                    _ => Some(AgentRole::Research),
                })
                .unwrap_or(AgentRole::Research);
            let agent_id = next_agent_name();
            let _ = self.event_tx.send(OpsEvent::AgentSpawned {
                name: agent_id.clone(),
                role: role.clone(),
                timestamp: now_ts(),
            });
            let _ = self.event_tx.send(OpsEvent::TaskAssigned {
                agent: agent_id.clone(),
                task: format!("{:?} agent {agent_id} initialized — analyze current system state and report findings", role),
                timestamp: now_ts(),
            });
        } else if let Some(raw) = command.strip_prefix("assign ") {
            let parts: Vec<&str> = raw.splitn(2, |c: char| c.is_whitespace()).collect();
            match parts.as_slice() {
                [agent_id, task] => {
                    self.selected_nav = 1;
                    let _ = self.event_tx.send(OpsEvent::TaskAssigned {
                        agent: agent_id.to_string(),
                        task: task.to_string(),
                        timestamp: now_ts(),
                    });
                }
                _ => {
                    self.activity
                        .push("Usage: /assign <agent_id> <task description>".into());
                }
            }
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
            let timestamp = now_ts();
            let conclusion = "Report generated from incident timeline and explainability ledger";
            let report_result =
                write_report_json(&self.state, &command, conclusion, 82, &timestamp);
            let conclusion = match report_result {
                Ok(path) => format!("{conclusion}. Stored JSON report at {path}"),
                Err(error) => format!("{conclusion}. JSON export failed: {error}"),
            };
            let _ = self.event_tx.send(OpsEvent::ResearchCompleted {
                topic: command.clone(),
                conclusion,
                confidence: 82,
                timestamp,
            });
        } else if command == "tasks-report" {
            self.selected_nav = 7;
            let timestamp = now_ts();
            let mut lines: Vec<String> = Vec::new();
            lines.push("=== Last 50 Task Events ===".into());
            lines.push(String::new());
            let mut task_events: Vec<String> = Vec::new();
            for event in self.state.events.iter().rev() {
                match event {
                    OpsEvent::TaskAssigned {
                        agent,
                        task,
                        timestamp,
                    } => {
                        task_events.push(format!(
                            "  [{ts}] ASSIGN  {ag:<12} → {t}",
                            ts = &timestamp[timestamp.len().saturating_sub(8)..],
                            ag = agent,
                            t = task
                        ));
                    }
                    OpsEvent::AgentLifecycleChanged {
                        agent,
                        status,
                        task,
                        timestamp,
                    } => {
                        task_events.push(format!(
                            "  [{ts}] STATUS  {ag:<12} {st:10?} {t}",
                            ts = &timestamp[timestamp.len().saturating_sub(8)..],
                            ag = agent,
                            st = status,
                            t = task
                        ));
                    }
                    OpsEvent::PlanCreated {
                        planner,
                        task,
                        sub_tasks,
                        timestamp,
                    } => {
                        task_events.push(format!(
                            "  [{ts}] PLAN    {pl:<12} → {t} ({n} sub-tasks)",
                            ts = &timestamp[timestamp.len().saturating_sub(8)..],
                            pl = planner,
                            t = task,
                            n = sub_tasks.len()
                        ));
                    }
                    OpsEvent::SubTaskCompleted {
                        planner,
                        executor,
                        sub_task,
                        ..
                    } => {
                        task_events.push(format!(
                            "  [       ] COMPLETE {ex:<12} → {pl} done: {t}",
                            ex = executor,
                            pl = planner,
                            t = sub_task
                        ));
                    }
                    OpsEvent::AgentSpawned {
                        name,
                        role,
                        timestamp,
                    } => {
                        task_events.push(format!(
                            "  [{ts}] SPAWN   {nm:<12} role={rl:?}",
                            ts = &timestamp[timestamp.len().saturating_sub(8)..],
                            nm = name,
                            rl = role
                        ));
                    }
                    OpsEvent::AgentMemoryStored {
                        agent, key, value, ..
                    } => {
                        task_events.push(format!(
                            "  [       ] MEM     {ag:<12} [{k}] {v}",
                            ag = agent,
                            k = key,
                            v = value
                        ));
                    }
                    _ => continue,
                }
                if task_events.len() >= 50 {
                    break;
                }
            }
            if task_events.is_empty() {
                lines.push("  No task events recorded yet.".into());
            } else {
                lines.extend(task_events.into_iter().rev());
            }
            let conclusion = lines.join("\n");
            let _ = self.event_tx.send(OpsEvent::ResearchCompleted {
                topic: "tasks-report".into(),
                conclusion,
                confidence: 95,
                timestamp,
            });
        } else if command == "research confidence" {
            self.selected_nav = 3;
            let profile = self.state.research_profile.clone();
            let conclusion = format!(
                "confidence profile refreshed for {} with {} signals",
                profile.subject,
                profile.signals.len()
            );
            let _ = self.event_tx.send(OpsEvent::ResearchCompleted {
                topic: profile.subject,
                conclusion,
                confidence: profile.ranking.max(profile.evidence_reliability),
                timestamp: now_ts(),
            });
        } else if let Some(raw) = command.strip_prefix("plugin add ") {
            self.selected_nav = 8;
            let mut parts = raw.split_whitespace();
            let name = parts.next().unwrap_or("custom-plugin");
            let kind = parse_plugin_kind(parts.next().unwrap_or("tool"));
            let plugin = PluginDescriptor {
                name: name.into(),
                kind,
                description: "user-registered local plugin".into(),
                version: "0.1.0".into(),
                status: PluginStatus::Registered,
                owner: format!("{:?}", self.state.current_role),
            };
            let _ = self.event_tx.send(OpsEvent::PluginRegistered { plugin });
        } else if let Some(name) = command.strip_prefix("plugin enable ") {
            self.selected_nav = 8;
            let _ = self.event_tx.send(OpsEvent::PluginStatusChanged {
                name: name.trim().into(),
                status: PluginStatus::Enabled,
                timestamp: now_ts(),
            });
        } else if let Some(name) = command.strip_prefix("plugin disable ") {
            self.selected_nav = 8;
            let _ = self.event_tx.send(OpsEvent::PluginStatusChanged {
                name: name.trim().into(),
                status: PluginStatus::Disabled,
                timestamp: now_ts(),
            });
        } else if let Some(raw) = command.strip_prefix("runtime set ") {
            self.selected_nav = 1;
            let mut parts = raw.split_whitespace();
            let agent = parts.next().unwrap_or("agent-local");
            let kind = parse_runtime_kind(parts.next().unwrap_or("local"));
            let endpoint = parts.collect::<Vec<_>>().join(" ");
            let endpoint = if endpoint.is_empty() {
                format!("local://process/{agent}")
            } else {
                endpoint
            };
            let runtime = AgentRuntime {
                agent: agent.into(),
                kind,
                endpoint: endpoint.clone(),
                status: runtime_status_for(&endpoint),
                heartbeat: now_ts(),
                notes: "runtime updated from operator command".into(),
            };
            let _ = self.event_tx.send(OpsEvent::RuntimeUpdated { runtime });
        } else if let Some(raw) = command.strip_prefix("graph link ") {
            self.selected_nav = 6;
            let mut parts = raw.split_whitespace();
            let from = parts.next().unwrap_or("deploy-1188");
            let relation = parts.next().unwrap_or("correlates-with");
            let to = parts.next().unwrap_or("inc-042");
            let _ = self.event_tx.send(OpsEvent::KnowledgeEdgeAdded {
                edge: KnowledgeEdge {
                    from: from.into(),
                    relation: relation.into(),
                    to: to.into(),
                    weight: 88,
                    timestamp: now_ts(),
                },
            });
        } else if let Some(raw) = command.strip_prefix("sandbox policy ") {
            self.selected_nav = 8;
            let mut parts = raw.split_whitespace();
            let role = parse_role(parts.next().unwrap_or("operator"));
            let keyword = parts.next().unwrap_or("restart").to_string();
            let mut policy = self.state.sandbox_policy.clone();
            if !policy.approved_roles.contains(&role) {
                policy.approved_roles.push(role);
            }
            if !policy.review_required_for.contains(&keyword) {
                policy.review_required_for.push(keyword);
            }
            policy.persisted = true;
            policy.updated_at = now_ts();
            policy.mode = "role-aware sandbox approval".into();
            let _ = self
                .event_tx
                .send(OpsEvent::SandboxPolicyUpdated { policy });
        } else if let Some(raw) = command.strip_prefix("login ") {
            self.selected_nav = 8;
            let parts: Vec<&str> = raw.splitn(2, |c: char| c.is_whitespace()).collect();
            let (kind, key_or_url) = match parts.as_slice() {
                [kind, val] => (*kind, *val),
                _ => {
                    self.activity.push("Usage: /login ollama <url>".into());
                    self.command.clear();
                    self.command_mode = false;
                    return;
                }
            };
            if kind != "ollama" {
                self.activity
                    .push("Only Ollama is supported. Use /login ollama <url>.".into());
            } else {
                let endpoint = key_or_url.trim_end_matches('/').to_string();
                let _ = self.event_tx.send(OpsEvent::AiProviderLogin {
                    kind: kind.into(),
                    endpoint: endpoint.clone(),
                    model: std::env::var("OCTOBOT_OLLAMA_MODEL")
                        .unwrap_or_else(|_| "llama3.1:8b".into()),
                    api_key: None,
                    timestamp: now_ts(),
                });
                self.activity.push(format!("/login {} (configured)", kind));
            }
        } else if let Some(raw) = command.strip_prefix("exec ") {
            self.selected_nav = 4;
            let _ = self.event_tx.send(OpsEvent::CommandRequested {
                id: next_id("cmd"),
                command: raw.trim().into(),
                reason: "Operator requested allowlisted infrastructure command".into(),
                dry_run: false,
                timestamp: now_ts(),
            });
        } else if command.starts_with("recover") {
            self.selected_nav = 6;
            let target = command.split_whitespace().nth(1).unwrap_or("edge-nginx");
            let action = RecoveryAction {
                id: next_id("rec"),
                name: format!("Restart {target}"),
                command: format!("systemctl restart {target}"),
                target: target.into(),
                status: RecoveryStatus::AwaitingApproval,
                risk: "write action remains dry-run only until external policy is configured"
                    .into(),
                requires_role: UserRole::Operator,
                evidence: vec![
                    "operator requested recovery workflow".into(),
                    "RBAC approval is required before execution".into(),
                ],
                requested_by: "operator".into(),
                approved_by: None,
                dry_run_only: true,
                timestamp: now_ts(),
            };
            let _ = self.event_tx.send(OpsEvent::RecoveryProposed { action });
        } else if let Some(raw) = command.strip_prefix("approve ") {
            self.selected_nav = 6;
            let action_id = raw.trim().to_string();
            let role = self.state.current_role.clone();
            let _ = self.event_tx.send(OpsEvent::RecoveryApproved {
                action_id: action_id.clone(),
                role: role.clone(),
                timestamp: now_ts(),
            });
            if role.can_approve_recovery() {
                let dry_run_command = self
                    .state
                    .recovery_actions
                    .iter()
                    .find(|action| action.id == action_id)
                    .map(|action| action.command.clone())
                    .unwrap_or_else(|| "systemctl restart edge-nginx".into());
                let _ = self.event_tx.send(OpsEvent::CommandRequested {
                    id: next_id("dryrun"),
                    command: dry_run_command,
                    reason: format!("RBAC-approved recovery dry-run by {:?}", role),
                    dry_run: true,
                    timestamp: now_ts(),
                });
            }
        } else if let Some(raw) = command.strip_prefix("role ") {
            let _ = self.event_tx.send(OpsEvent::RoleChanged {
                role: parse_role(raw.trim()),
                timestamp: now_ts(),
            });
        } else if command == "replay start" {
            self.selected_nav = 5;
            let _ = self.event_tx.send(OpsEvent::ReplayStarted {
                timestamp: now_ts(),
            });
        } else if command == "replay step" {
            self.selected_nav = 5;
            let _ = self.event_tx.send(OpsEvent::ReplayStepped {
                position: self.state.replay.position.saturating_add(1),
                timestamp: now_ts(),
            });
        }

        self.command.clear();
        self.command_mode = false;
    }
}
pub(crate) fn command_completion(input: &str) -> Option<&'static str> {
    let normalized = input.trim_start();
    if normalized.is_empty() {
        return COMMAND_SUGGESTIONS.first().copied();
    }
    COMMAND_SUGGESTIONS
        .iter()
        .copied()
        .find(|command| command.starts_with(normalized))
}

fn octopus_health(value: &u8) -> &'static str {
    match value {
        90..=100 => "●",
        70..=89 => "◉",
        50..=69 => "○",
        _ => "◎",
    }
}

fn event_timestamp(event: &OpsEvent) -> &str {
    match event {
        OpsEvent::IncidentDetected { timestamp, .. } => timestamp.as_str(),
        OpsEvent::CommandRequested { timestamp, .. } => timestamp.as_str(),
        OpsEvent::CommandExecuted { timestamp, .. } => timestamp.as_str(),
        OpsEvent::AgentSpawned { timestamp, .. } => timestamp.as_str(),
        OpsEvent::ResearchCompleted { timestamp, .. } => timestamp.as_str(),
        OpsEvent::ExplainabilityRecorded { record } => &record.timestamp,
        OpsEvent::RecoveryProposed { action } => &action.timestamp,
        OpsEvent::RecoveryApproved { timestamp, .. } => timestamp.as_str(),
        _ => "",
    }
}

fn event_type_tag(event: &OpsEvent) -> &'static str {
    match event {
        OpsEvent::IncidentDetected { .. } => "Incident",
        OpsEvent::CommandRequested { .. } => "CommandReq",
        OpsEvent::CommandExecuted { success: true, .. } => "CmdOK",
        OpsEvent::CommandExecuted { success: false, .. } => "CmdFail",
        OpsEvent::AgentSpawned { .. } => "AgentSpawn",
        OpsEvent::ResearchCompleted { .. } => "Research",
        OpsEvent::RecoveryProposed { .. } => "RecoveryProp",
        OpsEvent::RecoveryApproved { .. } => "RecoveryAppr",
        OpsEvent::ExplainabilityRecorded { .. } => "Explain",
        OpsEvent::WorkflowAdvanced { .. } => "Workflow",
        OpsEvent::AgentMemoryStored { .. } => "MemStore",
        OpsEvent::PlanCreated { .. } => "Plan",
        OpsEvent::SubTaskCompleted { .. } => "SubTaskOK",
        OpsEvent::ToolCallCompleted { .. } => "ToolCall",
        OpsEvent::InfrastructureSnapshotRecorded { .. } => "InfraSnap",
        OpsEvent::ReplayStarted { .. } => "ReplayStart",
        OpsEvent::ReplayStepped { .. } => "ReplayStep",
        _ => "Event",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SecurityUiSummary {
    pub(crate) active_threats: usize,
    pub(crate) suspicious_activity: usize,
    pub(crate) blocked_attacks: usize,
    pub(crate) permission_violations: usize,
    pub(crate) vulnerability_alerts: usize,
    pub(crate) runtime_integrity: u8,
}

impl SecurityUiSummary {
    pub(crate) fn from_state(state: &OpsState) -> Self {
        let active_threats = state
            .explainability
            .iter()
            .filter(|record| {
                record
                    .action
                    .to_ascii_lowercase()
                    .contains("threat detected")
            })
            .count();
        let blocked_attacks = state
            .executions
            .iter()
            .filter(|record| is_blocked_execution(record))
            .count()
            + state
                .explainability
                .iter()
                .filter(|record| {
                    record
                        .action
                        .to_ascii_lowercase()
                        .contains("blocked prompt")
                })
                .count();
        let permission_violations = state
            .executions
            .iter()
            .filter(|record| is_permission_violation(record))
            .count();
        let suspicious_activity = state
            .executions
            .iter()
            .filter(|record| record.status == "failed")
            .count()
            + state
                .coordination_links
                .iter()
                .filter(|link| is_security_record(&link.message))
                .count();
        let vulnerability_alerts = state
            .explainability
            .iter()
            .filter(|record| {
                let action = record.action.to_ascii_lowercase();
                let why = record.why.to_ascii_lowercase();
                action.contains("security audit")
                    || why.contains("findings")
                    || action.contains("vulnerab")
            })
            .count();
        let mut integrity = 100u8;
        integrity = integrity.saturating_sub((permission_violations as u8).saturating_mul(12));
        integrity = integrity.saturating_sub((active_threats as u8).saturating_mul(10));
        integrity = integrity.saturating_sub((vulnerability_alerts as u8).saturating_mul(4));
        if state.sandbox_policy.approved_roles.is_empty() {
            integrity = integrity.saturating_sub(8);
        }
        if state.current_role == UserRole::Admin {
            integrity = integrity.saturating_sub(4);
        }
        Self {
            active_threats,
            suspicious_activity,
            blocked_attacks,
            permission_violations,
            vulnerability_alerts,
            runtime_integrity: integrity.max(20),
        }
    }
}

fn is_security_record(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "security",
        "threat",
        "blocked",
        "sandbox",
        "injection",
        "permission",
        "vulnerab",
        "attack",
        "policy",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn is_blocked_execution(record: &ExecutionRecord) -> bool {
    record.status == "failed"
        && (record
            .output_preview
            .to_ascii_lowercase()
            .contains("blocked")
            || record.command.to_ascii_lowercase().contains("rm ")
            || record.command.contains(';')
            || record.command.contains('|'))
}

fn is_permission_violation(record: &ExecutionRecord) -> bool {
    let output = record.output_preview.to_ascii_lowercase();
    output.contains("not permitted")
        || output.contains("permission")
        || output.contains("approval")
        || output.contains("remediation engine")
}

fn is_security_event(event: &OpsEvent) -> bool {
    match event {
        OpsEvent::CommandExecuted {
            success,
            stderr,
            command,
            ..
        } => {
            !success
                && (is_security_record(stderr)
                    || command.contains(';')
                    || command.contains('|')
                    || command.to_ascii_lowercase().contains("rm "))
        }
        OpsEvent::ExplainabilityRecorded { record } => {
            is_security_record(&record.action) || is_security_record(&record.why)
        }
        OpsEvent::RecoveryProposed { action } => is_security_record(&action.risk),
        OpsEvent::SandboxPolicyUpdated { .. } => true,
        _ => false,
    }
}

fn plugin_security_risk(plugin: &PluginDescriptor) -> String {
    let name_safe = !plugin.name.is_empty()
        && plugin.name.len() <= 80
        && plugin
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    let scoped = plugin.description.contains("fs:deny")
        || plugin.description.contains("net:deny")
        || plugin.description.contains("net:configured");
    if !name_safe {
        "blocked-name".into()
    } else if scoped {
        "scoped".into()
    } else if plugin.status == PluginStatus::Enabled {
        "review".into()
    } else {
        "registered".into()
    }
}

fn estimated_memory_pressure(state: &OpsState) -> u8 {
    let units = state.events.len()
        + state.logs.len()
        + state.executions.len()
        + state.explainability.len()
        + state.reasoning_stream.len();
    ((units as u16 * 100) / 600).min(100) as u8
}

fn threat_color(value: usize) -> Color {
    match value {
        0 => Color::Green,
        1..=2 => Color::Yellow,
        _ => Color::Red,
    }
}

fn short_time(timestamp: &str) -> String {
    if timestamp.len() >= 8 {
        timestamp[timestamp.len().saturating_sub(8)..].into()
    } else if timestamp.is_empty() {
        "-".into()
    } else {
        timestamp.into()
    }
}

fn octopus_marker(status: &AgentStatus) -> &'static str {
    match status {
        AgentStatus::Running => "[>]",
        AgentStatus::Waiting => "[~]",
        AgentStatus::Completed => "[OK]",
        AgentStatus::Escalated => "[!]",
        AgentStatus::Failed => "[X]",
        AgentStatus::Idle => "[-]",
    }
}
fn health_color(value: u8) -> Color {
    match value {
        90..=100 => Color::Green,
        70..=89 => Color::Yellow,
        _ => Color::Red,
    }
}

fn parse_role(input: &str) -> UserRole {
    match input.to_ascii_lowercase().as_str() {
        "admin" => UserRole::Admin,
        "operator" => UserRole::Operator,
        "security" | "security-reviewer" => UserRole::SecurityReviewer,
        _ => UserRole::ReadOnly,
    }
}

fn parse_plugin_kind(input: &str) -> PluginKind {
    match input.to_ascii_lowercase().as_str() {
        "workflow" => PluginKind::Workflow,
        "integration" => PluginKind::Integration,
        "agent" => PluginKind::Agent,
        _ => PluginKind::Tool,
    }
}

fn parse_runtime_kind(input: &str) -> AgentRuntimeKind {
    match input.to_ascii_lowercase().as_str() {
        "remote" | "server" => AgentRuntimeKind::RemoteServer,
        "container" => AgentRuntimeKind::Container,
        "cluster" => AgentRuntimeKind::Cluster,
        _ => AgentRuntimeKind::LocalProcess,
    }
}

fn runtime_status_for(endpoint: &str) -> RuntimeStatus {
    if endpoint.starts_with("ssh://") || endpoint.starts_with("remote://") {
        RuntimeStatus::Active
    } else if endpoint.starts_with("container://") {
        RuntimeStatus::Provisioned
    } else {
        RuntimeStatus::Local
    }
}

fn format_duration(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    format!("{hours:02}:{minutes:02}:{seconds:02}")
}
