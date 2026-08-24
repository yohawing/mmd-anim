# MMDDumper

MikuMikuDance 9.32 x64 を read-only の oracle runtime として使い、MMD が評価した bone / morph / camera の状態を JSONL に記録するリポジトリ内プラグインです。MMD 本体やモデルなどの外部バイナリはこのディレクトリへコミットしません。

## セットアップ

Node.js 22 以上と pnpm を用意し、リポジトリのルートで依存関係をインストールします。

```powershell
pnpm --dir MMDDumper install --frozen-lockfile
```

native DLL をビルドする場合は Visual Studio の C++ ツール、CMake、MMDPlugin SDK の `MMDExport.lib` と `mmd_plugin.h` が必要です。入力資産は `MMDDumper` の外部に置き、manifest または fixture から絶対パス／相対パスで指定します。

## 基本操作

PMX から MMD で開ける最小 PMM を作成します。

```powershell
pnpm --dir MMDDumper create-base-pmm-from-pmx -- --pmx <model.pmx> --out <base.pmm>
```

PMX と VMD の bone / morph keyframe を PMM に埋め込みます。

```powershell
pnpm --dir MMDDumper create-pmm-from-pmx-vmd -- --pmx <model.pmx> --vmd <motion.vmd> --out <scene.pmm>
```

複数ケースは manifest で dry-run できます。`cases[].pmx`、`cases[].vmd`、`templates[]` の相対パスは manifest 基準、出力先は MMDDumper の作業ディレクトリ基準です。

```powershell
pnpm --dir MMDDumper oracle-batch -- --manifest <oracle-batch.json> --dry-run true
```

生成済み fixture を検証します。

```powershell
pnpm --dir MMDDumper validate -- <oracle.actual.jsonl>
pnpm --dir MMDDumper verify-coverage -- --fixture <fixture.json>
```

## 実 MMD で記録する

MMD の起動は事故防止のため二重 opt-in です。fixture／case 側で `recordOptIn: true` を指定し、実行時に環境変数を明示してください。

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm --dir MMDDumper oracle-batch -- --manifest <oracle-batch.json>
Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH
```

MMD は GUI / DirectX アプリなので、ログイン済み Windows セッションで実行します。実行中はウィンドウのフォーカスを取得する場合があります。runner は timeout、終了処理、生成物の atomic promotion、失敗時の診断 bundle を管理します。失敗時は `record-failure.zip` を残し、直前の成功結果を上書きしません。

Python の `mmd-oracle-runner` から prepare / record を行う場合も、Node 側の実行 root は自動的に `MMDDumper` を使います。

```powershell
uv run --project tools/mmd-oracle-runner mmd-oracle-runner prepare-batch --case <case.json>
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
uv run --project tools/mmd-oracle-runner mmd-oracle-runner record-batch --case <case.json> --mmd-exe <MikuMikuDance.exe>
Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH
```

## 調査・比較コマンド

```powershell
pnpm --dir MMDDumper inspect-vmd -- <motion.vmd>
pnpm --dir MMDDumper inspect-pmm-document-keyframes -- <scene.pmm>
pnpm --dir MMDDumper compare-pmm-document-vmd-keyframes -- --pmm <scene.pmm> --vmd <motion.vmd> --target-slot 0
pnpm --dir MMDDumper patch-pmm-document-vmd-keyframes -- --template <base.pmm> --target-vmd <motion.vmd> --out <patched.pmm> --target-slot 0
```

直接実行が必要な低レベルコマンドは次の入口から呼び出せます。

```powershell
node MMDDumper/src/cli.mjs <command> ...
```

## テストと native packaging

通常の Node テスト:

```powershell
pnpm --dir MMDDumper test
```

native writer の smoke test と MMDPlugin DLL の package:

```powershell
pnpm --dir MMDDumper native:test
pnpm --dir MMDDumper native:package
```

native build の入力は `MMDDumper/native/CMakeLists.txt`、ヘッダは `MMDDumper/lib/`、成果物は `MMDDumper/out/` にまとまります。MMD 本体の `d3d9.dll` を恒久的に差し替える運用は行いません。MMDPlugin 経路では `Plugin/mmd_oracle_plugin.dll` をロードし、終了時に変更を復元します。

## 安全上の制約

- 対象は MikuMikuDance 9.32 x64。runner は対象 executable を検証します。
- dump は read-only で、外部通信・injector・権限昇格・常駐化を行いません。
- MMD 起動は `MMD_DUMPER_ALLOW_MMD_LAUNCH=1` と case の `recordOptIn` の両方が必要です。
- 物理、材質、テクスチャ、頂点の完全一致はこの plugin の契約外です。
- 生成物・MMD バイナリ・モデル資産は git 管理対象外です。
