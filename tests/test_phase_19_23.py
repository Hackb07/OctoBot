from pathlib import Path

import pytest

from backend.octobot_orchestrator.agents.graph import AgentGraph
from backend.octobot_orchestrator.contracts import CodingTaskRequest, RepositoryRef
from backend.octobot_orchestrator.indexer.repository import RepositoryIndexer
from backend.octobot_orchestrator.memory.store import MemoryRecord, PersistentMemoryStore
from backend.octobot_orchestrator.rag import ContextRetriever
from backend.octobot_orchestrator.runtime.policy import RuntimePolicy
from backend.octobot_orchestrator.storage import SQLiteTaskStore
from backend.octobot_orchestrator.workflows.autonomous_loop import AutonomousExecutionLoop


def test_runtime_policy_marks_write_tools_for_approval():
    policy = RuntimePolicy() 

    assert policy.requires_approval("edit_file", dry_run=False)
    assert not policy.requires_approval("edit_file", dry_run=True)


def test_repository_indexer_builds_dependency_graph_and_cache(tmp_path: Path):
    (tmp_path / "app.py").write_text("import os\nclass Service:\n    pass\n", encoding="utf-8")
    indexer = RepositoryIndexer(cache_dir=str(tmp_path / ".cache"))

    index = indexer.index(str(tmp_path))
    cached = indexer.index(str(tmp_path))

    assert index.architecture_summary
    assert index.dependency_graph[0].target == "os"
    assert index.files[0].parser in {"regex", "tree-sitter"}
    assert cached.files[0].symbols[0].name == "Service"


def test_persistent_memory_retrieves_context(tmp_path: Path):
    memory = PersistentMemoryStore(str(tmp_path / "memory.sqlite3"))
    memory.add(MemoryRecord(id="m1", text="authentication middleware jwt token", metadata={"task_id": "t1"}))

    results = memory.search("jwt auth", filters={"task_id": "t1"})

    assert results[0].id == "m1"


def test_context_retriever_indexes_repository_summary(tmp_path: Path):
    (tmp_path / "lib.rs").write_text("use std::fs;\nfn load() {}\n", encoding="utf-8")
    index = RepositoryIndexer(cache_dir=str(tmp_path / ".cache")).index(str(tmp_path))
    memory = PersistentMemoryStore(str(tmp_path / "memory.sqlite3"))
    retriever = ContextRetriever(memory)

    retriever.index_repository("task-1", index)
    context = retriever.compressed_context("load fs", "task-1")

    assert "lib.rs" in context or "source files indexed" in context


@pytest.mark.asyncio
async def test_langgraph_plan_has_expected_agent_steps():
    plan = await AgentGraph().plan("task-1", "add jwt authentication")

    assert [step.agent for step in plan.steps]
    assert len(plan.steps) >= 5


@pytest.mark.asyncio
async def test_autonomous_loop_persists_tasks(tmp_path: Path):
    repo = tmp_path / "repo"
    repo.mkdir()
    (repo / "main.py").write_text("def main():\n    return 1\n", encoding="utf-8")
    store = SQLiteTaskStore(str(tmp_path / "tasks.sqlite3"))
    loop = AutonomousExecutionLoop(store=store)

    task = await loop.create_task(CodingTaskRequest(goal="inspect main", repository=RepositoryRef(path=str(repo))))
    await loop.run(task.id)

    loaded = SQLiteTaskStore(str(tmp_path / "tasks.sqlite3")).load_tasks()
    assert task.id in loaded
    assert loaded[task.id].report["task_id"] == task.id
