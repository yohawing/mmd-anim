# MMDPACK Phase 0 backend A/B decision

> Measurement evidence only. The MMDPACK V1 backend and container profile remain unfrozen.

## Result

**Decision blocked.** Windows Native and Node/WASM stand-in compatibility evidence is insufficient for a production backend. macOS Native, real browser Web Crypto, first-render, memory, and copy hard-gate evidence remain deferred.

## Reproducibility and configuration

- Run ID: 20260828T072659689Z-56061969; measured at: 2026-08-28T07:26:59.6941624Z
- Campaign manifest SHA-256: feda593f762d4e5ddcc4d975da8aca5b75c250f31b1e3db8fc4bd6fce804470b
- Standalone Cargo.lock SHA-256: 395401e6e01d864eef27005b4f89a646e8445dea3b6fb254c875d5b8788711ba
- Harness source digest: 2aa13a29b3404ba20b94e8df986162818eee65927e6bff8e9b941de4abbca950
- Native: windows-x86_64, rustc 1.98.0 (88d9e12ae 2026-08-18), package-backends-native 0.1.0; binary 1539584 bytes.
- WASM/Node: win32-x64; JS wrapper 8427 bytes / 8129ba0869332e2edcc86b6acca7c8e31e21bc3a9f235eeb602a7b74a192dc3c; _bg.wasm 396120 bytes / 4f1d3f95081af899b2c45d476d685e2bb902d94b87000ef09a0cc2c2c9f6a813. Node WebCrypto is a stand-in, not browser evidence.
- MSRV: MSRV 1.87 build not verified on this host; measured with rustc 1.98.0 (88d9e12ae 2026-08-18)
- AES wire: AES-256-GCM fixed profile (ciphertext \|\| 16-byte tag); Zstd encoder: zstd crate 0.13.3 / libzstd 1.5.7, level 3 (one frame per case)
- Decoder A/B: zstd crate 0.13.3 / libzstd 1.5.7 vs ruzstd 0.8.3; level 3
- Frame limits: single frame; declared content size; no dictionary/checksum; window <= 64 MiB; decoded <= 128 MiB
- Vector policy: campaign uses a fresh run-scoped 32-byte key from an ephemeral environment value; every warmup/repeat encryption has a backend/domain/iteration-unique nonce and AAD; one fixed public key/nonce/AAD vector is conformance-only and excluded from campaign performance; no secrets serialized
- Timing: one warmup plus five measured iterations; OS/file caches uncontrolled; p50/p95 are directional
- Fixed conformance vector: 42 bytes / c97d7f2d08368e7e338cda518af14a6fcd96d2c5868db8b8752903cca7d3f95f; all four Native/WASM AES backends match wire SHA-256 bc1bcc8bff0bea5d4359cae7f394dca42c4b9faafe939f8a50df5a20a0d415bb.

## Per-case measurements

| ID | Kind | Class | Input bytes | Source SHA-256 | Frame bytes | Frame SHA-256 | AES wire bytes | Native AES A/B (p50/p95 ms; MiB/s) | WASM AES A/B (p50/p95 ms; MiB/s) | Native Zstd A/B (p50/p95 ms; MiB/s) | WASM Zstd A/B (p50/p95 ms; MiB/s) | Checks |
|---|---|---|---:|---|---:|---|---:|---:|---:|---:|---:|---|
| pmx-small-book | pmx | small | 37360 | cc77518a29ce47f640eb487c4551c128ea7ca4d50bb190b0a8a14762bb7faf2d | 10455 | a3c312a118d25a0a1c2ab0d1a20e87be549bef9f078d6c1dece3326abfae8ac7 | 10471 | 0.012/0.023; 0.002/0.006 ms; 837.9/4985.3 MiB/s | 0.262/0.761; 0.112/0.228 ms; 38.0/88.8 MiB/s | 0.044/0.047; 0.121/0.133 ms; 815.3/294.0 MiB/s | 0.081/0.093; 0.216/0.222 ms; 441.0/164.8 MiB/s | A/B OK |
| pmx-small-weapon | pmx | small | 757902 | 413574822f823531aa32ed4c152abed9d667129ab031234aa86617b44d811190 | 145797 | 21d6ce8327054a3381064b97ad48e84349fa0aaae63e5243386c7769b4c2852e | 145813 | 0.142/0.149; 0.079/0.080 ms; 981.9/1762.3 MiB/s | 2.298/2.536; 0.135/0.151 ms; 60.5/1031.5 MiB/s | 0.555/0.602; 1.373/1.442 ms; 1302.6/526.5 MiB/s | 0.815/0.849; 1.804/2.552 ms; 887.3/400.6 MiB/s | A/B OK |
| pmx-medium-kizuna | pmx | medium | 2741268 | 33ba6869c1e428860bb8f47b25c3f5cc504d722aa069f1f6d2219307ef8328b1 | 1728709 | 2ba9903c8863a8841893b54b4e3634968187e74602e4a38ce103a65c73ba19ab | 1728725 | 1.228/1.261; 0.551/0.573 ms; 1342.3/2991.0 MiB/s | 26.136/26.752; 0.824/0.886 ms; 63.1/2001.7 MiB/s | 3.011/3.074; 9.517/9.601 ms; 868.3/274.7 MiB/s | 3.934/4.253; 12.772/12.950 ms; 664.6/204.7 MiB/s | A/B OK |
| pmx-large-nora | pmx | large | 5084212 | 15da7c1ba27a50ac91d64b2054082e1bb7d7e8aa9da494f3703879748031697f | 1718886 | 476f0cbde7a497e07be59dd337a03925fc234aa2de3bbccc77d2ddf752ee7e57 | 1718902 | 1.235/1.287; 0.540/0.558 ms; 1327.8/3034.0 MiB/s | 25.989/26.170; 0.826/0.859 ms; 63.1/1984.6 MiB/s | 3.860/3.999; 12.401/12.736 ms; 1256.1/391.0 MiB/s | 5.547/6.018; 16.794/17.017 ms; 874.1/288.7 MiB/s | A/B OK |
| pmx-large-quelle | pmx | large | 11318431 | 33e84626408dacd582ba7e154c298227c3a612624b4282dc9da78cd84938228c | 6092763 | 60a9ffee1103e48cfd48068328eea84e23bee8f4a947ed7b5c3622fe43126da4 | 6092779 | 4.185/4.227; 1.744/1.844 ms; 1388.4/3332.3 MiB/s | 93.225/93.747; 2.660/3.759 ms; 62.3/2184.7 MiB/s | 11.700/13.190; 37.717/38.098 ms; 922.5/286.2 MiB/s | 18.007/18.194; 53.245/58.336 ms; 599.4/202.7 MiB/s | A/B OK |
| vmd-small-camera | vmd | small | 1904 | d1052ae0616f80063b5cebd9bf1d65367979965321224fc4652e21f8ebf0962e | 498 | 359eef8a6b740bfe913c0b74c0188e1caed4f3a144b4f808fc6a7727efb8dac4 | 514 | 0.002/0.002; 0.001/0.001 ms; 279.4/527.7 MiB/s | 0.022/0.030; 0.076/0.127 ms; 21.9/6.2 MiB/s | 0.013/0.016; 0.006/0.008 ms; 135.5/302.6 MiB/s | 0.006/0.008; 0.010/0.011 ms; 292.9/181.6 MiB/s | A/B OK |
| vmd-small-camera-yumegita | vmd | small | 29903 | deefd09c4c7e8e1ffeb165a44f4ad4565b65d9697d3d7e310d73559fe5406458 | 11496 | 6eab3403d57747752e01780b77912d639e4e6f3f34dc16d1c042792246cdbf1e | 11512 | 0.011/0.017; 0.002/0.006 ms; 987.7/5481.7 MiB/s | 0.294/0.377; 0.072/0.081 ms; 37.3/151.6 MiB/s | 0.031/0.043; 0.091/0.107 ms; 925.9/314.8 MiB/s | 0.042/0.070; 0.112/0.113 ms; 674.2/254.9 MiB/s | A/B OK |
| vmd-medium-best-friends | vmd | medium | 851402 | 0c38e485adf98188b4049801b2896f3bca02481f9c90f1d52a088b8e2af443b3 | 85814 | 1fc28c4b8be9a843d3ee47c147d1eca7b50f9877e123ed30138a570a81a6f29a | 85830 | 0.085/0.089; 0.053/0.061 ms; 958.3/1547.0 MiB/s | 1.359/1.371; 0.106/0.161 ms; 60.2/772.8 MiB/s | 0.512/0.587; 1.180/1.260 ms; 1587.4/688.0 MiB/s | 0.639/0.685; 1.632/1.683 ms; 1271.5/497.6 MiB/s | A/B OK |
| vmd-large-addiction | vmd | large | 4470218 | a389220652869e98e373f73e62d774e8cbf62aeaa2ea12bdd9930b156690896f | 517421 | 84e6b0d6314486c73b22e101600b4c7f64c23e62d7ceae51a13bd97299bfc476 | 517437 | 0.275/0.318; 0.069/0.110 ms; 1791.8/7141.1 MiB/s | 8.309/8.974; 0.269/0.356 ms; 59.4/1831.7 MiB/s | 2.380/2.410; 6.469/6.603 ms; 1790.9/659.0 MiB/s | 3.072/3.782; 9.195/9.810 ms; 1387.9/463.7 MiB/s | A/B OK |
| vmd-large-kamippoi | vmd | large | 28401530 | 9569137d2f4f93b57547c38742823c0589bfd9670b236ed7de00b39355262e6b | 3496636 | e9e6b6fcc7737172e6550f7ba176bee6c49d93760356c1f720b918adb945fad4 | 3496652 | 2.511/2.534; 1.019/1.102 ms; 1327.8/3273.4 MiB/s | 52.932/55.239; 1.748/2.702 ms; 63.0/1907.6 MiB/s | 11.377/11.444; 30.402/30.935 ms; 2380.7/890.9 MiB/s | 20.726/22.136; 47.162/48.594 ms; 1306.9/574.3 MiB/s | A/B OK |

Rows share source/frame hashes across lanes. AES performance wires are not cross-lane compared because campaign nonce/AAD domains differ; fixed conformance wire parity is the cross-backend compatibility vector. Each lane A/B and each Zstd decoder matched its required plaintext/source hash.

## Boundary and observability limits

- Wrong-key, wrong-AAD, one-byte tamper, truncation, and declared-size checks were green for every case and conformance backend. Parser tests cover reserved/dictionary/checksum/trailing-frame and structurally complete single-frame over-window boundaries before decoder allocation.
- Native input: 10 calls / 53694130 bytes. WASM/Node reads: 20 calls / 67502605 bytes. JS/WASM copy count/bytes, RSS, JS heap peak, WASM linear-memory peak, and largest allocation are unavailable.
- Warmup/repeat distributions are directional; OS/file caches were uncontrolled. Native JSON, WASM JSON, and this Markdown are separate file-atomic publications, not a transactional set.

## Bounded conclusion and next task

No backend is selected for V1. Keep RustCrypto/ring AES and libzstd/ruzstd as explicit candidates. Next, measure macOS arm64 Native, real Chrome/Firefox/Safari Web Crypto, WebGL2/WebGPU first-render paths, and reliable memory/copy metrics; production choice remains blocked until each target hard-gate metric is observed.
