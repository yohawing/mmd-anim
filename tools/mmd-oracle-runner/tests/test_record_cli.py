from __future__ import annotations

import json
from pathlib import Path

from mmd_oracle_runner.cli import main
from mmd_oracle_runner.record_artifacts import resolve_mmd_exe
from prepare_test_support import write_case


def test_record_cli_forwards_explicit_mmd_executable(tmp_path: Path, monkeypatch, capsys):
    case_path = write_case(tmp_path)
    mmd_exe = tmp_path / "MikuMikuDance.exe"
    captured = {}

    def fake_record(case, executable):
        captured.update(case=case, executable=executable)
        return {"ok": True, "phase": "complete"}

    monkeypatch.setattr("mmd_oracle_runner.cli.record_case", fake_record)

    assert main(["record", "--case", str(case_path), "--mmd-exe", str(mmd_exe)]) == 0
    assert captured["case"].source_path == case_path.resolve()
    assert captured["executable"] == str(mmd_exe)
    assert json.loads(capsys.readouterr().out)["ok"] is True


def test_resolve_mmd_exe_uses_environment_default_and_explicit_override(tmp_path: Path):
    configured = tmp_path / "configured" / "MikuMikuDance.exe"
    override = tmp_path / "override" / "MikuMikuDance.exe"
    configured.parent.mkdir()
    override.parent.mkdir()
    configured.write_bytes(b"configured")
    override.write_bytes(b"override")
    environment = {"MMD_DUMPER_MMD_EXE": str(configured)}

    assert resolve_mmd_exe(None, environment=environment) == configured.resolve()
    assert resolve_mmd_exe(override, environment=environment) == override.resolve()


def test_record_cli_returns_one_for_non_pass_result(tmp_path: Path, monkeypatch, capsys):
    case_path = write_case(tmp_path)
    monkeypatch.setattr("mmd_oracle_runner.cli.record_case", lambda case, executable: {"ok": False, "phase": "coverage"})

    assert main(["record", "--case", str(case_path), "--mmd-exe", str(tmp_path / "MikuMikuDance.exe")]) == 1
    assert json.loads(capsys.readouterr().out)["phase"] == "coverage"


def test_record_cli_labels_case_validation_errors(tmp_path: Path, capsys):
    case_path = tmp_path / "invalid.json"
    case_path.write_text("{}", encoding="utf-8")

    assert main(["record", "--case", str(case_path), "--mmd-exe", str(tmp_path / "MikuMikuDance.exe")]) == 2
    assert json.loads(capsys.readouterr().err)["command"] == "record"
