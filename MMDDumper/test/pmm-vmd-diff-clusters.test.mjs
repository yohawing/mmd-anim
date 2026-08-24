import test from "node:test";
import assert from "node:assert/strict";
import { analyzePmmVmdDiffClusters } from "../src/pmm-vmd-diff-clusters.mjs";

test("clusters VMD bone positions inside a PMM changed middle", () => {
  const prefix = Buffer.from([1, 2, 3, 4]);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    makeKeyBytes({ frame: 30, position: [7, 8, 9] }),
    makeKeyBytes({ frame: 60, position: [10, 11, 12] }),
    suffix,
  ]);
  const report = analyzePmmVmdDiffClusters(base, variant, {
    modelName: "Tda",
    maxFrame: 60,
    counts: { boneFrames: 4 },
    bones: [
      { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
      { name: "センター", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
      { name: "左足", frame: 30, position: [7, 8, 9], rotation: [0, 0, 0, 1] },
      { name: "左足", frame: 60, position: [10, 11, 12], rotation: [0, 0, 0, 1] },
    ],
  });

  assert.equal(report.diff.byteLengthDelta, 246);
  assert.equal(report.coverage.matchedBoneFrames, 4);
  assert.equal(report.coverage.exactPositionClusterRatio, 1);
  assert.equal(report.positionKeyBlockProfile.verified, true);
  assert.equal(report.positionKeyBlockProfile.recordByteLength, 62);
  assert.equal(report.positionKeyBlockProfile.frameOffsetInRecord, 8);
  assert.equal(report.positionKeyBlockProfile.positionOffsetInRecord, 36);
  assert.equal(report.positionKeyBlockProfile.recordCount, 4);
  assert.equal(report.frameSequenceBlockProfile.verified, true);
  assert.equal(report.frameSequenceBlockProfile.recordCount, 4);
  assert.equal(report.coverage.frameSequenceMatchedBoneFrames, 4);
  assert.deepEqual(
    report.positionKeyBlockProfile.boneSpans.map((span) => [span.name, span.recordCount, span.spanStartHex, span.spanEndHex]),
    [
      ["センター", 2, "0x4", "0x80"],
      ["左足", 2, "0x80", "0xfc"],
    ],
  );
  assert.equal(report.perBone.length, 2);
  assert.deepEqual(
    report.perBone.map((bone) => [bone.name, bone.matchedFrameCount, bone.estimatedStrideSummary[0]?.stride]),
    [
      ["センター", 2, 62],
      ["左足", 2, 62],
    ],
  );
});

test("finds a full frame sequence block when non-movable bone positions are zeroed", () => {
  const prefix = Buffer.from([1, 2, 3, 4]);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    makeKeyBytes({ frame: 90, position: [7, 8, 9] }),
    makeKeyBytes({ frame: 30, position: [0, 0, 0] }),
    makeKeyBytes({ frame: 60, position: [0, 0, 0] }),
    makeKeyBytes({ frame: 90, position: [0, 0, 0] }),
    suffix,
  ]);
  const report = analyzePmmVmdDiffClusters(base, variant, {
    modelName: "Tda",
    maxFrame: 90,
    counts: { boneFrames: 6 },
    bones: [
      { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
      { name: "センター", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
      { name: "センター", frame: 90, position: [7, 8, 9], rotation: [0, 0, 0, 1] },
      { name: "左足", frame: 30, position: [10, 11, 12], rotation: [0, 0, 0, 1] },
      { name: "左足", frame: 60, position: [13, 14, 15], rotation: [0, 0, 0, 1] },
      { name: "左足", frame: 90, position: [16, 17, 18], rotation: [0, 0, 0, 1] },
    ],
  });

  assert.equal(report.coverage.matchedBoneFrames, 3);
  assert.equal(report.coverage.frameSequenceMatchedBoneFrames, 6);
  assert.equal(report.frameSequenceBlockProfile.verified, true);
  assert.deepEqual(
    report.frameSequenceBlockProfile.boneSpans.map((span) => [span.name, span.recordCount, span.spanStartHex, span.spanEndHex]),
    [
      ["センター", 3, "0x4", "0xbe"],
      ["左足", 3, "0xbe", "0x178"],
    ],
  );
});

test("profiles non-identity rotations inside frame sequence records", () => {
  const prefix = Buffer.from([1, 2, 3, 4]);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    suffix,
  ]);
  const report = analyzePmmVmdDiffClusters(base, variant, {
    modelName: "Tda",
    maxFrame: 60,
    counts: { boneFrames: 2 },
    bones: [
      { name: "全ての親", frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] },
      { name: "全ての親", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] },
    ],
  });

  assert.equal(report.coverage.nonIdentityRotationFrames, 2);
  assert.equal(report.coverage.rotationMatchedBoneFrames, 2);
  assert.equal(report.coverage.rotationClusterRatio, 1);
  assert.equal(report.transformKeyBlockProfile.verified, true);
  assert.equal(report.transformKeyBlockProfile.recordByteLength, 62);
  assert.equal(report.transformKeyBlockProfile.frameOffsetInRecord, 8);
  assert.equal(report.transformKeyBlockProfile.rotationOffsetInRecord, 20);
});

test("infers model slot context from the last model path before a motion block", () => {
  const prefix = Buffer.alloc(100);
  const middlePadding = Buffer.alloc(100);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, middlePadding, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    middlePadding,
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    suffix,
  ]);
  const report = analyzePmmVmdDiffClusters(
    base,
    variant,
    {
      modelName: "Tda",
      maxFrame: 60,
      counts: { boneFrames: 2 },
      bones: [
        { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
        { name: "センター", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
      ],
    },
    {
      modelSlots: [
        { slot: 0, path: "model-a.pmx", fileName: "model-a.pmx", offset: 20 },
        { slot: 1, path: "model-b.pmx", fileName: "model-b.pmx", offset: 120 },
      ],
    },
  );

  assert.equal(report.positionKeyBlockProfile.blockStart, 200);
  assert.equal(report.positionKeyBlockProfile.modelSlotContext.inferred, true);
  assert.equal(report.positionKeyBlockProfile.modelSlotContext.slot, 1);
  assert.equal(report.frameSequenceBlockProfile.modelSlotContext.slot, 1);
});

test("keeps first-slot context when a later model path shifts after an inserted block", () => {
  const prefix = Buffer.alloc(100);
  const suffix = Buffer.alloc(32);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 30, position: [1, 2, 3] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6] }),
    suffix,
  ]);
  const report = analyzePmmVmdDiffClusters(
    base,
    variant,
    {
      modelName: "Tda",
      maxFrame: 60,
      counts: { boneFrames: 2 },
      bones: [
        { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
        { name: "センター", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
      ],
    },
    {
      modelSlots: [
        { slot: 0, path: "tda.pmx", fileName: "tda.pmx", offset: 20 },
        { slot: 1, path: "sour.pmx", fileName: "sour.pmx", offset: 500 },
      ],
    },
  );

  assert.equal(report.positionKeyBlockProfile.blockStart, 100);
  assert.equal(report.positionKeyBlockProfile.modelSlotContext.slot, 0);
});

function makeKeyBytes({ frame, position, rotation = [0, 0, 0, 1] }) {
  const bytes = Buffer.alloc(62);
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
