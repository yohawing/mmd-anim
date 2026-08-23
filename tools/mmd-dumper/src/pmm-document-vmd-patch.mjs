import { readFile, writeFile } from "node:fs/promises";
import { comparePmmDocumentToVmd } from "./pmm-document-vmd-compare.mjs";
import { parsePmmDocumentKeyframes } from "./pmm-document-keyframes.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

const BONE_FRAME_OFFSET = 0x04;
const BONE_POSITION_OFFSET = 0x20;
const BONE_ROTATION_OFFSET = 0x2c;
const MORPH_FRAME_OFFSET = 0x04;
const MORPH_WEIGHT_OFFSET = 0x10;
const INITIAL_KEYFRAME_NEXT_OFFSET = 0x08;
// Initial (per-object) morph keyframes have no objectIndex prefix, so the weight float sits
// 4 bytes earlier than in an additional keyframe (which carries MORPH_WEIGHT_OFFSET = 0x10).
const INITIAL_MORPH_WEIGHT_OFFSET = 0x0c;
const BONE_RECORD_BYTES = 62;
const MORPH_RECORD_BYTES = 21;
const DEFAULT_INTERPOLATION = [20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107];

export async function writePmmDocumentVmdKeyframePatch(options) {
  const templateFile = requireString(options, "template");
  const targetVmdFile = requireString(options, "targetVmd");
  const outFile = requireString(options, "out");
  const [templateBytes, targetVmd] = await Promise.all([
    readFile(templateFile),
    readVmdInventory(targetVmdFile, { limit: Number.MAX_SAFE_INTEGER }),
  ]);
  const result = patchPmmDocumentVmdKeyframes(templateBytes, targetVmd, options);
  await writeFile(outFile, result.bytes);
  return {
    ...withoutBytes(result),
    templateFile,
    targetVmdFile,
    outFile,
  };
}

export function patchPmmDocumentVmdKeyframes(templateBytes, targetVmd, options = {}) {
  const targetSlot = options.targetSlot ?? 0;
  const template = parsePmmDocumentKeyframes(templateBytes);
  const model = template.models[targetSlot];
  if (!model) {
    throw new Error(`PMM model slot ${targetSlot} was not found.`);
  }
  validateSupportedTargetVmd(targetVmd);
  targetVmd = dedupeTargetVmdFrames(targetVmd);
  const patched = Buffer.from(templateBytes);
  const boneSection = buildBoneKeyframeSection(model, targetVmd.bones);
  const morphSection = buildMorphKeyframeSection(model, targetVmd.morphs);
  const appliedTargetVmd = pmmPatchTargetVmd(targetVmd, boneSection.appliedFrames, morphSection.appliedFrames);
  applyInitialNextLinks(patched, model.initialBoneKeyframes, boneSection.initialNextByObjectIndex);
  applyInitialNextLinks(patched, model.initialMorphKeyframes, morphSection.initialNextByObjectIndex);
  applyInitialMorphWeights(patched, model.initialMorphKeyframes, morphSection.initialWeightByObjectIndex);
  patched.writeInt32LE(appliedTargetVmd.maxFrame, model.lastFrameIndexOffset);

  let resized = replaceRange(
    patched,
    model.sections.morphKeyframeCountOffset,
    model.sections.morphKeyframesEndOffset,
    morphSection.bytes,
  );
  resized = replaceRange(
    resized,
    model.sections.boneKeyframeCountOffset,
    model.sections.boneKeyframesEndOffset,
    boneSection.bytes,
  );

  const patchedDocument = parsePmmDocumentKeyframes(resized);
  const comparison = comparePmmDocumentToVmd(patchedDocument, appliedTargetVmd, { targetSlot });
  if (options.requireVerified !== false && !comparison.ok) {
    throw new Error(`Patched PMM document/VMD comparison failed: ${JSON.stringify(comparison.counts)}`);
  }
  return {
    ok: true,
    mode: "pmm-document-vmd-keyframe-patch",
    warning:
      "This PMMv2 document patcher rebuilds model bone/morph keyframe sections from VMD. Camera, light, self-shadow, and property channels are still rejected.",
    targetSlot,
    byteLength: resized.byteLength,
    byteLengthDelta: resized.byteLength - templateBytes.byteLength,
    rewriteCount: boneSection.records.length + morphSection.records.length,
    resize: {
      boneKeyframesBefore: model.boneKeyframes.length,
      boneKeyframesAfter: boneSection.records.length,
      morphKeyframesBefore: model.morphKeyframes.length,
      morphKeyframesAfter: morphSection.records.length,
    },
    filter: {
      originalCounts: {
        boneFrames: targetVmd.counts.boneFrames,
        morphFrames: targetVmd.counts.morphFrames,
      },
      appliedCounts: {
        boneFrames: appliedTargetVmd.counts.boneFrames,
        morphFrames: appliedTargetVmd.counts.morphFrames,
      },
      skippedCounts: {
        boneFrames: targetVmd.counts.boneFrames - appliedTargetVmd.counts.boneFrames,
        morphFrames: targetVmd.counts.morphFrames - appliedTargetVmd.counts.morphFrames,
      },
      skippedBoneNames: boneSection.skippedNames,
      skippedMorphNames: morphSection.skippedNames,
    },
    rewrites: [...boneSection.records, ...morphSection.records],
    comparison,
    bytes: resized,
  };
}

function buildBoneKeyframeSection(model, vmdFrames) {
  const grouped = groupFramesByObjectIndex({
    frames: vmdFrames,
    names: model.boneNames,
    validateFrame: validateBoneFrame,
  });
  const records = [];
  const initialNextByObjectIndex = new Map(model.initialBoneKeyframes.map((keyframe) => [keyframe.objectIndex, 0]));
  const existingInterpolation = new Map(model.boneKeyframes.map((keyframe) => [keyframe.name, keyframe.interpolation.flat()]));
  for (const [objectIndex, frames] of grouped.framesByObjectIndex.entries()) {
    const firstDocumentIndex = model.boneCount + records.length;
    initialNextByObjectIndex.set(objectIndex, firstDocumentIndex);
    for (let index = 0; index < frames.length; index += 1) {
      const frame = frames[index];
      const documentIndex = model.boneCount + records.length;
      const previousKeyframeIndex = index === 0 ? objectIndex : documentIndex - 1;
      const nextKeyframeIndex = index === frames.length - 1 ? 0 : documentIndex + 1;
      const interpolation =
        boneInterpolationFromVmdFrame(frame) ?? existingInterpolation.get(frame.name) ?? DEFAULT_INTERPOLATION;
      records.push({
        kind: "bone",
        name: frame.name,
        frame: frame.frame,
        documentIndex,
        previousKeyframeIndex,
        nextKeyframeIndex,
        bytes: writeBoneRecord({
          documentIndex,
          previousKeyframeIndex,
          nextKeyframeIndex,
          interpolation,
          frame,
        }),
      });
    }
  }
  const section = makeSection(model.sections.boneKeyframeCountOffset, BONE_RECORD_BYTES, records, initialNextByObjectIndex);
  section.appliedFrames = grouped.appliedFrames;
  section.skippedNames = grouped.skippedNames;
  return section;
}

function buildMorphKeyframeSection(model, vmdFrames) {
  const grouped = groupFramesByObjectIndex({
    frames: vmdFrames,
    names: model.morphNames,
    validateFrame: validateMorphFrame,
  });
  const records = [];
  const initialNextByObjectIndex = new Map(model.initialMorphKeyframes.map((keyframe) => [keyframe.objectIndex, 0]));
  // MMD evaluates the value at frame 0 from each morph's INITIAL keyframe (the per-morph
  // head of the linked list), not from an additional keyframe that merely sits at frame 0.
  // Fold a frame-0 VMD keyframe into the initial keyframe in place (see applyInitialMorphWeights);
  // only frames > 0 become additional keyframes. Appending the weight as an extra frame-0
  // keyframe left the initial at weight 0 and the morph rendered inert.
  const initialWeightByObjectIndex = new Map();
  for (const [objectIndex, frames] of grouped.framesByObjectIndex.entries()) {
    const additionalFrames = [];
    for (const frame of frames) {
      if (frame.frame === 0) {
        initialWeightByObjectIndex.set(objectIndex, frame.weight);
      } else {
        additionalFrames.push(frame);
      }
    }
    if (additionalFrames.length === 0) {
      continue;
    }
    const firstDocumentIndex = model.morphCount + records.length;
    initialNextByObjectIndex.set(objectIndex, firstDocumentIndex);
    for (let index = 0; index < additionalFrames.length; index += 1) {
      const frame = additionalFrames[index];
      const documentIndex = model.morphCount + records.length;
      const previousKeyframeIndex = index === 0 ? objectIndex : documentIndex - 1;
      const nextKeyframeIndex = index === additionalFrames.length - 1 ? 0 : documentIndex + 1;
      records.push({
        kind: "morph",
        name: frame.name,
        frame: frame.frame,
        documentIndex,
        previousKeyframeIndex,
        nextKeyframeIndex,
        bytes: writeMorphRecord({ documentIndex, previousKeyframeIndex, nextKeyframeIndex, frame }),
      });
    }
  }
  const section = makeSection(model.sections.morphKeyframeCountOffset, MORPH_RECORD_BYTES, records, initialNextByObjectIndex);
  section.initialWeightByObjectIndex = initialWeightByObjectIndex;
  section.appliedFrames = grouped.appliedFrames;
  section.skippedNames = grouped.skippedNames;
  return section;
}

function groupFramesByObjectIndex({ frames, names, validateFrame }) {
  const nameToIndex = new Map(names.map((name, index) => [name, index]));
  const seen = new Map();
  const skippedFrames = [];
  const grouped = new Map();
  for (const frame of frames) {
    const objectIndex = nameToIndex.get(frame.name);
    if (objectIndex === undefined) {
      skippedFrames.push(frame);
      continue;
    }
    const key = `${frame.name}\u0000${frame.frame}`;
    if (seen.has(key)) {
      seen.set(key, { frame, objectIndex });
      continue;
    }
    seen.set(key, { frame, objectIndex });
  }
  for (const { frame, objectIndex } of seen.values()) {
    validateFrame(frame);
    if (!grouped.has(objectIndex)) {
      grouped.set(objectIndex, []);
    }
    grouped.get(objectIndex).push(frame);
  }
  const framesByObjectIndex = new Map(
    [...grouped.entries()]
      .sort(([leftIndex], [rightIndex]) => leftIndex - rightIndex)
      .map(([objectIndex, objectFrames]) => [
        objectIndex,
        objectFrames.slice().sort((left, right) => left.frame - right.frame),
      ]),
  );
  return {
    framesByObjectIndex,
    appliedFrames: [...seen.values()].map(({ frame }) => frame),
    skippedNames: countFramesByName(skippedFrames),
  };
}

function pmmPatchTargetVmd(targetVmd, bones, morphs) {
  const maxFrame = maxFrameNumber(bones, morphs);
  return {
    ...targetVmd,
    counts: {
      ...targetVmd.counts,
      boneFrames: bones.length,
      morphFrames: morphs.length,
    },
    maxFrame,
    bones,
    morphs,
    boneNameCounts: countFramesByName(bones),
    morphNameCounts: countFramesByName(morphs),
  };
}

function maxFrameNumber(...frameSets) {
  let max = 0;
  for (const frames of frameSets) {
    for (const frame of frames) {
      if (Number.isFinite(frame.frame) && frame.frame > max) {
        max = frame.frame;
      }
    }
  }
  return max;
}

function countFramesByName(frames) {
  const counts = new Map();
  for (const frame of frames) {
    counts.set(frame.name, (counts.get(frame.name) ?? 0) + 1);
  }
  return [...counts.entries()]
    .sort(([leftName, leftCount], [rightName, rightCount]) => rightCount - leftCount || leftName.localeCompare(rightName))
    .map(([name, count]) => ({ name, count }));
}

function makeSection(countOffset, recordByteLength, records, initialNextByObjectIndex) {
  const bytes = Buffer.alloc(4 + records.length * recordByteLength);
  bytes.writeInt32LE(records.length, 0);
  for (let index = 0; index < records.length; index += 1) {
    records[index].bytes.copy(bytes, 4 + index * recordByteLength);
    records[index] = {
      ...withoutRecordBytes(records[index]),
      recordOffset: countOffset + 4 + index * recordByteLength,
      recordOffsetHex: hex(countOffset + 4 + index * recordByteLength),
    };
  }
  return { bytes, records, initialNextByObjectIndex };
}

function writeBoneRecord({ documentIndex, previousKeyframeIndex, nextKeyframeIndex, interpolation, frame }) {
  const bytes = Buffer.alloc(BONE_RECORD_BYTES);
  bytes.writeInt32LE(documentIndex, 0x00);
  bytes.writeInt32LE(frame.frame, BONE_FRAME_OFFSET);
  bytes.writeInt32LE(previousKeyframeIndex, 0x08);
  bytes.writeInt32LE(nextKeyframeIndex, 0x0c);
  Buffer.from(interpolation).copy(bytes, 0x10, 0, 16);
  bytes.writeFloatLE(frame.position[0], BONE_POSITION_OFFSET);
  bytes.writeFloatLE(frame.position[1], BONE_POSITION_OFFSET + 4);
  bytes.writeFloatLE(frame.position[2], BONE_POSITION_OFFSET + 8);
  bytes.writeFloatLE(frame.rotation[0], BONE_ROTATION_OFFSET);
  bytes.writeFloatLE(frame.rotation[1], BONE_ROTATION_OFFSET + 4);
  bytes.writeFloatLE(frame.rotation[2], BONE_ROTATION_OFFSET + 8);
  bytes.writeFloatLE(frame.rotation[3], BONE_ROTATION_OFFSET + 12);
  bytes[0x3c] = 0;
  bytes[0x3d] = 0;
  return bytes;
}

function boneInterpolationFromVmdFrame(frame) {
  if (!frame.interpolationHex) {
    return undefined;
  }
  const interpolation = Buffer.from(frame.interpolationHex, "hex");
  if (interpolation.byteLength !== 64) {
    return undefined;
  }
  return [
    interpolation[0],
    interpolation[4],
    interpolation[8],
    interpolation[12],
    interpolation[1],
    interpolation[5],
    interpolation[9],
    interpolation[13],
    interpolation[2],
    interpolation[6],
    interpolation[10],
    interpolation[14],
    interpolation[3],
    interpolation[7],
    interpolation[11],
    interpolation[15],
  ];
}

function writeMorphRecord({ documentIndex, previousKeyframeIndex, nextKeyframeIndex, frame }) {
  const bytes = Buffer.alloc(MORPH_RECORD_BYTES);
  bytes.writeInt32LE(documentIndex, 0x00);
  bytes.writeInt32LE(frame.frame, MORPH_FRAME_OFFSET);
  bytes.writeInt32LE(previousKeyframeIndex, 0x08);
  bytes.writeInt32LE(nextKeyframeIndex, 0x0c);
  bytes.writeFloatLE(frame.weight, MORPH_WEIGHT_OFFSET);
  bytes[0x14] = 0;
  return bytes;
}

function applyInitialNextLinks(bytes, initialKeyframes, nextByObjectIndex) {
  for (const keyframe of initialKeyframes) {
    bytes.writeInt32LE(nextByObjectIndex.get(keyframe.objectIndex) ?? 0, keyframe.offset + INITIAL_KEYFRAME_NEXT_OFFSET);
  }
}

function applyInitialMorphWeights(bytes, initialKeyframes, weightByObjectIndex) {
  if (!(weightByObjectIndex instanceof Map)) {
    return;
  }
  for (const keyframe of initialKeyframes) {
    if (weightByObjectIndex.has(keyframe.objectIndex)) {
      bytes.writeFloatLE(weightByObjectIndex.get(keyframe.objectIndex), keyframe.offset + INITIAL_MORPH_WEIGHT_OFFSET);
    }
  }
}

function validateBoneFrame(frame) {
  validateFiniteInteger(frame.frame, "bone frame");
  validateFiniteArray(frame.position, 3, `bone position ${frame.name} frame ${frame.frame}`);
  validateFiniteArray(frame.rotation, 4, `bone rotation ${frame.name} frame ${frame.frame}`);
}

function validateMorphFrame(frame) {
  validateFiniteInteger(frame.frame, "morph frame");
  validateFiniteNumber(frame.weight, `morph weight ${frame.name} frame ${frame.frame}`);
}

function validateFiniteInteger(value, label) {
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`Invalid ${label}: ${value}`);
  }
}

function validateFiniteArray(values, length, label) {
  if (!Array.isArray(values) || values.length !== length) {
    throw new Error(`Invalid ${label}: expected ${length} values.`);
  }
  for (const value of values) {
    validateFiniteNumber(value, label);
  }
}

function validateFiniteNumber(value, label) {
  if (!Number.isFinite(value)) {
    throw new Error(`Invalid ${label}: ${value}`);
  }
}

function replaceRange(bytes, start, end, replacement) {
  return Buffer.concat([bytes.subarray(0, start), replacement, bytes.subarray(end)]);
}

function withoutRecordBytes(record) {
  const { bytes, ...metadata } = record;
  return metadata;
}

function dedupeTargetVmdFrames(targetVmd) {
  const bones = dedupeFramesByNameAndFrame(targetVmd.bones ?? []);
  const morphs = dedupeFramesByNameAndFrame(targetVmd.morphs ?? []);
  return {
    ...targetVmd,
    bones,
    morphs,
    counts: {
      ...targetVmd.counts,
      boneFrames: bones.length,
      morphFrames: morphs.length,
    },
  };
}

function dedupeFramesByNameAndFrame(frames) {
  const keyed = new Map();
  for (const frame of frames) {
    keyed.set(`${frame.name}\u0000${frame.frame}`, frame);
  }
  return [...keyed.values()];
}

function validateSupportedTargetVmd(vmd) {
  const unsupported = [
    ["cameraFrames", vmd.counts.cameraFrames],
    ["lightFrames", vmd.counts.lightFrames],
    ["selfShadowFrames", vmd.counts.selfShadowFrames],
    ["propertyFrames", vmd.counts.propertyFrames],
  ].filter(([, count]) => count > 0);
  if (unsupported.length > 0) {
    throw new Error(`Unsupported VMD channels for PMM document patching: ${unsupported.map(([name, count]) => `${name}=${count}`).join(", ")}`);
  }
}

function requireString(options, key) {
  const value = options[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${key} is required.`);
  }
  return value;
}

function withoutBytes(result) {
  const { bytes, ...metadata } = result;
  return metadata;
}

function hex(value) {
  return `0x${value.toString(16)}`;
}
