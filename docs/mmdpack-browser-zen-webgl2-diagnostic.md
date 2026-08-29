# MMDPACK Zen Browser WebGL2 diagnostic

- Status: fixed ten-case Zen Browser/Firefox-engine WebGL2 texture-entry functionality passed; performance is directional on the observed hardware GPU. This is diagnostic evidence and is not official Firefox authority.
- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, textured-quad draw, and `gl.finish()`. This is not full-MMD first render or compositor presentation evidence.
- Browser: Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:154.0) Gecko/20100101 Firefox/154.0
- Execution surface: zen_browser (server-authorized Zen diagnostic).
- GPU: Google Inc. (NVIDIA); ANGLE (NVIDIA, NVIDIA GeForce GTX 980 Direct3D11 vs_5_0 ps_5_0), or similar; classification: hardware.
- Three.js revision label: b5673888ec8b7ff279a93135c50bdb07f1900dba; control SHA-256: 21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125.
- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.
- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.
- Memory/copy limit: browser heap is unavailable unless explicitly exposed. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.
- Not run: official Firefox authority, Safari, macOS, WebGPU, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.

Run: zen-webgl2-1787992215061 (local raw artifact is intentionally ignored).

| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| toon-small-b | 10.880 / 11.780 | 11.120 / 12.220 | 0.020 / 1.340 | 0.040 / 0.060 | 0.560 / 0.580 |
| toon-small-sample | 11.380 / 13.820 | 12.020 / 16.560 | 0.720 / 5.180 | 0.060 / 0.060 | 1.140 / 1.520 |
| alpha-feathers | 11.620 / 11.880 | 11.820 / 11.960 | -0.060 / 0.840 | 0.080 / 0.080 | 1.660 / 2.280 |
| diffuse-small-yukichi | 10.840 / 11.500 | 10.900 / 11.620 | 0.060 / 1.120 | 0.080 / 0.100 | 1.000 / 1.080 |
| diffuse-small-a1 | 15.000 / 16.640 | 15.380 / 19.660 | 0.080 / 4.660 | 0.240 / 0.320 | 4.520 / 5.020 |
| normal-small-emitter | 16.940 / 17.600 | 16.320 / 20.220 | -0.620 / 4.320 | 0.200 / 0.260 | 4.680 / 5.800 |
| diffuse-medium-body | 30.640 / 39.240 | 29.640 / 33.060 | -2.540 / 3.620 | 0.640 / 0.740 | 16.060 / 20.100 |
| normal-medium-body | 104.220 / 117.240 | 103.340 / 133.220 | -1.380 / 46.480 | 1.420 / 7.880 | 78.620 / 93.520 |
| diffuse-medium-kabo | 93.480 / 99.980 | 85.740 / 93.980 | -7.740 / 0.340 | 1.620 / 3.540 | 64.720 / 73.840 |
| diffuse-large-bilijiang | 218.980 / 221.960 | 200.700 / 218.340 | -21.260 / 3.800 | 0.320 / 0.460 | 154.600 / 176.620 |

- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.
- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear GPU readback are the bounded substitute.
