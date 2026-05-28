from __future__ import annotations

from ..contracts import CodingTaskState, ToolStatus


def classify_failure(output: str) -> str:
    lower = output.lower()
    if "could not compile" in lower or "syntaxerror" in lower or "error[" in lower:
        return "compile_error"
    if "failed" in lower and ("test" in lower or "assert" in lower):
        return "test_failure"
    if "lint" in lower or "ruff" in lower or "clippy" in lower:
        return "lint_failure"
    if "no module named" in lower or "module not found" in lower or "unresolved import" in lower:
        return "missing_dependency"
    if "timed out" in lower:
        return "timeout"
    if "blocked" in lower or "not allowlisted" in lower:
        return "policy_block"
    return "unknown_failure"


def build_execution_report(task: CodingTaskState) -> dict:
    failed_tools = [result for result in task.tool_results if result.status == ToolStatus.FAILED]
    classifications = [
        {
            "tool": result.name,
            "classification": classify_failure(result.output),
            "summary": result.output[:500],
        }
        for result in failed_tools
    ]
    return {
        "task_id": task.id,
        "goal": task.request.goal,
        "status": task.status.value,
        "dry_run": task.request.dry_run,
        "plan_steps": len(task.plan.steps) if task.plan else 0,
        "agents": [message.role.value for message in task.messages],
        "tools_run": [
            {"name": result.name, "status": result.status.value}
            for result in task.tool_results
        ],
        "failure_classifications": classifications,
        "modified_files": _modified_files(task),
        "execution_snapshot": build_execution_snapshot(task),
        "validation_gate": validation_gate(task),
        "pr_summary": build_pr_summary(task),
        "confidence": confidence_score(task),
        "risks": task.plan.risks if task.plan else [],
    }


def _modified_files(task: CodingTaskState) -> list[str]:
    files = set()
    for result in task.tool_results:
        path = result.data.get("path")
        if path and result.name in {"write_file", "edit_file", "generate_patch"}:
            files.add(str(path))
    return sorted(files)


def confidence_score(task: CodingTaskState) -> int:
    score = 70
    failed = sum(1 for result in task.tool_results if result.status == ToolStatus.FAILED)
    completed = sum(1 for result in task.tool_results if result.status == ToolStatus.COMPLETED)
    score += min(20, completed * 3)
    score -= failed * 20
    if task.request.dry_run:
        score -= 10
    return max(0, min(100, score))


def build_execution_snapshot(task: CodingTaskState) -> dict:
    return {
        "event_count": len(task.events),
        "message_count": len(task.messages),
        "tool_count": len(task.tool_results),
        "last_event": task.events[-1].type.value if task.events else None,
    }


def validation_gate(task: CodingTaskState) -> dict:
    for event in reversed(task.events):
        if event.type.value == "audit" and event.payload.get("kind") == "validation.gate":
            return {
                "passed": bool(event.payload.get("passed")),
                "explanation": str(event.payload.get("explanation", "")),
                "validation_tools": event.payload.get("validation_tools", []),
            }
    return {
        "passed": False,
        "explanation": "validation gate was not recorded",
        "validation_tools": [],
    }


def build_pr_summary(task: CodingTaskState) -> str:
    files = _modified_files(task)
    tools = ", ".join(result.name for result in task.tool_results) or "no tools"
    file_text = ", ".join(files) if files else "no files modified"
    return (
        f"Task: {task.request.goal}\n"
        f"Status: {task.status.value}\n"
        f"Files: {file_text}\n"
        f"Validation/tools: {tools}\n"
        f"Confidence: {confidence_score(task)}"
    )
