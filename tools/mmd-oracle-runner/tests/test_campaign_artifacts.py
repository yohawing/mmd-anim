from __future__ import annotations

import json
from pathlib import Path

from mmd_oracle_runner.campaign_artifacts import cleanup_completed_case_run, cleanup_prepared_case_run


def _write_run(tmp_path: Path, *, completed: bool, escape: bool = False, foreign: bool = False) -> Path:
    run_dir = tmp_path / "case-001"
    run_dir.mkdir()
    shared = {"caseFile": str(tmp_path / "case.json"), "artifactName": run_dir.name, "frames": [0, 15], "inputInventory": {"pmx": {"size": 1, "sha256": "a" * 64}}}
    prepare_names = ["scene.pmm", "fixture.json", "model.mmd-utf16.pmx", "prepare-result.json"]
    prepare_owned = [str(run_dir / name) for name in prepare_names]
    if escape:
        prepare_owned.append(str(tmp_path / "outside.txt"))
    prepare_marker = {"schemaVersion": 1, "ok": True, "phase": "complete", **shared, "ownedArtifacts": prepare_owned, "artifacts": {name: {"path": str(run_dir / name), "exists": True} for name in ("scene.pmm", "fixture.json", "model.mmd-utf16.pmx", "prepare-result.json")}}
    for name in prepare_names[:-1]:
        (run_dir / name).write_text(name, encoding="utf-8")
    (run_dir / "prepare-result.json").write_text(json.dumps(prepare_marker), encoding="utf-8")
    if completed:
        record_names = ["oracle.actual.jsonl", "oracle.actual.jsonl.done", "record-result.json"]
        record_owned = [str(run_dir / name) for name in record_names]
        record_marker = {"schemaVersion": 1, "ok": True, "phase": "complete", "recorded": True, **shared, "ownedArtifacts": record_owned, "artifacts": {name: {"path": str(run_dir / name), "exists": True} for name in record_names}}
        for name in record_names[:-1]:
            (run_dir / name).write_text(name, encoding="utf-8")
        (run_dir / "record-result.json").write_text(json.dumps(record_marker), encoding="utf-8")
    if foreign:
        (run_dir / "foreign.jsonl").write_text("foreign", encoding="utf-8")
    return run_dir


def test_completed_owned_cleanup_succeeds(tmp_path: Path) -> None:
    run_dir = _write_run(tmp_path, completed=True)
    result = cleanup_completed_case_run(run_dir)
    assert result["ok"] is True
    assert result["removedRunDir"] is True
    assert not run_dir.exists()


def test_prepared_owned_cleanup_succeeds_without_record_marker(tmp_path: Path) -> None:
    run_dir = _write_run(tmp_path, completed=False)
    result = cleanup_prepared_case_run(run_dir)
    assert result["ok"] is True
    assert result["removedRunDir"] is True
    assert not run_dir.exists()


def test_foreign_entry_refuses_cleanup_without_deleting_owned_files(tmp_path: Path) -> None:
    run_dir = _write_run(tmp_path, completed=True, foreign=True)
    result = cleanup_completed_case_run(run_dir)
    assert result["ok"] is False
    assert result["error"]["code"] == "foreign-entry"
    assert (run_dir / "foreign.jsonl").exists()
    assert (run_dir / "prepare-result.json").exists()


def test_marker_path_escape_refuses_cleanup(tmp_path: Path) -> None:
    run_dir = _write_run(tmp_path, completed=True, escape=True)
    result = cleanup_completed_case_run(run_dir)
    assert result["ok"] is False
    assert result["error"]["code"] == "marker-escape"
    assert (run_dir / "prepare-result.json").exists()
