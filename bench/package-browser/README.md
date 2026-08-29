# MMDPACK browser harness

固定した Candidate B KTX2 10件について、平文EntryとAES-256-GCM暗号化Entryを実ブラウザで比較する保守者向けハーネスです。公式の固定Three.js KTX2Loaderを通し、テクスチャ付きquadのGPU完了までを測ります。readbackとhashは計測外の妥当性検証です。

WebGL2はloopback URLの `/`、WebGPUは `/webgpu` で独立して実行します。どちらも`.mmdpack`全体のopen、PMX/VMD parse、MMDの初回描画、compositor表示を証明するものではありません。

## 実行

まず自己テストを通します。

```powershell
node bench/package-browser/self-test.mjs
```

次にサーバーを起動し、表示されたloopback URLを通常のChromeで開いて **Run** を押します。

```powershell
node bench/package-browser/server.mjs
```

WebGPUを正式測定する場合は、外部Chromeであることをサーバー側でも固定して起動し、同じURLの `/webgpu` を開いて **Run** を押します。

```powershell
$env:MMDPACK_BROWSER_AUTHORITY = 'external_chrome_extension'
node bench/package-browser/server.mjs
```

このopt-inなしのWebGPUページは診断専用で、判断文書を公開しません。WebGPUページは実adapter/deviceを要求し、Three.WebGPURendererへ同じdeviceを渡して`await renderer.init()`します。Three.js内部のWebGL fallbackも初期化前に無効化します。adapter情報、features、主要limitsも成果物へ記録します。

Zen BrowserをFirefox系の先行診断として測る場合は、専用authorityで起動して `/zen` を開きます。成果物は明示的にdiagnosticとし、公式Firefoxの合格証拠には使いません。

```powershell
$env:MMDPACK_BROWSER_AUTHORITY = 'zen_browser'
node bench/package-browser/server.mjs
```

入力は `.ai/mmdpack/textures/latest.json` とそこから参照される固定10件です。サーバーはloopback専用で、明示的に許可したno-store routeだけを公開します。

各レーンはwarmup 1回、計測5回です。順序効果を抑えるため平文先行と暗号化先行を交互にし、レーン別p50/p95に加えて同一反復内の暗号化Entryマイナス平文Entryも記録します。AES復号、KTX2変換、upload/render/backend別GPU完了は別stageです。検証用plaintext copy、SHA-256、readback hashは計測区間から除外し、全反復の内容同値を確認してからtimingを採用します。

WebGPUの計測区間はrender targetへの描画後、同じdeviceの`queue.onSubmittedWorkDone()`完了までです。16x16 RenderTargetのreadback/hashはその後に実行します。WebGL2の計測区間は描画後の`gl.finish()`完了までです。

結果は厳格なschema、provenance、10件のcoverage、両レーン同値、GPU readback、暗号失敗境界をサーバー側で検証してから保存します。

- 判断文書: `docs/mmdpack-browser-webgl2-decision.md`（ローカルignored）
- WebGPU判断文書: `docs/mmdpack-browser-webgpu-decision.md`（ローカルignored）
- Zen診断文書: `docs/mmdpack-browser-zen-webgl2-diagnostic.md`（ローカルignored）
- raw result: `.ai/mmdpack/browser/runs/<browser-run-id>/report.json`（WebGL2、ローカルignored）
- WebGPU raw result: `.ai/mmdpack/browser/webgpu/runs/<browser-run-id>/report.json`（ローカルignored）
- Zen raw result: `.ai/mmdpack/browser/zen-webgl2/runs/<browser-run-id>/report.json`（ローカルignored）

鍵はWeb Cryptoへ `extractable: false` / decrypt-onlyでimportし、raw bytesは直後にbest-effortでゼロ化します。鍵、nonce、AAD、ciphertextは成果物へ保存しません。

Chromeから直接取得できるheap値は方向性のあるsnapshotに過ぎません。true peak、Basis WASM linear memory、物理copy回数、GPU常駐memory、compositor latencyはこのハーネスではunavailableとして明示します。
