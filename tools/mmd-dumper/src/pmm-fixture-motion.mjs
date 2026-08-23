import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { diffPmmBytes } from "./pmm-diff.mjs";
import { extractPmmMotionRecords } from "./pmm-motion-records.mjs";
import { mapVmdBoneFrameCoverageToPmmRecords, mapVmdBoneFramesToPmmRecords } from "./pmm-vmd-bone-map.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function readPmmFixtureMotionReport(options) {
  const baseFile = requireString(options, "base");
  const variantFile = requireString(options, "variant");
  const [baseBytes, variantBytes, vmd] = await Promise.all([
    readFile(baseFile),
    readFile(variantFile),
    options.vmd ? readVmdInventory(options.vmd, { limit: options.limit ?? 64 }) : undefined,
  ]);

  return analyzePmmFixtureMotion(baseBytes, variantBytes, {
    ...options,
    baseFile,
    variantFile,
    vmdFile: options.vmd,
    vmd,
  });
}

export async function writePmmFixtureMotionPatch(options) {
  const baseFile = requireString(options, "base");
  const donorBaseFile = requireString(options, "donorBase");
  const donorVariantFile = requireString(options, "donorVariant");
  const outFile = requireString(options, "out");
  const [baseBytes, donorBaseBytes, donorVariantBytes] = await Promise.all([
    readFile(baseFile),
    readFile(donorBaseFile),
    readFile(donorVariantFile),
  ]);
  const result = patchPmmFixtureMotion(baseBytes, donorBaseBytes, donorVariantBytes, options);
  await writeFile(outFile, result.bytes);
  return {
    ...withoutBytes(result),
    baseFile,
    donorBaseFile,
    donorVariantFile,
    outFile,
  };
}

export async function writePmmScalarRewrite(options) {
  const file = requireString(options, "file");
  const outFile = requireString(options, "out");
  const bytes = await readFile(file);
  const result = rewritePmmScalars(bytes, options);
  await writeFile(outFile, result.bytes);
  return {
    ...withoutBytes(result),
    file,
    outFile,
  };
}

export async function writePmmUnittestBoneKeys(options) {
  const templateFile = requireString(options, "template");
  const outFile = requireString(options, "out");
  const [bytes, oracleBytes] = await Promise.all([readFile(templateFile), options.oracle ? readFile(options.oracle) : undefined]);
  if (options.validateTemplate !== false) {
    validatePmmUnittestOneBoneKeyTemplate(bytes, templateFile);
  }
  const result = patchPmmUnittestBoneKeys(bytes, options);
  await writeFile(outFile, result.bytes);
  return {
    ...withoutBytes(result),
    templateFile,
    outFile,
    oracleFile: options.oracle,
    oracleComparison: oracleBytes ? compareOracleBytes(result.bytes, oracleBytes) : undefined,
  };
}

export async function writePmmUnittestVmdBoneKeys(options) {
  const templateFile = requireString(options, "template");
  const vmdFile = requireString(options, "vmd");
  const outFile = requireString(options, "out");
  const [bytes, vmd, oracleBytes] = await Promise.all([
    readFile(templateFile),
    readVmdInventory(vmdFile, { limit: options.limit ?? Number.POSITIVE_INFINITY }),
    options.oracle ? readFile(options.oracle) : undefined,
  ]);
  if (options.validateTemplate !== false) {
    validatePmmUnittestOneBoneKeyTemplate(bytes, templateFile);
  }
  const result = patchPmmUnittestVmdBoneKeys(bytes, vmd, options);
  await writeFile(outFile, result.bytes);
  return {
    ...withoutBytes(result),
    templateFile,
    vmdFile,
    outFile,
    oracleFile: options.oracle,
    oracleComparison: oracleBytes ? compareOracleBytes(result.bytes, oracleBytes) : undefined,
    sourceCounts: vmd.counts,
  };
}

export function patchPmmUnittestVmdBoneKeys(bytes, vmd, options = {}) {
  const keys = options.allowNonIdentityRotation ? extractUnittestBoneTransformKeysFromVmd(vmd, options) : extractUnittestBonePositionKeysFromVmd(vmd, options);
  const result = keys.some((key) => !isIdentityRotation(key.rotation))
    ? patchPmmUnittestBoneTransformKeys(bytes, { keys })
    : patchPmmUnittestBoneKeys(bytes, { keys });
  const boneName = options.boneName ?? keys[0]?.name;
  const generatedMapping = mapGeneratedUnittestBoneKeyPmm(result.bytes, vmd, {
    boneName,
    keys,
    keyCount: keys.length,
    markerHex: options.generatedMarkerHex ?? "14146b6b",
    recordByteLength: options.generatedRecordByteLength ?? 62,
    recordLimit: options.generatedRecordLimit ?? Math.max(keys.length + 8, 16),
    matchLimit: options.generatedMatchLimit ?? 8,
  });
  if (options.requireGeneratedMapping && !generatedMapping.structurallyVerified) {
    throw new Error(
      `Generated PMM key mapping verification failed: framesWithExactFrameRecord=${generatedMapping.coverage.framesWithExactFrameRecord}, framesWithLocalPositionEvidence=${generatedMapping.coverage.framesWithLocalPositionEvidence}, keyCount=${keys.length}.`,
    );
  }

  return {
    ...result,
    boneName,
    generatedMapping,
  };
}

function validatePmmUnittestOneBoneKeyTemplate(bytes, file = "template PMM") {
  if (bytes.byteLength < 0x346) {
    throw new Error(`PMM unittest writer requires the one-bone key template layout; ${file} is too small (${bytes.byteLength} bytes).`);
  }
  const header = bytes.subarray(0, 24).toString("latin1").replace(/\0+$/, "");
  if (header !== "Polygon Movie maker 0002") {
    throw new Error(`PMM unittest writer requires an MMD PMM template; ${file} has header ${JSON.stringify(header)}.`);
  }
  const keyCount = bytes.readUInt16LE(0x1ce);
  const firstFrame = bytes.readUInt32LE(0x1d6);
  const firstKeyControl = bytes.subarray(0x1e2, 0x1e6).toString("hex");
  if (keyCount < 1 || firstFrame > 0xffff || firstKeyControl !== "14141414") {
    throw new Error(
      `PMM unittest writer requires the hand-made one-bone key template layout; ${file} has keyCount=${keyCount}, firstFrame=${firstFrame}, control=${firstKeyControl}.`,
    );
  }
}

export function extractUnittestBonePositionKeysFromVmd(vmd, options = {}) {
  validateUnittestVmdBoneOnly(vmd, options);
  const boneName = options.boneName ?? inferSingleBoneName(vmd);
  const frames = vmd.bones.filter((frame) => frame.name === boneName).sort((left, right) => left.frame - right.frame);
  if (frames.length < 1) {
    throw new Error(`PMM unittest VMD writer needs at least one frame for ${boneName}.`);
  }
  const rotated = frames.find((frame) => !isIdentityRotation(frame.rotation));
  if (rotated) {
    const allowNote = options.allowNonIdentityRotation
      ? " --allow-non-identity-rotation does not enable writing rotation keys."
      : "";
    throw new Error(`PMM unittest VMD writer only supports position keys; ${boneName} frame ${rotated.frame} has non-identity rotation.${allowNote}`);
  }

  return frames.map((frame) => ({
    name: frame.name,
    frame: frame.frame,
    position: frame.position,
  }));
}

function validateUnittestVmdBoneOnly(vmd, options = {}) {
  const unsupportedCounts = [
    ["morphFrames", vmd.counts?.morphFrames ?? 0],
    ["cameraFrames", vmd.counts?.cameraFrames ?? 0],
    ["lightFrames", vmd.counts?.lightFrames ?? 0],
    ["selfShadowFrames", vmd.counts?.selfShadowFrames ?? 0],
    ["propertyFrames", vmd.counts?.propertyFrames ?? 0],
  ].filter(([, count]) => count > 0);
  if (unsupportedCounts.length > 0 && !options.ignoreUnsupported) {
    throw new Error(
      `PMM unittest VMD writer only supports bone position frames; unsupported VMD channels: ${unsupportedCounts
        .map(([name, count]) => `${name}=${count}`)
        .join(", ")}.`,
    );
  }
}

export function extractUnittestBoneTransformKeysFromVmd(vmd, options = {}) {
  validateUnittestVmdBoneOnly(vmd, options);
  const boneName = options.boneName ?? inferSingleBoneName(vmd);
  const frames = vmd.bones.filter((frame) => frame.name === boneName).sort((left, right) => left.frame - right.frame);
  if (frames.length < 1) {
    throw new Error(`PMM unittest VMD writer needs at least one frame for ${boneName}.`);
  }
  return frames.map((frame) => ({
    name: frame.name,
    frame: frame.frame,
    position: frame.position,
    rotation: frame.rotation,
  }));
}

export function patchPmmUnittestBoneKeys(bytes, options = {}) {
  const keys = normalizeUnittestBoneKeys(options.keys ?? []);
  if (keys.length < 1) {
    throw new Error("PMM unittest bone-key writer needs at least one position key.");
  }
  if (keys.length > 0xffff) {
    throw new Error(`PMM unittest bone-key writer supports at most 65535 keys, got ${keys.length}.`);
  }
  if (keys.length === 1 && hasExistingUnittestBoneKey(bytes, keys[0])) {
    const preserved = Buffer.from(bytes);
    return {
      mode: "unittest-bone-position-keys",
      keyCount: keys.length,
      keys,
      maxFrame: keys[0].frame,
      warning: "This is a fixture-specific PMM patcher for the MMD unittest one-bone model, not a general PMM writer.",
      byteLength: preserved.byteLength,
      byteLengthDelta: 0,
      sha256: sha256(preserved),
      replacementCount: 0,
      insertionCount: 0,
      replacements: [],
      insertions: [],
      preservedExistingKey: true,
      bytes: preserved,
    };
  }

  const maxFrame = Math.max(...keys.map((key) => key.frame));
  const rewrite = {
    u32At: [
      { offset: 0x26, value: 483 },
      { offset: 0x190, value: maxFrame },
      { offset: 0x1d6, value: keys[0].frame },
      { offset: 0x33a, value: maxFrame },
      { offset: 0x33e, value: Math.max(0, maxFrame - 15) },
      { offset: 0x342, value: maxFrame },
    ],
    float32At: [
      { offset: 0x1f2, value: keys[0].position[0] },
      { offset: 0x1f6, value: keys[0].position[1] },
      { offset: 0x1fa, value: keys[0].position[2] },
      { offset: 0x236, value: keys[keys.length - 1].position[0] },
      { offset: 0x23a, value: keys[keys.length - 1].position[1] },
      { offset: 0x23e, value: keys[keys.length - 1].position[2] },
    ],
    hexAt: [
      { offset: 0x1e2, hex: keys.length === 1 ? "4000407f4000407f4000407f14146b6b" : "40005757400057574000575714146b6b" },
      { offset: 0x2c9, hex: "01" },
    ],
    insertHexAt: [],
  };

  if (keys.length > 1) {
    rewrite.u32At.push({ offset: 0x1de, value: 2 });
    rewrite.hexAt.push({ offset: 0x1ce, hex: uint16le(keys.length).toString("hex") });
    rewrite.insertHexAt.push({
      offset: 0x20e,
      hex: Buffer.concat(keys.slice(1).map((key, index) => createFollowupUnittestBoneKeyRecord(key, index + 2))).toString("hex"),
    });
  }

  const result = rewritePmmScalars(bytes, rewrite);
  return {
    mode: "unittest-bone-position-keys",
    keyCount: keys.length,
    keys,
    maxFrame,
    warning:
      keys.length > 2
        ? "This is an experimental fixture-specific PMM patcher for position-only follow-up keys; three or more keys do not yet have a hand-made PMM SHA oracle."
        : "This is a fixture-specific PMM patcher for the MMD unittest one-bone model, not a general PMM writer.",
    ...result,
  };
}

export function patchPmmUnittestBoneTransformKeys(bytes, options = {}) {
  const keys = normalizeUnittestBoneTransformKeys(options.keys ?? []);
  if (keys.length < 1) {
    throw new Error("PMM unittest bone-key writer needs at least one transform key.");
  }
  if (keys.length > 0xffff) {
    throw new Error(`PMM unittest bone-key writer supports at most 65535 keys, got ${keys.length}.`);
  }
  if (keys.length === 1 && hasExistingUnittestBoneTransformKey(bytes, keys[0])) {
    const preserved = Buffer.from(bytes);
    return {
      mode: "unittest-bone-transform-keys",
      keyCount: keys.length,
      keys,
      maxFrame: keys[0].frame,
      warning: "This is a fixture-specific PMM patcher for the MMD unittest one-bone model, not a general PMM writer.",
      byteLength: preserved.byteLength,
      byteLengthDelta: 0,
      sha256: sha256(preserved),
      replacementCount: 0,
      insertionCount: 0,
      replacements: [],
      insertions: [],
      preservedExistingKey: true,
      bytes: preserved,
    };
  }

  const maxFrame = Math.max(...keys.map((key) => key.frame));
  const rewrite = {
    u32At: [
      { offset: 0x26, value: 482 },
      { offset: 0x190, value: maxFrame },
      { offset: 0x1d6, value: keys[0].frame },
      { offset: 0x342, value: maxFrame },
    ],
    float32At: [
      { offset: 0x1f2, value: keys[0].position[0] },
      { offset: 0x1f6, value: keys[0].position[1] },
      { offset: 0x1fa, value: keys[0].position[2] },
      { offset: 0x1fe, value: keys[0].rotation[0] },
      { offset: 0x202, value: keys[0].rotation[1] },
      { offset: 0x206, value: keys[0].rotation[2] },
      { offset: 0x20a, value: keys[0].rotation[3] },
    ],
    hexAt: [],
    insertHexAt: [],
  };

  if (keys.length > 1) {
    rewrite.u32At.push({ offset: 0x1de, value: 2 });
    rewrite.hexAt.push({ offset: 0x1ce, hex: uint16le(keys.length).toString("hex") });
    rewrite.insertHexAt.push({
      offset: 0x20e,
      hex: Buffer.concat(keys.slice(1).map((key, index) => createFollowupUnittestBoneTransformKeyRecord(key, index + 2))).toString("hex"),
    });
  }

  const result = rewritePmmScalars(bytes, rewrite);
  return {
    mode: "unittest-bone-transform-keys",
    keyCount: keys.length,
    keys,
    maxFrame,
    warning:
      keys.length > 2
        ? "This is an experimental fixture-specific PMM patcher for transform follow-up keys; three or more keys do not yet have a hand-made PMM SHA oracle."
        : "This is a fixture-specific PMM patcher for the MMD unittest one-bone model, not a general PMM writer.",
    ...result,
  };
}

function normalizeUnittestBoneKeys(keys) {
  const seenFrames = new Set();
  return [...keys]
    .map((key) => {
      if (!Number.isInteger(key.frame) || key.frame < 0 || key.frame > 0xffff) {
        throw new Error(`PMM unittest bone-key frame must be an integer in 0..65535, got ${key.frame}.`);
      }
      if (!Array.isArray(key.position) || key.position.length !== 3 || key.position.some((value) => !Number.isFinite(value))) {
        throw new Error(`PMM unittest bone-key position must contain three finite numbers for frame ${key.frame}.`);
      }
      if (seenFrames.has(key.frame)) {
        throw new Error(`PMM unittest bone-key writer does not support duplicate frame ${key.frame}.`);
      }
      seenFrames.add(key.frame);
      return { ...key, position: [...key.position] };
    })
    .sort((left, right) => left.frame - right.frame);
}

function normalizeUnittestBoneTransformKeys(keys) {
  return normalizeUnittestBoneKeys(keys).map((key) => {
    if (!Array.isArray(key.rotation) || key.rotation.length !== 4 || key.rotation.some((value) => !Number.isFinite(value))) {
      throw new Error(`PMM unittest bone-key rotation must contain four finite numbers for frame ${key.frame}.`);
    }
    return { ...key, rotation: [...key.rotation] };
  });
}

function mapGeneratedUnittestBoneKeyPmm(bytes, vmd, options) {
  const pmm = extractPmmMotionRecords(bytes, {
    markerHex: options.markerHex,
    recordByteLength: options.recordByteLength,
    limit: options.recordLimit,
  });
  if (options.keys?.some((key) => !isIdentityRotation(key.rotation))) {
    const coverage = verifyGeneratedUnittestTransformKeys(bytes, options.keys);
    return {
      markerHex: options.markerHex,
      recordByteLength: pmm.recordByteLength,
      recordTotal: pmm.recordTotal,
      layoutRecordByteLength: 62,
      layoutRecordTotal: options.keys.length,
      coverage,
      structurallyVerified:
        coverage.framesWithExactFrameRecord === options.keyCount && coverage.framesWithLocalRotationEvidence === options.keyCount,
    };
  }
  const report = mapVmdBoneFrameCoverageToPmmRecords(vmd, pmm, {
    boneName: options.boneName,
    frameMatchLimit: options.frameMatchLimit,
  });
  const coverage = report.coverage;
  return {
    markerHex: options.markerHex,
    recordByteLength: pmm.recordByteLength,
    recordTotal: pmm.recordTotal,
    coverage,
    structurallyVerified:
      coverage.framesWithExactFrameRecord === options.keyCount && coverage.framesWithLocalPositionEvidence === options.keyCount,
  };
}

function verifyGeneratedUnittestTransformKeys(bytes, keys) {
  const coverage = {
    framesWithExactFrameRecord: 0,
    framesWithPositionEvidence: 0,
    framesWithLocalPositionEvidence: 0,
    framesWithRotationEvidence: 0,
    framesWithLocalRotationEvidence: 0,
    exactFrameRecordOffsets: [],
  };
  keys.forEach((key, index) => {
    const layout = unittestTransformKeyLayout(index);
    if (bytes.readUInt32LE(layout.frameOffset) === key.frame) {
      coverage.framesWithExactFrameRecord += 1;
      coverage.exactFrameRecordOffsets.push(hexOffset(layout.recordStart));
    }
    if (vectorEqualsAt(bytes, layout.positionOffset, key.position)) {
      coverage.framesWithPositionEvidence += 1;
      coverage.framesWithLocalPositionEvidence += 1;
    }
    if (vectorEqualsAt(bytes, layout.rotationOffset, key.rotation)) {
      coverage.framesWithRotationEvidence += 1;
      coverage.framesWithLocalRotationEvidence += 1;
    }
  });
  return coverage;
}

function unittestTransformKeyLayout(index) {
  if (index === 0) {
    return {
      recordStart: 0x1ce,
      frameOffset: 0x1d6,
      positionOffset: 0x1f2,
      rotationOffset: 0x1fe,
    };
  }
  const recordStart = 0x20e + (index - 1) * 62;
  return {
    recordStart,
    frameOffset: recordStart + 6,
    positionOffset: recordStart + 34,
    rotationOffset: recordStart + 46,
  };
}

function vectorEqualsAt(bytes, offset, values) {
  return values.every((value, index) => approximatelyEqual(bytes.readFloatLE(offset + index * 4), value));
}

function compareOracleBytes(generatedBytes, oracleBytes) {
  return {
    matches: Buffer.compare(generatedBytes, oracleBytes) === 0,
    generatedByteLength: generatedBytes.byteLength,
    oracleByteLength: oracleBytes.byteLength,
    byteLengthDelta: generatedBytes.byteLength - oracleBytes.byteLength,
    generatedSha256: sha256(generatedBytes),
    oracleSha256: sha256(oracleBytes),
  };
}

export function rewritePmmScalars(bytes, options = {}) {
  const patched = Buffer.from(bytes);
  const replacements = [];
  for (const replacement of options.frames ?? []) {
    replacements.push(...replaceExact(patched, uint32le(replacement.from), uint32le(replacement.to), `u32:${replacement.from}->${replacement.to}`));
    replacements.push(
      ...replaceExact(
        patched,
        uint32le(replacement.from * 65536),
        uint32le(replacement.to * 65536),
        `u32:frame<<16:${replacement.from}->${replacement.to}`,
      ),
    );
  }
  for (const replacement of options.float32s ?? []) {
    replacements.push(...replaceExact(patched, float32le(replacement.from), float32le(replacement.to), `f32:${replacement.from}->${replacement.to}`));
  }
  for (const write of options.u32At ?? []) {
    const replacement = uint32le(write.value);
    replacement.copy(patched, write.offset);
    replacements.push(describeOffsetWrite(write.offset, replacement, `u32@${hexOffset(write.offset)}:${write.value}`));
  }
  for (const write of options.float32At ?? []) {
    const replacement = float32le(write.value);
    replacement.copy(patched, write.offset);
    replacements.push(describeOffsetWrite(write.offset, replacement, `f32@${hexOffset(write.offset)}:${write.value}`));
  }
  for (const write of options.hexAt ?? []) {
    const replacement = Buffer.from(write.hex, "hex");
    replacement.copy(patched, write.offset);
    replacements.push(describeOffsetWrite(write.offset, replacement, `hex@${hexOffset(write.offset)}`));
  }
  const inserted = [];
  let resized = patched;
  for (const write of [...(options.insertHexAt ?? [])].sort((left, right) => right.offset - left.offset)) {
    const insertion = Buffer.from(write.hex, "hex");
    resized = Buffer.concat([resized.subarray(0, write.offset), insertion, resized.subarray(write.offset)]);
    inserted.push(describeOffsetWrite(write.offset, insertion, `insert-hex@${hexOffset(write.offset)}`));
  }
  return {
    byteLength: resized.byteLength,
    byteLengthDelta: resized.byteLength - bytes.byteLength,
    sha256: sha256(resized),
    replacementCount: replacements.length,
    insertionCount: inserted.length,
    replacements,
    insertions: inserted,
    bytes: resized,
  };
}

export function patchPmmFixtureMotion(baseBytes, donorBaseBytes, donorVariantBytes, options = {}) {
  const donorDiff = diffPmmBytes(donorBaseBytes, donorVariantBytes, {
    context: options.context ?? 16,
    limit: options.limit ?? 32,
  });
  const donorBaseSuffixStart = donorBaseBytes.byteLength - donorDiff.commonSuffixLength;
  const donorVariantSuffixStart = donorVariantBytes.byteLength - donorDiff.commonSuffixLength;
  const donorBaseSuffix = donorBaseBytes.subarray(donorBaseSuffixStart);
  const baseSuffixStart = findCompatibleSuffixStart(baseBytes, donorBaseSuffix, donorDiff.commonPrefixLength);
  const donorMiddle = donorVariantBytes.subarray(donorDiff.commonPrefixLength, donorVariantSuffixStart);
  const patched = Buffer.concat([
    baseBytes.subarray(0, donorDiff.commonPrefixLength),
    donorMiddle,
    baseBytes.subarray(baseSuffixStart),
  ]);

  return {
    byteLength: patched.byteLength,
    byteLengthDelta: patched.byteLength - baseBytes.byteLength,
    commonPrefixLength: donorDiff.commonPrefixLength,
    commonPrefixHex: hexOffset(donorDiff.commonPrefixLength),
    baseSuffixStart,
    baseSuffixStartHex: hexOffset(baseSuffixStart),
    donorMiddleByteLength: donorMiddle.byteLength,
    donorMiddleSha256: sha256(donorMiddle),
    sha256: sha256(patched),
    donorVariantSha256: sha256(donorVariantBytes),
    matchesDonorVariant: Buffer.compare(patched, donorVariantBytes) === 0,
    bytes: patched,
  };
}

export function analyzePmmFixtureMotion(baseBytes, variantBytes, options = {}) {
  const recordByteLength = options.recordByteLength ?? 62;
  const markerOffsetInRecord = options.markerOffsetInRecord;
  const markerHex = options.markerHex;
  const limit = options.limit ?? 32;
  const diff = diffPmmBytes(baseBytes, variantBytes, {
    context: options.context ?? 16,
    limit,
  });
  const baseMiddle = sliceMiddle(baseBytes, diff.commonPrefixLength, diff.commonSuffixLength);
  const variantMiddle = sliceMiddle(variantBytes, diff.commonPrefixLength, diff.commonSuffixLength);
  const baseRecords = extractPmmMotionRecords(baseBytes, {
    markerHex,
    markerOffsetInRecord,
    recordByteLength,
    limit: options.recordLimit ?? 64,
  });
  const variantRecords = extractPmmMotionRecords(variantBytes, {
    markerHex,
    markerOffsetInRecord,
    recordByteLength,
    limit: options.recordLimit ?? 64,
  });

  return {
    base: {
      file: options.baseFile,
      byteLength: baseBytes.byteLength,
    },
    variant: {
      file: options.variantFile,
      byteLength: variantBytes.byteLength,
    },
    diff: {
      byteLengthDelta: diff.byteLengthDelta,
      commonPrefixLength: diff.commonPrefixLength,
      commonPrefixHex: hexOffset(diff.commonPrefixLength),
      commonSuffixLength: diff.commonSuffixLength,
      middleByteLengthDelta: variantMiddle.byteLength - baseMiddle.byteLength,
      changedRanges: diff.changedRanges,
    },
    middle: {
      base: describeMiddle(baseMiddle, options.hexLimit ?? 192, options.wordLimit ?? 96),
      variant: describeMiddle(variantMiddle, options.hexLimit ?? 192, options.wordLimit ?? 96),
    },
    valueMatches:
      options.values || options.frames
        ? createValueMatchReport(variantBytes, {
            values: options.values ?? [],
            frames: options.frames ?? [],
            limit,
          })
        : undefined,
    motionRecords: {
      recordByteLength,
      base: summarizeMotionRecords(baseRecords, baseMiddle),
      variant: summarizeMotionRecords(variantRecords, variantMiddle),
      variantRecordsInChangedMiddle: variantRecords.records
        .filter((record) => record.recordStart >= variantMiddle.start && record.recordStart < variantMiddle.end)
        .map(summarizeRecord),
    },
    vmd: options.vmd
      ? {
          file: options.vmdFile,
          modelName: options.vmd.modelName,
          counts: options.vmd.counts,
          maxFrame: options.vmd.maxFrame,
          searches: createVmdSearchReport(options.vmd, variantBytes, variantMiddle, limit),
        }
      : undefined,
  };
}

function sliceMiddle(bytes, commonPrefixLength, commonSuffixLength) {
  const start = commonPrefixLength;
  const end = bytes.byteLength - commonSuffixLength;
  return {
    start,
    startHex: hexOffset(start),
    end,
    endHex: hexOffset(end),
    byteLength: Math.max(0, end - start),
    bytes: bytes.subarray(start, end),
  };
}

function findCompatibleSuffixStart(baseBytes, donorBaseSuffix, fallbackStart) {
  const found = baseBytes.indexOf(donorBaseSuffix, fallbackStart);
  if (found < 0) {
    throw new Error("Could not find donor base suffix in target base PMM; refusing motion middle transplant.");
  }
  return found;
}

function describeMiddle(middle, hexLimit, wordLimit) {
  return {
    start: middle.start,
    startHex: middle.startHex,
    end: middle.end,
    endHex: middle.endHex,
    byteLength: middle.byteLength,
    headHex: middle.bytes.subarray(0, hexLimit).toString("hex"),
    truncated: middle.byteLength > hexLimit,
    words: readMiddleWords(middle, wordLimit),
    interestingFloats: readInterestingFloats(middle, wordLimit),
    interestingIntegers: readInterestingIntegers(middle, wordLimit),
  };
}

function readMiddleWords(middle, limit) {
  const words = [];
  for (let relativeOffset = 0; relativeOffset + 4 <= middle.bytes.byteLength && words.length < limit; relativeOffset += 4) {
    const absoluteOffset = middle.start + relativeOffset;
    words.push({
      relativeOffset,
      relativeOffsetHex: hexOffset(relativeOffset),
      offset: absoluteOffset,
      offsetHex: hexOffset(absoluteOffset),
      hex: middle.bytes.subarray(relativeOffset, relativeOffset + 4).toString("hex"),
      u32: middle.bytes.readUInt32LE(relativeOffset),
      f32: roundFloat(middle.bytes.readFloatLE(relativeOffset)),
      u16: [middle.bytes.readUInt16LE(relativeOffset), middle.bytes.readUInt16LE(relativeOffset + 2)],
    });
  }
  return words;
}

function readInterestingFloats(middle, limit) {
  const words = [];
  for (let relativeOffset = 0; relativeOffset + 4 <= middle.bytes.byteLength && words.length < limit; relativeOffset += 1) {
    const value = middle.bytes.readFloatLE(relativeOffset);
    if (!Number.isFinite(value) || Math.abs(value) < 0.00001 || Math.abs(value) > 100000) {
      continue;
    }
    const rounded = roundFloat(value);
    if (Object.is(rounded, -0) || rounded === 0) {
      continue;
    }
    words.push(describeScalar(middle, relativeOffset, {
      hex: middle.bytes.subarray(relativeOffset, relativeOffset + 4).toString("hex"),
      f32: rounded,
    }));
  }
  return words;
}

function readInterestingIntegers(middle, limit) {
  const words = [];
  for (let relativeOffset = 0; relativeOffset + 4 <= middle.bytes.byteLength && words.length < limit; relativeOffset += 1) {
    const u32 = middle.bytes.readUInt32LE(relativeOffset);
    if (u32 === 0 || u32 > 10000000) {
      continue;
    }
    const u16 = [middle.bytes.readUInt16LE(relativeOffset), middle.bytes.readUInt16LE(relativeOffset + 2)];
    if (u16[0] > 10000 || u16[1] > 10000) {
      continue;
    }
    words.push(describeScalar(middle, relativeOffset, {
      hex: middle.bytes.subarray(relativeOffset, relativeOffset + 4).toString("hex"),
      u32,
      u16,
    }));
  }
  return words;
}

function describeScalar(middle, relativeOffset, fields) {
  const absoluteOffset = middle.start + relativeOffset;
  return {
    relativeOffset,
    relativeOffsetHex: hexOffset(relativeOffset),
    offset: absoluteOffset,
    offsetHex: hexOffset(absoluteOffset),
    ...fields,
  };
}

function summarizeMotionRecords(report, middle) {
  return {
    recordTotal: report.recordTotal,
    markerHexes: report.markerHexes,
    summary: report.summary,
    recordsInChangedMiddle: report.records.filter((record) => record.recordStart >= middle.start && record.recordStart < middle.end).length,
    records: report.records.map(summarizeRecord),
  };
}

function summarizeRecord(record) {
  return {
    recordStart: record.recordStart,
    recordStartHex: record.recordStartHex,
    markerOffset: record.markerOffset,
    markerOffsetHex: record.markerOffsetHex,
    preMarkerHex: record.preMarkerHex,
    candidateFields: record.candidateFields,
  };
}

function createVmdSearchReport(vmd, bytes, middle, limit) {
  const searches = [];
  for (const frame of vmd.bones) {
    searches.push({
      type: "bone",
      name: frame.name,
      frame: frame.frame,
      framePatterns: searchFramePatterns(bytes, middle, frame.frame, limit),
      position: searchFloatVector(bytes, middle, frame.position, "position", limit),
      rotation: searchFloatVector(bytes, middle, frame.rotation, "rotation", limit),
    });
  }
  for (const frame of vmd.morphs) {
    searches.push({
      type: "morph",
      name: frame.name,
      frame: frame.frame,
      framePatterns: searchFramePatterns(bytes, middle, frame.frame, limit),
      weight: searchFloatValue(bytes, middle, frame.weight, "weight", limit),
    });
  }
  return searches;
}

function createValueMatchReport(bytes, options) {
  return {
    frames: options.frames.map((frame) => ({
      frame,
      u32: searchPatternInBytes(bytes, uint32le(frame), `u32:${frame}`, options.limit),
      shiftedU32: searchPatternInBytes(bytes, uint32le(frame * 65536), `u32:frame<<16:${frame}`, options.limit),
    })),
    floats: options.values.map((value) => ({
      value,
      exact: searchPatternInBytes(bytes, float32le(value), `f32:${value}`, options.limit),
      approximate: findApproxFloatOffsets(bytes, value, options.limit).map((match) => ({
        offset: match.offset,
        offsetHex: hexOffset(match.offset),
        value: roundFloat(match.value),
      })),
    })),
  };
}

function searchPatternInBytes(bytes, pattern, label, limit) {
  return {
    label,
    hex: pattern.toString("hex"),
    offsets: findOffsets(bytes, pattern, 0, bytes.byteLength, limit).map(offsetSummary),
  };
}

function replaceExact(bytes, needle, replacement, label) {
  const replacements = [];
  let offset = 0;
  while (true) {
    const found = bytes.indexOf(needle, offset);
    if (found < 0) {
      break;
    }
    replacement.copy(bytes, found);
    replacements.push({
      label,
      offset: found,
      offsetHex: hexOffset(found),
      fromHex: needle.toString("hex"),
      toHex: replacement.toString("hex"),
    });
    offset = found + replacement.byteLength;
  }
  return replacements;
}

function describeOffsetWrite(offset, replacement, label) {
  return {
    label,
    offset,
    offsetHex: hexOffset(offset),
    toHex: replacement.toString("hex"),
  };
}

function searchFramePatterns(bytes, middle, frame, limit) {
  return [
    searchPattern(bytes, middle, uint32le(frame), `u32:${frame}`, limit),
    searchPattern(bytes, middle, uint32le(frame * 65536), `u32:frame<<16:${frame}`, limit),
  ];
}

function searchFloatVector(bytes, middle, values, label, limit) {
  return values
    .map((value, index) => ({ value, index }))
    .filter(({ value }) => !Object.is(value, 0) && !Object.is(value, -0))
    .map(({ value, index }) => searchFloatValue(bytes, middle, value, `${label}[${index}]`, limit));
}

function searchFloatValue(bytes, middle, value, label, limit) {
  return {
    ...searchPattern(bytes, middle, float32le(value), `f32:${label}:${value}`, limit),
    approximateMiddleOffsets: findApproxFloatOffsets(middle.bytes, value, limit).map((match) => ({
      offset: middle.start + match.offset,
      offsetHex: hexOffset(middle.start + match.offset),
      relativeOffset: match.offset,
      relativeOffsetHex: hexOffset(match.offset),
      value: roundFloat(match.value),
    })),
  };
}

function searchPattern(bytes, middle, pattern, label, limit) {
  const allOffsets = findOffsets(bytes, pattern, 0, bytes.byteLength, limit);
  const middleOffsets = findOffsets(bytes, pattern, middle.start, middle.end, limit);
  return {
    label,
    hex: pattern.toString("hex"),
    allOffsets: allOffsets.map(offsetSummary),
    middleOffsets: middleOffsets.map((offset) => ({
      ...offsetSummary(offset),
      relativeOffset: offset - middle.start,
      relativeOffsetHex: hexOffset(offset - middle.start),
    })),
  };
}

function findOffsets(bytes, pattern, start, end, limit) {
  const offsets = [];
  let offset = start;
  while (offset < end && offsets.length < limit) {
    const found = bytes.indexOf(pattern, offset);
    if (found < 0 || found >= end) {
      break;
    }
    offsets.push(found);
    offset = found + 1;
  }
  return offsets;
}

function findApproxFloatOffsets(bytes, expected, limit, epsilon = 0.00001) {
  const offsets = [];
  for (let offset = 0; offset + 4 <= bytes.byteLength && offsets.length < limit; offset += 1) {
    const value = bytes.readFloatLE(offset);
    if (Number.isFinite(value) && Math.abs(value - expected) <= epsilon) {
      offsets.push({ offset, value });
    }
  }
  return offsets;
}

function offsetSummary(offset) {
  return {
    offset,
    offsetHex: hexOffset(offset),
  };
}

function roundFloat(value) {
  if (!Number.isFinite(value)) {
    return value;
  }
  return Math.round(value * 1_000_000) / 1_000_000;
}

function uint32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value >>> 0, 0);
  return bytes;
}

function float32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeFloatLE(value, 0);
  return bytes;
}

function createFollowupUnittestBoneKeyRecord(key, keyNumber) {
  const record = Buffer.from("0000020000000000000001000000000000002828407f2828407f2828407f14146b6b0000000000000000000000000000000000000000000000000000803f", "hex");
  record.writeUInt16LE(keyNumber, 2);
  record.writeUInt32LE(key.frame, 6);
  record.writeFloatLE(key.position[0], 34);
  record.writeFloatLE(key.position[1], 38);
  record.writeFloatLE(key.position[2], 42);
  return record;
}

function createFollowupUnittestBoneTransformKeyRecord(key, keyNumber) {
  const record = Buffer.alloc(62);
  record.writeUInt8(1, 0);
  record.writeUInt16LE(keyNumber, 2);
  record.writeUInt32LE(key.frame, 6);
  record.writeUInt32LE(1, 10);
  Buffer.from("14141414", "hex").copy(record, 18);
  record.writeFloatLE(key.position[0], 34);
  record.writeFloatLE(key.position[1], 38);
  record.writeFloatLE(key.position[2], 42);
  record.writeFloatLE(key.rotation[0], 46);
  record.writeFloatLE(key.rotation[1], 50);
  record.writeFloatLE(key.rotation[2], 54);
  record.writeFloatLE(key.rotation[3], 58);
  return record;
}

function uint16le(value) {
  const bytes = Buffer.alloc(2);
  bytes.writeUInt16LE(value, 0);
  return bytes;
}

function hasExistingUnittestBoneKey(bytes, key) {
  if (bytes.byteLength < 0x1fe) {
    return false;
  }
  return (
    bytes.readUInt32LE(0x1d6) === key.frame &&
    approximatelyEqual(bytes.readFloatLE(0x1f2), key.position[0]) &&
    approximatelyEqual(bytes.readFloatLE(0x1f6), key.position[1]) &&
    approximatelyEqual(bytes.readFloatLE(0x1fa), key.position[2])
  );
}

function hasExistingUnittestBoneTransformKey(bytes, key) {
  if (bytes.byteLength < 0x20e) {
    return false;
  }
  return (
    bytes.readUInt32LE(0x1d6) === key.frame &&
    approximatelyEqual(bytes.readFloatLE(0x1f2), key.position[0]) &&
    approximatelyEqual(bytes.readFloatLE(0x1f6), key.position[1]) &&
    approximatelyEqual(bytes.readFloatLE(0x1fa), key.position[2]) &&
    approximatelyEqual(bytes.readFloatLE(0x1fe), key.rotation[0]) &&
    approximatelyEqual(bytes.readFloatLE(0x202), key.rotation[1]) &&
    approximatelyEqual(bytes.readFloatLE(0x206), key.rotation[2]) &&
    approximatelyEqual(bytes.readFloatLE(0x20a), key.rotation[3])
  );
}

function approximatelyEqual(left, right, epsilon = 0.00001) {
  return Math.abs(left - right) <= epsilon;
}

function inferSingleBoneName(vmd) {
  const names = new Set(vmd.bones.map((frame) => frame.name));
  if (names.size !== 1) {
    throw new Error(`PMM unittest VMD writer needs --bone-name when VMD has ${names.size} bone names.`);
  }
  return [...names][0];
}

function isIdentityRotation(rotation, epsilon = 0.00001) {
  return (
    Math.abs((rotation?.[0] ?? 0) - 0) <= epsilon &&
    Math.abs((rotation?.[1] ?? 0) - 0) <= epsilon &&
    Math.abs((rotation?.[2] ?? 0) - 0) <= epsilon &&
    Math.abs((rotation?.[3] ?? 1) - 1) <= epsilon
  );
}

function hexOffset(value) {
  return `0x${value.toString(16)}`;
}

function requireString(options, key) {
  if (!options[key]) {
    throw new Error(`Missing ${key}.`);
  }
  return options[key];
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function withoutBytes(result) {
  const { bytes, ...metadata } = result;
  return metadata;
}
