# MMDPACK Texture WASM Control

This is maintainer-only Phase 0 compatibility evidence. It does not freeze a V1 codec or `.mmdpack` container.

- Run ID: `20260828T060314919Z-c03101f8`
- Input label: `threejs-example-2d-uastc`
- Input bytes: 2560
- Input SHA-256: `21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125`
- Status: `ok`
- Fixture provenance: `examples/textures/ktx2/2d_uastc.ktx2` from Three.js checkout revision `b5673888ec8b7ff279a93135c50bdb07f1900dba`.
- Three.js transcoder revision: `three.js b5673888ec8b7ff279a93135c50bdb07f1900dba`
- Transcoder JS SHA-256: `9042facffa0b63ec1c897919c5db43f000c3dee03d9698b6b2465bb06446d298`
- Transcoder WASM SHA-256: `6cf17dc889352c42e9acf8897107978d127005fe3386c36a0e3845e27967630a`
- Harness source SHA-256: `920258921739a6940ef81d0e1d636f67ac47aae9c06b0343446b175bf1b5a2b4`
- Adapter initialization mirrors the Three.js worker: await the factory, then call `initializeBasis()` before constructing `KTX2File`.
- Transcode format: RGBA32 (format 13); level info uses `(mip, layer, face)` and image transcode uses `(mip, layer, face, format, ...)`.

- Elapsed: 2.642 ms; input calls: 1; input bytes observed: 2560.

| Mip | Dimensions | RGBA32 bytes | SHA-256 |
|---:|---:|---:|---|
| 0 | 40×40 | 6400 | `fd9d51898dce01c2cdfd4ece339a4e5d5a5f198b80d124485af5874d62bba5ad` |
| 1 | 20×20 | 1600 | `979ad5b892b0e534b461afa8030f66192705111e25f9d335cf445f14d16ef82c` |
| 2 | 10×10 | 400 | `3dd59f9d2f4dd07236c25800f1ab0a361639bf5a5305543662550ac07daa20e8` |
| 3 | 5×5 | 100 | `632872db7aa5a991284a478664e0cbc3f296e950be28621cf4daea434dfcd167` |
| 4 | 2×2 | 16 | `8b43dd2b7f927805f9804fb536e0f0ba3b75a563b29342f03d30405ed792455f` |
| 5 | 1×1 | 4 | `2d40b1336c05768172457abbdceac9a87b96b6b80be0b7584f8052af1ab561d5` |

- JS/WASM copy counts and heap/RSS peaks are unavailable; only input call count and bytes are reported.
- Absolute filesystem paths are intentionally omitted from this report.
