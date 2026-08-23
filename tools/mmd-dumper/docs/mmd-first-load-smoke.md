# MMD First-Load Smoke

Evidence type: `smoke regression evidence`

Date: 2026-05-20

Command:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
pnpm -C MMDDumper mmd:smoke:first-load -- --fixture fixtures/sample-basic/fixture.json --timeout-ms 20000
pnpm -C MMDDumper validate -- fixtures/sample-basic/oracle.actual.jsonl
```

Result:

- `native:test`: passed.
- `native:package`: passed.
- `mmd:smoke:first-load`: passed.
- Output: `fixtures/sample-basic/oracle.actual.jsonl` (ignored generated artifact).
- Records: `1`.
- First frame: `0`.
- Models: `3`.
- `validate`: passed.
- MMD folder restore: no `MSIMG32.dll`, `mmd_oracle_dumper.dll`, or backup file remained in `MikuMikuDance_v932x64/` after the smoke.

Notes:

- This proves the MMD-local `MSIMG32.dll` proxy can load `mmd_oracle_dumper.dll` under MMD 9.32 x64 and write schema-valid JSONL.
- CP932 strings from `MMDExport` are converted to UTF-8 before JSONL writing. The smoke output includes readable Japanese names such as `初音ミクmetal.pmd`, `センター`, and `まばたき`.
- This does not yet prove frame-by-frame capture. That still needs a render-timing hook or an explicit MMDPlugin callback path.
