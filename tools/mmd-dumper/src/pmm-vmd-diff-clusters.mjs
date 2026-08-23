import { readFile } from "node:fs/promises";
import { diffPmmBytes } from "./pmm-diff.mjs";
import { parsePmmManifest } from "./pmm-manifest.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function readPmmVmdDiffClusters(options) {
  const baseFile = requireString(options, "base");
  const variantFile = requireString(options, "variant");
  const vmdFile = requireString(options, "vmd");
  const [baseBytes, variantBytes, vmd] = await Promise.all([
    readFile(baseFile),
    readFile(variantFile),
    readVmdInventory(vmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  const baseManifest = parsePmmManifest(baseBytes);
  return analyzePmmVmdDiffClusters(baseBytes, variantBytes, vmd, {
    ...options,
    baseFile,
    variantFile,
    vmdFile,
    modelSlots: baseManifest.modelSlots,
  });
}

export function analyzePmmVmdDiffClusters(baseBytes, variantBytes, vmd, options = {}) {
  const diff = diffPmmBytes(baseBytes, variantBytes, {
    context: options.context ?? 16,
    limit: options.diffLimit ?? options.limit ?? 128,
  });
  const middleStart = diff.commonPrefixLength;
  const middleEnd = variantBytes.byteLength - diff.commonSuffixLength;
  const middle = variantBytes.subarray(middleStart, middleEnd);
  const maxFramePositionDelta = options.maxFramePositionDelta ?? 96;
  const frameMatches = collectUniqueFrames(vmd.bones ?? []).map((frame) => ({
    frame,
    matches: findNeedle(middle, uint32le(frame), options.matchLimit ?? 1024).map((match) => absoluteMatch(match, middleStart)),
  }));
  const frameMatchMap = new Map(frameMatches.map((entry) => [entry.frame, entry.matches]));
  const frameSequenceBlockProfile = deriveFrameSequenceBlockProfile(middle, middleStart, vmd.bones ?? [], {
    recordByteLength: options.recordByteLength ?? 62,
    frameOffsetInRecord: options.frameOffsetInRecord ?? 8,
    limit: options.sequenceLimit ?? 16,
  });
  const boneFrames = (vmd.bones ?? []).map((bone, index) => {
    const positionMatches = isAllZeroVector(bone.position)
      ? []
      : findNeedle(middle, vector3f32le(bone.position), options.matchLimit ?? 1024).map((match) => absoluteMatch(match, middleStart));
    const candidates = positionMatches
      .map((positionMatch) => attachNearestFrameMatch(positionMatch, frameMatchMap.get(bone.frame) ?? [], maxFramePositionDelta))
      .filter(Boolean)
      .map((candidate) => ({
        frameOffset: candidate.frameMatch.offset,
        frameOffsetHex: hex(candidate.frameMatch.offset),
        positionOffset: candidate.positionMatch.offset,
        positionOffsetHex: hex(candidate.positionMatch.offset),
        frameToPositionDelta: candidate.positionMatch.offset - candidate.frameMatch.offset,
        estimatedRecordStart: candidate.frameMatch.offset - 8,
        estimatedRecordStartHex: hex(candidate.frameMatch.offset - 8),
      }));
    return {
      index,
      name: bone.name,
      frame: bone.frame,
      position: bone.position,
      rotation: bone.rotation,
      positionMatchCount: positionMatches.length,
      candidateCount: candidates.length,
      candidates: candidates.slice(0, options.candidateLimit ?? 8),
      sequenceCandidate: frameSequenceBlockProfile.records?.[index],
      matched: candidates.length > 0,
      frameSequenceMatched: Boolean(frameSequenceBlockProfile.records?.[index]),
    };
  });
  const perBone = groupBoneFrames(boneFrames);
  const modelSlots = options.modelSlots ?? [];
  const positionKeyBlockProfile = withModelSlotContext(derivePositionKeyBlockProfile(boneFrames), modelSlots);
  const frameSequenceBlockProfileWithSlot = withModelSlotContext(frameSequenceBlockProfile, modelSlots);
  const transformKeyBlockProfile = withModelSlotContext(
    deriveTransformKeyBlockProfile(variantBytes, frameSequenceBlockProfile, vmd.bones ?? []),
    modelSlots,
  );
  const matchedBoneFrames = boneFrames.filter((frame) => frame.matched).length;
  const nonIdentityRotationFrames = boneFrames.filter((frame) => !isIdentityRotation(frame.rotation)).length;
  return {
    baseFile: options.baseFile,
    variantFile: options.variantFile,
    vmdFile: options.vmdFile,
    diff: {
      leftByteLength: diff.leftByteLength,
      rightByteLength: diff.rightByteLength,
      byteLengthDelta: diff.byteLengthDelta,
      commonPrefixLength: diff.commonPrefixLength,
      commonPrefixLengthHex: hex(diff.commonPrefixLength),
      commonSuffixLength: diff.commonSuffixLength,
      changedMiddle: {
        start: middleStart,
        startHex: hex(middleStart),
        end: middleEnd,
        endHex: hex(middleEnd),
        byteLength: middle.byteLength,
      },
      changedRanges: diff.changedRanges,
      truncated: diff.truncated,
    },
    vmd: {
      modelName: vmd.modelName,
      counts: vmd.counts,
      boneFrameCount: vmd.bones?.length ?? 0,
      distinctBoneCount: perBone.length,
      maxFrame: vmd.maxFrame,
    },
    coverage: {
      boneFrameCount: boneFrames.length,
      matchedBoneFrames,
      frameSequenceMatchedBoneFrames: frameSequenceBlockProfileWithSlot.recordCount ?? 0,
      nonIdentityRotationFrames,
      rotationMatchedBoneFrames: transformKeyBlockProfile.matchedRotationCount ?? 0,
      unmatchedBoneFrames: boneFrames.length - matchedBoneFrames,
      exactPositionClusterRatio: boneFrames.length === 0 ? 1 : matchedBoneFrames / boneFrames.length,
      frameSequenceClusterRatio: boneFrames.length === 0 ? 1 : (frameSequenceBlockProfileWithSlot.recordCount ?? 0) / boneFrames.length,
      rotationClusterRatio: nonIdentityRotationFrames === 0 ? 1 : (transformKeyBlockProfile.matchedRotationCount ?? 0) / nonIdentityRotationFrames,
    },
    positionKeyBlockProfile,
    frameSequenceBlockProfile: frameSequenceBlockProfileWithSlot,
    transformKeyBlockProfile,
    frameMatches,
    perBone,
    boneFrames,
  };
}

function withModelSlotContext(profile, modelSlots) {
  if (!profile?.verified || !Number.isInteger(profile.blockStart) || modelSlots.length === 0) {
    return profile;
  }
  const slot = findModelSlotBeforeOffset(modelSlots, profile.blockStart);
  if (!slot) {
    return {
      ...profile,
      modelSlotContext: {
        inferred: false,
        reason: "No model slot path offset appears before the motion block.",
      },
    };
  }
  return {
    ...profile,
    modelSlotContext: {
      inferred: true,
      slot: slot.slot,
      path: slot.path,
      fileName: slot.fileName,
      modelPathOffset: slot.offset,
      modelPathOffsetHex: hex(slot.offset),
      method: "last-model-path-before-block-start",
    },
  };
}

function findModelSlotBeforeOffset(modelSlots, offset) {
  return modelSlots
    .filter((slot) => Number.isInteger(slot.offset) && slot.offset <= offset)
    .sort((left, right) => right.offset - left.offset)[0];
}

function deriveTransformKeyBlockProfile(bytes, frameSequenceBlockProfile, bones) {
  const nonIdentity = bones
    .map((bone, index) => ({ bone, index }))
    .filter(({ bone }) => !isIdentityRotation(bone.rotation));
  if (nonIdentity.length === 0) {
    return {
      verified: true,
      reasons: [],
      nonIdentityRotationCount: 0,
      matchedRotationCount: 0,
    };
  }
  if (!frameSequenceBlockProfile.records) {
    return {
      verified: false,
      reasons: ["no frame sequence records available for rotation matching"],
      nonIdentityRotationCount: nonIdentity.length,
      matchedRotationCount: 0,
    };
  }

  const matches = nonIdentity.map(({ bone, index }) => {
    const record = frameSequenceBlockProfile.records[index];
    const rotationOffsets = findVector4OffsetsInRecord(bytes, record.recordStart, frameSequenceBlockProfile.recordByteLength, bone.rotation);
    return {
      index,
      name: bone.name,
      frame: bone.frame,
      rotation: bone.rotation,
      recordStart: record.recordStart,
      recordStartHex: record.recordStartHex,
      rotationOffsets,
      matched: rotationOffsets.length > 0,
    };
  });
  const matched = matches.filter((match) => match.matched);
  const relativeOffsets = uniqueNumbers(matched.flatMap((match) => match.rotationOffsets.map((offset) => offset - match.recordStart)));
  const reasons = [];
  if (matched.length !== nonIdentity.length) {
    reasons.push(`matched ${matched.length}/${nonIdentity.length} non-identity rotations`);
  }
  if (relativeOffsets.length === 0) {
    reasons.push("no consistent rotation offset");
  } else if (relativeOffsets.length !== 1) {
    reasons.push(`rotation offset is not consistent: ${relativeOffsets.join(",")}`);
  }
  const rotationOffsetInRecord = relativeOffsets[0];
  return {
    verified: reasons.length === 0,
    reasons,
    blockStart: frameSequenceBlockProfile.blockStart,
    blockStartHex: frameSequenceBlockProfile.blockStartHex,
    blockEnd: frameSequenceBlockProfile.blockEnd,
    blockEndHex: frameSequenceBlockProfile.blockEndHex,
    recordByteLength: frameSequenceBlockProfile.recordByteLength,
    frameOffsetInRecord: frameSequenceBlockProfile.frameOffsetInRecord,
    rotationOffsetInRecord,
    nonIdentityRotationCount: nonIdentity.length,
    matchedRotationCount: matched.length,
    records: matches.map((match) => ({
      index: match.index,
      name: match.name,
      frame: match.frame,
      recordStart: match.recordStart,
      recordStartHex: match.recordStartHex,
      rotationOffsets: match.rotationOffsets,
      rotationOffsetHexes: match.rotationOffsets.map(hex),
      matched: match.matched,
    })),
  };
}

function findVector4OffsetsInRecord(bytes, recordStart, recordByteLength, values, epsilon = 0.00001) {
  const offsets = [];
  for (let relativeOffset = 0; relativeOffset < recordByteLength; relativeOffset += 1) {
    const offset = recordStart + relativeOffset;
    if (offset + 16 > bytes.byteLength) {
      continue;
    }
    if (values.every((value, index) => Math.abs(bytes.readFloatLE(offset + index * 4) - value) <= epsilon)) {
      offsets.push(offset);
    }
  }
  return offsets;
}

function deriveFrameSequenceBlockProfile(middle, middleStart, bones, options) {
  if (bones.length === 0) {
    return { verified: true, reasons: [], recordCount: 0 };
  }
  const { recordByteLength, frameOffsetInRecord, limit } = options;
  const byteLength = bones.length * recordByteLength;
  const matches = [];
  for (let offset = 0; offset + byteLength <= middle.byteLength && matches.length < limit; offset += 1) {
    if (!matchesFrameSequenceAt(middle, offset, bones, recordByteLength, frameOffsetInRecord)) {
      continue;
    }
    const blockStart = middleStart + offset;
    const records = bones.map((bone, index) => {
      const recordStart = blockStart + index * recordByteLength;
      return {
        index,
        name: bone.name,
        frame: bone.frame,
        recordStart,
        recordStartHex: hex(recordStart),
        frameOffset: recordStart + frameOffsetInRecord,
        frameOffsetHex: hex(recordStart + frameOffsetInRecord),
      };
    });
    matches.push({
      blockStart,
      blockStartHex: hex(blockStart),
      blockEnd: blockStart + byteLength,
      blockEndHex: hex(blockStart + byteLength),
      recordByteLength,
      frameOffsetInRecord,
      recordCount: bones.length,
      records,
      boneSpans: deriveSequenceBoneSpans(records, recordByteLength),
    });
  }
  const selected = matches[0];
  if (!selected) {
    return {
      verified: false,
      reasons: [`no ${bones.length} record frame sequence block found`],
      recordByteLength,
      frameOffsetInRecord,
      recordCount: 0,
      matches,
    };
  }
  return {
    verified: matches.length === 1,
    reasons: matches.length === 1 ? [] : [`multiple frame sequence blocks found: ${matches.length}`],
    ...selected,
    matchCount: matches.length,
    alternativeBlockStarts: matches.slice(1).map((match) => match.blockStartHex),
  };
}

function matchesFrameSequenceAt(bytes, blockOffset, bones, recordByteLength, frameOffsetInRecord) {
  for (let index = 0; index < bones.length; index += 1) {
    const frameOffset = blockOffset + index * recordByteLength + frameOffsetInRecord;
    if (frameOffset + 4 > bytes.byteLength || bytes.readUInt32LE(frameOffset) !== bones[index].frame) {
      return false;
    }
  }
  return true;
}

function deriveSequenceBoneSpans(records, recordByteLength) {
  const groups = new Map();
  for (const record of records) {
    if (!groups.has(record.name)) {
      groups.set(record.name, []);
    }
    groups.get(record.name).push(record);
  }
  return [...groups.entries()].map(([name, entries]) => {
    const starts = entries.map((entry) => entry.recordStart).sort((left, right) => left - right);
    return {
      name,
      recordCount: entries.length,
      spanStart: starts[0],
      spanStartHex: hex(starts[0]),
      spanEnd: starts[starts.length - 1] + recordByteLength,
      spanEndHex: hex(starts[starts.length - 1] + recordByteLength),
      recordStarts: starts.map(hex),
      frames: entries.map((entry) => entry.frame).sort((left, right) => left - right),
    };
  });
}

function attachNearestFrameMatch(positionMatch, frameMatches, maxDelta) {
  const candidates = frameMatches
    .map((frameMatch) => ({ frameMatch, positionMatch, delta: positionMatch.offset - frameMatch.offset }))
    .filter((candidate) => candidate.delta >= 0 && candidate.delta <= maxDelta)
    .sort((left, right) => left.delta - right.delta || left.frameMatch.offset - right.frameMatch.offset);
  return candidates[0];
}

function groupBoneFrames(boneFrames) {
  const groups = new Map();
  for (const frame of boneFrames) {
    if (!groups.has(frame.name)) {
      groups.set(frame.name, []);
    }
    groups.get(frame.name).push(frame);
  }
  return [...groups.entries()].map(([name, frames]) => {
    const matched = frames.filter((frame) => frame.matched);
    const estimatedRecordStarts = matched
      .map((frame) => frame.candidates[0]?.estimatedRecordStart)
      .filter((offset) => Number.isInteger(offset))
      .sort((left, right) => left - right);
    return {
      name,
      frameCount: frames.length,
      matchedFrameCount: matched.length,
      estimatedRecordStarts: estimatedRecordStarts.map(hex),
      estimatedStrideSummary: summarizeStrides(estimatedRecordStarts),
      frames: frames.map((frame) => ({
        frame: frame.frame,
        position: frame.position,
        matched: frame.matched,
        candidateCount: frame.candidateCount,
        candidates: frame.candidates,
      })),
    };
  });
}

function derivePositionKeyBlockProfile(boneFrames) {
  const matched = boneFrames
    .map((frame) => ({ frame, candidate: frame.candidates[0] }))
    .filter((entry) => entry.candidate)
    .sort((left, right) => left.candidate.estimatedRecordStart - right.candidate.estimatedRecordStart);
  if (matched.length === 0) {
    return {
      verified: false,
      reasons: ["no matched bone frames"],
    };
  }
  const recordStarts = matched.map((entry) => entry.candidate.estimatedRecordStart);
  const frameOffsets = uniqueNumbers(matched.map((entry) => entry.candidate.frameOffset - entry.candidate.estimatedRecordStart));
  const positionOffsets = uniqueNumbers(matched.map((entry) => entry.candidate.positionOffset - entry.candidate.estimatedRecordStart));
  const strideSummary = summarizeStrides(recordStarts);
  const dominantStride = strideSummary[0]?.stride;
  const reasons = [];
  if (matched.length !== boneFrames.length) {
    reasons.push(`matched ${matched.length}/${boneFrames.length} bone frames`);
  }
  if (frameOffsets.length !== 1) {
    reasons.push(`frame offset is not consistent: ${frameOffsets.join(",")}`);
  }
  if (positionOffsets.length !== 1) {
    reasons.push(`position offset is not consistent: ${positionOffsets.join(",")}`);
  }
  if (!dominantStride) {
    reasons.push("not enough matched records to infer stride");
  }
  if (dominantStride && strideSummary[0].count !== Math.max(0, matched.length - 1)) {
    reasons.push(`record starts are not fully contiguous with stride ${dominantStride}`);
  }
  const blockStart = Math.min(...recordStarts);
  const blockEnd = dominantStride ? Math.max(...recordStarts) + dominantStride : undefined;
  return {
    verified: reasons.length === 0,
    reasons,
    recordByteLength: dominantStride,
    blockStart,
    blockStartHex: hex(blockStart),
    blockEnd,
    blockEndHex: blockEnd === undefined ? undefined : hex(blockEnd),
    recordCount: matched.length,
    frameOffsetInRecord: frameOffsets[0],
    positionOffsetInRecord: positionOffsets[0],
    strideSummary,
    boneSpans: deriveBoneSpans(matched, dominantStride),
  };
}

function deriveBoneSpans(matched, recordByteLength) {
  const groups = new Map();
  for (const entry of matched) {
    const name = entry.frame.name;
    if (!groups.has(name)) {
      groups.set(name, []);
    }
    groups.get(name).push(entry);
  }
  return [...groups.entries()].map(([name, entries]) => {
    const starts = entries
      .map((entry) => entry.candidate.estimatedRecordStart)
      .sort((left, right) => left - right);
    const spanStart = starts[0];
    const spanEnd = recordByteLength ? starts[starts.length - 1] + recordByteLength : undefined;
    return {
      name,
      recordCount: entries.length,
      spanStart,
      spanStartHex: hex(spanStart),
      spanEnd,
      spanEndHex: spanEnd === undefined ? undefined : hex(spanEnd),
      recordStarts: starts.map(hex),
      frames: entries.map((entry) => entry.frame.frame).sort((left, right) => left - right),
    };
  });
}

function summarizeStrides(offsets) {
  const counts = new Map();
  for (let index = 1; index < offsets.length; index += 1) {
    const stride = offsets[index] - offsets[index - 1];
    counts.set(stride, (counts.get(stride) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([stride, count]) => ({ stride, count }))
    .sort((left, right) => right.count - left.count || left.stride - right.stride);
}

function uniqueNumbers(values) {
  return [...new Set(values)].sort((left, right) => left - right);
}

function collectUniqueFrames(bones) {
  return [...new Set(bones.map((bone) => bone.frame))].sort((left, right) => left - right);
}

function findNeedle(bytes, needle, limit) {
  const matches = [];
  let offset = 0;
  while (matches.length < limit) {
    const index = bytes.indexOf(needle, offset);
    if (index < 0) {
      break;
    }
    matches.push({ offset: index, byteLength: needle.byteLength, offsetHex: hex(index) });
    offset = index + 1;
  }
  return matches;
}

function absoluteMatch(match, baseOffset) {
  const offset = match.offset + baseOffset;
  return {
    ...match,
    offset,
    offsetHex: hex(offset),
    relativeOffset: match.offset,
    relativeOffsetHex: hex(match.offset),
  };
}

function uint32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value, 0);
  return bytes;
}

function vector3f32le(values) {
  const bytes = Buffer.alloc(12);
  bytes.writeFloatLE(values[0], 0);
  bytes.writeFloatLE(values[1], 4);
  bytes.writeFloatLE(values[2], 8);
  return bytes;
}

function isAllZeroVector(values = []) {
  return values.length === 3 && values.every((value) => Object.is(Math.fround(value), 0));
}

function isIdentityRotation(rotation = [], epsilon = 0.00001) {
  return (
    rotation.length === 4 &&
    Math.abs(rotation[0]) <= epsilon &&
    Math.abs(rotation[1]) <= epsilon &&
    Math.abs(rotation[2]) <= epsilon &&
    Math.abs(rotation[3] - 1) <= epsilon
  );
}

function hex(value) {
  return `0x${value.toString(16)}`;
}

function requireString(options, name) {
  const value = options[name];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`Missing required option: ${name}`);
  }
  return value;
}
