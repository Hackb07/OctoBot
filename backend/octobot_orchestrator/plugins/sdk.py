from __future__ import annotations

import hashlib
import hmac
import json
import re
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, Field


PluginPermission = Literal[
    "tool:execute",
    "model:provide",
    "index:read",
    "index:write",
    "memory:read",
    "memory:write",
    "workflow:register",
    "policy:evaluate",
    "frontend:panel",
]


class PluginManifest(BaseModel):
    name: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{1,62}$")
    version: str = Field(pattern=r"^\d+\.\d+\.\d+$")
    kind: Literal["tool", "model_provider", "indexer", "retriever", "workflow", "policy", "frontend"]
    entrypoint: str
    description: str = ""
    permissions: list[PluginPermission] = Field(default_factory=list)
    compatibility: dict[str, str] = Field(default_factory=lambda: {"octobot": ">=0.1.0"})
    signed: bool = False
    signature: str | None = None


class PluginScaffold(BaseModel):
    manifest_path: str
    entrypoint_path: str
    test_path: str


class PluginLockRecord(BaseModel):
    name: str
    version: str
    kind: str
    digest: str
    signature: str | None = None


DEFAULT_PERMISSIONS: dict[str, list[PluginPermission]] = {
    "tool": ["tool:execute"],
    "model_provider": ["model:provide"],
    "indexer": ["index:read", "index:write"],
    "retriever": ["memory:read"],
    "workflow": ["workflow:register"],
    "policy": ["policy:evaluate"],
    "frontend": ["frontend:panel"],
}


def validate_manifest(data: dict) -> PluginManifest:
    manifest = PluginManifest.model_validate(data)
    expected = set(DEFAULT_PERMISSIONS[manifest.kind])
    granted = set(manifest.permissions)
    if not expected.issubset(granted):
        missing = ", ".join(sorted(expected - granted))
        raise ValueError(f"manifest missing required permissions for {manifest.kind}: {missing}")
    if not _safe_entrypoint(manifest.entrypoint):
        raise ValueError("manifest entrypoint must be a relative .py, .js, .ts, or .sh path")
    if manifest.signed and not manifest.signature:
        raise ValueError("signed manifest requires a signature")
    return manifest


def sign_manifest(data: dict, key: str) -> PluginManifest:
    unsigned = dict(data)
    unsigned["signed"] = True
    unsigned.pop("signature", None)
    manifest = validate_manifest({**unsigned, "signature": "pending"})
    canonical = {k: v for k, v in manifest.model_dump().items() if k != "signature"}
    unsigned["signature"] = manifest_signature(canonical, key)
    return validate_manifest(unsigned)


def verify_manifest_signature(data: dict, key: str) -> bool:
    manifest = validate_manifest(data)
    if not manifest.signed or not manifest.signature:
        return False
    expected = manifest_signature(
        {k: v for k, v in manifest.model_dump().items() if k != "signature"},
        key,
    )
    return hmac.compare_digest(expected, manifest.signature)


def manifest_signature(data: dict, key: str) -> str:
    payload = json.dumps(data, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hmac.new(key.encode("utf-8"), payload, hashlib.sha256).hexdigest()


def scaffold_plugin(root: str | Path, name: str, kind: str) -> PluginScaffold:
    plugin_root = Path(root) / name
    manifest = validate_manifest(
        {
            "name": name,
            "version": "0.1.0",
            "kind": kind,
            "entrypoint": "plugin.py",
            "description": f"{name} {kind} plugin",
            "permissions": DEFAULT_PERMISSIONS[kind],
        }
    )
    plugin_root.mkdir(parents=True, exist_ok=True)
    manifest_path = plugin_root / "plugin.json"
    entrypoint_path = plugin_root / manifest.entrypoint
    test_path = plugin_root / "test_plugin.py"
    manifest_path.write_text(
        json.dumps(manifest.model_dump(), indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    entrypoint_path.write_text(
        "def run(payload):\n"
        "    return {\"status\": \"ok\", \"payload\": payload}\n",
        encoding="utf-8",
    )
    test_path.write_text(
        "from plugin import run\n\n\n"
        "def test_plugin_run():\n"
        "    assert run({\"ping\": True})[\"status\"] == \"ok\"\n",
        encoding="utf-8",
    )
    return PluginScaffold(
        manifest_path=str(manifest_path),
        entrypoint_path=str(entrypoint_path),
        test_path=str(test_path),
    )


def lock_manifest(data: dict) -> PluginLockRecord:
    manifest = validate_manifest(data)
    digest = hashlib.sha256(
        json.dumps(manifest.model_dump(), sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    return PluginLockRecord(
        name=manifest.name,
        version=manifest.version,
        kind=manifest.kind,
        digest=digest,
        signature=manifest.signature,
    )


def _safe_entrypoint(entrypoint: str) -> bool:
    path = Path(entrypoint)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and re.search(r"\.(py|js|ts|sh)$", entrypoint) is not None
    )
