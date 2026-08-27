# MMDDumper

`MMDDumper` records bone and morph states evaluated by MikuMikuDance 9.32 x64 as JSONL. The native components are repository-local; Python handles case preparation, MMD orchestration, and output validation.

## Build

Building requires Python 3.10, CMake, and a Windows C++ compiler. Run from the repository root:

```powershell
python MMDDumper/scripts/native_smoke.py
python MMDDumper/scripts/package_native.py
```

Outputs are written to the ignored `MMDDumper/out/` directory.

The MMD SDK library may be placed at `MMDDumper/lib/mmd/MMDExport.lib` to build the MMD-facing dumper DLL. `MMDExport.lib`, `MMDExport.h`, MMD itself, and other SDK files are local dependencies and are not distributed by this repository.

## Record with MMD

Copy [`case.example.json`](case.example.json), replace its absolute paths, and follow the [`mmd-oracle-runner` instructions](../tools/mmd-oracle-runner/README.md) to validate, prepare, and record the case.

Recording requires both safeguards:

- `recordOptIn` must be `true` in the case file.
- `MMD_DUMPER_ALLOW_MMD_LAUNCH` must be `1` in the recording process.

Configure the normal MMD executable with `MMD_DUMPER_MMD_EXE`. Use `--mmd-exe` only as a one-off override.

## Supported scope

The current preparation backend supports one PMX model and the bone and morph tracks from one body VMD. Camera, light, and self-shadow tracks fail closed. Property frames are reported and dropped because PMM preparation does not yet apply them.

MMD, models, motions, SDK files, and generated output must remain outside Git. This tool records MMD state; it does not by itself prove motion parity or physics correctness.
