from __future__ import annotations

import json
import subprocess
from datetime import datetime
from pathlib import Path
from typing import Any

from .config import GoldenGateConfig
from .report import save_report


class RunnerError(RuntimeError):
    pass


def generate_current_report(config: GoldenGateConfig) -> tuple[dict[str, Any], Path]:
    config = config.require_paths()
    report = run_numeric_verify(config)
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    report_path = config.report_dir / f"{_report_stem(config)}-{timestamp}.json"
    save_report(report_path, report)
    return report, report_path


def run_numeric_verify(config: GoldenGateConfig) -> dict[str, Any]:
    config = config.require_paths()
    if config.manifest is None:
        raise RunnerError("manifest is required")
    command = _command(config)
    completed = subprocess.run(
        command,
        cwd=config.repo_root,
        capture_output=True,
        text=True,
        encoding="utf-8",
        errors="strict",
        check=False,
    )
    if completed.returncode != 0:
        raise RunnerError(
            "numeric verify failed with exit code "
            f"{completed.returncode}\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
        )
    stdout = completed.stdout.strip()
    if not stdout:
        raise RunnerError("numeric verify produced empty stdout")
    try:
        value = json.loads(stdout)
    except json.JSONDecodeError as error:
        raise RunnerError(f"numeric verify did not emit valid JSON: {error}\nstdout:\n{stdout}") from error
    if not isinstance(value, dict):
        raise RunnerError("numeric verify JSON report must be an object")
    return value


def _command(config: GoldenGateConfig) -> list[str]:
    if config.mmd_anim_bin is not None:
        command = [
            str(config.mmd_anim_bin),
            "verify",
            str(config.manifest),
            "--mode",
            "numeric",
            "--json",
        ]
    else:
        command = [
            "cargo",
            "run",
            "-q",
            "-p",
            "mmd-anim-cli",
            "--",
            "verify",
            str(config.manifest),
            "--mode",
            "numeric",
            "--json",
        ]
    if config.physics_penetration:
        if config.diagnose_case is None or config.diagnose_frame is None:
            raise RunnerError("physics penetration report requires diagnose_case and diagnose_frame")
        command.extend(["--diagnose", config.diagnose_case, config.diagnose_frame])
        if config.diagnose_bone is not None:
            command.append(config.diagnose_bone)
        command.append("--physics-penetration")
        if config.diagnose_eval_frame is not None:
            command.extend(["--eval-frame", config.diagnose_eval_frame])
    return command


def _report_stem(config: GoldenGateConfig) -> str:
    if config.physics_penetration:
        return "physics-penetration-report"
    return "compare-numeric-report"
