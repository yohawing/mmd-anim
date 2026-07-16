#!/usr/bin/env python3
"""Check the Python ctypes ABI subset against mmd_runtime.h."""

from __future__ import annotations

import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PYTHON_ROOT = ROOT / "bindings" / "python"
HEADER = ROOT / "crates" / "mmd-anim-ffi" / "include" / "mmd_runtime.h"
sys.path.insert(0, str(PYTHON_ROOT))

from mmd_anim._abi import FUNCTION_SPECS, STRUCT_SPECS  # noqa: E402


def _strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    return re.sub(r"//.*?$", "", text, flags=re.MULTILINE)


def _normalize_type(value: str) -> str:
    value = " ".join(value.split())
    value = re.sub(r"\s*\*\s*", "*", value)
    value = re.sub(r"\s*\[\s*(\d+)\s*\]", r"[\1]", value)
    return value


def parse_structs(text: str) -> dict[str, tuple[tuple[str, str], ...]]:
    stripped = _strip_comments(text)
    structs: dict[str, tuple[tuple[str, str], ...]] = {}
    pattern = re.compile(
        r"typedef\s+struct\s+\w+\s*\{(.*?)\}\s*(mmd_runtime_\w+_t)\s*;",
        re.DOTALL,
    )
    for body, alias in pattern.findall(stripped):
        fields: list[tuple[str, str]] = []
        for declaration in body.split(";"):
            declaration = declaration.strip()
            if not declaration:
                continue
            match = re.fullmatch(r"(.+?)\s+(\w+)(\s*\[\s*\d+\s*\])?", declaration)
            if match is None:
                raise ValueError(f"could not parse {alias} field: {declaration!r}")
            field_type = match.group(1) + (match.group(3) or "")
            fields.append((match.group(2), _normalize_type(field_type)))
        structs[alias] = tuple(fields)
    return structs


def parse_functions(text: str) -> dict[str, tuple[str, tuple[str, ...]]]:
    stripped = _strip_comments(text)
    functions: dict[str, tuple[str, tuple[str, ...]]] = {}
    pattern = re.compile(
        r"([^;{}#]+?)\s+(mmd_runtime_\w+)\s*\((.*?)\)\s*;", re.DOTALL
    )
    for return_type, name, parameters in pattern.findall(stripped):
        argument_types: list[str] = []
        parameters = parameters.strip()
        if parameters and parameters != "void":
            for declaration in parameters.split(","):
                match = re.fullmatch(
                    r"(.+?[\s*])\w+(\s*\[\s*\d+\s*\])?",
                    declaration.strip(),
                )
                if match is None:
                    raise ValueError(
                        f"could not parse {name} parameter: {declaration.strip()!r}"
                    )
                argument_type = match.group(1)
                if match.group(2):
                    argument_type += "*"
                argument_types.append(_normalize_type(argument_type))
        functions[name] = (
            _normalize_type(return_type),
            tuple(argument_types),
        )
    return functions


def check_header(header_text: str) -> tuple[list[str], list[str]]:
    """Return fatal wrapped-ABI errors and non-fatal unwrapped symbols."""

    actual_functions = parse_functions(header_text)
    actual_structs = parse_structs(header_text)
    errors: list[str] = []
    for name, expected in FUNCTION_SPECS.items():
        actual = actual_functions.get(name)
        if actual is None:
            errors.append(f"missing wrapped function: {name}")
        elif actual != expected:
            errors.append(f"function {name}: header={actual}, Python={expected}")
    for name, expected in STRUCT_SPECS.items():
        actual = actual_structs.get(name)
        if actual is None:
            errors.append(f"missing wrapped struct: {name}")
        elif actual != expected:
            errors.append(f"struct {name}: header={actual}, Python={expected}")
    unwrapped = sorted(set(actual_functions) - set(FUNCTION_SPECS))
    return errors, unwrapped


def main() -> int:
    errors, unwrapped = check_header(HEADER.read_text(encoding="utf-8"))
    if errors:
        print("Python ABI drift detected:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print(
        f"OK: {len(FUNCTION_SPECS)} Python-wrapped functions and "
        f"{len(STRUCT_SPECS)} structs match mmd_runtime.h "
        f"({len(unwrapped)} unwrapped functions allowed)."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
