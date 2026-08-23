#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { copyFile, link, mkdir, mkdtemp, readdir, rm, stat } from "node:fs/promises";
import { existsSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { tmpdir } from "node:os";

const options = parseArgs(process.argv.slice(2));
const captureDir = resolve(options.captureDir ?? "out/mmdplugin-capture-smoke");
const output = resolve(options.output ?? resolve(captureDir, "capture.mp4"));
const fps = Number(options.fps ?? 30);
const crf = String(options.crf ?? 18);
const preset = String(options.preset ?? "medium");

if (!Number.isFinite(fps) || fps <= 0) {
  throw new Error("--fps must be a positive number");
}
if (!existsSync(captureDir)) {
  throw new Error(`Capture directory does not exist: ${captureDir}`);
}

const frames = await listBmpFrames(captureDir);
if (frames.length === 0) {
  throw new Error(`No frame_*.bmp files found in ${captureDir}`);
}

const tempDir = await mkdtemp(resolve(dirname(output), ".mmddumper-video-"));
const sequenceDir = resolve(tempDir, "frames");
try {
  await mkdir(sequenceDir, { recursive: true });
  for (let index = 0; index < frames.length; index += 1) {
    const destination = resolve(sequenceDir, `frame_${String(index + 1).padStart(6, "0")}.bmp`);
    await linkOrCopy(frames[index].path, destination);
  }

  const args = [
    "-hide_banner",
    "-y",
    "-framerate",
    String(fps),
    "-i",
    resolve(sequenceDir, "frame_%06d.bmp"),
    "-vf",
    "format=yuv420p",
    "-c:v",
    "libx264",
    "-preset",
    preset,
    "-crf",
    crf,
    "-movflags",
    "+faststart",
    output,
  ];
  run("ffmpeg", args);
} finally {
  await rm(tempDir, { recursive: true, force: true });
}

const outputStats = await stat(output);
const probe = probeVideo(output);
console.log(
  JSON.stringify(
    {
      ok: true,
      captureDir,
      output,
      fps,
      frames: frames.length,
      firstFrame: frames[0].frame,
      lastFrame: frames.at(-1).frame,
      bytes: outputStats.size,
      video: probe,
    },
    null,
    2,
  ),
);

async function listBmpFrames(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const frames = [];
  for (const entry of entries) {
    if (!entry.isFile()) {
      continue;
    }
    const match = /^frame_(\d+)\.bmp$/u.exec(entry.name);
    if (!match) {
      continue;
    }
    const path = resolve(directory, entry.name);
    const stats = await stat(path);
    if (stats.size <= 1024 * 1024) {
      continue;
    }
    frames.push({ frame: Number(match[1]), path, size: stats.size });
  }
  const maxSize = Math.max(...frames.map((frame) => frame.size));
  return frames
    .filter((frame) => frame.size === maxSize)
    .sort((left, right) => left.frame - right.frame || left.path.localeCompare(right.path));
}

function run(command, args) {
  const result = spawnSync(command, args, { cwd: dirname(output), encoding: "utf8", shell: false, timeout: 120000 });
  if (result.error?.code === "ETIMEDOUT") {
    throw new Error(`${command} timed out`);
  }
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit code ${result.status ?? 1}\n${result.stderr || result.stdout}`);
  }
}

function probeVideo(path) {
  const result = spawnSync(
    "ffprobe",
    ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height,nb_frames,duration,r_frame_rate", "-of", "json", path],
    { encoding: "utf8", shell: false, timeout: 30000 },
  );
  if (result.status !== 0) {
    return { available: false };
  }
  try {
    const parsed = JSON.parse(result.stdout);
    return { available: true, ...(parsed.streams?.[0] ?? {}) };
  } catch {
    return { available: false };
  }
}

async function linkOrCopy(source, destination) {
  try {
    await link(source, destination);
  } catch {
    await copyFile(source, destination);
  }
}

function parseArgs(args) {
  const parsed = {};
  const normalized = args.filter((arg) => arg !== "--");
  for (let i = 0; i < normalized.length; i += 1) {
    const token = normalized[i];
    if (token === "--capture-dir") {
      parsed.captureDir = normalized[++i];
    } else if (token === "--output") {
      parsed.output = normalized[++i];
    } else if (token === "--fps") {
      parsed.fps = normalized[++i];
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
