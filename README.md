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

> **Physics simulation is not included.** Rigid-body and joint data can be read
> and written, but cloth, hair, and other physics-driven motion must be handled
> by the host-side physics engine.

## Test Foundation

`mmd-anim` is a shared animation foundation for multiple projects, so test
coverage focuses on keeping evaluated results correct.

The repository includes tests for:

- animation evaluation, bone hierarchy evaluation, IK, append transforms, morphs, and format read/write paths;
- round-trip checks that write parsed data and read it back without changing the represented content;
- frame-by-frame PMX/VMD runtime evaluation against expected synthetic results;
- maintainer CLI diagnostics for inspecting loaded models and evaluated state;
- C ABI and WASM smoke checks to confirm host-facing APIs use the same runtime path.

Recommended public release checks:


```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

Maintainers with local GoldenOracle physics baselines should also run the local
physics release gate before cutting a release:

```powershell
.\scripts\local-physics-release-gate.ps1
```

The physics gate uses ignored `tools/golden-gate/physics-*.local.json` configs
and local `.ai/` baselines. It never updates baselines; use
`tools/golden-gate` directly when accepting a new baseline.

## Used By

`mmd-anim` is developed as the shared animation backend for MMD-related projects.

- [three-mmd-loader](https://github.com/yohawing/three-mmd-loader): A Three.js
  MMD loader that uses `mmd-anim` as its animation and format backend.
- [maya_mmd_tools](https://github.com/yohawing/maya_mmd_tools): Maya plugin for
  MMD model and motion handling, using `mmd-anim` as its native runtime.

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
| PMM | header, timeline, display state, referenced assets, PMMv2 summaries, and selected keyframe payload metadata | partial support: lossless parsed-byte round trip, limited source-byte patches, and experimental single-model PMX/VMD scene generation |
| X/VAC | text X mesh, material, UV, normal, vertex color + VAC settings and raw lines | text X / VAC wrapper writing |
| FBX | not loaded | experimental FBX 7.4 binary export for PMX mesh/skeleton/skin/bind pose, vertex morph blendshapes, runtime-baked VMD bone + vertex morph animation, and bones-only skeleton/motion output |

## Rust Usage

```toml
[dependencies]
mmd-anim = "0.1"
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

This native integration crate is not published to crates.io for the 0.1.x line. It is
kept in the Rust workspace for builds and checks.

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

The WASM package is not published to crates.io for the 0.1.x line. It is kept in the Rust
workspace for builds and checks.

## CLI

`mmd-anim-cli` is a command-line tool for inspecting, converting, and diagnosing
MMD format files (PMX, VMD, VPD, PMM, X/VAC).

```powershell
cargo install mmd-anim-cli
```

After installation, the `mmd-anim` command is available:

```powershell
mmd-anim --help
```

For development, you can also run directly from the workspace:

```powershell
cargo run -p mmd-anim-cli -- --help
```

You can export animated FBX files from PMX and VMD inputs.

```powershell
mmd-anim convert-fbx model.pmx model.fbx --vmd motion.vmd --max-frame 120
mmd-anim convert-fbx model.pmx model.fbx --copy-diffuse-textures
mmd-anim convert-fbx model.pmx motion.fbx --vmd motion.vmd --bones-only
mmd-anim convert-fbx model.pmx model.fbx --readable-bone-names
mmd-anim convert-fbx model.pmx model.fbx --write-physics-params
```

With `--vmd`, `convert-fbx` uses runtime-baked output for bones and vertex
morph weights, so IK, append transforms, and fixed-axis constraints are sampled
before writing FBX animation curves. Camera, light, self-shadow, visibility,
physics, and non-vertex morph tracks are not exported as FBX tracks.

Use `--bones-only` to write only the FBX skeleton and optional runtime-baked
bone animation, without mesh, materials, skin clusters, bind pose, textures, or
blendshapes.

By default, bone names keep the legacy UTF-8 hex encoding for compatibility.
Use `--readable-bone-names` to opt into English PMX names, a standard MMD bone
dictionary, and sanitized ASCII fallbacks instead.
When enabled, the CLI also writes `<fbx-stem>.bone-map.json` with PMX bone
indices, source names, FBX names, and name source labels.

Use `--write-physics-params` to write `<fbx-stem>.physics-params.json` with
PMX rigid-body and joint parameters as schema version 1 JSON. This is a
sidecar for future physics bake and DCC parameter-editing workflows; it does
not enable physics simulation in the exported FBX. PMX rigid-body
`collision.mask` is the collide-with group mask passed to Bullet. The sidecar
also writes `collision.collisionMask` / `collision.bulletCollisionMask` as
explicit aliases and `collision.nonCollisionMask` as the complementary mask for
tools that need a blocked-group view.

By default, PMX diffuse texture paths are written to FBX as-is. With
`--copy-diffuse-textures`, referenced diffuse textures are copied next to the
FBX into a managed `*-textures` directory and FBX paths are rewritten to those
relative files. Sphere, toon, and material-morph textures are not exported.

## Crates

| Crate | Role |
|---|---|
| `mmd-anim` | Main public crate. Provides the evaluation core and format handling through one entry point. |
| `mmd-anim-runtime` | Format-independent evaluation core: model arena, pose, VMD evaluation, append transforms, IK, and morphs. |
| `mmd-anim-format` | PMX/VMD runtime import, format detection, structured loading, and PMX/PMD/VMD/VPD/X/VAC writing. |
| `mmd-anim-ffi` | C ABI for native hosts. Exposes runtime operations and PMX parts writing. Repository-local for the 0.1.x line. |
| `mmd-anim-wasm` | `wasm-bindgen` wrapper for browsers. Exposes runtime operations, loading/writing APIs, and PMX parts writing. Workspace-local for the 0.1.x line. |
| `mmd-anim-cli` | Command-line tool for inspecting, converting, and diagnosing MMD format files, including maintainer-local oracle and numeric comparison schemas. Installable via `cargo install mmd-anim-cli`. |

For normal library use, depend on `mmd-anim`. Advanced users who only need a
lower layer can depend on `mmd-anim-format` or `mmd-anim-runtime` directly.

## Current Limitations

- **Evaluation core:** PMD loading and partial runtime import are supported for bones, IK, morph slots, and vertex morph offsets, but renderer-side vertex deformation and full PMD compatibility are not claimed yet.
- **Writing:** PMX generation from parts currently covers the initial range of geometry, materials, bones, display frames, morphs, and physics. PMM writing is limited to the data currently represented by the PMM manifest parser.
- **PMM:** Supported PMM data currently includes project header information, timeline-derived values, display state, initial model-slot data, referenced assets, PMMv2 summary information, and asset/header consistency diagnostics. The PMM exporter can re-emit that limited manifest/header/slot/asset-reference surface as a PMMv2 file, but it is not a full PMM project-graph exporter. Keyframe payloads, full camera/light/accessory/self-shadow tracks, and other binary project graph data that are only summarized or not preserved by the parser cannot be reconstructed from `PmmParsedManifest`.
- **X/VAC:** Text X mesh, material, normal, UV, vertex color, and common VAC line order are handled. Binary X is diagnostic-only.
- **Physics:** Rigid-body and joint data can be read and written, but physics simulation itself is not provided. Physics-driven parts should be handled by the host engine.
- **API / ABI / WASM:** These surfaces are still experimental. When integrating with an external host, start with a small smoke test and representative-frame checks.

## Japanese README

- [docs/README.ja.md](docs/README.ja.md)

## References

This project was developed with reference to:

- [Babylon-MMD](https://github.com/noname0310/babylon-mmd)
- [saba](https://github.com/benikabocha/saba)
- [nanoem](https://github.com/hkrn/nanoem)
