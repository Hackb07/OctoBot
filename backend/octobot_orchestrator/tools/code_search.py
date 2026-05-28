from __future__ import annotations

from pathlib import Path

from .base import result_from_data
from .filesystem import WorkspacePolicy
from ..contracts import ToolRequest, ToolResult
from ..indexer.repository import RepositoryIndexer


async def symbol_lookup(request: ToolRequest) -> ToolResult:
    root = request.arguments["root"]
    query = request.arguments["query"].lower()
    index = RepositoryIndexer().index(root)
    matches = [
        symbol.model_dump()
        for file in index.files
        for symbol in file.symbols
        if query in symbol.name.lower()
    ]
    return result_from_data(request, {"matches": matches}, output=f"{len(matches)} symbols")


async def semantic_code_search(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    query = request.arguments["query"].lower()
    limit = int(request.arguments.get("limit", 20))
    terms = [term for term in query.split() if term]
    matches: list[dict[str, object]] = []
    for path in policy.root.rglob("*"):
        if len(matches) >= limit:
            break
        if not path.is_file() or _ignored(path):
            continue
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        lower = text.lower()
        score = sum(1 for term in terms if term in lower)
        if score == 0:
            continue
        preview = next(
            (line.strip() for line in text.splitlines() if any(term in line.lower() for term in terms)),
            "",
        )
        matches.append(
            {
                "path": str(path.relative_to(policy.root)),
                "score": score,
                "preview": preview[:240],
            }
        )
    matches.sort(key=lambda item: int(item["score"]), reverse=True)
    return result_from_data(request, {"matches": matches[:limit]}, output=f"{len(matches[:limit])} matches")


def _ignored(path: Path) -> bool:
    ignored = {".git", ".venv", "__pycache__", "target", "node_modules", ".pytest_cache", ".ruff_cache"}
    return any(part in ignored for part in path.parts)
