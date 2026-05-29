from __future__ import annotations

import asyncio
from collections.abc import AsyncIterator
from datetime import UTC, datetime
import os

from fastapi import FastAPI, Header, HTTPException, Request, WebSocket, WebSocketDisconnect
from fastapi.responses import JSONResponse, PlainTextResponse, StreamingResponse

from .contracts import CodingTaskRequest, CodingTaskState, RuntimeEvent, RuntimeEventType, TaskStatus
from .env import load_dotenv
from .workflows.autonomous_loop import AutonomousExecutionLoop


load_dotenv()

app = FastAPI(title="OctoBot Autonomous Orchestrator", version="0.1.0")
loop = AutonomousExecutionLoop()


@app.middleware("http")
async def service_token_auth(request: Request, call_next):
    token = os.getenv("OCTOBOT_SERVICE_TOKEN")
    if token and request.url.path not in {"/health", "/metrics"}:
        if request.headers.get("x-octobot-token") != token:
            return JSONResponse({"detail": "invalid service token"}, status_code=401)
    response = await call_next(request)
    response.headers["x-octobot-correlation-id"] = request.headers.get(
        "x-correlation-id", f"corr-{datetime.now(UTC).timestamp():.0f}"
    )
    return response


@app.get("/health")
async def health() -> dict[str, str]:
    return {"status": "ok", "service": "octobot-orchestrator"}


@app.get("/metrics", response_class=PlainTextResponse)
async def metrics() -> str:
    tasks = list(loop._tasks.values())
    total_events = sum(len(task.events) for task in tasks)
    failed_tasks = sum(1 for task in tasks if task.status.value == "failed")
    completed_tasks = sum(1 for task in tasks if task.status.value == "completed")
    tool_runs = sum(len(task.tool_results) for task in tasks)
    failed_tools = sum(
        1
        for task in tasks
        for result in task.tool_results
        if result.status.value == "failed"
    )
    return "\n".join(
        [
            "# HELP octobot_tasks_total Total orchestrator tasks.",
            "# TYPE octobot_tasks_total gauge",
            f"octobot_tasks_total {len(tasks)}",
            "# HELP octobot_tasks_completed Completed orchestrator tasks.",
            "# TYPE octobot_tasks_completed gauge",
            f"octobot_tasks_completed {completed_tasks}",
            "# HELP octobot_tasks_failed Failed orchestrator tasks.",
            "# TYPE octobot_tasks_failed gauge",
            f"octobot_tasks_failed {failed_tasks}",
            "# HELP octobot_events_total Total task events.",
            "# TYPE octobot_events_total gauge",
            f"octobot_events_total {total_events}",
            "# HELP octobot_tool_runs_total Total tool results.",
            "# TYPE octobot_tool_runs_total gauge",
            f"octobot_tool_runs_total {tool_runs}",
            "# HELP octobot_tool_failures_total Failed tool results.",
            "# TYPE octobot_tool_failures_total gauge",
            f"octobot_tool_failures_total {failed_tools}",
            "",
        ]
    )


@app.get("/api/observability/traces")
async def observability_traces() -> dict:
    spans = []
    for task in loop._tasks.values():
        previous = task.created_at
        for event in task.events:
            spans.append(
                {
                    "trace_id": task.id,
                    "span_id": event.id,
                    "name": event.type.value,
                    "start_time": previous.isoformat(),
                    "end_time": event.timestamp.isoformat(),
                    "attributes": event.payload,
                }
            )
            previous = event.timestamp
    return {"resource": "octobot-orchestrator", "spans": spans}


@app.get("/api/observability/logs")
async def observability_logs(x_correlation_id: str | None = Header(default=None)) -> dict:
    logs = []
    for task in loop._tasks.values():
        for event in task.events:
            logs.append(
                {
                    "timestamp": event.timestamp.isoformat(),
                    "level": "info",
                    "correlation_id": x_correlation_id or task.id,
                    "task_id": task.id,
                    "event": event.type.value,
                    "payload": event.payload,
                }
            )
    return {"logs": logs}


@app.post("/api/tasks", response_model=CodingTaskState)
async def create_task(request: CodingTaskRequest) -> CodingTaskState:
    return await loop.create_task(request)


@app.post("/api/tasks/{task_id}/run", response_model=CodingTaskState)
async def run_task(task_id: str) -> CodingTaskState:
    task = loop.get_task(task_id)
    if task is None:
        raise HTTPException(status_code=404, detail="task not found")
    return await loop.run(task_id)


@app.get("/api/tasks/{task_id}", response_model=CodingTaskState)
async def get_task(task_id: str) -> CodingTaskState:
    task = loop.get_task(task_id)
    if task is None:
        raise HTTPException(status_code=404, detail="task not found")
    return task


@app.get("/api/tasks")
async def list_tasks() -> dict[str, list[CodingTaskState]]:
    return {"tasks": list(loop._tasks.values())}


@app.get("/api/tasks/{task_id}/report")
async def task_report(task_id: str) -> dict:
    task = loop.get_task(task_id)
    if task is None:
        raise HTTPException(status_code=404, detail="task not found")
    return task.report


@app.get("/api/tasks/{task_id}/events/replay")
async def task_event_replay(task_id: str) -> dict[str, list[RuntimeEvent]]:
    if loop.get_task(task_id) is None:
        raise HTTPException(status_code=404, detail="task not found")
    return {"events": loop._store.load_events(task_id)}


@app.get("/api/tasks/{task_id}/observability")
async def task_observability(task_id: str) -> dict:
    task = loop.get_task(task_id)
    if task is None:
        raise HTTPException(status_code=404, detail="task not found")
    event_counts: dict[str, int] = {}
    for event in task.events:
        event_counts[event.type.value] = event_counts.get(event.type.value, 0) + 1
    validation_gate = task.report.get("validation_gate", {}) if task.report else {}
    return {
        "task_id": task.id,
        "status": task.status.value,
        "event_counts": event_counts,
        "tool_count": len(task.tool_results),
        "agent_message_count": len(task.messages),
        "validation_gate": validation_gate,
        "last_event": task.events[-1].type.value if task.events else None,
    }


@app.post("/api/tasks/{task_id}/cancel", response_model=CodingTaskState)
async def cancel_task(task_id: str) -> CodingTaskState:
    task = loop.get_task(task_id)
    if task is None:
        raise HTTPException(status_code=404, detail="task not found")
    task.status = TaskStatus.CANCELLED
    loop.record_event(
        task_id,
        RuntimeEventType.TASK_UPDATED,
        {"status": TaskStatus.CANCELLED.value},
    )
    return task


@app.get("/api/tasks/{task_id}/events")
async def task_events(task_id: str) -> StreamingResponse:
    if loop.get_task(task_id) is None:
        raise HTTPException(status_code=404, detail="task not found")

    async def stream() -> AsyncIterator[str]:
        async for event in loop.subscribe(task_id):
            yield f"event: {event.type.value}\ndata: {event.model_dump_json()}\n\n"

    return StreamingResponse(stream(), media_type="text/event-stream")


@app.websocket("/ws/tasks/{task_id}")
async def task_websocket(websocket: WebSocket, task_id: str) -> None:
    if loop.get_task(task_id) is None:
        await websocket.close(code=4404)
        return
    await websocket.accept()
    try:
        async for event in loop.subscribe(task_id):
            await websocket.send_text(event.model_dump_json())
    except WebSocketDisconnect:
        return
    except asyncio.CancelledError:
        raise


def emit_task_event(task: CodingTaskState, event: RuntimeEvent) -> CodingTaskState:
    task.events.append(event)
    return task
