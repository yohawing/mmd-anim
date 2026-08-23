import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, extname, resolve } from "node:path";
import iconv from "iconv-lite";
import { parsePmmDocumentKeyframes } from "./pmm-document-keyframes.mjs";
import { patchPmmDocumentVmdKeyframes } from "./pmm-document-vmd-patch.mjs";
import { readPmdInventory } from "./pmd-inventory.mjs";
import { readPmxInventory } from "./pmx-inventory.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

const DEFAULT_INTERPOLATION = [20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107];
const DEFAULT_CAMERA_INTERPOLATION = [20, 20, 107, 107];
const PMX_BONE_FLAG_TRANSLATABLE = 0x0004;
const PMX_BONE_FLAG_VISIBLE = 0x0008;
const PMX_BONE_FLAG_IK = 0x0020;

export async function writeBasePmmFromPmx(options) {
  const pmx = resolve(requireString(options, "pmx"));
  const out = resolve(requireString(options, "out"));
  await access(pmx);
  const bytes = await createBasePmmFromPmx({ pmx, ...options });
  await mkdir(dirname(out), { recursive: true });
  await writeFile(out, bytes);
  return summarizeGeneratedPmm({ mode: "base-pmm-from-pmx", pmx, out, bytes });
}

export async function writePmmFromPmxVmd(options) {
  const pmx = resolve(requireString(options, "pmx"));
  const vmd = resolve(requireString(options, "vmd"));
  const cameraVmd = options.cameraVmd ? resolve(options.cameraVmd) : undefined;
  const out = resolve(requireString(options, "out"));
  await Promise.all([access(pmx), access(vmd), cameraVmd ? access(cameraVmd) : Promise.resolve()]);
  const [inventory, targetVmd, targetCameraVmd] = await Promise.all([
    readModelInventory(pmx, { limit: Number.MAX_SAFE_INTEGER }),
    readVmdInventory(vmd, { limit: Number.MAX_SAFE_INTEGER }),
    cameraVmd ? readVmdInventory(cameraVmd, { limit: Number.MAX_SAFE_INTEGER }) : Promise.resolve(undefined),
  ]);
  const filtered = filterVmdForPmxInventory(targetVmd, inventory, {
    missingNames: options.missingNames ?? "skip",
  });
  // MMD renders a freshly-loaded PMM from each model's current morph state (the editing
  // pose), not by evaluating the morph keyframe at the cursor. patchPmmDocumentVmdKeyframes
  // only rewrites the keyframe section, so without this the morph would stay at weight 0 on
  // screen even though frame 0 carries weight 1. Seed the current morph state from the morph
  // weights effective at the captured frame so a single-frame static render reflects the morph.
  const morphStateFrame = Number.isFinite(options.morphStateFrame) ? options.morphStateFrame : 0;
  const morphStateByName = morphWeightsAtFrame(filtered.vmd.morphs ?? [], morphStateFrame);
  const baseBytes = createBasePmmFromPmxInventory({
    ...options,
    pmx,
    inventory,
    cameraFrames: targetCameraVmd?.cameraFrames,
    lightFrames: targetVmd.lightFrames,
    maxFrame: Math.max(targetVmd.maxFrame, targetCameraVmd?.maxFrame ?? 0),
    morphStateByName,
  });
  const hasModelMotionFrames = filtered.vmd.counts.boneFrames > 0 || filtered.vmd.counts.morphFrames > 0;
  const patched = hasModelMotionFrames
    ? patchPmmDocumentVmdKeyframes(baseBytes, modelOnlyVmdForPmmPatch(filtered.vmd), { targetSlot: 0, requireVerified: true })
    : { bytes: baseBytes };
  await mkdir(dirname(out), { recursive: true });
  await writeFile(out, patched.bytes);
  return {
    ...summarizeGeneratedPmm({ mode: "pmm-from-pmx-vmd", pmx, out, bytes: patched.bytes }),
    vmd,
    cameraVmd,
    cameraVmdCounts: targetCameraVmd
      ? {
          cameraFrames: targetCameraVmd.counts.cameraFrames,
          maxFrame: maxFrameNumber(targetCameraVmd.cameraFrames ?? []),
        }
      : undefined,
    lightVmdCounts:
      targetVmd.counts.lightFrames > 0
        ? {
            lightFrames: targetVmd.counts.lightFrames,
            maxFrame: maxFrameNumber(targetVmd.lightFrames ?? []),
          }
        : undefined,
    filter: filtered.report,
    patch: hasModelMotionFrames ? withoutBytes(patched) : undefined,
  };
}

export async function writePmmCameraVmdPatch(options) {
  const template = resolve(requireString(options, "template"));
  const cameraVmd = resolve(requireString(options, "cameraVmd"));
  const out = resolve(requireString(options, "out"));
  await Promise.all([access(template), access(cameraVmd)]);
  const [templateBytes, targetCameraVmd] = await Promise.all([
    readFile(template),
    readVmdInventory(cameraVmd, { limit: Number.MAX_SAFE_INTEGER }),
  ]);
  const document = parsePmmDocumentKeyframes(templateBytes);
  const camera = options.camera ? normalizeCameraState(options) : cameraStateFromPmmKeyframe(document.camera.initialKeyframe);
  const cameraSection = createPmmCameraSection({ camera, cameraFrames: targetCameraVmd.cameraFrames });
  const start = document.camera.offset;
  const end = document.camera.offset + document.camera.byteLength;
  const patched = Buffer.concat([templateBytes.subarray(0, start), cameraSection, templateBytes.subarray(end)]);
  await mkdir(dirname(out), { recursive: true });
  await writeFile(out, patched);
  const patchedDocument = parsePmmDocumentKeyframes(patched);
  return {
    ok: true,
    mode: "pmm-camera-vmd-patch",
    template,
    cameraVmd,
    out,
    byteLength: patched.byteLength,
    byteLengthDelta: patched.byteLength - templateBytes.byteLength,
    cameraVmdCounts: {
      cameraFrames: targetCameraVmd.counts.cameraFrames,
      maxFrame: maxFrameNumber(targetCameraVmd.cameraFrames ?? []),
    },
    camera: {
      initialCameraKeyframe: patchedDocument.camera.counts.initialCameraKeyframe,
      cameraKeyframes: patchedDocument.camera.counts.cameraKeyframes,
      firstFrame: patchedDocument.camera.initialKeyframe.frame,
      lastFrame: patchedDocument.camera.keyframes.at(-1)?.frame ?? patchedDocument.camera.initialKeyframe.frame,
    },
  };
}

export async function createBasePmmFromPmx(options) {
  const pmx = resolve(requireString(options, "pmx"));
  const inventory = await readModelInventory(pmx, { limit: Number.MAX_SAFE_INTEGER });
  return createBasePmmFromPmxInventory({
    ...options,
    pmx,
    inventory,
  });
}

export async function readModelInventory(path, options = {}) {
  const extension = extname(path).toLowerCase();
  if (extension === ".pmd") {
    return readPmdInventory(path, options);
  }
  return readPmxInventory(path, options);
}

export function createBasePmmFromPmxInventory(options) {
  const pmx = requireString(options, "pmx");
  const inventory = options.inventory;
  if (!inventory || typeof inventory !== "object") {
    throw new Error("createBasePmmFromPmxInventory requires inventory.");
  }
  const writer = new PmmWriter();
  const bones = inventory.bones ?? [];
  const morphs = inventory.morphs ?? [];
  const camera = normalizeCameraState(options);
  const modelStructure = pmmModelStructureFromInventory(inventory);

  writer.fixedAscii("Polygon Movie maker 0002", 30);
  writer.int32(options.outputWidth ?? 640);
  writer.int32(options.outputHeight ?? 360);
  writer.int32(options.timelineWidth ?? 480);
  writer.float32(camera.fov);
  writer.bytes([0, 1, 1, 1, 1, 1, 0]);
  writer.byte(0); // selected model index
  writer.byte(1); // model count

  writer.byte(0); // document model index
  writer.variableString(inventory.modelName || "");
  writer.variableString(inventory.modelNameEnglish || "");
  writer.fixedString(pmx, 256, "PMX path");
  writer.byte(modelStructure.fixedTracks);
  writer.int32(bones.length);
  for (const bone of bones) {
    writer.variableString(bone.name);
  }
  writer.int32(morphs.length);
  for (const morph of morphs) {
    writer.variableString(morph.name);
  }
  writer.int32(modelStructure.constraintBoneIndices.length);
  for (const index of modelStructure.constraintBoneIndices) {
    writer.int32(index);
  }
  writer.int32(modelStructure.outsideParentSubjectBoneIndices.length);
  for (const index of modelStructure.outsideParentSubjectBoneIndices) {
    writer.int32(index);
  }
  writer.byte(1); // draw order (render order is a 1-based sequence per PMMv2 spec; 0 is invalid and MMD edge/draw handling depends on it)
  writer.byte(1); // visible
  writer.int32(bones.length > 0 ? 0 : -1);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.byte(0); // expansion state count
  writer.int32(0); // vertical scroll
  writer.int32(0); // last frame

  for (const bone of bones) {
    writer.boneKeyframe({ frame: 0, translation: [0, 0, 0], orientation: [0, 0, 0, 1] });
  }
  writer.int32(0); // additional bone keyframes
  for (const morph of morphs) {
    writer.morphKeyframe({ frame: 0, weight: 0 });
  }
  writer.int32(0); // additional morph keyframes
  writer.modelKeyframe({
    frame: 0,
    visible: true,
    constraintStates: modelStructure.constraintBoneIndices.map(() => true),
    outsideParents: modelStructure.outsideParentSubjectBoneIndices.map(() => defaultOutsideParentKeyframeState()),
  });
  writer.int32(0); // additional model keyframes

  for (const bone of bones) {
    writer.boneState();
  }
  const morphStateByName = options.morphStateByName instanceof Map ? options.morphStateByName : new Map();
  for (const morph of morphs) {
    const weight = morphStateByName.get(morph.name);
    writer.float32(Number.isFinite(weight) ? weight : 0);
  }
  for (const index of modelStructure.constraintBoneIndices) {
    writer.byte(1);
  }
  for (const index of modelStructure.outsideParentSubjectBoneIndices) {
    writer.outsideParentCurrentState(defaultOutsideParentCurrentState());
  }
  writer.byte(0); // blend disabled
  writer.float32(1); // edge width
  writer.byte(options.selfShadow === false ? 0 : 1); // per-model self-shadow (document-level is on; this opts the model in)
  writer.byte(1); // transform order (1-based, single model)
  writer.documentTail({
    camera,
    cameraFrames: options.cameraFrames,
    lightFrames: options.lightFrames,
    modelCount: 1,
    maxFrame: options.maxFrame,
    selfShadowDistance: options.selfShadowDistance,
  });

  const bytes = writer.buffer();
  parsePmmDocumentKeyframes(bytes);
  return bytes;
}

function pmmModelStructureFromInventory(inventory) {
  const bones = inventory.bones ?? [];
  const hasPmxBoneFlags = bones.some((bone) => Number.isInteger(bone.flags));
  const constraintBoneIndices = normalizeIndexList(
    inventory.ikBoneIndices ?? bones.filter((bone) => (bone.flags & PMX_BONE_FLAG_IK) !== 0).map((bone) => bone.index),
  );
  const outsideParentSubjectBoneIndices = normalizeIndexList(
    inventory.outsideParentSubjectBoneIndices ??
      (hasPmxBoneFlags
        ? [-1, ...bones.filter((bone) => isOutsideParentSubjectBone(bone)).map((bone) => bone.index)]
        : []),
  );
  return {
    fixedTracks: Number.isInteger(inventory.fixedTracks)
      ? clampByte(inventory.fixedTracks)
      : Array.isArray(inventory.displayFrames)
        ? clampByte(inventory.displayFrames.length > 0 ? inventory.displayFrames.length + 1 : 0)
        : 0,
    constraintBoneIndices,
    outsideParentSubjectBoneIndices,
  };
}

function isOutsideParentSubjectBone(bone) {
  if (!Number.isInteger(bone?.index) || !Number.isInteger(bone.flags)) {
    return false;
  }
  return (
    (bone.flags & PMX_BONE_FLAG_IK) !== 0 ||
    ((bone.flags & PMX_BONE_FLAG_VISIBLE) !== 0 && (bone.flags & PMX_BONE_FLAG_TRANSLATABLE) !== 0)
  );
}

function normalizeIndexList(indices) {
  if (!Array.isArray(indices)) {
    return [];
  }
  const normalized = [];
  for (const index of indices) {
    if (Number.isInteger(index) && !normalized.includes(index)) {
      normalized.push(index);
    }
  }
  return normalized;
}

function defaultOutsideParentKeyframeState() {
  return { modelIndex: -1, boneIndex: 0 };
}

function defaultOutsideParentCurrentState() {
  return { enabled: false, sourceModelIndex: 0, parentModelIndex: -1, parentBoneIndex: 0 };
}

function clampByte(value) {
  return Math.max(0, Math.min(255, value));
}

function summarizeGeneratedPmm({ mode, pmx, out, bytes }) {
  const document = parsePmmDocumentKeyframes(bytes);
  const model = document.models[0];
  return {
    ok: true,
    mode,
    pmx,
    out,
    byteLength: bytes.byteLength,
    counts: {
      models: document.counts.models,
      bones: model.boneCount,
      morphs: model.morphCount,
      boneKeyframes: model.counts.boneKeyframes,
      morphKeyframes: model.counts.morphKeyframes,
      cameraKeyframes: document.camera?.counts.cameraKeyframes ?? 0,
    },
    model: {
      name: model.nameJa,
      path: model.path,
    },
  };
}

function filterVmdForPmxInventory(vmd, inventory, options = {}) {
  const missingNames = options.missingNames ?? "skip";
  if (!["skip", "strict"].includes(missingNames)) {
    throw new Error(`missingNames must be "skip" or "strict", got ${missingNames}.`);
  }
  const boneNames = new Set((inventory.bones ?? []).map((bone) => bone.name));
  const morphNames = new Set((inventory.morphs ?? []).map((morph) => morph.name));
  const appliedBones = [];
  const skippedBones = [];
  for (const frame of vmd.bones) {
    (boneNames.has(frame.name) ? appliedBones : skippedBones).push(frame);
  }
  const appliedMorphs = [];
  const skippedMorphs = [];
  for (const frame of vmd.morphs) {
    (morphNames.has(frame.name) ? appliedMorphs : skippedMorphs).push(frame);
  }
  if (missingNames === "strict" && (skippedBones.length > 0 || skippedMorphs.length > 0)) {
    const firstBone = skippedBones[0]?.name;
    const firstMorph = skippedMorphs[0]?.name;
    throw new Error(
      `VMD references names missing from PMX: ${[
        firstBone ? `bone=${firstBone}` : undefined,
        firstMorph ? `morph=${firstMorph}` : undefined,
      ]
        .filter(Boolean)
        .join(", ")}.`,
    );
  }
  const maxFrame = maxFrameNumber(appliedBones, appliedMorphs);
  return {
    vmd: {
      ...vmd,
      counts: {
        ...vmd.counts,
        boneFrames: appliedBones.length,
        morphFrames: appliedMorphs.length,
        propertyFrames: 0,
      },
      maxFrame,
      bones: appliedBones,
      morphs: appliedMorphs,
      propertyFrames: [],
      boneNameCounts: countFramesByName(appliedBones),
      morphNameCounts: countFramesByName(appliedMorphs),
    },
    report: {
      mode: missingNames,
      originalCounts: {
        boneFrames: vmd.counts.boneFrames,
        morphFrames: vmd.counts.morphFrames,
      },
      appliedCounts: {
        boneFrames: appliedBones.length,
        morphFrames: appliedMorphs.length,
      },
      skippedCounts: {
        boneFrames: skippedBones.length,
        morphFrames: skippedMorphs.length,
      },
      skippedBoneNames: countFramesByName(skippedBones),
      skippedMorphNames: countFramesByName(skippedMorphs),
      droppedUnsupportedChannels: {
        propertyFrames: vmd.counts.propertyFrames ?? 0,
      },
    },
  };
}

function modelOnlyVmdForPmmPatch(vmd) {
  return {
    ...vmd,
    counts: {
      ...vmd.counts,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    cameraFrames: [],
    lightFrames: [],
    selfShadowFrames: [],
    propertyFrames: [],
  };
}

// Resolve each morph's weight effective at `frame` using MMD hold semantics:
// the latest keyframe at or before the frame, or the earliest keyframe if all are later.
function morphWeightsAtFrame(morphFrames, frame) {
  const chosen = new Map();
  for (const keyframe of morphFrames) {
    if (!keyframe || typeof keyframe.name !== "string") {
      continue;
    }
    const current = chosen.get(keyframe.name);
    if (!current || isMoreEffectiveAtFrame(keyframe, current, frame)) {
      chosen.set(keyframe.name, keyframe);
    }
  }
  const weights = new Map();
  for (const [name, keyframe] of chosen) {
    if (Number.isFinite(keyframe.weight)) {
      weights.set(name, keyframe.weight);
    }
  }
  return weights;
}

function isMoreEffectiveAtFrame(candidate, current, frame) {
  const candidateAtOrBefore = candidate.frame <= frame;
  const currentAtOrBefore = current.frame <= frame;
  if (candidateAtOrBefore && currentAtOrBefore) {
    return candidate.frame > current.frame;
  }
  if (candidateAtOrBefore !== currentAtOrBefore) {
    return candidateAtOrBefore;
  }
  return candidate.frame < current.frame;
}

function maxFrameNumber(...groups) {
  let max = 0;
  for (const frames of groups) {
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

function normalizeCameraState(options) {
  if (!options.camera) {
    return defaultCameraState(options.cameraFov);
  }
  const position = normalizeVector3(options.camera.position, "camera.position");
  const target = normalizeVector3(options.camera.target, "camera.target");
  const dx = position[0] - target[0];
  const dy = position[1] - target[1];
  const dz = position[2] - target[2];
  const distance = Math.hypot(dx, dy, dz);
  if (!Number.isFinite(distance) || distance <= 0) {
    throw new Error("camera position and target must not be the same point.");
  }
  const nx = dx / distance;
  const ny = dy / distance;
  const nz = dz / distance;
  return {
    distance: -distance,
    position: [target[0], target[1], -target[2]],
    // MMD camera pitch is inverted vs eye-above-target: a camera placed above the target
    // (ny>0) must pitch DOWN to look at it, so negate asin(ny). Without this, raising the
    // camera renders as if shooting from below. (Near-horizontal cameras with dy~0 are unaffected.)
    rotation: [-Math.asin(clamp(ny, -1, 1)), -Math.atan2(nx, nz), 0],
    fov: normalizePositiveNumber(options.camera.fov ?? options.cameraFov ?? 30, "camera.fov"),
    perspective: options.camera.perspective !== false,
  };
}

function defaultCameraState(cameraFov = 30) {
  return {
    distance: -45,
    position: [0, 10, 0],
    rotation: [0, 0, 0],
    fov: normalizePositiveNumber(cameraFov ?? 30, "camera.fov"),
    perspective: true,
  };
}

function normalizeCameraTimeline(camera, cameraFrames) {
  const deduped = new Map();
  for (const frame of cameraFrames ?? []) {
    if (Number.isFinite(frame.frame)) {
      deduped.set(frame.frame, frame);
    }
  }
  const sorted = [...deduped.values()].sort((left, right) => left.frame - right.frame);
  const firstFrame = sorted[0];
  const frameZero = sorted.find((frame) => frame.frame === 0);
  const initial = frameZero ? cameraStateFromVmdFrame(frameZero) : { ...camera, frame: 0 };
  const additionalSource = sorted.filter((frame) => frame !== frameZero);
  const additional = additionalSource.map((frame, index) => {
    const keyframeIndex = index + 1;
    return {
      ...cameraStateFromVmdFrame(frame),
      includeIndex: true,
      objectIndex: keyframeIndex,
      previous: keyframeIndex === 1 ? 0 : keyframeIndex - 1,
      next: keyframeIndex === additionalSource.length ? 0 : keyframeIndex + 1,
    };
  });
  return {
    initial: {
      ...initial,
      frame: initial.frame ?? 0,
      previous: 0,
      next: additional.length > 0 ? 1 : 0,
    },
    current: firstFrame ? cameraStateFromVmdFrame(firstFrame) : initial,
    additional,
  };
}

function normalizeLightTimeline(lightFrames) {
  const deduped = new Map();
  for (const frame of lightFrames ?? []) {
    if (Number.isFinite(frame.frame)) {
      deduped.set(frame.frame, frame);
    }
  }
  const sorted = [...deduped.values()].sort((left, right) => left.frame - right.frame);
  const fallback = defaultLightState();
  const firstFrame = sorted[0];
  const frameZero = sorted.find((frame) => frame.frame === 0);
  const initial = frameZero ? lightStateFromVmdFrame(frameZero) : { ...fallback, frame: 0 };
  const additionalSource = sorted.filter((frame) => frame !== frameZero);
  const additional = additionalSource.map((frame, index) => {
    const keyframeIndex = index + 1;
    return {
      ...lightStateFromVmdFrame(frame),
      includeIndex: true,
      objectIndex: keyframeIndex,
      previous: keyframeIndex === 1 ? 0 : keyframeIndex - 1,
      next: keyframeIndex === additionalSource.length ? 0 : keyframeIndex + 1,
    };
  });
  return {
    initial: {
      ...initial,
      frame: initial.frame ?? 0,
      previous: 0,
      next: additional.length > 0 ? 1 : 0,
    },
    current: firstFrame ? lightStateFromVmdFrame(firstFrame) : initial,
    additional,
  };
}

function defaultLightState() {
  return {
    frame: 0,
    color: [0.6, 0.6, 0.6],
    direction: [-0.5, -1, 0.5],
  };
}

function lightStateFromVmdFrame(frame) {
  return {
    frame: frame.frame,
    color: normalizeVector3(frame.color, "light.color"),
    direction: normalizeVector3(frame.direction, "light.direction"),
  };
}

function cameraStateFromVmdFrame(frame) {
  return {
    frame: frame.frame,
    distance: frame.distance,
    position: frame.position,
    rotation: frame.rotation,
    interpolation: cameraInterpolationFromVmdFrame(frame),
    fov: frame.fov,
    perspective: frame.perspective === 0 || frame.perspective === true,
  };
}

function cameraStateFromPmmKeyframe(keyframe) {
  return {
    frame: keyframe.frame ?? 0,
    distance: keyframe.distance,
    position: keyframe.position,
    rotation: keyframe.rotation,
    interpolation: keyframe.interpolation,
    fov: keyframe.fov,
    perspective: keyframe.perspective,
  };
}

function cameraInterpolationFromVmdFrame(frame) {
  if (!frame.interpolationHex) {
    return undefined;
  }
  const bytes = Buffer.from(frame.interpolationHex, "hex");
  if (bytes.byteLength !== 24) {
    return undefined;
  }
  return Array.from({ length: 6 }, (_, index) => [...bytes.subarray(index * 4, index * 4 + 4)]);
}

function createPmmCameraSection(options = {}) {
  const writer = new PmmWriter();
  const camera = options.camera ?? defaultCameraState();
  const cameraTimeline = normalizeCameraTimeline(camera, options.cameraFrames);
  const currentCamera = cameraTimeline.current;
  writer.cameraKeyframe(cameraTimeline.initial);
  writer.int32(cameraTimeline.additional.length);
  for (const keyframe of cameraTimeline.additional) {
    writer.cameraKeyframe(keyframe);
  }
  const worldPosition = mmdCameraWorldPosition(currentCamera);
  writer.float32(worldPosition[0]);
  writer.float32(worldPosition[1]);
  writer.float32(worldPosition[2]);
  writer.float32(currentCamera.position[0]);
  writer.float32(currentCamera.position[1]);
  writer.float32(currentCamera.position[2]);
  writer.float32(currentCamera.rotation[0]);
  writer.float32(currentCamera.rotation[1]);
  writer.float32(currentCamera.rotation[2]);
  writer.byte(currentCamera.perspective === false ? 1 : 0);
  return writer.buffer();
}

function createPmmLightSection(options = {}) {
  const writer = new PmmWriter();
  const lightTimeline = normalizeLightTimeline(options.lightFrames);
  const currentLight = lightTimeline.current;
  writer.lightKeyframe(lightTimeline.initial);
  writer.int32(lightTimeline.additional.length);
  for (const keyframe of lightTimeline.additional) {
    writer.lightKeyframe(keyframe);
  }
  for (const value of currentLight.color) {
    writer.float32(value);
  }
  for (const value of currentLight.direction) {
    writer.float32(value);
  }
  return writer.buffer();
}

function mmdCameraWorldPosition(camera) {
  const rx = camera.rotation[0];
  const ry = camera.rotation[1];
  const length = -camera.distance;
  const threeDx = Math.sin(-ry) * Math.cos(-rx) * length;
  const threeDy = Math.sin(rx) * length;
  const threeDz = Math.cos(-ry) * Math.cos(-rx) * length;
  return [camera.position[0] + threeDx, camera.position[1] + threeDy, camera.position[2] + threeDz];
}

function normalizeVector3(value, label) {
  if (!Array.isArray(value) || value.length !== 3 || value.some((item) => !Number.isFinite(item))) {
    throw new Error(`${label} must be a finite number array with 3 entries.`);
  }
  return value;
}

function normalizePositiveNumber(value, label) {
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`${label} must be a positive finite number.`);
  }
  return value;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

class PmmWriter {
  constructor() {
    this.parts = [];
  }

  buffer() {
    return Buffer.concat(this.parts);
  }

  byte(value) {
    this.parts.push(Buffer.from([value & 0xff]));
  }

  bytes(values) {
    this.parts.push(Buffer.from(values));
  }

  int32(value) {
    const bytes = Buffer.alloc(4);
    bytes.writeInt32LE(value);
    this.parts.push(bytes);
  }

  float32(value) {
    const bytes = Buffer.alloc(4);
    bytes.writeFloatLE(value);
    this.parts.push(bytes);
  }

  variableString(value) {
    const encoded = iconv.encode(value ?? "", "cp932");
    if (encoded.byteLength > 255) {
      throw new Error(`PMM variable string is too long: ${value}`);
    }
    this.byte(encoded.byteLength);
    this.parts.push(encoded);
  }

  fixedAscii(value, byteLength) {
    const bytes = Buffer.alloc(byteLength);
    Buffer.from(value, "ascii").copy(bytes);
    this.parts.push(bytes);
  }

  fixedString(value, byteLength, label = "fixed string") {
    const encoded = iconv.encode(value ?? "", "cp932");
    if (encoded.byteLength > byteLength) {
      throw new Error(`${label} is too long for PMM fixed field: ${value}`);
    }
    const bytes = Buffer.alloc(byteLength);
    encoded.copy(bytes);
    this.parts.push(bytes);
  }

  baseKeyframe({ includeIndex = false, objectIndex = 0, frame, previous = 0, next = 0 }) {
    if (includeIndex) {
      this.int32(objectIndex);
    }
    this.int32(frame);
    this.int32(previous);
    this.int32(next);
  }

  boneKeyframe(options) {
    this.baseKeyframe(options);
    this.bytes(DEFAULT_INTERPOLATION);
    for (const value of options.translation) {
      this.float32(value);
    }
    for (const value of options.orientation) {
      this.float32(value);
    }
    this.byte(options.selected ? 1 : 0);
    this.byte(options.physicsSimulationDisabled ? 1 : 0);
  }

  morphKeyframe(options) {
    this.baseKeyframe(options);
    this.float32(options.weight);
    this.byte(options.selected ? 1 : 0);
  }

  modelKeyframe(options) {
    this.baseKeyframe(options);
    this.byte(options.visible ? 1 : 0);
    for (const state of options.constraintStates ?? []) {
      this.byte(state ? 1 : 0);
    }
    for (const outsideParent of options.outsideParents ?? []) {
      this.int32(outsideParent.modelIndex ?? -1);
      this.int32(outsideParent.boneIndex ?? 0);
    }
    this.byte(options.selected ? 1 : 0);
  }

  cameraKeyframe(options = {}) {
    this.baseKeyframe(options);
    this.float32(options.distance ?? -45);
    const position = options.position ?? [0, 10, 0];
    const rotation = options.rotation ?? [0, 0, 0];
    this.float32(position[0]);
    this.float32(position[1]);
    this.float32(position[2]);
    this.float32(rotation[0]);
    this.float32(rotation[1]);
    this.float32(rotation[2]);
    this.int32(options.parentModelIndex ?? -1);
    this.int32(options.parentModelBoneIndex ?? 0);
    const interpolation = options.interpolation ?? [];
    for (let index = 0; index < 6; index += 1) {
      this.bytes(interpolation[index] ?? DEFAULT_CAMERA_INTERPOLATION);
    }
    this.byte(options.perspective === false ? 1 : 0);
    this.int32(options.fov ?? 30);
    this.byte(options.selected ? 1 : 0);
  }

  lightKeyframe(options = {}) {
    this.baseKeyframe(options);
    const light = { ...defaultLightState(), ...options };
    this.float32(light.color[0]);
    this.float32(light.color[1]);
    this.float32(light.color[2]);
    this.float32(light.direction[0]);
    this.float32(light.direction[1]);
    this.float32(light.direction[2]);
    this.byte(options.selected ? 1 : 0);
  }

  gravityKeyframe(options = {}) {
    this.baseKeyframe(options);
    this.byte(options.noiseEnabled ? 1 : 0);
    this.int32(10);
    this.float32(9.8);
    this.float32(0);
    this.float32(-1);
    this.float32(0);
    this.byte(options.selected ? 1 : 0);
  }

  selfShadowKeyframe(options = {}) {
    this.baseKeyframe(options);
    this.byte(options.mode ?? 1);
    this.float32(options.distance ?? 0.01125);
    this.byte(options.selected ? 1 : 0);
  }

  boneState() {
    this.float32(0);
    this.float32(0);
    this.float32(0);
    this.float32(0);
    this.float32(0);
    this.float32(0);
    this.float32(1);
    this.byte(0);
    this.byte(0);
    this.byte(0);
  }

  outsideParentCurrentState(options = {}) {
    this.int32(options.enabled ? 1 : 0);
    this.int32(options.sourceModelIndex ?? 0);
    this.int32(options.parentModelIndex ?? -1);
    this.int32(options.parentBoneIndex ?? 0);
  }

  documentTail(options = {}) {
    const camera = options.camera ?? defaultCameraState();
    const modelCount = options.modelCount ?? 1;

    this.parts.push(createPmmCameraSection({ camera, cameraFrames: options.cameraFrames }));

    this.parts.push(createPmmLightSection({ lightFrames: options.lightFrames }));

    this.byte(0); // selected accessory index
    this.int32(0); // accessory horizontal scroll
    this.byte(0); // accessory count

    this.int32(0); // current frame
    this.int32(0); // horizontal scroll
    this.int32(735); // horizontal scroll thumb
    this.int32(0); // editing mode
    this.byte(0); // camera look mode
    this.byte(0); // loop disabled
    this.byte(0); // begin frame disabled
    this.byte(options.maxFrame > 0 ? 1 : 0); // end frame enabled
    this.int32(0); // begin frame
    this.int32(options.maxFrame ?? 0); // end frame
    this.byte(0); // audio disabled
    this.fixedString("", 256, "audio path");
    this.int32(0); // background video offset x
    this.int32(0); // background video offset y
    this.float32(1); // background video scale
    this.fixedString("", 256, "background video path");
    this.int32(1); // background video disabled
    this.int32(0); // background image offset x
    this.int32(0); // background image offset y
    this.float32(1); // background image scale
    this.fixedString("", 256, "background image path");
    this.byte(1); // background image disabled
    this.byte(1); // information shown
    this.byte(1); // grid and axis shown
    this.byte(0); // ground shadow shown
    this.float32(60);
    this.int32(0); // screen capture mode off
    this.int32(-1); // accessory index after models
    this.float32(1); // ground shadow brightness

    this.byte(0); // translucent ground shadow disabled
    this.byte(3); // physics simulation tracing
    this.float32(9.8);
    this.int32(10);
    this.float32(0);
    this.float32(-1);
    this.float32(0);
    this.byte(0); // gravity noise disabled
    this.gravityKeyframe();
    this.int32(0); // additional gravity keyframes
    const selfShadowDistance = Number.isFinite(options.selfShadowDistance) ? options.selfShadowDistance : 0.01125;
    this.byte(1); // self-shadow enabled
    this.float32(selfShadowDistance); // self-shadow range/distance (smaller = tighter shadow map)
    this.selfShadowKeyframe({ distance: selfShadowDistance });
    this.int32(0); // additional self-shadow keyframes
    this.float32(0);
    this.float32(0);
    this.float32(0);
    this.byte(0); // black background disabled
    this.int32(-1); // camera look-at model index
    this.int32(0); // camera look-at model bone index
    for (let index = 0; index < 16; index += 1) {
      this.float32(index % 5 === 0 ? 1 : 0);
    }
    this.byte(0); // following look-at disabled
    this.byte(0); // unknown boolean
    this.byte(1); // physics ground enabled
    this.int32(0); // current frame text field
    this.byte(modelCount > 0 ? 1 : 0);
    for (let index = 0; index < modelCount; index += 1) {
      this.byte(index);
      this.int32(0); // model selection index
    }
  }
}

function requireString(options, key) {
  const value = options[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`Missing ${key}.`);
  }
  return value;
}

function withoutBytes(result) {
  const { bytes, ...rest } = result;
  return rest;
}
