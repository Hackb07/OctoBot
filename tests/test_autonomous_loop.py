from datetime import UTC, datetime

import pytest

from backend.octobot_orchestrator.contracts import (
    CodingTaskRequest,
    RepositoryRef,
    RuntimeEventType,
    TaskStatus,
    ToolRequest,
    ToolResult,
    ToolStatus,
)
from backend.octobot_orchestrator.workflows.autonomous_loop import AutonomousExecutionLoop


@pytest.mark.asyncio
async def test_autonomous_loop_creates_plan_and_completes_dry_run():
    loop = AutonomousExecutionLoop()
    task = await loop.create_task(
        CodingTaskRequest(goal="summarize repository", repository=RepositoryRef(path="."))
    )

    completed = await loop.run(task.id)

    assert completed.status == TaskStatus.COMPLETED
    assert completed.plan is not None
    assert completed.messages
    assert completed.tool_results
    assert completed.report["task_id"] == completed.id
    assert completed.report["tools_run"]
    assert any(result["name"] == "generate_patch" for result in completed.report["tools_run"])
    assert "pr_summary" in completed.report
    assert isinstance(completed.report["confidence"], int)
    assert completed.report["validation_gate"]["explanation"]


class FailingValidationRegistry:
    async def execute(self, request: ToolRequest) -> ToolResult:
        status = ToolStatus.FAILED if request.name == "run_tests" else ToolStatus.COMPLETED
        output = "tests failed: assertion failed" if request.name == "run_tests" else "ok"
        return ToolResult(
            id=request.id,
            task_id=request.task_id,
            name=request.name,
            status=status,
            output=output,
            completed_at=datetime.now(UTC),
        )


@pytest.mark.asyncio
async def test_autonomous_loop_records_repair_and_validation_gate_on_failure():
    loop = AutonomousExecutionLoop()
    loop._tools = FailingValidationRegistry()
    task = await loop.create_task(
        CodingTaskRequest(goal="fix failing tests", repository=RepositoryRef(path="."))
    )

    completed = await loop.run(task.id)

    audit_kinds = [
        event.payload.get("kind")
        for event in completed.events
        if event.type == RuntimeEventType.AUDIT
    ]
    assert "repair.started" in audit_kinds
    assert "repair.completed" in audit_kinds
    assert "validation.gate" in audit_kinds
    assert completed.report["validation_gate"]["passed"] is False
    assert completed.report["failure_classifications"][0]["classification"] == "test_failure"
