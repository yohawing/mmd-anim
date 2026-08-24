from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Any

from . import artifacts
from .case import OracleCase
from .prepare import (
    CommandResult,
    CommandRunner,
    SubprocessRunner,
    _artifact_name,
    _default_repo_root,
    _input_inventory,
    _text,
)
from .record_artifacts import (
    TEMPORARY_ARTIFACT_KEYS,
    artifact_identity as _artifact_identity,
    load_prepared_artifacts as _load_prepared_artifacts,
    paths as _paths,
    persist_result as _persist_result,
    promotion_state as _promotion_state,
    promote_record as _promote_record,
    recover_interrupted_record_artifacts as _recover_interrupted_record_artifacts,
    refresh as _refresh_artifacts,
    remove_attempt_marker as _remove_attempt_marker,
    remove_temporary_artifacts as _remove_temporary_artifacts,
    retain_failure_artifacts as _retain_failure_artifacts,
    rollback_promotion as _rollback_promotion,
    valid_done as _valid_done,
    validate_existing_record_artifacts as _validate_existing_record_artifacts,
    validate_mmd_exe as _validate_mmd_exe,
    validate_record_path_separation as _validate_record_path_separation,
    write_attempt_marker as _write_attempt_marker,
    write_temp_fixture as _write_temp_fixture,
)

_MMDDUMPER_ROOT = Path("MMDDumper")
_PYTHON_CLI = _MMDDUMPER_ROOT / "scripts" / "oracle_cli.py"
_DIAGNOSTIC_LIMIT = 4096
_TIMEOUT_MARKER = "Timed out waiting for"
_RESTORE_MARKERS = ("mmd-python:restore-error", "mmd-smoke:restore-error", "mmd-smoke:restore-missing-backup")
_PHASES = ("launchGuard", "process", "timeout", "dialog", "done", "schema", "coverage")


def record_case(
    case: OracleCase,
    mmd_exe: str | Path,
    *,
    runner: CommandRunner | None = None,
    repo_root: Path | None = None,
) -> dict[str, Any]:
    # The Python host runner owns its deadline, MMD shutdown, and DLL restore in one finally block.
    # Killing it from an outer Python timeout can strand the temporary plugin installation.
    runner = runner or SubprocessRunner(timeout_seconds=None)
    repo_root = (repo_root or _default_repo_root()).resolve()
    artifact_name = _artifact_name(case.name)
    run_dir = case.output_root / artifact_name
    paths = _paths(run_dir)
    result = _base_result(case, repo_root, paths, artifact_name)
    temp_fixture: Path | None = None
    temporary_owned: set[str] = set()
    attempt_owned: set[str] = set()
    existing_owned: set[str] = set()
    persist_result = True
    record_attempted = False

    try:
        artifacts.reject_reparse(case.output_root, run_dir, *paths.values())
        run_dir.mkdir(parents=True, exist_ok=True)
    except OSError as error:
        _fail(result, "launchGuard", f"artifact directory is unsafe or unavailable: {error}")
        return result

    try:
        try:
            result["inputInventory"] = _input_inventory(case)
        except (OSError, ValueError) as error:
            _fail(result, "preflight", f"cannot inventory current case inputs: {error}")
            # Without a complete inventory, a marker cannot prove ownership and
            # would reject a later retry as stale after the input is restored.
            persist_result = False

        if not result["errors"]:
            if not case.record_opt_in:
                _fail(result, "launchGuard", "case recordOptIn must be true")
            elif os.environ.get("MMD_DUMPER_ALLOW_MMD_LAUNCH") != "1":
                _fail(result, "launchGuard", "MMD_DUMPER_ALLOW_MMD_LAUNCH=1 is required")
            else:
                result["phases"]["launchGuard"] = {"status": "pass"}
            if result["errors"]:
                # The case file itself is part of inputInventory. Persisting a
                # guard-only marker would become stale when recordOptIn changes.
                persist_result = False

        if not result["errors"]:
            try:
                prepared = _load_prepared_artifacts(case, paths, result["inputInventory"])
                mmd_path = _validate_mmd_exe(mmd_exe)
                _validate_record_path_separation(paths, result["inputInventory"], mmd_path)
                result["mmdExecutable"] = _file_identity(mmd_path)
                _recover_interrupted_record_artifacts(paths, case, result["inputInventory"])
                existing_owned = _validate_existing_record_artifacts(paths, case, result["inputInventory"])
            except (OSError, ValueError) as error:
                _fail(result, "preflight", f"record artifacts are not owned by the current case: {error}")
                persist_result = False
        if result["errors"]:
            return result

        fixture = prepared["fixture"]
        _remove_temporary_artifacts(paths, existing_owned)
        temporary_owned = {str(paths[key].resolve()) for key in TEMPORARY_ARTIFACT_KEYS}
        attempt_owned = _write_attempt_marker(paths, case, result["inputInventory"], result["mmdExecutable"])
        temp_fixture = _write_temp_fixture(run_dir, fixture, mmd_path, output=paths["outputTemp"], done=paths["doneTemp"])

        backend_cwd = repo_root / _MMDDUMPER_ROOT
        record_command = [sys.executable, str(repo_root / _PYTHON_CLI), "record", "--fixture", str(temp_fixture)]
        if case.dialog_opt_in:
            record_command.extend(("--accept-dialog", "true"))
            result["phases"]["dialog"] = {"status": "pass", "optIn": True}
        else:
            result["phases"]["dialog"] = {"status": "not_run", "optIn": False}
        record_attempted = True
        outcome = runner.run(record_command, backend_cwd)
        _set_command(result, "record", outcome)
        stderr = _text(outcome.stderr)
        timeout = outcome.exit_code == 124 or _TIMEOUT_MARKER in stderr
        restore_error = next((marker for marker in _RESTORE_MARKERS if marker in stderr), None)
        process_ok = outcome.exit_code == 0 and not timeout and restore_error is None
        result["phases"]["timeout"] = {"status": "fail" if timeout else "pass"}
        result["phases"]["process"] = {"status": "pass" if process_ok else "fail", "exitCode": outcome.exit_code}
        if timeout:
            _fail(result, "timeout", "MMD runner reported a timeout")
        if restore_error is not None:
            _fail(result, "process", f"MMD runner reported DLL restore failure: {restore_error}")
        if outcome.exit_code != 0:
            _fail(result, "process", f"record command exited with {outcome.exit_code}")

        temporary_valid = _evaluate_dump(result, paths["outputTemp"], paths["doneTemp"], case, temp_fixture, backend_cwd, runner, allow_coverage=process_ok)
        if process_ok and temporary_valid and _subgates_pass(result):
            try:
                result["promotion"] = _promotion_state(paths)
                result["phase"] = "promotion"
                _persist_result(result, paths)
                _promote_record(paths, result)
                result["recorded"] = _valid_done(paths["done"], paths["output"], validate_records=False)[0]
                if not result["recorded"]:
                    raise OSError("atomic record promotion did not leave a valid stable dump")
            except (OSError, TypeError, ValueError) as error:
                if "promotion" not in result:
                    _fail(result, "artifacts", f"atomic record promotion failed: {error}")
                else:
                    try:
                        _rollback_promotion(paths, result)
                    except (OSError, ValueError) as rollback_error:
                        result["ok"] = False
                        result["errors"].append({
                            "phase": "artifacts",
                            "message": f"atomic record promotion failed: {error}; rollback is pending: {rollback_error}",
                        })
                    else:
                        _fail(result, "artifacts", f"atomic record promotion failed: {error}")
    except (OSError, ValueError, json.JSONDecodeError) as error:
        _fail(result, "preflight", str(error))
    finally:
        preserve_temporaries = False
        if record_attempted and not result["recorded"]:
            try:
                _retain_failure_artifacts(paths, existing_owned | temporary_owned)
            except OSError as error:
                _fail(result, "artifacts", f"cannot retain record failure artifacts: {error}")
                preserve_temporaries = True
        temporary_cleanup_ok = False
        if not preserve_temporaries:
            try:
                _remove_temporary_artifacts(paths, temporary_owned)
                temporary_cleanup_ok = True
            except OSError as error:
                _fail(result, "artifacts", f"cannot remove temporary record artifacts: {error}")
        if temporary_cleanup_ok and result["recorded"]:
            try:
                _remove_attempt_marker(paths, attempt_owned)
            except OSError as error:
                _fail(result, "artifacts", f"cannot remove record attempt marker: {error}")
        try:
            _refresh_artifacts(result)
        except OSError as error:
            _fail(result, "artifacts", f"cannot inspect record artifacts safely: {error}")
        result["ok"] = bool(result["recorded"] and _subgates_pass(result) and not result["errors"])
        if result["ok"]:
            result["phase"] = "complete"
        if persist_result:
            try:
                _persist_result(result, paths)
            except (OSError, TypeError, ValueError) as error:
                _fail(result, "artifacts", f"cannot write record-result: {error}")
                result["ok"] = False
                if result["recorded"]:
                    try:
                        _rollback_promotion(paths, result)
                        result["recorded"] = False
                    except (OSError, ValueError) as rollback_error:
                        _fail(result, "artifacts", f"cannot roll back unpersisted record: {rollback_error}")
                try:
                    _refresh_artifacts(result)
                except OSError as refresh_error:
                    _fail(result, "artifacts", f"cannot inspect record artifacts after persist failure: {refresh_error}")
    return result


def _evaluate_dump(result: dict[str, Any], output: Path, done: Path, case: OracleCase, fixture: Path, backend_cwd: Path, runner: CommandRunner, *, allow_coverage: bool) -> bool:
    # Keep the done gate limited to marker/framing/count checks. The coverage
    # command also validates every JSONL record, so a successful record needs
    # only one full parse. A failed process with an output still gets the
    # schema-only diagnostic below.
    done_valid, done_error = _valid_done(done, output, validate_records=False)
    result["phases"]["done"] = {"status": "pass" if done_valid else "fail"}
    if not done_valid:
        _fail(result, "done", done_error or "dump and done marker are required")
    if not output.is_file():
        result["phases"]["schema"] = {"status": "not_run"}
        result["phases"]["coverage"] = {"status": "not_run"}
        return done_valid

    if not allow_coverage:
        validation = runner.run([sys.executable, str(backend_cwd / "scripts" / "oracle_cli.py"), "validate", str(output)], backend_cwd)
        _set_command(result, "validate", validation)
        schema_report = _json_stdout(validation.stdout)
        schema_ok = validation.exit_code == 0 and isinstance(schema_report, dict) and schema_report.get("ok") is True
        result["phases"]["schema"] = {"status": "pass" if schema_ok else "fail", "report": schema_report}
        if not schema_ok:
            _fail(result, "schema", _command_error("validate", validation))
        result["phases"]["coverage"] = {"status": "not_run"}
        return done_valid

    coverage_command = [sys.executable, str(backend_cwd / "scripts" / "oracle_cli.py"), "verify-coverage", "--fixture", str(fixture), "--actual", str(output)]
    if case.camera_vmd is not None:
        coverage_command.extend(("--require-camera", "true"))
    coverage = runner.run(coverage_command, backend_cwd)
    _set_command(result, "coverage", coverage)
    coverage_report = _json_stdout(coverage.stdout)
    schema_ok = isinstance(coverage_report, dict) and isinstance(coverage_report.get("ok"), bool)
    result["phases"]["schema"] = {
        "status": "pass" if schema_ok else "fail",
        "report": {"ok": True, "records": coverage_report.get("records")} if schema_ok else None,
    }
    if not schema_ok:
        _fail(result, "schema", _command_error("verify-coverage", coverage))
        result["phases"]["coverage"] = {"status": "not_run"}
        return done_valid

    coverage_ok = coverage.exit_code == 0 and coverage_report.get("ok") is True
    result["phases"]["coverage"] = {"status": "pass" if coverage_ok else "fail", "report": coverage_report}
    if not coverage_ok:
        _fail(result, "coverage", _command_error("verify-coverage", coverage))
    return done_valid


def _base_result(case: OracleCase, repo_root: Path, paths: dict[str, Path], artifact_name: str) -> dict[str, Any]:
    return {
        "schemaVersion": 1, "ok": False, "phase": "launchGuard", "recorded": False,
        "caseName": case.name, "artifactName": artifact_name, "caseFile": str(case.source_path),
        "frames": list(case.frames),
        "mmdExecutable": None,
        "inputInventory": {}, "ownedArtifacts": [],
        "phases": {key: {"status": "not_run"} for key in _PHASES},
        "artifacts": {key: {"path": str(path), "exists": artifacts.exists(path)} for key, path in paths.items()},
        "commands": {}, "backend": {"cwd": str(repo_root / _MMDDUMPER_ROOT)}, "errors": [],
    }


def _set_command(result: dict[str, Any], name: str, outcome: CommandResult) -> None:
    result["commands"][name] = {"command": list(outcome.command), "cwd": str(outcome.cwd), "exitCode": outcome.exit_code, "stderr": _text(outcome.stderr)[-_DIAGNOSTIC_LIMIT:]}


def _command_error(name: str, outcome: CommandResult) -> str:
    stderr = _text(outcome.stderr).strip()
    return f"{name} exited with {outcome.exit_code}" + (f": {stderr[-_DIAGNOSTIC_LIMIT:]}" if stderr else "")


def _json_stdout(stdout: str) -> Any:
    try:
        return json.loads(_text(stdout))
    except json.JSONDecodeError:
        return None


def _subgates_pass(result: dict[str, Any]) -> bool:
    return result["phases"]["schema"]["status"] == "pass" and result["phases"]["coverage"]["status"] == "pass"


def _file_identity(path: Path) -> dict[str, Any]:
    return {"path": str(path), **_artifact_identity(path)}


def _fail(result: dict[str, Any], phase: str, message: str) -> None:
    result["ok"] = False
    if not result["errors"]:
        result["phase"] = phase
    if phase in result["phases"]:
        result["phases"][phase] = {**result["phases"][phase], "status": "fail"}
    result["errors"].append({"phase": phase, "message": message})
