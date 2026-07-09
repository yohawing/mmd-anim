from __future__ import annotations

import argparse
import json
from pathlib import Path

import pytest

from golden_gate.config import ConfigError, resolve_config


_GOLDEN_GATE_ENV_VARS = (
    "MMD_ANIM_GOLDEN_CONFIG",
    "MMD_ANIM_GOLDEN_MANIFEST",
    "MMD_ANIM_GOLDEN_BASELINE",
    "MMD_ANIM_GOLDEN_REPORT_DIR",
    "MMD_ANIM_GOLDEN_REPO_ROOT",
    "MMD_ANIM_BIN",
    "MMD_ANIM_GOLDEN_PHYSICS_PENETRATION",
    "MMD_ANIM_GOLDEN_DIAGNOSE_CASE",
    "MMD_ANIM_GOLDEN_DIAGNOSE_FRAME",
    "MMD_ANIM_GOLDEN_DIAGNOSE_BONE",
    "MMD_ANIM_GOLDEN_DIAGNOSE_EVAL_FRAME",
    "MMD_ANIM_GOLDEN_MAX_ABS_ERROR_TOLERANCE",
    "MMD_ANIM_GOLDEN_TRANSLATION_MAX_ERROR_TOLERANCE",
    "MMD_ANIM_GOLDEN_TRANSLATION_RMS_ERROR_TOLERANCE",
    "MMD_ANIM_GOLDEN_ROTATION_MAX_ANGLE_RAD_TOLERANCE",
    "MMD_ANIM_GOLDEN_ROTATION_RMS_ANGLE_RAD_TOLERANCE",
    "MMD_ANIM_GOLDEN_PENETRATION_MAX_DEPTH_TOLERANCE",
    "MMD_ANIM_GOLDEN_BULLET_PENETRATION_MAX_DEPTH_TOLERANCE",
    "MMD_ANIM_GOLDEN_PENETRATING_PAIR_COUNT_TOLERANCE",
    "MMD_ANIM_GOLDEN_SEVERE_PAIR_COUNT_TOLERANCE",
    "MMD_ANIM_GOLDEN_PENETRATING_CONTACT_COUNT_TOLERANCE",
    "MMD_ANIM_GOLDEN_MISMATCH_COUNT_TOLERANCE",
    "MMD_ANIM_GOLDEN_MISSING_TOLERANCE",
    "MMD_ANIM_GOLDEN_IMPORT_ERROR_TOLERANCE",
    "MMD_ANIM_GOLDEN_ALLOW_COUNT_CHANGES",
    "MMD_ANIM_GOLDEN_ALLOW_SKIPPED_TARGET_CHANGES",
    "MMD_ANIM_GOLDEN_REQUIRED_PHYSICS_BACKEND",
)


@pytest.fixture(autouse=True)
def _isolate_config_lookup(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.chdir(tmp_path)
    for name in _GOLDEN_GATE_ENV_VARS:
        monkeypatch.delenv(name, raising=False)


def args(**overrides):
    values = {
        "config": None,
        "repo_root": None,
        "manifest": None,
        "baseline": None,
        "report_dir": None,
        "mmd_anim_bin": None,
        "physics_penetration": None,
        "diagnose_case": None,
        "diagnose_frame": None,
        "diagnose_bone": None,
        "diagnose_eval_frame": None,
        "max_abs_error_tolerance": None,
        "translation_max_error_tolerance": None,
        "translation_rms_error_tolerance": None,
        "rotation_max_angle_rad_tolerance": None,
        "rotation_rms_angle_rad_tolerance": None,
        "penetration_max_depth_tolerance": None,
        "bullet_penetration_max_depth_tolerance": None,
        "penetrating_pair_count_tolerance": None,
        "severe_pair_count_tolerance": None,
        "penetrating_contact_count_tolerance": None,
        "mismatch_count_tolerance": None,
        "missing_tolerance": None,
        "import_error_tolerance": None,
        "allow_count_changes": None,
        "allow_skipped_target_changes": None,
        "required_physics_backend": None,
    }
    values.update(overrides)
    return argparse.Namespace(**values)


def test_config_reads_local_json_relative_paths(tmp_path: Path):
    config_path = tmp_path / "golden-gate.local.json"
    config_path.write_text(
        json.dumps(
            {
                "manifest": "manifest.json",
                "baseline": "reports/baseline.json",
                "report_dir": "reports",
                "physics_penetration": True,
                "diagnose_case": "rem-tail",
                "diagnose_frame": 119,
                "diagnose_bone": "左Tail_19",
                "diagnose_eval_frame": "119.5",
                "tolerances": {
                    "max_abs_error_tolerance": 0.25,
                    "translation_max_error_tolerance": 0.5,
                    "translation_rms_error_tolerance": 0.05,
                    "rotation_max_angle_rad_tolerance": 0.4,
                    "rotation_rms_angle_rad_tolerance": 0.04,
                    "penetration_max_depth_tolerance": 0.03,
                    "bullet_penetration_max_depth_tolerance": 0.02,
                    "penetrating_pair_count_tolerance": 3,
                    "severe_pair_count_tolerance": 1,
                    "penetrating_contact_count_tolerance": 2,
                    "mismatch_count_tolerance": 2,
                },
                "options": {
                    "allow_count_changes": True,
                    "required_physics_backend": "bullet-native",
                },
            },
            indent=2,
        ),
        encoding="utf-8",
    )

    config = resolve_config(args(config=str(config_path)))

    assert config.manifest == (tmp_path / "manifest.json").resolve()
    assert config.baseline == (tmp_path / "reports" / "baseline.json").resolve()
    assert config.report_dir == (tmp_path / "reports").resolve()
    assert config.physics_penetration is True
    assert config.diagnose_case == "rem-tail"
    assert config.diagnose_frame == "119"
    assert config.diagnose_bone == "左Tail_19"
    assert config.diagnose_eval_frame == "119.5"
    assert config.tolerances.max_abs_error_tolerance == 0.25
    assert config.tolerances.translation_max_error_tolerance == 0.5
    assert config.tolerances.translation_rms_error_tolerance == 0.05
    assert config.tolerances.rotation_max_angle_rad_tolerance == 0.4
    assert config.tolerances.rotation_rms_angle_rad_tolerance == 0.04
    assert config.tolerances.penetration_max_depth_tolerance == 0.03
    assert config.tolerances.bullet_penetration_max_depth_tolerance == 0.02
    assert config.tolerances.penetrating_pair_count_tolerance == 3
    assert config.tolerances.severe_pair_count_tolerance == 1
    assert config.tolerances.penetrating_contact_count_tolerance == 2
    assert config.tolerances.mismatch_count_tolerance == 2
    assert config.options.allow_count_changes is True
    assert config.options.required_physics_backend == "bullet-native"


def test_cli_overrides_env_and_config(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    config_path = tmp_path / "golden-gate.local.json"
    config_path.write_text(
        json.dumps({"manifest": "from-config.json", "baseline": "baseline.json"}),
        encoding="utf-8",
    )
    monkeypatch.setenv("MMD_ANIM_GOLDEN_MANIFEST", str(tmp_path / "from-env.json"))

    config = resolve_config(args(config=str(config_path), manifest=str(tmp_path / "from-cli.json")))

    assert config.manifest == tmp_path / "from-cli.json"


def test_env_overrides_config(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    config_path = tmp_path / "golden-gate.local.json"
    config_path.write_text(
        json.dumps({"manifest": "from-config.json", "baseline": "baseline.json"}),
        encoding="utf-8",
    )
    monkeypatch.setenv("MMD_ANIM_GOLDEN_MANIFEST", str(tmp_path / "from-env.json"))

    config = resolve_config(args(config=str(config_path)))

    assert config.manifest == tmp_path / "from-env.json"


def test_empty_backend_env_disables_required_physics_backend(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("MMD_ANIM_GOLDEN_REQUIRED_PHYSICS_BACKEND", "")

    config = resolve_config(args())

    assert config.options.required_physics_backend is None


def test_missing_required_paths_are_reported():
    with pytest.raises(ConfigError, match="manifest"):
        resolve_config(args()).require_paths()


def test_physics_penetration_requires_diagnose_case_and_frame(tmp_path: Path):
    manifest = tmp_path / "manifest.json"
    baseline = tmp_path / "baseline.json"

    with pytest.raises(ConfigError, match="diagnose_case"):
        resolve_config(
            args(manifest=str(manifest), baseline=str(baseline), physics_penetration=True)
        ).require_paths()


def test_diagnose_values_require_physics_penetration(tmp_path: Path):
    manifest = tmp_path / "manifest.json"
    baseline = tmp_path / "baseline.json"

    with pytest.raises(ConfigError, match="physics_penetration"):
        resolve_config(
            args(manifest=str(manifest), baseline=str(baseline), diagnose_case="case-a", diagnose_frame="60")
        ).require_paths()


def test_negative_tolerance_is_rejected():
    with pytest.raises(ConfigError, match="non-negative"):
        resolve_config(args(max_abs_error_tolerance=-0.1))
