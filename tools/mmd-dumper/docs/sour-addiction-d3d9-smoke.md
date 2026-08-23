# Sour Addiction D3D9 Smoke

Evidence type: `smoke regression evidence`

Date: 2026-05-20

Fixture:

- Project: `F:\Develop\MMDDev\data\pmm\sour_addiction.pmm`
- Fixture config: `fixtures/sour-addiction/fixture.json`
- Output: `fixtures/sour-addiction/oracle.actual.jsonl` (ignored generated artifact)

Command:

```powershell
pnpm -C MMDDumper native:test
pnpm -C MMDDumper native:package
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper mmd:smoke:d3d9 -- --fixture fixtures/sour-addiction/fixture.json --timeout-ms 60000
pnpm -C MMDDumper validate -- fixtures/sour-addiction/oracle.actual.jsonl
pnpm -C MMDDumper mmd:smoke:d3d9-next-frame -- --fixture fixtures/sour-addiction/fixture.json --timeout-ms 60000 --send-key-repeat 70 --send-key-interval-ms 40 --min-records 3 --min-last-frame 60
pnpm -C MMDDumper verify-coverage -- --fixture fixtures/sour-addiction/fixture.json
```

Result:

- `native:test`: passed.
- `native:package`: passed.
- `mmd:smoke:d3d9`: passed.
- `mmd:smoke:d3d9-next-frame`: passed.
- JSONL validation: passed.
- Fixture coverage verification: passed.
- Records: `1`.
- First frame: `0`.
- Follow-up next-frame smoke records: `71`.
- Follow-up next-frame smoke target frames: `0`, `30`, `60`.
- Target frame model channels: `319` bones and `131` morphs at each target.
- Models: `1`.
- MMD folder restore: no temporary `d3d9.dll`, `MSIMG32.dll`, `mmd_oracle_dumper.dll`, or backup file remained after the smoke.

Proxy trace:

```text
d3d9:DllMain:attach
d3d9:Direct3DCreate9
d3d9:load-real:start
d3d9:load-real:ok
d3d9:Direct3DCreate9:ok
d3d9:patch-CreateDevice
d3d9:CreateDevice:ok
d3d9:patch-device
d3d9:sampler:start
d3d9:dump-frame-changed
d3d9:load-dumper:start
d3d9:load-dumper:ok
```

Notes:

- `d3d9.dll` proxy is a stronger MMD-local render-device path than `GradientFill`; it reaches `Direct3DCreate9`, `CreateDevice`, and device patching on this PMM.
- Current sampler captures the first model snapshot with non-empty bone channels, avoiding the earlier too-early frame-0 model-only record.
- `mmd:smoke:d3d9-next-frame` uses `keybd_event` plus window message posting for `Right Arrow`. `PostMessage` alone was not enough to advance MMD's frame, but `keybd_event` produced runtime records through frame `60`.
