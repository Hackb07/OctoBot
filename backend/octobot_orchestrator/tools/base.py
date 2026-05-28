from __future__ import annotations

import asyncio
import os
from collections.abc import Awaitable, Callable
from datetime import UTC, datetime
from typing import Any

from ..contracts import ToolRequest, ToolResult, ToolStatus
from ..runtime.client import RustRuntimeClient

ToolHandler = Callable[[ToolRequest], Awaitable[ToolResult]]


RUNTIME_TOOLS = {
    "execute_terminal",
    "run_tests",
    "lint_project",
    "list_directory",
    "read_file",
    "write_file",
}


class CancellationRegistry:
    def __init__(self) -> None:
        self._events: dict[str, asyncio.Event] = {}

    def token(self, token: str | None) -> asyncio.Event | None:
        if not token:
            return None
        return self._events.setdefault(token, asyncio.Event())

    def cancel(self, token: str) -> None:
        self._events.setdefault(token, asyncio.Event()).set()


class ToolRegistry:
    def __init__(
        self,
        runtime_client: RustRuntimeClient | None = None,
        use_runtime: bool | None = None,
        cancellations: CancellationRegistry | None = None,
    ) -> None:
        self._handlers: dict[str, ToolHandler] = {}
        self._runtime_client = runtime_client or RustRuntimeClient()
        self._use_runtime = (
            use_runtime
            if use_runtime is not None
            else os.getenv("OCTOBOT_USE_RUST_RUNTIME", "1") != "0"
        )
        self._cancellations = cancellations or CancellationRegistry()

    def register(self, name: str, handler: ToolHandler) -> None:
        if name in self._handlers:
            raise ValueError(f"tool already registered: {name}")
        self._handlers[name] = handler

    def names(self) -> list[str]:
        return sorted(self._handlers)

    async def execute(self, request: ToolRequest) -> ToolResult:
        cancellation = self._cancellations.token(request.cancellation_token)
        if cancellation and cancellation.is_set():
            return cancelled_result(request)
        if self._use_runtime and request.name in RUNTIME_TOOLS:
            runtime_result = await self._execute_runtime(request, cancellation)
            if runtime_result is not None:
                return runtime_result
        handler = self._handlers.get(request.name)
        if handler is None:
            return ToolResult(
                id=request.id,
                task_id=request.task_id,
                name=request.name,
                status=ToolStatus.FAILED,
                output=f"unknown tool: {request.name}",
                completed_at=datetime.now(UTC),
            )
        try:
            task = asyncio.create_task(handler(request))
            if cancellation is None:
                return await asyncio.wait_for(task, timeout=request.timeout_seconds)
            cancel_task = asyncio.create_task(cancellation.wait())
            done, pending = await asyncio.wait(
                {task, cancel_task},
                timeout=request.timeout_seconds,
                return_when=asyncio.FIRST_COMPLETED,
            )
            for pending_task in pending:
                pending_task.cancel()
            if cancel_task in done:
                task.cancel()
                return cancelled_result(request)
            if task in done:
                return task.result()
            task.cancel()
            return timeout_result(request)
        except TimeoutError:
            return timeout_result(request)
        except Exception as error:
            return ToolResult(
                id=request.id,
                task_id=request.task_id,
                name=request.name,
                status=ToolStatus.FAILED,
                output=f"{type(error).__name__}: {error}",
                completed_at=datetime.now(UTC),
            )

    def cancel(self, token: str) -> None:
        self._cancellations.cancel(token)

    async def _execute_runtime(
        self, request: ToolRequest, cancellation: asyncio.Event | None
    ) -> ToolResult | None:
        output_chunks: list[str] = []
        final_payload: dict[str, Any] = {}
        try:
            stream = self._runtime_client.stream_tool(request)
            async for event in stream:
                if cancellation and cancellation.is_set():
                    await stream.aclose()
                    return cancelled_result(request)
                if event.type.value == "tool.output":
                    output_chunks.append(str(event.payload.get("data", "")))
                elif event.type.value == "tool.completed":
                    final_payload = event.payload
                    return ToolResult(
                        id=request.id,
                        task_id=request.task_id,
                        name=request.name,
                        status=ToolStatus.COMPLETED,
                        output="\n".join(output_chunks),
                        data=final_payload,
                        completed_at=datetime.now(UTC),
                    )
                elif event.type.value == "tool.failed":
                    return ToolResult(
                        id=request.id,
                        task_id=request.task_id,
                        name=request.name,
                        status=ToolStatus.FAILED,
                        output=str(event.payload.get("error", event.payload)),
                        data=event.payload,
                        completed_at=datetime.now(UTC),
                    )
        except Exception:
            return None
        return None


def result_from_data(request: ToolRequest, data: dict[str, Any], output: str = "") -> ToolResult:
    return ToolResult(
        id=request.id,
        task_id=request.task_id,
        name=request.name,
        status=ToolStatus.COMPLETED,
        output=output,
        data=data,
        completed_at=datetime.now(UTC),
    )


def timeout_result(request: ToolRequest) -> ToolResult:
    return ToolResult(
        id=request.id,
        task_id=request.task_id,
        name=request.name,
        status=ToolStatus.FAILED,
        output=f"tool timed out after {request.timeout_seconds}s",
        completed_at=datetime.now(UTC),
    )


def cancelled_result(request: ToolRequest) -> ToolResult:
    return ToolResult(
        id=request.id,
        task_id=request.task_id,
        name=request.name,
        status=ToolStatus.CANCELLED,
        output="tool cancelled",
        completed_at=datetime.now(UTC),
    )
