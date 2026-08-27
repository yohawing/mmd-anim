#!/usr/bin/env python3
"""Build and test MMDDumper native targets with CMake."""

from __future__ import annotations

import shutil
import subprocess
import os
from pathlib import Path

_GENERATOR_ARGS: tuple[str, ...] = ()

def main() -> int:
    root = Path(__file__).resolve().parents[1]
    if shutil.which("cmake") is None:
        print('{"ok": true, "skipped": true, "reason": "cmake not found"}')
        return 0
    global _GENERATOR_ARGS
    if any(shutil.which(command) for command in ("cl", "clang++", "g++", "c++")):
        _GENERATOR_ARGS = ("-G", "Ninja")
    elif os.name == "nt" and (generator := _visual_studio_generator()) is not None:
        _GENERATOR_ARGS = ("-G", generator, "-A", "x64")
    else:
        print('{"ok": true, "skipped": true, "reason": "no C++ compiler found in PATH"}')
        return 0
    out = root / "out"
    writer = out / "native-smoke"
    _configure_build(root, writer, "mmd_oracle_jsonl_writer_test", "-DMMD_ORACLE_BUILD_DLL=OFF", "-DMMD_ORACLE_BUILD_MSIMG32_PROXY=OFF")
    _run(("ctest", "--test-dir", str(writer), "--output-on-failure", "-C", "Release"), root)
    _require_built(writer, "mmd_oracle_jsonl_writer_test.exe", "mmd_oracle_jsonl_writer_test")

    mmd_export = root / "lib" / "mmd" / "MMDExport.lib"
    dll_built = False
    if mmd_export.is_file():
        dll = out / "native-dll-smoke"
        _configure_build(root, dll, "mmd_oracle_dumper", "-DMMD_ORACLE_BUILD_DLL=ON", "-DMMD_ORACLE_BUILD_MSIMG32_PROXY=OFF", f"-DMMD_EXPORT_LIB={mmd_export}")
        _require_built(dll, "mmd_oracle_dumper.dll")
        dll_built = True
    proxy = out / "native-proxy-smoke"
    _configure_build(root, proxy, "msimg32", "-DMMD_ORACLE_BUILD_DLL=OFF", "-DMMD_ORACLE_BUILD_MSIMG32_PROXY=ON")
    _require_built(proxy, "MSIMG32.dll")
    d3d9 = out / "native-d3d9-smoke"
    _configure_build(root, d3d9, "d3d9", "-DMMD_ORACLE_BUILD_DLL=OFF", "-DMMD_ORACLE_BUILD_D3D9_PROXY=ON")
    _require_built(d3d9, "d3d9.dll")
    print(f'{{"ok": true, "dllBuilt": {str(dll_built).lower()}, "writer": "{writer}"}}')
    return 0


def _configure_build(root: Path, build: Path, target: str, *definitions: str) -> None:
    cache = build / "CMakeCache.txt"
    if cache.is_file():
        cache_text = cache.read_text(encoding="utf-8", errors="replace")
        requested_generator = "Visual Studio" if any("Visual Studio" in arg for arg in _GENERATOR_ARGS) else "Ninja"
        if "MMDDumper/native" not in cache_text or requested_generator not in cache_text:
            shutil.rmtree(build)
    _run(("cmake", "-S", str(root / "native"), "-B", str(build), *_GENERATOR_ARGS, *definitions), root)
    _run(("cmake", "--build", str(build), "--target", target, "--config", "Release"), root)


def _run(command: tuple[str, ...], cwd: Path) -> None:
    try:
        completed = subprocess.run(command, cwd=cwd, check=False, timeout=120)
    except subprocess.TimeoutExpired:
        print("native smoke command timed out after 120 seconds", flush=True)
        raise SystemExit(124)
    if completed.returncode != 0:
        raise SystemExit(completed.returncode)


def _require_built(build: Path, *names: str) -> None:
    paths = tuple(directory / name for directory in (build, build / "Release") for name in names)
    if not any(path.is_file() for path in paths):
        raise FileNotFoundError("expected native artifact was not created: " + ", ".join(str(path) for path in paths))


def _visual_studio_generator() -> str | None:
    candidates = []
    for variable in ("ProgramFiles", "ProgramFiles(x86)"):
        value = os.environ.get(variable)
        if value:
            candidates.append(Path(value) / "Microsoft Visual Studio" / "Installer" / "vswhere.exe")
    for path in candidates:
        if not path.is_file():
            continue
        result = subprocess.run((str(path), "-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"), capture_output=True, check=False, timeout=10, text=True)
        if result.returncode != 0:
            continue
        major = result.stdout.strip().split("\\")[-2] if result.stdout.strip() else ""
        if major == "18":
            return "Visual Studio 18 2026"
        if major == "17":
            return "Visual Studio 17 2022"
        return "Visual Studio 17 2022"
    return None


if __name__ == "__main__":
    raise SystemExit(main())
