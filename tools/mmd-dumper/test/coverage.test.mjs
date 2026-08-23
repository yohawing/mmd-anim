import test from "node:test";
import assert from "node:assert/strict";
import { verifyOracleCoverage, verifyOraclePlaybackCoverage } from "../src/coverage.mjs";

test("accepts target frames with non-empty runtime model channels", () => {
  const report = verifyOracleCoverage({
    frames: [0, 30],
    records: [record(0, 2, 1), record(30.0001, 2, 1)],
    frameEpsilon: 0.01,
    requireBones: true,
    requireMorphs: true,
  });

  assert.equal(report.ok, true);
  assert.equal(report.targets[1].matchedFrame, 30.0001);
});

test("rejects target frames with empty bone or morph channels", () => {
  const report = verifyOracleCoverage({
    frames: [0],
    records: [record(0, 0, 1)],
    requireBones: true,
    requireMorphs: true,
  });

  assert.equal(report.ok, false);
  assert.equal(report.targets[0].ok, false);
});

test("accepts camera-only target frames without runtime model channels", () => {
  const report = verifyOracleCoverage({
    frames: [0, 1],
    records: [cameraRecord(0), cameraRecord(1.0001)],
    frameEpsilon: 0.01,
    requireBones: false,
    requireMorphs: false,
    requireCamera: true,
  });

  assert.equal(report.ok, true);
  assert.equal(report.targets[0].models.length, 0);
  assert.equal(report.targets[0].camera.available, true);
  assert.equal(report.targets[0].camera.current, true);
});

test("rejects camera-only target frames without current camera state", () => {
  const report = verifyOracleCoverage({
    frames: [0],
    records: [{ ...cameraRecord(0), camera: { available: true } }],
    requireBones: false,
    requireMorphs: false,
    requireCamera: true,
  });

  assert.equal(report.ok, false);
  assert.equal(report.targets[0].ok, false);
});

test("accepts playback camera coverage by observed frame span", () => {
  const report = verifyOraclePlaybackCoverage({
    frames: [0, 1, 2, 3, 4, 5],
    records: [cameraRecord(0), cameraRecord(2.4), cameraRecord(4.1)],
    playbackToleranceFrames: 1,
    requireCamera: true,
  });

  assert.equal(report.ok, true);
  assert.equal(report.mode, "playback");
  assert.equal(report.camera.records, 3);
});

test("rejects playback camera coverage that does not reach the expected span", () => {
  const report = verifyOraclePlaybackCoverage({
    frames: [0, 1, 2, 3, 4, 5],
    records: [cameraRecord(0), cameraRecord(2.4)],
    playbackToleranceFrames: 1,
    requireCamera: true,
  });

  assert.equal(report.ok, false);
});

function record(frame, boneCount, morphCount) {
  return {
    schemaVersion: 1,
    source: { mmdVersion: "9.32-x64", dumperVersion: "0.1.0" },
    frame,
    models: [
      {
        index: 0,
        name: "model.pmx",
        filename: "model.pmx",
        visible: true,
        bones: Array.from({ length: boneCount }, (_, index) => ({
          index,
          name: `bone-${index}`,
          worldMatrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
        })),
        morphs: Array.from({ length: morphCount }, (_, index) => ({
          index,
          name: `morph-${index}`,
          weight: 0,
        })),
      },
    ],
  };
}

function cameraRecord(frame) {
  return {
    schemaVersion: 1,
    source: { mmdVersion: "9.32-x64", dumperVersion: "0.1.0" },
    frame,
    models: [],
    camera: {
      available: true,
      current: {
        distance: 45,
        position: [0, 10, 0],
        rotation: [0, 0, 0],
        fov: 30,
      },
    },
  };
}
