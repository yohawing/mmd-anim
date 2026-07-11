# Changelog

## 0.2.0 - 2026-07-11

FBX conversion, optional native Bullet physics integration, PMX IK axis
correctness, and release-quality local regression gates.

### Added

- Added FBX skeleton and animation conversion, including bones-only export,
  readable bone-name policy, diffuse texture copying, vertex BlendShapes, and
  morph-weight curves validated in Maya, Unity, and Blender.
- Added an optional native Bullet bridge and feature-gated C ABI for typed
  physics-world creation, reset, stepping, diagnostics, and sequential clip
  baking. The default FFI build remains Bullet-free.
- Added local GoldenOracle physics release gates and benchmark-history tooling
  for explicit baseline-not-worse checks.

### Changed

- Extended PMX runtime import and IK evaluation to honor local-axis and
  fixed-axis constraints, including rotated base poses and combined angle
  limits.
- Made the first sequential physics-bake sample seed-only after world creation
  or reset, so frame zero initializes from the evaluated animation pose without
  advancing simulation.
- Expanded the experimental host-facing FFI and WASM validation surface while
  preserving caller-owned hot-path output buffers.

### Fixed

- Fixed physics frame-zero initialization that could advance Bullet before the
  intended evaluated pose had seeded the world.
- Fixed fixed-axis IK correction loss when a non-identity base rotation and
  angle limits were active together.
- Fixed PowerShell 7 FFI release smoke runs treating Cargo's normal stderr
  progress output as a native-command failure.

### Known limitations

- GoldenOracle comparisons and real-asset physics baselines remain
  maintainer-local and are not required for normal crate tests.
- Some Unity-reference rigid-body diagnostics still contain known host/harness
  residuals; the accepted v0.2.0 local release baselines gate regressions from
  the current runtime rather than claiming cross-host bitwise parity.
- The FFI, WASM, and optional native physics surfaces remain experimental.

## 0.1.9 - 2026-07-03

CLI/API brush-up, FFI hardening, typed diagnostics, and CI-built CLI release
assets.

### Added

- Added PMX parts export, VMD sample export, VMD DTO export round-trip coverage,
  and runtime batch import JSON support to `mmd-anim-cli`.
- Added PMX geometry handle accessors, skinning-mode reporting, and parser
  model-name retention for host diagnostics.
- Added typed numeric and GoldenOracle comparison reports for summary,
  per-case, unsupported-case, root-motion, lag, IK residual, and import
  diagnostics.
- Added tagged-release CLI binary assets for Linux and Windows, with
  `SHA256SUMS`, to support other projects consuming `mmd-anim` from CI.

### Changed

- Split large CLI, runtime, PMM, PMX, FFI, and format helper implementations
  into smaller modules while preserving the external command/API surface.
- Shared byte-reader, Shift-JIS, format-writer, flat-model, runtime IK, morph,
  and world-matrix helpers across parser/runtime/FFI code paths.
- Moved CLI support helpers out of `main.rs` and adopted `anyhow` for
  incremental CLI error handling cleanup.

### Fixed

- Hardened FFI exported functions with panic/error guards and added
  `mmd_runtime_last_error_message` for native-host diagnostics.
- Added a CI header-symbol check so Rust FFI exports stay synchronized with the
  public C header.
- Fixed review-gate numeric/parser API issues and tightened compare report
  sampling behavior.

## 0.1.8 - 2026-06-29

Camera, light, and self-shadow sampling APIs and GoldenOracle regression gate
tooling.

### Added

- Added VMD camera sampling helpers to `mmd-anim-format`, with interpolated
  distance, position, rotation, FOV, and perspective state.
- Added caller-owned output-buffer C ABI sampling APIs for VMD camera, light,
  and self-shadow tracks, plus one-shot helpers:
  `mmd_runtime_vmd_sample_camera`, `mmd_runtime_vmd_sample_light`, and
  `mmd_runtime_vmd_sample_self_shadow`.
- Added caller-owned `Float32Array` WASM sampling APIs for VMD camera, light,
  and self-shadow tracks: `sampleVmdCamera`, `sampleVmdLight`,
  `sampleVmdSelfShadow`, `WasmVmdCameraTrack`, `WasmVmdLightTrack`, and
  `WasmVmdSelfShadowTrack`.
- Added `camera-numeric-dump` handling to numeric compare reports so
  `camera.current` GoldenOracle output can be gated through the same report
  shape as motion numeric checks.
- Added the `tools/golden-gate` Python gate for baseline-not-worse local
  release checks.

### Changed

- Replaced the pre-release JSON-returning and array-returning camera sampling
  APIs with caller-owned output-buffer APIs before the `0.1.8` release tag.
  This keeps hot-path C ABI and WASM sampling allocation-free.
- Documented intentional pre-release error-surface changes: WASM flat model
  validation reports bone input errors before IK errors, and truncated NMD
  payloads now surface as `UnexpectedEof` instead of `SectionOverflow`.

## 0.1.7 - 2026-06-27

Runtime IK/append correctness, batch evaluation APIs, schema crate
consolidation, and repository cleanup.

### Added

- Added batch parallel clip evaluation APIs for C ABI (`mmd-anim-ffi`) and
  WASM (`mmd-anim-wasm`), enabling multi-clip evaluation in a single call.
  FFI uses Rayon thread-pool parallelism; WASM evaluates sequentially.
- Added `--json` flag to `verify --mode numeric` for structured JSON report
  output. JSON mode is report-only (exit 0 regardless of mismatches) to
  support external gate tooling.
- Added PMX runtime metadata accessors: `PmxBoneFlags`, bone flag queries,
  and morph metadata for host-side inspection.
- Added WASM smoke harness (`harness/smoke.mjs`) for batch evaluation
  testing.

### Changed

- Absorbed `mmd-anim-schema` crate into `mmd-anim-cli`. Oracle and fixture
  types moved to `cli/src/schema.rs` and `cli/src/mmd_dumper_oracle.rs`;
  schema crate removed from workspace.
- Removed public local-only smoke artifacts: root `scripts/`, C# FFI smoke
  harness, PMM inspect/provenance fixtures, and hardcoded local `F:` paths.
- Moved `RELEASE.md` and `TESTING.md` to gitignored `docs/` as local-only
  development documents.

### Fixed

- Fixed PMX ordered append/IK evaluation: transitive append targets are now
  recomputed after IK source rotation changes across evaluation phases,
  fixing incorrect arm poses (e.g. Kotora forearm).
- Consolidated PMM internal helper naming and removed dead inspect code.

## 0.1.6 - 2026-06-25

Patch release for CLI overhaul, rig primitives, crates.io CLI publishing, and
PMX roundtrip fixes.

### Added

- Restructured CLI from flat subcommands to operation-based groups (parse,
  export, import, compare, bench, rig, golden, oracle, patch) with clap derive.
- Added magic byte format detection so the CLI can identify files without
  relying on file extensions.
- Added IK and append transform evaluation primitives extracted from the
  runtime, enabling host-side rig solvers to call individual solve steps.
- Added rig primitive C ABI (`mmd-anim-ffi`) and PMX rig spec extraction for
  native host integration.
- Added `rig inspect` CLI command for diagnosing rig specs from PMX files.
- Enabled crates.io publishing for `mmd-anim-cli` and `mmd-anim-schema`.
  `mmd-anim-cli` is now installable via `cargo install mmd-anim-cli`.

### Fixed

- Fixed PMX roundtrip DTO precision loss where vertex deform types
  (BDEF1/BDEF2/SDEF/QDEF) were not preserved, causing all vertices to be
  re-exported as BDEF4.
- Fixed JSON roundtrip serialization of non-finite f32 values (NaN, Infinity,
  -Infinity) in PMX DTOs. Added `json_f32` serde helper that encodes these as
  JSON strings and restores them on deserialization.
- Improved CLI output readability and help text.

## 0.1.5 - 2026-06-21

Patch release for PMX material split host import and parser API parity.

### Added

- Added `split_pmx_model_by_material` and `PmxMaterialSplit` DTOs to
  `mmd-anim-format` for Maya-style per-material mesh import. Each split mesh
  remaps vertices to local indices and prunes/remaps morphs: vertex, UV, and
  material morphs are filtered to the split mesh and remapped to local indices;
  group/flip morphs are pruned to surviving children through a fixpoint pass
  that preserves forward references; bone morphs are excluded as
  skeleton-shared, and impulse morphs are diagnostic-only.
- Added an opaque PMX material split C ABI handle to `mmd-anim-ffi`
  (`create`/`free`/`mesh_count`/`manifest_json` plus mesh-indexed geometry
  getters). `manifest_json` carries `originalMaterialIndex`,
  `originalVertexIndices`, `morphIndexMap`, and diagnostics.
- Added one-shot PMX geometry C ABI getters for parity with `WasmPmxGeometry`:
  `edge_scale`, additional UVs (with count, per channel), `material_groups`,
  SDEF `rw0`/`rw1`, and `qdef_enabled`.
- Added `parseVmdAnimationJson` and `WasmPmxGeometry.skinningModes` to
  `mmd-anim-wasm` for parser surface parity with the C ABI.

## 0.1.4 - 2026-06-19

Patch release for host-facing parser JSON and PMX geometry FFI.

### Added

- Added native parser FFI for VMD JSON, PMX non-geometry JSON, PMX skinning-mode
  JSON, and flat PMX geometry buffers so Unity and other hosts can reuse the
  Rust parsers without embedding Unity-side JSON parser implementations.

## 0.1.3 - 2026-06-12

Patch release for limited PMM writing and RabbitHole IK stability.

### Added

- Added limited PMM manifest/header export for the PMM data currently preserved
  by `PmmParsedManifest`, including project settings, initial model slots, and
  asset references.
- Added `export-pmm-scene` CLI support for creating a limited PMMv2 scene from a
  PMX model and VMD motion, with a reparse check and export summary.

### Changed

- Updated constrained IK solving to apply multi-axis limits during each link
  step and to use Saba-style total-axis solving for single-axis plane links,
  improving RabbitHole leg stability without the experimental TwoBone path.

### Fixed

- Fixed a RabbitHole regression where the left knee could visibly jitter around
  frames 780-800 after repeated IK iterations.

## 0.1.2 - 2026-06-06

Patch release for IK runtime tuning, diagnostics, and maintainer numeric
comparison coverage.

### Added

- Added configurable IK solve options for Rust runtime, C FFI, and WASM hosts:
  tolerance and an optional maximum iteration cap.
- Added IK runtime statistics so hosts and CLI diagnostics can inspect solver
  evaluations, executed iterations, tolerance breaks, rollback breaks, and max
  iteration exhaustions.
- Added PMX/VMD pair benchmark diagnostics, including REM Miku profiling support
  and PMX IK iteration summaries.
- Added unified `compare-numeric` maintainer diagnostics for camera, motion, and
  physics-coarse GoldenOracle manifests.
- Added a checked-in synthetic camera VMD fixture and documented maintainer
  testing workflow in `docs/TESTING.md`.

### Changed

- Relaxed the default IK tolerance from `1.0e-4` to `1.0e-2` to reduce wasted
  iterations after practical convergence.
- Optimized IK world-matrix updates to recompute only the affected evaluation
  suffix while preserving default solver semantics.

### Fixed

- Fixed `compare-numeric` mixed-kind manifest dispatch so each case is handled by
  its own `case.kind`.
- Fixed motion numeric comparison to fail on epsilon mismatches, missing inputs,
  and import errors instead of reporting success with only `maxAbsError`.

## 0.1.1 - 2026-06-05

Patch release for parser and host-facing ABI improvements.

### Added

- Added split PMX WASM geometry APIs so host integrations can fetch large geometry buffers separately from non-geometry JSON.
- Added parsed PMX SDEF and QDEF geometry fields, including SDEF helper vectors and QDEF active flags.
- Added repository policy automation to fail `main`-targeted pull requests unless they come from `develop`.

### Fixed

- Hardened PMX and VMD count validation.
- Updated PMX geometry test fixtures for the expanded parsed geometry shape.
- Rewrote the Japanese README in plainer language.

## 0.1.0 - 2026-06-04

Initial experimental release of `mmd-anim`.

### Included

- `mmd-anim`, the umbrella crate that re-exports the public runtime and format crates.
- `mmd-anim-runtime`, for MMD model arenas, animation clips, morphs, append transforms, IK, and matrix outputs.
- `mmd-anim-format`, for PMX, PMD, VMD, PMM, VPD, X, and VAC detection / parser DTO coverage, plus exporter roundtrip support for supported DTO slices.
- Workspace-local CLI, schema, C ABI, and WASM crates remain in the repository, but are not published to crates.io for `0.1.0`.

### Limitations

- API, ABI, and WASM surfaces are experimental and may change before `0.2.0`.
- PMD runtime import is partial and does not claim full renderer-side PMD parity.
- Full semantic PMM project graph exporter is not provided until a full project
  graph DTO exists. Lossless parsed-byte export for parsed PMM data and limited
  scene export exist; full graph export remains unfinished.
- MMDDumper / GoldenOracle and real-asset corpus checks are maintainer-local QA references, not public release dependencies.
