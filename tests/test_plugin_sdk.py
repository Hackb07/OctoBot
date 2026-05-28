import json
from pathlib import Path

import pytest

from backend.octobot_orchestrator.plugins.sdk import (
    lock_manifest,
    scaffold_plugin,
    sign_manifest,
    validate_manifest,
    verify_manifest_signature,
)


def test_plugin_sdk_validates_permissions_and_entrypoint():
    manifest = validate_manifest(
        {
            "name": "docs-tool",
            "version": "0.1.0",
            "kind": "tool",
            "entrypoint": "plugin.py",
            "permissions": ["tool:execute"],
        }
    )

    assert manifest.name == "docs-tool"


def test_plugin_sdk_rejects_missing_required_permission():
    with pytest.raises(ValueError, match="missing required permissions"):
        validate_manifest(
            {
                "name": "bad-tool",
                "version": "0.1.0",
                "kind": "tool",
                "entrypoint": "plugin.py",
                "permissions": [],
            }
        )


def test_plugin_sdk_scaffolds_manifest_entrypoint_and_test(tmp_path: Path):
    scaffold = scaffold_plugin(tmp_path, "sample-tool", "tool")
    manifest = json.loads(Path(scaffold.manifest_path).read_text(encoding="utf-8"))

    assert manifest["permissions"] == ["tool:execute"]
    assert Path(scaffold.entrypoint_path).exists()
    assert Path(scaffold.test_path).exists()


def test_plugin_sdk_signs_and_verifies_manifest():
    manifest = {
        "name": "signed-tool",
        "version": "0.1.0",
        "kind": "tool",
        "entrypoint": "plugin.py",
        "permissions": ["tool:execute"],
    }

    signed = sign_manifest(manifest, "secret")

    assert signed.signed is True
    assert signed.signature
    assert verify_manifest_signature(signed.model_dump(), "secret")
    assert not verify_manifest_signature(signed.model_dump(), "wrong")


def test_plugin_sdk_creates_version_lock_record():
    manifest = validate_manifest(
        {
            "name": "locked-tool",
            "version": "1.2.3",
            "kind": "tool",
            "entrypoint": "plugin.py",
            "permissions": ["tool:execute"],
        }
    )

    lock = lock_manifest(manifest.model_dump())

    assert lock.name == "locked-tool"
    assert lock.version == "1.2.3"
    assert len(lock.digest) == 64
