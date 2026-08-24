import test from "node:test";
import assert from "node:assert/strict";
import { diffPmmBytes } from "../src/pmm-diff.mjs";

test("summarizes changed byte ranges with common prefix and suffix", () => {
  const left = Buffer.from([1, 2, 3, 4, 5, 6, 7]);
  const right = Buffer.from([1, 2, 9, 9, 5, 6, 7]);

  const diff = diffPmmBytes(left, right, { context: 1 });

  assert.equal(diff.commonPrefixLength, 2);
  assert.equal(diff.commonSuffixLength, 3);
  assert.equal(diff.changedRanges.length, 1);
  assert.equal(diff.changedRanges[0].start, 2);
  assert.equal(diff.changedRanges[0].byteLength, 2);
});

test("reports file size delta and limits changed ranges", () => {
  const left = Buffer.from([1, 2, 3, 4, 5]);
  const right = Buffer.from([1, 9, 3, 9, 5, 6]);

  const diff = diffPmmBytes(left, right, { limit: 1 });

  assert.equal(diff.byteLengthDelta, 1);
  assert.equal(diff.changedRanges.length, 1);
  assert.equal(diff.truncated, true);
});
