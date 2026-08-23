# MMDDumper source snapshot

This directory is a mechanical source snapshot imported from:

- Source repository: `F:\Develop\MMDDev\MMDDumper`
- Branch: `feature/mmdplugin-integration`
- Base commit: `ca9dea9`
- Snapshot date: `2026-08-23`

The snapshot contains the current working-tree bytes for all tracked repository
files, plus the explicitly selected untracked source needed by the
`mmdplugin-integration` work: `lib/README.md`, `lib/mmd/MMDExport.h`,
`lib/mmdplugin/mmd_plugin.h`, `native/compat/`,
`native/mmd_plugin_dumper.cpp`, and the selected capture/diagnostic scripts
under `scripts/`.

Included categories are package and lock files, JavaScript source and tests,
native source and headers, schemas, repository-owned fixtures, scripts, docs,
and repository guidance (`AGENTS.md`, `PLAN.md`, and `README.md`).

Excluded categories are Git metadata, the nested `mmd-mcp/` repository, the
MikuMikuDance installation, `node_modules/`, generated output and local build
artifacts, `.ai/`, and other local assets not explicitly selected above.

This is an imported baseline only. In the source repository, the baseline
characterization is **142 passing / 7 failing** tests: 3 failures require
missing external PMM assets, and 4 are PMM parser fixture/EOF failures. From
this relocated directory, the initial result is **139 passing / 10 failing**;
the 3 additional failures retain a hard-coded source-repository path. This
snapshot does not fix or reinterpret those failures.
