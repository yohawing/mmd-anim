#!/usr/bin/env python3
"""Validate the version 1 model-descriptor ABI manifest on the host.

The JSON manifest is consumed by the Python ctypes declarations and by the
Rust layout tests.  This checker adds a third leg: it compares ctypes layouts,
header declarations/constants, and (when a C compiler is available) a small
real C translation unit containing ``sizeof``/``alignof``/``offsetof`` static
assertions.  The C compiler step is deliberately optional so the workspace
stays usable on Windows machines without a C toolchain.
"""

from __future__ import annotations

import argparse
import ctypes
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
HEADER = ROOT / "crates" / "mmd-anim-ffi" / "include" / "mmd_runtime.h"
PYTHON_ROOT = ROOT / "bindings" / "python"
sys.path.insert(0, str(PYTHON_ROOT))

from mmd_anim._abi import MODEL_DESCRIPTOR_MANIFEST, STRUCT_TYPES  # noqa: E402
from check_python_abi_drift import check_header  # noqa: E402


def _platform_key() -> str | None:
    if sys.platform == "win32" and ctypes.sizeof(ctypes.c_void_p) == 8:
        return "windows-x86_64"
    if sys.platform.startswith("linux") and ctypes.sizeof(ctypes.c_void_p) == 8:
        return "ubuntu-x86_64"
    return None


def _header_macro_value(header: str, name: str) -> int | None:
    match = re.search(rf"^#define\s+{re.escape(name)}\s+(.+?)\s*$", header, re.MULTILINE)
    if match is None:
        return None
    expression = match.group(1).replace("u", "").replace("U", "").strip()
    shift = re.fullmatch(r"\(\s*1\s*<<\s*(\d+)\s*\)", expression)
    if shift:
        return 1 << int(shift.group(1))
    number = re.fullmatch(r"\(?\s*(\d+)\s*\)?", expression)
    return int(number.group(1)) if number else None


def _ctypes_layout_errors(manifest: dict[str, object]) -> list[str]:
    errors: list[str] = []
    target = _platform_key()
    platforms = manifest["platforms"]
    if target is None:
        print("SKIP: model descriptor layout target is not Windows/Ubuntu x86_64")
    else:
        platform = platforms[target]  # type: ignore[index]
        if ctypes.sizeof(ctypes.c_void_p) != platform["pointer_size"]:  # type: ignore[index]
            errors.append(f"ctypes pointer size does not match {target}")
        if ctypes.sizeof(ctypes.c_size_t) != platform["size_t_size"]:  # type: ignore[index]
            errors.append(f"ctypes size_t size does not match {target}")

    for record in manifest["records"]:  # type: ignore[index]
        name = record["name"]  # type: ignore[index]
        cls = STRUCT_TYPES[name]
        expected_size = int(record["sizeof"])  # type: ignore[index]
        expected_align = int(record["alignof"])  # type: ignore[index]
        if ctypes.sizeof(cls) != expected_size:
            errors.append(f"{name}: sizeof={ctypes.sizeof(cls)}, expected={expected_size}")
        if ctypes.alignment(cls) != expected_align:
            errors.append(f"{name}: alignof={ctypes.alignment(cls)}, expected={expected_align}")
        for field in record["fields"]:  # type: ignore[index]
            field_name = field["name"]  # type: ignore[index]
            expected_offset = int(field["offset"])  # type: ignore[index]
            actual_offset = getattr(cls, field_name).offset
            if actual_offset != expected_offset:
                errors.append(
                    f"{name}.{field_name}: offset={actual_offset}, expected={expected_offset}"
                )
    return errors


def _c_static_assert_source(manifest: dict[str, object]) -> str:
    lines = [
        '#include "mmd_runtime.h"',
        "#include <stddef.h>",
        "#include <stdalign.h>",
    ]
    for record in manifest["records"]:  # type: ignore[index]
        c_name = record["name"]  # type: ignore[index]
        token = c_name.removesuffix("_t")
        lines.append(
            f'_Static_assert(sizeof({c_name}) == {int(record["sizeof"])}, "{token} sizeof");'  # type: ignore[index]
        )
        lines.append(
            f'_Static_assert(alignof({c_name}) == {int(record["alignof"])}, "{token} alignof");'  # type: ignore[index]
        )
        for field in record["fields"]:  # type: ignore[index]
            lines.append(
                f'_Static_assert(offsetof({c_name}, {field["name"]}) == {int(field["offset"])}, "{token} {field["name"]} offset");'  # type: ignore[index]
            )
    return "\n".join(lines) + "\n"


def _run_c_static_asserts(manifest: dict[str, object]) -> tuple[bool, str]:
    configured = os.environ.get("CC")
    compiler = None
    if configured:
        configured_path = Path(configured)
        if configured_path.is_file():
            compiler = configured_path
        else:
            resolved = shutil.which(configured)
            if resolved:
                compiler = Path(resolved)
    if compiler is None:
        for candidate in ("cc", "gcc", "clang", "cl", "cl.exe", "clang-cl"):
            resolved = shutil.which(candidate)
            if resolved:
                compiler = Path(resolved)
                break
    if compiler is None:
        return False, "no C compiler found; C static assertions skipped"

    compiler_name = compiler.name.lower()
    with tempfile.TemporaryDirectory(prefix="mmd-model-descriptor-abi-") as directory:
        directory_path = Path(directory)
        source = directory_path / "layout.c"
        output = directory_path / "layout.o"
        source.write_text(_c_static_assert_source(manifest), encoding="utf-8")
        include = HEADER.parent
        if compiler_name in {"cl", "cl.exe", "clang-cl", "clang-cl.exe"}:
            command = [str(compiler), "/nologo", "/std:c11", "/c", str(source), f"/I{include}", f"/Fo{output}"]
        else:
            command = [str(compiler), "-std=c11", "-Werror", "-c", str(source), "-I", str(include), "-o", str(output)]
        completed = subprocess.run(
            command,
            capture_output=True,
            text=True,
            errors="replace",
            check=False,
        )
        if completed.returncode:
            detail = (completed.stderr or completed.stdout).strip()
            return True, f"C static assertions failed ({compiler.name}): {detail}"
    return True, f"C static assertions passed ({compiler.name})"


def check_descriptor_abi() -> list[str]:
    manifest = MODEL_DESCRIPTOR_MANIFEST
    errors = _ctypes_layout_errors(manifest)
    header = HEADER.read_text(encoding="utf-8")
    header_errors, _ = check_header(header)
    errors.extend(header_errors)

    if manifest["abi_version"] != 2:
        errors.append(f"manifest abi_version={manifest['abi_version']}, expected=2")
    feature = manifest["feature"]
    if feature["value"] != feature["bit"]:  # type: ignore[index]
        errors.append("manifest feature bit/value mismatch")
    if _header_macro_value(header, feature["name"]) != feature["value"]:  # type: ignore[index]
        errors.append("header model-descriptor feature macro differs from manifest")
    if _header_macro_value(header, "MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1") != manifest["descriptor_version"]:
        errors.append("header descriptor version differs from manifest")
    for name, value in manifest["flags"].items():  # type: ignore[index]
        actual = _header_macro_value(header, name)
        if actual != value:
            errors.append(f"header flag {name}={actual!r}, manifest={value!r}")
    return errors


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--require-c-compiler",
        action="store_true",
        help="fail when the real C static-assert translation unit cannot be compiled",
    )
    options = parser.parse_args(argv)
    errors = check_descriptor_abi()
    compiler_available, compiler_result = _run_c_static_asserts(MODEL_DESCRIPTOR_MANIFEST)
    if not compiler_available:
        if options.require_c_compiler:
            errors.append(compiler_result)
        else:
            print(f"INFO: {compiler_result}")
    elif not compiler_result.startswith("C static assertions passed"):
        errors.append(compiler_result)
    else:
        print(f"OK: {compiler_result}")
    if errors:
        print("Model descriptor ABI drift detected:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print(
        f"OK: ABI v{MODEL_DESCRIPTOR_MANIFEST['abi_version']} descriptor v{MODEL_DESCRIPTOR_MANIFEST['descriptor_version']}, "
        f"{len(MODEL_DESCRIPTOR_MANIFEST['records'])} records, feature bit {MODEL_DESCRIPTOR_MANIFEST['feature']['value']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
