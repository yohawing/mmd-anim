# Changelog

## 0.1.0 - Unreleased

Initial experimental release of `mmd-anim`.

### Included

- Runtime evaluation crate for MMD model arenas, animation clips, morphs, append transforms, IK, and matrix outputs.
- Format crate for PMX, PMD, VMD, PMM, VPD, X, VAC, and NMD detection / parser DTO coverage, plus exporter roundtrip support for supported DTO slices.
- CLI crate for diagnostics, import summaries, parser/exporter summaries, roundtrip checks, and GoldenOracle-oriented local QA commands.
- Schema crate for MMDDumper / GoldenOracle JSONL and manifest parsing.
- C ABI and WASM wrapper crates kept in the workspace, but not published to crates.io for `0.1.0`.

### Limitations

- API, ABI, and WASM surfaces are experimental and may change before `0.2.0`.
- PMD runtime import is partial and does not claim full renderer-side PMD parity.
- PMM exporter is not provided until a full project graph DTO exists.
- GoldenOracle and real-asset corpus checks are maintainer-local QA references, not public release dependencies.
