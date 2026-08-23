import test from "node:test";
import assert from "node:assert/strict";
import { analyzePmmBytes } from "../src/pmm-analysis.mjs";

test("finds PMM investigation text, int32, and float32 needles", () => {
  const bytes = Buffer.concat([
    Buffer.from("Polygon Movie maker 9.32\0", "ascii"),
    Buffer.from("Vocaloid Motion Data 0002", "ascii"),
    int32le(30),
    float32le(1),
    float32le(2),
    float32le(3),
    float32le(0.5),
  ]);

  const analysis = analyzePmmBytes(bytes, {
    int32s: [30],
    float32s: [1, 2, 3, 0.5],
  });

  assert.equal(analysis.textMatches[0].matches.length, 1);
  assert.equal(analysis.int32Matches[0].matches.length, 1);
  assert.deepEqual(
    analysis.float32Matches.map((match) => match.matches.length),
    [1, 1, 1, 1],
  );
});

test("uses a match limit per needle", () => {
  const repeated = Buffer.concat([int32le(30), int32le(30), int32le(30)]);
  const analysis = analyzePmmBytes(repeated, { int32s: [30], limit: 2 });

  assert.equal(analysis.int32Matches[0].matches.length, 2);
  assert.equal(analysis.int32Matches[0].truncated, true);
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
