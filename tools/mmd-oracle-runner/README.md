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

Cases accept only `generatorBackend: "python-rust"`. Rust `mmd-anim-cli build-pmm` reads the PMX/VMD inputs and the Python runner manages generated artifacts and ownership markers. The current preparation backend applies body VMD bone and morph tracks, reports and drops property frames, and fails closed for camera and multi-model preparation.

Recording requires a prepared case with `recordOptIn: true` and the launch guard enabled:

```powershell
$env:MMD_DUMPER_ALLOW_MMD_LAUNCH = '1'
try {
  uv run --project tools/mmd-oracle-runner mmd-oracle-runner record --case C:/absolute/case.json
} finally {
  Remove-Item Env:MMD_DUMPER_ALLOW_MMD_LAUNCH -ErrorAction SilentlyContinue
}
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

The snapshot is a version-`1` aggregate object. The report loader checks the
required aggregate fields and basic value types, then formats the same input
deterministically. It does not expose snapshot paths or per-frame/per-bone
data in Markdown. The campaign policy is to keep PMM, JSONL, and log artifacts
local; only the generated Markdown is tracked.

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

### Fixed local asset manifest

Keep one ignored or external JSON manifest as the selection authority. It
directly lists each `caseId`, `pmx`, `bodyVmd`, and optional per-case metadata;
asset values may be strings or `{ "path": ... }` objects with an optional
fixed `sha256`. Top-level `frames` and `outputRoot` are shared defaults. The
optional `run`, `discovered`, and `compare.thresholds` entries override
maintainer defaults. There is no repository command for discovering, randomly
selecting, or materializing assets. The campaign hashes the manifest for state
drift and verifies asset content where the fixed list supplies `sha256`.

```powershell
uv run --project tools/mmd-oracle-runner mmd-oracle-runner campaign `
  --config C:/local/quality/campaign.json `
  --snapshot C:/local/quality/motion-quality.snapshot.json `
  --state C:/local/quality/.motion-quality.campaign-state.json `
  --mmd-exe C:/path/to/MikuMikuDance.exe
```

The manifest is a pragmatic version-1 object with `run`, `discovered`,
`compare`, and direct `cases`. The run versions are self-reported config
labels; campaign execution requires a clean repository checkout and records
the repository HEAD SHA in compact state. `compare.thresholds` contains
`translationMaxError`, `translationRmsError`, `rotationMaxAngleRad`,
`rotationRmsAngleRad`, and `maxAbsError`; the latter also supplies the numeric
manifest epsilon. Metric quantiles use deterministic nearest-rank selection
over per-case values. A partially completed case is conservatively cleaned and
rerun on the next invocation. Keep the manifest, state, snapshot, and raw
artifacts outside tracked inputs; only the generated repository-root
`QUALITY_REPORT.md` is intended for Git tracking.
