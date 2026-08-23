# OSS Reference Notes

Purpose: collect implementation references for the MMD oracle dumper without copying source code.

## MMDPlugin / MMDUtility

- Source: https://github.com/oigami/MMDUtility
- Install notes mirror: https://github-wiki-see.page/m/oigami/MMDUtility/wiki/how_to_install
- Useful takeaway: the MMD community has an installer/plugin-management path where users select `MikuMikuDance.exe`, then install plugin zip files. This is a safer user-facing target than a generic injector.
- Constraint for this project: use only as workflow reference unless license/source compatibility is audited.

## MMPlus

- Source article: https://learnmmd.com/http%3A/learnmmd.com/mmplus-a-new-mmd-plugin/
- Useful takeaway: MMPlus installs next to `MikuMikuDance.exe` and uses an architecture-specific `MSIMG32.dll` proxy plus its own plugin DLL. This supports a proxy-DLL loading strategy for MMD-local tooling.
- Constraint for this project: keep the proxy path opt-in and MMD-local. Do not implement generic process injection.

## Ray-MMD / MikuMikuEffect ecosystem

- Source: https://github.com/ray-cast/ray-mmd
- Useful takeaway: MMD extensions commonly depend on MikuMikuEffect and Direct3D 9 render timing.
- Constraint for this project: visual effect hooks are outside the first oracle dumper MVP. Current MVP should stay on `MMDExport` data: PMD model count, bone world matrices, morph weights, frame time.

## Decision

Initial implementation path:

1. Keep the current read-only `MMDExport` API snapshot core.
2. Build a DLL that exports `MmdOracleDumpOnce` and does no heavy work in `DllMain`.
3. Prefer an MMD-local proxy/plugin loading path over an injector.
4. Add runner support only after the loading path and frame advance behavior are verified on MMD 9.32 x64.
