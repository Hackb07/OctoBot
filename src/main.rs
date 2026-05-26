#![allow(dead_code)]

mod agents;
mod ai;
mod api;
mod constants;
mod infra;
mod models;
mod observability;
mod persistence;
mod plugins;
mod remediation;
mod reports;
mod trace;
mod runtime;
mod ui;
mod utils;
mod workflows;

#[cfg(test)]
mod tests;

use color_eyre::eyre::{Context, Result};
use models::OpsState;
use runtime::{ops_runtime, run_live_log_stream};
use tokio::sync::{mpsc, watch};
use tracing::debug;
use ui::App;

fn load_dotenv() {
    let content = match std::fs::read_to_string(".env") {
        Ok(c) => c,
        Err(_) => return,
    };
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let k = key.trim();
            let v = value.trim().trim_matches(&['"', '\''][..]);
            if !k.is_empty() && !std::env::var(k).is_ok() {
                unsafe { std::env::set_var(k, v); }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    load_dotenv();
    color_eyre::install()?;
    if std::env::var("OCTOBOT_TRACE").is_ok() {
        tracing_subscriber::fmt::init();
    } else {
        tracing_subscriber::fmt().with_writer(std::io::sink).init();
    }

    let (tx, rx) = watch::channel(OpsState::seed());
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    tokio::spawn(ops_runtime(tx, event_rx, event_tx.clone()));
    tokio::spawn(run_live_log_stream(event_tx.clone()));
    tokio::spawn({
        let rx = rx.clone();
        async move {
            if let Err(error) = api::serve_api(rx).await {
                debug!(%error, "control API exited");
            }
        }
    });

    let mut terminal = ratatui::init();
    let result = App::new(event_tx).run(&mut terminal, rx).await;
    ratatui::restore();
    result.context("running OctoBot terminal UI")
}
