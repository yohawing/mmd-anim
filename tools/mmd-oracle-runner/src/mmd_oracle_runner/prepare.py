from __future__ import annotations

import hashlib
import json
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any, Protocol, Sequence

from . import artifacts
from .case import OracleCase
from .pmx import PmxError, stage_mmd_compatible_pmx

_RUST_UNSUPPORTED_CHANNELS = ("cameras", "lights", "selfShadows", "properties")
_WINDOWS_RESERVED = frozenset(("CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))))


@dataclass(frozen=True)
class CommandResult:
    command: tuple[str, ...]
    cwd: Path
    exit_code: int
    stdout: str
    stderr: str


class CommandRunner(Protocol):
    def run(self, command: Sequence[str], cwd: Path) -> CommandResult:
        ...


class SubprocessRunner:
    def __init__(self, timeout_seconds: float | None = 120.0):
        self.timeout_seconds = timeout_seconds

    def run(self, command: Sequence[str], cwd: Path) -> CommandResult:
        command_tuple = tuple(str(part) for part in command)
        try:
            completed = subprocess.run(
                command_tuple, cwd=cwd, capture_output=True, text=True,
                encoding="utf-8", errors="replace", check=False,
                timeout=self.timeout_seconds)
        except subprocess.TimeoutExpired as error:
            return CommandResult(command_tuple, cwd, 124, _text(error.stdout), _text(error.stderr) or f"command timed out after {self.timeout_seconds:g}s")
        except OSError as error:
            return CommandResult(command_tuple, cwd, -1, "", str(error))
        return CommandResult(command_tuple, cwd, completed.returncode, completed.stdout, completed.stderr)


def prepare_case(case: OracleCase, *, runner: CommandRunner | None = None, repo_root: Path | None = None) -> dict[str, Any]:
    """Prepare a PMM and fixture using Python orchestration and Rust formats."""
    runner = runner or SubprocessRunner()
    repo_root = (repo_root or _default_repo_root()).resolve()
    artifact_name = _artifact_name(case.name)
    run_dir = case.output_root / artifact_name
    paths = {
        "project": run_dir / "scene.pmm", "fixture": run_dir / "fixture.json",
        "model": run_dir / "model.mmd-utf16.pmx", "result": run_dir / "prepare-result.json",
    }
    temporary_result = run_dir / ".prepare-result.json.tmp"
    stale = (*paths.values(), temporary_result)
    result = _base_result(case, repo_root, paths, artifact_name)
    try:
        artifacts.reject_reparse(case.output_root, run_dir, *stale)
        run_dir.mkdir(parents=True, exist_ok=True)
    except OSError as error:
        _fail(result, "preflight", f"cannot create artifact directory: {error}")
        return result
    try:
        result["inputInventory"] = _input_inventory(case)
        artifacts.cleanup(result, stale, paths["result"], preserve=tuple(paths.values()))
    except (OSError, ValueError) as error:
        _fail(result, "preflight", f"artifact preflight failed: {error}")
        if not paths["result"].exists():
            artifacts.write_result(paths["result"], temporary_result, result, _fail)
        return result

    try:
        with TemporaryDirectory(prefix=f".{artifact_name}-prepare-", dir=case.output_root) as temporary:
            work_dir = Path(temporary) / artifact_name
            work_dir.mkdir()
            model_path = _stage_pmx(case, result, work_dir / "model.mmd-utf16.pmx", paths["model"])
            if model_path is None:
                ready = False
            elif case.generator_backend == "python-rust" and case.camera_vmd is None:
                ready = _prepare_rust(case, result, runner, repo_root, work_dir / "scene.pmm", model_path)
            elif case.camera_vmd is not None:
                ready = False
                _fail(result, "preflight", "camera VMD preparation is not supported by the Python/Rust backend yet")
            else:
                ready = False
                _fail(result, "preflight", f"unsupported generator backend: {case.generator_backend}")
            if ready:
                work_fixture = work_dir / "fixture.json"
                _write_fixture(case, work_fixture, paths["project"], paths["fixture"].parent / "oracle.actual.jsonl", repo_root)
                (work_dir / "scene.pmm").replace(paths["project"])
                work_fixture.replace(paths["fixture"])
    except OSError as error:
        _fail(result, "artifacts", f"artifact operation failed: {error}")
    except Exception as error:  # noqa: BLE001 - result artifact must capture backend failures
        _fail(result, "generator", f"prepare failed: {error}")
    _check_artifacts(result)
    artifacts.record(result, stale)
    artifacts.write_result(paths["result"], temporary_result, result, _fail)
    return result


def _stage_pmx(case: OracleCase, result: dict[str, Any], staged_path: Path, final_path: Path) -> Path | None:
    try:
        report = stage_mmd_compatible_pmx(case.pmx, staged_path)
    except (OSError, PmxError) as error:
        _fail(result, "generator", f"PMX staging failed: {error}")
        return None
    converted = report.get("converted") is True
    result["classifications"]["pmxStaging"] = "working" if converted else "not-applicable"
    result["backend"].setdefault("preflight", {})["pmxStaging"] = {
        "command": ["python", "pmx.stage_mmd_compatible_pmx"], "cwd": str(case.pmx.parent), "exitCode": 0, "report": report,
    }
    if converted:
        staged_path.replace(final_path)
        return final_path
    return case.pmx


def _prepare_rust(case: OracleCase, result: dict[str, Any], runner: CommandRunner, repo_root: Path, scene_path: Path, model_path: Path) -> bool:
    inspect_command = ("cargo", "run", "-q", "-p", "mmd-anim-cli", "--", "inspect", str(case.body_vmd))
    inspect = runner.run(inspect_command, repo_root)
    _set_backend_diagnostics(result, inspect, "bodyVmd")
    if inspect.exit_code != 0:
        _fail(result, "preflight", f"Rust inspect exited with {inspect.exit_code}")
        return False
    try:
        counts = _rust_counts(inspect.stdout)
    except ValueError as error:
        _fail(result, "preflight", str(error))
        return False
    result["preflight"] = {"metadataCounts": counts}
    unsupported = {name: counts[name] for name in _RUST_UNSUPPORTED_CHANNELS if counts[name] > 0}
    if unsupported:
        _fail(result, "preflight", "Rust PMM backend does not support VMD channels: " + ", ".join(unsupported))
        return False

    build_command = ("cargo", "run", "-q", "-p", "mmd-anim-cli", "--", "build-pmm", str(model_path), str(case.body_vmd), str(scene_path), "--json")
    outcome = runner.run(build_command, repo_root)
    _set_backend_diagnostics(result, outcome)
    if outcome.exit_code != 0:
        _fail(result, "generator", f"Rust PMM backend exited with {outcome.exit_code}")
        return False
    report = _json_stdout(outcome.stdout)
    if not isinstance(report, dict) or report.get("status") != "ok":
        _fail(result, "generator", "Rust PMM backend did not return a successful JSON report")
        return False
    keyframes = report.get("keyframes") if isinstance(report.get("keyframes"), dict) else {}
    skipped = {"boneFrames": _count(keyframes.get("skippedBones")), "morphFrames": _count(keyframes.get("skippedMorphs"))}
    result["skippedCounts"] = skipped
    result["comparison"] = {
        "status": "generated",
        "reason": "PMM contains tracks whose names match the model; unmatched VMD names are reported in skippedCounts",
        "generatorReport": {key: report[key] for key in ("status", "command", "mode", "counts", "keyframes") if key in report},
    }
    result["classifications"]["propertyFrames"] = "not-applicable"
    return True


def _base_result(case: OracleCase, repo_root: Path, paths: dict[str, Path], artifact_name: str) -> dict[str, Any]:
    return {
        "schemaVersion": 1, "ok": False, "phase": "preflight", "recorded": False,
        "caseName": case.name, "artifactName": artifact_name, "caseFile": str(case.source_path), "frames": list(case.frames),
        "generatorBackend": case.generator_backend, "inputInventory": {},
        "artifacts": {name: {"path": str(path), "exists": artifacts.exists(path)} for name, path in paths.items()},
        "comparison": {"status": "not-verified"}, "skippedBoneNames": [], "skippedMorphNames": [],
        "droppedUnsupportedChannels": {},
        "classifications": {"pmxStaging": "not-verified", "propertyFrames": "not-verified", "goldenOracleWrapper": "not-verified", "modelStructureDialog": "not-verified"},
        "backend": {"command": [], "cwd": str(repo_root), "exitCode": None, "stderr": ""}, "errors": [], "ownedArtifacts": [],
    }


def _input_inventory(case: OracleCase) -> dict[str, dict[str, Any]]:
    fields = {"caseFile": case.source_path, "pmx": case.pmx, "bodyVmd": case.body_vmd}
    if case.camera_vmd is not None:
        fields["cameraVmd"] = case.camera_vmd
    inventory = {}
    for field, path in fields.items():
        digest = hashlib.sha256(); size = 0
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                size += len(chunk); digest.update(chunk)
        inventory[field] = {"path": str(path), "size": size, "sha256": digest.hexdigest()}
    return inventory


def _check_artifacts(result: dict[str, Any]) -> None:
    for artifact in result["artifacts"].values():
        artifact["exists"] = artifacts.exists(Path(artifact["path"]))
    if result["errors"]:
        return
    missing = [name for name in ("project", "fixture") if not result["artifacts"][name]["exists"]]
    if missing:
        _fail(result, "artifacts", f"missing required artifact(s): {', '.join(missing)}")
    else:
        result["ok"] = True; result["phase"] = "complete"


def _set_backend_diagnostics(result: dict[str, Any], outcome: CommandResult, label: str | None = None) -> None:
    diagnostics = {"command": list(outcome.command), "cwd": str(outcome.cwd), "exitCode": outcome.exit_code, "stderr": _text(outcome.stderr)[-4096:]}
    if label:
        result["backend"].setdefault("preflight", {})[label] = diagnostics; return
    preflight = result["backend"].get("preflight"); result["backend"] = diagnostics
    if preflight is not None:
        result["backend"]["preflight"] = preflight


def _rust_counts(stdout: str) -> dict[str, int]:
    match = re.search(r"boneFrames=(\d+) morphFrames=(\d+) cameraFrames=(\d+) lightFrames=(\d+) selfShadowFrames=(\d+) propertyFrames=(\d+)", stdout)
    if match is None:
        raise ValueError("Rust inspect did not return compact VMD counts")
    names = ("bones", "morphs", *_RUST_UNSUPPORTED_CHANNELS)
    return dict(zip(names, (int(value) for value in match.groups()), strict=True))


def _json_stdout(stdout: str) -> Any:
    try:
        return json.loads(_text(stdout))
    except json.JSONDecodeError as error:
        raise ValueError(f"backend stdout was not JSON: {error}") from error


def _count(value: Any) -> int:
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else 0


def _artifact_name(name: str) -> str:
    if len(name) <= 64 and name == name.rstrip(". ") and not any(ord(char) < 32 or char in '\\/:*?"<>|' for char in name) and name.split(".", 1)[0].upper() not in _WINDOWS_RESERVED:
        return name
    prefix = "".join(char for char in name if ord(char) >= 32 and char not in '\\/:*?"<>|').rstrip(". ")[:32] or "case"
    return f"{prefix}-{hashlib.sha256(name.encode('utf-8')).hexdigest()[:12]}"


def _write_fixture(case: OracleCase, fixture_path: Path, scene_path: Path, output: Path, repo_root: Path) -> None:
    fixture = {
        "name": case.name, "mmdVersion": "9.32-x64",
        "mmdExe": str(repo_root / "MMDDumper" / "MikuMikuDance_v932x64" / "MikuMikuDance.exe"),
        "project": str(scene_path), "frames": list(case.frames), "output": str(output), "done": str(output) + ".done",
        "timeoutMs": 60000, "dump": {"bones": True, "morphs": True, "camera": case.camera_vmd is not None, "cameraKeyframes": True, "sceneParameters": False, "rigidBodies": False},
    }
    with fixture_path.open("w", encoding="utf-8") as stream:
        stream.write(json.dumps(fixture, ensure_ascii=True, indent=2) + "\n")


def _fail(result: dict[str, Any], phase: str, message: str) -> None:
    result["ok"] = False; result["phase"] = phase; result["errors"].append({"phase": phase, "message": message})


def _text(value: str | bytes | None) -> str:
    return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""


def _default_repo_root() -> Path:
    return Path(__file__).resolve().parents[4]
