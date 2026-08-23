import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import iconv from "iconv-lite";
import { createBasePmmFromPmxInventory, writePmmCameraVmdPatch, writePmmFromPmxVmd } from "../src/pmm-from-pmx-vmd.mjs";
import { parsePmmDocumentKeyframes } from "../src/pmm-document-keyframes.mjs";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";

test("creates a minimal PMMv2 base document from PMX inventory", () => {
  const bytes = createBasePmmFromPmxInventory({
    pmx: "F:\\Develop\\MMDDev\\data\\pmx\\Model.pmx",
    inventory: {
      modelName: "テストモデル",
      modelNameEnglish: "Test Model",
      bones: [
        { name: "センター" },
        { name: "上半身" },
      ],
      morphs: [{ name: "まばたき" }],
    },
  });
  const document = parsePmmDocumentKeyframes(bytes);
  const model = document.models[0];

  assert.equal(document.counts.models, 1);
  assert.equal(document.selectedModelIndex, 0);
  assert.equal(model.path, "F:\\Develop\\MMDDev\\data\\pmx\\Model.pmx");
  assert.equal(model.nameJa, "テストモデル");
  assert.equal(model.nameEn, "Test Model");
  assert.deepEqual(model.boneNames, ["センター", "上半身"]);
  assert.deepEqual(model.morphNames, ["まばたき"]);
  assert.equal(model.counts.boneKeyframes, 0);
  assert.equal(model.counts.morphKeyframes, 0);
  assert.equal(model.blendEnabled, false);
  assert.strictEqual(model.drawOrderIndex, 1);
  assert.strictEqual(model.transformOrderIndex, 1);
  assert.strictEqual(model.selfShadowEnabled, true);
  assert.equal(model.sections.morphStatesOffset - model.sections.boneStatesOffset, 2 * 31);
  const modelEnd = model.offset + model.byteLength;
  assert.equal(bytes.byteLength - modelEnd, 1221);
  assert.equal(bytes.subarray(modelEnd, modelEnd + 32).toString("hex"), "000000000000000000000000000034c200000000000020410000000000000000");
  assert.equal(model.documentModelIndex, 0);
  // pathOffset must be after the two variable strings (nameJa + nameEn), per real MMD PMMv2 layout
  const jaLen = iconv.encode(model.nameJa, "cp932").length;
  const enLen = iconv.encode(model.nameEn, "cp932").length;
  assert.equal(model.pathOffset, model.offset + 1 + 1 + jaLen + 1 + enLen);
});

test("roundtrips generated PMM model structure lists and states", () => {
  const bytes = createBasePmmFromPmxInventory({
    pmx: "F:\\Develop\\MMDDev\\data\\pmx\\Complex.pmx",
    inventory: {
      modelName: "構造モデル",
      modelNameEnglish: "Structure Model",
      bones: [
        { index: 0, name: "全ての親", flags: 0x001e },
        { index: 1, name: "非表示移動", flags: 0x0016 },
        { index: 2, name: "足ＩＫ", flags: 0x003e },
      ],
      morphs: [{ name: "まばたき" }],
      ikBoneIndices: [2],
      displayFrames: [
        { index: 0, name: "Root", special: 1, items: [{ type: 0, index: 0 }] },
        { index: 1, name: "ＩＫ", special: 0, items: [{ type: 0, index: 2 }] },
      ],
    },
  });
  const document = parsePmmDocumentKeyframes(bytes);
  const model = document.models[0];

  assert.equal(model.numFixedTracks, 3);
  const expansionStateCountOffset = model.lastFrameIndexOffset - 4 - model.numFixedTracks - 1;
  assert.equal(bytes[expansionStateCountOffset], model.numFixedTracks);
  assert.deepEqual(
    [...bytes.subarray(expansionStateCountOffset + 1, expansionStateCountOffset + 1 + model.numFixedTracks)],
    [0, 0, 0],
  );
  assert.deepEqual(model.constraintBoneIndices, [2]);
  assert.deepEqual(model.outsideParentSubjectBoneIndices, [-1, 0, 2]);
  assert.deepEqual(model.initialModelKeyframe.constraintStates, [true]);
  assert.deepEqual(model.initialModelKeyframe.outsideParents, [
    { modelIndex: -1, boneIndex: 0 },
    { modelIndex: -1, boneIndex: 0 },
    { modelIndex: -1, boneIndex: 0 },
  ]);
  assert.equal(model.sections.morphStatesOffset - model.sections.boneStatesOffset, 3 * 31);
  assert.equal(model.sections.constraintStatesOffset - model.sections.morphStatesOffset, 4);
  assert.equal(model.sections.outsideParentStatesOffset - model.sections.constraintStatesOffset, 1);
});

test("writes twelve collapsed expansion states for a twelve-track PMM model", () => {
  const fixedTracks = 12;
  const bytes = createBasePmmFromPmxInventory({
    pmx: "F:\\Develop\\MMDDev\\data\\pmx\\Tda.pmx",
    inventory: {
      modelName: "Tda-like model",
      fixedTracks,
      bones: [{ name: "センター" }],
      morphs: [],
    },
  });
  const model = parsePmmDocumentKeyframes(bytes).models[0];

  assert.equal(model.numFixedTracks, fixedTracks);
  const expansionStateCountOffset = model.lastFrameIndexOffset - 4 - model.numFixedTracks - 1;
  assert.equal(bytes[expansionStateCountOffset], fixedTracks);
  assert.deepEqual(
    [...bytes.subarray(expansionStateCountOffset + 1, expansionStateCountOffset + 1 + fixedTracks)],
    new Array(fixedTracks).fill(0),
  );
  assert.equal(model.sections.initialBoneKeyframesOffset, model.lastFrameIndexOffset + 4);
});

test("writes a PMM directly from PMX and VMD inputs", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmx-vmd-pmm-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const out = join(dir, "scene.pmm");
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

  const report = await writePmmFromPmxVmd({ pmx, vmd, out });
  const document = parsePmmDocumentKeyframes(await readFile(out));

  assert.equal(report.ok, true);
  assert.equal(report.patch.comparison.ok, true);
  assert.equal(report.patch.comparison.counts.mismatches, 0);
  assert.equal(document.models[0].boneKeyframes[0].name, "センター");
  assert.equal(document.models[0].boneKeyframes[0].frame, 30);
  assert.deepEqual(document.models[0].boneKeyframes[0].translation, [1, 2, 3]);
  assert.equal(document.models[0].morphKeyframes[0].name, "まばたき");
  assert.equal(document.models[0].morphKeyframes[0].weight, 0.5);
});

test("writes camera VMD keyframes into the PMM camera timeline", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmx-vmd-camera-pmm-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const cameraVmd = join(dir, "camera.vmd");
  const out = join(dir, "scene.pmm");
  const cameraInterpolation = [
    10, 20, 30, 40, 11, 21, 31, 41, 12, 22, 32, 42, 13, 23, 33, 43, 14, 24, 34, 44, 15, 25, 35, 45,
  ];
  await writeFile(pmx, makeMinimalPmx());
  await writeFile(vmd, createSyntheticVmd({ boneFrames: [{ name: "センター", frame: 30, position: [1, 2, 3] }] }));
  await writeFile(
    cameraVmd,
    createSyntheticVmd({
      cameraFrames: [
        { frame: 0, distance: -30, position: [1, 2, 3], rotation: [0.1, 0.2, 0.3], fov: 20, perspective: 0 },
        {
          frame: 45,
          distance: -15,
          position: [4, 5, 6],
          rotation: [0.4, 0.5, 0.6],
          interpolation: cameraInterpolation,
          fov: 35,
          perspective: 1,
        },
      ],
    }),
  );

  const report = await writePmmFromPmxVmd({ pmx, vmd, cameraVmd, out });
  const document = parsePmmDocumentKeyframes(await readFile(out));

  assert.equal(report.cameraVmdCounts.cameraFrames, 2);
  assert.equal(document.camera.initialKeyframe.frame, 0);
  assert.equal(document.camera.initialKeyframe.nextKeyframeIndex, 1);
  assert.equal(document.camera.initialKeyframe.distance, -30);
  assert.deepEqual(document.camera.initialKeyframe.position, [1, 2, 3]);
  assert.equal(document.camera.initialKeyframe.fov, 20);
  assert.equal(document.camera.initialKeyframe.perspective, true);
  assert.equal(document.camera.counts.cameraKeyframes, 1);
  assert.equal(document.camera.keyframes[0].frame, 45);
  assert.equal(document.camera.keyframes[0].previousKeyframeIndex, 0);
  assert.equal(document.camera.keyframes[0].distance, -15);
  assert.deepEqual(document.camera.keyframes[0].interpolation[0], [10, 20, 30, 40]);
  assert.deepEqual(document.camera.keyframes[0].interpolation[5], [15, 25, 35, 45]);
  assert.equal(document.camera.keyframes[0].fov, 35);
  assert.equal(document.camera.keyframes[0].perspective, false);
});

test("keeps a generated initial camera key when camera VMD starts after frame zero", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmx-vmd-late-camera-pmm-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const cameraVmd = join(dir, "camera.vmd");
  const out = join(dir, "scene.pmm");
  await writeFile(pmx, makeMinimalPmx());
  await writeFile(vmd, createSyntheticVmd({ boneFrames: [{ name: "センター", frame: 30, position: [1, 2, 3] }] }));
  await writeFile(
    cameraVmd,
    createSyntheticVmd({
      cameraFrames: [{ frame: 90, distance: -10, position: [7, 8, 9], rotation: [0.7, 0.8, 0.9], fov: 40 }],
    }),
  );

  await writePmmFromPmxVmd({ pmx, vmd, cameraVmd, out, cameraFov: 25 });
  const document = parsePmmDocumentKeyframes(await readFile(out));

  assert.equal(document.camera.initialKeyframe.frame, 0);
  assert.equal(document.camera.initialKeyframe.nextKeyframeIndex, 1);
  assert.equal(document.camera.initialKeyframe.distance, -45);
  assert.equal(document.camera.initialKeyframe.fov, 25);
  assert.equal(document.camera.keyframes[0].frame, 90);
  assert.equal(document.camera.keyframes[0].distance, -10);
});

test("patches an existing PMM camera section from a camera VMD", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-camera-patch-"));
  const template = join(dir, "template.pmm");
  const cameraVmd = join(dir, "camera.vmd");
  const out = join(dir, "patched.pmm");
  await writeFile(
    template,
    createBasePmmFromPmxInventory({
      pmx: "F:\\Develop\\MMDDev\\data\\pmx\\Model.pmx",
      inventory: { modelName: "テストモデル", bones: [{ name: "センター" }], morphs: [] },
    }),
  );
  await writeFile(
    cameraVmd,
    createSyntheticVmd({
      cameraFrames: [
        { frame: 0, distance: -20, position: [1, 2, 3], rotation: [0.1, 0.2, 0.3], fov: 24 },
        { frame: 30, distance: -10, position: [4, 5, 6], rotation: [0.4, 0.5, 0.6], fov: 42 },
      ],
    }),
  );

  const report = await writePmmCameraVmdPatch({ template, cameraVmd, out });
  const document = parsePmmDocumentKeyframes(await readFile(out));

  assert.equal(report.camera.cameraKeyframes, 1);
  assert.equal(document.models[0].nameJa, "テストモデル");
  assert.equal(document.camera.initialKeyframe.distance, -20);
  assert.equal(document.camera.keyframes[0].frame, 30);
  assert.equal(document.camera.keyframes[0].fov, 42);
});

test("skips VMD bone frames that do not exist in the PMX by default", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmx-vmd-missing-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const out = join(dir, "scene.pmm");
  await writeFile(pmx, makeMinimalPmx());
  await writeFile(
    vmd,
    createSyntheticVmd({
      boneFrames: [
        { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
        { name: "未存在", frame: 30, position: [9, 9, 9], rotation: [0, 0, 0, 1] },
      ],
      morphName: "まばたき",
      frame: 30,
      weight: 0.5,
    }),
  );

  const report = await writePmmFromPmxVmd({ pmx, vmd, out });
  const document = parsePmmDocumentKeyframes(await readFile(out));

  assert.equal(report.filter.mode, "skip");
  assert.deepEqual(report.filter.appliedCounts, { boneFrames: 1, morphFrames: 1 });
  assert.deepEqual(report.filter.skippedCounts, { boneFrames: 1, morphFrames: 0 });
  assert.deepEqual(report.filter.skippedBoneNames, [{ name: "未存在", count: 1 }]);
  assert.equal(report.patch.comparison.counts.mismatches, 0);
  assert.deepEqual(
    document.models[0].boneKeyframes.map((frame) => frame.name),
    ["センター"],
  );
});

test("can reject VMD names that do not exist in the PMX", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmx-vmd-missing-strict-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const out = join(dir, "scene.pmm");
  await writeFile(pmx, makeMinimalPmx());
  await writeFile(
    vmd,
    createSyntheticVmd({
      boneFrames: [{ name: "未存在", frame: 30, position: [9, 9, 9], rotation: [0, 0, 0, 1] }],
    }),
  );

  await assert.rejects(
    () => writePmmFromPmxVmd({ pmx, vmd, out, missingNames: "strict" }),
    /missing from PMX/,
  );
});

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
