from __future__ import annotations

import json
from pathlib import Path

import pytest

from mmd_oracle_runner.case import CaseValidationError, load_case
from mmd_oracle_runner.cli import main
from prepare_test_support import write_case


def _issues(tmp_path: Path, **overrides):
    case_path = write_case(tmp_path)
    payload = json.loads(case_path.read_text(encoding="utf-8"))
    for key, value in overrides.items():
        if key == "input":
            payload["input"].update(value)
        else:
            payload[key] = value
    case_path.write_text(json.dumps(payload), encoding="utf-8")
    with pytest.raises(CaseValidationError) as raised:
        load_case(case_path)
    return raised.value.issues


def test_minimal_case_is_typed_and_normalized(tmp_path: Path):
    case = load_case(write_case(tmp_path))
    assert case.generator_backend == "python-rust"
    assert case.frames == (0, 15, 30)


def test_frames_are_normalized_to_ascending_order(tmp_path: Path):
    case_path = write_case(tmp_path)
    payload = json.loads(case_path.read_text(encoding="utf-8")); payload["frames"] = [30, 0, 15]
    case_path.write_text(json.dumps(payload), encoding="utf-8")
    assert load_case(case_path).frames == (0, 15, 30)


def test_camera_is_preserved_for_record_and_property_fails_closed(tmp_path: Path):
    case_path = write_case(tmp_path, camera=True)
    assert load_case(case_path).camera_vmd is not None
    prop = _issues(tmp_path, requestedFeatures=["property-ik"])
    assert any(issue.field == "requestedFeatures" for issue in prop)


def test_invalid_paths_and_frames_are_rejected(tmp_path: Path):
    assert any(issue.field == "pmx" for issue in _issues(tmp_path, input={"pmx": "relative.pmx"}))
    assert any(issue.field == "outputRoot" for issue in _issues(tmp_path, outputRoot="relative"))
    assert any(issue.field == "frames" for issue in _issues(tmp_path, frames=[0, 0]))


def test_unknown_backend_is_rejected(tmp_path: Path):
    assert any(issue.field == "generatorBackend" for issue in _issues(tmp_path, generatorBackend="javascript"))


def test_cli_validate_emits_json(tmp_path: Path, capsys):
    assert main(["validate", "--case", str(write_case(tmp_path))]) == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["ok"] is True and payload["case"]["generatorBackend"] == "python-rust"
