from __future__ import annotations

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "MMDDumper" / "scripts"))
from oracle_cli import main


def _record(frame: int) -> dict:
    return {
        "schemaVersion": 1,
        "source": {"mmdVersion": "9.32-x64", "dumperVersion": "test"},
        "frame": frame,
        "models": [],
    }


def test_validate_and_coverage_are_python_only(tmp_path: Path, capsys):
    actual = tmp_path / "actual.jsonl"
    actual.write_text("".join(json.dumps(_record(frame)) + "\n" for frame in (0, 15, 30)), encoding="utf-8")
    fixture = tmp_path / "fixture.json"
    fixture.write_text(json.dumps({"frames": [0, 15, 30]}), encoding="utf-8")

    assert main(["validate", str(actual)]) == 0
    assert json.loads(capsys.readouterr().out)["records"] == 3
    assert main(["verify-coverage", "--fixture", str(fixture), "--actual", str(actual)]) == 0
    assert json.loads(capsys.readouterr().out)["ok"] is True


def test_invalid_jsonl_fails_closed(tmp_path: Path, capsys):
    actual = tmp_path / "actual.jsonl"
    actual.write_text("{}\n", encoding="utf-8")
    assert main(["validate", str(actual)]) == 1
    assert json.loads(capsys.readouterr().out)["ok"] is False
