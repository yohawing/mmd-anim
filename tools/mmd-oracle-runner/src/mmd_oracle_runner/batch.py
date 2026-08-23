"""Continue-on-error orchestration for already implemented case actions.

This module deliberately does not import or call the prepare/record
implementations.  Callers inject one action (for example, a wrapper around
``prepare_case`` or ``record_case``), which keeps batch policy independent of
subprocess and MMD-launch policy.
"""

from __future__ import annotations

import json
from collections.abc import Callable, Iterable, Mapping
from pathlib import Path
from typing import Any

from .case import CaseValidationError, OracleCase, load_case


CaseLoader = Callable[[str | Path], OracleCase]
CaseAction = Callable[[OracleCase], Mapping[str, Any]]


def run_batch(
    case_paths: Iterable[str | Path],
    *,
    action: CaseAction,
    action_name: str = "action",
    case_loader: CaseLoader = load_case,
) -> dict[str, Any]:
    """Run an injected action for every case, retaining failures per case.

    ``case_paths`` is consumed once and its iteration order is reflected in
    the returned ``cases`` list.  Validation failures, action-declared
    failures, and unexpected exceptions are isolated to their case so one
    bad input cannot prevent subsequent cases from running.
    """

    cases: list[dict[str, Any]] = []
    for index, case_path in enumerate(case_paths):
        path_text = str(case_path)
        entry: dict[str, Any] = {
            "index": index,
            "casePath": path_text,
            "action": action_name,
        }
        try:
            case = case_loader(case_path)
        except CaseValidationError as error:
            entry.update(
                {
                    "ok": False,
                    "error": {"kind": "validation", **error.as_dict()},
                }
            )
            cases.append(entry)
            continue
        except Exception as error:  # noqa: BLE001 - continue-on-error boundary
            entry.update({"ok": False, "error": _exception_error(error)})
            cases.append(entry)
            continue

        entry["caseName"] = case.name
        try:
            action_result = action(case)
            if not isinstance(action_result, Mapping):
                entry.update(
                    {
                        "ok": False,
                        "error": {
                            "kind": "action",
                            "reason": "action result must be a mapping",
                            "returnedType": type(action_result).__name__,
                        },
                    }
                )
            else:
                result = dict(action_result)
                contract_error = _action_contract_error(case, result)
                if contract_error is not None:
                    entry.update({"ok": False, "error": contract_error})
                    if contract_error["reason"] != "action result is not JSON serializable":
                        entry["result"] = result
                elif result.get("ok") is True:
                    entry["result"] = result
                    entry["ok"] = True
                else:
                    entry["result"] = result
                    entry.update(
                        {
                            "ok": False,
                            "error": {
                                "kind": "action",
                                "reason": "action reported failure",
                            },
                        }
                    )
        except Exception as error:  # noqa: BLE001 - continue-on-error boundary
            entry.update({"ok": False, "error": _exception_error(error)})
        cases.append(entry)

    passed = sum(1 for case in cases if case["ok"] is True)
    failed = len(cases) - passed
    return {
        "schemaVersion": 1,
        "action": action_name,
        "total": len(cases),
        "passed": passed,
        "failed": failed,
        "ok": failed == 0,
        "cases": cases,
    }


def _exception_error(error: Exception) -> dict[str, str]:
    return {
        "kind": "exception",
        "type": type(error).__name__,
        "message": str(error),
    }


def _action_contract_error(case: OracleCase, result: dict[str, Any]) -> dict[str, Any] | None:
    try:
        json.dumps(result, ensure_ascii=True, sort_keys=True, allow_nan=False)
    except (TypeError, ValueError) as error:
        return {
            "kind": "action-contract",
            "reason": "action result is not JSON serializable",
            "message": str(error),
        }
    expected = {
        "schemaVersion": 1,
        "caseFile": str(case.source_path),
        "caseName": case.name,
        "frames": list(case.frames),
    }
    mismatches = [field for field, value in expected.items() if result.get(field) != value]
    if mismatches:
        return {
            "kind": "action-contract",
            "reason": "action result identity does not match the loaded case",
            "fields": mismatches,
        }
    return None
