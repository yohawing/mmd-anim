from __future__ import annotations

import json
from pathlib import Path

import pytest

import mmd_oracle_runner.report as report_module
from mmd_oracle_runner.cli import main
from mmd_oracle_runner.report import ReportValidationError, generate_report, load_snapshot, write_report


def _snapshot() -> dict[str, object]:
    return {
        "schemaVersion": 1,
        "run": {
            "commitSha": "a" * 40,
            "repositoryState": "clean",
            "mmdVersion": "9.32-x64",
            "mmdVersionSource": "config-self-reported",
            "dumperVersion": "0.1.0",
            "dumperVersionSource": "config-self-reported",
            "timestamp": "2026-08-24T12:00:00Z",
            "samplingPolicy": "pilot-stratified-v1",
            "selectionHash": "c" * 64,
            "configHash": "d" * 64,
            "mmdExecutableSha256": "not-observed",
        },
        "funnel": {
            "discovered": 100,
            "selected": 20,
            "prepared": 19,
            "recorded": 18,
            "compared": 18,
            "passed": 17,
        },
        "thresholds": {
            "translationMaxError": 0.001,
            "translationRmsError": 0.001,
            "rotationMaxAngleRad": 0.003,
            "rotationRmsAngleRad": 0.003,
            "maxAbsError": 0.003,
        },
        "metrics": {
            "translationMaxError": {"p50": 0.0, "p95": 0.0001, "p99": 0.0005, "max": 0.001},
            "translationRmsError": {"p50": 0.0, "p95": 0.0001, "p99": 0.0005, "max": 0.001},
            "rotationMaxAngleRad": {"p50": 0.0001, "p95": 0.001, "p99": 0.002, "max": 0.003},
            "rotationRmsAngleRad": {"p50": 0.0001, "p95": 0.001, "p99": 0.002, "max": 0.003},
            "maxAbsError": {"p50": 0.0001, "p95": 0.001, "p99": 0.002, "max": 0.003},
        },
        "failures": {"prepare": 1, "record": 1, "compare": 0},
        "features": {"bone": {"selected": 20, "compared": 18, "passed": 17}, "morph": {"selected": 12, "compared": 12, "passed": 12}},
        "categories": {"ik-heavy": {"selected": 8, "compared": 8, "passed": 7}},
        "worstCases": [
            {"caseId": "case-001", "category": "ik-heavy", "metric": "rotationMaxAngleRad", "value": 0.003, "result": "pass"}
        ],
        "rawArtifacts": {"retained": False},
    }


def test_valid_snapshot_generates_aggregate_report_without_raw_details() -> None:
    report = generate_report(_snapshot())

    assert "# Motion Golden Oracle Quality Report" in report
    assert "| Discovered | 100 |" in report
    assert "| rotationMaxAngleRad | 0.0001 | 0.001 | 0.002 | 0.003 |" in report
    assert "case-001" in report
    assert "Raw PMM, JSONL, and log artifacts are not retained" in report
    assert "per-frame and per-bone details" in report


def test_dirty_snapshot_is_rejected_by_generation_and_write(tmp_path: Path) -> None:
    snapshot = _snapshot()
    snapshot["run"] = dict(snapshot["run"], repositoryState="dirty")

    with pytest.raises(ReportValidationError) as generated_error:
        generate_report(snapshot)
    assert generated_error.value.as_dict()["issues"] == [
        {"path": "run.repositoryState", "reason": "must be clean"}
    ]

    snapshot_path = tmp_path / "snapshot.json"
    snapshot_path.write_text(json.dumps(snapshot), encoding="utf-8")
    with pytest.raises(ReportValidationError) as written_error:
        write_report(snapshot_path, tmp_path / "report.md")
    assert written_error.value.as_dict()["issues"] == [
        {"path": "run.repositoryState", "reason": "must be clean"}
    ]


def test_generation_is_deterministic_for_different_json_key_order() -> None:
    first = _snapshot()
    second = json.loads(json.dumps(first, sort_keys=True))

    assert generate_report(first) == generate_report(second)


def test_zero_comparable_snapshot_is_valid_red_report() -> None:
    snapshot = _snapshot()
    snapshot["funnel"] = {"discovered": 1, "selected": 1, "prepared": 1, "recorded": 1, "compared": 0, "passed": 0}
    snapshot["metrics"] = {}
    snapshot["failures"] = {"compare-malformed": 1}

    report = generate_report(snapshot)

    assert "No comparable cases were available" in report
    assert "maxAbsError" in report
    assert "| Metric | p50 | p95 | p99 | max |" not in report


def test_invalid_schema_is_rejected_with_stable_issue() -> None:
    invalid = _snapshot()
    invalid["schemaVersion"] = 99

    with pytest.raises(ReportValidationError) as error:
        generate_report(invalid)

    assert error.value.as_dict()["issues"] == [{"path": "schemaVersion", "reason": "must be 1"}]


def test_metric_distribution_order_and_threshold_name_set_are_validated() -> None:
    invalid_order = _snapshot()
    invalid_order["metrics"]["rotationMaxAngleRad"] = {"p50": 0.003, "p95": 0.001, "p99": 0.002, "max": 0.003}
    with pytest.raises(ReportValidationError) as order_error:
        generate_report(invalid_order)
    assert order_error.value.as_dict()["issues"] == [
        {"path": "metrics.rotationMaxAngleRad.p95", "reason": "must be >= metrics.rotationMaxAngleRad.p50"}
    ]

    invalid_names = _snapshot()
    invalid_names["thresholds"] = {"rotationMaxAngleRad": 0.003, "morph": 0.01}
    with pytest.raises(ReportValidationError) as names_error:
        generate_report(invalid_names)
    assert names_error.value.as_dict()["issues"] == [
        {
            "path": "thresholds",
            "reason": "must contain exactly the five required metric names",
        }
    ]


def test_summary_counts_and_worst_result_are_strictly_validated() -> None:
    invalid_summary = _snapshot()
    invalid_summary["features"]["bone"] = {"selected": 1, "compared": 2, "passed": 1}
    with pytest.raises(ReportValidationError) as summary_error:
        generate_report(invalid_summary)
    assert {issue["path"] for issue in summary_error.value.as_dict()["issues"]} >= {"features.bone.compared"}

    invalid_result = _snapshot()
    invalid_result["worstCases"][0]["result"] = "unknown"
    with pytest.raises(ReportValidationError) as result_error:
        generate_report(invalid_result)
    assert result_error.value.as_dict()["issues"] == [{"path": "worstCases[0].result", "reason": "must be pass or fail"}]


def test_path_and_output_rejection_does_not_modify_snapshot(tmp_path: Path) -> None:
    snapshot_path = tmp_path / "snapshot.json"
    snapshot_path.write_text(json.dumps(_snapshot()), encoding="utf-8")
    before = snapshot_path.read_bytes()

    invalid = _snapshot()
    invalid["run"] = dict(invalid["run"], samplingPolicy=r"C:\\private\\asset.csv")
    invalid_path = tmp_path / "invalid.json"
    invalid_path.write_text(json.dumps(invalid), encoding="utf-8")
    with pytest.raises(ReportValidationError) as error:
        write_report(invalid_path, tmp_path / "report.md")
    assert error.value.as_dict()["issues"][0]["path"] == "run.samplingPolicy"

    with pytest.raises(ReportValidationError) as same_path:
        write_report(snapshot_path, snapshot_path)
    assert same_path.value.as_dict()["issues"] == [{"path": "output", "reason": "must differ from snapshot"}]
    assert snapshot_path.read_bytes() == before


def test_cli_writes_one_markdown_report_and_rejects_missing_snapshot(tmp_path: Path, capsys) -> None:
    snapshot_path = tmp_path / "snapshot.json"
    output_path = tmp_path / "report.md"
    snapshot_path.write_text(json.dumps(_snapshot()), encoding="utf-8")

    assert main(["quality-report", "--snapshot", str(snapshot_path), "--output", str(output_path)]) == 0
    result = json.loads(capsys.readouterr().out)
    assert result["ok"] is True
    assert output_path.exists()

    assert main(["quality-report", "--snapshot", str(tmp_path / "missing.json"), "--output", str(tmp_path / "x.md")]) == 2
    error = json.loads(capsys.readouterr().err)
    assert error["error"]["issues"] == [{"path": "snapshot", "reason": "file does not exist"}]


def test_atomic_write_preserves_existing_output_when_fsync_fails(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    snapshot_path = tmp_path / "snapshot.json"
    output_path = tmp_path / "report.md"
    snapshot_path.write_text(json.dumps(_snapshot()), encoding="utf-8")
    output_path.write_text("previous report\n", encoding="utf-8")

    def fail_fsync(_descriptor: int) -> None:
        raise OSError("injected fsync failure")

    monkeypatch.setattr(report_module.os, "fsync", fail_fsync)
    with pytest.raises(ReportValidationError) as error:
        write_report(snapshot_path, output_path)

    assert error.value.as_dict()["issues"] == [
        {"path": "output", "reason": "cannot atomically write file (OSError)"}
    ]
    assert output_path.read_text(encoding="utf-8") == "previous report\n"
    assert list(tmp_path.glob(f".{output_path.name}.*.tmp")) == []


def test_load_snapshot_rejects_absolute_path_in_worst_case(tmp_path: Path) -> None:
    invalid = _snapshot()
    absolute_path = r"C:\\MMD\\model.pmx"
    invalid["worstCases"] = [dict(invalid["worstCases"][0], caseId=absolute_path)]
    path = tmp_path / "invalid.json"
    path.write_text(json.dumps(invalid), encoding="utf-8")

    with pytest.raises(ReportValidationError) as error:
        load_snapshot(path)

    assert error.value.as_dict()["issues"] == [
        {"path": "worstCases[0].caseId", "reason": "absolute or path-like values are not allowed"}
    ]
