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
