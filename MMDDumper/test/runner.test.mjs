import test from "node:test";
import assert from "node:assert/strict";
import { selectRecordsForFrames } from "../src/runner.mjs";

test("selectRecordsForFrames applies capture offset and relabels records", () => {
  const records = [
    { frame: 1, payload: "pose-0" },
    { frame: 120, payload: "pose-119" },
    { frame: 301.00001, payload: "pose-300" },
  ];

  assert.deepEqual(selectRecordsForFrames(records, [0, 119, 300], 1), [
    { frame: 0, payload: "pose-0" },
    { frame: 119, payload: "pose-119" },
    { frame: 300, payload: "pose-300" },
  ]);
});

test("selectRecordsForFrames reports logical frames when offset captures are missing", () => {
  assert.throws(
    () => selectRecordsForFrames([{ frame: 1 }], [0, 119], 1),
    /requested frame\(s\): 119/,
  );
});
