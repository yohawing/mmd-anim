import { readFile } from "node:fs/promises";
import iconv from "iconv-lite";

const defaultTextNeedles = ["Vocaloid Motion Data 0002", "Vocaloid Motion Data"];

export async function readPmmAnalysis(file, options = {}) {
  const bytes = await readFile(file);
  return analyzePmmBytes(bytes, options);
}

export function analyzePmmBytes(bytes, options = {}) {
  const limit = options.limit ?? 32;
  const texts = options.texts?.length ? options.texts : defaultTextNeedles;
  return {
    byteLength: bytes.byteLength,
    textMatches: analyzeTextNeedles(bytes, texts, limit),
    int32Matches: analyzeInt32Needles(bytes, options.int32s ?? [], limit),
    float32Matches: analyzeFloat32Needles(bytes, options.float32s ?? [], limit),
  };
}

function analyzeTextNeedles(bytes, texts, limit) {
  return texts.map((text) => {
    const encodings = [
      { encoding: "ascii", needle: Buffer.from(text, "ascii") },
      { encoding: "cp932", needle: iconv.encode(text, "cp932") },
    ];
    const matches = uniqueMatches(encodings.flatMap(({ encoding, needle }) => findNeedle(bytes, needle, limit, encoding)));
    return {
      text,
      matches,
      truncated: matches.length >= limit,
    };
  });
}

function analyzeInt32Needles(bytes, values, limit) {
  return values.map((value) => {
    const needle = Buffer.alloc(4);
    needle.writeInt32LE(value, 0);
    const matches = findNeedle(bytes, needle, limit);
    return {
      value,
      littleEndianHex: needle.toString("hex"),
      matches,
      truncated: matches.length >= limit,
    };
  });
}

function analyzeFloat32Needles(bytes, values, limit) {
  return values.map((value) => {
    const needle = Buffer.alloc(4);
    needle.writeFloatLE(value, 0);
    const matches = findNeedle(bytes, needle, limit);
    return {
      value,
      littleEndianHex: needle.toString("hex"),
      matches,
      truncated: matches.length >= limit,
    };
  });
}

function findNeedle(bytes, needle, limit, encoding) {
  if (needle.byteLength === 0) {
    return [];
  }
  const matches = [];
  let offset = 0;
  while (matches.length < limit) {
    const index = bytes.indexOf(needle, offset);
    if (index < 0) {
      break;
    }
    matches.push(createMatch(bytes, index, needle.byteLength, encoding));
    offset = index + 1;
  }
  return matches;
}

function createMatch(bytes, offset, byteLength, encoding) {
  const contextStart = Math.max(0, offset - 16);
  const contextEnd = Math.min(bytes.byteLength, offset + byteLength + 16);
  return {
    offset,
    offsetHex: `0x${offset.toString(16)}`,
    byteLength,
    ...(encoding ? { encoding } : {}),
    contextHex: Buffer.from(bytes.subarray(contextStart, contextEnd)).toString("hex"),
  };
}

function uniqueMatches(matches) {
  const seen = new Set();
  return matches.filter((match) => {
    const key = `${match.offset}:${match.byteLength}`;
    if (seen.has(key)) {
      return false;
    }
    seen.add(key);
    return true;
  });
}
