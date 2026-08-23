import { readFile } from "node:fs/promises";
import { diffPmmBytes } from "./pmm-diff.mjs";
import { parsePmmManifest } from "./pmm-manifest.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";
import { analyzePmmVmdDiffClusters } from "./pmm-vmd-diff-clusters.mjs";

export async function readPmmKeyCountDelta(options) {
  const baseFile = requireString(options, "base");
  const smallVariantFile = requireString(options, "smallVariant");
  const largeVariantFile = requireString(options, "largeVariant");
  const smallVmdFile = requireString(options, "smallVmd");
  const largeVmdFile = requireString(options, "largeVmd");
  const [baseBytes, smallBytes, largeBytes, smallVmd, largeVmd] = await Promise.all([
    readFile(baseFile),
    readFile(smallVariantFile),
    readFile(largeVariantFile),
    readVmdInventory(smallVmdFile, { limit: Number.POSITIVE_INFINITY }),
    readVmdInventory(largeVmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);

  return analyzePmmKeyCountDelta(baseBytes, smallBytes, largeBytes, smallVmd, largeVmd, {
    ...options,
    baseFile,
    smallVariantFile,
    largeVariantFile,
    smallVmdFile,
    largeVmdFile,
  });
}

export function analyzePmmKeyCountDelta(baseBytes, smallBytes, largeBytes, smallVmd, largeVmd, options = {}) {
  const modelSlots = options.modelSlots ?? readModelSlots(baseBytes);
  const clusterOptions = {
    context: options.context ?? 16,
    diffLimit: options.clusterDiffLimit ?? options.diffLimit ?? 128,
    matchLimit: options.matchLimit ?? 1024,
    candidateLimit: options.candidateLimit ?? 8,
    sequenceLimit: options.sequenceLimit ?? 16,
    recordByteLength: options.recordByteLength ?? 62,
    frameOffsetInRecord: options.frameOffsetInRecord ?? 8,
    modelSlots,
  };
  const smallCluster = analyzePmmVmdDiffClusters(baseBytes, smallBytes, smallVmd, clusterOptions);
  const largeCluster = analyzePmmVmdDiffClusters(baseBytes, largeBytes, largeVmd, clusterOptions);
  const smallProfile = selectMotionProfile(smallCluster);
  const largeProfile = selectMotionProfile(largeCluster);
  const deltaDiff = diffPmmBytes(smallBytes, largeBytes, {
    context: options.context ?? 16,
    limit: options.diffLimit ?? 256,
  });

  const smallKeyCount = smallVmd.counts?.boneFrames ?? smallVmd.bones?.length ?? 0;
  const largeKeyCount = largeVmd.counts?.boneFrames ?? largeVmd.bones?.length ?? 0;
  const recordByteLength = consistentNumber(smallProfile.recordByteLength, largeProfile.recordByteLength);
  const recordCountDelta = largeKeyCount - smallKeyCount;
  const expectedRecordByteDelta = Number.isInteger(recordByteLength) ? recordCountDelta * recordByteLength : undefined;
  const changedRanges = deltaDiff.changedRanges ?? [];

  return {
    baseFile: options.baseFile,
    smallVariantFile: options.smallVariantFile,
    largeVariantFile: options.largeVariantFile,
    smallVmdFile: options.smallVmdFile,
    largeVmdFile: options.largeVmdFile,
    summary: {
      smallKeyCount,
      largeKeyCount,
      recordCountDelta,
      smallMaxFrame: smallVmd.maxFrame,
      largeMaxFrame: largeVmd.maxFrame,
      recordByteLength,
      expectedRecordByteDelta,
      actualByteLengthDelta: deltaDiff.byteLengthDelta,
      recordByteDeltaMatchesFileDelta: expectedRecordByteDelta === deltaDiff.byteLengthDelta,
      sharedBlockStart: smallProfile.blockStart === largeProfile.blockStart,
      blockStart: smallProfile.blockStart,
      blockStartHex: smallProfile.blockStartHex,
      smallBlockEnd: smallProfile.blockEnd,
      smallBlockEndHex: smallProfile.blockEndHex,
      largeBlockEnd: largeProfile.blockEnd,
      largeBlockEndHex: largeProfile.blockEndHex,
      blockExpansionByteLength: subtractIfNumbers(largeProfile.blockEnd, smallProfile.blockEnd),
    },
    smallProfile: summarizeProfile(smallProfile),
    largeProfile: summarizeProfile(largeProfile),
    scalarCandidates: {
      maxFrame: findU32Transitions(smallBytes, largeBytes, smallVmd.maxFrame, largeVmd.maxFrame, changedRanges, {
        limit: options.scalarLimit ?? 32,
      }),
      keyCount: findU32Transitions(smallBytes, largeBytes, smallKeyCount, largeKeyCount, changedRanges, {
        limit: options.scalarLimit ?? 32,
      }),
      changedBeforeBlock: summarizeChangedRangeScalars(smallBytes, largeBytes, changedRanges, {
        endBefore: smallProfile.blockStart,
        limit: options.scalarLimit ?? 32,
      }),
      changedAfterSmallBlock: summarizeChangedRangeScalars(smallBytes, largeBytes, changedRanges, {
        startAt: smallProfile.blockEnd,
        limit: options.scalarLimit ?? 32,
      }),
    },
    diff: {
      leftByteLength: deltaDiff.leftByteLength,
      rightByteLength: deltaDiff.rightByteLength,
      byteLengthDelta: deltaDiff.byteLengthDelta,
      commonPrefixLength: deltaDiff.commonPrefixLength,
      commonPrefixLengthHex: hex(deltaDiff.commonPrefixLength),
      commonSuffixLength: deltaDiff.commonSuffixLength,
      changedRanges,
      truncated: deltaDiff.truncated,
    },
    coverage: {
      small: summarizeCoverage(smallCluster),
      large: summarizeCoverage(largeCluster),
    },
    notes: buildNotes({
      expectedRecordByteDelta,
      actualByteLengthDelta: deltaDiff.byteLengthDelta,
      maxFrameCandidates: findU32Transitions(smallBytes, largeBytes, smallVmd.maxFrame, largeVmd.maxFrame, changedRanges, {
        limit: 4,
      }),
      keyCountCandidates: findU32Transitions(smallBytes, largeBytes, smallKeyCount, largeKeyCount, changedRanges, {
        limit: 4,
      }),
    }),
  };
}

function selectMotionProfile(cluster) {
  const transform = cluster.transformKeyBlockProfile;
  if (transform?.verified && Number.isInteger(transform.blockStart)) {
    return transform;
  }
  const frameSequence = cluster.frameSequenceBlockProfile;
  if (frameSequence?.verified && Number.isInteger(frameSequence.blockStart)) {
    return frameSequence;
  }
  return cluster.positionKeyBlockProfile ?? {};
}

function summarizeProfile(profile) {
  return {
    verified: profile.verified,
    reasons: profile.reasons ?? [],
    blockStart: profile.blockStart,
    blockStartHex: profile.blockStartHex,
    blockEnd: profile.blockEnd,
    blockEndHex: profile.blockEndHex,
    recordByteLength: profile.recordByteLength,
    frameOffsetInRecord: profile.frameOffsetInRecord,
    positionOffsetInRecord: profile.positionOffsetInRecord,
    rotationOffsetInRecord: profile.rotationOffsetInRecord,
    recordCount: profile.recordCount,
    modelSlotContext: profile.modelSlotContext,
    boneSpans: profile.boneSpans,
  };
}

function summarizeCoverage(cluster) {
  return {
    boneFrameCount: cluster.coverage?.boneFrameCount,
    frameSequenceMatchedBoneFrames: cluster.coverage?.frameSequenceMatchedBoneFrames,
    rotationMatchedBoneFrames: cluster.coverage?.rotationMatchedBoneFrames,
    frameSequenceClusterRatio: cluster.coverage?.frameSequenceClusterRatio,
    rotationClusterRatio: cluster.coverage?.rotationClusterRatio,
  };
}

function findU32Transitions(left, right, leftValue, rightValue, changedRanges, options = {}) {
  if (!Number.isInteger(leftValue) || !Number.isInteger(rightValue) || leftValue < 0 || rightValue < 0) {
    return [];
  }
  const limit = options.limit ?? 32;
  const results = [];
  const end = Math.min(left.byteLength, right.byteLength) - 4;
  for (let offset = 0; offset <= end && results.length < limit; offset += 1) {
    if (!u32CanRead(left, offset) || !u32CanRead(right, offset)) {
      continue;
    }
    if (left.readUInt32LE(offset) !== leftValue || right.readUInt32LE(offset) !== rightValue) {
      continue;
    }
    const overlapsChangedRange = changedRanges.some((range) => rangesOverlap(offset, offset + 4, range.start, range.end));
    if (!overlapsChangedRange) {
      continue;
    }
    results.push({
      offset,
      offsetHex: hex(offset),
      leftValue,
      rightValue,
      leftHex: left.subarray(offset, offset + 4).toString("hex"),
      rightHex: right.subarray(offset, offset + 4).toString("hex"),
      aligned4: offset % 4 === 0,
    });
  }
  return results;
}

function summarizeChangedRangeScalars(left, right, changedRanges, options = {}) {
  const limit = options.limit ?? 32;
  const candidates = [];
  for (const range of changedRanges) {
    if (Number.isInteger(options.endBefore) && range.start >= options.endBefore) {
      continue;
    }
    if (Number.isInteger(options.startAt) && range.end <= options.startAt) {
      continue;
    }
    const offset = range.start;
    if (!u32CanRead(left, offset) || !u32CanRead(right, offset)) {
      continue;
    }
    const leftU32 = left.readUInt32LE(offset);
    const rightU32 = right.readUInt32LE(offset);
    if (leftU32 === rightU32) {
      continue;
    }
    candidates.push({
      offset,
      offsetHex: hex(offset),
      end: range.end,
      endHex: hex(range.end),
      byteLength: range.byteLength,
      leftU32,
      rightU32,
      delta: rightU32 - leftU32,
      leftHex: left.subarray(offset, Math.min(left.byteLength, offset + 4)).toString("hex"),
      rightHex: right.subarray(offset, Math.min(right.byteLength, offset + 4)).toString("hex"),
      aligned4: offset % 4 === 0,
    });
    if (candidates.length >= limit) {
      break;
    }
  }
  return candidates;
}

function buildNotes({ expectedRecordByteDelta, actualByteLengthDelta, maxFrameCandidates, keyCountCandidates }) {
  const notes = [];
  if (expectedRecordByteDelta === actualByteLengthDelta) {
    notes.push("File length growth equals recordCountDelta * recordByteLength; extra fields appear to be in-place scalar/cache updates.");
  } else {
    notes.push("File length growth does not match recordCountDelta * recordByteLength; section size changes need separate investigation.");
  }
  if (keyCountCandidates.length > 0) {
    notes.push("At least one changed u32 field tracks the VMD bone key count.");
  }
  if (maxFrameCandidates.length > 0) {
    notes.push("At least one changed u32 field tracks the VMD max frame.");
  }
  notes.push("This report identifies candidates only; a key-count-changing writer must still verify cache/timeline fields by MMD roundtrip or runtime numeric evidence.");
  return notes;
}

function readModelSlots(bytes) {
  try {
    return parsePmmManifest(bytes).modelSlots ?? [];
  } catch {
    return [];
  }
}

function consistentNumber(left, right) {
  if (Number.isInteger(left) && left === right) {
    return left;
  }
  return undefined;
}

function subtractIfNumbers(left, right) {
  return Number.isInteger(left) && Number.isInteger(right) ? left - right : undefined;
}

function rangesOverlap(leftStart, leftEnd, rightStart, rightEnd) {
  return leftStart < rightEnd && rightStart < leftEnd;
}

function u32CanRead(bytes, offset) {
  return Number.isInteger(offset) && offset >= 0 && offset + 4 <= bytes.byteLength;
}

function hex(value) {
  return Number.isInteger(value) ? `0x${value.toString(16)}` : undefined;
}

function requireString(options, key) {
  if (!options?.[key]) {
    throw new Error(`Missing ${key}`);
  }
  return options[key];
}
