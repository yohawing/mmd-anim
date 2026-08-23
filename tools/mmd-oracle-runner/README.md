# mmd-oracle-runner

Python 3.10 orchestration for validating, preparing, and recording MMD oracle
cases. Backend selection is explicit and never falls back to another generator.

```powershell
cd tools/mmd-oracle-runner
uv run pytest
uv run mmd-oracle-runner validate --case C:\absolute\case.json
uv run mmd-oracle-runner prepare --case C:\absolute\case.json
uv run mmd-oracle-runner prepare-batch --case C:\absolute\body.json --case C:\absolute\camera.json
```

The case file requires absolute paths for the PMX, body VMD, and output root.
`name` is a display label; artifacts use a contained, Windows-safe derived name.
`node-mmddumper` accepts an optional camera VMD and verifies generated PMM body
and camera keyframes before recording. `rust-build-pmm` is body-only and reports
its direct PMM/VMD comparison as `not-verified`. `multi-model` and `accessory`
remain unsupported. `property-ik` requires an explicit `node-mmddumper` feature
opt-in and any dropped property frames remain visible in the result.

Minimal case:

```json
{
  "schemaVersion": 1,
  "name": "body-only",
  "input": {
    "pmx": "C:/absolute/model.pmx",
    "bodyVmd": "C:/absolute/body.vmd"
  },
  "frames": [0, 15, 30],
  "outputRoot": "C:/absolute/output",
  "generatorBackend": "node-mmddumper",
  "recordOptIn": false,
  "dialogOptIn": false
}
```

`prepare` never launches MMD. It writes `scene.pmm`, `fixture.json`, and
`prepare-result.json` under `<outputRoot>/<artifactName>/`. To record, first set
`recordOptIn` to `true`, then supply the separate process-level launch gate:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = "1"
uv run mmd-oracle-runner record --case C:\absolute\case.json --mmd-exe C:\absolute\MikuMikuDance.exe
uv run mmd-oracle-runner record-batch --case C:\absolute\body.json --case C:\absolute\camera.json --mmd-exe C:\absolute\MikuMikuDance.exe
Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH
```

Model-structure dialog automation is also fail-closed and requires
`dialogOptIn: true` in that case. Record output is schema- and frame-coverage
checked before atomically replacing the prior stable output. Failures keep the
prior stable output and write owned diagnostics to `record-failure.zip`; a later
success does not erase that failure evidence. Batch commands process every case,
emit one JSON summary, and exit `1` when any case fails.
