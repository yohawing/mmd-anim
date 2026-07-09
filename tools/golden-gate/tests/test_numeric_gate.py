from __future__ import annotations

import argparse
import os
from dataclasses import replace
from pathlib import Path

import pytest

from golden_gate.compare import compare_reports
from golden_gate.config import ConfigError, GateOptions, Tolerances, resolve_config
from golden_gate.report import save_report
from golden_gate.runner import generate_current_report


def test_physics_numeric_metrics_within_thresholds_pass():
    baseline = _physics_report()
    current = _physics_report(
        {
            "translationRms": 0.009,
            "translationMax": 0.019,
            "rotationRmsRad": 0.0009,
            "rotationMaxRad": 0.0019,
        }
    )

    assert compare_reports(baseline, current, _physics_tolerances()) == []


def test_physics_numeric_summary_accepts_motion_metric_aliases():
    baseline = _physics_report()
    current = _physics_report(
        {
            "motionTranslationRmsError": 0.009,
            "motionTranslationMaxError": 0.019,
            "motionRotationRmsAngleRad": 0.0009,
            "motionRotationMaxAngleRad": 0.0019,
        }
    )
    for legacy_field in ("translationRms", "translationMax", "rotationRmsRad", "rotationMaxRad"):
        del current["summary"][legacy_field]

    assert compare_reports(baseline, current, _physics_tolerances()) == []


def test_physics_numeric_per_case_accepts_motion_metric_aliases():
    baseline = _physics_report()
    current = _physics_report(
        cases=[
            {
                "motionTranslationRmsError": 0.009,
                "motionTranslationMaxError": 0.019,
                "motionRotationRmsAngleRad": 0.0009,
                "motionRotationMaxAngleRad": 0.0019,
            }
        ]
    )
    for legacy_field in ("translationRms", "translationMax", "rotationRmsRad", "rotationMaxRad"):
        del current["perCase"][0][legacy_field]

    assert compare_reports(baseline, current, _physics_tolerances()) == []


def test_physics_numeric_per_case_translation_rms_over_threshold_fails():
    baseline = _physics_report()
    current = _physics_report({"translationRms": 0.009}, [{"translationRms": 0.011}])

    failures = compare_reports(baseline, current, _physics_tolerances())

    assert "perCase.physics-a.translationRmsError" in _failure_paths(failures)
    assert "summary.translationRmsError" not in _failure_paths(failures)


def test_physics_numeric_case_requires_bullet_native_backend():
    baseline = _physics_report()
    current = _physics_report({"physicsBackend": "rapier"}, [{"physicsBackend": "rapier"}])

    failures = compare_reports(
        baseline,
        current,
        _physics_tolerances(),
        GateOptions(required_physics_backend="bullet-native"),
    )

    assert "perCase.physics-a.physicsBackend" in _failure_paths(failures)


def test_added_physics_case_requires_backend_even_when_count_changes_are_allowed():
    baseline = _physics_report()
    baseline["perCase"] = []
    baseline["summary"]["cases"] = 0
    baseline["summary"]["comparedCases"] = 0
    current = _physics_report({"physicsBackend": "rapier"}, [{"physicsBackend": "rapier"}])

    failures = compare_reports(
        baseline,
        current,
        _physics_tolerances(),
        GateOptions(allow_count_changes=True, required_physics_backend="bullet-native"),
    )

    assert "perCase.physics-a.physicsBackend" in _failure_paths(failures)


def test_physics_numeric_missing_current_metrics_fail():
    baseline = _physics_report()
    current = _physics_report()
    del current["summary"]["translationRms"]
    del current["perCase"][0]["translationRms"]

    failures = compare_reports(baseline, current, _physics_tolerances())

    assert "summary.translationRmsError" in _failure_paths(failures)
    assert "perCase.physics-a.translationRmsError" in _failure_paths(failures)


def test_required_physics_backend_is_opt_in():
    baseline = _physics_report()
    current = _physics_report({"physicsBackend": "none"}, [{"physicsBackend": "none"}])

    assert compare_reports(baseline, current, _physics_tolerances()) == []


def test_physics_penetration_report_within_thresholds_pass():
    baseline = _physics_penetration_report()
    current = _physics_penetration_report(
        {
            "maxPenetrationDepth": 0.051,
            "maxBulletPenetrationDepth": 0.021,
            "penetratingPairCount": 2,
            "severePairCount": 1,
            "penetratingContactCount": 1,
        }
    )

    assert compare_reports(baseline, current, _penetration_tolerances()) == []


def test_physics_penetration_depth_regression_fails():
    baseline = _physics_penetration_report()
    current = _physics_penetration_report({"maxPenetrationDepth": 0.053})

    failures = compare_reports(baseline, current, _penetration_tolerances())

    assert "summary.maxPenetrationDepth" in _failure_paths(failures)


def test_physics_penetration_contact_regression_fails():
    baseline = _physics_penetration_report()
    current = _physics_penetration_report(
        {
            "maxBulletPenetrationDepth": 0.024,
            "penetratingContactCount": 3,
        }
    )

    failures = compare_reports(baseline, current, _penetration_tolerances())

    assert "summary.maxBulletPenetrationDepth" in _failure_paths(failures)
    assert "summary.penetratingContactCount" in _failure_paths(failures)


def test_physics_penetration_report_rejects_mismatched_identity():
    baseline = _physics_penetration_report()
    current = _physics_penetration_report()
    current["caseName"] = "sour-necktie"
    current["oracleFrame"] = 60.0

    failures = compare_reports(baseline, current, _penetration_tolerances())

    assert "caseName" in _failure_paths(failures)
    assert "oracleFrame" in _failure_paths(failures)


def test_physics_penetration_absolute_limits_fail_independent_of_baseline():
    baseline = _physics_penetration_report(
        {
            "maxBulletPenetrationDepth": 0.2,
            "penetratingContactCount": 4,
        }
    )
    current = _physics_penetration_report(
        {
            "maxBulletPenetrationDepth": 0.1,
            "penetratingContactCount": 2,
        }
    )

    failures = compare_reports(
        baseline,
        current,
        _penetration_tolerances(),
        GateOptions(
            max_allowed_bullet_penetration_depth=0.0,
            max_allowed_penetrating_contact_count=0,
        ),
    )

    assert "summary.maxBulletPenetrationDepth" in _failure_paths(failures)
    assert "summary.penetratingContactCount" in _failure_paths(failures)


def test_physics_penetration_contact_zero_limit_can_ignore_shape_proxy_overlap():
    shape_proxy_overlap = {
        "maxPenetrationDepth": 0.74,
        "penetratingPairCount": 2,
        "severePairCount": 2,
        "maxBulletPenetrationDepth": 0.0,
        "penetratingContactCount": 0,
    }
    baseline = _physics_penetration_report(shape_proxy_overlap)
    current = _physics_penetration_report(shape_proxy_overlap)

    failures = compare_reports(
        baseline,
        current,
        _penetration_tolerances(),
        GateOptions(
            max_allowed_bullet_penetration_depth=0.0,
            max_allowed_penetrating_contact_count=0,
        ),
    )

    assert failures == []


def test_physics_penetration_report_requires_stable_summary_metrics():
    baseline = _physics_penetration_report()
    current = _physics_penetration_report()
    del current["summary"]["severePairCount"]

    failures = compare_reports(baseline, current, _penetration_tolerances())

    assert "summary.severePairCount" in _failure_paths(failures)


def test_numeric_gate_roundtrip_with_local_assets(tmp_path: Path):
    config = _local_config_or_skip()
    config = replace(config, baseline=tmp_path / "baseline.json", report_dir=tmp_path / "reports")

    baseline_report, baseline_report_path = generate_current_report(config)
    save_report(config.baseline, baseline_report)
    current_report, current_report_path = generate_current_report(config)

    assert baseline_report_path.exists()
    assert current_report_path.exists()
    assert config.baseline.exists()
    assert compare_reports(baseline_report, current_report, config.tolerances, config.options) == []


def _physics_report(summary=None, cases=None):
    case_values = (cases or [{}])[0]
    return {
        "summary": {
            "cases": 1,
            "comparedCases": 1,
            "missing": 0,
            "importErrors": 0,
            "comparedFrames": 10,
            "comparedBones": 20,
            "mismatchCount": 0,
            "maxAbsError": 0.0,
            "translationRms": 0.0,
            "translationMax": 0.0,
            "rotationRmsRad": 0.0,
            "rotationMaxRad": 0.0,
            "skippedTargets": [],
            **(summary or {}),
        },
        "perCase": [
            {
                "name": "physics-a",
                "kind": "physics-numeric",
                "status": "ok",
                "physicsBackend": "bullet-native",
                "comparedFrames": 10,
                "comparedBones": 20,
                "mismatchCount": 0,
                "maxAbsError": 0.0,
                "translationRms": 0.0,
                "translationMax": 0.0,
                "rotationRmsRad": 0.0,
                "rotationMaxRad": 0.0,
                "skippedTargets": [],
                **case_values,
            }
        ],
    }


def _physics_tolerances():
    return Tolerances(
        translation_rms_error_tolerance=0.01,
        translation_max_error_tolerance=0.02,
        rotation_rms_angle_rad_tolerance=0.001,
        rotation_max_angle_rad_tolerance=0.002,
    )


def _physics_penetration_report(summary=None):
    return {
        "caseName": "rem-tail-left",
        "oracleFrame": 119.0,
        "evalFrame": 119.0,
        "model": "rem.pmx",
        "motion": "motion.vmd",
        "metric": "shape-proxy+bullet-contacts",
        "summary": {
            "pairCount": 30,
            "penetratingPairCount": 1,
            "severePairCount": 0,
            "bulletContactCount": 2,
            "penetratingContactCount": 0,
            "minSignedDistance": -0.02,
            "maxPenetrationDepth": 0.05,
            "minBulletContactDistance": 0.01,
            "maxBulletPenetrationDepth": 0.02,
            **(summary or {}),
        },
        "pairs": [],
        "contacts": [],
    }


def _penetration_tolerances():
    return Tolerances(
        penetration_max_depth_tolerance=0.002,
        bullet_penetration_max_depth_tolerance=0.001,
        penetrating_pair_count_tolerance=1,
        severe_pair_count_tolerance=1,
        penetrating_contact_count_tolerance=1,
    )


def _failure_paths(failures):
    return {failure.path for failure in failures}


def _local_config_or_skip():
    namespace = argparse.Namespace(
        config=os.environ.get("MMD_ANIM_GOLDEN_CONFIG"),
        repo_root=None,
        manifest=None,
        baseline=None,
        report_dir=None,
        mmd_anim_bin=None,
        max_abs_error_tolerance=None,
        mismatch_count_tolerance=None,
        missing_tolerance=None,
        import_error_tolerance=None,
        allow_count_changes=None,
        allow_skipped_target_changes=None,
    )
    try:
        config = resolve_config(namespace).require_paths()
    except ConfigError as error:
        pytest.skip(f"local GoldenOracle config is unavailable: {error}")
    if not config.manifest.exists():
        pytest.skip(f"local GoldenOracle manifest is unavailable: {config.manifest}")
    if not config.baseline.exists():
        pytest.skip(f"local GoldenOracle baseline is unavailable: {config.baseline}")
    if not config.repo_root.exists():
        pytest.skip(f"mmd-anim repo root is unavailable: {config.repo_root}")
    return config
