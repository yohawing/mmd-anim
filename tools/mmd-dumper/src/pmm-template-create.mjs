import { readFile } from "node:fs/promises";
import { writePmmUnittestVmdBoneKeys } from "./pmm-fixture-motion.mjs";
import { readPmmModelSlotReport } from "./pmm-model-slots.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function createPmmFromTemplateDryRun(options = {}) {
  const template = requireString(options, "template");
  const vmdFile = requireString(options, "vmd");
  const modelSlot = requireInteger(options, "modelSlot");
  const [slotReport, vmd, templateBytes] = await Promise.all([
    readPmmModelSlotReport(template, { limit: options.limit ?? Number.POSITIVE_INFINITY }),
    readVmdInventory(vmdFile, { limit: options.limit ?? Number.POSITIVE_INFINITY }),
    readFile(template),
  ]);
  return planPmmTemplateCreation({
    template,
    vmdFile,
    slotReport,
    vmd,
    modelSlot,
    templateBytes,
  });
}

export async function writePmmFromTemplateProfile(options = {}) {
  const out = requireString(options, "out");
  if (options.requireVerified !== true) {
    throw new Error("create-pmm-from-template write mode requires requireVerified=true.");
  }
  const plan = await createPmmFromTemplateDryRun(options);
  if (!plan.okToWrite) {
    throw new Error(`PMM template input checks failed: ${plan.errors.join(" ")}`);
  }
  if (!plan.profile.profileWritable) {
    throw new Error(`PMM template is not writable with ${plan.profile.selected}: ${plan.profile.reasons.join(" ")}`);
  }
  const written = await writePmmUnittestVmdBoneKeys({
    template: requireString(options, "template"),
    vmd: requireString(options, "vmd"),
    out,
    boneName: plan.vmd.boneNameCounts[0]?.name,
    allowNonIdentityRotation: options.allowNonIdentityRotation === true,
    requireGeneratedMapping: true,
    limit: options.limit,
    oracle: options.oracle,
  });
  return {
    mode: "pmm-template-create-write",
    ok: true,
    plan,
    written,
  };
}

export function planPmmTemplateCreation(options = {}) {
  const slotReport = options.slotReport;
  const vmd = options.vmd;
  const modelSlot = requireInteger(options, "modelSlot");
  const targetSlot = slotReport.modelSlots.find((slot) => slot.slot === modelSlot);
  const errors = [];
  const warnings = [];

  if (!targetSlot) {
    errors.push(`Model slot ${modelSlot} does not exist.`);
  } else if (!targetSlot.readable) {
    errors.push(`Model slot ${modelSlot} PMX inventory is not readable: ${targetSlot.error}`);
  }

  const unsupportedChannels = unsupportedVmdChannels(vmd);
  if (unsupportedChannels.length > 0) {
    errors.push(`Unsupported VMD channels for PMM template creation: ${unsupportedChannels.map((entry) => `${entry.name}=${entry.count}`).join(", ")}.`);
  }

  const vmdBoneNames = unique(vmd.bones.map((frame) => frame.name));
  const targetBoneNames = new Set((targetSlot?.inventory?.bones ?? []).map((bone) => bone.name));
  const missingBones = vmdBoneNames.filter((name) => !targetBoneNames.has(name));
  if (missingBones.length > 0) {
    errors.push(`VMD contains bone names missing from model slot ${modelSlot}: ${missingBones.join(", ")}.`);
  }

  const ambiguousBones = findAmbiguousVmdBones(vmdBoneNames, slotReport.boneNameCollisions, modelSlot);
  if (ambiguousBones.length > 0) {
    warnings.push(
      `VMD bone names also exist in other model slots; keep --model-slot explicit: ${ambiguousBones.map((entry) => entry.name).join(", ")}.`,
    );
  }
  const profile = inspectTemplateWriteProfile({
    templateBytes: options.templateBytes,
    modelSlot,
    vmdBoneNames,
    errors,
  });

  return {
    mode: "pmm-template-create-dry-run",
    writable: profile.profileWritable && errors.length === 0,
    okToWrite: errors.length === 0,
    profile,
    templateFile: options.template,
    vmdFile: options.vmdFile,
    target: targetSlot
      ? {
          modelSlot,
          modelPath: targetSlot.path,
          modelFileName: targetSlot.fileName,
          modelName: targetSlot.inventory?.modelName,
          boneCount: targetSlot.inventory?.counts?.bones,
          morphCount: targetSlot.inventory?.counts?.morphs,
        }
      : { modelSlot },
    vmd: {
      modelName: vmd.modelName,
      maxFrame: vmd.maxFrame,
      counts: vmd.counts,
      boneNameCount: vmdBoneNames.length,
      boneNameCounts: vmd.boneNameCounts,
    },
    checks: {
      modelSlotExists: Boolean(targetSlot),
      modelInventoryReadable: Boolean(targetSlot?.readable),
      unsupportedChannels,
      missingBones,
      ambiguousBones,
    },
    errors,
    warnings,
    notes: [
      "This command is a dry-run plan only; it does not write a PMM.",
      "Template writing is allowed only when both input checks and a named write profile pass.",
    ],
  };
}

export function inspectTemplateWriteProfile(options = {}) {
  const reasons = [];
  const bytes = options.templateBytes;
  const unittest = bytes ? inspectUnittestOneBoneProfile(bytes) : { matches: false, reason: "template bytes were not provided" };
  if (!unittest.matches) {
    reasons.push(unittest.reason);
  }
  if (options.modelSlot !== 0) {
    reasons.push("unittest-one-bone-transform-v1 supports only model slot 0");
  }
  if ((options.vmdBoneNames?.length ?? 0) !== 1) {
    reasons.push(`unittest-one-bone-transform-v1 supports exactly one VMD bone name, got ${options.vmdBoneNames?.length ?? 0}`);
  }
  if ((options.errors?.length ?? 0) > 0) {
    reasons.push("input checks have errors");
  }
  return {
    selected: "unittest-one-bone-transform-v1",
    profileWritable: reasons.length === 0,
    reasons,
    template: unittest,
    note: "This is the only currently writable profile; general PMM templates remain dry-run only.",
  };
}

function inspectUnittestOneBoneProfile(bytes) {
  if (bytes.byteLength < 0x346) {
    return { matches: false, reason: `template is too small (${bytes.byteLength} bytes)` };
  }
  const header = bytes.subarray(0, 24).toString("latin1").replace(/\0+$/, "");
  const keyCount = bytes.readUInt16LE(0x1ce);
  const firstFrame = bytes.readUInt32LE(0x1d6);
  const firstKeyControl = bytes.subarray(0x1e2, 0x1e6).toString("hex");
  const matches = header === "Polygon Movie maker 0002" && keyCount >= 1 && firstFrame <= 0xffff && firstKeyControl === "14141414";
  return {
    matches,
    reason: matches ? undefined : `expected unittest one-bone key layout, got header=${JSON.stringify(header)}, keyCount=${keyCount}, firstFrame=${firstFrame}, control=${firstKeyControl}`,
    fields: {
      header,
      keyCount,
      firstFrame,
      firstKeyControl,
    },
  };
}

function unsupportedVmdChannels(vmd) {
  return [
    ["morphFrames", vmd.counts?.morphFrames ?? 0],
    ["cameraFrames", vmd.counts?.cameraFrames ?? 0],
    ["lightFrames", vmd.counts?.lightFrames ?? 0],
    ["selfShadowFrames", vmd.counts?.selfShadowFrames ?? 0],
    ["propertyFrames", vmd.counts?.propertyFrames ?? 0],
  ]
    .filter(([, count]) => count > 0)
    .map(([name, count]) => ({ name, count }));
}

function findAmbiguousVmdBones(vmdBoneNames, collisions, modelSlot) {
  const names = new Set(vmdBoneNames);
  return collisions
    .filter((collision) => names.has(collision.name))
    .filter((collision) => collision.entries.some((entry) => entry.slot === modelSlot))
    .map((collision) => ({
      name: collision.name,
      entries: collision.entries,
    }));
}

function unique(values) {
  return [...new Set(values)].sort((left, right) => left.localeCompare(right, "ja"));
}

function requireString(options, key) {
  if (!options[key]) {
    throw new Error(`Missing ${key}.`);
  }
  return options[key];
}

function requireInteger(options, key) {
  if (!Number.isInteger(options[key])) {
    throw new Error(`${key} must be an integer.`);
  }
  return options[key];
}
