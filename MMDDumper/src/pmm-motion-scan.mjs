import { readFile } from "node:fs/promises";

const defaultMarkerHex = [
  "14146b6b14146b6b",
  "3131313136363636",
];

export async function readPmmMotionScan(file, options = {}) {
  return scanPmmMotionBytes(await readFile(file), options);
}

export function scanPmmMotionBytes(bytes, options = {}) {
  const limit = options.limit ?? 64;
  const radius = options.radius ?? 96;
  const int32s = options.int32s ?? [];
  const float32s = options.float32s ?? [];
  const markers = (options.markerHex?.length ? options.markerHex : defaultMarkerHex).map((hex) => ({
    hex: normalizeHex(hex),
    bytes: Buffer.from(normalizeHex(hex), "hex"),
  }));
  const hits = [];
  for (const marker of markers) {
    let offset = 0;
    while (hits.length < limit) {
      const index = bytes.indexOf(marker.bytes, offset);
      if (index < 0) {
        break;
      }
      hits.push(createHit(bytes, marker.hex, index, marker.bytes.byteLength, radius, int32s, float32s));
      offset = index + Math.max(marker.bytes.byteLength, 32);
    }
    if (hits.length >= limit) {
      break;
    }
  }
  return {
    byteLength: bytes.byteLength,
    markers: markers.map((marker) => marker.hex),
    hits,
    strideSummary: summarizeStrides(hits),
    truncated: hits.length >= limit,
  };
}

function createHit(bytes, markerHex, offset, byteLength, radius, int32s, float32s) {
  const start = Math.max(0, offset - radius);
  const end = Math.min(bytes.byteLength, offset + byteLength + radius);
  const window = bytes.subarray(start, end);
  return {
    offset,
    offsetHex: `0x${offset.toString(16)}`,
    markerHex,
    contextStart: start,
    contextEnd: end,
    contextHex: Buffer.from(window).toString("hex"),
    nearbyInt32Matches: findNeedlesInWindow(bytes, start, end, int32s.map(int32Needle)),
    nearbyFloat32Matches: findNeedlesInWindow(bytes, start, end, float32s.map(float32Needle)),
  };
}

function findNeedlesInWindow(bytes, start, end, needles) {
  const matches = [];
  for (const needle of needles) {
    let offset = start;
    while (offset < end) {
      const index = bytes.indexOf(needle.bytes, offset);
      if (index < 0 || index + needle.bytes.byteLength > end) {
        break;
      }
      matches.push({
        value: needle.value,
        offset: index,
        offsetHex: `0x${index.toString(16)}`,
        littleEndianHex: needle.bytes.toString("hex"),
      });
      offset = index + 1;
    }
  }
  return matches.sort((left, right) => left.offset - right.offset);
}

function int32Needle(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeInt32LE(value, 0);
  return { value, bytes };
}

function float32Needle(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeFloatLE(value, 0);
  return { value, bytes };
}

function normalizeHex(value) {
  const hex = value.replace(/[^0-9a-f]/gi, "").toLowerCase();
  if (hex.length === 0 || hex.length % 2 !== 0) {
    throw new Error(`Invalid marker hex: ${value}`);
  }
  return hex;
}

function summarizeStrides(hits) {
  const counts = new Map();
  for (let index = 1; index < hits.length; index += 1) {
    const stride = hits[index].offset - hits[index - 1].offset;
    counts.set(stride, (counts.get(stride) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([stride, count]) => ({ stride, count }))
    .sort((left, right) => right.count - left.count || left.stride - right.stride)
    .slice(0, 8);
}
