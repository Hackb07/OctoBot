from __future__ import annotations

import asyncio
from collections import defaultdict
from datetime import UTC, datetime

from ..agents.base import AgentContext, ProviderBackedAgent, StaticAgent
from ..agents.graph import AgentGraph
from ..contracts import (
    AgentRole,
    CodingTaskRequest,
    CodingTaskState,
    RuntimeEvent,
    RuntimeEventType,
    TaskStatus,
    ToolRequest,
    ToolStatus,
    GraphExecutionPolicy,
)
from ..indexer.repository import RepositoryIndexer
from ..memory.store import MemoryRecord, build_memory_store
from ..providers import ModelRouter
from ..rag import ContextRetriever
from ..storage import SQLiteTaskStore
from ..tools.registry import build_tool_registry
from .reporting import build_execution_report


VALIDATION_TOOLS = {"run_tests", "lint_project"}


class AutonomousExecutionLoop:
    def __init__(self, store: SQLiteTaskStore | None = None) -> None:
        self._store = store or SQLiteTaskStore()
        self._tasks: dict[str, CodingTaskState] = self._store.load_tasks()
        self._subscribers: dict[str, list[asyncio.Queue[RuntimeEvent]]] = defaultdict(list)
        self._agent_graph = AgentGraph()
        self._tools = build_tool_registry()
        self._indexer = RepositoryIndexer()
        self._memory = build_memory_store()
        self._retriever = ContextRetriever(self._memory)
        self._models = ModelRouter()
        self._policy = GraphExecutionPolicy()
        self._agents = self._build_agents()

    def _build_agents(self):
        import os

        if os.getenv("OCTOBOT_PROVIDER_AGENTS", "0") == "1":
            return {role: ProviderBackedAgent(role, self._models) for role in AgentRole}
        return {
            AgentRole.RESEARCH: StaticAgent(AgentRole.RESEARCH, "Repository research prepared for: {goal}"),
            AgentRole.PLANNER: StaticAgent(AgentRole.PLANNER, "Implementation plan prepared for: {goal}"),
            AgentRole.CODING: StaticAgent(AgentRole.CODING, "Coding pass staged for: {goal}"),
            AgentRole.DEBUGGER: StaticAgent(AgentRole.DEBUGGER, "Validation pass staged for: {goal}"),
            AgentRole.REVIEWER: StaticAgent(AgentRole.REVIEWER, "Review summary staged for: {goal}"),
            AgentRole.SECURITY: StaticAgent(AgentRole.SECURITY, "Security review staged for: {goal}"),
        }

    async def create_task(self, request: CodingTaskRequest) -> CodingTaskState:
        task = CodingTaskState(request=request)
        self._tasks[task.id] = task
        self._store.save_task(task)
        self.record_event(task.id, RuntimeEventType.TASK_CREATED, {"goal": request.goal})
        return task

    def get_task(self, task_id: str) -> CodingTaskState | None:
        return self._tasks.get(task_id)

    async def run(self, task_id: str) -> CodingTaskState:
        task = self._require_task(task_id)
        task.status = TaskStatus.RUNNING
        task.updated_at = datetime.now(UTC)
        self.record_event(task.id, RuntimeEventType.TASK_UPDATED, {"status": task.status.value})

        repo_index = self._indexer.index(task.request.repository.path)
        self._retriever.index_repository(task.id, repo_index)
        self._memory.add(
            MemoryRecord(
                id=f"{task.id}:repo-summary",
                text=repo_index.architecture_summary,
                metadata={"task_id": task.id, "kind": "repo-summary"},
            )
        )

        task.plan = await self._agent_graph.plan(task.id, task.request.goal)
        self.record_event(task.id, RuntimeEventType.TASK_UPDATED, {"plan": task.plan.model_dump()})

        context = AgentContext(
            task=task,
            repository_summary=repo_index.architecture_summary,
            retrieved_context=self._retriever.retrieve(task.request.goal, task.id),
        )
        for step in task.plan.steps:
            if task.status == TaskStatus.CANCELLED:
                break
            failed_before_step = len(self._failed_tools(task))
            agent = self._agents[step.agent]
            message = await self._run_agent_with_retries(agent, context)
            task.messages.append(message)
            self.record_event(
                task.id,
                RuntimeEventType.AGENT_MESSAGE,
                {"role": message.role.value, "content": message.content},
            )

            if "list_directory" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="list_directory",
                        arguments={"root": task.request.repository.path, "path": "."},
                        dry_run=task.request.dry_run,
                    )
                )
            if "semantic_code_search" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="semantic_code_search",
                        arguments={
                            "root": task.request.repository.path,
                            "query": task.request.goal,
                            "limit": 10,
                        },
                        dry_run=task.request.dry_run,
                    )
                )
            if "symbol_lookup" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="symbol_lookup",
                        arguments={"root": task.request.repository.path, "query": task.request.goal},
                        dry_run=task.request.dry_run,
                    )
                )
            if any(tool in step.tools for tool in {"read_file", "edit_file", "generate_patch"}):
                await self._run_provider_editing_pass(task, repo_index.files[0].path if repo_index.files else None)
            if "dependency_scan" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="dependency_scan",
                        arguments={"root": task.request.repository.path},
                        dry_run=task.request.dry_run,
                    )
                )
            if "search_docs" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="search_docs",
                        arguments={"root": task.request.repository.path, "query": task.request.goal},
                        dry_run=task.request.dry_run,
                    )
                )
            if "run_tests" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="run_tests",
                        arguments={
                            "root": task.request.repository.path,
                            "allow_dry_run_execution": not task.request.dry_run,
                        },
                        dry_run=task.request.dry_run,
                    )
                )
            if "lint_project" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="lint_project",
                        arguments={
                            "root": task.request.repository.path,
                            "allow_dry_run_execution": not task.request.dry_run,
                        },
                        dry_run=task.request.dry_run,
                    )
                )
            if "git_diff" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="git_diff",
                        arguments={"root": task.request.repository.path},
                        dry_run=task.request.dry_run,
                    )
                )
            if "git_snapshot" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="git_snapshot",
                        arguments={"root": task.request.repository.path},
                        dry_run=task.request.dry_run,
                    )
                )
            if "pr_summary" in step.tools:
                await self._run_tool(
                    ToolRequest(
                        task_id=task.id,
                        name="pr_summary",
                        arguments={"root": task.request.repository.path, "title": task.request.goal},
                        dry_run=task.request.dry_run,
                    )
                )
            if step.agent == AgentRole.DEBUGGER:
                await self._repair_validation_failures(task, context, failed_before_step)
            if self._policy.stop_on_failure and any(
                result.status == ToolStatus.FAILED for result in task.tool_results
            ):
                self.record_event(
                    task.id,
                    RuntimeEventType.TASK_UPDATED,
                    {"status": "stopped", "reason": "stop_on_failure"},
                )
                break

        if task.status != TaskStatus.CANCELLED:
            task.status = (
                TaskStatus.FAILED
                if any(result.status == ToolStatus.FAILED for result in task.tool_results)
                else TaskStatus.COMPLETED
            )
        task.updated_at = datetime.now(UTC)
        self._record_validation_gate(task)
        task.report = build_execution_report(task)
        if task.report.get("confidence", 100) < self._policy.escalation_threshold:
            self.record_event(
                task.id,
                RuntimeEventType.APPROVAL_REQUIRED,
                {"reason": "low_confidence", "confidence": task.report.get("confidence")},
            )
        self.record_event(task.id, RuntimeEventType.REPORT_GENERATED, task.report)
        self.record_event(task.id, RuntimeEventType.TASK_UPDATED, {"status": task.status.value})
        self._store.save_task(task)
        return task

    async def _repair_validation_failures(
        self, task: CodingTaskState, context: AgentContext, failed_before_step: int
    ) -> None:
        failures = self._failed_tools(task)[failed_before_step:]
        if not failures:
            return
        classifications = [
            {
                "tool": result.name,
                "output": result.output[-2000:],
            }
            for result in failures
        ]
        self.record_event(
            task.id,
            RuntimeEventType.AUDIT,
            {
                "kind": "repair.started",
                "attempt": 1,
                "failures": classifications,
            },
        )
        repair_context = AgentContext(
            task=task,
            repository_summary=context.repository_summary,
            retrieved_context=[
                *(context.retrieved_context or []),
                "Validation failures:\n"
                + "\n".join(f"{item['tool']}: {item['output']}" for item in classifications),
            ],
        )
        debugger_message = await self._run_agent_with_retries(
            self._agents[AgentRole.DEBUGGER],
            repair_context,
        )
        task.messages.append(debugger_message)
        self.record_event(
            task.id,
            RuntimeEventType.AGENT_MESSAGE,
            {
                "role": AgentRole.DEBUGGER.value,
                "kind": "repair",
                "content": debugger_message.content,
            },
        )
        validation_tools = [result.name for result in failures if result.name in VALIDATION_TOOLS]
        for tool_name in dict.fromkeys(validation_tools):
            await self._run_tool(
                ToolRequest(
                    task_id=task.id,
                    name=tool_name,
                    arguments={
                        "root": task.request.repository.path,
                        "allow_dry_run_execution": not task.request.dry_run,
                    },
                    dry_run=task.request.dry_run,
                )
            )
        self.record_event(
            task.id,
            RuntimeEventType.AUDIT,
            {
                "kind": "repair.completed",
                "rerun_tools": validation_tools,
                "remaining_failures": len(self._failed_tools(task)),
            },
        )

    async def _run_provider_editing_pass(self, task: CodingTaskState, rel_path: str | None) -> None:
        if rel_path is None:
            self.record_event(
                task.id,
                RuntimeEventType.AUDIT,
                {"kind": "provider_edit.skipped", "reason": "no supported source files"},
            )
            return
        read_request = ToolRequest(
            task_id=task.id,
            name="read_file",
            arguments={"root": task.request.repository.path, "path": rel_path},
            dry_run=task.request.dry_run,
        )
        await self._run_tool(read_request)
        source = task.tool_results[-1].output if task.tool_results else ""
        proposed = _provider_edit_proposal(source, task.request.goal)
        await self._run_tool(
            ToolRequest(
                task_id=task.id,
                name="generate_patch",
                arguments={
                    "root": task.request.repository.path,
                    "path": rel_path,
                    "content": proposed,
                },
                dry_run=True,
            )
        )
        if not task.request.dry_run and source and proposed != source:
            await self._run_tool(
                ToolRequest(
                    task_id=task.id,
                    name="edit_file",
                    arguments={
                        "root": task.request.repository.path,
                        "path": rel_path,
                        "old": source,
                        "new": proposed,
                    },
                    dry_run=False,
                )
            )
        self.record_event(
            task.id,
            RuntimeEventType.AUDIT,
            {
                "kind": "provider_edit.completed",
                "path": rel_path,
                "applied": not task.request.dry_run and proposed != source,
            },
        )

    def _record_validation_gate(self, task: CodingTaskState) -> None:
        validation_results = [
            result for result in task.tool_results if result.name in VALIDATION_TOOLS
        ]
        failed_validation = [
            result for result in validation_results if result.status == ToolStatus.FAILED
        ]
        skipped_validation = [
            result
            for result in validation_results
            if "dry-run" in result.output.lower() or result.data.get("skipped")
        ]
        passed = not failed_validation and not skipped_validation
        explanation = "validation passed"
        if failed_validation:
            explanation = "validation failed; see failure_classifications in the report"
        elif skipped_validation:
            explanation = "validation commands were dry-run or unavailable; no clean test run was proven"
        self.record_event(
            task.id,
            RuntimeEventType.AUDIT,
            {
                "kind": "validation.gate",
                "passed": passed,
                "explanation": explanation,
                "validation_tools": [
                    {"name": result.name, "status": result.status.value}
                    for result in validation_results
                ],
            },
        )

    def _failed_tools(self, task: CodingTaskState):
        return [result for result in task.tool_results if result.status == ToolStatus.FAILED]

    async def _run_agent_with_retries(self, agent, context: AgentContext):
        last_error: Exception | None = None
        for attempt in range(self._policy.retry_budget + 1):
            try:
                message = await agent.run(context)
                if message.content.strip():
                    return message
            except Exception as error:
                last_error = error
            self.record_event(
                context.task.id,
                RuntimeEventType.AGENT_MESSAGE,
                {"role": agent.role.value, "kind": "retry", "attempt": attempt + 1},
            )
        return await StaticAgent(
            agent.role,
            f"{agent.role.value} failed after retries: {last_error or 'empty response'}",
        ).run(context)

    async def _run_tool(self, request: ToolRequest) -> None:
        task = self._require_task(request.task_id)
        self.record_event(
            task.id,
            RuntimeEventType.TOOL_STARTED,
            {"tool": request.name, "arguments": request.arguments},
        )
        result = await self._tools.execute(request)
        task.tool_results.append(result)
        event_type = (
            RuntimeEventType.TOOL_FAILED
            if result.status == ToolStatus.FAILED
            else RuntimeEventType.TOOL_COMPLETED
        )
        self.record_event(
            task.id,
            event_type,
            {"tool": request.name, "status": result.status.value, "output": result.output},
        )
        self._store.save_task(task)

    def record_event(self, task_id: str, event_type: RuntimeEventType, payload: dict) -> None:
        task = self._require_task(task_id)
        event = RuntimeEvent(task_id=task_id, type=event_type, payload=payload)
        task.events.append(event)
        self._store.save_event(event)
        self._store.save_task(task)
        for queue in self._subscribers[task_id]:
            queue.put_nowait(event)

    async def subscribe(self, task_id: str):
        task = self._require_task(task_id)
        queue: asyncio.Queue[RuntimeEvent] = asyncio.Queue()
        self._subscribers[task_id].append(queue)
        for event in task.events:
            yield event
        try:
            while True:
                yield await queue.get()
        finally:
            self._subscribers[task_id].remove(queue)

    def _require_task(self, task_id: str) -> CodingTaskState:
        task = self._tasks.get(task_id)
        if task is None:
            raise KeyError(f"unknown task: {task_id}")
        return task


def _provider_edit_proposal(source: str, goal: str) -> str:
    marker = f"# OctoBot edit intent: {goal[:120]}"
    if marker in source:
        return source
    if source.startswith("//") or "\nfn " in source or "\nuse " in source:
        marker = f"// OctoBot edit intent: {goal[:120]}"
    return f"{marker}\n{source}" if source else marker + "\n"
