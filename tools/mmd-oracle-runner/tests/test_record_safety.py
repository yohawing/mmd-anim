from __future__ import annotations

import json
import os
from dataclasses import replace
from pathlib import Path

import pytest

import mmd_oracle_runner.record_artifacts as record_artifacts_module
from mmd_oracle_runner.prepare import _input_inventory
from mmd_oracle_runner.record import record_case
from mmd_oracle_runner.record_artifacts import (
    paths,
    recover_interrupted_record_artifacts,
    remove_temporary_artifacts,
    valid_done,
    write_attempt_marker,
    write_temp_fixture,
)
from prepare_test_support import REPO_ROOT
from test_record import FakeRecordRunner, _prepared_case


def test_record_artifact_cannot_replace_mmd_executable(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    output = fixture.parent / "oracle.actual.jsonl"
    before = output.read_bytes()
    runner = FakeRecordRunner()

    result = record_case(case, output, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert runner.calls == []
    assert output.read_bytes() == before
    assert any("collides with protected path mmdExecutable" in error["message"] for error in result["errors"])


def test_record_artifact_cannot_replace_current_input(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    output = fixture.parent / "oracle.actual.jsonl"
    colliding_case = replace(case, pmx=output)
    current_inventory = _input_inventory(colliding_case)
    for marker_path in (fixture.parent / "prepare-result.json", fixture.parent / "record-result.json"):
        marker = json.loads(marker_path.read_text(encoding="utf-8"))
        marker["inputInventory"] = current_inventory
        marker_path.write_text(json.dumps(marker), encoding="utf-8")
    before = output.read_bytes()
    runner = FakeRecordRunner()

    result = record_case(colliding_case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert result["phase"] == "preflight"
    assert runner.calls == []
    assert output.read_bytes() == before
    assert any("collides with protected path pmx" in error["message"] for error in result["errors"])


def test_done_marker_rejects_non_json_record(tmp_path: Path):
    output = tmp_path / "oracle.actual.jsonl"
    done = tmp_path / "oracle.actual.jsonl.done"
    output.write_text("not-json\n", encoding="utf-8")
    done.write_text(json.dumps({"ok": True, "output": str(output), "records": 1}), encoding="utf-8")

    ok, reason = valid_done(done, output)

    assert ok is False
    assert reason is not None and "JSONL" in reason


def test_unowned_temporary_dump_is_preserved(tmp_path: Path):
    record_paths = paths(tmp_path)
    record_paths["outputTemp"].write_text("foreign\n", encoding="utf-8")

    with pytest.raises(OSError, match="not owned"):
        remove_temporary_artifacts(record_paths, set())

    assert record_paths["outputTemp"].read_text(encoding="utf-8") == "foreign\n"


def test_interrupted_proxy_log_is_recovered_after_prepared_case_validation(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)
    record_paths["proxyLogTemp"].write_text("interrupted native log\n", encoding="utf-8")
    write_attempt_marker(record_paths, case, _input_inventory(case), {"path": str(exe), "size": exe.stat().st_size})
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert not record_paths["proxyLogTemp"].exists()
    assert not record_paths["attempt"].exists()


def test_interrupted_temporary_fixture_is_attempt_owned_and_recovered(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)
    record_paths["fixtureTemp"].write_text("interrupted fixture\n", encoding="utf-8")
    write_attempt_marker(record_paths, case, _input_inventory(case), {"path": str(exe), "size": exe.stat().st_size})
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert not record_paths["fixtureTemp"].exists()
    assert not record_paths["attempt"].exists()


def test_attempt_marker_created_before_fixture_is_recoverable(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)
    write_attempt_marker(record_paths, case, _input_inventory(case), {"path": str(exe), "size": exe.stat().st_size})
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert result["ok"] is True
    assert not record_paths["attempt"].exists()


def test_attempt_marker_write_failure_removes_partial_file(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)

    def fail_dump(*args, **kwargs):
        del args, kwargs
        raise OSError("simulated JSON write failure")

    monkeypatch.setattr(record_artifacts_module.json, "dump", fail_dump)

    with pytest.raises(OSError, match="simulated JSON write failure"):
        write_attempt_marker(record_paths, case, _input_inventory(case), {"path": str(exe)})

    assert not record_paths["attempt"].exists()


def test_unowned_interrupted_proxy_log_fails_closed(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)
    record_paths["proxyLogTemp"].write_text("foreign native log\n", encoding="utf-8")
    runner = FakeRecordRunner()
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert runner.calls == []
    assert record_paths["proxyLogTemp"].read_text(encoding="utf-8") == "foreign native log\n"


def test_result_marker_alone_cannot_authorize_temporary_cleanup(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    marker = json.loads(record_paths["result"].read_text(encoding="utf-8"))
    marker["ownedArtifacts"].append(str(record_paths["proxyLogTemp"]))
    record_paths["result"].write_text(json.dumps(marker), encoding="utf-8")
    record_paths["proxyLogTemp"].write_text("foreign native log\n", encoding="utf-8")
    runner = FakeRecordRunner()

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert runner.calls == []
    assert record_paths["proxyLogTemp"].read_text(encoding="utf-8") == "foreign native log\n"


def test_complete_interrupted_result_is_promoted_before_validation(tmp_path: Path):
    case, _, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)
    marker = record_paths["result"]
    payload = {
        "schemaVersion": 1,
        "caseFile": str(case.source_path),
        "artifactName": fixture.parent.name,
        "inputInventory": _input_inventory(case),
        "frames": list(case.frames),
        "ownedArtifacts": [str(marker), str(record_paths["resultTemp"])],
    }
    marker.write_text(json.dumps(payload), encoding="utf-8")
    os.replace(marker, record_paths["resultTemp"])

    recover_interrupted_record_artifacts(record_paths, case, _input_inventory(case))

    assert marker.is_file()
    assert not record_paths["resultTemp"].exists()


def test_new_complete_result_temp_replaces_old_result(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    newer = json.loads(record_paths["result"].read_text(encoding="utf-8"))
    newer["commands"]["recovered"] = {"exitCode": 0}
    record_paths["resultTemp"].write_text(json.dumps(newer), encoding="utf-8")

    recover_interrupted_record_artifacts(record_paths, case, _input_inventory(case))

    stored = json.loads(record_paths["result"].read_text(encoding="utf-8"))
    assert stored["commands"]["recovered"]["exitCode"] == 0


def test_temporary_fixture_rejects_output_outside_run_directory(tmp_path: Path):
    run_dir = tmp_path / "run"
    run_dir.mkdir()
    mmd_exe = tmp_path / "MikuMikuDance.exe"
    mmd_exe.write_bytes(b"exe")

    with pytest.raises(ValueError, match="prepared artifact directory"):
        write_temp_fixture(run_dir, {}, mmd_exe, output=tmp_path / "outside.jsonl", done=tmp_path / "outside.done")


def test_prepared_fixture_reparse_is_rejected(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    external = tmp_path / "external-fixture.json"
    external.write_bytes(fixture.read_bytes())
    fixture.unlink()
    try:
        fixture.symlink_to(external)
    except OSError as error:
        pytest.skip(f"symlink creation is unavailable: {error}")
    runner = FakeRecordRunner()
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert runner.calls == []
    assert any("reparse point" in error["message"] for error in result["errors"])


def test_temporary_proxy_log_reparse_is_rejected_before_subprocess(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    external = tmp_path / "external-proxy.log"
    external.write_text("foreign\n", encoding="utf-8")
    proxy_log = paths(fixture.parent)["proxyLogTemp"]
    try:
        proxy_log.symlink_to(external)
    except OSError as error:
        pytest.skip(f"symlink creation is unavailable: {error}")
    runner = FakeRecordRunner()
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert runner.calls == []
    assert external.read_text(encoding="utf-8") == "foreign\n"
    assert any("reparse point" in error["message"] for error in result["errors"])


def test_prepared_fixture_dump_policy_is_immutable(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    payload = json.loads(fixture.read_text(encoding="utf-8"))
    payload["dump"]["bones"] = False
    fixture.write_text(json.dumps(payload), encoding="utf-8")
    runner = FakeRecordRunner()
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert runner.calls == []
    assert any("fixture" in error["message"] for error in result["errors"])
