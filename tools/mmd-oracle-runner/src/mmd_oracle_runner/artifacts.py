"""Narrow safety helpers for prepare artifacts; not a general storage API."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any


def exists(path: Path) -> bool:
    return path.exists() and path.is_file()


def reject_reparse(*paths: Path) -> None:
    for path in paths:
        probe = path
        while True:
            if os.path.lexists(probe):
                stat = os.lstat(probe)
                if probe.is_symlink() or getattr(stat, "st_file_attributes", 0) & 0x400:
                    raise OSError(f"reparse point is not allowed in artifact path: {probe}")
            if probe.parent == probe:
                break
            probe = probe.parent


def cleanup(result: dict[str, Any], paths: tuple[Path, ...], marker: Path) -> None:
    inputs = {_normalized(entry["path"]) for entry in result["inputInventory"].values()}
    if any(_normalized(path) in inputs for path in paths):
        raise ValueError("artifact path collides with an input path")
    existing = [path for path in paths if path.exists()]
    if not existing:
        return
    if not marker.is_file():
        raise ValueError("existing artifacts require an owned prepare-result marker")
    try:
        prior = json.loads(marker.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"invalid prepare-result marker: {error}") from error
    if not isinstance(prior, dict):
        raise ValueError("prepare-result marker must be a JSON object")
    if any(prior.get(key) != result[key] for key in ("schemaVersion", "caseFile", "artifactName")):
        raise ValueError("prepare-result marker does not own existing artifacts")
    owned = prior.get("ownedArtifacts")
    if not isinstance(owned, list) or any(not isinstance(path, str) for path in owned) or any(str(path) not in owned for path in existing):
        raise ValueError("prepare-result marker does not own every existing artifact")
    for path in existing:
        if not path.is_file():
            raise OSError(f"owned artifact path is not a file: {path}")
        path.unlink()


def record(result: dict[str, Any], paths: tuple[Path, ...]) -> None:
    result["ownedArtifacts"] = [str(path) for path in paths if exists(path)]


def write_result(path: Path, temporary_path: Path, result: dict[str, Any], fail) -> None:
    owned = result["ownedArtifacts"]
    if str(path) not in owned:
        owned.append(str(path))
    created_temporary = False
    try:
        result["artifacts"]["result"]["exists"] = True
        payload = json.dumps(result, ensure_ascii=True, indent=2) + "\n"
        with temporary_path.open("x", encoding="utf-8") as stream:
            created_temporary = True
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, path)
    except (OSError, TypeError, ValueError) as error:
        cleanup_error = ""
        if created_temporary:
            try:
                temporary_path.unlink(missing_ok=True)
            except OSError as temporary_error:
                cleanup_error = f"; temporary cleanup failed: {temporary_error}"
        fail(result, "artifacts", f"cannot write prepare-result: {error}{cleanup_error}")
        result["artifacts"]["result"]["exists"] = False
        result["ownedArtifacts"] = [artifact for artifact in owned if artifact != str(path)]


def _normalized(path: str | Path) -> str:
    return os.path.normcase(str(Path(path).resolve()))
