import test from "node:test";
import assert from "node:assert/strict";
import {
  mapAllVmdBoneFramesToPmmRecords,
  mapVmdBoneFrameCoverageToPmmRecords,
  mapVmdBoneFramesToPmmRecords,
} from "../src/pmm-vmd-bone-map.mjs";

test("maps VMD bone frames to marker-derived PMM records", () => {
  const report = mapVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: [
        { name: "bone", frame: 9, position: [1, 2, 3], rotation: [-0.382683, 0, 0, 0.92388] },
        { name: "bone", frame: 19, position: [0, 0, 0], rotation: [0, 0, 0, 1] },
      ],
    },
    {
      byteLength: 200,
      recordByteLength: 62,
      recordTotal: 2,
      summary: {},
      truncated: false,
      records: [createRecord(100, 9, [-0.382683, 0.92388]), createRecord(162, 19, [0, 1])],
    },
  );

  assert.equal(report.vmd.boneFrameCount, 2);
  assert.equal(report.coverage.framesWithExactFrameRecord, 2);
  assert.equal(report.coverage.framesWithPositionEvidence, 1);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 1);
  assert.equal(report.mapping[0].frameRecord.recordStart, 100);
  assert.equal(report.mapping[0].frameRecord.offset, 14);
  assert.deepEqual(
    [...new Set(report.mapping[0].positionEvidence.map((match) => match.component))],
    [0, 1, 2],
  );
  assert.deepEqual(
    [...new Set(report.mapping[0].localPositionEvidence.map((match) => match.component))],
    [0, 1, 2],
  );
  assert.equal(report.mapping[0].rotationEvidence.length, 2);
  assert.equal(report.mapping[0].localRotationEvidence.length, 2);
  assert.ok(
    report.rotationRecordDeltaSummary.some(
      (entry) => entry.component === 0 && entry.frameRecordDelta === 0 && entry.offset === 0 && entry.count === 1,
    ),
  );
  assert.ok(
    report.rotationRecordDeltaSummary.some(
      (entry) => entry.component === 3 && entry.frameRecordDelta === 0 && entry.offset === 4 && entry.count === 2,
    ),
  );
});

test("prefers frame records that also contain local position evidence", () => {
  const report = mapVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: [{ name: "bone", frame: 66, position: [196, 197, 198], rotation: [0, 0, 0, 1] }],
    },
    {
      byteLength: 300,
      recordByteLength: 62,
      recordTotal: 2,
      summary: {},
      truncated: false,
      records: [createIncidentalFrameRecord(100, 66), createPositionRecord(162, 66, [196, 197, 198])],
    },
    { includeRotationEvidence: false },
  );

  assert.equal(report.coverage.framesWithExactFrameRecord, 1);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 1);
  assert.equal(report.mapping[0].frameRecord.recordStart, 162);
  assert.equal(report.mapping[0].frameRecord.offset, 14);
});

test("reports coverage without building detailed mappings", () => {
  const report = mapVmdBoneFrameCoverageToPmmRecords(
    {
      modelName: "model",
      bones: [
        { name: "bone", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
        { name: "bone", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
      ],
    },
    {
      byteLength: 300,
      recordByteLength: 62,
      recordTotal: 2,
      summary: {},
      truncated: false,
      records: [createPositionRecord(100, 30, [1, 2, 3]), createPositionRecord(162, 60, [4, 5, 6])],
    },
  );

  assert.equal(report.vmd.boneFrameCount, 2);
  assert.equal(report.coverage.framesWithExactFrameRecord, 2);
  assert.equal(report.coverage.framesWithPositionEvidence, 2);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 2);
  assert.deepEqual(report.coverage.exactFrameRecordOffsets, ["0x64", "0xa2"]);
  assert.equal("mapping" in report, false);
});

test("reports all-bone coverage for multi-bone VMD inputs", () => {
  const report = mapAllVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: [
        { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
        { name: "センター", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
        { name: "左足", frame: 30, position: [7, 8, 9], rotation: [0, 0, 0.382683, 0.92388] },
      ],
      boneNameCounts: [
        { name: "センター", count: 2 },
        { name: "左足", count: 1 },
      ],
    },
    {
      byteLength: 400,
      recordByteLength: 62,
      recordTotal: 3,
      summary: {},
      truncated: false,
      records: [
        createPositionRecord(100, 30, [1, 2, 3]),
        createPositionRecord(162, 60, [4, 5, 6]),
        createTransformRecord(224, 30, [7, 8, 9], [0, 0, 0.382683, 0.92388]),
      ],
    },
  );

  assert.equal(report.vmd.boneNameCount, 2);
  assert.equal(report.vmd.boneFrameCount, 3);
  assert.equal(report.coverage.framesWithExactFrameRecord, 3);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 3);
  assert.equal(report.coverage.framesWithLocalRotationEvidence, 1);
  assert.equal(report.coverage.exactFrameRecordRatio, 1);
  assert.equal(report.perBone.length, 2);
  assert.deepEqual(
    report.perBone.map((entry) => entry.vmd.boneName),
    ["センター", "左足"],
  );
  assert.equal(report.notes[1].includes("model slots"), true);
});

function createRecord(recordStart, frame, floats) {
  const raw = Buffer.alloc(62);
  raw.writeUInt32LE(frame, 14);
  raw.writeUInt32LE(frame * 65536, 12);
  raw.writeFloatLE(1, 30);
  raw.writeFloatLE(2, 34);
  raw.writeFloatLE(3, 38);
  raw.writeFloatLE(floats[0], 0);
  raw.writeFloatLE(floats[1], 4);
  return {
    recordStart,
    recordStartHex: `0x${recordStart.toString(16)}`,
    rawHex: raw.toString("hex"),
  };
}

function createIncidentalFrameRecord(recordStart, frame) {
  const raw = Buffer.alloc(62);
  raw.writeUInt32LE(frame, 0);
  return {
    recordStart,
    recordStartHex: `0x${recordStart.toString(16)}`,
    rawHex: raw.toString("hex"),
  };
}

function createPositionRecord(recordStart, frame, position) {
  const raw = Buffer.alloc(62);
  raw.writeUInt32LE(frame * 65536, 12);
  raw.writeUInt32LE(frame, 14);
  raw.writeFloatLE(position[0], 30);
  raw.writeFloatLE(position[1], 34);
  raw.writeFloatLE(position[2], 38);
  return {
    recordStart,
    recordStartHex: `0x${recordStart.toString(16)}`,
    rawHex: raw.toString("hex"),
  };
}

function createTransformRecord(recordStart, frame, position, rotation) {
  const raw = Buffer.alloc(62);
  raw.writeUInt32LE(frame * 65536, 12);
  raw.writeUInt32LE(frame, 14);
  raw.writeFloatLE(position[0], 30);
  raw.writeFloatLE(position[1], 34);
  raw.writeFloatLE(position[2], 38);
  raw.writeFloatLE(rotation[0], 46);
  raw.writeFloatLE(rotation[1], 50);
  raw.writeFloatLE(rotation[2], 54);
  raw.writeFloatLE(rotation[3], 58);
  return {
    recordStart,
    recordStartHex: `0x${recordStart.toString(16)}`,
    rawHex: raw.toString("hex"),
  };
}
