from __future__ import annotations

import json
from pathlib import Path

from mmd_oracle_runner.case import load_case
from mmd_oracle_runner.prepare import CommandResult, _artifact_name, prepare_case
from prepare_test_support import FakeRunner, REPO_ROOT, write_case


class FrameRangeFakeRunner(FakeRunner):
    """Add the frame-range patch output to the shared Rust CLI fake."""

    def run(self, command, cwd):
        command = tuple(str(part) for part in command)
        if "patch" in command and "--frame-range" in command:
            cwd = Path(cwd)
            input_path = Path(command[command.index("patch") + 1])
            output_path = Path(command[command.index("--frame-range") + 1])
            output_path.write_bytes(input_path.read_bytes())
            self.calls.append((command, cwd))
            return CommandResult(command, cwd, 0, '{"status":"ok","mode":"scene-frame-range"}', "")
        return super().run(command, cwd)


def test_python_rust_prepare_writes_owned_artifacts(tmp_path: Path):
    case = load_case(write_case(tmp_path))
    runner = FrameRangeFakeRunner()
    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)
    assert result["ok"] is True
    assert result["comparison"]["status"] == "generated"
    assert ["cargo", "run"] == list(runner.calls[0][0][:2])
    assert all(entry["exists"] for entry in result["artifacts"].values())
    fixture = json.loads(Path(result["artifacts"]["fixture"]["path"]).read_text(encoding="utf-8"))
    assert Path(fixture["project"]) == Path(result["artifacts"]["project"]["path"])
    assert not Path(fixture["output"]).exists()


def test_prepare_rejects_rust_unsupported_channels(tmp_path: Path):
    case = load_case(write_case(tmp_path))
    result = prepare_case(case, runner=FakeRunner(mode="unsupported"), repo_root=REPO_ROOT)
    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert result["artifacts"]["result"]["exists"] is True


def test_prepare_rejects_camera_before_rust_build(tmp_path: Path):
    case = load_case(write_case(tmp_path, camera=True))
    runner = FakeRunner()
    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)
    assert result["ok"] is False
    assert any("camera VMD" in error["message"] for error in result["errors"])
    assert runner.calls == []


def test_prepare_reports_skipped_frames_without_rejecting_the_pair(tmp_path: Path):
    case = load_case(write_case(tmp_path))
    runner = FrameRangeFakeRunner(mode="skipped")
    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)
    assert result["ok"] is True
    assert result["skippedCounts"] == {"boneFrames": 1, "morphFrames": 0}


def test_prepare_patches_scene_to_largest_requested_frame(tmp_path: Path):
    case = load_case(write_case(tmp_path))
    runner = FrameRangeFakeRunner()
    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)
    assert result["ok"] is True
    patch_call = next(command for command, _ in runner.calls if "patch" in command)
    assert patch_call[patch_call.index("--begin-frame") + 1] == "0"
    assert patch_call[patch_call.index("--end-frame") + 1] == str(max(case.frames))
    assert patch_call[patch_call.index("--begin-frame-enabled") + 1] == "true"
    assert patch_call[patch_call.index("--end-frame-enabled") + 1] == "true"


def test_prepare_backend_failure_is_reported(tmp_path: Path):
    case = load_case(write_case(tmp_path))
    result = prepare_case(case, runner=FakeRunner(mode="failure"), repo_root=REPO_ROOT)
    assert result["ok"] is False
    assert result["phase"] == "preflight"


def test_artifact_name_is_safe_and_bounded():
    value = _artifact_name("CON: unsafe/long?name")
    assert len(value) <= 64 and ":" not in value and "/" not in value
