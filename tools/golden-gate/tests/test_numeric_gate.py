from __future__ import annotations

import argparse
import os
from dataclasses import replace
from pathlib import Path

import pytest

from golden_gate.compare import compare_reports
from golden_gate.config import ConfigError, resolve_config
from golden_gate.report import save_report
from golden_gate.runner import generate_current_report


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
