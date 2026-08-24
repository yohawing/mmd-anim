import { access, readFile, writeFile } from "node:fs/promises";
import { basename } from "node:path";
import { analyzePmmVmdDiffClusters } from "./pmm-vmd-diff-clusters.mjs";
import { analyzePmmKeyCountDelta } from "./pmm-key-count-delta.mjs";
import { patchPmmFixtureMotion } from "./pmm-fixture-motion.mjs";
import { parsePmmManifest } from "./pmm-manifest.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function writePmmVmdDiffClusterPatch(options) {
  const baseFile = requireString(options, "base");
  const donorBaseFile = requireString(options, "donorBase");
  const donorVariantFile = requireString(options, "donorVariant");
  const donorVmdFile = requireString(options, "donorVmd");
  const targetVmdFile = requireString(options, "targetVmd");
  const outFile = requireString(options, "out");
  const [baseBytes, donorBaseBytes, donorVariantBytes, donorVmd, targetVmd] = await Promise.all([
    readFile(baseFile),
    readFile(donorBaseFile),
    readFile(donorVariantFile),
    readVmdInventory(donorVmdFile, { limit: Number.POSITIVE_INFINITY }),
    readVmdInventory(targetVmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  const result = patchPmmVmdDiffClusterProfile(baseBytes, donorBaseBytes, donorVariantBytes, donorVmd, targetVmd, options);
  await writeFile(outFile, result.bytes);
  return {
    ...withoutBytes(result),
    baseFile,
    donorBaseFile,
    donorVariantFile,
    donorVmdFile,
    targetVmdFile,
    outFile,
  };
}

export async function writePmmVmdKeyCountDeltaPatch(options) {
  const baseFile = requireString(options, "base");
  const smallVariantFile = requireString(options, "smallVariant");
  const largeVariantFile = requireString(options, "largeVariant");
  const smallVmdFile = requireString(options, "smallVmd");
  const largeVmdFile = requireString(options, "largeVmd");
  const targetVmdFile = requireString(options, "targetVmd");
  const outFile = requireString(options, "out");
  const [baseBytes, smallVariantBytes, largeVariantBytes, smallVmd, largeVmd, targetVmd] = await Promise.all([
    readFile(baseFile),
    readFile(smallVariantFile),
    readFile(largeVariantFile),
    readVmdInventory(smallVmdFile, { limit: Number.POSITIVE_INFINITY }),
    readVmdInventory(largeVmdFile, { limit: Number.POSITIVE_INFINITY }),
    readVmdInventory(targetVmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  const result = patchPmmVmdKeyCountDeltaProfile(baseBytes, smallVariantBytes, largeVariantBytes, smallVmd, largeVmd, targetVmd, options);
  await writeFile(outFile, result.bytes);
  return {
    ...withoutBytes(result),
    baseFile,
    smallVariantFile,
    largeVariantFile,
    smallVmdFile,
    largeVmdFile,
    targetVmdFile,
    outFile,
  };
}

export async function writePmmVmdPatchFromProfileRegistry(options) {
  const plan = await readPmmVmdPatchRegistryPlan(options);
  const selected = plan.candidates.find((candidate) => candidate.ok);
  if (!selected) {
    return {
      ok: false,
      mode: "pmm-vmd-patch-from-profile-registry",
      plan,
      reasons: ["No compatible PMM patch profile was found."],
    };
  }
  const args = selected.command.args;
  const result = await writePmmVmdDiffClusterPatch({
    ...options,
    base: args.base,
    donorBase: args.donorBase,
    donorVariant: args.donorVariant,
    donorVmd: args.donorVmd,
    targetVmd: args.targetVmd,
    out: args.out,
    donorSlot: args.donorSlot,
    targetSlot: args.targetSlot,
  });
  return {
    ok: true,
    mode: "pmm-vmd-patch-from-profile-registry",
    selectedProfile: selected,
    patch: result,
  };
}

export async function writePmmVmdKeyCountDeltaPatchFromProfileRegistry(options) {
  const plan = await readPmmVmdKeyCountDeltaPatchRegistryPlan(options);
  const selected = plan.candidates.find((candidate) => candidate.ok);
  if (!selected) {
    return {
      ok: false,
      mode: "pmm-vmd-key-count-delta-patch-from-profile-registry",
      plan,
      reasons: ["No compatible PMM key-count delta profile was found."],
    };
  }
  const args = selected.command.args;
  const result = await writePmmVmdKeyCountDeltaPatch({
    ...options,
    base: args.base,
    smallVariant: args.smallVariant,
    largeVariant: args.largeVariant,
    smallVmd: args.smallVmd,
    largeVmd: args.largeVmd,
    targetVmd: args.targetVmd,
    out: args.out,
    targetSlot: args.targetSlot,
  });
  return {
    ok: true,
    mode: "pmm-vmd-key-count-delta-patch-from-profile-registry",
    selectedProfile: selected,
    patch: result,
  };
}

export async function writePmmVmdPatchFromAnyProfileRegistry(options) {
  const plan = await readPmmVmdAnyPatchRegistryPlan(options);
  const selected = plan.candidates.find((candidate) => candidate.ok);
  if (!selected) {
    return {
      ok: false,
      mode: "pmm-vmd-patch-from-any-profile-registry",
      plan,
      reasons: ["No compatible PMM patch profile was found in any registry."],
    };
  }
  const args = selected.command.args;
  const result =
    selected.command.script === "patch-pmm-vmd-key-count-delta"
      ? await writePmmVmdKeyCountDeltaPatch({
          ...options,
          base: args.base,
          smallVariant: args.smallVariant,
          largeVariant: args.largeVariant,
          smallVmd: args.smallVmd,
          largeVmd: args.largeVmd,
          targetVmd: args.targetVmd,
          out: args.out,
          targetSlot: args.targetSlot,
        })
      : await writePmmVmdDiffClusterPatch({
          ...options,
          base: args.base,
          donorBase: args.donorBase,
          donorVariant: args.donorVariant,
          donorVmd: args.donorVmd,
          targetVmd: args.targetVmd,
          out: args.out,
          donorSlot: args.donorSlot,
          targetSlot: args.targetSlot,
        });
  return {
    ok: true,
    mode: "pmm-vmd-patch-from-any-profile-registry",
    selectedProfile: selected,
    patch: result,
  };
}

export async function readPmmVmdPatchRegistryPlan(options) {
  const registryFile = requireString(options, "registry");
  const targetVmdFile = requireString(options, "targetVmd");
  const [registryText, targetVmd] = await Promise.all([
    readFile(registryFile, "utf8"),
    readVmdInventory(targetVmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  const registry = JSON.parse(registryText);
  return planPmmVmdPatchFromProfileRegistry(registry, targetVmd, {
    ...options,
    registryFile,
    targetVmdFile,
  });
}

export async function readPmmVmdKeyCountDeltaPatchRegistryPlan(options) {
  const registryFile = requireString(options, "registry");
  const targetVmdFile = requireString(options, "targetVmd");
  const [registryText, targetVmd] = await Promise.all([
    readFile(registryFile, "utf8"),
    readVmdInventory(targetVmdFile, { limit: Number.POSITIVE_INFINITY }),
  ]);
  const registry = JSON.parse(registryText);
  return planPmmVmdKeyCountDeltaPatchFromProfileRegistry(registry, targetVmd, {
    ...options,
    registryFile,
    targetVmdFile,
  });
}

export async function readPmmVmdAnyPatchRegistryPlan(options) {
  const registryFiles = requireArray(options, "registries");
  const targetVmdFile = requireString(options, "targetVmd");
  const targetVmd = await readVmdInventory(targetVmdFile, { limit: Number.POSITIVE_INFINITY });
  const patchOptions = await resolvePatchTargetOptions({
    ...options,
    targetVmdFile,
  });
  const plans = [];
  for (const [registryIndex, registryFile] of registryFiles.entries()) {
    const registry = JSON.parse(await readFile(registryFile, "utf8"));
    plans.push(
      await planPmmVmdPatchFromProfileRegistry(registry, targetVmd, {
        ...patchOptions,
        registryFile,
        registryIndex,
        targetVmdFile,
      }),
    );
    plans.push(
      await planPmmVmdKeyCountDeltaPatchFromProfileRegistry(registry, targetVmd, {
        ...patchOptions,
        registryFile,
        registryIndex,
        targetVmdFile,
      }),
    );
  }
  return summarizePmmVmdAnyPatchRegistryPlan(plans, patchOptions);
}

export async function readPmmVmdPatchCompatibility(options) {
  const registryFiles = requireArray(options, "registries");
  const targetVmdFile = requireString(options, "targetVmd");
  const targetVmd = await readVmdInventory(targetVmdFile, { limit: Number.POSITIVE_INFINITY });
  const patchOptions = await resolvePatchTargetOptions({
    ...options,
    targetVmdFile,
  });
  const unsupportedChannels = unsupportedVmdChannels(targetVmd);
  if (unsupportedChannels.length > 0) {
    return {
      ok: false,
      mode: "pmm-vmd-patch-compatibility",
      baseFile: patchOptions.base,
      targetVmdFile,
      targetPmx: patchOptions.targetPmx,
      targetModelSlot: patchOptions.targetModelSlot,
      registryCount: registryFiles.length,
      planCount: 0,
      compatibleProfileCount: 0,
      incompatibleProfileCount: 0,
      target: summarizeVmdForRegistry(targetVmd),
      unsupportedChannels,
      nextRequiredFixtures: unsupportedChannels.map((channel) => requiredFixtureForUnsupportedChannel(channel.name)),
      reasons: [`Target VMD contains unsupported channels: ${unsupportedChannels.map((entry) => `${entry.name}=${entry.count}`).join(", ")}.`],
    };
  }
  const plan = await readPmmVmdAnyPatchRegistryPlan({
    ...patchOptions,
    out: options.out ?? "<check-only>",
  });
  return summarizePmmVmdPatchCompatibility(plan, options);
}

export function summarizePmmVmdAnyPatchRegistryPlan(plans, options = {}) {
  const candidates = [];
  for (const [planIndex, plan] of plans.entries()) {
    for (const candidate of plan.candidates ?? []) {
      candidates.push({
        ...candidate,
        planIndex,
        registryFile: plan.registryFile,
        planMode: plan.mode,
      });
    }
  }
  const sorted = candidates.toSorted((left, right) => {
    if (right.score !== left.score) {
      return right.score - left.score;
    }
    if ((left.planIndex ?? 0) !== (right.planIndex ?? 0)) {
      return (left.planIndex ?? 0) - (right.planIndex ?? 0);
    }
    return left.index - right.index;
  });
  const limit = options.limit ?? 32;
  return {
    ok: sorted.some((candidate) => candidate.ok),
    mode: "pmm-vmd-any-patch-registry-plan",
    baseFile: options.base,
    targetVmdFile: options.targetVmd,
    outFile: options.out,
    targetPmx: options.targetPmx,
    targetModelSlot: options.targetModelSlot,
    registryCount: new Set(plans.map((plan) => plan.registryFile)).size,
    planCount: plans.length,
    candidates: sorted.slice(0, limit),
    truncated: sorted.length > limit,
  };
}

export function summarizePmmVmdPatchCompatibility(plan, options = {}) {
  const compatibleCandidates = (plan.candidates ?? []).filter((candidate) => candidate.ok);
  const incompatibleCandidates = (plan.candidates ?? []).filter((candidate) => !candidate.ok && !isPlannerKindMismatch(candidate));
  const selected = compatibleCandidates[0];
  const limit = options.limit ?? 32;
  return {
    ok: compatibleCandidates.length > 0,
    mode: "pmm-vmd-patch-compatibility",
    baseFile: plan.baseFile,
    targetVmdFile: plan.targetVmdFile,
    targetPmx: plan.targetPmx,
    targetModelSlot: plan.targetModelSlot,
    registryCount: plan.registryCount,
    planCount: plan.planCount,
    compatibleProfileCount: compatibleCandidates.length,
    incompatibleProfileCount: incompatibleCandidates.length,
    selectedProfile: selected ? summarizePatchCandidateForCompatibility(selected) : undefined,
    compatibleProfiles: compatibleCandidates.slice(0, limit).map(summarizePatchCandidateForCompatibility),
    incompatibleProfiles: incompatibleCandidates.slice(0, limit).map(summarizePatchCandidateForCompatibility),
    truncated: compatibleCandidates.length + incompatibleCandidates.length > limit,
  };
}

function isPlannerKindMismatch(candidate) {
  const reasons = candidate.reasons ?? [];
  return reasons.length === 1 && /^Registry entry kind .+ is not (same-shape|key-count-delta)\.$/.test(reasons[0]);
}

async function resolvePatchTargetOptions(options) {
  if (!options.targetPmx) {
    return options;
  }
  const baseFile = requireString(options, "base");
  const manifest = parsePmmManifest(await readFile(baseFile));
  const matches = manifest.modelSlots.filter((slot) => modelSlotMatchesTargetPmx(slot, options.targetPmx));
  if (options.targetSlot !== undefined) {
    const explicitSlot = manifest.modelSlots.find((slot) => slot.slot === options.targetSlot);
    if (!explicitSlot) {
      throw new Error(`Requested target slot ${options.targetSlot} does not exist in base PMM.`);
    }
    if (!modelSlotMatchesTargetPmx(explicitSlot, options.targetPmx)) {
      throw new Error(`Requested target PMX ${options.targetPmx} does not match target slot ${options.targetSlot} (${explicitSlot.path}).`);
    }
    return {
      ...options,
      targetModelSlot: summarizeTargetModelSlot(explicitSlot),
    };
  }
  if (matches.length === 0) {
    throw new Error(`Target PMX ${options.targetPmx} was not found in base PMM model slots.`);
  }
  if (matches.length > 1) {
    const slots = matches.map((slot) => `${slot.slot}:${slot.path}`).join(", ");
    throw new Error(`Target PMX ${options.targetPmx} is ambiguous in base PMM model slots: ${slots}. Pass --target-slot.`);
  }
  return {
    ...options,
    targetSlot: matches[0].slot,
    targetModelSlot: summarizeTargetModelSlot(matches[0]),
  };
}

function modelSlotMatchesTargetPmx(slot, targetPmx) {
  const slotPath = normalizeModelPathForMatch(slot.path);
  const targetPath = normalizeModelPathForMatch(targetPmx);
  const slotFileName = normalizeModelPathForMatch(slot.fileName);
  const targetFileName = normalizeModelPathForMatch(basename(targetPmx));
  return slotPath === targetPath || slotPath.endsWith(`/${targetPath}`) || slotFileName === targetFileName;
}

function normalizeModelPathForMatch(value = "") {
  return value.replaceAll("\\", "/").replace(/\/+/g, "/").toLowerCase();
}

function summarizeTargetModelSlot(slot) {
  return {
    slot: slot.slot,
    path: slot.path,
    fileName: slot.fileName,
    offset: slot.offset,
    offsetHex: slot.offsetHex,
  };
}

export async function readPmmPatchProfileRegistryInspection(options) {
  const registryFile = requireString(options, "registry");
  const registryText = await readFile(registryFile, "utf8");
  const registry = JSON.parse(registryText);
  return inspectPmmPatchProfileRegistry(registry, {
    ...options,
    registryFile,
  });
}

export async function readPmmPatchProfileRegistryInventory(options) {
  const registryFiles = requireArray(options, "registries");
  const inspections = [];
  for (const registryFile of registryFiles) {
    inspections.push(await readPmmPatchProfileRegistryInspection({ registry: registryFile, limit: options.entryLimit }));
  }
  return summarizePmmPatchProfileRegistryInventory(inspections, options);
}

export async function readUsablePmmPatchProfileRegistry(options) {
  const registryFiles = requireArray(options, "registries");
  const usableProfiles = [];
  const omittedProfiles = [];
  let sourceProfileCount = 0;
  const kindCounts = {};
  const usableKindCounts = {};
  for (const [registryIndex, registryFile] of registryFiles.entries()) {
    const registry = JSON.parse(await readFile(registryFile, "utf8"));
    const entries = normalizeProfileRegistry(registry);
    sourceProfileCount += entries.length;
    for (const [index, entry] of entries.entries()) {
      const report = await inspectPatchRegistryEntry(index, entry);
      kindCounts[report.kind] = (kindCounts[report.kind] ?? 0) + 1;
      if (report.ok) {
        usableKindCounts[entry.kind] = (usableKindCounts[entry.kind] ?? 0) + 1;
        usableProfiles.push({
          id: entry.id,
          kind: entry.kind,
          source: entry.source,
          profile: entry.profile,
        });
      } else {
        omittedProfiles.push(
          summarizePatchRegistryInspectionEntry(
            { registryFile },
            {
              ...report,
              registryIndex,
            },
          ),
        );
      }
    }
  }
  const omittedLimit = options.omittedLimit ?? options.limit ?? 64;
  const registry = {
    kind: "pmm-patch-profile-registry",
    version: 1,
    generatedAt: new Date().toISOString(),
    sourceRegistries: registryFiles,
    profileCount: usableProfiles.length,
    profiles: usableProfiles,
  };
  return {
    ok: usableProfiles.length > 0,
    mode: "usable-pmm-patch-profile-registry",
    registryCount: registryFiles.length,
    sourceProfileCount,
    profileCount: usableProfiles.length,
    omittedProfileCount: omittedProfiles.length,
    kindCounts,
    usableKindCounts,
    omittedProfiles: omittedProfiles.slice(0, omittedLimit),
    omittedTruncated: omittedProfiles.length > omittedLimit,
    registry,
  };
}

export async function writeUsablePmmPatchProfileRegistry(options) {
  const outFile = requireString(options, "out");
  const report = await readUsablePmmPatchProfileRegistry(options);
  await writeFile(outFile, `${JSON.stringify(report.registry, null, 2)}\n`);
  const { registry, ...summary } = report;
  return {
    ...summary,
    outFile,
  };
}

export function summarizePmmPatchProfileRegistryInventory(inspections, options = {}) {
  const profiles = [];
  const kindCounts = {};
  let usableProfileCount = 0;
  for (const inspection of inspections) {
    for (const entry of inspection.entries ?? []) {
      const profile = summarizePatchRegistryInspectionEntry(inspection, entry);
      profiles.push(profile);
      kindCounts[profile.kind] = (kindCounts[profile.kind] ?? 0) + 1;
      if (profile.ok) {
        usableProfileCount += 1;
      }
    }
  }
  const limit = options.limit ?? 128;
  return {
    ok: profiles.every((profile) => profile.ok),
    mode: "pmm-patch-profile-registry-inventory",
    registryCount: inspections.length,
    profileCount: profiles.length,
    usableProfileCount,
    kindCounts,
    profiles: profiles.slice(0, limit),
    truncated: profiles.length > limit,
  };
}

export async function inspectPmmPatchProfileRegistry(registry, options = {}) {
  const entries = normalizeProfileRegistry(registry);
  const reports = [];
  for (const [index, entry] of entries.entries()) {
    reports.push(await inspectPatchRegistryEntry(index, entry));
  }
  const kindCounts = {};
  for (const report of reports) {
    kindCounts[report.kind] = (kindCounts[report.kind] ?? 0) + 1;
  }
  const limit = options.limit ?? 64;
  return {
    ok: reports.every((report) => report.ok),
    mode: "pmm-patch-profile-registry-inspection",
    registryFile: options.registryFile,
    profileCount: entries.length,
    usableProfileCount: reports.filter((report) => report.ok).length,
    kindCounts,
    entries: reports.slice(0, limit),
    truncated: reports.length > limit,
  };
}

export async function planPmmVmdPatchFromProfileRegistry(registry, targetVmd, options = {}) {
  validateSupportedVmdChannels(targetVmd, "target VMD");
  const entries = normalizeProfileRegistry(registry);
  const candidates = [];
  for (const [index, entry] of entries.entries()) {
    candidates.push(await planRegistryEntryPatch(index, entry, targetVmd, options));
  }
  const sorted = candidates.toSorted((left, right) => {
    if (right.score !== left.score) {
      return right.score - left.score;
    }
    return left.index - right.index;
  });
  return {
    ok: sorted.some((candidate) => candidate.ok),
    mode: "pmm-vmd-patch-registry-plan",
    registryFile: options.registryFile,
    baseFile: options.base,
    targetVmdFile: options.targetVmdFile,
    outFile: options.out,
    profileCount: entries.length,
    candidates: sorted.slice(0, options.limit ?? 32),
    truncated: sorted.length > (options.limit ?? 32),
  };
}

export async function planPmmVmdKeyCountDeltaPatchFromProfileRegistry(registry, targetVmd, options = {}) {
  validateSupportedVmdChannels(targetVmd, "target VMD");
  const entries = normalizeProfileRegistry(registry);
  const candidates = [];
  for (const [index, entry] of entries.entries()) {
    candidates.push(await planDeltaRegistryEntryPatch(index, entry, targetVmd, options));
  }
  const sorted = candidates.toSorted((left, right) => {
    if (right.score !== left.score) {
      return right.score - left.score;
    }
    return left.index - right.index;
  });
  return {
    ok: sorted.some((candidate) => candidate.ok),
    mode: "pmm-vmd-key-count-delta-patch-registry-plan",
    registryFile: options.registryFile,
    baseFile: options.base,
    targetVmdFile: options.targetVmdFile,
    outFile: options.out,
    profileCount: entries.length,
    candidates: sorted.slice(0, options.limit ?? 32),
    truncated: sorted.length > (options.limit ?? 32),
  };
}

export function patchPmmVmdKeyCountDeltaProfile(baseBytes, smallVariantBytes, largeVariantBytes, smallVmd, largeVmd, targetVmd, options = {}) {
  validateSupportedVmdChannels(smallVmd, "small VMD");
  validateSupportedVmdChannels(largeVmd, "large VMD");
  validateSupportedVmdChannels(targetVmd, "target VMD");
  const modelSlots = options.modelSlots ?? readOptionalModelSlots(baseBytes);
  const deltaReport = analyzePmmKeyCountDelta(baseBytes, smallVariantBytes, largeVariantBytes, smallVmd, largeVmd, {
    ...options,
    modelSlots,
    candidateLimit: 1,
  });
  validateDeltaReport(deltaReport);
  const largeKeys = normalizeVmdBoneKeys(largeVmd);
  const targetKeys = normalizeVmdBoneKeys(targetVmd);
  assertSameProfileShape(largeKeys, targetKeys);

  const largeReport = analyzePmmVmdDiffClusters(baseBytes, largeVariantBytes, largeVmd, {
    matchLimit: options.matchLimit ?? 1024,
    candidateLimit: 1,
    diffLimit: options.diffLimit ?? 128,
    modelSlots,
  });
  const positionProfile = largeReport.positionKeyBlockProfile;
  const frameSequenceProfile = largeReport.frameSequenceBlockProfile;
  const transformProfile = largeReport.transformKeyBlockProfile;
  assertProfileSlot(positionProfile.verified ? positionProfile : frameSequenceProfile, options.targetSlot, "target");
  if (!frameSequenceProfile.verified) {
    throw new Error(`Large PMM/VMD frame sequence profile is not verified: ${frameSequenceProfile.reasons.join(" ")}`);
  }
  const needsPositionRewrite = targetKeys.some((key) => !isAllZeroVector(key.position));
  const needsRotationRewrite = [...largeKeys, ...targetKeys].some((key) => !isIdentityRotation(key.rotation));
  if (needsPositionRewrite && !positionProfile.verified) {
    throw new Error(`Large PMM/VMD position profile is not verified: ${positionProfile.reasons.join(" ")}`);
  }
  if (needsRotationRewrite && !transformProfile.verified) {
    throw new Error(`Large PMM/VMD transform profile is not verified: ${transformProfile.reasons.join(" ")}`);
  }

  const transplanted = patchPmmFixtureMotion(smallVariantBytes, smallVariantBytes, largeVariantBytes, options);
  const patched = Buffer.from(transplanted.bytes);
  const scalarRewrites = rewriteDeltaScalars(patched, deltaReport, targetVmd);
  const rewrites = rewriteVmdKeysIntoProfile(patched, targetKeys, {
    frameSequenceProfile,
    positionProfile,
    transformProfile,
    rewritePositions: positionProfile.verified,
    rewriteRotations: needsRotationRewrite,
  });

  const verification = analyzePmmVmdDiffClusters(baseBytes, patched, targetVmd, {
    matchLimit: options.matchLimit ?? 1024,
    candidateLimit: 1,
    diffLimit: options.diffLimit ?? 128,
    modelSlots,
  });
  assertProfileSlot(
    verification.positionKeyBlockProfile.verified ? verification.positionKeyBlockProfile : verification.frameSequenceBlockProfile,
    options.targetSlot,
    "target",
  );
  if (options.requireVerified !== false) {
    if (!verification.frameSequenceBlockProfile.verified) {
      throw new Error(`Patched PMM/VMD frame sequence verification failed: ${verification.frameSequenceBlockProfile.reasons.join(" ")}`);
    }
    if (needsPositionRewrite && !verification.positionKeyBlockProfile.verified) {
      throw new Error(`Patched PMM/VMD position verification failed: ${verification.positionKeyBlockProfile.reasons.join(" ")}`);
    }
    if (needsRotationRewrite && !verification.transformKeyBlockProfile.verified) {
      throw new Error(`Patched PMM/VMD transform verification failed: ${verification.transformKeyBlockProfile.reasons.join(" ")}`);
    }
  }

  return {
    mode: "pmm-vmd-key-count-delta-patch",
    warning:
      "This is an experimental fixture-profile PMM patcher: it applies a verified small->large key-count delta and rewrites the resulting large-shape key records. It is not a general PMM writer.",
    byteLength: patched.byteLength,
    byteLengthDeltaFromSmall: patched.byteLength - smallVariantBytes.byteLength,
    byteLengthDeltaFromBase: patched.byteLength - baseBytes.byteLength,
    deltaSummary: deltaReport.summary,
    scalarRewriteCount: scalarRewrites.length,
    scalarRewrites,
    largeFrameSequenceProfile: frameSequenceProfile,
    largePositionProfile: positionProfile,
    largeTransformProfile: transformProfile,
    transplanted: withoutBytes(transplanted),
    rewriteCount: rewrites.length,
    rewrites,
    verification: {
      coverage: verification.coverage,
      positionKeyBlockProfile: verification.positionKeyBlockProfile,
      frameSequenceBlockProfile: verification.frameSequenceBlockProfile,
      transformKeyBlockProfile: verification.transformKeyBlockProfile,
    },
    bytes: patched,
  };
}

export function patchPmmVmdDiffClusterProfile(baseBytes, donorBaseBytes, donorVariantBytes, donorVmd, targetVmd, options = {}) {
  validateSupportedVmdChannels(targetVmd, "target VMD");
  validateSupportedVmdChannels(donorVmd, "donor VMD");
  const donorModelSlots = options.donorModelSlots ?? readOptionalModelSlots(donorBaseBytes);
  const targetModelSlots = options.modelSlots ?? readOptionalModelSlots(baseBytes);
  const donorReport = analyzePmmVmdDiffClusters(donorBaseBytes, donorVariantBytes, donorVmd, {
    matchLimit: options.matchLimit ?? 1024,
    candidateLimit: 1,
    diffLimit: options.diffLimit ?? 128,
    modelSlots: donorModelSlots,
  });
  const profile = donorReport.positionKeyBlockProfile;
  const frameSequenceProfile = donorReport.frameSequenceBlockProfile;
  assertProfileSlot(profile.verified ? profile : frameSequenceProfile, options.donorSlot, "donor");
  const donorKeys = normalizeVmdBoneKeys(donorVmd);
  const targetKeys = normalizeVmdBoneKeys(targetVmd);
  assertSameProfileShape(donorKeys, targetKeys);
  const needsRotationRewrite = [...donorKeys, ...targetKeys].some((key) => !isIdentityRotation(key.rotation));
  const needsPositionRewrite = needsPositionRewriteForSameShapeKeys(donorKeys, targetKeys);
  if (needsPositionRewrite && !profile.verified) {
    throw new Error(`Donor PMM/VMD position profile is not verified: ${profile.reasons.join(" ")}`);
  }
  if (!profile.verified && !frameSequenceProfile.verified) {
    throw new Error(`Donor PMM/VMD frame sequence profile is not verified: ${frameSequenceProfile.reasons.join(" ")}`);
  }
  if (needsRotationRewrite && !donorReport.transformKeyBlockProfile.verified) {
    throw new Error(`Donor PMM/VMD transform profile is not verified: ${donorReport.transformKeyBlockProfile.reasons.join(" ")}`);
  }

  const transplanted = patchPmmFixtureMotion(baseBytes, donorBaseBytes, donorVariantBytes, options);
  const patched = Buffer.from(transplanted.bytes);
  const rewrites = [];
  for (const [index, targetKey] of targetKeys.entries()) {
    const donorFrame = donorReport.boneFrames[index];
    const candidate = donorFrame?.candidates?.[0];
    const sequenceRecord = frameSequenceProfile.records?.[index];
    const recordStart = profile.verified ? candidate?.estimatedRecordStart : sequenceRecord?.recordStart;
    if (!Number.isInteger(recordStart)) {
      throw new Error(`Missing donor PMM key record for ${targetKey.name} frame index ${index}.`);
    }
    const frameOffsetInRecord = profile.verified ? profile.frameOffsetInRecord : frameSequenceProfile.frameOffsetInRecord;
    const frameOffset = recordStart + frameOffsetInRecord;
    patched.writeUInt32LE(targetKey.frame, frameOffset);
    const positionOffset = profile.verified ? recordStart + profile.positionOffsetInRecord : undefined;
    if (positionOffset !== undefined) {
      patched.writeFloatLE(targetKey.position[0], positionOffset);
      patched.writeFloatLE(targetKey.position[1], positionOffset + 4);
      patched.writeFloatLE(targetKey.position[2], positionOffset + 8);
    }
    const rotationOffset = needsRotationRewrite
      ? recordStart + donorReport.transformKeyBlockProfile.rotationOffsetInRecord
      : undefined;
    if (rotationOffset !== undefined) {
      patched.writeFloatLE(targetKey.rotation[0], rotationOffset);
      patched.writeFloatLE(targetKey.rotation[1], rotationOffset + 4);
      patched.writeFloatLE(targetKey.rotation[2], rotationOffset + 8);
      patched.writeFloatLE(targetKey.rotation[3], rotationOffset + 12);
    }
    rewrites.push({
      index,
      name: targetKey.name,
      frame: targetKey.frame,
      frameOffset,
      frameOffsetHex: hex(frameOffset),
      position: positionOffset === undefined ? undefined : targetKey.position,
      positionOffset,
      positionOffsetHex: positionOffset === undefined ? undefined : hex(positionOffset),
      rotation: rotationOffset === undefined ? undefined : targetKey.rotation,
      rotationOffset,
      rotationOffsetHex: rotationOffset === undefined ? undefined : hex(rotationOffset),
    });
  }

  const verification = analyzePmmVmdDiffClusters(baseBytes, patched, targetVmd, {
    matchLimit: options.matchLimit ?? 1024,
    candidateLimit: 1,
    diffLimit: options.diffLimit ?? 128,
    modelSlots: targetModelSlots,
  });
  assertProfileSlot(
    verification.positionKeyBlockProfile.verified ? verification.positionKeyBlockProfile : verification.frameSequenceBlockProfile,
    options.targetSlot,
    "target",
  );
  if (options.requireVerified !== false) {
    if (!verification.frameSequenceBlockProfile.verified) {
      throw new Error(`Patched PMM/VMD frame sequence verification failed: ${verification.frameSequenceBlockProfile.reasons.join(" ")}`);
    }
    if (needsPositionRewrite && !verification.positionKeyBlockProfile.verified) {
      throw new Error(`Patched PMM/VMD position verification failed: ${verification.positionKeyBlockProfile.reasons.join(" ")}`);
    }
    if (needsRotationRewrite && !verification.transformKeyBlockProfile.verified) {
      throw new Error(`Patched PMM/VMD transform verification failed: ${verification.transformKeyBlockProfile.reasons.join(" ")}`);
    }
  }

  return {
    mode: "pmm-vmd-diff-cluster-patch",
    warning:
      "This is a fixture-profile PMM patcher: it transplants a verified donor key block and rewrites same-shape bone position/rotation VMD keys. It is not a general PMM writer.",
    byteLength: patched.byteLength,
    byteLengthDelta: patched.byteLength - baseBytes.byteLength,
    donorProfile: profile,
    donorFrameSequenceProfile: frameSequenceProfile,
    donorTransformProfile: donorReport.transformKeyBlockProfile,
    transplanted: withoutBytes(transplanted),
    rewriteCount: rewrites.length,
    rewrites,
    verification: {
      coverage: verification.coverage,
      positionKeyBlockProfile: verification.positionKeyBlockProfile,
      transformKeyBlockProfile: verification.transformKeyBlockProfile,
    },
    bytes: patched,
  };
}

function assertProfileSlot(profile, expectedSlot, label) {
  if (expectedSlot === undefined) {
    return;
  }
  const actualSlot = profile?.modelSlotContext?.slot;
  if (actualSlot !== expectedSlot) {
    const actual = actualSlot === undefined ? "unknown" : actualSlot;
    throw new Error(`${label} PMM motion block slot mismatch: expected slot ${expectedSlot}, got ${actual}.`);
  }
}

async function planRegistryEntryPatch(index, entry, targetVmd, options) {
  const reasons = [];
  if (entry.kind !== "same-shape") {
    reasons.push(`Registry entry kind ${entry.kind} is not same-shape.`);
    return makeRegistryKindMismatchCandidate(index, entry, targetVmd, options, "patch-pmm-vmd-diff-cluster", reasons);
  }
  const profile = entry.profile ?? {};
  const source = entry.source ?? {};
  const donorBase = source.base;
  const donorVariant = source.variant;
  const donorVmdFile = source.vmd;
  if (!donorBase) {
    reasons.push("Registry entry source.base is required for same-shape PMM patching.");
  }
  if (!donorVariant) {
    reasons.push("Registry entry source.variant is required for same-shape PMM patching.");
  }
  if (!donorVmdFile) {
    reasons.push("Registry entry source.vmd is required for same-shape PMM patching.");
  }
  if (!options.base) {
    reasons.push("Target base PMM --base is required to produce a patch command.");
  }
  if (!options.out) {
    reasons.push("Output PMM --out is required to produce a patch command.");
  }
  for (const [label, file] of [
    ["source.base", donorBase],
    ["source.variant", donorVariant],
    ["source.vmd", donorVmdFile],
    ["base", options.base],
  ]) {
    if (file) {
      const exists = await fileExists(file);
      if (!exists) {
        reasons.push(`${label} file does not exist: ${file}.`);
      }
    }
  }
  const profileSlot = profile?.modelSlotContext?.slot;
  const donorSlot = options.donorSlot ?? profileSlot;
  if (options.donorSlot !== undefined && profileSlot !== undefined && options.donorSlot !== profileSlot) {
    reasons.push(`Requested donor slot ${options.donorSlot} does not match profile slot ${profileSlot}.`);
  }
  let donorVmd;
  if (donorVmdFile && (await fileExists(donorVmdFile))) {
    try {
      donorVmd = await readVmdInventory(donorVmdFile, { limit: Number.POSITIVE_INFINITY });
      validateSupportedVmdChannels(donorVmd, "donor VMD");
      assertSameProfileShape(normalizeVmdBoneKeys(donorVmd), normalizeVmdBoneKeys(targetVmd));
    } catch (error) {
      reasons.push(error.message);
    }
  }
  const command = {
    script: "patch-pmm-vmd-diff-cluster",
    args: {
      base: options.base,
      donorBase,
      donorVariant,
      donorVmd: donorVmdFile,
      targetVmd: options.targetVmdFile,
      out: options.out,
      donorSlot,
      targetSlot: options.targetSlot,
    },
  };
  const ok = reasons.length === 0;
  return {
    index,
    id: entry.id,
    kind: entry.kind,
    ok,
    score: (ok ? 100 : 0) + (profile?.verified ? 5 : 0) + (profileSlot !== undefined ? 5 : 0),
    source,
    profile: {
      verified: Boolean(profile?.verified),
      recordCount: profile?.recordCount,
      recordByteLength: profile?.recordByteLength,
      modelSlotContext: profile?.modelSlotContext,
    },
    target: {
      boneFrameCount: targetVmd.counts?.boneFrames ?? targetVmd.bones?.length ?? 0,
      maxFrame: targetVmd.maxFrame,
    },
    donor: donorVmd
      ? {
          boneFrameCount: donorVmd.counts?.boneFrames ?? donorVmd.bones?.length ?? 0,
          maxFrame: donorVmd.maxFrame,
        }
      : undefined,
    command,
    reasons,
  };
}

async function planDeltaRegistryEntryPatch(index, entry, targetVmd, options) {
  const reasons = [];
  if (entry.kind !== "key-count-delta") {
    reasons.push(`Registry entry kind ${entry.kind} is not key-count-delta.`);
    return makeRegistryKindMismatchCandidate(index, entry, targetVmd, options, "patch-pmm-vmd-key-count-delta", reasons);
  }
  const profile = entry.profile ?? {};
  const source = entry.source ?? {};
  const smallVariant = source.smallVariant;
  const largeVariant = source.largeVariant;
  const smallVmdFile = source.smallVmd;
  const largeVmdFile = source.largeVmd;
  if (!smallVariant) {
    reasons.push("Registry entry source.smallVariant is required for key-count delta PMM patching.");
  }
  if (!largeVariant) {
    reasons.push("Registry entry source.largeVariant is required for key-count delta PMM patching.");
  }
  if (!smallVmdFile) {
    reasons.push("Registry entry source.smallVmd is required for key-count delta PMM patching.");
  }
  if (!largeVmdFile) {
    reasons.push("Registry entry source.largeVmd is required for key-count delta PMM patching.");
  }
  if (!options.base) {
    reasons.push("Target base PMM --base is required to produce a key-count delta patch command.");
  }
  if (!options.out) {
    reasons.push("Output PMM --out is required to produce a key-count delta patch command.");
  }
  for (const [label, file] of [
    ["source.smallVariant", smallVariant],
    ["source.largeVariant", largeVariant],
    ["source.smallVmd", smallVmdFile],
    ["source.largeVmd", largeVmdFile],
    ["base", options.base],
  ]) {
    if (file) {
      const exists = await fileExists(file);
      if (!exists) {
        reasons.push(`${label} file does not exist: ${file}.`);
      }
    }
  }
  const profileSlot = profile?.modelSlotContext?.slot;
  if (options.targetSlot !== undefined && profileSlot !== undefined && options.targetSlot !== profileSlot) {
    reasons.push(`Requested target slot ${options.targetSlot} does not match profile slot ${profileSlot}.`);
  }
  let smallVmd;
  let largeVmd;
  if (smallVmdFile && (await fileExists(smallVmdFile))) {
    try {
      smallVmd = await readVmdInventory(smallVmdFile, { limit: Number.POSITIVE_INFINITY });
      validateSupportedVmdChannels(smallVmd, "small VMD");
    } catch (error) {
      reasons.push(error.message);
    }
  }
  if (largeVmdFile && (await fileExists(largeVmdFile))) {
    try {
      largeVmd = await readVmdInventory(largeVmdFile, { limit: Number.POSITIVE_INFINITY });
      validateSupportedVmdChannels(largeVmd, "large VMD");
      assertSameProfileShape(normalizeVmdBoneKeys(largeVmd), normalizeVmdBoneKeys(targetVmd));
    } catch (error) {
      reasons.push(error.message);
    }
  }
  if (smallVmd && largeVmd && (largeVmd.counts?.boneFrames ?? 0) <= (smallVmd.counts?.boneFrames ?? 0)) {
    reasons.push(
      `Key-count delta registry entry must grow key count, got ${smallVmd.counts?.boneFrames ?? 0}->${largeVmd.counts?.boneFrames ?? 0}.`,
    );
  }
  const command = {
    script: "patch-pmm-vmd-key-count-delta",
    args: {
      base: options.base,
      smallVariant,
      largeVariant,
      smallVmd: smallVmdFile,
      largeVmd: largeVmdFile,
      targetVmd: options.targetVmdFile,
      out: options.out,
      targetSlot: options.targetSlot ?? profileSlot,
    },
  };
  const ok = reasons.length === 0;
  return {
    index,
    id: entry.id,
    kind: entry.kind,
    ok,
    score: (ok ? 100 : 0) + (profile?.verified ? 5 : 0) + (profileSlot !== undefined ? 5 : 0),
    source,
    profile: {
      verified: Boolean(profile?.verified),
      recordCount: profile?.recordCount,
      recordByteLength: profile?.recordByteLength,
      modelSlotContext: profile?.modelSlotContext,
    },
    target: {
      boneFrameCount: targetVmd.counts?.boneFrames ?? targetVmd.bones?.length ?? 0,
      maxFrame: targetVmd.maxFrame,
    },
    small: smallVmd
      ? {
          boneFrameCount: smallVmd.counts?.boneFrames ?? smallVmd.bones?.length ?? 0,
          maxFrame: smallVmd.maxFrame,
        }
      : undefined,
    large: largeVmd
      ? {
          boneFrameCount: largeVmd.counts?.boneFrames ?? largeVmd.bones?.length ?? 0,
          maxFrame: largeVmd.maxFrame,
        }
      : undefined,
    command,
    reasons,
  };
}

async function inspectPatchRegistryEntry(index, entry) {
  const reasons = [];
  const warnings = [];
  const sourceFiles = [];
  const profile = entry.profile ?? {};
  if (entry.kind === "same-shape") {
    sourceFiles.push(...requiredSourceFiles(entry.source, ["base", "variant", "vmd"]));
  } else if (entry.kind === "key-count-delta") {
    sourceFiles.push(...requiredSourceFiles(entry.source, ["smallVariant", "largeVariant", "smallVmd", "largeVmd"]));
  } else {
    reasons.push(`Registry entry kind ${entry.kind} is not supported.`);
  }
  for (const sourceFile of sourceFiles) {
    if (!sourceFile.path) {
      reasons.push(`Registry entry source.${sourceFile.key} is required for ${entry.kind} profiles.`);
      sourceFile.exists = false;
      continue;
    }
    sourceFile.exists = await fileExists(sourceFile.path);
    if (!sourceFile.exists) {
      reasons.push(`source.${sourceFile.key} file does not exist: ${sourceFile.path}.`);
    }
  }
  const vmdSummaries = {};
  for (const sourceFile of sourceFiles.filter((file) => file.path && file.exists && file.key.toLowerCase().endsWith("vmd"))) {
    try {
      const vmd = await readVmdInventory(sourceFile.path, { limit: Number.POSITIVE_INFINITY });
      validateSupportedVmdChannels(vmd, `${sourceFile.key} VMD`);
      vmdSummaries[sourceFile.key] = summarizeVmdForRegistry(vmd);
    } catch (error) {
      reasons.push(`source.${sourceFile.key}: ${error.message}`);
    }
  }
  if (entry.kind === "key-count-delta" && vmdSummaries.smallVmd && vmdSummaries.largeVmd) {
    if (vmdSummaries.largeVmd.boneFrameCount <= vmdSummaries.smallVmd.boneFrameCount) {
      reasons.push(
        `key-count-delta must grow bone key count, got ${vmdSummaries.smallVmd.boneFrameCount}->${vmdSummaries.largeVmd.boneFrameCount}.`,
      );
    }
  }
  if (!profile?.verified) {
    warnings.push("Profile is not marked verified.");
  }
  if (!Number.isInteger(profile?.recordByteLength)) {
    warnings.push("Profile recordByteLength is missing.");
  }
  if (!Number.isInteger(profile?.recordCount)) {
    warnings.push("Profile recordCount is missing.");
  }
  if (!Number.isInteger(profile?.modelSlotContext?.slot)) {
    warnings.push("Profile modelSlotContext.slot is missing; multi-model writes should pass explicit slots.");
  }
  return {
    index,
    id: entry.id,
    kind: entry.kind,
    sourceKind: entry.sourceKind,
    ok: reasons.length === 0,
    sourceFiles,
    profile: {
      verified: Boolean(profile?.verified),
      recordByteLength: profile?.recordByteLength,
      recordCount: profile?.recordCount,
      modelSlotContext: profile?.modelSlotContext,
    },
    vmd: vmdSummaries,
    reasons,
    warnings,
  };
}

function requiredSourceFiles(source = {}, keys) {
  return keys.map((key) => ({ key, path: source[key] }));
}

function summarizeVmdForRegistry(vmd) {
  return {
    modelName: vmd.modelName,
    boneFrameCount: vmd.counts?.boneFrames ?? vmd.bones?.length ?? 0,
    morphFrameCount: vmd.counts?.morphFrames ?? 0,
    cameraFrameCount: vmd.counts?.cameraFrames ?? 0,
    lightFrameCount: vmd.counts?.lightFrames ?? 0,
    selfShadowFrameCount: vmd.counts?.selfShadowFrames ?? 0,
    propertyFrameCount: vmd.counts?.propertyFrames ?? 0,
    maxFrame: vmd.maxFrame,
    boneNames: [...new Set((vmd.bones ?? []).map((bone) => bone.name))],
  };
}

function summarizePatchRegistryInspectionEntry(inspection, entry) {
  const small = entry.vmd?.smallVmd;
  const large = entry.vmd?.largeVmd;
  const same = entry.vmd?.vmd;
  const vmdShape =
    entry.kind === "key-count-delta"
      ? {
          smallBoneFrameCount: small?.boneFrameCount,
          largeBoneFrameCount: large?.boneFrameCount,
          boneFrameDelta:
            Number.isInteger(small?.boneFrameCount) && Number.isInteger(large?.boneFrameCount)
              ? large.boneFrameCount - small.boneFrameCount
              : undefined,
          largeMaxFrame: large?.maxFrame,
          boneNames: large?.boneNames ?? [],
        }
      : {
          boneFrameCount: same?.boneFrameCount,
          maxFrame: same?.maxFrame,
          boneNames: same?.boneNames ?? [],
        };
  return {
    registryFile: inspection.registryFile,
    index: entry.index,
    id: entry.id,
    kind: entry.kind,
    ok: entry.ok,
    recordByteLength: entry.profile?.recordByteLength,
    recordCount: entry.profile?.recordCount,
    verified: entry.profile?.verified,
    modelSlot: entry.profile?.modelSlotContext?.slot,
    vmdShape,
    reasons: entry.reasons ?? [],
    warnings: entry.warnings ?? [],
  };
}

function summarizePatchCandidateForCompatibility(candidate) {
  return {
    registryFile: candidate.registryFile,
    index: candidate.index,
    id: candidate.id,
    kind: candidate.kind,
    ok: candidate.ok,
    score: candidate.score,
    command: candidate.command?.script,
    recordByteLength: candidate.profile?.recordByteLength,
    recordCount: candidate.profile?.recordCount,
    verified: candidate.profile?.verified,
    modelSlot: candidate.profile?.modelSlotContext?.slot,
    target: candidate.target,
    donor: candidate.donor,
    small: candidate.small,
    large: candidate.large,
    reasons: candidate.reasons ?? [],
  };
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
  const kind = normalizeRegistryEntryKind(entry.kind, entry.source);
  return {
    id: entry.id ?? entry.name ?? `profile-${index}`,
    kind,
    sourceKind: entry.kind,
    source: entry.source,
    profile: entry.profile ?? entry,
  };
}

function normalizeRegistryEntryKind(kind, source) {
  if (kind === "same-shape" || kind === "pmm-keyframe-profile") {
    return "same-shape";
  }
  if (kind === "key-count-delta" || kind === "pmm-key-count-delta-profile") {
    return "key-count-delta";
  }
  return inferRegistryEntryKind(source);
}

function inferRegistryEntryKind(source = {}) {
  if (source.smallVariant || source.largeVariant || source.smallVmd || source.largeVmd) {
    return "key-count-delta";
  }
  if (source.base || source.variant || source.vmd) {
    return "same-shape";
  }
  return "unknown";
}

function makeRegistryKindMismatchCandidate(index, entry, targetVmd, options, script, reasons) {
  return {
    index,
    id: entry.id,
    kind: entry.kind,
    ok: false,
    score: -1,
    source: entry.source,
    profile: {
      verified: Boolean(entry.profile?.verified),
      recordCount: entry.profile?.recordCount,
      recordByteLength: entry.profile?.recordByteLength,
      modelSlotContext: entry.profile?.modelSlotContext,
    },
    target: {
      boneFrameCount: targetVmd.counts?.boneFrames ?? targetVmd.bones?.length ?? 0,
      maxFrame: targetVmd.maxFrame,
    },
    command: {
      script,
      args: {
        base: options.base,
        targetVmd: options.targetVmdFile,
        out: options.out,
      },
    },
    reasons,
  };
}

async function fileExists(file) {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}

function readOptionalModelSlots(bytes) {
  try {
    return parsePmmManifest(bytes).modelSlots;
  } catch {
    return [];
  }
}

function assertSameProfileShape(donorKeys, targetKeys) {
  if (targetKeys.length !== donorKeys.length) {
    throw new Error(`Target VMD key count ${targetKeys.length} does not match donor key count ${donorKeys.length}.`);
  }
  for (const [index, donorKey] of donorKeys.entries()) {
    const targetKey = targetKeys[index];
    if (targetKey.name !== donorKey.name) {
      throw new Error(`Target VMD key ${index} bone ${targetKey.name} does not match donor bone ${donorKey.name}.`);
    }
  }
}

function needsPositionRewriteForSameShapeKeys(donorKeys, targetKeys) {
  return donorKeys.some((donorKey, index) => !vector3Equal(donorKey.position, targetKeys[index].position));
}

function vector3Equal(left = [], right = []) {
  return left.length === 3 && right.length === 3 && left.every((value, index) => approximatelyEqual(value, right[index]));
}

function normalizeVmdBoneKeys(vmd) {
  return (vmd.bones ?? []).map((bone) => ({
    name: bone.name,
    frame: bone.frame,
    position: bone.position,
    rotation: bone.rotation,
  }));
}

function validateDeltaReport(deltaReport) {
  const summary = deltaReport.summary;
  if (!summary.recordByteDeltaMatchesFileDelta) {
    throw new Error(
      `PMM key-count delta byte length mismatch: expected ${summary.expectedRecordByteDelta}, got ${summary.actualByteLengthDelta}.`,
    );
  }
  if (!summary.sharedBlockStart) {
    throw new Error("PMM key-count delta block start changed; this experimental patcher only supports stable block starts.");
  }
  if (!deltaReport.largeProfile?.verified) {
    throw new Error(`Large PMM/VMD profile is not verified: ${(deltaReport.largeProfile?.reasons ?? []).join(" ")}`);
  }
  if (summary.largeKeyCount <= summary.smallKeyCount) {
    throw new Error(`PMM key-count delta must grow key count, got ${summary.smallKeyCount}->${summary.largeKeyCount}.`);
  }
}

function rewriteDeltaScalars(bytes, deltaReport, targetVmd) {
  const blockStart = deltaReport.summary.blockStart;
  const rewrites = [];
  const targetKeyCount = targetVmd.counts?.boneFrames ?? targetVmd.bones?.length ?? 0;
  for (const candidate of deltaReport.scalarCandidates.keyCount ?? []) {
    if (candidate.offset !== blockStart) {
      continue;
    }
    bytes.writeUInt32LE(targetKeyCount, candidate.offset);
    rewrites.push({ kind: "keyCount", offset: candidate.offset, offsetHex: candidate.offsetHex, value: targetKeyCount });
  }
  for (const candidate of deltaReport.scalarCandidates.maxFrame ?? []) {
    if (candidate.offset >= blockStart) {
      continue;
    }
    bytes.writeUInt32LE(targetVmd.maxFrame, candidate.offset);
    rewrites.push({ kind: "maxFrame", offset: candidate.offset, offsetHex: candidate.offsetHex, value: targetVmd.maxFrame });
  }
  return rewrites;
}

function rewriteVmdKeysIntoProfile(bytes, targetKeys, options) {
  const rewrites = [];
  for (const [index, targetKey] of targetKeys.entries()) {
    const sequenceRecord = options.frameSequenceProfile.records?.[index];
    const recordStart = sequenceRecord?.recordStart;
    if (!Number.isInteger(recordStart)) {
      throw new Error(`Missing PMM key record for ${targetKey.name} frame index ${index}.`);
    }
    const frameOffset = recordStart + options.frameSequenceProfile.frameOffsetInRecord;
    bytes.writeUInt32LE(targetKey.frame, frameOffset);
    const positionOffset = options.rewritePositions ? recordStart + options.positionProfile.positionOffsetInRecord : undefined;
    if (positionOffset !== undefined) {
      bytes.writeFloatLE(targetKey.position[0], positionOffset);
      bytes.writeFloatLE(targetKey.position[1], positionOffset + 4);
      bytes.writeFloatLE(targetKey.position[2], positionOffset + 8);
    }
    const rotationOffset = options.rewriteRotations ? recordStart + options.transformProfile.rotationOffsetInRecord : undefined;
    if (rotationOffset !== undefined) {
      bytes.writeFloatLE(targetKey.rotation[0], rotationOffset);
      bytes.writeFloatLE(targetKey.rotation[1], rotationOffset + 4);
      bytes.writeFloatLE(targetKey.rotation[2], rotationOffset + 8);
      bytes.writeFloatLE(targetKey.rotation[3], rotationOffset + 12);
    }
    rewrites.push({
      index,
      name: targetKey.name,
      frame: targetKey.frame,
      frameOffset,
      frameOffsetHex: hex(frameOffset),
      position: positionOffset === undefined ? undefined : targetKey.position,
      positionOffset,
      positionOffsetHex: positionOffset === undefined ? undefined : hex(positionOffset),
      rotation: rotationOffset === undefined ? undefined : targetKey.rotation,
      rotationOffset,
      rotationOffsetHex: rotationOffset === undefined ? undefined : hex(rotationOffset),
    });
  }
  return rewrites;
}

function isAllZeroVector(vector = []) {
  return vector.length === 3 && vector.every((value) => approximatelyEqual(value, 0));
}

function validateSupportedVmdChannels(vmd, label) {
  const unsupported = unsupportedVmdChannels(vmd);
  if (unsupported.length > 0) {
    throw new Error(`${label} has unsupported channels: ${unsupported.map((entry) => `${entry.name}=${entry.count}`).join(", ")}.`);
  }
}

function unsupportedVmdChannels(vmd) {
  return [
    { name: "morphFrames", count: vmd.counts?.morphFrames ?? 0 },
    { name: "cameraFrames", count: vmd.counts?.cameraFrames ?? 0 },
    { name: "lightFrames", count: vmd.counts?.lightFrames ?? 0 },
    { name: "selfShadowFrames", count: vmd.counts?.selfShadowFrames ?? 0 },
    { name: "propertyFrames", count: vmd.counts?.propertyFrames ?? 0 },
  ].filter((entry) => entry.count > 0);
}

function requiredFixtureForUnsupportedChannel(name) {
  switch (name) {
    case "morphFrames":
      return "base PMM plus the same PMX with one controlled morph key, then a matching VMD morph-only oracle.";
    case "cameraFrames":
      return "base PMM plus one controlled camera key, then a matching VMD camera-only oracle.";
    case "lightFrames":
      return "base PMM plus one controlled light key, then a matching VMD light-only oracle.";
    case "selfShadowFrames":
      return "base PMM plus one controlled self-shadow key, then a matching VMD self-shadow-only oracle.";
    case "propertyFrames":
      return "base PMM plus one controlled model property/IK key, then a matching VMD property-only oracle.";
    default:
      return `controlled PMM/VMD fixture for ${name}.`;
  }
}

function isIdentityRotation(rotation = []) {
  return (
    rotation.length === 4 &&
    approximatelyEqual(rotation[0], 0) &&
    approximatelyEqual(rotation[1], 0) &&
    approximatelyEqual(rotation[2], 0) &&
    approximatelyEqual(rotation[3], 1)
  );
}

function approximatelyEqual(left, right) {
  return Math.abs(left - right) < 0.00001;
}

function withoutBytes(value) {
  const { bytes, ...rest } = value;
  return rest;
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

function requireArray(options, name) {
  const value = options[name];
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error(`Missing required option: ${name}`);
  }
  return value;
}
