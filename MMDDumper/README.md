# MMDDumper

`MMDDumper` is a repository-local native dumper that records bone and morph states evaluated by MikuMikuDance 9.32 x64 as JSONL. It builds a PMM scene from a PMX model and a body VMD motion, then records selected frames while MMD plays the scene. Node.js, npm, and pnpm are not required at runtime.

## Prerequisites

Preparation requires Rust and Python 3.10. Building the native binaries also requires CMake and a C++ compiler for the target environment.

Recording starts MMD in an interactive Windows desktop session. Do not add MMD, PMX, VMD, or other external assets to the repository. Reference them with absolute paths in the case file or environment configuration.

Configure the MMD executable once for the local machine:

```powershell
$env:MMD_DUMPER_MMD_EXE = (Resolve-Path .\MikuMikuDance.exe).Path
```

The `--mmd-exe` option remains available as a one-off override. The `MMD_DUMPER_MMD_EXE` environment variable is never committed to the repository.

## Build the native binaries

Run these commands from the repository root.

```powershell
python MMDDumper/scripts/native_smoke.py
python MMDDumper/scripts/package_native.py
```

Build and package outputs are written to `MMDDumper/out/`. This directory is excluded from Git.

When `MMDDumper/lib/mmd/MMDExport.lib` is available, the MMD-facing dumper DLL is built as well. SDK libraries and the MMD executable are not included in the package.

## Create a case

A case JSON file defines the input assets, evaluation frames, and output directory. The following example creates a case from `model.pmx` and `motion.vmd` in the current directory. Replace the `Resolve-Path` arguments when the assets are stored elsewhere.

```powershell
$Pmx = (Resolve-Path .\model.pmx).Path
$BodyVmd = (Resolve-Path .\motion.vmd).Path
$OutputRoot = Join-Path $PWD 'mmd-dumper-output'

@{
  schemaVersion = 1
  name = 'body-only'
  input = @{
    pmx = $Pmx
    bodyVmd = $BodyVmd
  }
  frames = @(0, 15, 30)
  outputRoot = $OutputRoot
  generatorBackend = 'python-rust'
  recordOptIn = $true
  dialogOptIn = $false
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath .\case.json -Encoding utf8
```

`frames` must contain unique, non-negative frame numbers. `outputRoot` must identify a case output directory rather than an existing file.

## Prepare the PMM scene

The Python runner validates the inputs, converts PMX text to MMD-compatible UTF-16LE when necessary, and invokes `mmd-anim-cli build-pmm`. The Rust implementation in `mmd-anim-format` writes the PMM binary; MMD is not launched during preparation.

```powershell
$CasePath = (Resolve-Path .\case.json).Path

uv run --project tools/mmd-oracle-runner mmd-oracle-runner validate --case $CasePath
uv run --project tools/mmd-oracle-runner mmd-oracle-runner prepare --case $CasePath
```

On success, `outputRoot/<case-name>/` contains `scene.pmm`, `fixture.json`, and `prepare-result.json`. The PMM stores a PMX reference path so MMD can reload the model.

The current `python-rust` backend supports bone and morph tracks from a single model's body VMD. Cases containing camera, light, self-shadow, property, multi-model, or accessory tracks are rejected during preparation. Camera VMD parsing and comparison exist in lower-level `mmd-anim` diagnostics, but end-to-end camera-VMD PMM preparation and MMD recording are not part of this workflow.

## Record through MMD

Recording requires two explicit opt-ins: set `recordOptIn` to `true` in the case and set the launch guard immediately before starting the command.

```powershell
$CasePath = (Resolve-Path .\case.json).Path

$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = '1'
try {
  uv run --project tools/mmd-oracle-runner mmd-oracle-runner record --case $CasePath
} finally {
  Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH -ErrorAction SilentlyContinue
}
```

The runner starts MMD, advances to the requested frames, stops the process, and restores the native DLLs it installed. A successful JSONL dump is atomically promoted to `outputRoot/<case-name>/oracle.actual.jsonl`. The accompanying `oracle.actual.jsonl.done` and `record-result.json` files contain the validation results. A failed recording keeps the previous successful result and leaves a diagnostic `record-failure.zip`.

Use `--mmd-exe <absolute-path>` when a command must use a different executable than `MMD_DUMPER_MMD_EXE`.

## Run tests

Tests that do not start MMD can be run from the repository root.

```powershell
python -m pytest tools/mmd-oracle-runner -q
cargo test -p mmd-anim-cli
```

When changing the native build, also run `native_smoke.py` and `package_native.py` and inspect the generated package.

## Scope and safety

- MMD, models, motions, and SDK libraries are not committed. `MMDDumper/out/` and local assets are ignored by Git.
- MMD launch requires both `MMD_DUMPER_ALLOW_MMD_LAUNCH=1` and `recordOptIn: true`.
- Output paths are constrained to the case's `outputRoot`; input assets and the MMD executable are protected from overwrite.
- This tool records MMD state. It does not provide an automatic real-device motion parity verdict or guarantee physics correctness.
