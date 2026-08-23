import { access, mkdir, rm, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { readOracleJsonl } from "./jsonl.mjs";
import { defaultMmdExePath, validateFixtureInputs } from "./mmd-paths.mjs";
import { writePatchedPmmFromTemplate } from "./pmm-manifest.mjs";

export async function recordWithMmd(fixture, options = {}) {
  if (process.env.MMD_DUMPER_ALLOW_MMD_LAUNCH !== "1") {
    throw new Error("Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for an explicit local run.");
  }

  if (!options.fixturePath) {
    throw new Error("recordWithMmd requires fixturePath so the smoke runner can preserve fixture-relative paths.");
  }
  await validateFixtureInputs(fixture);

  await rm(fixture.done, { force: true });
  const maxFrame = Math.max(...fixture.frames);
  const playback = options.playback ?? fixture.playback ?? fixture.physics?.enabled === true;
  // MMD updates ExpGetFrameTime one frame ahead of the evaluated model snapshot
  // after frame edit jumps. Jump to N+1, then relabel the captured payload as N.
  const captureFrameOffset = options.captureFrameOffset ?? fixture.captureFrameOffset ?? (playback ? 0 : 1);
  const includesFrameZero = fixture.frames.some((frame) => Math.abs(frame) < 0.0001);
  const jumpFrameList = fixture.frames.map((frame) => frame + captureFrameOffset);
  const sendKeyRepeat = Math.ceil(maxFrame) + 10;
  const jumpFrames = options.jumpFrames ?? fixture.jumpFrames ?? true;
  const trigger = options.trigger ?? fixture.trigger ?? "mmdplugin";
  const shouldJumpFrames = jumpFrames && !playback;
  const playbackToleranceFrames = playback ? 2 : 0;
  const dumpFrameRange = playback && fixture.framesRange ? normalizeDumpFrameRange(fixture.framesRange) : null;
  const dumpFrameList = dumpFrameRange
    ? null
    : playback || shouldJumpFrames
    ? createDumpFrameList(jumpFrameList, { includeZero: playback && includesFrameZero, toleranceFrames: playbackToleranceFrames })
    : null;
  const effectiveTimeoutMs = playback
    ? Math.max(fixture.timeoutMs, Math.ceil(((maxFrame + captureFrameOffset) / 24) * 1000) + 120000)
    : fixture.timeoutMs;
  const sendKeyArgs = playback
    ? ["--send-key", "p", "--send-key-repeat", "1"]
    : shouldJumpFrames
    ? [
        "--jump-frames",
        jumpFrameList.map((frame) => Math.trunc(frame)).join(","),
        "--jump-frames-after-ms",
        String(options.sendKeyAfterMs ?? 3000),
        "--jump-frame-interval-ms",
        String(options.jumpFrameIntervalMs ?? fixture.jumpFrameIntervalMs ?? 1000),
      ]
    : includesFrameZero
      ? [
          "--send-key-sequence",
          [
            "right:1:40:250",
            "left:1:40:250",
            `right:${sendKeyRepeat}:40:0`,
          ].join(","),
        ]
      : ["--send-key", "right", "--send-key-repeat", String(sendKeyRepeat)];
  const scriptPath = resolve(import.meta.dirname, "..", "scripts", "mmd-first-load-smoke.mjs");
  await runSmokeRunner(scriptPath, [
    "--trigger",
    trigger,
    ...(options.acceptDialog
      ? [
          "--pre-send-key-after-ms",
          String(options.acceptDialogAfterMs ?? 1000),
          "--pre-send-key",
          "enter",
          "--pre-send-key-repeat",
          String(options.acceptDialogRepeat ?? 1),
          "--pre-send-key-interval-ms",
          String(options.acceptDialogIntervalMs ?? 100),
        ]
      : []),
    ...(options.launchArgs ?? []).flatMap((launchArg) => ["--launch-arg", launchArg]),
    ...(options.dropFile
      ? [
          "--drop-file",
          options.dropFile,
          "--drop-file-after-ms",
          String(options.dropFileAfterMs ?? 2500),
          "--drop-accept-key-after-ms",
          String(options.dropAcceptKeyAfterMs ?? 3500),
        ]
      : []),
    ...(options.captureDir ? ["--capture-dir", options.captureDir] : []),
    ...(options.captureFrames ? ["--capture-frames", options.captureFrames.join(",")] : []),
    ...(dumpFrameList ? ["--dump-frames", dumpFrameList.join(",")] : []),
    ...(dumpFrameRange ? ["--dump-frame-range", dumpFrameRange] : []),
    ...((options.cameraModeAfterMs ?? fixture.cameraModeAfterMs) !== undefined
      ? ["--camera-mode-after-ms", String(options.cameraModeAfterMs ?? fixture.cameraModeAfterMs)]
      : []),
    ...((options.keepInitialFrameZero ?? fixture.keepInitialFrameZero ?? (playback && includesFrameZero))
      ? ["--keep-initial-frame-zero"]
      : []),
    ...(options.minCaptureFiles ? ["--min-capture-files", String(options.minCaptureFiles)] : []),
    ...(shouldJumpFrames ? [] : ["--send-key-after-ms", String(options.sendKeyAfterMs ?? 3000)]),
    ...sendKeyArgs,
    "--send-key-interval-ms",
    "40",
    "--min-records",
    String(options.minRecords ?? (playback ? 1 : fixture.frames.length)),
    "--min-last-frame",
    String(Math.max(0, maxFrame + captureFrameOffset - playbackToleranceFrames)),
    "--fixture",
    options.fixturePath,
    "--timeout-ms",
    String(effectiveTimeoutMs),
    "--write-done",
  ]);
  if (options.minRecords === 0) {
    return [];
  }
  const records = await readOracleJsonl(fixture.output);
  if (playback || !shouldJumpFrames) {
    return records;
  }
  const selected = selectRecordsForFrames(records, fixture.frames, captureFrameOffset, { toleranceFrames: playbackToleranceFrames });
  await writeFile(fixture.output, `${selected.map((record) => JSON.stringify(record)).join("\n")}\n`, "utf8");
  await writeDoneFile(fixture.done, {
    fixture: fixture.name,
    output: fixture.output,
    records: selected.length,
    firstFrame: selected[0]?.frame,
    lastFrame: selected.at(-1)?.frame,
    frames: selected.map((record) => record.frame),
    filteredFromRecords: records.length,
    playback,
  });
  return selected;
}

export async function recordDirectWithMmd(options) {
  if (!options.model) {
    throw new Error("record-direct requires --model <model.pmx>.");
  }
  const template = options.template ? resolve(options.template) : undefined;
  const model = resolve(options.model);
  const motion = options.motion ? resolve(options.motion) : undefined;
  if (template) {
    await access(template);
  }
  await access(model);
  if (motion) {
    await access(motion);
  }

  const outDir = resolve(options.outDir ?? "out", "direct");
  await mkdir(outDir, { recursive: true });
  const project = options.projectOut ? resolve(options.projectOut) : resolve(outDir, "scene.pmm");
  const output = options.output ? resolve(options.output) : resolve(outDir, "oracle.actual.jsonl");
  const done = `${output}.done`;
  if (template || options.projectOut) {
    await mkdir(dirname(project), { recursive: true });
  }
  const patch = template
    ? await writePatchedPmmFromTemplate({ template, out: project, model, motion })
    : { out: model, replacements: [], manifest: null };
  const fixture = {
    name: "direct",
    mmdVersion: "9.32-x64",
    mmdExe: resolve(options.mmdExe ?? defaultMmdExePath()),
    project: template ? project : model,
    frames: options.frames,
    output,
    done,
    timeoutMs: options.timeoutMs ?? 60000,
    dump: { bones: true, morphs: true, rigidBodies: false },
  };
  const fixturePath = resolve(outDir, "fixture.json");
  await writeFile(fixturePath, `${JSON.stringify(toPortableFixture(fixture), null, 2)}\n`, "utf8");
  const records = await recordWithMmd(fixture, {
    fixturePath,
    trigger: options.trigger,
    acceptDialog: !template,
    dropFile: !template && motion ? motion : undefined,
    dropFileAfterMs: 6000,
    dropAcceptKeyAfterMs: 8000,
    sendKeyAfterMs: !template && motion ? 10000 : 3000,
  });
  return { records, fixture, patch };
}

export async function captureWithMmd(fixture, options = {}) {
  const frames = options.frames ?? fixture.frames;
  const records = await recordWithMmd(
    {
      ...fixture,
      frames,
      timeoutMs: options.timeoutMs ?? fixture.timeoutMs,
    },
    {
      fixturePath: options.fixturePath,
      captureDir: options.captureDir,
      captureFrames: frames.map((frame) => Math.trunc(frame + (options.captureFrameOffset ?? fixture.captureFrameOffset ?? 1))),
      minRecords: 0,
      sendKeyAfterMs: options.sendKeyAfterMs,
      trigger: options.trigger ?? "mmdplugin",
    },
  );
  return { records, captureDir: options.captureDir, frames };
}

export async function exportImageWithMmd(fixture, options = {}) {
  if (process.env.MMD_DUMPER_ALLOW_MMD_LAUNCH !== "1") {
    throw new Error("Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for an explicit local run.");
  }
  await validateFixtureInputs(fixture);

  const output = resolve(options.output ?? "out/mmd-export-image.bmp");
  await mkdir(dirname(output), { recursive: true });
  const scriptPath = resolve(import.meta.dirname, "..", "scripts", "mmd-export-image.ps1");
  const frame = options.frame ?? fixture.frames[0] ?? 0;
  await runPowerShell(scriptPath, [
    "-MmdExe",
    resolve(fixture.mmdExe),
    "-Project",
    resolve(fixture.project),
    "-Output",
    output,
    "-Frame",
    String(frame),
    "-LoadWaitMs",
    String(options.loadWaitMs ?? 5000),
    "-TimeoutMs",
    String(options.timeoutMs ?? fixture.timeoutMs ?? 60000),
    ...(options.outputWidth ? ["-OutputWidth", String(options.outputWidth)] : []),
    ...(options.outputHeight ? ["-OutputHeight", String(options.outputHeight)] : []),
    ...(options.hideAxis ? ["-HideAxis"] : []),
    ...(options.hideFloor ? ["-HideFloor"] : []),
    ...(options.blackBackground ? ["-BlackBackground"] : []),
  ]);
  return { output, frame };
}

export async function exportImagesWithMmd(fixture, requests, options = {}) {
  if (process.env.MMD_DUMPER_ALLOW_MMD_LAUNCH !== "1") {
    throw new Error("Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for an explicit local run.");
  }
  await validateFixtureInputs(fixture);
  if (!Array.isArray(requests) || requests.length === 0) {
    return [];
  }

  const normalized = requests.map((request) => ({
    frame: request.frame ?? fixture.frames[0] ?? 0,
    output: resolve(request.output),
  }));
  for (const request of normalized) {
    await mkdir(dirname(request.output), { recursive: true });
  }
  const batchFile = resolve(dirname(normalized[0].output), `.mmd-export-image-batch-${process.pid}-${Date.now()}.json`);
  await writeFile(batchFile, `${JSON.stringify(normalized, null, 2)}\n`, "utf8");
  const scriptPath = resolve(import.meta.dirname, "..", "scripts", "mmd-export-image.ps1");
  try {
    await runPowerShell(scriptPath, [
      "-MmdExe",
      resolve(fixture.mmdExe),
      "-Project",
      resolve(fixture.project),
      "-BatchFile",
      batchFile,
      "-LoadWaitMs",
      String(options.loadWaitMs ?? 5000),
      "-TimeoutMs",
      String(options.timeoutMs ?? fixture.timeoutMs ?? 60000),
      ...(options.outputWidth ? ["-OutputWidth", String(options.outputWidth)] : []),
      ...(options.outputHeight ? ["-OutputHeight", String(options.outputHeight)] : []),
      ...(options.hideAxis ? ["-HideAxis"] : []),
      ...(options.hideFloor ? ["-HideFloor"] : []),
      ...(options.blackBackground ? ["-BlackBackground"] : []),
    ]);
  } finally {
    await rm(batchFile, { force: true });
  }
  return normalized;
}

export async function exportAviWithMmd(fixture, options = {}) {
  if (process.env.MMD_DUMPER_ALLOW_MMD_LAUNCH !== "1") {
    throw new Error("Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for an explicit local run.");
  }
  await validateFixtureInputs(fixture);

  const output = resolve(options.output ?? "out/mmd-export.avi");
  await mkdir(dirname(output), { recursive: true });
  const scriptPath = resolve(import.meta.dirname, "..", "scripts", "mmd-export-avi.ps1");
  const startFrame = options.startFrame ?? 0;
  const endFrame = options.endFrame ?? fixture.frames.at(-1) ?? 30;
  const fps = options.fps ?? 30;
  await runPowerShell(scriptPath, [
    "-MmdExe",
    resolve(fixture.mmdExe),
    "-Project",
    resolve(fixture.project),
    "-Output",
    output,
    "-StartFrame",
    String(startFrame),
    "-EndFrame",
    String(endFrame),
    "-Fps",
    String(fps),
    "-LoadWaitMs",
    String(options.loadWaitMs ?? 5000),
    "-TimeoutMs",
    String(options.timeoutMs ?? fixture.timeoutMs ?? 120000),
  ]);
  return { output, startFrame, endFrame, fps };
}

export async function exportMp4WithMmd(fixture, options = {}) {
  if (process.env.MMD_DUMPER_ALLOW_MMD_LAUNCH !== "1") {
    throw new Error("Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for an explicit local run.");
  }
  if (!options.fixturePath) {
    throw new Error("exportMp4WithMmd requires fixturePath so the MMDPlugin capture runner can preserve fixture-relative paths.");
  }
  await validateFixtureInputs(fixture);

  const output = resolve(options.output ?? "out/mmdplugin-mp4/capture.mp4");
  const captureDir = resolve(options.captureDir ?? dirname(output));
  await mkdir(captureDir, { recursive: true });
  const fps = options.fps ?? 30;
  const minCaptureFiles = options.minCaptureFiles ?? Math.max(1, Math.ceil((options.endFrame ?? fixture.frames.at(-1) ?? 30) - (options.startFrame ?? 0) + 1));
  const scriptPath = resolve(import.meta.dirname, "..", "scripts", "mmdplugin-capture-mp4.mjs");
  await runNodeScript(scriptPath, [
    "--fixture",
    options.fixturePath,
    "--capture-dir",
    captureDir,
    "--output",
    output,
    "--fps",
    String(fps),
    "--min-capture-files",
    String(minCaptureFiles),
    "--timeout-ms",
    String(options.timeoutMs ?? fixture.timeoutMs ?? 120000),
    "--send-key-after-ms",
    String(options.sendKeyAfterMs ?? 3000),
    "--send-key",
    options.sendKey ?? "p",
    ...(options.captureEveryNFrames ? ["--capture-every-n-frames", String(options.captureEveryNFrames)] : []),
    ...(options.crf ? ["--crf", String(options.crf)] : []),
    ...(options.preset ? ["--preset", String(options.preset)] : []),
  ]);
  return { output, captureDir, fps, minCaptureFiles };
}

export function toPortableFixture(fixture) {
  return {
    name: fixture.name,
    mmdVersion: fixture.mmdVersion,
    mmdExe: fixture.mmdExe,
    project: fixture.project,
    frames: fixture.frames,
    output: fixture.output,
    timeoutMs: fixture.timeoutMs,
    ...(fixture.trigger !== undefined ? { trigger: fixture.trigger } : {}),
    ...(fixture.jumpFrameIntervalMs !== undefined ? { jumpFrameIntervalMs: fixture.jumpFrameIntervalMs } : {}),
    ...(fixture.jumpFrames !== undefined ? { jumpFrames: fixture.jumpFrames } : {}),
    ...(fixture.captureFrameOffset !== undefined ? { captureFrameOffset: fixture.captureFrameOffset } : {}),
    ...(fixture.cameraModeAfterMs !== undefined ? { cameraModeAfterMs: fixture.cameraModeAfterMs } : {}),
    ...(fixture.keepInitialFrameZero !== undefined ? { keepInitialFrameZero: fixture.keepInitialFrameZero } : {}),
    ...(fixture.playback !== undefined ? { playback: fixture.playback } : {}),
    ...(fixture.framesRange !== undefined ? { framesRange: fixture.framesRange } : {}),
    dump: fixture.dump,
    ...(fixture.physics ? { physics: fixture.physics } : {}),
  };
}

async function runSmokeRunner(scriptPath, args) {
  const child = spawn(process.execPath, [scriptPath, ...args], {
    cwd: resolve(import.meta.dirname, ".."),
    env: process.env,
    windowsHide: false,
    stdio: "inherit",
  });
  const exitCode = await new Promise((resolveExit) => {
    child.once("exit", resolveExit);
    child.once("error", () => resolveExit(1));
  });
  if (exitCode !== 0) {
    throw new Error(`MMD smoke runner failed with exit code ${exitCode}`);
  }
}

async function runPowerShell(scriptPath, args) {
  const child = spawn("pwsh.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", scriptPath, ...args], {
    cwd: resolve(import.meta.dirname, ".."),
    env: process.env,
    windowsHide: false,
    stdio: "inherit",
  });
  const exitCode = await new Promise((resolveExit) => {
    child.once("exit", resolveExit);
    child.once("error", () => resolveExit(1));
  });
  if (exitCode !== 0) {
    throw new Error(`MMD PowerShell automation failed with exit code ${exitCode}`);
  }
}

async function runNodeScript(scriptPath, args) {
  const child = spawn(process.execPath, [scriptPath, ...args], {
    cwd: resolve(import.meta.dirname, ".."),
    env: process.env,
    windowsHide: false,
    stdio: "inherit",
  });
  const exitCode = await new Promise((resolveExit) => {
    child.once("exit", resolveExit);
    child.once("error", () => resolveExit(1));
  });
  if (exitCode !== 0) {
    throw new Error(`MMD node automation failed with exit code ${exitCode}`);
  }
}

export async function writeDoneFile(path, payload = {}) {
  await writeFile(path, JSON.stringify({ ok: true, ...payload }) + "\n", "utf8");
}

function createDumpFrameList(frames, options = {}) {
  const toleranceFrames = options.toleranceFrames ?? 0;
  const dumpFrames = new Set(options.includeZero ? [0] : []);
  for (const frame of frames) {
    const center = Math.trunc(frame);
    for (let delta = -toleranceFrames; delta <= toleranceFrames; delta += 1) {
      const candidate = center + delta;
      if (candidate >= 0) {
        dumpFrames.add(candidate);
      }
    }
  }
  return [...dumpFrames].sort((left, right) => left - right);
}

function normalizeDumpFrameRange(range) {
  if (!range || typeof range !== "object" || Array.isArray(range)) {
    return null;
  }
  const start = Number(range.start);
  const end = Number(range.end);
  const step = Number(range.step ?? 1);
  if (!Number.isInteger(start) || start < 0 || !Number.isInteger(end) || end < start || !Number.isInteger(step) || step <= 0) {
    return null;
  }
  return `${start}:${end}:${step}`;
}

export function selectRecordsForFrames(records, frames, captureFrameOffset = 0, options = {}) {
  const byRoundedFrame = new Map();
  for (const record of records) {
    const roundedFrame = Math.round(record.frame);
    if (!byRoundedFrame.has(roundedFrame)) {
      byRoundedFrame.set(roundedFrame, record);
    }
  }
  const selected = [];
  const missing = [];
  for (const frame of frames) {
    const roundedFrame = Math.round(frame + captureFrameOffset);
    const exactRecord = byRoundedFrame.get(roundedFrame) ?? (Math.abs(frame) < 0.0001 ? byRoundedFrame.get(0) : undefined);
    const record = exactRecord ?? findNearestRecord(records, frame + captureFrameOffset, options.toleranceFrames ?? 0);
    if (record) {
      selected.push({ ...record, frame });
    } else {
      missing.push(frame);
    }
  }
  if (missing.length > 0) {
    throw new Error(`MMD dump did not produce requested frame(s): ${missing.join(", ")}`);
  }
  return selected;
}

function findNearestRecord(records, targetFrame, toleranceFrames) {
  if (toleranceFrames <= 0) {
    return undefined;
  }
  let best = undefined;
  let bestDiff = Number.POSITIVE_INFINITY;
  for (const record of records) {
    const diff = Math.abs(record.frame - targetFrame);
    if (diff <= toleranceFrames && diff < bestDiff) {
      best = record;
      bestDiff = diff;
    }
  }
  return best;
}
