import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { normalizeOracleBatchManifest, prepareOracleBatch } from "../src/oracle-batch.mjs";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

test("normalizes oracle batch PMX/VMD cases with template registry", () => {
  const manifest = normalizeOracleBatchManifest(
    {
      defaults: { outDir: "out/oracles" },
      templates: [{ pmx: "../data/pmx/model.pmx", templatePmm: "../data/pmm/model_base.pmm", targetSlot: 2 }],
      cases: [{ name: "walk", pmx: "../data/pmx/model.pmx", vmd: "motions/walk.vmd" }],
    },
    resolve(packageRoot, "oracle-batch.json"),
  );

  assert.equal(manifest.cases[0].name, "walk");
  assert.equal(manifest.outDir, resolve(packageRoot, "out", "oracles"));
  assert.match(manifest.cases[0].pmx, /data[\\/]pmx[\\/]model\.pmx$/);
  assert.match(manifest.cases[0].templatePmm, /data[\\/]pmm[\\/]model_base\.pmm$/);
  assert.equal(manifest.cases[0].targetSlot, 2);
  assert.equal(manifest.cases[0].vmd, resolve(packageRoot, "motions", "walk.vmd"));
});

test("normalizes oracle batch sendKeyAfterMs defaults and case overrides", () => {
  const manifest = normalizeOracleBatchManifest(
    {
      defaults: { sendKeyAfterMs: 12000 },
      cases: [
        { name: "default-delay", pmx: "model.pmx", vmd: "motion.vmd" },
        { name: "case-delay", pmx: "model.pmx", vmd: "motion.vmd", sendKeyAfterMs: 30000 },
      ],
    },
    resolve(packageRoot, "oracle-batch.json"),
  );

  assert.equal(manifest.cases[0].sendKeyAfterMs, 12000);
  assert.equal(manifest.cases[1].sendKeyAfterMs, 30000);
});

test("normalizes numeric-compare manifest cases from assets and oracle fields", () => {
  const manifest = normalizeOracleBatchManifest(
    {
      schemaVersion: 1,
      kind: "numeric-compare",
      backend: "mmd-native",
      defaults: { outDir: "../runs/motion", samplePolicy: "manifest-frames" },
      cases: [
        {
          name: "walk",
          kind: "motion-numeric",
          assets: {
            model: "../data/pmx/model.pmx",
            motion: "motions/walk.vmd",
            cameraMotion: null,
            pmm: "../data/pmm/model_base.pmm",
          },
          oracle: { path: "../runs/motion/walk/oracle.actual.jsonl", format: "jsonl" },
          frames: [0, 30],
          compare: { targets: ["bones"], epsilon: 0.003 },
        },
      ],
    },
    resolve(packageRoot, "manifests", "motion.json"),
  );

  assert.equal(manifest.cases[0].name, "walk");
  assert.equal(manifest.cases[0].pmx, resolve(packageRoot, "data", "pmx", "model.pmx"));
  assert.equal(manifest.cases[0].templatePmm, resolve(packageRoot, "data", "pmm", "model_base.pmm"));
  assert.equal(manifest.cases[0].vmd, resolve(packageRoot, "manifests", "motions", "walk.vmd"));
  assert.equal(manifest.cases[0].oraclePath, resolve(packageRoot, "runs", "motion", "walk", "oracle.actual.jsonl"));
  assert.deepEqual(manifest.cases[0].frames, [0, 30]);
});

test("normalizes only mmd-native compatible cases from mixed camera manifests", () => {
  const manifest = normalizeOracleBatchManifest(
    {
      defaults: { outDir: "../runs/camera-motion" },
      cases: [
        {
          name: "edge",
          backend: "mmd-native",
          assets: { model: "../data/pmx/model.pmx", motion: "motion.vmd", cameraMotion: "camera.vmd" },
          frames: [0],
        },
        {
          name: "nanoem-reference",
          backend: "native-nanoem",
          assets: { model: null, motion: null, cameraMotion: "nanoem-camera.vmd", pmm: null },
          frames: [0],
        },
      ],
    },
    resolve(packageRoot, "manifests", "camera_motion.json"),
  );

  assert.equal(manifest.cases.length, 1);
  assert.equal(manifest.cases[0].name, "edge");
  assert.equal(manifest.cases[0].backend, "mmd-native");
});

test("resolves oracle batch relative paths from the manifest directory", () => {
  const manifest = normalizeOracleBatchManifest(
    {
      defaults: { outDir: "out/nested-check" },
      cases: [{ name: "walk", pmx: "model.pmx", vmd: "motion.vmd" }],
    },
    resolve(packageRoot, "out", "nested-check", "oracle-batch.json"),
  );

  assert.equal(manifest.outDir, resolve(packageRoot, "out", "nested-check", "out", "nested-check"));
  assert.equal(manifest.cases[0].pmx, resolve(packageRoot, "out", "nested-check", "model.pmx"));
  assert.equal(manifest.cases[0].vmd, resolve(packageRoot, "out", "nested-check", "motion.vmd"));
});

test("prepares an oracle batch from PMX/VMD case inputs without launching MMD", async (t) => {
  const pmx = resolve(packageRoot, "..", "data", "pmx", "Tda式初音ミクV4X_Ver1.00", "Tda式初音ミクV4X_Ver1.00.pmx");
  const templatePmm = resolve(packageRoot, "..", "data", "pmm", "tda_base_no_motion.pmm");
  const vmd = resolve(packageRoot, "out", "pmm-analysis", "tda-parent-center-groove-transform-keys-target.vmd");
  if (skipMissing(t, [["PMX", pmx], ["template PMM", templatePmm], ["target VMD", vmd]])) {
    return;
  }

  const outDir = resolve(packageRoot, "out", "test-oracle-batch");
  await rm(outDir, { recursive: true, force: true });
  await mkdir(outDir, { recursive: true });
  const manifestPath = resolve(outDir, "oracle-batch.json");
  await writeFile(
    manifestPath,
    `${JSON.stringify(
      {
        defaults: { outDir },
        templates: [{ pmx, templatePmm, targetSlot: 0 }],
        cases: [{ name: "tda-transform", pmx, vmd }],
      },
      null,
      2,
    )}\n`,
    "utf8",
  );

  const result = await prepareOracleBatch({ manifest: manifestPath });

  assert.equal(result.ok, true);
  assert.equal(result.cases, 1);
  assert.equal(result.results[0].name, "tda-transform");
  assert.deepEqual(Object.keys(result.results[0].sourceCounts.bodyVmd), [
    "boneFrames", "morphFrames", "cameraFrames", "lightFrames", "selfShadowFrames", "propertyFrames",
  ]);
  assert.equal(result.results[0].patch.counts.mismatches, 0);
  assert.equal(existsSync(result.results[0].project), true);
  assert.equal(existsSync(result.results[0].fixturePath), true);
});

test("prepares an oracle batch from PMX/VMD inputs without a template PMM", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-oracle-batch-template-free-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const outDir = join(dir, "out");
  const manifestPath = join(dir, "oracle-batch.json");
  await writeFile(pmx, makeMinimalPmx());
  await writeFile(
    vmd,
    createSyntheticVmd({
      boneFrames: [{ name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] }],
      morphName: "まばたき",
      frame: 30,
      weight: 0.5,
    }),
  );
  await writeFile(
    manifestPath,
    `${JSON.stringify(
      {
        defaults: { outDir },
        cases: [{ name: "template-free", pmx, vmd }],
      },
      null,
      2,
    )}\n`,
    "utf8",
  );

  const result = await prepareOracleBatch({ manifest: manifestPath });

  assert.equal(result.ok, true);
  assert.equal(result.results[0].mode, "pmx-vmd-generated-pmm");
  assert.equal(result.results[0].templatePmm, null);
  assert.equal(result.results[0].patch.counts.mismatches, 0);
  assert.deepEqual(result.results[0].sourceCounts, {
    bodyVmd: {
      boneFrames: 1,
      morphFrames: 1,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
  });
  assert.deepEqual(result.results[0].filter.skippedCounts, { boneFrames: 0, morphFrames: 0 });
  const fixture = JSON.parse(await readFile(result.results[0].fixturePath, "utf8"));
  assert.equal(fixture.mmdExe, resolve(packageRoot, "MikuMikuDance_v932x64", "MikuMikuDance.exe"));
  assert.equal(existsSync(result.results[0].project), true);
  assert.equal(existsSync(result.results[0].fixturePath), true);
});

test("prepares a template-free camera dump batch with camera VMD and camera-mode fixture settings", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-oracle-batch-camera-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const cameraVmd = join(dir, "camera.vmd");
  const outDir = join(dir, "out");
  const manifestPath = join(dir, "oracle-batch.json");
  await writeFile(pmx, makeMinimalPmx());
  await writeFile(vmd, createSyntheticVmd());
  await writeFile(
    cameraVmd,
    createSyntheticVmd({
      cameraFrames: [
        { frame: 0, distance: -10, position: [1, 2, 3], rotation: [0.1, 0.2, 0.3], fov: 15, perspective: 0 },
        { frame: 30, distance: -45, position: [4, 5, 6], rotation: [0.4, 0.5, 0.6], fov: 60, perspective: 1 },
      ],
    }),
  );
  await writeFile(
    manifestPath,
    `${JSON.stringify(
      {
        defaults: {
          outDir,
          dump: { bones: false, morphs: false, camera: true },
          cameraModeAfterMs: 3000,
          captureFrameOffset: 0,
          keepInitialFrameZero: true,
          playback: true,
        },
        cases: [
          {
            name: "camera-edge",
            assets: { model: pmx, motion: vmd, cameraMotion: cameraVmd },
            framesRange: { start: 0, end: 30, step: 1 },
          },
        ],
      },
      null,
      2,
    )}\n`,
    "utf8",
  );

  const result = await prepareOracleBatch({ manifest: manifestPath });

  assert.equal(result.ok, true);
  assert.match(result.results[0].cameraVmd, /camera\.vmd$/);
  assert.equal(result.results[0].cameraComparison.ok, true);
  assert.equal(result.results[0].cameraComparison.expected, 2);
  assert.equal(result.results[0].cameraComparison.actual, 2);
  assert.deepEqual(result.results[0].cameraComparison.mismatches, []);
  assert.deepEqual(result.results[0].sourceCounts, {
    bodyVmd: {
      boneFrames: 0,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    cameraVmd: {
      boneFrames: 0,
      morphFrames: 0,
      cameraFrames: 2,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
  });
  const fixture = JSON.parse(await readFile(result.results[0].fixturePath, "utf8"));
  assert.deepEqual(fixture.dump, {
    bones: false,
    morphs: false,
    camera: true,
    cameraKeyframes: true,
    sceneParameters: false,
    rigidBodies: false,
  });
  assert.equal(fixture.cameraModeAfterMs, 3000);
  assert.equal(fixture.captureFrameOffset, 0);
  assert.equal(fixture.keepInitialFrameZero, true);
  assert.equal(fixture.playback, true);
  assert.deepEqual(fixture.framesRange, { start: 0, end: 30, step: 1 });
  assert.equal(fixture.frames.length, 31);
});

test("fails a template-free camera batch closed when the camera VMD has no keyframes", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-oracle-batch-camera-missing-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const cameraVmd = join(dir, "camera-empty.vmd");
  const outDir = join(dir, "out");
  const manifestPath = join(dir, "oracle-batch.json");
  await writeFile(pmx, makeMinimalPmx());
  await writeFile(vmd, createSyntheticVmd());
  await writeFile(cameraVmd, createSyntheticVmd());
  await writeFile(
    manifestPath,
    `${JSON.stringify({ defaults: { outDir }, cases: [{ name: "camera-missing", assets: { model: pmx, motion: vmd, cameraMotion: cameraVmd } }] }, null, 2)}\n`,
    "utf8",
  );

  const result = await prepareOracleBatch({ manifest: manifestPath });

  assert.equal(result.ok, false);
  assert.equal(result.results[0].ok, false);
  assert.equal(result.results[0].cameraComparison.ok, false);
  assert.equal(result.results[0].cameraComparison.reason, "CAMERA_FRAMES_MISSING");
});

function skipMissing(t, entries) {
  const missing = entries.filter(([, path]) => !existsSync(path)).map(([label, path]) => `${label}: ${path}`);
  if (missing.length > 0) {
    t.skip(`External fixture unavailable (${missing.join(", ")})`);
    return true;
  }
  return false;
}

function makeMinimalPmx() {
  const parts = [];
  const textEncoder = new TextEncoder();
  parts.push(Buffer.from("PMX ", "ascii"));
  pushFloat32(parts, 2.0);
  pushUInt8(parts, 8);
  parts.push(Buffer.from([1, 0, 4, 4, 4, 4, 4, 4]));
  pushText(parts, textEncoder, "テストモデル");
  pushText(parts, textEncoder, "test model");
  pushText(parts, textEncoder, "");
  pushText(parts, textEncoder, "");
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 1);
  pushBone(parts, textEncoder, "センター", "center");
  pushInt32(parts, 1);
  pushText(parts, textEncoder, "まばたき");
  pushText(parts, textEncoder, "blink");
  pushUInt8(parts, 2);
  pushUInt8(parts, 1);
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  return Buffer.concat(parts);
}

function pushBone(parts, textEncoder, name, englishName) {
  pushText(parts, textEncoder, name);
  pushText(parts, textEncoder, englishName);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
  pushInt32(parts, -1);
  pushInt32(parts, 0);
  pushUInt16(parts, 0);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
}

function pushText(parts, textEncoder, text) {
  const bytes = textEncoder.encode(text);
  pushInt32(parts, bytes.byteLength);
  parts.push(Buffer.from(bytes));
}

function pushUInt8(parts, value) {
  parts.push(Buffer.from([value]));
}

function pushUInt16(parts, value) {
  const buffer = Buffer.alloc(2);
  buffer.writeUInt16LE(value);
  parts.push(buffer);
}

function pushInt32(parts, value) {
  const buffer = Buffer.alloc(4);
  buffer.writeInt32LE(value);
  parts.push(buffer);
}

function pushFloat32(parts, value) {
  const buffer = Buffer.alloc(4);
  buffer.writeFloatLE(value);
  parts.push(buffer);
}
