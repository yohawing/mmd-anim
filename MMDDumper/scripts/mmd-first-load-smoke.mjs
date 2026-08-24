#!/usr/bin/env node
import { spawn } from "node:child_process";
import { copyFile, mkdir, rm, stat, rename, writeFile } from "node:fs/promises";
import { readFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { readFixture } from "../src/fixture.mjs";
import { parseOracleJsonl } from "../src/jsonl.mjs";

const root = resolve(import.meta.dirname, "..");
const options = parseArgs(process.argv.slice(2));
const fixturePath = options.fixture ?? resolve(root, "fixtures", "sample-basic", "fixture.json");
const timeoutMs = Number(options.timeoutMs ?? 15000);
const trigger = options.trigger ?? "first-load";
const minRecords = Number(options.minRecords ?? 1);
const minLastFrame = options.minLastFrame === undefined ? null : Number(options.minLastFrame);
const preSendKeyAfterMs = options.preSendKeyAfterMs === undefined ? null : Number(options.preSendKeyAfterMs);
const preSendKey = options.preSendKey ?? "enter";
const preSendKeyRepeat = Number(options.preSendKeyRepeat ?? 1);
const preSendKeyIntervalMs = Number(options.preSendKeyIntervalMs ?? 100);
const sendSpaceAfterMs = options.sendSpaceAfterMs === undefined ? null : Number(options.sendSpaceAfterMs);
const sendKey = options.sendKey ?? " ";
const sendKeyRepeat = Number(options.sendKeyRepeat ?? 1);
const sendKeyIntervalMs = Number(options.sendKeyIntervalMs ?? 100);
const sendKeySequence = options.sendKeySequence ? parseKeySequence(options.sendKeySequence) : null;
const jumpFrames = options.jumpFrames ? parseFrameList(options.jumpFrames) : null;
const jumpFramesAfterMs = options.jumpFramesAfterMs === undefined ? null : Number(options.jumpFramesAfterMs);
const jumpFrameIntervalMs = Number(options.jumpFrameIntervalMs ?? 1000);
const cameraModeAfterMs = options.cameraModeAfterMs === undefined ? null : Number(options.cameraModeAfterMs);
const launchVisible = options.showWindow || preSendKeyAfterMs !== null || sendSpaceAfterMs !== null || jumpFramesAfterMs !== null || cameraModeAfterMs !== null;
const writeDone = options.writeDone === true;
const launchArgs = options.launchArgs ?? [];
const dropFile = options.dropFile;
const dropFileAfterMs = options.dropFileAfterMs === undefined ? null : Number(options.dropFileAfterMs);
const dropAcceptKeyAfterMs = options.dropAcceptKeyAfterMs === undefined ? null : Number(options.dropAcceptKeyAfterMs);
const captureDir = options.captureDir;
const captureFrames = options.captureFrames;
const minCaptureFiles = Number(options.minCaptureFiles ?? 1);
const captureEveryNFrames = options.captureEveryNFrames;
const captureMaxFrame = options.captureMaxFrame;
const dumpFrames = options.dumpFrames;
const dumpFrameRange = options.dumpFrameRange;
const keepInitialFrameZero = options.keepInitialFrameZero === true;
const windowSnapshotAfterMs = options.windowSnapshotAfterMs === undefined ? null : Number(options.windowSnapshotAfterMs);
const windowSnapshotOut = options.windowSnapshotOut;
let preKeySendResult = null;
let preKeySendPromise = null;
let dropFileResult = null;
let dropFilePromise = null;
let dropAcceptKeyResult = null;
let dropAcceptKeyPromise = null;
let keySendResult = null;
let keySendPromise = null;
let jumpFramesResult = null;
let jumpFramesPromise = null;
let cameraModeResult = null;
let cameraModePromise = null;
let windowSnapshotResult = null;
let windowSnapshotPromise = null;

if (process.env.MMD_DUMPER_ALLOW_MMD_LAUNCH !== "1") {
  throw new Error("Refusing to launch MMD. Set MMD_DUMPER_ALLOW_MMD_LAUNCH=1 for this local smoke.");
}
if (!Number.isInteger(timeoutMs) || timeoutMs <= 0) {
  throw new Error("--timeout-ms must be a positive integer");
}
if (trigger !== "first-load" && trigger !== "gradient-fill" && trigger !== "d3d9" && trigger !== "mmdplugin") {
  throw new Error("--trigger must be first-load, gradient-fill, d3d9, or mmdplugin");
}
if (!Number.isInteger(minRecords) || minRecords < 0 || (minRecords === 0 && !captureDir)) {
  throw new Error("--min-records must be a positive integer, or 0 when --capture-dir is set");
}
if (!Number.isInteger(minCaptureFiles) || minCaptureFiles < 1) {
  throw new Error("--min-capture-files must be a positive integer");
}
if (minLastFrame !== null && (!Number.isFinite(minLastFrame) || minLastFrame < 0)) {
  throw new Error("--min-last-frame must be a non-negative finite number");
}
if (sendSpaceAfterMs !== null && (!Number.isInteger(sendSpaceAfterMs) || sendSpaceAfterMs < 0)) {
  throw new Error("--send-space-after-ms/--send-key-after-ms must be a non-negative integer");
}
if (preSendKeyAfterMs !== null && (!Number.isInteger(preSendKeyAfterMs) || preSendKeyAfterMs < 0)) {
  throw new Error("--pre-send-key-after-ms must be a non-negative integer");
}
if (typeof preSendKey !== "string" || preSendKey.length === 0) {
  throw new Error("--pre-send-key must be a non-empty string");
}
if (typeof sendKey !== "string" || sendKey.length === 0) {
  throw new Error("--send-key must be a non-empty string");
}
if (sendKeySequence !== null && sendKeySequence.length === 0) {
  throw new Error("--send-key-sequence must contain at least one step");
}
if (!Number.isInteger(preSendKeyRepeat) || preSendKeyRepeat <= 0) {
  throw new Error("--pre-send-key-repeat must be a positive integer");
}
if (!Number.isInteger(sendKeyRepeat) || sendKeyRepeat <= 0) {
  throw new Error("--send-key-repeat must be a positive integer");
}
if (!Number.isInteger(preSendKeyIntervalMs) || preSendKeyIntervalMs < 0) {
  throw new Error("--pre-send-key-interval-ms must be a non-negative integer");
}
if (!Number.isInteger(sendKeyIntervalMs) || sendKeyIntervalMs < 0) {
  throw new Error("--send-key-interval-ms must be a non-negative integer");
}
if (jumpFrames !== null && jumpFrames.length === 0) {
  throw new Error("--jump-frames must contain at least one non-negative integer");
}
if (dumpFrames !== undefined && parseFrameList(dumpFrames).length === 0) {
  throw new Error("--dump-frames must contain at least one non-negative integer");
}
if (dumpFrameRange !== undefined && !/^\d+:\d+:\d+$/u.test(String(dumpFrameRange))) {
  throw new Error("--dump-frame-range must be formatted as start:end:step");
}
if (jumpFramesAfterMs !== null && (!Number.isInteger(jumpFramesAfterMs) || jumpFramesAfterMs < 0)) {
  throw new Error("--jump-frames-after-ms must be a non-negative integer");
}
if (!Number.isInteger(jumpFrameIntervalMs) || jumpFrameIntervalMs < 0) {
  throw new Error("--jump-frame-interval-ms must be a non-negative integer");
}
if (cameraModeAfterMs !== null && (!Number.isInteger(cameraModeAfterMs) || cameraModeAfterMs < 0)) {
  throw new Error("--camera-mode-after-ms must be a non-negative integer");
}
if (dropFileAfterMs !== null && (!Number.isInteger(dropFileAfterMs) || dropFileAfterMs < 0)) {
  throw new Error("--drop-file-after-ms must be a non-negative integer");
}
if (dropAcceptKeyAfterMs !== null && (!Number.isInteger(dropAcceptKeyAfterMs) || dropAcceptKeyAfterMs < 0)) {
  throw new Error("--drop-accept-key-after-ms must be a non-negative integer");
}
if (windowSnapshotAfterMs !== null && (!Number.isInteger(windowSnapshotAfterMs) || windowSnapshotAfterMs < 0)) {
  throw new Error("--window-snapshot-after-ms must be a non-negative integer");
}

const fixture = await readFixture(fixturePath);
const skipInitialFrameZero = !keepInitialFrameZero && fixture.frames.some((frame) => Math.abs(frame) < 0.0001);
const packageDir = resolve(root, "out", "mmd-oracle-dumper-package");
const files = trigger === "mmdplugin"
  ? [
      {
        name: "mmd_oracle_plugin.dll",
        source: resolve(packageDir, "Plugin", "mmd_oracle_plugin.dll"),
        destination: resolve(dirname(fixture.mmdExe), "Plugin", "mmd_oracle_plugin.dll"),
      },
    ]
  : [
      {
        name: "mmd_oracle_dumper.dll",
        source: resolve(packageDir, "mmd_oracle_dumper.dll"),
        destination: resolve(dirname(fixture.mmdExe), "mmd_oracle_dumper.dll"),
      },
      {
        name: "MSIMG32.dll",
        source: resolve(packageDir, "MSIMG32.dll"),
        destination: resolve(dirname(fixture.mmdExe), "MSIMG32.dll"),
      },
      {
        name: "d3d9.dll",
        source: resolve(packageDir, "d3d9.dll"),
        destination: resolve(dirname(fixture.mmdExe), "d3d9.dll"),
      },
    ];

for (const file of files) {
  if (!existsSync(file.source)) {
    throw new Error(`Missing packaged native file: ${file.source}. Run pnpm -C MMDDumper native:test and native:package first.`);
  }
}
if (!existsSync(fixture.mmdExe)) {
  throw new Error(`Missing MMD executable: ${fixture.mmdExe}`);
}
if (!existsSync(fixture.project)) {
  throw new Error(`Missing MMD project: ${fixture.project}`);
}
for (const launchArg of launchArgs) {
  if (!existsSync(launchArg)) {
    throw new Error(`Missing launch argument file: ${launchArg}`);
  }
}
if (dropFile && !existsSync(dropFile)) {
  throw new Error(`Missing drop file: ${dropFile}`);
}

const backups = [];
let child = null;
try {
  await stopMmdByExecutablePath(fixture.mmdExe);
  await installFiles(files, backups);
  await rm(fixture.output, { force: true });
  await rm(fixture.done, { force: true });
  await mkdir(dirname(fixture.output), { recursive: true });
  if (captureDir) {
    await mkdir(resolve(captureDir), { recursive: true });
  }

  child = spawn(fixture.mmdExe, [fixture.project, ...launchArgs], {
    cwd: dirname(fixture.mmdExe),
    env: {
      ...process.env,
      MMD_ORACLE_DUMP_PATH: fixture.output,
      MMD_ORACLE_PROJECT_PATH: fixture.project,
      MMD_ORACLE_PROXY_LOG_PATH: `${fixture.output}.proxy.log`,
      ...(fixture.dump?.camera && !fixture.dump?.bones && !fixture.dump?.morphs ? { MMD_ORACLE_REQUIRE_MODEL: "0" } : {}),
      ...(fixture.dump?.camera && !fixture.dump?.bones && !fixture.dump?.morphs ? { MMD_ORACLE_MODEL_CHANNELS: "0" } : {}),
      ...(fixture.dump?.cameraKeyframes === false ? { MMD_ORACLE_CAMERA_KEYFRAMES: "0" } : {}),
      ...(skipInitialFrameZero ? { MMD_ORACLE_SKIP_INITIAL_FRAME_ZERO: "1" } : {}),
      ...(captureDir ? { MMD_ORACLE_CAPTURE_DIR: resolve(captureDir) } : {}),
      ...(captureFrames ? { MMD_ORACLE_CAPTURE_FRAMES: captureFrames } : {}),
      ...(dumpFrames ? { MMD_ORACLE_DUMP_FRAMES: dumpFrames } : {}),
      ...(dumpFrameRange ? { MMD_ORACLE_DUMP_FRAME_RANGE: dumpFrameRange } : {}),
      ...(captureDir && trigger === "mmdplugin" ? { MMD_ORACLE_CAPTURE_ON_MMDPLUGIN: "1" } : {}),
      ...(captureEveryNFrames ? { MMD_ORACLE_CAPTURE_EVERY_N_FRAMES: captureEveryNFrames } : {}),
      ...(captureMaxFrame ? { MMD_ORACLE_CAPTURE_MAX_FRAME: captureMaxFrame } : {}),
      ...triggerEnvironment(trigger),
    },
    stdio: "ignore",
    windowsHide: !launchVisible,
  });

  child.on("error", () => {
    // The timeout path reports a clearer smoke failure.
  });

  if (preSendKeyAfterMs !== null) {
    preKeySendPromise = scheduleKey(child.pid, preSendKeyAfterMs, preSendKey, preSendKeyRepeat, preSendKeyIntervalMs);
    preKeySendPromise.then((result) => {
      preKeySendResult = result;
      console.error(`mmd-smoke:pre-key-send ${JSON.stringify(result)}`);
    });
  }

  if (dropFile && dropFileAfterMs !== null) {
    dropFilePromise = scheduleDropFile(child.pid, dropFileAfterMs, dropFile);
    dropFilePromise.then((result) => {
      dropFileResult = result;
      console.error(`mmd-smoke:drop-file ${JSON.stringify(result)}`);
    });
  }

  if (dropAcceptKeyAfterMs !== null) {
    dropAcceptKeyPromise = scheduleKey(child.pid, dropAcceptKeyAfterMs, "enter", 1, 100);
    dropAcceptKeyPromise.then((result) => {
      dropAcceptKeyResult = result;
      console.error(`mmd-smoke:drop-accept-key ${JSON.stringify(result)}`);
    });
  }

  if (cameraModeAfterMs !== null) {
    cameraModePromise = scheduleCameraMode(child.pid, cameraModeAfterMs);
    cameraModePromise.then((result) => {
      cameraModeResult = result;
      console.error(`mmd-smoke:camera-mode ${JSON.stringify(result)}`);
    });
  }

  if (sendSpaceAfterMs !== null) {
    keySendPromise = sendKeySequence === null
      ? scheduleKey(child.pid, sendSpaceAfterMs, sendKey, sendKeyRepeat, sendKeyIntervalMs)
      : scheduleKeySequence(child.pid, sendSpaceAfterMs, sendKeySequence);
    keySendPromise.then((result) => {
      keySendResult = result;
      console.error(`mmd-smoke:key-send ${JSON.stringify(result)}`);
    });
  }

  if (jumpFramesAfterMs !== null) {
    jumpFramesPromise = scheduleFrameJumps(child.pid, jumpFramesAfterMs, jumpFrames ?? fixture.frames, jumpFrameIntervalMs);
    jumpFramesPromise.then((result) => {
      jumpFramesResult = result;
      console.error(`mmd-smoke:jump-frames ${JSON.stringify(result)}`);
    });
  }

  if (windowSnapshotAfterMs !== null) {
    windowSnapshotPromise = scheduleWindowSnapshot(child.pid, windowSnapshotAfterMs);
    windowSnapshotPromise.then((result) => {
      windowSnapshotResult = result;
      console.error(`mmd-smoke:window-snapshot ${JSON.stringify(result)}`);
      if (windowSnapshotOut) {
        (async () => {
          try {
            const target = resolve(windowSnapshotOut);
            const d = dirname(target);
            await mkdir(d, { recursive: true });
            await writeFile(target, `${JSON.stringify(result, null, 2)}\n`, "utf8");
          } catch (e) {
            console.error(`mmd-smoke:window-snapshot-out-error ${e && e.message || e}`);
          }
        })();
      }
    });
  }

  const records = await (minRecords === 0
    ? waitForCaptureOutput(resolve(captureDir), captureFrames, timeoutMs, minCaptureFiles)
    : waitForOracleOutput(fixture.output, timeoutMs, { minRecords, minLastFrame, pid: child?.pid ?? null })).catch(async (error) => {
    if (preKeySendPromise) {
      preKeySendResult = await Promise.race([
        preKeySendPromise,
        sleep(1000).then(() => preKeySendResult ?? { attempted: true, pending: true }),
      ]);
    }
    if (keySendPromise) {
      keySendResult = await Promise.race([
        keySendPromise,
        sleep(1000).then(() => keySendResult ?? { attempted: true, pending: true }),
      ]);
    }
    if (dropFilePromise) {
      dropFileResult = await Promise.race([
        dropFilePromise,
        sleep(1000).then(() => dropFileResult ?? { attempted: true, pending: true }),
      ]);
    }
    if (dropAcceptKeyPromise) {
      dropAcceptKeyResult = await Promise.race([
        dropAcceptKeyPromise,
        sleep(1000).then(() => dropAcceptKeyResult ?? { attempted: true, pending: true }),
      ]);
    }
    if (jumpFramesPromise) {
      jumpFramesResult = await Promise.race([
        jumpFramesPromise,
        sleep(1000).then(() => jumpFramesResult ?? { attempted: true, pending: true }),
      ]);
    }
    if (cameraModePromise) {
      cameraModeResult = await Promise.race([
        cameraModePromise,
        sleep(1000).then(() => cameraModeResult ?? { attempted: true, pending: true }),
      ]);
    }
    if (windowSnapshotPromise) {
      windowSnapshotResult = await Promise.race([
        windowSnapshotPromise,
        sleep(1000).then(() => windowSnapshotResult ?? { attempted: true, pending: true }),
      ]);
    }
    error.message = `${error.message}; preKeySend=${JSON.stringify(preKeySendResult)}; dropFile=${JSON.stringify(dropFileResult)}; dropAcceptKey=${JSON.stringify(dropAcceptKeyResult)}; cameraMode=${JSON.stringify(cameraModeResult)}; keySend=${JSON.stringify(keySendResult)}; jumpFrames=${JSON.stringify(jumpFramesResult)}; windowSnapshot=${JSON.stringify(windowSnapshotResult)}`;
    throw error;
  });
  if (preKeySendPromise && preKeySendResult === null) {
    preKeySendResult = await Promise.race([
      preKeySendPromise,
      sleep(5000).then(() => preKeySendResult ?? { attempted: true, pending: true }),
    ]);
  }
  if (keySendPromise && keySendResult === null) {
    keySendResult = await Promise.race([
      keySendPromise,
      sleep(5000).then(() => keySendResult ?? { attempted: true, pending: true }),
    ]);
  }
  if (dropFilePromise && dropFileResult === null) {
    dropFileResult = await Promise.race([
      dropFilePromise,
      sleep(5000).then(() => dropFileResult ?? { attempted: true, pending: true }),
    ]);
  }
  if (dropAcceptKeyPromise && dropAcceptKeyResult === null) {
    dropAcceptKeyResult = await Promise.race([
      dropAcceptKeyPromise,
      sleep(5000).then(() => dropAcceptKeyResult ?? { attempted: true, pending: true }),
    ]);
  }
  if (jumpFramesPromise && jumpFramesResult === null) {
    jumpFramesResult = await Promise.race([
      jumpFramesPromise,
      sleep(5000).then(() => jumpFramesResult ?? { attempted: true, pending: true }),
    ]);
  }
  if (cameraModePromise && cameraModeResult === null) {
    cameraModeResult = await Promise.race([
      cameraModePromise,
      sleep(5000).then(() => cameraModeResult ?? { attempted: true, pending: true }),
    ]);
  }
  if (windowSnapshotPromise && windowSnapshotResult === null) {
    windowSnapshotResult = await Promise.race([
      windowSnapshotPromise,
      sleep(5000).then(() => windowSnapshotResult ?? { attempted: true, pending: true }),
    ]);
  }
  const frames = records.map((record) => record.frame).filter((frame) => frame !== undefined);
  const maxFrame = frames.length > 0 ? Math.max(...frames) : undefined;
  const result = {
    ok: true,
    trigger,
    fixture: fixture.name,
    output: fixture.output,
    done: writeDone ? fixture.done : undefined,
    records: records.length,
    firstFrame: records[0]?.frame,
    lastFrame: maxFrame,
    frames,
    models: records[0]?.models?.length ?? 0,
    windowVisible: launchVisible,
    preKeySend: preKeySendResult,
    dropFile: dropFileResult,
    dropAcceptKey: dropAcceptKeyResult,
    cameraMode: cameraModeResult,
    keySend: keySendResult,
    jumpFrames: jumpFramesResult,
    captureDir: captureDir ? resolve(captureDir) : undefined,
    captureFrames,
  };
  if (writeDone) {
    await writeDoneFile(fixture.done, result);
  }
  console.log(
    JSON.stringify(
      result,
      null,
      2,
    ),
  );
} finally {
  await stopChild(child);
  await stopMmdByExecutablePath(fixture.mmdExe);
  await restoreFiles(backups);
}

async function writeDoneFile(path, result) {
  await writeFile(
    path,
    `${JSON.stringify({
      ok: true,
      mode: "mmd-d3d9",
      trigger: result.trigger,
      fixture: result.fixture,
      output: result.output,
      records: result.records,
      firstFrame: result.firstFrame,
      lastFrame: result.lastFrame,
    })}\n`,
    "utf8",
  );
}

function triggerEnvironment(trigger) {
  if (trigger === "first-load") {
    return {
      MMD_ORACLE_DUMP_ON_PROXY_LOAD: "1",
    };
  }
  if (trigger === "gradient-fill") {
    return {
      MMD_ORACLE_DUMP_ON_GRADIENTFILL: "1",
    };
  }
  if (trigger === "mmdplugin") {
    return {
      MMD_ORACLE_DUMP_ON_MMDPLUGIN: "1",
    };
  }
  return {
    MMD_ORACLE_DUMP_ON_D3D9: "1",
  };
}

async function installFiles(files, backups) {
  for (const file of files) {
    const backup = `${file.destination}.mmd-oracle-backup`;
    await rm(backup, { force: true });
    if (existsSync(file.destination)) {
      await rename(file.destination, backup);
      backups.push({ destination: file.destination, backup, hadOriginal: true });
    } else {
      await mkdir(dirname(file.destination), { recursive: true });
      backups.push({ destination: file.destination, backup, hadOriginal: false });
    }
    await copyFile(file.source, file.destination);
  }
}

async function restoreFiles(backups) {
  for (const entry of backups.reverse()) {
    try {
      if (entry.hadOriginal) {
        const backupExists = existsSync(entry.backup);
        if (backupExists) {
          await rmWithRetry(entry.destination);
          await rename(entry.backup, entry.destination);
        } else {
          console.error(`mmd-smoke:restore-missing-backup backup=${entry.backup} dest=${entry.destination} (hadOriginal=true; backup path missing, no safe restore action; destination left untouched to avoid deleting files)`);
        }
      } else {
        await rmWithRetry(entry.destination);
        if (entry.backup) {
          await rm(entry.backup, { force: true }).catch(() => {});
        }
      }
    } catch (err) {
      console.error(`mmd-smoke:restore-error dest=${entry.destination} backup=${entry.backup} hadOriginal=${!!entry.hadOriginal} error=${err && err.message || err}`);
    }
  }
}

async function rmWithRetry(path) {
  let lastError = null;
  for (let attempt = 0; attempt < 40; attempt += 1) {
    try {
      await rm(path, { force: true });
      return;
    } catch (error) {
      lastError = error;
      await sleep(500);
    }
  }
  throw lastError;
}

async function stopChild(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) {
    return;
  }
  child.kill();
  await new Promise((resolve) => {
    const timer = setTimeout(resolve, 5000);
    child.once("exit", () => {
      clearTimeout(timer);
      resolve();
    });
  });
  if (child.exitCode === null && child.signalCode === null && process.platform === "win32") {
    await new Promise((resolve) => {
      const taskkill = spawn("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true,
      });
      taskkill.once("exit", resolve);
      taskkill.once("error", resolve);
    });
  }
}

async function stopMmdByExecutablePath(mmdExe) {
  if (process.platform !== "win32") {
    return;
  }
  const script = [
    "$target = [System.IO.Path]::GetFullPath($args[0])",
    "function Get-TargetMmdProcessIds {",
    "  @(Get-CimInstance Win32_Process -Filter \"name = 'MikuMikuDance.exe'\" | Where-Object {",
    "    $_.ExecutablePath -and ([System.IO.Path]::GetFullPath($_.ExecutablePath) -eq $target)",
    "  } | ForEach-Object { [int]$_.ProcessId })",
    "}",
    "$ids = Get-TargetMmdProcessIds",
    "$ids | ForEach-Object { Stop-Process -Id $_ -Force -ErrorAction SilentlyContinue }",
    "for ($i = 0; $i -lt 40; $i++) {",
    "  if ((Get-TargetMmdProcessIds).Count -eq 0) { exit 0 }",
    "  Start-Sleep -Milliseconds 250",
    "}",
  ].join("; ");
  await new Promise((resolve) => {
    const powershell = spawn("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script, mmdExe], {
      stdio: "ignore",
      windowsHide: true,
    });
    powershell.once("exit", resolve);
    powershell.once("error", resolve);
  });
}

async function scheduleKey(processId, delayMs, key, repeat, intervalMs) {
  await sleep(delayMs);
  return sendKeyToProcess(processId, key, repeat, intervalMs);
}

async function scheduleKeySequence(processId, delayMs, sequence) {
  await sleep(delayMs);
  const steps = [];
  for (const step of sequence) {
    const result = await sendKeyToProcess(processId, step.key, step.repeat, step.intervalMs);
    steps.push({ ...step, result });
    if (step.afterMs > 0) {
      await sleep(step.afterMs);
    }
  }
  return {
    attempted: true,
    processId,
    sequence: steps,
    activated: steps.every((step) => step.result?.activated !== false && step.result?.code === 0),
  };
}

async function scheduleFrameJumps(processId, delayMs, frames, intervalMs) {
  await sleep(delayMs);
  return jumpFramesInProcess(processId, frames, intervalMs);
}

async function scheduleCameraMode(processId, delayMs) {
  await sleep(delayMs);
  return setCameraOperationModeInProcess(processId);
}

async function scheduleWindowSnapshot(processId, delayMs) {
  await sleep(delayMs);
  return captureProcessWindowSnapshot(processId);
}

async function scheduleDropFile(processId, delayMs, file) {
  await sleep(delayMs);
  return dropFileToProcess(processId, file);
}

async function setCameraOperationModeInProcess(processId) {
  if (process.platform !== "win32") {
    return { attempted: false, reason: "non-win32" };
  }
  if (!Number.isInteger(processId) || processId <= 0) {
    return { attempted: false, reason: "invalid-pid", processId };
  }
  const script = `
param([int]$processId)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$signature = @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MmdCameraModeWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern IntPtr GetDlgItem(IntPtr hDlg, int nIDDlgItem);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

  public static string GetTitle(IntPtr hWnd) {
    int length = GetWindowTextLength(hWnd);
    if (length <= 0) {
      return "";
    }
    StringBuilder builder = new StringBuilder(length + 1);
    GetWindowText(hWnd, builder, builder.Capacity);
    return builder.ToString();
  }

  public static IntPtr FindMainWindowForProcess(int targetProcessId) {
    IntPtr fallback = IntPtr.Zero;
    IntPtr main = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {
      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid != targetProcessId || !IsWindowVisible(hWnd)) {
        return true;
      }
      if (fallback == IntPtr.Zero) {
        fallback = hWnd;
      }
      string title = GetTitle(hWnd);
      if (title.StartsWith("MikuMikuDance")) {
        main = hWnd;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return main != IntPtr.Zero ? main : fallback;
  }
}
'@
Add-Type -TypeDefinition $signature
$handle = [IntPtr]::Zero
$title = ''
for ($i = 0; $i -lt 50; $i++) {
  $handle = [MmdCameraModeWin32]::FindMainWindowForProcess($processId)
  if ($handle -ne [IntPtr]::Zero) {
    $title = [MmdCameraModeWin32]::GetTitle($handle)
    break
  }
  Start-Sleep -Milliseconds 100
}
if ($handle -eq [IntPtr]::Zero) {
  Write-Output "not-found"
  exit 2
}
[void][MmdCameraModeWin32]::ShowWindowAsync($handle, 9)
[void][MmdCameraModeWin32]::SetForegroundWindow($handle)
Start-Sleep -Milliseconds 100
$operationCombo = [MmdCameraModeWin32]::GetDlgItem($handle, 436)
if ($operationCombo -eq [IntPtr]::Zero) {
  Write-Output "combo-not-found title=$title"
  exit 3
}
$cameraOperationIndex = 0
$comboBoxSetCurSel = 0x014E
$comboBoxGetCurSel = 0x0147
$comboBoxSelectionChanged = 1
[void][MmdCameraModeWin32]::SendMessage($operationCombo, $comboBoxSetCurSel, [IntPtr]$cameraOperationIndex, [IntPtr]::Zero)
$selected = [MmdCameraModeWin32]::SendMessage($operationCombo, $comboBoxGetCurSel, [IntPtr]::Zero, [IntPtr]::Zero).ToInt64()
$command = ($comboBoxSelectionChanged -shl 16) -bor 436
$posted = [MmdCameraModeWin32]::PostMessage($handle, 0x0111, [IntPtr]$command, $operationCombo)
Start-Sleep -Milliseconds 300
Write-Output "camera-mode title=$title combo=True selected=$selected posted=$posted"
exit 0
`;
  const command = `& { ${script} } ${processId}`;
  return await new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    const ps = spawn("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command], {
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    ps.stdout.setEncoding("utf8");
    ps.stderr.setEncoding("utf8");
    ps.stdout.on("data", (chunk) => { stdout += chunk; });
    ps.stderr.on("data", (chunk) => { stderr += chunk; });
    ps.once("exit", (code) => {
      resolve({
        attempted: true,
        processId,
        code,
        output: stdout.trim().slice(0, 500),
        errorOutput: stderr.trim().slice(0, 300),
        activated: code === 0,
      });
    });
    ps.once("error", (err) => {
      resolve({ attempted: true, processId, error: err.message, activated: false });
    });
  });
}

async function dropFileToProcess(processId, file) {
  if (process.platform !== "win32") {
    return { attempted: false, reason: "non-win32" };
  }
  const escapedFile = file.replaceAll("'", "''");
  const script = `
param([int]$processId, [string]$file)
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MmdDropWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("kernel32.dll")]
  public static extern IntPtr GlobalAlloc(uint uFlags, UIntPtr dwBytes);
  [DllImport("kernel32.dll")]
  public static extern IntPtr GlobalLock(IntPtr hMem);
  [DllImport("kernel32.dll")]
  public static extern bool GlobalUnlock(IntPtr hMem);

  public static IntPtr FindWindowForProcess(int targetProcessId) {
    IntPtr fallback = IntPtr.Zero;
    IntPtr main = IntPtr.Zero;
    IntPtr dialog = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {
      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid != targetProcessId || !IsWindowVisible(hWnd)) {
        return true;
      }
      if (fallback == IntPtr.Zero) {
        fallback = hWnd;
      }
      string title = GetTitle(hWnd);
      if (title.Length > 0) {
        if (title.StartsWith("MikuMikuDance")) {
          if (main == IntPtr.Zero) {
            main = hWnd;
          }
        } else {
          dialog = hWnd;
          return false;
        }
      }
      return true;
    }, IntPtr.Zero);
    if (dialog != IntPtr.Zero) {
      return dialog;
    }
    if (main != IntPtr.Zero) {
      return main;
    }
    return fallback;
  }

  public static IntPtr FindMainWindowForProcess(int targetProcessId) {
    IntPtr fallback = IntPtr.Zero;
    IntPtr main = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {
      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid != targetProcessId || !IsWindowVisible(hWnd)) {
        return true;
      }
      if (fallback == IntPtr.Zero) {
        fallback = hWnd;
      }
      string title = GetTitle(hWnd);
      if (title.StartsWith("MikuMikuDance")) {
        main = hWnd;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return main != IntPtr.Zero ? main : fallback;
  }

  public static string GetTitle(IntPtr hWnd) {
    int length = GetWindowTextLength(hWnd);
    if (length <= 0) {
      return "";
    }
    StringBuilder builder = new StringBuilder(length + 1);
    GetWindowText(hWnd, builder, builder.Capacity);
    return builder.ToString();
  }
}
'@
Add-Type -TypeDefinition $signature
$handle = [IntPtr]::Zero
$title = ''
for ($i = 0; $i -lt 50; $i++) {
  $handle = [MmdDropWin32]::FindWindowForProcess($processId)
  if ($handle -ne [IntPtr]::Zero) {
    $title = [MmdDropWin32]::GetTitle($handle)
    break
  }
  Start-Sleep -Milliseconds 100
}
if ($handle -eq [IntPtr]::Zero) {
  Write-Output "not-found"
  exit 2
}
$fullPath = [System.IO.Path]::GetFullPath($file)
$pathBytes = [System.Text.Encoding]::Unicode.GetBytes($fullPath + [char]0 + [char]0)
$headerSize = 20
$totalSize = $headerSize + $pathBytes.Length
$hDrop = [MmdDropWin32]::GlobalAlloc(0x0042, [UIntPtr]::new([uint64]$totalSize))
if ($hDrop -eq [IntPtr]::Zero) {
  Write-Output "global-alloc-failed title=$title"
  exit 3
}
$ptr = [MmdDropWin32]::GlobalLock($hDrop)
if ($ptr -eq [IntPtr]::Zero) {
  Write-Output "global-lock-failed title=$title"
  exit 4
}
$bytes = New-Object byte[] $totalSize
[BitConverter]::GetBytes([uint32]$headerSize).CopyTo($bytes, 0)
[BitConverter]::GetBytes([uint32]1).CopyTo($bytes, 16)
$pathBytes.CopyTo($bytes, $headerSize)
[Runtime.InteropServices.Marshal]::Copy($bytes, 0, $ptr, $bytes.Length)
[MmdDropWin32]::GlobalUnlock($hDrop) | Out-Null
$ok = [MmdDropWin32]::PostMessage($handle, 0x0233, $hDrop, [IntPtr]::Zero)
Write-Output "drop-posted title=$title ok=$ok file=$fullPath"
if ($ok) { exit 0 }
exit 5
`;
  const command = `& { ${script} } ${processId} '${escapedFile}'`;
  return await new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    const powershell = spawn("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command], {
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    powershell.stdout.setEncoding("utf8");
    powershell.stderr.setEncoding("utf8");
    powershell.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    powershell.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    powershell.once("exit", (code) => {
      resolve({ attempted: true, processId, file, posted: code === 0, code, output: stdout.trim(), errorOutput: stderr.trim() });
    });
    powershell.once("error", (error) => {
      resolve({ attempted: true, processId, file, posted: false, error: error.message });
    });
  });
}

async function jumpFramesInProcess(processId, frames, intervalMs) {
  if (process.platform !== "win32") {
    return { attempted: false, reason: "non-win32" };
  }
  const frameList = frames.map((frame) => Math.max(0, Math.trunc(frame))).join(",");
  const script = `
param([int]$processId, [string]$frameCsv, [int]$intervalMs)
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MmdSmokeFrameWin32 {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern IntPtr GetDlgItem(IntPtr hDlg, int nIDDlgItem);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, string lParam);
  [DllImport("user32.dll")]
  public static extern IntPtr SetFocus(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern bool PostMessage(IntPtr hWnd, uint msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern uint MapVirtualKey(uint uCode, uint uMapType);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowTextLength(IntPtr hWnd);

  public static IntPtr FindWindowForProcess(int targetProcessId) {
    IntPtr fallback = IntPtr.Zero;
    IntPtr main = IntPtr.Zero;
    IntPtr dialog = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {
      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid != targetProcessId || !IsWindowVisible(hWnd)) {
        return true;
      }
      if (fallback == IntPtr.Zero) {
        fallback = hWnd;
      }
      string title = GetTitle(hWnd);
      if (title.Length > 0) {
        if (title.StartsWith("MikuMikuDance")) {
          if (main == IntPtr.Zero) {
            main = hWnd;
          }
        } else {
          dialog = hWnd;
          return false;
        }
      }
      return true;
    }, IntPtr.Zero);
    if (dialog != IntPtr.Zero) {
      return dialog;
    }
    if (main != IntPtr.Zero) {
      return main;
    }
    return fallback;
  }

  public static IntPtr FindMainWindowForProcess(int targetProcessId) {
    IntPtr fallback = IntPtr.Zero;
    IntPtr main = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {
      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid != targetProcessId || !IsWindowVisible(hWnd)) {
        return true;
      }
      if (fallback == IntPtr.Zero) {
        fallback = hWnd;
      }
      string title = GetTitle(hWnd);
      if (title.StartsWith("MikuMikuDance")) {
        main = hWnd;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return main != IntPtr.Zero ? main : fallback;
  }

  public static string GetTitle(IntPtr hWnd) {
    int length = GetWindowTextLength(hWnd);
    if (length <= 0) {
      return "";
    }
    StringBuilder builder = new StringBuilder(length + 1);
    GetWindowText(hWnd, builder, builder.Capacity);
    return builder.ToString();
  }
}
'@
Add-Type -TypeDefinition $signature
function ConvertTo-SignedIntPtr([uint32]$value) {
  return [IntPtr]([BitConverter]::ToInt32([BitConverter]::GetBytes($value), 0))
}
function Send-Enter([IntPtr]$handle) {
  $vk = 0x0d
  $scan = [MmdSmokeFrameWin32]::MapVirtualKey([uint32]$vk, 0)
  $downLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16)))
  $upLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16) -bor (1 -shl 30) -bor (1 -shl 31)))
  $vkPtr = [UIntPtr]::new([uint64]$vk)
  [MmdSmokeFrameWin32]::PostMessage($handle, 0x0100, $vkPtr, $downLParam) | Out-Null
  Start-Sleep -Milliseconds 20
  [MmdSmokeFrameWin32]::PostMessage($handle, 0x0101, $vkPtr, $upLParam) | Out-Null
}
$frames = $frameCsv.Split(',') | Where-Object { $_.Length -gt 0 }
$mainWindow = [IntPtr]::Zero
$frameEdit = [IntPtr]::Zero
for ($i = 0; $i -lt 100; $i++) {
  $proc = Get-Process -Id $processId -ErrorAction SilentlyContinue
  if ($null -eq $proc) { break }
  if ($proc.MainWindowHandle -ne 0) {
    $mainWindow = $proc.MainWindowHandle
    $title = [MmdSmokeFrameWin32]::GetTitle($mainWindow)
    if (-not $title.StartsWith('MikuMikuDance')) {
      $mainWindow = [MmdSmokeFrameWin32]::FindMainWindowForProcess($processId)
    }
  } else {
    $mainWindow = [MmdSmokeFrameWin32]::FindMainWindowForProcess($processId)
  }
  if ($mainWindow -ne [IntPtr]::Zero) {
    $title = [MmdSmokeFrameWin32]::GetTitle($mainWindow)
    if ($title.StartsWith('MikuMikuDance')) {
      $frameEdit = [MmdSmokeFrameWin32]::GetDlgItem($mainWindow, 417)
      if ($frameEdit -ne [IntPtr]::Zero) { break }
    }
  }
  Start-Sleep -Milliseconds 100
}
if ($mainWindow -eq [IntPtr]::Zero) {
  Write-Output "window-not-found"
  exit 2
}
if ($frameEdit -eq [IntPtr]::Zero) {
  Write-Output "frame-edit-not-found"
  exit 3
}
[MmdSmokeFrameWin32]::ShowWindowAsync($mainWindow, 9) | Out-Null
[MmdSmokeFrameWin32]::SetForegroundWindow($mainWindow) | Out-Null
Start-Sleep -Milliseconds 200
$applied = 0
foreach ($frame in $frames) {
  [MmdSmokeFrameWin32]::SetFocus($frameEdit) | Out-Null
  [MmdSmokeFrameWin32]::SendMessage($frameEdit, 0x000C, [IntPtr]::Zero, [string]$frame) | Out-Null
  Start-Sleep -Milliseconds 80
  Send-Enter $frameEdit
  Send-Enter $mainWindow
  $applied++
  if ($intervalMs -gt 0) { Start-Sleep -Milliseconds $intervalMs }
}
Write-Output "applied=$applied frames=$frameCsv intervalMs=$intervalMs"
exit 0
`;
  const command = `& { ${script} } ${processId} '${frameList}' ${intervalMs}`;
  return await new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    const powershell = spawn("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command], {
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    });
    powershell.stdout.setEncoding("utf8");
    powershell.stderr.setEncoding("utf8");
    powershell.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    powershell.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    powershell.once("exit", (code) => {
      resolve({ attempted: true, processId, frames, applied: code === 0, code, output: stdout.trim(), errorOutput: stderr.trim() });
    });
    powershell.once("error", (error) => {
      resolve({ attempted: true, processId, frames, applied: false, error: error.message });
    });
  });
}

async function sendKeyToProcess(processId, key, repeat = 1, intervalMs = 0) {
  if (process.platform !== "win32") {
    return { attempted: false, reason: "non-win32" };
  }
  const escapedKey = key.replaceAll("'", "''");
  const script = `
param([int]$processId, [string]$sendKey, [int]$repeat, [int]$intervalMs)
$ErrorActionPreference = 'Stop'
$signature = @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MmdSmokeWin32 {
  [StructLayout(LayoutKind.Sequential)]
  public struct RECT {
    public int Left;
    public int Top;
    public int Right;
    public int Bottom;
  }

  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll")]
  public static extern bool ShowWindowAsync(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")]
  public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern bool PostMessage(IntPtr hWnd, uint msg, UIntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern IntPtr SendMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern IntPtr GetDlgItem(IntPtr hDlg, int nIDDlgItem);
  [DllImport("user32.dll")]
  public static extern uint MapVirtualKey(uint uCode, uint uMapType);
  [DllImport("user32.dll")]
  public static extern void keybd_event(byte bVk, byte bScan, uint dwFlags, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")]
  public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
  [DllImport("user32.dll")]
  public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")]
  public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);

  public static IntPtr FindWindowForProcess(int targetProcessId) {
    IntPtr fallback = IntPtr.Zero;
    IntPtr best = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {
      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid != targetProcessId || !IsWindowVisible(hWnd)) {
        return true;
      }
      if (fallback == IntPtr.Zero) {
        fallback = hWnd;
      }
      if (GetWindowTextLength(hWnd) > 0) {
        best = hWnd;
        return false;
      }
      return true;
    }, IntPtr.Zero);
    return best != IntPtr.Zero ? best : fallback;
  }

  public static string GetTitle(IntPtr hWnd) {
    int length = GetWindowTextLength(hWnd);
    if (length <= 0) {
      return "";
    }
    StringBuilder builder = new StringBuilder(length + 1);
    GetWindowText(hWnd, builder, builder.Capacity);
    return builder.ToString();
  }

  public static bool ClickWindowCenter(IntPtr hWnd) {
    RECT rect;
    if (!GetWindowRect(hWnd, out rect)) {
      return false;
    }
    int x = rect.Left + Math.Max(1, (rect.Right - rect.Left) / 2);
    int y = rect.Top + Math.Max(1, (rect.Bottom - rect.Top) / 2);
    if (!SetCursorPos(x, y)) {
      return false;
    }
    mouse_event(0x0002, 0, 0, 0, UIntPtr.Zero);
    mouse_event(0x0004, 0, 0, 0, UIntPtr.Zero);
    return true;
  }
}
'@
Add-Type -TypeDefinition $signature
function ConvertTo-SignedIntPtr([uint32]$value) {
  return [IntPtr]([BitConverter]::ToInt32([BitConverter]::GetBytes($value), 0))
}
$shell = New-Object -ComObject WScript.Shell
$activated = $false
$title = ''
$handle = [IntPtr]::Zero
for ($i = 0; $i -lt 50; $i++) {
  $proc = Get-Process -Id $processId -ErrorAction SilentlyContinue
  if ($null -eq $proc) { break }
  $title = $proc.MainWindowTitle
  if ($proc.MainWindowHandle -ne 0 -or [MmdSmokeWin32]::FindWindowForProcess($processId) -ne [IntPtr]::Zero) {
    if ($proc.MainWindowHandle -ne 0) {
      $handle = $proc.MainWindowHandle
    } else {
      $handle = [MmdSmokeWin32]::FindWindowForProcess($processId)
      $title = [MmdSmokeWin32]::GetTitle($handle)
    }
    [MmdSmokeWin32]::ShowWindowAsync($handle, 9) | Out-Null
    [MmdSmokeWin32]::SetForegroundWindow($handle) | Out-Null
    Start-Sleep -Milliseconds 200
    $activated = $shell.AppActivate($processId)
    if (-not $activated -and $title.Length -gt 0) { $activated = $shell.AppActivate($title) }
    break
  }
  Start-Sleep -Milliseconds 100
}
if ($handle -ne [IntPtr]::Zero) {
  $normalizedKey = $sendKey.ToLowerInvariant()
  if ($normalizedKey -eq 'right') {
    $vk = 0x27
  } elseif ($normalizedKey -eq 'enter') {
    $vk = 0x0d
  } elseif ($normalizedKey -eq 'space') {
    $vk = 0x20
  } elseif ($normalizedKey -eq 'left') {
    $vk = 0x25
  } elseif ($normalizedKey -eq 'home') {
    $vk = 0x24
  } elseif ($normalizedKey -eq 'end') {
    $vk = 0x23
  } elseif ($sendKey.Length -eq 1) {
    $char = [char]$sendKey[0]
    if ($char -eq ' ') {
      $vk = 0x20
    } else {
      $vk = [int][char]::ToUpperInvariant($char)
    }
  } else {
    $vk = 0
  }
  if ($vk -ne 0) {
  $scan = [MmdSmokeWin32]::MapVirtualKey([uint32]$vk, 0)
  $downLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16)))
  $upLParam = ConvertTo-SignedIntPtr ([uint32](1 -bor ($scan -shl 16) -bor (1 -shl 30) -bor (1 -shl 31)))
  $vkPtr = [UIntPtr]::new([uint64]$vk)
  $postedDown = 0
  $postedUp = 0
  $clickedOk = 0
  $clickedMain = 0
  if ($title.StartsWith('MikuMikuDance')) {
    [MmdSmokeWin32]::SetForegroundWindow($handle) | Out-Null
    Start-Sleep -Milliseconds 100
    if ([MmdSmokeWin32]::ClickWindowCenter($handle)) {
      $clickedMain = 1
      Start-Sleep -Milliseconds 100
    }
  }
  for ($n = 0; $n -lt $repeat; $n++) {
    if ($normalizedKey -eq 'enter' -and -not $title.StartsWith('MikuMikuDance')) {
      $okButton = [MmdSmokeWin32]::GetDlgItem($handle, 1)
      if ($okButton -ne [IntPtr]::Zero) {
        [MmdSmokeWin32]::SendMessage($okButton, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
        $clickedOk++
      }
      if ([MmdSmokeWin32]::PostMessage($handle, 0x0111, [UIntPtr]::new(1), [IntPtr]::Zero)) {
        $clickedOk++
      }
    }
    [MmdSmokeWin32]::keybd_event([byte]$vk, [byte]$scan, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 20
    [MmdSmokeWin32]::keybd_event([byte]$vk, [byte]$scan, 2, [UIntPtr]::Zero)
    $downOk = [MmdSmokeWin32]::PostMessage($handle, 0x0100, $vkPtr, $downLParam)
    Start-Sleep -Milliseconds 20
    $upOk = [MmdSmokeWin32]::PostMessage($handle, 0x0101, $vkPtr, $upLParam)
    if ($downOk) { $postedDown++ }
    if ($upOk) { $postedUp++ }
    if ($n + 1 -lt $repeat -and $intervalMs -gt 0) { Start-Sleep -Milliseconds $intervalMs }
  }
  Write-Output "keybd-event-and-posted title=$title activated=$activated clickedMain=$clickedMain down=$postedDown up=$postedUp clickedOk=$clickedOk vk=$vk scan=$scan repeat=$repeat intervalMs=$intervalMs"
  if (($postedDown -gt 0 -and $postedUp -gt 0) -or $clickedOk -gt 0) { exit 0 }
  }
}
if ($activated) {
  Start-Sleep -Milliseconds 200
  $shell.SendKeys($sendKey)
  Write-Output "activated title=$title"
  exit 0
}
Write-Output "not-activated title=$title"
exit 2
`;
  const command = `& { ${script} } ${processId} '${escapedKey}' ${repeat} ${intervalMs}`;
  return await new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    const powershell = spawn("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command], {
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    powershell.stdout.setEncoding("utf8");
    powershell.stderr.setEncoding("utf8");
    powershell.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    powershell.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    powershell.once("exit", (code) => {
      resolve({ attempted: true, processId, key, activated: code === 0, code, output: stdout.trim(), errorOutput: stderr.trim() });
    });
    powershell.once("error", (error) => {
      resolve({ attempted: true, processId, key, activated: false, error: error.message });
    });
  });
}

async function waitForOracleOutput(path, timeoutMs, conditions) {
  const startedAt = Date.now();
  let lastError = null;
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const stats = await stat(path);
      if (stats.size > 0) {
        const records = await readCompleteOracleJsonl(path);
        const frames = records.map((record) => record.frame).filter((frame) => Number.isFinite(frame));
        const maxFrame = frames.length > 0 ? Math.max(...frames) : undefined;
        const hasMinRecords = records.length >= conditions.minRecords;
        const hasMinLastFrame = conditions.minLastFrame === null || (maxFrame !== undefined && maxFrame + 0.001 >= conditions.minLastFrame);
        if (hasMinRecords && hasMinLastFrame) {
          return records;
        }
      }
    } catch (error) {
      if (error.code !== "ENOENT") {
        lastError = error;
      }
    }
    await sleep(250);
  }
  const frameCondition = conditions.minLastFrame === null ? "" : ` and last frame >= ${conditions.minLastFrame}`;
  let msg = `Timed out waiting for ${conditions.minRecords} MMD oracle record(s)${frameCondition}: ${path}${lastError ? ` (${lastError.message})` : ""}`;
  const pid = conditions && conditions.pid;
  if (pid) {
    try {
      const snap = await captureProcessWindowSnapshot(pid);
      if (snap && snap.attempted) {
        msg += `; mmdWindow=${JSON.stringify(snap)}`;
      }
    } catch (snapErr) {
      msg += `; mmdWindow-error=${String(snapErr && snapErr.message || snapErr)}`;
    }
  }
  throw new Error(msg);
}

async function waitForCaptureOutput(captureDir, captureFrames, timeoutMs, minCaptureFiles) {
  const requestedFrames = captureFrames ? parseCaptureFrameList(captureFrames) : [];
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (requestedFrames.length > 0) {
      const files = [];
      for (const frame of requestedFrames) {
        const path = resolve(captureDir, `frame_${String(frame).padStart(6, "0")}.bmp`);
        try {
          const stats = await stat(path);
          if (stats.size > 1024 * 1024) {
            files.push(path);
          }
        } catch {
          // Retry until timeout.
        }
      }
      if (files.length >= requestedFrames.length) {
        return files.map((file) => ({ capture: file }));
      }
    } else {
      const files = await listCaptureBmpFiles(captureDir);
      if (files.length >= minCaptureFiles) {
        return files.map((file) => ({ capture: file }));
      }
    }
    await sleep(250);
  }
  const target = requestedFrames.length > 0 ? requestedFrames.length : minCaptureFiles;
  throw new Error(`Timed out waiting for ${target} MMD capture file(s): ${captureDir}`);
}

async function captureProcessWindowSnapshot(processId) {
  if (process.platform !== "win32") {
    return { attempted: false, reason: "non-win32" };
  }
  if (!Number.isInteger(processId) || processId <= 0) {
    return { attempted: false, reason: "invalid-pid", processId };
  }
  const script = `
param([int]$processId)
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$signature = @'
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class MmdWindowSnap {
  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern bool EnumChildWindows(IntPtr hWndParent, EnumWindowsProc lpEnumFunc, IntPtr lParam);
  [DllImport("user32.dll")]
  public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  [DllImport("user32.dll")]
  public static extern bool IsWindowVisible(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetWindowTextLength(IntPtr hWnd);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)]
  public static extern int GetClassName(IntPtr hWnd, StringBuilder lpClassName, int nMaxCount);

  public static string GetTitle(IntPtr hWnd) {
    int len = GetWindowTextLength(hWnd);
    if (len <= 0) return "";
    var b = new StringBuilder(len + 1);
    GetWindowText(hWnd, b, b.Capacity);
    return b.ToString();
  }
  public static string GetCls(IntPtr hWnd) {
    var b = new StringBuilder(256);
    if (GetClassName(hWnd, b, b.Capacity) > 0) return b.ToString();
    return "";
  }
  public static List<object> GetChildEntries(IntPtr hWnd, int maxItems, int maxLen) {
    var list = new List<object>();
    EnumChildWindows(hWnd, delegate(IntPtr child, IntPtr l) {
      if (list.Count >= maxItems) return false;
      bool vis = IsWindowVisible(child);
      string t = GetTitle(child);
      string c = GetCls(child);
      bool include = false;
      if (t.Length > 0) include = true;
      else if (c.Length > 0 && vis) include = true;
      if (include) {
        if (t.Length > maxLen) t = t.Substring(0, maxLen) + "...";
        list.Add(new { title = t, @class = c, visible = vis });
      }
      return true;
    }, IntPtr.Zero);
    return list;
  }

  public static IntPtr FindWindowForProcess(int targetProcessId) {
    IntPtr fallback = IntPtr.Zero;
    IntPtr main = IntPtr.Zero;
    IntPtr dialog = IntPtr.Zero;
    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {
      uint pid;
      GetWindowThreadProcessId(hWnd, out pid);
      if (pid != targetProcessId) {
        return true;
      }
      if (fallback == IntPtr.Zero) {
        fallback = hWnd;
      }
      string title = GetTitle(hWnd);
      if (title.Length > 0) {
        if (title.StartsWith("MikuMikuDance")) {
          if (main == IntPtr.Zero) {
            main = hWnd;
          }
        } else {
          dialog = hWnd;
          return false;
        }
      }
      return true;
    }, IntPtr.Zero);
    if (dialog != IntPtr.Zero) return dialog;
    if (main != IntPtr.Zero) return main;
    return fallback;
  }
}
'@
Add-Type -TypeDefinition $signature
$results = @()
$seen = @{}
[MmdWindowSnap]::EnumWindows({
  param($hWnd, $l)
  [uint32]$pid = 0
  [MmdWindowSnap]::GetWindowThreadProcessId($hWnd, [ref]$pid) | Out-Null
  if ($pid -ne $processId) { return $true }
  $title = [MmdWindowSnap]::GetTitle($hWnd)
  $cls = [MmdWindowSnap]::GetCls($hWnd)
  $key = "$($hWnd)|$title|$cls"
  if (-not $seen.ContainsKey($key)) {
    $seen[$key] = $true
    $ch = [MmdWindowSnap]::GetChildEntries($hWnd, 6, 80)
    $props = @{ title = $title; class = $cls; visible = [MmdWindowSnap]::IsWindowVisible($hWnd) }
    if ($ch -and $ch.Count -gt 0) { $props.children = $ch }
    $results += [pscustomobject]$props
  }
  return $true
}, [IntPtr]::Zero) | Out-Null
if ($results.Count -eq 0) {
  $h = [MmdWindowSnap]::FindWindowForProcess($processId)
  if ($h -ne [IntPtr]::Zero) {
    $title = [MmdWindowSnap]::GetTitle($h)
    $cls = [MmdWindowSnap]::GetCls($h)
    $visible = $false
    try { $visible = [MmdWindowSnap]::IsWindowVisible($h) } catch {}
    $ch = [MmdWindowSnap]::GetChildEntries($h, 6, 80)
    $props = @{ title = $title; class = $cls; visible = $visible }
    if ($ch -and $ch.Count -gt 0) { $props.children = $ch }
    $results += [pscustomobject]$props
  }
}
if ($results.Count -eq 0) {
  $proc = Get-Process -Id $processId -ErrorAction SilentlyContinue
  if ($proc -and $proc.MainWindowHandle -and $proc.MainWindowHandle -ne 0) {
    $h = $proc.MainWindowHandle
    $title = $proc.MainWindowTitle
    $cls = ''
    try { $cls = [MmdWindowSnap]::GetCls($h) } catch {}
    $visible = $false
    try { $visible = [MmdWindowSnap]::IsWindowVisible($h) } catch {}
    $ch = [MmdWindowSnap]::GetChildEntries($h, 6, 80)
    $props = @{ title = $title; class = $cls; visible = $visible }
    if ($ch -and $ch.Count -gt 0) { $props.children = $ch }
    $results += [pscustomobject]$props
  }
}
$json = ConvertTo-Json -InputObject @($results) -Depth 4 -Compress
if (-not $json) { $json = '[]' }
Write-Output "SNAPSHOT_JSON:$json"
exit 0
`;
  const command = `& { ${script} } ${processId}`;
  return await new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    const ps = spawn("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", command], {
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    });
    ps.stdout.setEncoding("utf8");
    ps.stderr.setEncoding("utf8");
    ps.stdout.on("data", (c) => { stdout += c; });
    ps.stderr.on("data", (c) => { stderr += c; });
    ps.once("exit", (code) => {
      let snap = null;
      const m = stdout.match(/SNAPSHOT_JSON:(.*)/s);
      if (m) {
        try {
          snap = JSON.parse(m[1].trim());
        } catch {}
      }
      resolve({
        attempted: true,
        processId,
        code,
        windows: snap || [],
        output: stdout.trim().slice(0, 500),
        errorOutput: stderr.trim().slice(0, 300),
      });
    });
    ps.once("error", (err) => {
      resolve({ attempted: true, processId, error: err.message });
    });
  });
}

async function listCaptureBmpFiles(captureDir) {
  const { readdir } = await import("node:fs/promises");
  try {
    const entries = await readdir(captureDir, { withFileTypes: true });
    const files = [];
    for (const entry of entries) {
      if (!entry.isFile() || !/^frame_\d+\.bmp$/u.test(entry.name)) {
        continue;
      }
      const path = resolve(captureDir, entry.name);
      try {
        const stats = await stat(path);
        if (stats.size > 1024 * 1024) {
          files.push(path);
        }
      } catch {
        // Ignore files still being written.
      }
    }
    return files.sort();
  } catch {
    return [];
  }
}

function parseCaptureFrameList(value) {
  return parseFrameList(value);
}

function parseFrameList(value) {
  return String(value ?? "")
    .split(",")
    .map((part) => Number(part.trim()))
    .filter((frame) => Number.isInteger(frame) && frame >= 0);
}

async function readCompleteOracleJsonl(path) {
  const text = await readFile(path, "utf8");
  const completeText = text.endsWith("\n") ? text : text.replace(/\r?\n[^\r\n]*$/u, "\n");
  return parseOracleJsonl(completeText, path);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function parseArgs(args) {
  const parsed = {};
  const normalized = args.filter((arg) => arg !== "--");
  for (let i = 0; i < normalized.length; i += 1) {
    const token = normalized[i];
    if (token === "--fixture") {
      parsed.fixture = normalized[++i];
    } else if (token === "--launch-arg") {
      parsed.launchArgs ??= [];
      parsed.launchArgs.push(normalized[++i]);
    } else if (token === "--drop-file") {
      parsed.dropFile = normalized[++i];
    } else if (token === "--drop-file-after-ms") {
      parsed.dropFileAfterMs = normalized[++i];
    } else if (token === "--drop-accept-key-after-ms") {
      parsed.dropAcceptKeyAfterMs = normalized[++i];
    } else if (token === "--capture-dir") {
      parsed.captureDir = normalized[++i];
    } else if (token === "--capture-frames") {
      parsed.captureFrames = normalized[++i];
    } else if (token === "--dump-frames") {
      parsed.dumpFrames = normalized[++i];
    } else if (token === "--dump-frame-range") {
      parsed.dumpFrameRange = normalized[++i];
    } else if (token === "--keep-initial-frame-zero") {
      parsed.keepInitialFrameZero = true;
    } else if (token === "--min-capture-files") {
      parsed.minCaptureFiles = normalized[++i];
    } else if (token === "--capture-every-n-frames") {
      parsed.captureEveryNFrames = normalized[++i];
    } else if (token === "--capture-max-frame") {
      parsed.captureMaxFrame = normalized[++i];
    } else if (token === "--timeout-ms") {
      parsed.timeoutMs = normalized[++i];
    } else if (token === "--min-records") {
      parsed.minRecords = normalized[++i];
    } else if (token === "--min-last-frame") {
      parsed.minLastFrame = normalized[++i];
    } else if (token === "--pre-send-key-after-ms") {
      parsed.preSendKeyAfterMs = normalized[++i];
    } else if (token === "--pre-send-key") {
      parsed.preSendKey = normalized[++i];
    } else if (token === "--pre-send-key-repeat") {
      parsed.preSendKeyRepeat = normalized[++i];
    } else if (token === "--pre-send-key-interval-ms") {
      parsed.preSendKeyIntervalMs = normalized[++i];
    } else if (token === "--send-space-after-ms" || token === "--send-key-after-ms") {
      parsed.sendSpaceAfterMs = normalized[++i];
    } else if (token === "--send-key") {
      parsed.sendKey = normalized[++i];
    } else if (token === "--send-key-sequence") {
      parsed.sendKeySequence = normalized[++i];
    } else if (token === "--send-key-repeat") {
      parsed.sendKeyRepeat = normalized[++i];
    } else if (token === "--send-key-interval-ms") {
      parsed.sendKeyIntervalMs = normalized[++i];
    } else if (token === "--jump-frames") {
      parsed.jumpFrames = normalized[++i];
    } else if (token === "--jump-frames-after-ms") {
      parsed.jumpFramesAfterMs = normalized[++i];
    } else if (token === "--jump-frame-interval-ms") {
      parsed.jumpFrameIntervalMs = normalized[++i];
    } else if (token === "--camera-mode-after-ms") {
      parsed.cameraModeAfterMs = normalized[++i];
    } else if (token === "--window-snapshot-after-ms") {
      parsed.windowSnapshotAfterMs = normalized[++i];
    } else if (token === "--window-snapshot-out") {
      parsed.windowSnapshotOut = normalized[++i];
    } else if (token === "--show-window") {
      parsed.showWindow = true;
    } else if (token === "--write-done") {
      parsed.writeDone = true;
    } else if (token === "--trigger") {
      parsed.trigger = normalized[++i];
    } else {
      throw new Error(`Unknown argument: ${token}`);
    }
  }
  return parsed;
}

function parseKeySequence(value) {
  if (typeof value !== "string") {
    return [];
  }
  return value
    .split(",")
    .map((rawStep) => rawStep.trim())
    .filter(Boolean)
    .map((rawStep) => {
      const [key, repeat = "1", intervalMs = "100", afterMs = "0"] = rawStep.split(":");
      if (!key) {
        throw new Error(`Invalid --send-key-sequence step: ${rawStep}`);
      }
      const parsed = {
        key,
        repeat: Number(repeat),
        intervalMs: Number(intervalMs),
        afterMs: Number(afterMs),
      };
      if (!Number.isInteger(parsed.repeat) || parsed.repeat <= 0) {
        throw new Error(`Invalid repeat in --send-key-sequence step: ${rawStep}`);
      }
      if (!Number.isInteger(parsed.intervalMs) || parsed.intervalMs < 0) {
        throw new Error(`Invalid intervalMs in --send-key-sequence step: ${rawStep}`);
      }
      if (!Number.isInteger(parsed.afterMs) || parsed.afterMs < 0) {
        throw new Error(`Invalid afterMs in --send-key-sequence step: ${rawStep}`);
      }
      return parsed;
    });
}
