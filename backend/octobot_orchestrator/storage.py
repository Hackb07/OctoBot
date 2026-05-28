from __future__ import annotations

import json
import sqlite3
from pathlib import Path

from .contracts import CodingTaskState, RuntimeEvent


class SQLiteTaskStore:
    def __init__(self, path: str = ".octobot/orchestrator.sqlite3") -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._init()

    def _connect(self) -> sqlite3.Connection:
        conn = sqlite3.connect(self.path)
        conn.execute("PRAGMA journal_mode=WAL")
        return conn

    def _init(self) -> None:
        with self._connect() as conn:
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS tasks (
                    id TEXT PRIMARY KEY,
                    state_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )
                """
            )
            conn.execute(
                """
                CREATE TABLE IF NOT EXISTS task_events (
                    id TEXT PRIMARY KEY,
                    task_id TEXT NOT NULL,
                    event_json TEXT NOT NULL,
                    timestamp TEXT NOT NULL
                )
                """
            )

    def save_task(self, task: CodingTaskState) -> None:
        with self._connect() as conn:
            conn.execute(
                """
                INSERT INTO tasks (id, state_json, updated_at)
                VALUES (?, ?, ?)
                ON CONFLICT(id) DO UPDATE SET
                    state_json = excluded.state_json,
                    updated_at = excluded.updated_at
                """,
                (task.id, task.model_dump_json(), task.updated_at.isoformat()),
            )

    def load_tasks(self) -> dict[str, CodingTaskState]:
        with self._connect() as conn:
            rows = conn.execute("SELECT state_json FROM tasks").fetchall()
        return {
            task.id: task
            for task in (CodingTaskState.model_validate_json(row[0]) for row in rows)
        }

    def save_event(self, event: RuntimeEvent) -> None:
        with self._connect() as conn:
            conn.execute(
                """
                INSERT OR IGNORE INTO task_events (id, task_id, event_json, timestamp)
                VALUES (?, ?, ?, ?)
                """,
                (event.id, event.task_id, event.model_dump_json(), event.timestamp.isoformat()),
            )

    def load_events(self, task_id: str) -> list[RuntimeEvent]:
        with self._connect() as conn:
            rows = conn.execute(
                "SELECT event_json FROM task_events WHERE task_id = ? ORDER BY timestamp ASC",
                (task_id,),
            ).fetchall()
        return [RuntimeEvent.model_validate(json.loads(row[0])) for row in rows]
