from __future__ import annotations

import json
from collections.abc import AsyncIterator

import httpx
import websockets

from ..contracts import RuntimeEvent, ToolRequest


class RustRuntimeClient:
    def __init__(self, base_ws_url: str = "ws://127.0.0.1:7879") -> None:
        self.base_ws_url = base_ws_url.rstrip("/")

    async def stream_tool(self, request: ToolRequest) -> AsyncIterator[RuntimeEvent]:
        uri = f"{self.base_ws_url}/runtime/tools/{request.name}"
        async with websockets.connect(uri) as websocket:
            await websocket.send(request.model_dump_json())
            async for raw in websocket:
                yield RuntimeEvent.model_validate(json.loads(raw))


class OctoBotControlClient:
    def __init__(self, base_url: str = "http://127.0.0.1:7878") -> None:
        self.base_url = base_url.rstrip("/")

    async def health(self) -> dict:
        async with httpx.AsyncClient(timeout=10) as client:
            response = await client.get(f"{self.base_url}/health")
            response.raise_for_status()
            return response.json()

    async def state(self) -> dict:
        async with httpx.AsyncClient(timeout=20) as client:
            response = await client.get(f"{self.base_url}/api/state")
            response.raise_for_status()
            return response.json()

    async def replay_events(self) -> dict:
        async with httpx.AsyncClient(timeout=20) as client:
            response = await client.get(f"{self.base_url}/api/replay/events")
            response.raise_for_status()
            return response.json()
