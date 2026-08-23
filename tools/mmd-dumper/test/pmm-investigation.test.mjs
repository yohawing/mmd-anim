import test from "node:test";
import assert from "node:assert/strict";
import { investigatePmmBytes } from "../src/pmm-investigation.mjs";

test("keeps only value matches that overlap changed ranges", () => {
  const left = Buffer.concat([int32le(30), Buffer.from([1, 2, 3, 4]), float32le(0.5)]);
  const right = Buffer.concat([int32le(30), Buffer.from([9, 9, 9, 9]), float32le(0.5)]);

  const report = investigatePmmBytes(left, right, {
    int32s: [30],
    float32s: [0.5],
  });

  assert.equal(report.diff.changedRanges.length, 1);
  assert.equal(report.changedInt32Matches[0].changedMatchCount, 0);
  assert.equal(report.changedFloat32Matches[0].changedMatchCount, 0);
});

test("attaches changed range metadata to overlapping matches", () => {
  const left = Buffer.concat([Buffer.from([1, 2]), float32le(1), Buffer.from([3, 4])]);
  const right = Buffer.concat([Buffer.from([1, 2]), float32le(2), Buffer.from([3, 4])]);

  const report = investigatePmmBytes(left, right, {
    float32s: [2],
  });

  assert.equal(report.changedFloat32Matches[0].changedMatchCount, 1);
  assert.equal(report.changedFloat32Matches[0].matches[0].changedRange.index, 0);
});

function int32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeInt32LE(value, 0);
  return bytes;
}

function float32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeFloatLE(value, 0);
  return bytes;
}
