from __future__ import annotations

import json
from pathlib import Path

import pytest

import mmd_oracle_runner.campaign as campaign_module
from mmd_oracle_runner.case import load_case
from mmd_oracle_runner.prepare import CommandResult
from mmd_oracle_runner.campaign import (
    CampaignValidationError,
    _parse_compare_result,
    _write_numeric_manifest,
    load_campaign_config,
    run_campaign,
)
from mmd_oracle_runner.report import load_snapshot
from mmd_oracle_runner.selection import _selection_hash


@pytest.fixture(autouse=True)
def _clean_repository_probe(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(campaign_module, "_probe_repository", lambda _: {"commitSha": "a" * 40, "repositoryState": "clean"})


def _make_config(tmp_path: Path, *, case_count: int = 2) -> tuple[Path, list[Path], list[Path]]:
    cases: list[Path] = []
    pmx_paths: list[Path] = []
    for index in range(case_count):
        pmx = tmp_path / f"model-{index}.pmx"
        vmd = tmp_path / f"motion-{index}.vmd"
        pmx.write_bytes(f"pmx-{index}".encode())
        vmd.write_bytes(f"vmd-{index}".encode())
        case_path = tmp_path / f"case-{index}.json"
        case_path.write_text(
            json.dumps(
                {
                    "schemaVersion": 1,
                    "name": f"case-{index}",
                    "input": {"pmx": str(pmx), "bodyVmd": str(vmd)},
                    "frames": [0, 15],
                    "outputRoot": str(tmp_path / "runs" / str(index)),
                    "generatorBackend": "python-rust",
                    "recordOptIn": True,
                    "dialogOptIn": False,
                    "requestedFeatures": [],
                }
            ),
            encoding="utf-8",
        )
        cases.append(case_path)
        pmx_paths.append(pmx)
    config = tmp_path / "campaign.json"
    selection = {
        "schemaVersion": 1,
        "seed": "test-v1",
        "requestedCases": case_count,
        "frames": [0, 15],
        "roots": {"pmx": str(tmp_path), "vmd": str(tmp_path)},
        "policy": {
            "pmxExtension": ".pmx",
            "vmdExtension": ".vmd",
            "maxPmxBytes": 1024,
            "maxVmdBytes": 1024,
            "requireBodyBoneFrames": True,
            "ordering": "sha256(seed-kind-relative-path-v1)",
        },
        "discovery": {
            "pmxFiles": case_count,
            "vmdFiles": case_count,
            "eligiblePmx": case_count,
            "eligibleBodyVmd": case_count,
            "selected": case_count,
        },
        "cases": [
            {
                "caseId": f"case-{index}",
                "pmx": {
                    "path": str(pmx_paths[index]),
                    "relativePath": pmx_paths[index].name,
                    "size": pmx_paths[index].stat().st_size,
                    "sha256": campaign_module._sha256_file(pmx_paths[index]),
                },
                "bodyVmd": {
                    "path": str(tmp_path / f"motion-{index}.vmd"),
                    "relativePath": f"motion-{index}.vmd",
                    "size": (tmp_path / f"motion-{index}.vmd").stat().st_size,
                    "sha256": campaign_module._sha256_file(tmp_path / f"motion-{index}.vmd"),
                },
                "features": ["bone"],
                "categories": ["pilot"],
            }
            for index in range(case_count)
        ],
    }
    selection["selectionHash"] = _selection_hash(selection)
    selection_path = tmp_path / "selection.json"
    selection_path.write_text(json.dumps(selection), encoding="utf-8")
    config.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "selectionFile": str(selection_path),
                "selectionHash": selection["selectionHash"],
                "run": {
                    "mmdVersion": "9.32-x64",
                    "dumperVersion": "test",
                    "timestamp": "2026-08-24T12:00:00Z",
                    "samplingPolicy": "deterministic-test-v1",
                },
                "discovered": case_count,
                "compare": {
                    "focusBones": ["センター"],
                    "thresholds": {
                        "translationMaxError": 0.1,
                        "translationRmsError": 0.1,
                        "rotationMaxAngleRad": 0.1,
                        "rotationRmsAngleRad": 0.1,
                        "maxAbsError": 0.1,
                    },
                },
                "cases": [
                    {
                        "caseFile": str(case_path),
                        "caseId": f"case-{index}",
                        "features": ["bone"],
                        "categories": ["pilot"],
                    }
                    for index, case_path in enumerate(cases)
                ],
            }
        ),
        encoding="utf-8",
    )
    return config, cases, pmx_paths


def _fake_actions(events: list[str], values: list[float] | None = None):
    values = values or [0.01, 0.02]
    state = {"index": 0}

    def prepare(case):
        events.append(f"prepare:{case.name}")
        run_dir = case.output_root / case.name
        run_dir.mkdir(parents=True, exist_ok=True)
        (run_dir / "model.mmd-utf16.pmx").write_bytes(b"staged")
        (run_dir / "oracle.actual.jsonl").write_text("{}\n", encoding="utf-8")
        (run_dir / "prepare-result.json").write_text("{}", encoding="utf-8")
        return {"ok": True, "phase": "complete", "artifactName": case.name}

    def record(case, executable):
        del executable
        events.append(f"record:{case.name}")
        run_dir = case.output_root / case.name
        (run_dir / "record-result.json").write_text("{}", encoding="utf-8")
        return {"ok": True, "recorded": True}

    def compare(manifest, repo_root):
        del repo_root
        payload = json.loads(manifest.read_text(encoding="utf-8"))
        events.append(f"compare:{payload['cases'][0]['name']}")
        value = values[state["index"]]
        state["index"] += 1
        return CommandResult(
            ("fake",),
            manifest.parent,
            0,
            json.dumps(
                {
                    "perCase": [
                        {
                            "name": payload["cases"][0]["name"],
                            "status": "ok",
                            "translationMaxError": value,
                            "translationRmsError": value,
                            "rotationMaxAngleRad": value,
                            "rotationRmsAngleRad": value,
                            "maxAbsError": value,
                            "mismatchCount": 0,
                            "comparedFrames": 2,
                            "comparedBones": 1,
                            "missing": 0,
                            "importErrors": 0,
                            "noTargets": 0,
                            "skippedTargets": [],
                        }
                    ]
                }
            ),
            "",
        )

    def cleanup(run_dir):
        events.append(f"cleanup:{run_dir.name}")
        return {"ok": True, "deleted": [], "removedRunDir": False}

    return prepare, record, compare, cleanup


def test_campaign_is_sequential_and_persists_state_before_cleanup(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    config, _, _ = _make_config(tmp_path)
    snapshot = tmp_path / "QUALITY_REPORT.snapshot.json"
    state = tmp_path / "state.json"
    events: list[str] = []
    prepare, record, compare, cleanup = _fake_actions(events)
    original_persist = campaign_module._persist_state

    def persist(path, value):
        events.append("persist")
        return original_persist(path, value)

    monkeypatch.setattr(campaign_module, "_persist_state", persist)
    result = run_campaign(config, snapshot, state, prepare_action=prepare, record_action=record, compare_action=compare, cleanup_action=cleanup)

    assert result["ok"] is True
    assert events.index("persist") < events.index("cleanup:case-0")
    persist_after_first_cleanup = events.index("persist", events.index("cleanup:case-0") + 1)
    assert events.index("cleanup:case-0") < persist_after_first_cleanup < events.index("prepare:case-1")
    assert events.index("cleanup:case-1") < events.index("persist", events.index("cleanup:case-1") + 1)


def test_production_record_action_is_called_with_failure_retention_disabled(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    config, _, _ = _make_config(tmp_path, case_count=1)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    events: list[str] = []
    prepare, _, compare, cleanup = _fake_actions(events)
    called: list[bool] = []

    def record(case, executable, *, retain_failure_artifacts):
        del executable
        called.append(retain_failure_artifacts)
        (case.output_root / case.name / "record-result.json").write_text("{}", encoding="utf-8")
        return {"ok": True, "recorded": True}

    monkeypatch.setattr(campaign_module, "record_case", record)
    result = run_campaign(config, snapshot, state, prepare_action=prepare, compare_action=compare, cleanup_action=cleanup)

    assert result["ok"] is True
    assert called == [False]


def test_exit_zero_threshold_mismatch_is_failure_and_snapshot_has_no_paths(tmp_path: Path):
    config, cases, _ = _make_config(tmp_path, case_count=1)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    events: list[str] = []
    prepare, record, compare, cleanup = _fake_actions(events, [0.2])

    result = run_campaign(config, snapshot, state, prepare_action=prepare, record_action=record, compare_action=compare, cleanup_action=cleanup)

    assert result["ok"] is False
    assert result["snapshotWritten"] is True
    data = json.loads(snapshot.read_text(encoding="utf-8"))
    assert load_snapshot(snapshot)["schemaVersion"] == 1
    assert data["funnel"]["compared"] == 1
    assert data["funnel"]["passed"] == 0
    assert data["failures"]["threshold"] == 1
    text = snapshot.read_text(encoding="utf-8")
    assert str(cases[0]) not in text
    assert "model.mmd-utf16.pmx" not in text
    assert "oracle.actual.jsonl" not in text


def test_malformed_compare_zero_comparable_emits_red_snapshot(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    events: list[str] = []
    prepare, record, _, cleanup = _fake_actions(events)

    def malformed(manifest, repo_root):
        del manifest, repo_root
        return CommandResult(("fake",), tmp_path, 0, "not-json", "")

    result = run_campaign(config, snapshot, state, prepare_action=prepare, record_action=record, compare_action=malformed, cleanup_action=cleanup)

    assert result["ok"] is False
    assert result["error"]["code"] == "zero-comparable"
    assert snapshot.exists()
    data = json.loads(snapshot.read_text(encoding="utf-8"))
    assert data["funnel"]["compared"] == 0
    assert data["metrics"] == {}
    assert "No comparable cases" in campaign_module.json.dumps(data) or result["error"]["code"] == "zero-comparable"


def test_cleanup_failure_stops_before_next_prepare(tmp_path: Path):
    config, _, _ = _make_config(tmp_path)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    events: list[str] = []
    prepare, record, compare, _ = _fake_actions(events)
    cleanup_calls = 0

    def failing_cleanup(run_dir):
        nonlocal cleanup_calls
        cleanup_calls += 1
        events.append(f"cleanup:{run_dir.name}")
        return {"ok": False, "error": {"code": "foreign-entry"}}

    result = run_campaign(config, snapshot, state, prepare_action=prepare, record_action=record, compare_action=compare, cleanup_action=failing_cleanup)

    assert result["ok"] is False
    assert cleanup_calls == 1
    assert "prepare:case-1" not in events
    assert json.loads(state.read_text(encoding="utf-8"))["cases"]["case-0"]["cleanup"]["status"] == "failed"


def test_resume_skips_cleaned_cases_without_prepare_record_or_compare(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    first_events: list[str] = []
    actions = _fake_actions(first_events, [0.01])
    assert run_campaign(config, snapshot, state, prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])["ok"] is True

    second_events: list[str] = []
    actions = _fake_actions(second_events, [0.01])
    second = run_campaign(config, snapshot, state, prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])

    assert second["ok"] is True
    assert second_events == []


def test_frozen_asset_drift_rejects_resume_before_prepare(tmp_path: Path):
    config, _, pmx_paths = _make_config(tmp_path, case_count=1)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    actions = _fake_actions([])
    assert run_campaign(config, snapshot, state, prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])["ok"] is True
    pmx_paths[0].write_bytes(b"changed")
    events: list[str] = []
    actions = _fake_actions(events)

    result = run_campaign(config, snapshot, state, prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])

    assert result["ok"] is False
    assert result["error"]["code"] == "selection"
    assert events == []


def test_deterministic_nearest_rank_quantiles_and_snapshot_repeat(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=2)
    snapshot = tmp_path / "snapshot.json"
    state = tmp_path / "state.json"
    actions = _fake_actions([], [0.01, 0.02])
    assert run_campaign(config, snapshot, state, prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])["ok"] is True
    first = snapshot.read_bytes()
    actions = _fake_actions([], [0.01, 0.02])
    assert run_campaign(config, snapshot, state, prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])["ok"] is True

    assert snapshot.read_bytes() == first
    data = json.loads(first)
    assert data["metrics"]["translationMaxError"] == {"p50": 0.01, "p95": 0.02, "p99": 0.02, "max": 0.02}


def test_validation_failure_is_recorded_cleaned_and_next_case_continues(tmp_path: Path):
    config, cases, _ = _make_config(tmp_path, case_count=2)
    cases[0].write_text("not-json", encoding="utf-8")
    events: list[str] = []
    actions = _fake_actions(events, [0.01])

    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])

    assert result["ok"] is False
    assert "prepare:case-1" in events
    state = json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))
    assert state["cases"]["case-0"]["status"] == "completed"
    assert state["cases"]["case-0"]["cleanup"]["status"] == "cleaned"
    assert json.loads((tmp_path / "snapshot.json").read_text(encoding="utf-8"))["funnel"]["compared"] == 1


def test_validation_failure_has_null_run_dir_and_never_touches_accidental_cwd_dir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    config, cases, _ = _make_config(tmp_path, case_count=2)
    cases[0].write_text("not-json", encoding="utf-8")
    monkeypatch.chdir(tmp_path)
    accidental = tmp_path / "case-0"
    accidental.mkdir()
    (accidental / "foreign.txt").write_text("keep", encoding="utf-8")
    prepare_cleanup_calls = 0
    actions = _fake_actions([])

    def prepare_cleanup(_run_dir):
        nonlocal prepare_cleanup_calls
        prepare_cleanup_calls += 1
        return {"ok": True}

    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3], prepare_cleanup_action=prepare_cleanup)

    assert result["snapshotWritten"] is True
    state = json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))
    assert state["cases"]["case-0"]["runDir"] is None
    assert state["cases"]["case-0"]["cleanupKind"] == "none"
    assert prepare_cleanup_calls == 0
    assert (accidental / "foreign.txt").read_text(encoding="utf-8") == "keep"


def test_manifest_uses_staged_model_when_present_and_case_model_when_unconverted(tmp_path: Path):
    config_path, case_paths, _ = _make_config(tmp_path, case_count=1)
    config = load_campaign_config(config_path)
    case = load_case(case_paths[0])
    run_dir = case.output_root / case.name
    run_dir.mkdir(parents=True)
    (run_dir / "oracle.actual.jsonl").write_text("{}\n", encoding="utf-8")
    staged = run_dir / "model.mmd-utf16.pmx"
    staged.write_bytes(b"staged")
    manifest = tmp_path / "staged-manifest.json"
    _write_numeric_manifest(manifest, config.cases[0], case, run_dir, config)
    assert json.loads(manifest.read_text(encoding="utf-8"))["cases"][0]["assets"]["model"] == str(staged)
    staged.unlink()
    manifest = tmp_path / "unconverted-manifest.json"
    _write_numeric_manifest(manifest, config.cases[0], case, run_dir, config)
    assert json.loads(manifest.read_text(encoding="utf-8"))["cases"][0]["assets"]["model"] == str(case.pmx)


def test_campaign_rejects_reordered_selection_cases_and_false_discovered_count(tmp_path: Path) -> None:
    config_path, _, _ = _make_config(tmp_path, case_count=2)
    payload = json.loads(config_path.read_text(encoding="utf-8"))
    payload["cases"].reverse()
    config_path.write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(CampaignValidationError, match="case order"):
        load_campaign_config(config_path)

    payload["cases"].reverse()
    payload["discovered"] += 1
    config_path.write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(CampaignValidationError, match="eligible asset count"):
        load_campaign_config(config_path)


def test_prepare_failure_without_artifacts_continues_and_is_resume_skipped(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=2)
    events: list[str] = []
    normal = _fake_actions(events, [0.01])
    calls = {"count": 0}

    def prepare(case):
        calls["count"] += 1
        events.append(f"prepare:{case.name}")
        if case.name == "case-0":
            return {"ok": False, "phase": "preflight", "artifactName": case.name}
        return normal[0](case)

    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=prepare, record_action=normal[1], compare_action=normal[2], cleanup_action=normal[3])
    assert result["ok"] is False
    assert "prepare:case-1" in events
    second_events: list[str] = []
    second = _fake_actions(second_events, [0.01])
    resumed = run_campaign(config, tmp_path / "snapshot-2.json", tmp_path / "state.json", prepare_action=second[0], record_action=second[1], compare_action=second[2], cleanup_action=second[3])
    assert resumed["ok"] is False
    assert second_events == []


def test_prepare_failure_with_unsafe_run_stops_before_next_case(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=2)
    events: list[str] = []
    normal = _fake_actions(events, [0.01])

    def prepare(case):
        events.append(f"prepare:{case.name}")
        if case.name == "case-0":
            run_dir = case.output_root / case.name
            run_dir.mkdir(parents=True)
            (run_dir / "foreign.txt").write_text("foreign", encoding="utf-8")
            return {"ok": False, "phase": "preflight", "artifactName": case.name}
        return normal[0](case)

    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=prepare, record_action=normal[1], compare_action=normal[2], cleanup_action=normal[3])
    assert result["ok"] is False
    assert result["error"]["code"] == "prepare"
    assert "prepare:case-1" not in events


def test_tampered_resumable_run_dir_stops_before_cleanup(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    first = _fake_actions([])

    def failing_cleanup(_run_dir):
        return {"ok": False, "error": {"code": "foreign-entry"}}

    assert not run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=first[0], record_action=first[1], compare_action=first[2], cleanup_action=failing_cleanup)["ok"]
    state_path = tmp_path / "state.json"
    state = json.loads(state_path.read_text(encoding="utf-8"))
    state["cases"]["case-0"]["runDir"] = str(tmp_path / "tampered")
    state_path.write_text(json.dumps(state), encoding="utf-8")
    cleanup_calls = 0

    def cleanup(_run_dir):
        nonlocal cleanup_calls
        cleanup_calls += 1
        return {"ok": True}

    second = _fake_actions([])
    result = run_campaign(config, tmp_path / "snapshot-2.json", state_path, prepare_action=second[0], record_action=second[1], compare_action=second[2], cleanup_action=cleanup)
    assert result["ok"] is False
    assert result["error"]["code"] == "state"
    assert cleanup_calls == 0


def test_persist_failure_before_cleanup_is_structured_and_does_not_cleanup(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    config, _, _ = _make_config(tmp_path, case_count=1)
    actions = _fake_actions([])
    cleanup_calls = 0

    def cleanup(_run_dir):
        nonlocal cleanup_calls
        cleanup_calls += 1
        return {"ok": True}

    def fail_persist(_path, _state):
        raise OSError("state fsync failed")

    monkeypatch.setattr(campaign_module, "_persist_state", fail_persist)
    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=cleanup)
    assert result["ok"] is False
    assert result["error"]["kind"] == "campaign"
    assert cleanup_calls == 0


def test_worst_cases_rank_by_threshold_ratio_and_metric_result(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    payload = json.loads(config.read_text(encoding="utf-8"))
    payload["compare"]["thresholds"].update({"translationMaxError": 100.0, "maxAbsError": 0.001})
    config.write_text(json.dumps(payload), encoding="utf-8")
    actions = _fake_actions([])

    def asymmetric_compare(manifest, repo_root):
        del repo_root
        name = json.loads(manifest.read_text(encoding="utf-8"))["cases"][0]["name"]
        return CommandResult(("fake",), manifest.parent, 0, json.dumps({"perCase": [{"name": name, "status": "mismatch", "translationMaxError": 0.1, "translationRmsError": 0.0, "rotationMaxAngleRad": 0.0, "rotationRmsAngleRad": 0.0, "maxAbsError": 0.01, "mismatchCount": 1, "comparedFrames": 1, "comparedBones": 1, "missing": 0, "importErrors": 0, "noTargets": 0, "skippedTargets": []}]}), "")

    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=asymmetric_compare, cleanup_action=actions[3])
    assert result["ok"] is False
    worst = json.loads((tmp_path / "snapshot.json").read_text(encoding="utf-8"))["worstCases"]
    assert worst[0]["metric"] == "maxAbsError"
    assert {item["metric"]: item["result"] for item in worst}["translationMaxError"] == "pass"
    assert {item["metric"]: item["result"] for item in worst}["maxAbsError"] == "fail"


def test_status_mismatch_is_structural_only_and_thresholds_decide_pass() -> None:
    result = CommandResult(("fake",), Path("."), 0, json.dumps({"perCase": [{"name": "case-0", "status": "mismatch", "translationMaxError": 0.01, "translationRmsError": 0.01, "rotationMaxAngleRad": 0.01, "rotationRmsAngleRad": 0.01, "maxAbsError": 0.01, "mismatchCount": 1, "comparedFrames": 1, "comparedBones": 1, "missing": 0, "importErrors": 0, "noTargets": 0, "skippedTargets": []}]}), "")
    thresholds = {name: 0.1 for name in ("translationMaxError", "translationRmsError", "rotationMaxAngleRad", "rotationRmsAngleRad", "maxAbsError")}

    outcome = _parse_compare_result(result, "case-0", thresholds)

    assert outcome.compared is True
    assert outcome.passed is True
    assert "compare-status" not in outcome.failures


def test_nonzero_compare_exit_is_not_comparable() -> None:
    result = CommandResult(("fake",), Path("."), 1, "{}", "failed")
    outcome = _parse_compare_result(result, "case-0", {name: 1.0 for name in ("translationMaxError", "translationRmsError", "rotationMaxAngleRad", "rotationRmsAngleRad", "maxAbsError")})
    assert outcome.compared is False
    assert outcome.failures == ("compare-command",)


def test_campaign_uses_injected_head_provenance_and_publishes_hash(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    actions = _fake_actions([])
    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3], provenance_probe=lambda _: {"commitSha": "b" * 40, "repositoryState": "clean"})
    assert result["ok"] is True
    run = json.loads((tmp_path / "snapshot.json").read_text(encoding="utf-8"))["run"]
    assert run["commitSha"] == "b" * 40
    assert run["repositoryState"] == "clean"
    assert len(run["selectionHash"]) == 64
    assert run["mmdExecutableSha256"] == "not-observed"


def test_conflicting_mmd_hash_stops_after_safe_cleanup(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=2)
    events: list[str] = []
    actions = _fake_actions(events, [0.01, 0.01])
    hashes = iter(("a" * 64, "b" * 64))

    def record(case, executable):
        value = actions[1](case, executable)
        value["mmdExecutable"] = {"sha256": next(hashes)}
        return value

    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=record, compare_action=actions[2], cleanup_action=actions[3])
    assert result["ok"] is False
    assert result["error"]["code"] == "mmd-executable-conflict"
    assert "prepare:case-1" in events


def test_dirty_provenance_is_rejected_before_prepare(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    actions = _fake_actions([])

    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3], provenance_probe=lambda _: {"commitSha": "a" * 40, "repositoryState": "dirty"})

    assert result["ok"] is False
    assert result["error"]["code"] == "provenance-dirty"
    assert not (tmp_path / "state.json").exists()


def test_resume_rejects_head_change_from_state_without_prepare(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    first = _fake_actions([])
    assert run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=first[0], record_action=first[1], compare_action=first[2], cleanup_action=first[3], provenance_probe=lambda _: {"commitSha": "a" * 40, "repositoryState": "clean"})["ok"] is True
    second_events: list[str] = []
    second = _fake_actions(second_events)
    result = run_campaign(config, tmp_path / "snapshot-2.json", tmp_path / "state.json", prepare_action=second[0], record_action=second[1], compare_action=second[2], cleanup_action=second[3], provenance_probe=lambda _: {"commitSha": "b" * 40, "repositoryState": "clean"})

    assert result["ok"] is False
    assert result["error"]["code"] == "state-mismatch"
    assert second_events == []


def test_completed_state_cannot_be_reassociated_at_final_probe(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    first = _fake_actions([])
    assert run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=first[0], record_action=first[1], compare_action=first[2], cleanup_action=first[3], provenance_probe=lambda _: {"commitSha": "a" * 40, "repositoryState": "clean"})["ok"] is True
    snapshot_before = (tmp_path / "snapshot.json").read_bytes()
    probes = iter((
        {"commitSha": "a" * 40, "repositoryState": "clean"},
        {"commitSha": "a" * 40, "repositoryState": "clean"},
        {"commitSha": "b" * 40, "repositoryState": "clean"},
    ))
    second = _fake_actions([])
    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=second[0], record_action=second[1], compare_action=second[2], cleanup_action=second[3], provenance_probe=lambda _: next(probes))

    assert result["ok"] is False
    assert result["error"]["code"] == "provenance-mismatch"
    assert (tmp_path / "snapshot.json").read_bytes() == snapshot_before


def test_mid_run_provenance_change_cleans_owned_result_before_stopping(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=2)
    probes = iter((
        {"commitSha": "a" * 40, "repositoryState": "clean"},
        {"commitSha": "a" * 40, "repositoryState": "clean"},
        {"commitSha": "b" * 40, "repositoryState": "clean"},
    ))
    events: list[str] = []
    actions = _fake_actions(events)
    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3], provenance_probe=lambda _: next(probes))

    assert result["ok"] is False
    assert result["error"]["code"] == "provenance-mismatch"
    assert "cleanup:case-0" in events
    assert "prepare:case-1" not in events
    assert not (tmp_path / "snapshot.json").exists()
    state = json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))
    entry = state["cases"]["case-0"]
    assert entry["compared"] is False
    assert entry["passed"] is False
    assert entry["metrics"] == {}
    assert "provenance-mismatch" in entry["failures"]
    assert entry["cleanup"]["status"] == "cleaned"


def test_mid_run_input_drift_invalidates_result_and_cleans_before_stopping(tmp_path: Path):
    config, _, pmx_paths = _make_config(tmp_path, case_count=2)
    events: list[str] = []
    actions = _fake_actions(events)

    def drifting_compare(manifest, repo_root):
        result = actions[2](manifest, repo_root)
        pmx_paths[0].write_bytes(b"changed-during-compare")
        return result

    result = run_campaign(
        config,
        tmp_path / "snapshot.json",
        tmp_path / "state.json",
        prepare_action=actions[0],
        record_action=actions[1],
        compare_action=drifting_compare,
        cleanup_action=actions[3],
    )

    assert result["ok"] is False
    assert result["error"]["code"] == "selection"
    assert "cleanup:case-0" in events
    assert "prepare:case-1" not in events
    assert not (tmp_path / "snapshot.json").exists()
    entry = json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))["cases"]["case-0"]
    assert entry["compared"] is False
    assert entry["metrics"] == {}
    assert "selection" in entry["failures"]
    assert entry["cleanup"]["status"] == "cleaned"


def test_probe_failure_after_provisional_persist_keeps_result_unverified_and_cleans(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    config, _, _ = _make_config(tmp_path, case_count=2)
    events: list[str] = []
    actions = _fake_actions(events)
    original_reprobe = campaign_module._reprobe_provenance
    calls = 0

    def fail_after_case_acceptance(probe, repo_root, expected_commit_sha):
        nonlocal calls
        calls += 1
        if calls == 2:
            raise OSError("injected provenance probe failure")
        return original_reprobe(probe, repo_root, expected_commit_sha)

    monkeypatch.setattr(campaign_module, "_reprobe_provenance", fail_after_case_acceptance)
    result = run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=actions[0], record_action=actions[1], compare_action=actions[2], cleanup_action=actions[3])

    assert result["ok"] is False
    assert result["error"]["code"] == "provenance"
    assert "cleanup:case-0" in events
    assert "prepare:case-1" not in events
    assert not (tmp_path / "snapshot.json").exists()
    entry = json.loads((tmp_path / "state.json").read_text(encoding="utf-8"))["cases"]["case-0"]
    assert entry["provenanceValidated"] is False
    assert entry["compared"] is False
    assert entry["passed"] is False
    assert entry["metrics"] == {}
    assert entry["cleanup"]["status"] == "cleaned"


def test_resume_provisional_state_cleans_and_stops_without_accepting_metrics(tmp_path: Path):
    config, _, _ = _make_config(tmp_path, case_count=1)
    first = _fake_actions([])
    assert run_campaign(config, tmp_path / "snapshot.json", tmp_path / "state.json", prepare_action=first[0], record_action=first[1], compare_action=first[2], cleanup_action=first[3])["ok"] is True

    state_path = tmp_path / "state.json"
    state = json.loads(state_path.read_text(encoding="utf-8"))
    entry = state["cases"]["case-0"]
    entry["provenanceValidated"] = False
    entry["compared"] = False
    entry["passed"] = False
    entry["metrics"] = {}
    entry["failures"] = ["provenance-unvalidated"]
    entry["cleanup"] = {"status": "pending"}
    state_path.write_text(json.dumps(state), encoding="utf-8")

    events: list[str] = []
    second = _fake_actions(events)
    result = run_campaign(config, tmp_path / "snapshot-2.json", state_path, prepare_action=second[0], record_action=second[1], compare_action=second[2], cleanup_action=second[3])

    assert result["ok"] is False
    assert result["error"]["code"] == "result-unverified"
    assert events == ["cleanup:case-0"]
    resumed_entry = json.loads(state_path.read_text(encoding="utf-8"))["cases"]["case-0"]
    assert resumed_entry["provenanceValidated"] is False
    assert resumed_entry["metrics"] == {}
    assert resumed_entry["cleanup"]["status"] == "cleaned"
    assert not (tmp_path / "snapshot-2.json").exists()
