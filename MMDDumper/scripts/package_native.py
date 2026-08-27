#!/usr/bin/env python3
"""Package locally-built MMDDumper native artifacts without Node.js."""

from __future__ import annotations

import json
import shutil
from pathlib import Path


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    out_dir = root / "out" / "mmd-oracle-dumper-package"
    artifacts = (
        ("mmd_oracle_dumper.dll", _built_artifact(root, "native-dll-smoke", "mmd_oracle_dumper.dll"), True),
        ("MSIMG32.dll", _built_artifact(root, "native-proxy-smoke", "MSIMG32.dll"), True),
        ("d3d9.dll", _built_artifact(root, "native-d3d9-smoke", "d3d9.dll"), False),
        ("Plugin/mmd_oracle_plugin.dll", _built_artifact(root, "native-mmdplugin-smoke", "mmd_oracle_plugin.dll"), False),
        ("oracle-v1.schema.json", root / "schema" / "oracle-v1.schema.json", True),
    )
    out_dir.mkdir(parents=True, exist_ok=True)
    copied: list[dict[str, str]] = []
    for relative, source, required in artifacts:
        if not source.is_file():
            if required:
                raise FileNotFoundError(f"required artifact is missing: {source}; run native_smoke.py first")
            continue
        destination = out_dir / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        copied.append({"name": relative, "source": str(source), "destination": str(destination)})

    readme = out_dir / "README.md"
    readme.write_text(
        "# MMDDumper native package\n\n"
        "Copy the MMDPlugin DLL into the local MMD `Plugin/` directory.\n"
        "The legacy proxy DLLs are retained for local diagnostics only.\n"
        "This package is repository-local and never installs files globally.\n",
        encoding="utf-8",
    )
    copied.append({"name": "README.md", "source": str(readme), "destination": str(readme)})
    schema = json.loads((root / "schema" / "oracle-v1.schema.json").read_text(encoding="utf-8"))
    manifest = {
        "name": "mmd-oracle-dumper",
        "schema": schema["$id"],
        "mmdVersion": "9.32-x64",
        "files": [{"name": item["name"], "destinationHint": "documentation" if item["name"].endswith((".json", ".md")) else "next to MikuMikuDance.exe"} for item in copied],
        "safety": {"genericInjector": False, "globalInstall": False, "networkAccess": False, "targetProcess": "MikuMikuDance.exe"},
    }
    (out_dir / "package-manifest.json").write_text(json.dumps(manifest, ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"ok": True, "outDir": str(out_dir), "files": [item["name"] for item in copied]}, ensure_ascii=True, indent=2))
    return 0


def _built_artifact(root: Path, build: str, name: str) -> Path:
    directory = root / "out" / build
    candidates = (directory / name, directory / "Release" / name)
    return next((path for path in candidates if path.is_file()), candidates[0])


if __name__ == "__main__":
    raise SystemExit(main())
