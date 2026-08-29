# MMDPACK Firefox WebGL2 decision

- Status: fixed ten-case official Firefox/WebGL2 texture-entry functionality passed; performance is directional on the observed hardware GPU.
- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, textured-quad draw, and `gl.finish()`. This is not full-MMD first render or compositor presentation evidence.
- Browser: Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:154.0) Gecko/20100101 Firefox/154.0
- Execution surface: official_firefox (server-authorized official Firefox).
- GPU: Google Inc. (NVIDIA); ANGLE (NVIDIA, NVIDIA GeForce GTX 980 Direct3D11 vs_5_0 ps_5_0), or similar; classification: hardware.
- Three.js revision label: b5673888ec8b7ff279a93135c50bdb07f1900dba; control SHA-256: 21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125.
- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.
- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.
- Memory/copy limit: browser heap is unavailable unless explicitly exposed. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.
- Not run: Safari, macOS, Firefox WebGPU, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.

Run: firefox-webgl2-1787993260994 (local raw artifact is intentionally ignored).

| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| toon-small-b | 14.200 / 21.740 | 30.840 / 55.540 | 9.460 / 43.020 | 0.060 / 0.100 | 0.660 / 0.900 |
| toon-small-sample | 21.080 / 22.180 | 21.700 / 32.460 | 3.120 / 13.700 | 0.060 / 0.080 | 1.100 / 1.180 |
| alpha-feathers | 15.300 / 23.480 | 16.000 / 56.340 | 0.480 / 42.360 | 0.100 / 0.120 | 1.780 / 2.660 |
| diffuse-small-yukichi | 12.620 / 15.380 | 12.680 / 26.720 | -0.100 / 14.100 | 0.080 / 0.100 | 1.060 / 1.060 |
| diffuse-small-a1 | 15.260 / 35.680 | 16.280 / 16.560 | 0.500 / 1.240 | 0.240 / 0.280 | 4.140 / 4.380 |
| normal-small-emitter | 15.000 / 17.640 | 15.740 / 16.000 | 0.140 / 1.320 | 0.180 / 0.200 | 4.180 / 4.200 |
| diffuse-medium-body | 28.000 / 28.960 | 29.680 / 31.980 | 1.620 / 4.220 | 0.560 / 0.660 | 15.200 / 19.200 |
| normal-medium-body | 90.500 / 108.080 | 82.560 / 97.140 | -5.340 / 1.520 | 1.600 / 3.140 | 62.000 / 73.960 |
| diffuse-medium-kabo | 73.220 / 77.260 | 77.440 / 79.560 | 3.580 / 6.540 | 3.320 / 3.360 | 56.440 / 57.140 |
| diffuse-large-bilijiang | 172.640 / 175.380 | 173.640 / 175.420 | 0.340 / 4.880 | 0.300 / 0.320 | 129.960 / 133.320 |

- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.
- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear GPU readback are the bounded substitute.
