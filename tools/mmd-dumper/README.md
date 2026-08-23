# MMDDumper

MikuMikuDance 9.32 x64 を oracle runtime として使い、`three-mmd-loader` / 3GS loader の検証用 JSONL を作るためのツールです。

主目的は `runtime numeric evidence` です。MMD が実際に評価した frame / model / bone world matrix / morph weight を dump します。

## まず使うコマンド

CI や一括生成では、基本的に `oracle-batch` を使います。

```powershell
pnpm -C MMDDumper oracle-batch -- --manifest <oracle-batch.json> --dry-run true
```

`--dry-run true` は MMD を起動せず、次だけを確認します。

- PMX/VMD case manifest を読める
- PMX から最小PMMを生成できる
- VMD の bone/morph keyframe を一時PMMへ埋め込める
- PMM/VMD keyframe比較が `mismatches=0` になる
- `scene.pmm` と `fixture.json` を生成できる

実際に MMD を起動して oracle JSONL を作る場合は、明示的に許可して `--dry-run true` を外します。

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper oracle-batch -- --manifest <oracle-batch.json>
```

MMD は Windows GUI / DirectX アプリなので、完全なサービス型 headless 実行ではありません。ログイン済み Windows セッション上で動かしてください。フレーム送りのために MMD ウィンドウがフォーカスを取る場合があります。

## Batch Manifest

CI で扱うテストデータは PMX/VMD ペアに寄せます。`templates` は任意です。指定しない場合、PMXのモデル名・bone一覧・morph一覧から最小PMMv2を生成し、そのPMMへVMDのbone/morph keyframeを埋め込みます。

`cases[].pmx` / `cases[].vmd` / `templates[]` の相対パスは manifest ファイル位置基準です。`defaults.outDir` と CLI の `--out-dir` は、コマンド実行時の `MMDDumper` 作業ディレクトリ基準です。

```json
{
  "defaults": {
    "outDir": "out/oracle-batch"
  },
  "cases": [
    {
      "name": "tda-transform",
      "pmx": "../data/pmx/Tda式初音ミクV4X_Ver1.00/Tda式初音ミクV4X_Ver1.00.pmx",
      "vmd": "out/pmm-analysis/tda-parent-center-groove-transform-keys-target.vmd"
    }
  ]
}
```

MMDで作ったbase PMMを使いたい場合だけ、`templates` または caseごとの `templatePmm` を追加します。

```json
{
  "templates": [
    {
      "pmx": "../data/pmx/Tda式初音ミクV4X_Ver1.00/Tda式初音ミクV4X_Ver1.00.pmx",
      "templatePmm": "../data/pmm/tda_base_no_motion.pmm",
      "targetSlot": 0
    }
  ],
  "cases": [
    {
      "name": "tda-transform-template",
      "pmx": "../data/pmx/Tda式初音ミクV4X_Ver1.00/Tda式初音ミクV4X_Ver1.00.pmx",
      "vmd": "out/pmm-analysis/tda-parent-center-groove-transform-keys-target.vmd"
    }
  ]
}
```

特定ケースだけ実行する場合:

```powershell
pnpm -C MMDDumper oracle-batch -- --manifest <oracle-batch.json> --case tda-transform --dry-run true
```

## PMX/VMD To PMM

PMXだけを含む最小PMMを作る場合:

```powershell
pnpm -C MMDDumper create-base-pmm-from-pmx -- --pmx <model.pmx> --out <base.pmm>
```

PMXとVMDから、一時的なscene PMMを作る場合:

```powershell
pnpm -C MMDDumper create-pmm-from-pmx-vmd -- --pmx <model.pmx> --vmd <motion.vmd> --out <scene.pmm>
```

この生成器は現時点で1モデルのbone/morph keyframeを対象にします。VMDが参照するbone/morph名がPMXに存在しない場合、デフォルトではそのkeyframeだけをskipし、reportの `filter.skipped*` に出します。完全一致を要求したい場合は `--missing-names strict` を指定します。

VMD側にcamera / light / self-shadow / property IK channel がある場合は、MMD同等に生成できたとは扱わず拒否または未対応として扱います。

## Single VMD

PMM template と VMD を直接指定して1件だけ作る場合は `oracle-from-vmd` を使います。

```powershell
pnpm -C MMDDumper oracle-from-vmd -- `
  --template-pmm ..\data\pmm\tda_base_no_motion.pmm `
  --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd `
  --out-dir out\oracle-from-vmd-smoke `
  --dry-run true
```

MMDまで起動する場合:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper oracle-from-vmd -- `
  --template-pmm ..\data\pmm\tda_base_no_motion.pmm `
  --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd `
  --out-dir out\oracle-from-vmd-smoke
```

## Existing Fixture

既存の `fixture.json` をそのまま MMD で開いて dump する場合だけ `record` を使います。

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper record -- --fixture fixtures/tda-step/fixture.json
```

出力を読むだけなら:

```powershell
pnpm -C MMDDumper validate -- fixtures/tda-step/oracle.actual.jsonl
pnpm -C MMDDumper verify-coverage -- --fixture fixtures/tda-step/fixture.json
```

## PMM/VMD Keyframe Tools

PMMへ埋め込む前後の確認には、公開入口として次だけを package script に残しています。

```powershell
pnpm -C MMDDumper inspect-vmd -- <motion.vmd>
pnpm -C MMDDumper inspect-pmm-document-keyframes -- <scene.pmm>
pnpm -C MMDDumper compare-pmm-document-vmd-keyframes -- --pmm <scene.pmm> --vmd <motion.vmd> --target-slot 0
pnpm -C MMDDumper patch-pmm-document-vmd-keyframes -- --template <base.pmm> --target-vmd <motion.vmd> --out <patched.pmm> --target-slot 0
```

現在の直接PMM writerが扱うのは、PMMv2 document内のモデル bone/morph keyframe です。camera / light / self-shadow / property IK VMD channel はまだ拒否します。

## Build And Test

通常のJSテスト:

```powershell
pnpm -C MMDDumper test
```

native DLL / proxy を含む確認:

```powershell
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
```

`native:test` は、`lib/mmd/MMDExport.lib` と `lib/mmdplugin/mmd_plugin.h` がある場合に MMDPlugin adapter も build します。adapter の出力は `out/native-mmdplugin-smoke/mmd_oracle_plugin.dll` です。

MMD D3D9 hook smoke:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
pnpm -C MMDDumper mmd:smoke:d3d9-next-frame -- --fixture fixtures/tda-step/fixture.json --timeout-ms 60000 --send-key-repeat 70 --send-key-interval-ms 40 --min-records 3 --min-last-frame 60
```

MMDPlugin hook smoke:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
pnpm -C MMDDumper mmd:smoke:mmdplugin -- --fixture fixtures/sample-basic/fixture.json --timeout-ms 60000 --send-key-after-ms 3000 --send-key right --send-key-repeat 70 --send-key-interval-ms 40 --min-records 2 --min-last-frame 1 --write-done
```

この経路では MMDDumper の `d3d9.dll` を MMD フォルダに差し替えず、MMDPlugin の `d3d9.dll` を入口にして `Plugin/mmd_oracle_plugin.dll` をロードします。

再生中の全フレーム dump smoke:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
pnpm -C MMDDumper mmd:smoke:mmdplugin-playback -- --fixture fixtures/sample-basic/fixture.json --timeout-ms 60000 --min-records 30 --min-last-frame 30 --write-done
```

PostPresent capture smoke:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
pnpm -C MMDDumper mmd:smoke:mmdplugin-capture -- --fixture fixtures/sample-basic/fixture.json --capture-dir out/mmdplugin-capture-smoke --timeout-ms 60000 --min-capture-files 10 --write-done
```

Captured BMP frames can be encoded to MP4:

```powershell
pnpm -C MMDDumper capture:video -- --capture-dir out/mmdplugin-capture-smoke --output out/mmdplugin-capture-smoke/capture.mp4 --fps 30
```

Capture and MP4 encoding can also be run as one MMDPlugin workflow:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
pnpm -C MMDDumper mmd:capture-mp4 -- --fixture fixtures/sample-basic/fixture.json --capture-dir out/mmdplugin-mp4 --output out/mmdplugin-mp4/capture.mp4 --fps 30 --min-capture-files 30
```

The same MMDPlugin MP4 path is available from the normal CLI, so new automation should prefer this over MMD's legacy AVI export automation:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper export-mp4 -- --fixture fixtures/sample-basic/fixture.json --output out/mmdplugin-mp4/capture.mp4 --fps 30 --min-capture-files 30
```

## Advanced Investigation

PMM解析中に作った低レベルコマンドは `src/cli.mjs` に残していますが、日常入口としては package script から外しています。必要な場合は直接呼んでください。

```powershell
node MMDDumper/src/cli.mjs <command> ...
```

主な調査資料:

- `MMDDumper/docs/pmm-loader-development-plan.md`
- `MMDDumper/docs/pmm-motion-record-hypothesis.md`
- `MMDDumper/docs/oss-reference-notes.md`

## Safety

- MMD本体、PMM/PMX/VMD、DLL/EXEなどのバイナリ資産は git ignore 対象です。
- MMD起動系は `MMD_DUMPER_ALLOW_MMD_LAUNCH=1` がないと失敗します。
- injector、権限昇格、外部通信、常駐化は目的にしません。
- 生成物は基本的に `MMDDumper/out/` 以下へ出します。
