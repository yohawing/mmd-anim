from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "MMDDumper" / "scripts"))
import oracle_cli
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
    actual.write_text("".join(json.dumps(_record(frame)) + "\n" for frame in (0, 30)), encoding="utf-8")
    fixture = tmp_path / "fixture.json"
    fixture.write_text(json.dumps({"frames": [0, 15, 30]}), encoding="utf-8")

    assert main(["validate", str(actual)]) == 0
    assert json.loads(capsys.readouterr().out)["records"] == 2
    assert main(["verify-coverage", "--fixture", str(fixture), "--actual", str(actual)]) == 0
    report = json.loads(capsys.readouterr().out)
    assert report["ok"] is True
    assert report["missingFrames"] == [15]


def test_invalid_jsonl_fails_closed(tmp_path: Path, capsys):
    actual = tmp_path / "actual.jsonl"
    actual.write_text("{}\n", encoding="utf-8")
    assert main(["validate", str(actual)]) == 1
    assert json.loads(capsys.readouterr().out)["ok"] is False


def test_drive_frames_accepts_missing_intermediate_frame_after_reaching_end(monkeypatch: pytest.MonkeyPatch, tmp_path: Path):
    clicks: list[int] = []
    records = [{"frame": frame} for frame in (0, 30)]
    monkeypatch.setattr(oracle_cli, "_wait_for_records", lambda *_args: records[:1])
    monkeypatch.setattr(oracle_cli, "_read_records", lambda _path: records)
    monkeypatch.setattr(oracle_cli, "_click_play", lambda pid: clicks.append(pid))

    child = type("Child", (), {"pid": 42, "poll": lambda self: None})()
    result = oracle_cli._drive_frames(child, [0, 15, 30], tmp_path / "oracle.jsonl", 10)

    assert result == records
    assert clicks == [42]


def test_stop_child_waits_after_forced_kill():
    class Child:
        def __init__(self):
            self.calls: list[str] = []
            self.killed = False

        def poll(self):
            return None

        def terminate(self):
            self.calls.append("terminate")

        def wait(self, timeout: float):
            self.calls.append("wait")
            if not self.killed:
                raise subprocess.TimeoutExpired("MikuMikuDance.exe", timeout)

        def kill(self):
            self.calls.append("kill")
            self.killed = True

    child = Child()
    oracle_cli._stop_child(child)

    assert child.calls == ["terminate", "wait", "kill", "wait"]
