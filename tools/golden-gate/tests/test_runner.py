from __future__ import annotations

from pathlib import Path

from golden_gate.config import GateOptions, GoldenGateConfig, Tolerances
from golden_gate.runner import _command, _report_stem


def test_numeric_command_uses_manifest_json_report():
    config = _config()

    assert _command(config) == [
        "cargo",
        "run",
        "-q",
        "-p",
        "mmd-anim-cli",
        "--",
        "verify",
        "manifest.json",
        "--mode",
        "numeric",
        "--json",
    ]
    assert _report_stem(config) == "compare-numeric-report"


def test_physics_penetration_command_uses_diagnose_json_report():
    config = _config(
        mmd_anim_bin=Path("mmd-anim.exe"),
        physics_penetration=True,
        diagnose_case="rem-tail",
        diagnose_frame="119",
        diagnose_bone="左Tail_19",
        diagnose_eval_frame="119.5",
    )

    assert _command(config) == [
        "mmd-anim.exe",
        "verify",
        "manifest.json",
        "--mode",
        "numeric",
        "--json",
        "--diagnose",
        "rem-tail",
        "119",
        "左Tail_19",
        "--physics-penetration",
        "--eval-frame",
        "119.5",
    ]
    assert _report_stem(config) == "physics-penetration-report"


def _config(**overrides):
    values = {
        "repo_root": Path("."),
        "manifest": Path("manifest.json"),
        "baseline": Path("baseline.json"),
        "report_dir": Path("reports"),
        "mmd_anim_bin": None,
        "physics_penetration": False,
        "diagnose_case": None,
        "diagnose_frame": None,
        "diagnose_bone": None,
        "diagnose_eval_frame": None,
        "tolerances": Tolerances(),
        "options": GateOptions(),
    }
    values.update(overrides)
    return GoldenGateConfig(**values)
