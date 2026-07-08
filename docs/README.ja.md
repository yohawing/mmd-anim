# mmd-anim

`mmd-anim` は、MikuMikuDanceのアニメーションをマルチプラットフォームで再生するためのRust製アニメーション基盤です。

PMX/VMD を読み込み、任意フレームからワールド行列、スキニング行列、
モーフ重み、IK 状態を計算する機能を提供します。
この機能を、ブラウザ、CLI, Rustアプリケーション、モバイルアプリケーション、ゲームエンジンなど、
様々なプラットフォームから呼び出すためのライブラリを提供します。

## ステータス

`mmd-anim` は、まだ評価段階です。

本家MMDから出力したデータとの検証、およびいくつかのPMX/VMDデータで検証されていますが、使用実績がすくないため、
APIや機能はまだ固定されておらず、1.0 までに互換性のない変更が入る可能性があります。
ぜひともフィードバックお待ちしております。

## ランタイム評価

- PMX（モデル）を読み込んで、再生に使えるモデルデータに変換する。
- VMD（モーション）を読み込み、ボーン・カメラ・ライトなどのモーションを、再生できる形に変換する。
- MMD と同じベジェ補間（位置・回転）で計算するので、動きの緩急を再現できる。

> **物理演算は行いません。** 剛体・ジョイントのデータは読み書きできますが、揺れものなどの物理シミュレーションは提供しません。物理が必要な場合は、ホスト側の物理エンジンと組み合わせてください。

## テスト基盤

`mmd-anim` は複数のプロジェクトで共有するアニメーション基盤なので、「結果が正しいこと」を重視してテストを整備しています。

このリポジトリでは、次のようなテストを行っています。

- アニメーションの再生計算、ボーンの親子関係の計算、IK、付与変形、モーフ、各形式の読み書きが正しく動くかを確かめる単体テスト。
- 読み込んだデータを書き出し、もう一度読み込んでも内容が変わらないことを確かめるテスト（往復テスト）。
- PMX/VMD を実際にフレーム単位で評価し、想定どおりの結果になるかを確かめるテスト。
- 読み込んだモデルや計算結果を見比べるための、開発者向け CLI による点検。
- 各プラットフォーム（C ABI / WASM）から呼んでも、同じ計算が行われることを確かめる動作確認テスト。

公開リリース前には、次のチェックを実行することを推奨します。

```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

## 採用プロジェクト

`mmd-anim` は、MMD 関連プロジェクトで共有するアニメーション backend として開発されています。

- [three-mmd-loader](https://github.com/yohawing/three-mmd-loader): `mmd-anim` を
  アニメーション・形式処理 backend として利用する Three.js 向け MMD loader。
- [maya_mmd_tools](https://github.com/yohawing/maya_mmd_tools):
  Maya 向け MMD アニメーション編集ツールとして利用する Maya プラグイン。 VMDインポート時のフルベイク用と、リグ実装にあたっての正本として利用。
- [unity-mmd-loader](https://github.com/yohawing/unity-mmd-loader): Unity6、URPに最適化Unity向けMMD Loader。インポーターと、コアアニメーションランタイムとして利用。

Rust API、C ABI、WASM wrapper を通じて、他のホストや製品にも同じ機能を組み込めます。

## 対応形式

形式ごとの対応状況です。「読み込み」は対象ファイルを解析して構造化データにすること、
「書き出し」は対象ファイルとして出力できることを指します。

| 形式 | 読み込み | 書き出し |
|--------|-----------|-------------------|
| PMX | モデル各セクションの構造化 + ソフトボディのヘッダ診断 | 書き出し / JSON 変換 / メッシュデータから生成 |
| PMD | モデルの構造化 + 一部のランタイム取り込み | 書き出し / JSON 変換 |
| VMD | **対応** | **対応** |
| VPD | **対応** | **対応** |
| PMM | ヘッダ、タイムライン、表示状態、参照アセット、PMMv2 の概要情報、一部 keyframe payload metadata | 部分対応: parse 済み byte の lossless round trip、限定 source-byte patch、単一モデル PMX/VMD scene の試験生成 |
| X/VAC | テキスト X のメッシュ、材質、UV、法線、頂点色の構造化 + VAC の設定/生データ行 | テキスト X / VAC ラッパーの書き出し |
| FBX | 読み込みなし | 試験対応: PMX mesh / skeleton / skin / bind pose、vertex morph blendshape、runtime bake 済み VMD ボーン + vertex morph animation、bones-only skeleton / motion 出力を FBX 7.4 binary として書き出し |

## Rust から使う

```toml
[dependencies]
mmd-anim = "0.1"
```

## ネイティブ (C ABI) から使う

ネイティブアプリやゲームエンジンなどのホストからは、`mmd-anim-ffi` の C ABI を利用します。
これは特定のエンジンに限定したものではなく、C ABI を呼び出せる環境であれば利用できます
（Unity はその一例です）。
ヘッダーは [crates/mmd-anim-ffi/include/mmd_runtime.h](../crates/mmd-anim-ffi/include/mmd_runtime.h) です。

```c
// 1. PMX のバイト列からモデルを作成
mmd_runtime_model_t* model =
    mmd_runtime_model_create_from_pmx_bytes(pmx_bytes, pmx_len);

// 2. VMD のバイト列からアニメーションクリップを作成
mmd_runtime_clip_t* clip =
    mmd_runtime_clip_create_from_vmd_bytes_for_model(model, vmd_bytes, vmd_len);

// 3. インスタンスを作成
mmd_runtime_instance_t* instance =
    mmd_runtime_instance_create_for_model(model);

// 4. フレーム評価
mmd_runtime_instance_evaluate_clip_frame(instance, clip, 300.0f);

// 5. ワールド行列をコピー
size_t len = mmd_runtime_instance_world_matrix_f32_len(instance);
mmd_runtime_instance_copy_world_matrices(instance, out_f32, len);

// 6. 解放
mmd_runtime_instance_free(instance);
mmd_runtime_clip_free(clip);
mmd_runtime_model_free(model);
```

想定している分担は、メッシュ、材質、テクスチャはホスト側で保持し、このランタイムからは行列、モーフ、IK 状態だけを受け取る形です。
ホスト側の形状データから PMX を生成したい場合は `mmd_runtime_export_pmx_from_parts` を使います。
入力配列の所有権は呼び出し元に残り、返却されたバイト列は `mmd_runtime_byte_buffer_free` で解放します。

## WASM / ブラウザから使う

ビルドはブラウザ向けの `wasm-pack build --target web` に固定しています。Node.js 単体用ビルドは使いません。

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

const world = runtime.worldMatrices();

// 不要になったら必ず解放する。
runtime.free();
clip.free();
model.free();

// 読み込み / 書き出し (runtime handle 不要)
const json = parseMmdFormatJson(vmdBytes, "motion.vmd");
const exportedBytes = exportVmdAnimationJsonBytes(json);
const normalizedBytes = exportMmdFormatBytes(vmdBytes, "motion.vmd");

// 型付き配列から PMX を生成
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

`mmd-anim-cli` は MMD 形式ファイル（PMX, VMD, VPD, PMM, X/VAC）の検査・変換・診断を行うコマンドラインツールです。

```powershell
cargo install mmd-anim-cli
```

インストール後、`mmd-anim` コマンドが使えます。

```powershell
mmd-anim --help
```

開発中はワークスペースから直接実行することもできます。

```powershell
cargo run -p mmd-anim-cli -- --help
```

PMXとVMDから、アニメーション付きFBXを書き出せます。

```powershell
mmd-anim convert-fbx model.pmx model.fbx --vmd motion.vmd --max-frame 120
mmd-anim convert-fbx model.pmx model.fbx --copy-diffuse-textures
mmd-anim convert-fbx model.pmx motion.fbx --vmd motion.vmd --bones-only
mmd-anim convert-fbx model.pmx model.fbx --readable-bone-names
```

`--vmd` 指定時の `convert-fbx` は、ボーンと vertex morph weight を runtime bake
経路で出力します。IK、付与変形、fixed-axis constraint は FBX animation curve
へ書く前にサンプルされます。camera、light、self-shadow、visibility、physics、
非 vertex morph track は FBX track としては出力しません。

`--bones-only` を指定すると、mesh、material、skin cluster、bind pose、texture、
blendshape を出さず、FBX skeleton と任意の runtime bake 済み bone animation だけを
書き出します。

既定の bone 名は互換性のため legacy UTF-8 hex です。`--readable-bone-names` を指定すると、
PMX 英語名、標準 MMD 辞書、sanitize 済み ASCII fallback を使う readable policy に切り替わります。
この指定時は、PMX bone index、元の名前、FBX 名、名前 source を記録した
`<fbx-stem>.bone-map.json` も横に出力します。

既定では PMX の diffuse texture path をそのまま FBX に書きます。
`--copy-diffuse-textures` を指定すると、参照された diffuse texture を FBX 横の
`*-textures` ディレクトリへコピーし、FBX 内の path をその相対 path に差し替えます。
sphere / toon / material morph texture は対象外です。

## クレート構成

| Crate | 役割 |
|---|---|
| `mmd-anim` | 主要な公開クレート。評価コアと形式処理をまとめて使えるようにする。 |
| `mmd-anim-runtime` | ファイル形式に依存しない評価コア。モデルアリーナ、ポーズ、VMD 評価、付与変形、IK、モーフを扱う。 |
| `mmd-anim-format` | PMX/VMD のランタイム取り込み、形式判定、読み込み（構造化）、PMX/PMD/VMD/VPD/X/VAC の書き出しを提供する。 |
| `mmd-anim-ffi` | ネイティブホスト向けの C ABI。ランタイム操作と PMX パーツ書き出しを公開する。0.1.x 系ではリポジトリ内専用。 |
| `mmd-anim-wasm` | ブラウザ向けの `wasm-bindgen` ラッパー。ランタイム操作、読み込み/書き出し、PMX パーツ書き出しを公開する。0.1.x 系ではワークスペース内専用。 |
| `mmd-anim-cli` | MMD 形式ファイルの検査・変換・診断コマンド。メンテナ向け oracle / numeric compare schema もこの crate 側に含む。`cargo install mmd-anim-cli` でインストール可能。 |

通常のライブラリ利用では `mmd-anim` を依存に追加してください。低レイヤだけを直接使いたい場合は
`mmd-anim-format` や `mmd-anim-runtime` に直接依存できます。

## 現在の制限と注意点

- **評価コア:** PMD は読み込みと一部のランタイム取り込み（ボーン、IK、モーフ枠、頂点モーフの移動量）に対応していますが、描画側の頂点変形や PMD の完全互換ではありません。
- **書き出し:** メッシュからの生成は形状、材質、ボーン、表示枠、モーフ、物理情報の初期範囲までです。PMM の書き出しは、現在の PMM manifest parser が表現している範囲に限定されます。
- **PMM:** プロジェクトのヘッダ情報、タイムライン由来の値、表示状態、モデル枠の初期範囲、参照アセット、PMMv2 の概要情報、アセット/ヘッダの整合性診断までです。PMM exporter は、この限定された manifest/header/slot/asset-reference 情報を PMMv2 ファイルとして再出力できますが、完全な PMM project graph exporter ではありません。parser が要約だけしている、または保持していないキーフレーム本体、camera/light/accessory/self-shadow の完全なトラック、その他のバイナリ project graph データは `PmmParsedManifest` から復元できません。
- **X/VAC:** テキスト X のメッシュ、材質、法線、UV、頂点色と VAC の共通行順を扱います。バイナリ X は診断のみです。
- **物理演算:** 剛体・ジョイントのデータの読み書きには対応しますが、物理シミュレーション自体は提供しません。揺れものなどはホスト側の物理エンジンで処理してください。
- **API / ABI / WASM:** まだ実験段階です。外部ホストに繋ぐ場合は、まず簡単な動作確認と代表フレームでの確認から始めてください。

## 参考にしたプロジェクト

このプロジェクトは、以下の実装を参考にしながら開発しています。

- [Babylon-MMD](https://github.com/noname0310/babylon-mmd)
- [saba](https://github.com/benikabocha/saba)
- [nanoem](https://github.com/hkrn/nanoem)
