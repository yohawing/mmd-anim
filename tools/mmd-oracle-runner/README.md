# mmd-oracle-runner

`mmd-oracle-runner` is a Python 3.10 orchestrator for validating PMX/VMD inputs, staging PMX for MMD, generating PMM through Rust, and validating recorded JSONL. It uses no Node.js, npm, pnpm, or external GoldenOracle data.

Configure the local MMD executable once:

```powershell
$env:MMD_DUMPER_MMD_EXE = 'C:\path\to\MikuMikuDance.exe'
```

The command-line `--mmd-exe` option can override this value for one invocation. The launch guard remains separate:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = '1'
```

Typical preparation commands are:

```powershell
uv run --project tools/mmd-oracle-runner pytest
uv run --project tools/mmd-oracle-runner mmd-oracle-runner validate --case C:/absolute/case.json
uv run --project tools/mmd-oracle-runner mmd-oracle-runner prepare --case C:/absolute/case.json
```

Cases accept only `generatorBackend: "python-rust"`. Rust `mmd-anim-cli build-pmm` reads the PMX/VMD inputs and the Python runner manages generated artifacts and ownership markers. The current preparation backend is limited to body VMD bone and morph tracks. Camera, property, and multi-model preparation fails closed.

Recording requires a prepared case with `recordOptIn: true` and the launch guard enabled:

```powershell
uv run --project tools/mmd-oracle-runner mmd-oracle-runner record --case C:/absolute/case.json
Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH
```

The Python runner manages temporary native DLL installation, MMD process shutdown, and DLL restoration. It validates the JSONL schema and requested-frame coverage before atomically promoting the stable output. Environments without MMD or the native DLLs can still run `validate`, `prepare`, and the Rust/Python test suite.

## Quality report snapshots

The `quality-report` command turns one compact, local snapshot into the only
report artifact intended for Git tracking. It is deterministic: the timestamp
comes from the snapshot and no current clock is read.

```powershell
# Run from the repository root; this is the only report artifact intended for Git.
uv run --project tools/mmd-oracle-runner mmd-oracle-runner quality-report `
  --snapshot C:/local/quality-run/snapshot.json `
  --output QUALITY_REPORT.md
```

The snapshot schema is version `1` and contains `run`, `funnel`, `thresholds`,
`metrics`, `failures`, `features`, `categories`, `worstCases`, and
`rawArtifacts`. Metric distributions require `p50`, `p95`, `p99`, and `max`.
Each distribution must be ordered p50 <= p95 <= p99 <= max, and the threshold
and metric name sets must match exactly.
The funnel is reported as discovered -> selected -> prepared -> recorded ->
compared -> passed. `rawArtifacts.retained` must be `false`, and the generated
report explicitly states that raw PMM, JSONL, and log artifacts are not
retained. Snapshot paths and any per-frame/per-bone data stay local and are
never copied into the Markdown report.

For campaign callers, `record_case(..., retain_failure_artifacts=False)` keeps
the existing record behavior but does not create or retain
`record-failure.zip`; this policy also removes only a prior bundle proven
owned by the current case. Attempt-local JSONL and proxy-log temporary files
are still cleaned through the existing ownership markers. The default remains
`True` for standalone diagnostics.

Campaign orchestration durably persists compact local state before calling
`cleanup_completed_case_run(run_dir)`; the final snapshot is written after all
cases have been processed and cleaned. Cleanup requires both valid
`prepare-result.json` and `record-result.json` ownership markers, examines only
immediate regular files in the exact run directory, and uses an exact artifact
basename allowlist. Foreign files, reparse points, marker path escapes, and
missing markers fail closed; no input PMX/VMD, MMD executable, or external path
is removed.
Prepare failures use the separate `cleanup_prepared_case_run` primitive with
the prepare marker as its sole ownership proof; an unsafe or foreign run stops
the campaign before the next case.

## Sequential campaign

`campaign` processes cases one at a time: prepare, record, numeric compare,
durable local state, and owned cleanup. The campaign does not launch MMD during
validation tests; production recording still requires the existing
`MMD_DUMPER_ALLOW_MMD_LAUNCH=1` guard and `recordOptIn: true`.

### Frozen local asset selection

Real asset paths stay in an ignored or external JSON file. `freeze-selection`
walks the PMX and VMD library without parsing every asset, rejects invalid
magic, camera-only VMDs with no body bone frames, oversized files, and reparse
points, then orders eligible paths by a seeded SHA-256 rank. Only the selected
files are content-hashed. Existing selections are never overwritten.

```powershell
uv run --project tools/mmd-oracle-runner mmd-oracle-runner freeze-selection `
  --pmx-root F:/MMD/pmx `
  --vmd-root F:/MMD/vmd `
  --output F:/local/motion-quality/assets-v1.json `
  --count 128 `
  --seed mmd-anim-motion-quality-2026-v1

uv run --project tools/mmd-oracle-runner mmd-oracle-runner verify-selection `
  --selection F:/local/motion-quality/assets-v1.json
```

`materialize-selection` re-verifies every selected PMX/VMD hash and creates
one case JSON per pair plus a campaign config. Its local template contains
exactly `schemaVersion`, `run`, `compare`, and an absolute `outputRoot`.
The destination directory must not already exist.

```powershell
uv run --project tools/mmd-oracle-runner mmd-oracle-runner materialize-selection `
  --selection F:/local/motion-quality/assets-v1.json `
  --template F:/local/motion-quality/campaign-template.json `
  --output-dir F:/local/motion-quality/campaign-v1
```

The materialized campaign binds `selectionFile` and `selectionHash`; campaign
loading rejects missing, tampered, reordered, or differently tagged cases.
The final Markdown publishes the frozen selection hash without exposing local
asset paths.

```powershell
uv run --project tools/mmd-oracle-runner mmd-oracle-runner campaign `
  --config C:/local/quality/campaign.json `
  --snapshot C:/local/quality/motion-quality.snapshot.json `
  --state C:/local/quality/.motion-quality.campaign-state.json `
  --mmd-exe C:/path/to/MikuMikuDance.exe
```

The config is a strict version-1 JSON object with `selectionFile`,
`selectionHash`, `run`, `discovered`, `compare`, and `cases`. The run versions
are self-reported config labels;
campaign execution requires a clean repository checkout and records the
repository HEAD SHA in compact state. The HEAD is rechecked before each case,
before accepting each case result, and immediately before final snapshot
generation. `compare.thresholds` must contain exactly
`translationMaxError`, `translationRmsError`, `rotationMaxAngleRad`,
`rotationRmsAngleRad`, and `maxAbsError`; the latter also supplies the numeric
manifest epsilon. Metric quantiles use deterministic nearest-rank selection
over per-case values. Compact local state is durably written before each
cleanup attempt; the final snapshot is written after all cases are processed
and cleaned. Keep both local artifacts outside tracked inputs when possible.
Only the generated repository-root `QUALITY_REPORT.md` is intended for Git
tracking.
