import { readFile } from "node:fs/promises";

export async function readPmmDiff(leftFile, rightFile, options = {}) {
  const [left, right] = await Promise.all([readFile(leftFile), readFile(rightFile)]);
  return diffPmmBytes(left, right, options);
}

export function diffPmmBytes(left, right, options = {}) {
  const context = options.context ?? 16;
  const limit = options.limit ?? 32;
  const prefixLength = commonPrefixLength(left, right);
  const suffixLength = commonSuffixLength(left, right, prefixLength);
  const comparedLength = Math.min(left.byteLength, right.byteLength);
  const changedRanges = findChangedRanges(left, right, {
    start: prefixLength,
    end: comparedLength - suffixLength,
    context,
    limit,
  });

  return {
    leftByteLength: left.byteLength,
    rightByteLength: right.byteLength,
    byteLengthDelta: right.byteLength - left.byteLength,
    commonPrefixLength: prefixLength,
    commonSuffixLength: suffixLength,
    changedRanges,
    truncated: changedRanges.length >= limit,
  };
}

function findChangedRanges(left, right, options) {
  const ranges = [];
  const { start, end, context, limit } = options;
  let index = start;
  while (index < end && ranges.length < limit) {
    if (left[index] === right[index]) {
      index += 1;
      continue;
    }
    const diffStart = index;
    while (index < end && left[index] !== right[index]) {
      index += 1;
    }
    ranges.push(createRange(left, right, diffStart, index, context));
  }
  return ranges;
}

function createRange(left, right, start, end, context) {
  const leftContextStart = Math.max(0, start - context);
  const rightContextStart = Math.max(0, start - context);
  const leftContextEnd = Math.min(left.byteLength, end + context);
  const rightContextEnd = Math.min(right.byteLength, end + context);
  return {
    start,
    startHex: `0x${start.toString(16)}`,
    end,
    endHex: `0x${end.toString(16)}`,
    byteLength: end - start,
    leftContextHex: Buffer.from(left.subarray(leftContextStart, leftContextEnd)).toString("hex"),
    rightContextHex: Buffer.from(right.subarray(rightContextStart, rightContextEnd)).toString("hex"),
  };
}

function commonPrefixLength(left, right) {
  const length = Math.min(left.byteLength, right.byteLength);
  for (let index = 0; index < length; index += 1) {
    if (left[index] !== right[index]) {
      return index;
    }
  }
  return length;
}

function commonSuffixLength(left, right, prefixLength) {
  const maxSuffix = Math.min(left.byteLength, right.byteLength) - prefixLength;
  for (let length = 0; length < maxSuffix; length += 1) {
    if (left[left.byteLength - 1 - length] !== right[right.byteLength - 1 - length]) {
      return length;
    }
  }
  return maxSuffix;
}
