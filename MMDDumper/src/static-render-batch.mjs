import { access, copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, extname, resolve } from "node:path";
import { defaultMmdExePath } from "./mmd-paths.mjs";
import { stageMmdCompatiblePmx } from "./pmx-mmd-compat.mjs";
import { writeBasePmmFromPmx, writePmmCameraVmdPatch, writePmmFromPmxVmd } from "./pmm-from-pmx-vmd.mjs";
import { exportImagesWithMmd, toPortableFixture } from "./runner.mjs";

export async function readStaticRenderManifest(path, options = {}) {
  const manifestPath = resolve(path);
  const text = await readFile(manifestPath, "utf8");
  let manifest;
  try {
    manifest = JSON.parse(text);
  } catch (cause) {
    throw new Error(`Invalid static render manifest ${path}: ${cause.message}`);
  }
  return normalizeStaticRenderManifest(manifest, manifestPath, options);
}

export function normalizeStaticRenderManifest(manifest, manifestPath = resolve("static-render.json"), options = {}) {
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new Error(`${manifestPath} must be an object.`);
  }
  const root = dirname(resolve(manifestPath));
  const defaults = manifest.defaults && typeof manifest.defaults === "object" ? manifest.defaults : {};
  const outDir = resolveOutputPath(root, options.outDir ?? defaults.outDir ?? "out/static-render");
  const image = normalizeImage({ ...(defaults.image ?? {}), ...(options.image ?? {}) });
  const camera = normalizeCamera(defaults.camera ?? {});
  const cases = normalizeCases(manifest.cases, { root, defaults, image, camera });
  return {
    ok: true,
    manifestPath: resolve(manifestPath),
    outDir,
    defaults,
    image,
    cases,
  };
}

export async function prepareStaticRenderBatch(options) {
  const batch = await readStaticRenderManifest(requireString(options, "manifest"), options);
  const outDir = options.outDir ? resolve(options.outDir) : batch.outDir;
  const selected = filterCases(batch.cases, options.caseName);
  await mkdir(outDir, { recursive: true });
  const results = [];
  for (const testCase of selected) {
    results.push(await prepareOneCase(testCase, { ...options, outDir, outDirOverride: Boolean(options.outDir) }));
  }
  return summarizeBatch("static-render-prepare", batch, results, outDir);
}

export async function exportStaticRenderBatch(options) {
  const batch = await readStaticRenderManifest(requireString(options, "manifest"), options);
  const outDir = options.outDir ? resolve(options.outDir) : batch.outDir;
  const selected = filterCases(batch.cases, options.caseName);
  await mkdir(outDir, { recursive: true });
  const results = [];
  for (const testCase of selected) {
    const prepared = await prepareOneCase(testCase, { ...options, outDir, outDirOverride: Boolean(options.outDir) });
    const imageExtension = imageExportExtension(testCase.image.format);
    const images = await exportImagesWithMmd(
      prepared.fixture,
      prepared.frames.map((frame) => ({
        frame,
        output: resolve(prepared.caseDir, `frame-${formatFrame(frame)}.${imageExtension}`),
      })),
      {
        loadWaitMs: options.loadWaitMs,
        timeoutMs: options.timeoutMs ?? prepared.fixture.timeoutMs,
        outputWidth: testCase.image.width,
        outputHeight: testCase.image.height,
        hideAxis: testCase.display.hideAxis,
        hideFloor: testCase.display.hideFloor,
        blackBackground: isBlackBackground(testCase.image.background),
      },
    );
    results.push({ ...prepared, images });
  }
  return summarizeBatch("static-render-export", batch, results, outDir);
}

async function prepareOneCase(testCase, options) {
  await access(testCase.pmx);
  if (testCase.vmd) {
    await access(testCase.vmd);
  }
  if (testCase.cameraVmd) {
    await access(testCase.cameraVmd);
  }
  const caseDir = resolve(options.outDir, sanitizeName(testCase.name));
  const project = resolve(caseDir, "scene.pmm");
  const fixturePath = resolve(caseDir, "fixture.json");
  const output = resolveOracleOutput(testCase, options, caseDir);
  await rm(caseDir, { recursive: true, force: true });
  await mkdir(caseDir, { recursive: true });

  let projectSource = "generated-pmx-vmd";
  let pmmReport = null;
  const warnings = [];
  let stagedPmx = null;
  if (testCase.templatePmm) {
    await access(testCase.templatePmm);
    await copyFile(testCase.templatePmm, project);
    await patchPmmOutputResolution(project, testCase.image);
    projectSource = "template-pmm";
    if (testCase.cameraVmd && testCase.patchTemplateCameraVmd) {
      pmmReport = await writePmmCameraVmdPatch({
        template: project,
        cameraVmd: testCase.cameraVmd,
        out: project,
      });
      await patchPmmOutputResolution(project, testCase.image);
      projectSource = "template-pmm-camera-vmd-patch";
    }
    if (testCase.vmd) {
      warnings.push("templatePmm was used as the render source; model VMD is metadata unless the template already contains that motion.");
    }
    if (testCase.cameraVmd && !testCase.patchTemplateCameraVmd) {
      warnings.push("templatePmm was used as the render source; cameraVmd is metadata unless the template already contains that camera motion.");
    }
  } else {
    stagedPmx = await stageMmdCompatibleModel(testCase.pmx, resolve(caseDir, "model.mmd-utf16.pmx"));
    if (testCase.vmd) {
      pmmReport = await writePmmFromPmxVmd({
        pmx: stagedPmx.output,
        vmd: testCase.vmd,
        cameraVmd: testCase.cameraVmd,
        out: project,
        outputWidth: testCase.image.width,
        outputHeight: testCase.image.height,
        cameraFov: testCase.camera.fov,
        camera: testCase.camera,
        selfShadow: testCase.selfShadow,
        selfShadowDistance: testCase.selfShadowDistance,
      });
      if (!testCase.cameraVmd) {
        warnings.push("PMX/VMD generated PMM uses the default PMM camera/display state; use a template PMM when the render needs a scene-specific camera setup.");
      }
    } else {
      pmmReport = await writeBasePmmFromPmx({
        pmx: stagedPmx.output,
        out: project,
        outputWidth: testCase.image.width,
        outputHeight: testCase.image.height,
        cameraFov: testCase.camera.fov,
        camera: testCase.camera,
        selfShadow: testCase.selfShadow,
        selfShadowDistance: testCase.selfShadowDistance,
      });
      projectSource = "generated-base-pmm";
    }
  }
  const fixture = {
    name: testCase.name,
    mmdVersion: "9.32-x64",
    mmdExe: options.mmdExe ?? testCase.mmdExe ?? defaultMmdExePath(),
    project,
    frames: testCase.frames,
    output,
    done: `${output}.done`,
    timeoutMs: options.timeoutMs ?? testCase.timeoutMs ?? 120000,
    dump: { bones: false, morphs: false, rigidBodies: false },
  };
  await writeFile(fixturePath, `${JSON.stringify(toPortableFixture(fixture), null, 2)}\n`, "utf8");
  return {
    ok: true,
    name: testCase.name,
    enabled: testCase.enabled,
    skipReason: testCase.skipReason,
    caseDir,
    project,
    fixturePath,
    fixture,
    output,
    frames: testCase.frames,
    projectSource,
    templatePmm: testCase.templatePmm,
    pmx: testCase.pmx,
    vmd: testCase.vmd,
    cameraVmd: testCase.cameraVmd,
    stagedPmx,
    image: testCase.image,
    camera: testCase.camera,
    display: testCase.display,
    warnings,
    pmm: pmmReport
      ? {
          mode: pmmReport.mode,
          patch: {
            ok: pmmReport.patch?.comparison?.ok,
            counts: pmmReport.patch?.comparison?.counts,
          },
          filter: pmmReport.filter,
          cameraVmdCounts: pmmReport.cameraVmdCounts,
          camera: pmmReport.camera,
        }
      : undefined,
  };
}

function normalizeCases(cases, context) {
  if (!Array.isArray(cases) || cases.length === 0) {
    throw new Error("static render cases must be a non-empty array.");
  }
  return cases.map((entry, index) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
      throw new Error(`cases[${index}] must be an object.`);
    }
    const assets = normalizeAssets(entry.assets, entry);
    const metadata = entry.metadata && typeof entry.metadata === "object" && !Array.isArray(entry.metadata) ? entry.metadata : {};
    const frames = entry.frames ?? (entry.frame === undefined ? context.defaults.frames : [entry.frame]);
    const image = normalizeImage({ ...context.image, ...(metadata.image ?? {}), ...(entry.image ?? {}) });
    const display = normalizeDisplay({ ...(context.defaults.display ?? {}), ...(metadata.display ?? {}), ...(entry.display ?? {}) });
    return {
      name: entry.name ?? `static-render-${index}`,
      enabled: (entry.enabled ?? metadata.enabled) !== false,
      skipReason: entry.skipReason ?? metadata.skipReason,
      kind: entry.kind ?? "static-render",
      pmx: resolvePath(context.root, requireString(assets, "model", `cases[${index}].assets`)),
      vmd: assets.motion ? resolvePath(context.root, assets.motion) : undefined,
      cameraVmd: assets.cameraMotion ? resolvePath(context.root, assets.cameraMotion) : undefined,
      templatePmm: assets.pmm ? resolvePath(context.root, assets.pmm) : undefined,
      patchTemplateCameraVmd: (entry.patchTemplateCameraVmd ?? metadata.patchTemplateCameraVmd) === true,
      frames: normalizeFrames(frames, `cases[${index}].frames`),
      image,
      camera: normalizeCamera({ ...context.camera, ...(metadata.camera ?? {}), ...(entry.camera ?? {}) }),
      display,
      selfShadow: (entry.selfShadow ?? metadata.selfShadow ?? context.defaults.selfShadow) === true,
      selfShadowDistance: entry.selfShadowDistance ?? metadata.selfShadowDistance ?? context.defaults.selfShadowDistance,
      oraclePath: entry.oracle?.path ? resolvePath(context.root, entry.oracle.path) : undefined,
      mmdExe: entry.mmdExe
        ? resolvePath(context.root, entry.mmdExe)
        : context.defaults.mmdExe
          ? resolvePath(context.root, context.defaults.mmdExe)
          : undefined,
      timeoutMs: entry.timeoutMs ?? context.defaults.timeoutMs,
    };
  });
}

function normalizeAssets(assets, entry) {
  const source = assets && typeof assets === "object" && !Array.isArray(assets) ? assets : {};
  return {
    model: source.model ?? entry.pmx,
    motion: source.motion ?? entry.vmd,
    cameraMotion: source.cameraMotion ?? entry.cameraVmd ?? entry.sourceCameraVmd,
    pmm: source.pmm ?? entry.templatePmm ?? entry.pmm,
  };
}

function resolveOracleOutput(testCase, options, caseDir) {
  return options.outDirOverride || !testCase.oraclePath ? resolve(caseDir, "oracle.actual.jsonl") : testCase.oraclePath;
}

function normalizeCamera(camera) {
  return {
    position: normalizeVector3(camera.position ?? [0, 10, 45], "camera.position"),
    target: normalizeVector3(camera.target ?? [0, 10, 0], "camera.target"),
    fov: normalizePositiveNumber(camera.fov ?? 30, "camera.fov"),
  };
}

function normalizeVector3(value, label) {
  if (!Array.isArray(value) || value.length !== 3 || value.some((item) => !Number.isFinite(item))) {
    throw new Error(`${label} must be a finite number array with 3 entries.`);
  }
  return value;
}

function normalizeDisplay(display) {
  return {
    hideAxis: display.hideAxis !== false,
    hideFloor: display.hideFloor !== false,
  };
}

function normalizeImage(image) {
  const format = String(image.format ?? "png").toLowerCase();
  if (!["bmp", "png"].includes(format)) {
    throw new Error(`static render image.format must be "bmp" or "png", got ${JSON.stringify(image.format)}.`);
  }
  return {
    ...image,
    width: normalizePositiveInteger(image.width ?? 640, "image.width"),
    height: normalizePositiveInteger(image.height ?? 360, "image.height"),
    format,
    cropContent: image.cropContent === true,
  };
}

function normalizePositiveInteger(value, context) {
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${context} must be a positive integer.`);
  }
  return value;
}

function normalizePositiveNumber(value, context) {
  if (!Number.isFinite(value) || value <= 0) {
    throw new Error(`${context} must be a positive number.`);
  }
  return value;
}

function imageFormatExtension(format) {
  return format === "bmp" ? "bmp" : "png";
}

function imageExportExtension(format) {
  return format === "png" ? "bmp" : imageFormatExtension(format);
}

function isBlackBackground(background) {
  return (
    Array.isArray(background) &&
    background.length >= 3 &&
    background[0] === 0 &&
    background[1] === 0 &&
    background[2] === 0
  );
}

async function stageMmdCompatibleModel(input, output) {
  if (extname(input).toLowerCase() === ".pmd") {
    return { input, output: input, converted: false, encoding: "pmd" };
  }
  return stageMmdCompatiblePmx(input, output);
}

async function patchPmmOutputResolution(path, image) {
  const bytes = await readFile(path);
  if (bytes.length < 38 || bytes.subarray(0, 24).toString("latin1") !== "Polygon Movie maker 0002") {
    return;
  }
  bytes.writeInt32LE(image.width, 30);
  bytes.writeInt32LE(image.height, 34);
  await writeFile(path, bytes);
}

function filterCases(cases, caseName) {
  if (!caseName) {
    return cases.filter((testCase) => testCase.enabled !== false);
  }
  const names = new Set(caseName.split(",").map((name) => name.trim()).filter(Boolean));
  const selected = cases.filter((testCase) => names.has(testCase.name));
  if (selected.length !== names.size) {
    const found = new Set(selected.map((testCase) => testCase.name));
    const missing = [...names].filter((name) => !found.has(name));
    throw new Error(`Unknown static render case(s): ${missing.join(", ")}`);
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
    results: results.map(({ fixture, ...result }) => result),
  };
}

function normalizeFrames(frames, context) {
  if (!Array.isArray(frames) || frames.length === 0 || frames.some((frame) => !Number.isFinite(frame) || frame < 0)) {
    throw new Error(`${context} must be a non-empty non-negative finite number array.`);
  }
  return [...new Set(frames)].sort((left, right) => left - right);
}

function formatFrame(frame) {
  return String(frame).replace(/[^0-9.-]/g, "_");
}

function resolvePath(root, path) {
  return /^[A-Za-z]:[\\/]/.test(path) ? path : resolve(root, path);
}

function resolveOutputPath(root, path) {
  return /^[A-Za-z]:[\\/]/.test(path) ? path : resolve(root, path);
}

function sanitizeName(name) {
  return String(name).replace(/[<>:"/\\|?*\x00-\x1f]/g, "_");
}

function requireString(options, key, context = "options") {
  const value = options[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${context}.${key} must be a non-empty string.`);
  }
  return value;
}
