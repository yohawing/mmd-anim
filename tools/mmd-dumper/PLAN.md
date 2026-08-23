
## 2. MMD本家から正解データを作る案

MMD本家の内部状態をdumpして、`three-mmd-loader` の検証用 oracle にする案は有効。

狙う正解データは、FBX exporter 的な「全部出力」ではなく、**検証用 oracle dump** に絞るのがよい。

最初に欲しい値はこれ。

```text
frame
model name
bone name
bone local transform
bone world matrix
morph name
morph weight
```

次段階で欲しい値。

```text
post-IK bone matrix
post-append bone matrix
post-physics bone matrix
rigid body world matrix
```

ただし physics は完全一致を期待しない方がよい。
Bullet / Ammo / timestep / sleep / float error の影響でズレるため、まずは参考値・安定性チェック扱い。

---

## 3. MMDのDLLフックの仕組み

MMD界隈では、内部DLLフック型の拡張が実質的なプラグイン手法として存在する。
FBX exporter のようなものがあるなら、この方向は現実的。

仕組みはこう。

```text
MikuMikuDance.exe に自分のDLLを読み込ませる
↓
MMD本体 or DirectX の関数呼び出しをhookする
↓
毎フレームの処理タイミングで自分の処理を挟む
↓
MMD内部のbone / morph / physics状態を読む
↓
JSONLなどにdumpする
↓
本来の処理へ戻す
```

代表的なDLL読み込み方法。

| 方法                | コメント                       |
| ----------------- | -------------------------- |
| proxy DLL         | MMDが読むDLL名を利用して自作DLLをロードする |
| 既存拡張経由            | MMEや既存プラグイン基盤を足場にする        |
| external injector | 開発用には強いが、配布・安全性の面では慎重に扱う   |

代表的なhook方式。

| 方式                 | 用途                                         |
| ------------------ | ------------------------------------------ |
| IAT hook           | WinAPI / DirectX入口の差し替え                    |
| vtable hook        | Direct3D `Present` / `EndScene` などの毎フレーム処理 |
| inline detour      | MMD内部関数の横取り                                |
| memory offset read | 内部構造体を直接読む                                 |

oracle dumper としては、最初は **MMD 9.32 x64 固定** がよい。

---

## 4. 作るべきものは「MMD 9.32 oracle dumper」

汎用プラグインやFBX exporterではなく、目的を検証に絞る。

推奨構成。

```text
mmd-oracle-dumper/
  native/
    mmd_oracle_dumper.dll
    snapshot_buffer.cpp
    jsonl_writer.cpp
    mmd932_reader.cpp
    mmd932_offsets.h

  schema/
    oracle-v1.schema.json

  tools/
    mmd-oracle-runner.ts
    validate-oracle.ts
    compare-oracle.ts

  fixtures/
    sample01/
      scene.pmm
      fixture.json
      oracle.expected.jsonl
```

出力はまず JSONL。

```json
{"frame":0,"bones":[{"name":"センター","worldMatrix":[1,0,0,0]}],"morphs":[{"name":"まばたき","weight":0}]}
{"frame":30,"bones":[{"name":"センター","worldMatrix":[1,0,0,0]}],"morphs":[{"name":"まばたき","weight":0.5}]}
```

初期は readable / debuggable を優先。
大量化したら binary dump に移行すればよい。

---

## 5. CLI runner化する案

MMDはGUIアプリだが、検証ループを回すにはCLI runner化が重要。

理想の流れ。

```text
CLI runner
  ↓
MMDを起動
  ↓
scene.pmm を開く
  ↓
dumper DLL が有効化される
  ↓
指定frameをdump
  ↓
oracle.actual.jsonl を生成
  ↓
done file を作る
  ↓
runnerがMMDを終了
  ↓
schema validation
  ↓
three-mmd-loader側とcompare
```

最初は PMX/VMD をCLIから直接読み込ませるより、**MMDで事前に保存した `scene.pmm` をfixtureにする** 方が安定。

fixture例。

```json
{
  "name": "sample01-basic-vmd",
  "mmdVersion": "9.32-x64",
  "project": "fixtures/mmd/sample01/scene.pmm",
  "frames": [0, 30, 60],
  "dump": {
    "bones": true,
    "morphs": true,
    "rigidBodies": false
  },
  "output": "fixtures/mmd/sample01/oracle.actual.jsonl",
  "timeoutMs": 60000
}
```

コマンドイメージ。

```powershell
pnpm mmd:oracle:record --fixture fixtures/mmd/sample01/fixture.json
pnpm mmd:oracle:compare --fixture fixtures/mmd/sample01/fixture.json
```

一発実行なら。

```powershell
pnpm mmd:oracle:test --fixture fixtures/mmd/sample01/fixture.json
```

---

## 6. AIに自律的に開発させる方法

完全に「AIがMMD内部を勝手に解析して完成」ではなく、**小さく検証可能なタスクに分けて自律ループを作る**のが現実的。

AIに向いている部分。

```text
- oracle schema設計
- JSONL writer
- snapshot buffer
- fake memory test
- three-mmd-loader側のcompare test
- matrix / quaternion diff
- error report生成
- fixture管理
- CLI runner
```

AIに任せにくい部分。

```text
- MMD内部構造体の特定
- versionごとのoffset調査
- post-IK / post-physics など評価段階の判定
- hook位置の確定
```

なので、開発順はこうするのがよい。

```text
Phase 1:
  MMDなしで oracle schema / compare test を作る

Phase 2:
  C++側の dumper core を fake memory でテストする

Phase 3:
  MMD 9.32 adapter を追加する

Phase 4:
  実MMDで 1モデル + 1モーション をdumpする

Phase 5:
  three-mmd-loader と比較する
```

---

## 7. 安全・制約設計

DLL hookは用途が広すぎるため、実装上は明確に制約する。

```text
- 対象は MikuMikuDance.exe のみに限定
- MMD 9.32 x64 のhash/versionを確認
- read-only dumpに限定
- 外部通信しない
- persistence / stealth / privilege escalation なし
- MMD以外のプロセスにはattachしない
- timeout必須
- crash / error logを残す
```

これは技術的にも重要。
対象と目的を絞ることで、AI開発ループも安定する。

---

## 8. 最初のMVP

最小ゴールはこれ。

```text
Target:
  MMD 9.32 x64

Input:
  scene.pmm

Frames:
  [0, 30, 60]

Dump:
  bone world matrix
  morph weight

Output:
  oracle.actual.jsonl
  oracle.actual.done

Runner:
  MMD起動
  dump完了待ち
  MMD終了
  schema validation

Compare:
  three-mmd-loaderで同じPMX/VMDをevaluate
  bone matrix / morph weightをepsilon比較
```

最初からやらないもの。

```text
- physics
- material dump
- texture dump
- vertex dump
- camera
- multi-model
- full visual equivalence
```

---

## 9. 最終的な検証ループ

目指す形。

```text
pnpm build
↓
pnpm mmd:oracle:record --fixture sample01
↓
pnpm mmd:oracle:compare --fixture sample01
↓
差分を見る
↓
three-mmd-loader runtimeを修正
↓
再実行
```

差分レポートはこういう形式がよい。

```text
frame 60

bone: 左ひざ
  worldMatrix maxAbsDiff: 0.034
  rotationDiffDeg: 2.1

morph: まばたき
  expected: 0.72
  actual:   0.70
  diff:     0.02
```

---

## 結論

作るべきものは、**MMD本家を使った検証用 oracle recorder**。

名前をつけるならこう。

```text
mmd-oracle-dumper
mmd-oracle-runner
three-mmd-loader oracle comparison test
```

最初の到達点はこれ。

```text
MMD 9.32で scene.pmm を開く
↓
frame 0 / 30 / 60 の bone world matrix と morph weight をdump
↓
three-mmd-loaderのruntime結果と比較
```

これができると、`three-mmd-loader` の次の精度改善、特に

```text
- VMD Bezier補間
- morph評価
- IK
- append transform
- bone evaluation order
```

をかなり機械的に詰められるようになります。
