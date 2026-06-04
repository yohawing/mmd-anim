# mmd-anim

`mmd-anim` は、MikuMikuDance 向けツール群のための、検証済み Rust アニメーション基盤です。

PMX/VMD をランタイム向けに正規化し、任意フレームのワールド行列、スキニング行列、
モーフ重み、IK 状態を評価する、レンダラー非依存の評価コアを提供します。
同じ評価コアを Rust ライブラリ、ネイティブ連携、ブラウザ WASM、CLI 診断、
および下流の MMD 製品群から共有できます。

このリポジトリは、`yohawing` MMD ツールチェーンのアニメーション中核として設計されています。
形式の読み込み・書き出しは、ランタイム評価、変換、診断、アセット生成を支えるために提供されます。

## ステータス

`mmd-anim` は production-oriented ですが、まだ pre-1.0 です。

ランタイム構造、検証方針、対応形式の処理系は、下流連携で使う前提で設計しています。
ただし Rust API、C ABI、WASM の呼び出し口はまだ固定されておらず、1.0 までに互換性のない変更が入る可能性があります。

## ランタイム評価

- PMX バイト列からランタイム用モデルを構築する。
- VMD バイト列を、PMX 由来の名前マップで解決して `AnimationClip` に変換する。
- 任意のフレームを評価して、ワールド行列、スキニング用行列、モーフの重み、IK の有効状態を取得する。

## 検証

`mmd-anim` は共有アニメーション基盤として開発しているため、正しさを公開 API の一部として扱います。

公開リポジトリでは、次の検証を行います。

- アニメーションサンプリング、階層評価、IK descriptor、付与変形、モーフ展開、形式別の読み書き経路に対する Rust unit test。
- 意味を保った出力が期待される writer 経路の round-trip check。
- PMX/VMD ランタイム評価挙動に対する synthetic runtime frame check。
- 取り込んだアセットや評価済みアニメーション状態を比較するための、メンテナ向け CLI 診断。
- ホスト向け API が同じランタイム経路を通ることを確認する FFI / WASM smoke test。

公開リリース前の推奨チェックは次の通りです。

```powershell
rtk cargo test --workspace
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo doc --workspace --no-deps
```

一部の参照アセットはライセンス上の理由で公開リポジトリには含めていません。
メンテナ専用のローカルアセット照合や参照データとの比較はリリース判断に有用ですが、
配布可能な公開リリースゲートには含めていません。

## 採用プロジェクト

`mmd-anim` は、MMD 関連プロジェクトで共有するアニメーション backend として開発されています。

- [three-mmd-loader](https://github.com/yohawing/three-mmd-loader): `mmd-anim` を
  アニメーション・形式処理 backend として利用する Three.js 向け MMD loader。

Rust API、C ABI、WASM wrapper を通じて、他のホストや製品にも同じ評価コアを組み込めます。

## 対応形式

形式ごとの対応状況です。「読み込み」は対象ファイルを解析して構造化データにすること、
「書き出し」は対象ファイルとして出力できることを指します。

| 形式 | 読み込み | 書き出し |
|--------|-----------|-------------------|
| PMX | モデル各セクションの構造化 + ソフトボディのヘッダ診断 | 意味を保った書き出し / JSON を介した読み書き / パーツからの生成 |
| PMD | モデルの構造化 + 一部のランタイム取り込み | 意味を保った書き出し / JSON を介した読み書き |
| VMD | アニメーションの構造化 | **対応** |
| PMM | ヘッダ、タイムライン、表示状態、参照アセット、PMMv2 の概要情報 | — |
| VPD | ポーズの構造化 | **対応** |
| X/VAC | テキスト X のメッシュ、材質、UV、法線、頂点色の構造化 + VAC の設定/生データ行 | テキスト X / VAC ラッパーの意味を保った書き出し |

## クレート構成

| Crate | 役割 |
|---|---|
| `mmd-anim` | 主要な公開クレート。評価コアと形式処理をまとめて使えるようにする。 |
| `mmd-anim-runtime` | ファイル形式に依存しない評価コア。モデルアリーナ、ポーズ、VMD 評価、付与変形、IK、モーフを扱う。 |
| `mmd-anim-format` | PMX/VMD のランタイム取り込み、形式判定、読み込み（構造化）、PMX/PMD/VMD/VPD/X/VAC の書き出しを提供する。 |
| `mmd-anim-ffi` | ネイティブホスト向けの C ABI。ランタイム操作と PMX パーツ書き出しを公開する。0.1.0 ではリポジトリ内専用。 |
| `mmd-anim-wasm` | ブラウザ向けの `wasm-bindgen` ラッパー。ランタイム操作、読み込み/書き出し、PMX パーツ書き出しを公開する。0.1.0 ではワークスペース内専用。 |
| `mmd-anim-cli` | メンテナ向けの診断・検証コマンド。0.1.0 ではリポジトリ内専用。 |
| `mmd-anim-schema` | メンテナ向けの品質確認用スキーマ補助クレート。0.1.0 ではリポジトリ内専用。 |

通常のライブラリ利用では `mmd-anim` を依存に追加してください。低レイヤだけを直接使いたい場合は
`mmd-anim-format` や `mmd-anim-runtime` に直接依存できます。

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

このネイティブ連携クレートは 0.1.0 では crates.io への公開対象外で、Rust ワークスペース内のビルド・チェック対象として維持しています。

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
const world = runtime.worldMatricesView();

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

`worldMatricesView()` はコピーを避けますが、次の評価処理や WASM メモリの拡張で無効になります。
永続化する場合は `worldMatrices()` または `copyWorldMatrices()` を使ってください。

WASM パッケージは 0.1.0 では crates.io への公開対象外です。Rust ワークスペース内ではビルド・チェック対象として維持します。

## CLI で読み込み / 書き出しを確認する

形式の読み込みや書き出しを手元で確認したい場合は、リポジトリ内専用の `mmd-anim-cli` が使えます。
利用できるサブコマンドの一覧は次のコマンドで確認できます。

```powershell
rtk cargo run -p mmd-anim-cli -- --help
```

このコマンドラインツールはメンテナ向けの診断ツールで、公開リリースの前提条件にはしていません。

## 現在の制限と注意点

- **評価コア:** PMD は読み込みと一部のランタイム取り込み（ボーン、IK、モーフ枠、頂点モーフの移動量）に対応していますが、描画側の頂点変形や PMD の完全互換はまだ名乗りません。
- **書き出し:** PMX パーツからの生成は形状、材質、ボーン、表示枠、モーフ、物理情報の初期範囲までです。PMM の書き出しは、プロジェクト全体のグラフを保持できるようになるまで未提供です。
- **PMM:** プロジェクトのヘッダ情報、タイムライン由来の値、表示状態、モデル枠の初期範囲、参照アセット、PMMv2 の概要情報、アセット/ヘッダの整合性診断までです。プロジェクト全体のグラフを保持しないため、書き出しは未提供です。
- **X/VAC:** テキスト X のメッシュ、材質、法線、UV、頂点色と VAC の共通行順を扱います。バイナリ X は診断のみです。
- **API / ABI / WASM:** まだ実験段階です。外部ホストに繋ぐ場合は、まず簡単な動作確認と代表フレームでの確認から始めてください。

## 参考にしたプロジェクト

このプロジェクトは、以下の実装を参考にしながら開発しています。

- [Babylon-MMD](https://github.com/noname0310/babylon-mmd)
- [saba](https://github.com/benikabocha/saba)
- [nanoem](https://github.com/hkrn/nanoem)
