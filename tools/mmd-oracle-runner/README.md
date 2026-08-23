# mmd-oracle-runner

Python 3.10 validation foundation for the MMD oracle runner. This slice owns
the stable `case.json` contract and fail-closed preflight only; PMM `prepare`
and MMD `record` execution are planned for later slices.

```powershell
cd tools/mmd-oracle-runner
uv run pytest
uv run mmd-oracle-runner validate --case C:\absolute\case.json
```

The case file requires absolute paths for the PMX, body VMD, and output root.
The optional camera VMD is accepted by `node-mmddumper`; `rust-build-pmm` rejects
camera input until that backend declares support. `multi-model`, `accessory`,
and `property-ik` are explicit unsupported capabilities and fail validation;
the validator never falls back to another backend.

Minimal shape:

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

Successful validation emits one JSON object to stdout and exits `0`. Contract
failures emit one JSON object to stderr and exit `2`.
