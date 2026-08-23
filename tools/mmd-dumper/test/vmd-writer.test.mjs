import test from "node:test";
import assert from "node:assert/strict";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";

test("creates a minimal VMD with one bone and one morph frame", () => {
  const bytes = createSyntheticVmd({
    modelName: "Tda",
    boneName: "センター",
    morphName: "まばたき",
    frame: 30,
    position: [1, 2, 3],
    weight: 0.5,
  });

  assert.equal(bytes.subarray(0, 25).toString("ascii"), "Vocaloid Motion Data 0002");
  assert.equal(bytes.readUInt32LE(50), 1);
  assert.equal(bytes.readUInt32LE(69), 30);
  assert.equal(bytes.readFloatLE(73), 1);
  assert.equal(bytes.readFloatLE(77), 2);
  assert.equal(bytes.readFloatLE(81), 3);
  assert.equal(bytes.readUInt32LE(165), 1);
  assert.equal(bytes.readUInt32LE(184), 30);
  assert.equal(bytes.readFloatLE(188), 0.5);
});

test("creates a VMD with multiple bone position frames", () => {
  const bytes = createSyntheticVmd({
    modelName: "Tda",
    boneName: "センター",
    boneFrames: [
      { frame: 30, position: [1, 2, 3] },
      { frame: 60, position: [4, 5, 6] },
      { frame: 90, position: [7, 8, 9] },
      { frame: 120, position: [10, 11, 12] },
    ],
  });

  assert.equal(bytes.readUInt32LE(50), 4);
  assert.equal(bytes.readUInt32LE(69), 30);
  assert.equal(bytes.readFloatLE(73), 1);
  assert.equal(bytes.readUInt32LE(69 + 111), 60);
  assert.equal(bytes.readFloatLE(73 + 111), 4);
  assert.equal(bytes.readUInt32LE(69 + 222), 90);
  assert.equal(bytes.readFloatLE(73 + 222), 7);
  assert.equal(bytes.readUInt32LE(69 + 333), 120);
  assert.equal(bytes.readFloatLE(73 + 333), 10);
  assert.equal(bytes.readUInt32LE(50 + 4 + 111 * 4), 0);
});

test("creates a VMD with multiple named bones", () => {
  const bytes = createSyntheticVmd({
    modelName: "Tda",
    boneFrames: [
      { name: "センター", frame: 30, position: [1, 2, 3] },
      { name: "左足", frame: 60, position: [7, 8, 9] },
    ],
  });

  assert.equal(bytes.readUInt32LE(50), 2);
  assert.equal(bytes.readUInt32LE(69), 30);
  assert.equal(bytes.readFloatLE(73), 1);
  assert.equal(bytes.readUInt32LE(69 + 111), 60);
  assert.equal(bytes.readFloatLE(73 + 111), 7);
});

test("rejects text that does not fit fixed VMD fields", () => {
  assert.throws(
    () =>
      createSyntheticVmd({
        boneName: "this bone name is definitely too long",
      }),
    /too long/,
  );
});
