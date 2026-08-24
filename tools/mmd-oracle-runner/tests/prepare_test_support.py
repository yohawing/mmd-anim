from __future__ import annotations

import json
from pathlib import Path

from mmd_oracle_runner.prepare import CommandResult

REPO_ROOT = Path(__file__).resolve().parents[3]
M0_FIXTURE_ROOT = REPO_ROOT / "MMDDumper" / "fixtures" / "m0"
M0_BASELINE = json.loads((M0_FIXTURE_ROOT / "expected-baseline.json").read_text(encoding="utf-8"))


class FakeRunner:
    def __init__(self, *, mode="node-success", property_frames=0, reported_property_frames=None):
        self.mode = mode
        self.property_frames = property_frames
        self.reported_property_frames = property_frames if reported_property_frames is None else reported_property_frames
        self.calls: list[tuple[tuple[str, ...], Path]] = []

    def run(self, command, cwd):
        command, cwd = tuple(str(part) for part in command), Path(cwd)
        self.calls.append((command, cwd))
        if command[0] == "node":
            if "inspect-vmd" in command:
                counts = self._source_counts(any(part.endswith("simple_camera.vmd") for part in command))
                return CommandResult(command, cwd, 0, json.dumps({"counts": counts}), "")
            if "stage-pmx" in command:
                source = Path(command[command.index("--input") + 1])
                output = Path(command[command.index("--output") + 1])
                if self.mode == "node-utf16":
                    return CommandResult(command, cwd, 0, json.dumps({"ok": True, "input": str(source), "output": str(source), "converted": False, "encoding": "utf-16le"}), "")
                output.write_bytes(source.read_bytes())
                return CommandResult(command, cwd, 0, json.dumps({"ok": True, "input": str(source), "output": str(output), "converted": True, "encoding": "utf-8"}), "")
            if self.mode == "failure":
                return CommandResult(command, cwd, 7, "", "fake backend failure")
            if self.mode == "timeout":
                return CommandResult(command, cwd, 124, "", "command timed out")
            return self._node_result(command, cwd)
        if "inspect" in command:
            counts = {"bones": 3, "morphs": 0, "cameras": 1 if self.mode == "rust-unsupported" else 0, "lights": 0, "selfShadows": 0, "properties": 0}
            text = "VMD parser: " + " ".join(f"{key[:-1] if key.endswith('s') else key}Frames={value}" for key, value in counts.items())
            text = text.replace("camerFrames", "cameraFrames").replace("propertieFrames", "propertyFrames")
            return CommandResult(command, cwd, 0, text, "")
        scene = Path(command[command.index("build-pmm") + 3])
        scene.write_bytes(b"fake-rust-pmm")
        report = {
            "status": "ok", "command": "build-pmm", "mode": "pmx-vmd-scene", "counts": {"bones": 3, "morphs": 0},
            "keyframes": {"bone": 5, "morph": 0, "skippedBones": 1 if self.mode == "rust-skipped" else 0, "skippedMorphs": 0},
        }
        return CommandResult(command, cwd, 0, json.dumps(report), "")

    def _node_result(self, command, cwd):
        manifest = json.loads(Path(command[command.index("--manifest") + 1]).read_text(encoding="utf-8"))
        case = manifest["cases"][0]
        run_dir = Path(command[command.index("--out-dir") + 1]) / case["name"]
        run_dir.mkdir(parents=True, exist_ok=True)
        (run_dir / "scene.pmm").write_bytes(b"fake-node-pmm")
        (run_dir / "fixture.json").write_text("{}", encoding="utf-8")
        node_case = {
            "ok": True, "name": case["name"],
            "stagedPmx": {"converted": self.mode != "node-utf16", "encoding": "utf-16le" if self.mode == "node-utf16" else "utf-8", "output": str(run_dir / "model.mmd-utf16.pmx")},
            "project": str(run_dir / "scene.pmm"), "fixturePath": str(run_dir / "fixture.json"), "frames": case["frames"],
            "sourceCounts": {"bodyVmd": self._source_counts(False), **({"cameraVmd": self._source_counts(True)} if "cameraMotion" in case["assets"] else {})},
            "patch": {"ok": self.mode not in ("node-mismatch", "node-patch-false"), "counts": {"pmmBoneKeyframes": 5, "vmdBoneFrames": 5, "pmmMorphKeyframes": 0, "vmdMorphFrames": 0, "mismatches": 1 if self.mode == "node-mismatch" else 0}},
            "filter": {"skippedBoneNames": ["missing"] if self.mode in ("node-all-skipped", "node-skipped") else [], "skippedMorphNames": [], "droppedUnsupportedChannels": {"propertyFrames": self.reported_property_frames}},
        }
        if self.mode == "node-empty":
            del node_case["patch"]
            node_case["mode"] = "pmx-generated-pmm"
        if self.mode == "node-all-skipped":
            del node_case["patch"]
        return CommandResult(command, cwd, 0, json.dumps({"ok": True, "results": [node_case]}), "")

    def _source_counts(self, is_camera):
        counts = {
            "boneFrames": 0 if is_camera or self.mode == "node-empty" else 5, "morphFrames": 0,
            "cameraFrames": 1 if is_camera else 0,
            "lightFrames": 1 if self.mode == "node-light" or (is_camera and self.mode == "camera-extra") else 0,
            "selfShadowFrames": 0, "propertyFrames": self.property_frames if not is_camera else 0,
        }
        if self.mode == "inspect-float":
            counts["lightFrames"] = 0.0
        return counts


def write_case(tmp_path: Path, *, name="body-only", camera=False, property_opt_in=False, backend="node-mmddumper") -> Path:
    inputs = {
        "pmx": str(REPO_ROOT / "crates" / "mmd-anim-format" / "fixtures" / "pmx" / "ik_multi_axis_limit.pmx"),
        "bodyVmd": str(REPO_ROOT / "crates" / "mmd-anim-format" / "fixtures" / "vmd" / "ik_multi_bone_nondefault.vmd"),
    }
    if property_opt_in:
        inputs["bodyVmd"] = str(M0_FIXTURE_ROOT / "body-property-ik.vmd")
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
