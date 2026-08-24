import test from "node:test";
import assert from "node:assert/strict";
import { inspectVmd } from "../src/vmd-inventory.mjs";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";

test("inspects VMD frame counts and sample fields", () => {
  const bytes = createSyntheticVmd({
    modelName: "Tda",
    boneName: "センター",
    morphName: "まばたき",
    frame: 30,
    position: [1, 2, 3],
    weight: 0.5,
  });

  const inventory = inspectVmd(bytes);

  assert.equal(inventory.header, "Vocaloid Motion Data 0002");
  assert.equal(inventory.modelName, "Tda");
  assert.deepEqual(inventory.counts, {
    boneFrames: 1,
    morphFrames: 1,
    cameraFrames: 0,
    lightFrames: 0,
    selfShadowFrames: 0,
    propertyFrames: 0,
  });
  assert.equal(inventory.maxFrame, 30);
  assert.equal(inventory.bones[0].name, "センター");
  assert.deepEqual(inventory.bones[0].position, [1, 2, 3]);
  assert.equal(inventory.morphs[0].weight, 0.5);
});
