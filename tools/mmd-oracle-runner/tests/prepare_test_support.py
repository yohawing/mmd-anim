from __future__ import annotations

import json
from pathlib import Path

from mmd_oracle_runner.prepare import CommandResult

REPO_ROOT = Path(__file__).resolve().parents[3]


class FakeRunner:
    """Deterministic Rust CLI substitute for unit tests."""

    def __init__(self, *, mode="success"):
        self.mode = mode
        self.calls: list[tuple[tuple[str, ...], Path]] = []

    def run(self, command, cwd):
        command, cwd = tuple(str(part) for part in command), Path(cwd)
        self.calls.append((command, cwd))
        if self.mode == "failure":
            return CommandResult(command, cwd, 7, "", "fake backend failure")
        if self.mode == "timeout":
            return CommandResult(command, cwd, 124, "", "command timed out")
        if "inspect" in command:
            camera = 1 if self.mode == "unsupported" else 0
            text = f"VMD parser: boneFrames=5 morphFrames=0 cameraFrames={camera} lightFrames=0 selfShadowFrames=0 propertyFrames=0"
            return CommandResult(command, cwd, 0, text, "")
        scene = Path(command[command.index("build-pmm") + 3])
        scene.parent.mkdir(parents=True, exist_ok=True)
        scene.write_bytes(b"fake-rust-pmm")
        report = {
            "status": "ok", "command": "build-pmm", "mode": "pmx-vmd-scene",
            "counts": {"bones": 3, "morphs": 0},
            "keyframes": {"bone": 5, "morph": 0, "skippedBones": 1 if self.mode == "skipped" else 0, "skippedMorphs": 0},
        }
        return CommandResult(command, cwd, 0, json.dumps(report), "")


def write_case(tmp_path: Path, *, name="body-only", camera=False, property_opt_in=False, backend="python-rust") -> Path:
    inputs = {
        "pmx": str(REPO_ROOT / "crates" / "mmd-anim-format" / "fixtures" / "pmx" / "ik_multi_axis_limit.pmx"),
        "bodyVmd": str(REPO_ROOT / "crates" / "mmd-anim-format" / "fixtures" / "vmd" / "ik_multi_bone_nondefault.vmd"),
    }
    if camera:
        inputs["cameraVmd"] = str(REPO_ROOT / "crates" / "mmd-anim-format" / "fixtures" / "vmd" / "simple_camera.vmd")
    payload = {
        "schemaVersion": 1, "name": name, "input": inputs,
        "frames": [0, 15, 30, 45] if camera else [0, 15, 30],
        "outputRoot": str(tmp_path / "output"), "generatorBackend": backend,
        "recordOptIn": False, "dialogOptIn": False,
    }
    if property_opt_in:
        payload["requestedFeatures"] = ["property-ik"]
    case_path = tmp_path / "case.json"
    case_path.write_text(json.dumps(payload), encoding="utf-8")
    return case_path
