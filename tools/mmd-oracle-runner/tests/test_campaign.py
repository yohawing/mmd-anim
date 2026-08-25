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


def _compare_result(case_id: str, *, mismatch: int = 0) -> CommandResult:
    payload = {"perCase": [{"name": case_id, "status": "ok" if mismatch == 0 else "mismatch", "mismatchCount": mismatch, "comparedFrames": 2, "comparedBones": 1, **{metric: 0.01 for metric in campaign_module._METRIC_NAMES}}]}
    return CommandResult(("verify",), Path.cwd(), 0, json.dumps(payload), "")


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


def test_manifest_accepts_shared_frames_output_and_nested_asset_paths(tmp_path: Path) -> None:
    manifest = _manifest(tmp_path)
    payload = json.loads(manifest.read_text(encoding="utf-8"))
    case = payload["cases"][0]
    case["pmx"] = {"path": case["pmx"]}
    case["bodyVmd"] = {"path": case["bodyVmd"]}
    payload["frames"] = case.pop("frames")
    payload["outputRoot"] = case.pop("outputRoot")
    case.pop("features")
    case.pop("categories")
    manifest.write_text(json.dumps(payload), encoding="utf-8")
    config = load_campaign_config(manifest.resolve())
    assert config.cases[0].frames == (0, 15)
    assert config.cases[0].oracle_case.output_root == Path(payload["outputRoot"]).resolve()
    assert config.cases[0].features == ()


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
