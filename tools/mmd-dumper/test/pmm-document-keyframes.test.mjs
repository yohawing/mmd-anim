import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import iconv from "iconv-lite";
import { parsePmmDocumentKeyframes } from "../src/pmm-document-keyframes.mjs";
import { patchPmmDocumentVmdKeyframes } from "../src/pmm-document-vmd-patch.mjs";
import { comparePmmDocumentToVmd } from "../src/pmm-document-vmd-compare.mjs";
import { inspectVmd } from "../src/vmd-inventory.mjs";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";

test("parses PMMv2 model bone/morph keyframe sections by nanoem document layout", () => {
  const bytes = makeMinimalPmmV2();

  const report = parsePmmDocumentKeyframes(bytes);
  const model = report.models[0];

  assert.equal(report.document.formatVersion, 2);
  assert.equal(report.counts.models, 1);
  assert.equal(report.counts.boneKeyframes, 1);
  assert.equal(report.counts.morphKeyframes, 1);
  assert.equal(model.path, "F:\\Develop\\MMDDev\\data\\pmx\\Model.pmx");
  assert.deepEqual(model.boneNames, ["センター"]);
  assert.deepEqual(model.morphNames, ["まばたき"]);
  assert.equal(model.initialBoneKeyframes[0].byteLength, 58);
  assert.equal(model.boneKeyframes[0].byteLength, 62);
  assert.equal(model.boneKeyframes[0].name, "センター");
  assert.equal(model.boneKeyframes[0].frame, 30);
  assert.deepEqual(model.boneKeyframes[0].translation, [1, 2, 3]);
  assert.equal(model.initialMorphKeyframes[0].byteLength, 17);
  assert.equal(model.morphKeyframes[0].byteLength, 21);
  assert.equal(model.morphKeyframes[0].name, "まばたき");
  assert.equal(model.morphKeyframes[0].frame, 30);
  assert.equal(model.morphKeyframes[0].weight, 0.5);
});

test("parses real Tda PMM transform keyframes without VMD diff profiles", async (t) => {
  const fixture = new URL("../../data/pmm/tda_parent_center_groove_transform_keys.pmm", import.meta.url);
  if (skipMissing(t, [["Tda transform PMM", fixture]])) {
    return;
  }
  const report = parsePmmDocumentKeyframes(await readFile(fixture), { keyframeLimit: 16 });
  const model = report.models[0];

  assert.equal(report.ok, true);
  assert.equal(report.counts.models, 1);
  assert.equal(report.counts.boneKeyframes, 9);
  assert.equal(report.counts.morphKeyframes, 0);
  assert.equal(model.boneKeyframes.every((keyframe) => keyframe.byteLength === 62), true);
  assert.deepEqual(
    model.boneKeyframes.map((keyframe) => [keyframe.name, keyframe.frame, keyframe.translation]),
    [
      ["全ての親", 30, [1, 2, 3]],
      ["全ての親", 60, [4, 5, 6]],
      ["全ての親", 90, [7, 8, 9]],
      ["センター", 30, [10, 11, 12]],
      ["センター", 60, [13, 14, 15]],
      ["センター", 90, [16, 17, 18]],
      ["グルーブ", 30, [19, 20, 21]],
      ["グルーブ", 60, [22, 23, 24]],
      ["グルーブ", 90, [25, 26, 27]],
    ],
  );
});

test("compares direct PMMv2 document keyframes against VMD frames", () => {
  const pmm = parsePmmDocumentKeyframes(makeMinimalPmmV2());
  const vmd = inspectVmd(
    createSyntheticVmd({
      boneFrames: [{ name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] }],
      morphName: "まばたき",
      frame: 30,
      weight: 0.5,
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );

  const comparison = comparePmmDocumentToVmd(pmm, vmd);

  assert.equal(comparison.ok, true);
  assert.equal(comparison.counts.mismatches, 0);
  assert.equal(comparison.boneComparison.matched, 1);
  assert.equal(comparison.morphComparison.matched, 1);
});

test("compares VMD frame 0 against PMM initialBoneKeyframes / initialMorphKeyframes", () => {
  // Use a direct minimal pmm document object (bypassing incomplete synthetic binary fixture)
  // so the test proves the frame-0 initial* selection logic in comparePmmDocumentToVmd
  // without depending on parsePmmDocumentKeyframes/makeMinimalPmmV2 which lacks camera/parameters sections.
  const pmm = {
    document: { version: "0002" },
    models: [
      {
        slot: 0,
        nameJa: "テストモデル",
        path: "F:\\Develop\\MMDDev\\data\\pmx\\Model.pmx",
        counts: {
          boneKeyframes: 1,
          morphKeyframes: 1,
        },
        initialBoneKeyframes: [
          {
            name: "センター",
            frame: 0,
            translation: [0, 0, 0],
            orientation: [0, 0, 0, 1],
          },
        ],
        boneKeyframes: [
          {
            name: "センター",
            frame: 30,
            translation: [1, 2, 3],
            orientation: [0, 0, 0, 1],
          },
        ],
        initialMorphKeyframes: [
          {
            name: "まばたき",
            frame: 0,
            weight: 0,
          },
        ],
        morphKeyframes: [
          {
            name: "まばたき",
            frame: 30,
            weight: 0.5,
          },
        ],
      },
    ],
  };
  const vmd = inspectVmd(
    createSyntheticVmd({
      boneFrames: [
        { name: "センター", frame: 0, position: [0, 0, 0], rotation: [0, 0, 0, 1] },
        { name: "センター", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
      ],
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );
  vmd.morphs = [
    { name: "まばたき", frame: 0, weight: 0 },
    { name: "まばたき", frame: 30, weight: 0.5 },
  ];
  vmd.counts.morphFrames = vmd.morphs.length;

  const comparison = comparePmmDocumentToVmd(pmm, vmd);

  assert.equal(comparison.ok, true);
  assert.equal(comparison.counts.mismatches, 0);
  assert.equal(comparison.boneComparison.matched, 2);
  assert.equal(comparison.boneComparison.actual, 2);
  assert.equal(comparison.morphComparison.matched, 2);
  // counts reflect the compared PMM totals (initial selected for frame0 + additional)
  assert.equal(comparison.counts.pmmBoneKeyframes, 2);
  assert.equal(comparison.counts.pmmMorphKeyframes, 2);
});

test("compares real Tda PMM transform keyframes against VMD without base PMM diff", async (t) => {
  const pmmFixture = new URL("../../data/pmm/tda_parent_center_groove_transform_keys.pmm", import.meta.url);
  const vmdFixture = new URL("../out/pmm-analysis/tda-parent-center-groove-transform-keys.vmd", import.meta.url);
  if (skipMissing(t, [["Tda transform PMM", pmmFixture], ["Tda target VMD", vmdFixture]])) {
    return;
  }

  const pmm = parsePmmDocumentKeyframes(await readFile(pmmFixture));
  const vmd = inspectVmd(await readFile(vmdFixture), { limit: Number.MAX_SAFE_INTEGER });
  const comparison = comparePmmDocumentToVmd(pmm, vmd);

  assert.equal(comparison.ok, true);
  assert.equal(comparison.boneComparison.matched, 9);
  assert.equal(comparison.counts.mismatches, 0);
});

test("patches same-shape PMMv2 document bone and morph keyframes from VMD", () => {
  const targetVmd = inspectVmd(
    createSyntheticVmd({
      boneFrames: [{ name: "センター", frame: 45, position: [4, 5, 6], rotation: [0.1, 0.2, 0.3, 0.9] }],
      morphName: "まばたき",
      frame: 45,
      weight: 0.75,
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );

  const patched = patchPmmDocumentVmdKeyframes(makeMinimalPmmV2(), targetVmd);
  const document = parsePmmDocumentKeyframes(patched.bytes);
  const model = document.models[0];

  assert.equal(patched.ok, true);
  assert.equal(patched.rewriteCount, 2);
  assert.equal(model.boneKeyframes[0].frame, 45);
  assert.deepEqual(model.boneKeyframes[0].translation, [4, 5, 6]);
  assert.deepEqual(model.boneKeyframes[0].orientation.map((value) => Math.round(value * 10) / 10), [0.1, 0.2, 0.3, 0.9]);
  assert.equal(model.morphKeyframes[0].frame, 45);
  assert.equal(model.morphKeyframes[0].weight, 0.75);
  assert.equal(patched.comparison.ok, true);
});

test("patches PMMv2 bone interpolation from VMD interpolation bytes", async (t) => {
  const template = new URL("../../data/pmm/tda_base_no_motion.pmm", import.meta.url);
  if (skipMissing(t, [["Tda base PMM", template]])) {
    return;
  }
  const targetVmd = inspectVmd(
    createSyntheticVmd({
      boneFrames: [{ name: "センター", frame: 45, position: [4, 5, 6], rotation: [0, 0, 0, 1] }],
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );
  const interpolation = Buffer.alloc(64);
  interpolation[0] = 11;
  interpolation[4] = 12;
  interpolation[8] = 13;
  interpolation[12] = 14;
  interpolation[1] = 21;
  interpolation[5] = 22;
  interpolation[9] = 23;
  interpolation[13] = 24;
  interpolation[2] = 31;
  interpolation[6] = 32;
  interpolation[10] = 33;
  interpolation[14] = 34;
  interpolation[3] = 41;
  interpolation[7] = 42;
  interpolation[11] = 43;
  interpolation[15] = 44;
  targetVmd.bones[0].interpolationHex = interpolation.toString("hex");

  const patched = patchPmmDocumentVmdKeyframes(await readFile(template), targetVmd);
  const document = parsePmmDocumentKeyframes(patched.bytes);

  const centerKeyframe = document.models[0].boneKeyframes.find((keyframe) => keyframe.name === "センター");
  assert.deepEqual(centerKeyframe.interpolation, [
    [11, 12, 13, 14],
    [21, 22, 23, 24],
    [31, 32, 33, 34],
    [41, 42, 43, 44],
  ]);
});

test("rebuilds resized PMMv2 document bone and morph keyframe sections from VMD", () => {
  const targetVmd = inspectVmd(
    createSyntheticVmd({
      boneFrames: [
        { name: "センター", frame: 15, position: [1.5, 2.5, 3.5], rotation: [0, 0, 0, 1] },
        { name: "センター", frame: 45, position: [4, 5, 6], rotation: [0.1, 0.2, 0.3, 0.9] },
      ],
    }),
    { limit: Number.MAX_SAFE_INTEGER },
  );
  targetVmd.morphs = [
    { name: "まばたき", frame: 15, weight: 0.25 },
    { name: "まばたき", frame: 45, weight: 0.75 },
  ];
  targetVmd.counts.morphFrames = targetVmd.morphs.length;
  targetVmd.maxFrame = 45;

  const source = makeMinimalPmmV2();
  const patched = patchPmmDocumentVmdKeyframes(source, targetVmd);
  const document = parsePmmDocumentKeyframes(patched.bytes);
  const model = document.models[0];

  assert.equal(patched.ok, true);
  assert.equal(patched.byteLengthDelta, 62 + 21);
  assert.equal(model.lastFrameIndex, 45);
  assert.equal(model.initialBoneKeyframes[0].nextKeyframeIndex, 1);
  assert.deepEqual(
    model.boneKeyframes.map((keyframe) => [
      keyframe.documentObjectIndex,
      keyframe.frame,
      keyframe.previousKeyframeIndex,
      keyframe.nextKeyframeIndex,
    ]),
    [
      [1, 15, 0, 2],
      [2, 45, 1, 0],
    ],
  );
  assert.equal(model.initialMorphKeyframes[0].nextKeyframeIndex, 1);
  assert.deepEqual(
    model.morphKeyframes.map((keyframe) => [
      keyframe.documentObjectIndex,
      keyframe.frame,
      keyframe.previousKeyframeIndex,
      keyframe.nextKeyframeIndex,
    ]),
    [
      [1, 15, 0, 2],
      [2, 45, 1, 0],
    ],
  );
  assert.equal(patched.comparison.ok, true);
  assert.equal(patched.comparison.counts.mismatches, 0);
});

test("patches real Tda same-shape PMM document keyframes from a target VMD", async (t) => {
  const template = new URL("../../data/pmm/tda_parent_center_groove_transform_keys.pmm", import.meta.url);
  const targetVmdFile = new URL("../out/pmm-analysis/tda-parent-center-groove-transform-keys-target.vmd", import.meta.url);
  if (skipMissing(t, [["Tda transform PMM", template], ["Tda target VMD", targetVmdFile]])) {
    return;
  }

  const targetVmd = inspectVmd(await readFile(targetVmdFile), { limit: Number.MAX_SAFE_INTEGER });
  const patched = patchPmmDocumentVmdKeyframes(await readFile(template), targetVmd);

  assert.equal(patched.ok, true);
  assert.equal(patched.comparison.ok, true);
  assert.equal(patched.comparison.boneComparison.matched, 9);
  assert.equal(patched.comparison.counts.mismatches, 0);
});

test("grows real Tda base PMM document keyframe section from VMD without donor diff", async (t) => {
  const template = new URL("../../data/pmm/tda_base_no_motion.pmm", import.meta.url);
  const targetVmdFile = new URL("../out/pmm-analysis/tda-parent-center-groove-transform-keys-target.vmd", import.meta.url);
  if (skipMissing(t, [["Tda base PMM", template], ["Tda target VMD", targetVmdFile]])) {
    return;
  }

  const targetVmd = inspectVmd(await readFile(targetVmdFile), { limit: Number.MAX_SAFE_INTEGER });
  const patched = patchPmmDocumentVmdKeyframes(await readFile(template), targetVmd);
  const document = parsePmmDocumentKeyframes(patched.bytes);
  const model = document.models[0];

  assert.equal(patched.ok, true);
  assert.equal(patched.byteLengthDelta, 9 * 62);
  assert.deepEqual(patched.resize, {
    boneKeyframesBefore: 0,
    boneKeyframesAfter: 9,
    morphKeyframesBefore: 0,
    morphKeyframesAfter: 0,
  });
  assert.equal(model.lastFrameIndex, 91);
  assert.equal(model.initialBoneKeyframes[2].nextKeyframeIndex, 239);
  assert.equal(model.initialBoneKeyframes[3].nextKeyframeIndex, 242);
  assert.equal(model.initialBoneKeyframes[4].nextKeyframeIndex, 245);
  assert.equal(patched.comparison.ok, true);
  assert.equal(patched.comparison.boneComparison.matched, 9);
  assert.equal(patched.comparison.counts.mismatches, 0);
});

test("parses and grows a selected slot in a real two-model PMM document", async (t) => {
  const template = new URL("../../data/pmm/tda_two_models_base_no_motion.pmm", import.meta.url);
  const targetVmdFile = new URL("../out/pmm-analysis/tda-multimodel-slot-transform-keys-target.vmd", import.meta.url);
  if (skipMissing(t, [["Tda two-model PMM", template], ["Tda multimodel target VMD", targetVmdFile]])) {
    return;
  }

  const templateDocument = parsePmmDocumentKeyframes(await readFile(template));
  assert.equal(templateDocument.counts.models, 2);
  assert.equal(templateDocument.models[0].counts.boneKeyframes, 0);
  assert.equal(templateDocument.models[1].counts.boneKeyframes, 0);

  const targetVmd = inspectVmd(await readFile(targetVmdFile), { limit: Number.MAX_SAFE_INTEGER });
  const patched = patchPmmDocumentVmdKeyframes(await readFile(template), targetVmd, { targetSlot: 1 });
  const document = parsePmmDocumentKeyframes(patched.bytes);

  assert.equal(patched.ok, true);
  assert.equal(patched.byteLengthDelta, 6 * 62);
  assert.equal(document.models[0].counts.boneKeyframes, 0);
  assert.equal(document.models[1].counts.boneKeyframes, 6);
  assert.equal(document.models[1].initialBoneKeyframes[2].nextKeyframeIndex, 239);
  assert.equal(document.models[1].initialBoneKeyframes[3].nextKeyframeIndex, 241);
  assert.equal(document.models[1].initialBoneKeyframes[4].nextKeyframeIndex, 243);
  assert.equal(patched.comparison.ok, true);
  assert.equal(patched.comparison.counts.mismatches, 0);
});

test("parses and grows a selected slot in a real multi-PMX PMM document", async (t) => {
  const template = new URL("../../data/pmm/tda_sour_base_no_motion.pmm", import.meta.url);
  const targetVmdFile = new URL("../out/pmm-analysis/tda-sour-common-transform-keys-target.vmd", import.meta.url);
  if (skipMissing(t, [["Tda Sour PMM", template], ["Tda Sour target VMD", targetVmdFile]])) {
    return;
  }

  const templateDocument = parsePmmDocumentKeyframes(await readFile(template));
  assert.equal(templateDocument.counts.models, 2);
  assert.match(templateDocument.models[0].path, /Tda/u);
  assert.match(templateDocument.models[1].path, /Black\.pmx/u);

  const targetVmd = inspectVmd(await readFile(targetVmdFile), { limit: Number.MAX_SAFE_INTEGER });
  const patched = patchPmmDocumentVmdKeyframes(await readFile(template), targetVmd, { targetSlot: 1 });
  const document = parsePmmDocumentKeyframes(patched.bytes);

  assert.equal(patched.ok, true);
  assert.equal(patched.byteLengthDelta, 6 * 62);
  assert.equal(document.models[0].counts.boneKeyframes, 0);
  assert.equal(document.models[1].counts.boneKeyframes, 6);
  assert.equal(patched.comparison.pmm.modelName, "Sour_Miku_Black");
  assert.equal(patched.comparison.ok, true);
  assert.equal(patched.comparison.counts.mismatches, 0);
});

function makeMinimalPmmV2() {
  const writer = new BinaryWriter();
  writer.fixedAscii("Polygon Movie maker 0002", 30);
  writer.int32(640);
  writer.int32(360);
  writer.int32(480);
  writer.float32(30);
  writer.bytes([0, 1, 1, 1, 1, 1, 0]);
  writer.byte(0);
  writer.byte(1);

  writer.byte(0);
  writer.variableString("テストモデル");
  writer.variableString("Test Model");
  writer.fixedString("F:\\Develop\\MMDDev\\data\\pmx\\Model.pmx", 256);
  writer.byte(0);
  writer.int32(1);
  writer.variableString("センター");
  writer.int32(1);
  writer.variableString("まばたき");
  writer.int32(0);
  writer.int32(0);
  writer.byte(0);
  writer.byte(1);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.byte(0);
  writer.int32(0);
  writer.int32(30);

  writer.boneKeyframe({ frame: 0, next: 1, translation: [0, 0, 0], orientation: [0, 0, 0, 1] });
  writer.int32(1);
  writer.boneKeyframe({
    includeIndex: true,
    objectIndex: 0,
    frame: 30,
    translation: [1, 2, 3],
    orientation: [0, 0, 0, 1],
    selected: true,
  });
  writer.morphKeyframe({ frame: 0, next: 1, weight: 0 });
  writer.int32(1);
  writer.morphKeyframe({ includeIndex: true, objectIndex: 0, frame: 30, weight: 0.5, selected: true });
  writer.modelKeyframe({ frame: 0, visible: true });
  writer.int32(0);
  writer.bytes(new Uint8Array(33));
  writer.float32(0);
  writer.byte(1);
  writer.float32(1);
  writer.byte(0);
  writer.byte(0);
  appendMinimalPmmV2Tail(writer);
  return writer.buffer();
}

function appendMinimalPmmV2Tail(writer) {
  const zeroFloats = (count) => {
    for (let index = 0; index < count; index += 1) {
      writer.float32(0);
    }
  };

  // Camera: initial keyframe, no additional keyframes, and current state.
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.float32(0);
  zeroFloats(6);
  writer.int32(-1);
  writer.int32(-1);
  writer.bytes(new Uint8Array(24));
  writer.byte(1);
  writer.int32(30);
  writer.byte(0);
  writer.int32(0);
  zeroFloats(9);
  writer.byte(1);

  // Light: initial keyframe, no additional keyframes, and current state.
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  zeroFloats(6);
  writer.byte(0);
  writer.int32(0);
  zeroFloats(6);

  // Accessory list is empty.
  writer.byte(0);
  writer.int32(0);
  writer.byte(0);

  // Timeline state.
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.byte(0);
  writer.byte(0);
  writer.byte(0);
  writer.byte(0);
  writer.int32(0);
  writer.int32(0);

  // Audio, background video, and background image are disabled and empty.
  writer.byte(0);
  writer.fixedString("", 256);
  writer.int32(0);
  writer.int32(0);
  writer.float32(1);
  writer.fixedString("", 256);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.float32(1);
  writer.fixedString("", 256);
  writer.byte(0);

  // Display state.
  writer.byte(0);
  writer.byte(0);
  writer.byte(0);
  writer.float32(30);
  writer.int32(0);
  writer.int32(0);
  writer.float32(0);
  writer.byte(0);
  writer.byte(0);

  // Gravity: current state, initial keyframe, and no additional keyframes.
  writer.float32(0);
  writer.int32(0);
  zeroFloats(3);
  writer.byte(0);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.byte(0);
  writer.int32(0);
  writer.float32(0);
  zeroFloats(3);
  writer.byte(0);
  writer.int32(0);

  // Self-shadow: current state, initial keyframe, and no additional keyframes.
  writer.byte(0);
  writer.float32(0);
  writer.int32(0);
  writer.int32(0);
  writer.int32(0);
  writer.byte(0);
  writer.float32(0);
  writer.byte(0);
  writer.int32(0);
}

function skipMissing(t, entries) {
  const missing = entries.filter(([, path]) => !existsSync(path)).map(([label, path]) => `${label}: ${path}`);
  if (missing.length > 0) {
    t.skip(`External fixture unavailable (${missing.join(", ")})`);
    return true;
  }
  return false;
}

class BinaryWriter {
  constructor() {
    this.parts = [];
  }

  buffer() {
    return Buffer.concat(this.parts);
  }

  byte(value) {
    this.parts.push(Buffer.from([value & 0xff]));
  }

  bytes(values) {
    this.parts.push(Buffer.from(values));
  }

  int32(value) {
    const bytes = Buffer.alloc(4);
    bytes.writeInt32LE(value);
    this.parts.push(bytes);
  }

  float32(value) {
    const bytes = Buffer.alloc(4);
    bytes.writeFloatLE(value);
    this.parts.push(bytes);
  }

  variableString(value) {
    const encoded = iconv.encode(value, "cp932");
    this.byte(encoded.byteLength);
    this.parts.push(encoded);
  }

  fixedAscii(value, byteLength) {
    const bytes = Buffer.alloc(byteLength);
    Buffer.from(value, "ascii").copy(bytes);
    this.parts.push(bytes);
  }

  fixedString(value, byteLength) {
    const bytes = Buffer.alloc(byteLength);
    iconv.encode(value, "cp932").copy(bytes);
    this.parts.push(bytes);
  }

  baseKeyframe({ includeIndex = false, objectIndex = 0, frame, previous = 0, next = 0 }) {
    if (includeIndex) {
      this.int32(objectIndex);
    }
    this.int32(frame);
    this.int32(previous);
    this.int32(next);
  }

  boneKeyframe(options) {
    this.baseKeyframe(options);
    this.bytes([20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107]);
    for (const value of options.translation) {
      this.float32(value);
    }
    for (const value of options.orientation) {
      this.float32(value);
    }
    this.byte(options.selected ? 1 : 0);
    this.byte(options.physicsSimulationDisabled ? 1 : 0);
  }

  morphKeyframe(options) {
    this.baseKeyframe(options);
    this.float32(options.weight);
    this.byte(options.selected ? 1 : 0);
  }

  modelKeyframe(options) {
    this.baseKeyframe(options);
    this.byte(options.visible ? 1 : 0);
    this.byte(options.selected ? 1 : 0);
  }
}
