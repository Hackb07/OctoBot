from pathlib import Path

import pytest

from backend.octobot_orchestrator.contracts import ToolRequest, ToolStatus
from backend.octobot_orchestrator.memory.store import build_embedding_provider
from backend.octobot_orchestrator.tools.execution import validate_command
from backend.octobot_orchestrator.tools.registry import build_tool_registry


@pytest.mark.asyncio
async def test_edit_file_dry_run_returns_patch(tmp_path: Path):
    target = tmp_path / "app.py"
    target.write_text("def hello():\n    return 'old'\n", encoding="utf-8")
    registry = build_tool_registry()

    result = await registry.execute(
        ToolRequest(
            task_id="task-test",
            name="edit_file",
            arguments={
                "root": str(tmp_path),
                "path": "app.py",
                "old": "old",
                "new": "new",
            },
            dry_run=True,
        )
    )

    assert result.status == ToolStatus.COMPLETED
    assert "+    return 'new'" in result.output
    assert target.read_text(encoding="utf-8") == "def hello():\n    return 'old'\n"


@pytest.mark.asyncio
async def test_symbol_lookup_finds_python_symbol(tmp_path: Path):
    (tmp_path / "service.py").write_text("class Service:\n    pass\n", encoding="utf-8")
    registry = build_tool_registry()

    result = await registry.execute(
        ToolRequest(
            task_id="task-test",
            name="symbol_lookup",
            arguments={"root": str(tmp_path), "query": "Service"},
        )
    )

    assert result.status == ToolStatus.COMPLETED
    assert result.data["matches"][0]["name"] == "Service"


def test_command_policy_blocks_destructive_command():
    assert validate_command("sudo rm -rf /") is not None
    assert validate_command("cargo test") is None


@pytest.mark.asyncio
async def test_run_tests_dry_run_skips_execution(tmp_path: Path):
    (tmp_path / "Cargo.toml").write_text("[package]\nname='x'\nversion='0.1.0'\n", encoding="utf-8")
    registry = build_tool_registry()

    result = await registry.execute(
        ToolRequest(
            task_id="task-test",
            name="run_tests",
            arguments={"root": str(tmp_path)},
            dry_run=True,
        )
    )

    assert result.status == ToolStatus.COMPLETED
    assert result.data["dry_run"] is True


@pytest.mark.asyncio
async def test_registry_cancels_token_before_execution(tmp_path: Path):
    registry = build_tool_registry(use_runtime=False)
    registry.cancel("cancel-me")

    result = await registry.execute(
        ToolRequest(
            task_id="task-test",
            name="list_directory",
            arguments={"root": str(tmp_path)},
            cancellation_token="cancel-me",
        )
    )

    assert result.status == ToolStatus.CANCELLED


@pytest.mark.asyncio
async def test_git_snapshot_and_pr_summary_skip_non_git_repo(tmp_path: Path):
    registry = build_tool_registry(use_runtime=False)

    snapshot = await registry.execute(
        ToolRequest(task_id="task-test", name="git_snapshot", arguments={"root": str(tmp_path)})
    )
    summary = await registry.execute(
        ToolRequest(task_id="task-test", name="pr_summary", arguments={"root": str(tmp_path)})
    )

    assert snapshot.status == ToolStatus.COMPLETED
    assert snapshot.data["skipped"] is True
    assert summary.data["skipped"] is True


def test_active_embedding_provider_has_stable_vector():
    provider = build_embedding_provider()

    vector = provider.embed("jwt authentication middleware")

    assert vector
    assert provider.name in {"deterministic", "sentence-transformers"}
