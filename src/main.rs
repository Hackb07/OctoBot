#![allow(dead_code)]

use std::{
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

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
mod runtime;
mod runtime_service;
mod security;
mod trace;
mod ui;
mod utils;
mod workflows;

#[cfg(test)]
mod tests;

use color_eyre::eyre::{Context, Result};
use models::OpsState;
use runtime::{ops_runtime, run_live_log_stream};
use tokio::process::Command;
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
            if !k.is_empty() && std::env::var(k).is_err() {
                unsafe {
                    std::env::set_var(k, v);
                }
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

    if std::env::var("OCTOBOT_RUNTIME_ONLY").is_ok() {
        return runtime_service::serve_runtime_service()
            .await
            .context("running OctoBot runtime service only");
    }

    if std::env::var("OCTOBOT_NO_AUTOSTART").is_err() {
        autostart_local_services().await;
    }

    let (tx, rx) = watch::channel(OpsState::seed());
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    tokio::spawn(ops_runtime(tx, event_rx, event_tx.clone()));
    tokio::spawn(run_live_log_stream(event_tx.clone()));
    tokio::spawn({
        let rx = rx.clone();
        let event_tx = event_tx.clone();
        async move {
            if let Err(error) = api::serve_api(rx, event_tx).await {
                debug!(%error, "control API exited");
            }
        }
    });
    tokio::spawn(async move {
        if let Err(error) = runtime_service::serve_runtime_service().await {
            debug!(%error, "runtime service exited");
        }
    });

    let mut terminal = ratatui::init();
    let result = App::new(event_tx).run(&mut terminal, rx).await;
    ratatui::restore();
    result.context("running OctoBot terminal UI")
}

#[derive(Debug, Clone)]
struct LocalService {
    name: &'static str,
    health_url: &'static str,
    command: &'static str,
    args: &'static [&'static str],
    cwd: Option<&'static str>,
    log_name: &'static str,
    required_path: Option<&'static str>,
}

async fn autostart_local_services() {
    let services = [
        LocalService {
            name: "ollama",
            health_url: "http://127.0.0.1:11434/api/tags",
            command: "ollama",
            args: &["serve"],
            cwd: None,
            log_name: "ollama.log",
            required_path: None,
        },
        LocalService {
            name: "python-orchestrator",
            health_url: "http://127.0.0.1:8787/health",
            command: ".venv/bin/uvicorn",
            args: &[
                "backend.octobot_orchestrator.main:app",
                "--host",
                "127.0.0.1",
                "--port",
                "8787",
            ],
            cwd: None,
            log_name: "orchestrator.log",
            required_path: Some(".venv/bin/uvicorn"),
        },
        LocalService {
            name: "frontend-dev",
            health_url: "http://127.0.0.1:5173/",
            command: "npm",
            args: &["run", "dev", "--", "--host", "127.0.0.1"],
            cwd: Some("frontend"),
            log_name: "frontend.log",
            required_path: Some("frontend/node_modules"),
        },
    ];

    for service in services {
        if let Err(error) = start_or_reuse_service(&service).await {
            eprintln!("[warn] {} autostart skipped: {error}", service.name);
        }
    }
}

async fn start_or_reuse_service(service: &LocalService) -> Result<()> {
    if service_is_up(service.health_url).await {
        eprintln!("[reuse] {} {}", service.name, service.health_url);
        return Ok(());
    }

    if let Some(path) = service.required_path
        && !Path::new(path).exists()
    {
        eprintln!(
            "[skip] {} requires `{}`. Install dependencies first.",
            service.name, path
        );
        return Ok(());
    }

    let log_dir = PathBuf::from(".octobot").join("logs");
    std::fs::create_dir_all(&log_dir).context("creating .octobot/logs")?;
    let log_path = log_dir.join(service.log_name);
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("opening {}", log_path.display()))?;
    let stderr = stdout
        .try_clone()
        .with_context(|| format!("cloning {}", log_path.display()))?;

    let mut command = Command::new(service.command);
    command
        .args(service.args)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    if let Some(cwd) = service.cwd {
        command.current_dir(cwd);
    }

    let mut child = command
        .spawn()
        .with_context(|| format!("starting {}", service.name))?;
    let pid = child.id().unwrap_or_default();
    eprintln!(
        "[start] {} pid={} log={}",
        service.name,
        pid,
        log_path.display()
    );

    for _ in 0..40 {
        if service_is_up(service.health_url).await {
            eprintln!("[ok] {} {}", service.name, service.health_url);
            tokio::spawn(async move {
                let _ = child.wait().await;
            });
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("checking {} startup", service.name))?
        {
            eprintln!(
                "[warn] {} exited during startup with status {status}. Check {}",
                service.name,
                log_path.display()
            );
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    eprintln!(
        "[warn] {} did not become healthy yet; continuing in background. Check {}",
        service.name,
        log_path.display()
    );
    Ok(())
}

async fn service_is_up(url: &str) -> bool {
    let client = reqwest::Client::new();
    match client
        .get(url)
        .timeout(Duration::from_millis(700))
        .send()
        .await
    {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}
