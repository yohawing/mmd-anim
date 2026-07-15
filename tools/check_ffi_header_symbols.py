#!/usr/bin/env python3
"""Verify Rust FFI exports match C header declarations in mmd_runtime.h."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIB_RS = ROOT / "crates/mmd-anim-ffi/src/lib.rs"
HEADER = ROOT / "crates/mmd-anim-ffi/include/mmd_runtime.h"

RUST_NO_MANGLE_RE = re.compile(r"#\[\s*(?:unsafe\s*\(\s*)?no_mangle\s*\)?\s*\]")
RUST_FN_RE = re.compile(r"fn\s+(mmd_runtime_\w+)\s*\(")
HEADER_FN_RE = re.compile(r"\b(mmd_runtime_\w+)\s*\(")

FORBIDDEN_DENSE_REDUCED_POSE_SYMBOLS = {
    "mmd_runtime_reduced_pose_sample",
}

UNITY_CONSTANTS = {
    "MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_TRANSLATION": 0,
    "MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_EULER": 1,
    "MMD_RUNTIME_UNITY_CURVE_MORPH_WEIGHT": 2,
    "MMD_RUNTIME_UNITY_CURVE_AXIS_X": 0,
    "MMD_RUNTIME_UNITY_CURVE_AXIS_Y": 1,
    "MMD_RUNTIME_UNITY_CURVE_AXIS_Z": 2,
    "MMD_RUNTIME_UNITY_CURVE_AXIS_NONE": 3,
}

UNITY_STRUCTS = {
    "MmdRuntimeFfiUnityCurveDescriptor": (
        "mmd_runtime_ffi_unity_curve_descriptor_t",
        [
            ("semantic", "u32"),
            ("target_index", "u32"),
            ("axis", "u32"),
            ("key_count", "usize"),
        ],
    ),
    "MmdRuntimeFfiUnityCurveKey": (
        "mmd_runtime_ffi_unity_curve_key_t",
        [
            ("time_seconds", "f32"),
            ("value", "f32"),
            ("in_tangent", "f32"),
            ("out_tangent", "f32"),
        ],
    ),
}

UNITY_FUNCTIONS = {
    "mmd_runtime_reduced_pose_unity_curve_count": (
        "status",
        [
            ("pose", "const_reduced_pose_ptr"),
            ("frames_per_second", "f32"),
            ("flip_z", "bool"),
            ("out_curve_count", "usize_ptr"),
        ],
    ),
    "mmd_runtime_reduced_pose_unity_curve_descriptor": (
        "status",
        [
            ("pose", "const_reduced_pose_ptr"),
            ("frames_per_second", "f32"),
            ("flip_z", "bool"),
            ("curve_index", "usize"),
            ("out_descriptor", "unity_descriptor_ptr"),
        ],
    ),
    "mmd_runtime_reduced_pose_unity_curve_keys": (
        "status",
        [
            ("pose", "const_reduced_pose_ptr"),
            ("frames_per_second", "f32"),
            ("flip_z", "bool"),
            ("curve_index", "usize"),
            ("out_keys", "unity_key_ptr"),
            ("out_key_capacity", "usize"),
            ("out_required_count", "usize_ptr"),
        ],
    ),
}


def rust_exported_symbols(text: str) -> set[str]:
    lines = text.splitlines()
    symbols: set[str] = set()
    for index, line in enumerate(lines):
        if RUST_NO_MANGLE_RE.fullmatch(line.strip()) is None:
            continue
        preceding = lines[max(0, index - 3) : index]
        if any(part.strip() == "#[cfg(test)]" for part in preceding):
            continue
        signature_parts: list[str] = []
        cursor = index + 1
        while cursor < len(lines):
            line = lines[cursor]
            if "{" in line:
                signature_parts.append(line.split("{", 1)[0].strip())
                break
            signature_parts.append(line.strip())
            cursor += 1
        else:
            raise ValueError(f"missing function body after no_mangle attribute at line {index + 1}")
        signature = " ".join(part for part in signature_parts if part)
        match = RUST_FN_RE.search(signature)
        if match is None:
            raise ValueError(f"could not parse Rust FFI signature near line {index + 1}: {signature!r}")
        symbols.add(match.group(1))
    return symbols


def header_declared_symbols(text: str) -> set[str]:
    stripped = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    stripped = re.sub(r"//.*?$", "", stripped, flags=re.MULTILINE)
    compact = " ".join(stripped.split())
    return set(HEADER_FN_RE.findall(compact))


def strip_c_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    return re.sub(r"//.*?$", "", text, flags=re.MULTILINE)


def canonical_rust_type(type_name: str) -> str:
    compact = " ".join(type_name.split())
    return {
        "u32": "u32",
        "usize": "usize",
        "f32": "f32",
        "bool": "bool",
        "MmdRuntimeStatus": "status",
        "*const MmdRuntimeReducedPose": "const_reduced_pose_ptr",
        "*mut usize": "usize_ptr",
        "*mut MmdRuntimeFfiUnityCurveDescriptor": "unity_descriptor_ptr",
        "*mut MmdRuntimeFfiUnityCurveKey": "unity_key_ptr",
    }.get(compact, compact)


def canonical_c_type(type_name: str) -> str:
    compact = re.sub(r"\s*\*\s*", "*", " ".join(type_name.split()))
    return {
        "uint32_t": "u32",
        "size_t": "usize",
        "float": "f32",
        "bool": "bool",
        "mmd_runtime_status_t": "status",
        "const mmd_runtime_reduced_pose_t*": "const_reduced_pose_ptr",
        "size_t*": "usize_ptr",
        "mmd_runtime_ffi_unity_curve_descriptor_t*": "unity_descriptor_ptr",
        "mmd_runtime_ffi_unity_curve_key_t*": "unity_key_ptr",
    }.get(compact, compact)


def rust_struct_fields(text: str, name: str) -> list[tuple[str, str]]:
    match = re.search(rf"pub struct {re.escape(name)}\s*\{{(.*?)\n\}}", text, re.DOTALL)
    if match is None:
        raise ValueError(f"missing Rust struct {name}")
    return [
        (field, canonical_rust_type(type_name))
        for field, type_name in re.findall(r"pub\s+(\w+)\s*:\s*([^,]+),", match.group(1))
    ]


def c_struct_fields(text: str, alias: str) -> list[tuple[str, str]]:
    stripped = strip_c_comments(text)
    tag = alias.removesuffix("_t")
    match = re.search(
        rf"typedef struct\s+{re.escape(tag)}\s*\{{(.*?)\}}\s*{re.escape(alias)}\s*;",
        stripped,
        re.DOTALL,
    )
    if match is None:
        raise ValueError(f"missing C struct {alias}")
    fields: list[tuple[str, str]] = []
    for declaration in match.group(1).split(";"):
        declaration = declaration.strip()
        if not declaration:
            continue
        field_match = re.fullmatch(r"(.+?)\s+(\w+)", declaration)
        if field_match is None:
            raise ValueError(f"could not parse C field in {alias}: {declaration!r}")
        fields.append((field_match.group(2), canonical_c_type(field_match.group(1))))
    return fields


def rust_function_shape(text: str, name: str) -> tuple[str, list[tuple[str, str]]]:
    match = re.search(
        rf'pub unsafe extern "C" fn {re.escape(name)}\s*\((.*?)\)\s*->\s*(\w+)',
        text,
        re.DOTALL,
    )
    if match is None:
        raise ValueError(f"missing Rust function {name}")
    params = [
        (param_name, canonical_rust_type(type_name))
        for param_name, type_name in re.findall(r"(\w+)\s*:\s*([^,]+),?", match.group(1))
    ]
    return canonical_rust_type(match.group(2)), params


def c_function_shape(text: str, name: str) -> tuple[str, list[tuple[str, str]]]:
    stripped = strip_c_comments(text)
    match = re.search(
        rf"(\w+)\s+{re.escape(name)}\s*\((.*?)\)\s*;", stripped, re.DOTALL
    )
    if match is None:
        raise ValueError(f"missing C function {name}")
    params: list[tuple[str, str]] = []
    for declaration in match.group(2).split(","):
        declaration = declaration.strip()
        param_match = re.fullmatch(r"(.+?[\s*])(\w+)", declaration)
        if param_match is None:
            raise ValueError(f"could not parse C parameter in {name}: {declaration!r}")
        params.append((param_match.group(2), canonical_c_type(param_match.group(1))))
    return canonical_c_type(match.group(1)), params


def check_unity_abi_shapes(rust_text: str, header_text: str) -> list[str]:
    errors: list[str] = []
    for name, expected in UNITY_CONSTANTS.items():
        rust_match = re.search(rf"pub const {name}: u32 = (\d+);", rust_text)
        header_match = re.search(rf"\b{name}\s*=\s*(\d+)", header_text)
        rust_value = int(rust_match.group(1)) if rust_match else None
        header_value = int(header_match.group(1)) if header_match else None
        if rust_value != expected or header_value != expected or rust_value != header_value:
            errors.append(
                f"constant {name}: Rust={rust_value}, header={header_value}, expected={expected}"
            )
    for rust_name, (c_alias, expected) in UNITY_STRUCTS.items():
        rust_fields = rust_struct_fields(rust_text, rust_name)
        c_fields = c_struct_fields(header_text, c_alias)
        if rust_fields != expected or c_fields != expected or rust_fields != c_fields:
            errors.append(
                f"struct {rust_name}/{c_alias}: Rust={rust_fields}, header={c_fields}, expected={expected}"
            )
    for name, expected in UNITY_FUNCTIONS.items():
        rust_shape = rust_function_shape(rust_text, name)
        c_shape = c_function_shape(header_text, name)
        if rust_shape != expected or c_shape != expected or rust_shape != c_shape:
            errors.append(
                f"function {name}: Rust={rust_shape}, header={c_shape}, expected={expected}"
            )
    return errors


def main() -> int:
    rust_text = LIB_RS.read_text(encoding="utf-8")
    header_text = HEADER.read_text(encoding="utf-8")

    rust_symbols = rust_exported_symbols(rust_text)
    header_symbols = header_declared_symbols(header_text)

    missing_in_header = sorted(rust_symbols - header_symbols)
    missing_in_rust = sorted(header_symbols - rust_symbols)
    forbidden_in_rust = sorted(FORBIDDEN_DENSE_REDUCED_POSE_SYMBOLS & rust_symbols)
    forbidden_in_header = sorted(FORBIDDEN_DENSE_REDUCED_POSE_SYMBOLS & header_symbols)
    shape_errors = check_unity_abi_shapes(rust_text, header_text)

    if missing_in_header or missing_in_rust or forbidden_in_rust or forbidden_in_header or shape_errors:
        print("FFI header symbol mismatch detected.", file=sys.stderr)
        if missing_in_header:
            print("\nExported in Rust but missing from mmd_runtime.h:", file=sys.stderr)
            for symbol in missing_in_header:
                print(f"  - {symbol}", file=sys.stderr)
        if missing_in_rust:
            print("\nDeclared in mmd_runtime.h but missing from Rust exports:", file=sys.stderr)
            for symbol in missing_in_rust:
                print(f"  - {symbol}", file=sys.stderr)
        if forbidden_in_rust or forbidden_in_header:
            print("\nDense reduced-pose output must not be public:", file=sys.stderr)
            for symbol in sorted(set(forbidden_in_rust + forbidden_in_header)):
                print(f"  - {symbol}", file=sys.stderr)
        if shape_errors:
            print("\nUnity reduced-curve ABI shape drift:", file=sys.stderr)
            for error in shape_errors:
                print(f"  - {error}", file=sys.stderr)
        return 1

    print(
        f"OK: {len(rust_symbols)} Rust FFI exports and Unity curve ABI shapes "
        "match mmd_runtime.h declarations."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
