import { readFile } from "node:fs/promises";
import { dirname, isAbsolute, resolve } from "node:path";
import { defaultMmdExePath } from "./mmd-paths.mjs";

export async function readFixture(path) {
  const text = await readFile(path, "utf8");
  let value;
  try {
    value = JSON.parse(text);
  } catch (cause) {
    throw new Error(`Invalid fixture JSON ${path}: ${cause.message}`);
  }
  return normalizeFixture(value, path);
}

export function normalizeFixture(value, path = "fixture.json") {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${path} must be an object`);
  }

  const root = dirname(resolve(path));
  const frames = normalizeFrames(value, path);

  const project = requireString(value.project, `${path}.project`);
  const output = requireString(value.output, `${path}.output`);
  const mmdExe = value.mmdExe === undefined ? defaultMmdExePath() : requireString(value.mmdExe, `${path}.mmdExe`);

  return {
    name: requireString(value.name, `${path}.name`),
    mmdVersion: value.mmdVersion === undefined ? "9.32-x64" : requireString(value.mmdVersion, `${path}.mmdVersion`),
    project: resolveFixturePath(root, project),
    projectRaw: project,
    frames,
    output: resolveFixturePath(root, output),
    outputRaw: output,
    done: resolveFixturePath(root, value.done ?? `${output}.done`),
    mmdExe: resolveFixturePath(root, mmdExe),
    mmdExeRaw: mmdExe,
    timeoutMs: value.timeoutMs === undefined ? 60000 : requirePositiveInteger(value.timeoutMs, `${path}.timeoutMs`),
    jumpFrameIntervalMs:
      value.jumpFrameIntervalMs === undefined
        ? undefined
        : requireNonNegativeInteger(value.jumpFrameIntervalMs, `${path}.jumpFrameIntervalMs`),
    jumpFrames: value.jumpFrames === undefined ? undefined : requireBoolean(value.jumpFrames, `${path}.jumpFrames`),
    playback: value.playback === undefined ? undefined : requireBoolean(value.playback, `${path}.playback`),
    framesRange: value.framesRange,
    dump: {
      bones: value.dump?.bones !== false,
      morphs: value.dump?.morphs !== false,
      camera: value.dump?.camera === true,
      cameraKeyframes: value.dump?.cameraKeyframes !== false,
      sceneParameters: value.dump?.sceneParameters === true,
      rigidBodies: value.dump?.rigidBodies === true,
    },
  };
}

function normalizeFrames(value, path) {
  if (value.frames !== undefined) {
    const frames = value.frames;
    if (!Array.isArray(frames) || frames.length === 0 || frames.some((frame) => !Number.isFinite(frame))) {
      throw new Error(`${path}.frames must be a non-empty finite number array`);
    }
    return frames;
  }

  if (value.framesRange !== undefined) {
    const range = value.framesRange;
    if (!range || typeof range !== "object" || Array.isArray(range)) {
      throw new Error(`${path}.framesRange must be an object`);
    }
    const start = requireNonNegativeInteger(range.start, `${path}.framesRange.start`);
    const end = requireNonNegativeInteger(range.end, `${path}.framesRange.end`);
    const step = range.step === undefined ? 1 : requirePositiveInteger(range.step, `${path}.framesRange.step`);
    if (end < start) {
      throw new Error(`${path}.framesRange.end must be greater than or equal to start`);
    }
    const frames = [];
    for (let frame = start; frame <= end; frame += step) {
      frames.push(frame);
    }
    return frames;
  }

  throw new Error(`${path}.frames must be a non-empty finite number array`);
}

function resolveFixturePath(root, path) {
  return isAbsolute(path) ? path : resolve(root, path);
}

function requireString(value, path) {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${path} must be a non-empty string`);
  }
  return value;
}

function requirePositiveInteger(value, path) {
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${path} must be a positive integer`);
  }
  return value;
}

function requireNonNegativeInteger(value, path) {
  if (!Number.isInteger(value) || value < 0) {
    throw new Error(`${path} must be a non-negative integer`);
  }
  return value;
}

function requireBoolean(value, path) {
  if (typeof value !== "boolean") {
    throw new Error(`${path} must be a boolean`);
  }
  return value;
}
