from __future__ import annotations

import hashlib
import math
import os
import sqlite3
from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class MemoryRecord:
    id: str
    text: str
    metadata: dict[str, str] = field(default_factory=dict)


class InMemoryVectorStore:
    """Small deterministic memory facade until ChromaDB/FAISS is wired in."""

    def __init__(self) -> None:
        self._records: list[MemoryRecord] = []

    def add(self, record: MemoryRecord) -> None:
        self._records.append(record)

    def search(self, query: str, limit: int = 5) -> list[MemoryRecord]:
        terms = {term.lower() for term in query.split() if term.strip()}
        scored: list[tuple[int, MemoryRecord]] = []
        for record in self._records:
            haystack = record.text.lower()
            score = sum(1 for term in terms if term in haystack)
            if score > 0:
                scored.append((score, record))
        return [record for _, record in sorted(scored, key=lambda item: item[0], reverse=True)[:limit]]


class PersistentMemoryStore:
    def __init__(self, path: str = ".octobot/memory.sqlite3", embedding_provider: "EmbeddingProvider | None" = None) -> None:
        self.path = Path(path)
        self.embedding_provider = embedding_provider or build_embedding_provider()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._init()

    def _connect(self) -> sqlite3.Connection:
        return sqlite3.connect(self.path)

    def _init(self) -> None:
        with self._connect() as conn:
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS memory_records (
                    id TEXT PRIMARY KEY,
                    text TEXT NOT NULL,
                    metadata_json TEXT NOT NULL,
                    embedding TEXT NOT NULL
                )
                """
            )

    def add(self, record: MemoryRecord) -> None:
        import json

        embedding = self.embedding_provider.embed(record.text)
        with self._connect() as conn:
            conn.execute(
                """
                INSERT INTO memory_records (id, text, metadata_json, embedding)
                VALUES (?, ?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    text = excluded.text,
                    metadata_json = excluded.metadata_json,
                    embedding = excluded.embedding
                """,
                (record.id, record.text, json.dumps(record.metadata), json.dumps(embedding)),
            )

    def search(self, query: str, limit: int = 5, filters: dict[str, str] | None = None) -> list[MemoryRecord]:
        import json

        query_embedding = self.embedding_provider.embed(query)
        with self._connect() as conn:
            rows = conn.execute("SELECT id, text, metadata_json, embedding FROM memory_records").fetchall()
        scored: list[tuple[float, MemoryRecord]] = []
        for row in rows:
            metadata = json.loads(row[2])
            if filters and any(metadata.get(key) != value for key, value in filters.items()):
                continue
            score = cosine(query_embedding, json.loads(row[3]))
            scored.append((score, MemoryRecord(id=row[0], text=row[1], metadata=metadata)))
        return [record for _, record in sorted(scored, key=lambda item: item[0], reverse=True)[:limit]]


def deterministic_embedding(text: str, dimensions: int = 96) -> list[float]:
    vector = [0.0] * dimensions
    for token in text.lower().split():
        digest = hashlib.sha256(token.encode()).digest()
        idx = int.from_bytes(digest[:2], "big") % dimensions
        sign = 1.0 if digest[2] % 2 == 0 else -1.0
        vector[idx] += sign
    norm = math.sqrt(sum(value * value for value in vector)) or 1.0
    return [value / norm for value in vector]


class EmbeddingProvider:
    name = "base"

    def embed(self, text: str) -> list[float]:
        raise NotImplementedError


class DeterministicEmbeddingProvider(EmbeddingProvider):
    name = "deterministic"

    def embed(self, text: str) -> list[float]:
        return deterministic_embedding(text)


class SentenceTransformerEmbeddingProvider(EmbeddingProvider):
    name = "sentence-transformers"

    def __init__(self, model_name: str | None = None) -> None:
        from sentence_transformers import SentenceTransformer

        self.model = SentenceTransformer(model_name or os.getenv("OCTOBOT_EMBEDDING_MODEL", "all-MiniLM-L6-v2"))

    def embed(self, text: str) -> list[float]:
        vector = self.model.encode(text, normalize_embeddings=True)
        return [float(value) for value in vector]


class ChromaMemoryStore:
    def __init__(self, path: str = ".octobot/chroma", collection: str = "octobot_code_memory") -> None:
        import chromadb

        self.client = chromadb.PersistentClient(path=path)
        self.collection = self.client.get_or_create_collection(collection)
        self.embedding_provider = build_embedding_provider()

    def add(self, record: MemoryRecord) -> None:
        self.collection.upsert(
            ids=[record.id],
            documents=[record.text],
            metadatas=[record.metadata],
            embeddings=[self.embedding_provider.embed(record.text)],
        )

    def search(self, query: str, limit: int = 5, filters: dict[str, str] | None = None) -> list[MemoryRecord]:
        result = self.collection.query(
            query_embeddings=[self.embedding_provider.embed(query)],
            n_results=limit,
            where=filters or None,
        )
        ids = result.get("ids", [[]])[0]
        docs = result.get("documents", [[]])[0]
        metas = result.get("metadatas", [[]])[0]
        return [
            MemoryRecord(id=str(record_id), text=doc or "", metadata=dict(meta or {}))
            for record_id, doc, meta in zip(ids, docs, metas, strict=False)
        ]


def build_embedding_provider() -> EmbeddingProvider:
    if os.getenv("OCTOBOT_EMBEDDING_PROVIDER") == "sentence-transformers":
        try:
            return SentenceTransformerEmbeddingProvider()
        except Exception:
            return DeterministicEmbeddingProvider()
    return DeterministicEmbeddingProvider()


def build_memory_store():
    backend = os.getenv("OCTOBOT_MEMORY_BACKEND", "sqlite")
    if backend == "chroma":
        try:
            return ChromaMemoryStore()
        except Exception:
            return PersistentMemoryStore()
    return PersistentMemoryStore()


def cosine(left: list[float], right: list[float]) -> float:
    return sum(a * b for a, b in zip(left, right, strict=False))


def compress_context(chunks: list[str], max_chars: int = 6000) -> str:
    output: list[str] = []
    used = 0
    for chunk in chunks:
        clean = " ".join(chunk.split())
        remaining = max_chars - used
        if remaining <= 0:
            break
        if len(clean) > remaining:
            clean = clean[: max(0, remaining - 20)] + "...[truncated]"
        output.append(clean)
        used += len(clean)
    return "\n\n".join(output)
