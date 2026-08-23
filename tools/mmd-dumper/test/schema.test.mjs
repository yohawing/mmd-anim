import test from "node:test";
import assert from "node:assert/strict";
import { assertOracleRecord } from "../src/schema.mjs";

test("accepts a minimal oracle record", () => {
  const record = {
    schemaVersion: 1,
    source: { mmdVersion: "9.32-x64", dumperVersion: "0.1.0" },
    frame: 0,
    models: [
      {
        index: 0,
        name: "model",
        filename: "model.pmd",
        visible: true,
        bones: [{ index: 0, name: "センター", worldMatrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1] }],
        morphs: [{ index: 0, name: "まばたき", weight: 0 }],
      },
    ],
  };

  assert.equal(assertOracleRecord(record), record);
});

test("accepts oracle camera and scene parameter fields", () => {
  const interpolation = { x1: 20, y1: 20, x2: 107, y2: 107 };
  const record = {
    schemaVersion: 1,
    source: { mmdVersion: "9.32-x64", dumperVersion: "0.1.0", project: "scene.pmm" },
    frame: 30,
    camera: {
      available: true,
      current: { distance: -45, position: [1, 2, 3], rotation: [0.1, 0.2, 0.3] },
      keyframes: [
        {
          index: 0,
          frame: 30,
          previousKeyframeIndex: 0,
          nextKeyframeIndex: 0,
          distance: -45,
          position: [1, 2, 3],
          rotation: [0.1, 0.2, 0.3],
          fov: 30,
          perspective: true,
          selected: false,
          followModelIndex: -1,
          followBoneIndex: 0,
          interpolation: {
            x: interpolation,
            y: interpolation,
            z: interpolation,
            rotation: interpolation,
            distance: interpolation,
            fov: interpolation,
          },
        },
      ],
    },
    sceneParameters: {
      available: true,
      outputWidth: 1024,
      outputHeight: 768,
      pmmPath: "scene.pmm",
    },
    models: [],
  };

  assert.equal(assertOracleRecord(record), record);
});

test("rejects non-finite matrix values", () => {
  assert.throws(
    () =>
      assertOracleRecord({
        schemaVersion: 1,
        source: { mmdVersion: "9.32-x64", dumperVersion: "0.1.0" },
        frame: 0,
        models: [
          {
            index: 0,
            name: "model",
            filename: "model.pmd",
            visible: true,
            bones: [{ index: 0, name: "センター", worldMatrix: [Infinity, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }],
            morphs: [],
          },
        ],
      }),
    /worldMatrix\[0\] must be finite/,
  );
});
