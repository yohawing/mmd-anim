"""Small maintainer-only runner for a fixed local motion quality manifest.

The manifest directly lists local PMX/VMD pairs.  The runner keeps one compact
state file, processes cases sequentially, and deletes only artifacts proven to
be owned by the existing prepare/record markers.
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
from .case import OracleCase
from .prepare import CommandResult, _artifact_name, _default_repo_root, prepare_case
from .record import record_case

CAMPAIGN_SCHEMA_VERSION = 1
_METRIC_NAMES = ("translationMaxError", "translationRmsError", "rotationMaxAngleRad", "rotationRmsAngleRad", "maxAbsError")
_RUN_FIELDS = ("mmdVersion", "dumperVersion", "timestamp", "samplingPolicy")
_CASE_FIELDS = ("caseId", "pmx", "bodyVmd")
_DEFAULT_RUN = {
    "mmdVersion": "9.32-x64",
    "dumperVersion": "local-manifest",
    "timestamp": "local",
    "samplingPolicy": "fixed-local-manifest",
}
_DEFAULT_THRESHOLDS = {
    "translationMaxError": 0.003,
    "translationRmsError": 0.001,
    "rotationMaxAngleRad": 0.003,
    "rotationRmsAngleRad": 0.001,
    "maxAbsError": 0.003,
}


class CampaignValidationError(ValueError):
    """A concise, stable error at the CLI boundary."""

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
    oracle_case: OracleCase

    @property
    def frames(self) -> tuple[int, ...]:
        return self.oracle_case.frames


@dataclass(frozen=True)
class CampaignConfig:
    manifest_file: Path
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
    """Load and lightly validate the one ignored local campaign manifest."""

    manifest = _require_absolute_file(Path(path), "manifest")
    payload = _read_json_object(manifest, "manifest")
    if payload.get("schemaVersion") != CAMPAIGN_SCHEMA_VERSION:
        raise CampaignValidationError("manifest", "schemaVersion must be 1")
    run_value = {**_DEFAULT_RUN, **(payload.get("run", {}) if isinstance(payload.get("run", {}), dict) else {})}
    if not isinstance(run_value, dict):
        raise CampaignValidationError("manifest", "run must be an object")
    run: dict[str, str] = {}
    for field in _RUN_FIELDS:
        value = run_value.get(field)
        if not isinstance(value, str) or not value.strip() or "\n" in value or "\r" in value:
            raise CampaignValidationError("manifest", f"run.{field} must be a non-empty string")
        run[field] = value

    compare = payload.get("compare", {})
    if not isinstance(compare, dict):
        raise CampaignValidationError("manifest", "compare must be an object")
    focus_bones = tuple(_strings(compare.get("focusBones", []), "compare.focusBones", allow_empty=True))
    raw_thresholds = {**_DEFAULT_THRESHOLDS, **(compare.get("thresholds", {}) if isinstance(compare.get("thresholds", {}), dict) else {})}
    if not isinstance(raw_thresholds, dict):
        raise CampaignValidationError("manifest", "compare.thresholds must be an object")
    thresholds: dict[str, float] = {}
    for name in _METRIC_NAMES:
        value = raw_thresholds.get(name)
        if not _number(value):
            raise CampaignValidationError("manifest", f"compare.thresholds.{name} must be finite and non-negative")
        thresholds[name] = float(value)

    raw_cases = payload.get("cases")
    if not isinstance(raw_cases, list) or not raw_cases:
        raise CampaignValidationError("manifest", "cases must be a non-empty array")
    default_frames = _frames(payload.get("frames", [0, 15, 30, 60, 120]), "frames")
    default_output_root = _output_directory(payload.get("outputRoot", str(manifest.parent / "runs-v1")), "outputRoot")
    cases: list[CampaignCase] = []
    seen: set[str] = set()
    for index, raw_case in enumerate(raw_cases):
        if not isinstance(raw_case, dict):
            raise CampaignValidationError("manifest", f"cases[{index}] must be an object")
        missing = [field for field in _CASE_FIELDS if field not in raw_case]
        if missing:
            raise CampaignValidationError("manifest", f"cases[{index}] missing {', '.join(missing)}")
        case_id = _label(raw_case["caseId"], f"cases[{index}].caseId")
        if case_id in seen:
            raise CampaignValidationError("manifest", f"duplicate caseId: {case_id}")
        seen.add(case_id)
        pmx = _existing_file(raw_case["pmx"], f"cases[{index}].pmx")
        body_vmd = _existing_file(raw_case["bodyVmd"], f"cases[{index}].bodyVmd")
        camera_raw = raw_case.get("cameraVmd")
        camera_vmd = None if camera_raw in (None, "") else _existing_file(camera_raw, f"cases[{index}].cameraVmd")
        frames = _frames(raw_case.get("frames", list(default_frames)), f"cases[{index}].frames")
        output_root = _output_directory(raw_case.get("outputRoot", str(default_output_root)), f"cases[{index}].outputRoot")
        features = tuple(_strings(raw_case.get("features", []), f"cases[{index}].features", allow_empty=True))
        categories = tuple(_strings(raw_case.get("categories", []), f"cases[{index}].categories", allow_empty=True))
        requested = tuple(_strings(raw_case.get("requestedFeatures", []), f"cases[{index}].requestedFeatures", allow_empty=True))
        oracle_case = OracleCase(
            schema_version=1, name=case_id, pmx=pmx, body_vmd=body_vmd, camera_vmd=camera_vmd,
            frames=frames, output_root=output_root, generator_backend="python-rust", record_opt_in=True,
            dialog_opt_in=False, requested_features=requested, source_path=manifest,
        )
        cases.append(CampaignCase(manifest, case_id, features, categories, oracle_case))
    discovery = payload.get("discovery", {})
    discovered_default = discovery.get("selected", len(cases)) if isinstance(discovery, dict) else len(cases)
    discovered = payload.get("discovered", discovered_default)
    if isinstance(discovered, bool) or not isinstance(discovered, int) or discovered < len(cases):
        raise CampaignValidationError("manifest", "discovered must be an integer >= cases length")
    digest = hashlib.sha256(_canonical(payload).encode("utf-8")).hexdigest()
    return CampaignConfig(manifest, run, discovered, tuple(focus_bones), thresholds, tuple(cases), digest)


def run_campaign(
    config_path: Path, snapshot_path: Path, state_path: Path | None = None, mmd_exe: str | Path | None = None, *,
    prepare_action: PrepareAction | None = None, record_action: RecordAction | None = None,
    compare_action: CompareAction | None = None, cleanup_action: CleanupAction | None = None,
    prepare_cleanup_action: CleanupAction | None = None, repo_root: Path | None = None,
    provenance_probe: ProvenanceProbe | None = None,
) -> dict[str, Any]:
    """Run each manifest entry once, resuming unfinished entries conservatively."""

    try:
        config = load_campaign_config(Path(config_path))
        snapshot = _require_output(Path(snapshot_path), "snapshot")
        state_file = _require_output(Path(state_path), "state") if state_path is not None else _default_state_path(snapshot)
        if snapshot in {config.manifest_file, state_file} or state_file == config.manifest_file:
            raise CampaignValidationError("path", "manifest, snapshot, and state must be different files")
        root = (repo_root or _default_repo_root()).resolve()
        probe = provenance_probe or _probe_repository
        provenance = probe(root)
        _require_clean_provenance(provenance)
        input_hashes = {case.case_id: _case_input_hash(config.manifest_file, case) for case in config.cases}
        state = _load_state(state_file, config, input_hashes, provenance["commitSha"])
    except CampaignValidationError as error:
        return _campaign_failure(error)
    except (OSError, UnicodeError, json.JSONDecodeError, subprocess.SubprocessError) as error:
        return _campaign_failure(CampaignValidationError("input", str(error)))

    prepare_fn = prepare_action or prepare_case
    record_fn = record_action or (lambda case, executable: record_case(case, executable, retain_failure_artifacts=False))
    compare_fn = compare_action or _run_numeric_compare
    cleanup_fn = cleanup_action or cleanup_completed_case_run
    prepared_cleanup_fn = prepare_cleanup_action or cleanup_prepared_case_run
    event: dict[str, Any] = {"ok": False, "schemaVersion": 1, "configHash": config.config_hash, "state": str(state_file), "snapshot": str(snapshot), "casesProcessed": 0, "snapshotWritten": False, "error": None}

    for campaign_case in config.cases:
        case_id = campaign_case.case_id
        old = state["cases"].get(case_id)
        if old is not None and old.get("cleanup") == "cleaned" and old.get("status") == "complete":
            continue
        if old is not None:
            if not _resume_cleanup(old, cleanup_fn, prepared_cleanup_fn):
                return _stop(event, "cleanup", f"cannot safely resume {case_id}", state, state_file)
            state["cases"].pop(case_id, None)
            _persist_state(state_file, state)

        event["casesProcessed"] += 1
        case = campaign_case.oracle_case
        run_dir = _run_dir(campaign_case, case)
        entry = _entry(campaign_case, input_hashes[case_id], run_dir)
        state["cases"][case_id] = entry
        _persist_state(state_file, state)
        try:
            prepared = prepare_fn(case)
        except Exception as error:  # noqa: BLE001 - preserve bounded case result
            prepared = {"ok": False, "error": str(error)}
        if not isinstance(prepared, dict):
            prepared = {"ok": False}
        entry["prepared"] = prepared.get("ok") is True and prepared.get("phase") == "complete"
        if not entry["prepared"]:
            entry["failures"] = ["prepare"]
            if not _cleanup_entry(entry, run_dir, prepared_cleanup_fn):
                _persist_state(state_file, state)
                return _stop(event, "cleanup", f"prepare cleanup failed for {case_id}", state, state_file)
            _persist_state(state_file, state)
            continue

        try:
            recorded = record_fn(case, mmd_exe)
        except Exception as error:  # noqa: BLE001 - preserve bounded case result
            recorded = {"ok": False, "recorded": False, "error": str(error)}
        if not isinstance(recorded, dict):
            recorded = {"ok": False, "recorded": False}
        entry["recorded"] = recorded.get("ok") is True and recorded.get("recorded") is True
        mmd_hash, hash_error = _mmd_executable_hash(recorded) if entry["recorded"] else (None, None)
        entry["mmdExecutableSha256"] = mmd_hash
        failures: list[str] = [] if entry["recorded"] else ["record"]
        if hash_error:
            failures.append(hash_error)
        outcome = CompareOutcome(False, False, {}, tuple(failures))
        if entry["recorded"] and hash_error is None:
            try:
                with tempfile.TemporaryDirectory(prefix=f".{case_id}-compare-") as temporary:
                    manifest = Path(temporary) / "manifest.json"
                    _write_numeric_manifest(manifest, campaign_case, case, run_dir, config)
                    outcome = _parse_compare_result(compare_fn(manifest, root), case_id, config.thresholds)
            except Exception:  # noqa: BLE001 - malformed compare is a case failure
                outcome = CompareOutcome(False, False, {}, ("compare-execution",))
        entry["compared"] = outcome.compared
        entry["passed"] = outcome.passed
        entry["metrics"] = outcome.metrics
        entry["failures"] = list(dict.fromkeys((*failures, *outcome.failures)))
        entry["status"] = "complete"
        entry["cleanup"] = "pending"
        _persist_state(state_file, state)
        marker_cleanup = cleanup_fn if (run_dir / "record-result.json").is_file() else prepared_cleanup_fn
        if not _cleanup_entry(entry, run_dir, marker_cleanup):
            _persist_state(state_file, state)
            return _stop(event, "cleanup", f"cleanup failed for {case_id}", state, state_file)
        _persist_state(state_file, state)

    try:
        _require_clean_provenance(probe(root), expected=provenance["commitSha"])
        aggregate = _build_snapshot(config, state, provenance)
        _atomic_write_json(snapshot, aggregate)
    except CampaignValidationError as error:
        return _stop(event, error.code, error.message, state, state_file)
    except OSError as error:
        return _stop(event, "snapshot", str(error), state, state_file)
    event["snapshotWritten"] = True
    event["summary"] = aggregate["funnel"]
    event["ok"] = aggregate["funnel"]["compared"] > 0 and aggregate["funnel"]["passed"] == aggregate["funnel"]["compared"] and not aggregate["failures"]
    if not event["ok"]:
        event["error"] = {"code": "zero-comparable" if aggregate["funnel"]["compared"] == 0 else "quality-failures", "message": "campaign completed with quality failures"}
    return event


def _read_json_object(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise CampaignValidationError("json", f"{label} cannot be read as JSON: {error}") from error
    if not isinstance(value, dict):
        raise CampaignValidationError("json", f"{label} must be an object")
    return value


def _require_absolute_file(path: Path, label: str) -> Path:
    if not path.is_absolute() or not path.is_file():
        raise CampaignValidationError("path", f"{label} must be an existing absolute file")
    return path.resolve()


def _require_output(path: Path, label: str) -> Path:
    if not path.is_absolute() or (path.exists() and path.is_dir()) or not path.parent.exists():
        raise CampaignValidationError("path", f"{label} must be an absolute file path with an existing parent")
    return path.resolve()


def _existing_file(value: object, label: str) -> Path:
    expected_hash = None
    if isinstance(value, dict):
        expected_hash = value.get("sha256")
        value = value.get("path")
    if not isinstance(value, str):
        raise CampaignValidationError("manifest", f"{label} must be an absolute file path")
    path = Path(value)
    if not path.is_absolute() or not path.is_file():
        raise CampaignValidationError("manifest", f"{label} must be an existing absolute file")
    path = path.resolve()
    if expected_hash is not None:
        if not isinstance(expected_hash, str) or len(expected_hash) != 64 or any(char not in "0123456789abcdefABCDEF" for char in expected_hash):
            raise CampaignValidationError("manifest", f"{label}.sha256 must be a hexadecimal SHA-256")
        if _sha256_file(path) != expected_hash.lower():
            raise CampaignValidationError("manifest", f"{label} content hash does not match the fixed manifest")
    return path


def _output_directory(value: object, label: str) -> Path:
    if not isinstance(value, str):
        raise CampaignValidationError("manifest", f"{label} must be an absolute directory path")
    path = Path(value)
    if not path.is_absolute() or (path.exists() and not path.is_dir()) or not path.parent.is_dir():
        raise CampaignValidationError("manifest", f"{label} must be an absolute directory path with an existing parent")
    return path.resolve()


def _label(value: object, label: str) -> str:
    if not isinstance(value, str) or not value.strip() or any(char in value for char in "\r\n/\\:"):
        raise CampaignValidationError("manifest", f"{label} must be a safe identifier")
    return value


def _strings(value: object, label: str, *, allow_empty: bool) -> list[str]:
    if not isinstance(value, list) or (not allow_empty and not value) or any(not isinstance(item, str) or not item.strip() for item in value):
        raise CampaignValidationError("manifest", f"{label} must be an array of strings")
    return list(value)


def _frames(value: object, label: str) -> tuple[int, ...]:
    if not isinstance(value, list) or not value or any(isinstance(item, bool) or not isinstance(item, int) or item < 0 for item in value) or len(set(value)) != len(value):
        raise CampaignValidationError("manifest", f"{label} must contain unique non-negative frame integers")
    return tuple(sorted(value))


def _number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and value >= 0


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _canonical(value: object) -> str:
    try:
        return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False)
    except (TypeError, ValueError) as error:
        raise CampaignValidationError("manifest", f"manifest is not canonical JSON: {error}") from error


def _default_state_path(snapshot_path: Path) -> Path:
    return snapshot_path.with_name(f".{snapshot_path.stem}.campaign-state.json")


def _case_input_hash(manifest: Path, campaign_case: CampaignCase) -> str:
    value = {"manifest": manifest.read_text(encoding="utf-8"), "caseId": campaign_case.case_id}
    return hashlib.sha256(_canonical(value).encode("utf-8")).hexdigest()


def _load_state(path: Path, config: CampaignConfig, input_hashes: dict[str, str], commit_sha: str) -> dict[str, Any]:
    if not path.exists():
        return {"schemaVersion": 1, "configHash": config.config_hash, "commitSha": commit_sha, "cases": {}}
    value = _read_json_object(path, "state")
    if value.get("schemaVersion") != 1 or value.get("configHash") != config.config_hash or value.get("commitSha") != commit_sha:
        raise CampaignValidationError("state", "state belongs to a different manifest or commit")
    cases = value.get("cases")
    if not isinstance(cases, dict) or any(case_id not in input_hashes for case_id in cases):
        raise CampaignValidationError("state", "state cases do not match manifest")
    for case_id, entry in cases.items():
        if not isinstance(entry, dict) or entry.get("inputHash") != input_hashes[case_id]:
            raise CampaignValidationError("state", f"state input drift for {case_id}")
    return value


def _entry(case: CampaignCase, input_hash: str, run_dir: Path) -> dict[str, Any]:
    return {"caseId": case.case_id, "inputHash": input_hash, "features": list(case.features), "categories": list(case.categories), "runDir": str(run_dir.resolve()), "status": "running", "prepared": False, "recorded": False, "compared": False, "passed": False, "metrics": {}, "failures": [], "cleanup": "not-attempted", "mmdExecutableSha256": None}


def _run_dir(case: CampaignCase, oracle_case: OracleCase) -> Path:
    return oracle_case.output_root / _artifact_name(case.case_id)


def _resume_cleanup(entry: dict[str, Any], cleanup_fn: CleanupAction, prepared_cleanup_fn: CleanupAction) -> bool:
    raw_dir = entry.get("runDir")
    if not isinstance(raw_dir, str):
        return False
    run_dir = Path(raw_dir).resolve()
    if not run_dir.exists():
        return True
    fn = cleanup_fn if (run_dir / "record-result.json").is_file() else prepared_cleanup_fn
    return _safe_cleanup(fn, run_dir).get("ok") is True


def _cleanup_entry(entry: dict[str, Any], run_dir: Path, cleanup_fn: CleanupAction) -> bool:
    if not run_dir.exists():
        entry["cleanup"] = "cleaned"
        return True
    result = _safe_cleanup(cleanup_fn, run_dir)
    if result.get("ok") is True:
        entry["cleanup"] = "cleaned"
        return True
    entry["cleanup"] = "failed"
    if "cleanup" not in entry["failures"]:
        entry["failures"].append("cleanup")
    return False


def _write_numeric_manifest(path: Path, campaign_case: CampaignCase, case: OracleCase, run_dir: Path, config: CampaignConfig) -> None:
    model = run_dir / "model.mmd-utf16.pmx"
    model = model if model.is_file() else case.pmx
    output = run_dir / "oracle.actual.jsonl"
    if not model.is_file() or not output.is_file():
        raise CampaignValidationError("record", f"recorded artifacts are missing for {campaign_case.case_id}")
    _atomic_write_json(path, {"schemaVersion": 1, "defaults": {"compare": {"epsilon": config.thresholds["maxAbsError"]}, "focus": {"bones": list(config.focus_bones)}}, "cases": [{"name": campaign_case.case_id, "kind": "motion-numeric", "assets": {"model": str(model), "motion": str(case.body_vmd)}, "frames": list(case.frames), "oracle": {"format": "jsonl", "path": str(output)}, "metadata": {"focus": {"bones": list(config.focus_bones)}}}]})


def _run_numeric_compare(manifest_path: Path, repo_root: Path) -> CommandResult:
    from .prepare import SubprocessRunner
    return SubprocessRunner(timeout_seconds=None).run(("cargo", "run", "-q", "-p", "mmd-anim-cli", "--", "verify", str(manifest_path), "--mode", "numeric", "--json"), repo_root)


def _parse_compare_result(result: CommandResult, case_id: str, thresholds: dict[str, float]) -> CompareOutcome:
    if result.exit_code != 0:
        return CompareOutcome(False, False, {}, ("compare-command",))
    try:
        value = json.loads(result.stdout)
        current = value["perCase"][0]
    except (TypeError, KeyError, IndexError, json.JSONDecodeError):
        return CompareOutcome(False, False, {}, ("compare-malformed",))
    if not isinstance(value, dict) or not isinstance(value.get("perCase"), list) or len(value["perCase"]) != 1 or not isinstance(current, dict) or current.get("name") != case_id:
        return CompareOutcome(False, False, {}, ("compare-shape",))
    metrics: dict[str, float] = {}
    for metric in _METRIC_NAMES:
        raw = current.get(metric)
        if not _number(raw):
            return CompareOutcome(False, False, {}, ("compare-metrics",))
        metrics[metric] = float(raw)
    status, mismatch, frames, bones = current.get("status"), current.get("mismatchCount"), current.get("comparedFrames"), current.get("comparedBones")
    if status not in {"ok", "mismatch"} or not all(isinstance(item, int) and not isinstance(item, bool) and item >= 0 for item in (mismatch, frames, bones)):
        return CompareOutcome(False, False, {}, ("compare-fields",))
    failures: list[str] = []
    if mismatch != 0:
        failures.append("threshold")
    if frames == 0 or bones == 0:
        failures.append("compare-no-targets")
    for metric, metric_value in metrics.items():
        if metric_value > thresholds[metric]:
            failures.append("threshold")
    failures = list(dict.fromkeys(failures))
    compared = not any(item.startswith("compare-") for item in failures) and frames > 0 and bones > 0
    return CompareOutcome(compared, compared and not failures, metrics if compared else {}, tuple(failures))


def _mmd_executable_hash(recorded: dict[str, Any]) -> tuple[str | None, str | None]:
    identity = recorded.get("mmdExecutable")
    value = identity.get("sha256") if isinstance(identity, dict) else None
    if value is None:
        return None, None
    if not isinstance(value, str) or len(value) != 64 or any(char not in "0123456789abcdefABCDEF" for char in value):
        return None, "mmd-executable-hash"
    return value.lower(), None


def _build_snapshot(config: CampaignConfig, state: dict[str, Any], provenance: dict[str, str]) -> dict[str, Any]:
    entries = list(state["cases"].values())
    funnel = {"discovered": config.discovered, "selected": len(config.cases), "prepared": sum(entry.get("prepared") is True for entry in entries), "recorded": sum(entry.get("recorded") is True for entry in entries), "compared": sum(entry.get("compared") is True for entry in entries), "passed": sum(entry.get("passed") is True for entry in entries)}
    failures: dict[str, int] = {}
    for entry in entries:
        for failure in entry.get("failures", []):
            failures[failure] = failures.get(failure, 0) + 1
    metrics: dict[str, dict[str, float]] = {}
    for metric in _METRIC_NAMES:
        values = sorted(float(entry["metrics"][metric]) for entry in entries if entry.get("compared") and metric in entry.get("metrics", {}))
        if values:
            metrics[metric] = {"p50": _nearest_rank(values, .50), "p95": _nearest_rank(values, .95), "p99": _nearest_rank(values, .99), "max": values[-1]}
    features = _tag_summaries(config.cases, entries, "features")
    categories = _tag_summaries(config.cases, entries, "categories")
    worst: list[dict[str, Any]] = []
    case_by_id = {case.case_id: case for case in config.cases}
    for entry in entries:
        if not entry.get("compared"):
            continue
        category = case_by_id[entry["caseId"]].categories[0] if case_by_id[entry["caseId"]].categories else "uncategorized"
        for metric, value in entry.get("metrics", {}).items():
            threshold = config.thresholds[metric]
            ratio = math.inf if threshold == 0 and value > 0 else (0 if threshold == 0 else value / threshold)
            worst.append({"caseId": entry["caseId"], "category": category, "metric": metric, "value": value, "result": "pass" if value <= threshold else "fail", "_ratio": ratio})
    worst.sort(key=lambda item: (-item["_ratio"], -item["value"], item["caseId"], item["metric"]))
    for item in worst:
        item.pop("_ratio", None)
    hashes = {entry.get("mmdExecutableSha256") for entry in entries if entry.get("mmdExecutableSha256")}
    observed = next(iter(hashes), "not-observed")
    return {"schemaVersion": 1, "run": {**config.run, "commitSha": provenance["commitSha"], "repositoryState": provenance["repositoryState"], "mmdVersionSource": "config-self-reported", "dumperVersionSource": "config-self-reported", "manifestHash": config.config_hash, "mmdExecutableSha256": observed}, "funnel": funnel, "thresholds": dict(config.thresholds), "metrics": metrics, "failures": failures, "features": features, "categories": categories, "worstCases": worst[:20], "rawArtifacts": {"retained": False}}


def _tag_summaries(cases: tuple[CampaignCase, ...], entries: list[dict[str, Any]], field: str) -> dict[str, dict[str, int]]:
    result: dict[str, dict[str, int]] = {}
    for case in cases:
        for tag in getattr(case, field):
            result.setdefault(tag, {"selected": 0, "compared": 0, "passed": 0})["selected"] += 1
    for entry in entries:
        for tag in entry.get(field, []):
            item = result.setdefault(tag, {"selected": 0, "compared": 0, "passed": 0})
            item["compared"] += int(entry.get("compared") is True)
            item["passed"] += int(entry.get("passed") is True)
    return {tag: result[tag] for tag in sorted(result)}


def _nearest_rank(values: list[float], quantile: float) -> float:
    return values[max(0, math.ceil(quantile * len(values)) - 1)]


def _probe_repository(repo_root: Path) -> dict[str, str]:
    try:
        commit = subprocess.run(("git", "rev-parse", "HEAD"), cwd=repo_root, capture_output=True, text=True, check=True).stdout.strip()
        status = subprocess.run(("git", "status", "--porcelain"), cwd=repo_root, capture_output=True, text=True, check=True).stdout
    except (OSError, subprocess.SubprocessError) as error:
        raise CampaignValidationError("provenance", f"cannot inspect repository: {error}") from error
    return {"commitSha": commit, "repositoryState": "dirty" if status else "clean"}


def _require_clean_provenance(value: dict[str, str], expected: str | None = None) -> None:
    if value.get("repositoryState") != "clean":
        raise CampaignValidationError("provenance-dirty", "campaign requires a clean repository checkout")
    commit = value.get("commitSha")
    if not isinstance(commit, str) or not commit:
        raise CampaignValidationError("provenance", "repository commit SHA is unavailable")
    if expected is not None and commit != expected:
        raise CampaignValidationError("provenance-mismatch", "repository HEAD changed during campaign")


def _safe_cleanup(fn: CleanupAction, run_dir: Path) -> dict[str, Any]:
    try:
        result = fn(run_dir)
    except Exception:  # noqa: BLE001 - cleanup failure must stop the campaign
        return {"ok": False}
    return result if isinstance(result, dict) else {"ok": False}


def _persist_state(path: Path, state: dict[str, Any]) -> None:
    _atomic_write_json(path, state)


def _atomic_write_json(path: Path, value: dict[str, Any]) -> None:
    descriptor: int | None = None
    temporary: Path | None = None
    try:
        descriptor, name = tempfile.mkstemp(dir=path.parent, prefix=f".{path.name}.", suffix=".tmp")
        temporary = Path(name)
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
            temporary.unlink(missing_ok=True)


def _campaign_failure(error: CampaignValidationError) -> dict[str, Any]:
    return {"ok": False, "schemaVersion": CAMPAIGN_SCHEMA_VERSION, "casesProcessed": 0, "snapshotWritten": False, "error": error.as_dict()}


def _stop(event: dict[str, Any], code: str, message: str, state: dict[str, Any], state_path: Path) -> dict[str, Any]:
    event["error"] = {"code": code, "message": message}
    event["stateCases"] = len(state.get("cases", {}))
    try:
        _persist_state(state_path, state)
    except OSError:
        pass
    return event
