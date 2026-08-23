import { readFile } from "node:fs/promises";
import { analyzePmmVmdDiffClusters } from "./pmm-vmd-diff-clusters.mjs";
import { parsePmmManifest } from "./pmm-manifest.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function readPmmVmdKeyframes(options) {
  const baseFile = requireString(options, "base");
  const variantFile = requireString(options, "variant");
  const vmdFile = requireString(options, "vmd");
  const [baseBytes, variantBytes, vmd] = await Promise.all([
    readFile(baseFile),
    readFile(variantFile),
    readVmdInventory(vmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  return extractPmmVmdKeyframes(baseBytes, variantBytes, vmd, {
    ...options,
    baseFile,
    variantFile,
    vmdFile,
  });
}

export async function readPmmVmdKeyframeComparison(options) {
  const baseFile = requireString(options, "base");
  const variantFile = requireString(options, "variant");
  const vmdFile = requireString(options, "vmd");
  const [baseBytes, variantBytes, vmd] = await Promise.all([
    readFile(baseFile),
    readFile(variantFile),
    readVmdInventory(vmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  return comparePmmVmdKeyframes(baseBytes, variantBytes, vmd, {
    ...options,
    baseFile,
    variantFile,
    vmdFile,
  });
}

export async function readPmmKeyframesWithProfile(options) {
  const pmmFile = requireString(options, "pmm");
  const profileFile = requireString(options, "profile");
  const [pmmBytes, profileText] = await Promise.all([readFile(pmmFile), readFile(profileFile, "utf8")]);
  const profileInput = JSON.parse(profileText);
  const profile = profileInput.profile ?? profileInput;
  return extractPmmKeyframesWithProfile(pmmBytes, profile, {
    ...options,
    pmmFile,
    profileFile,
  });
}

export async function readPmmKeyframesWithProfileRegistry(options) {
  const pmmFile = requireString(options, "pmm");
  const registryFile = requireString(options, "registry");
  const [pmmBytes, registryText] = await Promise.all([readFile(pmmFile), readFile(registryFile, "utf8")]);
  const registry = JSON.parse(registryText);
  return extractPmmKeyframesWithProfileRegistry(pmmBytes, registry, {
    ...options,
    pmmFile,
    registryFile,
  });
}

export async function readPmmKeyframeProfileCheck(options) {
  const pmmFile = requireString(options, "pmm");
  const profileFile = requireString(options, "profile");
  const [pmmBytes, profileText] = await Promise.all([readFile(pmmFile), readFile(profileFile, "utf8")]);
  const profileInput = JSON.parse(profileText);
  const profile = profileInput.profile ?? profileInput;
  return checkPmmKeyframeProfile(pmmBytes, profile, {
    ...options,
    pmmFile,
    profileFile,
  });
}

export async function readPmmKeyframeProfileRegistryCheck(options) {
  const pmmFile = requireString(options, "pmm");
  const registryFile = requireString(options, "registry");
  const [pmmBytes, registryText] = await Promise.all([readFile(pmmFile), readFile(registryFile, "utf8")]);
  const registry = JSON.parse(registryText);
  return checkPmmKeyframeProfileRegistry(pmmBytes, registry, {
    ...options,
    pmmFile,
    registryFile,
  });
}

export async function readPmmKeyframesWithProfileComparison(options) {
  const pmmFile = requireString(options, "pmm");
  const profileFile = requireString(options, "profile");
  const vmdFile = requireString(options, "vmd");
  const [pmmBytes, profileText, vmd] = await Promise.all([
    readFile(pmmFile),
    readFile(profileFile, "utf8"),
    readVmdInventory(vmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  const profileInput = JSON.parse(profileText);
  const profile = profileInput.profile ?? profileInput;
  return comparePmmKeyframesWithProfile(pmmBytes, profile, vmd, {
    ...options,
    pmmFile,
    profileFile,
    vmdFile,
  });
}

export function extractPmmVmdKeyframes(baseBytes, variantBytes, vmd, options = {}) {
  const modelSlots = options.modelSlots ?? readModelSlots(baseBytes);
  const cluster = analyzePmmVmdDiffClusters(baseBytes, variantBytes, vmd, {
    context: options.context ?? 16,
    diffLimit: options.diffLimit ?? 128,
    matchLimit: options.matchLimit ?? 1024,
    candidateLimit: options.candidateLimit ?? 1,
    sequenceLimit: options.sequenceLimit ?? 16,
    recordByteLength: options.recordByteLength ?? 62,
    frameOffsetInRecord: options.frameOffsetInRecord ?? 8,
    modelSlots,
  });
  const frameSequenceProfile = cluster.frameSequenceBlockProfile;
  if (!frameSequenceProfile?.verified) {
    throw new Error(`PMM/VMD frame sequence profile is not verified: ${(frameSequenceProfile?.reasons ?? []).join(" ")}`);
  }
  const positionProfile = cluster.positionKeyBlockProfile;
  const transformProfile = cluster.transformKeyBlockProfile;
  const positionOffsetInRecord = positionProfile.verified ? positionProfile.positionOffsetInRecord : options.positionOffsetInRecord;
  const rotationOffsetInRecord = transformProfile.verified ? transformProfile.rotationOffsetInRecord : options.rotationOffsetInRecord;
  const records = frameSequenceProfile.records.map((record, index) => {
    const frameOffset = record.recordStart + frameSequenceProfile.frameOffsetInRecord;
    const positionOffset = Number.isInteger(positionOffsetInRecord) ? record.recordStart + positionOffsetInRecord : undefined;
    const rotationOffset = Number.isInteger(rotationOffsetInRecord) ? record.recordStart + rotationOffsetInRecord : undefined;
    return {
      index,
      name: record.name,
      frame: variantBytes.readUInt32LE(frameOffset),
      recordStart: record.recordStart,
      recordStartHex: record.recordStartHex,
      frameOffset,
      frameOffsetHex: hex(frameOffset),
      position: positionOffset === undefined ? undefined : readFloatVector(variantBytes, positionOffset, 3),
      positionOffset,
      positionOffsetHex: positionOffset === undefined ? undefined : hex(positionOffset),
      rotation: rotationOffset === undefined ? undefined : readFloatVector(variantBytes, rotationOffset, 4),
      rotationOffset,
      rotationOffsetHex: rotationOffset === undefined ? undefined : hex(rotationOffset),
    };
  });
  return {
    baseFile: options.baseFile,
    variantFile: options.variantFile,
    vmdFile: options.vmdFile,
    profile: {
      verified: true,
      recordByteLength: frameSequenceProfile.recordByteLength,
      recordCount: frameSequenceProfile.recordCount,
      blockStart: frameSequenceProfile.blockStart,
      blockStartHex: frameSequenceProfile.blockStartHex,
      blockEnd: frameSequenceProfile.blockEnd,
      blockEndHex: frameSequenceProfile.blockEndHex,
      frameOffsetInRecord: frameSequenceProfile.frameOffsetInRecord,
      positionOffsetInRecord,
      positionVerified: Boolean(positionProfile.verified),
      rotationOffsetInRecord,
      rotationVerified: Boolean(transformProfile.verified),
      modelSlotContext: frameSequenceProfile.modelSlotContext,
      boneSpans: frameSequenceProfile.boneSpans,
      records: frameSequenceProfile.records.map((record) => ({
        index: record.index,
        name: record.name,
        recordStart: record.recordStart,
        recordStartHex: record.recordStartHex,
        frameOffset: record.recordStart + frameSequenceProfile.frameOffsetInRecord,
        frameOffsetHex: hex(record.recordStart + frameSequenceProfile.frameOffsetInRecord),
      })),
    },
    coverage: cluster.coverage,
    records,
  };
}

export function extractPmmKeyframesWithProfile(pmmBytes, profile, options = {}) {
  validateProfile(profile);
  const profileRecords = profile.records ?? recordsFromBoneSpans(profile);
  const records = profileRecords.map((record, index) => {
    const recordStart = numberFromProfile(record.recordStart, `profile.records[${index}].recordStart`);
    const frameOffset = numberFromProfile(record.frameOffset, `profile.records[${index}].frameOffset`, recordStart + profile.frameOffsetInRecord);
    const positionOffset = Number.isInteger(profile.positionOffsetInRecord) ? recordStart + profile.positionOffsetInRecord : undefined;
    const rotationOffset = Number.isInteger(profile.rotationOffsetInRecord) ? recordStart + profile.rotationOffsetInRecord : undefined;
    ensureReadable(pmmBytes, frameOffset, 4, "frame");
    if (positionOffset !== undefined) {
      ensureReadable(pmmBytes, positionOffset, 12, "position");
    }
    if (rotationOffset !== undefined) {
      ensureReadable(pmmBytes, rotationOffset, 16, "rotation");
    }
    return {
      index,
      name: record.name,
      frame: pmmBytes.readUInt32LE(frameOffset),
      recordStart,
      recordStartHex: hex(recordStart),
      frameOffset,
      frameOffsetHex: hex(frameOffset),
      position: positionOffset === undefined ? undefined : readFloatVector(pmmBytes, positionOffset, 3),
      positionOffset,
      positionOffsetHex: positionOffset === undefined ? undefined : hex(positionOffset),
      rotation: rotationOffset === undefined ? undefined : readFloatVector(pmmBytes, rotationOffset, 4),
      rotationOffset,
      rotationOffsetHex: rotationOffset === undefined ? undefined : hex(rotationOffset),
    };
  });
  return {
    ok: true,
    pmmFile: options.pmmFile,
    profileFile: options.profileFile,
    profile: {
      ...profile,
      records: profileRecords,
    },
    records,
  };
}

export function checkPmmKeyframeProfile(pmmBytes, profile, options = {}) {
  const reasons = [];
  let profileRecords = [];
  try {
    validateProfile(profile);
    profileRecords = profile.records ?? recordsFromBoneSpans(profile);
  } catch (error) {
    reasons.push(error.message);
  }
  if (!Buffer.isBuffer(pmmBytes)) {
    reasons.push("PMM bytes must be a Buffer.");
  }
  const pmmByteLength = Buffer.isBuffer(pmmBytes) ? pmmBytes.byteLength : 0;
  if (Number.isInteger(profile?.recordCount) && profileRecords.length !== profile.recordCount) {
    reasons.push(`Profile recordCount ${profile.recordCount} does not match record list length ${profileRecords.length}.`);
  }
  if (Number.isInteger(profile?.blockStart) && (profile.blockStart < 0 || profile.blockStart > pmmByteLength)) {
    reasons.push(`Profile blockStart is outside PMM bytes: ${profile.blockStart}.`);
  }
  if (Number.isInteger(profile?.blockEnd) && (profile.blockEnd < 0 || profile.blockEnd > pmmByteLength)) {
    reasons.push(`Profile blockEnd is outside PMM bytes: ${profile.blockEnd}.`);
  }
  if (Number.isInteger(profile?.blockStart) && Number.isInteger(profile?.blockEnd) && profile.blockEnd < profile.blockStart) {
    reasons.push(`Profile blockEnd ${profile.blockEnd} is before blockStart ${profile.blockStart}.`);
  }
  for (const [index, record] of profileRecords.entries()) {
    const recordStart = Number.isInteger(record.recordStart) ? record.recordStart : undefined;
    if (recordStart === undefined) {
      reasons.push(`profile.records[${index}].recordStart must be an integer.`);
      continue;
    }
    if (Number.isInteger(profile?.blockStart) && recordStart < profile.blockStart) {
      reasons.push(`profile.records[${index}] starts before blockStart: ${recordStart}.`);
    }
    if (Number.isInteger(profile?.blockEnd) && recordStart + profile.recordByteLength > profile.blockEnd) {
      reasons.push(`profile.records[${index}] extends beyond blockEnd: ${recordStart}.`);
    }
    collectReadableReason(reasons, pmmByteLength, recordStart + profile.frameOffsetInRecord, 4, `profile.records[${index}].frame`);
    if (Number.isInteger(profile?.positionOffsetInRecord)) {
      collectReadableReason(reasons, pmmByteLength, recordStart + profile.positionOffsetInRecord, 12, `profile.records[${index}].position`);
    }
    if (Number.isInteger(profile?.rotationOffsetInRecord)) {
      collectReadableReason(reasons, pmmByteLength, recordStart + profile.rotationOffsetInRecord, 16, `profile.records[${index}].rotation`);
    }
  }
  return {
    ok: reasons.length === 0,
    pmmFile: options.pmmFile,
    profileFile: options.profileFile,
    pmmByteLength,
    profile: {
      verified: Boolean(profile?.verified),
      recordByteLength: profile?.recordByteLength,
      recordCount: profile?.recordCount,
      blockStart: profile?.blockStart,
      blockStartHex: profile?.blockStartHex,
      blockEnd: profile?.blockEnd,
      blockEndHex: profile?.blockEndHex,
      frameOffsetInRecord: profile?.frameOffsetInRecord,
      positionOffsetInRecord: profile?.positionOffsetInRecord,
      rotationOffsetInRecord: profile?.rotationOffsetInRecord,
      modelSlotContext: profile?.modelSlotContext,
    },
    recordsChecked: profileRecords.length,
    reasons,
  };
}

export function checkPmmKeyframeProfileRegistry(pmmBytes, registry, options = {}) {
  const entries = normalizeProfileRegistry(registry);
  const modelSlots = readModelSlots(pmmBytes);
  const candidates = entries.map((entry, index) => {
    const check = checkPmmKeyframeProfile(pmmBytes, entry.profile, options);
    const slot = checkProfileSlotCompatibility(entry.profile, modelSlots);
    const score = (check.ok ? 100 : 0) + (slot.ok ? 20 : 0) + (entry.profile?.verified ? 5 : 0);
    return {
      index,
      id: entry.id,
      source: entry.source,
      ok: check.ok && slot.ok,
      checkOk: check.ok,
      slotOk: slot.ok,
      score,
      slot,
      recordsChecked: check.recordsChecked,
      profile: check.profile,
      reasons: [...check.reasons, ...slot.reasons],
    };
  });
  const sorted = candidates.toSorted((left, right) => {
    if (right.score !== left.score) {
      return right.score - left.score;
    }
    return left.index - right.index;
  });
  return {
    ok: sorted.some((candidate) => candidate.ok),
    pmmFile: options.pmmFile,
    registryFile: options.registryFile,
    profileCount: entries.length,
    modelSlots: modelSlots.map(({ slot, fileName, path, offset, offsetHex }) => ({ slot, fileName, path, offset, offsetHex })),
    candidates: sorted.slice(0, options.limit ?? 32),
    truncated: sorted.length > (options.limit ?? 32),
  };
}

export function extractPmmKeyframesWithProfileRegistry(pmmBytes, registry, options = {}) {
  const entries = normalizeProfileRegistry(registry);
  const report = checkPmmKeyframeProfileRegistry(pmmBytes, registry, options);
  const selected = report.candidates.find((candidate) => candidate.ok);
  if (!selected) {
    return {
      ok: false,
      pmmFile: options.pmmFile,
      registryFile: options.registryFile,
      profileCount: report.profileCount,
      candidates: report.candidates,
      truncated: report.truncated,
      records: [],
      reasons: ["No compatible PMM keyframe profile was found."],
    };
  }
  const entry = entries[selected.index];
  const decoded = extractPmmKeyframesWithProfile(pmmBytes, entry.profile, options);
  return {
    ok: true,
    pmmFile: options.pmmFile,
    registryFile: options.registryFile,
    selectedProfile: selected,
    records: decoded.records,
  };
}

export function comparePmmVmdKeyframes(baseBytes, variantBytes, vmd, options = {}) {
  const extracted = extractPmmVmdKeyframes(baseBytes, variantBytes, vmd, options);
  return compareExtractedPmmKeyframesToVmd(extracted, vmd, options, {
    baseFile: options.baseFile,
    variantFile: options.variantFile,
  });
}

export function comparePmmKeyframesWithProfile(pmmBytes, profile, vmd, options = {}) {
  const extracted = extractPmmKeyframesWithProfile(pmmBytes, profile, options);
  return compareExtractedPmmKeyframesToVmd(extracted, vmd, options, {
    pmmFile: options.pmmFile,
    profileFile: options.profileFile,
  });
}

function compareExtractedPmmKeyframesToVmd(extracted, vmd, options = {}, files = {}) {
  const expected = vmd.bones ?? [];
  const frameEpsilon = options.frameEpsilon ?? 0;
  const positionEpsilon = options.positionEpsilon ?? 0.00001;
  const rotationEpsilon = options.rotationEpsilon ?? 0.00001;
  const mismatches = [];
  const compareCount = Math.min(extracted.records.length, expected.length);
  if (extracted.records.length !== expected.length) {
    mismatches.push({
      kind: "count",
      expected: expected.length,
      actual: extracted.records.length,
      message: `PMM record count ${extracted.records.length} does not match VMD bone frame count ${expected.length}.`,
    });
  }
  let maxPositionError = 0;
  let maxRotationError = 0;
  for (let index = 0; index < compareCount; index += 1) {
    const actual = extracted.records[index];
    const want = expected[index];
    if (actual.name !== want.name) {
      mismatches.push({ kind: "name", index, expected: want.name, actual: actual.name });
    }
    if (Math.abs(actual.frame - want.frame) > frameEpsilon) {
      mismatches.push({ kind: "frame", index, name: want.name, expected: want.frame, actual: actual.frame });
    }
    if (actual.position === undefined && Array.isArray(want.position)) {
      mismatches.push({ kind: "position-missing", index, name: want.name, frame: want.frame, expected: want.position });
    } else if (actual.position !== undefined) {
      const result = compareVector(actual.position, want.position, positionEpsilon);
      maxPositionError = Math.max(maxPositionError, result.maxError);
      if (!result.ok) {
        mismatches.push({ kind: "position", index, name: want.name, frame: want.frame, ...result });
      }
    }
    if (actual.rotation === undefined && Array.isArray(want.rotation)) {
      mismatches.push({ kind: "rotation-missing", index, name: want.name, frame: want.frame, expected: want.rotation });
    } else if (actual.rotation !== undefined) {
      const result = compareVector(actual.rotation, want.rotation, rotationEpsilon);
      maxRotationError = Math.max(maxRotationError, result.maxError);
      if (!result.ok) {
        mismatches.push({ kind: "rotation", index, name: want.name, frame: want.frame, ...result });
      }
    }
  }
  return {
    ok: mismatches.length === 0,
    ...files,
    vmdFile: options.vmdFile,
    profile: extracted.profile,
    counts: {
      pmmRecords: extracted.records.length,
      vmdBoneFrames: expected.length,
    },
    tolerances: {
      frameEpsilon,
      positionEpsilon,
      rotationEpsilon,
    },
    maxErrors: {
      position: maxPositionError,
      rotation: maxRotationError,
    },
    mismatchCount: mismatches.length,
    mismatches: mismatches.slice(0, options.limit ?? 32),
    truncated: mismatches.length > (options.limit ?? 32),
  };
}

function compareVector(actual = [], expected = [], epsilon) {
  const componentCount = Math.max(actual.length, expected.length);
  const components = [];
  let maxError = 0;
  for (let index = 0; index < componentCount; index += 1) {
    const actualValue = actual[index];
    const expectedValue = expected[index];
    const error = Math.abs(actualValue - expectedValue);
    maxError = Math.max(maxError, Number.isFinite(error) ? error : Number.POSITIVE_INFINITY);
    if (!Number.isFinite(error) || error > epsilon) {
      components.push({ index, expected: expectedValue, actual: actualValue, error });
    }
  }
  return {
    ok: components.length === 0,
    expected,
    actual,
    maxError,
    components,
  };
}

function readFloatVector(bytes, offset, count) {
  return Array.from({ length: count }, (_, index) => roundFloat(bytes.readFloatLE(offset + index * 4)));
}

function validateProfile(profile) {
  if (!profile || typeof profile !== "object") {
    throw new Error("PMM keyframe profile must be an object.");
  }
  if (!Number.isInteger(profile.recordByteLength) || profile.recordByteLength <= 0) {
    throw new Error(`PMM keyframe profile needs recordByteLength, got ${profile.recordByteLength}.`);
  }
  if (!Number.isInteger(profile.frameOffsetInRecord)) {
    throw new Error(`PMM keyframe profile needs frameOffsetInRecord, got ${profile.frameOffsetInRecord}.`);
  }
  if (!Array.isArray(profile.records) && !Array.isArray(profile.boneSpans)) {
    throw new Error("PMM keyframe profile needs records or boneSpans.");
  }
}

function recordsFromBoneSpans(profile) {
  const records = [];
  for (const span of profile.boneSpans ?? []) {
    for (const recordStartHex of span.recordStarts ?? []) {
      const recordStart = parseHexOffset(recordStartHex);
      records.push({
        index: records.length,
        name: span.name,
        recordStart,
        recordStartHex: hex(recordStart),
        frameOffset: recordStart + profile.frameOffsetInRecord,
        frameOffsetHex: hex(recordStart + profile.frameOffsetInRecord),
      });
    }
  }
  return records.sort((left, right) => left.recordStart - right.recordStart).map((record, index) => ({ ...record, index }));
}

function parseHexOffset(value) {
  if (typeof value !== "string" || !value.startsWith("0x")) {
    throw new Error(`Expected hex offset string, got ${value}`);
  }
  const parsed = Number.parseInt(value, 16);
  if (!Number.isInteger(parsed)) {
    throw new Error(`Invalid hex offset string: ${value}`);
  }
  return parsed;
}

function numberFromProfile(value, label, fallback) {
  if (Number.isInteger(value)) {
    return value;
  }
  if (Number.isInteger(fallback)) {
    return fallback;
  }
  throw new Error(`${label} must be an integer.`);
}

function ensureReadable(bytes, offset, byteLength, label) {
  if (!Number.isInteger(offset) || offset < 0 || offset + byteLength > bytes.byteLength) {
    throw new Error(`PMM profile ${label} offset is out of range: ${offset}`);
  }
}

function collectReadableReason(reasons, pmmByteLength, offset, byteLength, label) {
  if (!Number.isInteger(offset) || offset < 0 || offset + byteLength > pmmByteLength) {
    reasons.push(`${label} offset is out of PMM byte range: ${offset}.`);
  }
}

function normalizeProfileRegistry(input) {
  if (Array.isArray(input)) {
    return input.map(normalizeProfileEntry);
  }
  if (Array.isArray(input?.profiles)) {
    return input.profiles.map(normalizeProfileEntry);
  }
  if (input?.profile) {
    return [normalizeProfileEntry(input)];
  }
  throw new Error("PMM keyframe profile registry must be a profile object, an array, or an object with profiles.");
}

function normalizeProfileEntry(entry, index) {
  const profile = entry.profile ?? entry;
  return {
    id: entry.id ?? entry.name ?? `profile-${index}`,
    source: entry.source,
    profile,
  };
}

function checkProfileSlotCompatibility(profile, modelSlots) {
  const context = profile?.modelSlotContext;
  if (!context || !Number.isInteger(context.slot)) {
    return { ok: true, method: "no-profile-slot", reasons: [] };
  }
  const slot = modelSlots.find((candidate) => candidate.slot === context.slot);
  if (!slot) {
    return {
      ok: false,
      expectedSlot: context.slot,
      method: "slot-index",
      reasons: [`Profile expects model slot ${context.slot}, but the PMM has no such slot.`],
    };
  }
  const reasons = [];
  if (context.fileName && slot.fileName && context.fileName !== slot.fileName) {
    reasons.push(`Profile slot fileName ${context.fileName} does not match PMM slot fileName ${slot.fileName}.`);
  }
  if (Number.isInteger(context.modelPathOffset) && Number.isInteger(slot.offset) && context.modelPathOffset !== slot.offset) {
    reasons.push(`Profile slot offset ${context.modelPathOffset} does not match PMM slot offset ${slot.offset}.`);
  }
  return {
    ok: reasons.length === 0,
    expectedSlot: context.slot,
    actualSlot: {
      slot: slot.slot,
      fileName: slot.fileName,
      path: slot.path,
      offset: slot.offset,
      offsetHex: slot.offsetHex,
    },
    method: "slot-index-file-offset",
    reasons,
  };
}

function roundFloat(value) {
  return Math.round(value * 1_000_000) / 1_000_000;
}

function readModelSlots(bytes) {
  try {
    return parsePmmManifest(bytes).modelSlots ?? [];
  } catch {
    return [];
  }
}

function hex(value) {
  return `0x${value.toString(16)}`;
}

function requireString(options, key) {
  if (!options?.[key]) {
    throw new Error(`Missing ${key}`);
  }
  return options[key];
}
