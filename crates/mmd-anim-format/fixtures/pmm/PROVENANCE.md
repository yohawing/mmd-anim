# PMM Fixture Provenance (ik_multi_bone_from_pmx_vmd)

Important: `ik_multi_bone_from_pmx_vmd.pmm` is a portable parser/exporter fixture,
not a direct MMD GUI load fixture. It intentionally embeds the relative model path
`..\pmx\ik_multi_axis_limit.pmx`; MikuMikuDance can show
`"ik_multi_axis_limit_fixture" のモデルファイルが見つかりません` when this checked-in
PMM is opened directly. Generate a fresh PMM with `export-pmm-scene` and an existing
PMX argument for GUI smoke testing; the CLI embeds a normal absolute model path.

## Artifacts

- `ik_multi_bone_from_pmx_vmd.pmm`
  - SHA-256: `F5C2A444910622E98E388761DA4997E640E709A05769CC739BC4C0B420EB68D3`
- `ik_multi_bone_from_pmx_vmd.inspect.json`
  - SHA-256: `1DA1342DDC2F2CCE5FB5C3C9A93059C80C81DA8E12D734E22C1D4C3DB7887032`
- Source VMD: `../vmd/ik_multi_bone_nondefault.vmd` (repo-local)
  - SHA-256: `8373BD1FFE9AA0ABB1B1DD9D4729D6EB76AD0ADA6DF441EBB626BAC0C1B3FF68`

## Source Model (PMX)

- Fixture: `crates/mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx`
- Bones: 3 (`link_root`, `effector`, `ik_controller`)
- Morphs: 0

Embedded model path in this checked-in PMM fixture (relative):
`..\pmx\ik_multi_axis_limit.pmx`

The current `export-pmm-scene` CLI resolves the PMX argument to a normal absolute path before embedding it in newly generated PMM files. This checked-in relative-path fixture is kept as a portable parser/exporter fixture and as historical evidence for the relative-path GUI caveat below.

Note: The PMMv2 document model block stores the document model index byte, then the variable-length display names, then the 256-byte fixed model path. Layout: `[docModelIndex][nameJa var][nameEn var][path256]...`. This is aligned across mmd-anim writer/reader and MMDDumper after byte-level checks against MMD 9.32 and a real PMMv2 sample.

Rejected layout notes:

- A one-byte reserved field after the document model index was tested and rejected.
- The old path-first layout (`[docModelIndex][path256][nameJa][nameEn]`) was tested and rejected. MMD 9.32 consumed the first path byte as a variable-string length and reported truncated paths such as `telephone.pmx` -> `"elephone.pmx"` and `xtelephone.pmx` -> `"telephone.pmx"`.
- A real PMMv2 sample (`F:\MMD\vmd\123_響喜乱舞\1.pmm`) showed `[docModelIndex][nameJa var][nameEn var][path256]` around the document model block.

## Generation Context

- Tool: `mmd-anim-cli export-pmm-scene`
- Command:
  - From `crates/mmd-anim-format/fixtures/pmm/`:
  - `rtk cargo run --manifest-path F:\Develop\MMDDev\mmd-anim\Cargo.toml -p mmd-anim-cli -- export-pmm-scene ..\pmx\ik_multi_axis_limit.pmx ..\vmd\ik_multi_bone_nondefault.vmd ik_multi_bone_from_pmx_vmd.pmm`
- VMD: generated as a deterministic synthetic repo-local fixture (hand-crafted non-default bone keyframes for IK multi-bone test coverage).
- PMM: produced from the PMX + VMD pair with the mmd-anim PMMv2 document scene writer.
- Inspect JSON: produced with `node F:\Develop\MMDDev\MMDDumper\src\cli.mjs inspect-pmm-document-keyframes F:\Develop\MMDDev\mmd-anim\crates\mmd-anim-format\fixtures\pmm\ik_multi_bone_from_pmx_vmd.pmm`.
- Normal Cargo tests read the checked-in PMM and inspect JSON fixtures via `include_bytes!` / `include_str!`; they do not invoke MMDDumper at test runtime.
- MMDDumper oracle status:
  - MMDDumper base commit observed in this slice: `ca9dea9` (dirty tree during this round).
  - Local MMDDumper reader/writer updated for name-first document model layout (`documentModelIndex`, model name, English model name, fixed PMX path) to match mmd-anim pmm.rs writer/reader and pmm_with_document_summary helper.
  - Regenerated PMM byte check: `docIndex=0`, `nameLen=27`, `name=ik_multi_axis_limit_fixture`, `englishLen=27`, `pathOffset=112`, `path=..\pmx\ik_multi_axis_limit.pmx`.
  - `compare-pmm-document-vmd-keyframes` result on this PMM + VMD pair:
    - ok: true
    - pmmBoneKeyframes: 5 compared (3 initial + 2 additional)
    - vmdBoneFrames: 5
    - pmmMorphKeyframes: 0
    - vmdMorphFrames: 0
    - mismatches: 0

## Verification Status

- MMD GUI (MikuMikuDance 9.32) verification:
  - Local MMDDumper smoke diagnostics use `--window-snapshot-after-ms` / `--window-snapshot-out` to capture early `pmm ver.2.0 ロード` dialogs as JSON.
  - MMDDumper writer, name-first/fixed-path, absolute real PMX (`telephone.pmx`): MMD 9.32 produced `records=1`; early snapshot only contained `MSCTFIME UI`, with no model-file-not-found dialog.
  - mmd-anim writer, name-first/fixed-path, absolute real PMX (`F:\Develop\MMDDev\MMDDumper\out\real-pmx-smoke\telephone.pmx`): MMD 9.32 produced `records=1`; early snapshot only contained `MSCTFIME UI`, with no model-file-not-found dialog. Output: `F:\Develop\MMDDev\MMDDumper\out\mmd-anim-writer-absolute-telephone\telephone_scene_mmd_anim.pmm`.
  - Current CLI path policy smoke: a repo-local PMX path is canonicalized to a normal absolute `F:\...` string, not a Windows `\\?\` verbatim path, before being embedded in the PMM document model block. MMDDumper `inspect-pmm-document-keyframes` and `compare-pmm-document-vmd-keyframes` both pass on the generated smoke PMM with `mismatches=0`.
  - The repository-local relative fixture (`..\pmx\ik_multi_axis_limit.pmx`) also produced `records=1` under automation, but the early snapshot still showed a model-file-not-found dialog for `"ik_multi_axis_limit_fixture"`. Treat this as a relative-path/synthetic-PMX GUI resolution caveat, not as a byte-layout failure.
- This document was updated as part of the `ik_multi_bone_*` PMM/VMD test fixture correction (name-first, no-reserved document model layout alignment).

## Related Document Model Counts (from inspect)

- initialBoneKeyframes: 3
- boneKeyframes: 2 additional (5 total including 3 initial)
- initialMorphKeyframes: 0
- morphKeyframes: 0
- initialModelKeyframe: 1
- modelKeyframes: 0

## Non-default Additional Bone Keyframes (to be asserted in tests)

- link_root @ frame 15:
  - previousKeyframeIndex: 0
  - nextKeyframeIndex: 0
  - translation: [0.05000000074505806, 0.0, 0.0]
  - orientation: [0.0, 0.0, 0.0, 1.0]
  - interpolation (flat 16 bytes): [10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160]
  - physicsSimulationDisabled: false
- effector @ frame 30:
  - previousKeyframeIndex: 1
  - nextKeyframeIndex: 0
  - translation: [0.0, 0.10000000149011612, 0.0]
  - orientation: [0.0, 0.0, 0.0, 1.0]
  - physicsSimulationDisabled: false
