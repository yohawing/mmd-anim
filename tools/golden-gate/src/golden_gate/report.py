from __future__ import annotations

import json
from pathlib import Path
from typing import Any

from .compare import RegressionFailure


def load_report(path: Path) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as handle:
            value = json.load(handle)
    except OSError as error:
        raise ValueError(f"failed to read report {path}: {error}") from error
    except json.JSONDecodeError as error:
        raise ValueError(f"failed to parse report {path}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"report must be a JSON object: {path}")
    return value


def save_report(path: Path, report: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        json.dump(report, handle, ensure_ascii=False, indent=2)
        handle.write("\n")


def summarize_report(report: dict[str, Any]) -> str:
    summary = report.get("summary", {})
    if not isinstance(summary, dict):
        return "summary unavailable"
    fields = [
        "cases",
        "comparedCases",
        "missing",
        "importErrors",
        "comparedFrames",
        "comparedBones",
        "mismatchCount",
        "maxAbsError",
        "worst",
        "pairCount",
        "penetratingPairCount",
        "severePairCount",
        "jointConnectedPairCount",
        "jointConnectedPenetratingPairCount",
        "jointConnectedSeverePairCount",
        "unconnectedPairCount",
        "unconnectedPenetratingPairCount",
        "unconnectedSeverePairCount",
        "bulletContactCount",
        "penetratingContactCount",
        "maxPenetrationDepth",
        "maxBulletPenetrationDepth",
    ]
    parts = [f"{field}={summary.get(field)}" for field in fields if field in summary]
    return " ".join(parts) if parts else "summary empty"


def format_failures(failures: list[RegressionFailure]) -> str:
    if not failures:
        return "No regressions detected."
    lines = [f"{len(failures)} regression(s) detected:"]
    for failure in failures:
        lines.append(f"- {failure.path}: {failure.message}")
    return "\n".join(lines)
