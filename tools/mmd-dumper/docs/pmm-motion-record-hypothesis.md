# PMM Motion Record Hypothesis

Evidence type: `fixture inventory evidence`

This note tracks the current PMM motion-section hypothesis. It is intentionally narrower than a PMM specification.

## Current Result

Known sampled PMMs do not contain a raw VMD header (`Vocaloid Motion Data 0002`). MMD appears to expand loaded VMD data into PMM-native records.

The strongest current reference is nanoem's PMM document implementation:

```text
https://github.com/hkrn/nanoem/blob/30acffaa29f5d2eb9e997d69418f2e4b97b5894f/emapp/src/internal/project/PMM.cc
https://github.com/hkrn/nanoem/blob/30acffaa29f5d2eb9e997d69418f2e4b97b5894f/nanoem/ext/document.c
```

`PMM.cc` is the project-side loader, not the byte-layout definition. In `PMM::Context::load`, it creates a `nanoem_document_t`, calls `nanoemDocumentLoadFromBuffer`, then loads accessories, models, camera, light, and self-shadow from that document. The binary PMMv2 layout is implemented in `ext/document.c`: `nanoemDocumentLoadFromBuffer` detects the PMMv1/PMMv2 signature and dispatches to `nanoemDocumentParse`, while model bone and morph keyframes are parsed by `nanoemDocumentModelBoneKeyframeParse` and `nanoemDocumentModelMorphKeyframeParse`.

Useful source anchors:

```text
PMM.cc:584-614                  PMM::Context::load document load/transfer flow
PMM.cc:1062-1088                model document objects resolved into project models
ext/document.c:2781-2806        PMMv1/PMMv2 signature detection and document parse dispatch
ext/document.c:433-442          model bone keyframe parse: base, interpolation, translation, orientation, flags
ext/document.c:1345-1349        model morph keyframe parse: base, weight, selected flag
```

`MMDDumper` now has `inspect-pmm-document-keyframes`, which reads the PMMv2 model keyframe sections directly from that layout instead of inferring the 62-byte records only from VMD/PMM diffs.

`compare-pmm-document-vmd-keyframes` uses the same direct PMMv2 reader to compare a PMM against a VMD without a base PMM or diff profile. On the Tda 9-key transform fixture it reports `mismatches=0` for 9/9 bone frames.

`patch-pmm-document-vmd-keyframes` is the first direct-document embedding path. It rebuilds the target model's PMMv2 bone/morph keyframe sections from a VMD:

```powershell
pnpm -C MMDDumper patch-pmm-document-vmd-keyframes -- --template ..\data\pmm\tda_parent_center_groove_transform_keys.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_direct_document_patch_generated.pmm
pnpm -C MMDDumper compare-pmm-document-vmd-keyframes -- --pmm out\pmm-analysis\tda_direct_document_patch_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd
pnpm -C MMDDumper patch-pmm-document-vmd-keyframes -- --template ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_base_direct_document_grow_patch_generated.pmm
```

The same-shape Tda run has `byteLengthDelta=0`, rewrites 9 existing bone records, and verifies `mismatches=0`. The direct grow Tda run starts from `tda_base_no_motion.pmm`, inserts 9 additional bone records with `byteLengthDelta=558`, updates document keyframe indices and prev/next links, sets `lastFrameIndex=91`, and verifies `mismatches=0`. MMD also opens the generated PMM through the runtime hook and writes `95` oracle JSONL records through frame `92.9999971`.

The parser initially over-skipped PMMv2 bone states as 33 bytes. The nanoem layout is 31 bytes: translation `float3`, orientation `float4`, and three bytes for dirty / disable-physics / selected-track count. Correcting this makes direct document parsing work across two-model PMMs. The direct grow patcher now verifies slot-specific insertion on `tda_two_models_base_no_motion.pmm`: `--target-slot 1` grows slot 1 from 0 to 6 bone keyframes while slot 0 remains unchanged. The same path also verifies a multi-PMX Tda+Sour base PMM, growing the Sour slot from 0 to 6 keyframes with `mismatches=0`.

`oracle-from-vmd` is the current one-command fixture path for runtime numeric evidence:

```powershell
pnpm -C MMDDumper oracle-from-vmd -- --template-pmm ..\data\pmm\tda_base_no_motion.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out-dir out\oracle-from-vmd-smoke --dry-run true
```

In dry-run mode it writes the generated PMM and fixture JSON without launching MMD. The Tda smoke dry-run reports `byteLengthDelta=558`, `rewriteCount=9`, and direct document/VMD comparison `mismatches=0`. Without `--dry-run true`, the same command launches MMD through the existing runtime dumper and writes oracle JSONL. This is automated but not fully headless: MMD still needs a logged-in Windows GUI session, and the frame-stepping key sender can restore/focus the MMD window.

For CI-style datasets, `oracle-batch` keeps the visible test input as PMX/VMD pairs and moves the required base PMM into a template registry:

```json
{
  "defaults": {
    "outDir": "out/oracle-batch"
  },
  "templates": [
    {
      "pmx": "../data/pmx/Tda式初音ミクV4X_Ver1.00/Tda式初音ミクV4X_Ver1.00.pmx",
      "templatePmm": "../data/pmm/tda_base_no_motion.pmm",
      "targetSlot": 0
    }
  ],
  "cases": [
    {
      "name": "tda-transform",
      "pmx": "../data/pmx/Tda式初音ミクV4X_Ver1.00/Tda式初音ミクV4X_Ver1.00.pmx",
      "vmd": "out/pmm-analysis/tda-parent-center-groove-transform-keys-target.vmd"
    }
  ]
}
```

Run it with:

```powershell
pnpm -C MMDDumper oracle-batch -- --manifest out\test-oracle-batch\oracle-batch.json --dry-run true
```

The current Tda batch dry-run reports one case, writes `scene.pmm` and `fixture.json` under the case output directory, and verifies direct document/VMD comparison with `mismatches=0`.

This is still model bone/morph keyframe embedding only; camera, light, self-shadow, and property IK VMD channels are rejected.

For the controlled Tda fixtures, the most useful current layout is no longer just a marker-derived candidate. A hand-made MMD-saved PMM with three movable bones and three transform keys each verifies a 62-byte transform key record:

```text
data/pmm/tda_parent_center_groove_transform_keys.pmm
  base: data/pmm/tda_base_no_motion.pmm
  VMD:  out/pmm-analysis/tda-parent-center-groove-transform-keys.vmd

  byteLengthDelta: +558 = 9 * 62
  block:           0x4341..0x456f
  records:         9
  initialRecord:   58 bytes per initial bone keyframe
  recordSize:      62 bytes per additional bone keyframe
  documentIndex:   record + 0x00
  frame:           record + 0x04
  prev/next links: record + 0x08 / +0x0c
  interpolation:   record + 0x10
  position:        record + 0x20
  rotation:        record + 0x2c
  selected/physics:record + 0x3c / +0x3d

  position match:       9/9
  frame sequence match: 9/9
  rotation match:       9/9
```

Earlier diff profiles reported frame/position/rotation at `+0x08/+0x24/+0x30` because the profile block started four bytes before the actual additional keyframe record, at the preceding key-count field. The direct PMMv2 parser uses the actual record offset.

Additional bone keyframes do not store the bone index directly in the first int32. That field is the document keyframe index. The bone name is resolved using the PMM keyframe chain: follow `previousKeyframeIndex` until reaching an initial bone keyframe, then use that initial keyframe's object index. This matches nanoem's `nanoemDocumentModelBoneKeyframeGetName` behavior.

The record order follows VMD bone-frame order for this fixture:

```text
全ての親: 0x4341, 0x437f, 0x43bd
センター: 0x43fb, 0x4439, 0x4477
グルーブ: 0x44b5, 0x44f3, 0x4531
```

`patch-pmm-vmd-diff-cluster` can use this donor fixture as a verified same-shape profile and rewrite another VMD with the same bone order/key count. The generated target PMM `out/pmm-analysis/tda_parent_center_groove_transform_keys_target_generated.pmm` verifies the same coverage:

```text
position match:       9/9
frame sequence match: 9/9
rotation match:       9/9
```

This proves same-shape embedding for one Tda model with multiple movable bones, multiple keys, and position+rotation. It does not yet prove arbitrary key-count insertion/deletion or multi-model routing.

The two-model Tda/Tda fixtures show how the same 62-byte transform-key block is routed when the same PMX path and same bone names appear twice in one PMM:

```text
data/pmm/tda_two_models_base_no_motion.pmm
  model slot 0 path offset: 0x56
  model slot 1 path offset: 0x6839

data/pmm/tda_two_models_slot0_transform_keys.pmm
  base: data/pmm/tda_two_models_base_no_motion.pmm
  VMD:  out/pmm-analysis/tda-multimodel-slot-transform-keys.vmd

  byteLengthDelta: +372 = 6 * 62
  block:           0x4341..0x44b5
  inferred slot:   0, by last model path before block start
  records:         6
  frame:           record + 0x08
  position:        record + 0x24
  rotation:        record + 0x30

data/pmm/tda_two_models_slot1_transform_keys.pmm
  byteLengthDelta: +372 = 6 * 62
  block:           0xab24..0xac98
  inferred slot:   1, by last model path before block start
  records:         6
  frame:           record + 0x08
  position:        record + 0x24
  rotation:        record + 0x30
```

This strongly suggests MMD stores per-model motion sections in model-slot order. A bone-name-only writer is unsafe when model slots share bone names. `cluster-pmm-vmd-diff` now attaches `modelSlotContext` to verified block profiles using the provisional `last-model-path-before-block-start` rule.

The same fixtures also verify slot-specific same-shape embedding. Using the slot0 donor patches the slot0 block, and using the slot1 donor patches the slot1 block:

```powershell
pnpm -C MMDDumper patch-pmm-vmd-diff-cluster -- --base ..\data\pmm\tda_two_models_base_no_motion.pmm --donor-base ..\data\pmm\tda_two_models_base_no_motion.pmm --donor-variant ..\data\pmm\tda_two_models_slot0_transform_keys.pmm --donor-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys.vmd --target-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys-target.vmd --out out\pmm-analysis\tda_two_models_slot0_transform_keys_target_generated.pmm
pnpm -C MMDDumper patch-pmm-vmd-diff-cluster -- --base ..\data\pmm\tda_two_models_base_no_motion.pmm --donor-base ..\data\pmm\tda_two_models_base_no_motion.pmm --donor-variant ..\data\pmm\tda_two_models_slot1_transform_keys.pmm --donor-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys.vmd --target-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys-target.vmd --out out\pmm-analysis\tda_two_models_slot1_transform_keys_target_generated.pmm
```

Both generated PMMs verify `6/6` position, frame sequence, and rotation coverage. The patch command now preserves `modelSlotContext` in both donor and verification profiles. This is still same-shape donor-profile embedding, not arbitrary key-count generation.

For multi-model PMMs, pass `--donor-slot` and `--target-slot` to make the command fail if the inferred block slot does not match the intended model:

```powershell
pnpm -C MMDDumper patch-pmm-vmd-diff-cluster -- --base ..\data\pmm\tda_two_models_base_no_motion.pmm --donor-base ..\data\pmm\tda_two_models_base_no_motion.pmm --donor-variant ..\data\pmm\tda_two_models_slot1_transform_keys.pmm --donor-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys.vmd --target-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys-target.vmd --out out\pmm-analysis\tda_two_models_slot1_transform_keys_target_generated.pmm --donor-slot 1 --target-slot 1
```

The Tda+Sour fixtures confirm the same model-slot rule across different PMX files that share common bone names:

```text
data/pmm/tda_sour_base_no_motion.pmm
  slot 0: Tda式初音ミクV4X_Ver1.00.pmx, path offset 0x56
  slot 1: Black.pmx, path offset 0x6830
  shared bone names include 全ての親, センター, グルーブ, 腰, 操作中心

data/pmm/tda_sour_tda_transform_keys.pmm
  VMD:            out/pmm-analysis/tda-multimodel-slot-transform-keys.vmd
  byteLengthDelta:+372 = 6 * 62
  block:          0x4341..0x44b5
  inferred slot:  0
  verification:   position/frame/rotation 6/6

data/pmm/tda_sour_sour_transform_keys.pmm
  VMD:            out/pmm-analysis/tda-sour-common-transform-keys.vmd
  byteLengthDelta:+372 = 6 * 62
  block:          0xc1ac..0xc320
  inferred slot:  1
  verification:   position/frame/rotation 6/6
```

Both Tda and Sour blocks can be same-shape-patched with explicit slot guards:

```powershell
pnpm -C MMDDumper patch-pmm-vmd-diff-cluster -- --base ..\data\pmm\tda_sour_base_no_motion.pmm --donor-base ..\data\pmm\tda_sour_base_no_motion.pmm --donor-variant ..\data\pmm\tda_sour_tda_transform_keys.pmm --donor-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys.vmd --target-vmd out\pmm-analysis\tda-multimodel-slot-transform-keys-target.vmd --out out\pmm-analysis\tda_sour_tda_transform_keys_target_generated.pmm --donor-slot 0 --target-slot 0
pnpm -C MMDDumper patch-pmm-vmd-diff-cluster -- --base ..\data\pmm\tda_sour_base_no_motion.pmm --donor-base ..\data\pmm\tda_sour_base_no_motion.pmm --donor-variant ..\data\pmm\tda_sour_sour_transform_keys.pmm --donor-vmd out\pmm-analysis\tda-sour-common-transform-keys.vmd --target-vmd out\pmm-analysis\tda-sour-common-transform-keys-target.vmd --out out\pmm-analysis\tda_sour_sour_transform_keys_target_generated.pmm --donor-slot 1 --target-slot 1
```

Sour differs from Tda in early project/cache-like bytes when motion is loaded, so this is not a minimal record-only patch. It is a donor-profile patch that transplants the donor changed middle and then rewrites same-shape key records.

The corresponding two-keys-per-bone fixture confirms the same layout with fewer records:

```text
data/pmm/tda_parent_center_groove_two_transform_keys.pmm
  base: data/pmm/tda_base_no_motion.pmm
  VMD:  out/pmm-analysis/tda-parent-center-groove-two-transform-keys.vmd

  byteLengthDelta: +372 = 6 * 62
  block:           0x4341..0x44b5
  records:         6
  recordSize:      62 bytes
  frame:           record + 0x08
  position:        record + 0x24
  rotation:        record + 0x30

  position match:       6/6
  frame sequence match: 6/6
  rotation match:       6/6
```

Comparing the two-key and three-key Tda transform PMMs:

```text
two-key PMM byteLength:   28243
three-key PMM byteLength: 28429
delta:                   +186 = 3 * 62

shared block start:       0x4341
two-key block end:        0x44b5
three-key block end:      0x456f
```

Important non-record fields also change:

```text
0x0d17: max frame-like value, 60 -> 90
0x0dd1: model/bone cache-like id, 0xf1 -> 0xf2
0x0e0b: model/bone cache-like id, 0xf3 -> 0xf5
0x4341: key count-like value, 6 -> 9
```

`analyze-pmm-key-count-delta` now emits this as a repeatable fixture inventory report:

```powershell
pnpm -C MMDDumper analyze-pmm-key-count-delta -- --base ..\data\pmm\tda_base_no_motion.pmm --small-variant ..\data\pmm\tda_parent_center_groove_two_transform_keys.pmm --small-vmd out\pmm-analysis\tda-parent-center-groove-two-transform-keys.vmd --large-variant ..\data\pmm\tda_parent_center_groove_transform_keys.pmm --large-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys.vmd
```

Observed summary:

```text
smallKeyCount:                6
largeKeyCount:                9
recordCountDelta:             3
recordByteLength:             62
expectedRecordByteDelta:      186
actualByteLengthDelta:        186
recordByteDeltaMatchesFileDelta: true
sharedBlockStart:             true
blockExpansionByteLength:     186

maxFrame candidates:
  0x0d17: 60 -> 90
  0x447f: 60 -> 90

keyCount candidates:
  0x4341: 6 -> 9

changedBeforeBlock scalar candidates:
  0x0d17: 60 -> 90
  0x0dd1: 241 -> 242
  0x0e0b: 243 -> 245
```

There are also larger cache/timeline-like regions around `0x495e..0x4aa8` and later frame/cache values near the suffix. That means a key-count-changing writer cannot safely insert only the 62-byte key records. It must also update count/max-frame/cache fields, or use a donor profile whose key count already matches the target.

An experimental key-count-delta patcher now exists for this controlled shape. It applies the verified 6-key -> 9-key PMM delta first, then rewrites the resulting 9-record block from a target VMD:

```powershell
pnpm -C MMDDumper patch-pmm-vmd-key-count-delta -- --base ..\data\pmm\tda_base_no_motion.pmm --small-variant ..\data\pmm\tda_parent_center_groove_two_transform_keys.pmm --small-vmd out\pmm-analysis\tda-parent-center-groove-two-transform-keys.vmd --large-variant ..\data\pmm\tda_parent_center_groove_transform_keys.pmm --large-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys.vmd --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --target-slot 0
```

Observed verification:

```text
byteLengthDeltaFromSmall:  +186
byteLengthDeltaFromBase:   +558
scalar rewrites:
  0x4341 keyCount = 9
  0x0d17 maxFrame = 91

position match:       9/9
frame sequence match: 9/9
rotation match:       9/9
output SHA-256:       317d7e000618601ddc4066916378f50bf4a184c145962f2b2544602a43e93473
```

The same controlled key-count delta can now be registered and selected by profile. A delta registry entry carries MMD-authored small/large PMMs and their VMDs:

```json
{
  "id": "tda-parent-center-groove-6-to-9-delta-v1",
  "kind": "key-count-delta",
  "source": {
    "smallVariant": "..\\data\\pmm\\tda_parent_center_groove_two_transform_keys.pmm",
    "largeVariant": "..\\data\\pmm\\tda_parent_center_groove_transform_keys.pmm",
    "smallVmd": "out\\pmm-analysis\\tda-parent-center-groove-two-transform-keys.vmd",
    "largeVmd": "out\\pmm-analysis\\tda-parent-center-groove-transform-keys.vmd"
  },
  "profile": {
    "verified": true,
    "recordByteLength": 62,
    "recordCount": 9,
    "modelSlotContext": { "slot": 0 }
  }
}
```

Registry entries may now carry `kind`. Supported values are `same-shape` and `key-count-delta`; the older saved profile label `pmm-keyframe-profile` is normalized to `same-shape`. If `kind` is omitted, the planner infers it from the `source` fields: `base`/`variant`/`vmd` means `same-shape`, while `smallVariant`/`largeVariant`/`smallVmd`/`largeVmd` means `key-count-delta`. This lets one registry hold both patch families without same-shape planning reporting delta entries as ordinary missing-field failures.

`inspect-pmm-patch-profile-registry` performs a target-free inventory pass over these patch registries:

```powershell
pnpm -C MMDDumper inspect-pmm-patch-profile-registry -- --registry out\pmm-analysis\tda_keyframe_profile_registry_cli.json --limit 4
pnpm -C MMDDumper inspect-pmm-patch-profile-registry -- --registry out\pmm-analysis\tda_key_count_delta_registry_cli.json --limit 4
```

Observed inspection summaries:

```text
same-shape registry:       profileCount 2, usableProfileCount 1, kindCounts same-shape=1 unknown=1
key-count-delta registry:  profileCount 2, usableProfileCount 1, kindCounts key-count-delta=1 unknown=1
```

The usable same-shape entry reports the 9-key Tda VMD (`全ての親`, `センター`, `グルーブ`) and all source files present. The usable delta entry reports `smallVmd` 6 bone frames and `largeVmd` 9 bone frames, also with all source files present. The intentionally broken entries are rejected as `unknown`.

For a higher-level capability view, `inventory-pmm-patch-profile-registries` merges multiple registries:

```powershell
pnpm -C MMDDumper inventory-pmm-patch-profile-registries -- --registries out\pmm-analysis\tda_keyframe_profile_registry_cli.json,out\pmm-analysis\tda_key_count_delta_registry_cli.json --limit 8
```

Observed capability inventory:

```text
registryCount:      2
profileCount:       4
usableProfileCount: 2
kindCounts:
  same-shape:       1
  key-count-delta:  1
  unknown:          2

usable same-shape:
  id:               tda-parent-center-groove-9key-v1
  recordByteLength: 62
  recordCount:      9
  modelSlot:        0
  boneFrameCount:   9
  boneNames:        全ての親, センター, グルーブ

usable key-count-delta:
  id:               tda-parent-center-groove-6-to-9-delta-v1
  recordByteLength: 62
  recordCount:      9
  modelSlot:        0
  smallBoneFrames:  6
  largeBoneFrames:  9
  boneFrameDelta:   3
```

The command currently returns non-zero when any registry entry is unusable. That is intentional for fixture inventory evidence: invalid profiles should not be silently accepted when building an embedding capability list.

For practical embedding runs, use `write-usable-pmm-patch-profile-registry` to export only entries that pass registry inspection:

```powershell
pnpm -C MMDDumper write-usable-pmm-patch-profile-registry -- --registries out\pmm-analysis\tda_keyframe_profile_registry_cli.json,out\pmm-analysis\tda_key_count_delta_registry_cli.json --out out\pmm-analysis\tda_usable_patch_registry_cli.json
pnpm -C MMDDumper inspect-pmm-patch-profile-registry -- --registry out\pmm-analysis\tda_usable_patch_registry_cli.json --limit 4
```

Observed usable export:

```text
sourceProfileCount:  4
profileCount:        2
omittedProfileCount: 2
usableKindCounts:
  same-shape:        1
  key-count-delta:   1

inspection:
  ok:                true
  profileCount:      2
  usableProfileCount: 2
```

The clean registry removes the intentionally broken `wrong-record-count` and `missing-delta-source` entries, while preserving the valid same-shape and key-count-delta profiles.

Before writing a PMM, `check-pmm-vmd-patch-compatibility` can ask the same registry whether a target VMD has a compatible embedding route:

```powershell
pnpm -C MMDDumper check-pmm-vmd-patch-compatibility -- --registries out\pmm-analysis\tda_usable_patch_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --target-slot 0 --limit 4
```

Observed compatibility result:

```text
ok:                     true
compatibleProfileCount: 2
incompatibleProfileCount: 0
selectedProfile:        tda-parent-center-groove-9key-v1
selectedKind:           same-shape
selectedCommand:        patch-pmm-vmd-diff-cluster
fallbackProfile:        tda-parent-center-groove-6-to-9-delta-v1
fallbackKind:           key-count-delta
```

The checker filters out planner-family mismatches, so a clean registry does not report a same-shape profile as "incompatible" merely because it is not a key-count-delta profile. Real incompatibilities still appear with reasons, for example target/donor key count or bone order mismatches.

If the target VMD contains channels outside the currently verified PMM writer scope, the checker returns a structured `unsupportedChannels` report instead of crashing:

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\target_with_morph_for_compat.vmd --bone-frame-keys "全ての親:31:101,102,103;全ての親:61:104,105,106;全ての親:91:107,108,109" --morph-name まばたき
pnpm -C MMDDumper check-pmm-vmd-patch-compatibility -- --registries out\pmm-analysis\tda_usable_patch_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\target_with_morph_for_compat.vmd --target-slot 0 --limit 4
```

Observed unsupported-channel result:

```text
ok: false
target:
  boneFrameCount:  3
  morphFrameCount: 1
unsupportedChannels:
  morphFrames: 1
nextRequiredFixtures:
  base PMM plus the same PMX with one controlled morph key, then a matching VMD morph-only oracle.
```

This keeps the current embedding claim narrow: bone transform records are writable through verified profiles, while morph/camera/light/self-shadow/property keyframes still need controlled PMM/VMD fixture pairs before they can be promoted.

For PMMs with multiple model slots, the same commands can resolve the target slot from a PMX path or filename:

```powershell
pnpm -C MMDDumper check-pmm-vmd-patch-compatibility -- --registries out\pmm-analysis\tda_usable_patch_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-pmx Tda式初音ミクV4X_Ver1.00.pmx --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --limit 4
pnpm -C MMDDumper patch-pmm-vmd-from-any-profile-registry -- --registries out\pmm-analysis\tda_usable_patch_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-pmx Tda式初音ミクV4X_Ver1.00.pmx --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_target_pmx_registry_patch_generated.pmm --diff-limit 96 --limit 4
```

Observed target PMX resolution:

```text
targetPmx:       Tda式初音ミクV4X_Ver1.00.pmx
targetModelSlot: 0
targetModelPath: F:/Develop/MMDDev/data/pmx/Tda式初音ミクV4X_Ver1.00/Tda式初音ミクV4X_Ver1.00.pmx
comparison:
  ok:             true
  pmmRecords:     9
  vmdBoneFrames:  9
  mismatchCount:  0
```

If the PMM contains duplicate matching PMX filenames, `--target-pmx` is intentionally ambiguous and the caller must pass `--target-slot`.

The patch side can now consume the same registry set through a unified planner. It evaluates same-shape profiles and key-count-delta profiles, then dispatches to the selected verified patcher:

```powershell
pnpm -C MMDDumper plan-pmm-vmd-patch-from-any-profile-registry -- --registries out\pmm-analysis\tda_keyframe_profile_registry_cli.json,out\pmm-analysis\tda_key_count_delta_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_any_registry_patch_target_generated.pmm --target-slot 0 --limit 4
pnpm -C MMDDumper patch-pmm-vmd-from-any-profile-registry -- --registries out\pmm-analysis\tda_keyframe_profile_registry_cli.json,out\pmm-analysis\tda_key_count_delta_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_any_registry_patch_target_generated.pmm --target-slot 0 --diff-limit 96 --limit 4
pnpm -C MMDDumper compare-pmm-vmd-keyframes -- --base ..\data\pmm\tda_base_no_motion.pmm --variant out\pmm-analysis\tda_any_registry_patch_target_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --diff-limit 64 --limit 8
```

Observed unified patch result:

```text
selectedProfile: tda-parent-center-groove-9key-v1
selectedKind:    same-shape
fallbackCandidate:
  id:            tda-parent-center-groove-6-to-9-delta-v1
  kind:          key-count-delta
  also ok:       true

comparison:
  ok:             true
  pmmRecords:     9
  vmdBoneFrames:  9
  maxPositionError: 0
  maxRotationError: 0
  mismatchCount:  0
```

This is now a practical embedding entry point for verified PMM keyframe profiles: callers can pass the current capability registries and let MMDDumper choose between same-shape transplant/rewrite and donor-delta growth. It is still bounded by the verified profile registry; it does not synthesize arbitrary PMM sections.

The same unified planner/patcher also works against the clean usable registry:

```powershell
pnpm -C MMDDumper plan-pmm-vmd-patch-from-any-profile-registry -- --registries out\pmm-analysis\tda_usable_patch_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_usable_registry_patch_target_generated.pmm --target-slot 0 --limit 4
pnpm -C MMDDumper patch-pmm-vmd-from-any-profile-registry -- --registries out\pmm-analysis\tda_usable_patch_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_usable_registry_patch_target_generated.pmm --target-slot 0 --diff-limit 96 --limit 4
pnpm -C MMDDumper compare-pmm-vmd-keyframes -- --base ..\data\pmm\tda_base_no_motion.pmm --variant out\pmm-analysis\tda_usable_registry_patch_target_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --diff-limit 64 --limit 8
```

Observed clean-registry patch result:

```text
selectedProfile: tda-parent-center-groove-9key-v1
selectedKind:    same-shape
comparison:
  ok:             true
  pmmRecords:     9
  vmdBoneFrames:  9
  maxPositionError: 0
  maxRotationError: 0
  mismatchCount:  0
```

`plan-pmm-vmd-key-count-delta-from-profile-registry` validates that the registered large VMD has the same bone key shape as the target VMD and emits the equivalent `patch-pmm-vmd-key-count-delta` inputs. `patch-pmm-vmd-key-count-delta-from-profile-registry` then selects the compatible delta and runs the verified delta patcher:

```powershell
pnpm -C MMDDumper plan-pmm-vmd-key-count-delta-from-profile-registry -- --registry out\pmm-analysis\tda_key_count_delta_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_delta_registry_patch_target_generated.pmm --target-slot 0 --limit 4
pnpm -C MMDDumper patch-pmm-vmd-key-count-delta-from-profile-registry -- --registry out\pmm-analysis\tda_key_count_delta_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_delta_registry_patch_target_generated.pmm --target-slot 0 --diff-limit 96 --limit 4
pnpm -C MMDDumper compare-pmm-vmd-keyframes -- --base ..\data\pmm\tda_base_no_motion.pmm --variant out\pmm-analysis\tda_delta_registry_patch_target_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --diff-limit 64 --limit 8
```

Observed delta registry comparison:

```text
ok:             true
selectedProfile: tda-parent-center-groove-6-to-9-delta-v1
pmmRecords:     9
vmdBoneFrames:  9
maxPositionError: 0
maxRotationError: 0
mismatchCount:  0
```

This is a key-count-changing embedder for a verified donor delta profile, not a fully semantic PMM writer. It still depends on a small PMM and a large PMM saved by MMD for the same model slot and bone/key shape transition so that unknown cache/timeline bytes come from MMD-authored data.

The same verified block profile can now be used in loader direction with `extract-pmm-vmd-keyframes`. This decodes the PMM-native records into a structured keyframe list using the VMD-shaped block as the bone/frame order oracle:

```powershell
pnpm -C MMDDumper extract-pmm-vmd-keyframes -- --base ..\data\pmm\tda_base_no_motion.pmm --variant out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd
```

Observed generated-PMM extraction:

```text
recordByteLength: 62
recordCount:      9
block:            0x4341..0x456f
slot:             0
frame offset:     +0x08
position offset:  +0x24
rotation offset:  +0x30

first record:
  name:     全ての親
  frame:    31
  position: [101, 102, 103]
  rotation: [0, 0.382683, 0, 0.92388]

last record:
  name:     グルーブ
  frame:    91
  position: [125, 126, 127]
  rotation: [0, 0, 0.382683, 0.92388]
```

This is not a standalone PMM parser yet because it still requires a base PMM and VMD-shaped oracle to locate and label the block. It is, however, a reusable PMM keyframe loader slice for verified motion blocks.

For gate-style checks, `compare-pmm-vmd-keyframes` compares the extracted PMM keyframes back to the VMD bone frames and exits non-zero on mismatch:

```powershell
pnpm -C MMDDumper compare-pmm-vmd-keyframes -- --base ..\data\pmm\tda_base_no_motion.pmm --variant out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --diff-limit 64
```

Observed comparison:

```text
ok:             true
pmmRecords:     9
vmdBoneFrames:  9
maxPositionError: 0
maxRotationError: 0
mismatchCount:  0
```

Once a verified profile has been saved, `extract-pmm-keyframes-with-profile` can decode the same-shape PMM records without loading the VMD again:

```powershell
pnpm -C MMDDumper extract-pmm-vmd-keyframes -- --base ..\data\pmm\tda_base_no_motion.pmm --variant out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --profile-out out\pmm-analysis\tda_parent_center_groove_9key_profile.json
pnpm -C MMDDumper extract-pmm-keyframes-with-profile -- --pmm out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --profile out\pmm-analysis\tda_parent_center_groove_9key_profile.json
```

Observed profile-only decode:

```text
ok: true
recordCount: 9
frames: 31 / 61 / 91 for each of 全ての親, センター, グルーブ
position/rotation: decoded directly from PMM record offsets
```

Before decoding, `check-pmm-keyframe-profile` can verify that the saved record/block offsets are readable in the target PMM:

```powershell
pnpm -C MMDDumper check-pmm-keyframe-profile -- --pmm out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --profile out\pmm-analysis\tda_parent_center_groove_9key_profile.json
```

For multiple saved layouts, `check-pmm-keyframe-profile-registry` ranks profile candidates and rejects entries whose record offsets or model slot context do not fit the PMM:

```powershell
pnpm -C MMDDumper check-pmm-keyframe-profile-registry -- --pmm out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --registry out\pmm-analysis\tda_keyframe_profile_registry_cli.json --limit 4
```

Observed registry check:

```text
ok: true
profileCount: 2
bestCandidate: tda-parent-center-groove-9key-v1
bestScore: 125
recordsChecked: 9
rejectedCandidate: wrong-record-count
rejection: Profile recordCount 99 does not match record list length 9.
```

`extract-pmm-keyframes-with-profile-registry` uses the same candidate ranking, selects the best compatible profile, and decodes the records directly:

```powershell
pnpm -C MMDDumper extract-pmm-keyframes-with-profile-registry -- --pmm out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --registry out\pmm-analysis\tda_keyframe_profile_registry_cli.json --limit 4
```

The same registry can drive the embedding side. `plan-pmm-vmd-patch-from-profile-registry` validates the donor source files, checks the donor VMD has the same bone key shape as the target VMD, applies slot hints from the profile, and emits the equivalent `patch-pmm-vmd-diff-cluster` inputs:

```powershell
pnpm -C MMDDumper plan-pmm-vmd-patch-from-profile-registry -- --registry out\pmm-analysis\tda_keyframe_profile_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_registry_plan_target_generated.pmm --target-slot 0 --limit 4
```

`patch-pmm-vmd-from-profile-registry` then selects the best compatible profile and runs the verified same-shape patcher:

```powershell
pnpm -C MMDDumper patch-pmm-vmd-from-profile-registry -- --registry out\pmm-analysis\tda_keyframe_profile_registry_cli.json --base ..\data\pmm\tda_base_no_motion.pmm --target-vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --out out\pmm-analysis\tda_registry_patch_target_generated.pmm --target-slot 0 --limit 4
pnpm -C MMDDumper compare-pmm-vmd-keyframes -- --base ..\data\pmm\tda_base_no_motion.pmm --variant out\pmm-analysis\tda_registry_patch_target_generated.pmm --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --diff-limit 64 --limit 8
```

Observed registry patch comparison:

```text
ok:             true
selectedProfile: tda-parent-center-groove-9key-v1
pmmRecords:     9
vmdBoneFrames:  9
maxPositionError: 0
maxRotationError: 0
mismatchCount:  0
```

`compare-pmm-keyframes-with-profile` uses that saved profile as the PMM-side decoder and compares the decoded records to a VMD without re-running base/variant PMM diff discovery:

```powershell
pnpm -C MMDDumper compare-pmm-keyframes-with-profile -- --pmm out\pmm-analysis\tda_parent_center_groove_6_to_9_delta_target_generated.pmm --profile out\pmm-analysis\tda_parent_center_groove_9key_profile.json --vmd out\pmm-analysis\tda-parent-center-groove-transform-keys-target.vmd --limit 8
```

Observed profile comparison:

```text
ok:             true
pmmRecords:     9
vmdBoneFrames:  9
maxPositionError: 0
maxRotationError: 0
mismatchCount:  0
```

This still depends on a verified profile that was derived from a PMM/VMD fixture pair. It removes VMD from the repeated decode path, and it removes base/variant diff discovery from the repeated comparison path, but it does not yet discover motion sections in an arbitrary PMM by itself.

`scan-pmm-motion` found repeated interpolation-like byte markers. The strongest high-precision marker is:

```text
14146b6b14146b6b
```

Using that marker, one candidate family lines up as:

```text
recordStart = markerOffset - 26
recordSize  = 58 bytes
```

Observed samples:

```text
sour_addiction.pmm
  first record: 0x1175
  first marker: 0x118f
  first 58-byte run: 0x1175..0x59bb, 319 records

tda_step.pmm
  first record: 0x0d0d
  first marker: 0x0d27
  all-candidate stride summary: 58 x 196, with 116/174/232 gaps
```

`sour_addiction.pmm` also contains a much larger 62-byte-stride family after its first 58-byte run:

```text
sour_addiction.pmm
  default marker candidates: 38994
  all-candidate stride summary: 62 x 37616, 58 x 377, 124 x 332
  largest 62-byte run with --record-bytes 62: 0xc255b..0x1ff57d, 20943 records
```

So the current conclusion is not "PMM motion record is always 58 bytes". It is: marker offset `+26` is useful, and at least two candidate record families are visible: a 58-byte family and a 62-byte family.

The gaps in `tda_step.pmm` line up with another interpolation-like marker:

```text
3131313136363636
```

That second marker is noisier, so `extract-pmm-motion-records` keeps `14146b6b14146b6b` as the default and requires `--marker 14146b6b14146b6b,3131313136363636` for combined extraction. The command emits `summary` for all candidates and `candidateFields` for the first 26 bytes so the current field hypothesis can be inspected without hand-decoding hex.

## Candidate Layout

The 58-byte candidate record currently looks like this:

```text
offset  size  hypothesis
0       12    three float32 values; often 0,0,1 or small normalized values
12      4     small uint32 flag; observed 0 or 1
16      4     usually zero in early samples
20      6     unknown; Sour sample has a u16-like sequence at offset 22
26      32    interpolation-like bytes, starts with marker
```

For the 62-byte family, the first 26 bytes still align before the interpolation marker. The extra 4 bytes are currently only captured by running `extract-pmm-motion-records --record-bytes 62`; their field meaning is still unknown.

The tiny rotating unittest fixture shows a more specific follow-up-record pattern in that 62-byte family. `extract-pmm-motion-records` now decodes the intentionally unaligned fields so the pattern can be checked without reading raw hex:

```text
offset  size  observed follow-up meaning in unittest_with_1bone_motion.pmm
0       4     float32 rotation-like component/cache
4       4     float32 rotation-like component/cache, often quaternion w
8       2     small state value, observed 1 on follow-up keys
10      2     key index-like value
12      4     frame << 16
14      4     unaligned exact frame
16      4     previous/index-like value << 16
18      2     previous/index-like value
22      2     next/index-like value
26      32    interpolation-like bytes, starts with marker
54      4     sometimes contains another rotation-like component/cache
```

This record-field hypothesis is still useful for large PMM inspection, but the smaller hand-made rotation fixtures below now provide the safer writer evidence. The same quaternion component can appear at offset 0, 4, or 54 and sometimes in neighboring records in this older six-frame fixture, so the writer uses the controlled unittest layout instead of this broad candidate-map alone.

## Minimal Fixture Findings

The hand-made same-model PMM fixtures under `data/pmm` provide a much cleaner signal:

```text
unittest_base_no_motion.pmm
  byteLength: 1795
  default marker candidates: 2

unittest_with_one_bone_key.pmm
  byteLength: 1857
  delta from base: +62
  commonPrefix: 0x190
  commonSuffix: 1022 bytes

unittest_with_1bone_motion.pmm
  byteLength: 2105
  delta from base: +310 = 5 * 62
  commonPrefix: 0x190
  commonSuffix: 1022 bytes

unittest_with_one_bone_key_frame31.pmm
  byteLength: 1857
  delta from base: +62

unittest_with_two_bone_keys.pmm
  byteLength: 1919
  delta from base: +124 = 2 * 62
```

The one-key fixture strongly suggests one PMM bone key adds or exposes one 62-byte unit in this tiny model case. The six-frame VMD fixture has seven marker candidates: the base/control record at `0x186`, a contiguous five-record run at `0x1c8..0x2fe`, and a trailing baseline/control-like record. This lines up with "frame 0 is represented by the existing first record, frames 9/19/29/39/49 add five 62-byte records" rather than raw VMD blob embedding.

For `unittest_with_one_bone_key.pmm`, the distinctive frame and position values are visible near the first candidate:

```text
frame 30:
  0x190: 1e 00 00 00
  0x1d6: 1e 00 00 00
position [1, 2, 3]:
  0x1f2: 00 00 80 3f
  0x1f6: 00 00 00 40
  0x1fa: 00 00 40 40
```

That means the key payload is not only in the 26 pre-marker bytes, and it is not always aligned to the PMM diff middle's 4-byte boundary. The surrounding changed middle region still matters for a future writer/patcher. `analyze-pmm-fixture-motion` now emits `middle.*.interestingFloats` and `middle.*.interestingIntegers` to catch these unaligned values.

The follow-up fixtures confirm the same offsets for edited key payloads:

```text
unittest_with_one_bone_key_frame31.pmm
  frame 31:
    0x190, 0x1d6
  position [7, 8, 9]:
    0x1f2, 0x1f6, 0x1fa

unittest_with_two_bone_keys.pmm
  first key, frame 30, position [1, 2, 3]:
    frame:    0x1d6
    position: 0x1f2, 0x1f6, 0x1fa
  second key, frame 60, position [4, 5, 6]:
    frame:    0x214
    position: 0x230, 0x234, 0x238
```

The first marker record's frame-like value changes to the latest/max key frame (`30`, `31`, or `60` in these fixtures), while individual key payloads appear later in the changed motion region.

Observed fields in `unittest_with_1bone_motion.pmm` with `--record-bytes 62`:

```text
recordStart  frame evidence
0x186        frame 49 appears in the first record's pre-marker bytes
0x1c8        frame 9 appears as u32 9 and as u32 (9 << 16)
0x206        frame 19 appears as u32 19 and as u32 (19 << 16)
0x244        frame 29 appears as u32 29 and as u32 (29 << 16)
0x282        frame 39 appears as u32 39 and as u32 (39 << 16)
0x2c0        frame 49 appears as u32 49 and as u32 (49 << 16)
```

Rotation values also appear in or around the changed middle section, but the marker-derived `recordStart = markerOffset - 26` is still only a useful candidate boundary. Some quaternion components are offset in a way that needs another controlled fixture before calling this a semantic record layout.

This is not enough to patch PMM safely yet. The next useful fixture set is another same-model PMM pair/trio:

```text
base_no_motion.pmm
with_one_bone_key.pmm
with_one_morph_key.pmm
```

Each VMD should use a distinctive frame and values. Then compare `extract-pmm-motion-records` output against known VMD fields.

## Useful Commands

```powershell
pnpm -C MMDDumper scan-pmm-motion -- ..\data\pmm\sour_addiction.pmm --limit 16
pnpm -C MMDDumper inspect-vmd -- out\pmm-analysis\kit\one-bone-one-morph.vmd
pnpm -C MMDDumper compare-vmd-pmm-motion -- --vmd out\pmm-analysis\kit\one-bone-one-morph.vmd --pmm ..\data\pmm\tda_step.pmm --limit 4
pnpm -C MMDDumper analyze-pmm-fixture-motion -- --base ..\data\pmm\unittest_base_no_motion.pmm --variant ..\data\pmm\unittest_with_one_bone_key.pmm --record-bytes 62 --limit 8
pnpm -C MMDDumper analyze-pmm-fixture-motion -- --base ..\data\pmm\unittest_base_no_motion.pmm --variant ..\data\pmm\unittest_with_1bone_motion.pmm --vmd ..\data\unittest\test_1bone_cube_motion.vmd --record-bytes 62 --limit 8
pnpm -C MMDDumper analyze-pmm-fixture-motion -- --base ..\data\pmm\unittest_base_no_motion.pmm --variant ..\data\pmm\unittest_with_two_bone_keys.pmm --record-bytes 62 --frames 30,60 --values 1,2,3,4,5,6 --limit 16
pnpm -C MMDDumper extract-pmm-motion-records -- ..\data\pmm\sour_addiction.pmm --limit 16
pnpm -C MMDDumper extract-pmm-motion-records -- ..\data\pmm\sour_addiction.pmm --record-bytes 62 --limit 16
pnpm -C MMDDumper dump-pmm-motion-records -- ..\data\pmm\sour_addiction.pmm --record-start 0x1175 --count 319 --record-bytes 58 --out out\pmm-analysis\sour-first-58-records.bin
pnpm -C MMDDumper patch-pmm-motion-records -- ..\data\pmm\sour_addiction.pmm --records out\pmm-analysis\sour-first-58-records.bin --record-start 0x1175 --count 319 --record-bytes 58 --out out\pmm-analysis\sour-first-58-records-roundtrip.pmm
pnpm -C MMDDumper extract-pmm-motion-records -- ..\data\pmm\tda_step.pmm --limit 16
pnpm -C MMDDumper extract-pmm-motion-records -- ..\data\pmm\tda_step.pmm --marker 14146b6b14146b6b,3131313136363636 --limit 16
```

`patch-pmm-motion-records` is intentionally same-size only. It does not yet create new PMM sections, resize sections, or update unknown counts.

`patch-pmm-fixture-motion` is a narrower experimental transplant command. Given a donor base PMM and donor motion PMM, it computes the changed middle region and splices that region into a compatible target base PMM:

```powershell
pnpm -C MMDDumper patch-pmm-fixture-motion -- --base ..\data\pmm\unittest_base_no_motion.pmm --donor-base ..\data\pmm\unittest_base_no_motion.pmm --donor-variant ..\data\pmm\unittest_with_one_bone_key.pmm --out out\pmm-analysis\unittest_one_bone_key_transplant.pmm
```

For the one-bone unittest fixture, this produces a PMM whose SHA-256 exactly matches `unittest_with_one_bone_key.pmm`. This is still a template/donor transplant, not a VMD-to-PMM writer.

The same transplant also exactly matches `unittest_with_two_bone_keys.pmm` when using `unittest_base_no_motion.pmm` as both target base and donor base. In that case the donor middle is larger because MMD also changes an early size/count-like field at `0x26`; the command remains a donor-template transplant rather than a semantic section writer.

## Direct Scalar Rewrite Experiment

`rewrite-pmm-scalars` is an unsafe but useful controlled-patch command. It can rewrite exact scalar patterns or write typed values at explicit offsets:

```powershell
pnpm -C MMDDumper rewrite-pmm-scalars -- ..\data\pmm\unittest_with_one_bone_key.pmm --out out\pmm-analysis\one_key_frame30_to_frame31_exact_patch.pmm --u32-at 0x26:483,0x190:31,0x1d6:31,0x33a:31,0x33e:16,0x342:31 --float32-at 0x1f2:7,0x1f6:8,0x1fa:9,0x236:7,0x23a:8,0x23e:9 --hex-at 0x1e2:4000407f4000407f4000407f14146b6b,0x2c9:01
```

This output exactly matches `unittest_with_one_bone_key_frame31.pmm` by SHA-256. The important split is:

```text
semantic-looking key values:
  max/current frame: 0x190, 0x1d6
  key position:      0x1f2, 0x1f6, 0x1fa
  duplicate/cache position:
                     0x236, 0x23a, 0x23e

non-semantic or still-hypothesis bytes:
  0x26               header/size/state-like value
  0x1e2..0x1f1       interpolation/control bytes
  0x2c9              one-byte state/index value
  0x33a,0x33e,0x342  timeline/cache frame-like values
```

So a semantic VMD-to-PMM writer is not proven yet, but the smallest one-key PMM can now be reproduced by explicit scalar and byte patching without copying the entire donor middle.

`write-pmm-unittest-bone-keys` wraps the same fixture-specific knowledge behind a small key writer for the one-bone unittest PMM template:

```powershell
pnpm -C MMDDumper write-pmm-unittest-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --out out\pmm-analysis\generated_one_key_frame31_command.pmm --keys "31:7,8,9"
pnpm -C MMDDumper write-pmm-unittest-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --out out\pmm-analysis\generated_two_keys_command.pmm --keys "30:1,2,3;60:4,5,6"
```

These outputs exactly match the hand-made `unittest_with_one_bone_key_frame31.pmm` and `unittest_with_two_bone_keys.pmm` fixtures by SHA-256. This is still a fixture-specific writer for the MMD unittest one-bone model, not a general PMM writer.

When a hand-made MMD-saved fixture exists, pass it as an oracle to make the SHA comparison explicit in the command output:

```powershell
pnpm -C MMDDumper write-pmm-unittest-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --out out\pmm-analysis\generated_two_keys_command.pmm --keys "30:1,2,3;60:4,5,6" --oracle ..\data\pmm\unittest_with_two_bone_keys.pmm
```

The output includes `oracleComparison.matches`, generated/oracle byte lengths, and generated/oracle SHA-256 values. The VMD-input writer accepts the same `--oracle` option.

The same command can now emit three or more position-only follow-up keys by repeating the observed 62-byte follow-up block:

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\three-position-keys.vmd --model-name Tda --bone-name センター --bone-keys "30:1,2,3;60:4,5,6;90:7,8,9"
pnpm -C MMDDumper write-pmm-unittest-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --out out\pmm-analysis\position_three_keys_probe.pmm --keys "30:1,2,3;60:4,5,6;90:7,8,9"
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\three-position-keys.vmd --out out\pmm-analysis\position_three_keys_from_vmd.pmm
```

This produces a structurally inspectable PMM with `keyCount=3`, `byteLengthDelta=124`, frame matches for `30/60/90`, and position matches for `[1,2,3]`, `[4,5,6]`, and `[7,8,9]`. With `map-vmd-pmm-bone-frames --marker 14146b6b --record-bytes 62`, the generated PMM reports `framesWithExactFrameRecord=3`, `framesWithPositionEvidence=3`, `framesWithLocalPositionEvidence=3`, and `framesWithRotationEvidence=3`. The VMD-input output and direct `--keys` output match each other by SHA-256. Unlike the one-key and two-key cases, there is not yet a hand-made MMD-saved three-position-key PMM fixture, so this is an experimental generated candidate rather than SHA-oracle-proven output.

The same repeated follow-up block path is covered for four position-only keys in unit tests and by CLI fixture inventory evidence. Direct `--keys` input is normalized by frame number, and duplicate frame keys are rejected because the fixture-specific key count/index fields do not establish a safe duplicate-frame representation.

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\four-position-keys.vmd --model-name Tda --bone-name センター --bone-keys "30:1,2,3;60:4,5,6;90:7,8,9;120:10,11,12"
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\four-position-keys.vmd --out out\pmm-analysis\position_four_keys_from_vmd.pmm
pnpm -C MMDDumper map-vmd-pmm-bone-frames -- --vmd out\pmm-analysis\four-position-keys.vmd --pmm out\pmm-analysis\position_four_keys_from_vmd.pmm --marker 14146b6b --record-bytes 62 --record-limit 16 --match-limit 4
```

The generated four-key PMM has `keyCount=4`, `byteLengthDelta=186`, SHA-256 `556447e6d7d853751b0ebdb046f34518320e3c6a1eb94d7863ca1ea602d2135e`, and mapping coverage `framesWithExactFrameRecord=4`, `framesWithPositionEvidence=4`, `framesWithLocalPositionEvidence=4`. `write-pmm-unittest-vmd-bone-keys` now includes this check in its own output as `generatedMapping.structurallyVerified`, so a separate mapping command is no longer required for the basic generated-position-key sanity check. Use `--require-verified true` when the command should fail instead of writing an unverified generated PMM.

The same verified path is covered for eight position-only keys:

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\eight-position-keys.vmd --model-name Tda --bone-name センター --bone-keys "30:1,2,3;60:4,5,6;90:7,8,9;120:10,11,12;150:13,14,15;180:16,17,18;210:19,20,21;240:22,23,24"
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\eight-position-keys.vmd --out out\pmm-analysis\position_eight_keys_from_vmd_required.pmm --require-verified true
```

The generated eight-key PMM has `keyCount=8`, `byteLengthDelta=434`, SHA-256 `5607f766c18e74690002b2aa801e44dd1d784d5500ee0be60bb15b8750212b08`, `generatedMapping.structurallyVerified=true`, and coverage `framesWithExactFrameRecord=8`, `framesWithPositionEvidence=8`, `framesWithLocalPositionEvidence=8`. This is still not SHA-oracle-proven against a hand-made MMD-saved eight-key PMM.

Unit and CLI coverage now extend the same repeated follow-up record path to thirty-two position-only keys:

```powershell
$keys = (0..31 | ForEach-Object { "$(( $_ + 1 ) * 30):$(( $_ * 3 ) + 1),$(( $_ * 3 ) + 2),$(( $_ * 3 ) + 3)" }) -join ';'
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\thirty-two-position-keys.vmd --model-name Tda --bone-name センター --bone-keys $keys
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\thirty-two-position-keys.vmd --out out\pmm-analysis\position_thirty_two_keys_from_vmd_required.pmm --require-verified true
```

The generated thirty-two-key PMM has `keyCount=32`, `byteLengthDelta=1922`, SHA-256 `0229caf5f68d043cec688c0bab5ac1d4b81f4ba7c6a2b8b1087b5f59c41fffd4`, `generatedMapping.structurallyVerified=true`, frame/local position coverage 32/32, and a 32-record contiguous run in unit coverage. This remains structural fixture inventory evidence; a hand-made MMD-saved thirty-two-key PMM is still needed before claiming SHA-oracle equivalence.

For larger probes, `write-test-vmd` can generate regular position-only key sequences without a long `--bone-keys` string:

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\dense-65535-position-keys.vmd --model-name Tda --bone-name センター --bone-key-count 65535 --bone-key-start-frame 1 --bone-key-frame-step 1 --bone-key-start-position 1,2,3 --bone-key-position-step 3,3,3
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\dense-65535-position-keys.vmd --out out\pmm-analysis\dense_65535_keys_compact_required.pmm --require-verified true --compact true
```

The dense 65535-key generated VMD path was checked with `--require-verified true` as well. The generated PMM has `keyCount=65535`, `maxFrame=65535`, `byteLengthDelta=4063108`, SHA-256 `93b9a686787266717fc1ee18a7620e951d9beb055a1590ca625428c6c803b28b`, `generatedMapping.structurallyVerified=true`, and frame/local position coverage 65535/65535. For this position-only generated mapping, rotation evidence is intentionally skipped, so rotation coverage is 0 rather than a writer claim. Use `--compact true` on `write-pmm-unittest-vmd-bone-keys` for these larger probes so `keys` and `exactFrameRecordOffsets` are summarized instead of printed in full. The VMD-driven PMM writer now reads all VMD bone frames by default, uses a coverage-only verification path for generated PMMs, and the frame-record mapper prefers candidates whose local record also contains the VMD position values so dense frame values do not get confused with incidental integer matches. `--limit` is only for intentional capped investigations.

`write-pmm-unittest-vmd-bone-keys` adds a VMD input path for the same restricted writer:

```powershell
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\kit\one-bone-one-morph.vmd --out out\pmm-analysis\generated_from_vmd_one_key_command.pmm --ignore-unsupported true
```

By default the command extracts frames for one bone from a VMD and writes position keys. It still rejects non-identity bone rotations unless `--allow-non-identity-rotation true` is passed. `--ignore-unsupported true` may be used for investigation VMDs that intentionally contain extra non-bone channels, but it does not silently drop unsupported channels unless explicitly requested.

For hand-made rotation PMM fixtures, generate rotation-only VMDs with zero position:

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\rotation-fixtures\one-rotation-key-x30.vmd --model-name テスト用モデル_arm --bone-name 全ての親 --position 0,0,0 --bone-rotation-keys "30:0.382683,0,0,0.92388"
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\rotation-fixtures\two-rotation-keys-x30-z60.vmd --model-name テスト用モデル_arm --bone-name 全ての親 --position 0,0,0 --bone-rotation-keys "30:0.382683,0,0,0.92388;60:0,0,0.382683,0.92388"
```

The resulting hand-made PMMs identify the controlled transform-key fields:

```text
first key:
  frame:    0x1d6
  position: 0x1f2, 0x1f6, 0x1fa
  rotation: 0x1fe, 0x202, 0x206, 0x20a

follow-up transform key inserted at 0x20e + (index - 1) * 62:
  frame:    +0x06
  position: +0x22, +0x26, +0x2a
  rotation: +0x2e, +0x32, +0x36, +0x3a
```

With `--allow-non-identity-rotation true`, the VMD-input writer now reproduces both rotation fixtures exactly:

```powershell
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\rotation-fixtures\one-rotation-key-x30.vmd --out out\pmm-analysis\generated-one-rotation-key-x30.pmm --bone-name 全ての親 --allow-non-identity-rotation true --oracle ..\data\pmm\unittest_with_one_rotation_key_x30.pmm
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\rotation-fixtures\two-rotation-keys-x30-z60.vmd --out out\pmm-analysis\generated-two-rotation-keys-x30-z60.pmm --bone-name 全ての親 --allow-non-identity-rotation true --oracle ..\data\pmm\unittest_with_two_rotation_keys_x30_z60.pmm
```

The one-key output has SHA-256 `db78b6b7d78332015a3da39f0a81f797aa0a9d9b937ffdc3e93be956a9c1d53c`; the two-key output has SHA-256 `684e16723d7a5b49ff91f1fb2e596ccdc4a19bcffc4e7eece309601e6ac1dfc1`. In both cases `oracleComparison.matches=true`. This is still a fixture-specific transform-key writer for the MMD unittest one-bone PMM layout, not a general PMM writer.

The same rotation offset (`record + 0x30`) also appears in the Tda transform fixture above. For the older unittest fixtures, the record start differs between the first key and follow-up keys, but `cluster-pmm-vmd-diff` now derives the rotation offset from the verified frame sequence instead of assuming it from marker position.

For generated probes that combine position and rotation, `write-test-vmd` accepts `--bone-transform-keys`:

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\three-transform-keys.vmd --model-name テスト用モデル_arm --bone-name 全ての親 --bone-transform-keys "30:1,2,3:0.382683,0,0,0.92388;60:4,5,6:0,0,0.382683,0.92388;90:7,8,9:0,0.382683,0,0.92388"
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\three-transform-keys.vmd --out out\pmm-analysis\three_transform_keys_from_vmd_required.pmm --bone-name 全ての親 --allow-non-identity-rotation true --require-verified true --compact true
```

The three-key transform candidate is structurally verified with exact frame, local position, and local rotation coverage 3/3. There is not yet a hand-made MMD-saved three-transform-key PMM fixture, so this is structural fixture inventory evidence rather than SHA-oracle equivalence.

For larger transform probes, `--bone-transform-key-count` generates regular frame/position values and cycles a quaternion sequence:

```powershell
pnpm -C MMDDumper write-test-vmd -- --out out\pmm-analysis\dense-65535-transform-keys.vmd --model-name テスト用モデル_arm --bone-name 全ての親 --bone-transform-key-count 65535 --bone-key-start-frame 1 --bone-key-frame-step 1 --bone-key-start-position 1,2,3 --bone-key-position-step 3,3,3
pnpm -C MMDDumper write-pmm-unittest-vmd-bone-keys -- ..\data\pmm\unittest_with_one_bone_key.pmm --vmd out\pmm-analysis\dense-65535-transform-keys.vmd --out out\pmm-analysis\dense_65535_transform_keys_required.pmm --bone-name 全ての親 --allow-non-identity-rotation true --require-verified true --compact true
```

The 65535-key transform candidate has `keyCount=65535`, `maxFrame=65535`, `byteLengthDelta=4063108`, SHA-256 `ee0480ca547d01347aab44525aaa68541dacc7bc13e63b23c87568460ddf7f88`, and exact frame/local position/local rotation coverage 65535/65535. The generated VMD is 7,274,459 bytes and the generated PMM is 4,064,965 bytes. The default rotation sequence cycles X/Y/Z 45-degree-like quaternions; use `--bone-transform-rotation-sequence "qx,qy,qz,qw;..."` to override it. In compact output, `generatedMapping.layoutRecordTotal` is the controlled transform layout record count, while `recordTotal` remains the older marker-scan candidate count and is not the transform verification count.

The file-writing commands now validate that the input template looks like the hand-made one-bone key PMM layout before applying fixed offsets. This rejects `unittest_base_no_motion.pmm` and larger arbitrary PMMs instead of silently patching the wrong layout.

With the current one-key investigation VMD, the generated PMM exactly matches `unittest_with_one_bone_key.pmm` by SHA-256. Position-only VMDs with three or more frames are accepted as experimental generated candidates. Rotation VMDs are supported only through the controlled unittest transform-key layout and must opt in with `--allow-non-identity-rotation true`.

For multi-key rotating VMDs, use `map-vmd-pmm-bone-frames` before attempting a writer:

```powershell
pnpm -C MMDDumper map-vmd-pmm-bone-frames -- --vmd ..\data\unittest\test_1bone_cube_motion.vmd --pmm ..\data\pmm\unittest_with_1bone_motion.pmm --record-bytes 62 --record-limit 32 --match-limit 2
```

On the current six-frame fixture this maps all six VMD frames to marker-derived PMM candidate records:

```text
frame 0  -> 0x186
frame 9  -> 0x1c8
frame 19 -> 0x206
frame 29 -> 0x244
frame 39 -> 0x282
frame 49 -> 0x2c0
```

This confirms that the five-record contiguous run at `0x1c8..0x2fe` represents the non-zero follow-up frames, while the first/control record at `0x186` also carries frame-0/max-frame state. Rotation evidence is present for all frames, but local rotation evidence is complete for only one frame in this fixture; the same float value may appear in neighboring records and cache-like areas. `rotationRecordDeltaSummary` now aggregates those candidates by component, offset, and the relative record delta from the matched frame record. On this fixture, the strongest buckets include:

```text
component 3, delta +62, offset 4, count 4, value 0.92388
component 3, delta   0, offset 4, count 3, value 0.92388
component 3, delta +124, offset 4, count 3, value 0.92388
component 0, delta   0, offset 54, count 2, values -0.382683 / 0.382683
component 2, delta +62, offset 0, count 2, values -0.382683 / 0.382683
```

This makes the record-to-record relationship easier to inspect, but it also shows why a rotation writer is not safe yet: the same semantic quaternion component can appear both in the matched frame record and in neighboring records depending on frame/component.

The same command can reproduce `unittest_with_two_bone_keys.pmm` from `unittest_with_one_bone_key.pmm` with one explicit 62-byte insertion plus small scalar/control updates:

```powershell
pnpm -C MMDDumper rewrite-pmm-scalars -- ..\data\pmm\unittest_with_one_bone_key.pmm --out out\pmm-analysis\one_key_to_two_keys_explicit_patch.pmm --u32-at 0x26:483,0x190:60,0x1de:2,0x33a:60,0x33e:45,0x342:60 --hex-at 0x1ce:0200,0x1e2:40005757400057574000575714146b6b,0x2c9:01 --float32-at 0x236:4,0x23a:5,0x23e:6 --insert-hex-at 0x20e:0000020000003c00000001000000000000002828407f2828407f2828407f14146b6b000080400000a0400000c0400000000000000000000000000000803f
```

This output exactly matches `unittest_with_two_bone_keys.pmm` by SHA-256. The explicit insertion appears to be the second key record plus adjacent state:

```text
insert offset: 0x20e
insert size:   62 bytes
second frame:  60 at inserted offset + 6
second pos:    [4, 5, 6] at inserted offset + 34/38/42
```

Existing-region updates around the first key also change:

```text
0x190        max/current frame 60
0x1ce        key count-like value 2
0x1de        key count/current index-like value 2
0x1e2..0x1f1 interpolation/control bytes for the first key
0x236..      duplicate/cache position for second key before insertion shift
0x33a..      timeline/cache frame-like values
```

This establishes that the minimal bone-key PMM can be changed both by editing one key and by inserting a second key, but the control/interpolation bytes and cache fields are still inferred rather than semantically decoded.
