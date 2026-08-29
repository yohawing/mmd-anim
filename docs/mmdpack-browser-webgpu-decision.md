# MMDPACK browser WebGPU decision

- Status: fixed ten-case Chrome/WebGPU texture-entry functionality passed; performance is directional on the observed hardware GPU.
- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, WebGPURenderer using the requested device, textured-quad draw into a 16x16 RenderTarget, `queue.onSubmittedWorkDone()`, and asynchronous RenderTarget readback. This is not full-MMD first render or compositor presentation evidence.
- Browser: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36
- Execution surface: external_chrome_extension (server-authorized).
- Adapter: nvidia; blackwell; unavailable; classification: hardware.
- Three.js revision label: b5673888ec8b7ff279a93135c50bdb07f1900dba; control SHA-256: 21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125.
- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.
- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.
- Memory/copy limit: the Chrome heap value is a directional snapshot, not a peak. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.
- Not run: Firefox, Safari, macOS, WebGL2, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.

Run: chrome-webgpu-1787983437589 (local raw artifact is intentionally ignored).

| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| toon-small-b | 6.930 / 7.920 | 5.960 / 7.855 | -0.065 / 0.925 | 0.020 / 0.030 | 0.330 / 0.405 |
| toon-small-sample | 4.290 / 5.940 | 4.420 / 7.800 | -0.395 / 3.510 | 0.025 / 0.030 | 0.805 / 0.990 |
| alpha-feathers | 5.375 / 7.895 | 5.890 / 8.065 | 0.770 / 3.020 | 0.035 / 0.045 | 1.830 / 2.275 |
| diffuse-small-yukichi | 6.950 / 7.570 | 5.145 / 7.430 | -0.300 / 2.780 | 0.040 / 0.045 | 1.080 / 1.420 |
| diffuse-small-a1 | 12.510 / 13.125 | 10.620 / 11.995 | -0.515 / 2.790 | 0.130 / 0.140 | 4.540 / 5.755 |
| normal-small-emitter | 12.265 / 13.390 | 11.115 / 11.935 | -1.150 / 2.355 | 0.120 / 0.125 | 5.735 / 6.225 |
| diffuse-medium-body | 29.850 / 32.820 | 30.445 / 32.115 | 0.465 / 5.565 | 0.270 / 0.305 | 22.800 / 23.400 |
| normal-medium-body | 100.445 / 107.145 | 99.240 / 107.900 | -1.200 / 15.175 | 1.295 / 1.385 | 85.335 / 94.315 |
| diffuse-medium-kabo | 98.890 / 111.155 | 92.240 / 99.470 | -9.245 / 3.085 | 1.495 / 1.545 | 78.490 / 83.165 |
| diffuse-large-bilijiang | 174.590 / 196.870 | 175.050 / 198.575 | 0.460 / 5.060 | 0.255 / 0.330 | 155.905 / 177.285 |

- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.
- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear RenderTarget readback are the bounded substitute.
