# PMM Loader Development Plan

Evidence type: `fixture inventory evidence` first, then `runtime numeric evidence` when generated PMM fixtures are opened by MMD and dumped.

## First Work Slice

The first slice is a PMM/VMD bone-key investigation toolkit inside `MMDDumper`, not a general PMM writer or public release.

- Parse PMM asset references and report model/motion inventory.
- Parse VMD bone frames and group them by bone name.
- Map all VMD bone groups to marker-derived PMM motion candidate records.
- Treat nanoem `PMM.cc` as the project loader reference and `nanoem/ext/document.c` as the PMMv1/PMMv2 byte-layout reference.
- Keep PMM writing behind explicit verified layout profiles.
- Require `--require-verified true` and `--oracle` whenever a writer path is promoted from investigation to fixture generation.

## Non-Scope

- Arbitrary PMM generation.
- Arbitrary multi-character PMM rewriting.
- Morph, camera, light, self-shadow, and property IK embedding.
- Claiming PMM section semantics that are only marker-derived candidates.
- Treating MMD smoke screenshots as PMM writer equivalence.

## Milestones

1. Multi-bone mapping report.
   - `map-vmd-pmm-bone-frames --all-bones true`
   - Per-bone frame coverage and aggregate coverage.
   - No PMM writes.

2. VMD bone-group diagnostics.
   - Duplicate frame checks per bone.
   - Non-finite transform rejection.
   - Position-only versus transform-key summary.

3. PMM model-slot inventory.
   - PMM model path order as provisional slot IDs.
   - PMX bone inventory attached to each slot when PMX files are available.
   - Same-name bone collision reporting across slots.
   - Current slice: `inspect-pmm` reports provisional `modelSlots` from model path order.
   - Current slice: `inspect-pmm-model-slots` attaches readable PMX bone inventory and reports same-name bone collisions.
   - Current slice: duplicate PMX path references are preserved as distinct PMM slots, so the same model loaded twice reports slot 0 and slot 1 instead of collapsing to one model.
   - Current slice: `inspect-pmm-document-keyframes` parses PMMv2 model sections with the nanoem `ext/document.c` layout and reports model slot, bone/morph names, keyframe counts, record offsets, and resolved bone/morph keyframe names without using VMD diff profiles.

4. Layout profile extraction.
   - Move `unittest_with_one_bone_key.pmm` fixed offsets into a named `unittest-one-bone-transform-v1` profile.
   - Unknown templates are rejected before writing.
   - Current slice: `create-pmm-from-template` write mode delegates only writable profiles to the underlying writer and requires `--require-verified true`.

5. Multi-bone fixture writer.
   - Only after hand-made MMD fixture SHA-oracle exists.
   - Start with one PMX, two bones, two keys each.
   - Add rotation only after position SHA-oracle passes.
   - Current slice: Tda one-model / three movable bones / three transform keys each is verified as a same-shape donor profile.
   - Current slice: `patch-pmm-vmd-diff-cluster` rewrites same bone order/key count position+rotation keys and verifies 9/9 position, frame sequence, and rotation matches.
   - Current slice: Tda one-model / three movable bones / two transform keys each is also verified; the two-key to three-key delta is exactly `3 * 62` bytes plus count/max-frame/cache updates.
   - Current slice: `analyze-pmm-key-count-delta` turns the two-key versus three-key comparison into a reusable report and identifies candidate max-frame (`0x0d17`), key-count (`0x4341`), and cache-like (`0x0dd1`, `0x0e0b`) fields.
   - Current slice: `patch-pmm-vmd-key-count-delta` applies the verified 6-key -> 9-key delta and rewrites the resulting 9 transform records from a target VMD; Tda verifies 9/9 position, frame sequence, and rotation coverage.
   - Current slice: `plan-pmm-vmd-key-count-delta-from-profile-registry` selects a verified key-count delta donor from a registry and emits the `patch-pmm-vmd-key-count-delta` inputs.
   - Current slice: `patch-pmm-vmd-key-count-delta-from-profile-registry` applies the selected registry delta; generated Tda 6->9 output compares with max position/rotation error `0`.
   - Current slice: `plan-pmm-vmd-patch-from-profile-registry` selects a same-shape donor profile from a registry and emits the verified `patch-pmm-vmd-diff-cluster` inputs.
   - Current slice: `patch-pmm-vmd-from-profile-registry` selects the compatible registry profile and runs the existing verified same-shape patcher; generated Tda 9-key output compares with max position/rotation error `0`.
   - Current slice: mixed registries now support explicit or inferred `kind` values (`same-shape`, `key-count-delta`) so planner commands reject the other patch family as a type mismatch instead of a malformed profile.
   - Current slice: `inspect-pmm-patch-profile-registry` inventories same-shape and key-count-delta patch registries without a target VMD, checks required source files, summarizes source VMD bone counts, and rejects unsupported or incomplete entries.
   - Current slice: `inventory-pmm-patch-profile-registries` merges multiple patch registries into a capability inventory so the currently usable PMM embedding shapes can be audited at once.
   - Current slice: `write-usable-pmm-patch-profile-registry` filters investigation registries down to only verified/source-present entries; the Tda registry set currently exports 2 usable profiles and omits 2 intentionally invalid profiles.
   - Current slice: `check-pmm-vmd-patch-compatibility` reports whether a target VMD can be embedded by the current clean registry before writing a PMM; Tda currently reports 2 compatible profiles and 0 real incompatibilities.
   - Current slice: compatibility checks now report unsupported target VMD channels (`morphFrames`, `cameraFrames`, `lightFrames`, `selfShadowFrames`, `propertyFrames`) as structured fixture requirements instead of throwing an unclassified error.
   - Current slice: compatibility / unified registry patch commands accept `--target-pmx <model.pmx>` and resolve it to a PMM model slot before planning. Ambiguous duplicate PMX filenames still require `--target-slot`.
   - Current slice: `plan-pmm-vmd-patch-from-any-profile-registry` and `patch-pmm-vmd-from-any-profile-registry` select a compatible same-shape or key-count-delta profile from multiple registries and dispatch to the verified patcher.
   - Current slice: `extract-pmm-vmd-keyframes` decodes verified PMM-native transform records into structured keyframes (`name`, `frame`, `position`, `rotation`, offsets) using the VMD-shaped block as the temporary oracle.
   - Current slice: `compare-pmm-vmd-keyframes` provides a gate-style PMM-keyframe-versus-VMD comparison and returns non-zero on mismatch; generated Tda 9-key output compares with max position/rotation error `0`.
   - Current slice: `extract-pmm-vmd-keyframes --profile-out <profile.json>` saves the verified record layout as a reusable profile.
   - Current slice: `extract-pmm-keyframes-with-profile` decodes same-shape PMM records without loading VMD once a verified keyframe profile has been saved.
   - Current slice: `extract-pmm-keyframes-with-profile-registry` selects the best compatible saved profile and decodes PMM records without a caller-provided single profile path.
   - Current slice: `check-pmm-keyframe-profile` validates saved profile record/block offsets against a PMM before profile-based decode.
   - Current slice: `check-pmm-keyframe-profile-registry` ranks multiple saved profiles against a PMM and rejects candidates whose record offsets or model slot context do not fit.
   - Current slice: `compare-pmm-keyframes-with-profile` compares profile-decoded PMM records against a VMD without re-running base/variant PMM diff discovery; generated Tda 9-key output compares with max position/rotation error `0`.
   - Current slice: direct PMMv2 document parsing confirms the transform key records are `58` bytes for initial per-bone state keyframes and `62` bytes for additional bone keyframes. Additional bone keyframe names are resolved through the PMM document keyframe chain: the record's first int32 is a document keyframe index, and the bone/morph name is recovered by following `previousKeyframeIndex` back to an initial keyframe.
   - Current slice: `compare-pmm-document-vmd-keyframes` compares direct PMMv2 document bone/morph keyframes against VMD frames without requiring a base PMM, variant PMM diff, or saved profile. The Tda 9-key fixture reports `mismatches=0`.
   - Current slice: `patch-pmm-document-vmd-keyframes` rebuilds PMMv2 document bone/morph keyframe sections from VMD without donor diff transplant. It updates additional keyframe counts, document keyframe indices, per-object prev/next links, initial keyframe next links, and the model `lastFrameIndex`.
   - Current slice: direct document grow is verified on Tda base-no-motion -> 9 transform keys with `byteLengthDelta=558`, 9 rewritten bone records, and direct document/VMD comparison `mismatches=0`.
   - Current slice: the generated direct-grow PMM `out/pmm-analysis/tda_base_direct_document_grow_patch_generated.pmm` opens in MMD through the MMDDumper runtime hook and writes `95` oracle JSONL records through frame `92.9999971`.
   - Current slice: direct PMMv2 document parsing and grow patching work on a two-model PMM with an explicit `--target-slot`; slot 1 grows from 0 to 6 keyframes while slot 0 remains unchanged.
   - Current slice: the same direct document path works on a multi-PMX Tda+Sour PMM; `--target-slot 1` grows the Sour slot from 0 to 6 keyframes and direct document/VMD comparison reports `mismatches=0`.
   - Current slice: `oracle-from-vmd` connects the direct document patcher to the MMD runtime dumper. It writes a generated PMM, a fixture JSON, and then records oracle JSONL through MMD unless `--dry-run true` is passed.
   - Current slice: `oracle-from-vmd --dry-run true` on `tda_base_no_motion.pmm` and `tda-parent-center-groove-transform-keys-target.vmd` generates a PMM fixture with `byteLengthDelta=558`, `rewriteCount=9`, and direct document/VMD comparison `mismatches=0`.
   - Current slice: `oracle-batch` accepts CI-style PMX/VMD cases from a JSON manifest. Each case is expressed as `pmx + vmd`; the matching base PMM is resolved from `templates[]`, then `oracle-from-vmd` prepares or records the generated oracle.

6. Multi-PMX dry-run.
   - Report candidate model slots and ambiguous bone names.
   - Refuse writes unless target slot is explicit.
   - Current slice: `create-pmm-from-template --dry-run true` validates template slot, VMD channels, missing bones, and ambiguity without writing PMM bytes.
   - Current slice: dry-run separates input `okToWrite` from profile-gated `writable`; only `unittest-one-bone-transform-v1` is currently writable.
   - Current slice: two Tda instances in one PMM route the same six transform keys to different 62-byte blocks:
     `slot0 => 0x4341..0x44b5`, `slot1 => 0xab24..0xac98`.
   - Current slice: `cluster-pmm-vmd-diff` reports provisional `modelSlotContext` on verified block profiles using the last model path offset before the motion block.
   - Current slice: `patch-pmm-vmd-diff-cluster` can use slot0 or slot1 as a same-shape donor profile and verifies the generated slot-specific PMM with 6/6 position, frame sequence, and rotation coverage.
   - Current slice: `patch-pmm-vmd-diff-cluster --donor-slot <n> --target-slot <n>` rejects inferred slot mismatches before writing.
   - Current slice: Tda+Sour two-model fixtures confirm the same slot rule across different PMX files with shared bone names; Tda slot0 and Sour slot1 both verify 6/6 position, frame sequence, and rotation coverage.
   - Next slice: any multi-model writer must require an explicit target slot and choose a donor/target profile for that slot; bone-name-only routing remains rejected.

## Required Fixtures

- Existing one-bone position and rotation fixtures remain gating fixtures.
- New: one PMX, two bones, two position keys each.
- New: one PMX, two bones, two transform keys each.
- Existing: one PMX, three movable bones, three transform keys each:
  - `data/pmm/tda_parent_center_groove_transform_keys.pmm`
  - `MMDDumper/out/pmm-analysis/tda-parent-center-groove-transform-keys.vmd`
- Existing: one PMX, three movable bones, two transform keys each:
  - `data/pmm/tda_parent_center_groove_two_transform_keys.pmm`
  - `MMDDumper/out/pmm-analysis/tda-parent-center-groove-two-transform-keys.vmd`
- Existing: two Tda model instances in one PMM, motion on only one model:
  - `data/pmm/tda_two_models_base_no_motion.pmm`
  - `data/pmm/tda_two_models_slot0_transform_keys.pmm`
  - `data/pmm/tda_two_models_slot1_transform_keys.pmm`
  - `MMDDumper/out/pmm-analysis/tda-multimodel-slot-transform-keys.vmd`
- Existing: Tda + Sour in one PMM, motion on only one model:
  - `data/pmm/tda_sour_base_no_motion.pmm`
  - `data/pmm/tda_sour_tda_transform_keys.pmm`
  - `data/pmm/tda_sour_sour_transform_keys.pmm`
  - `MMDDumper/out/pmm-analysis/tda-sour-common-transform-keys.vmd`
- Recommended: hand-made PMM oracle for a generated Tda+Sour slot-specific target VMD, if we want runtime numeric evidence beyond same-shape structural verification.

## Gates

- `pnpm -C MMDDumper test`
- `pnpm -C MMDDumper oracle-batch -- --manifest <oracle-batch.json> [--case <name[,name]>] [--out-dir <dir>] [--dry-run true]`
- `pnpm -C MMDDumper oracle-from-vmd -- --template-pmm <template.pmm> --vmd <motion.vmd> [--target-slot <slot>] [--output <oracle.jsonl>] [--dry-run true]`
- `pnpm -C MMDDumper inspect-vmd -- <motion.vmd>`
- `pnpm -C MMDDumper inspect-pmm-document-keyframes -- <scene.pmm> [--limit <count>]`
- `pnpm -C MMDDumper compare-pmm-document-vmd-keyframes -- --pmm <scene.pmm> --vmd <motion.vmd> [--target-slot <slot>]`
- `pnpm -C MMDDumper patch-pmm-document-vmd-keyframes -- --template <template.pmm> --target-vmd <motion.vmd> --out <patched.pmm> [--target-slot <slot>]`
- Advanced investigation commands still exist under `node MMDDumper/src/cli.mjs <command> ...`, but they are intentionally not exposed as package scripts.
- Writer promotion requires SHA-256 match against hand-made PMM oracle.

## Risks

- The 58/62-byte bone keyframe record family and 31-byte PMMv2 bone state record are now backed by nanoem's PMMv2 document layout for parsed model sections. Direct document patching handles bone/morph key count changes inside one explicit target model slot.
- Direct document patching rejects camera, light, self-shadow, and property IK VMD channels.
- `oracle-from-vmd` automates MMD launch and dump, but MMD is still a Windows GUI/DirectX app. It is not service-style fully headless; the key sender may restore/focus the MMD window while stepping frames.
- Multiple characters usually share bone names, so bone-name-only routing is unsafe.
- Rotation evidence can appear in adjacent records or cache-like areas in older marker-derived scans; same-shape transform profiles must use verified frame sequence records.
- Generated dense-key coverage is structural evidence until a hand-made PMM oracle exists for that shape.
- Key-count-delta patching is verified donor-delta embedding. It is not arbitrary PMM section synthesis because unknown cache/timeline bytes are still transplanted from an MMD-saved large fixture.
