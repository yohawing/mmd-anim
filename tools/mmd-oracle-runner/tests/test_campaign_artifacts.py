from __future__ import annotations

import json
from pathlib import Path

import pytest

from mmd_oracle_runner.campaign_artifacts import cleanup_completed_case_run, cleanup_prepared_case_run
from mmd_oracle_runner.case import load_case
from mmd_oracle_runner.prepare import prepare_case
from prepare_test_support import FakeRunner, REPO_ROOT, write_case


def _write_completed_run(tmp_path: Path, *, escape_prepare_path: bool = False) -> tuple[Path, Path]:
    run_dir = tmp_path / "case-001"
    run_dir.mkdir()
    shared = {
        "caseFile": str(tmp_path / "case.json"),
        "artifactName": run_dir.name,
        "frames": [0, 15],
        "inputInventory": {"pmx": {"size": 1, "sha256": "a" * 64}},
    }
    prepare_owned = [
        str(run_dir / name)
        for name in ("scene.pmm", "fixture.json", "model.mmd-utf16.pmx", "prepare-result.json")
    ]
    record_owned = [
        str(run_dir / name)
        for name in (
            "oracle.actual.jsonl",
            "oracle.actual.jsonl.done",
            "record-result.json",
        )
    ]
    if escape_prepare_path:
        prepare_owned.append(str(tmp_path / "outside.txt"))
    prepare_marker = {
        "schemaVersion": 1,
        "ok": True,
        "phase": "complete",
        **shared,
        "ownedArtifacts": prepare_owned,
        "artifacts": {
            "project": {"path": str(run_dir / "scene.pmm"), "exists": True},
            "fixture": {"path": str(run_dir / "fixture.json"), "exists": True},
            "model": {"path": str(run_dir / "model.mmd-utf16.pmx"), "exists": True},
            "result": {"path": str(run_dir / "prepare-result.json"), "exists": True},
        },
    }
    record_marker = {
        "schemaVersion": 1,
        "ok": True,
        "phase": "complete",
        "recorded": True,
        **shared,
        "ownedArtifacts": record_owned,
        "artifacts": {
            "output": {"path": str(run_dir / "oracle.actual.jsonl"), "exists": True},
            "done": {"path": str(run_dir / "oracle.actual.jsonl.done"), "exists": True},
            "result": {"path": str(run_dir / "record-result.json"), "exists": True},
        },
    }
    for name in ("scene.pmm", "fixture.json", "model.mmd-utf16.pmx", "oracle.actual.jsonl", "oracle.actual.jsonl.done"):
        (run_dir / name).write_text(name, encoding="utf-8")
    (run_dir / "prepare-result.json").write_text(json.dumps(prepare_marker), encoding="utf-8")
    (run_dir / "record-result.json").write_text(json.dumps(record_marker), encoding="utf-8")
    return run_dir, tmp_path / "outside.txt"


def test_owned_completed_run_is_removed_and_result_is_structured(tmp_path: Path) -> None:
    run_dir, _ = _write_completed_run(tmp_path)

    result = cleanup_completed_case_run(run_dir)

    assert result["ok"] is True
    assert result["removedRunDir"] is True
    assert set(result["deleted"]) == {
        "scene.pmm",
        "fixture.json",
        "model.mmd-utf16.pmx",
        "oracle.actual.jsonl",
        "oracle.actual.jsonl.done",
        "prepare-result.json",
        "record-result.json",
    }


def test_foreign_entry_fails_closed_without_deleting_owned_files(tmp_path: Path) -> None:
    run_dir, _ = _write_completed_run(tmp_path)
    foreign = run_dir / "foreign.jsonl"
    foreign.write_text("foreign", encoding="utf-8")

    result = cleanup_completed_case_run(run_dir)

    assert result["ok"] is False
    assert result["error"]["code"] == "foreign-entry"
    assert foreign.exists()
    assert (run_dir / "scene.pmm").exists()
    assert (run_dir / "prepare-result.json").exists()


def test_marker_run_dir_escape_fails_closed(tmp_path: Path) -> None:
    run_dir, outside = _write_completed_run(tmp_path, escape_prepare_path=True)

    result = cleanup_completed_case_run(run_dir)

    assert result["ok"] is False
    assert result["error"]["code"] == "marker-escape"
    assert (run_dir / "scene.pmm").exists()
    assert not outside.exists()


def test_hardlink_to_input_fails_closed_when_platform_allows_hardlinks(tmp_path: Path) -> None:
    run_dir, _ = _write_completed_run(tmp_path)
    input_path = tmp_path / "input.pmx"
    input_path.write_text("protected", encoding="utf-8")
    hardlink = run_dir / "scene.pmm"
    hardlink.unlink()
    try:
        hardlink.hardlink_to(input_path)
    except (OSError, NotImplementedError):
        pytest.skip("hardlink creation is unavailable")
    marker_path = run_dir / "prepare-result.json"
    marker = json.loads(marker_path.read_text(encoding="utf-8"))
    marker["inputInventory"]["pmx"]["path"] = str(input_path)
    marker_path.write_text(json.dumps(marker), encoding="utf-8")
    record_marker_path = run_dir / "record-result.json"
    record_marker = json.loads(record_marker_path.read_text(encoding="utf-8"))
    record_marker["inputInventory"]["pmx"]["path"] = str(input_path)
    record_marker_path.write_text(json.dumps(record_marker), encoding="utf-8")

    result = cleanup_completed_case_run(run_dir)

    assert result["ok"] is False
    assert result["error"]["code"] == "protected-artifact"
    assert input_path.exists()
    assert hardlink.exists()


def test_reparse_entry_fails_closed_when_platform_allows_symlink(tmp_path: Path) -> None:
    run_dir, _ = _write_completed_run(tmp_path)
    link = run_dir / "foreign-link"
    try:
        link.symlink_to(tmp_path / "outside.txt")
    except (OSError, NotImplementedError):
        pytest.skip("symlink creation is unavailable")

    result = cleanup_completed_case_run(run_dir)

    assert result["ok"] is False
    assert result["error"]["code"] == "reparse-artifact"
    assert link.is_symlink()
    assert (run_dir / "prepare-result.json").exists()


def test_reparse_run_dir_fails_closed_when_platform_allows_directory_symlink(tmp_path: Path) -> None:
    real_parent = tmp_path / "real-parent"
    real_parent.mkdir()
    run_dir, _ = _write_completed_run(real_parent)
    alias = tmp_path / "run-alias"
    try:
        alias.symlink_to(run_dir, target_is_directory=True)
    except (OSError, NotImplementedError):
        pytest.skip("directory symlink creation is unavailable")

    result = cleanup_completed_case_run(alias)

    assert result["ok"] is False
    assert result["error"]["code"] == "reparse-run-dir"
    assert run_dir.exists()
    assert (run_dir / "prepare-result.json").exists()


def test_missing_marker_fails_closed_before_any_cleanup(tmp_path: Path) -> None:
    run_dir, _ = _write_completed_run(tmp_path)
    (run_dir / "record-result.json").unlink()

    result = cleanup_completed_case_run(run_dir)

    assert result["ok"] is False
    assert result["error"]["code"] == "missing-marker"
    assert (run_dir / "scene.pmm").exists()


def test_owned_prepare_only_run_is_removed_without_record_marker(tmp_path: Path) -> None:
    run_dir, _ = _write_completed_run(tmp_path)
    (run_dir / "record-result.json").unlink()
    marker = json.loads((run_dir / "prepare-result.json").read_text(encoding="utf-8"))
    marker["ownedArtifacts"] = [path for path in marker["ownedArtifacts"] if Path(path).name != "record-result.json"]
    (run_dir / "prepare-result.json").write_text(json.dumps(marker), encoding="utf-8")
    for name in ("oracle.actual.jsonl", "oracle.actual.jsonl.done"):
        (run_dir / name).unlink()

    result = cleanup_prepared_case_run(run_dir)

    assert result["ok"] is True
    assert result["removedRunDir"] is True
    assert not run_dir.exists()


def test_failed_prepare_marker_is_cleanable_but_not_a_completed_run(tmp_path: Path) -> None:
    run_dir, _ = _write_completed_run(tmp_path)
    for name in ("scene.pmm", "fixture.json", "oracle.actual.jsonl", "oracle.actual.jsonl.done"):
        (run_dir / name).unlink()
    marker_path = run_dir / "prepare-result.json"
    marker = json.loads(marker_path.read_text(encoding="utf-8"))
    model = run_dir / "model.mmd-utf16.pmx"
    marker.update(
        {
            "ok": False,
            "phase": "artifacts",
            "ownedArtifacts": [str(model), str(marker_path)],
            "artifacts": {
                "model": {"path": str(model), "exists": True},
                "result": {"path": str(marker_path), "exists": True},
            },
        }
    )
    marker_path.write_text(json.dumps(marker), encoding="utf-8")

    completed = cleanup_completed_case_run(run_dir)
    assert completed["ok"] is False
    assert completed["error"]["code"] == "marker-mismatch"
    assert run_dir.exists()

    (run_dir / "record-result.json").unlink()
    prepared = cleanup_prepared_case_run(run_dir)
    assert prepared["ok"] is True
    assert prepared["removedRunDir"] is True
    assert not run_dir.exists()


def test_prepare_only_cleanup_requires_prepare_marker(tmp_path: Path) -> None:
    run_dir = tmp_path / "case-001"
    run_dir.mkdir()
    (run_dir / "fixture.json").write_text("foreign", encoding="utf-8")

    result = cleanup_prepared_case_run(run_dir)

    assert result["ok"] is False
    assert result["error"]["code"] == "missing-marker"
    assert (run_dir / "fixture.json").exists()


def test_real_prepare_marker_is_consumed_by_prepare_cleanup_without_touching_inputs(tmp_path: Path) -> None:
    case_path = write_case(tmp_path)
    case = load_case(case_path)
    input_before = {path: path.read_bytes() for path in (case.pmx, case.body_vmd, case.source_path)}

    prepared = prepare_case(case, runner=FakeRunner(), repo_root=REPO_ROOT)

    assert prepared["ok"] is True
    run_dir = Path(prepared["artifacts"]["result"]["path"]).parent
    result = cleanup_prepared_case_run(run_dir)

    assert result["ok"] is True
    assert not run_dir.exists()
    assert {path: path.read_bytes() for path in input_before} == input_before
