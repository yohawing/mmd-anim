import test from "node:test";
import assert from "node:assert/strict";
import { analyzePmmKeyCountDelta } from "../src/pmm-key-count-delta.mjs";

test("reports key-count-changing scalar candidates around a resized motion block", () => {
  const blockStart = 64;
  const suffix = Buffer.from([9, 8, 7, 6, 5, 4]);
  const base = Buffer.concat([makeHeader({ maxFrame: 0, cacheA: 0 }), Buffer.from([0xaa, 0xbb]), suffix]);
  const small = Buffer.concat([
    makeHeader({ maxFrame: 60, cacheA: 0xf1 }),
    makeKeyBytes({ count: 2, frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    suffix,
  ]);
  const large = Buffer.concat([
    makeHeader({ maxFrame: 90, cacheA: 0xf2 }),
    makeKeyBytes({ count: 3, frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] }),
    makeKeyBytes({ frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] }),
    makeKeyBytes({ frame: 90, position: [7, 8, 9], rotation: [0, 0.382683, 0, 0.92388] }),
    suffix,
  ]);
  const report = analyzePmmKeyCountDelta(base, small, large, makeVmd(2, 60), makeVmd(3, 90), {
    diffLimit: 32,
  });

  assert.equal(report.summary.smallKeyCount, 2);
  assert.equal(report.summary.largeKeyCount, 3);
  assert.equal(report.summary.recordByteLength, 62);
  assert.equal(report.summary.expectedRecordByteDelta, 62);
  assert.equal(report.summary.actualByteLengthDelta, 62);
  assert.equal(report.summary.recordByteDeltaMatchesFileDelta, true);
  assert.equal(report.summary.blockStart, blockStart);
  assert.equal(report.summary.blockExpansionByteLength, 62);
  assert.deepEqual(
    report.scalarCandidates.maxFrame.map((candidate) => candidate.offsetHex),
    ["0x10"],
  );
  assert.deepEqual(
    report.scalarCandidates.keyCount.map((candidate) => candidate.offsetHex),
    ["0x40"],
  );
  assert.equal(report.scalarCandidates.changedBeforeBlock[0].offsetHex, "0x10");
  assert.equal(report.scalarCandidates.changedBeforeBlock[1].offsetHex, "0x20");
  assert.equal(report.coverage.small.rotationMatchedBoneFrames, 2);
  assert.equal(report.coverage.large.rotationMatchedBoneFrames, 3);
});

function makeHeader({ maxFrame, cacheA }) {
  const bytes = Buffer.alloc(64);
  bytes.writeUInt32LE(maxFrame, 0x10);
  bytes.writeUInt32LE(cacheA, 0x20);
  return bytes;
}

function makeKeyBytes({ count = 0, frame, position, rotation }) {
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

function makeVmd(count, maxFrame) {
  const frames = [
    { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] },
    { name: "センター", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] },
    { name: "センター", frame: 90, position: [7, 8, 9], rotation: [0, 0.382683, 0, 0.92388] },
  ].slice(0, count);
  return {
    modelName: "Tda",
    maxFrame,
    counts: { boneFrames: count },
    bones: frames,
  };
}
