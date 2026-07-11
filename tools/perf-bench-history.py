#!/usr/bin/env python3
"""Normalize and compare local mmd-anim benchmark history records.

This is a local helper for PERF-0.2-5a. It intentionally uses only the Python
standard library so it can run from a plain checkout.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import math
import os
import platform
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
KIND = "perfBenchHistory"
REPORT_KIND = "perfBenchHistoryCompare"


def utc_now() -> str:
    return dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def read_jsonl(path: Path) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    with path.open("r", encoding="utf-8") as handle:
        for line_number, line in enumerate(handle, 1):
            text = line.strip()
            if not text:
                continue
            value = json.loads(text)
            if not isinstance(value, dict):
                raise ValueError(f"{path}:{line_number}: JSONL entry must be an object")
            records.append(value)
    return records


def write_jsonl(path: Path, records: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as handle:
        for record in records:
            handle.write(json.dumps(record, ensure_ascii=False, sort_keys=True, separators=(",", ":"), allow_nan=False))
            handle.write("\n")


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True, allow_nan=False) + "\n", encoding="utf-8")


def get_path(value: dict[str, Any], *keys: str, default: Any = None) -> Any:
    current: Any = value
    for key in keys:
        if not isinstance(current, dict) or key not in current:
            return default
        current = current[key]
    return current


def as_float(value: Any, default: float | None = None) -> float | None:
    if value is None:
        return default
    if isinstance(value, bool):
        raise ValueError(f"float value must not be boolean: {value!r}")
    if not isinstance(value, (int, float)):
        raise ValueError(f"float value must be numeric: {value!r}")
    number = float(value)
    if not math.isfinite(number):
        raise ValueError(f"non-finite float value: {value!r}")
    return number


def as_int(value: Any, default: int | None = None) -> int | None:
    if value is None:
        return default
    if isinstance(value, bool):
        raise ValueError(f"integer value must not be boolean: {value!r}")
    if isinstance(value, int):
        return value
    if isinstance(value, float) and math.isfinite(value) and value.is_integer():
        return int(value)
    raise ValueError(f"integer value must be integral: {value!r}")


def require_positive_int(value: Any, label: str) -> int:
    number = as_int(value)
    if number is None or number <= 0:
        raise ValueError(f"{label} must be greater than zero")
    return number


def require_non_negative_float(value: Any, label: str) -> float:
    number = as_float(value)
    if number is None or number < 0:
        raise ValueError(f"{label} must be non-negative")
    return number


def require_positive_float(value: Any, label: str) -> float:
    number = as_float(value)
    if number is None or number <= 0:
        raise ValueError(f"{label} must be greater than zero")
    return number


def as_bool(value: Any, label: str) -> bool:
    if not isinstance(value, bool):
        raise ValueError(f"{label} must be a boolean")
    return value


def require_fields(value: dict[str, Any], fields: tuple[str, ...], label: str) -> None:
    missing = [field for field in fields if field not in value]
    if missing:
        raise ValueError(f"{label} missing required field(s): {', '.join(missing)}")


def require_nested(value: dict[str, Any], path: tuple[str, ...], fields: tuple[str, ...], label: str) -> dict[str, Any]:
    nested = get_path(value, *path)
    if not isinstance(nested, dict):
        raise ValueError(f"{label} missing required object: {'.'.join(path)}")
    require_fields(nested, fields, f"{label}.{'.'.join(path)}")
    return nested


def canonical_number(value: float | int | None) -> str:
    if value is None:
        return "null"
    number = float(value)
    if number.is_integer():
        return str(int(number))
    return f"{number:.9g}"


def normalized_path_text(path: Path) -> str:
    return os.path.normcase(os.path.normpath(str(path))).replace("\\", "/")


def rel_or_hashed_asset_key(paths: list[str], repo_root: Path) -> str:
    repo_root = repo_root.resolve()
    parts: list[str] = []
    absolute_path_keys: list[str] = []
    all_inside_repo = True
    for raw_path in paths:
        path = Path(raw_path)
        if not path.is_absolute():
            path = (repo_root / path).resolve()
        resolved = path.resolve()
        absolute_path_keys.append(os.path.normcase(os.path.normpath(str(resolved))))
        try:
            relative = resolved.relative_to(repo_root)
            parts.append(normalized_path_text(relative))
        except ValueError:
            all_inside_repo = False
            parts.append(normalized_path_text(Path(resolved.name)))
    if all_inside_repo:
        return "+".join(parts)
    digest = hashlib.sha256("\n".join(absolute_path_keys).encode("utf-8")).hexdigest()[:12]
    return f"{'+'.join(parts)}#{digest}"


def environment(args: argparse.Namespace) -> dict[str, Any]:
    env = {
        "gitSha": args.git_sha,
        "buildProfile": args.build_profile,
        "rustcVersion": args.rustc_version,
        "host": args.host or platform.node(),
    }
    return {key: value for key, value in env.items() if value is not None}


def base_record(
    args: argparse.Namespace,
    bench_kind: str,
    source: dict[str, Any],
    scenario: dict[str, Any],
    compare_key: str,
    deterministic: dict[str, Any],
    metrics: dict[str, Any],
    raw: dict[str, Any],
) -> dict[str, Any]:
    return {
        "schemaVersion": SCHEMA_VERSION,
        "kind": KIND,
        "capturedAt": args.captured_at or utc_now(),
        "compareKey": compare_key,
        "benchKind": bench_kind,
        "source": source,
        "scenario": scenario,
        "environment": environment(args),
        "deterministic": deterministic,
        "metrics": metrics,
        "raw": raw,
    }


def normalize_synthetic(raw: dict[str, Any], args: argparse.Namespace) -> dict[str, Any]:
    require_fields(raw, ("models", "bones", "frames", "elapsedMs", "totalFrames", "fps", "checksum"), "synthetic")
    models = require_positive_int(raw.get("models"), "synthetic.models")
    bones = require_positive_int(raw.get("bones"), "synthetic.bones")
    frames = require_positive_int(raw.get("frames"), "synthetic.frames")
    elapsed_ms = require_non_negative_float(raw.get("elapsedMs"), "synthetic.elapsedMs")
    total_frames = require_positive_float(raw.get("totalFrames"), "synthetic.totalFrames")
    ms_per_eval = elapsed_ms / total_frames
    scenario = {"models": models, "bones": bones, "frames": frames}
    return base_record(
        args,
        "synthetic",
        {"tool": "mmd-anim-cli", "command": args.command or "bench --synthetic --json", "rawFormat": "syntheticCli"},
        scenario,
        f"synthetic/{models}/{bones}/{frames}",
        {"status": raw.get("status", "ok"), "checksum": raw.get("checksum")},
        {
            "primary": {"msPerEvaluation": ms_per_eval, "elapsedMs": elapsed_ms},
            "secondary": {"fps": require_non_negative_float(raw.get("fps"), "synthetic.fps"), "totalFrames": as_int(raw.get("totalFrames"))},
        },
        raw,
    )


def normalize_pair(raw: dict[str, Any], args: argparse.Namespace) -> dict[str, Any]:
    require_fields(raw, ("status", "command", "mode", "model", "motion", "config", "timing", "result"), "pair")
    if raw.get("mode") != "pair":
        raise ValueError(f"pair mode must be 'pair', got {raw.get('mode')!r}")
    if raw.get("command") != "bench":
        raise ValueError(f"pair command must be 'bench', got {raw.get('command')!r}")
    repo_root = Path(args.repo_root).resolve()
    model_path = str(raw.get("model", ""))
    motion_path = str(raw.get("motion", ""))
    asset_key = rel_or_hashed_asset_key([model_path, motion_path], repo_root)
    config = require_nested(raw, ("config",), ("startFrame", "frameCount", "step", "instances", "solveIk"), "pair")
    timing = require_nested(raw, ("timing",), ("msPerEvaluation", "hotLoopMs", "evalMs"), "pair")
    result = require_nested(raw, ("result",), ("checksum", "morphChecksum"), "pair")
    solve_ik = as_bool(config.get("solveIk"), "pair.config.solveIk")
    ik_label = "ik" if solve_ik else "no-ik"
    start_frame = require_non_negative_float(config.get("startFrame"), "pair.config.startFrame")
    frame_count = require_positive_int(config.get("frameCount"), "pair.config.frameCount")
    step = require_positive_float(config.get("step"), "pair.config.step")
    instances = require_positive_int(config.get("instances"), "pair.config.instances")
    ik_tolerance = as_float(config.get("ikTolerance")) if config.get("ikTolerance") is not None else None
    ik_max_iterations_cap = as_int(config.get("ikMaxIterationsCap")) if config.get("ikMaxIterationsCap") is not None else None
    scenario = {
        "assetKey": asset_key,
        "instances": instances,
        "startFrame": start_frame,
        "frameCount": frame_count,
        "step": step,
        "solveIk": solve_ik,
        "ikTolerance": ik_tolerance,
        "ikMaxIterationsCap": ik_max_iterations_cap,
    }
    secondary = {
        "poseEvalMs": as_float(timing.get("poseEvalMs")),
        "applyPoseMs": as_float(timing.get("applyPoseMs")),
        "morphExpandMs": as_float(timing.get("morphExpandMs")),
        "worldCopyMs": as_float(timing.get("worldCopyMs")),
        "skinningCopyMs": as_float(timing.get("skinningCopyMs")),
        "morphCopyMs": as_float(timing.get("morphCopyMs")),
        "evaluationsPerSecond": as_float(timing.get("evaluationsPerSecond")),
    }
    if "ik" in raw:
        secondary["ik"] = raw["ik"]
    return base_record(
        args,
        "pair",
        {"tool": "mmd-anim-cli", "command": args.command or "bench <pmx> <vmd> --json", "rawFormat": "pairCli"},
        scenario,
        f"pair/{asset_key}/{instances}/{canonical_number(start_frame)}/{frame_count}/{canonical_number(step)}/{ik_label}",
        {"status": raw.get("status", "ok"), "checksum": result.get("checksum"), "morphChecksum": result.get("morphChecksum")},
        {
            "primary": {
                "msPerEvaluation": require_non_negative_float(timing.get("msPerEvaluation"), "pair.timing.msPerEvaluation"),
                "hotLoopMs": require_non_negative_float(timing.get("hotLoopMs"), "pair.timing.hotLoopMs"),
                "evalMs": require_non_negative_float(timing.get("evalMs"), "pair.timing.evalMs"),
            },
            "secondary": secondary,
        },
        raw,
    )


def normalize_real_model_runtime(raw: dict[str, Any], args: argparse.Namespace) -> dict[str, Any] | None:
    require_fields(raw, ("kind", "status"), "realModelRuntime")
    if raw.get("kind") != "realModelRuntimeBench":
        raise ValueError(f"realModelRuntime kind must be 'realModelRuntimeBench', got {raw.get('kind')!r}")
    status = raw.get("status", "ok")
    if status == "skipped":
        return None
    repo_root = Path(args.repo_root).resolve()
    assets = require_nested(raw, ("assets",), ("pmx", "vmd"), "realModelRuntime")
    asset_key = rel_or_hashed_asset_key([str(assets.get("pmx", "")), str(assets.get("vmd", ""))], repo_root)
    scenario_raw = require_nested(raw, ("scenario",), ("modelCount",), "realModelRuntime")
    frame_range = require_nested(raw, ("frameRange",), ("startFrame", "frameCount", "step"), "realModelRuntime")
    timings = require_nested(raw, ("timingsMsPerFrame",), ("evalMs", "worldCopyMs", "skinningCopyMs", "morphCopyMs"), "realModelRuntime")
    model_count = require_positive_int(scenario_raw.get("modelCount"), "realModelRuntime.scenario.modelCount")
    start_frame = require_non_negative_float(frame_range.get("startFrame"), "realModelRuntime.frameRange.startFrame")
    frame_count = require_positive_int(frame_range.get("frameCount"), "realModelRuntime.frameRange.frameCount")
    step = require_positive_float(frame_range.get("step"), "realModelRuntime.frameRange.step")
    scenario = {
        "assetKey": asset_key,
        "modelCount": model_count,
        "startFrame": start_frame,
        "frameCount": frame_count,
        "step": step,
    }
    return base_record(
        args,
        "realModelRuntime",
        {"tool": "cargo-bench", "command": args.command or "cargo bench -p mmd-anim --bench real_model_runtime", "rawFormat": "realModelRuntimeBench"},
        scenario,
        f"realModelRuntime/{asset_key}/{model_count}/{canonical_number(start_frame)}/{frame_count}/{canonical_number(step)}",
        {"status": status, "model": raw.get("model")},
        {
            "primary": {
                "evalMsPerFrame": require_non_negative_float(timings.get("evalMs"), "realModelRuntime.timingsMsPerFrame.evalMs"),
                "worldCopyMsPerFrame": require_non_negative_float(timings.get("worldCopyMs"), "realModelRuntime.timingsMsPerFrame.worldCopyMs"),
                "skinningCopyMsPerFrame": require_non_negative_float(timings.get("skinningCopyMs"), "realModelRuntime.timingsMsPerFrame.skinningCopyMs"),
                "morphCopyMsPerFrame": require_non_negative_float(timings.get("morphCopyMs"), "realModelRuntime.timingsMsPerFrame.morphCopyMs"),
            },
            "secondary": {"setupMs": raw.get("setupMs"), "iterations": raw.get("iterations")},
        },
        raw,
    )


def normalize(args: argparse.Namespace) -> int:
    raw_records = read_jsonl(Path(args.raw))
    normalized: list[dict[str, Any]] = []
    for raw in raw_records:
        if args.bench_kind == "synthetic":
            normalized.append(normalize_synthetic(raw, args))
        elif args.bench_kind == "pair":
            normalized.append(normalize_pair(raw, args))
        elif args.bench_kind == "real-model-runtime":
            record = normalize_real_model_runtime(raw, args)
            if record is not None:
                normalized.append(record)
        else:
            raise ValueError(f"unknown bench kind: {args.bench_kind}")
    write_jsonl(Path(args.out), normalized)
    print(f"wrote {len(normalized)} normalized records to {args.out}")
    return 0


def validate_history_record(record: dict[str, Any]) -> None:
    if record.get("schemaVersion") != SCHEMA_VERSION:
        raise ValueError(f"record has unsupported schemaVersion: {record.get('schemaVersion')!r}")
    if record.get("kind") != KIND:
        raise ValueError(f"record has unsupported kind: {record.get('kind')!r}")
    if not isinstance(record.get("benchKind"), str):
        raise ValueError("record missing string benchKind")
    if not isinstance(record.get("compareKey"), str):
        raise ValueError("record missing string compareKey")


def records_by_key(records: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for record in records:
        validate_history_record(record)
        key = record.get("compareKey")
        if key in result:
            raise ValueError(f"duplicate compareKey: {key}")
        result[key] = record
    return result


def timing_tolerance(record: dict[str, Any], metric: str) -> float | None:
    bench_kind = record.get("benchKind")
    if bench_kind == "synthetic" and metric == "msPerEvaluation":
        return 0.05
    if bench_kind == "pair" and metric in {"msPerEvaluation", "hotLoopMs"}:
        return 0.05
    if bench_kind == "realModelRuntime" and metric == "evalMsPerFrame":
        return 0.07
    return None


def compare_metric(metric: str, baseline: float, current: float, tolerance: float | None) -> dict[str, Any]:
    if not math.isfinite(baseline) or not math.isfinite(current):
        raise ValueError(f"{metric}: metric values must be finite")
    if baseline < 0 or current < 0:
        raise ValueError(f"{metric}: metric values must be non-negative")
    delta = current - baseline
    delta_ratio = None if baseline == 0 else delta / baseline
    status = "observed" if tolerance is None else "pass"
    if tolerance is not None:
        if baseline == 0:
            if current > 0:
                status = "regression"
        elif current > baseline * (1.0 + tolerance):
            status = "regression"
    return {
        "metric": metric,
        "baseline": baseline,
        "current": current,
        "delta": delta,
        "deltaRatio": delta_ratio,
        "tolerance": tolerance,
        "status": status,
    }


def compare_deterministic(baseline: dict[str, Any], current: dict[str, Any]) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    baseline_det = baseline.get("deterministic", {})
    current_det = current.get("deterministic", {})
    for field in ("status", "checksum", "morphChecksum"):
        if field not in baseline_det and field not in current_det:
            continue
        status = "pass" if baseline_det.get(field) == current_det.get(field) else "regression"
        results.append(
            {
                "field": field,
                "baseline": baseline_det.get(field),
                "current": current_det.get(field),
                "status": status,
            }
        )
    if baseline.get("benchKind") == "realModelRuntime":
        for field in ("boneCount", "ikCount", "morphCount"):
            baseline_value = get_path(baseline_det, "model", field)
            current_value = get_path(current_det, "model", field)
            if baseline_value is None and current_value is None:
                continue
            status = "pass" if baseline_value == current_value else "regression"
            results.append({"field": f"model.{field}", "baseline": baseline_value, "current": current_value, "status": status})
    return results


def compare_identity(baseline: dict[str, Any], current: dict[str, Any]) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for field in ("schemaVersion", "kind", "benchKind"):
        status = "pass" if baseline.get(field) == current.get(field) else "regression"
        results.append({"field": field, "baseline": baseline.get(field), "current": current.get(field), "status": status})
    return results


def environment_warnings(baseline: dict[str, Any], current: dict[str, Any]) -> list[dict[str, Any]]:
    warnings: list[dict[str, Any]] = []
    baseline_env = baseline.get("environment", {})
    current_env = current.get("environment", {})
    for field in ("buildProfile", "rustcVersion", "host"):
        baseline_value = baseline_env.get(field)
        current_value = current_env.get(field)
        if baseline_value is not None and current_value is not None and baseline_value != current_value:
            warnings.append({"field": field, "baseline": baseline_value, "current": current_value})
    return warnings


def scenario_warnings(baseline: dict[str, Any], current: dict[str, Any]) -> list[dict[str, Any]]:
    if baseline.get("benchKind") != "pair" or current.get("benchKind") != "pair":
        return []
    baseline_scenario = baseline.get("scenario", {})
    current_scenario = current.get("scenario", {})
    warnings: list[dict[str, Any]] = []
    for field in ("ikTolerance", "ikMaxIterationsCap"):
        baseline_value = baseline_scenario.get(field) if isinstance(baseline_scenario, dict) else None
        current_value = current_scenario.get(field) if isinstance(current_scenario, dict) else None
        if baseline_value != current_value:
            warnings.append({"field": field, "baseline": baseline_value, "current": current_value})
    return warnings


def compare_missing_metric(metric: str, baseline: Any, current: Any, tolerance: float | None) -> dict[str, Any]:
    if current is None:
        status = "missingCurrent"
    else:
        status = "missingBaseline"
    return {
        "metric": metric,
        "baseline": baseline,
        "current": current,
        "delta": None,
        "deltaRatio": None,
        "tolerance": tolerance,
        "status": status,
    }


def finite_metric_value(metric: str, value: Any) -> float:
    if isinstance(value, bool):
        raise ValueError(f"{metric}: metric values must not be boolean")
    if not isinstance(value, (int, float)):
        raise ValueError(f"{metric}: metric values must be numeric")
    number = float(value)
    if not math.isfinite(number):
        raise ValueError(f"{metric}: metric values must be finite")
    if number < 0:
        raise ValueError(f"{metric}: metric values must be non-negative")
    return number


def compare(args: argparse.Namespace) -> int:
    baseline = records_by_key(read_jsonl(Path(args.baseline)))
    current = records_by_key(read_jsonl(Path(args.current)))
    compared: list[dict[str, Any]] = []
    missing_baseline: list[str] = []
    missing_current = sorted(set(baseline) - set(current))
    regressions: list[dict[str, Any]] = []
    env_warnings: list[dict[str, Any]] = []
    scen_warnings: list[dict[str, Any]] = []

    for key, current_record in sorted(current.items()):
        baseline_record = baseline.get(key)
        if baseline_record is None:
            missing_baseline.append(key)
            continue
        identity_results = compare_identity(baseline_record, current_record)
        for result in identity_results:
            if result["status"] == "regression":
                regressions.append({"compareKey": key, "kind": "identity", **result})
        for warning in environment_warnings(baseline_record, current_record):
            env_warnings.append({"compareKey": key, **warning})
        current_scenario_warnings = [{"compareKey": key, **warning} for warning in scenario_warnings(baseline_record, current_record)]
        if current_scenario_warnings:
            scen_warnings.extend(current_scenario_warnings)
            compared.append(
                {
                    "compareKey": key,
                    "benchKind": current_record.get("benchKind"),
                    "identity": identity_results,
                    "deterministic": [],
                    "metrics": [],
                    "skipped": True,
                    "skipReason": "scenarioWarning",
                }
            )
            continue
        metric_results: list[dict[str, Any]] = []
        current_primary = get_path(current_record, "metrics", "primary", default={})
        baseline_primary = get_path(baseline_record, "metrics", "primary", default={})
        if not isinstance(current_primary, dict) or not isinstance(baseline_primary, dict):
            raise ValueError(f"{key}: metrics.primary must be an object")
        for metric in sorted(set(baseline_primary) | set(current_primary)):
            current_has_metric = metric in current_primary
            baseline_has_metric = metric in baseline_primary
            current_value = finite_metric_value(metric, current_primary[metric]) if current_has_metric else None
            baseline_value = finite_metric_value(metric, baseline_primary[metric]) if baseline_has_metric else None
            tolerance = timing_tolerance(current_record, metric)
            if not baseline_has_metric or not current_has_metric:
                result = compare_missing_metric(metric, baseline_value, current_value, tolerance)
                metric_results.append(result)
                if result["status"] == "missingCurrent":
                    regressions.append({"compareKey": key, "kind": "timingMissing", **result})
                continue
            result = compare_metric(metric, float(baseline_value), float(current_value), tolerance)
            metric_results.append(result)
            if result["status"] == "regression":
                regressions.append({"compareKey": key, "kind": "timing", **result})
        deterministic_results = compare_deterministic(baseline_record, current_record)
        for result in deterministic_results:
            if result["status"] == "regression":
                regressions.append({"compareKey": key, "kind": "deterministic", **result})
        compared.append(
            {
                "compareKey": key,
                "benchKind": current_record.get("benchKind"),
                "identity": identity_results,
                "deterministic": deterministic_results,
                "metrics": metric_results,
            }
        )

    status = "pass"
    if regressions:
        status = "regression"
    elif missing_current:
        status = "incomplete"
    elif scen_warnings:
        status = "incomplete"
    elif missing_baseline:
        status = "passWithMissingBaseline"

    report = {
        "schemaVersion": 1,
        "kind": REPORT_KIND,
        "createdAt": utc_now(),
        "baseline": str(args.baseline),
        "current": str(args.current),
        "failOnRegression": bool(args.fail_on_regression),
        "summary": {
            "baselineRecords": len(baseline),
            "currentRecords": len(current),
            "compared": len(compared),
            "missingBaseline": len(missing_baseline),
            "missingCurrent": len(missing_current),
            "regressions": len(regressions),
            "environmentWarnings": len(env_warnings),
            "scenarioWarnings": len(scen_warnings),
            "status": status,
        },
        "missingBaseline": missing_baseline,
        "missingCurrent": missing_current,
        "environmentWarnings": env_warnings,
        "scenarioWarnings": scen_warnings,
        "regressions": regressions,
        "comparisons": compared,
    }
    write_json(Path(args.report), report)
    print(f"wrote compare report to {args.report}")
    if args.fail_on_regression and (regressions or missing_current or scen_warnings):
        return 1
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command_name", required=True)

    normalize_parser = subcommands.add_parser("normalize", help="convert raw benchmark JSONL to perfBenchHistory records")
    normalize_parser.add_argument("--bench-kind", choices=["synthetic", "pair", "real-model-runtime"], required=True)
    normalize_parser.add_argument("--raw", required=True)
    normalize_parser.add_argument("--out", required=True)
    normalize_parser.add_argument("--repo-root", default=".")
    normalize_parser.add_argument("--captured-at")
    normalize_parser.add_argument("--git-sha")
    normalize_parser.add_argument("--build-profile", default="release")
    normalize_parser.add_argument("--rustc-version")
    normalize_parser.add_argument("--host")
    normalize_parser.add_argument("--command")
    normalize_parser.set_defaults(func=normalize)

    compare_parser = subcommands.add_parser("compare", help="compare current normalized records against a baseline")
    compare_parser.add_argument("--baseline", required=True)
    compare_parser.add_argument("--current", required=True)
    compare_parser.add_argument("--report", required=True)
    compare_parser.add_argument("--fail-on-regression", action="store_true")
    compare_parser.set_defaults(func=compare)
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    try:
        return int(args.func(args))
    except Exception as exc:  # noqa: BLE001 - CLI input errors should become exit 2.
        print(f"error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
