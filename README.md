# mmd-anim

`mmd-anim` is a Rust animation foundation for playing MikuMikuDance animation
across multiple platforms.

It loads PMX/VMD data and evaluates world matrices, skinning matrices, morph
weights, and IK state at any frame. The library is designed so the same runtime
can be called from browsers, command-line tools, Rust applications, mobile
applications, game engines, and other host environments.

## Status

`mmd-anim` is still in an evaluation stage.

It is tested against data exported from the original MMD and against several
PMX/VMD assets, but real-world usage is still limited. APIs and features are not
frozen yet, and breaking changes may happen before 1.0. Feedback is welcome.

## Runtime Evaluation

- Load a PMX model and convert it into runtime-ready model data.
- Load a VMD motion and convert bone, camera, light, and other motion tracks into a playable form.
- Interpolate between keyframes with MMD-style Bezier interpolation for translation and rotation.
- An optional MMD physics backend using Bullet Physics, available to
  `mmd-anim-cli` and `mmd-anim-ffi` builds through their
  `physics-bullet-native` feature. The required Bullet3 modules are vendored
  and compiled for the consumer's target; no separate Bullet installation is
  required.

## Test Foundation

`mmd-anim` is a shared animation foundation for multiple projects, so test
coverage focuses on keeping evaluated results correct.

The repository includes tests for:

- animation evaluation, bone hierarchy evaluation, IK, append transforms, morphs, and format read/write paths;
- round-trip checks that write parsed data and read it back without changing the represented content;
- frame-by-frame PMX/VMD runtime evaluation against expected results;
- maintainer CLI diagnostics for inspecting loaded models and evaluated state;
- C ABI and WASM smoke checks to confirm host-facing APIs use the same runtime path.

Recommended public release checks:

```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

## Used By

`mmd-anim` is developed as the shared animation backend for MMD-related projects.

- [three-mmd-loader](https://github.com/yohawing/three-mmd-loader): A Three.js
  MMD loader that uses `mmd-anim` as its animation and format backend.
- [maya_mmd_tools](https://github.com/yohawing/maya_mmd_tools): A Maya plugin
  for editing MMD animation. It uses `mmd-anim` for full-bake VMD import and as
  the source of truth for its rig implementation.
- [unity-mmd-loader](https://github.com/yohawing/unity-mmd-loader): An MMD
  loader optimized for Unity 6 and URP, using `mmd-anim` for importing and as
  its core animation runtime.

More integrations can share the same runtime core through the Rust API, C ABI,
or WASM wrapper.

## Supported Formats

Format support overview. "Loading" means parsing a file into structured data.
"Writing" means outputting the target file format.

| Format | Loading | Writing |
|--------|---------|---------|
| PMX | model sections + soft-body header diagnostics | writing / JSON conversion / generation from mesh data |
| PMD | model structure + partial runtime import | writing / JSON conversion |
| VMD | **supported** | **supported** |
| VPD | **supported** | **supported** |
| PMM | header, timeline, display state, referenced assets, PMMv2 summaries, and selected keyframe payload metadata | partial support: rewriting selected parts of the original data and experimental generation of single-model PMX/VMD scenes |
| X/VAC | text X mesh, material, UV, normal, vertex color + VAC settings and raw lines | text X / VAC wrapper writing |
| FBX | not supported | Experimental binary export for PMX meshes, skeletons, skinning, bind poses, and vertex morphs (blendshapes), as well as runtime-baked VMD bone and vertex-morph animation. |

## Rust Usage

```toml
[dependencies]
mmd-anim = "0.3"
```

## Native Hosts (C ABI)

Native applications and game engines can use the C ABI from `mmd-anim-ffi`.
It is not tied to a specific engine; any host that can call a C ABI can use it
(Unity is one example).
The header is [crates/mmd-anim-ffi/include/mmd_runtime.h](crates/mmd-anim-ffi/include/mmd_runtime.h).

```c
// 1. Create a model from PMX bytes
mmd_runtime_model_t* model =
    mmd_runtime_model_create_from_pmx_bytes(pmx_bytes, pmx_len);

// 2. Create an animation clip from VMD bytes
mmd_runtime_clip_t* clip =
    mmd_runtime_clip_create_from_vmd_bytes_for_model(model, vmd_bytes, vmd_len);

// 3. Create an instance
mmd_runtime_instance_t* instance =
    mmd_runtime_instance_create_for_model(model);

// 4. Evaluate a frame
mmd_runtime_instance_evaluate_clip_frame(instance, clip, 300.0f);

// 5. Copy world matrices
size_t len = mmd_runtime_instance_world_matrix_f32_len(instance);
mmd_runtime_instance_copy_world_matrices(instance, out_f32, len);

// 6. Free resources
mmd_runtime_instance_free(instance);
mmd_runtime_clip_free(clip);
mmd_runtime_model_free(model);
```

The intended split is that the host owns meshes, materials, and textures, while
this runtime provides matrices, morph state, and IK state. To generate PMX from
host-side geometry data, use `mmd_runtime_export_pmx_from_parts`.
Input arrays remain owned by the caller, and returned bytes must be freed with
`mmd_runtime_byte_buffer_free`.

## WASM / Browser

The browser build uses `wasm-pack build --target web`. A Node.js-only build is
not used.

```powershell
cd .\crates\mmd-anim-wasm\harness
npm run build
```

Generated files are written to `crates/mmd-anim-wasm/harness/pkg/`.

```ts
import init, {
  exportMmdFormatBytes,
  exportPmxFromParts,
  exportVmdAnimationJsonBytes,
  parseMmdFormatJson,
  WasmMmdClip,
  WasmMmdModel,
  WasmMmdRuntimeInstance,
} from "./pkg/mmd_anim_wasm.js";

await init();

// Runtime evaluation
const model = WasmMmdModel.fromPmxBytes(pmxBytes);
const clip = WasmMmdClip.fromVmdBytesForModel(model, vmdBytes);
const runtime = WasmMmdRuntimeInstance.forModel(model);

runtime.evaluateClipFrame(clip, 300);
const world = runtime.worldMatrices();

// Explicitly free resources when they are no longer needed.
runtime.free();
clip.free();
model.free();

// Loading / writing without a runtime handle
const json = parseMmdFormatJson(vmdBytes, "motion.vmd");
const exportedBytes = exportVmdAnimationJsonBytes(json);
const normalizedBytes = exportMmdFormatBytes(vmdBytes, "motion.vmd");

// Generate PMX from typed arrays
const generatedPmxBytes = exportPmxFromParts(
  JSON.stringify({
    modelName: "generated",
    materials: [{ name: "mat", faceCount: 1 }],
    bones: [{ name: "root", parentIndex: -1, position: [0, 0, 0] }],
  }),
  positionsXyz,
  normalsXyz,
  uvsXy,
  indices,
  skinIndices,
  skinWeights,
  edgeScale,
);
```

## CLI

`mmd-anim-cli` is a command-line tool for inspecting, converting, and diagnosing
MMD format files (PMX, VMD, VPD, PMM, X/VAC).

```powershell
cargo install mmd-anim-cli
mmd-anim --help
```

You can export animated FBX files from PMX and VMD inputs.

```powershell
mmd-anim convert-fbx model.pmx model.fbx --vmd motion.vmd --max-frame 120
```

## Crates

| Crate | Role |
|---|---|
| `mmd-anim` | Main public crate. Provides the evaluation core and format handling through one entry point. |
| `mmd-anim-runtime` | Format-independent evaluation core: model arena, pose, VMD evaluation, append transforms, IK, and morphs. |
| `mmd-anim-format` | PMX/VMD runtime import, format detection, structured loading, and PMX/PMD/VMD/VPD/X/VAC writing. |
| `mmd-anim-ffi` | C ABI for native hosts. Exposes runtime operations, PMX parts writing, sparse curves, and optional physics integration. Repository-local and not published to crates.io. |
| `mmd-anim-wasm` | `wasm-bindgen` wrapper for browsers. Exposes runtime operations, loading/writing APIs, PMX parts writing, and sparse curves. Workspace-local and not published to crates.io. |
| `mmd-anim-cli` | Command-line tool for inspecting, converting, and diagnosing MMD format files, including maintainer-local oracle and numeric comparison schemas. Installable via `cargo install mmd-anim-cli`. |

For normal library use, depend on `mmd-anim`. Advanced users who only need a
lower layer can depend on `mmd-anim-format` or `mmd-anim-runtime` directly.

## Current Limitations

- **Evaluation core:** PMD loading and partial runtime import are supported for bones, IK, morph slots, and vertex morph offsets, but renderer-side vertex deformation and full PMD compatibility are not claimed yet.
- **Writing:** PMX generation from parts currently covers the initial range of geometry, materials, bones, display frames, morphs, and physics. PMM writing is limited to the data currently represented by the PMM manifest parser.
- **PMM:** Supported PMM data currently includes project header information, timeline-derived values, display state, initial model-slot data, referenced assets, PMMv2 summary information, and asset/header consistency diagnostics. The PMM exporter can re-emit that limited manifest/header/slot/asset-reference surface as a PMMv2 file, but it is not a full PMM project-graph exporter. Keyframe payloads, full camera/light/accessory/self-shadow tracks, and other binary project graph data that are only summarized or not preserved by the parser cannot be reconstructed from `PmmParsedManifest`.
- **X/VAC:** Text X mesh, material, normal, UV, vertex color, and common VAC line order are handled. Binary X is diagnostic-only.

## Japanese README

- [docs/README.ja.md](docs/README.ja.md)

## References

This project was developed with reference to:

- [Babylon-MMD](https://github.com/noname0310/babylon-mmd)
- [saba](https://github.com/benikabocha/saba)
- [nanoem](https://github.com/hkrn/nanoem)
