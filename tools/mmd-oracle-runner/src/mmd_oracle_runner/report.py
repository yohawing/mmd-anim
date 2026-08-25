"""Deterministic Markdown reports for compact motion-parity snapshots."""

from __future__ import annotations

import json
import math
import os
import re
import tempfile
from pathlib import Path, PureWindowsPath
from typing import Any

REPORT_SCHEMA_VERSION = 1
_FUNNEL_FIELDS = ("discovered", "selected", "prepared", "recorded", "compared", "passed")
_METRIC_NAMES = ("translationMaxError", "translationRmsError", "rotationMaxAngleRad", "rotationRmsAngleRad", "maxAbsError")
_DISTRIBUTION_FIELDS = ("p50", "p95", "p99", "max")
_SHA_RE = re.compile(r"^[0-9a-fA-F]{7,64}$")
_SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")
_SAFE_LABEL_RE = re.compile(r"^[^\r\n]+$")


class ReportValidationError(ValueError):
    """A stable, user-readable validation error for report input/output."""

    def __init__(self, issues: tuple[tuple[str, str], ...]):
        self.issues = issues
        super().__init__("; ".join(f"{path}: {reason}" for path, reason in issues))

    def as_dict(self) -> dict[str, object]:
        return {"kind": "report-validation", "issues": [{"path": path, "reason": reason} for path, reason in self.issues]}


def load_snapshot(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise ReportValidationError((("snapshot", "file does not exist"),))
    if not path.is_file():
        raise ReportValidationError((("snapshot", "path is not a regular file"),))
    try:
        raw = path.read_text(encoding="utf-8")
    except (OSError, UnicodeError) as error:
        raise ReportValidationError((("snapshot", f"cannot read file ({error.__class__.__name__})"),)) from error
    try:
        snapshot = json.loads(raw)
    except json.JSONDecodeError as error:
        raise ReportValidationError((("snapshot", f"invalid JSON at line {error.lineno}, column {error.colno}"),)) from error
    issues = _validate_snapshot(snapshot)
    if issues:
        raise ReportValidationError(tuple(issues))
    return snapshot


def generate_report(snapshot: dict[str, Any]) -> str:
    issues = _validate_snapshot(snapshot)
    if issues:
        raise ReportValidationError(tuple(issues))
    run = snapshot["run"]
    funnel = snapshot["funnel"]
    lines = [
        "# Motion Golden Oracle Quality Report",
        "",
        "This report is generated from a compact quality snapshot. It contains aggregate results only; per-frame and per-bone details are not included.",
        "",
        "## Run and provenance",
        "",
        "| Field | Value |",
        "| --- | --- |",
        f"| Commit SHA | {_cell(run['commitSha'])} |",
        f"| Repository state | {_cell(run['repositoryState'])} |",
        f"| MMD version (self-reported) | {_cell(run['mmdVersion'])} |",
        f"| MMDDumper version (self-reported) | {_cell(run['dumperVersion'])} |",
        f"| MMD version source | {_cell(run['mmdVersionSource'])} |",
        f"| MMDDumper version source | {_cell(run['dumperVersionSource'])} |",
        f"| MMD executable SHA-256 | {_cell(run['mmdExecutableSha256'])} |",
        f"| Timestamp | {_cell(run['timestamp'])} |",
        f"| Sampling policy | {_cell(run['samplingPolicy'])} |",
        f"| Frozen selection hash | {_cell(run['selectionHash'])} |",
        f"| Configuration hash | {_cell(run['configHash'])} |",
        "",
        "## Execution funnel",
        "",
        "| Stage | Cases |",
        "| --- | ---: |",
    ]
    lines.extend(f"| {field.capitalize()} | {funnel[field]} |" for field in _FUNNEL_FIELDS)
    lines.extend(("", "## Parity thresholds", "", "| Metric | Threshold |", "| --- | ---: |"))
    for metric in _METRIC_NAMES:
        lines.append(f"| {_cell(metric)} | {_number(snapshot['thresholds'][metric])} |")

    lines.extend(("", "## Metric distributions", ""))
    if snapshot["metrics"]:
        lines.extend(("| Metric | p50 | p95 | p99 | max |", "| --- | ---: | ---: | ---: | ---: |"))
        for metric in _METRIC_NAMES:
            distribution = snapshot["metrics"][metric]
            lines.append(f"| {_cell(metric)} | " + " | ".join(_number(distribution[field]) for field in _DISTRIBUTION_FIELDS) + " |")
    else:
        lines.append("No comparable cases were available; metric distributions are empty.")

    lines.extend(("", "## Failure classifications", ""))
    if snapshot["failures"]:
        lines.extend(("| Classification | Cases |", "| --- | ---: |"))
        lines.extend(f"| {_cell(name)} | {count} |" for name, count in sorted(snapshot["failures"].items()))
    else:
        lines.append("No failures were classified.")
    _append_summary_table(lines, "Feature summaries", snapshot["features"])
    _append_summary_table(lines, "Category summaries", snapshot["categories"])
    lines.extend(("", "## Worst cases", ""))
    if snapshot["worstCases"]:
        lines.extend(("| Case | Category | Metric | Value | Result |", "| --- | --- | --- | ---: | --- |"))
        for case in snapshot["worstCases"]:
            lines.append(f"| {_cell(case['caseId'])} | {_cell(case['category'])} | {_cell(case['metric'])} | {_number(case['value'])} | {_cell(case['result'])} |")
    else:
        lines.append("No worst cases were reported.")
    lines.extend(("", "## Raw artifact retention", "", "Raw PMM, JSONL, and log artifacts are not retained by this quality-report workflow.", f"Snapshot retention flag: `{str(snapshot['rawArtifacts']['retained']).lower()}`.", ""))
    return "\n".join(lines)


def write_report(snapshot_path: Path, output_path: Path) -> None:
    snapshot_path = snapshot_path.resolve()
    output_path = output_path.resolve()
    if snapshot_path == output_path:
        raise ReportValidationError((("output", "must differ from snapshot"),))
    if not snapshot_path.exists():
        raise ReportValidationError((("snapshot", "file does not exist"),))
    if not output_path.parent.exists():
        raise ReportValidationError((("output", "parent directory does not exist"),))
    if output_path.exists() and output_path.is_dir():
        raise ReportValidationError((("output", "path is a directory"),))
    report = generate_report(load_snapshot(snapshot_path))
    temporary_path: Path | None = None
    descriptor: int | None = None
    try:
        descriptor, temporary_name = tempfile.mkstemp(dir=output_path.parent, prefix=f".{output_path.name}.", suffix=".tmp")
        temporary_path = Path(temporary_name)
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
            descriptor = None
            stream.write(report)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary_path, output_path)
        temporary_path = None
    except OSError as error:
        raise ReportValidationError((("output", f"cannot atomically write file ({error.__class__.__name__})"),)) from error
    finally:
        if descriptor is not None:
            os.close(descriptor)
        if temporary_path is not None:
            try:
                temporary_path.unlink()
            except OSError:
                pass


def _validate_snapshot(snapshot: object) -> list[tuple[str, str]]:
    if not isinstance(snapshot, dict):
        return [("snapshot", "root must be an object")]
    issues: list[tuple[str, str]] = []
    _require_keys(snapshot, {"schemaVersion", "run", "funnel", "thresholds", "metrics", "failures", "features", "categories", "worstCases", "rawArtifacts"}, "snapshot", issues)
    if snapshot.get("schemaVersion") != REPORT_SCHEMA_VERSION:
        issues.append(("schemaVersion", f"must be {REPORT_SCHEMA_VERSION}"))

    run = snapshot.get("run")
    if not isinstance(run, dict):
        issues.append(("run", "must be an object"))
    else:
        _require_keys(run, {"commitSha", "repositoryState", "mmdVersion", "mmdVersionSource", "dumperVersion", "dumperVersionSource", "timestamp", "samplingPolicy", "selectionHash", "configHash", "mmdExecutableSha256"}, "run", issues)
        _validate_safe_string(run.get("commitSha"), "run.commitSha", issues)
        if isinstance(run.get("commitSha"), str) and not _SHA_RE.fullmatch(run["commitSha"]):
            issues.append(("run.commitSha", "must be a hexadecimal commit SHA (7-64 characters)"))
        if run.get("repositoryState") != "clean":
            issues.append(("run.repositoryState", "must be clean"))
        for field in ("mmdVersion", "dumperVersion", "timestamp", "samplingPolicy"):
            _validate_safe_string(run.get(field), f"run.{field}", issues)
        for field in ("mmdVersionSource", "dumperVersionSource"):
            _validate_safe_string(run.get(field), f"run.{field}", issues)
            if run.get(field) != "config-self-reported":
                issues.append((f"run.{field}", "must be config-self-reported"))
        for field in ("selectionHash", "configHash"):
            _validate_safe_string(run.get(field), f"run.{field}", issues)
            if isinstance(run.get(field), str) and not _SHA256_RE.fullmatch(run[field]):
                issues.append((f"run.{field}", "must be a 64-character hexadecimal SHA-256"))
        mmd_hash = run.get("mmdExecutableSha256")
        if mmd_hash != "not-observed" and (not isinstance(mmd_hash, str) or not _SHA256_RE.fullmatch(mmd_hash)):
            issues.append(("run.mmdExecutableSha256", "must be not-observed or a 64-character hexadecimal SHA-256"))

    funnel = snapshot.get("funnel")
    if not isinstance(funnel, dict):
        issues.append(("funnel", "must be an object"))
    else:
        _require_keys(funnel, set(_FUNNEL_FIELDS), "funnel", issues)
        previous_field: str | None = None
        previous_value: int | None = None
        for field in _FUNNEL_FIELDS:
            value = funnel.get(field)
            if not _is_count(value):
                issues.append((f"funnel.{field}", "must be a non-negative integer"))
            elif previous_value is not None and value > previous_value:
                issues.append((f"funnel.{field}", f"must be <= funnel.{previous_field}"))
            if _is_count(value):
                previous_field, previous_value = field, value

    thresholds = snapshot.get("thresholds")
    if not isinstance(thresholds, dict) or set(thresholds) != set(_METRIC_NAMES):
        issues.append(("thresholds", "must contain exactly the five required metric names"))
    elif isinstance(thresholds, dict):
        _validate_numeric_map(thresholds, "thresholds", issues)

    metrics = snapshot.get("metrics")
    compared = funnel.get("compared") if isinstance(funnel, dict) else None
    if not isinstance(metrics, dict):
        issues.append(("metrics", "must be an object"))
    elif not metrics and compared != 0:
        issues.append(("metrics", "must be non-empty when funnel.compared is greater than zero"))
    elif isinstance(metrics, dict) and metrics:
        if set(metrics) != set(_METRIC_NAMES):
            issues.append(("metrics", "metric names must contain exactly the five required metric names"))
        for metric, distribution in metrics.items():
            _validate_safe_string(metric, f"metrics.{metric}", issues)
            if not isinstance(distribution, dict):
                issues.append((f"metrics.{metric}", "must be an object"))
                continue
            _require_keys(distribution, set(_DISTRIBUTION_FIELDS), f"metrics.{metric}", issues)
            previous_field: str | None = None
            previous_value: float | None = None
            for field in _DISTRIBUTION_FIELDS:
                value = distribution.get(field)
                if not _is_number(value, non_negative=True):
                    issues.append((f"metrics.{metric}.{field}", "must be a finite non-negative number"))
                elif previous_value is not None and value < previous_value:
                    issues.append((f"metrics.{metric}.{field}", f"must be >= metrics.{metric}.{previous_field}"))
                if _is_number(value, non_negative=True):
                    previous_field, previous_value = field, float(value)

    if compared and isinstance(metrics, dict) and isinstance(thresholds, dict) and set(thresholds) == set(_METRIC_NAMES) and set(metrics) != set(thresholds):
        issues.append(("thresholds", "metric names must exactly match metrics"))

    failures = snapshot.get("failures")
    if not isinstance(failures, dict):
        issues.append(("failures", "must be an object mapping classifications to counts"))
    else:
        _validate_count_map(failures, "failures", issues)
    for field in ("features", "categories"):
        summaries = snapshot.get(field)
        if not isinstance(summaries, dict):
            issues.append((field, "must be an object mapping names to summary objects"))
            continue
        for name, summary in summaries.items():
            _validate_safe_string(name, f"{field}.{name}", issues)
            if not isinstance(summary, dict) or set(summary) != {"selected", "compared", "passed"}:
                issues.append((f"{field}.{name}", "must contain exactly selected, compared, and passed"))
                continue
            _validate_count_map(summary, f"{field}.{name}", issues)
            if all(_is_count(summary.get(key)) for key in ("selected", "compared", "passed")):
                if summary["compared"] > summary["selected"]:
                    issues.append((f"{field}.{name}.compared", "must be <= selected"))
                if summary["passed"] > summary["compared"]:
                    issues.append((f"{field}.{name}.passed", "must be <= compared"))
    worst_cases = snapshot.get("worstCases")
    if not isinstance(worst_cases, list):
        issues.append(("worstCases", "must be an array"))
    else:
        for index, case in enumerate(worst_cases):
            prefix = f"worstCases[{index}]"
            if not isinstance(case, dict):
                issues.append((prefix, "must be an object"))
                continue
            _require_keys(case, {"caseId", "category", "metric", "value", "result"}, prefix, issues)
            for field in ("caseId", "category", "metric", "result"):
                _validate_safe_string(case.get(field), f"{prefix}.{field}", issues)
            if case.get("result") not in {"pass", "fail"}:
                issues.append((f"{prefix}.result", "must be pass or fail"))
            if not _is_number(case.get("value"), non_negative=True):
                issues.append((f"{prefix}.value", "must be a finite non-negative number"))
    raw_artifacts = snapshot.get("rawArtifacts")
    if not isinstance(raw_artifacts, dict):
        issues.append(("rawArtifacts", "must be an object"))
    else:
        _require_keys(raw_artifacts, {"retained"}, "rawArtifacts", issues)
        if raw_artifacts.get("retained") is not False:
            issues.append(("rawArtifacts.retained", "must be false"))
    return issues


def _append_summary_table(lines: list[str], title: str, summaries: dict[str, dict[str, int]]) -> None:
    lines.extend(("", f"## {title}", "", "| Name | Counts |", "| --- | --- |"))
    for name, summary in sorted(summaries.items()):
        lines.append(f"| {_cell(name)} | {_cell(', '.join(f'{key}={summary[key]}' for key in sorted(summary)))} |")


def _require_keys(value: dict[str, Any], required: set[str], prefix: str, issues: list[tuple[str, str]]) -> None:
    for key in sorted(required - value.keys()):
        issues.append((f"{prefix}.{key}", "is required"))
    for key in sorted(value.keys() - required):
        issues.append((f"{prefix}.{key}", "is not supported"))


def _validate_numeric_map(value: dict[str, Any], prefix: str, issues: list[tuple[str, str]]) -> None:
    for key, item in value.items():
        _validate_safe_string(key, f"{prefix}.{key}", issues)
        if not _is_number(item, non_negative=True):
            issues.append((f"{prefix}.{key}", "must be a finite non-negative number"))


def _validate_count_map(value: dict[str, Any], prefix: str, issues: list[tuple[str, str]]) -> None:
    for key, item in value.items():
        _validate_safe_string(key, f"{prefix}.{key}", issues)
        if not _is_count(item):
            issues.append((f"{prefix}.{key}", "must be a non-negative integer"))


def _validate_safe_string(value: object, path: str, issues: list[tuple[str, str]]) -> None:
    if not isinstance(value, str) or not value:
        issues.append((path, "must be a non-empty string"))
        return
    if not _SAFE_LABEL_RE.fullmatch(value):
        issues.append((path, "must not contain newlines"))
    if _looks_like_path(value):
        issues.append((path, "absolute or path-like values are not allowed"))


def _looks_like_path(value: str) -> bool:
    return value.startswith(("/", "\\")) or bool(PureWindowsPath(value).drive) or "\\" in value or ("/" in value and not value.startswith("https://"))


def _is_count(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _is_number(value: object, *, non_negative: bool) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and (not non_negative or value >= 0)


def _cell(value: object) -> str:
    return str(value).replace("|", "\\|").replace("`", "\\`")


def _number(value: int | float) -> str:
    return format(value, ".12g")
