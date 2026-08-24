# mmd-oracle-runner

Python 3.10 のオーケストレーターです。PMX/VMD の入力検証、MMD 用 PMX ステージング、Rust の PMM 生成、録画 JSONL の検証を一つのローカル環境で行います。Node.js、npm、pnpm、外部の GoldenOracle データには依存しません。

```powershell
uv run --project tools/mmd-oracle-runner pytest
uv run --project tools/mmd-oracle-runner mmd-oracle-runner validate --case C:/absolute/case.json
uv run --project tools/mmd-oracle-runner mmd-oracle-runner prepare --case C:/absolute/case.json
```

ケースの `generatorBackend` は `python-rust` のみを受け付けます。Rust の `mmd-anim-cli build-pmm` が PMX/VMD を読み込み、Python が生成物と所有権マーカーを管理します。現在の準備バックエンドはボーン・モーフの body VMD に限定されます。カメラ、プロパティ、複数モデルは準備時に fail-closed になります。

録画は準備済みケースに対して明示的なオプトインが必要です。

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
uv run --project tools/mmd-oracle-runner mmd-oracle-runner record `
  --case C:/absolute/case.json `
  --mmd-exe C:/absolute/MikuMikuDance.exe
Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH
```

録画では、ネイティブ DLL の一時配置、MMD プロセスの終了、DLL の復元を Python が管理します。出力は JSONL スキーマとケース指定フレームを検証してから安定ファイルへ原子的に昇格します。MMD 本体や DLL がない環境では `prepare` と Rust/Python のテストだけを実行できます。
