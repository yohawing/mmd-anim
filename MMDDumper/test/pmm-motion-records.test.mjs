import test from "node:test";
import assert from "node:assert/strict";
import { extractPmmMotionRecordSlice, extractPmmMotionRecords, patchPmmMotionRecordSlice } from "../src/pmm-motion-records.mjs";

test("extracts 58-byte PMM motion candidate records from marker offsets", () => {
  const record = Buffer.concat([
    float32le(1),
    float32le(2),
    float32le(3),
    uint32le(1),
    uint32le(30),
    Buffer.alloc(6),
    Buffer.from("14146b6b14146b6b14146b6b14146b6b", "hex"),
    Buffer.alloc(16),
  ]);
  const bytes = Buffer.concat([Buffer.from([9, 9]), record, record]);

  const report = extractPmmMotionRecords(bytes);

  assert.equal(record.byteLength, 58);
  assert.equal(report.records.length, 2);
  assert.equal(report.records[0].recordStart, 2);
  assert.equal(report.records[0].markerHex, "14146b6b14146b6b");
  assert.deepEqual(report.records[0].candidateFields.float32x3, [1, 2, 3]);
  assert.equal(report.records[0].candidateFields.flagU32At12, 1);
  assert.equal(report.records[0].candidateFields.u32At16, 30);
  assert.equal(report.records[0].f32le[0].value, 1);
  assert.deepEqual(report.strideSummary[0], { stride: 58, count: 1 });
  assert.deepEqual(report.summary.fullStrideSummary[0], { stride: 58, count: 1 });
  assert.deepEqual(report.summary.contiguousRuns[0], { recordStart: 2, recordStartHex: "0x2", recordEnd: 118, recordEndHex: "0x76", count: 2 });
});

test("decodes unaligned frame and index fields in 62-byte PMM motion candidates", () => {
  const record = Buffer.concat([
    float32le(0.92388),
    float32le(0.382683),
    uint16le(1),
    uint16le(4),
    uint32le(39 << 16),
    uint32le(3 << 16),
    uint16le(0),
    uint16le(5),
    uint16le(0),
    Buffer.from("14146b6b14146b6b14146b6b2a00557f000000000000000000000080", "hex"),
    Buffer.alloc(8),
  ]);

  const report = extractPmmMotionRecords(record, { recordByteLength: 62 });

  assert.equal(record.byteLength, 62);
  assert.equal(report.records[0].candidateFields.u16At8, 1);
  assert.equal(report.records[0].candidateFields.u16At10, 4);
  assert.equal(report.records[0].candidateFields.flagU32At12, 39 << 16);
  assert.equal(report.records[0].candidateFields.frameAt14, 39);
  assert.equal(report.records[0].candidateFields.u32At16, 3 << 16);
  assert.equal(report.records[0].candidateFields.u16At18, 3);
  assert.equal(report.records[0].candidateFields.u16At22, 5);
  assert.deepEqual(report.summary.candidateFields.frameAt14[0], { value: "39", count: 1 });
});

test("extracts requested interpolation marker variants in file order", () => {
  const first = recordWithMarker("14146b6b14146b6b14146b6b14146b6b");
  const second = recordWithMarker("31313131363636363131313136363636");
  const report = extractPmmMotionRecords(Buffer.concat([first, second]), {
    markerHex: "14146b6b14146b6b,3131313136363636",
  });

  assert.deepEqual(
    report.records.map((record) => record.markerHex),
    ["14146b6b14146b6b", "3131313136363636"],
  );
  assert.deepEqual(report.strideSummary[0], { stride: 58, count: 1 });
});

test("extracts a contiguous PMM motion record byte slice", () => {
  const bytes = Buffer.from([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

  const slice = extractPmmMotionRecordSlice(bytes, {
    recordStart: 2,
    count: 2,
    recordByteLength: 3,
  });

  assert.equal(slice.byteStartHex, "0x2");
  assert.equal(slice.byteEndHex, "0x8");
  assert.deepEqual([...slice.bytes], [2, 3, 4, 5, 6, 7]);
});

test("patches a contiguous PMM motion record byte slice without resizing", () => {
  const bytes = Buffer.from([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

  const patched = patchPmmMotionRecordSlice(bytes, Buffer.from([20, 21, 22, 23, 24, 25]), {
    recordStart: 2,
    count: 2,
    recordByteLength: 3,
  });

  assert.equal(patched.replacedByteLength, 6);
  assert.deepEqual([...patched.bytes], [0, 1, 20, 21, 22, 23, 24, 25, 8, 9]);
});

function recordWithMarker(markerHex) {
  return Buffer.concat([
    Buffer.alloc(12),
    uint32le(1),
    uint32le(30),
    Buffer.alloc(6),
    Buffer.from(markerHex, "hex"),
    Buffer.alloc(16),
  ]);
}

function uint32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value, 0);
  return bytes;
}

function uint16le(value) {
  const bytes = Buffer.alloc(2);
  bytes.writeUInt16LE(value, 0);
  return bytes;
}

function float32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeFloatLE(value, 0);
  return bytes;
}
