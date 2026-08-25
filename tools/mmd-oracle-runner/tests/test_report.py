from __future__ import annotations

import json
from pathlib import Path

import pytest

from mmd_oracle_runner.report import ReportValidationError, generate_report, load_snapshot, write_report


def _snapshot() -> dict[str, object]:
    metrics = {name: {"p50": 0.0, "p95": 0.001, "p99": 0.002, "max": 0.003} for name in ("translationMaxError", "translationRmsError", "rotationMaxAngleRad", "rotationRmsAngleRad", "maxAbsError")}
    return {
        "schemaVersion": 1,
        "run": {"commitSha": "a" * 40, "repositoryState": "clean", "mmdVersion": "9.32-x64", "mmdVersionSource": "config", "dumperVersion": "test", "dumperVersionSource": "config", "timestamp": "2026-08-25T00:00:00Z", "samplingPolicy": "fixed-local-v1", "manifestHash": "c" * 64, "mmdExecutableSha256": "not-observed"},
        "funnel": {field: 1 for field in ("discovered", "selected", "prepared", "recorded", "compared", "passed")},
        "thresholds": {name: 0.1 for name in metrics}, "metrics": metrics, "failures": {},
        "features": {"bone": {"selected": 1, "compared": 1, "passed": 1}}, "categories": {},
        "worstCases": [{"caseId": "case-0", "category": "bone", "metric": "maxAbsError", "value": 0.003, "result": "pass"}],
        "rawArtifacts": {"retained": False},
    }


def test_report_is_deterministic_and_aggregate_only() -> None:
    snapshot = _snapshot()
    assert generate_report(snapshot) == generate_report(json.loads(json.dumps(snapshot, sort_keys=True)))
    report = generate_report(snapshot)
    assert "case-0" in report
    assert "per-frame and per-bone details" in report
    assert "Raw PMM, JSONL, and log artifacts are not retained" in report


def test_dirty_snapshot_is_rejected(tmp_path: Path) -> None:
    snapshot = _snapshot()
    snapshot["run"] = dict(snapshot["run"], repositoryState="dirty")
    with pytest.raises(ReportValidationError, match="must be clean"):
        generate_report(snapshot)
    path = tmp_path / "snapshot.json"
    path.write_text(json.dumps(snapshot), encoding="utf-8")
    with pytest.raises(ReportValidationError):
        load_snapshot(path)


def test_non_numeric_threshold_is_rejected() -> None:
    snapshot = _snapshot()
    snapshot["thresholds"] = dict(snapshot["thresholds"], maxAbsError="invalid")
    with pytest.raises(ReportValidationError, match="thresholds.maxAbsError"):
        generate_report(snapshot)


def test_write_report_uses_requested_output(tmp_path: Path) -> None:
    snapshot_path = tmp_path / "snapshot.json"
    output_path = tmp_path / "quality.md"
    snapshot_path.write_text(json.dumps(_snapshot()), encoding="utf-8")
    write_report(snapshot_path, output_path)
    assert output_path.read_text(encoding="utf-8").startswith("# Motion Golden Oracle Quality Report")
