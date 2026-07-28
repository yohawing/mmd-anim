# Sour Miku `(Not) A Devil` arm-twist evidence

This note records the source-PMM lane for `sour-miku-not-a-devil-arm-twist`.

## Provenance

- PMX: `F:/MMD/pmx/Sour式初音ミクVer.1.02/Black.pmx`
- VMD: `F:/MMD/vmd/DECO27 x PinocchioP - (Not) A Devil feat. Hatsune Miku/(Not) A Devil - Motion (Nikisa San).vmd`
- Source PMM (read-only): `F:/Develop/MMDDev/GoldenOracle/sour_notdevil.pmm`
- MMDDumper/MMD: MikuMikuDance 9.32 x64
- The PMM model slot references the exact Black PMX. The raw VMD has 28,908 bone and 3,266 morph keyframes; the source PMM contains 28,819 and 3,208 respectively.

`compare-pmm-document-vmd-keyframes` reports 1,474 differences: 1,327 keyed bone transform differences and 147 missing frames (89 bones, 58 morphs). All 1,327 keyed transform differences are on the four PMX fixed-axis bones `左腕捩`, `右腕捩`, `左手捩`, and `右手捩`; no non-fixed keyed bone has a direct PMM/VMD transform mismatch. The PMX fixed-axis vectors are retained in the PMX parser (`fixedAxis=true`).

The earlier interpolation comparison was invalid because nested PMM curve arrays were compared without flattening them. The corrected, flattened study finds that all 28,819 common bone keys use MMD's registered diagonal/block VMD interpolation layout. Nanoem/raw parsing instead reads the first 16 bytes in its strided layout and skips the remaining 48 bytes.

## Record and compare

The shared manifest and synchronized `oracle-batch.json` contain the exact PMX/VMD paths and `assets.pmm: "../sour_notdevil.pmm"`, with focused arm/twist bones and 16 stress frames. Dry-run, MMD record, JSONL validation, and coverage all pass (16 records; capture frames are the established +1 runtime samples).

The oracle-batch implementation patches the PMM template from raw VMD before recording. That generated-scene control passes (`mismatchCount=0`, `maxAbsError=0.0000438690`). The source-PMM document clip, built directly from the supplied PMM model tracks, also passes all 16 stress frames (`mismatchCount=0`, `maxAbsError=3.62396e-05`).

The final live raw-VMD Golden also passes 16 frames and 192 bone comparisons (`mismatchCount=0`, `maxAbsError=3.6239624e-05`).

A live MMD PMX+VMD import captured before any PMM save matches the source saved/reloaded PMM on the focused 12-arm-bone chain exactly. This establishes that the PMM save/load round trip is not the source of the fixed-axis rotation change.

## Fixed-axis experiment

The raw-vs-imported key study contains 1,989 fixed-axis keys. Exactly 1,327 keys change iff the raw quaternion has an off-axis vector component; the other 662 keys are already on-axis and remain unchanged. Translation and key interpolation bytes are preserved on disk, but MMD selects different redundant curve bytes during registration. `q.w` and vector magnitude are preserved within the observed numerical error. For example, `右腕捩` frame 2828 changes from `[-0.169383,-0.129608,0.005373,0.976976]` to `[-0.15898279,-0.14216678,-0.00554833,0.976976]`, matching the PMM value `[-0.15898275,-0.14216675,-0.00554833,0.976976]`.

The model-aware compatibility path combines two registration rules: (1) signed, angle-preserving PMX fixed-axis projection (`xyz = normalizedAxis * |q.xyz| * sign(dot(q.xyz, axis))`, `w = q.w`) and (2) MMD's block-layout interpolation decode for every mapped bone key, including non-fixed bones.

A controlled six-scenario synthetic VMD matrix was executed: mixed positive/negative dot, near-perpendicular, already `+axis`, already `-axis`, and identity cases. Raw and fixed-only-preprojected MMD dumps match on the arm chain; preprojecting the non-fixed right arm diverges by up to 3.38, while already-axis cases remain zero.

Conclusion: the trigger is PMX fixed-axis metadata and MMD's registered interpolation layout applied during VMD registration/import. It is not Twist naming and not PMM save/load. The direct source-PMM clip parity, the flattened interpolation study, the six-scenario synthetic matrix, and the final live raw-VMD Golden all agree with this model-aware compatibility path.
