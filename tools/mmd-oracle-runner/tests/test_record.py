from __future__ import annotations

import json
from dataclasses import replace
from pathlib import Path

import pytest

from mmd_oracle_runner.case import load_case
from mmd_oracle_runner.prepare import CommandResult, _input_inventory
from mmd_oracle_runner.record import record_case
from prepare_test_support import REPO_ROOT, write_case


class FakeRecordRunner:
    def __init__(self, *, mode: str = "success"):
        self.mode = mode
        self.calls: list[tuple[tuple[str, ...], Path]] = []

    def run(self, command, cwd):
        command = tuple(str(part) for part in command)
        cwd = Path(cwd)
        self.calls.append((command, cwd))
        if command[2] == "record":
            fixture = json.loads(Path(command[command.index("--fixture") + 1]).read_text(encoding="utf-8"))
            output = Path(fixture["output"])
            Path(f"{output}.proxy.log").write_text("native proxy log\n", encoding="utf-8")
            if self.mode not in {"timeout", "process-fail", "no-done", "bad-done"}:
                output.write_text("not-json\n" if self.mode == "schema-fail" else "{}\n", encoding="utf-8")
            if self.mode not in {"timeout", "process-fail", "no-done"}:
                done = Path(fixture["done"])
                payload = {"ok": True, "output": str(output), "records": 2 if self.mode == "count-mismatch" else 1}
                if self.mode == "bad-done":
                    payload = {"ok": False}
                done.write_text(json.dumps(payload), encoding="utf-8")
            stderr = ""
            exit_code = 0
            if self.mode == "timeout":
                exit_code, stderr = 1, "Timed out waiting for MMD process"
            elif self.mode == "process-fail":
                exit_code, stderr = 7, "MMD process failed"
            elif self.mode == "process-schema-fail":
                exit_code, stderr = 7, "MMD process failed after writing output"
            elif self.mode == "restore":
                stderr = "mmd-smoke:restore-missing-backup"
            return CommandResult(command, cwd, exit_code, "{\"record\":1}\n{\"joined\":true}\n", stderr)
        if command[2] == "validate":
            if self.mode == "schema-fail":
                return CommandResult(command, cwd, 1, "", "invalid JSONL")
            return CommandResult(command, cwd, 0, json.dumps({"ok": True, "records": 1}), "")
        if command[2] == "verify-coverage":
            if self.mode == "schema-fail":
                return CommandResult(command, cwd, 1, "", "invalid JSONL")
            if self.mode == "coverage-fail":
                return CommandResult(command, cwd, 1, json.dumps({"ok": False, "records": 1}), "coverage incomplete")
            return CommandResult(command, cwd, 0, json.dumps({"ok": True, "records": 1}), "")
        raise AssertionError(f"unexpected command: {command}")


def _prepared_case(tmp_path: Path, *, camera: bool = False, dialog: bool = False):
    case = load_case(write_case(tmp_path, camera=camera))
    case = replace(case, record_opt_in=True, dialog_opt_in=dialog)
    run_dir = case.output_root / "body-only"
    run_dir.mkdir(parents=True)
    project = run_dir / "scene.pmm"
    fixture = run_dir / "fixture.json"
    output = run_dir / "oracle.actual.jsonl"
    done = run_dir / "oracle.actual.jsonl.done"
    project.write_bytes(b"prepared-pmm")
    fixture.write_text(
        json.dumps(
            {
                "name": case.name,
                "mmdVersion": "9.32-x64",
                "mmdExe": "old-mmd.exe",
                "project": str(project),
                "frames": list(case.frames),
                "output": str(output),
                "done": str(done),
                "timeoutMs": 60000,
                "dump": {
                    "bones": True,
                    "morphs": True,
                    "camera": camera,
                    "cameraKeyframes": True,
                    "sceneParameters": False,
                    "rigidBodies": False,
                },
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    marker = {
        "schemaVersion": 1,
        "ok": True,
        "phase": "complete",
        "caseFile": str(case.source_path),
        "artifactName": "body-only",
        "frames": list(case.frames),
        "inputInventory": _input_inventory(case),
        "ownedArtifacts": [str(project), str(fixture)],
        "artifacts": {
            "project": {"path": str(project), "exists": True},
            "fixture": {"path": str(fixture), "exists": True},
        },
    }
    (run_dir / "prepare-result.json").write_text(json.dumps(marker), encoding="utf-8")
    exe = tmp_path / "MikuMikuDance.exe"
    exe.write_bytes(b"fake exe")
    return case, exe, fixture


def test_launch_requires_case_opt_in_and_environment_guard(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(replace(case, record_opt_in=False), exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["recorded"] is False
    assert result["phases"]["launchGuard"]["status"] == "fail"
    assert runner.calls == []
    assert not (case.output_root / "body-only" / "record-result.json").exists()

    retried = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)
    assert retried["ok"] is True
    calls_after_retry = len(runner.calls)

    monkeypatch.delenv("MMD_DUMPER_ALLOW_MMD_LAUNCH")
    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)
    assert result["phases"]["launchGuard"]["status"] == "fail"
    assert len(runner.calls) == calls_after_retry


def test_record_success_uses_temp_fixture_and_preserves_stable_fixture(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path, dialog=False)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    before = fixture.read_bytes()
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert result["recorded"] is True
    assert result["frames"] == list(case.frames)
    assert result["phases"]["schema"]["report"]["records"] == 1
    assert result["phases"]["coverage"]["report"]["ok"] is True
    assert result["mmdExecutable"]["path"] == str(exe)
    assert result["mmdExecutable"]["size"] == len(b"fake exe")
    assert len(result["mmdExecutable"]["sha256"]) == 64
    assert fixture.read_bytes() == before
    assert len(runner.calls) == 2
    record_command = runner.calls[0][0]
    assert "--accept-dialog" not in record_command
    temp_fixture = Path(record_command[record_command.index("--fixture") + 1])
    assert not temp_fixture.exists()
    assert not Path(f"{case.output_root / 'body-only' / '.oracle.actual.jsonl.tmp'}.proxy.log").exists()
    assert not (fixture.parent / ".record-attempt.json").exists()
    assert result["phases"]["dialog"]["status"] == "not_run"


def test_dialog_opt_in_adds_only_explicit_accept_flag(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path, dialog=True)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    command = runner.calls[0][0]
    assert command[command.index("--accept-dialog") + 1] == "true"
    assert result["phases"]["dialog"]["status"] == "pass"


def test_invalid_mmd_exe_is_rejected_before_subprocess(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, _, _ = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(case, tmp_path / "missing.exe", runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phases"]["process"]["status"] == "not_run"
    assert runner.calls == []
    assert any("existing file" in error["message"] for error in result["errors"])


@pytest.mark.parametrize("mode,phase", [("no-done", "done"), ("bad-done", "done"), ("schema-fail", "schema"), ("coverage-fail", "coverage")])
def test_record_subgates_fail_closed(tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mode: str, phase: str):
    case, exe, _ = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=FakeRecordRunner(mode=mode), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phases"][phase]["status"] == "fail"
    if mode in {"schema-fail", "coverage-fail"}:
        assert result["recorded"] is False
    if mode == "schema-fail":
        assert len(result["commands"]) == 2
        assert result["phases"]["coverage"]["status"] == "not_run"
    if mode == "coverage-fail":
        assert result["phases"]["coverage"]["report"]["ok"] is False


def test_timeout_and_restore_markers_are_diagnostics_even_without_outer_timeout(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    for mode, expected in (("timeout", "timeout"), ("restore", "process")):
        case_root = tmp_path / mode
        case_root.mkdir()
        case, exe, _ = _prepared_case(case_root)
        result = record_case(case, exe, runner=FakeRecordRunner(mode=mode), repo_root=REPO_ROOT)
        assert result["ok"] is False
        assert result["phases"][expected]["status"] == "fail"


def test_camera_coverage_requests_camera_requirement(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path, camera=True)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is True
    coverage_command = runner.calls[1][0]
    assert "--require-camera" in coverage_command
    assert coverage_command[coverage_command.index("--require-camera") + 1] == "true"


def test_process_failure_skips_schema_and_coverage(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner(mode="process-fail")

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phases"]["process"]["status"] == "fail"
    assert result["phases"]["process"]["exitCode"] == 7
    assert result["phases"]["schema"]["status"] == "not_run"
    assert result["phases"]["coverage"]["status"] == "not_run"
    assert len(runner.calls) == 1


def test_process_failure_still_validates_written_output(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner(mode="process-schema-fail")

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phases"]["process"]["status"] == "fail"
    assert result["phases"]["schema"]["status"] == "pass"
    assert result["phases"]["coverage"]["status"] == "not_run"
    assert runner.calls[1][0][2] == "validate"


def test_existing_unowned_dump_fails_closed_without_overwrite(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    run_dir = fixture.parent
    old_output = run_dir / "oracle.actual.jsonl"
    old_output.write_bytes(b"old evidence\n")
    before = old_output.read_bytes()
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert runner.calls == []
    assert old_output.read_bytes() == before
    assert not (run_dir / "record-result.json").exists()


def test_invalid_record_marker_fails_closed_without_overwrite(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    run_dir = fixture.parent
    old_output = run_dir / "oracle.actual.jsonl"
    old_marker = run_dir / "record-result.json"
    old_output.write_bytes(b"old evidence\n")
    old_marker.write_bytes(b"not json")
    before_output, before_marker = old_output.read_bytes(), old_marker.read_bytes()
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert runner.calls == []
    assert old_output.read_bytes() == before_output
    assert old_marker.read_bytes() == before_marker


def test_record_failure_preserves_previous_success_dump_and_done(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    first = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)
    assert first["ok"] is True
    run_dir = fixture.parent
    output, done = run_dir / "oracle.actual.jsonl", run_dir / "oracle.actual.jsonl.done"
    output_before, done_before = output.read_bytes(), done.read_bytes()

    second = record_case(case, exe, runner=FakeRecordRunner(mode="process-fail"), repo_root=REPO_ROOT)

    assert second["ok"] is False
    assert output.read_bytes() == output_before
    assert done.read_bytes() == done_before
    assert second["phases"]["schema"]["status"] == "not_run"
    assert second["phases"]["coverage"]["status"] == "not_run"


def test_stale_prepare_marker_is_rejected_before_record(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    marker_path = fixture.parent / "prepare-result.json"
    marker = json.loads(marker_path.read_text(encoding="utf-8"))
    marker["schemaVersion"] = 2
    marker_path.write_text(json.dumps(marker), encoding="utf-8")
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert runner.calls == []


def test_prepare_input_inventory_mismatch_is_rejected(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    marker_path = fixture.parent / "prepare-result.json"
    marker = json.loads(marker_path.read_text(encoding="utf-8"))
    marker["inputInventory"]["bodyVmd"]["sha256"] = "stale"
    marker_path.write_text(json.dumps(marker), encoding="utf-8")
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert runner.calls == []


def test_result_marker_has_identity_inventory_and_owned_artifacts(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    stored = json.loads((fixture.parent / "record-result.json").read_text(encoding="utf-8"))
    assert result["ok"] is True
    assert stored["schemaVersion"] == 1
    assert stored["caseFile"] == str(case.source_path)
    assert stored["artifactName"] == "body-only"
    assert stored["inputInventory"] == _input_inventory(case)
    assert all(path in stored["ownedArtifacts"] for path in (str(fixture.parent / "oracle.actual.jsonl"), str(fixture.parent / "oracle.actual.jsonl.done"), str(fixture.parent / "record-result.json")))
    assert "_firstFailure" not in result
    assert "_firstFailure" not in stored


def test_done_record_count_must_match_jsonl(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    result = record_case(case, exe, runner=FakeRecordRunner(mode="count-mismatch"), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["recorded"] is False
    assert result["phases"]["done"]["status"] == "fail"
    assert any("do not match dump JSONL count" in error["message"] for error in result["errors"])
