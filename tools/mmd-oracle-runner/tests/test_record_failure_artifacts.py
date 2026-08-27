from __future__ import annotations

import json
import os
import zipfile
from pathlib import Path

import pytest

import mmd_oracle_runner.record as record_module
from mmd_oracle_runner.record import record_case
from mmd_oracle_runner.prepare import CommandResult, _input_inventory
from mmd_oracle_runner.record_artifacts import (
    FAILURE_ARTIFACT_KEYS,
    LEGACY_TEMPORARY_ARTIFACT_KEYS,
    TEMPORARY_ARTIFACT_KEYS,
    paths,
    write_attempt_marker,
)
from prepare_test_support import REPO_ROOT
from test_record import FakeRecordRunner, _prepared_case


def _bundle_entries(path: Path) -> dict[str, bytes]:
    with zipfile.ZipFile(path) as archive:
        return {name: archive.read(name) for name in archive.namelist()}


class NoEvidenceRunner:
    def run(self, command, cwd):
        return CommandResult(tuple(str(part) for part in command), Path(cwd), 7, "", "MMD failed before output")


def test_failed_attempt_retains_raw_evidence_without_replacing_stable_dump(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)["ok"] is True
    record_paths = paths(fixture.parent)
    stable_output = record_paths["output"].read_bytes()
    stable_done = record_paths["done"].read_bytes()

    failed = record_case(case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT)
    marker = json.loads(record_paths["result"].read_text(encoding="utf-8"))

    assert failed["ok"] is False
    assert record_paths["output"].read_bytes() == stable_output
    assert record_paths["done"].read_bytes() == stable_done
    bundle = _bundle_entries(record_paths["failureBundle"])
    assert bundle["oracle.actual.jsonl"].splitlines() == [b"{}"]
    assert json.loads(bundle["oracle.actual.jsonl.done"])["ok"] is True
    assert bundle["proxy.log"].splitlines() == [b"native proxy log"]
    assert all(str(record_paths[key]) in marker["ownedArtifacts"] for key in FAILURE_ARTIFACT_KEYS)
    assert all(marker["artifacts"][key]["exists"] is True for key in FAILURE_ARTIFACT_KEYS)
    assert record_paths["attempt"].is_file()


def test_later_failure_removes_stale_evidence_missing_from_current_attempt(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(
        case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT
    )["ok"] is False
    record_paths = paths(fixture.parent)
    assert all(record_paths[key].is_file() for key in FAILURE_ARTIFACT_KEYS)

    failed = record_case(case, exe, runner=FakeRecordRunner(mode="process-fail"), repo_root=REPO_ROOT)

    assert failed["ok"] is False
    bundle = _bundle_entries(record_paths["failureBundle"])
    assert set(bundle) == {"proxy.log"}
    assert bundle["proxy.log"].splitlines() == [b"native proxy log"]


def test_failure_without_raw_evidence_removes_prior_bundle(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(
        case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT
    )["ok"] is False
    record_paths = paths(fixture.parent)
    assert record_paths["failureBundle"].is_file()

    failed = record_case(case, exe, runner=NoEvidenceRunner(), repo_root=REPO_ROOT)

    assert failed["ok"] is False
    assert not record_paths["failureBundle"].exists()


def test_successful_retry_preserves_prior_failure_evidence(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(
        case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT
    )["ok"] is False
    record_paths = paths(fixture.parent)

    succeeded = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert succeeded["ok"] is True
    assert all(record_paths[key].is_file() for key in FAILURE_ARTIFACT_KEYS)
    assert all(str(record_paths[key]) in succeeded["ownedArtifacts"] for key in FAILURE_ARTIFACT_KEYS)
    assert not record_paths["attempt"].exists()


def test_success_final_persist_failure_preserves_prior_failure_evidence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    assert record_case(
        case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT
    )["ok"] is False
    record_paths = paths(fixture.parent)
    prior_failure = {key: record_paths[key].read_bytes() for key in FAILURE_ARTIFACT_KEYS}
    original_persist = record_module._persist_result
    persist_calls = 0

    def fail_final_persist(result, current_paths):
        nonlocal persist_calls
        persist_calls += 1
        if persist_calls == 2:
            raise OSError("simulated final persist failure")
        return original_persist(result, current_paths)

    monkeypatch.setattr(record_module, "_persist_result", fail_final_persist)
    retried = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert retried["ok"] is False
    assert retried["recorded"] is False
    assert persist_calls == 2
    assert {key: record_paths[key].read_bytes() for key in FAILURE_ARTIFACT_KEYS} == prior_failure


def test_interrupted_promotion_does_not_retain_done_without_failure_output(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    def interrupt_after_output(current_paths, journal):
        del journal
        os.replace(current_paths["outputTemp"], current_paths["output"])
        raise KeyboardInterrupt("simulated promotion interruption")

    monkeypatch.setattr(record_module, "_promote_record", interrupt_after_output)
    with pytest.raises(KeyboardInterrupt, match="simulated promotion interruption"):
        record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)
    record_paths = paths(fixture.parent)

    bundle = _bundle_entries(record_paths["failureBundle"])
    assert set(bundle) == {"proxy.log"}
    assert bundle["proxy.log"].splitlines() == [b"native proxy log"]


def test_result_persist_failure_keeps_retryable_attempt_ownership(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    original_persist = record_module._persist_result

    def fail_persist(result, record_paths):
        del result, record_paths
        raise OSError("simulated result persist failure")

    monkeypatch.setattr(record_module, "_persist_result", fail_persist)
    failed = record_case(case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT)
    record_paths = paths(fixture.parent)
    attempt = json.loads(record_paths["attempt"].read_text(encoding="utf-8"))

    assert failed["ok"] is False
    assert not record_paths["result"].exists()
    assert all(str(record_paths[key]) in attempt["ownedArtifacts"] for key in FAILURE_ARTIFACT_KEYS)

    original_write_fixture = record_module._write_temp_fixture
    saw_existing_attempt = False
    observed_attempt_mmd_size = None

    def assert_attempt_before_fixture(*args, **kwargs):
        nonlocal saw_existing_attempt, observed_attempt_mmd_size
        saw_existing_attempt = record_paths["attempt"].is_file()
        observed_attempt_mmd_size = json.loads(record_paths["attempt"].read_text(encoding="utf-8"))[
            "mmdExecutable"
        ]["size"]
        return original_write_fixture(*args, **kwargs)

    monkeypatch.setattr(record_module, "_persist_result", original_persist)
    monkeypatch.setattr(record_module, "_write_temp_fixture", assert_attempt_before_fixture)
    exe.write_bytes(b"updated fake exe")
    retried = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert retried["ok"] is True
    assert saw_existing_attempt is True
    assert observed_attempt_mmd_size == len(b"updated fake exe")
    assert json.loads(record_paths["result"].read_text(encoding="utf-8"))["mmdExecutable"]["size"] == len(
        b"updated fake exe"
    )
    assert all(record_paths[key].is_file() for key in FAILURE_ARTIFACT_KEYS)
    assert not record_paths["attempt"].exists()


def test_retention_failure_keeps_attempt_owned_temporary_evidence(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    original_retain = record_module._retain_failure_artifacts

    def fail_retain(record_paths, owned_paths):
        del record_paths, owned_paths
        raise OSError("simulated retention failure")

    monkeypatch.setattr(record_module, "_retain_failure_artifacts", fail_retain)
    failed = record_case(case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT)
    record_paths = paths(fixture.parent)

    assert failed["ok"] is False
    assert record_paths["attempt"].is_file()
    assert all(record_paths[key].is_file() for key in LEGACY_TEMPORARY_ARTIFACT_KEYS)

    monkeypatch.setattr(record_module, "_retain_failure_artifacts", original_retain)
    retried = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert retried["ok"] is True
    assert all(not record_paths[key].exists() for key in TEMPORARY_ARTIFACT_KEYS)
    assert not record_paths["attempt"].exists()


def test_legacy_attempt_marker_remains_retryable(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    record_paths = paths(fixture.parent)
    record_paths["outputTemp"].write_text("interrupted output\n", encoding="utf-8")
    write_attempt_marker(record_paths, case, _input_inventory(case), {"path": str(exe)})
    attempt = json.loads(record_paths["attempt"].read_text(encoding="utf-8"))
    legacy_paths = {
        str(record_paths["attempt"]),
        *(str(record_paths[key]) for key in LEGACY_TEMPORARY_ARTIFACT_KEYS),
    }
    attempt["ownedArtifacts"] = [path for path in attempt["ownedArtifacts"] if path in legacy_paths]
    record_paths["attempt"].write_text(json.dumps(attempt), encoding="utf-8")
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")

    retried = record_case(case, exe, runner=FakeRecordRunner(), repo_root=REPO_ROOT)

    assert retried["ok"] is True
    assert not record_paths["outputTemp"].exists()
    assert not record_paths["attempt"].exists()


def test_foreign_failure_artifact_is_preserved_and_blocks_launch(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    record_paths = paths(fixture.parent)
    record_paths["failureBundle"].write_text("foreign evidence\n", encoding="utf-8")
    runner = FakeRecordRunner()

    rejected = record_case(case, exe, runner=runner, repo_root=REPO_ROOT)

    assert rejected["ok"] is False
    assert rejected["phase"] == "preflight"
    assert runner.calls == []
    assert record_paths["failureBundle"].read_text(encoding="utf-8") == "foreign evidence\n"


def test_failure_retention_can_be_disabled_without_raw_duplicates(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    record_paths = paths(fixture.parent)

    failed = record_case(
        case,
        exe,
        runner=FakeRecordRunner(mode="process-schema-fail"),
        repo_root=REPO_ROOT,
        retain_failure_artifacts=False,
    )

    assert failed["ok"] is False
    assert not record_paths["failureBundle"].exists()
    assert all(not record_paths[key].exists() for key in TEMPORARY_ARTIFACT_KEYS)
    assert record_paths["attempt"].is_file()


def test_disabled_failure_retention_removes_only_prior_owned_bundle(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    record_paths = paths(fixture.parent)
    assert record_case(
        case, exe, runner=FakeRecordRunner(mode="process-schema-fail"), repo_root=REPO_ROOT
    )["ok"] is False
    assert record_paths["failureBundle"].is_file()

    failed = record_case(
        case,
        exe,
        runner=FakeRecordRunner(mode="process-fail"),
        repo_root=REPO_ROOT,
        retain_failure_artifacts=False,
    )

    assert failed["ok"] is False
    assert not record_paths["failureBundle"].exists()


def test_disabled_failure_retention_preserves_foreign_bundle(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    case, exe, fixture = _prepared_case(tmp_path)
    monkeypatch.setenv("MMD_DUMPER_ALLOW_MMD_LAUNCH", "1")
    record_paths = paths(fixture.parent)
    record_paths["failureBundle"].write_text("foreign evidence\n", encoding="utf-8")

    rejected = record_case(
        case,
        exe,
        runner=FakeRecordRunner(),
        repo_root=REPO_ROOT,
        retain_failure_artifacts=False,
    )

    assert rejected["ok"] is False
    assert rejected["phase"] == "preflight"
    assert record_paths["failureBundle"].read_text(encoding="utf-8") == "foreign evidence\n"
