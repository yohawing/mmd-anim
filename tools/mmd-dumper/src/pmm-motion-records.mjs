import { readFile } from "node:fs/promises";

const defaultMarkerHex = "14146b6b14146b6b";
const defaultMarkerOffsetInRecord = 26;
const defaultRecordByteLength = 58;

export async function readPmmMotionRecords(file, options = {}) {
  return extractPmmMotionRecords(await readFile(file), options);
}

export async function readPmmMotionRecordSlice(file, options = {}) {
  return extractPmmMotionRecordSlice(await readFile(file), options);
}

export function extractPmmMotionRecordSlice(bytes, options = {}) {
  const recordStart = requireIntegerOption(options, "recordStart");
  const count = requireIntegerOption(options, "count");
  const recordByteLength = options.recordByteLength ?? defaultRecordByteLength;
  if (recordStart < 0 || count < 1 || recordByteLength < 1) {
    throw new Error("recordStart, count, and recordByteLength must describe a non-empty byte range.");
  }
  const byteStart = recordStart;
  const byteEnd = byteStart + count * recordByteLength;
  if (byteEnd > bytes.byteLength) {
    throw new Error(`Record slice exceeds PMM length: end ${byteEnd}, length ${bytes.byteLength}`);
  }
  return {
    byteStart,
    byteStartHex: `0x${byteStart.toString(16)}`,
    byteEnd,
    byteEndHex: `0x${byteEnd.toString(16)}`,
    count,
    recordByteLength,
    bytes: Buffer.from(bytes.subarray(byteStart, byteEnd)),
  };
}

export function patchPmmMotionRecordSlice(bytes, replacementBytes, options = {}) {
  const recordStart = requireIntegerOption(options, "recordStart");
  const count = requireIntegerOption(options, "count");
  const recordByteLength = options.recordByteLength ?? defaultRecordByteLength;
  const expectedByteLength = count * recordByteLength;
  if (replacementBytes.byteLength !== expectedByteLength) {
    throw new Error(`Replacement length ${replacementBytes.byteLength} does not match expected ${expectedByteLength}.`);
  }
  const slice = extractPmmMotionRecordSlice(bytes, { recordStart, count, recordByteLength });
  const patched = Buffer.from(bytes);
  Buffer.from(replacementBytes).copy(patched, slice.byteStart);
  return {
    ...withoutBytes(slice),
    replacedByteLength: expectedByteLength,
    bytes: patched,
  };
}

export function extractPmmMotionRecords(bytes, options = {}) {
  const markers = parseMarkerHexes(options.markerHex ?? defaultMarkerHex);
  const markerOffsetInRecord = options.markerOffsetInRecord ?? defaultMarkerOffsetInRecord;
  const recordByteLength = options.recordByteLength ?? defaultRecordByteLength;
  const limit = options.limit ?? 64;
  const records = [];

  for (const marker of markers) {
    let offset = 0;
    while (true) {
      const markerOffset = bytes.indexOf(marker.bytes, offset);
      if (markerOffset < 0) {
        break;
      }
      const recordStart = markerOffset - markerOffsetInRecord;
      const recordEnd = recordStart + recordByteLength;
      if (recordStart >= 0 && recordEnd <= bytes.byteLength) {
        records.push(
          decodeCandidateRecord(
            bytes.subarray(recordStart, recordEnd),
            recordStart,
            markerOffset,
            markerOffsetInRecord,
            marker.hex,
          ),
        );
      }
      offset = markerOffset + Math.max(marker.bytes.byteLength, 32);
    }
  }
  records.sort((left, right) => left.recordStart - right.recordStart || left.markerHex.localeCompare(right.markerHex));
  const selectedRecords = records.slice(0, limit);

  return {
    byteLength: bytes.byteLength,
    markerHexes: markers.map((marker) => marker.hex),
    markerOffsetInRecord,
    recordByteLength,
    recordTotal: records.length,
    summary: summarizeRecords(records, recordByteLength),
    records: selectedRecords,
    strideSummary: summarizeRecordStrides(selectedRecords),
    truncated: records.length >= limit,
  };
}

function decodeCandidateRecord(record, recordStart, markerOffset, markerOffsetInRecord, markerHex) {
  const markerStart = markerOffsetInRecord;
  const markerEnd = markerStart + 32;
  return {
    recordStart,
    recordStartHex: `0x${recordStart.toString(16)}`,
    markerOffset,
    markerOffsetHex: `0x${markerOffset.toString(16)}`,
    markerHex,
    rawHex: Buffer.from(record).toString("hex"),
    preMarkerHex: Buffer.from(record.subarray(0, markerStart)).toString("hex"),
    markerAndInterpolationHex: Buffer.from(record.subarray(markerStart, markerEnd)).toString("hex"),
    candidateFields: readCandidateFields(record.subarray(0, markerStart)),
    f32le: readFloat32Words(record),
    u32le: readUInt32Words(record),
  };
}

function readCandidateFields(preMarker) {
  return {
    float32x3: [
      roundFloat(preMarker.readFloatLE(0)),
      roundFloat(preMarker.readFloatLE(4)),
      roundFloat(preMarker.readFloatLE(8)),
    ],
    u16At8: preMarker.readUInt16LE(8),
    u16At10: preMarker.readUInt16LE(10),
    flagU32At12: preMarker.readUInt32LE(12),
    frameAt14: preMarker.readUInt32LE(14),
    u32At16: preMarker.readUInt32LE(16),
    u16At18: preMarker.readUInt16LE(18),
    u16At20: preMarker.readUInt16LE(20),
    u16At22: preMarker.readUInt16LE(22),
    u16At24: preMarker.readUInt16LE(24),
  };
}

function readFloat32Words(record) {
  const values = [];
  for (let offset = 0; offset + 4 <= record.byteLength; offset += 4) {
    values.push({
      offset,
      value: roundFloat(record.readFloatLE(offset)),
      hex: record.subarray(offset, offset + 4).toString("hex"),
    });
  }
  return values;
}

function readUInt32Words(record) {
  const values = [];
  for (let offset = 0; offset + 4 <= record.byteLength; offset += 4) {
    values.push({
      offset,
      value: record.readUInt32LE(offset),
      hex: record.subarray(offset, offset + 4).toString("hex"),
    });
  }
  return values;
}

function summarizeRecordStrides(records) {
  const counts = new Map();
  for (let index = 1; index < records.length; index += 1) {
    const stride = records[index].recordStart - records[index - 1].recordStart;
    counts.set(stride, (counts.get(stride) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([stride, count]) => ({ stride, count }))
    .sort((left, right) => right.count - left.count || left.stride - right.stride)
    .slice(0, 8);
}

function summarizeRecords(records, recordByteLength) {
  return {
    markerCounts: countBy(records, (record) => record.markerHex),
    fullStrideSummary: summarizeRecordStrides(records),
    contiguousRuns: summarizeContiguousRuns(records, recordByteLength),
    candidateFields: {
      float32x3Kinds: countBy(records, (record) => classifyFloat3(record.candidateFields.float32x3)),
      u16At8: countBy(records, (record) => record.candidateFields.u16At8),
      u16At10: countBy(records, (record) => record.candidateFields.u16At10),
      flagU32At12: countBy(records, (record) => record.candidateFields.flagU32At12),
      frameAt14: countBy(records, (record) => record.candidateFields.frameAt14),
      u32At16: countBy(records, (record) => record.candidateFields.u32At16),
      u16At18: countBy(records, (record) => record.candidateFields.u16At18),
      u16At20: countBy(records, (record) => record.candidateFields.u16At20),
      u16At22: countBy(records, (record) => record.candidateFields.u16At22),
      u16At24: countBy(records, (record) => record.candidateFields.u16At24),
    },
  };
}

function summarizeContiguousRuns(records, recordByteLength) {
  const runs = [];
  let runStart = 0;
  for (let index = 1; index <= records.length; index += 1) {
    const previous = records[index - 1];
    const current = records[index];
    if (current && current.recordStart - previous.recordStart === recordByteLength) {
      continue;
    }
    const first = records[runStart];
    if (first) {
      runs.push({
        recordStart: first.recordStart,
        recordStartHex: first.recordStartHex,
        recordEnd: previous.recordStart + recordByteLength,
        recordEndHex: `0x${(previous.recordStart + recordByteLength).toString(16)}`,
        count: index - runStart,
      });
    }
    runStart = index;
  }
  return runs.sort((left, right) => right.count - left.count || left.recordStart - right.recordStart).slice(0, 8);
}

function classifyFloat3(values) {
  if (values.every((value) => value === 0)) {
    return "zero";
  }
  if (values[0] === 0 && values[1] === 0 && values[2] === 1) {
    return "unit-z";
  }
  const length = Math.hypot(...values);
  if (Math.abs(length - 1) < 0.01) {
    return "unit-like";
  }
  return "other";
}

function countBy(records, keyOf) {
  const counts = new Map();
  for (const record of records) {
    const key = String(keyOf(record));
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([value, count]) => ({ value, count }))
    .sort((left, right) => right.count - left.count || left.value.localeCompare(right.value, "en", { numeric: true }))
    .slice(0, 12);
}

function roundFloat(value) {
  if (!Number.isFinite(value)) {
    return value;
  }
  return Math.round(value * 1_000_000) / 1_000_000;
}

function normalizeHex(value) {
  const hex = value.replace(/[^0-9a-f]/gi, "").toLowerCase();
  if (hex.length === 0 || hex.length % 2 !== 0) {
    throw new Error(`Invalid marker hex: ${value}`);
  }
  return hex;
}

function parseMarkerHexes(value) {
  const values = Array.isArray(value) ? value : String(value).split(",");
  return values
    .map((part) => normalizeHex(part))
    .map((hex) => ({ hex, bytes: Buffer.from(hex, "hex") }));
}

function requireIntegerOption(options, key) {
  const value = options[key];
  if (!Number.isInteger(value)) {
    throw new Error(`${key} must be an integer.`);
  }
  return value;
}

function withoutBytes(result) {
  const { bytes, ...metadata } = result;
  return metadata;
}
