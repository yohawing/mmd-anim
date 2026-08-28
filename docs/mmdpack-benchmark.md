# MMDPACK Phase 0: compression and authenticated-encryption benchmark

> Measurement evidence only. The `.mmdpack` V1 format and backend choices remain unfrozen.

## Scope and configuration

Curated local PMX/VMD cases selected by deterministic size quantiles from the local MMD asset tree on 2026-08-28.

- Pipeline: `input -> Zstandard -> AES-256-GCM -> AES-256-GCM decrypt -> Zstandard decompress`
- Compression: `zstd crate 0.13.3 / zstd 1.5.7` at level `3`
- Encryption: `RustCrypto aes-gcm 0.10.3 (AES-256-GCM)`; key=32 bytes, nonce=12 bytes, tag=16 bytes
- AAD: `mmdpack-phase0/<case id>`
- Key policy: fresh random key and nonce per encrypt call; never serialized
- Timing policy: single measured iteration; OS/file caches uncontrolled; no warmup or repeat; timings are directional

- Run ID: `eceb092da2bb442b9904a901e64293ee`
- Measured at (UTC): `2026-08-28T02:10:54.8615211Z`
- Campaign manifest SHA-256: `feda593f762d4e5ddcc4d975da8aca5b75c250f31b1e3db8fc4bd6fce804470b`
- Standalone Cargo.lock SHA-256: `5268f4d3b3555433aea56f9fa9cb78c234321fd6baa875ad4831c4a4d862e1ff`
- wasm-pack: `wasm-pack 0.15.0`
- Harness source digest: `42fbc97b69b8bc0bafb2ef6ef7cc9bdd9ec74cee603211c744bd506821044f26`
- MSRV: MSRV 1.87 build not verified on this host; measured with rustc 1.98.0 (88d9e12ae 2026-08-18)

## Reproducibility metadata

| Lane | Platform | Toolchain | Runner |
|---|---|---|---|
| Native | `windows-x86_64` | `rustc 1.98.0 (88d9e12ae 2026-08-18)` | `package-codecs-native 0.1.0` |
| WASM/Node | `win32-x64` | `rustc 1.98.0 (88d9e12ae 2026-08-18)` | `Node.js v22.14.0; package-codecs-wasm 0.1.0` |

Generated raw JSON is local-only under `.ai/mmdpack/`; cryptographic keys/nonces and asset payloads are never written.

## Native results

| ID | Kind | Class | Input | Zstd | Ratio | Ciphertext | SHA-256 | Compress ms | Encrypt ms | Decrypt ms | Decompress ms | Pipeline ms | MiB/s | Round-trip | Wrong key | Tamper | Truncation |
|---|---|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---|---|---|---|
| `pmx-small-book` | pmx | small | 37360 | 10455 | 0.2798 | 10471 | `cc77518a29ce47f640eb487c4551c128ea7ca4d50bb190b0a8a14762bb7faf2d` | 0.183 | 0.152 | 0.011 | 0.062 | 0.409 | 87.07 | true | true | true | true |
| `pmx-small-weapon` | pmx | small | 757902 | 147395 | 0.1945 | 147411 | `413574822f823531aa32ed4c152abed9d667129ab031234aa86617b44d811190` | 1.216 | 0.117 | 0.091 | 0.483 | 1.908 | 378.88 | true | true | true | true |
| `pmx-medium-kizuna` | pmx | medium | 2741268 | 1661594 | 0.6061 | 1661610 | `33ba6869c1e428860bb8f47b25c3f5cc504d722aa069f1f6d2219307ef8328b1` | 10.302 | 1.088 | 1.013 | 3.027 | 15.431 | 169.42 | true | true | true | true |
| `pmx-large-nora` | pmx | large | 5084212 | 1683588 | 0.3311 | 1683604 | `15da7c1ba27a50ac91d64b2054082e1bb7d7e8aa9da494f3703879748031697f` | 13.506 | 1.089 | 1.006 | 3.989 | 19.590 | 247.50 | true | true | true | true |
| `pmx-large-quelle` | pmx | large | 11318431 | 6065961 | 0.5359 | 6065977 | `33e84626408dacd582ba7e154c298227c3a612624b4282dc9da78cd84938228c` | 41.237 | 3.961 | 3.576 | 12.534 | 61.309 | 176.06 | true | true | true | true |
| `vmd-small-camera` | vmd | small | 1904 | 498 | 0.2616 | 514 | `d1052ae0616f80063b5cebd9bf1d65367979965321224fc4652e21f8ebf0962e` | 0.046 | 0.010 | 0.001 | 0.012 | 0.069 | 26.35 | true | true | true | true |
| `vmd-small-camera-yumegita` | vmd | small | 29903 | 11496 | 0.3844 | 11512 | `deefd09c4c7e8e1ffeb165a44f4ad4565b65d9697d3d7e310d73559fe5406458` | 0.150 | 0.052 | 0.011 | 0.049 | 0.262 | 108.97 | true | true | true | true |
| `vmd-medium-best-friends` | vmd | medium | 851402 | 85814 | 0.1008 | 85830 | `0c38e485adf98188b4049801b2896f3bca02481f9c90f1d52a088b8e2af443b3` | 0.861 | 0.075 | 0.059 | 0.393 | 1.388 | 585.03 | true | true | true | true |
| `vmd-large-addiction` | vmd | large | 4470218 | 517754 | 0.1158 | 517770 | `a389220652869e98e373f73e62d774e8cbf62aeaa2ea12bdd9930b156690896f` | 4.112 | 0.359 | 0.325 | 2.351 | 7.147 | 596.48 | true | true | true | true |
| `vmd-large-kamippoi` | vmd | large | 28401530 | 3494164 | 0.1230 | 3494180 | `9569137d2f4f93b57547c38742823c0589bfd9670b236ed7de00b39355262e6b` | 26.959 | 2.198 | 2.047 | 12.264 | 43.467 | 623.13 | true | true | true | true |
Failures/skips: none.

## WASM/Node results

| ID | Kind | Class | Input | Zstd | Ratio | Ciphertext | SHA-256 | Compress ms | Encrypt ms | Decrypt ms | Decompress ms | Pipeline ms | MiB/s | Round-trip | Wrong key | Tamper | Truncation |
|---|---|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---|---|---|---|
| `pmx-small-book` | pmx | small | 37360 | 10455 | 0.2798 | 10471 | `cc77518a29ce47f640eb487c4551c128ea7ca4d50bb190b0a8a14762bb7faf2d` | 2.511 | 1.966 | 0.659 | 0.894 | 6.050 | 5.89 | true | true | true | true |
| `pmx-small-weapon` | pmx | small | 757902 | 147395 | 0.1945 | 147411 | `413574822f823531aa32ed4c152abed9d667129ab031234aa86617b44d811190` | 2.742 | 2.456 | 2.422 | 0.907 | 8.530 | 84.74 | true | true | true | true |
| `pmx-medium-kizuna` | pmx | medium | 2741268 | 1661594 | 0.6061 | 1661610 | `33ba6869c1e428860bb8f47b25c3f5cc504d722aa069f1f6d2219307ef8328b1` | 15.420 | 25.653 | 25.127 | 6.603 | 72.813 | 35.90 | true | true | true | true |
| `pmx-large-nora` | pmx | large | 5084212 | 1683588 | 0.3311 | 1683604 | `15da7c1ba27a50ac91d64b2054082e1bb7d7e8aa9da494f3703879748031697f` | 16.381 | 25.038 | 25.004 | 7.390 | 73.822 | 65.68 | true | true | true | true |
| `pmx-large-quelle` | pmx | large | 11318431 | 6065961 | 0.5359 | 6065977 | `33e84626408dacd582ba7e154c298227c3a612624b4282dc9da78cd84938228c` | 54.747 | 90.588 | 90.777 | 17.393 | 253.517 | 42.58 | true | true | true | true |
| `vmd-small-camera` | vmd | small | 1904 | 498 | 0.2616 | 514 | `d1052ae0616f80063b5cebd9bf1d65367979965321224fc4652e21f8ebf0962e` | 0.032 | 0.053 | 0.010 | 0.009 | 0.105 | 17.31 | true | true | true | true |
| `vmd-small-camera-yumegita` | vmd | small | 29903 | 11496 | 0.3844 | 11512 | `deefd09c4c7e8e1ffeb165a44f4ad4565b65d9697d3d7e310d73559fe5406458` | 0.095 | 0.186 | 0.171 | 0.033 | 0.487 | 58.59 | true | true | true | true |
| `vmd-medium-best-friends` | vmd | medium | 851402 | 85814 | 0.1008 | 85830 | `0c38e485adf98188b4049801b2896f3bca02481f9c90f1d52a088b8e2af443b3` | 1.005 | 1.302 | 1.322 | 0.350 | 3.981 | 203.96 | true | true | true | true |
| `vmd-large-addiction` | vmd | large | 4470218 | 517754 | 0.1158 | 517770 | `a389220652869e98e373f73e62d774e8cbf62aeaa2ea12bdd9930b156690896f` | 5.510 | 7.678 | 7.726 | 1.952 | 22.869 | 186.41 | true | true | true | true |
| `vmd-large-kamippoi` | vmd | large | 28401530 | 3494164 | 0.1230 | 3494180 | `9569137d2f4f93b57547c38742823c0589bfd9670b236ed7de00b39355262e6b` | 40.343 | 51.874 | 53.166 | 17.708 | 163.102 | 166.07 | true | true | true | true |
Failures/skips: none.

## Memory and copy observations

- Native RSS, WASM linear-memory peak, JS heap peak, and largest allocation were not measured by this small harness; no unsupported values are claimed.
- JS↔WASM copy counts and byte totals were not instrumented; the Node lane passes one `Uint8Array` per case and records this as an observation, not a measurement.
- Package-layer buffer lifetime and `loadPackage()`/`openPackage()` behavior are outside this Phase 0 codec probe.

## Bounded conclusion

This campaign validates the draft operation order and fail-closed authentication boundaries on the selected cases. It is insufficient to freeze a V1 backend or memory gate. Native/WASM adoption should remain provisional until a larger, separately reviewed campaign compares alternative backends and records reliable memory data.
