from __future__ import annotations

import json
import struct
from pathlib import Path

import pytest

import mmd_oracle_runner.selection as selection_module
from mmd_oracle_runner.campaign import load_campaign_config
from mmd_oracle_runner.cli import main
from mmd_oracle_runner.selection import (
    SelectionError,
    _selection_hash,
    freeze_selection,
    load_selection,
    materialize_selection,
    verify_selection,
)


def _pmx(path: Path, marker: bytes = b"") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"PMX " + b"\x00" * 8 + marker)


def _vmd(path: Path, bone_frames: int = 1, marker: bytes = b"") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    header = bytearray(54)
    header[:20] = b"Vocaloid Motion Data"
    struct.pack_into("<I", header, 50, bone_frames)
    path.write_bytes(bytes(header) + marker)


def test_freeze_is_deterministic_content_hashed_and_loadable(tmp_path: Path) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    for index in range(5):
        _pmx(pmx_root / f"model-{index}.pmx", bytes([index]))
        _vmd(vmd_root / f"motion-{index}.vmd", marker=bytes([index]))

    first = freeze_selection(
        pmx_root.resolve(),
        vmd_root.resolve(),
        (tmp_path / "first.json").resolve(),
        count=3,
        seed="fixed-v1",
    )
    second = freeze_selection(
        pmx_root.resolve(),
        vmd_root.resolve(),
        (tmp_path / "second.json").resolve(),
        count=3,
        seed="fixed-v1",
    )

    assert first == second
    assert len(first["cases"]) == 3
    assert load_selection((tmp_path / "first.json").resolve()) == first
    assert all(len(case["pmx"]["sha256"]) == 64 for case in first["cases"])
    assert all(Path(case["bodyVmd"]["path"]).is_absolute() for case in first["cases"])


def test_camera_only_and_invalid_magic_are_not_eligible(tmp_path: Path) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    (pmx_root / "bad.pmx").write_bytes(b"not-pmx")
    _vmd(vmd_root / "body.vmd", bone_frames=2)
    _vmd(vmd_root / "camera.vmd", bone_frames=0)

    selection = freeze_selection(
        pmx_root.resolve(),
        vmd_root.resolve(),
        (tmp_path / "selection.json").resolve(),
        count=1,
        seed="fixed-v1",
    )

    assert selection["discovery"] == {
        "pmxFiles": 2,
        "vmdFiles": 2,
        "eligiblePmx": 1,
        "eligibleBodyVmd": 1,
        "selected": 1,
    }


def test_existing_selection_is_never_overwritten(tmp_path: Path) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    _vmd(vmd_root / "body.vmd")
    output = (tmp_path / "selection.json").resolve()
    output.write_text("keep", encoding="utf-8")

    with pytest.raises(SelectionError, match="not overwritten") as raised:
        freeze_selection(pmx_root.resolve(), vmd_root.resolve(), output, count=1, seed="fixed-v1")

    assert raised.value.code == "selection-exists"
    assert output.read_text(encoding="utf-8") == "keep"


def test_tampered_selection_hash_is_rejected(tmp_path: Path) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    _vmd(vmd_root / "body.vmd")
    output = (tmp_path / "selection.json").resolve()
    freeze_selection(pmx_root.resolve(), vmd_root.resolve(), output, count=1, seed="fixed-v1")
    payload = json.loads(output.read_text(encoding="utf-8"))
    payload["frames"] = [999]
    output.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(SelectionError, match="selectionHash") as raised:
        load_selection(output)

    assert raised.value.code == "selection-hash"


@pytest.mark.parametrize(
    ("field", "value"),
    (("caseId", "../../outside"), ("relativePath", "../outside.pmx")),
)
def test_selection_rejects_path_traversal(tmp_path: Path, field: str, value: str) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    _vmd(vmd_root / "body.vmd")
    output = (tmp_path / "selection.json").resolve()
    freeze_selection(pmx_root.resolve(), vmd_root.resolve(), output, count=1, seed="fixed-v1")
    payload = json.loads(output.read_text(encoding="utf-8"))
    if field == "caseId":
        payload["cases"][0][field] = value
    else:
        payload["cases"][0]["pmx"][field] = value
        payload["cases"][0]["pmx"]["path"] = str((pmx_root / value).resolve())
    payload["selectionHash"] = _selection_hash({key: item for key, item in payload.items() if key != "selectionHash"})
    output.write_text(json.dumps(payload), encoding="utf-8")

    with pytest.raises(SelectionError, match="invalid|relative|escapes"):
        load_selection(output)


def test_insufficient_assets_fails_without_creating_output(tmp_path: Path) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    _vmd(vmd_root / "body.vmd")
    output = (tmp_path / "selection.json").resolve()

    with pytest.raises(SelectionError, match="requested 2"):
        freeze_selection(pmx_root.resolve(), vmd_root.resolve(), output, count=2, seed="fixed-v1")

    assert not output.exists()


def test_verify_selection_detects_asset_drift(tmp_path: Path) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    pmx_path = pmx_root / "good.pmx"
    _pmx(pmx_path)
    _vmd(vmd_root / "body.vmd")
    output = (tmp_path / "selection.json").resolve()
    selection = freeze_selection(pmx_root.resolve(), vmd_root.resolve(), output, count=1, seed="fixed-v1")

    assert verify_selection(output) == {
        "ok": True,
        "selectionHash": selection["selectionHash"],
        "cases": 1,
        "assetsChecked": 2,
    }
    pmx_path.write_bytes(pmx_path.read_bytes() + b"changed")
    with pytest.raises(SelectionError, match="frozen asset changed") as raised:
        verify_selection(output)
    assert raised.value.code == "asset-drift"


def test_materialize_selection_creates_campaign_bound_to_frozen_assets(tmp_path: Path) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    _vmd(vmd_root / "body.vmd")
    selection_path = (tmp_path / "selection.json").resolve()
    selection = freeze_selection(pmx_root.resolve(), vmd_root.resolve(), selection_path, count=1, seed="fixed-v1")
    template_path = (tmp_path / "template.json").resolve()
    template_path.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "run": {
                    "mmdVersion": "9.32-x64",
                    "dumperVersion": "test",
                    "timestamp": "2026-08-25T00:00:00+09:00",
                    "samplingPolicy": "frozen-selection-v1",
                },
                "compare": {
                    "focusBones": ["センター"],
                    "thresholds": {
                        "translationMaxError": 0.001,
                        "translationRmsError": 0.001,
                        "rotationMaxAngleRad": 0.003,
                        "rotationRmsAngleRad": 0.003,
                        "maxAbsError": 0.003,
                    },
                },
                "outputRoot": str((tmp_path / "runs").resolve()),
            }
        ),
        encoding="utf-8",
    )

    result = materialize_selection(selection_path, template_path, (tmp_path / "materialized").resolve())

    assert result["cases"] == 1
    config = load_campaign_config(Path(result["campaign"]))
    assert config.selection_hash == selection["selectionHash"]
    assert config.cases[0].case_id == selection["cases"][0]["caseId"]


def test_materialize_never_overwrites_output_directory(tmp_path: Path) -> None:
    output = (tmp_path / "materialized").resolve()
    output.mkdir()
    with pytest.raises(SelectionError, match="not overwritten") as raised:
        materialize_selection((tmp_path / "missing-selection.json").resolve(), (tmp_path / "missing.json").resolve(), output)
    assert raised.value.code == "materialize-exists"


def test_materialize_loads_selection_once_and_verifies_that_exact_object(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    _vmd(vmd_root / "body.vmd")
    selection_path = (tmp_path / "selection.json").resolve()
    freeze_selection(pmx_root.resolve(), vmd_root.resolve(), selection_path, count=1, seed="fixed-v1")
    template_path = (tmp_path / "template.json").resolve()
    template_path.write_text(
        json.dumps(
            {
                "schemaVersion": 1,
                "run": {
                    "mmdVersion": "9.32-x64",
                    "dumperVersion": "test",
                    "timestamp": "2026-08-25T00:00:00+09:00",
                    "samplingPolicy": "fixed-v1",
                },
                "compare": {
                    "focusBones": ["センター"],
                    "thresholds": {name: 0.003 for name in selection_module._THRESHOLD_NAMES},
                },
                "outputRoot": str((tmp_path / "runs").resolve()),
            }
        ),
        encoding="utf-8",
    )
    original_load = selection_module.load_selection
    calls = 0

    def counted_load(path):
        nonlocal calls
        calls += 1
        return original_load(path)

    monkeypatch.setattr(selection_module, "load_selection", counted_load)
    materialize_selection(selection_path, template_path, (tmp_path / "materialized").resolve())
    assert calls == 1


def test_freeze_and_verify_cli_are_machine_readable(tmp_path: Path, capsys: pytest.CaptureFixture[str]) -> None:
    pmx_root = tmp_path / "pmx"
    vmd_root = tmp_path / "vmd"
    _pmx(pmx_root / "good.pmx")
    _vmd(vmd_root / "body.vmd")
    output = (tmp_path / "selection.json").resolve()

    assert main(
        [
            "freeze-selection",
            "--pmx-root",
            str(pmx_root.resolve()),
            "--vmd-root",
            str(vmd_root.resolve()),
            "--output",
            str(output),
            "--count",
            "1",
            "--seed",
            "fixed-v1",
        ]
    ) == 0
    frozen = json.loads(capsys.readouterr().out)
    assert frozen["ok"] is True
    assert frozen["selected"] == 1

    assert main(["verify-selection", "--selection", str(output)]) == 0
    verified = json.loads(capsys.readouterr().out)
    assert verified["assetsChecked"] == 2
    assert verified["selectionHash"] == frozen["selectionHash"]
