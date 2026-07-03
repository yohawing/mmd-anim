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


def main() -> int:
    rust_text = LIB_RS.read_text(encoding="utf-8")
    header_text = HEADER.read_text(encoding="utf-8")

    rust_symbols = rust_exported_symbols(rust_text)
    header_symbols = header_declared_symbols(header_text)

    missing_in_header = sorted(rust_symbols - header_symbols)
    missing_in_rust = sorted(header_symbols - rust_symbols)

    if missing_in_header or missing_in_rust:
        print("FFI header symbol mismatch detected.", file=sys.stderr)
        if missing_in_header:
            print("\nExported in Rust but missing from mmd_runtime.h:", file=sys.stderr)
            for symbol in missing_in_header:
                print(f"  - {symbol}", file=sys.stderr)
        if missing_in_rust:
            print("\nDeclared in mmd_runtime.h but missing from Rust exports:", file=sys.stderr)
            for symbol in missing_in_rust:
                print(f"  - {symbol}", file=sys.stderr)
        return 1

    print(
        f"OK: {len(rust_symbols)} Rust FFI exports match mmd_runtime.h declarations."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
