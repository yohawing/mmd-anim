import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { inspectTemplateWriteProfile, planPmmTemplateCreation, writePmmFromTemplateProfile } from "../src/pmm-template-create.mjs";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";
import { createUnittestOneBoneTemplate } from "./unittest-pmm-fixture.mjs";

test("plans PMM template creation for a matching model slot and VMD", () => {
  const report = planPmmTemplateCreation({
    template: "base.pmm",
    vmdFile: "motion.vmd",
    modelSlot: 0,
    slotReport: createSlotReport(),
    vmd: createVmd([{ name: "センター", frame: 30 }]),
  });

  assert.equal(report.mode, "pmm-template-create-dry-run");
  assert.equal(report.okToWrite, true);
  assert.equal(report.writable, false);
  assert.equal(report.profile.profileWritable, false);
  assert.equal(report.target.modelSlot, 0);
  assert.equal(report.vmd.boneNameCount, 1);
  assert.deepEqual(report.checks.missingBones, []);
  assert.deepEqual(report.errors, []);
});

test("reports missing bones and unsupported VMD channels before PMM writes", () => {
  const report = planPmmTemplateCreation({
    template: "base.pmm",
    vmdFile: "motion.vmd",
    modelSlot: 0,
    slotReport: createSlotReport(),
    vmd: {
      ...createVmd([{ name: "未存在", frame: 30 }]),
      counts: {
        boneFrames: 1,
        morphFrames: 1,
        cameraFrames: 0,
        lightFrames: 0,
        selfShadowFrames: 0,
        propertyFrames: 0,
      },
    },
  });

  assert.equal(report.okToWrite, false);
  assert.deepEqual(report.checks.missingBones, ["未存在"]);
  assert.deepEqual(report.checks.unsupportedChannels, [{ name: "morphFrames", count: 1 }]);
  assert.equal(report.errors.length, 2);
});

test("warns when VMD bones are ambiguous across model slots", () => {
  const report = planPmmTemplateCreation({
    template: "base.pmm",
    vmdFile: "motion.vmd",
    modelSlot: 0,
    slotReport: createSlotReport({
      boneNameCollisions: [
        {
          name: "センター",
          entries: [
            { slot: 0, model: "model-a.pmx", boneIndex: 0 },
            { slot: 1, model: "model-b.pmx", boneIndex: 0 },
          ],
        },
      ],
    }),
    vmd: createVmd([{ name: "センター", frame: 30 }]),
  });

  assert.equal(report.okToWrite, true);
  assert.equal(report.checks.ambiguousBones[0].name, "センター");
  assert.match(report.warnings[0], /model slots/);
});

test("marks only the unittest one-bone profile as writable", () => {
  const templateBytes = createUnittestOneBoneTemplate();
  const report = planPmmTemplateCreation({
    template: "unittest.pmm",
    vmdFile: "motion.vmd",
    modelSlot: 0,
    templateBytes,
    slotReport: createSlotReport(),
    vmd: createVmd([{ name: "センター", frame: 30 }]),
  });

  assert.equal(report.okToWrite, true);
  assert.equal(report.profile.profileWritable, true);
  assert.equal(report.writable, true);
  assert.equal(report.profile.template.fields.firstKeyControl, "14141414");
});

test("keeps general PMM templates dry-run only even when input checks pass", () => {
  const profile = inspectTemplateWriteProfile({
    templateBytes: Buffer.from("Polygon Movie maker 0002\0not-a-unittest-template", "latin1"),
    modelSlot: 0,
    vmdBoneNames: ["センター"],
    errors: [],
  });

  assert.equal(profile.profileWritable, false);
  assert.match(profile.reasons[0], /too small|expected unittest one-bone key layout/);
});

test("writes PMM only through a verified writable profile", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmm-template-write-"));
  const pmxFile = join(dir, "model.pmx");
  const templateFile = join(dir, "template.pmm");
  const vmdFile = join(dir, "motion.vmd");
  const outFile = join(dir, "generated.pmm");
  await writeFile(pmxFile, createMinimalPmx("モデルA", ["センター"]));
  await writeFile(templateFile, createUnittestOneBoneTemplate({ modelPath: pmxFile }));
  await writeFile(
    vmdFile,
    createSyntheticVmd({
      modelName: "モデルA",
      boneName: "センター",
      boneFrames: [
        { name: "センター", frame: 30, position: [1, 2, 3] },
        { name: "センター", frame: 60, position: [4, 5, 6] },
      ],
    }),
  );

  const result = await writePmmFromTemplateProfile({
    template: templateFile,
    vmd: vmdFile,
    out: outFile,
    modelSlot: 0,
    requireVerified: true,
  });
  const generated = await readFile(outFile);

  assert.equal(result.mode, "pmm-template-create-write");
  assert.equal(result.plan.writable, true);
  assert.equal(result.written.keyCount, 2);
  assert.equal(generated.readUInt16LE(0x1ce), 2);
  assert.equal(generated.readUInt32LE(0x20e + 6), 60);
});

test("rejects write mode when verification is not required", async () => {
  await assert.rejects(
    () =>
      writePmmFromTemplateProfile({
        template: "template.pmm",
        vmd: "motion.vmd",
        out: "generated.pmm",
        modelSlot: 0,
      }),
    /requires requireVerified=true/,
  );
});

function createSlotReport(overrides = {}) {
  return {
    modelSlotCount: 1,
    modelSlots: [
      {
        slot: 0,
        path: "model-a.pmx",
        fileName: "model-a.pmx",
        readable: true,
        inventory: {
          modelName: "モデルA",
          counts: { bones: 2, morphs: 0 },
          bones: [
            { index: 0, name: "センター" },
            { index: 1, name: "左足" },
          ],
        },
      },
    ],
    boneNameCollisions: [],
    ...overrides,
  };
}

function createVmd(bones) {
  return {
    modelName: "model",
    maxFrame: Math.max(...bones.map((bone) => bone.frame), 0),
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones: bones.map((bone) => ({
      name: bone.name,
      frame: bone.frame,
      position: [0, 0, 0],
      rotation: [0, 0, 0, 1],
    })),
    boneNameCounts: bones.map((bone) => ({ name: bone.name, count: 1 })),
  };
}

function createMinimalPmx(modelName, boneNames) {
  const parts = [];
  const textEncoder = new TextEncoder();

  parts.push(Buffer.from("PMX ", "ascii"));
  pushFloat32(parts, 2.0);
  pushUInt8(parts, 8);
  parts.push(Buffer.from([1, 0, 4, 4, 4, 4, 4, 4]));

  pushText(parts, textEncoder, modelName);
  pushText(parts, textEncoder, modelName);
  pushText(parts, textEncoder, "");
  pushText(parts, textEncoder, "");

  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 0);
  pushInt32(parts, 0);

  pushInt32(parts, boneNames.length);
  for (const name of boneNames) {
    pushBone(parts, textEncoder, name);
  }

  pushInt32(parts, 0);
  pushInt32(parts, 0);

  return Buffer.concat(parts);
}

function pushBone(parts, textEncoder, name) {
  pushText(parts, textEncoder, name);
  pushText(parts, textEncoder, name);
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
