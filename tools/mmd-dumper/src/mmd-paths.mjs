import { access } from "node:fs/promises";
import { dirname, extname, isAbsolute, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { readPmmManifest } from "./pmm-manifest.mjs";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

export function defaultMmdExePath() {
  return resolve(packageRoot, "MikuMikuDance_v932x64", "MikuMikuDance.exe");
}

export async function validateFixtureInputs(fixture) {
  const checks = [
    checkPath("mmdExe", fixture.mmdExe),
    checkPath("project", fixture.project),
  ];
  if (extname(fixture.project).toLowerCase() === ".pmm") {
    checks.push(...(await checkPmmAssetReferences(fixture.project, fixture.mmdExe)));
  }
  const results = await Promise.all(checks);
  const missing = results.flat().filter(Boolean);
  if (missing.length > 0) {
    const detail = missing.map((entry) => `${entry.label}: ${entry.path}`).join("\n");
    throw new Error(`MMDDumper input check failed before launching MMD:\n${detail}`);
  }
}

async function checkPath(label, path) {
  try {
    await access(path);
    return [];
  } catch {
    return [{ label, path }];
  }
}

async function checkPmmAssetReferences(project, mmdExe) {
  let manifest;
  try {
    manifest = await readPmmManifest(project);
  } catch {
    return [];
  }
  const references = (manifest.assetReferences ?? []).filter((reference) =>
    ["model", "motion", "accessory", "audio"].includes(reference.kind),
  );
  const results = [];
  for (const reference of references) {
    if (!(await anyPathExists(resolvePmmReferenceCandidates(reference.normalizedPath, project, mmdExe)))) {
      results.push({ label: `PMM ${reference.kind} reference`, path: reference.normalizedPath });
    }
  }
  return results;
}

function resolvePmmReferenceCandidates(referencePath, project, mmdExe) {
  const normalized = referencePath.replaceAll("/", "\\");
  if (isAbsolute(normalized)) {
    return [normalized];
  }
  const mmdRoot = dirname(mmdExe);
  return [
    resolve(dirname(project), normalized),
    resolve(mmdRoot, normalized),
    resolve(mmdRoot, "Data", normalized),
  ];
}

async function anyPathExists(paths) {
  for (const path of paths) {
    try {
      await access(path);
      return true;
    } catch {
      // Try the next candidate.
    }
  }
  return false;
}
