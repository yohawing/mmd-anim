#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { rm } from "node:fs/promises";
import { resolve } from "node:path";

const options = parseArgs(process.argv.slice(2));
const fixture = options.fixture ?? "fixtures/sample-basic/fixture.json";
const captureDir = resolve(options.captureDir ?? "out/mmdplugin-mp4");
const output = resolve(options.output ?? resolve(captureDir, "capture.mp4"));
const fps = options.fps ?? "30";
const timeoutMs = options.timeoutMs ?? "60000";
const minCaptureFiles = options.minCaptureFiles ?? "30";
const captureEveryNFrames = options.captureEveryNFrames ?? "1";
const captureMaxFrame = options.captureMaxFrame;

await rm(captureDir, { recursive: true, force: true });

runNode([
  "scripts/mmd-first-load-smoke.mjs",
  "--trigger",
  "mmdplugin",
  "--send-key-after-ms",
  options.sendKeyAfterMs ?? "3000",
  "--send-key",
  options.sendKey ?? "p",
  "--min-records",
  "0",
  "--fixture",
  fixture,
  "--capture-dir",
  captureDir,
  "--timeout-ms",
  timeoutMs,
  "--min-capture-files",
  minCaptureFiles,
  "--capture-every-n-frames",
  captureEveryNFrames,
  ...(captureMaxFrame ? ["--capture-max-frame", captureMaxFrame] : []),
  "--write-done",
]);

runNode([
  "scripts/capture-to-video.mjs",
  "--capture-dir",
  captureDir,
  "--output",
  output,
  "--fps",
  fps,
  ...(options.crf ? ["--crf", options.crf] : []),
  ...(options.preset ? ["--preset", options.preset] : []),
]);

console.log(JSON.stringify({ ok: true, captureDir, output, fps: Number(fps) }, null, 2));

function runNode(args) {
  const result = spawnSync(process.execPath, args, {
    cwd: resolve(import.meta.dirname, ".."),
    env: process.env,
    stdio: "inherit",
    shell: false,
    timeout: 180000,
    windowsHide: false,
  });
  if (result.error?.code === "ETIMEDOUT") {
    throw new Error(`${args[0]} timed out`);
  }
  if (result.status !== 0) {
    throw new Error(`${args[0]} failed with exit code ${result.status ?? 1}`);
  }
}

function parseArgs(args) {
  const parsed = {};
  const normalized = args.filter((arg) => arg !== "--");
  for (let i = 0; i < normalized.length; i += 1) {
    const token = normalized[i];
    if (token === "--fixture") {
      parsed.fixture = normalized[++i];
    } else if (token === "--capture-dir") {
      parsed.captureDir = normalized[++i];
    } else if (token === "--output") {
      parsed.output = normalized[++i];
    } else if (token === "--fps") {
      parsed.fps = normalized[++i];
    } else if (token === "--timeout-ms") {
      parsed.timeoutMs = normalized[++i];
    } else if (token === "--min-capture-files") {
      parsed.minCaptureFiles = normalized[++i];
    } else if (token === "--capture-every-n-frames") {
      parsed.captureEveryNFrames = normalized[++i];
    } else if (token === "--capture-max-frame") {
      parsed.captureMaxFrame = normalized[++i];
    } else if (token === "--send-key-after-ms") {
      parsed.sendKeyAfterMs = normalized[++i];
    } else if (token === "--send-key") {
      parsed.sendKey = normalized[++i];
    } else if (token === "--crf") {
      parsed.crf = normalized[++i];
    } else if (token === "--preset") {
      parsed.preset = normalized[++i];
    } else {
      throw new Error(`Unknown argument: ${token}`);
    }
  }
  return parsed;
}
