#!/usr/bin/env python3
"""Run the native Python/C ABI release gate.

This is intentionally shared by CI and tagged-release workflows.  Keeping the
manifest checks, release cdylib build/existence check, and native Python tests
in one command prevents a release job from accidentally omitting one leg of
the ABI gate while still allowing the matrix job to select its native C
compiler.
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
TOOLS = ROOT / "tools"
TARGET_RELEASE = ROOT / "target" / "release"


def run(label: str, command: list[str], *, env: dict[str, str] | None = None) -> None:
    print(f"[ffi-release-gate] {label}", flush=True)
    completed = subprocess.run(command, cwd=ROOT, env=env, check=False)
    if completed.returncode:
        raise SystemExit(
            f"[ffi-release-gate] {label} failed with exit code {completed.returncode}"
        )


def release_library_path() -> Path:
    if sys.platform == "win32":
        return TARGET_RELEASE / "mmd_runtime_ffi.dll"
    if sys.platform == "darwin":
        return TARGET_RELEASE / "libmmd_runtime_ffi.dylib"
    return TARGET_RELEASE / "libmmd_runtime_ffi.so"


def main() -> int:
    python = sys.executable
    run(
        "C header symbol check",
        [python, str(TOOLS / "check_ffi_header_symbols.py")],
    )
    run(
        "Python ABI drift check",
        [python, str(TOOLS / "check_python_abi_drift.py")],
    )

    descriptor_env = os.environ.copy()
    # CI's Windows matrix has an MSVC developer shell; Ubuntu uses GCC.  Do
    # not let a runner-level CC override silently turn this into a different
    # compiler check or an optional skip.
    descriptor_env["CC"] = "cl" if os.name == "nt" else "gcc"
    run(
        "model descriptor ABI layout (required C compiler)",
        [
            python,
            str(TOOLS / "check_model_descriptor_abi.py"),
            "--require-c-compiler",
        ],
        env=descriptor_env,
    )

    run(
        "release mmd-anim-ffi cdylib build",
        ["cargo", "build", "-p", "mmd-anim-ffi", "--release", "--locked"],
    )
    library = release_library_path()
    if not library.is_file():
        raise SystemExit(f"[ffi-release-gate] release cdylib not found: {library}")
    print(f"[ffi-release-gate] release cdylib: {library}", flush=True)

    # Do not let unittest's optional native-smoke skip choose an unrelated
    # environment override (or pass merely because target/release exists).
    # The tests must load the exact artifact built by this gate.
    native_env = os.environ.copy()
    native_env["MMD_RUNTIME_LIBRARY"] = str(library)
    run(
        "native Python binding tests",
        [
            python,
            "-m",
            "unittest",
            "discover",
            "-s",
            "bindings/python/tests",
            "-v",
        ],
        env=native_env,
    )
    print("[ffi-release-gate] passed", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
