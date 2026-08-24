import { access, mkdir, writeFile } from "node:fs/promises";
import { basename, dirname, extname, resolve } from "node:path";
import { readVmdInventory } from "./vmd-inventory.mjs";
import { writePmmDocumentVmdKeyframePatch } from "./pmm-document-vmd-patch.mjs";
import { defaultMmdExePath } from "./mmd-paths.mjs";
import { recordWithMmd, toPortableFixture } from "./runner.mjs";

export async function prepareOracleFromVmd(options) {
  const templatePmm = resolve(requireString(options, "templatePmm"));
  const targetVmd = resolve(requireString(options, "targetVmd"));
  await access(templatePmm);
  await access(targetVmd);

  const outDir = resolve(options.outDir ?? defaultOutDir(targetVmd));
  const project = resolve(options.projectOut ?? resolve(outDir, "scene.pmm"));
  const output = resolve(options.output ?? resolve(outDir, "oracle.actual.jsonl"));
  const fixturePath = resolve(options.fixtureOut ?? resolve(outDir, "fixture.json"));
  await mkdir(dirname(project), { recursive: true });
  await mkdir(dirname(output), { recursive: true });
  await mkdir(dirname(fixturePath), { recursive: true });

  const vmd = await readVmdInventory(targetVmd, { limit: Number.MAX_SAFE_INTEGER });
  const frames = normalizeFrames(options.frames ?? deriveOracleFrames(vmd));
  const patch = await writePmmDocumentVmdKeyframePatch({
    template: templatePmm,
    targetVmd,
    out: project,
    targetSlot: options.targetSlot,
    requireVerified: true,
  });
  const fixture = {
    name: options.name ?? `oracle-from-vmd-${stripExtension(basename(targetVmd))}`,
    mmdVersion: options.mmdVersion ?? "9.32-x64",
    mmdExe: options.mmdExe ? resolve(options.mmdExe) : defaultMmdExePath(),
    project,
    frames,
    output,
    done: `${output}.done`,
    timeoutMs: options.timeoutMs ?? 60000,
    dump: {
      bones: options.dumpBones !== false,
      morphs: options.dumpMorphs !== false,
      rigidBodies: options.dumpRigidBodies === true,
    },
  };
  await writeFile(fixturePath, `${JSON.stringify(toPortableFixture(fixture), null, 2)}\n`, "utf8");
  return {
    ok: true,
    mode: "oracle-from-vmd-prepare",
    templatePmm,
    targetVmd,
    sourceCounts: vmd.counts,
    targetSlot: options.targetSlot ?? 0,
    project,
    fixturePath,
    output,
    frames,
    patch,
    fixture,
  };
}

export async function recordOracleFromVmd(options) {
  const prepared = await prepareOracleFromVmd(options);
  const records = await recordWithMmd(prepared.fixture, {
    fixturePath: prepared.fixturePath,
    sendKeyAfterMs: options.sendKeyAfterMs ?? 3000,
  });
  return {
    ...withoutFixture(prepared),
    ok: true,
    mode: "oracle-from-vmd-record",
    records: records.length,
    firstFrame: records[0]?.frame,
    lastFrame: records.at(-1)?.frame,
  };
}

export function deriveOracleFrames(vmd) {
  const frames = new Set([0]);
  for (const frame of [...(vmd.bones ?? []), ...(vmd.morphs ?? [])]) {
    if (Number.isFinite(frame.frame)) {
      frames.add(frame.frame);
    }
  }
  if (Number.isFinite(vmd.maxFrame)) {
    frames.add(vmd.maxFrame);
  }
  return [...frames].sort((left, right) => left - right);
}

function normalizeFrames(frames) {
  if (!Array.isArray(frames) || frames.length === 0 || frames.some((frame) => !Number.isFinite(frame))) {
    throw new Error("oracle-from-vmd requires at least one finite frame.");
  }
  return [...new Set(frames)].sort((left, right) => left - right);
}

function defaultOutDir(targetVmd) {
  return resolve("out", "oracle-from-vmd", stripExtension(basename(targetVmd)));
}

function stripExtension(file) {
  const extension = extname(file);
  return extension ? file.slice(0, -extension.length) : file;
}

function requireString(options, key) {
  const value = options[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`Missing ${key}`);
  }
  return value;
}

function withoutFixture(prepared) {
  const { fixture, ...rest } = prepared;
  return rest;
}
