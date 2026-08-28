# MMDPACK Phase 0 backend probe

This is a maintainer-only, standalone measurement harness. It is not a production
`.mmdpack` container and is intentionally outside the root Cargo workspace.

`run-campaign.ps1` reads the existing ignored 10-case PMX/VMD campaign, encodes
each source once with `zstd` 0.13.3/libzstd 1.5.7 at level 3, then compares two
AES-GCM implementations and two decoders against those same bytes. Native AES
uses RustCrypto `aes-gcm` 0.10.3 versus `ring` 0.17.14. WASM uses the same
RustCrypto code versus Node.js Web Crypto SubtleCrypto; the latter is a Node
stand-in, not browser evidence. Zstd uses the single libzstd frame for both
Native and WASM decoder A/B.

Campaign runs generate a fresh run-scoped 32-byte key in an ephemeral process
environment value; it is never written to raw JSON or Markdown. The fixed key,
nonce, and AAD are a one-time public conformance vector only and are excluded
from performance campaign calls. Each warmup/repeat encrypt call uses a unique
backend/domain/iteration nonce; case-derived nonce/AAD and all keys must not be
used in production. Every frame is checked before decoding for one declared-size
frame, no dictionary/checksum, a 64 MiB window limit, and a 128 MiB decoded-size
limit. Wrong-key, wrong-AAD, tamper, truncation, and size-boundary checks are mandatory.
The Native ring lane uses `Nonce::assume_unique_for_key` only under this
per-encryption unique-nonce campaign contract.

Timing uses one warmup and five measured iterations. OS/file caches are
uncontrolled; p50/p95 and throughput are directional. RSS, JS heap, WASM linear
memory, largest allocation, and JS↔WASM copy counts are unavailable rather than
invented. Native file input and Node source/frame read calls are recorded.

Run from the repository root with PowerShell:

```powershell
& .\bench\package-backends\run-campaign.ps1
```

The script validates all input/source hashes before build, after rendering, and
immediately before publication. Native JSON, WASM JSON, and Markdown are each
file-atomic publications; they are not a transactional set. Intermediate run
directories and raw data stay under ignored `.ai/mmdpack/backends/`.

For wasm-pack's C toolchain, the script preserves an existing
`CC_wasm32_unknown_unknown`/`AR_wasm32_unknown_unknown`, then resolves `clang`
or `llvm-ar` from PATH, and finally checks `%ProgramFiles%\LLVM\bin` without
assuming a fixed drive or installation path. The WASM report records the JS
wrapper and `_bg.wasm` as separate hashes and sizes.

The decision report remains blocked until macOS arm64 Native, real Chrome/
Firefox/Safari browser evidence, first-render paths, and reliable memory/copy
hard-gate metrics are measured by a later task.
