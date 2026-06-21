# Changelog

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
