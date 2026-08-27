from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
from typing import Any

from mmd_oracle_runner.batch import run_batch
from mmd_oracle_runner.case import CaseValidationError, ValidationIssue


@dataclass(frozen=True)
class FakeCase:
    name: str
    source_path: Path
    frames: tuple[int, ...] = (0,)


def _loader(path: str | Path) -> FakeCase:
    return FakeCase(Path(path).stem, Path(path))


def _result(case: FakeCase, *, ok: bool = True, **extra: Any) -> dict[str, Any]:
    return {
        "schemaVersion": 1,
        "ok": ok,
        "caseFile": str(case.source_path),
        "caseName": case.name,
        "frames": list(case.frames),
        **extra,
    }


def test_run_batch_preserves_path_order_and_injected_action_boundary() -> None:
    seen: list[str] = []

    def action(case: FakeCase) -> dict[str, Any]:
        seen.append(case.name)
        return _result(case)

    summary = run_batch(
        ["body.json", "camera.json"],
        action=action,
        action_name="prepare",
        case_loader=_loader,
    )

    assert seen == ["body", "camera"]
    assert [case["casePath"] for case in summary["cases"]] == ["body.json", "camera.json"]
    assert summary["action"] == "prepare"
    assert summary["schemaVersion"] == 1
    assert summary["total"] == 2
    assert summary["passed"] == 2
    assert summary["failed"] == 0
    assert summary["ok"] is True


def test_validation_and_action_failures_are_structured_and_do_not_stop_later_cases() -> None:
    called: list[str] = []

    def loader(path: str | Path) -> FakeCase:
        if Path(path).stem == "invalid":
            raise CaseValidationError((ValidationIssue("input.pmx", "file does not exist"),))
        return _loader(path)

    def action(case: FakeCase) -> dict[str, Any]:
        called.append(case.name)
        if case.name == "action-fails":
            return _result(case, ok=False, phase="generator", errors=["backend failed"])
        return _result(case)

    summary = run_batch(
        ["first.json", "invalid.json", "action-fails.json", "last.json"],
        action=action,
        action_name="record",
        case_loader=loader,
    )

    assert called == ["first", "action-fails", "last"]
    assert summary["total"] == 4
    assert summary["passed"] == 2
    assert summary["failed"] == 2
    assert summary["ok"] is False
    assert summary["cases"][1]["error"] == {
        "kind": "validation",
        "code": "case-validation-error",
        "issues": [{"field": "input.pmx", "reason": "file does not exist"}],
    }
    assert summary["cases"][2]["result"]["phase"] == "generator"
    assert summary["cases"][2]["error"] == {
        "kind": "action",
        "reason": "action reported failure",
    }


def test_unexpected_loader_and_action_exceptions_are_isolated() -> None:
    called: list[str] = []

    def loader(path: str | Path) -> FakeCase:
        if Path(path).stem == "loader-boom":
            raise OSError("case store unavailable")
        return _loader(path)

    def action(case: FakeCase) -> dict[str, Any]:
        called.append(case.name)
        if case.name == "action-boom":
            raise RuntimeError("backend crashed")
        return _result(case)

    summary = run_batch(
        ["loader-boom.json", "action-boom.json", "after.json"],
        action=action,
        case_loader=loader,
    )

    assert called == ["action-boom", "after"]
    assert summary["passed"] == 1
    assert summary["failed"] == 2
    assert summary["cases"][0]["error"] == {
        "kind": "exception",
        "type": "OSError",
        "message": "case store unavailable",
    }
    assert summary["cases"][1]["error"] == {
        "kind": "exception",
        "type": "RuntimeError",
        "message": "backend crashed",
    }


def test_invalid_action_result_is_a_case_failure_and_batch_continues() -> None:
    summary = run_batch(
        ["bad-result.json", "good.json"],
        action=lambda case: ["not", "a", "mapping"] if case.name == "bad-result" else _result(case),
        case_loader=_loader,
    )

    assert summary["ok"] is False
    assert summary["passed"] == 1
    assert summary["failed"] == 1
    assert summary["cases"][0]["error"] == {
        "kind": "action",
        "reason": "action result must be a mapping",
        "returnedType": "list",
    }
    assert summary["cases"][1]["ok"] is True


def test_empty_batch_is_successful() -> None:
    assert run_batch([], action=_result, case_loader=_loader) == {
        "schemaVersion": 1,
        "action": "action",
        "total": 0,
        "passed": 0,
        "failed": 0,
        "ok": True,
        "cases": [],
    }


def test_mismatched_action_result_identity_is_a_case_failure() -> None:
    summary = run_batch(
        ["body.json", "after.json"],
        action=lambda case: {**_result(case), "caseName": "another-case"} if case.name == "body" else _result(case),
        case_loader=_loader,
    )

    assert summary["passed"] == 1
    assert summary["failed"] == 1
    assert summary["cases"][0]["error"] == {
        "kind": "action-contract",
        "reason": "action result identity does not match the loaded case",
        "fields": ["caseName"],
    }


def test_non_json_action_result_is_a_case_failure() -> None:
    summary = run_batch(
        ["bad.json", "after.json"],
        action=lambda case: _result(case, payload=Path("not-json")) if case.name == "bad" else _result(case),
        case_loader=_loader,
    )

    assert summary["passed"] == 1
    assert summary["failed"] == 1
    assert summary["cases"][0]["error"]["kind"] == "action-contract"
    assert summary["cases"][0]["error"]["reason"] == "action result is not JSON serializable"
    assert "result" not in summary["cases"][0]
