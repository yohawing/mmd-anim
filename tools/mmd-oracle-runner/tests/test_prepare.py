from __future__ import annotations

import json
import subprocess
from pathlib import Path

from mmd_oracle_runner.case import load_case
from mmd_oracle_runner.cli import main
from mmd_oracle_runner.prepare import CommandResult, SubprocessRunner, prepare_case
from prepare_test_support import FakeRunner, REPO_ROOT, write_case as _write_case


class CameraComparisonRunner(FakeRunner):
    def __init__(self, report, *, mode="node-success", case_ok=True):
        super().__init__(mode=mode)
        self.report = report
        self.case_ok = case_ok

    def run(self, command, cwd):
        outcome = super().run(command, cwd)
        if outcome.exit_code == 0 and command and command[0] == "node" and "oracle-batch" in command:
            payload = json.loads(outcome.stdout)
            payload["results"][0]["cameraComparison"] = self.report
            payload["results"][0]["ok"] = self.case_ok
            return CommandResult(outcome.command, outcome.cwd, 0 if self.case_ok else 1, json.dumps(payload), outcome.stderr)
        return outcome


def test_fake_node_prepare_writes_stable_result_and_never_records(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    runner = FakeRunner()

    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert result["phase"] == "complete"
    assert result["recorded"] is False
    assert result["comparison"]["patchCounts"]["mismatches"] == 0
    assert result["artifacts"]["project"]["exists"] is True
    assert result["artifacts"]["fixture"]["exists"] is True
    assert result["artifacts"]["result"]["exists"] is True
    assert result["preflight"]["bodyVmd"]["boneFrames"] == 5
    assert "stage-pmx" in runner.calls[0][0]
    command, cwd = runner.calls[1]
    assert command[0] == "node" and "--dry-run" in command and "true" in command
    assert "record" not in command
    assert cwd == REPO_ROOT / "MMDDumper"


def test_property_frames_without_opt_in_fail_closed_after_node_preflight(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    result = prepare_case(case, runner=FakeRunner(property_frames=1), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert result["droppedUnsupportedChannels"]["propertyFrames"] == 1
    assert result["classifications"]["propertyFrames"] == "reproduced"
    assert result["artifacts"]["result"]["exists"] is True


def test_property_frames_with_node_opt_in_are_reported_not_hidden(tmp_path: Path):
    case = load_case(_write_case(tmp_path, name="body-property-ik", property_opt_in=True))
    result = prepare_case(case, runner=FakeRunner(property_frames=1), repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert result["droppedUnsupportedChannels"]["propertyFrames"] == 1
    assert result["classifications"]["propertyFrames"] == "reproduced"


def test_property_drop_count_must_match_preflight(tmp_path: Path):
    case = load_case(_write_case(tmp_path, property_opt_in=True))
    result = prepare_case(case, runner=FakeRunner(property_frames=1, reported_property_frames=0), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert "drop count" in result["errors"][-1]["message"]


def test_backend_failure_writes_failure_result(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    result = prepare_case(case, runner=FakeRunner(mode="failure"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "generator"
    assert result["backend"]["exitCode"] == 7
    assert result["artifacts"]["result"]["exists"] is True


def test_backend_failure_preserves_last_successful_owned_artifacts(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    prepare_case(case, runner=FakeRunner(), repo_root=REPO_ROOT)
    run_dir = case.output_root / case.name
    scene_before = (run_dir / "scene.pmm").read_bytes()
    fixture_before = (run_dir / "fixture.json").read_bytes()

    result = prepare_case(case, runner=FakeRunner(mode="failure"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "generator"
    assert (run_dir / "scene.pmm").read_bytes() == scene_before
    assert (run_dir / "fixture.json").read_bytes() == fixture_before
    assert result["artifacts"]["result"]["exists"] is True


def test_unowned_artifacts_are_preserved_without_subprocess(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    run_dir = case.output_root / case.name
    run_dir.mkdir(parents=True)
    scene = run_dir / "scene.pmm"
    marker = run_dir / "prepare-result.json"
    scene.write_bytes(b"user-owned")
    marker.write_text("not-json", encoding="utf-8")
    runner = FakeRunner()

    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)

    assert result["phase"] == "preflight" and runner.calls == []
    assert scene.read_bytes() == b"user-owned" and marker.read_text(encoding="utf-8") == "not-json"
    second = prepare_case(case, runner=runner, repo_root=REPO_ROOT)
    assert second["phase"] == "preflight" and runner.calls == []
    assert scene.read_bytes() == b"user-owned"


def test_input_artifact_collision_fails_before_cleanup(tmp_path: Path):
    case_path = _write_case(tmp_path)
    payload = json.loads(case_path.read_text(encoding="utf-8"))
    collision = tmp_path / "output" / "body-only" / "scene.pmm"
    collision.parent.mkdir(parents=True)
    collision.write_bytes(b"input")
    payload["input"]["pmx"] = str(collision)
    case_path.write_text(json.dumps(payload), encoding="utf-8")
    runner = FakeRunner()

    result = prepare_case(load_case(case_path), runner=runner, repo_root=REPO_ROOT)

    assert result["phase"] == "preflight" and runner.calls == []
    assert collision.read_bytes() == b"input"


def test_case_file_artifact_collision_fails_before_cleanup(tmp_path: Path):
    case_path = _write_case(tmp_path)
    payload = json.loads(case_path.read_text(encoding="utf-8"))
    collision = tmp_path / "output" / "body-only" / "prepare-result.json"
    collision.parent.mkdir(parents=True)
    collision.write_text(json.dumps(payload), encoding="utf-8")
    runner = FakeRunner()

    result = prepare_case(load_case(collision), runner=runner, repo_root=REPO_ROOT)

    assert result["phase"] == "preflight" and runner.calls == []
    assert json.loads(collision.read_text(encoding="utf-8")) == payload


def test_node_empty_body_without_patch_counts_is_not_applicable(tmp_path: Path):
    case = load_case(_write_case(tmp_path, name="empty-body", property_opt_in=True))

    result = prepare_case(case, runner=FakeRunner(mode="node-empty", property_frames=1), repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert result["comparison"] == {"status": "not-applicable", "reason": "node backend emitted no patch counts for an empty body VMD"}
    assert result["artifacts"]["project"]["exists"] is True
    assert result["artifacts"]["fixture"]["exists"] is True


def test_node_comparison_mismatch_is_non_pass(tmp_path: Path):
    case = load_case(_write_case(tmp_path))

    result = prepare_case(case, runner=FakeRunner(mode="node-mismatch"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["comparison"]["status"] == "failed"
    assert result["comparison"]["mismatches"] == 1


def test_unsafe_case_names_are_contained_and_collision_resistant(tmp_path: Path):
    first = load_case(_write_case(tmp_path, name="../escape"))
    first_result = prepare_case(first, runner=FakeRunner(), repo_root=REPO_ROOT)
    second = load_case(_write_case(tmp_path, name="CON"))
    second_result = prepare_case(second, runner=FakeRunner(), repo_root=REPO_ROOT)

    assert first_result["caseName"] == "../escape"
    assert first_result["artifactName"] != first.name
    assert second_result["artifactName"] != first_result["artifactName"]
    assert Path(first_result["artifacts"]["project"]["path"]).parent == first.output_root / first_result["artifactName"]
    assert (first.output_root / first_result["artifactName"]).resolve().parent == first.output_root.resolve()


def test_rust_prepare_uses_one_explicit_backend_and_marks_comparison_unverified(tmp_path: Path):
    case = load_case(_write_case(tmp_path, backend="rust-build-pmm"))
    runner = FakeRunner(mode="rust-success")

    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "artifacts"
    assert result["comparison"]["status"] == "not-verified"
    assert result["classifications"]["pmxStaging"] == "working"
    assert len(runner.calls) == 3
    assert "stage-pmx" in runner.calls[0][0]
    assert runner.calls[1][0][-2:] == ("inspect", str(case.body_vmd))
    assert runner.calls[2][0][:6] == ("cargo", "run", "-q", "-p", "mmd-anim-cli", "--")
    assert "build-pmm" in runner.calls[2][0]
    assert result["droppedUnsupportedChannels"] == {}


def test_node_source_counts_reject_non_property_channels(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    runner = FakeRunner(mode="node-light")
    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert result["preflight"]["bodyVmd"]["lightFrames"] == 1
    assert len(runner.calls) == 2


def test_camera_source_counts_reject_non_camera_channels(tmp_path: Path):
    case = load_case(_write_case(tmp_path, camera=True))
    runner = FakeRunner(mode="camera-extra")

    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False and result["phase"] == "preflight"
    assert result["preflight"]["cameraVmd"]["lightFrames"] == 1
    assert len(runner.calls) == 2 and "oracle-batch" in runner.calls[1][0]


def test_camera_artifacts_report_partial_comparison(tmp_path: Path):
    case = load_case(_write_case(tmp_path, camera=True))

    result = prepare_case(case, runner=CameraComparisonRunner({"ok": True, "expected": 1, "actual": 1, "mismatches": []}), repo_root=REPO_ROOT)

    assert result["ok"] is True and result["phase"] == "complete"
    assert result["comparison"]["status"] == "verified"
    assert result["comparison"]["camera"]["status"] == "verified"
    assert result["comparison"]["camera"]["comparison"]["expected"] == 1
    assert result["artifacts"]["project"]["exists"] is True


def test_camera_with_empty_body_is_partial_not_verified(tmp_path: Path):
    case = load_case(_write_case(tmp_path, camera=True))

    result = prepare_case(case, runner=CameraComparisonRunner({"ok": True, "expected": 1, "actual": 1, "mismatches": []}, mode="node-empty"), repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert result["comparison"]["status"] == "partial"
    assert result["comparison"]["camera"]["status"] == "verified"


def test_camera_comparison_report_is_required(tmp_path: Path):
    case = load_case(_write_case(tmp_path, camera=True))

    result = prepare_case(case, runner=FakeRunner(), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "artifacts"
    assert result["comparison"]["status"] == "failed"
    assert result["comparison"]["camera"]["status"] == "failed"
    assert "did not return camera comparison" in result["comparison"]["camera"]["reason"]


def test_camera_comparison_mismatch_is_non_pass(tmp_path: Path):
    case = load_case(_write_case(tmp_path, camera=True))
    report = {"ok": False, "expected": 1, "actual": 1, "mismatches": [{"kind": "camera", "key": "0"}]}

    result = prepare_case(case, runner=CameraComparisonRunner(report), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "artifacts"
    assert result["comparison"]["status"] == "failed"
    assert result["comparison"]["camera"]["status"] == "failed"
    assert result["comparison"]["camera"]["comparison"] == report


def test_camera_comparison_failure_detail_survives_node_case_failure(tmp_path: Path):
    case = load_case(_write_case(tmp_path, camera=True))
    report = {"ok": False, "expected": 1, "actual": 1, "mismatches": [{"kind": "camera", "key": "0"}]}

    result = prepare_case(case, runner=CameraComparisonRunner(report, case_ok=False), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "generator"
    assert result["comparison"]["camera"]["status"] == "failed"
    assert result["comparison"]["camera"]["comparison"] == report


def test_all_skipped_tracks_keep_names_in_failure_result(tmp_path: Path):
    result = prepare_case(load_case(_write_case(tmp_path)), runner=FakeRunner(mode="node-all-skipped"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["skippedBoneNames"] == ["missing"]


def test_partially_skipped_tracks_are_non_pass(tmp_path: Path):
    result = prepare_case(load_case(_write_case(tmp_path)), runner=FakeRunner(mode="node-skipped"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["comparison"]["status"] == "failed"
    assert result["skippedBoneNames"] == ["missing"]


def test_utf16_pmx_staging_is_not_applicable(tmp_path: Path):
    result = prepare_case(load_case(_write_case(tmp_path)), runner=FakeRunner(mode="node-utf16"), repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert result["classifications"]["pmxStaging"] == "not-applicable"


def test_node_inspect_counts_require_integers(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    runner = FakeRunner(mode="inspect-float")

    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False and result["phase"] == "preflight"
    assert len(runner.calls) == 2


def test_rust_preflight_rejects_unsupported_vmd_channels_without_build(tmp_path: Path):
    case = load_case(_write_case(tmp_path, backend="rust-build-pmm"))
    runner = FakeRunner(mode="rust-unsupported")

    result = prepare_case(case, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert result["preflight"]["metadataCounts"]["cameras"] == 1
    assert result["comparison"]["unsupportedCounts"] == {"cameras": 1}
    assert len(runner.calls) == 2
    assert all("build-pmm" not in command for command, _ in runner.calls)


def test_rust_skipped_frames_fail_without_unverifiable_names(tmp_path: Path):
    case = load_case(_write_case(tmp_path, backend="rust-build-pmm"))
    result = prepare_case(case, runner=FakeRunner(mode="rust-skipped"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "artifacts"
    assert result["skippedCounts"] == {"boneFrames": 1, "morphFrames": 0}
    assert "skipped frames" in result["comparison"]["reason"]


def test_subprocess_timeout_is_converted_to_exit_124(tmp_path: Path, monkeypatch):
    def timeout_run(*args, **kwargs):
        raise subprocess.TimeoutExpired(args[0], kwargs["timeout"], output=b"partial", stderr=b"late")

    monkeypatch.setattr(subprocess, "run", timeout_run)
    outcome = SubprocessRunner(timeout_seconds=0.01).run(("fake",), tmp_path)

    assert outcome.exit_code == 124
    assert outcome.stdout == "partial"
    assert outcome.stderr == "late"


def test_prepare_timeout_writes_generator_failure_result(tmp_path: Path):
    case = load_case(_write_case(tmp_path))
    result = prepare_case(case, runner=FakeRunner(mode="timeout"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "generator"
    assert result["backend"]["exitCode"] == 124
    assert result["artifacts"]["result"]["exists"] is True


def test_prepare_cli_returns_one_for_prepare_failure(tmp_path: Path, monkeypatch, capsys):
    case_path = _write_case(tmp_path)
    monkeypatch.setattr("mmd_oracle_runner.cli.prepare_case", lambda case: {"ok": False, "phase": "generator"})

    assert main(["prepare", "--case", str(case_path)]) == 1
    payload = json.loads(capsys.readouterr().out)
    assert payload["ok"] is False


def test_prepare_cli_returns_zero_for_complete_result(tmp_path: Path, monkeypatch, capsys):
    case_path = _write_case(tmp_path)
    monkeypatch.setattr("mmd_oracle_runner.cli.prepare_case", lambda case: {"ok": True, "phase": "complete"})

    assert main(["prepare", "--case", str(case_path)]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["ok"] is True


def test_prepare_cli_labels_case_validation_errors_as_prepare(tmp_path: Path, capsys):
    case_path = tmp_path / "invalid.json"
    case_path.write_text("{}", encoding="utf-8")

    assert main(["prepare", "--case", str(case_path)]) == 2
    payload = json.loads(capsys.readouterr().err)
    assert payload["command"] == "prepare"


def test_prepare_reports_artifact_directory_creation_failure_without_backend(tmp_path: Path, capsys):
    blocked = tmp_path / "blocked"
    blocked.write_text("not a directory", encoding="utf-8")
    case_path = _write_case(tmp_path, name="blocked-case")
    payload = json.loads(case_path.read_text(encoding="utf-8"))
    payload["outputRoot"] = str(blocked / "output")
    case_path.write_text(json.dumps(payload), encoding="utf-8")

    runner = FakeRunner()
    result = prepare_case(load_case(case_path), runner=runner, repo_root=REPO_ROOT)
    assert result["phase"] == "preflight"
    assert result["artifacts"]["result"]["exists"] is False
    assert runner.calls == []

    assert main(["prepare", "--case", str(case_path)]) == 1
    result = json.loads(capsys.readouterr().out)
    assert result["phase"] == "preflight"
    assert result["artifacts"]["result"]["exists"] is False
