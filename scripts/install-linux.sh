#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${OCTOBOT_REPO_URL:-https://github.com/Hackb07/OctoBot}"
INSTALL_DIR="${OCTOBOT_INSTALL_DIR:-$HOME/openAi/OctoBot}"
RUN_APP=1
INSTALL_OLLAMA=1
PULL_MODELS=1

usage() {
  cat <<'EOF'
OctoBot Linux installer

Usage:
  scripts/install-linux.sh [options]

Options:
  --dir PATH        Install or update OctoBot at PATH.
  --repo URL        Clone from URL. Default: https://github.com/Hackb07/OctoBot
  --no-run          Install and verify, but do not launch cargo run.
  --skip-ollama     Do not install or start Ollama.
  --skip-models     Do not pull default Ollama models.
  -h, --help        Show this help.

Environment:
  OCTOBOT_INSTALL_DIR=/path/to/OctoBot
  OCTOBOT_REPO_URL=https://github.com/Hackb07/OctoBot
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dir)
      INSTALL_DIR="${2:?missing path for --dir}"
      shift 2
      ;;
    --repo)
      REPO_URL="${2:?missing URL for --repo}"
      shift 2
      ;;
    --no-run)
      RUN_APP=0
      shift
      ;;
    --skip-ollama)
      INSTALL_OLLAMA=0
      shift
      ;;
    --skip-models)
      PULL_MODELS=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

log() {
  printf '\n==> %s\n' "$*"
}

have() {
  command -v "$1" >/dev/null 2>&1
}

run_sudo() {
  if [ "$(id -u)" -eq 0 ]; then
    "$@"
  else
    sudo "$@"
  fi
}

install_system_packages() {
  log "Installing Linux system packages"
  if have apt-get; then
    run_sudo apt-get update
    run_sudo apt-get install -y \
      build-essential pkg-config libssl-dev ca-certificates curl git \
      python3 python3-venv python3-pip nodejs npm
  elif have dnf; then
    run_sudo dnf install -y \
      gcc gcc-c++ make pkgconf-pkg-config openssl-devel ca-certificates curl git \
      python3 python3-pip nodejs npm
  elif have pacman; then
    run_sudo pacman -Sy --needed --noconfirm \
      base-devel pkgconf openssl ca-certificates curl git \
      python python-pip nodejs npm
  elif have zypper; then
    run_sudo zypper refresh
    run_sudo zypper install -y \
      gcc gcc-c++ make pkg-config libopenssl-devel ca-certificates curl git \
      python3 python3-pip nodejs npm
  else
    echo "Unsupported package manager. Install build tools, OpenSSL dev headers, curl, git, Python 3, Node.js, and npm manually." >&2
    exit 1
  fi
}

install_rust() {
  if have cargo && have rustc; then
    log "Rust already installed: $(rustc --version)"
    return
  fi

  log "Installing Rust stable with rustup"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
  rustup update stable
}

install_ollama_if_needed() {
  if [ "$INSTALL_OLLAMA" -eq 0 ]; then
    log "Skipping Ollama install"
    return
  fi

  if have ollama; then
    log "Ollama already installed"
  else
    log "Installing Ollama"
    curl -fsSL https://ollama.com/install.sh | sh
  fi

  if curl -s http://127.0.0.1:11434/api/tags >/dev/null 2>&1; then
    log "Ollama already running"
  else
    log "Starting Ollama in the background"
    mkdir -p "$INSTALL_DIR/.octobot/logs"
    nohup ollama serve >"$INSTALL_DIR/.octobot/logs/ollama.log" 2>&1 &
    sleep 2
  fi

  if [ "$PULL_MODELS" -eq 1 ]; then
    log "Pulling default Ollama models"
    ollama pull llama3.1:8b || true
    ollama pull qwen2.5-coder:7b || true
  fi
}

clone_or_update_repo() {
  log "Preparing repository at $INSTALL_DIR"
  if [ -d "$INSTALL_DIR/.git" ]; then
    git -C "$INSTALL_DIR" fetch --all --prune
    git -C "$INSTALL_DIR" pull --ff-only
  else
    mkdir -p "$(dirname "$INSTALL_DIR")"
    git clone "$REPO_URL" "$INSTALL_DIR"
  fi
}

setup_project() {
  cd "$INSTALL_DIR"

  log "Setting up Python virtual environment"
  python3 -m venv .venv
  .venv/bin/pip install --upgrade pip
  .venv/bin/pip install -e ".[dev]"

  log "Installing frontend dependencies"
  npm --prefix frontend install

  log "Building and testing OctoBot"
  cargo test
  PYTHONPATH=. .venv/bin/pytest
  npm --prefix frontend run build
}

main() {
  install_system_packages
  install_rust
  clone_or_update_repo
  install_ollama_if_needed
  setup_project

  log "OctoBot is installed"
  echo "Project: $INSTALL_DIR"
  echo "Run later with:"
  echo "  cd \"$INSTALL_DIR\" && cargo run"

  if [ "$RUN_APP" -eq 1 ]; then
    log "Starting OctoBot"
    cd "$INSTALL_DIR"
    cargo run
  fi
}

main "$@"
