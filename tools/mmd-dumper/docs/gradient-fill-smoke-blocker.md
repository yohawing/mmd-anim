# GradientFill Callback Smoke Blocker

Evidence type: `smoke regression evidence`

Date: 2026-05-20

Command:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
pnpm -C MMDDumper mmd:smoke:gradient-fill -- --fixture fixtures/sample-basic/fixture.json --timeout-ms 20000
```

Result:

- `native:test`: passed.
- `native:package`: passed.
- `mmd:smoke:gradient-fill`: failed by timeout.
- MMD folder restore: no temporary `MSIMG32.dll`, `mmd_oracle_dumper.dll`, or backup file remained after the timeout.

Interpretation:

- `dumpbin /imports MikuMikuDance.exe` shows `MSIMG32.dll` is imported and `GradientFill` is the imported function.
- The proxy builds and forwards `GradientFill`, and can call `MmdOracleDumpFrameChanged` when `MMD_ORACLE_DUMP_ON_GRADIENTFILL=1`.
- MMD 9.32 x64 did not call the `GradientFill` path during the startup fixture smoke within 20 seconds, so this is not a reliable frame sampling hook.

Next action:

- Keep the `GradientFill` hook as a harmless optional diagnostic path.
- Implement a D3D9 render-timing hook (`Direct3DCreate9` proxy with `IDirect3DDevice9::EndScene` or `Present` wrapper), or use a known MMDPlugin callback path, for frame-by-frame oracle capture.
