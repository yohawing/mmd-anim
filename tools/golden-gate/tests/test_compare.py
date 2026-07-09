from __future__ import annotations

from golden_gate.compare import compare_reports
from golden_gate.config import GateOptions, Tolerances


def report(summary=None, cases=None):
    return {
        "summary": {
            "cases": 1,
            "comparedCases": 1,
            "missing": 0,
            "importErrors": 0,
            "comparedFrames": 10,
            "comparedBones": 20,
            "mismatchCount": 0,
            "maxAbsError": 0.1,
            "skippedTargets": [],
            **(summary or {}),
        },
        "perCase": [
            {
                "name": "case-a",
                "kind": "motion-numeric",
                "status": "ok",
                "comparedFrames": 10,
                "comparedBones": 20,
                "mismatchCount": 0,
                "maxAbsError": 0.1,
                "skippedTargets": [],
                **((cases or [{}])[0]),
            }
        ],
    }


def failure_paths(failures):
    return {failure.path for failure in failures}


def test_equal_reports_pass():
    assert compare_reports(report(), report()) == []


def test_max_abs_error_uses_tolerance_boundary():
    baseline = report()
    current = report({"maxAbsError": 0.15}, [{"maxAbsError": 0.15}])

    assert compare_reports(baseline, current, Tolerances(max_abs_error_tolerance=0.05)) == []
    failures = compare_reports(baseline, current, Tolerances(max_abs_error_tolerance=0.049))

    assert "summary.maxAbsError" in failure_paths(failures)
    assert "perCase.case-a.maxAbsError" in failure_paths(failures)


def test_mismatch_missing_and_import_error_tolerances():
    baseline = report()
    current = report(
        {"mismatchCount": 2, "missing": 1, "importErrors": 1},
        [{"mismatchCount": 2, "missing": 1, "importErrors": 1}],
    )

    assert compare_reports(
        baseline,
        current,
        Tolerances(mismatch_count_tolerance=2, missing_tolerance=1, import_error_tolerance=1),
    ) == []
    failures = compare_reports(baseline, current)

    assert {"summary.mismatchCount", "summary.missing", "summary.importErrors"}.issubset(
        failure_paths(failures)
    )


def test_coverage_decrease_fails_even_when_counts_are_allowed():
    failures = compare_reports(
        report(),
        report({"comparedFrames": 9}, [{"comparedFrames": 9}]),
        options=GateOptions(allow_count_changes=True),
    )

    assert "summary.comparedFrames" in failure_paths(failures)
    assert "perCase.case-a.comparedFrames" in failure_paths(failures)


def test_case_presence_is_fixed_unless_allowed():
    baseline = report()
    current = {
        "summary": {**baseline["summary"], "cases": 0, "comparedCases": 0},
        "perCase": [],
    }

    assert "perCase.case-a" in failure_paths(compare_reports(baseline, current))
    assert compare_reports(baseline, current, options=GateOptions(allow_count_changes=True)) == []


def test_skipped_targets_are_fixed_unless_allowed():
    failures = compare_reports(report(), report({"skippedTargets": ["morphs"]}, [{"skippedTargets": ["morphs"]}]))

    assert "summary.skippedTargets" in failure_paths(failures)
    assert "perCase.case-a.skippedTargets" in failure_paths(failures)
    assert compare_reports(
        report(),
        report({"skippedTargets": ["morphs"]}, [{"skippedTargets": ["morphs"]}]),
        options=GateOptions(allow_skipped_target_changes=True),
    ) == []


def test_per_case_regression_is_not_hidden_by_summary():
    baseline = report()
    current = report({"maxAbsError": 0.1}, [{"maxAbsError": 0.2}])

    paths = failure_paths(compare_reports(baseline, current))

    assert "summary.maxAbsError" not in paths
    assert "perCase.case-a.maxAbsError" in paths


def test_legacy_motion_prefixed_fields_are_supported():
    baseline = report({"motionMaxAbsError": 0.1})
    del baseline["summary"]["maxAbsError"]
    del baseline["perCase"][0]["maxAbsError"]
    baseline["perCase"][0]["motionMaxAbsError"] = 0.1
    current = report({"motionMaxAbsError": 0.11})
    del current["summary"]["maxAbsError"]
    del current["perCase"][0]["maxAbsError"]
    current["perCase"][0]["motionMaxAbsError"] = 0.11

    assert compare_reports(baseline, current, Tolerances(max_abs_error_tolerance=0.01)) == []


def test_physics_metrics_are_ignored_for_non_physics_cases():
    current = report(
        {"motionTranslationRmsError": 10.0},
        [{"physicsBackend": "none", "translationRmsError": 10.0}],
    )

    assert compare_reports(report(), current) == []


def test_status_worsening_fails():
    failures = compare_reports(report(cases=[{"status": "ok"}]), report(cases=[{"status": "missing"}]))

    assert "perCase.case-a.status" in failure_paths(failures)
