# MMDPACK Texture Phase 0 Decision

This is maintainer-only Phase 0 evidence. It is not a frozen V1 codec or `.mmdpack` container decision.

- Run ID: `20260828T060314919Z-c03101f8`
- Timing policy: single measured iteration; OS/file caches uncontrolled; no warmup or repeat; timings are directional.
- Manifest SHA-256: `96994ef6599f483657c6275baef2c4a5b78f00673db4483729818867b64fdeaa`
- Standalone Cargo.lock SHA-256: `07c0111c4f5c304568678254c97e0b80d3004320a0b385a0fc5a394387f10172`
- Harness source digest: `ef3e6e27668a30e6ae36f2bc2b1431dd93ee00f6fe867140a3b1464999e70fd9`
- Native Basis Universal: v2.50.0, executable SHA-256 `38572d6c218e65c25e5d8efa6adac4cc40ba0a8489e63ff2ec709d0a8995998a`
- Three.js transcoder revision: `three.js b5673888ec8b7ff279a93135c50bdb07f1900dba`, JS SHA-256 `9042facffa0b63ec1c897919c5db43f000c3dee03d9698b6b2465bb06446d298`, WASM SHA-256 `6cf17dc889352c42e9acf8897107978d127005fe3386c36a0e3845e27967630a`
- Known-good control: `threejs-example-2d-uastc`, 6 RGBA32 mips, input SHA-256 `21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125`.
- Fixture provenance: Three.js revision `three.js b5673888ec8b7ff279a93135c50bdb07f1900dba`; repo-relative label `examples/textures/ktx2/2d_uastc.ktx2`.
- Aggregate coverage: 10 cases; Native A/B 103/103 mips (206 total); WASM A/B 103/103 mips (206 total).
- Environment: Windows native lane and Node v22.14.0; MSRV 1.87 build not verified on this host; measured with Rust 1.98.0.
- Encoding config: UASTC LDR 4x4, `uastc_level=2`, RDO lambda `1.0`, mipmaps enabled, Candidate A KTX2 without Zstandard plus probe-level Zstandard 3, Candidate B KTX2 internal Zstandard 3.
- Observable input evidence: each candidate was supplied through one WASM input call, and input bytes are counted; JS/WASM copy counts, heap/RSS peaks are unavailable and not claimed.

## Decision

Recommendation: KTX2 UASTC with internal Zstandard level 3. The pinned Three.js known-good control (6 mips) succeeded, both candidates decoded through the same lane-local A/B checks, and all candidate per-mip raw UASTC hashes agree. Candidate B is the standard KTX2 path and was smaller than Candidate A's reconstructed KTX2 in every case; Candidate A also requires custom probe metadata/reconstruction. Candidate A remains a conservative probe payload; this is Phase 0 evidence and does not freeze V1.

## Case measurements

| Case | Class | Source | Dimensions | A raw payload | A KTX2 reconstructed | B KTX2 | A/B internal compressed | Native A/B ms | WASM A/B ms | A/B decoded | Quality evidence |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| `toon-small-b` | toon | 403 B / `e993c4faa91d…` | 64×64 | 1364 B | 5784 B | 764 B | 328 / 436 B | 92.441 / 91.282 | 2.943 / 0.287 | match | same raw UASTC mip bytes; source metric N/A |
| `toon-small-sample` | toon | 2163 B / `428a346be7e9…` | 256×256 | 1959 B | 87752 B | 1310 B | 685 / 934 B | 92.962 / 92.385 | 0.747 / 0.658 | match | same raw UASTC mip bytes; source metric N/A |
| `alpha-feathers` | alpha_cutout | 18413 B / `e3af9b373f06…` | 512×256 | 26902 B | 175168 B | 26843 B | 25514 / 26443 B | 94.511 / 92.728 | 1.441 / 1.935 | match | same raw UASTC mip bytes; source metric N/A |
| `diffuse-small-yukichi` | diffuse | 16994 B / `c3be7fec8eaa…` | 256×256 | 27094 B | 87752 B | 26565 B | 25820 / 26189 B | 92.683 / 94.837 | 0.839 / 0.929 | match | same raw UASTC mip bytes; source metric N/A |
| `diffuse-small-a1` | diffuse | 224879 B / `aab950f9715d…` | 512×512 | 208355 B | 349920 B | 206509 B | 206962 / 206109 B | 111.199 / 110.804 | 4.193 / 4.776 | match | same raw UASTC mip bytes; source metric N/A |
| `normal-small-emitter` | normal | 213795 B / `d184c48a4ed3…` | 512×512 | 180356 B | 349920 B | 179856 B | 178961 / 179456 B | 111.209 / 111.042 | 4.100 / 3.803 | match | same raw UASTC mip bytes; source metric N/A |
| `diffuse-medium-body` | diffuse | 365963 B / `8b5526e31e1c…` | 1024×1024 | 619003 B | 1398520 B | 622719 B | 617486 / 622295 B | 609.743 / 606.502 | 12.461 / 13.797 | match | same raw UASTC mip bytes; source metric N/A |
| `normal-medium-body` | normal | 2087845 B / `34c328293158…` | 2048×2048 | 2395938 B | 5592848 B | 2400712 B | 2394297 / 2400264 B | 6049.181 / 6140.112 | 52.100 / 58.476 | match | same raw UASTC mip bytes; source metric N/A |
| `diffuse-medium-kabo` | diffuse | 3931555 B / `c6db606f94a1…` | 2048×2048 | 2566249 B | 5592848 B | 2568308 B | 2564610 / 2567860 B | 5896.277 / 5871.772 | 46.426 / 51.879 | match | same raw UASTC mip bytes; source metric N/A |
| `diffuse-large-bilijiang` | diffuse | 440054 B / `76ad0dcc8090…` | 4096×4096 | 533414 B | 22370088 B | 534158 B | 531652 / 533686 B | 59552.572 / 59446.713 | 93.042 / 93.201 | match | same raw UASTC mip bytes; source metric N/A |

## Evidence limits

- Candidate A is a conservative probe payload: JSON metadata plus compressed raw blocks are counted together. This is not an estimate of a future minimal production representation; reconstruction retains no uncounted KTX2 template bytes.
- Candidate A and B raw UASTC levels were byte-identical for every native case. This proves candidate decode equivalence, not source-image quality. Source-quality metrics were unavailable in this bounded probe because a PNG decoder/quality reference was not added.
- The size comparison favors standard Candidate B KTX2 files over the reconstructed Candidate A files in this campaign; Candidate A raw payload bytes are not a minimal-production estimate. Standard-loader control success and the extra custom A reconstruction path are the bounded interoperability/complexity evidence.
- Native A/B decoded DDS hashes agree within the Native lane; WASM A/B decoded RGBA32 hashes agree within the WASM lane. These are lane-local checks, not cross-lane byte parity claims.
- Raw JSON, decision Markdown, and control Markdown are published atomically per file through same-volume replacement; the three published artifacts are not a transactional set.
- All reported paths are manifest labels/IDs only; local asset paths and detailed command errors remain in ignored run artifacts.
