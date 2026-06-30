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
    "MMD_ANIM_GOLDEN_MAX_ABS_ERROR_TOLERANCE",
    "MMD_ANIM_GOLDEN_MISMATCH_COUNT_TOLERANCE",
    "MMD_ANIM_GOLDEN_MISSING_TOLERANCE",
    "MMD_ANIM_GOLDEN_IMPORT_ERROR_TOLERANCE",
    "MMD_ANIM_GOLDEN_ALLOW_COUNT_CHANGES",
    "MMD_ANIM_GOLDEN_ALLOW_SKIPPED_TARGET_CHANGES",
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
        "max_abs_error_tolerance": None,
        "mismatch_count_tolerance": None,
        "missing_tolerance": None,
        "import_error_tolerance": None,
        "allow_count_changes": None,
        "allow_skipped_target_changes": None,
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
                "tolerances": {
                    "max_abs_error_tolerance": 0.25,
                    "mismatch_count_tolerance": 2,
                },
                "options": {
                    "allow_count_changes": True,
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
    assert config.tolerances.max_abs_error_tolerance == 0.25
    assert config.tolerances.mismatch_count_tolerance == 2
    assert config.options.allow_count_changes is True


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


def test_missing_required_paths_are_reported():
    with pytest.raises(ConfigError, match="manifest"):
        resolve_config(args()).require_paths()


def test_negative_tolerance_is_rejected():
    with pytest.raises(ConfigError, match="non-negative"):
        resolve_config(args(max_abs_error_tolerance=-0.1))
