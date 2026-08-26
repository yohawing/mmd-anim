#!/usr/bin/env python3
"""Node-free MMDDumper host helper.

This file deliberately uses only the Python standard library.  It validates
oracle JSONL and owns the small amount of process/file lifecycle required to
launch MMD with the repository-built native dumper.
"""

from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="oracle_cli.py")
    commands = parser.add_subparsers(dest="command", required=True)
    record = commands.add_parser("record")
    record.add_argument("--fixture", required=True)
    record.add_argument("--accept-dialog", default="false")
    validate = commands.add_parser("validate")
    validate.add_argument("actual")
    coverage = commands.add_parser("verify-coverage")
    coverage.add_argument("--fixture", required=True)
    coverage.add_argument("--actual", required=True)
    coverage.add_argument("--require-camera", default="false")
    args = parser.parse_args(argv)
    try:
        if args.command == "record":
            return _record(Path(args.fixture), args.accept_dialog.lower() == "true")
        if args.command == "validate":
            report = _validate_path(Path(args.actual))
        else:
            report = _coverage(Path(args.fixture), Path(args.actual), args.require_camera.lower() == "true")
        _print(report)
        return 0 if report.get("ok") is True else 1
    except TimeoutError as error:
        _print({"ok": False, "error": str(error)})
        return 124
    except (OSError, ValueError, json.JSONDecodeError) as error:
        _print({"ok": False, "error": str(error)})
        return 1


def _record(fixture_path: Path, accept_dialog: bool) -> int:
    if os.environ.get("MMD_DUMPER_ALLOW_MMD_LAUNCH") != "1":
        raise ValueError("Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for this local run.")
    fixture = _read_object(fixture_path)
    exe = _required_path(fixture, "mmdExe")
    project = _required_path(fixture, "project")
    output = _required_path(fixture, "output", allow_missing=True)
    done = _required_path(fixture, "done", allow_missing=True)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.unlink(missing_ok=True)
    done.unlink(missing_ok=True)
    package_root = Path(__file__).resolve().parents[1] / "out" / "mmd-oracle-dumper-package"
    trigger = str(fixture.get("trigger", "first-load"))
    install = _install_native(package_root, exe.parent, trigger)
    child: subprocess.Popen[bytes] | None = None
    try:
        timeout_ms = int(fixture.get("timeoutMs", 60000))
        environment = os.environ.copy()
        environment.update({
            "MMD_ORACLE_DUMP_PATH": str(output),
            "MMD_ORACLE_PROJECT_PATH": str(project),
            "MMD_ORACLE_PROXY_LOG_PATH": str(output) + ".proxy.log",
            "MMD_ORACLE_DUMP_ON_PROXY_LOAD": "1",
            "MMD_ORACLE_DUMP_ON_D3D9": "1",
            "MMD_ORACLE_DUMP_ON_MMDPLUGIN": "1",
            "MMD_ORACLE_DUMP_FRAMES": ",".join(str(int(frame)) for frame in fixture.get("frames", [])),
            "MMD_ORACLE_D3D9_SAMPLER_MS": str(timeout_ms),
            "MMD_ORACLE_D3D9_SAMPLER_INTERVAL_MS": "50",
        })
        child = subprocess.Popen([str(exe), str(project)], cwd=exe.parent, env=environment, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        timeout = timeout_ms / 1000
        deadline = time.monotonic() + timeout
        _wait_for_mmd_ready(child, accept_dialog, deadline)
        time.sleep(min(3.0, max(0.0, deadline - time.monotonic())))
        records = _drive_frames(child, [int(frame) for frame in fixture.get("frames", [])], output, deadline - time.monotonic())
        _stop_child(child)
        done.write_text(json.dumps({"ok": True, "mode": "python-mmd", "records": len(records), "output": str(output)}) + "\n", encoding="utf-8")
        _print({"ok": True, "records": len(records), "output": str(output), "done": str(done)})
        return 0
    finally:
        try:
            if child is not None:
                _stop_child(child)
        finally:
            _restore_native(install)


def _stop_child(child: subprocess.Popen[bytes]) -> None:
    if child.poll() is not None:
        return
    child.terminate()
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait(timeout=5)


def _install_native(package_root: Path, mmd_root: Path, trigger: str) -> list[tuple[Path, Path | None]]:
    names = ["mmd_oracle_dumper.dll", "MSIMG32.dll", "d3d9.dll"]
    if trigger == "mmdplugin":
        names = ["Plugin/mmd_oracle_plugin.dll"]
    installed: list[tuple[Path, Path | None]] = []
    for relative in names:
        source = package_root / relative
        if not source.is_file():
            raise ValueError(f"missing packaged native file: {source}; build MMDDumper/native first")
        destination = mmd_root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        backup = destination.with_name(destination.name + ".mmd-oracle-backup")
        backup.unlink(missing_ok=True)
        original: Path | None = None
        if destination.exists():
            destination.replace(backup)
            original = backup
        shutil.copy2(source, destination)
        installed.append((destination, original))
    return installed


def _restore_native(installed: list[tuple[Path, Path | None]]) -> None:
    for destination, original in reversed(installed):
        try:
            destination.unlink(missing_ok=True)
            if original is not None and original.exists():
                original.replace(destination)
        except OSError as error:
            # Preserve the diagnostic on disk; never remove a destination we
            # cannot safely restore.
            print(f"mmd-python:restore-error destination={destination} error={error}", file=sys.stderr)


def _wait_for_records(path: Path, timeout: float, minimum: int) -> list[dict[str, Any]]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.is_file():
            try:
                records = _read_records(path)
            except ValueError:
                # The native writer may be between two line writes.  Retry
                # until the deadline; the final validation remains strict.
                records = []
            if len(records) >= minimum:
                return records
        time.sleep(0.1)
    raise TimeoutError(f"timed out waiting for {minimum} oracle records: {path}")


def _drive_frames(child: subprocess.Popen[bytes], frames: list[int], output: Path, timeout: float) -> list[dict[str, Any]]:
    """Start MMD playback and wait until every requested frame is dumped."""
    if not frames:
        raise ValueError("fixture.frames must contain at least one frame")
    deadline = time.monotonic() + timeout
    _wait_for_records(output, min(timeout, 10.0), 1)
    _send_key(child.pid, 0x50)  # P: Play/Stop
    actual: set[int] = set()
    while time.monotonic() < deadline:
        try:
            records = _read_records(output)
        except ValueError:
            records = []
        actual = {int(round(float(record["frame"]))) for record in records}
        if all(frame in actual for frame in frames):
            return records
        if child.poll() is not None or (os.name == "nt" and not _visible_windows(child.pid)):
            raise ValueError(f"MMD exited before all requested frames were dumped: missing {[frame for frame in frames if frame not in actual]}")
        time.sleep(0.1)
    missing = [frame for frame in frames if frame not in actual]
    raise TimeoutError(f"timed out waiting for oracle frames {missing}: {output}")


def _wait_for_mmd_ready(child: subprocess.Popen[bytes], accept_dialog: bool, deadline: float) -> None:
    if os.name != "nt":
        return
    saw_window = False
    while time.monotonic() < deadline:
        windows = _visible_windows(child.pid)
        if windows:
            saw_window = True
        elif saw_window or child.poll() is not None:
            raise ValueError("MMD exited before its main window became ready")
        dialogs = [window for window in windows if window[1] == "#32770"]
        if dialogs:
            if not accept_dialog:
                raise ValueError(f"MMD requires a dialog; rerun with --accept-dialog true: {dialogs[0][2]}")
            if not any(_accept_model_structure_dialog(hwnd) for hwnd, _, _ in dialogs):
                raise ValueError(f"MMD showed an unsupported dialog: {dialogs[0][2]}")
        elif any(class_name == "Polygon Movie Maker" for _, class_name, _ in windows):
            return
        time.sleep(0.1)
    raise TimeoutError("timed out waiting for the MMD main window")


def _visible_windows(pid: int) -> list[tuple[int, str, str]]:
    user32 = ctypes.windll.user32
    windows: list[tuple[int, str, str]] = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
    def callback(candidate, _lparam):
        process_id = ctypes.c_ulong()
        user32.GetWindowThreadProcessId(candidate, ctypes.byref(process_id))
        if process_id.value == pid and user32.IsWindowVisible(candidate):
            windows.append((int(candidate), _window_string(user32.GetClassNameW, candidate), _window_string(user32.GetWindowTextW, candidate)))
        return True

    user32.EnumWindows(callback, 0)
    return windows


def _accept_model_structure_dialog(dialog: int) -> bool:
    user32 = ctypes.windll.user32
    accepted = False

    @ctypes.WINFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p)
    def callback(candidate, _lparam):
        nonlocal accepted
        title = _window_string(user32.GetWindowTextW, candidate)
        if title.startswith("このモデルにpmmファイルのモデル情報を適応させて続行"):
            user32.PostMessageW(candidate, 0x00F5, 0, 0)
            accepted = True
            return False
        return True

    user32.EnumChildWindows(dialog, callback, 0)
    return accepted


def _window_string(getter, hwnd: int) -> str:
    value = ctypes.create_unicode_buffer(512)
    getter(hwnd, value, len(value))
    return value.value


def _send_key(pid: int, virtual_key: int) -> None:
    if os.name != "nt":
        return
    user32 = ctypes.windll.user32
    main_windows = [hwnd for hwnd, class_name, _ in _visible_windows(pid) if class_name == "Polygon Movie Maker"]
    if not main_windows:
        raise ValueError("MMD main window is not visible")
    hwnd = main_windows[0]
    user32.ShowWindowAsync(hwnd, 9)
    user32.SetForegroundWindow(hwnd)
    rect = wintypes.RECT()
    if user32.GetWindowRect(hwnd, ctypes.byref(rect)):
        user32.SetCursorPos((rect.left + rect.right) // 2, (rect.top + rect.bottom) // 2)
        user32.mouse_event(0x0002, 0, 0, 0, 0)
        user32.mouse_event(0x0004, 0, 0, 0, 0)
    time.sleep(0.1)
    scan = user32.MapVirtualKeyW(virtual_key, 0)
    user32.keybd_event(virtual_key, scan, 0, 0)
    time.sleep(0.02)
    user32.keybd_event(virtual_key, scan, 0x0002, 0)


def _coverage(fixture_path: Path, actual_path: Path, require_camera: bool) -> dict[str, Any]:
    fixture = _read_object(fixture_path)
    records = _read_records(actual_path)
    expected = [int(frame) for frame in fixture.get("frames", [])]
    actual = [int(record["frame"]) for record in records]
    missing = [frame for frame in expected if frame not in actual]
    report: dict[str, Any] = {"ok": not missing, "records": len(records), "expectedFrames": expected, "actualFrames": actual, "missingFrames": missing}
    if require_camera:
        missing_camera = [record["frame"] for record in records if not isinstance(record.get("camera"), dict) or record["camera"].get("available") is not True]
        report["camera"] = {"ok": not missing_camera, "missingFrames": missing_camera}
        report["ok"] = report["ok"] and not missing_camera
    return report


def _validate_path(path: Path) -> dict[str, Any]:
    records = _read_records(path)
    return {"ok": True, "records": len(records)}


def _read_records(path: Path) -> list[dict[str, Any]]:
    if not path.is_file():
        raise ValueError(f"oracle JSONL does not exist: {path}")
    records: list[dict[str, Any]] = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"{path}:{line_number}: invalid JSON: {error}") from error
        _validate_record(value, f"{path}:{line_number}")
        records.append(value)
    if not records:
        raise ValueError(f"oracle JSONL is empty: {path}")
    return records


def _validate_record(value: Any, label: str) -> None:
    if not isinstance(value, dict):
        raise ValueError(f"{label}: record must be an object")
    if value.get("schemaVersion") != 1:
        raise ValueError(f"{label}: schemaVersion must be 1")
    if not isinstance(value.get("source"), dict) or not isinstance(value["source"].get("mmdVersion"), str) or not isinstance(value["source"].get("dumperVersion"), str):
        raise ValueError(f"{label}: source.mmdVersion and source.dumperVersion are required strings")
    if not isinstance(value.get("frame"), (int, float)) or isinstance(value.get("frame"), bool):
        raise ValueError(f"{label}: frame must be numeric")
    models = value.get("models")
    if not isinstance(models, list):
        raise ValueError(f"{label}: models must be an array")
    for model in models:
        if not isinstance(model, dict) or not isinstance(model.get("index"), int) or not isinstance(model.get("name"), str) or not isinstance(model.get("filename"), str) or not isinstance(model.get("visible"), bool):
            raise ValueError(f"{label}: model identity is invalid")
        if not isinstance(model.get("bones"), list) or not isinstance(model.get("morphs"), list):
            raise ValueError(f"{label}: model bones/morphs are required arrays")


def _read_object(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain an object")
    return value


def _required_path(value: dict[str, Any], key: str, *, allow_missing: bool = False) -> Path:
    raw = value.get(key)
    if not isinstance(raw, str) or not raw:
        raise ValueError(f"fixture.{key} must be a non-empty path")
    path = Path(raw).resolve()
    if not allow_missing and not path.is_file():
        raise ValueError(f"fixture.{key} does not exist: {path}")
    return path


def _print(value: dict[str, Any]) -> None:
    print(json.dumps(value, ensure_ascii=True, sort_keys=True, allow_nan=False))


if __name__ == "__main__":
    raise SystemExit(main())
