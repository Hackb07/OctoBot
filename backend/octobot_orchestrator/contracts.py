from __future__ import annotations

from datetime import UTC, datetime
from enum import StrEnum
from typing import Any, Literal
from uuid import uuid4

from pydantic import BaseModel, Field


def utc_now() -> datetime:
    return datetime.now(UTC)


class AgentRole(StrEnum):
    PLANNER = "planner"
    CODING = "coding"
    REVIEWER = "reviewer"
    DEBUGGER = "debugger"
    RESEARCH = "research"
    SECURITY = "security"


class TaskStatus(StrEnum):
    QUEUED = "queued"
    RUNNING = "running"
    WAITING_FOR_APPROVAL = "waiting_for_approval"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


class ToolStatus(StrEnum):
    STARTED = "started"
    STREAMING = "streaming"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


class RuntimeEventType(StrEnum):
    TASK_CREATED = "task.created"
    TASK_UPDATED = "task.updated"
    AGENT_MESSAGE = "agent.message"
    TOOL_STARTED = "tool.started"
    TOOL_OUTPUT = "tool.output"
    TOOL_COMPLETED = "tool.completed"
    TOOL_FAILED = "tool.failed"
    APPROVAL_REQUIRED = "approval.required"
    REPORT_GENERATED = "report.generated"
    AUDIT = "audit"


class RuntimeCommandKind(StrEnum):
    TERMINAL = "terminal"
    FILESYSTEM = "filesystem"
    GIT = "git"
    SANDBOX = "sandbox"


class RepositoryRef(BaseModel):
    path: str
    branch: str | None = None
    commit: str | None = None


class CodingTaskRequest(BaseModel):
    goal: str = Field(min_length=1)
    repository: RepositoryRef
    dry_run: bool = True
    auto_commit: bool = False
    max_iterations: int = Field(default=5, ge=1, le=20)


class ExecutionPlanStep(BaseModel):
    id: str = Field(default_factory=lambda: f"step-{uuid4().hex[:10]}")
    agent: AgentRole
    title: str
    rationale: str
    depends_on: list[str] = Field(default_factory=list)
    tools: list[str] = Field(default_factory=list)


class ExecutionPlan(BaseModel):
    task_id: str
    summary: str
    steps: list[ExecutionPlanStep]
    risks: list[str] = Field(default_factory=list)


class ToolRequest(BaseModel):
    id: str = Field(default_factory=lambda: f"tool-{uuid4().hex[:10]}")
    task_id: str
    name: str
    arguments: dict[str, Any] = Field(default_factory=dict)
    timeout_seconds: int = Field(default=120, ge=1, le=3600)
    dry_run: bool = True
    cancellation_token: str | None = None


class RuntimeToolEnvelope(BaseModel):
    id: str = Field(default_factory=lambda: f"runtime-{uuid4().hex[:10]}")
    kind: RuntimeCommandKind
    tool: ToolRequest
    workspace_root: str
    requires_approval: bool = False
    resource_limits: dict[str, Any] = Field(default_factory=dict)


class RuntimeStreamChunk(BaseModel):
    envelope_id: str
    sequence: int
    stream: Literal["stdout", "stderr", "event"]
    data: str
    timestamp: datetime = Field(default_factory=utc_now)


class ToolResult(BaseModel):
    id: str
    task_id: str
    name: str
    status: ToolStatus
    output: str = ""
    data: dict[str, Any] = Field(default_factory=dict)
    started_at: datetime = Field(default_factory=utc_now)
    completed_at: datetime | None = None


class GraphExecutionPolicy(BaseModel):
    retry_budget: int = Field(default=2, ge=0, le=10)
    stop_on_failure: bool = False
    confidence_threshold: int = Field(default=60, ge=0, le=100)
    escalation_threshold: int = Field(default=40, ge=0, le=100)


class RuntimeEvent(BaseModel):
    id: str = Field(default_factory=lambda: f"evt-{uuid4().hex[:12]}")
    task_id: str
    type: RuntimeEventType
    payload: dict[str, Any] = Field(default_factory=dict)
    timestamp: datetime = Field(default_factory=utc_now)


class AgentMessage(BaseModel):
    task_id: str
    role: AgentRole
    content: str
    kind: Literal["thought", "action", "observation", "summary"] = "summary"
    timestamp: datetime = Field(default_factory=utc_now)


class CodingTaskState(BaseModel):
    id: str = Field(default_factory=lambda: f"task-{uuid4().hex[:10]}")
    request: CodingTaskRequest
    status: TaskStatus = TaskStatus.QUEUED
    plan: ExecutionPlan | None = None
    messages: list[AgentMessage] = Field(default_factory=list)
    tool_results: list[ToolResult] = Field(default_factory=list)
    events: list[RuntimeEvent] = Field(default_factory=list)
    report: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime = Field(default_factory=utc_now)
    updated_at: datetime = Field(default_factory=utc_now)
