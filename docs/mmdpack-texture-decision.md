# MMDPACK Texture Phase 0 Decision

This is maintainer-only Phase 0 evidence. It is not a frozen V1 codec or `.mmdpack` container decision.

- Run ID: `20260828T050837367Z-c2269e64`
- Timing policy: single measured iteration; OS/file caches uncontrolled; no warmup or repeat; timings are directional.
- Manifest SHA-256: `96994ef6599f483657c6275baef2c4a5b78f00673db4483729818867b64fdeaa`
- Standalone Cargo.lock SHA-256: `07c0111c4f5c304568678254c97e0b80d3004320a0b385a0fc5a394387f10172`
- Harness source digest: `fee3663b7a139ed290d288ee291b2b3ed9c076660798ce662a8296c8fce5e0c4`
- Native Basis Universal: v2.50.0, executable SHA-256 `38572d6c218e65c25e5d8efa6adac4cc40ba0a8489e63ff2ec709d0a8995998a`
- Three.js transcoder revision: `three.js b5673888ec8b7ff279a93135c50bdb07f1900dba`, JS SHA-256 `9042facffa0b63ec1c897919c5db43f000c3dee03d9698b6b2465bb06446d298`, WASM SHA-256 `6cf17dc889352c42e9acf8897107978d127005fe3386c36a0e3845e27967630a`
- Environment: Windows native lane and Node v22.14.0; MSRV 1.87 build not verified on this host; measured with Rust 1.98.0.
- Encoding config: UASTC LDR 4x4, `uastc_level=2`, RDO lambda `1.0`, mipmaps enabled, Candidate A KTX2 without Zstandard plus probe-level Zstandard 3, Candidate B KTX2 internal Zstandard 3.
- Observable input evidence: each candidate was supplied through one WASM input call, and input bytes are counted; JS/WASM copy counts, heap/RSS peaks are unavailable and not claimed.

## Decision

Decision blocked: the configured Three.js transcoder and pinned Basis Universal v2.50 encoder output were not compatible (`KTX2File.isValid() == false`). A known-good control was not run, so adapter/tool-version causes were not isolated; the smallest missing evidence is a compatible, reviewed transcode control. No candidate is recommended.

## Case measurements

| Case | Class | Source | Dimensions | A raw payload | A KTX2 reconstructed | B KTX2 | A/B internal compressed | Native A/B ms | WASM A/B ms | A/B decoded | Quality evidence |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| `toon-small-b` | toon | 403 B / `e993c4faa91d…` | 64×64 | 1364 B | 5784 B | 764 B | 328 / 436 B | 91.547 / 93.382 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `toon-small-sample` | toon | 2163 B / `428a346be7e9…` | 256×256 | 1959 B | 87752 B | 1310 B | 685 / 934 B | 93.088 / 91.947 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `alpha-feathers` | alpha_cutout | 18413 B / `e3af9b373f06…` | 512×256 | 26902 B | 175168 B | 26843 B | 25514 / 26443 B | 94.008 / 95.114 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `diffuse-small-yukichi` | diffuse | 16994 B / `c3be7fec8eaa…` | 256×256 | 27094 B | 87752 B | 26565 B | 25820 / 26189 B | 92.103 / 92.227 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `diffuse-small-a1` | diffuse | 224879 B / `aab950f9715d…` | 512×512 | 208355 B | 349920 B | 206509 B | 206962 / 206109 B | 113.121 / 114.171 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `normal-small-emitter` | normal | 213795 B / `d184c48a4ed3…` | 512×512 | 180356 B | 349920 B | 179856 B | 178961 / 179456 B | 112.019 / 115.674 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `diffuse-medium-body` | diffuse | 365963 B / `8b5526e31e1c…` | 1024×1024 | 619003 B | 1398520 B | 622719 B | 617486 / 622295 B | 613.386 / 596.836 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `normal-medium-body` | normal | 2087845 B / `34c328293158…` | 2048×2048 | 2395938 B | 5592848 B | 2400712 B | 2394297 / 2400264 B | 10661.952 / 8900.742 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `diffuse-medium-kabo` | diffuse | 3931555 B / `c6db606f94a1…` | 2048×2048 | 2566249 B | 5592848 B | 2568308 B | 2564610 / 2567860 B | 9063.055 / 9410.332 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |
| `diffuse-large-bilijiang` | diffuse | 440054 B / `76ad0dcc8090…` | 4096×4096 | 533414 B | 22370088 B | 534158 B | 531652 / 533686 B | 56367.443 / 56267.075 | unavailable | unavailable | same raw UASTC mip bytes; source metric N/A |

## Evidence limits

- Candidate A is a conservative probe payload: JSON metadata plus compressed raw blocks are counted together. This is not an estimate of a future minimal production representation; reconstruction retains no uncounted KTX2 template bytes.
- Candidate A and B raw UASTC levels were byte-identical for every native case. This proves candidate decode equivalence, not source-image quality. Source-quality metrics were unavailable in this bounded probe because a PNG decoder/quality reference was not added.
- Native output hashes are RGBA32 DDS mip payload hashes. WASM output hashes use Three.js Basis transcoder format id 13 (RGBA32) when compatible.
- Raw JSON and Markdown are published atomically per file through same-volume replacement; the pair is not transactional.
- All reported paths are manifest labels/IDs only; local asset paths and detailed command errors remain in ignored run artifacts.
