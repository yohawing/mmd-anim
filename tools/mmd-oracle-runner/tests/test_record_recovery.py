from __future__ import annotations

import json
import os
from dataclasses import replace
from pathlib import Path

import pytest

import mmd_oracle_runner.record as record_module
import mmd_oracle_runner.record_artifacts as record_artifacts_module
from mmd_oracle_runner.prepare import _input_inventory
from mmd_oracle_runner.record import record_case
from mmd_oracle_runner.record_artifacts import (
    paths,
    recover_interrupted_record_artifacts,
    valid_done,
)
from prepare_test_support import REPO_ROOT
from test_record import FakeRecordRunner, _prepared_case


def test_record_rejects_prepare_after_only_frames_change(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path)
    changed = replace(case, frames=(*case.frames, 99))
    runner = FakeRecordRunner()
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    result = record_case(changed, exe, runner=runner, repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert runner.calls == []
    assert any("frames" in error["message"] for error in result["errors"])


def test_record_retry_restores_interrupted_promotion_backup(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    first = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)
    assert first["ok"] is True
    record_paths = paths(fixture.parent)
    old_output = record_paths["output"].read_bytes()
    old_done = record_paths["done"].read_bytes()
    os.replace(record_paths["output"], record_paths["backupOutput"])
    os.replace(record_paths["done"], record_paths["backupDone"])

    retried = record_case(case, exe, runner=FakeRecordRunner(mode="process-fail"), repo_root=REPO_ROOT)

    assert retried["ok"] is False
    assert record_paths["output"].read_bytes() == old_output
    assert record_paths["done"].read_bytes() == old_done
    assert not record_paths["backupOutput"].exists()
    assert not record_paths["backupDone"].exists()


def test_failed_promotion_marker_owns_missing_backup_targets(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    old_output = record_paths["output"].read_bytes()
    old_done = record_paths["done"].read_bytes()
    original_promote = record_module._promote_record

    def fail_after_backups(current_paths, journal):
        del journal
        os.replace(current_paths["output"], current_paths["backupOutput"])
        os.replace(current_paths["done"], current_paths["backupDone"])
        raise OSError("simulated promotion and rollback failure")

    monkeypatch.setattr(record_module, "_promote_record", fail_after_backups)
    failed = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)
    marker = json.loads(record_paths["result"].read_text(encoding="utf-8"))
    assert failed["ok"] is False
    assert set(marker["promotion"]) == {"priorStable", "candidateStable"}
    assert str(record_paths["output"]) in marker["ownedArtifacts"]
    assert str(record_paths["done"]) in marker["ownedArtifacts"]

    monkeypatch.setattr(record_module, "_promote_record", original_promote)
    retried = record_case(case, exe, runner=FakeRecordRunner(mode="process-fail"), repo_root=REPO_ROOT)

    assert retried["ok"] is False
    assert record_paths["output"].read_bytes() == old_output
    assert record_paths["done"].read_bytes() == old_done
    assert not record_paths["backupOutput"].exists()
    assert not record_paths["backupDone"].exists()


def test_incomplete_inventory_does_not_persist_retry_blocking_marker(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    body_bytes = case.body_vmd.read_bytes()
    case.body_vmd.unlink()

    failed = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert failed["phase"] == "preflight"
    assert not paths(fixture.parent)["result"].exists()

    case.body_vmd.write_bytes(body_bytes)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    retried = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert retried["ok"] is True


def test_launch_guard_failure_does_not_recover_or_rewrite_existing_artifacts(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    prior_result = record_paths["result"].read_bytes()
    os.replace(record_paths["output"], record_paths["backupOutput"])
    os.replace(record_paths["done"], record_paths["backupDone"])
    monkeypatch.delenv("MMD_DUMPER_ALLOW_MMD_LAUNCH")

    rejected = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert rejected["phases"]["launchGuard"]["status"] == "fail"
    assert record_paths["result"].read_bytes() == prior_result
    assert record_paths["backupOutput"].is_file()
    assert record_paths["backupDone"].is_file()
    assert not record_paths["output"].exists()
    assert not record_paths["done"].exists()


def test_launch_guard_failure_does_not_claim_foreign_output(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)
    record_paths["output"].write_text("foreign\n", encoding="utf-8")
    monkeypatch.delenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", raising=False)

    rejected = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert rejected["phases"]["launchGuard"]["status"] == "fail"
    assert record_paths["output"].read_text(encoding="utf-8") == "foreign\n"
    assert not record_paths["result"].exists()


def test_completed_promotion_backups_are_discarded_on_retry(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    assert record_paths["backupOutput"].is_file()
    assert record_paths["backupDone"].is_file()

    retried = record_case(case, exe, runner=FakeRecordRunner(mode="process-fail"), repo_root=REPO_ROOT)

    assert retried["ok"] is False
    assert record_paths["output"].is_file()
    assert record_paths["done"].is_file()
    assert not record_paths["backupOutput"].exists()
    assert not record_paths["backupDone"].exists()


def test_completed_promotion_restores_backup_when_stable_dump_is_missing(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    record_paths["output"].unlink()

    recover_interrupted_record_artifacts(record_paths, case, _input_inventory(case))

    assert valid_done(record_paths["done"], record_paths["output"])[0] is True
    assert not record_paths["backupOutput"].exists()
    assert not record_paths["backupDone"].exists()


def test_result_persist_failure_rolls_back_new_stable_dump(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    original_persist = record_module._persist_result
    persist_calls = 0

    def fail_final_persist(result, record_paths):
        nonlocal persist_calls
        persist_calls += 1
        if persist_calls == 2:
            raise OSError("simulated final persist failure")
        return original_persist(result, record_paths)

    monkeypatch.setattr(record_module, "_persist_result", fail_final_persist)
    result = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)
    record_paths = paths(fixture.parent)
    journal = json.loads(record_paths["result"].read_text(encoding="utf-8"))

    assert result["ok"] is False
    assert result["recorded"] is False
    assert persist_calls == 2
    assert journal["phase"] == "promotion"
    assert not record_paths["output"].exists()
    assert not record_paths["done"].exists()
    assert not record_paths["backupOutput"].exists()
    assert not record_paths["backupDone"].exists()


@pytest.mark.parametrize("interrupt_after_stage", [1, 3])
def test_retry_recovers_actual_interrupted_promotion_stages(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, interrupt_after_stage: int
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    old_output = record_paths["output"].read_bytes()
    old_done = record_paths["done"].read_bytes()
    original_replace = record_artifacts_module.os.replace
    promotion_pairs = {
        (record_paths["output"].resolve(), record_paths["backupOutput"].resolve()),
        (record_paths["done"].resolve(), record_paths["backupDone"].resolve()),
        (record_paths["outputTemp"].resolve(), record_paths["output"].resolve()),
        (record_paths["doneTemp"].resolve(), record_paths["done"].resolve()),
    }
    promotion_stages = 0

    def interrupt_after_replace(source, destination):
        nonlocal promotion_stages
        pair = (Path(source).resolve(), Path(destination).resolve())
        original_replace(source, destination)
        if pair in promotion_pairs:
            promotion_stages += 1
            if promotion_stages == interrupt_after_stage:
                raise KeyboardInterrupt(f"simulated promotion interruption at stage {interrupt_after_stage}")

    monkeypatch.setattr(record_artifacts_module.os, "replace", interrupt_after_replace)
    with pytest.raises(KeyboardInterrupt, match="simulated promotion interruption"):
        record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    journal = json.loads(record_paths["result"].read_text(encoding="utf-8"))
    assert journal["phase"] == "promotion"
    monkeypatch.setattr(record_artifacts_module.os, "replace", original_replace)

    retried = record_case(case, exe, runner=FakeRecordRunner(mode="process-fail"), repo_root=REPO_ROOT)

    assert retried["ok"] is False
    assert record_paths["output"].read_bytes() == old_output
    assert record_paths["done"].read_bytes() == old_done
    assert not record_paths["backupOutput"].exists()
    assert not record_paths["backupDone"].exists()


def test_first_record_retry_recovers_crash_after_promotion_before_final_result(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    original_persist = record_module._persist_result
    persist_calls = 0

    def crash_before_final_result(result, record_paths):
        nonlocal persist_calls
        persist_calls += 1
        if persist_calls == 2:
            raise KeyboardInterrupt("simulated process termination")
        return original_persist(result, record_paths)

    monkeypatch.setattr(record_module, "_persist_result", crash_before_final_result)

    with pytest.raises(KeyboardInterrupt, match="simulated process termination"):
        record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    record_paths = paths(fixture.parent)
    journal = json.loads(record_paths["result"].read_text(encoding="utf-8"))
    assert journal["phase"] == "promotion"
    assert record_paths["output"].is_file()
    assert record_paths["done"].is_file()

    retried = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert retried["ok"] is True
    assert persist_calls == 4
    assert not record_paths["backupOutput"].exists()
    assert not record_paths["backupDone"].exists()


def test_first_record_recovery_preserves_foreign_stable_target(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    original_persist = record_module._persist_result
    persist_calls = 0

    def crash_before_final_result(result, record_paths):
        nonlocal persist_calls
        persist_calls += 1
        if persist_calls == 2:
            raise KeyboardInterrupt("simulated process termination")
        return original_persist(result, record_paths)

    monkeypatch.setattr(record_module, "_persist_result", crash_before_final_result)
    with pytest.raises(KeyboardInterrupt):
        record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    record_paths = paths(fixture.parent)
    record_paths["output"].write_bytes(b"foreign stable target\n")
    runner = FakeRecordRunner()

    retried = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert retried["ok"] is False
    assert retried["phase"] == "preflight"
    assert runner.calls == []
    assert record_paths["output"].read_bytes() == b"foreign stable target\n"
    assert any("promoted candidate" in error["message"] for error in retried["errors"])


def test_persist_failure_refresh_error_remains_structured(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, _ = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    original_refresh = record_module._refresh_artifacts
    refresh_calls = 0

    def fail_persist(result, record_paths):
        del result, record_paths
        raise OSError("simulated persist failure")

    def fail_second_refresh(result):
        nonlocal refresh_calls
        refresh_calls += 1
        if refresh_calls == 2:
            raise OSError("simulated unsafe artifact")
        original_refresh(result)

    monkeypatch.setattr(record_module, "_persist_result", fail_persist)
    monkeypatch.setattr(record_module, "_refresh_artifacts", fail_second_refresh)

    result = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert result["ok"] is False
    assert any("cannot write record-result" in error["message"] for error in result["errors"])
    assert any("cannot inspect record artifacts after persist failure" in error["message"] for error in result["errors"])
