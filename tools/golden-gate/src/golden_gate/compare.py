from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Iterable

from .config import GateOptions, Tolerances


@dataclass(frozen=True)
class RegressionFailure:
    path: str
    check: str
    baseline: Any
    current: Any
    tolerance: float | int | None
    message: str


STATUS_RANK = {
    "ok": 0,
    "pass": 0,
    "mismatch": 1,
    "skipped-unsupported": 1,
    "missing": 2,
    "import-error": 2,
}

SUMMARY_NUMERIC_FIELDS = {
    "maxAbsError": ("motionMaxAbsError",),
    "translationMaxError": ("motionTranslationMaxError", "translationMax"),
    "translationRmsError": ("motionTranslationRmsError", "translationRms"),
    "rotationMaxAngleRad": ("motionRotationMaxAngleRad", "rotationMaxRad"),
    "rotationRmsAngleRad": ("motionRotationRmsAngleRad", "rotationRmsRad"),
    "mismatchCount": ("motionMismatchCount", "motionMismatches"),
    "missing": ("motionMissing",),
    "importErrors": ("motionImportErrors",),
}

CASE_NUMERIC_FIELDS = {
    "maxAbsError": ("motionMaxAbsError",),
    "translationMaxError": ("motionTranslationMaxError", "translationMax"),
    "translationRmsError": ("motionTranslationRmsError", "translationRms"),
    "rotationMaxAngleRad": ("motionRotationMaxAngleRad", "rotationMaxRad"),
    "rotationRmsAngleRad": ("motionRotationRmsAngleRad", "rotationRmsRad"),
    "mismatchCount": ("motionMismatchCount", "motionMismatches"),
    "missing": ("motionMissing",),
    "importErrors": ("motionImportErrors",),
}

PENETRATION_SUMMARY_FIELDS = {
    "maxPenetrationDepth": (),
    "maxBulletPenetrationDepth": (),
    "penetratingPairCount": (),
    "severePairCount": (),
    "penetratingContactCount": (),
}

PENETRATION_IDENTITY_FIELDS = (
    "caseName",
    "oracleFrame",
    "evalFrame",
    "model",
    "motion",
    "metric",
)

COVERAGE_FIELDS = {
    "comparedFrames": ("motionComparedFrames", "motionFrames"),
    "comparedBones": ("motionComparedBones", "motionBones"),
}

COUNT_FIELDS = {
    "cases": ("motionCases",),
    "comparedCases": ("motionComparedCases",),
}


def compare_reports(
    baseline: dict[str, Any],
    current: dict[str, Any],
    tolerances: Tolerances | None = None,
    options: GateOptions | None = None,
) -> list[RegressionFailure]:
    tolerances = tolerances or Tolerances()
    options = options or GateOptions()
    failures: list[RegressionFailure] = []

    baseline_summary = _object_at(baseline, "summary")
    current_summary = _object_at(current, "summary")
    if _is_penetration_report(baseline) or _is_penetration_report(current):
        _compare_penetration_identity(baseline, current, failures)
        _compare_penetration_summary(baseline_summary, current_summary, tolerances, failures)
    _compare_summary(baseline_summary, current_summary, _has_physics_cases(current), tolerances, options, failures)
    _compare_cases(
        _case_map(baseline),
        _case_map(current),
        tolerances,
        options,
        failures,
    )
    return failures


def _compare_penetration_identity(
    baseline: dict[str, Any],
    current: dict[str, Any],
    failures: list[RegressionFailure],
) -> None:
    for field in PENETRATION_IDENTITY_FIELDS:
        baseline_value = baseline.get(field)
        current_value = current.get(field)
        if current_value != baseline_value:
            failures.append(
                RegressionFailure(
                    path=field,
                    check="fixedIdentity",
                    baseline=baseline_value,
                    current=current_value,
                    tolerance=None,
                    message=f"{field} changed: current {current_value!r} != baseline {baseline_value!r}",
                )
            )


def _compare_penetration_summary(
    baseline: dict[str, Any],
    current: dict[str, Any],
    tolerances: Tolerances,
    failures: list[RegressionFailure],
) -> None:
    _require_numeric_fields(current, "summary", PENETRATION_SUMMARY_FIELDS, failures)
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "maxPenetrationDepth",
        PENETRATION_SUMMARY_FIELDS["maxPenetrationDepth"],
        tolerances.penetration_max_depth_tolerance,
        failures,
    )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "maxBulletPenetrationDepth",
        PENETRATION_SUMMARY_FIELDS["maxBulletPenetrationDepth"],
        tolerances.bullet_penetration_max_depth_tolerance,
        failures,
    )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "penetratingPairCount",
        PENETRATION_SUMMARY_FIELDS["penetratingPairCount"],
        tolerances.penetrating_pair_count_tolerance,
        failures,
    )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "severePairCount",
        PENETRATION_SUMMARY_FIELDS["severePairCount"],
        tolerances.severe_pair_count_tolerance,
        failures,
    )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "penetratingContactCount",
        PENETRATION_SUMMARY_FIELDS["penetratingContactCount"],
        tolerances.penetrating_contact_count_tolerance,
        failures,
    )


def _compare_summary(
    baseline: dict[str, Any],
    current: dict[str, Any],
    has_physics_cases: bool,
    tolerances: Tolerances,
    options: GateOptions,
    failures: list[RegressionFailure],
) -> None:
    if has_physics_cases:
        _require_numeric_fields(
            current,
            "summary",
            {
                "translationMaxError": SUMMARY_NUMERIC_FIELDS["translationMaxError"],
                "translationRmsError": SUMMARY_NUMERIC_FIELDS["translationRmsError"],
                "rotationMaxAngleRad": SUMMARY_NUMERIC_FIELDS["rotationMaxAngleRad"],
                "rotationRmsAngleRad": SUMMARY_NUMERIC_FIELDS["rotationRmsAngleRad"],
            },
            failures,
        )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "maxAbsError",
        SUMMARY_NUMERIC_FIELDS["maxAbsError"],
        tolerances.max_abs_error_tolerance,
        failures,
    )
    if has_physics_cases:
        _compare_not_greater(
            baseline,
            current,
            "summary",
            "translationMaxError",
            SUMMARY_NUMERIC_FIELDS["translationMaxError"],
            tolerances.translation_max_error_tolerance,
            failures,
        )
        _compare_not_greater(
            baseline,
            current,
            "summary",
            "translationRmsError",
            SUMMARY_NUMERIC_FIELDS["translationRmsError"],
            tolerances.translation_rms_error_tolerance,
            failures,
        )
        _compare_not_greater(
            baseline,
            current,
            "summary",
            "rotationMaxAngleRad",
            SUMMARY_NUMERIC_FIELDS["rotationMaxAngleRad"],
            tolerances.rotation_max_angle_rad_tolerance,
            failures,
        )
        _compare_not_greater(
            baseline,
            current,
            "summary",
            "rotationRmsAngleRad",
            SUMMARY_NUMERIC_FIELDS["rotationRmsAngleRad"],
            tolerances.rotation_rms_angle_rad_tolerance,
            failures,
        )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "mismatchCount",
        SUMMARY_NUMERIC_FIELDS["mismatchCount"],
        tolerances.mismatch_count_tolerance,
        failures,
    )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "missing",
        SUMMARY_NUMERIC_FIELDS["missing"],
        tolerances.missing_tolerance,
        failures,
    )
    _compare_not_greater(
        baseline,
        current,
        "summary",
        "importErrors",
        SUMMARY_NUMERIC_FIELDS["importErrors"],
        tolerances.import_error_tolerance,
        failures,
    )
    for field, aliases in COVERAGE_FIELDS.items():
        _compare_not_lower(baseline, current, "summary", field, aliases, failures)
    if not options.allow_count_changes:
        for field, aliases in COUNT_FIELDS.items():
            _compare_equal(baseline, current, "summary", field, aliases, failures)
    if not options.allow_skipped_target_changes:
        _compare_set_equal(baseline, current, "summary", "skippedTargets", (), failures)


def _compare_cases(
    baseline_cases: dict[str, dict[str, Any]],
    current_cases: dict[str, dict[str, Any]],
    tolerances: Tolerances,
    options: GateOptions,
    failures: list[RegressionFailure],
) -> None:
    baseline_names = set(baseline_cases)
    current_names = set(current_cases)
    if not options.allow_count_changes:
        for name in sorted(baseline_names - current_names):
            failures.append(
                RegressionFailure(
                    path=f"perCase.{name}",
                    check="casePresence",
                    baseline="present",
                    current="missing",
                    tolerance=None,
                    message=f"case removed: {name}",
                )
            )
        for name in sorted(current_names - baseline_names):
            current = current_cases[name]
            prefix = f"perCase.{name}"
            failures.append(
                RegressionFailure(
                    path=prefix,
                    check="casePresence",
                    baseline="missing",
                    current="present",
                    tolerance=None,
                    message=f"case added: {name}",
                )
            )

    for name in sorted(current_names - baseline_names):
        current = current_cases[name]
        prefix = f"perCase.{name}"
        if _is_physics_case(current):
            _require_numeric_fields(
                current,
                prefix,
                {
                    "translationMaxError": CASE_NUMERIC_FIELDS["translationMaxError"],
                    "translationRmsError": CASE_NUMERIC_FIELDS["translationRmsError"],
                    "rotationMaxAngleRad": CASE_NUMERIC_FIELDS["rotationMaxAngleRad"],
                    "rotationRmsAngleRad": CASE_NUMERIC_FIELDS["rotationRmsAngleRad"],
                },
                failures,
            )
        _compare_required_physics_backend({}, current, prefix, options, failures)

    for name in sorted(baseline_names & current_names):
        baseline = baseline_cases[name]
        current = current_cases[name]
        prefix = f"perCase.{name}"
        is_physics_case = _is_physics_case(baseline) or _is_physics_case(current)
        _compare_status(baseline, current, prefix, failures)
        if is_physics_case:
            _require_numeric_fields(
                current,
                prefix,
                {
                    "translationMaxError": CASE_NUMERIC_FIELDS["translationMaxError"],
                    "translationRmsError": CASE_NUMERIC_FIELDS["translationRmsError"],
                    "rotationMaxAngleRad": CASE_NUMERIC_FIELDS["rotationMaxAngleRad"],
                    "rotationRmsAngleRad": CASE_NUMERIC_FIELDS["rotationRmsAngleRad"],
                },
                failures,
            )
        _compare_not_greater(
            baseline,
            current,
            prefix,
            "maxAbsError",
            CASE_NUMERIC_FIELDS["maxAbsError"],
            tolerances.max_abs_error_tolerance,
            failures,
        )
        if is_physics_case:
            _compare_not_greater(
                baseline,
                current,
                prefix,
                "translationMaxError",
                CASE_NUMERIC_FIELDS["translationMaxError"],
                tolerances.translation_max_error_tolerance,
                failures,
            )
            _compare_not_greater(
                baseline,
                current,
                prefix,
                "translationRmsError",
                CASE_NUMERIC_FIELDS["translationRmsError"],
                tolerances.translation_rms_error_tolerance,
                failures,
            )
            _compare_not_greater(
                baseline,
                current,
                prefix,
                "rotationMaxAngleRad",
                CASE_NUMERIC_FIELDS["rotationMaxAngleRad"],
                tolerances.rotation_max_angle_rad_tolerance,
                failures,
            )
            _compare_not_greater(
                baseline,
                current,
                prefix,
                "rotationRmsAngleRad",
                CASE_NUMERIC_FIELDS["rotationRmsAngleRad"],
                tolerances.rotation_rms_angle_rad_tolerance,
                failures,
            )
        _compare_not_greater(
            baseline,
            current,
            prefix,
            "mismatchCount",
            CASE_NUMERIC_FIELDS["mismatchCount"],
            tolerances.mismatch_count_tolerance,
            failures,
        )
        _compare_not_greater(
            baseline,
            current,
            prefix,
            "missing",
            CASE_NUMERIC_FIELDS["missing"],
            tolerances.missing_tolerance,
            failures,
        )
        _compare_not_greater(
            baseline,
            current,
            prefix,
            "importErrors",
            CASE_NUMERIC_FIELDS["importErrors"],
            tolerances.import_error_tolerance,
            failures,
        )
        _compare_required_physics_backend(baseline, current, prefix, options, failures)
        for field, aliases in COVERAGE_FIELDS.items():
            _compare_not_lower(baseline, current, prefix, field, aliases, failures)
        if not options.allow_skipped_target_changes:
            _compare_set_equal(baseline, current, prefix, "skippedTargets", (), failures)


def _compare_status(
    baseline: dict[str, Any],
    current: dict[str, Any],
    prefix: str,
    failures: list[RegressionFailure],
) -> None:
    baseline_status = str(baseline.get("status", "ok"))
    current_status = str(current.get("status", "ok"))
    baseline_rank = STATUS_RANK.get(baseline_status, 1)
    current_rank = STATUS_RANK.get(current_status, 1)
    if current_rank > baseline_rank:
        failures.append(
            RegressionFailure(
                path=f"{prefix}.status",
                check="status",
                baseline=baseline_status,
                current=current_status,
                tolerance=None,
                message=f"status worsened from {baseline_status} to {current_status}",
            )
        )


def _compare_required_physics_backend(
    baseline: dict[str, Any],
    current: dict[str, Any],
    prefix: str,
    options: GateOptions,
    failures: list[RegressionFailure],
) -> None:
    expected = options.required_physics_backend
    if expected is None or not (_is_physics_case(baseline) or _is_physics_case(current)):
        return
    current_backend = current.get("physicsBackend")
    if current_backend != expected:
        failures.append(
            RegressionFailure(
                path=f"{prefix}.physicsBackend",
                check="requiredPhysicsBackend",
                baseline=expected,
                current=current_backend,
                tolerance=None,
                message=f"{prefix}.physicsBackend must be {expected!r}, got {current_backend!r}",
            )
        )


def _is_physics_case(source: dict[str, Any]) -> bool:
    kind = source.get("kind")
    if isinstance(kind, str) and "physics" in kind.lower():
        return True
    backend = source.get("physicsBackend")
    return isinstance(backend, str) and backend.strip().lower() not in {"", "none"}


def _has_physics_cases(report_or_summary: dict[str, Any]) -> bool:
    cases = report_or_summary.get("perCase")
    if not isinstance(cases, list):
        return False
    return any(isinstance(case, dict) and _is_physics_case(case) for case in cases)


def _is_penetration_report(source: dict[str, Any]) -> bool:
    kind = source.get("kind")
    if isinstance(kind, str) and "penetration" in kind.lower():
        return True
    metric = source.get("metric")
    if isinstance(metric, str) and ("penetration" in metric.lower() or "bullet-contacts" in metric.lower()):
        return True
    summary = source.get("summary")
    return isinstance(summary, dict) and any(field in summary for field in PENETRATION_SUMMARY_FIELDS)


def _require_numeric_fields(
    source: dict[str, Any],
    prefix: str,
    fields: dict[str, Iterable[str]],
    failures: list[RegressionFailure],
) -> None:
    for field, aliases in fields.items():
        if _has_number(source, field, aliases):
            continue
        failures.append(
            RegressionFailure(
                path=f"{prefix}.{field}",
                check="requiredMetric",
                baseline="present",
                current="missing",
                tolerance=None,
                message=f"{prefix}.{field} is required for this report kind",
            )
        )


def _compare_not_greater(
    baseline: dict[str, Any],
    current: dict[str, Any],
    prefix: str,
    field: str,
    aliases: Iterable[str],
    tolerance: float | int,
    failures: list[RegressionFailure],
) -> None:
    baseline_value = _number(baseline, field, aliases)
    current_value = _number(current, field, aliases)
    if current_value > baseline_value + tolerance:
        failures.append(
            RegressionFailure(
                path=f"{prefix}.{field}",
                check="notGreater",
                baseline=baseline_value,
                current=current_value,
                tolerance=tolerance,
                message=(
                    f"{prefix}.{field} worsened: current {current_value} > "
                    f"baseline {baseline_value} + tolerance {tolerance}"
                ),
            )
        )


def _compare_not_lower(
    baseline: dict[str, Any],
    current: dict[str, Any],
    prefix: str,
    field: str,
    aliases: Iterable[str],
    failures: list[RegressionFailure],
) -> None:
    baseline_value = _number(baseline, field, aliases)
    current_value = _number(current, field, aliases)
    if current_value < baseline_value:
        failures.append(
            RegressionFailure(
                path=f"{prefix}.{field}",
                check="coverage",
                baseline=baseline_value,
                current=current_value,
                tolerance=None,
                message=f"{prefix}.{field} decreased: current {current_value} < baseline {baseline_value}",
            )
        )


def _compare_equal(
    baseline: dict[str, Any],
    current: dict[str, Any],
    prefix: str,
    field: str,
    aliases: Iterable[str],
    failures: list[RegressionFailure],
) -> None:
    baseline_value = _number(baseline, field, aliases)
    current_value = _number(current, field, aliases)
    if current_value != baseline_value:
        failures.append(
            RegressionFailure(
                path=f"{prefix}.{field}",
                check="fixedCount",
                baseline=baseline_value,
                current=current_value,
                tolerance=None,
                message=f"{prefix}.{field} changed: current {current_value} != baseline {baseline_value}",
            )
        )


def _compare_set_equal(
    baseline: dict[str, Any],
    current: dict[str, Any],
    prefix: str,
    field: str,
    aliases: Iterable[str],
    failures: list[RegressionFailure],
) -> None:
    baseline_value = set(_string_list(baseline, field, aliases))
    current_value = set(_string_list(current, field, aliases))
    if current_value != baseline_value:
        failures.append(
            RegressionFailure(
                path=f"{prefix}.{field}",
                check="fixedSet",
                baseline=sorted(baseline_value),
                current=sorted(current_value),
                tolerance=None,
                message=f"{prefix}.{field} changed",
            )
        )


def _object_at(report: dict[str, Any], key: str) -> dict[str, Any]:
    value = report.get(key)
    if isinstance(value, dict):
        return value
    return {}


def _case_map(report: dict[str, Any]) -> dict[str, dict[str, Any]]:
    cases = report.get("perCase", [])
    mapped: dict[str, dict[str, Any]] = {}
    if not isinstance(cases, list):
        return mapped
    for index, case in enumerate(cases):
        if not isinstance(case, dict):
            continue
        name = case.get("name")
        if isinstance(name, str) and name:
            mapped[name] = case
        else:
            mapped[f"#{index}"] = case
    return mapped


def _number(source: dict[str, Any], field: str, aliases: Iterable[str]) -> float:
    for key in (field, *aliases):
        value = source.get(key)
        if isinstance(value, bool):
            continue
        if isinstance(value, (int, float)):
            return float(value)
    return 0.0


def _has_number(source: dict[str, Any], field: str, aliases: Iterable[str]) -> bool:
    for key in (field, *aliases):
        value = source.get(key)
        if isinstance(value, bool):
            continue
        if isinstance(value, (int, float)):
            return True
    return False


def _string_list(source: dict[str, Any], field: str, aliases: Iterable[str]) -> list[str]:
    for key in (field, *aliases):
        value = source.get(key)
        if isinstance(value, list):
            return [item for item in value if isinstance(item, str)]
    return []
