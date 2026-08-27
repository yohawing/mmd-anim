from __future__ import annotations

import json
from pathlib import Path

import pytest

from mmd_oracle_runner import artifacts


def _result() -> dict:
    return {"schemaVersion": 1, "caseFile": "case.json", "artifactName": "case", "inputInventory": {}, "ownedArtifacts": [], "artifacts": {"result": {"exists": False}}}


@pytest.mark.parametrize("marker", [None, "not-json", "[]", json.dumps({"schemaVersion": 2, "ownedArtifacts": []})])
def test_cleanup_preserves_unowned_artifact_twice(tmp_path: Path, marker: str | None):
    scene, result_path = tmp_path / "scene.pmm", tmp_path / "prepare-result.json"
    scene.write_bytes(b"user")
    if marker is not None:
        result_path.write_text(marker, encoding="utf-8")
    result = _result()
    for _ in range(2):
        with pytest.raises(ValueError):
            artifacts.cleanup(result, (scene, result_path), result_path)
        assert scene.read_bytes() == b"user"


def test_collision_and_staging_are_rejected(tmp_path: Path):
    source = tmp_path / "model.mmd-utf16.pmx"
    source.write_bytes(b"input")
    result = _result()
    result["inputInventory"] = {"pmx": {"path": str(source.resolve())}}
    with pytest.raises(ValueError):
        artifacts.cleanup(result, (source,), tmp_path / "prepare-result.json")
    assert source.read_bytes() == b"input"


def test_cleanup_can_preserve_owned_stable_artifacts(tmp_path: Path):
    scene, marker = tmp_path / "scene.pmm", tmp_path / "prepare-result.json"
    scene.write_bytes(b"last-good")
    marker.write_text(json.dumps({**_result(), "ownedArtifacts": [str(scene), str(marker)]}), encoding="utf-8")

    artifacts.cleanup(_result(), (scene, marker), marker, preserve=(scene, marker))

    assert scene.read_bytes() == b"last-good"
    assert marker.exists()


def test_atomic_result_replaces_temp(tmp_path: Path):
    result = _result()
    path, temporary = tmp_path / "prepare-result.json", tmp_path / ".prepare-result.json.tmp"
    artifacts.write_result(path, temporary, result, lambda *_: None)
    assert json.loads(path.read_text(encoding="utf-8"))["ownedArtifacts"] == [str(path)]
    assert not temporary.exists()


def test_atomic_result_removes_owned_temp_after_replace_failure(tmp_path: Path, monkeypatch):
    result = _result()
    path, temporary = tmp_path / "prepare-result.json", tmp_path / ".prepare-result.json.tmp"
    monkeypatch.setattr(artifacts.os, "replace", lambda *_: (_ for _ in ()).throw(OSError("replace failed")))

    artifacts.write_result(path, temporary, result, lambda *_: None)

    assert not path.exists() and not temporary.exists()
    assert result["artifacts"]["result"]["exists"] is False


def test_serialization_failure_does_not_create_temp(tmp_path: Path):
    result = _result()
    result["invalid"] = object()
    path, temporary = tmp_path / "prepare-result.json", tmp_path / ".prepare-result.json.tmp"

    artifacts.write_result(path, temporary, result, lambda *_: None)

    assert not path.exists() and not temporary.exists()
    assert result["ownedArtifacts"] == []


def test_dangling_symlink_is_rejected_when_supported(tmp_path: Path):
    link = tmp_path / "dangling"
    try:
        link.symlink_to(tmp_path / "missing")
    except OSError:
        pytest.skip("symlink creation unavailable")
    with pytest.raises(OSError):
        artifacts.reject_reparse(link / "output")
