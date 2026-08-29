# MMDPACK Firefox WebGPU decision

- Status: fixed ten-case official Firefox/WebGPU texture-entry functionality passed; performance is blocked because GPU classification is unknown.
- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, WebGPURenderer using the requested device, textured-quad draw into a 16x16 RenderTarget, `queue.onSubmittedWorkDone()`, and asynchronous RenderTarget readback. This is not full-MMD first render or compositor presentation evidence.
- Browser: Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:154.0) Gecko/20100101 Firefox/154.0
- Execution surface: official_firefox (server-authorized official Firefox).
- Adapter: unavailable; unavailable; unavailable; classification: unknown.
- Three.js revision label: b5673888ec8b7ff279a93135c50bdb07f1900dba; control SHA-256: 21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125.
- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.
- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.
- Memory/copy limit: browser heap is unavailable unless explicitly exposed. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.
- Not run: Safari, macOS, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.

Run: firefox-webgpu-1787993977821 (local raw artifact is intentionally ignored).

| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| toon-small-b | 99.700 / 100.440 | 99.580 / 100.100 | -0.240 / 0.660 | 0.060 / 0.100 | 0.560 / 0.660 |
| toon-small-sample | 99.620 / 100.800 | 99.880 / 99.940 | 0.020 / 0.320 | 0.060 / 0.080 | 1.100 / 1.180 |
| alpha-feathers | 99.680 / 99.900 | 99.840 / 99.860 | -0.060 / 0.600 | 0.120 / 0.140 | 1.860 / 1.980 |
| diffuse-small-yukichi | 99.520 / 100.960 | 98.240 / 99.120 | -1.940 / -0.000 | 0.240 / 0.560 | 1.920 / 3.880 |
| diffuse-small-a1 | 99.280 / 99.580 | 99.120 / 99.520 | 0.080 / 0.280 | 0.240 / 0.320 | 4.680 / 7.580 |
| normal-small-emitter | 99.280 / 99.400 | 99.580 / 99.960 | 0.320 / 1.120 | 0.200 / 0.220 | 4.440 / 4.720 |
| diffuse-medium-body | 98.120 / 98.220 | 98.320 / 98.400 | 0.100 / 0.560 | 0.540 / 0.560 | 16.260 / 16.700 |
| normal-medium-body | 93.480 / 94.420 | 92.940 / 93.600 | -0.600 / 0.360 | 2.520 / 2.880 | 67.500 / 68.520 |
| diffuse-medium-kabo | 93.080 / 93.560 | 93.580 / 93.620 | 0.060 / 0.720 | 2.300 / 2.580 | 60.500 / 70.020 |
| diffuse-large-bilijiang | 187.340 / 188.880 | 186.380 / 187.240 | -1.200 / -0.260 | 0.620 / 0.680 | 138.700 / 146.820 |

- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.
- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear RenderTarget readback are the bounded substitute.
