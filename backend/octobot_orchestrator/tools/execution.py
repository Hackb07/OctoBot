from __future__ import annotations

import asyncio
import json
import shlex
from datetime import UTC, datetime
from pathlib import Path

from .base import result_from_data
from .filesystem import WorkspacePolicy
from ..contracts import ToolRequest, ToolResult, ToolStatus

BLOCKED_TOKENS = {
    "sudo",
    "su",
    "mkfs",
    "shutdown",
    "reboot",
    "poweroff",
    "halt",
}

DANGEROUS_FRAGMENTS = {
    "rm -rf /",
    ":(){",
    "chmod -R 777 /",
    "chown -R",
}

ALLOWED_COMMANDS = {
    "cargo": {"test", "check", "fmt", "clippy"},
    "python": {"-m"},
    "python3": {"-m"},
    "pytest": set(),
    "ruff": {"check", "format"},
    "npm": {"test", "run"},
    "pnpm": {"test", "run"},
    "yarn": {"test", "run"},
    "go": {"test", "vet"},
}


async def execute_terminal(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    command = request.arguments["command"]
    cwd = policy.resolve(request.arguments.get("cwd", "."))
    timeout = int(request.arguments.get("timeout_seconds", request.timeout_seconds))
    validation_error = validate_command(command)
    if validation_error:
        return ToolResult(
            id=request.id,
            task_id=request.task_id,
            name=request.name,
            status=ToolStatus.FAILED,
            output=validation_error,
            completed_at=datetime.now(UTC),
        )
    if request.dry_run and not request.arguments.get("allow_dry_run_execution", False):
        return result_from_data(
            request,
            {"command": command, "cwd": str(cwd), "dry_run": True},
            output="dry-run: command skipped",
        )
    process = await asyncio.create_subprocess_exec(
        *shlex.split(command),
        cwd=str(cwd),
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    try:
        stdout, stderr = await asyncio.wait_for(process.communicate(), timeout=timeout)
    except TimeoutError:
        process.kill()
        await process.wait()
        return ToolResult(
            id=request.id,
            task_id=request.task_id,
            name=request.name,
            status=ToolStatus.FAILED,
            output=f"command timed out after {timeout}s",
            completed_at=datetime.now(UTC),
        )
    output = (stdout + stderr).decode("utf-8", errors="replace")
    status = ToolStatus.COMPLETED if process.returncode == 0 else ToolStatus.FAILED
    return ToolResult(
        id=request.id,
        task_id=request.task_id,
        name=request.name,
        status=status,
        output=output[-12000:],
        data={"command": command, "exit_code": process.returncode, "cwd": str(cwd)},
        completed_at=datetime.now(UTC),
    )


async def run_tests(request: ToolRequest) -> ToolResult:
    root = Path(request.arguments["root"])
    command = request.arguments.get("command") or detect_test_command(root)
    if not command:
        return result_from_data(request, {"skipped": True}, output="no test command detected")
    return await execute_terminal(
        request.model_copy(
            update={
                "name": request.name,
                "arguments": {
                    **request.arguments,
                    "command": command,
                    "allow_dry_run_execution": request.arguments.get("allow_dry_run_execution", False),
                },
            }
        )
    )


async def lint_project(request: ToolRequest) -> ToolResult:
    root = Path(request.arguments["root"])
    command = request.arguments.get("command") or detect_lint_command(root)
    if not command:
        return result_from_data(request, {"skipped": True}, output="no lint command detected")
    return await execute_terminal(
        request.model_copy(
            update={
                "name": request.name,
                "arguments": {
                    **request.arguments,
                    "command": command,
                    "allow_dry_run_execution": request.arguments.get("allow_dry_run_execution", False),
                },
            }
        )
    )


async def dependency_scan(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    manifests = []
    for name in ["Cargo.toml", "pyproject.toml", "package.json", "go.mod"]:
        path = policy.root / name
        if path.exists():
            manifests.append({"path": name, "bytes": path.stat().st_size})
    findings = []
    package_json = policy.root / "package.json"
    if package_json.exists():
        try:
            data = json.loads(package_json.read_text(encoding="utf-8"))
            for dep, version in {**data.get("dependencies", {}), **data.get("devDependencies", {})}.items():
                if str(version).startswith(("*", "latest")):
                    findings.append(f"{dep} uses unpinned version {version}")
        except json.JSONDecodeError:
            findings.append("package.json is invalid JSON")
    return result_from_data(request, {"manifests": manifests, "findings": findings}, output="\n".join(findings))


async def search_docs(request: ToolRequest) -> ToolResult:
    query = request.arguments["query"].lower()
    root = WorkspacePolicy(request.arguments["root"]).root
    matches = []
    paths = [root / "README.md"]
    docs_dir = root / "docs"
    if docs_dir.exists():
        paths.extend(docs_dir.glob("*.md"))
    for path in paths:
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        for line_no, line in enumerate(text.splitlines(), start=1):
            if query in line.lower():
                matches.append({"path": str(path.relative_to(root)), "line": line_no, "text": line.strip()})
    return result_from_data(request, {"matches": matches[:50]}, output=f"{len(matches[:50])} matches")


def validate_command(command: str) -> str | None:
    lowered = command.lower()
    if any(fragment in lowered for fragment in DANGEROUS_FRAGMENTS):
        return "blocked dangerous command fragment"
    try:
        parts = shlex.split(command)
    except ValueError as error:
        return f"invalid shell syntax: {error}"
    if not parts:
        return "empty command"
    if any(token in BLOCKED_TOKENS for token in parts):
        return "blocked privileged or destructive command"
    binary = parts[0]
    allowed = ALLOWED_COMMANDS.get(binary)
    if allowed is None:
        return f"command is not allowlisted: {binary}"
    if allowed and (len(parts) < 2 or parts[1] not in allowed):
        return f"command subcommand is not allowlisted: {' '.join(parts[:2])}"
    return None


def detect_test_command(root: Path) -> str | None:
    if (root / "Cargo.toml").exists():
        return "cargo test"
    if (root / "pyproject.toml").exists():
        return "python -m pytest tests"
    if (root / "package.json").exists():
        return "npm test"
    if (root / "go.mod").exists():
        return "go test ./..."
    return None


def detect_lint_command(root: Path) -> str | None:
    if (root / "Cargo.toml").exists():
        return "cargo check"
    if (root / "pyproject.toml").exists():
        return "ruff check ."
    if (root / "package.json").exists():
        return "npm run lint"
    if (root / "go.mod").exists():
        return "go vet ./..."
    return None
