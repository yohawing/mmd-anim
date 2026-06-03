# Changelog

## 0.1.0 - Unreleased

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
