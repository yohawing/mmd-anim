# MMDPACK texture Phase 0 probe

This is a maintainer-only measurement harness. It does not define the production
`.mmdpack` format or freeze a V1 codec decision.

`run-campaign.ps1` first runs the pinned Three.js `2d_uastc.ktx2` known-good
control, then reads a fixed local manifest and invokes a configured Basis
Universal v2.50 executable for Candidate A (raw UASTC probe payload) and
Candidate B (KTX2 UASTC with internal Zstandard), and loads a configured
Three.js Basis transcoder in Node for the WASM lane. Tool paths and manifest
payloads are intentionally local/ignored; do not vendor the official tool or
reference transcoder.

The run uses one measured iteration per operation. OS/file caches are
uncontrolled; there is no warmup or repeat, so timings are directional. The
script publishes only all-green native/comparability validation results, except
that a pinned-transcoder compatibility rejection is retained as an explicit
blocked Phase 0 result.

Raw JSON and Markdown are replaced atomically per file on the same volume. The
published raw JSON, decision Markdown, and WASM control Markdown are three
separate publications, not a transactional set, so a failure or input drift
between replacements can leave different run IDs in the files; each run records
a unique run ID to make that state explicit. A failed known-good control stops
the campaign before candidate measurement/publication.
