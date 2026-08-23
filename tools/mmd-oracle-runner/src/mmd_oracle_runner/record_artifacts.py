from __future__ import annotations

import json
import os
import tempfile
from pathlib import Path
from typing import Any

from . import artifacts
from .case import OracleCase

TEMPORARY_ARTIFACT_KEYS = ("outputTemp", "doneTemp", "proxyLogTemp")
_RECORD_ARTIFACT_KEYS = (
    "output", "done", "result", *TEMPORARY_ARTIFACT_KEYS, "attempt", "resultTemp", "backupOutput", "backupDone"
)


def paths(run_dir: Path) -> dict[str, Path]:
    return {
        "project": run_dir / "scene.pmm",
        "fixture": run_dir / "fixture.json",
        "output": run_dir / "oracle.actual.jsonl",
        "done": run_dir / "oracle.actual.jsonl.done",
        "result": run_dir / "record-result.json",
        "outputTemp": run_dir / ".oracle.actual.jsonl.tmp",
        "doneTemp": run_dir / ".oracle.actual.jsonl.done.tmp",
        "proxyLogTemp": run_dir / ".oracle.actual.jsonl.tmp.proxy.log",
        "attempt": run_dir / ".record-attempt.json",
        "resultTemp": run_dir / ".record-result.json.tmp",
        "backupOutput": run_dir / ".record-backup-output.tmp",
        "backupDone": run_dir / ".record-backup-done.tmp",
    }


def load_prepared_artifacts(case: OracleCase, paths: dict[str, Path], input_inventory: dict[str, Any]) -> dict[str, Any]:
    marker_path = paths["result"].parent / "prepare-result.json"
    if not marker_path.is_file():
        raise ValueError("successful prepare-result.json is required")
    artifacts.reject_reparse(marker_path, paths["project"], paths["fixture"])
    marker = read_json_object(marker_path, "prepare-result.json")
    if marker.get("schemaVersion") != 1:
        raise ValueError("prepare-result.json schemaVersion must be 1")
    if marker.get("ok") is not True or marker.get("phase") != "complete":
        raise ValueError("prepare-result.json does not report a successful prepare")
    if marker.get("caseFile") != str(case.source_path) or marker.get("artifactName") != paths["result"].parent.name:
        raise ValueError("prepare-result.json does not belong to this case artifact")
    if marker.get("inputInventory") != input_inventory:
        raise ValueError("prepare-result.json inputInventory is stale")
    if marker.get("frames") != list(case.frames):
        raise ValueError("prepare-result.json frames are stale")
    owned = marker.get("ownedArtifacts")
    if not isinstance(owned, list) or any(not isinstance(path, str) for path in owned):
        raise ValueError("prepare-result.json ownedArtifacts is invalid")
    marker_artifacts = marker.get("artifacts")
    if not isinstance(marker_artifacts, dict):
        raise ValueError("prepare-result.json has no artifact inventory")
    for key in ("project", "fixture"):
        entry = marker_artifacts.get(key)
        if (
            not isinstance(entry, dict)
            or Path(str(entry.get("path"))).resolve() != paths[key].resolve()
            or entry.get("exists") is not True
            or str(paths[key]) not in owned
        ):
            raise ValueError(f"prepare-result.json does not own required {key} artifact")
    if not paths["project"].is_file() or not paths["fixture"].is_file():
        raise ValueError("prepared scene.pmm and fixture.json must exist")
    fixture = read_json_object(paths["fixture"], "fixture.json")
    project = fixture_path(fixture, "project", paths["result"].parent)
    if project != paths["project"]:
        raise ValueError("prepared fixture project does not match scene.pmm")
    _validate_prepared_fixture(fixture, case, paths)
    return {"marker": marker, "fixture": fixture}


def recover_interrupted_record_artifacts(paths: dict[str, Path], case: OracleCase, input_inventory: dict[str, Any]) -> None:
    result_path, result_temp = paths["result"], paths["resultTemp"]
    artifacts.reject_reparse(result_path, result_temp, paths["backupOutput"], paths["backupDone"])
    if result_temp.exists():
        if not result_temp.is_file():
            raise ValueError("interrupted record result must be a regular file")
        temporary_marker = _load_record_marker(result_temp, case, paths, input_inventory)
        if str(result_temp.resolve()) not in _owned_paths(temporary_marker, "record-result.json"):
            raise ValueError("interrupted record-result.json does not own its temporary file")
        if result_path.exists():
            _load_record_marker(result_path, case, paths, input_inventory)
        os.replace(result_temp, result_path)

    backups = ((paths["backupOutput"], paths["output"]), (paths["backupDone"], paths["done"]))
    existing_backups = [(backup, target) for backup, target in backups if backup.exists()]
    if not existing_backups:
        return
    marker = _load_record_marker(result_path, case, paths, input_inventory)
    owned = _owned_paths(marker, "record-result.json")
    completed = marker.get("ok") is True and marker.get("recorded") is True and marker.get("phase") == "complete"
    if completed and all(str(backup.resolve()) in owned for backup, _ in existing_backups):
        stable_valid, _ = valid_done(paths["done"], paths["output"])
        if stable_valid:
            for backup, _ in existing_backups:
                backup.unlink()
            return
    for backup, target in existing_backups:
        if not backup.is_file():
            raise ValueError(f"interrupted record backup must be a regular file: {backup}")
        if str(target.resolve()) not in owned:
            raise ValueError(f"record-result.json does not own interrupted backup target: {target}")
    for backup, target in existing_backups:
        os.replace(backup, target)
    if completed and not valid_done(paths["done"], paths["output"])[0]:
        raise ValueError("record promotion recovery did not restore a valid stable dump")


def validate_existing_record_artifacts(paths: dict[str, Path], case: OracleCase, input_inventory: dict[str, Any]) -> set[str]:
    artifacts.reject_reparse(*paths.values())
    existing = [paths[key] for key in _RECORD_ARTIFACT_KEYS if paths[key].exists()]
    if not existing:
        return set()
    if any(not path.is_file() for path in existing):
        raise ValueError("existing record artifacts must be regular files")
    owned: set[str] = set()
    attempt_owned: set[str] = set()
    marker_path, attempt_path = paths["result"], paths["attempt"]
    if marker_path.is_file():
        marker = _load_record_marker(marker_path, case, paths, input_inventory)
        owned.update(_owned_paths(marker, "record-result.json"))
    if attempt_path.is_file():
        attempt_owned = _load_attempt_marker(attempt_path, case, paths, input_inventory)
        owned.update(attempt_owned)
    if not owned:
        raise ValueError("existing record artifacts require record-result.json")
    existing_temporary = {str(paths[key].resolve()) for key in TEMPORARY_ARTIFACT_KEYS if paths[key].exists()}
    if existing_temporary and not existing_temporary.issubset(attempt_owned):
        raise ValueError("existing temporary record artifacts require a valid record attempt marker")
    if any(str(path.resolve()) not in owned for path in existing):
        raise ValueError("record-result.json does not own every existing record artifact")
    return owned


def write_attempt_marker(
    paths: dict[str, Path],
    case: OracleCase,
    input_inventory: dict[str, Any],
    mmd_executable: dict[str, Any],
) -> set[str]:
    marker = paths["attempt"]
    artifacts.reject_reparse(marker, *(paths[key] for key in TEMPORARY_ARTIFACT_KEYS))
    owned = {str(marker.resolve()), *(str(paths[key].resolve()) for key in TEMPORARY_ARTIFACT_KEYS)}
    payload = {
        "schemaVersion": 1,
        "phase": "record-attempt",
        "caseFile": str(case.source_path),
        "artifactName": paths["result"].parent.name,
        "inputInventory": input_inventory,
        "frames": list(case.frames),
        "mmdExecutable": mmd_executable,
        "ownedArtifacts": sorted(owned),
    }
    created = False
    try:
        with marker.open("x", encoding="utf-8") as stream:
            created = True
            json.dump(payload, stream, ensure_ascii=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
    except BaseException as error:
        if created:
            try:
                marker.unlink(missing_ok=True)
            except OSError as cleanup_error:
                raise OSError(f"cannot remove partial record attempt marker after write failure: {cleanup_error}") from error
        raise
    return owned


def remove_attempt_marker(paths: dict[str, Path], owned_paths: set[str]) -> None:
    marker = paths["attempt"]
    if not marker.exists():
        return
    artifacts.reject_reparse(marker)
    if not marker.is_file() or str(marker.resolve()) not in owned_paths:
        raise OSError("record attempt marker is not owned by this run")
    marker.unlink()


def validate_mmd_exe(value: str | Path) -> Path:
    path = Path(value)
    if not path.is_absolute():
        raise ValueError("mmd_exe must be an absolute path")
    if not path.is_file():
        raise ValueError("mmd_exe must point to an existing file")
    return path


def fixture_path(fixture: dict[str, Any], field: str, run_dir: Path) -> Path:
    raw = fixture.get(field)
    if not isinstance(raw, str) or not raw:
        raise ValueError(f"fixture.{field} must be a path")
    path = Path(raw).resolve()
    if path.parent != run_dir.resolve():
        raise ValueError(f"fixture.{field} must remain in the prepared artifact directory")
    return path


def write_temp_fixture(
    run_dir: Path,
    fixture: dict[str, Any],
    mmd_exe: Path,
    *,
    output: Path,
    done: Path,
) -> Path:
    expected = paths(run_dir)
    artifacts.reject_reparse(run_dir, output, done, expected["proxyLogTemp"])
    if output.resolve() != expected["outputTemp"].resolve() or done.resolve() != expected["doneTemp"].resolve():
        raise ValueError("temporary record output paths must remain in the prepared artifact directory")
    fd, raw_path = tempfile.mkstemp(prefix=".record-fixture-", suffix=".json", dir=run_dir)
    path = Path(raw_path)
    try:
        payload = dict(fixture)
        payload["mmdExe"] = str(mmd_exe)
        payload["output"] = str(output)
        payload["done"] = str(done)
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(payload, stream, ensure_ascii=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
    except BaseException:
        try:
            path.unlink(missing_ok=True)
        finally:
            raise
    return path


def remove_temporary_artifacts(paths: dict[str, Path], owned_paths: set[str]) -> None:
    for key in TEMPORARY_ARTIFACT_KEYS:
        path = paths[key]
        if path.exists():
            artifacts.reject_reparse(path)
            if not path.is_file():
                raise OSError(f"record temporary artifact is not a file: {path}")
            if str(path.resolve()) not in owned_paths:
                raise OSError(f"record temporary artifact is not owned by this run: {path}")
            path.unlink()


def promote_record(paths: dict[str, Path]) -> None:
    output, done = paths["output"], paths["done"]
    temp_output, temp_done = paths["outputTemp"], paths["doneTemp"]
    backup_output, backup_done = paths["backupOutput"], paths["backupDone"]
    artifacts.reject_reparse(output, done, temp_output, temp_done, backup_output, backup_done)
    if backup_output.exists() or backup_done.exists():
        raise OSError("stale record promotion backup exists")
    had_output, had_done = output.is_file(), done.is_file()
    try:
        rewrite_done_output(temp_done, output)
        if had_output:
            os.replace(output, backup_output)
        if had_done:
            os.replace(done, backup_done)
        os.replace(temp_output, output)
        os.replace(temp_done, done)
    except OSError:
        try:
            if output.exists() and not had_output:
                output.unlink()
            if done.exists() and not had_done:
                done.unlink()
            if backup_output.exists():
                os.replace(backup_output, output)
            if backup_done.exists():
                os.replace(backup_done, done)
        except OSError:
            pass
        raise


def rollback_unpersisted_record(paths: dict[str, Path]) -> None:
    pairs = ((paths["backupOutput"], paths["output"]), (paths["backupDone"], paths["done"]))
    artifacts.reject_reparse(*(path for pair in pairs for path in pair))
    for backup, target in pairs:
        if backup.exists():
            if not backup.is_file():
                raise OSError(f"record promotion backup is not a file: {backup}")
            os.replace(backup, target)
        elif target.exists():
            if not target.is_file():
                raise OSError(f"unpersisted record artifact is not a file: {target}")
            target.unlink()


def rewrite_done_output(done: Path, output: Path) -> None:
    artifacts.reject_reparse(done, output)
    payload = read_json_object(done, "done marker")
    payload["output"] = str(output)
    with done.open("w", encoding="utf-8") as stream:
        json.dump(payload, stream, ensure_ascii=True)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())


def valid_done(done: Path, output: Path, *, validate_records: bool = True) -> tuple[bool, str | None]:
    """Validate the done marker and JSONL framing; the caller owns oracle schema validation."""
    try:
        artifacts.reject_reparse(done, output)
    except OSError as error:
        return False, str(error)
    if not output.is_file():
        return False, "dump JSONL is missing"
    if not done.is_file():
        return False, "done marker is missing"
    try:
        payload = read_json_object(done, "done marker")
    except (OSError, ValueError, json.JSONDecodeError) as error:
        return False, str(error)
    if payload.get("ok") is not True:
        return False, "done marker does not report ok=true"
    records = payload.get("records")
    if isinstance(records, bool) or not isinstance(records, int) or records <= 0:
        return False, "done marker records must be a positive integer"
    if payload.get("output") is not None and Path(str(payload["output"])).resolve() != output.resolve():
        return False, "done marker output does not match dump JSONL"
    try:
        with output.open("r", encoding="utf-8") as stream:
            actual_records = 0
            for line_number, line in enumerate(stream, start=1):
                if not line.strip():
                    continue
                if validate_records:
                    record = json.loads(line)
                    if not isinstance(record, dict):
                        return False, f"dump JSONL line {line_number} must be a JSON object"
                actual_records += 1
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        return False, f"cannot read dump JSONL records: {error}"
    if records != actual_records:
        return False, f"done marker records {records} do not match dump JSONL count {actual_records}"
    return True, None


def refresh(result: dict[str, Any]) -> None:
    for artifact in result["artifacts"].values():
        path = Path(artifact["path"])
        artifacts.reject_reparse(path)
        artifact["exists"] = artifacts.exists(path)


def persist_result(result: dict[str, Any], paths: dict[str, Path]) -> None:
    path, temporary = paths["result"], paths["resultTemp"]
    prior_owned = list(result["ownedArtifacts"])
    prior_exists = result["artifacts"]["result"]["exists"]
    created = False
    try:
        artifacts.reject_reparse(path.parent, *paths.values())
        missing_recovery_targets = {
            "output": artifacts.exists(paths["backupOutput"]),
            "done": artifacts.exists(paths["backupDone"]),
        }
        result["ownedArtifacts"] = [
            str(paths[key])
            for key in _RECORD_ARTIFACT_KEYS
            if artifacts.exists(paths[key]) or key in ("result", "resultTemp") or missing_recovery_targets.get(key, False)
        ]
        result["artifacts"]["result"]["exists"] = True
        with temporary.open("x", encoding="utf-8") as stream:
            created = True
            json.dump(result, stream, ensure_ascii=True, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    except (OSError, TypeError, ValueError):
        result["ownedArtifacts"] = prior_owned
        result["artifacts"]["result"]["exists"] = prior_exists
        if created:
            temporary.unlink(missing_ok=True)
        raise


def read_json_object(path: Path, label: str) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{label} must be a JSON object")
    return value


def _load_record_marker(path: Path, case: OracleCase, paths: dict[str, Path], input_inventory: dict[str, Any]) -> dict[str, Any]:
    if not path.is_file():
        raise ValueError("existing record artifacts require record-result.json")
    artifacts.reject_reparse(path)
    marker = read_json_object(path, "record-result.json")
    if marker.get("schemaVersion") != 1:
        raise ValueError("record-result.json schemaVersion must be 1")
    if marker.get("caseFile") != str(case.source_path):
        raise ValueError("record-result.json caseFile does not match current case")
    if marker.get("artifactName") != paths["result"].parent.name:
        raise ValueError("record-result.json artifactName does not match current artifact")
    if marker.get("inputInventory") != input_inventory:
        raise ValueError("record-result.json inputInventory is stale")
    if marker.get("frames") != list(case.frames):
        raise ValueError("record-result.json frames are stale")
    _owned_paths(marker, "record-result.json")
    return marker


def _load_attempt_marker(path: Path, case: OracleCase, paths: dict[str, Path], input_inventory: dict[str, Any]) -> set[str]:
    marker = read_json_object(path, "record attempt marker")
    expected_owned = {str(path.resolve()), *(str(paths[key].resolve()) for key in TEMPORARY_ARTIFACT_KEYS)}
    if (
        marker.get("schemaVersion") != 1
        or marker.get("phase") != "record-attempt"
        or marker.get("caseFile") != str(case.source_path)
        or marker.get("artifactName") != paths["result"].parent.name
        or marker.get("inputInventory") != input_inventory
        or marker.get("frames") != list(case.frames)
        or _owned_paths(marker, "record attempt marker") != expected_owned
    ):
        raise ValueError("record attempt marker does not match the current case")
    return expected_owned


def _owned_paths(marker: dict[str, Any], label: str) -> set[str]:
    owned = marker.get("ownedArtifacts")
    if not isinstance(owned, list) or any(not isinstance(path, str) for path in owned):
        raise ValueError(f"{label} ownedArtifacts is invalid")
    return {str(Path(path).resolve()) for path in owned}


def _validate_prepared_fixture(fixture: dict[str, Any], case: OracleCase, artifact_paths: dict[str, Path]) -> None:
    expected_keys = {"name", "mmdVersion", "mmdExe", "project", "frames", "output", "done", "timeoutMs", "dump"}
    expected_dump = {
        "bones": True,
        "morphs": True,
        "camera": case.camera_vmd is not None,
        "cameraKeyframes": True,
        "sceneParameters": False,
        "rigidBodies": False,
    }
    if (
        set(fixture) != expected_keys
        or fixture.get("name") != case.name
        or fixture.get("mmdVersion") != "9.32-x64"
        or not isinstance(fixture.get("mmdExe"), str)
        or fixture.get("frames") != list(case.frames)
        or fixture.get("timeoutMs") != 60000
        or fixture.get("dump") != expected_dump
        or fixture_path(fixture, "output", artifact_paths["result"].parent) != artifact_paths["output"]
        or fixture_path(fixture, "done", artifact_paths["result"].parent) != artifact_paths["done"]
    ):
        raise ValueError("prepared fixture does not match the current case contract")
