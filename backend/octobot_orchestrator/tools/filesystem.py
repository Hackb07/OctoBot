from __future__ import annotations

import fnmatch
import difflib
from pathlib import Path

from .base import result_from_data
from ..contracts import ToolRequest, ToolResult, ToolStatus


class WorkspacePolicy:
    def __init__(self, root: str) -> None:
        self.root = Path(root).resolve()

    def resolve(self, path: str) -> Path:
        target = (self.root / path).resolve()
        if target != self.root and self.root not in target.parents:
            raise ValueError(f"path escapes workspace: {path}")
        return target


async def list_directory(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    rel_path = request.arguments.get("path", ".")
    target = policy.resolve(rel_path)
    if not target.is_dir():
        return ToolResult(
            id=request.id,
            task_id=request.task_id,
            name=request.name,
            status=ToolStatus.FAILED,
            output=f"not a directory: {rel_path}",
        )
    entries = [
        {"name": entry.name, "type": "dir" if entry.is_dir() else "file"}
        for entry in sorted(target.iterdir(), key=lambda p: p.name)
        if entry.name not in {".git", "target", "__pycache__", ".venv"}
    ]
    return result_from_data(request, {"entries": entries})


async def read_file(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    target = policy.resolve(request.arguments["path"])
    if not target.is_file():
        return ToolResult(
            id=request.id,
            task_id=request.task_id,
            name=request.name,
            status=ToolStatus.FAILED,
            output=f"not a file: {request.arguments['path']}",
        )
    text = target.read_text(encoding="utf-8", errors="replace")
    return result_from_data(request, {"path": str(target), "content": text}, output=text)


async def write_file(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    target = policy.resolve(request.arguments["path"])
    content = request.arguments.get("content", "")
    if request.dry_run:
        return result_from_data(
            request,
            {"path": str(target), "bytes": len(content.encode()), "dry_run": True},
            output="dry-run: write skipped",
        )
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")
    return result_from_data(request, {"path": str(target), "bytes": len(content.encode())})


async def edit_file(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    target = policy.resolve(request.arguments["path"])
    old = request.arguments["old"]
    new = request.arguments["new"]
    replace_all = bool(request.arguments.get("replace_all", False))
    if not target.is_file():
        return ToolResult(
            id=request.id,
            task_id=request.task_id,
            name=request.name,
            status=ToolStatus.FAILED,
            output=f"not a file: {request.arguments['path']}",
        )
    original = target.read_text(encoding="utf-8", errors="replace")
    if old not in original:
        return ToolResult(
            id=request.id,
            task_id=request.task_id,
            name=request.name,
            status=ToolStatus.FAILED,
            output="edit target text was not found",
        )
    updated = original.replace(old, new) if replace_all else original.replace(old, new, 1)
    diff = "".join(
        difflib.unified_diff(
            original.splitlines(keepends=True),
            updated.splitlines(keepends=True),
            fromfile=f"a/{request.arguments['path']}",
            tofile=f"b/{request.arguments['path']}",
        )
    )
    if request.dry_run:
        return result_from_data(
            request,
            {"path": str(target), "diff": diff, "dry_run": True},
            output=diff,
        )
    target.write_text(updated, encoding="utf-8")
    return result_from_data(request, {"path": str(target), "diff": diff}, output=diff)


async def generate_patch(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    target = policy.resolve(request.arguments["path"])
    proposed = request.arguments["content"]
    original = target.read_text(encoding="utf-8", errors="replace") if target.exists() else ""
    diff = "".join(
        difflib.unified_diff(
            original.splitlines(keepends=True),
            proposed.splitlines(keepends=True),
            fromfile=f"a/{request.arguments['path']}",
            tofile=f"b/{request.arguments['path']}",
        )
    )
    return result_from_data(request, {"path": str(target), "diff": diff}, output=diff)


async def grep_code(request: ToolRequest) -> ToolResult:
    policy = WorkspacePolicy(request.arguments["root"])
    pattern = request.arguments["pattern"]
    glob = request.arguments.get("glob", "*")
    limit = int(request.arguments.get("limit", 100))
    matches: list[dict[str, object]] = []
    for path in policy.root.rglob("*"):
        if len(matches) >= limit:
            break
        if not path.is_file() or ".git" in path.parts or "target" in path.parts:
            continue
        if not fnmatch.fnmatch(path.name, glob):
            continue
        try:
            for line_no, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
                if pattern in line:
                    matches.append(
                        {
                            "path": str(path.relative_to(policy.root)),
                            "line": line_no,
                            "text": line.strip(),
                        }
                    )
                    if len(matches) >= limit:
                        break
        except UnicodeDecodeError:
            continue
    return result_from_data(request, {"matches": matches}, output=f"{len(matches)} matches")
