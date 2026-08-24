import test from "node:test";
import assert from "node:assert/strict";
import { scanPmmMotionBytes } from "../src/pmm-motion-scan.mjs";

test("finds PMM motion marker hits with nearby search values", () => {
  const bytes = Buffer.concat([
    Buffer.from([0, 1, 2]),
    int32le(30),
    float32le(0.5),
    Buffer.from("14146b6b14146b6b", "hex"),
    Buffer.from([3, 4, 5]),
  ]);

  const report = scanPmmMotionBytes(bytes, {
    int32s: [30],
    float32s: [0.5],
    radius: 16,
  });

  assert.equal(report.hits.length, 1);
  assert.equal(report.hits[0].nearbyInt32Matches.length, 1);
  assert.equal(report.hits[0].nearbyFloat32Matches.length, 1);
});

test("accepts custom marker hex", () => {
  const bytes = Buffer.from("0011223344", "hex");
  const report = scanPmmMotionBytes(bytes, { markerHex: ["22 33"], radius: 1 });

  assert.equal(report.hits.length, 1);
  assert.equal(report.hits[0].offset, 2);
});

test("summarizes marker hit strides", () => {
  const marker = Buffer.from("14146b6b14146b6b000000000000000000000000000000000000000000000000", "hex");
  const record = Buffer.concat([marker, Buffer.alloc(26)]);
  const bytes = Buffer.concat([record, record, record]);
  const report = scanPmmMotionBytes(bytes);

  assert.equal(report.hits.length, 3);
  assert.deepEqual(report.strideSummary[0], { stride: 58, count: 2 });
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
