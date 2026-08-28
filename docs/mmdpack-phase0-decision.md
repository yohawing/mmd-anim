# MMDPACK Phase 0 decision

これは、Phase 0の測定結果を次の実装順へ変換する maintainer-only decision memo です。`.mmdpack`のV1形式、公開API、production backend は凍結しません。

## Decided — Phase 0 baseline

- 処理順序は `compress -> AES-256-GCM` とします。復号側は `AES-256-GCM decrypt -> decompress` です。暗号化後の圧縮は採用しません。
- PMX/VMDの圧縮 baseline は Zstandard level `3` とします。圧縮・暗号 benchmark の10ケース（PMX 5件、VMD 5件）で、Native/WASMのround-trip、wrong-key、1-byte tamper、truncation rejectionが全件成功しました。
- TextureのPhase 0候補は KTX2 UASTC + internal Zstandard level `3` とします。Three.js pinned control（6 mip）が成功し、固定10ケースで Native/WASM各 lane の A/B 206 mip が成功しました。Candidate Bは全ケースでCandidate A reconstructed KTX2より小さく、A/Bのraw UASTC mip bytesも一致しました。
- TextureのA/B一致は各lane内の判定です。NativeのDDS payloadとWASMのRGBA32 payloadのcross-lane byte parityは主張しません。

## Provisional — V1へ持ち越す前提

測定に使った `RustCrypto aes-gcm 0.10.3`、`zstd` crate `0.13.3`（libzstd `1.5.7`）、Basis Universal v2.50、Three.js transcoderは、再現用の測定構成であって既定のproduction backendではありません。RustCryptoまたは`zstd` crateをV1の既定実装と断定しません。

KTX2候補は標準コンテナ、既存loaderとの相互運用、custom metadata/reconstructionが不要という点で優位です。ただし、Codec ID、metadata、UASTC quality/RDO、色空間、alpha、orientation、mipmap policy、fallback targetは未凍結です。Candidate Aは比較用の保守的probe payloadであり、将来の最小raw形式のサイズ見積もりではありません。

## Blocked — まだ決められない事項

- AES-GCMとZstandardのNative/WASM production backend。現状は各1構成のsingle-run測定で、代替backend比較とbrowser host証拠がありません。
- `first render`、decrypt/decompress/transcode、GPU uploadの基準値と、peak JS heap、WASM linear memory、Native RSS、largest allocation、JS↔WASM copyのHard Gate。現行benchmarkはsingle measured iterationで、OS/file cacheは制御外です。memoryとcopyの有効な値は取得していません。
- V1のTexture Codec ID、metadata schema、UASTC encoder/transcoder version・quality、通常Texture／toon／normalのmipmap規則、GPU fallback範囲。
- V1 packageのheader、Manifest、Entry compression、nonce/AADのnormative profileと、Native/WASM/C ABI共通のgolden vector。
- MSRV 1.87の実ビルド。現測定hostはRust 1.98.0です。Textureのsource PNGに対する絶対品質metricも対象外で未実施です。

## 選定根拠

### 圧縮・暗号

`docs/mmdpack-benchmark.md`は `input -> Zstandard -> AES-256-GCM` を実測し、全ケースでbyte-identical round-tripと認証失敗境界を確認しています。Nativeのdirectional throughputは約87–623 MiB/s、WASM/Nodeは約6–204 MiB/sでしたが、single iterationのためbackend選定やHard Gateの合否には使いません。鍵とnonceは各暗号化呼び出しで新規生成され、reportへ出力されていません。

### Texture

`docs/mmdpack-texture-decision.md`と`docs/mmdpack-texture-wasm-control.md`は、Three.js pinned controlを含む同一transcoder pathを記録しています。Candidate Bは標準KTX2として読みやすく、固定10ケースの全mipでA/Bのraw UASTC bytesとlane内decoded hashesが一致し、reconstructed Candidate Aより小さくなりました。一方、source画像の絶対品質、cross-lane decoded byte parity、peak memoryは証明していません。

## Backend候補と最小A/B測定

| 領域 | 測定済みbaseline | 次に比較する候補 | 必須の判定 |
|---|---|---|---|
| AES-256-GCM Native | 測定済みRustCrypto `aes-gcm 0.10.3` | `ring 0.17.14`の`AES_256_GCM` | 同一wire/AAD/nonce/tag、plaintext一致、認証失敗境界、decrypt throughput、buffer/copy観測 |
| AES-256-GCM Browser | 測定済みRustCrypto WASM | Web Crypto `SubtleCrypto AES-GCM` | 同一wire/AAD/nonce/tag、plaintext一致、認証失敗境界、browser throughput、buffer/copy観測 |
| Zstandard Native/Browser | `zstd` crate `0.13.3`／libzstd `1.5.7`、level 3 | pure-Rust `ruzstd 0.8.3`（local Cargo registryで確認。実装task開始時に依存とversionを固定） | 同じ圧縮frameのdecompressed bytes一致、window/limit、decompress throughput、buffer寿命 |
| Texture | KTX2 UASTC + internal Zstd 3 | KTX2 standard pathを維持したtranscoder/backend候補 | 全mip transcode、dimensions/hash、first-render、fallback、実装・binary size |

次のbackend A/Bは、同じ10–20件のPMX/VMD/Texture manifest、同じ暗号化package bytes、同じ固定profileで実施します。AES NativeはRustCryptoと`ring`、BrowserはRustCrypto WASMとWeb Cryptoを比較します。Zstandardは同じlibzstdでencodeした固定compressed frameをdecoder A/Bへ入力し、別encoderの圧縮出力比較と混ぜません。Windows x64 Native、macOS arm64 Native、Browser/WASMを分けて測り、cold/warmを区別し、single-runではなく複数反復の分布と失敗数を保存します。候補間で結果bytes、認証境界、エラー分類が一致しない場合は速度より互換性を優先してblockedとします。Windows固有のCNGはcross-platformのprimary候補にしません。

## Performance/Memory gateを確定する最小測定

1. Reference setはroadmapの最低条件を満たします。小規模PMX + 1K Texture、標準PMX + 2K Texture、大規模PMX + 4K Texture、alpha、toon ramp、normal map、大量Morph、長尺VMD、複数VMD、100 MiB超Packageを含めます。
2. Windows x64 Native、macOS arm64 Native、Chrome、Firefox、SafariのBrowser/WASMを対象にし、WebGL2とWebGPUの各pathを分けます。cold start／warm startで `header parse`、key request、Manifest decrypt、Entry decrypt、Zstd decompress、Texture transcode、GPU upload、first renderを分離記録します。未利用のhostまたはWebGL2/WebGPU pathはgateをblockedとします。
3. 同一入力のPackageなしbaselineと比較し、peak JS heap、peak WASM memory、Native RSS、largest allocation、package-layer live bytes、JS↔WASM copy回数／byte数、復号後bufferの寿命、同時Texture transcode数を取得します。必須metricが取得できない場合は`unavailable`のまま除外せず、そのhost/metricのHard Gateをblockedとします。代替観測を用意し、全必須値が取得できてからHard Gateを確定します。
4. roadmapの暫定目標（Native 750 MiB/s、Browser 250 MiB/s、100 MiB decrypt+decompress 150/400 ms、Header+Manifest 2/5 ms、package first-render overhead `max(50 ms, baseline * 20%)`）を候補値として再測定し、Reference set全体の分布と失敗を見て最終値を決めます。現行single-run結果は候補値の合否判定に使いません。
5. `loadPackage()`と`openPackage()`を分け、load後に暗号化Package全体を保持しないこと、100 MiB超PackageをBrowserで扱えること、全Textureを無制限に並列transcodeしないことを確認します。

## V1 golden vector prerequisites

次の4項目を完了してからvectorを生成します。

- AES/Zstd backend A/BとBrowser測定を終え、backend差替えでwire formatが変わらないことを確認する。
- KTX2 Codec ID、metadata、UASTC profile、mipmap／color／alpha／orientation policy、Zstd profileをnormative profileとして凍結する。
- Fixed Header、Manifest、Entry layout、`package_id` byte order、nonce、AAD、ciphertext/tag、decoded size、limitsを凍結する。
- Native、WASM、C ABIで同じvectorを読み、正常系に加えてwrong key、tamper、truncation、malformed manifest、range/size overflowのfailure vectorを確認する。秘密鍵は共有Markdownへ出しません。

## 次のdependency-ordered tasks

1. **Backend A/B** — AES-GCMとZstandardの代替backendを同一wire profile・同一reference setで比較し、暫定backendを選ぶ。
2. **Browser first-render / memory** — 選定候補をBrowserで測り、first-render、peak memory、copy、load/openのHard Gateを確定する。
3. **Normative profile freeze** — Texture Codec ID/metadata、UASTC quality、mipmap、fallback、AES/Zstd profileとcontainer metadataを凍結する。
4. **V1 golden vector** — 凍結したprofileで正常系とfailure vectorを生成し、Native/WASM/C ABI parityを固定する。

## 根拠資料

- `docs/roadmap/mmdpack.md`（0.2 Draft、V1未凍結）
- `docs/mmdpack-benchmark.md`（`90fafe0`、圧縮・認証付き暗号化測定）
- `docs/mmdpack-texture-decision.md`、`docs/mmdpack-texture-wasm-control.md`（`63c720d`、`22c9c6e`、Texture A/BとThree.js control）

この文書はPhase 0の意思決定を記録します。production実装、公開API変更、roadmap本文の更新は次タスク以降に行います。
