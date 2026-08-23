import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, dirname, resolve } from "node:path";
import { deriveOracleFrames, prepareOracleFromVmd, recordOracleFromVmd } from "./oracle-from-vmd.mjs";
import { defaultMmdExePath } from "./mmd-paths.mjs";
import { stageMmdCompatiblePmx } from "./pmx-mmd-compat.mjs";
import { writePmmFromPmxVmd } from "./pmm-from-pmx-vmd.mjs";
import { recordWithMmd, toPortableFixture } from "./runner.mjs";
import { readVmdInventory } from "./vmd-inventory.mjs";

export async function readOracleBatchManifest(path) {
  const manifestPath = resolve(path);
  const text = await readFile(manifestPath, "utf8");
  let manifest;
  try {
    manifest = JSON.parse(text);
  } catch (cause) {
    throw new Error(`Invalid oracle batch manifest ${path}: ${cause.message}`);
  }
  return normalizeOracleBatchManifest(manifest, manifestPath);
}

export function normalizeOracleBatchManifest(manifest, manifestPath = resolve("oracle-batch.json"), options = {}) {
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new Error(`${manifestPath} must be an object.`);
  }
  const root = dirname(resolve(manifestPath));
  const defaults = manifest.defaults && typeof manifest.defaults === "object" ? manifest.defaults : {};
  const outDir = resolveOutputPath(root, options.outDir ?? defaults.outDir ?? "out/oracle-batch");
  const templates = normalizeTemplates(manifest.templates ?? [], root);
  const cases = normalizeCases(manifest.cases, { root, outDir, backend: manifest.backend, defaults, templates });
  return {
    ok: true,
    manifestPath: resolve(manifestPath),
    outDir,
    defaults,
    templates,
    cases,
  };
}

export async function prepareOracleBatch(options) {
  const batch = await readOracleBatchManifest(requireString(options, "manifest"));
  const outDir = options.outDir ? resolve(options.outDir) : batch.outDir;
  const selected = filterCases(batch.cases, options.caseName);
  await mkdir(outDir, { recursive: true });
  const results = [];
  for (const testCase of selected) {
    results.push(await runOneCase(testCase, { ...options, outDir, outDirOverride: Boolean(options.outDir), dryRun: true }));
  }
  return summarizeBatch("oracle-batch-prepare", batch, results, outDir);
}

export async function recordOracleBatch(options) {
  const batch = await readOracleBatchManifest(requireString(options, "manifest"));
  const outDir = options.outDir ? resolve(options.outDir) : batch.outDir;
  const selected = filterCases(batch.cases, options.caseName);
  await mkdir(outDir, { recursive: true });
  const results = [];
  for (const testCase of selected) {
    results.push(await runOneCase(testCase, { ...options, outDir, outDirOverride: Boolean(options.outDir), dryRun: false }));
  }
  return summarizeBatch("oracle-batch-record", batch, results, outDir);
}

async function runOneCase(testCase, options) {
  await access(testCase.pmx);
  await access(testCase.vmd);
  const caseDir = resolve(options.outDir, sanitizeName(testCase.name));
  if (!testCase.templatePmm) {
    return runOneTemplateFreeCase(testCase, { ...options, caseDir });
  }
  await access(testCase.templatePmm);
  const request = {
    templatePmm: testCase.templatePmm,
    targetVmd: testCase.vmd,
    targetSlot: testCase.targetSlot,
    frames: testCase.frames,
    outDir: caseDir,
    projectOut: resolve(caseDir, "scene.pmm"),
    fixtureOut: resolve(caseDir, "fixture.json"),
    output: resolveOracleOutput(testCase, options, caseDir),
    mmdExe: options.mmdExe ?? testCase.mmdExe,
    timeoutMs: options.timeoutMs ?? testCase.timeoutMs,
    sendKeyAfterMs: options.sendKeyAfterMs ?? testCase.sendKeyAfterMs,
    cameraModeAfterMs: options.cameraModeAfterMs ?? testCase.cameraModeAfterMs,
    keepInitialFrameZero: testCase.keepInitialFrameZero,
    name: testCase.name,
  };
  const result = options.dryRun ? await prepareOracleFromVmd(request) : await recordOracleFromVmd(request);
  return {
    ok: result.ok,
    name: testCase.name,
    pmx: testCase.pmx,
    vmd: testCase.vmd,
    templatePmm: testCase.templatePmm,
    targetSlot: testCase.targetSlot,
    project: result.project,
    fixturePath: result.fixturePath,
    output: result.output,
    frames: result.frames,
    records: result.records,
    patch: {
      ok: result.patch?.ok,
      byteLengthDelta: result.patch?.byteLengthDelta,
      rewriteCount: result.patch?.rewriteCount,
      counts: result.patch?.comparison?.counts,
    },
  };
}

async function runOneTemplateFreeCase(testCase, options) {
  const project = resolve(options.caseDir, "scene.pmm");
  const fixturePath = resolve(options.caseDir, "fixture.json");
  const output = resolveOracleOutput(testCase, options, options.caseDir);
  await mkdir(options.caseDir, { recursive: true });
  const stagedPmx = await stageMmdCompatiblePmx(testCase.pmx, resolve(options.caseDir, "model.mmd-utf16.pmx"));
  const vmd = await readVmdInventory(testCase.vmd, { limit: Number.MAX_SAFE_INTEGER });
  const cameraVmd = testCase.cameraVmd ? await readVmdInventory(testCase.cameraVmd, { limit: Number.MAX_SAFE_INTEGER }) : null;
  const derivedFramesRange =
    testCase.frames === undefined &&
    testCase.framesRange === undefined &&
    testCase.playback === true &&
    cameraVmd &&
    Number.isFinite(cameraVmd.maxFrame)
      ? { start: 0, end: cameraVmd.maxFrame, step: 1 }
      : undefined;
  const frames = testCase.frames ?? expandFramesRange(testCase.framesRange ?? derivedFramesRange) ?? deriveOracleFrames(cameraVmd ?? vmd);
  const pmm = await writePmmFromPmxVmd({
    pmx: stagedPmx.output,
    vmd: testCase.vmd,
    cameraVmd: testCase.cameraVmd,
    out: project,
  });
  const fixture = {
    name: testCase.name,
    mmdVersion: "9.32-x64",
    mmdExe: options.mmdExe ?? testCase.mmdExe ?? defaultMmdExePath(),
    project,
    frames,
    output,
    done: `${output}.done`,
    timeoutMs: options.timeoutMs ?? testCase.timeoutMs ?? 60000,
    trigger: testCase.trigger,
    jumpFrameIntervalMs: testCase.jumpFrameIntervalMs,
    captureFrameOffset: testCase.captureFrameOffset,
    cameraModeAfterMs: testCase.cameraModeAfterMs,
    keepInitialFrameZero: testCase.keepInitialFrameZero,
    playback: testCase.playback,
    framesRange: testCase.framesRange ?? derivedFramesRange,
    dump: testCase.dump,
    physics: testCase.physics,
  };
  await writeFile(fixturePath, `${JSON.stringify(toPortableFixture(fixture), null, 2)}\n`, "utf8");
  const records = options.dryRun
    ? undefined
    : await recordWithMmd(fixture, {
        fixturePath,
        sendKeyAfterMs: options.sendKeyAfterMs ?? testCase.sendKeyAfterMs ?? (testCase.physics?.enabled ? 8000 : 3000),
        cameraModeAfterMs: options.cameraModeAfterMs ?? testCase.cameraModeAfterMs,
        keepInitialFrameZero: testCase.keepInitialFrameZero,
      });
  return {
    ok: true,
    name: testCase.name,
    mode: isEmptyVmd(vmd) ? "pmx-generated-pmm" : "pmx-vmd-generated-pmm",
    pmx: testCase.pmx,
    stagedPmx,
    vmd: testCase.vmd,
    cameraVmd: testCase.cameraVmd,
    templatePmm: null,
    targetSlot: 0,
    project,
    fixturePath,
    output,
    frames,
    records: records?.length,
    patch: {
      ok: pmm.patch?.comparison?.ok,
      byteLengthDelta: pmm.patch?.byteLengthDelta,
      rewriteCount: pmm.patch?.rewriteCount,
      counts: pmm.patch?.comparison?.counts,
    },
    filter: pmm?.filter ?? null,
  };
}

function isEmptyVmd(vmd) {
  return (
    vmd.counts.boneFrames === 0 &&
    vmd.counts.morphFrames === 0 &&
    vmd.counts.cameraFrames === 0 &&
    vmd.counts.lightFrames === 0 &&
    vmd.counts.selfShadowFrames === 0 &&
    vmd.counts.propertyFrames === 0
  );
}

function normalizeTemplates(templates, root) {
  if (!Array.isArray(templates)) {
    throw new Error("oracle batch templates must be an array.");
  }
  return templates.map((entry, index) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
      throw new Error(`templates[${index}] must be an object.`);
    }
    return {
      pmx: resolvePath(root, requireString(entry, "pmx", `templates[${index}]`)),
      templatePmm: resolvePath(root, requireString(entry, "templatePmm", `templates[${index}]`)),
      targetSlot: requireNonNegativeInteger(entry.targetSlot ?? 0, `templates[${index}].targetSlot`),
    };
  });
}

function isOracleBatchBackend(backend) {
  if (backend === undefined || backend === null) {
    return true;
  }
  return ["mmd-native", "mmd-native-render"].includes(String(backend));
}

function normalizeCases(cases, context) {
  if (!Array.isArray(cases) || cases.length === 0) {
    throw new Error("oracle batch cases must be a non-empty array.");
  }
  const supportedCases = cases
    .map((entry, index) => ({ entry, index }))
    .filter(({ entry }) => isOracleBatchBackend(entry.backend ?? context.backend ?? context.defaults.backend));
  if (supportedCases.length === 0) {
    throw new Error("oracle batch manifest does not contain any mmd-native compatible cases.");
  }
  return supportedCases.map(({ entry, index }) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
      throw new Error(`cases[${index}] must be an object.`);
    }
    const assets = normalizeAssets(entry.assets, entry, context.defaults.assets);
    const pmx = resolvePath(context.root, requireString(assets, "model", `cases[${index}].assets`));
    const template = assets.pmm
      ? {
          templatePmm: resolvePath(context.root, assets.pmm),
          targetSlot: entry.targetSlot ?? 0,
        }
      : resolveTemplateForPmx(pmx, context.templates, `cases[${index}]`, { optional: true });
    return {
      name: entry.name ?? stripExtension(basename(assets.motion ?? `case-${index}`)),
      kind: entry.kind ?? "motion-numeric",
      backend: entry.backend ?? context.backend ?? context.defaults.backend ?? "mmd-native",
      pmx,
      vmd: resolvePath(context.root, requireString(assets, "motion", `cases[${index}].assets`)),
      cameraVmd: assets.cameraMotion ? resolvePath(context.root, assets.cameraMotion) : undefined,
      templatePmm: template?.templatePmm,
      targetSlot: requireNonNegativeInteger(entry.targetSlot ?? template?.targetSlot ?? 0, `cases[${index}].targetSlot`),
      frames:
        entry.frames === undefined && entry.framesRange === undefined
          ? undefined
          : normalizeFrameSpec(entry.frames, entry.framesRange, `cases[${index}]`),
      oraclePath: entry.oracle?.path ? resolvePath(context.root, entry.oracle.path) : undefined,
      mmdExe: entry.mmdExe
        ? resolvePath(context.root, entry.mmdExe)
        : context.defaults.mmdExe
          ? resolvePath(context.root, context.defaults.mmdExe)
          : undefined,
      timeoutMs: entry.timeoutMs ?? context.defaults.timeoutMs,
      trigger: entry.trigger ?? context.defaults.trigger,
      jumpFrameIntervalMs: normalizeOptionalNonNegativeInteger(
        entry.jumpFrameIntervalMs ?? context.defaults.jumpFrameIntervalMs,
        `cases[${index}].jumpFrameIntervalMs`,
      ),
      captureFrameOffset: normalizeOptionalFiniteNumber(
        entry.captureFrameOffset ?? context.defaults.captureFrameOffset,
        `cases[${index}].captureFrameOffset`,
      ),
      sendKeyAfterMs: normalizeOptionalNonNegativeInteger(
        entry.sendKeyAfterMs ?? context.defaults.sendKeyAfterMs,
        `cases[${index}].sendKeyAfterMs`,
      ),
      cameraModeAfterMs: normalizeOptionalNonNegativeInteger(
        entry.cameraModeAfterMs ?? context.defaults.cameraModeAfterMs,
        `cases[${index}].cameraModeAfterMs`,
      ),
      keepInitialFrameZero: normalizeOptionalBoolean(
        entry.keepInitialFrameZero ?? context.defaults.keepInitialFrameZero,
        `cases[${index}].keepInitialFrameZero`,
      ),
      playback: normalizeOptionalBoolean(entry.playback ?? context.defaults.playback, `cases[${index}].playback`),
      framesRange: entry.framesRange,
      dump: normalizeDump(entry.dump, context.defaults.dump),
      physics: normalizePhysics(entry.physics, context.defaults.physics),
    };
  });
}

function normalizeDump(entryDump, defaultDump) {
  const defaults = defaultDump && typeof defaultDump === "object" && !Array.isArray(defaultDump) ? defaultDump : {};
  const source = entryDump && typeof entryDump === "object" && !Array.isArray(entryDump) ? entryDump : {};
  return {
    bones: source.bones ?? defaults.bones ?? true,
    morphs: source.morphs ?? defaults.morphs ?? true,
    camera: source.camera ?? defaults.camera ?? false,
    cameraKeyframes: source.cameraKeyframes ?? defaults.cameraKeyframes ?? true,
    sceneParameters: source.sceneParameters ?? defaults.sceneParameters ?? false,
    rigidBodies: source.rigidBodies ?? defaults.rigidBodies ?? false,
  };
}

function normalizePhysics(entryPhysics, defaultPhysics) {
  const defaults = defaultPhysics && typeof defaultPhysics === "object" && !Array.isArray(defaultPhysics) ? defaultPhysics : {};
  const source = entryPhysics && typeof entryPhysics === "object" && !Array.isArray(entryPhysics) ? entryPhysics : {};
  const enabled = source.enabled ?? defaults.enabled;
  if (enabled === undefined) {
    return undefined;
  }
  return {
    enabled: Boolean(enabled),
    warmupFrames: source.warmupFrames ?? defaults.warmupFrames,
    timeStep: source.timeStep ?? defaults.timeStep,
    comparison: source.comparison ?? defaults.comparison,
  };
}

function normalizeAssets(assets, entry, defaultAssets) {
  const defaults = defaultAssets && typeof defaultAssets === "object" && !Array.isArray(defaultAssets) ? defaultAssets : {};
  const source = assets && typeof assets === "object" && !Array.isArray(assets) ? assets : {};
  return {
    model: source.model ?? entry.pmx ?? defaults.model,
    motion: source.motion ?? entry.vmd ?? defaults.motion,
    cameraMotion: source.cameraMotion ?? entry.cameraVmd ?? entry.sourceCameraVmd ?? defaults.cameraMotion,
    pmm: source.pmm ?? entry.templatePmm ?? entry.pmm ?? defaults.pmm,
  };
}

function resolveOracleOutput(testCase, options, caseDir) {
  return options.outDirOverride || !testCase.oraclePath ? resolve(caseDir, "oracle.actual.jsonl") : testCase.oraclePath;
}

function resolveTemplateForPmx(pmx, templates, context, options = {}) {
  if (templates.length === 0 && options.optional) {
    return null;
  }
  const exact = templates.filter((template) => samePath(template.pmx, pmx));
  if (exact.length === 1) {
    return exact[0];
  }
  if (exact.length > 1) {
    throw new Error(`${context} PMX matches multiple templates exactly: ${pmx}`);
  }
  const byFileName = templates.filter((template) => basename(template.pmx).toLowerCase() === basename(pmx).toLowerCase());
  if (byFileName.length === 1) {
    return byFileName[0];
  }
  if (byFileName.length > 1) {
    throw new Error(`${context} PMX basename is ambiguous in templates: ${basename(pmx)}.`);
  }
  if (options.optional) {
    return null;
  }
  throw new Error(`${context} has no template for PMX: ${pmx}. Add case.templatePmm or templates[].`);
}

function filterCases(cases, caseName) {
  if (!caseName) {
    return cases;
  }
  const names = new Set(caseName.split(",").map((name) => name.trim()).filter(Boolean));
  const selected = cases.filter((testCase) => names.has(testCase.name));
  if (selected.length !== names.size) {
    const found = new Set(selected.map((testCase) => testCase.name));
    const missing = [...names].filter((name) => !found.has(name));
    throw new Error(`Unknown oracle batch case(s): ${missing.join(", ")}`);
  }
  return selected;
}

function summarizeBatch(mode, batch, results, outDir) {
  return {
    ok: results.every((result) => result.ok),
    mode,
    manifestPath: batch.manifestPath,
    outDir,
    cases: results.length,
    results,
  };
}

function normalizeFrames(frames, context) {
  if (!Array.isArray(frames) || frames.length === 0 || frames.some((frame) => !Number.isFinite(frame))) {
    throw new Error(`${context} must be a non-empty finite number array.`);
  }
  return [...new Set(frames)].sort((left, right) => left - right);
}

function normalizeFrameSpec(frames, range, context) {
  if (frames !== undefined) {
    return normalizeFrames(frames, `${context}.frames`);
  }
  if (!range || typeof range !== "object" || Array.isArray(range)) {
    throw new Error(`${context}.framesRange must be an object.`);
  }
  const start = requireNonNegativeInteger(range.start, `${context}.framesRange.start`);
  const end = requireNonNegativeInteger(range.end, `${context}.framesRange.end`);
  const step = range.step === undefined ? 1 : requireNonNegativeInteger(range.step, `${context}.framesRange.step`);
  if (step <= 0) {
    throw new Error(`${context}.framesRange.step must be a positive integer.`);
  }
  if (end < start) {
    throw new Error(`${context}.framesRange.end must be greater than or equal to start.`);
  }
  const normalized = [];
  for (let frame = start; frame <= end; frame += step) {
    normalized.push(frame);
  }
  return normalized;
}

function expandFramesRange(range) {
  if (!range) {
    return undefined;
  }
  const frames = [];
  for (let frame = range.start; frame <= range.end; frame += range.step ?? 1) {
    frames.push(frame);
  }
  return frames;
}

function requireNonNegativeInteger(value, context) {
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${context} must be a non-negative integer.`);
  }
  return value;
}

function normalizeOptionalNonNegativeInteger(value, context) {
  if (value === undefined) {
    return undefined;
  }
  return requireNonNegativeInteger(value, context);
}

function normalizeOptionalFiniteNumber(value, context) {
  if (value === undefined) {
    return undefined;
  }
  if (!Number.isFinite(value)) {
    throw new Error(`${context} must be a finite number.`);
  }
  return value;
}

function normalizeOptionalBoolean(value, context) {
  if (value === undefined) {
    return undefined;
  }
  if (typeof value !== "boolean") {
    throw new Error(`${context} must be a boolean.`);
  }
  return value;
}

function resolvePath(root, path) {
  return /^[A-Za-z]:[\\/]/.test(path) ? path : resolve(root, path);
}

function resolveOutputPath(root, path) {
  return /^[A-Za-z]:[\\/]/.test(path) ? path : resolve(root, path);
}

function samePath(left, right) {
  return left.toLowerCase() === right.toLowerCase();
}

function sanitizeName(name) {
  return String(name).replace(/[<>:"/\\|?*\x00-\x1f]/g, "_");
}

function stripExtension(file) {
  const index = file.lastIndexOf(".");
  return index > 0 ? file.slice(0, index) : file;
}

function requireString(options, key, context = "options") {
  const value = options[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${context}.${key} must be a non-empty string.`);
  }
  return value;
}
