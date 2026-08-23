from __future__ import annotations

import hashlib
import json
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any, Protocol, Sequence

from .case import OracleCase
from . import artifacts

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
            detail = f"command timed out after {self.timeout_seconds:g}s"
            return CommandResult(command_tuple, cwd, 124, _text(error.stdout), _text(error.stderr) or detail)
        except OSError as error:
            return CommandResult(command_tuple, cwd, -1, "", str(error))
        return CommandResult(command_tuple, cwd, completed.returncode, completed.stdout, completed.stderr)


def prepare_case(case: OracleCase, *, runner: CommandRunner | None = None, repo_root: Path | None = None) -> dict[str, Any]:
    runner = runner or SubprocessRunner()
    repo_root = (repo_root or _default_repo_root()).resolve()
    artifact_name = _artifact_name(case.name)
    run_dir = case.output_root / artifact_name
    paths = {"project": run_dir / "scene.pmm", "fixture": run_dir / "fixture.json", "model": run_dir / "model.mmd-utf16.pmx", "result": run_dir / "prepare-result.json"}
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
            work_root = Path(temporary)
            work_dir = work_root / artifact_name
            work_dir.mkdir()
            work_project, work_fixture = work_dir / "scene.pmm", work_dir / "fixture.json"
            model_path = _stage_pmx(case, result, runner, repo_root, work_dir / "model.mmd-utf16.pmx", paths["model"])
            if model_path is None:
                ready = False
            elif case.generator_backend == "node-mmddumper":
                ready = _prepare_node(case, result, runner, repo_root, work_root, work_dir, artifact_name, model_path)
            elif case.generator_backend == "rust-build-pmm":
                ready = _prepare_rust(case, result, runner, repo_root, work_project, model_path)
            else:
                ready = False
                _fail(result, "preflight", f"unsupported generator backend: {case.generator_backend}")
            if ready:
                _write_fixture(case, work_fixture, paths["project"], paths["fixture"].parent / "oracle.actual.jsonl", repo_root)
                work_project.replace(paths["project"])
                work_fixture.replace(paths["fixture"])
    except OSError as error:
        _fail(result, "artifacts", f"artifact operation failed: {error}")
    except Exception as error:  # noqa: BLE001 - result artifact must capture backend failures
        _fail(result, "generator", f"prepare failed: {error}")
    _check_artifacts(result)
    artifacts.record(result, stale)
    artifacts.write_result(paths["result"], temporary_result, result, _fail)
    return result


def _prepare_node(case: OracleCase, result: dict[str, Any], runner: CommandRunner, repo_root: Path, work_root: Path, work_dir: Path, artifact_name: str, model_path: Path) -> bool:
    node_root = repo_root / "tools" / "mmd-dumper"
    manifest_path = work_dir / ".node-oracle-manifest.json"
    with manifest_path.open("x", encoding="utf-8") as stream:
        stream.write(json.dumps(_node_manifest(case, artifact_name, model_path), ensure_ascii=True, indent=2) + "\n")
    command = ("node", str(node_root / "src" / "cli.mjs"), "oracle-batch", "--manifest", str(manifest_path), "--out-dir", str(work_root), "--dry-run", "true")
    outcome = runner.run(command, node_root)
    _set_backend_diagnostics(result, outcome)
    if outcome.exit_code != 0:
        _fail(result, "generator", f"node backend exited with {outcome.exit_code}")
        return False
    report = _json_stdout(outcome.stdout)
    cases = report.get("results") if isinstance(report, dict) else None
    node_case = next((item for item in cases if isinstance(item, dict) and item.get("name") == artifact_name), None) if isinstance(cases, list) else None
    if not isinstance(node_case, dict):
        _fail(result, "generator", "node backend result did not contain the requested case")
        return False
    if node_case.get("ok") is not True:
        _fail(result, "generator", "node backend reported a failed case")
        return False
    source = node_case.get("sourceCounts")
    try:
        body = _node_counts(source.get("bodyVmd") if isinstance(source, dict) else None)
        camera = _node_counts(source.get("cameraVmd")) if case.camera_vmd is not None and isinstance(source, dict) else None
    except ValueError as error:
        _fail(result, "preflight", str(error))
        return False
    result["preflight"] = {"bodyVmd": body, **({"cameraVmd": camera} if camera is not None else {})}
    if body["propertyFrames"]:
        result["droppedUnsupportedChannels"] = {"propertyFrames": body["propertyFrames"]}
        result["classifications"]["propertyFrames"] = "reproduced"
    unsupported = {name: body[name] for name in ("cameraFrames", "lightFrames", "selfShadowFrames") if body[name]}
    if body["propertyFrames"] and "property-ik" not in case.requested_features:
        unsupported["propertyFrames"] = body["propertyFrames"]
    if camera is not None:
        unsupported.update({f"cameraVmd.{name}": count for name, count in camera.items() if name != "cameraFrames" and count})
    if unsupported:
        _fail(result, "preflight", "node backend unsupported VMD channels: " + ", ".join(unsupported))
        return False
    filter_report = node_case.get("filter") if isinstance(node_case.get("filter"), dict) else {}
    result["skippedBoneNames"] = filter_report.get("skippedBoneNames") if isinstance(filter_report.get("skippedBoneNames"), list) else []
    result["skippedMorphNames"] = filter_report.get("skippedMorphNames") if isinstance(filter_report.get("skippedMorphNames"), list) else []
    dropped = filter_report.get("droppedUnsupportedChannels")
    result["droppedUnsupportedChannels"] = dropped if isinstance(dropped, dict) else {"propertyFrames": 0}
    property_count = _count(result["droppedUnsupportedChannels"].get("propertyFrames"))
    result["classifications"] = _node_classifications(property_count, result["classifications"]["pmxStaging"])
    patch = node_case.get("patch") if isinstance(node_case.get("patch"), dict) else {}
    counts = patch.get("counts") if isinstance(patch.get("counts"), dict) else None
    if counts is None and body["boneFrames"] == body["morphFrames"] == 0:
        result["comparison"] = {"status": "not-applicable", "reason": "node backend emitted no patch counts for an empty body VMD"}
    elif counts is None:
        _fail(result, "artifacts", "node backend result did not contain patch counts")
    else:
        mismatches = counts.get("mismatches")
        skipped = bool(result["skippedBoneNames"] or result["skippedMorphNames"])
        comparison_ok = patch.get("ok") is True and type(mismatches) is int and mismatches == 0 and not skipped
        result["comparison"] = {"status": "verified" if comparison_ok else "failed", "patchCounts": counts, "mismatches": mismatches}
        if skipped:
            result["comparison"]["reason"] = "node backend skipped VMD tracks absent from the PMX"
            _fail(result, "artifacts", "node backend skipped VMD tracks absent from the PMX")
        elif not comparison_ok:
            _fail(result, "artifacts", "node PMM/VMD keyframe comparison failed")
    if case.camera_vmd is not None and result["comparison"]["status"] in ("verified", "not-applicable"):
        result["comparison"].update(status="partial", camera={"status": "not-verified", "reason": "camera PMM/VMD keyframes were not compared"})
    if property_count != body["propertyFrames"]:
        _fail(result, "artifacts", "node property-frame drop count did not match preflight")
    return not result["errors"]


def _stage_pmx(case: OracleCase, result: dict[str, Any], runner: CommandRunner, repo_root: Path, staged_path: Path, final_path: Path) -> Path | None:
    node_root = repo_root / "tools" / "mmd-dumper"
    outcome = runner.run(("node", str(node_root / "src" / "cli.mjs"), "stage-pmx", "--input", str(case.pmx), "--output", str(staged_path)), node_root)
    _set_backend_diagnostics(result, outcome, "pmxStaging")
    if outcome.exit_code != 0:
        _fail(result, "generator", f"PMX staging exited with {outcome.exit_code}")
        return None
    try:
        report = _stage_report(outcome.stdout, case.pmx, staged_path)
    except ValueError as error:
        _fail(result, "generator", str(error))
        return None
    result["classifications"]["pmxStaging"] = "working" if report["converted"] else "not-applicable"
    if report["converted"]:
        staged_path.replace(final_path)
        return final_path
    return case.pmx


def _prepare_rust(case: OracleCase, result: dict[str, Any], runner: CommandRunner, repo_root: Path, scene_path: Path, model_path: Path) -> bool:
    inspect_command = ("cargo", "run", "-q", "-p", "mmd-anim-cli", "--", "inspect", str(case.body_vmd))
    inspect = runner.run(inspect_command, repo_root)
    _set_backend_diagnostics(result, inspect, "bodyVmd")
    if inspect.exit_code != 0:
        _fail(result, "preflight", f"rust inspect exited with {inspect.exit_code}")
        return False
    try:
        counts = _rust_counts(inspect.stdout)
    except ValueError as error:
        _fail(result, "preflight", str(error))
        return False
    result["preflight"] = {"metadataCounts": counts}
    unsupported = {name: counts[name] for name in _RUST_UNSUPPORTED_CHANNELS if counts[name] > 0}
    if unsupported:
        result["comparison"] = {"status": "not-verified", "reason": "rust build-pmm does not support these VMD channels", "unsupportedCounts": unsupported}
        _fail(result, "preflight", "rust backend capability unsupported for VMD channels: " + ", ".join(unsupported))
        return False

    build_command = ("cargo", "run", "-q", "-p", "mmd-anim-cli", "--", "build-pmm", str(model_path), str(case.body_vmd), str(scene_path), "--json")
    outcome = runner.run(build_command, repo_root)
    _set_backend_diagnostics(result, outcome)
    if outcome.exit_code != 0:
        _fail(result, "generator", f"rust backend exited with {outcome.exit_code}")
        return False
    report = _json_stdout(outcome.stdout)
    if not isinstance(report, dict) or report.get("status") != "ok":
        _fail(result, "generator", "rust backend did not return a successful JSON report")
        return False
    result["comparison"] = {
        "status": "not-verified",
        "reason": "rust build report does not compare PMM keyframes with VMD",
        "generatorReport": {key: report[key] for key in ("status", "command", "mode", "counts", "keyframes") if key in report},
    }
    keyframes = report.get("keyframes")
    if isinstance(keyframes, dict):
        skipped = {"boneFrames": _count(keyframes.get("skippedBones")), "morphFrames": _count(keyframes.get("skippedMorphs"))}
        result["skippedCounts"] = skipped
        if any(skipped.values()):
            result["comparison"] = {"status": "not-verified", "reason": "rust build-pmm skipped frames without names", "generatorReport": result["comparison"]["generatorReport"]}
    reason = "rust build-pmm skipped frames without names" if isinstance(keyframes, dict) and any(result.get("skippedCounts", {}).values()) else "rust build-pmm comparison is not verified"
    _fail(result, "artifacts", reason)
    return True


def _node_manifest(case: OracleCase, artifact_name: str, model_path: Path) -> dict[str, Any]:
    assets = {"model": str(model_path), "motion": str(case.body_vmd)}
    if case.camera_vmd is not None:
        assets["cameraMotion"] = str(case.camera_vmd)
    return {
        "schemaVersion": 1, "kind": "mmd-oracle-runner-prepare", "backend": "mmd-native",
        "defaults": {"dump": {"bones": True, "morphs": True, "camera": case.camera_vmd is not None, "cameraKeyframes": True, "sceneParameters": False, "rigidBodies": False}},
        "cases": [{"name": artifact_name, "assets": assets, "frames": list(case.frames)}],
    }


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
        digest = hashlib.sha256()
        size = 0
        with path.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                size += len(chunk)
                digest.update(chunk)
        inventory[field] = {"path": str(path), "size": size, "sha256": digest.hexdigest()}
    return inventory


def _check_artifacts(result: dict[str, Any]) -> None:
    produced = result["artifacts"]
    for artifact in produced.values():
        artifact["exists"] = artifacts.exists(Path(artifact["path"]))
    if result["errors"]:
        return
    missing = [name for name in ("project", "fixture") if not produced[name]["exists"]]
    if missing:
        _fail(result, "artifacts", f"missing required artifact(s): {', '.join(missing)}")
    else:
        result["ok"] = True
        result["phase"] = "complete"


def _set_backend_diagnostics(result: dict[str, Any], outcome: CommandResult, label: str | None = None) -> None:
    diagnostics = {"command": list(outcome.command), "cwd": str(outcome.cwd), "exitCode": outcome.exit_code, "stderr": (outcome.stderr or "")[-4096:]}
    if label:
        result["backend"].setdefault("preflight", {})[label] = diagnostics
        return
    preflight = result["backend"].get("preflight")
    result["backend"] = diagnostics
    if preflight is not None:
        result["backend"]["preflight"] = preflight


def _node_classifications(property_count: int, staging: str) -> dict[str, str]:
    return {
        "pmxStaging": staging,
        "propertyFrames": "reproduced" if property_count else "not-applicable",
        "goldenOracleWrapper": "not-verified", "modelStructureDialog": "not-verified",
    }


def _rust_counts(stdout: str) -> dict[str, int]:
    match = re.search(r"boneFrames=(\d+) morphFrames=(\d+) cameraFrames=(\d+) lightFrames=(\d+) selfShadowFrames=(\d+) propertyFrames=(\d+)", stdout)
    if match is None:
        raise ValueError("rust inspect did not return compact VMD counts")
    names = ("bones", "morphs", *_RUST_UNSUPPORTED_CHANNELS)
    return dict(zip(names, (int(value) for value in match.groups()), strict=True))


def _node_counts(counts: Any) -> dict[str, int]:
    names = ("boneFrames", "morphFrames", "cameraFrames", "lightFrames", "selfShadowFrames", "propertyFrames")
    if not isinstance(counts, dict) or any(type(counts.get(name)) is not int or counts[name] < 0 for name in names):
        raise ValueError("node backend did not return valid source channel counts")
    return {name: counts[name] for name in names}


def _json_stdout(stdout: str) -> Any:
    try:
        return json.loads(stdout)
    except json.JSONDecodeError as error:
        raise ValueError(f"backend stdout was not JSON: {error}") from error


def _stage_report(stdout: str, source: Path, staged: Path) -> dict[str, Any]:
    report = _json_stdout(stdout)
    if not isinstance(report, dict):
        raise ValueError("PMX staging did not return a JSON object")
    converted, output = report.get("converted"), report.get("output")
    expected = staged if converted is True else source if converted is False else None
    if report.get("ok") is not True or not isinstance(output, str) or expected is None or Path(output).resolve() != expected:
        raise ValueError("PMX staging did not return the requested output")
    if not expected.is_file():
        raise ValueError("PMX staging output does not exist")
    return report


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
        "mmdExe": str(repo_root / "tools" / "mmd-dumper" / "MikuMikuDance_v932x64" / "MikuMikuDance.exe"),
        "project": str(scene_path), "frames": list(case.frames), "output": str(output), "done": str(output) + ".done",
        "timeoutMs": 60000, "dump": {"bones": True, "morphs": True, "camera": case.camera_vmd is not None, "cameraKeyframes": True, "sceneParameters": False, "rigidBodies": False},
    }
    with fixture_path.open("w", encoding="utf-8") as stream:
        stream.write(json.dumps(fixture, ensure_ascii=True, indent=2) + "\n")


def _fail(result: dict[str, Any], phase: str, message: str) -> None:
    result["ok"] = False
    result["phase"] = phase
    result["errors"].append({"phase": phase, "message": message})


def _text(value: str | bytes | None) -> str:
    return value.decode("utf-8", errors="replace") if isinstance(value, bytes) else value or ""


def _default_repo_root() -> Path:
    return Path(__file__).resolve().parents[4]
