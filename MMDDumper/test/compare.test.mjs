import test from "node:test";
import assert from "node:assert/strict";
import { compareOracleRecords } from "../src/compare.mjs";
import { createFakeRecord } from "../src/fake-record.mjs";

const fixture = {
  mmdVersion: "9.32-x64",
  projectRaw: "scene.pmm",
  dump: { bones: true, morphs: true },
};

test("passes identical records", () => {
  const expected = [createFakeRecord(fixture, 0), createFakeRecord(fixture, 30)];
  const actual = [createFakeRecord(fixture, 0), createFakeRecord(fixture, 30)];

  assert.deepEqual(compareOracleRecords(expected, actual).differences, []);
});

test("reports matrix and morph differences", () => {
  const expected = [createFakeRecord(fixture, 60)];
  const actual = [createFakeRecord(fixture, 60)];
  actual[0].models[0].bones[0].worldMatrix[12] += 0.5;
  actual[0].models[0].morphs[0].weight -= 0.25;

  const report = compareOracleRecords(expected, actual, { matrixEpsilon: 0.01, morphEpsilon: 0.01 });

  assert.equal(report.ok, false);
  assert.equal(report.differences.length, 2);
  assert.equal(report.differences[0].kind, "bone-world-matrix");
  assert.equal(report.differences[1].kind, "morph-weight");
});
