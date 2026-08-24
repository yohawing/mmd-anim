import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { analyzePmmVmdDiffClusters } from "../src/pmm-vmd-diff-clusters.mjs";
import {
  inspectPmmPatchProfileRegistry,
  patchPmmVmdDiffClusterProfile,
  patchPmmVmdKeyCountDeltaProfile,
  readUsablePmmPatchProfileRegistry,
  readPmmVmdAnyPatchRegistryPlan,
  readPmmVmdPatchCompatibility,
  planPmmVmdKeyCountDeltaPatchFromProfileRegistry,
  planPmmVmdPatchFromProfileRegistry,
  summarizePmmPatchProfileRegistryInventory,
  writeUsablePmmPatchProfileRegistry,
  writePmmVmdPatchFromAnyProfileRegistry,
  writePmmVmdKeyCountDeltaPatchFromProfileRegistry,
  writePmmVmdPatchFromProfileRegistry,
} from "../src/pmm-vmd-diff-patch.mjs";
import { writeSyntheticVmd } from "../src/vmd-writer.mjs";

test("patches same-shape position-only VMD keys into a donor PMM diff cluster", () => {
  const prefix = Buffer.from([1, 2, 3, 4]);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const donorVariant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    makeKeyBytes({ frame: 30, position: [7, 8, 9] }),
    makeKeyBytes({ frame: 60, position: [10, 11, 12] }),
    suffix,
  ]);
  const donorVmd = makeVmd([
    ["センター", 30, [1, 2, 3]],
    ["センター", 60, [4, 5, 6]],
    ["左足", 30, [7, 8, 9]],
    ["左足", 60, [10, 11, 12]],
  ]);
  const targetVmd = makeVmd([
    ["センター", 31, [101, 102, 103]],
    ["センター", 61, [104, 105, 106]],
    ["左足", 31, [107, 108, 109]],
    ["左足", 61, [110, 111, 112]],
  ]);

  const patched = patchPmmVmdDiffClusterProfile(base, base, donorVariant, donorVmd, targetVmd);
  const report = analyzePmmVmdDiffClusters(base, patched.bytes, targetVmd);

  assert.equal(patched.rewriteCount, 4);
  assert.equal(patched.byteLengthDelta, 246);
  assert.equal(patched.verification.coverage.matchedBoneFrames, 4);
  assert.equal(report.positionKeyBlockProfile.verified, true);
  assert.equal(report.positionKeyBlockProfile.recordByteLength, 62);
  assert.equal(patched.bytes.readUInt32LE(0x4 + 0x08), 31);
  assert.equal(patched.bytes.readFloatLE(0x4 + 0x24), 101);
  assert.equal(patched.bytes.readUInt32LE(0x4 + 62 * 3 + 0x08), 61);
  assert.equal(patched.bytes.readFloatLE(0x4 + 62 * 3 + 0x24), 110);
});

test("patches same-shape transform VMD keys into a donor PMM diff cluster", () => {
  const prefix = Buffer.from([1, 2, 3, 4]);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const donorVariant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    suffix,
  ]);
  const donorVmd = makeVmd([
    ["全ての親", 30, [1, 2, 3], [0.382683, 0, 0, 0.92388]],
    ["全ての親", 60, [4, 5, 6], [0, 0, 0.382683, 0.92388]],
  ]);
  const targetVmd = makeVmd([
    ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["全ての親", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
  ]);

  const patched = patchPmmVmdDiffClusterProfile(base, base, donorVariant, donorVmd, targetVmd);
  const report = analyzePmmVmdDiffClusters(base, patched.bytes, targetVmd);

  assert.equal(patched.rewriteCount, 2);
  assert.equal(patched.verification.coverage.rotationMatchedBoneFrames, 2);
  assert.equal(patched.verification.transformKeyBlockProfile.verified, true);
  assert.equal(report.transformKeyBlockProfile.rotationOffsetInRecord, 20);
  assert.equal(patched.bytes.readUInt32LE(0x4 + 0x08), 31);
  assert.equal(patched.bytes.readFloatLE(0x4 + 0x24), 101);
  assert.equal(patched.bytes.readFloatLE(0x4 + 0x14 + 4), Math.fround(0.382683));
  assert.equal(patched.bytes.readUInt32LE(0x4 + 62 + 0x08), 61);
  assert.equal(patched.bytes.readFloatLE(0x4 + 62 + 0x14 + 8), Math.fround(-0.382683));
});

test("preserves model slot context while patching a same-shape donor block", () => {
  const prefix = Buffer.alloc(200);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const donorVariant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    suffix,
  ]);
  const donorVmd = makeVmd([
    ["全ての親", 30, [1, 2, 3], [0.382683, 0, 0, 0.92388]],
    ["全ての親", 60, [4, 5, 6], [0, 0, 0.382683, 0.92388]],
  ]);
  const targetVmd = makeVmd([
    ["全ての親", 30, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["全ての親", 60, [104, 105, 106], [0.382683, 0, 0, 0.92388]],
  ]);
  const modelSlots = [
    { slot: 0, path: "model-a.pmx", fileName: "model-a.pmx", offset: 20 },
    { slot: 1, path: "model-b.pmx", fileName: "model-b.pmx", offset: 120 },
  ];

  const patched = patchPmmVmdDiffClusterProfile(base, base, donorVariant, donorVmd, targetVmd, {
    donorModelSlots: modelSlots,
    modelSlots,
  });

  assert.equal(patched.donorProfile.modelSlotContext.slot, 1);
  assert.equal(patched.verification.positionKeyBlockProfile.modelSlotContext.slot, 1);
  assert.equal(patched.verification.transformKeyBlockProfile.modelSlotContext.slot, 1);
});

test("rejects explicit model slot mismatches for donor blocks", () => {
  const prefix = Buffer.alloc(200);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const donorVariant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    suffix,
  ]);
  const donorVmd = makeVmd([
    ["全ての親", 30, [1, 2, 3]],
    ["全ての親", 60, [4, 5, 6]],
  ]);
  const targetVmd = makeVmd([
    ["全ての親", 30, [101, 102, 103]],
    ["全ての親", 60, [104, 105, 106]],
  ]);
  const modelSlots = [
    { slot: 0, path: "model-a.pmx", fileName: "model-a.pmx", offset: 20 },
    { slot: 1, path: "model-b.pmx", fileName: "model-b.pmx", offset: 120 },
  ];

  assert.throws(
    () =>
      patchPmmVmdDiffClusterProfile(base, base, donorVariant, donorVmd, targetVmd, {
        donorModelSlots: modelSlots,
        donorSlot: 0,
      }),
    /donor PMM motion block slot mismatch: expected slot 0, got 1/,
  );
});

test("patches same-shape rotation-only VMD keys through frame sequence records", () => {
  const prefix = Buffer.from([1, 2, 3, 4]);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const donorVariant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [0, 0, 0], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [0, 0, 0], rotation: [0, 0, 0.382683, 0.92388] }),
    suffix,
  ]);
  const donorVmd = makeVmd([
    ["全ての親", 30, [0, 0, 0], [0.382683, 0, 0, 0.92388]],
    ["全ての親", 60, [0, 0, 0], [0, 0, 0.382683, 0.92388]],
  ]);
  const targetVmd = makeVmd([
    ["全ての親", 31, [0, 0, 0], [0, 0.382683, 0, 0.92388]],
    ["全ての親", 61, [0, 0, 0], [0, 0, -0.382683, 0.92388]],
  ]);

  const patched = patchPmmVmdDiffClusterProfile(base, base, donorVariant, donorVmd, targetVmd);

  assert.equal(patched.verification.coverage.matchedBoneFrames, 0);
  assert.equal(patched.verification.coverage.frameSequenceMatchedBoneFrames, 2);
  assert.equal(patched.verification.coverage.rotationMatchedBoneFrames, 2);
  assert.equal(patched.verification.positionKeyBlockProfile.verified, false);
  assert.equal(patched.verification.transformKeyBlockProfile.verified, true);
  assert.equal(patched.bytes.readUInt32LE(0x4 + 0x08), 31);
  assert.equal(patched.bytes.readFloatLE(0x4 + 0x14 + 4), Math.fround(0.382683));
  assert.equal(patched.bytes.readUInt32LE(0x4 + 62 + 0x08), 61);
  assert.equal(patched.bytes.readFloatLE(0x4 + 62 + 0x14 + 8), Math.fround(-0.382683));
});

test("rejects VMDs with a different donor key shape", () => {
  const base = Buffer.alloc(4);
  const donorVariant = Buffer.concat([makeKeyBytes({ frame: 30, position: [1, 2, 3] }), makeKeyBytes({ frame: 60, position: [4, 5, 6] })]);
  const donorVmd = makeVmd([
    ["センター", 30, [1, 2, 3]],
    ["センター", 60, [4, 5, 6]],
  ]);
  const targetVmd = makeVmd([["左足", 30, [1, 2, 3]], ["左足", 60, [4, 5, 6]]]);

  assert.throws(() => patchPmmVmdDiffClusterProfile(base, base, donorVariant, donorVmd, targetVmd), /does not match donor bone/);
});

test("plans same-shape PMM patching from a keyframe profile registry", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-patch-plan-"));
  const baseFile = join(dir, "base.pmm");
  const donorBaseFile = join(dir, "donor-base.pmm");
  const donorVariantFile = join(dir, "donor-variant.pmm");
  const donorVmdFile = join(dir, "donor.vmd");
  const outFile = join(dir, "patched.pmm");
  await Promise.all([
    writeFile(baseFile, Buffer.alloc(1)),
    writeFile(donorBaseFile, Buffer.alloc(1)),
    writeFile(donorVariantFile, Buffer.alloc(1)),
    writeSyntheticVmd(donorVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "センター", frame: 60, position: [4, 5, 6] },
      ],
    }),
  ]);
  const targetVmd = makeVmd([
    ["全ての親", 31, [101, 102, 103]],
    ["センター", 61, [104, 105, 106]],
  ]);
  const report = await planPmmVmdPatchFromProfileRegistry(
    {
      profiles: [
        {
          id: "same-shape",
          source: { base: donorBaseFile, variant: donorVariantFile, vmd: donorVmdFile },
          profile: {
            verified: true,
            recordByteLength: 62,
            recordCount: 2,
            modelSlotContext: { slot: 1 },
          },
        },
      ],
    },
    targetVmd,
    { base: baseFile, targetVmdFile: "target.vmd", out: outFile, targetSlot: 1 },
  );

  assert.equal(report.ok, true);
  assert.equal(report.candidates[0].id, "same-shape");
  assert.equal(report.candidates[0].command.args.donorSlot, 1);
  assert.equal(report.candidates[0].command.args.targetSlot, 1);
  assert.equal(report.candidates[0].command.args.donorVmd, donorVmdFile);
});

test("plans mixed PMM patch registries by explicit or inferred profile kind", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-mixed-registry-"));
  const baseFile = join(dir, "base.pmm");
  const donorVariantFile = join(dir, "same-shape.pmm");
  const donorVmdFile = join(dir, "same-shape.vmd");
  const smallVariantFile = join(dir, "small.pmm");
  const largeVariantFile = join(dir, "large.pmm");
  const smallVmdFile = join(dir, "small.vmd");
  const largeVmdFile = join(dir, "large.vmd");
  const outFile = join(dir, "patched.pmm");
  await Promise.all([
    writeFile(baseFile, Buffer.alloc(1)),
    writeFile(donorVariantFile, Buffer.alloc(1)),
    writeFile(smallVariantFile, Buffer.alloc(1)),
    writeFile(largeVariantFile, Buffer.alloc(1)),
    writeSyntheticVmd(donorVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
        { name: "全ての親", frame: 90, position: [7, 8, 9] },
      ],
    }),
    writeSyntheticVmd(smallVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
      ],
    }),
    writeSyntheticVmd(largeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
        { name: "全ての親", frame: 90, position: [7, 8, 9] },
      ],
    }),
  ]);
  const registry = {
    profiles: [
      {
        id: "same-shape-inferred",
        kind: "pmm-keyframe-profile",
        source: { base: baseFile, variant: donorVariantFile, vmd: donorVmdFile },
        profile: { verified: true, recordByteLength: 62, recordCount: 3 },
      },
      {
        id: "delta-explicit",
        kind: "key-count-delta",
        source: {
          smallVariant: smallVariantFile,
          largeVariant: largeVariantFile,
          smallVmd: smallVmdFile,
          largeVmd: largeVmdFile,
        },
        profile: { verified: true, recordByteLength: 62, recordCount: 3 },
      },
    ],
  };
  const targetVmd = makeVmd([
    ["全ての親", 31, [101, 102, 103]],
    ["全ての親", 61, [104, 105, 106]],
    ["全ての親", 91, [107, 108, 109]],
  ]);

  const sameShapePlan = await planPmmVmdPatchFromProfileRegistry(registry, targetVmd, {
    base: baseFile,
    targetVmdFile: "target.vmd",
    out: outFile,
  });
  const deltaPlan = await planPmmVmdKeyCountDeltaPatchFromProfileRegistry(registry, targetVmd, {
    base: baseFile,
    targetVmdFile: "target.vmd",
    out: outFile,
  });

  assert.equal(sameShapePlan.ok, true);
  assert.equal(sameShapePlan.candidates[0].id, "same-shape-inferred");
  assert.equal(sameShapePlan.candidates[0].kind, "same-shape");
  assert.match(sameShapePlan.candidates.find((candidate) => candidate.id === "delta-explicit").reasons[0], /not same-shape/);
  assert.equal(deltaPlan.ok, true);
  assert.equal(deltaPlan.candidates[0].id, "delta-explicit");
  assert.equal(deltaPlan.candidates[0].kind, "key-count-delta");
  assert.match(deltaPlan.candidates.find((candidate) => candidate.id === "same-shape-inferred").reasons[0], /not key-count-delta/);
});

test("inspects mixed PMM patch registries without a target VMD", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-registry-inspect-"));
  const baseFile = join(dir, "base.pmm");
  const sameShapeFile = join(dir, "same-shape.pmm");
  const sameShapeVmdFile = join(dir, "same-shape.vmd");
  const smallFile = join(dir, "small.pmm");
  const largeFile = join(dir, "large.pmm");
  const smallVmdFile = join(dir, "small.vmd");
  const largeVmdFile = join(dir, "large.vmd");
  await Promise.all([
    writeFile(baseFile, Buffer.alloc(1)),
    writeFile(sameShapeFile, Buffer.alloc(1)),
    writeFile(smallFile, Buffer.alloc(1)),
    writeFile(largeFile, Buffer.alloc(1)),
    writeSyntheticVmd(sameShapeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "センター", frame: 60, position: [4, 5, 6] },
      ],
    }),
    writeSyntheticVmd(smallVmdFile, {
      boneFrames: [{ name: "全ての親", frame: 30, position: [1, 2, 3] }],
    }),
    writeSyntheticVmd(largeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
      ],
    }),
  ]);

  const report = await inspectPmmPatchProfileRegistry({
    profiles: [
      {
        id: "same-shape",
        kind: "pmm-keyframe-profile",
        source: { base: baseFile, variant: sameShapeFile, vmd: sameShapeVmdFile },
        profile: { verified: true, recordByteLength: 62, recordCount: 2, modelSlotContext: { slot: 0 } },
      },
      {
        id: "delta",
        source: { smallVariant: smallFile, largeVariant: largeFile, smallVmd: smallVmdFile, largeVmd: largeVmdFile },
        profile: { verified: true, recordByteLength: 62, recordCount: 2, modelSlotContext: { slot: 0 } },
      },
      {
        id: "broken",
        source: { base: baseFile, variant: join(dir, "missing.pmm"), vmd: sameShapeVmdFile },
        profile: { recordByteLength: 62 },
      },
    ],
  });

  assert.equal(report.ok, false);
  assert.equal(report.profileCount, 3);
  assert.equal(report.usableProfileCount, 2);
  assert.equal(report.kindCounts["same-shape"], 2);
  assert.equal(report.kindCounts["key-count-delta"], 1);
  assert.equal(report.entries[0].kind, "same-shape");
  assert.deepEqual(report.entries[0].vmd.vmd.boneNames, ["全ての親", "センター"]);
  assert.equal(report.entries[1].kind, "key-count-delta");
  assert.equal(report.entries[1].vmd.smallVmd.boneFrameCount, 1);
  assert.equal(report.entries[1].vmd.largeVmd.boneFrameCount, 2);
  assert.match(report.entries[2].reasons[0], /source.variant file does not exist/);
  assert.match(report.entries[2].warnings.join(" "), /not marked verified/);
});

test("summarizes PMM patch profile registry capability inventory", () => {
  const inventory = summarizePmmPatchProfileRegistryInventory([
    {
      registryFile: "same.json",
      entries: [
        {
          index: 0,
          id: "same",
          kind: "same-shape",
          ok: true,
          profile: { verified: true, recordByteLength: 62, recordCount: 9, modelSlotContext: { slot: 0 } },
          vmd: { vmd: { boneFrameCount: 9, maxFrame: 91, boneNames: ["全ての親", "センター"] } },
          reasons: [],
          warnings: [],
        },
      ],
    },
    {
      registryFile: "delta.json",
      entries: [
        {
          index: 0,
          id: "delta",
          kind: "key-count-delta",
          ok: true,
          profile: { verified: true, recordByteLength: 62, recordCount: 9, modelSlotContext: { slot: 0 } },
          vmd: {
            smallVmd: { boneFrameCount: 6, maxFrame: 60, boneNames: ["全ての親", "センター"] },
            largeVmd: { boneFrameCount: 9, maxFrame: 90, boneNames: ["全ての親", "センター"] },
          },
          reasons: [],
          warnings: [],
        },
        {
          index: 1,
          id: "bad",
          kind: "unknown",
          ok: false,
          profile: { verified: false },
          vmd: {},
          reasons: ["Registry entry kind unknown is not supported."],
          warnings: ["Profile is not marked verified."],
        },
      ],
    },
  ]);

  assert.equal(inventory.ok, false);
  assert.equal(inventory.registryCount, 2);
  assert.equal(inventory.profileCount, 3);
  assert.equal(inventory.usableProfileCount, 2);
  assert.equal(inventory.kindCounts["same-shape"], 1);
  assert.equal(inventory.kindCounts["key-count-delta"], 1);
  assert.equal(inventory.kindCounts.unknown, 1);
  assert.deepEqual(inventory.profiles[0].vmdShape.boneNames, ["全ての親", "センター"]);
  assert.equal(inventory.profiles[1].vmdShape.boneFrameDelta, 3);
  assert.match(inventory.profiles[2].reasons[0], /unknown/);
});

test("writes a usable-only PMM patch profile registry", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-usable-registry-"));
  const baseFile = join(dir, "base.pmm");
  const sameShapeFile = join(dir, "same-shape.pmm");
  const sameShapeVmdFile = join(dir, "same-shape.vmd");
  const smallFile = join(dir, "small.pmm");
  const largeFile = join(dir, "large.pmm");
  const smallVmdFile = join(dir, "small.vmd");
  const largeVmdFile = join(dir, "large.vmd");
  const registryFile = join(dir, "registry.json");
  const outFile = join(dir, "usable.json");
  await Promise.all([
    writeFile(baseFile, Buffer.alloc(1)),
    writeFile(sameShapeFile, Buffer.alloc(1)),
    writeFile(smallFile, Buffer.alloc(1)),
    writeFile(largeFile, Buffer.alloc(1)),
    writeSyntheticVmd(sameShapeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "センター", frame: 60, position: [4, 5, 6] },
      ],
    }),
    writeSyntheticVmd(smallVmdFile, {
      boneFrames: [{ name: "全ての親", frame: 30, position: [1, 2, 3] }],
    }),
    writeSyntheticVmd(largeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
      ],
    }),
  ]);
  await writeFile(
    registryFile,
    JSON.stringify({
      profiles: [
        {
          id: "same-shape",
          kind: "pmm-keyframe-profile",
          source: { base: baseFile, variant: sameShapeFile, vmd: sameShapeVmdFile },
          profile: { verified: true, recordByteLength: 62, recordCount: 2 },
        },
        {
          id: "delta",
          source: { smallVariant: smallFile, largeVariant: largeFile, smallVmd: smallVmdFile, largeVmd: largeVmdFile },
          profile: { verified: true, recordByteLength: 62, recordCount: 2 },
        },
        {
          id: "broken",
          source: { base: baseFile, variant: join(dir, "missing.pmm"), vmd: sameShapeVmdFile },
          profile: { recordByteLength: 62 },
        },
      ],
    }),
  );

  const report = await readUsablePmmPatchProfileRegistry({ registries: [registryFile] });
  const written = await writeUsablePmmPatchProfileRegistry({ registries: [registryFile], out: outFile });
  const usableRegistry = JSON.parse(await readFile(outFile, "utf8"));

  assert.equal(report.ok, true);
  assert.equal(report.sourceProfileCount, 3);
  assert.equal(report.profileCount, 2);
  assert.equal(report.omittedProfileCount, 1);
  assert.deepEqual(report.registry.profiles.map((profile) => profile.id), ["same-shape", "delta"]);
  assert.deepEqual(report.registry.profiles.map((profile) => profile.kind), ["same-shape", "key-count-delta"]);
  assert.equal(report.omittedProfiles[0].id, "broken");
  assert.equal(written.outFile, outFile);
  assert.equal(usableRegistry.kind, "pmm-patch-profile-registry");
  assert.equal(usableRegistry.profileCount, 2);
  assert.deepEqual(usableRegistry.profiles.map((profile) => profile.kind), ["same-shape", "key-count-delta"]);
});

test("writes same-shape PMM patches from a keyframe profile registry", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-registry-patch-"));
  const baseFile = join(dir, "base.pmm");
  const donorVariantFile = join(dir, "donor-variant.pmm");
  const donorVmdFile = join(dir, "donor.vmd");
  const targetVmdFile = join(dir, "target.vmd");
  const registryFile = join(dir, "registry.json");
  const outFile = join(dir, "patched.pmm");
  const base = Buffer.concat([Buffer.from([1, 2, 3, 4]), Buffer.from([0xaa, 0xbb]), Buffer.from([9, 8, 7, 6])]);
  const donorVariant = Buffer.concat([
    Buffer.from([1, 2, 3, 4]),
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    Buffer.from([9, 8, 7, 6]),
  ]);
  await Promise.all([
    writeFile(baseFile, base),
    writeFile(donorVariantFile, donorVariant),
    writeSyntheticVmd(donorVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
      ],
    }),
    writeSyntheticVmd(targetVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 31, position: [101, 102, 103] },
        { name: "全ての親", frame: 61, position: [104, 105, 106] },
      ],
    }),
    writeFile(
      registryFile,
      JSON.stringify({
        profiles: [
          {
            id: "same-shape",
            source: { base: baseFile, variant: donorVariantFile, vmd: donorVmdFile },
            profile: { verified: true, recordByteLength: 62, recordCount: 2 },
          },
        ],
      }),
    ),
  ]);
  const report = await writePmmVmdPatchFromProfileRegistry({
    registry: registryFile,
    base: baseFile,
    targetVmd: targetVmdFile,
    out: outFile,
  });

  assert.equal(report.ok, true);
  assert.equal(report.selectedProfile.id, "same-shape");
  assert.equal(report.patch.rewriteCount, 2);
  assert.equal(report.patch.verification.coverage.frameSequenceMatchedBoneFrames, 2);
});

test("plans and writes PMM patches from any compatible profile registry", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-any-registry-patch-"));
  const baseFile = join(dir, "base.pmm");
  const donorVariantFile = join(dir, "same-shape.pmm");
  const donorVmdFile = join(dir, "same-shape.vmd");
  const smallVariantFile = join(dir, "small.pmm");
  const largeVariantFile = join(dir, "large.pmm");
  const smallVmdFile = join(dir, "small.vmd");
  const largeVmdFile = join(dir, "large.vmd");
  const targetVmdFile = join(dir, "target.vmd");
  const sameRegistryFile = join(dir, "same-registry.json");
  const deltaRegistryFile = join(dir, "delta-registry.json");
  const outFile = join(dir, "patched.pmm");
  const base = Buffer.concat([Buffer.from([1, 2, 3, 4]), Buffer.from([0xaa, 0xbb]), Buffer.from([9, 8, 7, 6])]);
  const donorVariant = Buffer.concat([
    Buffer.from([1, 2, 3, 4]),
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    makeKeyBytes({ frame: 90, position: [7, 8, 9] }),
    Buffer.from([9, 8, 7, 6]),
  ]);
  const prefix = Buffer.alloc(64);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const smallVariant = Buffer.concat([
    makeHeader(60),
    makeKeyBytes({ count: 2, frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    suffix,
  ]);
  const largeVariant = Buffer.concat([
    makeHeader(90),
    makeKeyBytes({ count: 3, frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    makeKeyBytes({ frame: 90, position: [7, 8, 9] }),
    suffix,
  ]);
  await Promise.all([
    writeFile(baseFile, base),
    writeFile(donorVariantFile, donorVariant),
    writeFile(smallVariantFile, smallVariant),
    writeFile(largeVariantFile, largeVariant),
    writeSyntheticVmd(donorVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
        { name: "全ての親", frame: 90, position: [7, 8, 9] },
      ],
    }),
    writeSyntheticVmd(smallVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
      ],
    }),
    writeSyntheticVmd(largeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
        { name: "全ての親", frame: 90, position: [7, 8, 9] },
      ],
    }),
    writeSyntheticVmd(targetVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 31, position: [101, 102, 103] },
        { name: "全ての親", frame: 61, position: [104, 105, 106] },
        { name: "全ての親", frame: 91, position: [107, 108, 109] },
      ],
    }),
    writeFile(
      sameRegistryFile,
      JSON.stringify({
        profiles: [
          {
            id: "same-shape",
            source: { base: baseFile, variant: donorVariantFile, vmd: donorVmdFile },
            profile: { verified: true, recordByteLength: 62, recordCount: 3 },
          },
        ],
      }),
    ),
    writeFile(
      deltaRegistryFile,
      JSON.stringify({
        profiles: [
          {
            id: "delta-2-to-3",
            source: { smallVariant: smallVariantFile, largeVariant: largeVariantFile, smallVmd: smallVmdFile, largeVmd: largeVmdFile },
            profile: { verified: true, recordByteLength: 62, recordCount: 3 },
          },
        ],
      }),
    ),
  ]);
  const plan = await readPmmVmdAnyPatchRegistryPlan({
    registries: [sameRegistryFile, deltaRegistryFile],
    base: baseFile,
    targetVmd: targetVmdFile,
    out: outFile,
  });
  const patched = await writePmmVmdPatchFromAnyProfileRegistry({
    registries: [sameRegistryFile, deltaRegistryFile],
    base: baseFile,
    targetVmd: targetVmdFile,
    out: outFile,
  });

  assert.equal(plan.ok, true);
  assert.equal(plan.candidates[0].id, "same-shape");
  assert.equal(plan.candidates[0].command.script, "patch-pmm-vmd-diff-cluster");
  assert.equal(patched.ok, true);
  assert.equal(patched.selectedProfile.id, "same-shape");
  assert.equal(patched.patch.mode, "pmm-vmd-diff-cluster-patch");
  assert.equal(patched.patch.rewriteCount, 3);
  assert.equal(patched.patch.verification.coverage.frameSequenceMatchedBoneFrames, 3);
});

test("checks PMM patch compatibility before writing", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-compatibility-"));
  const baseFile = join(dir, "base.pmm");
  const donorVariantFile = join(dir, "same-shape.pmm");
  const donorVmdFile = join(dir, "same-shape.vmd");
  const smallVariantFile = join(dir, "small.pmm");
  const largeVariantFile = join(dir, "large.pmm");
  const smallVmdFile = join(dir, "small.vmd");
  const largeVmdFile = join(dir, "large.vmd");
  const targetVmdFile = join(dir, "target.vmd");
  const shortTargetVmdFile = join(dir, "target-short.vmd");
  const morphTargetVmdFile = join(dir, "target-morph.vmd");
  const registryFile = join(dir, "registry.json");
  const base = Buffer.concat([Buffer.from([1, 2, 3, 4]), Buffer.from([0xaa, 0xbb]), Buffer.from([9, 8, 7, 6])]);
  const donorVariant = Buffer.concat([
    Buffer.from([1, 2, 3, 4]),
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    makeKeyBytes({ frame: 90, position: [7, 8, 9] }),
    Buffer.from([9, 8, 7, 6]),
  ]);
  await Promise.all([
    writeFile(baseFile, base),
    writeFile(donorVariantFile, donorVariant),
    writeFile(smallVariantFile, donorVariant),
    writeFile(largeVariantFile, donorVariant),
    writeSyntheticVmd(donorVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
        { name: "全ての親", frame: 90, position: [7, 8, 9] },
      ],
    }),
    writeSyntheticVmd(smallVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
      ],
    }),
    writeSyntheticVmd(largeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3] },
        { name: "全ての親", frame: 60, position: [4, 5, 6] },
        { name: "全ての親", frame: 90, position: [7, 8, 9] },
      ],
    }),
    writeSyntheticVmd(targetVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 31, position: [101, 102, 103] },
        { name: "全ての親", frame: 61, position: [104, 105, 106] },
        { name: "全ての親", frame: 91, position: [107, 108, 109] },
      ],
    }),
    writeSyntheticVmd(shortTargetVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 31, position: [101, 102, 103] },
        { name: "全ての親", frame: 61, position: [104, 105, 106] },
      ],
    }),
    writeSyntheticVmd(morphTargetVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 31, position: [101, 102, 103] },
        { name: "全ての親", frame: 61, position: [104, 105, 106] },
        { name: "全ての親", frame: 91, position: [107, 108, 109] },
      ],
      morphName: "まばたき",
    }),
  ]);
  await writeFile(
    registryFile,
    JSON.stringify({
      profiles: [
        {
          id: "same-shape",
          kind: "same-shape",
          source: { base: baseFile, variant: donorVariantFile, vmd: donorVmdFile },
          profile: { verified: true, recordByteLength: 62, recordCount: 3, modelSlotContext: { slot: 0 } },
        },
        {
          id: "delta-2-to-3",
          kind: "key-count-delta",
          source: { smallVariant: smallVariantFile, largeVariant: largeVariantFile, smallVmd: smallVmdFile, largeVmd: largeVmdFile },
          profile: { verified: true, recordByteLength: 62, recordCount: 3, modelSlotContext: { slot: 0 } },
        },
      ],
    }),
  );

  const compatible = await readPmmVmdPatchCompatibility({
    registries: [registryFile],
    base: baseFile,
    targetVmd: targetVmdFile,
    targetSlot: 0,
  });
  const incompatible = await readPmmVmdPatchCompatibility({
    registries: [registryFile],
    base: baseFile,
    targetVmd: shortTargetVmdFile,
    targetSlot: 0,
  });
  const unsupported = await readPmmVmdPatchCompatibility({
    registries: [registryFile],
    base: baseFile,
    targetVmd: morphTargetVmdFile,
    targetSlot: 0,
  });

  assert.equal(compatible.ok, true);
  assert.equal(compatible.compatibleProfileCount, 2);
  assert.equal(compatible.selectedProfile.id, "same-shape");
  assert.equal(compatible.selectedProfile.command, "patch-pmm-vmd-diff-cluster");
  assert.equal(incompatible.ok, false);
  assert.equal(incompatible.compatibleProfileCount, 0);
  assert.match(incompatible.incompatibleProfiles.map((profile) => profile.reasons.join(" ")).join(" "), /Target VMD key count 2/);
  assert.equal(unsupported.ok, false);
  assert.deepEqual(unsupported.unsupportedChannels, [{ name: "morphFrames", count: 1 }]);
  assert.match(unsupported.nextRequiredFixtures[0], /morph key/);
});

test("resolves target PMX to a PMM model slot before planning patches", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-target-pmx-"));
  const baseFile = join(dir, "base.pmm");
  const donorVariantFile = join(dir, "same-shape.pmm");
  const donorVmdFile = join(dir, "same-shape.vmd");
  const targetVmdFile = join(dir, "target.vmd");
  const registryFile = join(dir, "registry.json");
  const outFile = join(dir, "patched.pmm");
  const duplicateBaseFile = join(dir, "duplicate-base.pmm");
  await Promise.all([
    writeFile(baseFile, makePmmManifestBytes(["C:\\models\\tda.pmx", "C:\\models\\sour.pmx"])),
    writeFile(duplicateBaseFile, makePmmManifestBytes(["C:\\models-a\\sour.pmx", "C:\\models-b\\sour.pmx"])),
    writeFile(donorVariantFile, Buffer.alloc(1)),
    writeSyntheticVmd(donorVmdFile, {
      boneFrames: [{ name: "全ての親", frame: 30, position: [1, 2, 3] }],
    }),
    writeSyntheticVmd(targetVmdFile, {
      boneFrames: [{ name: "全ての親", frame: 31, position: [101, 102, 103] }],
    }),
  ]);
  await writeFile(
    registryFile,
    JSON.stringify({
      profiles: [
        {
          id: "slot-1-profile",
          kind: "same-shape",
          source: { base: baseFile, variant: donorVariantFile, vmd: donorVmdFile },
          profile: { verified: true, recordByteLength: 62, recordCount: 1, modelSlotContext: { slot: 1 } },
        },
      ],
    }),
  );

  const plan = await readPmmVmdAnyPatchRegistryPlan({
    registries: [registryFile],
    base: baseFile,
    targetPmx: "sour.pmx",
    targetVmd: targetVmdFile,
    out: outFile,
  });
  const compatibility = await readPmmVmdPatchCompatibility({
    registries: [registryFile],
    base: baseFile,
    targetPmx: "sour.pmx",
    targetVmd: targetVmdFile,
  });

  assert.equal(plan.ok, true);
  assert.equal(plan.targetModelSlot.slot, 1);
  assert.equal(plan.candidates[0].command.args.targetSlot, 1);
  assert.equal(compatibility.ok, true);
  assert.equal(compatibility.targetModelSlot.slot, 1);
  assert.equal(compatibility.selectedProfile.modelSlot, 1);
  await assert.rejects(
    readPmmVmdAnyPatchRegistryPlan({
      registries: [registryFile],
      base: duplicateBaseFile,
      targetPmx: "sour.pmx",
      targetVmd: targetVmdFile,
      out: outFile,
    }),
    /ambiguous/,
  );
});

test("patches a PMM key-count delta and rewrites the resulting large-shape keys", () => {
  const prefix = Buffer.alloc(64);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const smallVariant = Buffer.concat([
    makeHeader(60),
    makeKeyBytes({ count: 2, frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    suffix,
  ]);
  const largeVariant = Buffer.concat([
    makeHeader(90),
    makeKeyBytes({ count: 3, frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    makeKeyBytes({ frame: 90, position: [7, 8, 9], rotation: [0, 0.382683, 0, 0.92388] }),
    suffix,
  ]);
  const smallVmd = makeVmd([
    ["全ての親", 30, [1, 2, 3], [0.382683, 0, 0, 0.92388]],
    ["全ての親", 60, [4, 5, 6], [0, 0, 0.382683, 0.92388]],
  ]);
  const largeVmd = makeVmd([
    ["全ての親", 30, [1, 2, 3], [0.382683, 0, 0, 0.92388]],
    ["全ての親", 60, [4, 5, 6], [0, 0, 0.382683, 0.92388]],
    ["全ての親", 90, [7, 8, 9], [0, 0.382683, 0, 0.92388]],
  ]);
  const targetVmd = makeVmd([
    ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["全ての親", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
    ["全ての親", 91, [107, 108, 109], [0.382683, 0, 0, 0.92388]],
  ]);

  const patched = patchPmmVmdKeyCountDeltaProfile(base, smallVariant, largeVariant, smallVmd, largeVmd, targetVmd);

  assert.equal(patched.mode, "pmm-vmd-key-count-delta-patch");
  assert.equal(patched.byteLengthDeltaFromSmall, 62);
  assert.equal(patched.deltaSummary.recordByteDeltaMatchesFileDelta, true);
  assert.equal(patched.scalarRewrites.find((rewrite) => rewrite.kind === "maxFrame")?.value, 91);
  assert.equal(patched.rewriteCount, 3);
  assert.equal(patched.verification.coverage.frameSequenceMatchedBoneFrames, 3);
  assert.equal(patched.verification.coverage.rotationMatchedBoneFrames, 3);
  assert.equal(patched.bytes.readUInt32LE(0x10), 91);
  assert.equal(patched.bytes.readUInt32LE(0x40), 3);
  assert.equal(patched.bytes.readUInt32LE(0x40 + 0x08), 31);
  assert.equal(patched.bytes.readFloatLE(0x40 + 0x24), 101);
  assert.equal(patched.bytes.readUInt32LE(0x40 + 62 * 2 + 0x08), 91);
  assert.equal(patched.bytes.readFloatLE(0x40 + 62 * 2 + 0x24), 107);
});

test("plans and writes PMM key-count delta patches from a profile registry", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-dumper-delta-registry-"));
  const baseFile = join(dir, "base.pmm");
  const smallVariantFile = join(dir, "small.pmm");
  const largeVariantFile = join(dir, "large.pmm");
  const smallVmdFile = join(dir, "small.vmd");
  const largeVmdFile = join(dir, "large.vmd");
  const targetVmdFile = join(dir, "target.vmd");
  const registryFile = join(dir, "registry.json");
  const outFile = join(dir, "patched.pmm");
  const prefix = Buffer.alloc(64);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const smallVariant = Buffer.concat([
    makeHeader(60),
    makeKeyBytes({ count: 2, frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    suffix,
  ]);
  const largeVariant = Buffer.concat([
    makeHeader(90),
    makeKeyBytes({ count: 3, frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    makeKeyBytes({ frame: 90, position: [7, 8, 9], rotation: [0, 0.382683, 0, 0.92388] }),
    suffix,
  ]);
  await Promise.all([
    writeFile(baseFile, base),
    writeFile(smallVariantFile, smallVariant),
    writeFile(largeVariantFile, largeVariant),
    writeSyntheticVmd(smallVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] },
        { name: "全ての親", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] },
      ],
    }),
    writeSyntheticVmd(largeVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] },
        { name: "全ての親", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] },
        { name: "全ての親", frame: 90, position: [7, 8, 9], rotation: [0, 0.382683, 0, 0.92388] },
      ],
    }),
    writeSyntheticVmd(targetVmdFile, {
      boneFrames: [
        { name: "全ての親", frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] },
        { name: "全ての親", frame: 61, position: [104, 105, 106], rotation: [0, 0, -0.382683, 0.92388] },
        { name: "全ての親", frame: 91, position: [107, 108, 109], rotation: [0.382683, 0, 0, 0.92388] },
      ],
    }),
    writeFile(
      registryFile,
      JSON.stringify({
        profiles: [
          {
            id: "delta-2-to-3",
            source: {
              smallVariant: smallVariantFile,
              largeVariant: largeVariantFile,
              smallVmd: smallVmdFile,
              largeVmd: largeVmdFile,
            },
            profile: { verified: true, recordByteLength: 62, recordCount: 3 },
          },
        ],
      }),
    ),
  ]);
  const plan = await planPmmVmdKeyCountDeltaPatchFromProfileRegistry(
    JSON.parse(await readFile(registryFile, "utf8")),
    makeVmd([
      ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
      ["全ての親", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
      ["全ての親", 91, [107, 108, 109], [0.382683, 0, 0, 0.92388]],
    ]),
    { base: baseFile, targetVmdFile, out: outFile },
  );
  const patched = await writePmmVmdKeyCountDeltaPatchFromProfileRegistry({
    registry: registryFile,
    base: baseFile,
    targetVmd: targetVmdFile,
    out: outFile,
  });

  assert.equal(plan.ok, true);
  assert.equal(plan.candidates[0].command.script, "patch-pmm-vmd-key-count-delta");
  assert.equal(patched.ok, true);
  assert.equal(patched.selectedProfile.id, "delta-2-to-3");
  assert.equal(patched.patch.mode, "pmm-vmd-key-count-delta-patch");
  assert.equal(patched.patch.rewriteCount, 3);
  assert.equal(patched.patch.verification.coverage.frameSequenceMatchedBoneFrames, 3);
});

function makeVmd(entries) {
  const bones = entries.map(([name, frame, position, rotation = [0, 0, 0, 1]]) => ({ name, frame, position, rotation }));
  return {
    modelName: "Tda",
    maxFrame: Math.max(...bones.map((bone) => bone.frame)),
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones,
  };
}

function makeHeader(maxFrame) {
  const bytes = Buffer.alloc(64);
  bytes.writeUInt32LE(maxFrame, 0x10);
  return bytes;
}

function makePmmManifestBytes(modelPaths) {
  return Buffer.from(`Polygon Movie maker 0002\0${modelPaths.join("\0")}\0`, "binary");
}

function makeKeyBytes({ count = 0, frame, position, rotation = [0, 0, 0, 1] }) {
  const bytes = Buffer.alloc(62);
  bytes.writeUInt32LE(count, 0);
  bytes.writeUInt32LE(frame, 8);
  bytes.writeFloatLE(rotation[0], 20);
  bytes.writeFloatLE(rotation[1], 24);
  bytes.writeFloatLE(rotation[2], 28);
  bytes.writeFloatLE(rotation[3], 32);
  bytes.writeFloatLE(position[0], 36);
  bytes.writeFloatLE(position[1], 40);
  bytes.writeFloatLE(position[2], 44);
  return bytes;
}
