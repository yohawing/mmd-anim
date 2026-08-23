# D3D9 Playback Control Blocker

Evidence type: `smoke regression evidence`

Date: 2026-05-20

Fixture:

- `fixtures/sour-addiction/fixture.json`
- PMM: `F:\Develop\MMDDev\data\pmm\sour_addiction.pmm`

Command:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
pnpm -C MMDDumper mmd:smoke:d3d9-play -- --fixture fixtures/sour-addiction/fixture.json --timeout-ms 60000
```

Result:

- Failed by timeout waiting for `3` records.
- The generated JSONL remained schema-valid.
- Records: `1`.
- Frame: `0`.
- Models: `1`.
- MMD process and temporary DLLs were cleaned up after the run.
- Follow-up run with direct window key posting also timed out:
  - `keySend.attempted`: `true`
  - `keySend.activated`: `true`
  - `PostMessage(WM_KEYDOWN/WM_KEYUP)` for `P`: `down=True`, `up=True`, `vk=80`
  - Result still stayed at `1` record / frame `0`.
- Follow-up run with direct `Right Arrow` frame-advance posting also timed out when no manual input was used:
  - Command used `--send-key right --send-key-repeat 5 --send-key-interval-ms 250 --min-records 2 --min-last-frame 1`.
  - Each key send reported `activated=True`, `down=True`, `up=True`, `vk=39`.
  - Result still stayed at `1` record / frame `0`.
- Later follow-up with `keybd_event` plus window message posting succeeded:
  - `mmd:smoke:d3d9-next-frame` reached frame `60`.
  - `verify-coverage` passed for fixture frames `0`, `30`, and `60`.
  - Each target frame had `319` bones and `131` morphs.

What worked:

- `d3d9.dll` proxy loaded.
- `Direct3DCreate9` was forwarded.
- `CreateDevice` was observed.
- Device vtable patch ran.
- `EndScene` and `Present` callbacks fired repeatedly.
- `MmdOracleDumpFrameChanged` was called repeatedly.

What did not work:

- The runner sent MMD's documented Play/Stop shortcut (`P`), including a follow-up path that posts `WM_KEYDOWN/WM_KEYUP` directly to MMD's main window, but `ExpGetFrameTime()` did not advance from `0`.
- The runner also posted MMD's documented next-frame shortcut (`Right Arrow`) directly to the main window. Without manual keyboard input, `ExpGetFrameTime()` still did not advance from `0`.
- Active desktop use can disturb `SendKeys`, but the direct `PostMessage` follow-up means this blocker is no longer explained only by foreground focus loss.

Resolved path:

- `keybd_event` with `Right Arrow` advances the MMD frame.
- The runner still posts `WM_KEYDOWN/WM_KEYUP` after `keybd_event`, but the observed behavioral difference is that `PostMessage` alone failed while `keybd_event` succeeded.
- This resolves the immediate frame-advance blocker for smoke evidence, but it remains input-driven rather than a true MMD frame seek API.

Reference:

- MMD 9.32 keyboard shortcut reference lists `P` as Play/Stop: https://defkey.com/miku-miku-dance-mmd-9-32-shortcuts
- MMD 9.32 shortcut references list left/right arrows as previous/next frame shortcuts: https://defkey.com/shortcut-actions/go-to-previous-frame-19

Next action:

- Keep investigating explicit frame control instead of relying on global keyboard playback.
- Options to investigate:
  - Find a stable MMD command/menu message for Play and frame seek.
  - Use an MMDPlugin-compatible callback path if it exposes frame control.
  - Add a narrowly scoped MMD 9.32 x64 memory adapter for current frame / playback state after version/hash checks.
