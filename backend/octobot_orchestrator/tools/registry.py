from __future__ import annotations

from ..runtime.client import RustRuntimeClient
from .base import CancellationRegistry, ToolRegistry
from .code_search import semantic_code_search, symbol_lookup
from .execution import dependency_scan, execute_terminal, lint_project, run_tests, search_docs
from .filesystem import edit_file, generate_patch, grep_code, list_directory, read_file, write_file
from .git_tools import git_branch, git_commit, git_diff, git_snapshot, pr_summary, rollback_changes


def build_tool_registry(
    runtime_client: RustRuntimeClient | None = None,
    use_runtime: bool | None = None,
    cancellations: CancellationRegistry | None = None,
) -> ToolRegistry:
    registry = ToolRegistry(runtime_client=runtime_client, use_runtime=use_runtime, cancellations=cancellations)
    registry.register("list_directory", list_directory)
    registry.register("read_file", read_file)
    registry.register("write_file", write_file)
    registry.register("edit_file", edit_file)
    registry.register("grep_code", grep_code)
    registry.register("semantic_code_search", semantic_code_search)
    registry.register("symbol_lookup", symbol_lookup)
    registry.register("generate_patch", generate_patch)
    registry.register("execute_terminal", execute_terminal)
    registry.register("run_tests", run_tests)
    registry.register("lint_project", lint_project)
    registry.register("dependency_scan", dependency_scan)
    registry.register("search_docs", search_docs)
    registry.register("git_diff", git_diff)
    registry.register("git_branch", git_branch)
    registry.register("git_commit", git_commit)
    registry.register("rollback_changes", rollback_changes)
    registry.register("git_snapshot", git_snapshot)
    registry.register("pr_summary", pr_summary)
    return registry
