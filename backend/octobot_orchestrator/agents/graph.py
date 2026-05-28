from __future__ import annotations

from typing import TypedDict

from ..contracts import AgentRole, ExecutionPlan, ExecutionPlanStep


class AgentGraph:
    """LangGraph-compatible orchestration boundary.

    The first implementation produces a deterministic plan. The class is kept
    isolated so LangGraph nodes can replace this planner without changing API
    contracts or tool execution.
    """

    async def plan(self, task_id: str, goal: str) -> ExecutionPlan:
        try:
            return await self._langgraph_plan(task_id, goal)
        except Exception:
            return self._deterministic_plan(task_id, goal)

    async def _langgraph_plan(self, task_id: str, goal: str) -> ExecutionPlan:
        from langgraph.graph import END, StateGraph

        class PlanState(TypedDict):
            task_id: str
            goal: str
            steps: list[ExecutionPlanStep]
            risks: list[str]

        async def research_node(state: PlanState) -> PlanState:
            state["steps"].append(
                ExecutionPlanStep(
                    agent=AgentRole.RESEARCH,
                    title="Index and summarize repository",
                    rationale="The coding agents need repository structure before editing.",
                    tools=["list_directory", "semantic_code_search", "search_docs", "dependency_scan"],
                )
            )
            return state

        async def plan_node(state: PlanState) -> PlanState:
            state["steps"].append(
                ExecutionPlanStep(
                    agent=AgentRole.PLANNER,
                    title="Create implementation plan",
                    rationale="Break the request into safe, reviewable code changes.",
                    tools=["symbol_lookup"],
                )
            )
            return state

        async def code_node(state: PlanState) -> PlanState:
            state["steps"].append(
                ExecutionPlanStep(
                    agent=AgentRole.CODING,
                    title="Apply code changes",
                    rationale="Edit the smallest relevant file set.",
                    tools=["read_file", "edit_file", "generate_patch"],
                )
            )
            return state

        async def validate_node(state: PlanState) -> PlanState:
            state["steps"].append(
                ExecutionPlanStep(
                    agent=AgentRole.DEBUGGER,
                    title="Run validation and repair failures",
                    rationale="Tests and linters determine whether the change is correct.",
                    tools=["run_tests", "lint_project"],
                )
            )
            return state

        async def review_node(state: PlanState) -> PlanState:
            state["steps"].append(
                ExecutionPlanStep(
                    agent=AgentRole.REVIEWER,
                    title="Review diff and summarize",
                    rationale="Final review catches regressions and documents the change.",
                    tools=["git_diff", "git_snapshot", "pr_summary"],
                )
            )
            return state

        graph = StateGraph(PlanState)
        graph.add_node("research", research_node)
        graph.add_node("plan", plan_node)
        graph.add_node("code", code_node)
        graph.add_node("validate", validate_node)
        graph.add_node("review", review_node)
        graph.set_entry_point("research")
        graph.add_edge("research", "plan")
        graph.add_edge("plan", "code")
        graph.add_edge("code", "validate")
        graph.add_edge("validate", "review")
        graph.add_edge("review", END)
        compiled = graph.compile()
        result = await compiled.ainvoke(
            {
                "task_id": task_id,
                "goal": goal,
                "steps": [],
                "risks": [
                    "Provider-backed reasoning depends on configured model providers.",
                    "Filesystem write tools default to dry-run until approval policy allows writes.",
                ],
            }
        )
        return ExecutionPlan(
            task_id=task_id,
            summary=f"LangGraph autonomous execution plan for: {goal}",
            steps=result["steps"],
            risks=result["risks"],
        )

    def _deterministic_plan(self, task_id: str, goal: str) -> ExecutionPlan:
        steps = [
            ExecutionPlanStep(
                agent=AgentRole.RESEARCH,
                title="Index and summarize repository",
                rationale="The coding agents need repository structure before editing.",
                tools=["list_directory", "semantic_code_search", "search_docs", "dependency_scan"],
            ),
            ExecutionPlanStep(
                agent=AgentRole.PLANNER,
                title="Create implementation plan",
                rationale="Break the request into safe, reviewable code changes.",
                tools=["symbol_lookup"],
            ),
            ExecutionPlanStep(
                agent=AgentRole.CODING,
                title="Apply code changes",
                rationale="Edit the smallest relevant file set.",
                tools=["read_file", "edit_file", "generate_patch"],
            ),
            ExecutionPlanStep(
                agent=AgentRole.DEBUGGER,
                title="Run validation and repair failures",
                rationale="Tests and linters determine whether the change is correct.",
                tools=["run_tests", "lint_project"],
            ),
            ExecutionPlanStep(
                agent=AgentRole.REVIEWER,
                title="Review diff and summarize",
                rationale="Final review catches regressions and documents the change.",
                tools=["git_diff", "git_snapshot", "pr_summary"],
            ),
        ]
        return ExecutionPlan(
            task_id=task_id,
            summary=f"Autonomous execution plan for: {goal}",
            steps=steps,
            risks=[
                "Provider-backed reasoning is not wired yet.",
                "Filesystem write tools default to dry-run until approval policy is expanded.",
            ],
        )
