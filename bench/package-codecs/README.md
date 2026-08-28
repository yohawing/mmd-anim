# MMDPACK Phase 0 codec probe

This is a standalone maintainer-only measurement harness. It is intentionally
not a workspace member and does not implement the `.mmdpack` container.

## Reproduce

1. Copy `campaign.example.json` to `.ai/mmdpack/campaign.json` and replace the
   example paths with a curated local set of PMX/VMD files. Keep the manifest
   and all raw JSON under `.ai/mmdpack/`; asset bytes and cryptographic keys are
   never copied or written by the harness.
2. Run `run-campaign.ps1` from the repository root (or pass explicit paths).

The script creates a unique ignored `.ai/mmdpack/runs/<run-id>/` directory,
measures Native and WASM exactly once, validates both JSON lanes, and only then
atomically replaces the final raw JSON and Markdown files. A failed campaign
does not publish a new report.

The orchestration guard can be smoke-tested without touching the published
report with `run-campaign.ps1 -ForceCommandFailure`; it must exit nonzero before
any lane or publication step.

The campaign uses Zstandard level 3 followed by AES-256-GCM. Keys and nonces
are generated afresh per encryption call and are never included in JSON or the
Markdown report. Memory/RSS and JS/WASM copy metrics are reported as
unavailable unless separately instrumented.
