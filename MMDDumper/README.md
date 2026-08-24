# MMDDumper

MikuMikuDance からボーン・モーフの状態を JSONL で取得する、リポジトリ内完結のネイティブプラグインです。実行時に Node.js / npm / pnpm は必要ありません。

## ビルド

リポジトリのルートで、CMake と C++ コンパイラが使える環境から実行します。

```powershell
python MMDDumper/scripts/native_smoke.py
python MMDDumper/scripts/package_native.py
```

`MMDExport.lib` が `MMDDumper/lib/mmd/` にある場合は、MMD 本体用 DLL もビルドされます。生成物は `MMDDumper/out/`（Git ignore 対象）にのみ出力されます。

## MMD で録画する

Python runner で PMX と VMD から PMM を生成し、明示的に許可した場合だけ MMD を起動します。

```powershell
uv run --project tools/mmd-oracle-runner mmd-oracle-runner prepare --case C:/absolute/case.json
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
uv run --project tools/mmd-oracle-runner mmd-oracle-runner record `
  --case C:/absolute/case.json `
  --mmd-exe C:/absolute/MikuMikuDance.exe
Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH
```

ケースの `generatorBackend` は `python-rust` を指定します。PMX の UTF-8 テキストは Python が MMD 用 UTF-16LE にステージングし、PMM の読み書きは `mmd-anim-cli build-pmm` が担当します。録画結果はケースの `outputRoot` 配下に保存され、JSONL のスキーマと指定フレームを検証してから安定ファイルへ昇格します。

最小のケース例:

```json
{
  "schemaVersion": 1,
  "name": "body-only",
  "input": {
    "pmx": "C:/absolute/model.pmx",
    "bodyVmd": "C:/absolute/motion.vmd"
  },
  "frames": [0, 15, 30],
  "outputRoot": "C:/absolute/output",
  "generatorBackend": "python-rust",
  "recordOptIn": false,
  "dialogOptIn": false
}
```

通常の Rust / Python テストは以下で実行できます。

```powershell
cargo test --workspace
python -m pytest tools/mmd-oracle-runner
```
