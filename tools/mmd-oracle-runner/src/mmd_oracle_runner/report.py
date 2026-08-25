"""Deterministic Markdown for the local motion-quality snapshot."""

from __future__ import annotations

import json
import math
import os
import tempfile
from pathlib import Path
from typing import Any

REPORT_SCHEMA_VERSION = 1
_FUNNEL_FIELDS = ("discovered", "selected", "prepared", "recorded", "compared", "passed")
_METRIC_NAMES = ("translationMaxError", "translationRmsError", "rotationMaxAngleRad", "rotationRmsAngleRad", "maxAbsError")
_DISTRIBUTION_FIELDS = ("p50", "p95", "p99", "max")


class ReportValidationError(ValueError):
    def __init__(self, issues: tuple[tuple[str, str], ...]):
        self.issues = issues
        super().__init__("; ".join(f"{path}: {reason}" for path, reason in issues))

    def as_dict(self) -> dict[str, object]:
        return {"kind": "report-validation", "issues": [{"path": path, "reason": reason} for path, reason in self.issues]}


def load_snapshot(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        raise ReportValidationError((("snapshot", "file does not exist"),)) from error
    except (OSError, UnicodeError, json.JSONDecodeError) as error:
        raise ReportValidationError((("snapshot", "cannot read valid JSON"),)) from error
    _validate_snapshot(value)
    return value


def generate_report(snapshot: dict[str, Any]) -> str:
    _validate_snapshot(snapshot)
    run = snapshot["run"]
    funnel = snapshot["funnel"]
    lines = [
        "# Motion Golden Oracle Quality Report", "",
        "This report is generated from a compact quality snapshot. It contains aggregate results only; per-frame and per-bone details are not included.", "",
        "## Run and provenance", "", "| Field | Value |", "| --- | --- |",
        f"| Commit SHA | {_cell(run['commitSha'])} |", f"| Repository state | {_cell(run['repositoryState'])} |",
        f"| MMD version (self-reported) | {_cell(run['mmdVersion'])} |", f"| MMDDumper version (self-reported) | {_cell(run['dumperVersion'])} |",
        f"| MMD version source | {_cell(run['mmdVersionSource'])} |", f"| MMDDumper version source | {_cell(run['dumperVersionSource'])} |",
        f"| MMD executable SHA-256 | {_cell(run['mmdExecutableSha256'])} |", f"| Timestamp | {_cell(run['timestamp'])} |",
        f"| Sampling policy | {_cell(run['samplingPolicy'])} |", f"| Manifest SHA-256 | {_cell(run['manifestHash'])} |",
        "", "## Execution funnel", "", "| Stage | Cases |", "| --- | ---: |",
    ]
    lines.extend(f"| {field.capitalize()} | {funnel[field]} |" for field in _FUNNEL_FIELDS)
    lines.extend(("", "## Parity thresholds", "", "| Metric | Threshold |", "| --- | ---: |"))
    lines.extend(f"| {_cell(metric)} | {_number(snapshot['thresholds'][metric])} |" for metric in _METRIC_NAMES)
    lines.extend(("", "## Metric distributions", ""))
    if snapshot["metrics"]:
        lines.extend(("| Metric | p50 | p95 | p99 | max |", "| --- | ---: | ---: | ---: | ---: |"))
        for metric in _METRIC_NAMES:
            if metric in snapshot["metrics"]:
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
    snapshot_path, output_path = snapshot_path.resolve(), output_path.resolve()
    if snapshot_path == output_path:
        raise ReportValidationError((("output", "must differ from snapshot"),))
    if not output_path.parent.exists():
        raise ReportValidationError((("output", "parent directory does not exist"),))
    report = generate_report(load_snapshot(snapshot_path))
    descriptor: int | None = None
    temporary: Path | None = None
    try:
        descriptor, name = tempfile.mkstemp(dir=output_path.parent, prefix=f".{output_path.name}.", suffix=".tmp")
        temporary = Path(name)
        with os.fdopen(descriptor, "w", encoding="utf-8", newline="\n") as stream:
            descriptor = None
            stream.write(report)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, output_path)
        temporary = None
    except OSError as error:
        raise ReportValidationError((("output", "cannot atomically write report"),)) from error
    finally:
        if descriptor is not None:
            os.close(descriptor)
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def _validate_snapshot(snapshot: object) -> None:
    if not isinstance(snapshot, dict):
        raise ReportValidationError((("snapshot", "root must be an object"),))
    required = {"schemaVersion", "run", "funnel", "thresholds", "metrics", "failures", "features", "categories", "worstCases", "rawArtifacts"}
    missing = sorted(required - set(snapshot))
    if missing or snapshot.get("schemaVersion") != REPORT_SCHEMA_VERSION:
        reason = f"missing fields: {', '.join(missing)}" if missing else "schemaVersion must be 1"
        raise ReportValidationError((("snapshot", reason),))
    run = snapshot["run"]
    if not isinstance(run, dict) or run.get("repositoryState") != "clean":
        raise ReportValidationError((("run.repositoryState", "must be clean"),))
    for field in ("commitSha", "mmdVersion", "dumperVersion", "mmdVersionSource", "dumperVersionSource", "timestamp", "samplingPolicy", "manifestHash", "mmdExecutableSha256"):
        if not isinstance(run.get(field), str) or not run[field]:
            raise ReportValidationError(((f"run.{field}", "must be a non-empty string"),))
    funnel = snapshot["funnel"]
    if not isinstance(funnel, dict) or any(not _count(funnel.get(field)) for field in _FUNNEL_FIELDS):
        raise ReportValidationError((("funnel", "must contain non-negative integer counts"),))
    for metric in _METRIC_NAMES:
        if not _is_number(snapshot["thresholds"].get(metric)):
            raise ReportValidationError(((f"thresholds.{metric}", "must be finite and non-negative"),))
    if not isinstance(snapshot["metrics"], dict) or not isinstance(snapshot["failures"], dict) or not isinstance(snapshot["features"], dict) or not isinstance(snapshot["categories"], dict) or not isinstance(snapshot["worstCases"], list):
        raise ReportValidationError((("snapshot", "aggregate fields have invalid types"),))
    for metric, distribution in snapshot["metrics"].items():
        if metric not in _METRIC_NAMES or not isinstance(distribution, dict) or any(not _is_number(distribution.get(field)) for field in _DISTRIBUTION_FIELDS):
            raise ReportValidationError(((f"metrics.{metric}", "has invalid distribution"),))
    if not isinstance(snapshot["rawArtifacts"], dict) or not isinstance(snapshot["rawArtifacts"].get("retained"), bool):
        raise ReportValidationError((("rawArtifacts.retained", "must be boolean"),))


def _append_summary_table(lines: list[str], title: str, summaries: dict[str, Any]) -> None:
    lines.extend(("", f"## {title}", ""))
    if not summaries:
        lines.append("No tagged cases were reported.")
        return
    lines.extend(("| Tag | Selected | Compared | Passed |", "| --- | ---: | ---: | ---: |"))
    for tag in sorted(summaries):
        value = summaries[tag]
        lines.append(f"| {_cell(tag)} | {value['selected']} | {value['compared']} | {value['passed']} |")


def _count(value: object) -> bool:
    return isinstance(value, int) and not isinstance(value, bool) and value >= 0


def _number(value: object) -> str:
    if not isinstance(value, (int, float)) or isinstance(value, bool) or not math.isfinite(value):
        return "invalid"
    return f"{value:.12g}"


def _is_number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and value >= 0


def _cell(value: object) -> str:
    return str(value).replace("|", "\\|").replace("\r", " ").replace("\n", " ")
