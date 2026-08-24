import { readFile } from "node:fs/promises";
import iconv from "iconv-lite";

const PMM_SIGNATURE_PREFIX = "Polygon Movie maker ";
const PMM_V1_SIGNATURE = "Polygon Movie maker 0001";
const PMM_V2_SIGNATURE = "Polygon Movie maker 0002";
const PMM_HEADER_BYTE_LENGTH = 30;
const PMM_PATH_BYTE_LENGTH = 256;

export async function readPmmDocumentKeyframes(file, options = {}) {
  return parsePmmDocumentKeyframes(await readFile(file), options);
}

export function parsePmmDocumentKeyframes(bytes, options = {}) {
  const cursor = new PmmCursor(bytes);
  const signature = cursor.peekAscii(0, PMM_HEADER_BYTE_LENGTH).replace(/\0+$/u, "");
  const version = pmmVersionFromSignature(signature);
  cursor.skip(PMM_HEADER_BYTE_LENGTH);

  const document = {
    signature: PMM_SIGNATURE_PREFIX.trimEnd(),
    version: signature.slice(PMM_SIGNATURE_PREFIX.length),
    formatVersion: version,
    byteLength: cursor.length,
    outputWidth: cursor.readInt32(),
    outputHeight: cursor.readInt32(),
    timelineWidth: cursor.readInt32(),
    cameraFov: cursor.readFloat32(),
    panelStateOffset: cursor.offset,
    expandFlags: readDocumentExpandFlags(cursor, version),
  };

  const selectedModelIndex = cursor.readByte();
  const modelCount = cursor.readByte();
  const models = [];
  for (let modelIndex = 0; modelIndex < modelCount; modelIndex += 1) {
    models.push(readPmmV2Model(cursor, { version, modelIndex, keyframeLimit: options.keyframeLimit }));
  }
  const camera = readPmmV2Camera(cursor, { keyframeLimit: options.keyframeLimit });
  const parameters = readPmmV2Parameters(cursor, { keyframeLimit: options.keyframeLimit });

  return {
    ok: true,
    source: "nanoem/ext/document.c PMMv2 layout",
    document,
    counts: summarizeCounts(models, camera),
    selectedModelIndex,
    models,
    camera,
    parameters,
  };
}

function pmmVersionFromSignature(signature) {
  if (signature.startsWith(PMM_V2_SIGNATURE)) {
    return 2;
  }
  if (signature.startsWith(PMM_V1_SIGNATURE)) {
    throw new Error("PMM_V1_NOT_SUPPORTED_BY_DOCUMENT_KEYFRAME_READER");
  }
  throw new Error("PMM_HEADER_NOT_FOUND");
}

function readDocumentExpandFlags(cursor, version) {
  const flags = {
    editingCla: cursor.readBool(),
    cameraPanel: cursor.readBool(),
    lightPanel: cursor.readBool(),
    accessoryPanel: cursor.readBool(),
    bonePanel: cursor.readBool(),
    morphPanel: cursor.readBool(),
  };
  if (version > 1) {
    flags.selfShadowPanel = cursor.readBool();
  }
  return flags;
}

function readPmmV2Model(cursor, { version, modelIndex, keyframeLimit }) {
  if (version !== 2) {
    throw new Error(`UNSUPPORTED_PMM_MODEL_VERSION:${version}`);
  }
  const offset = cursor.offset;
  const documentModelIndex = cursor.readByte();
  const nameJa = cursor.readVariableString();
  const nameEn = cursor.readVariableString();
  const pathOffset = cursor.offset;
  const path = cursor.readFixedString(PMM_PATH_BYTE_LENGTH);
  const numFixedTracks = cursor.readByte();
  const boneCount = cursor.readInt32();
  const boneNames = readVariableStrings(cursor, boneCount);
  const morphCount = cursor.readInt32();
  const morphNames = readVariableStrings(cursor, morphCount);
  const constraintBoneCount = cursor.readInt32();
  const constraintBoneIndices = readInt32Array(cursor, constraintBoneCount);
  const outsideParentSubjectBoneCount = cursor.readInt32();
  const outsideParentSubjectBoneIndices = readInt32Array(cursor, outsideParentSubjectBoneCount);
  const drawOrderIndex = cursor.readByte();
  const visible = cursor.readBool();
  const selectedBoneIndex = cursor.readInt32();
  const selectedMorphIndices = [cursor.readInt32(), cursor.readInt32(), cursor.readInt32(), cursor.readInt32()];
  const expansionStateCount = cursor.readByte();
  cursor.skip(expansionStateCount);
  const verticalScroll = cursor.readInt32();
  const lastFrameIndexOffset = cursor.offset;
  const lastFrameIndex = cursor.readInt32();

  const initialBoneKeyframesOffset = cursor.offset;
  const initialBoneKeyframes = readRepeated(cursor, boneCount, (index) =>
    readBoneKeyframe(cursor, { version, includeIndex: false, names: boneNames, initialIndex: index }),
  );
  const boneKeyframeCountOffset = cursor.offset;
  const boneKeyframeCount = cursor.readInt32();
  const boneKeyframesOffset = cursor.offset;
  const boneKeyframes = readRepeated(cursor, boneKeyframeCount, () =>
    readBoneKeyframe(cursor, { version, includeIndex: true, names: boneNames }),
  );
  const boneKeyframesEndOffset = cursor.offset;
  resolveNamedKeyframeObjects(initialBoneKeyframes, boneKeyframes, boneNames);
  const initialMorphKeyframesOffset = cursor.offset;
  const initialMorphKeyframes = readRepeated(cursor, morphCount, (index) =>
    readMorphKeyframe(cursor, { includeIndex: false, names: morphNames, initialIndex: index }),
  );
  const morphKeyframeCountOffset = cursor.offset;
  const morphKeyframeCount = cursor.readInt32();
  const morphKeyframesOffset = cursor.offset;
  const morphKeyframes = readRepeated(cursor, morphKeyframeCount, () =>
    readMorphKeyframe(cursor, { includeIndex: true, names: morphNames }),
  );
  const morphKeyframesEndOffset = cursor.offset;
  resolveNamedKeyframeObjects(initialMorphKeyframes, morphKeyframes, morphNames);
  const initialModelKeyframe = readModelKeyframe(cursor, {
    includeIndex: false,
    constraintBoneCount,
    outsideParentSubjectBoneCount,
  });
  const modelKeyframeCount = cursor.readInt32();
  const modelKeyframes = readRepeated(cursor, modelKeyframeCount, () =>
    readModelKeyframe(cursor, { includeIndex: true, constraintBoneCount, outsideParentSubjectBoneCount }),
  );

  const boneStatesOffset = cursor.offset;
  cursor.skip(boneCount * 31);
  const morphStatesOffset = cursor.offset;
  cursor.skip(morphCount * 4);
  const constraintStatesOffset = cursor.offset;
  cursor.skip(constraintBoneCount);
  const outsideParentStatesOffset = cursor.offset;
  cursor.skip(outsideParentSubjectBoneCount * 16);

  const blendEnabled = cursor.readBool();
  const edgeWidth = cursor.readFloat32();
  const selfShadowEnabled = cursor.readBool();
  const transformOrderIndex = cursor.readByte();
  const endOffset = cursor.offset;

  return {
    slot: modelIndex,
    documentModelIndex,
    offset,
    offsetHex: hex(offset),
    byteLength: endOffset - offset,
    nameJa,
    nameEn,
    path,
    pathOffset,
    pathOffsetHex: hex(pathOffset),
    numFixedTracks,
    boneCount,
    morphCount,
    constraintBoneCount,
    outsideParentSubjectBoneCount,
    constraintBoneIndices,
    outsideParentSubjectBoneIndices,
    drawOrderIndex,
    transformOrderIndex,
    visible,
    blendEnabled,
    edgeWidth,
    selfShadowEnabled,
    selectedBoneIndex,
    selectedBoneName: boneNames[selectedBoneIndex] ?? null,
    selectedMorphIndices,
    verticalScroll,
    lastFrameIndexOffset,
    lastFrameIndexOffsetHex: hex(lastFrameIndexOffset),
    lastFrameIndex,
    sections: {
      initialBoneKeyframesOffset,
      initialBoneKeyframesOffsetHex: hex(initialBoneKeyframesOffset),
      boneKeyframeCountOffset,
      boneKeyframeCountOffsetHex: hex(boneKeyframeCountOffset),
      boneKeyframesOffset,
      boneKeyframesOffsetHex: hex(boneKeyframesOffset),
      boneKeyframesEndOffset,
      boneKeyframesEndOffsetHex: hex(boneKeyframesEndOffset),
      initialMorphKeyframesOffset,
      initialMorphKeyframesOffsetHex: hex(initialMorphKeyframesOffset),
      morphKeyframeCountOffset,
      morphKeyframeCountOffsetHex: hex(morphKeyframeCountOffset),
      morphKeyframesOffset,
      morphKeyframesOffsetHex: hex(morphKeyframesOffset),
      morphKeyframesEndOffset,
      morphKeyframesEndOffsetHex: hex(morphKeyframesEndOffset),
      boneStatesOffset,
      boneStatesOffsetHex: hex(boneStatesOffset),
      morphStatesOffset,
      morphStatesOffsetHex: hex(morphStatesOffset),
      constraintStatesOffset,
      constraintStatesOffsetHex: hex(constraintStatesOffset),
      outsideParentStatesOffset,
      outsideParentStatesOffsetHex: hex(outsideParentStatesOffset),
    },
    boneNames: limited(boneNames, keyframeLimit),
    morphNames: limited(morphNames, keyframeLimit),
    counts: {
      initialBoneKeyframes: initialBoneKeyframes.length,
      boneKeyframes: boneKeyframes.length,
      initialMorphKeyframes: initialMorphKeyframes.length,
      morphKeyframes: morphKeyframes.length,
      initialModelKeyframe: initialModelKeyframe ? 1 : 0,
      modelKeyframes: modelKeyframes.length,
    },
    initialBoneKeyframes: limited(initialBoneKeyframes, keyframeLimit),
    boneKeyframes: limited(boneKeyframes, keyframeLimit),
    initialMorphKeyframes: limited(initialMorphKeyframes, keyframeLimit),
    morphKeyframes: limited(morphKeyframes, keyframeLimit),
    initialModelKeyframe,
    modelKeyframes: limited(modelKeyframes, keyframeLimit),
  };
}

function readBoneKeyframe(cursor, { version, includeIndex, names, initialIndex }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const objectIndex = includeIndex ? base.objectIndex : initialIndex;
  const interpolation = [
    cursor.readBytes(4),
    cursor.readBytes(4),
    cursor.readBytes(4),
    cursor.readBytes(4),
  ];
  const translation = cursor.readFloat32Array(3);
  const orientation = cursor.readFloat32Array(4);
  const selected = cursor.readBool();
  const physicsSimulationDisabled = version > 1 ? cursor.readBool() : false;
  const endOffset = cursor.offset;
  return {
    kind: "bone",
    offset,
    offsetHex: hex(offset),
    byteLength: endOffset - offset,
    documentObjectIndex: includeIndex ? base.objectIndex : null,
    objectIndex,
    name: names[objectIndex] ?? null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    interpolation: interpolation.map((bytes) => [...bytes]),
    translation,
    orientation,
    selected,
    physicsSimulationDisabled,
  };
}

function readMorphKeyframe(cursor, { includeIndex, names, initialIndex }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const objectIndex = includeIndex ? base.objectIndex : initialIndex;
  const weight = cursor.readFloat32();
  const selected = cursor.readBool();
  const endOffset = cursor.offset;
  return {
    kind: "morph",
    offset,
    offsetHex: hex(offset),
    byteLength: endOffset - offset,
    documentObjectIndex: includeIndex ? base.objectIndex : null,
    objectIndex,
    name: names[objectIndex] ?? null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    weight,
    selected,
  };
}

function readModelKeyframe(cursor, { includeIndex, constraintBoneCount, outsideParentSubjectBoneCount }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const visible = cursor.readBool();
  const constraintStates = readRepeated(cursor, constraintBoneCount, () => cursor.readBool());
  const outsideParents = readRepeated(cursor, outsideParentSubjectBoneCount, () => ({
    modelIndex: cursor.readInt32(),
    boneIndex: cursor.readInt32(),
  }));
  const selected = cursor.readBool();
  const endOffset = cursor.offset;
  return {
    kind: "model",
    offset,
    offsetHex: hex(offset),
    byteLength: endOffset - offset,
    objectIndex: includeIndex ? base.objectIndex : null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    visible,
    constraintStates,
    outsideParents,
    selected,
  };
}

function readPmmV2Camera(cursor, { keyframeLimit }) {
  const offset = cursor.offset;
  const initialKeyframe = readCameraKeyframe(cursor, { includeIndex: false });
  const keyframeCountOffset = cursor.offset;
  const cameraKeyframeCount = cursor.readInt32();
  const keyframesOffset = cursor.offset;
  const keyframes = readRepeated(cursor, cameraKeyframeCount, () => readCameraKeyframe(cursor, { includeIndex: true }));
  const keyframesEndOffset = cursor.offset;
  const current = {
    position: cursor.readFloat32Array(3),
    target: cursor.readFloat32Array(3),
    rotation: cursor.readFloat32Array(3),
    perspective: !cursor.readBool(),
  };
  const endOffset = cursor.offset;
  return {
    offset,
    offsetHex: hex(offset),
    byteLength: endOffset - offset,
    sections: {
      keyframeCountOffset,
      keyframeCountOffsetHex: hex(keyframeCountOffset),
      keyframesOffset,
      keyframesOffsetHex: hex(keyframesOffset),
      keyframesEndOffset,
      keyframesEndOffsetHex: hex(keyframesEndOffset),
    },
    counts: {
      initialCameraKeyframe: 1,
      cameraKeyframes: keyframes.length,
    },
    initialKeyframe,
    keyframes: limited(keyframes, keyframeLimit),
    current,
  };
}

function readPmmV2Parameters(cursor, { keyframeLimit }) {
  const light = readLightSection(cursor, { keyframeLimit });
  const accessories = readAccessorySection(cursor, { keyframeLimit });
  const timeline = {
    currentFrame: cursor.readInt32(),
    horizontalScroll: cursor.readInt32(),
    horizontalScrollThumb: cursor.readInt32(),
    editingMode: cursor.readInt32(),
    cameraLookMode: cursor.readBool(),
    loop: cursor.readBool(),
    beginFrameEnabled: cursor.readBool(),
    endFrameEnabled: cursor.readBool(),
    beginFrame: cursor.readInt32(),
    endFrame: cursor.readInt32(),
  };
  const audio = {
    enabled: cursor.readBool(),
    path: cursor.readFixedString(PMM_PATH_BYTE_LENGTH),
  };
  const backgroundVideo = {
    offset: [cursor.readInt32(), cursor.readInt32()],
    scale: cursor.readFloat32(),
    path: cursor.readFixedString(PMM_PATH_BYTE_LENGTH),
    disabled: cursor.readInt32() !== 0,
  };
  const backgroundImage = {
    offset: [cursor.readInt32(), cursor.readInt32()],
    scale: cursor.readFloat32(),
    path: cursor.readFixedString(PMM_PATH_BYTE_LENGTH),
    disabled: cursor.readBool(),
  };
  const display = {
    informationShown: cursor.readBool(),
    gridAndAxisShown: cursor.readBool(),
    groundShadowShown: cursor.readBool(),
    frameRate: cursor.readFloat32(),
    screenCaptureMode: cursor.readInt32(),
    accessoryRenderAfterModelsIndex: cursor.readInt32(),
    groundShadowBrightness: cursor.readFloat32(),
    translucentGroundShadowDisabled: cursor.readBool(),
    physicsSimulationMode: cursor.readByte(),
  };
  const gravity = readGravitySection(cursor, { keyframeLimit });
  const selfShadow = readSelfShadowSection(cursor, { keyframeLimit });
  return {
    light,
    accessories,
    timeline,
    audio,
    backgroundVideo,
    backgroundImage,
    display,
    gravity,
    selfShadow,
  };
}

function readLightSection(cursor, { keyframeLimit }) {
  const initialKeyframe = readLightKeyframe(cursor, { includeIndex: false });
  const keyframeCount = cursor.readInt32();
  const keyframes = readRepeated(cursor, keyframeCount, () => readLightKeyframe(cursor, { includeIndex: false }));
  const current = {
    color: cursor.readFloat32Array(3),
    direction: cursor.readFloat32Array(3),
  };
  return {
    counts: { initialLightKeyframe: 1, lightKeyframes: keyframes.length },
    initialKeyframe,
    keyframes: limited(keyframes, keyframeLimit),
    current,
  };
}

function readLightKeyframe(cursor, { includeIndex }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const color = cursor.readFloat32Array(3);
  const direction = cursor.readFloat32Array(3);
  const selected = cursor.readBool();
  return {
    kind: "light",
    offset,
    offsetHex: hex(offset),
    byteLength: cursor.offset - offset,
    objectIndex: includeIndex ? base.objectIndex : null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    color,
    direction,
    selected,
  };
}

function readAccessorySection(cursor, { keyframeLimit }) {
  const selectedAccessoryIndex = cursor.readByte();
  const verticalScroll = cursor.readInt32();
  const count = cursor.readByte();
  const accessories = readRepeated(cursor, count, () => readAccessory(cursor, { keyframeLimit }));
  return { selectedAccessoryIndex, verticalScroll, count, accessories };
}

function readAccessory(cursor, { keyframeLimit }) {
  const index = cursor.readByte();
  const name = cursor.readFixedString(100);
  const path = cursor.readFixedString(PMM_PATH_BYTE_LENGTH);
  const initialKeyframe = readAccessoryKeyframe(cursor, { includeIndex: false });
  const keyframeCount = cursor.readInt32();
  const keyframes = readRepeated(cursor, keyframeCount, () => readAccessoryKeyframe(cursor, { includeIndex: false }));
  return {
    index,
    name,
    path,
    counts: { initialAccessoryKeyframe: 1, accessoryKeyframes: keyframes.length },
    initialKeyframe,
    keyframes: limited(keyframes, keyframeLimit),
  };
}

function readAccessoryKeyframe(cursor, { includeIndex }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const visible = cursor.readBool();
  const position = cursor.readFloat32Array(3);
  const rotation = cursor.readFloat32Array(3);
  const scale = cursor.readFloat32();
  const opacity = cursor.readFloat32();
  const parentModelIndex = cursor.readInt32();
  const parentBoneIndex = cursor.readInt32();
  const shadowEnabled = cursor.readBool();
  const additiveBlending = cursor.readBool();
  const selected = cursor.readBool();
  return {
    kind: "accessory",
    offset,
    offsetHex: hex(offset),
    byteLength: cursor.offset - offset,
    objectIndex: includeIndex ? base.objectIndex : null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    visible,
    position,
    rotation,
    scale,
    opacity,
    parentModelIndex,
    parentBoneIndex,
    shadowEnabled,
    additiveBlending,
    selected,
  };
}

function readGravitySection(cursor, { keyframeLimit }) {
  const current = {
    acceleration: cursor.readFloat32(),
    noiseFrequency: cursor.readInt32(),
    direction: cursor.readFloat32Array(3),
    noiseEnabled: cursor.readBool(),
  };
  const initialKeyframe = readGravityKeyframe(cursor, { includeIndex: false });
  const keyframeCount = cursor.readInt32();
  const keyframes = readRepeated(cursor, keyframeCount, () => readGravityKeyframe(cursor, { includeIndex: false }));
  return {
    current,
    counts: { initialGravityKeyframe: 1, gravityKeyframes: keyframes.length },
    initialKeyframe,
    keyframes: limited(keyframes, keyframeLimit),
  };
}

function readGravityKeyframe(cursor, { includeIndex }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const noiseEnabled = cursor.readBool();
  const noiseFrequency = cursor.readInt32();
  const acceleration = cursor.readFloat32();
  const direction = cursor.readFloat32Array(3);
  const selected = cursor.readBool();
  return {
    kind: "gravity",
    offset,
    offsetHex: hex(offset),
    byteLength: cursor.offset - offset,
    objectIndex: includeIndex ? base.objectIndex : null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    noiseEnabled,
    noiseFrequency,
    acceleration,
    direction,
    selected,
  };
}

function readSelfShadowSection(cursor, { keyframeLimit }) {
  const enabled = cursor.readBool();
  const currentDistance = cursor.readFloat32();
  const initialKeyframe = readSelfShadowKeyframe(cursor, { includeIndex: false });
  const keyframeCount = cursor.readInt32();
  const keyframes = readRepeated(cursor, keyframeCount, () => readSelfShadowKeyframe(cursor, { includeIndex: false }));
  return {
    enabled,
    currentDistance,
    counts: { initialSelfShadowKeyframe: 1, selfShadowKeyframes: keyframes.length },
    initialKeyframe,
    keyframes: limited(keyframes, keyframeLimit),
  };
}

function readSelfShadowKeyframe(cursor, { includeIndex }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const mode = cursor.readByte();
  const distance = cursor.readFloat32();
  const selected = cursor.readBool();
  return {
    kind: "selfShadow",
    offset,
    offsetHex: hex(offset),
    byteLength: cursor.offset - offset,
    objectIndex: includeIndex ? base.objectIndex : null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    mode,
    distance,
    selected,
  };
}

function readCameraKeyframe(cursor, { includeIndex }) {
  const offset = cursor.offset;
  const base = readBaseKeyframe(cursor, includeIndex);
  const distance = cursor.readFloat32();
  const position = cursor.readFloat32Array(3);
  const rotation = cursor.readFloat32Array(3);
  const parentModelIndex = cursor.readInt32();
  const parentModelBoneIndex = cursor.readInt32();
  const interpolation = readRepeated(cursor, 6, () => [...cursor.readBytes(4)]);
  const perspective = !cursor.readBool();
  const fov = cursor.readInt32();
  const selected = cursor.readBool();
  const endOffset = cursor.offset;
  return {
    kind: "camera",
    offset,
    offsetHex: hex(offset),
    byteLength: endOffset - offset,
    objectIndex: includeIndex ? base.objectIndex : null,
    frame: base.frame,
    previousKeyframeIndex: base.previousKeyframeIndex,
    nextKeyframeIndex: base.nextKeyframeIndex,
    distance,
    position,
    rotation,
    parentModelIndex,
    parentModelBoneIndex,
    interpolation,
    perspective,
    fov,
    selected,
  };
}

function resolveNamedKeyframeObjects(initialKeyframes, keyframes, names) {
  const allKeyframes = [...initialKeyframes, ...keyframes];
  for (const keyframe of keyframes) {
    const objectIndex = resolveKeyframeObjectIndex(keyframe, allKeyframes, initialKeyframes.length);
    keyframe.objectIndex = objectIndex;
    keyframe.name = names[objectIndex] ?? null;
  }
}

function resolveKeyframeObjectIndex(keyframe, allKeyframes, initialKeyframeCount) {
  let lastKeyframeIndex = 0;
  let keyframeIndex = keyframe.previousKeyframeIndex;
  if (keyframeIndex > 0) {
    while (keyframeIndex > 0 && keyframeIndex < allKeyframes.length && keyframeIndex !== lastKeyframeIndex) {
      const previous = allKeyframes[keyframeIndex];
      lastKeyframeIndex = keyframeIndex;
      keyframeIndex = previous.previousKeyframeIndex;
    }
    return lastKeyframeIndex < initialKeyframeCount ? allKeyframes[lastKeyframeIndex].objectIndex : 0;
  }
  return keyframe.objectIndex < initialKeyframeCount ? keyframe.objectIndex : 0;
}

function readBaseKeyframe(cursor, includeIndex) {
  return {
    objectIndex: includeIndex ? cursor.readInt32() : null,
    frame: cursor.readInt32(),
    previousKeyframeIndex: cursor.readInt32(),
    nextKeyframeIndex: cursor.readInt32(),
  };
}

function readRepeated(cursor, count, read) {
  const values = [];
  for (let index = 0; index < count; index += 1) {
    values.push(read(index));
  }
  return values;
}

function readVariableStrings(cursor, count) {
  return readRepeated(cursor, count, () => cursor.readVariableString());
}

function readInt32Array(cursor, count) {
  return readRepeated(cursor, count, () => cursor.readInt32());
}

function summarizeCounts(models, camera) {
  return {
    models: models.length,
    bones: sum(models, (model) => model.boneCount),
    morphs: sum(models, (model) => model.morphCount),
    boneKeyframes: sum(models, (model) => model.counts.boneKeyframes),
    morphKeyframes: sum(models, (model) => model.counts.morphKeyframes),
    modelKeyframes: sum(models, (model) => model.counts.modelKeyframes),
    cameraKeyframes: camera?.counts.cameraKeyframes ?? 0,
  };
}

function sum(values, getValue) {
  return values.reduce((total, value) => total + getValue(value), 0);
}

function limited(values, limit) {
  return typeof limit === "number" ? values.slice(0, limit) : values;
}

function hex(value) {
  return `0x${value.toString(16)}`;
}

class PmmCursor {
  constructor(bytes) {
    this.bytes = Buffer.from(bytes);
    this.offset = 0;
  }

  get length() {
    return this.bytes.byteLength;
  }

  peekAscii(offset, length) {
    this.assertCanReadAt(offset, length);
    return this.bytes.subarray(offset, offset + length).toString("ascii");
  }

  readByte() {
    this.assertCanRead(1);
    const value = this.bytes[this.offset];
    this.offset += 1;
    return value;
  }

  readBool() {
    return this.readByte() !== 0;
  }

  readInt32() {
    this.assertCanRead(4);
    const value = this.bytes.readInt32LE(this.offset);
    this.offset += 4;
    return value;
  }

  readFloat32() {
    this.assertCanRead(4);
    const value = this.bytes.readFloatLE(this.offset);
    this.offset += 4;
    return value;
  }

  readFloat32Array(count) {
    return readRepeated(this, count, () => this.readFloat32());
  }

  readBytes(length) {
    this.assertCanRead(length);
    const value = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return value;
  }

  readFixedString(length) {
    const bytes = this.readBytes(length);
    const nullIndex = bytes.indexOf(0);
    return decodeShiftJis(nullIndex >= 0 ? bytes.subarray(0, nullIndex) : bytes);
  }

  readVariableString() {
    const length = this.readByte();
    return decodeShiftJis(this.readBytes(length));
  }

  skip(length) {
    this.assertCanRead(length);
    this.offset += length;
  }

  assertCanRead(length) {
    this.assertCanReadAt(this.offset, length);
  }

  assertCanReadAt(offset, length) {
    if (offset < 0 || length < 0 || offset + length > this.length) {
      throw new Error(`PMM_UNEXPECTED_EOF:${hex(offset)}+${length}>${hex(this.length)}`);
    }
  }
}

function decodeShiftJis(bytes) {
  return iconv.decode(Buffer.from(bytes), "cp932");
}
