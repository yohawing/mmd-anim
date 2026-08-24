import { readFile } from "node:fs/promises";
import { analyzePmmBytes } from "./pmm-analysis.mjs";
import { diffPmmBytes } from "./pmm-diff.mjs";

export async function readPmmInvestigation(leftFile, rightFile, options = {}) {
  const [left, right] = await Promise.all([readFile(leftFile), readFile(rightFile)]);
  return investigatePmmBytes(left, right, options);
}

export function investigatePmmBytes(left, right, options = {}) {
  const diff = diffPmmBytes(left, right, {
    context: options.context ?? 16,
    limit: options.diffLimit ?? options.limit ?? 64,
  });
  const analysis = analyzePmmBytes(right, {
    texts: options.texts,
    int32s: options.int32s,
    float32s: options.float32s,
    limit: options.matchLimit ?? options.limit ?? 128,
  });
  return {
    diff,
    changedTextMatches: filterMatchGroups(analysis.textMatches, diff.changedRanges),
    changedInt32Matches: filterMatchGroups(analysis.int32Matches, diff.changedRanges),
    changedFloat32Matches: filterMatchGroups(analysis.float32Matches, diff.changedRanges),
  };
}

function filterMatchGroups(groups, ranges) {
  return groups.map((group) => {
    const matches = group.matches
      .map((match) => attachChangedRange(match, ranges))
      .filter((match) => match.changedRange !== undefined);
    return {
      ...group,
      matches,
      changedMatchCount: matches.length,
    };
  });
}

function attachChangedRange(match, ranges) {
  const matchStart = match.offset;
  const matchEnd = match.offset + match.byteLength;
  const rangeIndex = ranges.findIndex((range) => rangesOverlap(matchStart, matchEnd, range.start, range.end));
  if (rangeIndex < 0) {
    return match;
  }
  const range = ranges[rangeIndex];
  return {
    ...match,
    changedRange: {
      index: rangeIndex,
      start: range.start,
      startHex: range.startHex,
      end: range.end,
      endHex: range.endHex,
    },
  };
}

function rangesOverlap(leftStart, leftEnd, rightStart, rightEnd) {
  return leftStart < rightEnd && rightStart < leftEnd;
}
