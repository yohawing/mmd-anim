# golden-gate

Local NumericMotion GoldenOracle regression gate for `mmd-anim`.

This tool is pinned to Python 3.10 to stay compatible with FBX SDK 2020.3.9
Python bindings. It keeps real asset paths out of tracked files. Configure local
paths via `golden-gate.local.json`, environment variables, or CLI flags. CLI
flags win over environment variables, and environment variables win over the
local config file.

```powershell
cd tools/golden-gate
uv sync
uv run golden-gate baseline --config golden-gate.local.json
uv run golden-gate gate --config golden-gate.local.json
uv run pytest -q
```

Example local config:

```json
{
  "manifest": "<path-to-local-golden-oracle>/manifests/motion-numeric.json",
  "baseline": "<path-to-local-golden-oracle>/reports/baseline-numeric.json",
  "report_dir": "<path-to-local-golden-oracle>/reports",
  "repo_root": "<path-to-mmd-anim>",
  "mmd_anim_bin": null,
  "tolerances": {
    "max_abs_error_tolerance": 0.0,
    "translation_max_error_tolerance": 0.0,
    "translation_rms_error_tolerance": 0.0,
    "rotation_max_angle_rad_tolerance": 0.0,
    "rotation_rms_angle_rad_tolerance": 0.0,
    "mismatch_count_tolerance": 0,
    "missing_tolerance": 0,
    "import_error_tolerance": 0
  },
  "options": {
    "allow_count_changes": false,
    "allow_skipped_target_changes": false,
    "required_physics_backend": null
  }
}
```

For `physics-coarse` cases, the gate also compares translation / rotation max
and RMS metrics in both `summary` and `perCase`. For physics quality gates that
must use the native Bullet path, set `required_physics_backend` to
`"bullet-native"` and run with an `mmd_anim_bin` built with
`physics-bullet-native`, or another runner path that emits that backend.

If `mmd_anim_bin` or `MMD_ANIM_BIN` is set, the binary is used directly. Without
it, the runner uses:

```text
cargo run -q -p mmd-anim-cli -- verify <manifest> --mode numeric --json
```

`baseline` writes the current report to the configured baseline path and refuses
to replace an existing baseline unless `--replace` is passed. `gate` never
updates the baseline; it saves a timestamped current report and exits with
`0` for pass, `1` for regression, and `2` for input errors.

Camera current checks use the same runner with a camera-specific local config.
Keep this config and baseline outside tracked release inputs:

```powershell
uv run golden-gate gate --config golden-gate.camera-current.local.json
```

Example camera current config:

```json
{
  "manifest": "<path-to-local-golden-oracle>/manifests/camera_motion.json",
  "baseline": "<path-to-local-golden-oracle>/reports/baseline-camera-current.json",
  "report_dir": "<path-to-local-golden-oracle>/reports",
  "repo_root": "<path-to-mmd-anim>",
  "tolerances": {
    "max_abs_error_tolerance": 0.0,
    "mismatch_count_tolerance": 0,
    "missing_tolerance": 0,
    "import_error_tolerance": 0
  }
}
```
