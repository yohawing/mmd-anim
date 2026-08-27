"""Fail-closed cleanup for one completed oracle case run.

This is deliberately narrower than a directory cleanup helper.  Valid prepare
and (for completed runs) record markers are the proof of ownership; basenames
alone are never enough to authorize deletion.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any

from . import artifacts


CAMPAIGN_CLEANUP_SCHEMA_VERSION = 1
_ALLOWED_BASENAMES = frozenset(
    {
        "scene.pmm",
        "fixture.json",
        "model.mmd-utf16.pmx",
        "prepare-result.json",
        ".prepare-result.json.tmp",
        "oracle.actual.jsonl",
        "oracle.actual.jsonl.done",
        "record-result.json",
        ".record-fixture.json.tmp",
        ".oracle.actual.jsonl.tmp",
        ".oracle.actual.jsonl.done.tmp",
        ".oracle.actual.jsonl.tmp.proxy.log",
        "record-failure.zip",
        ".record-failure.zip.tmp",
        ".record-attempt.json",
        ".record-attempt.json.tmp",
        ".record-result.json.tmp",
        ".record-backup-output.tmp",
        ".record-backup-done.tmp",
    }
)
_MARKER_BASENAMES = frozenset({"prepare-result.json", "record-result.json"})


def cleanup_completed_case_run(run_dir: Path) -> dict[str, Any]:
    """Remove only marker-proven, immediate child artifacts from ``run_dir``.

    The caller is responsible for invoking this only after compact campaign
    state durability (the final snapshot is written after the campaign).
    A failed result never performs recursive or basename-only cleanup.
    """

    requested_run_dir = Path(run_dir)
    run_dir = requested_run_dir.resolve()
    result: dict[str, Any] = {
        "schemaVersion": CAMPAIGN_CLEANUP_SCHEMA_VERSION,
        "ok": False,
        "runDir": str(run_dir),
        "deleted": [],
        "removedRunDir": False,
        "error": None,
    }
    try:
        deleted = _cleanup_owned_run(requested_run_dir, _MARKER_BASENAMES, require_record=True)
    except _CleanupFailure as error:
        result["error"] = {"kind": "campaign-cleanup", "code": error.code, "message": error.message}
        return result
    result["ok"] = True
    result["deleted"] = deleted
    result["removedRunDir"] = True
    return result


def cleanup_prepared_case_run(run_dir: Path) -> dict[str, Any]:
    """Remove a prepare-only run after proving ownership from its prepare marker."""

    requested_run_dir = Path(run_dir)
    result: dict[str, Any] = {
        "schemaVersion": CAMPAIGN_CLEANUP_SCHEMA_VERSION,
        "ok": False,
        "runDir": str(requested_run_dir.resolve()),
        "deleted": [],
        "removedRunDir": False,
        "error": None,
    }
    try:
        deleted = _cleanup_owned_run(requested_run_dir, frozenset({"prepare-result.json"}), require_record=False)
    except _CleanupFailure as error:
        result["error"] = {"kind": "campaign-cleanup", "code": error.code, "message": error.message}
        return result
    result["ok"] = True
    result["deleted"] = deleted
    result["removedRunDir"] = True
    return result


class _CleanupFailure(ValueError):
    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(message)


def _cleanup_owned_run(run_dir: Path, marker_names: frozenset[str], *, require_record: bool) -> list[str]:
    run_dir = Path(run_dir)
    if not run_dir.exists():
        raise _CleanupFailure("missing-run-dir", "run directory does not exist")
    if not run_dir.is_dir():
        raise _CleanupFailure("invalid-run-dir", "run path is not a directory")
    try:
        artifacts.reject_reparse(run_dir)
    except OSError as error:
        raise _CleanupFailure("reparse-run-dir", str(error)) from error
    run_dir = run_dir.resolve()

    markers = {name: run_dir / name for name in marker_names}
    if any(not path.exists() for path in markers.values()):
        expected = " and ".join(sorted(marker_names))
        raise _CleanupFailure("missing-marker", f"{expected} are required")

    try:
        prepare_marker = _read_marker(markers["prepare-result.json"], "prepare-result.json")
        prepare_owned = _validate_marker(
            prepare_marker,
            markers["prepare-result.json"],
            run_dir,
            "prepare",
            require_completed=require_record,
        )
        record_marker = None
        record_owned: set[Path] = set()
        if require_record:
            record_marker = _read_marker(markers["record-result.json"], "record-result.json")
            record_owned = _validate_marker(record_marker, markers["record-result.json"], run_dir, "record")
    except _CleanupFailure:
        raise
    except OSError as error:
        raise _CleanupFailure("marker-read", str(error)) from error

    if require_record:
        assert record_marker is not None
        _validate_marker_relationship(prepare_marker, record_marker, run_dir)
    owned = prepare_owned | record_owned
    protected_paths = _protected_paths(prepare_marker, record_marker)
    children = _validate_children(run_dir, owned, protected_paths)
    deleted: list[str] = []

    for child in sorted((child for child in children if child.name not in marker_names), key=lambda path: path.name):
        try:
            child.unlink()
        except OSError as error:
            raise _CleanupFailure("delete-failed", f"cannot delete owned artifact {child.name}: {error}") from error
        deleted.append(child.name)

    # Markers are intentionally deleted only after every other owned artifact.
    for name in sorted(marker_names):
        marker = markers[name]
        try:
            marker.unlink()
        except OSError as error:
            raise _CleanupFailure("delete-marker-failed", f"cannot delete ownership marker {name}: {error}") from error
        deleted.append(name)

    try:
        if any(run_dir.iterdir()):
            raise _CleanupFailure("remaining-entry", "run directory is not empty after owned cleanup")
        run_dir.rmdir()
    except _CleanupFailure:
        raise
    except OSError as error:
        raise _CleanupFailure("remove-run-dir-failed", str(error)) from error
    return deleted


def _read_marker(path: Path, label: str) -> dict[str, Any]:
    if not path.is_file():
        raise _CleanupFailure("invalid-marker", f"{label} must be a regular file")
    artifacts.reject_reparse(path)
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise _CleanupFailure("invalid-marker", f"{label} is not valid JSON: {error}") from error
    if not isinstance(value, dict):
        raise _CleanupFailure("invalid-marker", f"{label} must be a JSON object")
    return value


def _validate_marker(
    marker: dict[str, Any],
    marker_path: Path,
    run_dir: Path,
    label: str,
    *,
    require_completed: bool = True,
) -> set[Path]:
    if marker.get("schemaVersion") != 1:
        raise _CleanupFailure("invalid-marker", f"{label} marker schemaVersion must be 1")
    if marker.get("artifactName") != run_dir.name:
        raise _CleanupFailure("marker-mismatch", f"{label} marker artifactName does not match run directory")
    if not isinstance(marker.get("caseFile"), str) or not marker["caseFile"]:
        raise _CleanupFailure("invalid-marker", f"{label} marker caseFile is required")
    if not isinstance(marker.get("inputInventory"), dict):
        raise _CleanupFailure("invalid-marker", f"{label} marker inputInventory is required")
    if not isinstance(marker.get("frames"), list):
        raise _CleanupFailure("invalid-marker", f"{label} marker frames are required")
    if label == "prepare" and require_completed and (marker.get("ok") is not True or marker.get("phase") != "complete"):
        raise _CleanupFailure("marker-mismatch", "prepare marker does not report a completed prepare")

    owned = marker.get("ownedArtifacts")
    if not isinstance(owned, list) or not owned or any(not isinstance(value, str) for value in owned):
        raise _CleanupFailure("invalid-marker", f"{label} marker ownedArtifacts is invalid")
    resolved_owned: set[Path] = set()
    for value in owned:
        path = Path(value).resolve()
        _validate_owned_path(path, run_dir, label)
        resolved_owned.add(path)
    if marker_path.resolve() not in resolved_owned:
        raise _CleanupFailure("invalid-marker", f"{label} marker does not own itself")

    inventory = marker.get("artifacts")
    if not isinstance(inventory, dict):
        raise _CleanupFailure("invalid-marker", f"{label} marker artifacts inventory is required")
    for name, entry in inventory.items():
        if not isinstance(entry, dict) or not isinstance(entry.get("path"), str):
            raise _CleanupFailure("invalid-marker", f"{label} marker artifact {name} is invalid")
        _validate_owned_path(Path(entry["path"]).resolve(), run_dir, label)
    return resolved_owned


def _validate_owned_path(path: Path, run_dir: Path, label: str) -> None:
    if path.parent != run_dir.resolve():
        raise _CleanupFailure("marker-escape", f"{label} marker path escapes the exact run directory: {path}")
    if path.name not in _ALLOWED_BASENAMES:
        raise _CleanupFailure("unsupported-artifact", f"{label} marker names unsupported artifact: {path.name}")
    try:
        artifacts.reject_reparse(path)
    except OSError as error:
        raise _CleanupFailure("reparse-artifact", str(error)) from error


def _validate_marker_relationship(
    prepare_marker: dict[str, Any], record_marker: dict[str, Any], run_dir: Path
) -> None:
    for field in ("caseFile", "inputInventory", "frames"):
        if prepare_marker.get(field) != record_marker.get(field):
            raise _CleanupFailure("marker-mismatch", f"prepare and record marker {field} do not match")
    if record_marker.get("artifactName") != run_dir.name:
        raise _CleanupFailure("marker-mismatch", "record marker artifactName does not match run directory")


def _protected_paths(prepare_marker: dict[str, Any], record_marker: dict[str, Any] | None) -> list[Path]:
    protected: list[Path] = []
    inventory = prepare_marker["inputInventory"]
    for entry in inventory.values():
        if isinstance(entry, dict) and isinstance(entry.get("path"), str):
            protected.append(Path(entry["path"]))
    executable = record_marker.get("mmdExecutable") if record_marker is not None else None
    if isinstance(executable, dict) and isinstance(executable.get("path"), str):
        protected.append(Path(executable["path"]))
    return protected


def _validate_children(
    run_dir: Path,
    owned: set[Path],
    protected_paths: list[Path],
) -> list[Path]:
    try:
        children = list(run_dir.iterdir())
    except OSError as error:
        raise _CleanupFailure("list-run-dir-failed", str(error)) from error
    for child in children:
        try:
            artifacts.reject_reparse(child)
        except OSError as error:
            raise _CleanupFailure("reparse-artifact", str(error)) from error
        if child.name not in _ALLOWED_BASENAMES:
            raise _CleanupFailure("foreign-entry", f"unsupported or foreign run entry remains: {child.name}")
        if not child.is_file():
            raise _CleanupFailure("non-regular-artifact", f"run entry is not a regular file: {child.name}")
        if child.resolve() not in owned:
            raise _CleanupFailure("unowned-artifact", f"run entry is not marker-owned: {child.name}")
        for protected in protected_paths:
            try:
                if protected.exists() and os.path.samefile(child, protected):
                    raise _CleanupFailure(
                        "protected-artifact",
                        f"run entry aliases a protected input or executable: {child.name}",
                    )
            except OSError as error:
                raise _CleanupFailure("protected-path-check", str(error)) from error
    return children
