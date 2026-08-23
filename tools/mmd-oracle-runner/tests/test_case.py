from __future__ import annotations

import json
import io
from pathlib import Path

import pytest

from mmd_oracle_runner.case import CaseValidationError, load_case
from mmd_oracle_runner.cli import main


def _write_case(tmp_path: Path, **overrides) -> Path:
    assets = tmp_path / "assets"
    assets.mkdir(exist_ok=True)
    pmx = assets / "model.pmx"
    body = assets / "body.vmd"
    camera = assets / "camera.vmd"
    pmx.write_bytes(b"pmx")
    body.write_bytes(b"body")
    camera.write_bytes(b"camera")
    payload = {
        "schemaVersion": 1,
        "name": "body-only",
        "input": {"pmx": str(pmx), "bodyVmd": str(body)},
        "frames": [0, 15, 30],
        "outputRoot": str(tmp_path / "output"),
        "generatorBackend": "node-mmddumper",
        "recordOptIn": False,
        "dialogOptIn": False,
    }
    _deep_update(payload, overrides)
    case_path = tmp_path / "case.json"
    case_path.write_text(json.dumps(payload), encoding="utf-8")
    return case_path


def _deep_update(payload: dict, overrides: dict) -> None:
    for key, value in overrides.items():
        if key == "input":
            payload["input"].update(value)
        else:
            payload[key] = value


def _issues(tmp_path: Path, **overrides):
    with pytest.raises(CaseValidationError) as raised:
        load_case(_write_case(tmp_path, **overrides))
    return raised.value.issues


def test_minimal_case_is_typed_and_normalized(tmp_path: Path):
    case = load_case(_write_case(tmp_path))

    assert case.name == "body-only"
    assert case.frames == (0, 15, 30)
    assert case.camera_vmd is None
    assert case.generator_backend == "node-mmddumper"
    assert case.output_root == (tmp_path / "output").resolve()


def test_frames_are_normalized_to_ascending_order(tmp_path: Path):
    case = load_case(_write_case(tmp_path, frames=[30, 0, 15]))

    assert case.frames == (0, 15, 30)


def test_node_backend_accepts_camera_case(tmp_path: Path):
    case_path = _write_case(tmp_path, input={"cameraVmd": str(tmp_path / "assets" / "camera.vmd")})

    case = load_case(case_path)

    assert case.camera_vmd == (tmp_path / "assets" / "camera.vmd").resolve()


def test_relative_input_path_is_rejected(tmp_path: Path):
    issues = _issues(tmp_path, input={"pmx": "assets/model.pmx"})

    assert any(issue.field == "pmx" and "absolute" in issue.reason for issue in issues)


def test_relative_output_root_is_rejected(tmp_path: Path):
    issues = _issues(tmp_path, outputRoot="output")

    assert any(issue.field == "outputRoot" and "absolute" in issue.reason for issue in issues)


def test_existing_output_file_is_rejected(tmp_path: Path):
    output_file = tmp_path / "output-file"
    output_file.write_text("not a directory", encoding="utf-8")
    issues = _issues(tmp_path, outputRoot=str(output_file))

    assert any(issue.field == "outputRoot" and "directory" in issue.reason for issue in issues)


def test_output_root_symlink_is_rejected_when_supported(tmp_path: Path):
    destination = tmp_path / "destination"
    destination.mkdir()
    link = tmp_path / "output-link"
    try:
        link.symlink_to(destination, target_is_directory=True)
    except OSError:
        pytest.skip("symlink creation unavailable")

    issues = _issues(tmp_path, outputRoot=str(link))

    assert any(issue.field == "outputRoot" and "reparse point" in issue.reason for issue in issues)


def test_missing_input_file_is_rejected(tmp_path: Path):
    missing = tmp_path / "assets" / "missing.pmx"
    issues = _issues(tmp_path, input={"pmx": str(missing)})

    assert any(issue.field == "pmx" and "does not exist" in issue.reason for issue in issues)


@pytest.mark.parametrize(
    "frames",
    [[], [-1], [0, 0], [0, True], ["15"]],
)
def test_invalid_frames_are_rejected(tmp_path: Path, frames):
    issues = _issues(tmp_path, frames=frames)

    assert any(issue.field == "frames" for issue in issues)


def test_unknown_backend_is_rejected(tmp_path: Path):
    issues = _issues(tmp_path, generatorBackend="python-pmm")

    assert any(issue.field == "generatorBackend" for issue in issues)


def test_rust_backend_rejects_camera_capability(tmp_path: Path):
    issues = _issues(
        tmp_path,
        generatorBackend="rust-build-pmm",
        input={"cameraVmd": str(tmp_path / "assets" / "camera.vmd")},
    )

    assert any(issue.field == "input.cameraVmd" and "capability" in issue.reason for issue in issues)


def test_unsupported_feature_is_fail_closed(tmp_path: Path):
    issues = _issues(tmp_path, requestedFeatures=["multi-model"])

    assert any(issue.field == "requestedFeatures" and "unsupported capability" in issue.reason for issue in issues)


def test_property_feature_is_explicit_node_opt_in(tmp_path: Path):
    case = load_case(_write_case(tmp_path, requestedFeatures=["property-ik"]))

    assert case.requested_features == ("property-ik",)


def test_rust_backend_rejects_property_feature(tmp_path: Path):
    issues = _issues(tmp_path, generatorBackend="rust-build-pmm", requestedFeatures=["property-ik"])

    assert any(issue.field == "requestedFeatures" and "rust-build-pmm" in issue.reason for issue in issues)


def test_cli_emits_stable_success_json_and_zero(tmp_path: Path, capsys):
    case_path = _write_case(tmp_path)

    assert main(["validate", "--case", str(case_path)]) == 0
    output = capsys.readouterr()
    payload = json.loads(output.out)
    assert payload["ok"] is True
    assert payload["command"] == "validate"
    assert payload["case"]["name"] == "body-only"
    assert output.err == ""


def test_cli_emits_stable_failure_json_and_two(tmp_path: Path, capsys):
    case_path = _write_case(tmp_path, generatorBackend="unknown")

    assert main(["validate", "--case", str(case_path)]) == 2
    output = capsys.readouterr()
    payload = json.loads(output.err)
    assert payload["ok"] is False
    assert payload["error"]["code"] == "case-validation-error"
    assert any(issue["field"] == "generatorBackend" for issue in payload["error"]["issues"])
    assert output.out == ""


def test_cli_escapes_non_ascii_for_legacy_console(tmp_path: Path, monkeypatch):
    case_path = _write_case(tmp_path, name="体", outputRoot=str(tmp_path / "出力"))
    case_path = case_path.rename(tmp_path / "ケース.json")
    stream = _NarrowStringStream()
    monkeypatch.setattr("sys.stdout", stream)

    assert main(["validate", "--case", str(case_path)]) == 0
    payload = json.loads(stream.getvalue())
    assert payload["case"]["name"] == "体"
    assert payload["caseFile"].endswith("ケース.json")


class _NarrowStringStream(io.StringIO):
    encoding = "cp1252"

    def write(self, text: str) -> int:
        text.encode(self.encoding)
        return super().write(text)
