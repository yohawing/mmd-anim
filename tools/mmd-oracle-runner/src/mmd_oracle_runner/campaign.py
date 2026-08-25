"""Sequential orchestration for a compact motion-quality campaign.

The campaign deliberately keeps only compact state and a summary snapshot.
Every case is prepared, recorded, compared, state-persisted, and cleaned
before the next case starts.
"""

from __future__ import annotations

import hashlib
import json
import math
import os
import subprocess
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from .campaign_artifacts import cleanup_completed_case_run, cleanup_prepared_case_run
from .case import CaseValidationError, OracleCase, load_case
from .prepare import CommandResult, _artifact_name, _default_repo_root, prepare_case
from .record import record_case
from .selection import SelectionError, load_selection, verify_selection

CAMPAIGN_SCHEMA_VERSION = 1
_CONFIG_TOP_LEVEL = {"schemaVersion", "selectionFile", "selectionHash", "run", "discovered", "compare", "cases"}
_RUN_FIELDS = {"mmdVersion", "dumperVersion", "timestamp", "samplingPolicy"}
_COMPARE_FIELDS = {"focusBones", "thresholds"}
_THRESHOLD_NAMES = (
    "translationMaxError",
    "translationRmsError",
    "rotationMaxAngleRad",
    "rotationRmsAngleRad",
    "maxAbsError",
)
_CASE_FIELDS = {"caseFile", "caseId", "features", "categories"}
_METRIC_NAMES = _THRESHOLD_NAMES
_STATE_SCHEMA_VERSION = 1
_STATE_CLEANED = "cleaned"
_SHA256_RE = frozenset("0123456789abcdefABCDEF")


class CampaignValidationError(ValueError):
    """Stable validation error for campaign configuration/state."""

    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(message)

    def as_dict(self) -> dict[str, str]:
        return {"kind": "campaign", "code": self.code, "message": self.message}


@dataclass(frozen=True)
class CampaignCase:
    case_file: Path
    case_id: str
    features: tuple[str, ...]
    categories: tuple[str, ...]


@dataclass(frozen=True)
class CampaignConfig:
    selection_file: Path
    selection_hash: str
    selection_frames: tuple[int, ...]
    selected_cases: dict[str, dict[str, Any]]
    run: dict[str, str]
    discovered: int
    focus_bones: tuple[str, ...]
    thresholds: dict[str, float]
    cases: tuple[CampaignCase, ...]
    config_hash: str


@dataclass(frozen=True)
class CompareOutcome:
    compared: bool
    passed: bool
    metrics: dict[str, float]
    failures: tuple[str, ...]


PrepareAction = Callable[[OracleCase], dict[str, Any]]
RecordAction = Callable[[OracleCase, str | Path | None], dict[str, Any]]
CompareAction = Callable[[Path, Path], CommandResult]
CleanupAction = Callable[[Path], dict[str, Any]]
ProvenanceProbe = Callable[[Path], dict[str, str]]


def load_campaign_config(path: Path) -> CampaignConfig:
    """Read, strictly validate, and hash one campaign configuration."""

    path = _require_absolute_file(path, "config")
    payload = _read_json_object(path, "config")
    if set(payload) != _CONFIG_TOP_LEVEL:
        raise CampaignValidationError(
            "config-schema",
            "config fields must exactly be schemaVersion, selectionFile, selectionHash, run, discovered, compare, cases",
        )
    if payload.get("schemaVersion") != CAMPAIGN_SCHEMA_VERSION:
        raise CampaignValidationError("config-schema", f"schemaVersion must be {CAMPAIGN_SCHEMA_VERSION}")
    selection_hash = payload.get("selectionHash")
    if not _is_sha256(selection_hash):
        raise CampaignValidationError("config-value", "selectionHash must be a 64-character hexadecimal SHA-256")
    selection_file = _require_absolute_file(Path(str(payload.get("selectionFile"))), "selectionFile")
    try:
        selection = load_selection(selection_file)
    except ValueError as error:
        raise CampaignValidationError("selection", str(error)) from error
    if selection["selectionHash"] != selection_hash.lower():
        raise CampaignValidationError("selection", "selectionHash does not match selectionFile")
    run = payload.get("run")
    if not isinstance(run, dict) or set(run) != _RUN_FIELDS:
        raise CampaignValidationError("config-schema", "run fields are missing or unsupported")
    for field in _RUN_FIELDS:
        _require_safe_string(run.get(field), f"run.{field}")
    discovered = payload.get("discovered")
    if not _is_count(discovered):
        raise CampaignValidationError("config-value", "discovered must be a non-negative integer")
    frozen_discovered = min(selection["discovery"]["eligiblePmx"], selection["discovery"]["eligibleBodyVmd"])
    if discovered != frozen_discovered:
        raise CampaignValidationError("selection", "discovered does not match the frozen eligible asset count")
    compare = payload.get("compare")
    if not isinstance(compare, dict) or set(compare) != _COMPARE_FIELDS:
        raise CampaignValidationError("config-schema", "compare fields are missing or unsupported")
    focus_bones = _require_safe_string_array(compare.get("focusBones"), "compare.focusBones")
    thresholds = compare.get("thresholds")
    if not isinstance(thresholds, dict) or set(thresholds) != set(_THRESHOLD_NAMES):
        raise CampaignValidationError("config-schema", "compare.thresholds must contain exactly the five required metric names")
    parsed_thresholds: dict[str, float] = {}
    for name in _THRESHOLD_NAMES:
        value = thresholds[name]
        if not _is_number(value, non_negative=True):
            raise CampaignValidationError("config-value", f"compare.thresholds.{name} must be finite and non-negative")
        parsed_thresholds[name] = float(value)
    raw_cases = payload.get("cases")
    if not isinstance(raw_cases, list) or not raw_cases:
        raise CampaignValidationError("config-value", "cases must be a non-empty array")
    if discovered < len(raw_cases):
        raise CampaignValidationError("config-value", "discovered must be >= cases length")
    cases: list[CampaignCase] = []
    seen_ids: set[str] = set()
    for index, raw_case in enumerate(raw_cases):
        prefix = f"cases[{index}]"
        if not isinstance(raw_case, dict) or set(raw_case) != _CASE_FIELDS:
            raise CampaignValidationError("config-schema", f"{prefix} fields are missing or unsupported")
        case_path = _require_absolute_file(Path(str(raw_case["caseFile"])), f"{prefix}.caseFile")
        case_id = raw_case.get("caseId")
        _require_safe_label(case_id, f"{prefix}.caseId")
        if case_id in seen_ids:
            raise CampaignValidationError("config-value", f"duplicate caseId: {case_id}")
        seen_ids.add(case_id)
        features = tuple(_require_safe_string_array(raw_case.get("features"), f"{prefix}.features", allow_empty=True))
        categories = tuple(_require_safe_string_array(raw_case.get("categories"), f"{prefix}.categories", allow_empty=True))
        cases.append(CampaignCase(case_path, case_id, features, categories))
    _validate_selection_cases(selection, cases)
    try:
        canonical = json.dumps(payload, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False)
    except (TypeError, ValueError) as error:
        raise CampaignValidationError("config-schema", f"config is not canonical JSON: {error}") from error
    config_hash = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
    return CampaignConfig(
        selection_file=selection_file,
        selection_hash=selection_hash.lower(),
        selection_frames=tuple(selection["frames"]),
        selected_cases={case["caseId"]: case for case in selection["cases"]},
        run={field: run[field] for field in sorted(_RUN_FIELDS)},
        discovered=discovered,
        focus_bones=tuple(focus_bones),
        thresholds=parsed_thresholds,
        cases=tuple(cases),
        config_hash=config_hash,
    )


def _validate_selection_cases(selection: dict[str, Any], cases: list[CampaignCase]) -> None:
    selected = selection["cases"]
    if len(selected) != len(cases):
        raise CampaignValidationError("selection", "campaign cases do not match the frozen selection count")
    selected_by_id = {case["caseId"]: case for case in selected}
    if [case["caseId"] for case in selected] != [case.case_id for case in cases]:
        raise CampaignValidationError("selection", "campaign case order does not match the frozen selection")
    for campaign_case in cases:
        selected_case = selected_by_id[campaign_case.case_id]
        if list(campaign_case.features) != selected_case["features"] or list(campaign_case.categories) != selected_case["categories"]:
            raise CampaignValidationError("selection", f"{campaign_case.case_id} tags do not match the frozen selection")


def _validate_loaded_case_selection(config: CampaignConfig, campaign_case: CampaignCase, oracle_case: OracleCase) -> None:
    selected_case = config.selected_cases[campaign_case.case_id]
    if oracle_case.name != campaign_case.case_id:
        raise CampaignValidationError("selection", f"{campaign_case.case_id} case name does not match")
    if oracle_case.pmx != Path(selected_case["pmx"]["path"]).resolve():
        raise CampaignValidationError("selection", f"{campaign_case.case_id} PMX does not match the frozen selection")
    if oracle_case.body_vmd != Path(selected_case["bodyVmd"]["path"]).resolve():
        raise CampaignValidationError("selection", f"{campaign_case.case_id} VMD does not match the frozen selection")
    if oracle_case.frames != config.selection_frames:
        raise CampaignValidationError("selection", f"{campaign_case.case_id} frames do not match the frozen selection")


def run_campaign(
    config_path: Path,
    snapshot_path: Path,
    state_path: Path | None = None,
    mmd_exe: str | Path | None = None,
    *,
    prepare_action: PrepareAction | None = None,
    record_action: RecordAction | None = None,
    compare_action: CompareAction | None = None,
    cleanup_action: CleanupAction | None = None,
    prepare_cleanup_action: CleanupAction | None = None,
    repo_root: Path | None = None,
    provenance_probe: ProvenanceProbe | None = None,
) -> dict[str, Any]:
    """Run a campaign without exposing state/execution exceptions to the CLI."""

    try:
        return _run_campaign_impl(
            config_path,
            snapshot_path,
            state_path,
            mmd_exe,
            prepare_action=prepare_action,
            record_action=record_action,
            compare_action=compare_action,
            cleanup_action=cleanup_action,
            prepare_cleanup_action=prepare_cleanup_action,
            repo_root=repo_root,
            provenance_probe=provenance_probe,
        )
    except CampaignValidationError as error:
        return _campaign_failure(error)
    except Exception as error:  # noqa: BLE001 - campaign boundary must be structured
        return _campaign_failure(CampaignValidationError("execution", f"campaign execution failed: {error.__class__.__name__}"))


def _run_campaign_impl(
    config_path: Path,
    snapshot_path: Path,
    state_path: Path | None = None,
    mmd_exe: str | Path | None = None,
    *,
    prepare_action: PrepareAction | None = None,
    record_action: RecordAction | None = None,
    compare_action: CompareAction | None = None,
    cleanup_action: CleanupAction | None = None,
    prepare_cleanup_action: CleanupAction | None = None,
    repo_root: Path | None = None,
    provenance_probe: ProvenanceProbe | None = None,
) -> dict[str, Any]:
    """Run a campaign sequentially and return a compact machine summary."""

    try:
        config = load_campaign_config(Path(config_path))
        try:
            verification = verify_selection(config.selection_file)
        except SelectionError as error:
            raise CampaignValidationError("selection", error.message) from error
        if verification["selectionHash"] != config.selection_hash:
            raise CampaignValidationError("selection", "selectionFile changed while starting campaign")
        config_path = Path(config_path).resolve()
        snapshot_path = _require_absolute_output(Path(snapshot_path), "snapshot")
        state_path = _default_state_path(snapshot_path) if state_path is None else _require_absolute_output(Path(state_path), "state")
        if config_path in {snapshot_path, state_path} or snapshot_path == state_path:
            raise CampaignValidationError("path", "config, snapshot, and state paths must be distinct")
        if not snapshot_path.parent.exists() or not state_path.parent.exists():
            raise CampaignValidationError("path", "snapshot and state parent directories must exist")
        repo_root = (repo_root or _default_repo_root()).resolve()
        provenance = (provenance_probe or _probe_repository)(repo_root)
        _validate_provenance(provenance)
        input_hashes = {
            case.case_id: _case_input_hash(case, config.selected_cases[case.case_id])
            for case in config.cases
        }
        state = _load_or_initialize_state(state_path, config, input_hashes, provenance["commitSha"])
    except CampaignValidationError as error:
        return _campaign_failure(error)
    except (OSError, UnicodeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        return _campaign_failure(CampaignValidationError("input", str(error)))

    prepare_fn = prepare_action or prepare_case
    if record_action is None:
        def record_fn(case: OracleCase, executable: str | Path | None) -> dict[str, Any]:
            return record_case(case, executable, retain_failure_artifacts=False)
    else:
        record_fn = record_action
    compare_fn = compare_action or _run_numeric_compare
    cleanup_fn = cleanup_action or cleanup_completed_case_run
    prepare_cleanup_fn = prepare_cleanup_action or cleanup_prepared_case_run
    event: dict[str, Any] = {
        "ok": False,
        "schemaVersion": CAMPAIGN_SCHEMA_VERSION,
        "configHash": config.config_hash,
        "state": str(state_path),
        "snapshot": str(snapshot_path),
        "casesProcessed": 0,
        "snapshotWritten": False,
        "error": None,
    }

    for campaign_case in config.cases:
        try:
            _reprobe_provenance(provenance_probe or _probe_repository, repo_root, provenance["commitSha"])
            _validate_case_input_hash(
                campaign_case,
                input_hashes[campaign_case.case_id],
                config.selected_cases[campaign_case.case_id],
            )
        except CampaignValidationError as error:
            return _stop(event, error.code, error.message, state)
        case_entry = state["cases"].get(campaign_case.case_id)
        if case_entry is not None:
            _validate_state_entry(case_entry, campaign_case, input_hashes[campaign_case.case_id])
            if not case_entry["provenanceValidated"]:
                _append_failure(case_entry, "result-unverified")
                if case_entry["cleanup"]["status"] in {"pending", "failed"}:
                    if case_entry.get("cleanupKind") not in {"prepared", "completed"} or not isinstance(case_entry.get("runDir"), str):
                        return _stop(event, "result-unverified", "unverified case result has no exact cleanup ownership", state)
                    try:
                        resume_case = load_case(campaign_case.case_file)
                    except CaseValidationError as error:
                        return _stop(event, "state", f"cannot validate resumable case: {error._message()}", state)
                    expected_run_dir = _run_dir_for_case(None, campaign_case, resume_case).resolve()
                    actual_run_dir = Path(case_entry["runDir"]).resolve()
                    if actual_run_dir != expected_run_dir:
                        return _stop(event, "state", "resumable cleanup runDir does not match the case-owned directory", state)
                    if not actual_run_dir.exists():
                        case_entry["cleanup"] = {"status": _STATE_CLEANED, "result": {"ok": True, "inferred": True}}
                    else:
                        cleanup_fn_for_entry = prepare_cleanup_fn if case_entry.get("cleanupKind") == "prepared" else cleanup_fn
                        cleanup_result = _safe_cleanup(cleanup_fn_for_entry, actual_run_dir)
                        if not cleanup_result.get("ok"):
                            case_entry["status"] = "completed"
                            case_entry["cleanup"] = {"status": "failed", "result": _compact_cleanup_result(cleanup_result)}
                            _append_failure(case_entry, "cleanup")
                            _persist_state(state_path, state)
                            return _stop(event, "cleanup", "unverified case cleanup failed", state)
                        case_entry["cleanup"] = {"status": _STATE_CLEANED, "result": _compact_cleanup_result(cleanup_result)}
                else:
                    case_entry["cleanup"] = {"status": _STATE_CLEANED, "result": {"ok": True, "inferred": True, "reason": "result-unverified-no-owned-artifacts"}}
                case_entry["status"] = "completed"
                _persist_state(state_path, state)
                return _stop(event, "result-unverified", "campaign state contains an unverified case result", state)
            if case_entry["cleanup"]["status"] == _STATE_CLEANED and case_entry["status"] == "completed":
                continue
            if case_entry["cleanup"]["status"] in {"pending", "failed"} and case_entry["status"] in {"running", "completed"}:
                if case_entry.get("cleanupKind") not in {"prepared", "completed"} or not isinstance(case_entry.get("runDir"), str):
                    return _stop(event, "state", "resumable cleanup state has no exact run directory", state)
                try:
                    resume_case = load_case(campaign_case.case_file)
                except CaseValidationError as error:
                    return _stop(event, "state", f"cannot validate resumable case: {error._message()}", state)
                expected_run_dir = _run_dir_for_case(None, campaign_case, resume_case).resolve()
                actual_run_dir = Path(case_entry["runDir"]).resolve()
                if actual_run_dir != expected_run_dir:
                    return _stop(event, "state", "resumable cleanup runDir does not match the case-owned directory", state)
                if not actual_run_dir.exists():
                    case_entry["cleanup"] = {"status": _STATE_CLEANED, "result": {"ok": True, "inferred": True}}
                    case_entry["status"] = "completed"
                    _persist_state(state_path, state)
                    continue
                cleanup_fn_for_entry = prepare_cleanup_fn if case_entry.get("cleanupKind") == "prepared" else cleanup_fn
                cleanup_result = _safe_cleanup(cleanup_fn_for_entry, actual_run_dir)
                if not cleanup_result.get("ok"):
                    case_entry["status"] = "completed"
                    case_entry["cleanup"] = {"status": "failed", "result": _compact_cleanup_result(cleanup_result)}
                    _append_failure(case_entry, "cleanup")
                    _persist_state(state_path, state)
                    return _stop(event, "cleanup", "resume cleanup failed", state)
                case_entry["cleanup"] = {"status": _STATE_CLEANED, "result": _compact_cleanup_result(cleanup_result)}
                case_entry["status"] = "completed"
                _persist_state(state_path, state)
                continue
            return _stop(event, "state", "state contains an unfinished case; safe resume is unavailable", state)

        event["casesProcessed"] += 1
        try:
            case = load_case(campaign_case.case_file)
            _validate_loaded_case_selection(config, campaign_case, case)
        except (CaseValidationError, CampaignValidationError) as error:
            if not _persist_failed_case_and_cleanup(state, state_path, campaign_case, input_hashes[campaign_case.case_id], None, "validation", prepare_cleanup_fn):
                message = error._message() if isinstance(error, CaseValidationError) else error.message
                return _stop(event, "validation", message, state)
            continue

        try:
            prepared = prepare_fn(case)
        except Exception as error:  # noqa: BLE001 - persist bounded phase failure
            run_dir = _run_dir_for_case(None, campaign_case, case)
            if not _persist_failed_case_and_cleanup(state, state_path, campaign_case, input_hashes[campaign_case.case_id], run_dir, "prepare", prepare_cleanup_fn):
                return _stop(event, "prepare", f"case preparation raised {error.__class__.__name__}", state)
            continue
        if not isinstance(prepared, dict):
            prepared = {"ok": False}
        try:
            run_dir = _run_dir_for_case(prepared, campaign_case, case)
        except CampaignValidationError as error:
            run_dir = _run_dir_for_case(None, campaign_case, case)
            if not _persist_failed_case_and_cleanup(state, state_path, campaign_case, input_hashes[campaign_case.case_id], run_dir, "prepare", prepare_cleanup_fn):
                return _stop(event, "prepare", error.message, state)
            continue
        prepared_ok = prepared.get("ok") is True and prepared.get("phase") == "complete"
        if not prepared_ok:
            if not _persist_failed_case_and_cleanup(state, state_path, campaign_case, input_hashes[campaign_case.case_id], run_dir, "prepare", prepare_cleanup_fn):
                return _stop(event, "prepare", "case preparation failed", state)
            continue

        try:
            recorded = record_fn(case, mmd_exe)
        except Exception as error:  # noqa: BLE001 - campaign records a bounded phase failure
            recorded = {"ok": False, "recorded": False, "error": str(error)}
        if not isinstance(recorded, dict):
            recorded = {"ok": False, "recorded": False}
        recorded_ok = recorded.get("ok") is True and recorded.get("recorded") is True
        failures: list[str] = [] if recorded_ok else ["record"]
        mmd_hash, mmd_hash_error = _mmd_executable_hash(recorded) if recorded_ok else (None, None)
        if mmd_hash_error:
            failures.append(mmd_hash_error)
        if mmd_hash is not None and _observed_mmd_hash(state) not in {None, mmd_hash}:
            failures.append("mmd-executable-conflict")
        outcome = CompareOutcome(False, False, {}, tuple(failures))
        if recorded_ok and mmd_hash_error is None:
            try:
                with tempfile.TemporaryDirectory(prefix=f".{campaign_case.case_id}-compare-") as temporary:
                    manifest = Path(temporary) / "manifest.json"
                    _write_numeric_manifest(manifest, campaign_case, case, run_dir, config)
                    compare_result = compare_fn(manifest, repo_root)
                    outcome = _parse_compare_result(compare_result, campaign_case.case_id, config.thresholds)
            except Exception:  # noqa: BLE001 - malformed compare is a bounded case failure
                outcome = CompareOutcome(False, False, {}, tuple(dict.fromkeys((*failures, "compare-execution"))))
            else:
                outcome = CompareOutcome(outcome.compared, outcome.passed, outcome.metrics, tuple(dict.fromkeys((*failures, *outcome.failures))))

        marker_ready = prepared_ok and _record_marker_exists(run_dir)
        entry = _state_entry(
            campaign_case, input_hashes[campaign_case.case_id], run_dir, prepared=prepared_ok, recorded=recorded_ok,
            compared=False, passed=False, metrics={}, failures=("provenance-unvalidated",),
            cleanup_status="pending" if marker_ready else "not-attempted", cleanup_kind="completed" if marker_ready else "none", mmd_hash=None,
            provenance_validated=False,
        )
        state["cases"][campaign_case.case_id] = entry
        _persist_state(state_path, state)
        try:
            _reprobe_provenance(provenance_probe or _probe_repository, repo_root, provenance["commitSha"])
            _validate_case_input_hash(
                campaign_case,
                input_hashes[campaign_case.case_id],
                config.selected_cases[campaign_case.case_id],
            )
        except Exception as error:  # noqa: BLE001 - probe failures must preserve the cleanup boundary
            # The result was produced while the repository provenance was no
            # longer trustworthy.  Do not let it contribute to the report,
            # but still clean owned recording artifacts before stopping.
            provenance_error = error if isinstance(error, CampaignValidationError) else CampaignValidationError("provenance", f"repository provenance probe failed: {error.__class__.__name__}")
            entry["compared"] = False
            entry["passed"] = False
            entry["metrics"] = {}
            _append_failure(entry, provenance_error.code)
            _persist_state(state_path, state)
            if marker_ready:
                entry["status"] = "completed"
                cleanup_result = _safe_cleanup(cleanup_fn, run_dir)
                if not cleanup_result.get("ok"):
                    _append_failure(entry, "cleanup")
                    entry["cleanup"] = {"status": "failed", "result": _compact_cleanup_result(cleanup_result)}
                    _persist_state(state_path, state)
                    return _stop(event, "cleanup", "case cleanup failed after provenance change", state)
                entry["cleanup"] = {"status": _STATE_CLEANED, "result": _compact_cleanup_result(cleanup_result)}
                _persist_state(state_path, state)
            else:
                entry["status"] = "failed"
                _persist_state(state_path, state)
            return _stop(event, provenance_error.code, provenance_error.message, state)
        entry["provenanceValidated"] = True
        entry["compared"] = outcome.compared
        entry["passed"] = outcome.passed
        entry["metrics"] = {name: outcome.metrics[name] for name in _METRIC_NAMES if name in outcome.metrics}
        entry["failures"] = list(outcome.failures)
        entry["mmdExecutableSha256"] = mmd_hash
        _persist_state(state_path, state)
        if marker_ready:
            entry["status"] = "completed"
            cleanup_result = _safe_cleanup(cleanup_fn, run_dir)
            if not cleanup_result.get("ok"):
                _append_failure(entry, "cleanup")
                entry["cleanup"] = {"status": "failed", "result": _compact_cleanup_result(cleanup_result)}
                _persist_state(state_path, state)
                return _stop(event, "cleanup", "case cleanup failed", state)
            entry["cleanup"] = {"status": _STATE_CLEANED, "result": _compact_cleanup_result(cleanup_result)}
            _persist_state(state_path, state)
        else:
            entry["status"] = "failed"
            _persist_state(state_path, state)
            return _stop(event, entry["failures"][0] if entry["failures"] else "record", "case did not produce a safely cleanable record", state)
        if "mmd-executable-conflict" in entry["failures"]:
            return _stop(event, "mmd-executable-conflict", "recorded cases used conflicting MMD executable hashes", state)

    try:
        _reprobe_provenance(provenance_probe or _probe_repository, repo_root, provenance["commitSha"])
    except CampaignValidationError as error:
        return _stop(event, error.code, error.message, state)
    aggregate = _build_snapshot(config, state, provenance)
    try:
        _atomic_write_json(snapshot_path, aggregate)
    except OSError as error:
        return _stop(event, "snapshot", f"cannot write final snapshot: {error}", state)
    event["snapshotWritten"] = True
    event["summary"] = aggregate["funnel"]
    event["ok"] = aggregate["funnel"]["compared"] > 0 and aggregate["funnel"]["passed"] == aggregate["funnel"]["compared"] and not aggregate["failures"]
    if not event["ok"]:
        code = "zero-comparable" if aggregate["funnel"]["compared"] == 0 else "quality-failures"
        message = "no cases produced a comparable numeric result" if code == "zero-comparable" else "campaign completed with recorded quality failures"
        event["error"] = {"code": code, "message": message}
    return event


def _read_json_object(path: Path, label: str) -> dict[str, Any]:
    try:
        raw = path.read_text(encoding="utf-8")
        value = json.loads(raw, parse_constant=lambda token: (_ for _ in ()).throw(ValueError(f"invalid constant {token}")))
    except (OSError, UnicodeError, json.JSONDecodeError, ValueError) as error:
        raise CampaignValidationError("json", f"{label} cannot be read as JSON: {error}") from error
    if not isinstance(value, dict):
        raise CampaignValidationError("json", f"{label} must be a JSON object")
    return value


def _require_absolute_file(path: Path, label: str) -> Path:
    if not path.is_absolute():
        raise CampaignValidationError("path", f"{label} must be an absolute path")
    if not path.exists():
        raise CampaignValidationError("path", f"{label} file does not exist")
    if not path.is_file():
        raise CampaignValidationError("path", f"{label} must be a regular file")
    return path.resolve()


def _require_absolute_output(path: Path, label: str) -> Path:
    if not path.is_absolute():
        raise CampaignValidationError("path", f"{label} must be an absolute path")
    if path.exists() and path.is_dir():
        raise CampaignValidationError("path", f"{label} must not be a directory")
    return path.resolve()


def _default_state_path(snapshot_path: Path) -> Path:
    return snapshot_path.with_name(f".{snapshot_path.stem}.campaign-state.json")


def _require_safe_string(value: object, field: str) -> str:
    if not isinstance(value, str) or not value or "\n" in value or "\r" in value or _looks_like_path(value):
        raise CampaignValidationError("config-value", f"{field} must be a non-empty path-free string")
    return value


def _require_safe_label(value: object, field: str) -> str:
    text = _require_safe_string(value, field)
    if ":" in text or text in {".", ".."} or ".." in text:
        raise CampaignValidationError("config-value", f"{field} must be a safe non-path identifier")
    return text


def _require_safe_string_array(value: object, field: str, *, allow_empty: bool = False) -> list[str]:
    if not isinstance(value, list) or (not allow_empty and not value):
        raise CampaignValidationError("config-value", f"{field} must be a non-empty string array")
    output: list[str] = []
    for index, item in enumerate(value):
        output.append(_require_safe_label(item, f"{field}[{index}]"))
    if len(set(output)) != len(output):
        raise CampaignValidationError("config-value", f"{field} must not contain duplicates")
    return output


def _looks_like_path(value: str) -> bool:
    return value.startswith(("/", "\\")) or bool(Path(value).drive) or "/" in value or "\\" in value


def _is_count(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _is_number(value: object, *, non_negative: bool) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and (not non_negative or value >= 0)


def _is_sha256(value: object) -> bool:
    return isinstance(value, str) and len(value) == 64 and all(character in _SHA256_RE for character in value)


def _is_commit_sha(value: object) -> bool:
    return isinstance(value, str) and 7 <= len(value) <= 64 and all(character in _SHA256_RE for character in value)


def _case_input_hash(campaign_case: CampaignCase, selected_case: dict[str, Any] | None = None) -> str:
    try:
        case_text = campaign_case.case_file.read_text(encoding="utf-8")
    except OSError as error:
        raise CampaignValidationError("input", f"{campaign_case.case_id} case JSON cannot be read: {error}") from error
    try:
        case_payload = json.loads(case_text)
    except (UnicodeError, json.JSONDecodeError):
        return _sha256_file(campaign_case.case_file)
    if not isinstance(case_payload, dict):
        return _sha256_file(campaign_case.case_file)
    input_data = case_payload.get("input")
    if not isinstance(input_data, dict):
        return _sha256_file(campaign_case.case_file)
    paths: list[tuple[str, Path]] = [("case", campaign_case.case_file)]
    for key in ("pmx", "bodyVmd", "cameraVmd"):
        value = input_data.get(key)
        if isinstance(value, str):
            paths.append((key, Path(value)))
    digest = hashlib.sha256()
    for label, path in paths:
        if not path.is_absolute() or not path.is_file():
            raise CampaignValidationError("input", f"{campaign_case.case_id} input {label} is not an existing absolute file")
        label_bytes = label.encode("utf-8")
        digest.update(len(label_bytes).to_bytes(4, "big"))
        digest.update(label_bytes)
        size_before = path.stat().st_size
        digest.update(size_before.to_bytes(8, "big"))
        asset_digest = hashlib.sha256()
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                digest.update(chunk)
                asset_digest.update(chunk)
        if path.stat().st_size != size_before:
            raise CampaignValidationError("input-drift", f"{campaign_case.case_id} input {label} changed while hashing")
        selected_field = {"pmx": "pmx", "bodyVmd": "bodyVmd"}.get(label)
        if selected_case is not None and selected_field is not None:
            frozen = selected_case[selected_field]
            if size_before != frozen["size"] or asset_digest.hexdigest() != frozen["sha256"]:
                raise CampaignValidationError("selection", f"{campaign_case.case_id} input {label} differs from the frozen asset")
    return digest.hexdigest()


def _validate_case_input_hash(
    campaign_case: CampaignCase,
    expected_hash: str,
    selected_case: dict[str, Any],
) -> None:
    if _case_input_hash(campaign_case, selected_case) != expected_hash:
        raise CampaignValidationError("input-drift", f"{campaign_case.case_id} input changed during campaign")


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _probe_repository(repo_root: Path) -> dict[str, str]:
    def run_git(*arguments: str) -> str:
        outcome = subprocess.run(("git", *arguments), cwd=repo_root, capture_output=True, text=True, encoding="utf-8", check=False)
        if outcome.returncode != 0:
            raise CampaignValidationError("provenance", f"git {' '.join(arguments)} failed with exit code {outcome.returncode}")
        return outcome.stdout.strip()

    commit_sha = run_git("rev-parse", "HEAD")
    if not _is_commit_sha(commit_sha):
        raise CampaignValidationError("provenance", "git HEAD is not a hexadecimal commit SHA")
    dirty = run_git("status", "--porcelain", "--untracked-files=all")
    return {"commitSha": commit_sha, "repositoryState": "dirty" if dirty else "clean"}


def _validate_provenance(provenance: object) -> None:
    if not isinstance(provenance, dict) or set(provenance) != {"commitSha", "repositoryState"}:
        raise CampaignValidationError("provenance", "provenance must contain exactly commitSha and repositoryState")
    if not _is_commit_sha(provenance.get("commitSha")):
        raise CampaignValidationError("provenance", "provenance.commitSha must be a hexadecimal commit SHA")
    if provenance.get("repositoryState") != "clean":
        raise CampaignValidationError("provenance-dirty", "campaign requires a clean repository checkout")


def _reprobe_provenance(probe: ProvenanceProbe, repo_root: Path, expected_commit_sha: str) -> None:
    current = probe(repo_root)
    _validate_provenance(current)
    if current["commitSha"] != expected_commit_sha:
        raise CampaignValidationError("provenance-mismatch", "repository HEAD changed during campaign")


def _load_or_initialize_state(state_path: Path, config: CampaignConfig, input_hashes: dict[str, str], commit_sha: str) -> dict[str, Any]:
    if not state_path.exists():
        return {"schemaVersion": _STATE_SCHEMA_VERSION, "configHash": config.config_hash, "commitSha": commit_sha, "cases": {}}
    state = _read_json_object(state_path, "campaign state")
    if set(state) != {"schemaVersion", "configHash", "commitSha", "cases"}:
        raise CampaignValidationError("state-schema", "campaign state fields are unsupported")
    if state.get("schemaVersion") != _STATE_SCHEMA_VERSION or state.get("configHash") != config.config_hash or state.get("commitSha") != commit_sha:
        raise CampaignValidationError("state-mismatch", "campaign state schema, configHash, or commitSha does not match campaign")
    cases = state.get("cases")
    if not isinstance(cases, dict):
        raise CampaignValidationError("state-schema", "campaign state cases must be an object")
    known_ids = {case.case_id for case in config.cases}
    if set(cases) - known_ids:
        raise CampaignValidationError("state-mismatch", "campaign state contains a case absent from config")
    observed: str | None = None
    for case_id, entry in cases.items():
        if not isinstance(entry, dict) or entry.get("inputHash") != input_hashes[case_id]:
            raise CampaignValidationError("state-mismatch", f"campaign state input hash is stale for {case_id}")
        value = entry.get("mmdExecutableSha256")
        if value is not None and not _is_sha256(value):
            raise CampaignValidationError("state-schema", f"campaign state MMD executable hash is invalid for {case_id}")
        if value is not None:
            if observed is not None and observed != value:
                raise CampaignValidationError("mmd-executable-conflict", "campaign state contains conflicting MMD executable hashes")
            observed = value
    return state


def _validate_state_entry(entry: dict[str, Any], campaign_case: CampaignCase, input_hash: str) -> None:
    if entry.get("inputHash") != input_hash or entry.get("caseId") != campaign_case.case_id:
        raise CampaignValidationError("state-mismatch", f"campaign state input or case identity is stale for {campaign_case.case_id}")
    if entry.get("features") != list(campaign_case.features) or entry.get("categories") != list(campaign_case.categories):
        raise CampaignValidationError("state-mismatch", f"campaign state tags are stale for {campaign_case.case_id}")
    run_dir = entry.get("runDir")
    if run_dir is not None and (not isinstance(run_dir, str) or not run_dir):
        raise CampaignValidationError("state-schema", f"campaign state runDir is invalid for {campaign_case.case_id}")
    if entry.get("status") not in {"running", "completed", "failed"}:
        raise CampaignValidationError("state-schema", f"campaign state status is invalid for {campaign_case.case_id}")
    for field in ("prepared", "recorded", "compared", "passed"):
        if not isinstance(entry.get(field), bool):
            raise CampaignValidationError("state-schema", f"campaign state {field} is invalid for {campaign_case.case_id}")
    provenance_validated = entry.get("provenanceValidated")
    if not isinstance(provenance_validated, bool):
        raise CampaignValidationError("state-schema", f"campaign state provenanceValidated is invalid for {campaign_case.case_id}")
    if not isinstance(entry.get("metrics"), dict) or not all(metric in _METRIC_NAMES and _is_number(value, non_negative=True) for metric, value in entry["metrics"].items()):
        raise CampaignValidationError("state-schema", f"campaign state metrics are invalid for {campaign_case.case_id}")
    if entry["passed"] and not entry["compared"]:
        raise CampaignValidationError("state-schema", f"campaign state passed result is not comparable for {campaign_case.case_id}")
    if entry["compared"] and not entry["recorded"]:
        raise CampaignValidationError("state-schema", f"campaign state compared result was not recorded for {campaign_case.case_id}")
    if entry["recorded"] and not entry["prepared"]:
        raise CampaignValidationError("state-schema", f"campaign state recorded result was not prepared for {campaign_case.case_id}")
    if entry.get("compared") is True and set(entry["metrics"]) != set(_METRIC_NAMES):
        raise CampaignValidationError("state-schema", f"campaign state comparable metrics are incomplete for {campaign_case.case_id}")
    if not provenance_validated and (entry["compared"] or entry["passed"] or entry["metrics"]):
        raise CampaignValidationError("state-schema", f"unvalidated campaign state result must remain non-comparable for {campaign_case.case_id}")
    if not isinstance(entry.get("failures"), list) or any(not isinstance(value, str) for value in entry["failures"]):
        raise CampaignValidationError("state-schema", f"campaign state failures are invalid for {campaign_case.case_id}")
    cleanup_kind = entry.get("cleanupKind")
    if cleanup_kind not in {"none", "prepared", "completed"}:
        raise CampaignValidationError("state-schema", f"campaign state cleanupKind is invalid for {campaign_case.case_id}")
    if cleanup_kind in {"prepared", "completed"} and not isinstance(run_dir, str):
        raise CampaignValidationError("state-schema", f"campaign state runDir is required for {campaign_case.case_id}")
    value = entry.get("mmdExecutableSha256")
    if value is not None and not _is_sha256(value):
        raise CampaignValidationError("state-schema", f"campaign state MMD executable hash is invalid for {campaign_case.case_id}")
    cleanup = entry.get("cleanup")
    if not isinstance(cleanup, dict) or cleanup.get("status") not in {"pending", "failed", "cleaned", "not-attempted"}:
        raise CampaignValidationError("state-schema", f"campaign state cleanup status is invalid for {campaign_case.case_id}")


def _state_entry(campaign_case: CampaignCase, input_hash: str, run_dir: Path | None, *, prepared: bool, recorded: bool, compared: bool, passed: bool, metrics: dict[str, float], failures: tuple[str, ...], cleanup_status: str, cleanup_kind: str, mmd_hash: str | None, provenance_validated: bool = True) -> dict[str, Any]:
    return {
        "caseId": campaign_case.case_id,
        "inputHash": input_hash,
        "features": list(campaign_case.features),
        "categories": list(campaign_case.categories),
        "runDir": str(run_dir.resolve()) if run_dir is not None else None,
        "status": "running",
        "prepared": prepared,
        "recorded": recorded,
        "compared": compared,
        "passed": passed,
        "provenanceValidated": provenance_validated,
        "metrics": {name: metrics[name] for name in _METRIC_NAMES if name in metrics},
        "failures": list(dict.fromkeys(failures)),
        "cleanupKind": cleanup_kind,
        "mmdExecutableSha256": mmd_hash,
        "cleanup": {"status": cleanup_status},
    }


def _persist_failed_case_and_cleanup(state: dict[str, Any], state_path: Path, campaign_case: CampaignCase, input_hash: str, run_dir: Path | None, failure: str, cleanup_fn: CleanupAction) -> bool:
    has_artifacts = run_dir is not None and run_dir.exists()
    entry = _state_entry(campaign_case, input_hash, run_dir, prepared=False, recorded=False, compared=False, passed=False, metrics={}, failures=(failure,), cleanup_status="pending" if has_artifacts else "cleaned", cleanup_kind="prepared" if has_artifacts else "none", mmd_hash=None)
    state["cases"][campaign_case.case_id] = entry
    _persist_state(state_path, state)
    if not has_artifacts:
        entry["status"] = "completed"
        entry["cleanup"]["result"] = {"ok": True, "inferred": True, "reason": "no-artifacts"}
        _persist_state(state_path, state)
        return True
    cleanup_result = _safe_cleanup(cleanup_fn, run_dir)
    if not cleanup_result.get("ok"):
        entry["status"] = "completed"
        entry["cleanup"] = {"status": "failed", "result": _compact_cleanup_result(cleanup_result)}
        _append_failure(entry, "cleanup")
        _persist_state(state_path, state)
        return False
    entry["status"] = "completed"
    entry["cleanup"] = {"status": _STATE_CLEANED, "result": _compact_cleanup_result(cleanup_result)}
    _persist_state(state_path, state)
    return True


def _run_dir_for_case(prepared: dict[str, Any] | None, campaign_case: CampaignCase, case: OracleCase | None = None) -> Path:
    artifact_name = prepared.get("artifactName") if isinstance(prepared, dict) else None
    expected_name = _artifact_name(case.name if case is not None else campaign_case.case_id)
    if not isinstance(artifact_name, str) or not artifact_name:
        artifact_name = expected_name
    if artifact_name != expected_name:
        raise CampaignValidationError("artifact", "prepare artifactName does not match the case-owned artifact directory")
    output_root = case.output_root if case is not None else Path(prepared.get("outputRoot", ".")) if isinstance(prepared, dict) else Path(".")
    return Path(output_root) / artifact_name


def _record_marker_exists(run_dir: Path) -> bool:
    return (run_dir / "prepare-result.json").is_file() and (run_dir / "record-result.json").is_file()


def _write_numeric_manifest(path: Path, campaign_case: CampaignCase, case: OracleCase, run_dir: Path, config: CampaignConfig) -> None:
    staged_model = run_dir / "model.mmd-utf16.pmx"
    model = staged_model if staged_model.is_file() else case.pmx
    output = run_dir / "oracle.actual.jsonl"
    if not model.is_file() or not output.is_file():
        raise CampaignValidationError("record", f"recorded artifacts are missing for {campaign_case.case_id}")
    payload = {
        "schemaVersion": 1,
        "defaults": {"compare": {"epsilon": config.thresholds["maxAbsError"]}, "focus": {"bones": list(config.focus_bones)}},
        "cases": [{"name": campaign_case.case_id, "kind": "motion-numeric", "assets": {"model": str(model), "motion": str(case.body_vmd)}, "frames": list(case.frames), "oracle": {"format": "jsonl", "path": str(output)}, "metadata": {"focus": {"bones": list(config.focus_bones)}}}],
    }
    _atomic_write_json(path, payload)


def _run_numeric_compare(manifest_path: Path, repo_root: Path) -> CommandResult:
    from .prepare import SubprocessRunner
    command = ("cargo", "run", "-q", "-p", "mmd-anim-cli", "--", "verify", str(manifest_path), "--mode", "numeric", "--json")
    return SubprocessRunner(timeout_seconds=None).run(command, repo_root)


def _parse_compare_result(result: CommandResult, case_id: str, thresholds: dict[str, float]) -> CompareOutcome:
    if result.exit_code != 0:
        return CompareOutcome(False, False, {}, ("compare-command",))
    try:
        value = json.loads(result.stdout)
    except (TypeError, json.JSONDecodeError):
        return CompareOutcome(False, False, {}, ("compare-malformed",))
    if not isinstance(value, dict):
        return CompareOutcome(False, False, {}, ("compare-malformed",))
    per_case = value.get("perCase")
    if not isinstance(per_case, list) or len(per_case) != 1 or not isinstance(per_case[0], dict):
        return CompareOutcome(False, False, {}, ("compare-shape",))
    current = per_case[0]
    if current.get("name") != case_id:
        return CompareOutcome(False, False, {}, ("compare-case-mismatch",))
    metrics: dict[str, float] = {}
    for metric in _METRIC_NAMES:
        raw_metric = current.get(metric)
        if not _is_number(raw_metric, non_negative=True):
            return CompareOutcome(False, False, {}, ("compare-metrics",))
        metrics[metric] = float(raw_metric)
    status = current.get("status")
    if status not in {"ok", "mismatch"}:
        return CompareOutcome(False, False, {}, ("compare-status-schema",))
    mismatch_count = current.get("mismatchCount")
    compared_frames = current.get("comparedFrames")
    compared_bones = current.get("comparedBones")
    if not all(_is_count(item) for item in (mismatch_count, compared_frames, compared_bones)):
        return CompareOutcome(False, False, {}, ("compare-fields",))
    if (status == "ok") != (mismatch_count == 0):
        return CompareOutcome(False, False, {}, ("compare-status-schema",))
    failures: list[str] = []
    for field in ("missing", "importErrors", "noTargets"):
        count = current.get(field)
        if not _is_count(count):
            return CompareOutcome(False, False, {}, ("compare-fields",))
        if count != 0:
            failures.append(f"compare-{field}")
    skipped = current.get("skippedTargets")
    if not isinstance(skipped, list) or any(not isinstance(item, str) for item in skipped):
        return CompareOutcome(False, False, {}, ("compare-fields",))
    if skipped:
        failures.append("compare-skippedTargets")
    if compared_frames == 0 or compared_bones == 0:
        failures.append("compare-no-targets")
    for metric, metric_value in metrics.items():
        if metric_value > thresholds[metric]:
            failures.append("threshold")
    structural_failures = any(item.startswith("compare-") for item in failures)
    compared = not structural_failures and compared_frames > 0 and compared_bones > 0
    if not compared:
        return CompareOutcome(False, False, {}, tuple(dict.fromkeys(failures)))
    unique_failures = tuple(dict.fromkeys(failures))
    return CompareOutcome(True, not unique_failures, metrics, unique_failures)


def _mmd_executable_hash(recorded: dict[str, Any]) -> tuple[str | None, str | None]:
    identity = recorded.get("mmdExecutable")
    if identity is None:
        return None, None
    if not isinstance(identity, dict):
        return None, "mmd-executable-hash"
    value = identity.get("sha256")
    if value is None:
        return None, None
    if not _is_sha256(value):
        return None, "mmd-executable-hash"
    return value.lower(), None


def _observed_mmd_hash(state: dict[str, Any]) -> str | None:
    values = {entry.get("mmdExecutableSha256") for entry in state.get("cases", {}).values() if entry.get("mmdExecutableSha256") is not None}
    if len(values) > 1:
        raise CampaignValidationError("mmd-executable-conflict", "campaign state contains conflicting MMD executable hashes")
    return next(iter(values), None)


def _build_snapshot(config: CampaignConfig, state: dict[str, Any], provenance: dict[str, str]) -> dict[str, Any]:
    entries = list(state["cases"].values())
    funnel = {"discovered": config.discovered, "selected": len(config.cases), "prepared": sum(1 for entry in entries if entry.get("prepared") is True), "recorded": sum(1 for entry in entries if entry.get("recorded") is True), "compared": sum(1 for entry in entries if entry.get("compared") is True), "passed": sum(1 for entry in entries if entry.get("passed") is True)}
    failures: dict[str, int] = {}
    for entry in entries:
        for failure in entry.get("failures", []):
            failures[failure] = failures.get(failure, 0) + 1
    metrics: dict[str, dict[str, float]] = {}
    for metric in _METRIC_NAMES:
        values = sorted(float(entry["metrics"][metric]) for entry in entries if entry.get("compared") is True and metric in entry.get("metrics", {}))
        if values:
            metrics[metric] = {"p50": _nearest_rank(values, 0.50), "p95": _nearest_rank(values, 0.95), "p99": _nearest_rank(values, 0.99), "max": values[-1]}
    features = _tag_summaries(config.cases, entries, "features")
    categories = _tag_summaries(config.cases, entries, "categories")
    worst_cases: list[dict[str, Any]] = []
    case_by_id = {case.case_id: case for case in config.cases}
    for entry in entries:
        if entry.get("compared") is not True:
            continue
        case = case_by_id.get(entry.get("caseId"))
        category = case.categories[0] if case and case.categories else "uncategorized"
        for metric in _METRIC_NAMES:
            if metric in entry.get("metrics", {}):
                value = float(entry["metrics"][metric])
                threshold = config.thresholds[metric]
                ratio = math.inf if threshold == 0 and value > 0 else (0.0 if threshold == 0 else value / threshold)
                worst_cases.append({"caseId": entry["caseId"], "category": category, "metric": metric, "value": value, "result": "pass" if value <= threshold else "fail", "_ratio": ratio})
    worst_cases.sort(key=lambda item: (-item["_ratio"], -item["value"], item["caseId"], item["metric"]))
    for item in worst_cases:
        item.pop("_ratio", None)
    observed = _observed_mmd_hash(state)
    run = {**config.run, "commitSha": provenance["commitSha"], "repositoryState": provenance["repositoryState"], "mmdVersionSource": "config-self-reported", "dumperVersionSource": "config-self-reported", "selectionHash": config.selection_hash, "configHash": config.config_hash, "mmdExecutableSha256": observed or "not-observed"}
    return {"schemaVersion": 1, "run": run, "funnel": funnel, "thresholds": {metric: config.thresholds[metric] for metric in _THRESHOLD_NAMES}, "metrics": metrics, "failures": failures, "features": features, "categories": categories, "worstCases": worst_cases[:20], "rawArtifacts": {"retained": False}}


def _tag_summaries(cases: tuple[CampaignCase, ...], entries: list[dict[str, Any]], field: str) -> dict[str, dict[str, int]]:
    summaries: dict[str, dict[str, int]] = {}
    for case in cases:
        for tag in getattr(case, field):
            summaries.setdefault(tag, {"selected": 0, "compared": 0, "passed": 0})["selected"] += 1
    for entry in entries:
        for tag in entry.get(field, []):
            summary = summaries.setdefault(tag, {"selected": 0, "compared": 0, "passed": 0})
            if entry.get("compared") is True:
                summary["compared"] += 1
            if entry.get("passed") is True:
                summary["passed"] += 1
    return {tag: summaries[tag] for tag in sorted(summaries)}


def _nearest_rank(values: list[float], quantile: float) -> float:
    return values[max(0, math.ceil(quantile * len(values)) - 1)]


def _safe_cleanup(cleanup_fn: CleanupAction, run_dir: Path) -> dict[str, Any]:
    try:
        result = cleanup_fn(run_dir)
    except Exception:  # noqa: BLE001 - caller must stop on cleanup errors
        return {"ok": False, "error": {"code": "cleanup-exception"}}
    return result if isinstance(result, dict) else {"ok": False, "error": {"code": "cleanup-result"}}


def _append_failure(entry: dict[str, Any], failure: str) -> None:
    if failure not in entry["failures"]:
        entry["failures"].append(failure)


def _compact_cleanup_result(result: dict[str, Any]) -> dict[str, Any]:
    return {"ok": result.get("ok") is True, "deletedCount": len(result.get("deleted", [])) if isinstance(result.get("deleted"), list) else 0, "removedRunDir": result.get("removedRunDir") is True, "errorCode": result.get("error", {}).get("code") if isinstance(result.get("error"), dict) else None}


def _persist_state(state_path: Path, state: dict[str, Any]) -> None:
    _atomic_write_json(state_path, state)


def _atomic_write_json(path: Path, value: dict[str, Any]) -> None:
    if not path.parent.exists():
        raise OSError(f"parent directory does not exist: {path.parent}")
    descriptor: int | None = None
    temporary: Path | None = None
    try:
        descriptor, temporary_name = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.", suffix=".tmp")
        temporary = Path(temporary_name)
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
            descriptor = None
            json.dump(value, stream, ensure_ascii=True, sort_keys=True, indent=2, allow_nan=False)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        temporary = None
    finally:
        if descriptor is not None:
            os.close(descriptor)
        if temporary is not None:
            try:
                temporary.unlink()
            except OSError:
                pass


def _campaign_failure(error: CampaignValidationError) -> dict[str, Any]:
    return {"ok": False, "schemaVersion": CAMPAIGN_SCHEMA_VERSION, "casesProcessed": 0, "snapshotWritten": False, "error": error.as_dict()}


def _stop(event: dict[str, Any], code: str, message: str, state: dict[str, Any]) -> dict[str, Any]:
    event["error"] = {"code": code, "message": message}
    event["stateCases"] = len(state.get("cases", {}))
    return event
