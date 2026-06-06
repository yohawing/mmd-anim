# Testing

This repository uses two levels of validation:

- repository-local tests and fixtures, run by Cargo;
- optional GoldenOracle comparisons, run from local GoldenOracle outputs or
  manifests.

## Repository Tests

Run the full Rust test suite:

```powershell
rtk cargo test --workspace
```

Run only the VMD/parser fixture tests:

```powershell
rtk cargo test -p mmd-anim-format
```

Repository-local fixtures live under crate `fixtures/` directories. For example,
`crates/mmd-anim-format/fixtures/vmd/simple_camera.vmd` is embedded into unit
tests with `include_bytes!`. These tests do not read GoldenOracle files at
runtime.

## GoldenOracle Comparisons

GoldenOracle comparisons are maintainer checks for comparing `mmd-anim` results
against data generated from MMD/MMDDumper. They are intentionally separate from
ordinary unit tests because they depend on local GoldenOracle data.

The supported comparison entry points currently include:

- `mmd-anim golden-ik-summary <golden-run-root>`
- `mmd-anim golden-parser-summary <golden-run-root>`
- `mmd-anim golden-ik-compare <golden-run-root> [sample-frame-offset]`
- `mmd-anim golden-ik-diagnose <golden-run-root> <case-name> <frame> <bone-name> [sample-frame-offset]`
- `mmd-anim compare-numeric <manifest.json>`

The motion/IK workflow is documented in `docs/GOLDEN_ORACLE_WORKFLOW.md`.

## Manifest-Based Numeric Compare

Manifest-based numeric compares take a JSON manifest path on the CLI. The
manifest uses a common shape: source files are declared under `assets`, expected
outputs under `oracle`, and comparison settings under `compare`. The CLI
resolves relative paths from the manifest file directory, evaluates the relevant
runtime path, and reports mismatches.

### Numeric Manifest

`compare-numeric` reads the unified `numeric-compare` manifest shape and
dispatches each case by `case.kind` and `compare.targets`.

Implemented runners:

- `camera-vmd`: compares VMD camera sampling against GoldenOracle camera frame
  output. It checks `distance`, `position`, `rotation`, `fov`, and
  `perspective`.
- `motion-numeric`: compares focused bone world matrices from PMX/PMD + VMD
  evaluation against GoldenOracle JSONL output.
- `physics-coarse`: currently uses the same focused bone world-matrix comparison
  path as `motion-numeric`; `rigidBodies` target data is reported as skipped
  until physics runtime comparison exists.

Example:

```powershell
rtk cargo run -p mmd-anim-cli -- compare-numeric path/to/GoldenOracle/manifests/camera-nanoem.json
```

The CLI expects a JSON object with `cases`.

```json
{
  "schemaVersion": 1,
  "kind": "numeric-compare",
  "backend": "native-nanoem",
  "defaults": {
    "outDir": "../runs/pmm-parameters",
    "samplePolicy": "manifest-frames"
  },
  "cases": [
    {
      "name": "case-name",
      "kind": "camera-vmd",
      "assets": {
        "model": null,
        "motion": null,
        "cameraMotion": "path/to/source-camera.vmd",
        "pmm": null
      },
      "oracle": {
        "path": "../runs/pmm-parameters/case-name/oracle.actual.json",
        "format": "frame-json"
      },
      "frames": [0, 1, 2],
      "compare": {
        "targets": ["camera"],
        "epsilon": 0.003
      }
    }
  ]
}
```

Case fields:

- `name`: required case name.
- `kind`: comparison case kind. Supported values are `camera-vmd`,
  `motion-numeric`, and `physics-coarse`.
- `assets.cameraMotion`: required source camera VMD path for `camera-vmd`.
- `assets.model`: required PMX/PMD path for `motion-numeric` and
  `physics-coarse`.
- `assets.motion`: required VMD path for `motion-numeric` and `physics-coarse`.
- `oracle.path`: expected oracle path.
- `oracle.format`: expected output format. `camera-vmd` expects `frame-json`;
  `motion-numeric` and `physics-coarse` expect `jsonl`.
- `frames`: sampled frames documented by the manifest. The current camera
  comparison reads the authoritative frame records from `oracle.path`.
- `compare.targets`: fields or domains to compare. Implemented targets are
  `camera` and `bones`; `morphs` and `rigidBodies` are accepted in manifests
  but currently reported as skipped.
- `compare.epsilon`: optional numeric epsilon. If omitted, the CLI default is
  `0.003`.

If `oracle.path` is omitted, `defaults.outDir` must be present. The CLI will
read:

```text
<defaults.outDir>/<case-name>/oracle.actual.json
```

The legacy `compare-camera-vmd-numeric` command is kept as a compatibility
alias, but new testing scripts should call `compare-numeric`. The CLI also still
accepts the older `cameraVmd` / `cameraMotion` and `output` case-level fields as
a compatibility fallback, but new manifests should use `assets.cameraMotion` and
`oracle.path`.

The expected oracle JSON must contain `frames`.

```json
{
  "frames": [
    {
      "frame": 0,
      "camera": {
        "distance": -45.0,
        "position": [0.0, 10.0, 0.0],
        "rotation": [0.0, 0.0, 0.0],
        "fov": 45,
        "perspective": true
      }
    }
  ]
}
```

The camera numeric comparison currently uses an absolute epsilon of `0.003` for
numeric fields.
