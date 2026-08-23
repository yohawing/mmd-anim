# MMDDumper Agent Instructions

このディレクトリでは、MikuMikuDance 9.32 x64 を実機 oracle runtime として扱う。`MMDDumper` の出力は `three-mmd-loader` などの runtime 検証に使う golden numeric evidence であり、nanoem や loader 自身の出力より上位の基準として扱う。

## 基本方針

- 回答は日本語で行う。
- まず実ファイル・実ログ・実コマンド結果を確認する。推測で PMM/VMD/PMX の状態を決めない。
- MMD 実機 dump は golden oracle 生成用。日常の軽量テストや自己整合性チェックと混同しない。
- `native-health` や loader runtime の dump を authoritative oracle と呼ばない。これらは diagnostic/surrogate 扱いにする。
- MMD/PMM/PMX/VMD/DLL/EXE などのバイナリ資産は基本的に git 管理対象外。生成物は原則 `MMDDumper/out/` 以下に置く。

## 安全ルール

- MMD を起動するコマンドは、ユーザーが明示的に許可している場合だけ実行する。実行時は必ず次を設定する。

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
```

- MMD は GUI / DirectX アプリで、起動中にウィンドウを前面化し、右キー送信で frame を進める。フォーカスを奪うことを作業報告で明示する。
- injector、権限昇格、外部通信、常駐化は目的にしない。
- MMD 起動前に `fixture.json`、`scene.pmm`、PMM 内の model reference、`MikuMikuDance.exe` の実在を確認する。preflight が落ちた場合は MMD を起動してはいけない。
- 既存のユーザー生成 artifact を削除・上書きする前に、対象パスと理由を明示する。

## 推奨ワークフロー

まず `oracle-batch` を使う。PMX/VMD から一時 `scene.pmm` と `fixture.json` を生成し、必要な時だけ MMD で JSONL を dump する。

Dry-run:

```powershell
pnpm -C F:\Develop\MMDDev\MMDDumper oracle-batch -- `
  --manifest <oracle-batch.json> `
  --dry-run true
```

実機 dump:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C F:\Develop\MMDDev\MMDDumper oracle-batch -- `
  --manifest <oracle-batch.json> `
  --timeout-ms 90000
```

生成後の確認:

```powershell
pnpm -C F:\Develop\MMDDev\MMDDumper validate <oracle.actual.jsonl>
pnpm -C F:\Develop\MMDDev\MMDDumper verify-coverage -- --fixture <fixture.json>
```

既存 fixture を直接 record するのは、PMM 内参照が現在の環境で解決できると確認できた場合だけにする。

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C F:\Develop\MMDDev\MMDDumper record -- --fixture <fixture.json>
```

## MMD 標準書き出し

MMD 本体の `画像ファイルに出力` / `AVIファイルに出力` を自動操作する経路がある。これは Direct3D backbuffer capture やスクリーンショットではなく、MMD の通常書き出しメニューを Win32 で叩く。MMD GUI を起動するので、安全ルールと同じくユーザー許可と `MMD_DUMPER_ALLOW_MMD_LAUNCH=1` が必須。

画像出力:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C F:\Develop\MMDDev\MMDDumper export-image -- `
  --fixture <fixture.json> `
  --output out\frame.bmp `
  --frame 30
```

- MMD の `画像ファイルに出力` ダイアログで BMP を保存し、その BMP から同名の PNG も生成する。
- `--crop-content true` を付けると PNG は content crop 版になる。
- MMD 標準書き出しは出力カメラ側を使う。最小生成 PMM など、カメラ/出力構図がない PMM では白画像になることがある。カメラつき PMM で確認する。

AVI 出力:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C F:\Develop\MMDDev\MMDDumper export-avi -- `
  --fixture <fixture.json> `
  --output out\clip.avi `
  --start-frame 0 `
  --end-frame 30 `
  --fps 30 `
  --timeout-ms 120000
```

- MMD の `AVIファイルに出力` と `AVI出力設定` を自動操作する。
- 既定では現在の MMD 出力サイズと codec 設定を使う。確認済みの短尺 smoke では rawvideo AVI になり、31 frames / 1024x768 / 約 1.03 秒で約 97MB になる。
- raw AVI は巨大になりやすい。配布・確認用途では生成後に `ffmpeg` で MP4 や PNG 連番へ変換する。
- 書き出し完了待ちは AVI ファイルサイズの安定で判定している。長尺や遅い codec では `--timeout-ms` を伸ばす。

## Manifest 指針

- PMX/VMD ペアから作れるケースは `oracle-batch` の `cases[]` に寄せる。
- `defaults.outDir` は通常 `out/<purpose>` にする。
- `frames` は目的の比較 frame を明示する。まずは `[0, 30, 60]` のように小さく始める。
- `templatePmm` は、MMD で作った base PMM が必要な場合だけ使う。通常は PMX/VMD 直生成を優先する。
- VMD が参照する bone/morph が PMX に存在しない場合、既定では該当 keyframe を skip し、`filter.skipped*` に記録される。完全一致が必要な検証では `--missing-names strict` を使う。
- PMX/VMD 直生成では VMD property channel は PMM document patch 対象外として落とされ、`filter.droppedUnsupportedChannels.propertyFrames` に記録される。IK ON/OFF の正確な再現が必要な検証では、この差を無視しない。

## よくある失敗と対処

- `MMD_DUMPER_ALLOW_MMD_LAUNCH=1` がない:
  - 実機起動は拒否される。ユーザー許可を確認してから環境変数を設定する。
- `MMDDumper input check failed before launching MMD`:
  - MMD 起動前の preflight で止まっている。表示された `mmdExe`、`project`、`PMM model reference` の実在を確認する。
- MMD の「モデルファイルが見つかりません」ダイアログ:
  - 古い fixture の PMM 内モデルパスが現在の環境に存在しない。PMM を作り直すか、実在 PMX/VMD から `oracle-batch --dry-run true` で新しい fixture を生成する。
- `frame 0` が rest/未評価に見える:
  - MMD ロード直後に未評価寄りの `frame 0` snapshot が出ることがある。現在の runner は `frame 0` を含む fixture で `right -> left -> right...` の pre-roll を行い、native dumper は初期 `frame 0` を skip する。Golden を読む側は `fixture.frames` の target frame に合わせ、JSONL の先頭 record を frame 0 とみなさない。
- `Unsupported VMD channels`:
  - camera/light/self-shadow/property など PMM patch が扱わない channel がある。PMX/VMD 直生成の property frame は落とせるが、camera/light/self-shadow は別経路を検討する。
- 巨大 VMD:
  - `ラビットホール.vmd` のような 20万 keyframe 級は dry-run に時間がかかる。まず dry-run だけ通し、`mismatches=0` と dropped/skipped 情報を確認してから実機 dump する。
- `pnpm test -- <files>`:
  - この package script は全テストを走らせる。個別テストは `node --test test/foo.test.mjs` を使う。

## 検証コマンド

局所変更ではまず対象テストを直接実行する。

```powershell
node --test test/fixture.test.mjs test/oracle-batch.test.mjs test/pmm-from-pmx-vmd.test.mjs
```

依存や fixture が揃っている環境で全体確認する場合:

```powershell
pnpm -C F:\Develop\MMDDev\MMDDumper test
```

native DLL / proxy を触った場合:

```powershell
pnpm -C F:\Develop\MMDDev\MMDDumper native:test
pnpm -C F:\Develop\MMDDev\MMDDumper native:package
```

## Oracle の使い分け

- `MMDDumper` 実機 dump: MMD 9.32 x64 の golden oracle。
- nanoem wrapper dump: 軽量 surrogate。MMDDumper golden と差分検証してから使う。
- loader 自身の runtime dump: regression/smoke 用。golden oracle ではない。

Oracle 生成器を改修するときは、必ず「どの runtime の、どの評価段階の、どの座標系/行列順序の値か」を schema または metadata に残す。
