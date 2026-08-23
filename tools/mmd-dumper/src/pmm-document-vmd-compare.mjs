import { readPmmDocumentKeyframes } from "./pmm-document-keyframes.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function readPmmDocumentVmdComparison(options) {
  const pmm = await readPmmDocumentKeyframes(options.pmm);
  const vmd = await readVmdInventory(options.vmd, { limit: Number.MAX_SAFE_INTEGER });
  return comparePmmDocumentToVmd(pmm, vmd, options);
}

export function comparePmmDocumentToVmd(pmm, vmd, options = {}) {
  const targetSlot = options.targetSlot ?? 0;
  const model = pmm.models[targetSlot];
  if (!model) {
    return {
      ok: false,
      reason: "PMM_MODEL_SLOT_NOT_FOUND",
      targetSlot,
      availableSlots: pmm.models.map((candidate) => candidate.slot),
    };
  }

  const unsupportedChannels = unsupportedVmdChannels(vmd);

  const vmdBoneFrame0Names = new Set((vmd.bones || []).filter((f) => f.frame === 0).map((f) => f.name));
  const explicitPmmBoneFrame0Names = new Set((model.boneKeyframes || []).filter((f) => f.frame === 0).map((f) => f.name));
  const pmmBoneFrames = [
    ...((model.initialBoneKeyframes || []).filter((f) => vmdBoneFrame0Names.has(f.name) && !explicitPmmBoneFrame0Names.has(f.name))),
    ...(model.boneKeyframes || []),
  ];
  const boneComparison = compareNamedFrames({
    kind: "bone",
    pmmFrames: pmmBoneFrames,
    vmdFrames: vmd.bones,
    compareFrame: (pmmFrame, vmdFrame) => compareBoneFrame(pmmFrame, vmdFrame, options),
  });

  const vmdMorphFrame0Names = new Set((vmd.morphs || []).filter((f) => f.frame === 0).map((f) => f.name));
  const explicitPmmMorphFrame0Names = new Set((model.morphKeyframes || []).filter((f) => f.frame === 0).map((f) => f.name));
  const pmmMorphFrames = [
    ...((model.initialMorphKeyframes || []).filter((f) => vmdMorphFrame0Names.has(f.name) && !explicitPmmMorphFrame0Names.has(f.name))),
    ...(model.morphKeyframes || []),
  ];
  const morphComparison = compareNamedFrames({
    kind: "morph",
    pmmFrames: pmmMorphFrames,
    vmdFrames: vmd.morphs,
    compareFrame: (pmmFrame, vmdFrame) => compareMorphFrame(pmmFrame, vmdFrame, options),
  });
  const vmdCameraFrames = vmd.cameraFrames || [];
  const vmdCameraFrame0 = vmdCameraFrames.some((frame) => frame.frame === 0);
  const explicitPmmCameraFrame0 = (pmm.camera?.keyframes || []).some((frame) => frame.frame === 0);
  const pmmCameraFrames = vmdCameraFrames.length > 0
    ? [
        ...(vmdCameraFrame0 && pmm.camera?.initialKeyframe && !explicitPmmCameraFrame0
          ? [pmm.camera.initialKeyframe]
          : []),
        ...(pmm.camera?.keyframes || []),
      ]
    : [];
  const cameraComparison = compareFrames({
    kind: "camera",
    pmmFrames: pmmCameraFrames,
    vmdFrames: vmdCameraFrames,
    keyOf: (frame) => String(frame.frame),
    compareFrame: (pmmFrame, vmdFrame) => compareCameraFrame(pmmFrame, vmdFrame, options),
  });
  const mismatchCount =
    unsupportedChannels.length +
    boneComparison.mismatches.length +
    morphComparison.mismatches.length +
    cameraComparison.mismatches.length;
  return {
    ok: mismatchCount === 0,
    targetSlot,
    pmm: {
      version: pmm.document.version,
      modelName: model.nameJa,
      modelPath: model.path,
      boneKeyframes: model.counts.boneKeyframes,
      morphKeyframes: model.counts.morphKeyframes,
    },
    vmd: {
      modelName: vmd.modelName,
      counts: vmd.counts,
    },
    unsupportedChannels,
    counts: {
      pmmBoneKeyframes: pmmBoneFrames.length,
      vmdBoneFrames: vmd.bones.length,
      pmmMorphKeyframes: pmmMorphFrames.length,
      vmdMorphFrames: vmd.morphs.length,
      mismatches: mismatchCount,
    },
    boneComparison,
    morphComparison,
    cameraComparison,
  };
}

function unsupportedVmdChannels(vmd) {
  return [
    ["lightFrames", vmd.counts.lightFrames],
    ["selfShadowFrames", vmd.counts.selfShadowFrames],
    ["propertyFrames", vmd.counts.propertyFrames],
  ]
    .filter(([, count]) => count > 0)
    .map(([name, count]) => ({ name, count }));
}

function compareNamedFrames({ kind, pmmFrames, vmdFrames, compareFrame }) {
  return compareFrames({ kind, pmmFrames, vmdFrames, compareFrame });
}

function compareFrames({ kind, pmmFrames, vmdFrames, keyOf = namedFrameKey, compareFrame }) {
  const pmmByKey = frameMap(pmmFrames, keyOf);
  const vmdByKey = frameMap(vmdFrames, keyOf);
  const keys = [...new Set([...pmmByKey.keys(), ...vmdByKey.keys()])].sort();
  const mismatches = [];
  for (const key of keys) {
    const pmmFrame = pmmByKey.get(key);
    const vmdFrame = vmdByKey.get(key);
    if (pmmFrame?.duplicate || vmdFrame?.duplicate) {
      mismatches.push({ kind, key, reason: pmmFrame?.duplicate ? "DUPLICATE_PMM_FRAME" : "DUPLICATE_VMD_FRAME" });
      continue;
    }
    if (!pmmFrame || !vmdFrame) {
      mismatches.push({ kind, key, reason: pmmFrame ? "MISSING_VMD_FRAME" : "MISSING_PMM_FRAME" });
      continue;
    }
    const mismatch = compareFrame(pmmFrame, vmdFrame);
    if (mismatch) {
      mismatches.push({ kind, key, ...mismatch });
    }
  }
  return {
    ok: mismatches.length === 0,
    matched: keys.length - mismatches.length,
    expected: vmdFrames.length,
    actual: pmmFrames.length,
    mismatches,
  };
}

function compareBoneFrame(pmmFrame, vmdFrame, options) {
  const positionDiff = maxAbsDiff(pmmFrame.translation, vmdFrame.position);
  const rotationDiff = maxAbsDiff(pmmFrame.orientation, vmdFrame.rotation);
  const positionEpsilon = options.positionEpsilon ?? 1e-5;
  const rotationEpsilon = options.rotationEpsilon ?? 1e-5;
  if (positionDiff > positionEpsilon || rotationDiff > rotationEpsilon) {
    return {
      reason: "BONE_TRANSFORM_MISMATCH",
      positionDiff,
      rotationDiff,
      pmm: { position: pmmFrame.translation, rotation: pmmFrame.orientation },
      vmd: { position: vmdFrame.position, rotation: vmdFrame.rotation },
    };
  }
  return null;
}

function compareMorphFrame(pmmFrame, vmdFrame, options) {
  const weightDiff = Math.abs(pmmFrame.weight - vmdFrame.weight);
  const weightEpsilon = options.weightEpsilon ?? 1e-5;
  if (weightDiff > weightEpsilon) {
    return {
      reason: "MORPH_WEIGHT_MISMATCH",
      weightDiff,
      pmm: { weight: pmmFrame.weight },
      vmd: { weight: vmdFrame.weight },
    };
  }
  return null;
}

function compareCameraFrame(pmmFrame, vmdFrame, options) {
  const invalidFields = invalidCameraFields(pmmFrame, vmdFrame);
  if (invalidFields.length > 0) {
    return {
      reason: "CAMERA_KEYFRAME_INVALID",
      invalidFields,
      pmm: cameraValues(pmmFrame, pmmFrame.interpolation),
      vmd: cameraValues(vmdFrame, cameraInterpolationFromVmdFrame(vmdFrame)),
    };
  }
  const distanceDiff = Math.abs(pmmFrame.distance - vmdFrame.distance);
  const positionDiff = maxAbsDiff(pmmFrame.position, vmdFrame.position);
  const rotationDiff = maxAbsDiff(pmmFrame.rotation, vmdFrame.rotation);
  const fovDiff = Math.abs(pmmFrame.fov - vmdFrame.fov);
  const pmmInterpolation = pmmFrame.interpolation;
  const vmdInterpolation = cameraInterpolationFromVmdFrame(vmdFrame);
  const interpolationMismatch = !equalNestedArrays(pmmInterpolation, vmdInterpolation);
  const vmdPerspective = vmdFrame.perspective === 0 || vmdFrame.perspective === true;
  const perspectiveMismatch = pmmFrame.perspective !== vmdPerspective;
  const distanceEpsilon = options.cameraDistanceEpsilon ?? options.distanceEpsilon ?? 1e-5;
  const positionEpsilon = options.cameraPositionEpsilon ?? options.positionEpsilon ?? 1e-5;
  const rotationEpsilon = options.cameraRotationEpsilon ?? options.rotationEpsilon ?? 1e-5;
  const fovEpsilon = options.cameraFovEpsilon ?? options.fovEpsilon ?? 0;
  if (
    distanceDiff > distanceEpsilon ||
    positionDiff > positionEpsilon ||
    rotationDiff > rotationEpsilon ||
    fovDiff > fovEpsilon ||
    perspectiveMismatch ||
    interpolationMismatch
  ) {
    return {
      reason: "CAMERA_KEYFRAME_MISMATCH",
      distanceDiff,
      positionDiff,
      rotationDiff,
      fovDiff,
      perspectiveMismatch,
      interpolationMismatch,
      pmm: cameraValues(pmmFrame, pmmInterpolation),
      vmd: cameraValues({ ...vmdFrame, perspective: vmdPerspective }, vmdInterpolation),
    };
  }
  return null;
}

function invalidCameraFields(pmmFrame, vmdFrame) {
  const invalid = [];
  for (const [side, frame] of [["pmm", pmmFrame], ["vmd", vmdFrame]]) {
    for (const field of ["distance", "fov"]) {
      if (!Number.isFinite(frame[field])) {
        invalid.push(`${side}.${field}`);
      }
    }
    for (const field of ["position", "rotation"]) {
      if (!isFiniteVector3(frame[field])) {
        invalid.push(`${side}.${field}`);
      }
    }
  }
  return invalid;
}

function isFiniteVector3(value) {
  return Array.isArray(value) && value.length === 3 && value.every(Number.isFinite);
}

function cameraValues(frame, interpolation) {
  return {
    distance: frame.distance,
    position: frame.position,
    rotation: frame.rotation,
    interpolation,
    fov: frame.fov,
    perspective: frame.perspective,
  };
}

function cameraInterpolationFromVmdFrame(frame) {
  if (typeof frame.interpolationHex !== "string" || !/^[0-9a-f]{48}$/iu.test(frame.interpolationHex)) {
    return null;
  }
  const bytes = Buffer.from(frame.interpolationHex, "hex");
  return Array.from({ length: 6 }, (_, index) => [...bytes.subarray(index * 4, index * 4 + 4)]);
}

function equalNestedArrays(left, right) {
  if (!Array.isArray(left) || !Array.isArray(right) || left.length !== right.length) {
    return false;
  }
  return left.every(
    (values, index) =>
      Array.isArray(values) &&
      Array.isArray(right[index]) &&
      values.length === right[index].length &&
      values.every((value, valueIndex) => value === right[index][valueIndex]),
  );
}

function namedFrameKey(frame) {
  return `${frame.name}\u0000${frame.frame}`;
}

function frameMap(frames, keyOf = namedFrameKey) {
  const map = new Map();
  for (const frame of frames) {
    const key = keyOf(frame);
    if (map.has(key)) {
      map.set(key, { duplicate: true, frame });
    }
    else {
      map.set(key, frame);
    }
  }
  return map;
}

function maxAbsDiff(left, right) {
  let max = 0;
  for (let index = 0; index < left.length; index += 1) {
    max = Math.max(max, Math.abs(left[index] - right[index]));
  }
  return max;
}
