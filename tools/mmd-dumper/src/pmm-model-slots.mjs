import { dirname, isAbsolute, resolve } from "node:path";
import { parsePmmManifest, readPmmManifest } from "./pmm-manifest.mjs";
import { readPmxInventory } from "./pmx-inventory.mjs";

export async function readPmmModelSlotReport(file, options = {}) {
  const manifest = await readPmmManifest(file);
  return inspectPmmModelSlots(manifest, {
    ...options,
    pmmFile: file,
  });
}

export async function inspectPmmModelSlots(manifest, options = {}) {
  const slots = await Promise.all(
    manifest.modelSlots.map(async (slot) => {
      const resolvedPath = resolveModelPath(slot.path, options.pmmFile);
      const model = await readOptionalPmxInventory(resolvedPath, options);
      return {
        ...slot,
        resolvedPath,
        readable: model.ok,
        inventory: model.inventory,
        error: model.error,
      };
    }),
  );

  return summarizeModelSlots(manifest, slots);
}

export function summarizeModelSlots(manifest, slots) {
  return {
    signature: manifest.signature,
    version: manifest.version,
    byteLength: manifest.byteLength,
    modelSlotCount: slots.length,
    modelSlots: slots,
    boneNameCollisions: findBoneNameCollisions(slots),
    notes: [
      "Model slots are provisional and inferred from PMM model path order.",
      "Use this as fixture inventory evidence before enabling any multi-model PMM write path.",
    ],
  };
}

export function parsePmmModelSlotManifest(bytes) {
  return parsePmmManifest(bytes);
}

async function readOptionalPmxInventory(path, options) {
  try {
    const inventory = await readPmxInventory(path, { limit: options.limit ?? 32 });
    return { ok: true, inventory };
  } catch (error) {
    return { ok: false, error: error.message };
  }
}

function findBoneNameCollisions(slots) {
  const byName = new Map();
  for (const slot of slots) {
    for (const bone of slot.inventory?.bones ?? []) {
      const entries = byName.get(bone.name) ?? [];
      entries.push({
        slot: slot.slot,
        model: slot.fileName,
        boneIndex: bone.index,
      });
      byName.set(bone.name, entries);
    }
  }
  return [...byName.entries()]
    .filter(([, entries]) => new Set(entries.map((entry) => entry.slot)).size > 1)
    .map(([name, entries]) => ({ name, entries }))
    .sort((left, right) => left.name.localeCompare(right.name, "ja"));
}

function resolveModelPath(modelPath, pmmFile) {
  if (isWindowsAbsolutePath(modelPath) || isAbsolute(modelPath)) {
    return modelPath;
  }
  return resolve(pmmFile ? dirname(pmmFile) : process.cwd(), modelPath);
}

function isWindowsAbsolutePath(value) {
  return /^[A-Za-z]:[\\/]/.test(value);
}
