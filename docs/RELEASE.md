# Release

This checklist is the operator-facing release procedure for `mmd-anim`.

Use this for every release. Replace `X.Y.Z` with the target version and run the
publish steps in dependency order.

## crates.io Setup

For local publishing, log in with a crates.io API token:

```powershell
rtk cargo login
```

For GitHub Actions publishing, create a repository secret named
`CARGO_REGISTRY_TOKEN` with a crates.io token that can publish these crates:

- `mmd-anim-runtime`
- `mmd-anim-format`
- `mmd-anim`

The release workflow publishes on `vX.Y.Z` tag pushes after release checks pass.
For the first release, it publishes in dependency order and waits for each crate
to become visible in the crates.io index before publishing the next dependent
crate.

## Branch And Release Flow

- `develop` is the main development branch.
- `main` is the release-management branch. It should represent reviewed release
  state, not day-to-day development.
- Release preparation is done from `develop` or a short-lived release branch,
  then merged into `main` through a pull request.
- Release tags are created from `main` after the release pull request is merged
  and the release checks pass.
- Do not create release tags from `develop` or from local-only release-prep
  branches.

## 1. Preflight

- Confirm the target version and release scope.
- Confirm `CHANGELOG.md` has an entry for the target version.
- Build the `CHANGELOG.md` entry from the commits since the previous release tag,
  not only from the final release-prep commit.
- Confirm all workspace package versions match the intended release.
- Confirm `README.md` and `docs/README.ja.md` describe the same release-facing
  behavior, API examples, limitations, and publish scope. When one README is
  changed, translate the same user-visible change into the other before
  release.
- If parser APIs changed in `mmd-anim-format`, `mmd-anim-ffi`, or
  `mmd-anim-wasm`, confirm `docs/PARSER_API.md` Parser API Matrix was updated
  with the C ABI/WASM mapping, output shape, error behavior, and lifetime/free
  policy.
- Confirm the working tree is clean.
- Confirm `origin` points to `git@github.com:yohawing/mmd-anim.git`.
- Confirm `develop` is up to date with `origin/develop` before preparing the
  release branch.
- Confirm `main` is up to date with `origin/main` before opening or merging the
  release pull request.
- Confirm workspace-local crates have deliberate `publish = false` status for
  this release.

```powershell
rtk git status --short
rtk git remote -v
rtk git fetch origin
rtk git status --short --branch
rtk cargo metadata --no-deps --format-version 1
```

## 2. Local Checks

Run the same release-blocking checks that CI runs before packaging:

```powershell
rtk cargo fmt --all -- --check
rtk cargo clippy --workspace --all-targets -- -D warnings
rtk cargo test --workspace
rtk cargo doc --workspace --no-deps
```

If FFI or WASM surfaces changed, also run their target-specific checks before
tagging:

```powershell
rtk cargo check -p mmd-anim-wasm --target wasm32-unknown-unknown
rtk cargo build -p mmd-anim-ffi --release
```

## 3. Package Audit

Run package list audits for every crate that will be published:

```powershell
rtk cargo package -p mmd-anim-runtime --list
rtk cargo package -p mmd-anim-format --list
rtk cargo package -p mmd-anim --list
```

Confirm package lists do not include local corpora, generated reports, `target`,
`.opencode`, agent workspaces, root `scripts`, ignored docs, or generated WASM
harness output.

## 4. Dry Run

Run crates.io dry runs in dependency order. For a first release of multiple
interdependent crates, crates.io can only dry-run crates whose published
dependencies already exist in the crates.io index. That means the normal first
release flow is:

1. Dry-run the first dependency-free crate.
2. Publish it.
3. Dry-run the next dependent crate.
4. Publish it.
5. Repeat until the publish set is complete.

For this workspace, the order is:

```powershell
rtk cargo publish -p mmd-anim-runtime --dry-run
rtk cargo publish -p mmd-anim-runtime

rtk cargo publish -p mmd-anim-format --dry-run
rtk cargo publish -p mmd-anim-format

rtk cargo publish -p mmd-anim --dry-run
rtk cargo publish -p mmd-anim
```

Do not publish a crate until its own dry-run passes. For later releases, when all
workspace crates already exist on crates.io at compatible versions, it may be
possible to run all dry-runs before publishing.

## 5. Commit

Commit the version, changelog, and release-prep changes on `develop` or a
short-lived release branch. Open a pull request into `main` after local checks
and package audits pass.

```powershell
rtk git status --short
rtk git add Cargo.toml Cargo.lock CHANGELOG.md README.md docs/RELEASE.md
rtk git commit -m "chore(release): vX.Y.Z"
rtk git push origin HEAD
```

Adjust the staged files if the release includes additional source, metadata, or
CI changes. Do not include local-only docs, corpus outputs, generated reports,
or ignored workspace artifacts.

## 6. Tag

After the release pull request is reviewed, merged, and verified on `main`,
create the release tag from the reviewed `main` commit.

```powershell
rtk git checkout main
rtk git pull --ff-only origin main
rtk git tag vX.Y.Z
rtk git push origin vX.Y.Z
```

The tag must match the Cargo package version exactly. For example, version
`0.2.0` must be tagged as `v0.2.0`.

## 7. Publish

Publish manually unless release automation has been explicitly enabled and
verified for crates.io publishing. Follow the dry-run/publish sequence in
section 4 and wait for each crate to become available on crates.io before
publishing crates that depend on it.

## 8. GitHub Release

Create or update the GitHub Release for `vX.Y.Z`.

- Use the matching `CHANGELOG.md` version section as the release notes.
- Mention the experimental API / ABI status when it still applies.
- Confirm the release points to the same commit as the pushed tag.

## 9. Post-release Verification

Verify the published artifacts rather than only the local checkout:

```powershell
mkdir mmd-anim-consumer-smoke
cd mmd-anim-consumer-smoke
rtk cargo init --lib
rtk cargo add mmd-anim@X.Y.Z
rtk cargo check
```

Check the crates.io pages for every published crate:

- version
- repository URL
- license
- README rendering
- categories and keywords
- dependency versions

When compatibility matters for a downstream app, install the published version
there or in a temporary consumer before updating that app.

## 10. Rollback Notes

Crates.io packages cannot be deleted after publication. If a bad package is
published:

1. Yank the affected version with `cargo yank`.
2. Publish a fixed patch release if needed.
3. Document the issue in `CHANGELOG.md`.
