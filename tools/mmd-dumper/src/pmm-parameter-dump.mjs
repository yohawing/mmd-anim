import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { parsePmmDocumentKeyframes } from "./pmm-document-keyframes.mjs";

export async function writePmmParameterDump(options) {
  const pmm = resolve(requireString(options, "pmm"));
  const out = resolve(requireString(options, "out"));
  const document = parsePmmDocumentKeyframes(await readFile(pmm), { keyframeLimit: options.keyframeLimit });
  const record = createPmmParameterRecord({ pmm, document });
  await mkdir(dirname(out), { recursive: true });
  await writeFile(out, `${JSON.stringify(record)}\n`, "utf8");
  return {
    ok: true,
    mode: "pmm-parameter-dump",
    pmm,
    out,
    counts: record.counts,
  };
}

export function createPmmParameterRecord({ pmm, document }) {
  return {
    schemaVersion: 1,
    source: {
      kind: "pmm-parameter-dump",
      pmm,
      pmmVersion: document.document.version,
      parser: "MMDDumper pmm-document-keyframes",
    },
    document: {
      outputWidth: document.document.outputWidth,
      outputHeight: document.document.outputHeight,
      cameraFov: document.document.cameraFov,
      expandFlags: document.document.expandFlags,
    },
    counts: {
      models: document.counts.models,
      cameraKeyframes: document.camera.counts.cameraKeyframes,
      lightKeyframes: document.parameters.light.counts.lightKeyframes,
      accessories: document.parameters.accessories.count,
      gravityKeyframes: document.parameters.gravity.counts.gravityKeyframes,
      selfShadowKeyframes: document.parameters.selfShadow.counts.selfShadowKeyframes,
    },
    camera: document.camera,
    light: document.parameters.light,
    accessories: document.parameters.accessories,
    timeline: document.parameters.timeline,
    display: document.parameters.display,
    gravity: document.parameters.gravity,
    selfShadow: document.parameters.selfShadow,
  };
}

function requireString(options, key) {
  const value = options[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`Missing ${key}.`);
  }
  return value;
}
