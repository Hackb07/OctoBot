from __future__ import annotations

from dataclasses import dataclass

from ..contracts import AgentMessage, AgentRole, CodingTaskState
from ..providers import ModelRouter


@dataclass(frozen=True)
class AgentContext:
    task: CodingTaskState
    repository_summary: str = ""
    retrieved_context: list[str] | None = None


class AutonomousAgent:
    role: AgentRole

    def __init__(self, role: AgentRole) -> None:
        self.role = role

    async def run(self, context: AgentContext) -> AgentMessage:
        raise NotImplementedError


class StaticAgent(AutonomousAgent):
    """Deterministic scaffold agent used until provider-backed prompts are wired in."""

    def __init__(self, role: AgentRole, summary_template: str) -> None:
        super().__init__(role)
        self.summary_template = summary_template

    async def run(self, context: AgentContext) -> AgentMessage:
        return AgentMessage(
            task_id=context.task.id,
            role=self.role,
            kind="summary",
            content=self.summary_template.format(goal=context.task.request.goal),
        )


class ProviderBackedAgent(AutonomousAgent):
    def __init__(self, role: AgentRole, router: ModelRouter) -> None:
        super().__init__(role)
        self.router = router

    async def run(self, context: AgentContext) -> AgentMessage:
        prompt = (
            f"Task: {context.task.request.goal}\n"
            f"Repository summary: {context.repository_summary}\n"
            f"Relevant context:\n{chr(10).join(context.retrieved_context or [])}\n"
            f"Respond as the {self.role.value} agent with a concise execution summary."
        )
        try:
            content = await self.router.complete(
                prompt,
                system=f"You are OctoBot's {self.role.value} autonomous software engineering agent.",
            )
        except Exception:
            content = f"{self.role.value} provider unavailable; scaffold summary used."
        return AgentMessage(task_id=context.task.id, role=self.role, kind="summary", content=content)
