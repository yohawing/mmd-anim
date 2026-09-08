# mmd-anim

`mmd-anim`は、MikuMikuDanceのアニメーションをマルチプラットフォームで再生するRust製の基盤です。

PMXやVMDを読み込み、任意のフレームでワールド行列、スキニング行列、モーフの重み、IKの状態を計算します。ブラウザ、CLI、Rustアプリケーション、モバイルアプリケーション、ゲームエンジンなどから利用できます。

## ステータス

`mmd-anim`は評価段階です。

本家MMDから出力したデータと複数のPMX/VMDデータで検証していますが、利用実績はまだ限られています。APIや機能は固定されておらず、1.0までに互換性のない変更が入る可能性があります。フィードバックをお待ちしています。

## ランタイム評価

- PMX（モデル）を読み込み、再生に使えるモデルデータへ変換する。
- VMD（モーション）を読み込み、ボーン、カメラ、ライトなどのモーションを再生できる形へ変換する。
- MMDと同じベジェ補間（位置、回転）で計算し、動きの緩急を再現する。
- `model-bound host rig`のポーズを評価し、ホストが管理するボーンと明示されたIK goalを保持する。
- Bullet PhysicsによるMMD向けの物理演算をCLIやAPIから利用できる。

## テスト基盤

`mmd-anim`は複数のプロジェクトで共有するアニメーション基盤です。結果の正しさを重視してテストを整備しています。

このリポジトリでは、次のテストを行っています。

- アニメーションの再生計算、ボーンの親子関係の計算、IK、付与変形、モーフ、各形式の読み書きが正しく動くかを確かめる単体テスト。
- 読み込んだデータを書き出し、もう一度読み込んでも内容が変わらないことを確かめるテスト（往復テスト）。
- PMX/VMDを実際にフレーム単位で評価し、想定どおりの結果になるかを確かめるテスト。
- 読み込んだモデルや計算結果を見比べるための、開発者向けCLIによる点検。
- 各プラットフォーム（C ABI、WASM）から呼び出しても同じ計算になることを確かめる動作確認テスト。

公開リリース前には、次のチェックを実行することを推奨します。

```powershell
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --workspace --no-deps
```

## 採用プロジェクト

`mmd-anim`は、MMD関連プロジェクトで共有するアニメーションバックエンドとして開発しています。

- [three-mmd-loader](https://github.com/yohawing/three-mmd-loader): `mmd-anim`をアニメーションと形式処理のバックエンドとして利用する、Three.js向けMMDローダー。
- [maya_mmd_tools](https://github.com/yohawing/maya_mmd_tools): Maya向けMMDアニメーション編集プラグイン。VMDインポート時のフルベイクと、リグ実装の基盤として`mmd-anim`を利用します。
- [unity-mmd-loader](https://github.com/yohawing/unity-mmd-loader): Unity 6とURP向けのMMDローダー。インポーターとコアアニメーションランタイムに`mmd-anim`を利用します。

Rust API、C ABI、WASMラッパーを通じて、他のホストや製品にも同じ機能を組み込めます。

## 対応形式

形式ごとの対応状況を示します。「読み込み」は対象ファイルを解析して構造化データにすること、
「書き出し」は対象ファイルとして出力することを指します。

| 形式 | 読み込み | 書き出し |
|--------|-----------|-------------------|
| PMX | モデル各セクションの構造化とソフトボディのヘッダー診断 | 書き出し、JSON変換、メッシュデータからの生成 |
| PMD | モデルの構造化と一部のランタイム取り込み | 書き出し、JSON変換 |
| VMD | **対応** | **対応** |
| VPD | **対応** | **対応** |
| PMM | ヘッダー、タイムライン、表示状態、参照アセット、PMMv2の概要情報、一部のkeyframe payload metadata | 部分対応。元データの一部を書き換え、単一モデルのPMX/VMDシーンを試験的に生成できます。 |
| X/VAC | テキストXのメッシュ、材質、UV、法線、頂点色の構造化と、VACの設定および生データ行 | テキストXとVACラッパーの書き出し |
| FBX | 読み込み非対応 | 試験対応。PMXのメッシュ、スケルトン、スキン、バインドポーズ、頂点モーフ（ブレンドシェイプ）に加え、ランタイムでベイクしたVMDのボーンと頂点モーフアニメーションをバイナリ形式で出力します。 |

## Rustから使う

```toml
[dependencies]
mmd-anim = "0.5.0"
```

## ネイティブ（C ABI）から使う

ネイティブアプリやゲームエンジンなどのホストからは、`mmd-anim-ffi`のC ABIを利用します。特定のエンジンに限定されず、C ABIを呼び出せる環境で利用できます（Unityはその一例です）。ヘッダーは[crates/mmd-anim-ffi/include/mmd_runtime.h](../crates/mmd-anim-ffi/include/mmd_runtime.h)です。

ネイティブVMDの書き出しでは、`mmd_runtime_export_vmd_from_parts`に型付きのBone/Morph SoA配列と、名前および低密度のカメラ、ライト、セルフシャドウ、Property/IKセクションを含むJSONメタデータを渡せます。ファイルI/Oは行わず、所有権を移したVMD 0002バイト列を返します。返却バッファは呼び出し側で`mmd_runtime_byte_buffer_free`を使って解放してください。高密度のキー値はJSONに展開しません。

VPDポーズの受け渡しには、camelCaseのJSONポーズDTOをShift-JISのVPDバイト列へ変換する`mmd_runtime_export_vpd_pose_json`と、VPDバイト列をUTF-8のJSONへ戻す`mmd_runtime_parse_vpd_pose_json`を利用できます。どちらの返却バッファも呼び出し側で`mmd_runtime_byte_buffer_free`を使って解放してください。

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

ホストはメッシュ、材質、テクスチャを保持し、ランタイムから行列、モーフ、IK状態を受け取る構成を想定しています。ホスト側の形状データからPMXを生成する場合は`mmd_runtime_export_pmx_from_parts`を使います。入力配列の所有権は呼び出し元に残り、返却されたバイト列は`mmd_runtime_byte_buffer_free`で解放します。

## WASM/ブラウザから使う

ビルドはブラウザ向けの`wasm-pack build --target web`に固定しています。Node.js単体用ビルドは使いません。

```powershell
cd .\crates\mmd-anim-wasm\harness
npm run build
```

生成物は`crates/mmd-anim-wasm/harness/pkg/`に出ます。

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

`mmd-anim-cli`は、MMD形式ファイル（PMX、VMD、VPD、PMM、X/VAC）の検査、変換、診断を行うコマンドラインツールです。現在は`mmd-anim-package`に依存するため、crates.ioでは公開していません。GitHub Releasesのバイナリ、またはこのワークスペースからビルドしたCLIを使います。

次の例では、ワークスペースからCLIを実行します。GitHub Releasesのバイナリを使う場合は、コマンドの先頭にある`cargo run -p mmd-anim-cli --`を`mmd-anim`に置き換えてください。

```powershell
cargo run -p mmd-anim-cli -- --help
```

CLI、ネイティブAPI、物理演算クレートをソースからビルドする場合は、対象環境向けのC++コンパイラが必要です。Bullet自体を別途インストールする必要はありません。

PMXとVMDからアニメーション付きFBXを書き出すには、次を実行します。

```powershell
cargo run -p mmd-anim-cli -- convert-fbx model.pmx model.fbx --vmd motion.vmd --max-frame 120
```

## MMDPACK（試験的）

MMDPACKは、コーデック処理済み（codec-ready）のモデル、モーション、テクスチャ、音声などのデータを、1つの認証付き暗号化パッケージにまとめる形式です。CLIでは、マニフェストの認証、エントリの復号と展開、PMXのテクスチャ対応付けの検証を行います。PNG/JPEGのデコードや画像形式の変換は行いません。

### パッケージを作る

`assets/`に入力ファイルと`mmdpack.json`を用意します。`mmdpack.json`は、入力ファイルの種類、コーデック、圧縮方式、モデルとテクスチャの対応を指定する厳密なJSON（`strict JSON`）です。次の例は、テクスチャを含まないPMXモデル1つだけの最小構成です。

```json
{
  "defaultModelEntryId": 1,
  "entries": [
    {
      "id": 1,
      "path": "model/model.pmx",
      "kind": "model",
      "codec": "pmx",
      "compression": "none"
    }
  ],
  "modelBindings": [
    { "modelEntryId": 1, "textureBindings": [] }
  ]
}
```

`assets/model/model.pmx`を配置したら、次のコマンドでパッケージを作成します。

```powershell
cargo run -p mmd-anim-cli -- package pack assets --config assets/mmdpack.json -o scene.mmdpack --key-out scene.key
```

パッケージを作成したら、次のコマンドで内容を検証します。`--strict-codecs`を付けると、パッケージ側で認識できないコーデックをエラーとして扱います。

```powershell
cargo run -p mmd-anim-cli -- package verify scene.mmdpack --key-file scene.key --strict-codecs
```

`package pack`は`scene.mmdpack`と32バイトの鍵ファイル`scene.key`を作成します。2つの出力先は、あらかじめ存在しないパスを指定します。鍵はパッケージと分けて保管し、リポジトリへ登録しないでください。

`mmd-anim-package`は現在ワークスペース内だけで利用できる非公開クレートです。Draft 0.2のフォーマット、`codec profile`、公開設定はV1までに変更される可能性があります。

## クレート構成

| クレート | 役割 |
|---|---|
| `mmd-anim` | 主要な公開クレート。評価コアと形式処理をまとめて利用できます。 |
| `mmd-anim-runtime` | ファイル形式に依存しない評価コア。モデルアリーナ、ポーズ、VMD評価、付与変形、IK、モーフを扱います。 |
| `mmd-anim-format` | PMX/VMDのランタイム取り込み、形式判定、構造化データへの読み込み、PMX/PMD/VMD/VPD/X/VACの書き出しを提供します。 |
| `mmd-anim-physics-bullet` | MMD向けのBullet Physicsバックエンド。同梱のBullet3を対象環境向けにビルドし、ランタイムやPMX形式と連携します。 |
| `mmd-anim-package` | Draft MMDPACKの試験的な読み込みとパッキング。現在はワークスペース内だけで利用でき、wire formatは未確定です。codec-ready payloadをそのまま扱います。 |
| `mmd-anim-ffi` | ネイティブホスト向けのC ABI。ランタイム操作、PMXパーツの書き出し、疎カーブ、物理演算を公開します。crates.ioでは公開していません。 |
| `mmd-anim-wasm` | ブラウザ向けの`wasm-bindgen`ラッパー。ランタイム操作、読み込みと書き出し、PMXパーツの書き出し、疎カーブを公開します。crates.ioでは公開していません。 |
| `mmd-anim-cli` | MMD形式ファイルの検査、変換、診断を行うコマンド。メンテナ向けのoracleやnumeric compare schemaも含みます。crates.ioでは公開していませんが、GitHub ReleasesでCLIバイナリを提供します。 |

通常のライブラリ利用では`mmd-anim`を依存に追加します。低レイヤだけを直接使う場合は、
`mmd-anim-format`や`mmd-anim-runtime`に直接依存できます。Rustから物理演算バックエンドを直接使う場合は、
`mmd-anim-physics-bullet`を利用します。

## 現在の制限と注意点

- **評価コア:** PMDの読み込みと一部のランタイム取り込み（ボーン、IK、モーフ枠、頂点モーフの移動量）に対応しています。描画側の頂点変形やPMDとの完全互換には対応していません。
- **書き出し:** メッシュからの生成は、形状、材質、ボーン、表示枠、モーフ、物理情報の初期範囲に限られます。PMMの書き出しも、現在のPMM manifest parserが表現できる範囲に限られます。
- **PMM:** プロジェクトのヘッダー情報、タイムライン由来の値、表示状態、モデル枠の初期範囲、参照アセット、PMMv2の概要情報、アセットとヘッダーの整合性診断に対応しています。PMM exporterは、保持しているmanifest、header、slot、asset-reference情報をPMMv2ファイルとして再出力できますが、完全なPMM project graph exporterではありません。parserが要約または保持していないキーフレーム本体、camera、light、accessory、self-shadowの完全なトラック、その他のバイナリproject graphデータは`PmmParsedManifest`から復元できません。
- **X/VAC:** テキストXのメッシュ、材質、法線、UV、頂点色と、VACの共通行順を扱います。バイナリXは診断のみです。
- **MMDPACK:** `mmd-anim-package`と関連CLIコマンドは、Draft 0.2の試験対応です。フォーマット、`codec profile`、公開設定はV1までに変更される可能性があります。現在のpackはcodec-ready payloadの受け渡しに限られ、PNG/JPEGのデコード、パッケージのWASM/FFI、高レベルのPMX/VMD読み込みには対応していません。`mmd-anim-package`と`mmd-anim-cli`はcrates.ioで公開していません。利用時はワークスペースからビルドするか、GitHub ReleasesのCLIバイナリを使ってください。

## 参考にしたプロジェクト

このプロジェクトは、以下の実装を参考にしています。

- [Babylon-MMD](https://github.com/noname0310/babylon-mmd)
- [saba](https://github.com/benikabocha/saba)
- [nanoem](https://github.com/hkrn/nanoem)
