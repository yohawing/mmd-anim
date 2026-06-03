# mmd-anim

MikuMikuDance アセットの形式 import/export、アニメーション評価、host integration のための
実験的な Rust toolkit です。Unity (Native C ABI)・ブラウザ (WASM)・CLI から同一コアを利用できます。

ランタイム評価に加え、PMX/PMD/VMD/PMM/VPD/X/VAC の形式検出と Parser DTO 化、
PMX/PMD/VMD/VPD/X/VAC の exporter roundtrip、PMX parts からの実用向け authoring
surface を提供します。

> **ステータス:** experimental `0.1.0` — ABI / API はまだ固定版ではありません。

## クイックスタート

```powershell
# Build and test the Rust workspace
rtk cargo test --workspace

# Release-facing local checks
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo doc --workspace --no-deps
```

`mmd-anim` 0.1.0 は実験的な初期リリースです。API / ABI / WASM surface はまだ固定ではありません。
ローカルの実アセット corpus や GoldenOracle 比較は maintainer QA として扱い、公開 checkout の必須 gate にはしていません。

## 機能概要

### ランタイム評価

- PMX bytes からランタイム用モデルを構築する。
- VMD bytes を PMX 由来の名前マップで解決して `AnimationClip` に変換する。
- 任意フレームを評価して world matrices / skinning matrices / morph weights / IK enabled state を取得する。
- GoldenOracle (MMD 9.32 x64 + MMDDumper 由来) と数値比較し回帰を検出する。

### Parser / Exporter

フォーマットごとの対応状況:

| Format | Parser DTO | Exporter roundtrip |
|--------|-----------|-------------------|
| PMX | full model-section DTO + soft-body header diagnostics | semantic roundtrip / JSON DTO roundtrip / parts authoring |
| PMD | model DTO + partial runtime import | semantic roundtrip / JSON DTO roundtrip |
| VMD | animation DTO | **対応** |
| PMM | header/timeline/display state + manifest assets + PMMv2 document/global scalar summaries | — |
| VPD | pose DTO | **対応** |
| X/VAC | text X mesh/material/UV/normal/vertex-color DTO + VAC semantic settings/raw lines | text X / VAC wrapper semantic roundtrip |

VMD/VPD exporter は `parse → export → parse` および `parse → JSON DTO → export → parse` の同一性を保証します。
PMX/PMD/X/VAC exporter は現在の DTO 範囲で roundtrip slice に対応しています。PMX/PMD は JSON DTO roundtrip に対応済みです。
PMX は WASM/C ABI の `exportPmxFromParts` / `mmd_runtime_export_pmx_from_parts` から、geometry typed arrays と小さい descriptor JSON を渡して PMX bytes を生成できます。
X は text mesh/material/normal/UV/vertex color の re-emission、VAC は raw line 保持と semantic field からの wrapper fallback に対応しています。
PMM は PMMv2 document/global scalar summary まで読めますが、full project graph DTO ではないため exporter はまだ提供していません。
NMD は nanoem 専用フォーマットなので、mmd-anim の parser/exporter 対象外です。

Parser / Exporter の詳細 API は [docs/PARSER_API.md](docs/PARSER_API.md) を参照してください。

## クレート構成

| Crate | 役割 |
|---|---|
| `mmd-anim-runtime` | ファイル形式に依存しない評価コア。モデルアリーナ、ポーズ、VMD 評価、append、IK、morph を扱う。 |
| `mmd-anim-format` | PMX/VMD runtime importer、MMD format detector、parser DTO、PMX/PMD/VMD/VPD/X/VAC exporter を提供する。 |
| `mmd-anim-ffi` | Unity など native host 向け C ABI。runtime handle API と PMX parts exporter を公開する。 |
| `mmd-anim-wasm` | browser / web host 向け `wasm-bindgen` wrapper。runtime handle API、parser/exporter API、PMX parts exporter を公開する。 |
| `mmd-anim-cli` | GoldenOracle 比較、診断、parser/exporter summary、roundtrip 検証用 CLI。 |
| `mmd-anim-schema` | MMDDumper / GoldenOracle JSONL と manifest の schema parsing。 |

詳細な設計は [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) を参照してください。

## Native / Unity から使う

Native host では `mmd-anim-ffi` の C ABI を使います。
ヘッダーは [crates/mmd-anim-ffi/include/mmd_runtime.h](crates/mmd-anim-ffi/include/mmd_runtime.h) です。

```c
// 1. PMX bytes -> model
mmd_runtime_model_t* model =
    mmd_runtime_model_create_from_pmx_bytes(pmx_bytes, pmx_len);

// 2. VMD bytes -> clip
mmd_runtime_clip_t* clip =
    mmd_runtime_clip_create_from_vmd_bytes_for_model(model, vmd_bytes, vmd_len);

// 3. インスタンスを作成
mmd_runtime_instance_t* instance =
    mmd_runtime_instance_create_for_model(model);

// 4. フレーム評価
mmd_runtime_instance_evaluate_clip_frame(instance, clip, 300.0f);

// 5. world matrices をコピー
size_t len = mmd_runtime_instance_world_matrix_f32_len(instance);
mmd_runtime_instance_copy_world_matrices(instance, out_f32, len);

// 6. 解放
mmd_runtime_instance_free(instance);
mmd_runtime_clip_free(clip);
mmd_runtime_model_free(model);
```

ABI の詳細、エクスポート一覧、C# smoke は [docs/ABI.md](docs/ABI.md) を参照してください。

```powershell
pwsh -NoProfile -File .\crates\mmd-anim-ffi\scripts\smoke.ps1
pwsh -NoProfile -File .\crates\mmd-anim-ffi\scripts\smoke-csharp.ps1
```

Unity へ繋ぐ場合は mesh/material/texture を Unity 側で保持し、このランタイムから行列・morph・IK state だけを受け取ります。
PMX を外部アプリ側の geometry から生成する場合は `mmd_runtime_export_pmx_from_parts` を使います。
入力配列の ownership は呼び出し元に残り、返却された bytes は `mmd_runtime_byte_buffer_free` で解放します。

## WASM / ブラウザから使う

ビルドは browser 向けの `wasm-pack build --target web` に固定しています。Node.js 単体用ビルドは使いません。

```powershell
cd .\crates\mmd-anim-wasm\harness
npm run build
```

生成物は `crates/mmd-anim-wasm/harness/pkg/` に出ます。

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

// ランタイム評価
const model = WasmMmdModel.fromPmxBytes(pmxBytes);
const clip = WasmMmdClip.fromVmdBytesForModel(model, vmdBytes);
const runtime = WasmMmdRuntimeInstance.forModel(model);

runtime.evaluateClipFrame(clip, 300);
const world = runtime.worldMatricesView();

// Parser / Exporter (runtime handle 不要)
const json = parseMmdFormatJson(vmdBytes, "motion.vmd");
const exportedBytes = exportVmdAnimationJsonBytes(json);
const normalizedBytes = exportMmdFormatBytes(vmdBytes, "motion.vmd");

// PMX authoring from typed arrays
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

`worldMatricesView()` はコピーを避けますが、次の evaluate や WASM memory growth で無効になります。
永続化する場合は `worldMatrices()` または `copyWorldMatrices()` を使ってください。

WASM package は 0.1.0 では crates.io publish 対象外です。Rust workspace 内では build / check 対象として維持します。

## Parser / Exporter を CLI で検証する

```powershell
# parser summary / JSON
rtk cargo run -p mmd-anim-cli -- parse-format-summary <file.pmd>
rtk cargo run -p mmd-anim-cli -- parse-format-json <file.vpd>

# exporter roundtrip
rtk cargo run -p mmd-anim-cli -- export-roundtrip-summary <file.vmd>
rtk cargo run -p mmd-anim-cli -- export-json-roundtrip-summary <file.vmd>
rtk cargo run -p mmd-anim-cli -- export-roundtrip-summary <file.pmx>
rtk cargo run -p mmd-anim-cli -- export-json-roundtrip-summary <file.pmd>
rtk cargo run -p mmd-anim-cli -- export-roundtrip-summary <file.x>

# machine-readable roundtrip result
rtk cargo run -p mmd-anim-cli -- export-roundtrip-json <file.vmd>
rtk cargo run -p mmd-anim-cli -- export-json-roundtrip-json <file.vpd>

```

Maintainer-local corpus scans and GoldenOracle comparisons are intentionally not required for public release gates.

## 現在の制限と注意点

- **評価コア:** PMX/VMD が中心。PMD は parser DTO と partial runtime import（bones / IK / morph slots / vertex morph offsets）に対応済みだが、renderer-side vertex deformation と full PMD parity はまだ名乗らない。
- **Exporter:** PMX/PMD/VMD/VPD/X/VAC は現在の DTO 範囲で semantic roundtrip を検証済み。PMX/PMD は JSON DTO roundtrip に対応済み。PMX parts authoring は geometry/material/bone/display-frame/morph/physics の初期 slice まで。PMM exporter は full project graph DTO ができるまで未提供。
- **NMD:** nanoem 専用フォーマットのため対象外。mmd-anim の exporter / roundtrip gate には入れない。
- **PMX version:** MMD 実用対象は PMX 2.0 / 2.1。
- **PMM:** project header metadata、timeline 派生値、display state、model slot 初期 slice、manifest-derived asset references、PMMv2 document/global scalar summaries、asset/header consistency diagnostics まで。full project graph を保持しないため exporter は未提供。
- **X/VAC:** text X の mesh/material/normal/UV/vertex color と VAC common line order は扱う。binary X は diagnostic-only。
- **GoldenOracle:** local maintainer QA reference only. Real-asset oracle data is not shipped in this repository or crate packages.
- **IK:** solver-focused worst はまだ完全解消ではない。膝 IK の細部追跡より回帰検出を優先。
- **tolerance:** per-bone / per-motion の細粒度 tolerance は未実装。
- **ABI / WASM API:** experimental。外部 host に繋ぐ場合はまず smoke test と Golden representative frame で確認。

## ドキュメント

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md): 全体設計とレイヤリング
- [docs/PARSER_API.md](docs/PARSER_API.md): Parser / Exporter API と roundtrip 方針
- [docs/TODO.md](docs/TODO.md): Parser / Exporter 追加後の残タスク
- [docs/ABI.md](docs/ABI.md): Native C ABI / C# P/Invoke
- [docs/GOLDEN_QUALITY_TASKS.md](docs/GOLDEN_QUALITY_TASKS.md): Golden 品質タスク
- [docs/FIXTURES.md](docs/FIXTURES.md): fixture 方針
- [docs/PLAN.md](docs/PLAN.md): 実装計画
- [docs/RELEASE.md](docs/RELEASE.md): maintainer release runbook
