#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

LOG_DIR="$ROOT_DIR/.octobot/logs"
mkdir -p "$LOG_DIR"

is_up() {
  [ -n "$(curl -s "$1")" ]
}

start_or_reuse() {
  local name="$1"
  local health_url="$2"
  local log_file="$3"
  shift 3

  if is_up "$health_url"; then
    printf '[reuse] %s already running at %s\n' "$name" "$health_url"
    return 0
  fi

  printf '[start] %s\n' "$name"
  "$@" >"$log_file" 2>&1 &
  local pid="$!"
  printf '%s\n' "$pid" >"$log_file.pid"

  for _ in $(seq 1 40); do
    if is_up "$health_url"; then
      printf '[ok] %s pid=%s\n' "$name" "$pid"
      return 0
    fi
    if ! kill -0 "$pid" >/dev/null 2>&1; then
      printf '[error] %s exited during startup. Log: %s\n' "$name" "$log_file"
      tail -n 80 "$log_file" || true
      return 1
    fi
    sleep 0.25
  done

  printf '[error] %s did not become healthy. Log: %s\n' "$name" "$log_file"
  tail -n 80 "$log_file" || true
  return 1
}

start_or_reuse \
  "ollama" \
  "http://127.0.0.1:11434/api/tags" \
  "$LOG_DIR/ollama.log" \
  ollama serve

start_or_reuse \
  "python-orchestrator" \
  "http://127.0.0.1:8787/health" \
  "$LOG_DIR/orchestrator.log" \
  .venv/bin/uvicorn backend.octobot_orchestrator.main:app --host 127.0.0.1 --port 8787

if is_up "http://127.0.0.1:5173/"; then
  printf '[reuse] frontend-dev already running at http://127.0.0.1:5173/\n'
elif [ -d octobot-web/node_modules ]; then
  printf '[start] frontend-dev\n'
  (cd octobot-web && npm run dev -- --host 127.0.0.1) >"$LOG_DIR/frontend.log" 2>&1 &
  printf '%s\n' "$!" >"$LOG_DIR/frontend.log.pid"
  printf '[ok] frontend-dev starting at http://127.0.0.1:5173/; log=%s\n' "$LOG_DIR/frontend.log"
else
  printf '[skip] frontend-dev dependencies missing; run: cd octobot-web && npm install\n'
fi

printf '\nRun the full Rust TUI/control API in another terminal when needed:\n'
printf '  cargo run\n\n'
printf 'Health check:\n'
printf '  scripts/healthcheck.sh\n'
