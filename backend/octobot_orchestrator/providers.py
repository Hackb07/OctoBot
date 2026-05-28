from __future__ import annotations

import os
import json
from collections.abc import AsyncIterator
from dataclasses import dataclass

import httpx


@dataclass(frozen=True)
class ModelRequest:
    prompt: str
    system: str = ""
    model: str | None = None
    temperature: float = 0.2


class ModelProvider:
    name: str

    async def stream(self, request: ModelRequest) -> AsyncIterator[str]:
        raise NotImplementedError


class OllamaProvider(ModelProvider):
    name = "ollama"

    def __init__(self, base_url: str | None = None, model: str | None = None) -> None:
        self.base_url = (base_url or os.getenv("OCTOBOT_OLLAMA_URL") or "http://127.0.0.1:11434").rstrip("/")
        self.model = model or os.getenv("OCTOBOT_OLLAMA_MODEL") or "llama3.1:8b"

    async def stream(self, request: ModelRequest) -> AsyncIterator[str]:
        payload = {
            "model": request.model or self.model,
            "prompt": request.prompt,
            "system": request.system,
            "stream": True,
            "options": {"temperature": request.temperature},
        }
        async with httpx.AsyncClient(timeout=120) as client:
            async with client.stream("POST", f"{self.base_url}/api/generate", json=payload) as response:
                response.raise_for_status()
                async for line in response.aiter_lines():
                    if not line:
                        continue
                    data = json.loads(line)
                    token = data.get("response")
                    if token:
                        yield token


class OpenAIProvider(ModelProvider):
    name = "openai"

    async def stream(self, request: ModelRequest) -> AsyncIterator[str]:
        try:
            from openai import AsyncOpenAI
        except ImportError as error:
            raise RuntimeError("openai extra is not installed") from error
        client = AsyncOpenAI(api_key=os.getenv("OPENAI_API_KEY"))
        stream = await client.chat.completions.create(
            model=request.model or os.getenv("OCTOBOT_OPENAI_MODEL", "gpt-4.1-mini"),
            messages=[
                {"role": "system", "content": request.system},
                {"role": "user", "content": request.prompt},
            ],
            temperature=request.temperature,
            stream=True,
        )
        async for chunk in stream:
            token = chunk.choices[0].delta.content
            if token:
                yield token


class GroqProvider(ModelProvider):
    name = "groq"

    async def stream(self, request: ModelRequest) -> AsyncIterator[str]:
        try:
            from openai import AsyncOpenAI
        except ImportError as error:
            raise RuntimeError("openai extra is not installed") from error
        client = AsyncOpenAI(
            api_key=os.getenv("OCTOBOT_GROQ_API_KEY"),
            base_url=os.getenv("OCTOBOT_GROQ_BASE_URL", "https://api.groq.com/openai/v1"),
        )
        stream = await client.chat.completions.create(
            model=request.model or os.getenv("OCTOBOT_GROQ_MODEL", "llama-3.1-8b-instant"),
            messages=[
                {"role": "system", "content": request.system},
                {"role": "user", "content": request.prompt},
            ],
            temperature=request.temperature,
            stream=True,
        )
        async for chunk in stream:
            token = chunk.choices[0].delta.content
            if token:
                yield token


class AnthropicProvider(ModelProvider):
    name = "anthropic"

    async def stream(self, request: ModelRequest) -> AsyncIterator[str]:
        try:
            from anthropic import AsyncAnthropic
        except ImportError as error:
            raise RuntimeError("anthropic extra is not installed") from error
        client = AsyncAnthropic(api_key=os.getenv("ANTHROPIC_API_KEY"))
        async with client.messages.stream(
            model=request.model or os.getenv("OCTOBOT_ANTHROPIC_MODEL", "claude-sonnet-4-5"),
            max_tokens=2048,
            system=request.system,
            messages=[{"role": "user", "content": request.prompt}],
        ) as stream:
            async for text in stream.text_stream:
                yield text


class ModelRouter:
    def __init__(self) -> None:
        self.providers: dict[str, ModelProvider] = {
            "ollama": OllamaProvider(),
            "openai": OpenAIProvider(),
            "anthropic": AnthropicProvider(),
            "groq": GroqProvider(),
        }

    def provider_for(self, preferred: str | None = None) -> ModelProvider:
        name = preferred or os.getenv("OCTOBOT_DEFAULT_PROVIDER", "ollama")
        if name not in self.providers:
            raise ValueError(f"unknown model provider: {name}")
        return self.providers[name]

    async def complete(self, prompt: str, system: str = "", provider: str | None = None) -> str:
        chunks = []
        async for chunk in self.provider_for(provider).stream(ModelRequest(prompt=prompt, system=system)):
            chunks.append(chunk)
        return "".join(chunks)
