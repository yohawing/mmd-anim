# MMDPACK browser WebGL2 decision

- Status: fixed ten-case Chrome/WebGL2 texture-entry functionality passed; performance is directional on the observed hardware GPU.
- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, textured-quad draw, and `gl.finish()`. This is not full-MMD first render or compositor presentation evidence.
- Browser: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36
- GPU: Google Inc. (NVIDIA); ANGLE (NVIDIA, NVIDIA GeForce RTX 5070 Ti (0x00002C05) Direct3D11 vs_5_0 ps_5_0, D3D11); classification: hardware.
- Three.js revision label: b5673888ec8b7ff279a93135c50bdb07f1900dba; control SHA-256: 21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125.
- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.
- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.
- Memory/copy limit: the Chrome heap value is a directional snapshot, not a peak. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.
- Not run: Firefox, Safari, macOS, WebGPU, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.

Run: chrome-1787927176155 (local raw artifact is intentionally ignored).

| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| toon-small-b | 7.600 / 7.880 | 7.170 / 8.095 | -0.240 / 0.470 | 0.025 / 0.035 | 0.315 / 0.435 |
| toon-small-sample | 7.575 / 8.305 | 7.480 / 8.535 | -0.090 / 1.155 | 0.030 / 0.035 | 0.800 / 1.025 |
| alpha-feathers | 9.290 / 10.145 | 8.720 / 9.260 | -0.400 / -0.030 | 0.040 / 0.050 | 1.845 / 1.990 |
| diffuse-small-yukichi | 7.575 / 7.890 | 7.615 / 8.270 | 0.070 / 0.825 | 0.040 / 0.045 | 0.970 / 1.025 |
| diffuse-small-a1 | 9.845 / 11.920 | 11.775 / 13.685 | 0.280 / 3.840 | 0.100 / 0.130 | 4.505 / 5.780 |
| normal-small-emitter | 11.830 / 13.545 | 10.205 / 13.260 | -0.175 / 1.995 | 0.110 / 0.260 | 4.825 / 6.405 |
| diffuse-medium-body | 23.640 / 23.920 | 24.705 / 26.285 | 1.065 / 2.365 | 0.255 / 0.270 | 17.970 / 19.550 |
| normal-medium-body | 81.385 / 85.675 | 80.885 / 85.335 | 1.935 / 3.950 | 1.385 / 1.685 | 70.940 / 71.430 |
| diffuse-medium-kabo | 74.105 / 75.220 | 75.145 / 81.630 | 1.690 / 6.545 | 1.280 / 1.295 | 63.625 / 64.310 |
| diffuse-large-bilijiang | 166.185 / 167.955 | 168.025 / 177.010 | 1.840 / 9.650 | 0.225 / 0.235 | 154.555 / 163.575 |

- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.
- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear GPU readback are the bounded substitute.
