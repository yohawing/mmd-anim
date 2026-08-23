import test from "node:test";
import assert from "node:assert/strict";
import { comparePmmDocumentToVmd } from "../src/pmm-document-vmd-compare.mjs";
import { inspectVmd } from "../src/vmd-inventory.mjs";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";

test("compares PMM camera values and interpolation against VMD camera frames", () => {
  const interpolation0 = [
    10, 20, 30, 40,
    11, 21, 31, 41,
    12, 22, 32, 42,
    13, 23, 33, 43,
    14, 24, 34, 44,
    15, 25, 35, 45,
  ];
  const interpolation45 = interpolation0.map((value) => value + 1);
  const vmd = inspectVmd(
    createSyntheticVmd({
      cameraFrames: [
        {
          frame: 0,
          distance: -30,
          position: [1, 2, 3],
          rotation: [0.1, 0.2, 0.3],
          interpolation: interpolation0,
          fov: 20,
          perspective: 0,
        },
        {
          frame: 45,
          distance: -15,
          position: [4, 5, 6],
          rotation: [0.4, 0.5, 0.6],
          interpolation: interpolation45,
          fov: 35,
          perspective: 1,
        },
      ],
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );
  const pmm = createPmmWithCamera({ interpolation0, interpolation45 });

  const comparison = comparePmmDocumentToVmd(pmm, vmd);

  assert.equal(comparison.ok, true);
  assert.deepEqual(comparison.unsupportedChannels, []);
  assert.equal(comparison.counts.mismatches, 0);
  assert.equal(comparison.cameraComparison.matched, 2);
  assert.equal(comparison.cameraComparison.actual, 2);
  assert.equal(comparison.cameraComparison.expected, 2);
});

test("reports camera value and interpolation mismatches as structured differences", () => {
  const interpolation0 = Array.from({ length: 24 }, (_, index) => index + 1);
  const interpolation45 = interpolation0.map((value) => value + 1);
  const vmd = inspectVmd(
    createSyntheticVmd({
      cameraFrames: [
        { frame: 0, distance: -30, position: [1, 2, 3], rotation: [0.1, 0.2, 0.3], interpolation: interpolation0, fov: 20, perspective: 0 },
        { frame: 45, distance: -15, position: [4, 5, 6], rotation: [0.4, 0.5, 0.6], interpolation: interpolation45, fov: 35, perspective: 1 },
      ],
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );
  const pmm = createPmmWithCamera({ interpolation0, interpolation45 });
  pmm.camera.keyframes[0].distance = -14;
  pmm.camera.keyframes[0].interpolation[3][3] += 1;

  const comparison = comparePmmDocumentToVmd(pmm, vmd);
  const mismatch = comparison.cameraComparison.mismatches[0];

  assert.equal(comparison.ok, false);
  assert.equal(comparison.counts.mismatches, 1);
  assert.equal(mismatch.kind, "camera");
  assert.equal(mismatch.key, "45");
  assert.equal(mismatch.reason, "CAMERA_KEYFRAME_MISMATCH");
  assert.equal(mismatch.distanceDiff, 1);
  assert.equal(mismatch.interpolationMismatch, true);
  assert.equal(mismatch.pmm.distance, -14);
  assert.equal(mismatch.vmd.distance, -15);
  assert.deepEqual(mismatch.vmd.interpolation, [
    [2, 3, 4, 5],
    [6, 7, 8, 9],
    [10, 11, 12, 13],
    [14, 15, 16, 17],
    [18, 19, 20, 21],
    [22, 23, 24, 25],
  ]);
});

test("prefers an explicit PMM camera frame zero over the initial keyframe", () => {
  const interpolation0 = Array.from({ length: 24 }, (_, index) => index + 1);
  const interpolation45 = interpolation0.map((value) => value + 1);
  const vmd = inspectVmd(
    createSyntheticVmd({
      cameraFrames: [
        { frame: 0, distance: -30, position: [1, 2, 3], rotation: [0.1, 0.2, 0.3], interpolation: interpolation0, fov: 20, perspective: 0 },
        { frame: 45, distance: -15, position: [4, 5, 6], rotation: [0.4, 0.5, 0.6], interpolation: interpolation45, fov: 35, perspective: 1 },
      ],
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );
  const pmm = createPmmWithCamera({ interpolation0, interpolation45 });
  pmm.camera.keyframes.unshift({ ...pmm.camera.initialKeyframe });
  pmm.camera.initialKeyframe.distance = -999;

  const comparison = comparePmmDocumentToVmd(pmm, vmd);

  assert.equal(comparison.ok, true);
  assert.equal(comparison.cameraComparison.actual, 2);
  assert.equal(comparison.cameraComparison.matched, 2);
});

test("fails closed on non-finite and malformed camera values", () => {
  const interpolation0 = Array.from({ length: 24 }, (_, index) => index + 1);
  const interpolation45 = interpolation0.map((value) => value + 1);
  const vmd = inspectVmd(
    createSyntheticVmd({
      cameraFrames: [
        { frame: 0, distance: -30, position: [1, 2, 3], rotation: [0.1, 0.2, 0.3], interpolation: interpolation0, fov: 20, perspective: 0 },
        { frame: 45, distance: -15, position: [4, 5, 6], rotation: [0.4, 0.5, 0.6], interpolation: interpolation45, fov: 35, perspective: 1 },
      ],
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );
  const pmm = createPmmWithCamera({ interpolation0, interpolation45 });
  pmm.camera.keyframes[0].distance = Number.NaN;
  pmm.camera.keyframes[0].position = [4, 5];

  const comparison = comparePmmDocumentToVmd(pmm, vmd);
  const mismatch = comparison.cameraComparison.mismatches[0];

  assert.equal(comparison.ok, false);
  assert.equal(mismatch.reason, "CAMERA_KEYFRAME_INVALID");
  assert.deepEqual(mismatch.invalidFields, ["pmm.distance", "pmm.position"]);
});

function createPmmWithCamera({ interpolation0, interpolation45 }) {
  return {
    document: { version: "0002" },
    models: [
      {
        slot: 0,
        nameJa: "テストモデル",
        path: "model.pmx",
        counts: { boneKeyframes: 0, morphKeyframes: 0 },
        boneKeyframes: [],
        initialBoneKeyframes: [],
        morphKeyframes: [],
        initialMorphKeyframes: [],
      },
    ],
    camera: {
      initialKeyframe: {
        frame: 0,
        distance: -30,
        position: [1, 2, 3],
        rotation: [0.1, 0.2, 0.3],
        interpolation: toInterpolation(interpolation0),
        fov: 20,
        perspective: true,
      },
      keyframes: [
        {
          frame: 45,
          distance: -15,
          position: [4, 5, 6],
          rotation: [0.4, 0.5, 0.6],
          interpolation: toInterpolation(interpolation45),
          fov: 35,
          perspective: false,
        },
      ],
    },
  };
}

function toInterpolation(values) {
  return Array.from({ length: 6 }, (_, index) => values.slice(index * 4, index * 4 + 4));
}
