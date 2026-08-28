# MMDPACK browser WebGL2 harness

固定した Candidate B KTX2 10件について、平文EntryとAES-256-GCM暗号化Entryを実Chromeで比較する保守者向けハーネスです。公式の固定Three.js KTX2Loaderを通し、テクスチャ付きquadの描画から`gl.finish()`までを測ります。readbackとhashは計測外の妥当性検証です。

これはChrome/WebGL2のtexture-entry検証です。`.mmdpack`全体のopen、PMX/VMD parse、MMDの初回描画、compositor表示、WebGPUを証明するものではありません。

## 実行

まず自己テストを通します。

```powershell
node bench/package-browser/self-test.mjs
```

次にサーバーを起動し、表示されたloopback URLを通常のChromeで開いて **Run** を押します。

```powershell
node bench/package-browser/server.mjs
```

入力は `.ai/mmdpack/textures/latest.json` とそこから参照される固定10件です。サーバーはloopback専用で、明示的に許可したno-store routeだけを公開します。

各レーンはwarmup 1回、計測5回です。順序効果を抑えるため平文先行と暗号化先行を交互にし、レーン別p50/p95に加えて同一反復内の暗号化Entryマイナス平文Entryも記録します。AES復号、KTX2変換、upload/render/`gl.finish()`は別stageです。検証用plaintext copy、SHA-256、readback hashは計測区間から除外し、全反復の内容同値を確認してからtimingを採用します。

結果は厳格なschema、provenance、10件のcoverage、両レーン同値、GPU readback、暗号失敗境界をサーバー側で検証してから保存します。

- 判断文書: `docs/mmdpack-browser-webgl2-decision.md`（ローカルignored）
- raw result: `.ai/mmdpack/browser/runs/<browser-run-id>/report.json`（ローカルignored）

鍵はWeb Cryptoへ `extractable: false` / decrypt-onlyでimportし、raw bytesは直後にbest-effortでゼロ化します。鍵、nonce、AAD、ciphertextは成果物へ保存しません。

Chromeから直接取得できるheap値は方向性のあるsnapshotに過ぎません。true peak、Basis WASM linear memory、物理copy回数、GPU常駐memory、compositor latencyはこのハーネスではunavailableとして明示します。
