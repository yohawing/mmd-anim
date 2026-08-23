# Sour Addiction PMX Inventory

Evidence type: `fixture inventory evidence`

Date: 2026-05-20

Fixture:

- `fixtures/sour-addiction/fixture.json`
- PMM: `F:\Develop\MMDDev\data\pmm\sour_addiction.pmm`
- PMX observed in runtime dump: `F:\Develop\MMDDev\data\pmx\Sour式初音ミクVer.1.02\Black.pmx`

Command:

```powershell
pnpm -C MMDDumper inspect-pmx -- "F:\Develop\MMDDev\data\pmx\Sour式初音ミクVer.1.02\Black.pmx" --limit 8
```

Result:

- PMX version: `2`
- Model name: `Sour_Miku_Black`
- Bones: `319`
- Morphs: `131`
- First bones include `操作中心`, `全ての親`, `センター`, `グルーブ`, `腰`, `上半身`, `上半身2`, `首`.
- First morphs include `まばたき`, `笑い`, `ウィンク`, `ウィンク右`, `ウィンク２`, `ｳｨﾝｸ２右`, `なごみ`, `はぅ`.

Interpretation:

- The Sour PMX itself has bone and morph inventory.
- The current MMD runtime dump for this PMM still writes `bones: []` and `morphs: []`.
- Therefore the empty runtime bone/morph arrays are a dumper/MMDExport access limitation, not absence of data in the PMX fixture.

Next action:

- Keep `inspect-pmx` as fixture inventory tooling only; do not use PMX static names as a replacement for runtime numeric evidence.
- Add an MMD 9.32 x64 runtime adapter that can read post-evaluation bone matrices and morph weights for PMX models, or find a documented plugin API path that exposes those values.
