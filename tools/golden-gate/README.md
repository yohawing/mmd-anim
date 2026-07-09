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
  "physics_penetration": false,
  "diagnose_case": null,
  "diagnose_frame": null,
  "diagnose_bone": null,
  "diagnose_eval_frame": null,
  "tolerances": {
    "max_abs_error_tolerance": 0.0,
    "translation_max_error_tolerance": 0.0,
    "translation_rms_error_tolerance": 0.0,
    "rotation_max_angle_rad_tolerance": 0.0,
    "rotation_rms_angle_rad_tolerance": 0.0,
    "penetration_max_depth_tolerance": 0.0,
    "bullet_penetration_max_depth_tolerance": 0.0,
    "penetrating_pair_count_tolerance": 0,
    "severe_pair_count_tolerance": 0,
    "penetrating_contact_count_tolerance": 0,
    "rigid_body_position_tolerance": 0.0,
    "rigid_body_rotation_tolerance": 0.0,
    "mismatch_count_tolerance": 0,
    "missing_tolerance": 0,
    "import_error_tolerance": 0
  },
  "options": {
    "allow_count_changes": false,
    "allow_skipped_target_changes": false,
    "required_physics_backend": null,
    "max_allowed_penetration_depth": null,
    "max_allowed_bullet_penetration_depth": null,
    "max_allowed_penetrating_pair_count": null,
    "max_allowed_severe_pair_count": null,
    "max_allowed_penetrating_contact_count": null
  }
}
```

For `physics-coarse` cases, the gate also compares translation / rotation max
and RMS metrics in both `summary` and `perCase`. For physics quality gates that
must use the native Bullet path, set `required_physics_backend` to
`"bullet-native"` and run with an `mmd_anim_bin` built with
`physics-bullet-native`, or another runner path that emits that backend.

The same comparison layer also accepts `verify --mode numeric --diagnose ...
--physics-penetration --json` reports as a baseline/current pair. For those
diagnostic reports it compares `summary.maxPenetrationDepth`,
`summary.maxBulletPenetrationDepth`, `summary.penetratingPairCount`,
`summary.severePairCount`, `summary.unconnectedPenetratingPairCount`,
`summary.unconnectedSeverePairCount`, and `summary.penetratingContactCount`
with the penetration-specific tolerances above. If an absolute safety limit is
needed, set the matching `max_allowed_*` option; those limits are enforced
against the current report regardless of the baseline. A useful first contact
gate is `max_allowed_bullet_penetration_depth: 0.0` and
`max_allowed_penetrating_contact_count: 0`, while joint-connected shape-proxy
overlaps can remain baseline-not-worse. Unconnected shape-proxy penetration
counts are compared with zero tolerance so a new unconnected overlap fails even
when total shape-proxy pair tolerances are relaxed.

If a penetration baseline includes `rigidBodies`, the gate also fixes each
baselined rigid body's identity (`index`, `name`, `boneIndex`, `mode`, `shape`)
and compares `positionWorld` / `rotationXyzw` component deltas using
`rigid_body_position_tolerance` and `rigid_body_rotation_tolerance`. Legacy
baselines without `rigidBodies` keep working; extra current rigid-body dumps are
ignored until a new body-transform baseline is accepted.

To make `golden-gate baseline` / `golden-gate gate` produce a penetration
diagnostic report directly, opt in with `physics_penetration` and identify the
diagnostic case/frame:

```json
{
  "manifest": "<path-to-local-golden-oracle>/manifests/physics-coarse.json",
  "baseline": "<path-to-local-golden-oracle>/reports/baseline-rem-tail-penetration.json",
  "report_dir": "<path-to-local-golden-oracle>/reports",
  "repo_root": "<path-to-mmd-anim>",
  "mmd_anim_bin": "<path-to-mmd-anim-built-with-physics-bullet-native>",
  "physics_penetration": true,
  "diagnose_case": "rem-tail",
  "diagnose_frame": "119",
  "diagnose_bone": "左Tail_19",
  "diagnose_eval_frame": null,
  "options": {
    "max_allowed_bullet_penetration_depth": 0.0,
    "max_allowed_penetrating_contact_count": 0
  }
}
```

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
