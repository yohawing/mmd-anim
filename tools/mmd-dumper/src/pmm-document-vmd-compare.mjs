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
  const mismatchCount =
    unsupportedChannels.length + boneComparison.mismatches.length + morphComparison.mismatches.length;
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
  };
}

function unsupportedVmdChannels(vmd) {
  return [
    ["cameraFrames", vmd.counts.cameraFrames],
    ["lightFrames", vmd.counts.lightFrames],
    ["selfShadowFrames", vmd.counts.selfShadowFrames],
    ["propertyFrames", vmd.counts.propertyFrames],
  ]
    .filter(([, count]) => count > 0)
    .map(([name, count]) => ({ name, count }));
}

function compareNamedFrames({ kind, pmmFrames, vmdFrames, compareFrame }) {
  const pmmByKey = frameMap(pmmFrames);
  const vmdByKey = frameMap(vmdFrames);
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

function frameMap(frames) {
  const map = new Map();
  for (const frame of frames) {
    const key = `${frame.name}\u0000${frame.frame}`;
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
