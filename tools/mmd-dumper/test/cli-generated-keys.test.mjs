import test from "node:test";
import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { promisify } from "node:util";
import { createUnittestOneBoneTemplate } from "./unittest-pmm-fixture.mjs";

const execFileAsync = promisify(execFile);

test("CLI generates counted bone keys and writes compact verified PMM output", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-cli-keys-"));
  const vmdFile = join(dir, "five-position-keys.vmd");
  const pmmFile = join(dir, "five-position-keys.pmm");
  const templateFile = join(dir, "unittest_with_one_bone_key.pmm");
  await writeFile(templateFile, createUnittestOneBoneTemplate());

  const vmd = await runCli([
    "write-test-vmd",
    "--out",
    vmdFile,
    "--model-name",
    "Tda",
    "--bone-name",
    "センター",
    "--bone-key-count",
    "5",
    "--bone-key-start-frame",
    "30",
    "--bone-key-frame-step",
    "30",
    "--bone-key-start-position",
    "1,2,3",
    "--bone-key-position-step",
    "3,3,3",
  ]);

  assert.equal(vmd.ok, true);
  assert.equal(vmd.boneFrames, 5);

  const pmm = await runCli([
    "write-pmm-unittest-vmd-bone-keys",
    templateFile,
    "--vmd",
    vmdFile,
    "--out",
    pmmFile,
    "--require-verified",
    "true",
    "--compact",
    "true",
  ]);

  assert.equal(pmm.ok, true);
  assert.equal(pmm.keyCount, 5);
  assert.equal(pmm.keys.count, 5);
  assert.equal(pmm.keys.first[0].frame, 30);
  assert.equal(pmm.keys.last.at(-1).frame, 150);
  assert.equal(pmm.generatedMapping.structurallyVerified, true);
  assert.equal(pmm.generatedMapping.coverage.framesWithExactFrameRecord, 5);
  assert.equal(pmm.generatedMapping.coverage.framesWithLocalPositionEvidence, 5);
  assert.equal(pmm.generatedMapping.coverage.exactFrameRecordOffsets.count, 5);
});

test("CLI VMD-driven PMM writer reads dense keys beyond the default VMD sample limit", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-cli-many-keys-"));
  const vmdFile = join(dir, "many-position-keys.vmd");
  const pmmFile = join(dir, "many-position-keys.pmm");
  const templateFile = join(dir, "unittest_with_one_bone_key.pmm");
  await writeFile(templateFile, createUnittestOneBoneTemplate());

  const vmd = await runCli([
    "write-test-vmd",
    "--out",
    vmdFile,
    "--model-name",
    "Tda",
    "--bone-name",
    "センター",
    "--bone-key-count",
    "1025",
    "--bone-key-start-frame",
    "1",
    "--bone-key-frame-step",
    "1",
    "--bone-key-start-position",
    "1,2,3",
    "--bone-key-position-step",
    "3,3,3",
  ]);

  assert.equal(vmd.ok, true);
  assert.equal(vmd.boneFrames, 1025);

  const pmm = await runCli([
    "write-pmm-unittest-vmd-bone-keys",
    templateFile,
    "--vmd",
    vmdFile,
    "--out",
    pmmFile,
    "--require-verified",
    "true",
    "--compact",
    "true",
  ]);

  assert.equal(pmm.ok, true);
  assert.equal(pmm.sourceCounts.boneFrames, 1025);
  assert.equal(pmm.keyCount, 1025);
  assert.equal(pmm.keys.count, 1025);
  assert.equal(pmm.keys.last.at(-1).frame, 1025);
  assert.deepEqual(pmm.keys.last.at(-1).position, [3073, 3074, 3075]);
  assert.equal(pmm.generatedMapping.structurallyVerified, true);
  assert.equal(pmm.generatedMapping.coverage.framesWithExactFrameRecord, 1025);
  assert.equal(pmm.generatedMapping.coverage.framesWithLocalPositionEvidence, 1025);
});

test("CLI generates bone rotation keys for PMM rotation fixtures", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-cli-rotation-keys-"));
  const vmdFile = join(dir, "rotation-keys.vmd");

  const vmd = await runCli([
    "write-test-vmd",
    "--out",
    vmdFile,
    "--model-name",
    "テスト用モデル_arm",
    "--bone-name",
    "全ての親",
    "--position",
    "0,0,0",
    "--bone-rotation-keys",
    "30:0.382683,0,0,0.92388;60:0,0,0.382683,0.92388",
  ]);

  assert.equal(vmd.ok, true);
  assert.equal(vmd.boneFrames, 2);

  const inspected = await runCli(["inspect-vmd", vmdFile, "--limit", "2"]);
  assert.equal(inspected.counts.boneFrames, 2);
  assert.deepEqual(inspected.bones.map((bone) => bone.frame), [30, 60]);
  assert.deepEqual(inspected.bones[0].position, [0, 0, 0]);
  assert.deepEqual(inspected.bones[0].rotation, [0.382683, 0, 0, 0.92388]);
  assert.deepEqual(inspected.bones[1].rotation, [0, 0, 0.382683, 0.92388]);
});

test("CLI generates bone transform keys with position and rotation", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-cli-transform-keys-"));
  const vmdFile = join(dir, "transform-keys.vmd");

  const vmd = await runCli([
    "write-test-vmd",
    "--out",
    vmdFile,
    "--model-name",
    "テスト用モデル_arm",
    "--bone-name",
    "全ての親",
    "--bone-transform-keys",
    "30:1,2,3:0.382683,0,0,0.92388;60:4,5,6:0,0,0.382683,0.92388",
  ]);

  assert.equal(vmd.ok, true);
  assert.equal(vmd.boneFrames, 2);

  const inspected = await runCli(["inspect-vmd", vmdFile, "--limit", "2"]);
  assert.equal(inspected.counts.boneFrames, 2);
  assert.deepEqual(inspected.bones.map((bone) => bone.frame), [30, 60]);
  assert.deepEqual(inspected.bones[0].position, [1, 2, 3]);
  assert.deepEqual(inspected.bones[0].rotation, [0.382683, 0, 0, 0.92388]);
  assert.deepEqual(inspected.bones[1].position, [4, 5, 6]);
  assert.deepEqual(inspected.bones[1].rotation, [0, 0, 0.382683, 0.92388]);
});

test("CLI generates counted transform keys and writes verified PMM output", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-cli-many-transform-keys-"));
  const vmdFile = join(dir, "many-transform-keys.vmd");
  const pmmFile = join(dir, "many-transform-keys.pmm");
  const templateFile = join(dir, "unittest_with_one_bone_key.pmm");
  await writeFile(templateFile, createUnittestOneBoneTemplate());

  const vmd = await runCli([
    "write-test-vmd",
    "--out",
    vmdFile,
    "--model-name",
    "テスト用モデル_arm",
    "--bone-name",
    "全ての親",
    "--bone-transform-key-count",
    "257",
    "--bone-key-start-frame",
    "1",
    "--bone-key-frame-step",
    "1",
    "--bone-key-start-position",
    "1,2,3",
    "--bone-key-position-step",
    "3,3,3",
  ]);

  assert.equal(vmd.ok, true);
  assert.equal(vmd.boneFrames, 257);

  const pmm = await runCli([
    "write-pmm-unittest-vmd-bone-keys",
    templateFile,
    "--vmd",
    vmdFile,
    "--out",
    pmmFile,
    "--bone-name",
    "全ての親",
    "--allow-non-identity-rotation",
    "true",
    "--require-verified",
    "true",
    "--compact",
    "true",
  ]);

  assert.equal(pmm.ok, true);
  assert.equal(pmm.keyCount, 257);
  assert.equal(pmm.keys.count, 257);
  assert.equal(pmm.keys.last.at(-1).frame, 257);
  assert.deepEqual(pmm.keys.last.at(-1).position, [769, 770, 771]);
  assert.equal(pmm.generatedMapping.structurallyVerified, true);
  assert.equal(pmm.generatedMapping.layoutRecordByteLength, 62);
  assert.equal(pmm.generatedMapping.layoutRecordTotal, 257);
  assert.equal(pmm.generatedMapping.coverage.framesWithExactFrameRecord, 257);
  assert.equal(pmm.generatedMapping.coverage.framesWithLocalPositionEvidence, 257);
  assert.equal(pmm.generatedMapping.coverage.framesWithLocalRotationEvidence, 257);
});

async function runCli(args) {
  const { stdout } = await execFileAsync(process.execPath, ["src/cli.mjs", ...args], {
    cwd: process.cwd(),
    windowsHide: true,
  });
  return JSON.parse(stdout);
}
