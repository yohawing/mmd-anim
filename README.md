# mmd-anim

`mmd-anim` is an experimental Rust toolkit for loading, writing, and evaluating
MikuMikuDance assets. The same evaluation core can be used from Rust libraries,
native integrations, browsers, and command-line tools.

In addition to runtime evaluation, it provides format detection and structured
loading for PMX/PMD/VMD/PMM/VPD/X/VAC, writing support for PMX/PMD/VMD/VPD/X/VAC,
and PMX generation from parts.

> **Status:** experimental `0.1.0` — the Rust API and C ABI are not stable yet.
> Breaking changes may happen.

## Usage

```toml
[dependencies]
mmd-anim = "0.1"
```

```powershell
# Build and test the Rust workspace
rtk cargo test --workspace

# Local release checks
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo doc --workspace --no-deps
```

`mmd-anim` 0.1.0 is an alpha release. The Rust API, C ABI, and WASM entry points
are not stable yet.

## Features

### Runtime Evaluation

- Build a runtime model from PMX bytes.
- Resolve VMD bytes against names from a PMX model and convert them into an `AnimationClip`.
- Evaluate any frame and read world matrices, skinning matrices, morph weights, and IK enabled state.

### Loading / Writing

Format support overview. "Loading" means parsing a file into structured data.
"Writing" means outputting the target file format.

| Format | Loading | Writing |
|--------|---------|---------|
| PMX | model sections + soft-body header diagnostics | meaning-preserving write / JSON-based read-write / generation from parts |
| PMD | model structure + partial runtime import | meaning-preserving write / JSON-based read-write |
| VMD | animation structure | **supported** |
| PMM | header, timeline, display state, referenced assets, and PMMv2 summary information | — |
| VPD | pose structure | **supported** |
| X/VAC | text X mesh, material, UV, normal, vertex color + VAC settings and raw lines | meaning-preserving text X / VAC wrapper write |

## Crates

| Crate | Role |
|---|---|
| `mmd-anim` | Main public crate. Provides the evaluation core and format handling through one entry point. |
| `mmd-anim-runtime` | Format-independent evaluation core: model arena, pose, VMD evaluation, append transforms, IK, and morphs. |
| `mmd-anim-format` | PMX/VMD runtime import, format detection, structured loading, and PMX/PMD/VMD/VPD/X/VAC writing. |
| `mmd-anim-ffi` | C ABI for native hosts. Exposes runtime operations and PMX parts writing. Repository-local for 0.1.0. |
| `mmd-anim-wasm` | `wasm-bindgen` wrapper for browsers. Exposes runtime operations, loading/writing APIs, and PMX parts writing. Workspace-local for 0.1.0. |
| `mmd-anim-cli` | Maintainer diagnostics and verification command-line tool. Repository-local for 0.1.0. |
| `mmd-anim-schema` | Maintainer quality-check schema helper crate. Repository-local for 0.1.0. |

For normal library use, depend on `mmd-anim`. Advanced users who only need a
lower layer can depend on `mmd-anim-format` or `mmd-anim-runtime` directly.

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

This native integration crate is not published to crates.io for 0.1.0. It is
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
const world = runtime.worldMatricesView();

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

`worldMatricesView()` avoids a copy, but it becomes invalid after the next
evaluation or WASM memory growth. Use `worldMatrices()` or `copyWorldMatrices()`
when the data needs to live longer.

The WASM package is not published to crates.io for 0.1.0. It is kept in the Rust
workspace for builds and checks.

## CLI Checks

For local loading and writing checks, use the repository-local `mmd-anim-cli`.
Available subcommands can be listed with:

```powershell
rtk cargo run -p mmd-anim-cli -- --help
```

This command-line tool is for maintainer diagnostics and is not required for
public releases. Local asset comparisons and reference-data checks are also not
part of the public release gate.

## Current Limitations

- **Evaluation core:** PMD loading and partial runtime import are supported for bones, IK, morph slots, and vertex morph offsets, but renderer-side vertex deformation and full PMD compatibility are not claimed yet.
- **Writing:** PMX generation from parts currently covers the initial range of geometry, materials, bones, display frames, morphs, and physics. PMM writing is not provided until the full project graph can be represented.
- **PMM:** Supported PMM data currently includes project header information, timeline-derived values, display state, initial model-slot data, referenced assets, PMMv2 summary information, and asset/header consistency diagnostics. PMM writing is not provided because the full project graph is not preserved yet.
- **X/VAC:** Text X mesh, material, normal, UV, vertex color, and common VAC line order are handled. Binary X is diagnostic-only.
- **API / ABI / WASM:** These surfaces are still experimental. When integrating with an external host, start with a small smoke test and representative-frame checks.

## Related Project

The following project is developed alongside `mmd-anim` and uses it as a backend:

- [three-mmd-loader](https://github.com/yohawing/three-mmd-loader): MMD loader for Three.js.

## Japanese README

- [docs/README.ja.md](docs/README.ja.md)

## References

This project was developed with reference to:

- [Babylon-MMD](https://github.com/noname0310/babylon-mmd)
- [saba](https://github.com/benikabocha/saba)
- [nanoem](https://github.com/hkrn/nanoem)
