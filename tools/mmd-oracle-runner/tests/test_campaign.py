from __future__ import annotations

import json
from pathlib import Path

import pytest

import mmd_oracle_runner.campaign as campaign_module
from mmd_oracle_runner.campaign import load_campaign_config, run_campaign
from mmd_oracle_runner.prepare import CommandResult


def _manifest(tmp_path: Path, *, case_count: int = 1) -> Path:
    cases = []
    runs = tmp_path / "runs"
    runs.mkdir()
    for index in range(case_count):
        pmx = tmp_path / f"model-{index}.pmx"
        vmd = tmp_path / f"motion-{index}.vmd"
        pmx.write_bytes(b"pmx" + bytes([index]))
        vmd.write_bytes(b"vmd" + bytes([index]))
        cases.append({
            "caseId": f"case-{index}",
            "pmx": str(pmx.resolve()),
            "bodyVmd": str(vmd.resolve()),
            "frames": [0, 15],
            "outputRoot": str(runs.resolve()),
            "features": ["bone"],
            "categories": ["pilot"],
        })
    manifest = tmp_path / "campaign.json"
    manifest.write_text(json.dumps({
        "schemaVersion": 1,
        "run": {"mmdVersion": "9.32-x64", "dumperVersion": "test", "timestamp": "2026-08-25T00:00:00Z", "samplingPolicy": "fixed-local-v1"},
        "discovered": case_count,
        "compare": {"focusBones": ["センター"], "thresholds": {name: 0.1 for name in campaign_module._METRIC_NAMES}},
        "cases": cases,
    }), encoding="utf-8")
    return manifest


def _compare_result(case_id: str, *, mismatch: int = 0, no_targets: bool = False) -> CommandResult:
    payload = {"perCase": [{"name": case_id, "status": "ok" if mismatch == 0 else "mismatch", "mismatchCount": mismatch, "comparedFrames": 0 if no_targets else 2, "comparedBones": 0 if no_targets else 1, **{metric: 0.01 for metric in campaign_module._METRIC_NAMES}}]}
    return CommandResult(("verify",), Path.cwd(), 0, json.dumps(payload), "")


def _prepared(case: object, keyframes: dict[str, int]) -> dict[str, object]:
    run_dir = case.output_root / case.name
    run_dir.mkdir(exist_ok=True)
    (run_dir / "model.mmd-utf16.pmx").write_bytes(b"model")
    return {"ok": True, "phase": "complete", "comparison": {"generatorReport": {"keyframes": keyframes}}}


def _recorded(case: object) -> dict[str, object]:
    run_dir = case.output_root / case.name
    (run_dir / "oracle.actual.jsonl").write_text("{}\n", encoding="utf-8")
    return {"ok": True, "recorded": True}


def _run_case(tmp_path: Path, monkeypatch: pytest.MonkeyPatch, *, keyframes: object, expectation: str | None = None, case_count: int = 1, compare_result: CommandResult | None = None) -> tuple[dict[str, object], dict[str, object], list[str], list[str]]:
    manifest = _manifest(tmp_path, case_count=case_count)
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    if expectation is not None:
        payload["cases"][0]["expectation"] = expectation
    manifest.write_text(json.dumps(payload), encoding="utf-8")
    snapshot = tmp_path / "snapshot.json"
    monkeypatch.setattr(campaign_module, "_probe_repository", lambda _: {"commitSha": "a" * 40, "repositoryState": "clean"})
    config = load_campaign_config(manifest.resolve())
    record_calls: list[str] = []
    compare_calls: list[str] = []

    def prepare(case: object) -> dict[str, object]:
        selected = keyframes(case.name) if callable(keyframes) else keyframes
        return _prepared(case, selected)

    def record(case: object, *_: object) -> dict[str, object]:
        record_calls.append(case.name)
        return _recorded(case)

    def compare(*_: object) -> CommandResult:
        compare_calls.append("compare")
        return compare_result or _compare_result(config.cases[-1].case_id)

    cleanup = lambda run_dir: {"ok": True, "removedRunDir": True, "deleted": []}
    result = run_campaign(manifest, snapshot, prepare_action=prepare, record_action=record, compare_action=compare, cleanup_action=cleanup, prepare_cleanup_action=cleanup, repo_root=tmp_path)
    saved = json.loads(snapshot.read_text(encoding="utf-8"))
    return result, saved, record_calls, compare_calls


def test_manifest_loads_direct_cases_and_hashes_content(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path, case_count=2)
    config = load_campaign_config(manifest.resolve())
    assert len(config.cases) == 2
    assert config.cases[0].oracle_case.pmx.name == "model-0.pmx"
    assert len(config.config_hash) == 64


def test_duplicate_case_ids_are_rejected(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path, case_count=2)
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    payload["cases"][1]["caseId"] = payload["cases"][0]["caseId"]
    manifest.write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(campaign_module.CampaignValidationError, match="duplicate caseId"):
        load_campaign_config(manifest.resolve())


def test_unknown_expectation_is_rejected(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path)
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    payload["cases"][0]["expectation"] = "maybe-motion"
    manifest.write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(campaign_module.CampaignValidationError, match="expectation"):
        load_campaign_config(manifest.resolve())


def test_manifest_accepts_shared_frames_output_and_nested_asset_paths(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path)
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    case = payload["cases"][0]
    case["pmx"] = {"path": case["pmx"], "relativePath": "models/model-0.pmx"}
    case["bodyVmd"] = {"path": case["bodyVmd"], "relativePath": "motions/motion-0.vmd"}
    payload["frames"] = case.pop("frames")
    payload["outputRoot"] = case.pop("outputRoot")
    case.pop("features")
    case.pop("categories")
    manifest.write_text(json.dumps(payload), encoding="utf-8")
    config = load_campaign_config(manifest.resolve())
    assert config.cases[0].frames == (0, 15)
    assert config.cases[0].oracle_case.output_root == Path(payload["outputRoot"]).resolve()
    assert config.cases[0].features == ()
    assert config.cases[0].model_label == "models/model-0.pmx"
    assert config.cases[0].motion_label == "motions/motion-0.vmd"


def test_manifest_applies_dialog_opt_in_default(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path)
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    payload["dialogOptIn"] = True
    manifest.write_text(json.dumps(payload), encoding="utf-8")

    config = load_campaign_config(manifest.resolve())

    assert config.cases[0].oracle_case.dialog_opt_in is True


def test_campaign_writes_snapshot_and_resumes_clean_state(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    manifest = _manifest(tmp_path)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    monkeypatch.setattr(campaign_module, "_probe_repository", lambda _: {"commitSha": "a" * 40, "repositoryState": "clean"})
    config = load_campaign_config(manifest.resolve())
    case = config.cases[0].oracle_case

    def prepare(_: object) -> dict[str, object]:
        run_dir = case.output_root / case.name
        run_dir.mkdir(exist_ok=True)
        (run_dir / "model.mmd-utf16.pmx").write_bytes(b"model")
        return {"ok": True, "phase": "complete"}

    def record(_: object, __: object) -> dict[str, object]:
        run_dir = case.output_root / case.name
        (run_dir / "oracle.actual.jsonl").write_text("{}\n", encoding="utf-8")
        return {"ok": True, "recorded": True}

    cleanup = lambda run_dir: {"ok": True, "removedRunDir": True, "deleted": []}
    result = run_campaign(manifest, snapshot, state, prepare_action=prepare, record_action=record, compare_action=lambda *_: _compare_result(case.name), cleanup_action=cleanup, prepare_cleanup_action=cleanup, repo_root=tmp_path)
    assert result["snapshotWritten"] is True
    assert result["ok"] is True
    resumed = run_campaign(manifest, snapshot, state, prepare_action=prepare, record_action=record, compare_action=lambda *_: _compare_result(case.name), cleanup_action=cleanup, prepare_cleanup_action=cleanup, repo_root=tmp_path)
    assert resumed["snapshotWritten"] is True
    assert resumed["casesProcessed"] == 0


def test_campaign_resumes_after_interrupt_before_prepare_marker(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    manifest = _manifest(tmp_path)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    monkeypatch.setattr(campaign_module, "_probe_repository", lambda _: {"commitSha": "a" * 40, "repositoryState": "clean"})
    case = load_campaign_config(manifest.resolve()).cases[0].oracle_case
    run_dir = case.output_root / case.name

    def interrupt(_: object) -> dict[str, object]:
        run_dir.mkdir()
        raise KeyboardInterrupt

    with pytest.raises(KeyboardInterrupt):
        run_campaign(manifest, snapshot, state, prepare_action=interrupt, repo_root=tmp_path)

    def prepare(_: object) -> dict[str, object]:
        assert not run_dir.exists()
        return _prepared(case, {"bone": 1, "frame0Bones": 0, "morph": 0, "frame0Morphs": 0})

    cleanup = lambda _: {"ok": True, "removedRunDir": True, "deleted": []}
    result = run_campaign(
        manifest, snapshot, state,
        prepare_action=prepare,
        record_action=lambda current, *_: _recorded(current),
        compare_action=lambda *_: _compare_result(case.name),
        cleanup_action=cleanup,
        prepare_cleanup_action=cleanup,
        repo_root=tmp_path,
    )
    assert result["ok"] is True


def test_campaign_rejects_mixed_mmd_executable_hashes(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    manifest = _manifest(tmp_path, case_count=2)
    snapshot = tmp_path / "snapshot.json"
    monkeypatch.setattr(campaign_module, "_probe_repository", lambda _: {"commitSha": "a" * 40, "repositoryState": "clean"})

    def prepare(case: object) -> dict[str, object]:
        return _prepared(case, {"bone": 1, "frame0Bones": 0, "morph": 0, "frame0Morphs": 0})

    def record(case: object, *_: object) -> dict[str, object]:
        result = _recorded(case)
        result["mmdExecutable"] = {"sha256": ("a" if case.name == "case-0" else "b") * 64}
        return result

    cleanup = lambda _: {"ok": True, "removedRunDir": True, "deleted": []}
    result = run_campaign(
        manifest, snapshot,
        prepare_action=prepare,
        record_action=record,
        compare_action=lambda path, *_: _compare_result(json.loads(path.read_text(encoding="utf-8"))["cases"][0]["name"]),
        cleanup_action=cleanup,
        prepare_cleanup_action=cleanup,
        repo_root=tmp_path,
    )
    assert result["snapshotWritten"] is False
    assert result["error"]["code"] == "mmd-executable"
    assert not snapshot.exists()


def test_campaign_records_compare_failure_without_raw_retention(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    manifest = _manifest(tmp_path)
    snapshot = tmp_path / "snapshot.json"
    monkeypatch.setattr(campaign_module, "_probe_repository", lambda _: {"commitSha": "b" * 40, "repositoryState": "clean"})
    config = load_campaign_config(manifest.resolve())
    case = config.cases[0].oracle_case

    def prepare(_: object) -> dict[str, object]:
        run_dir = case.output_root / case.name
        run_dir.mkdir(exist_ok=True)
        (run_dir / "model.mmd-utf16.pmx").write_bytes(b"model")
        return {"ok": True, "phase": "complete"}

    def record(_: object, __: object) -> dict[str, object]:
        run_dir = case.output_root / case.name
        (run_dir / "oracle.actual.jsonl").write_text("{}\n", encoding="utf-8")
        return {"ok": True, "recorded": True}

    cleanup = lambda run_dir: {"ok": True, "removedRunDir": True, "deleted": []}
    result = run_campaign(manifest, snapshot, prepare_action=prepare, record_action=record, compare_action=lambda *_: _compare_result(case.name, mismatch=1), cleanup_action=cleanup, prepare_cleanup_action=cleanup, repo_root=tmp_path)
    assert result["snapshotWritten"] is True
    assert result["ok"] is False
    assert json.loads(snapshot.read_text(encoding="utf-8"))["funnel"]["compared"] == 1
    assert json.loads(snapshot.read_text(encoding="utf-8"))["cases"][0] == {
        "caseId": "case-0",
        "model": "model-0.pmx",
        "motion": "motion-0.vmd",
        "expectation": "numeric-parity",
        "result": "threshold-fail",
        "failures": ["threshold"],
    }


def test_no_compatible_motion_requires_zero_written_keyframes_and_recording(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    result, saved, record_calls, compare_calls = _run_case(
        tmp_path, monkeypatch, expectation="no-compatible-motion",
        keyframes={"bone": 0, "frame0Bones": 0, "morph": 0, "frame0Morphs": 0},
    )
    assert result["ok"] is True
    assert record_calls == ["case-0"] and compare_calls == []
    assert saved["funnel"]["compared"] == saved["funnel"]["passed"] == 0
    assert saved["cases"][0]["result"] == "applicability-pass"


@pytest.mark.parametrize(
    ("keyframes", "failure"),
    [({"bone": 1, "frame0Bones": 0, "morph": 0, "frame0Morphs": 0}, "unexpected-motion-applied"), ({"bone": 0}, "applicability-evidence")],
)
def test_no_compatible_motion_rejects_nonzero_or_malformed_evidence(tmp_path: Path, monkeypatch: pytest.MonkeyPatch, keyframes: dict[str, int], failure: str) -> None:
    result, saved, record_calls, compare_calls = _run_case(tmp_path, monkeypatch, expectation="no-compatible-motion", keyframes=keyframes)
    assert record_calls == [] and compare_calls == []
    assert saved["cases"][0]["result"] == "applicability-fail"
    assert saved["cases"][0]["failures"] == [failure]


def test_numeric_no_targets_remains_a_failure(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    result, saved, record_calls, compare_calls = _run_case(
        tmp_path, monkeypatch, keyframes={"bone": 1, "frame0Bones": 0, "morph": 0, "frame0Morphs": 0},
        compare_result=_compare_result("case-0", no_targets=True),
    )
    assert record_calls == ["case-0"] and compare_calls == ["compare"]
    assert saved["cases"][0]["result"] == "compare-fail"
    assert saved["cases"][0]["failures"] == ["compare-no-targets"]


def test_mixed_numeric_and_applicability_cases_are_separated(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    result, saved, _, compare_calls = _run_case(
        tmp_path, monkeypatch, case_count=2,
        expectation="no-compatible-motion",
        keyframes=lambda name: {"bone": 0, "frame0Bones": 0, "morph": 0, "frame0Morphs": 0} if name == "case-0" else {"bone": 1, "frame0Bones": 0, "morph": 0, "frame0Morphs": 0},
        compare_result=_compare_result("case-1"),
    )
    assert compare_calls == ["compare"]
    assert saved["funnel"]["compared"] == saved["funnel"]["passed"] == 1
    assert [case["result"] for case in saved["cases"]] == ["applicability-pass", "pass"]
