# MMDDumper

`MMDDumper`は、MikuMikuDance 9.32 x64で評価したボーンとモーフの状態をJSONLとして保存する、リポジトリ内のネイティブダンパーです。PMXとbody VMDからPMMを生成し、指定したフレームをMMDで再生して記録できます。実行時にNode.js、npm、pnpmは必要ありません。

## 前提条件

準備だけならRustとPython 3.10が必要です。ネイティブバイナリのビルドには、対象環境のCMakeとC++コンパイラも必要です。

MMDを起動する記録処理は、Windowsのデスクトップセッションで実行します。MMD本体、PMX、VMDなどのアセットはリポジトリへ追加せず、ケースファイルから絶対パスで指定します。

## ネイティブバイナリをビルドする

リポジトリのルートで次を実行します。

```powershell
python MMDDumper/scripts/native_smoke.py
python MMDDumper/scripts/package_native.py
```

ビルドとパッケージの出力先は`MMDDumper/out/`です。このディレクトリはGitの管理対象外です。

`MMDDumper/lib/mmd/MMDExport.lib`がある場合は、MMD向けダンパーDLLもビルドされます。SDKのライブラリやMMD本体は配布物に含まれません。

## ケースを作成する

ケースは、入力アセット、評価フレーム、出力先をまとめたJSONファイルです。次の例は、現在のディレクトリにある`model.pmx`と`motion.vmd`からケースを作成します。別の場所にあるアセットを使う場合は、`Resolve-Path`の引数を置き換えます。

```powershell
$Pmx = (Resolve-Path .\model.pmx).Path
$BodyVmd = (Resolve-Path .\motion.vmd).Path
$OutputRoot = (Join-Path $PWD 'mmd-dumper-output')

@{
  schemaVersion = 1
  name = 'body-only'
  input = @{
    pmx = $Pmx
    bodyVmd = $BodyVmd
  }
  frames = @(0, 15, 30)
  outputRoot = $OutputRoot
  generatorBackend = 'python-rust'
  recordOptIn = $true
  dialogOptIn = $false
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath .\case.json -Encoding utf8
```

`frames`には重複しない0以上のフレーム番号を指定します。`outputRoot`は既存のファイルではなく、ケースごとの出力ディレクトリを指定してください。

## PMMを準備する

Python runnerが入力を検証し、PMXをMMD互換のUTF-16LEテキストへ必要に応じて変換してから、`mmd-anim-cli build-pmm`を呼び出します。PMMのバイナリ生成は`mmd-anim-format`のRust実装が担当し、MMDは起動しません。

```powershell
$CasePath = (Resolve-Path .\case.json).Path

uv run --project tools/mmd-oracle-runner mmd-oracle-runner validate --case $CasePath
uv run --project tools/mmd-oracle-runner mmd-oracle-runner prepare --case $CasePath
```

成功すると、`outputRoot/<ケース名>/`に`scene.pmm`、`fixture.json`、`prepare-result.json`が作成されます。PMMには、MMDがモデルを再読込するためのPMX参照パスが保存されます。

現在の`python-rust`バックエンドが扱うのは、1モデルのbody VMDに含まれるボーンとモーフです。カメラ、ライト、セルフシャドウ、プロパティ、複数モデル、アクセサリを含むケースは準備時に拒否します。

## MMDで記録する

記録は二重の明示的オプトインです。ケースの`recordOptIn`を`true`にし、起動直前だけ環境変数を設定します。

```powershell
$CasePath = (Resolve-Path .\case.json).Path
$MmdExe = (Resolve-Path .\MikuMikuDance.exe).Path

$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = '1'
try {
  uv run --project tools/mmd-oracle-runner mmd-oracle-runner record `
    --case $CasePath `
    --mmd-exe $MmdExe
} finally {
  Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH -ErrorAction SilentlyContinue
}
```

記録中はMMDを起動し、指定フレームへ進めます。終了時にはMMDプロセスを停止し、使用したネイティブDLLを復元します。成功したJSONLは`outputRoot/<ケース名>/oracle.actual.jsonl`へ原子的に昇格され、`oracle.actual.jsonl.done`と`record-result.json`で検証結果を確認できます。記録に失敗した場合は、前回の成功結果を上書きせず、診断用の`record-failure.zip`を残します。

## テスト

MMDを起動しないテストは、リポジトリのルートで実行できます。

```powershell
python -m pytest tools/mmd-oracle-runner -q
cargo test -p mmd-anim-cli
```

ネイティブビルドを変更した場合は、`native_smoke.py`と`package_native.py`も実行して生成物を確認します。

## 制約と安全策

- MMD本体、モデル、モーション、SDKライブラリはリポジトリに含めません。`MMDDumper/out/`とローカルアセットはGit ignore対象です。
- MMDの起動は`MMD_DUMPER_ALLOW_MMD_LAUNCH=1`と`recordOptIn: true`の両方が必要です。
- 出力先はケースの`outputRoot`配下に限定され、入力アセットやMMD本体を上書きしないよう検証します。
- これはMMDで記録した状態を保存するダンパーです。実機モーションとの自動Parity判定や物理演算の正しさを保証する機能は含みません。
