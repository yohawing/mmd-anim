# Tda Step D3D9 Smoke

Evidence type: `runtime numeric evidence`

Date: 2026-05-20

Fixture:

- Project: `F:\Develop\MMDDev\data\pmm\tda_step.pmm`
- Fixture config: `fixtures/tda-step/fixture.json`
- Output: `fixtures/tda-step/oracle.actual.jsonl` (ignored generated artifact)

Command:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper mmd:smoke:d3d9-next-frame -- --fixture fixtures/tda-step/fixture.json --timeout-ms 60000 --send-key-repeat 70 --send-key-interval-ms 40 --min-records 3 --min-last-frame 60
pnpm -C MMDDumper mmd:smoke:d3d9-next-frame -- --fixture fixtures/tda-step/fixture.json --timeout-ms 60000 --send-key-repeat 70 --send-key-interval-ms 40 --min-records 3 --min-last-frame 60 --write-done
pnpm -C MMDDumper record -- --fixture fixtures/tda-step/fixture.json
pnpm -C MMDDumper validate -- fixtures/tda-step/oracle.actual.jsonl
pnpm -C MMDDumper verify-coverage -- --fixture fixtures/tda-step/fixture.json
```

Result:

- `mmd:smoke:d3d9-next-frame`: passed.
- `mmd:smoke:d3d9-next-frame --write-done`: passed and wrote `oracle.actual.jsonl.done`.
- `record --fixture fixtures/tda-step/fixture.json`: passed.
- JSONL validation: passed.
- Fixture coverage verification: passed.
- Records after validation: `71`.
- Target frames: `0`, `30`, `60`.
- Model: `F:\Develop\MMDDev\data\pmx\Tda式初音ミクV4X_Ver1.00\Tda式初音ミクV4X_Ver1.00.pmx`
- Target frame model channels: `239` bones and `69` morphs at each target.

Center bone translation samples:

| Frame | Center translation |
| --- | --- |
| `0` | `[0, 8, 0]` |
| `30` | `[-0.0821322426, 7.73081875, -0.113048553]` |
| `60` | `[3.86268115, 7.36649036, 0.331300735]` |

Notes:

- This fixture has stronger early motion than `sour_addiction.pmm` for numeric regression evidence.
- The PMM and PMX binaries remain outside git; only the fixture config and evidence notes are tracked.
