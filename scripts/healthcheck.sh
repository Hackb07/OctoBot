#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

check() {
  local name="$1"
  local url="$2"

  if [ -n "$(curl -s "$url")" ]; then
    printf '[ok] %s %s\n' "$name" "$url"
  else
    printf '[missing] %s %s\n' "$name" "$url"
    return 1
  fi
}

status=0
check "ollama" "http://127.0.0.1:11434/api/tags" || status=1
check "rust-control-api" "http://127.0.0.1:7878/health" || status=1
check "rust-runtime-service" "http://127.0.0.1:7879/runtime/health" || status=1
check "python-orchestrator" "http://127.0.0.1:8787/health" || status=1

if [ -n "$(curl -s "http://127.0.0.1:5173/")" ]; then
  printf '[ok] frontend-dev http://127.0.0.1:5173/\n'
else
  printf '[optional] frontend-dev http://127.0.0.1:5173/ is not running\n'
fi

exit "$status"
