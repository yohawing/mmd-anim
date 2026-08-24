from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import pytest

import mmd_oracle_runner.cli as cli
from mmd_oracle_runner.case import CaseValidationError, ValidationIssue


@dataclass(frozen=True)
class FakeCase:
    name: str
    source_path: Path
    frames: tuple[int, ...] = (0,)


def _loader(path: str | Path) -> FakeCase:
    path = Path(path)
    if path.stem == "invalid":
        raise CaseValidationError((ValidationIssue("input.pmx", "file does not exist"),))
    return FakeCase(path.stem, path)


def _result(case: FakeCase, *, ok: bool = True, **extra: Any) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "ok": ok,
        "caseFile": str(case.source_path),
        "caseName": case.name,
        "frames": list(case.frames),
        **extra,
    }


def _payload(capsys: pytest.CaptureFixture[str]) -> dict[str, Any]:
    captured = capsys.readouterr()
    assert captured.err == ""
    return json.loads(captured.out)


def test_prepare_batch_prints_ordered_machine_summary_and_returns_zero(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    called: list[str] = []

    def prepare(case: FakeCase) -> dict[str, Any]:
        called.append(case.name)
        return _result(case, phase="complete")

    monkeypatch.setattr(cli, "load_case", _loader)
    monkeypatch.setattr(cli, "prepare_case", prepare)

    exit_code = cli.main(
        [
            "prepare-batch",
            "--case",
            "body.json",
            "--case",
            "camera.json",
        ]
    )

    summary = _payload(capsys)
    assert exit_code == 0
    assert called == ["body", "camera"]
    assert summary["command"] == "prepare-batch"
    assert summary["action"] == "prepare"
    assert summary["total"] == 2
    assert summary["passed"] == 2
    assert summary["failed"] == 0
    assert summary["ok"] is True
    assert [case["casePath"] for case in summary["cases"]] == ["body.json", "camera.json"]


def test_record_batch_continues_validation_and_action_failures_and_passes_common_exe(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str]
) -> None:
    called: list[tuple[str, str]] = []
    mmd_exe = r"C:\MMD\MikuMikuDance.exe"

    def record(case: FakeCase, executable: str) -> dict[str, Any]:
        called.append((case.name, executable))
        return _result(case, ok=case.name != "action-fails", phase="process")

    monkeypatch.setattr(cli, "load_case", _loader)
    monkeypatch.setattr(cli, "record_case", record)

    exit_code = cli.main(
        [
            "record-batch",
            "--case",
            "first.json",
            "--case",
            "invalid.json",
            "--case",
            "action-fails.json",
            "--case",
            "last.json",
            "--mmd-exe",
            mmd_exe,
        ]
    )

    summary = _payload(capsys)
    assert exit_code == 1
    assert called == [("first", mmd_exe), ("action-fails", mmd_exe), ("last", mmd_exe)]
    assert summary["command"] == "record-batch"
    assert summary["action"] == "record"
    assert summary["total"] == 4
    assert summary["passed"] == 2
    assert summary["failed"] == 2
    assert summary["ok"] is False
    assert summary["cases"][1]["error"]["kind"] == "validation"
    assert summary["cases"][2]["error"]["kind"] == "action"
    assert summary["cases"][3]["ok"] is True


@pytest.mark.parametrize("payload", [Path("not-json"), {1: "number", "text": "string"}, float("nan")])
def test_prepare_batch_serializes_non_json_action_failure(
    monkeypatch: pytest.MonkeyPatch, capsys: pytest.CaptureFixture[str], payload: object
) -> None:
    def prepare(case: FakeCase) -> dict[str, Any]:
        return _result(case, payload=payload)

    monkeypatch.setattr(cli, "load_case", _loader)
    monkeypatch.setattr(cli, "prepare_case", prepare)

    exit_code = cli.main(["prepare-batch", "--case", "body.json"])

    summary = _payload(capsys)
    assert exit_code == 1
    assert summary["ok"] is False
    assert summary["cases"][0]["error"]["kind"] == "action-contract"
    assert summary["cases"][0]["error"]["reason"] == "action result is not JSON serializable"
    assert "result" not in summary["cases"][0]
