# Changelog

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
- PMM exporter is not provided until a full project graph DTO exists.
- MMDDumper / GoldenOracle and real-asset corpus checks are maintainer-local QA references, not public release dependencies.
