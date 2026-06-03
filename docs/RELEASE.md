# Release Runbook

This is the maintainer runbook for the first experimental `mmd-anim` release.

## Scope

- Release version: `0.1.0`
- Publish set:
  - `mmd-anim-schema`
  - `mmd-anim-runtime`
  - `mmd-anim-format`
  - `mmd-anim-cli`
- Workspace-only for `0.1.0`:
  - `mmd-anim-ffi`
  - `mmd-anim-wasm`

## Local Checks

Run from the repository root:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
rtk cargo doc --workspace --no-deps
```

Run package list audits:

```powershell
rtk cargo package -p mmd-anim-schema --list
rtk cargo package -p mmd-anim-runtime --list
rtk cargo package -p mmd-anim-format --list
rtk cargo package -p mmd-anim-cli --list
```

Confirm package lists do not include local corpora, generated reports, `target`, `.opencode`,
agent workspaces, root `scripts`, or generated WASM harness output.

## Dry Run

Publish dry runs must follow dependency order:

```powershell
rtk cargo publish -p mmd-anim-schema --dry-run
rtk cargo publish -p mmd-anim-runtime --dry-run
rtk cargo publish -p mmd-anim-format --dry-run
rtk cargo publish -p mmd-anim-cli --dry-run
```

## GitHub Setup

Before public release:

1. Create `yohawing/mmd-anim`.
2. Add `origin` to this checkout.
3. Push `main`.
4. Confirm CI is green.
5. Create tag `v0.1.0` only after dry runs pass.

## Publish

Publish manually for the first release:

```powershell
rtk cargo publish -p mmd-anim-schema
rtk cargo publish -p mmd-anim-runtime
rtk cargo publish -p mmd-anim-format
rtk cargo publish -p mmd-anim-cli
```

## Post-Publish Verification

```powershell
rtk cargo install mmd-anim-cli --version 0.1.0
mmd-anim --version
```

Verify the crates.io pages show the expected repository, license, README, categories, and
experimental status.

## Rollback Notes

Crates.io packages cannot be deleted after publication. If a bad package is published:

1. Yank the affected version with `cargo yank`.
2. Publish a fixed patch release if needed.
3. Document the issue in `CHANGELOG.md`.
