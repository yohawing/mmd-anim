import { readPmmMotionRecords } from "./pmm-motion-records.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function mapVmdBoneFramesToPmm(options) {
  const vmd = await readVmdInventory(requireString(options, "vmd"), { limit: options.limit ?? Number.POSITIVE_INFINITY });
  const pmm = await readPmmMotionRecords(requireString(options, "pmm"), {
    markerHex: options.markerHex,
    markerOffsetInRecord: options.markerOffsetInRecord,
    recordByteLength: options.recordByteLength ?? 62,
    limit: options.recordLimit ?? Math.max(vmd.counts?.boneFrames + 8, 4096),
  });
  return options.allBones ? mapAllVmdBoneFramesToPmmRecords(vmd, pmm, options) : mapVmdBoneFramesToPmmRecords(vmd, pmm, options);
}

export function mapVmdBoneFramesToPmmRecords(vmd, pmm, options = {}) {
  const boneName = options.boneName ?? inferSingleBoneName(vmd);
  const frames = vmd.bones.filter((frame) => frame.name === boneName).sort((left, right) => left.frame - right.frame);
  const records = pmm.records;
  const searchIndex = createRecordSearchIndex(records);
  const mappings = frames.map((frame) => mapFrame(frame, records, options, searchIndex));

  return {
    vmd: {
      file: options.vmd,
      modelName: vmd.modelName,
      boneName,
      boneFrameCount: frames.length,
      maxFrame: Math.max(...frames.map((frame) => frame.frame), 0),
    },
    pmm: {
      file: options.pmm,
      byteLength: pmm.byteLength,
      recordByteLength: pmm.recordByteLength,
      recordTotal: pmm.recordTotal,
      summary: pmm.summary,
      truncated: pmm.truncated,
    },
    mapping: mappings,
    coverage: {
      framesWithExactFrameRecord: mappings.filter((mapping) => mapping.frameRecord).length,
      framesWithPositionEvidence: mappings.filter((mapping) => hasCompleteNonZeroEvidence(mapping.positionEvidence, mapping.position)).length,
      framesWithLocalPositionEvidence: mappings.filter((mapping) => hasCompleteNonZeroEvidence(mapping.localPositionEvidence, mapping.position)).length,
      framesWithRotationEvidence: mappings.filter((mapping) => mapping.rotationEvidence.length > 0).length,
      framesWithLocalRotationEvidence: mappings.filter((mapping) => hasCompleteNonZeroEvidence(mapping.localRotationEvidence, mapping.rotation)).length,
      exactFrameRecordOffsets: mappings.filter((mapping) => mapping.frameRecord).map((mapping) => mapping.frameRecord.recordStartHex),
    },
    rotationRecordDeltaSummary: summarizeRelativeEvidence(mappings.flatMap((mapping) => mapping.rotationEvidence)),
    notes: [
      "This is fixture inventory evidence, not a PMM writer.",
      "Frame fields are searched in marker-derived candidate records; rotation values may appear in neighboring records or cache areas.",
    ],
  };
}

export function mapAllVmdBoneFramesToPmmRecords(vmd, pmm, options = {}) {
  const boneNames = options.boneName ? [options.boneName] : uniqueBoneNames(vmd);
  const perBone = boneNames.map((boneName) =>
    mapVmdBoneFrameCoverageToPmmRecords(vmd, pmm, {
      ...options,
      boneName,
    }),
  );
  const totals = perBone.reduce(
    (accumulator, report) => {
      accumulator.boneFrameCount += report.vmd.boneFrameCount;
      accumulator.framesWithExactFrameRecord += report.coverage.framesWithExactFrameRecord;
      accumulator.framesWithPositionEvidence += report.coverage.framesWithPositionEvidence;
      accumulator.framesWithLocalPositionEvidence += report.coverage.framesWithLocalPositionEvidence;
      accumulator.framesWithRotationEvidence += report.coverage.framesWithRotationEvidence;
      accumulator.framesWithLocalRotationEvidence += report.coverage.framesWithLocalRotationEvidence;
      return accumulator;
    },
    {
      boneFrameCount: 0,
      framesWithExactFrameRecord: 0,
      framesWithPositionEvidence: 0,
      framesWithLocalPositionEvidence: 0,
      framesWithRotationEvidence: 0,
      framesWithLocalRotationEvidence: 0,
    },
  );

  return {
    vmd: {
      file: options.vmd,
      modelName: vmd.modelName,
      boneNameCount: boneNames.length,
      boneFrameCount: totals.boneFrameCount,
      maxFrame: Math.max(...vmd.bones.map((frame) => frame.frame), 0),
      boneNameCounts: vmd.boneNameCounts,
    },
    pmm: {
      file: options.pmm,
      byteLength: pmm.byteLength,
      recordByteLength: pmm.recordByteLength,
      recordTotal: pmm.recordTotal,
      summary: pmm.summary,
      truncated: pmm.truncated,
    },
    coverage: {
      ...totals,
      exactFrameRecordRatio: ratio(totals.framesWithExactFrameRecord, totals.boneFrameCount),
      localPositionEvidenceRatio: ratio(totals.framesWithLocalPositionEvidence, totals.boneFrameCount),
      localRotationEvidenceRatio: ratio(totals.framesWithLocalRotationEvidence, totals.boneFrameCount),
    },
    perBone,
    notes: [
      "This is fixture inventory evidence, not a PMM writer.",
      "All-bone mode reports candidate coverage per VMD bone name; it does not yet bind PMM records to PMM model slots.",
    ],
  };
}

export function mapVmdBoneFrameCoverageToPmmRecords(vmd, pmm, options = {}) {
  const boneName = options.boneName ?? inferSingleBoneName(vmd);
  const frames = vmd.bones.filter((frame) => frame.name === boneName).sort((left, right) => left.frame - right.frame);
  const records = pmm.records;
  const searchIndex = createRecordSearchIndex(records);
  const coverage = {
    framesWithExactFrameRecord: 0,
    framesWithPositionEvidence: 0,
    framesWithLocalPositionEvidence: 0,
    framesWithRotationEvidence: 0,
    framesWithLocalRotationEvidence: 0,
    exactFrameRecordOffsets: [],
  };

  for (const frame of frames) {
    const frameRecord = findBestFrameRecord(frame, records, searchIndex, options.frameMatchLimit ?? records.length);
    if (!frameRecord) {
      continue;
    }
    coverage.framesWithExactFrameRecord += 1;
    coverage.exactFrameRecordOffsets.push(frameRecord.recordStartHex);
    const record = searchIndex.recordByStart.get(frameRecord.recordStart);
    if (recordHasCompleteNonZeroVector(record, frame.position)) {
      coverage.framesWithPositionEvidence += 1;
      coverage.framesWithLocalPositionEvidence += 1;
    }
    if (!isIdentityRotation(frame.rotation) && recordHasCompleteNonZeroVector(record, frame.rotation)) {
      coverage.framesWithRotationEvidence += 1;
      coverage.framesWithLocalRotationEvidence += 1;
    }
  }

  return {
    vmd: {
      file: options.vmd,
      modelName: vmd.modelName,
      boneName,
      boneFrameCount: frames.length,
      maxFrame: Math.max(...frames.map((frame) => frame.frame), 0),
    },
    pmm: {
      file: options.pmm,
      byteLength: pmm.byteLength,
      recordByteLength: pmm.recordByteLength,
      recordTotal: pmm.recordTotal,
      summary: pmm.summary,
      truncated: pmm.truncated,
    },
    coverage,
  };
}

function mapFrame(frame, records, options, searchIndex) {
  const frameRecord = findBestFrameRecord(frame, records, searchIndex, options.frameMatchLimit ?? records.length);
  const localRecords = frameRecord ? [searchIndex.recordByStart.get(frameRecord.recordStart)].filter(Boolean) : [];
  const localSearchIndex = createRecordSearchIndex(localRecords);
  const matchLimit = options.matchLimit ?? 8;
  const includeRotationEvidence = options.includeRotationEvidence !== false;
  return {
    name: frame.name,
    frame: frame.frame,
    position: frame.position,
    rotation: frame.rotation,
    frameRecord,
    shiftedFrameRecords: findRecordsWithU32(records, frame.frame * 65536, matchLimit, searchIndex),
    positionEvidence: addFrameRecordDelta(findVectorEvidence(records, frame.position, matchLimit, searchIndex), frameRecord),
    localPositionEvidence: addFrameRecordDelta(findVectorEvidence(localRecords, frame.position, matchLimit, localSearchIndex), frameRecord),
    rotationEvidence: includeRotationEvidence ? addFrameRecordDelta(findVectorEvidence(records, frame.rotation, matchLimit, searchIndex), frameRecord) : [],
    localRotationEvidence: includeRotationEvidence
      ? addFrameRecordDelta(findVectorEvidence(localRecords, frame.rotation, matchLimit, localSearchIndex), frameRecord)
      : [],
  };
}

function findBestFrameRecord(frame, records, searchIndex, limit) {
  const matches = findRecordsWithU32(records, frame.frame, limit, searchIndex);
  if (matches.length === 0) {
    return undefined;
  }
  return [...matches].sort((left, right) => compareFrameRecordCandidates(left, right, frame, searchIndex))[0];
}

function compareFrameRecordCandidates(left, right, frame, searchIndex) {
  return scoreFrameRecordCandidate(right, frame, searchIndex) - scoreFrameRecordCandidate(left, frame, searchIndex) || compareRecordMatches(left, right);
}

function scoreFrameRecordCandidate(match, frame, searchIndex) {
  const record = searchIndex?.recordByStart.get(match.recordStart);
  const hasLocalPosition = record ? recordHasCompleteNonZeroVector(record, frame.position) : false;
  return (hasLocalPosition ? 10_000 : 0) + frameOffsetScore(match.offset);
}

function recordHasCompleteNonZeroVector(record, values, epsilon = 0.00001) {
  if (!record) {
    return false;
  }
  if (countNonZeroComponents(values) === 0) {
    return false;
  }
  const raw = Buffer.from(record.rawHex, "hex");
  return values.every((expected) => {
    if (Math.abs(expected) < epsilon) {
      return true;
    }
    for (let offset = 0; offset + 4 <= raw.byteLength; offset += 1) {
      const value = raw.readFloatLE(offset);
      if (Number.isFinite(value) && Math.abs(value - expected) <= epsilon) {
        return true;
      }
    }
    return false;
  });
}

function frameOffsetScore(offset) {
  if (offset === 14) {
    return 1_000;
  }
  if (offset === 2) {
    return 900;
  }
  if (offset === 12) {
    return 800;
  }
  return 0;
}

function findRecordsWithU32(records, value, limit, searchIndex) {
  const indexed = searchIndex?.u32.get(String(value));
  if (indexed) {
    return indexed.slice(0, limit);
  }
  const matches = [];
  for (const record of records) {
    const raw = Buffer.from(record.rawHex, "hex");
    for (let offset = 0; offset + 4 <= raw.byteLength; offset += 1) {
      if (raw.readUInt32LE(offset) !== value) {
        continue;
      }
      matches.push(describeRecordMatch(record, offset, { u32: value }));
      if (matches.length >= limit) {
        return matches;
      }
    }
  }
  return matches;
}

function findRecordsWithApproxFloat(records, expected, limit, searchIndex, epsilon = 0.00001) {
  const indexed = findIndexedApproxFloats(expected, limit, searchIndex, epsilon);
  if (indexed.length > 0) {
    return indexed;
  }
  const matches = [];
  for (const record of records) {
    const raw = Buffer.from(record.rawHex, "hex");
    for (let offset = 0; offset + 4 <= raw.byteLength; offset += 1) {
      const value = raw.readFloatLE(offset);
      if (!Number.isFinite(value) || Math.abs(value - expected) > epsilon) {
        continue;
      }
      matches.push(describeRecordMatch(record, offset, { f32: roundFloat(value) }));
      if (matches.length >= limit) {
        return matches;
      }
    }
  }
  return matches;
}

function findIndexedApproxFloats(expected, limit, searchIndex, epsilon) {
  if (!searchIndex) {
    return [];
  }
  const keys = new Set([roundFloat(expected), roundFloat(expected - epsilon), roundFloat(expected + epsilon)].map(String));
  const matches = [];
  for (const key of keys) {
    for (const match of searchIndex.f32.get(key) ?? []) {
      if (Math.abs(match.f32 - expected) > epsilon) {
        continue;
      }
      matches.push(match);
      if (matches.length >= limit) {
        return matches.sort(compareRecordMatches);
      }
    }
  }
  return matches.sort(compareRecordMatches).slice(0, limit);
}

function describeRecordMatch(record, offset, fields) {
  return {
    recordStart: record.recordStart,
    recordStartHex: record.recordStartHex,
    offset,
    offsetHex: hexOffset(offset),
    absoluteOffset: record.recordStart + offset,
    absoluteOffsetHex: hexOffset(record.recordStart + offset),
    ...fields,
  };
}

function findVectorEvidence(records, values, limit, searchIndex) {
  return values.flatMap((value, index) =>
    Math.abs(value) < 0.00001
      ? []
      : findRecordsWithApproxFloat(records, value, limit, searchIndex).map((match) => ({
          component: index,
          expected: value,
          ...match,
        })),
  );
}

function createRecordSearchIndex(records) {
  const index = {
    recordByStart: new Map(),
    u32: new Map(),
    f32: new Map(),
  };
  for (const record of records) {
    index.recordByStart.set(record.recordStart, record);
    const raw = Buffer.from(record.rawHex, "hex");
    for (let offset = 0; offset + 4 <= raw.byteLength; offset += 1) {
      const u32 = raw.readUInt32LE(offset);
      addIndexMatch(index.u32, u32, describeRecordMatch(record, offset, { u32 }));
      const value = raw.readFloatLE(offset);
      if (Number.isFinite(value)) {
        addIndexMatch(index.f32, roundFloat(value), describeRecordMatch(record, offset, { f32: roundFloat(value) }));
      }
    }
  }
  return index;
}

function addIndexMatch(map, value, match) {
  const key = String(value);
  const matches = map.get(key) ?? [];
  matches.push(match);
  map.set(key, matches);
}

function compareRecordMatches(left, right) {
  return left.recordStart - right.recordStart || left.offset - right.offset;
}

function addFrameRecordDelta(matches, frameRecord) {
  if (!frameRecord) {
    return matches;
  }
  return matches.map((match) => ({
    ...match,
    frameRecordDelta: match.recordStart - frameRecord.recordStart,
    frameRecordDeltaHex: signedHexOffset(match.recordStart - frameRecord.recordStart),
  }));
}

function summarizeRelativeEvidence(matches) {
  const buckets = new Map();
  for (const match of matches) {
    if (match.frameRecordDelta === undefined) {
      continue;
    }
    const key = `${match.component}:${match.frameRecordDelta}:${match.offset}`;
    const bucket = buckets.get(key) ?? {
      component: match.component,
      frameRecordDelta: match.frameRecordDelta,
      frameRecordDeltaHex: match.frameRecordDeltaHex,
      offset: match.offset,
      offsetHex: match.offsetHex,
      count: 0,
      expectedValues: [],
    };
    bucket.count += 1;
    if (!bucket.expectedValues.includes(match.expected)) {
      bucket.expectedValues.push(match.expected);
    }
    buckets.set(key, bucket);
  }
  return [...buckets.values()].sort(
    (left, right) =>
      right.count - left.count ||
      Math.abs(left.frameRecordDelta) - Math.abs(right.frameRecordDelta) ||
      left.component - right.component ||
      left.offset - right.offset,
  );
}

function countMatchedComponents(matches) {
  return new Set(matches.map((match) => match.component)).size;
}

function hasCompleteNonZeroEvidence(matches, values) {
  const nonZeroComponents = countNonZeroComponents(values);
  return nonZeroComponents > 0 && countMatchedComponents(matches) === nonZeroComponents;
}

function countNonZeroComponents(values) {
  return values.filter((value) => Math.abs(value) >= 0.00001).length;
}

function isIdentityRotation(rotation, epsilon = 0.00001) {
  return (
    Math.abs(rotation[0]) < epsilon &&
    Math.abs(rotation[1]) < epsilon &&
    Math.abs(rotation[2]) < epsilon &&
    Math.abs(rotation[3] - 1) < epsilon
  );
}

function inferSingleBoneName(vmd) {
  const names = new Set(vmd.bones.map((frame) => frame.name));
  if (names.size !== 1) {
    throw new Error(`PMM/VMD bone map needs --bone-name when VMD has ${names.size} bone names.`);
  }
  return [...names][0];
}

function uniqueBoneNames(vmd) {
  return [...new Set(vmd.bones.map((frame) => frame.name))].sort((left, right) => left.localeCompare(right, "ja"));
}

function ratio(numerator, denominator) {
  return denominator === 0 ? null : roundFloat(numerator / denominator);
}

function roundFloat(value) {
  return Math.round(value * 1_000_000) / 1_000_000;
}

function hexOffset(value) {
  return `0x${value.toString(16)}`;
}

function signedHexOffset(value) {
  return value < 0 ? `-0x${Math.abs(value).toString(16)}` : `0x${value.toString(16)}`;
}

function requireString(options, key) {
  if (!options[key]) {
    throw new Error(`Missing ${key}.`);
  }
  return options[key];
}
