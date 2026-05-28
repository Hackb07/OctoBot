from __future__ import annotations

from .indexer.repository import RepositoryIndex
from .memory.store import MemoryRecord, PersistentMemoryStore, compress_context


class ContextRetriever:
    def __init__(self, memory: PersistentMemoryStore) -> None:
        self.memory = memory

    def index_repository(self, task_id: str, repo_index: RepositoryIndex) -> None:
        self.memory.add(
            MemoryRecord(
                id=f"{task_id}:architecture",
                text=repo_index.architecture_summary,
                metadata={"task_id": task_id, "kind": "architecture"},
            )
        )
        for file in repo_index.files:
            symbol_text = ", ".join(f"{symbol.kind} {symbol.name}" for symbol in file.symbols)
            edge_text = ", ".join(f"{edge.kind}:{edge.target}" for edge in [*file.imports, *file.calls])
            self.memory.add(
                MemoryRecord(
                    id=f"{task_id}:file:{file.path}",
                    text=f"{file.path} ({file.language}) symbols: {symbol_text}; edges: {edge_text}",
                    metadata={"task_id": task_id, "kind": "file", "path": file.path},
                )
            )

    def retrieve(self, query: str, task_id: str, limit: int = 8) -> list[str]:
        records = self.memory.search(query, limit=limit, filters={"task_id": task_id})
        expanded = list(records)
        paths = {record.metadata.get("path") for record in records if record.metadata.get("path")}
        for path in paths:
            expanded.extend(self.memory.search(path or "", limit=3, filters={"task_id": task_id}))
        seen = set()
        chunks = []
        for record in expanded:
            if record.id in seen:
                continue
            seen.add(record.id)
            chunks.append(record.text)
        return chunks

    def compressed_context(self, query: str, task_id: str, max_chars: int = 6000) -> str:
        return compress_context(self.retrieve(query, task_id), max_chars=max_chars)
