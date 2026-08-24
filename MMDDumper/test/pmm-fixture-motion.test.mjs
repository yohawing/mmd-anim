import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import {
  analyzePmmFixtureMotion,
  extractUnittestBonePositionKeysFromVmd,
  patchPmmFixtureMotion,
  patchPmmUnittestBoneKeys,
  patchPmmUnittestBoneTransformKeys,
  patchPmmUnittestVmdBoneKeys,
  rewritePmmScalars,
  writePmmUnittestBoneKeys,
} from "../src/pmm-fixture-motion.mjs";
import { extractPmmMotionRecords } from "../src/pmm-motion-records.mjs";
import { mapVmdBoneFramesToPmmRecords } from "../src/pmm-vmd-bone-map.mjs";

test("summarizes PMM fixture middle diff and motion markers", () => {
  const prefix = Buffer.from([1, 2, 3]);
  const suffix = Buffer.from([9, 8, 7]);
  const baseMiddle = Buffer.from([4, 4]);
  const variantMiddle = Buffer.concat([createCandidateRecord(30), Buffer.from([5, 5])]);
  const base = Buffer.concat([prefix, baseMiddle, suffix]);
  const variant = Buffer.concat([prefix, variantMiddle, suffix]);

  const report = analyzePmmFixtureMotion(base, variant, {
    recordByteLength: 62,
    hexLimit: 8,
    frames: [30],
  });

  assert.equal(report.diff.commonPrefixLength, 3);
  assert.equal(report.diff.commonSuffixLength, 3);
  assert.equal(report.diff.middleByteLengthDelta, 62);
  assert.equal(report.middle.variant.byteLength, 64);
  assert.deepEqual(report.middle.variant.words[3].u16, [0, 30]);
  assert.equal(report.middle.variant.interestingIntegers.some((word) => word.u16[1] === 30), true);
  assert.equal(report.motionRecords.variant.recordTotal, 1);
  assert.equal(report.valueMatches.frames[0].shiftedU32.offsets[0].offset, 15);
  assert.equal(report.motionRecords.variant.recordsInChangedMiddle, 1);
  assert.equal(report.motionRecords.variant.records[0].recordStart, 3);
});

test("finds approximate non-zero VMD float values in changed middle", () => {
  const prefix = Buffer.from([1, 2, 3]);
  const suffix = Buffer.from([9, 8, 7]);
  const base = Buffer.concat([prefix, Buffer.alloc(4), suffix]);
  const middle = Buffer.alloc(4);
  middle.writeFloatLE(0.9238795, 0);
  const variant = Buffer.concat([prefix, middle, suffix]);

  const report = analyzePmmFixtureMotion(base, variant, {
    vmd: {
      bones: [{ name: "bone", frame: 1, position: [0, 0, 0], rotation: [0, 0, 0, 0.92388] }],
      morphs: [],
      modelName: "model",
      counts: { boneFrames: 1 },
      maxFrame: 1,
    },
  });

  assert.equal(report.vmd.searches[0].rotation[0].approximateMiddleOffsets[0].relativeOffset, 0);
  assert.equal(report.middle.variant.interestingFloats[0].f32, 0.92388);
});

test("patches a PMM fixture by transplanting donor middle bytes", () => {
  const prefix = Buffer.from([1, 2, 3]);
  const suffix = Buffer.from([9, 8, 7]);
  const donorBase = Buffer.concat([prefix, Buffer.from([4, 4]), suffix]);
  const donorVariant = Buffer.concat([prefix, createCandidateRecord(30), Buffer.from([5, 5]), suffix]);

  const patched = patchPmmFixtureMotion(donorBase, donorBase, donorVariant);

  assert.equal(patched.matchesDonorVariant, true);
  assert.equal(Buffer.compare(patched.bytes, donorVariant), 0);
  assert.equal(patched.byteLengthDelta, 62);
});

test("rewrites exact frame and float scalar patterns", () => {
  const bytes = Buffer.concat([uint32le(30), uint32le(30 * 65536), float32le(1), Buffer.alloc(2)]);

  const patched = rewritePmmScalars(bytes, {
    u32At: [{ offset: 0, value: 29 }],
    float32At: [{ offset: 8, value: 6 }],
    hexAt: [{ offset: 12, hex: "aabb" }],
    insertHexAt: [{ offset: 14, hex: "ccdd" }],
    frames: [{ from: 30, to: 31 }],
    float32s: [{ from: 1, to: 7 }],
  });

  assert.equal(patched.replacementCount, 6);
  assert.equal(patched.insertionCount, 1);
  assert.equal(patched.byteLengthDelta, 2);
  assert.equal(patched.bytes.readUInt32LE(0), 29);
  assert.equal(patched.bytes.readUInt32LE(4), 31 * 65536);
  assert.equal(patched.bytes.readFloatLE(8), 6);
  assert.equal(patched.bytes.subarray(12, 14).toString("hex"), "aabb");
  assert.equal(patched.bytes.subarray(14, 16).toString("hex"), "ccdd");
});

test("writes fixture-specific unittest one-bone position keys", () => {
  const template = Buffer.alloc(0x400);

  const patched = patchPmmUnittestBoneKeys(template, {
    keys: [
      { frame: 30, position: [1, 2, 3] },
      { frame: 60, position: [4, 5, 6] },
    ],
  });

  assert.equal(patched.mode, "unittest-bone-position-keys");
  assert.equal(patched.keyCount, 2);
  assert.equal(patched.byteLengthDelta, 62);
  assert.equal(patched.bytes.readUInt32LE(0x190), 60);
  assert.equal(patched.bytes.readUInt16LE(0x1ce), 2);
  assert.equal(patched.bytes.readUInt32LE(0x1d6), 30);
  assert.equal(patched.bytes.readUInt32LE(0x20e + 6), 60);
  assert.equal(patched.bytes.readFloatLE(0x1f2), 1);
  assert.equal(patched.bytes.readFloatLE(0x1f6), 2);
  assert.equal(patched.bytes.readFloatLE(0x1fa), 3);
  assert.equal(patched.bytes.readFloatLE(0x20e + 34), 4);
  assert.equal(patched.bytes.readFloatLE(0x20e + 38), 5);
  assert.equal(patched.bytes.readFloatLE(0x20e + 42), 6);
});

test("writes experimental fixture-specific unittest follow-up position keys", () => {
  const template = Buffer.alloc(0x500);

  const patched = patchPmmUnittestBoneKeys(template, {
    keys: [
      { frame: 30, position: [1, 2, 3] },
      { frame: 60, position: [4, 5, 6] },
      { frame: 90, position: [7, 8, 9] },
    ],
  });

  assert.equal(patched.keyCount, 3);
  assert.equal(patched.byteLengthDelta, 124);
  assert.equal(patched.bytes.readUInt32LE(0x190), 90);
  assert.equal(patched.bytes.readUInt16LE(0x1ce), 3);
  assert.equal(patched.bytes.readUInt16LE(0x20e + 2), 2);
  assert.equal(patched.bytes.readUInt32LE(0x20e + 6), 60);
  assert.equal(patched.bytes.readFloatLE(0x20e + 34), 4);
  assert.equal(patched.bytes.readFloatLE(0x20e + 38), 5);
  assert.equal(patched.bytes.readFloatLE(0x20e + 42), 6);
  assert.equal(patched.bytes.readUInt16LE(0x20e + 62 + 2), 3);
  assert.equal(patched.bytes.readUInt32LE(0x20e + 62 + 6), 90);
  assert.equal(patched.bytes.readFloatLE(0x20e + 62 + 34), 7);
  assert.equal(patched.bytes.readFloatLE(0x20e + 62 + 38), 8);
  assert.equal(patched.bytes.readFloatLE(0x20e + 62 + 42), 9);
});

test("writes fixture-specific unittest rotation keys", () => {
  const template = Buffer.alloc(0x500);

  const patched = patchPmmUnittestBoneTransformKeys(template, {
    keys: [
      { frame: 30, position: [0, 0, 0], rotation: [0.382683, 0, 0, 0.92388] },
      { frame: 60, position: [0, 0, 0], rotation: [0, 0, 0.382683, 0.92388] },
    ],
  });

  assert.equal(patched.mode, "unittest-bone-transform-keys");
  assert.equal(patched.keyCount, 2);
  assert.equal(patched.byteLengthDelta, 62);
  assert.equal(patched.bytes.readUInt32LE(0x190), 60);
  assert.equal(patched.bytes.readUInt16LE(0x1ce), 2);
  assert.equal(patched.bytes.readUInt32LE(0x1d6), 30);
  assert.equal(patched.bytes.readUInt32LE(0x20e + 6), 60);
  assert.equal(patched.bytes.readFloatLE(0x1fe), Math.fround(0.382683));
  assert.equal(patched.bytes.readFloatLE(0x20e + 54), Math.fround(0.382683));
  assert.equal(patched.bytes.readFloatLE(0x20e + 58), Math.fround(0.92388));
});

test("sorts fixture-specific unittest bone keys and rejects duplicate frames", () => {
  const template = Buffer.alloc(0x500);

  const patched = patchPmmUnittestBoneKeys(template, {
    keys: [
      { frame: 90, position: [7, 8, 9] },
      { frame: 30, position: [1, 2, 3] },
      { frame: 60, position: [4, 5, 6] },
    ],
  });

  assert.deepEqual(
    patched.keys.map((key) => key.frame),
    [30, 60, 90],
  );
  assert.equal(patched.bytes.readUInt32LE(0x1d6), 30);
  assert.equal(patched.bytes.readUInt32LE(0x20e + 6), 60);
  assert.equal(patched.bytes.readUInt32LE(0x20e + 62 + 6), 90);
  assert.throws(
    () =>
      patchPmmUnittestBoneKeys(template, {
        keys: [
          { frame: 30, position: [1, 2, 3] },
          { frame: 30, position: [4, 5, 6] },
        ],
      }),
    /duplicate frame 30/,
  );
});

test("reports oracle comparison for generated unittest bone-key PMMs", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmm-oracle-"));
  const template = Buffer.alloc(0x600);
  const keys = [
    { frame: 30, position: [1, 2, 3] },
    { frame: 60, position: [4, 5, 6] },
  ];
  const expected = patchPmmUnittestBoneKeys(template, { keys });
  const templateFile = join(dir, "template.pmm");
  const oracleFile = join(dir, "oracle.pmm");
  const outFile = join(dir, "generated.pmm");
  await writeFile(templateFile, template);
  await writeFile(oracleFile, expected.bytes);

  const written = await writePmmUnittestBoneKeys({ template: templateFile, out: outFile, keys, oracle: oracleFile, validateTemplate: false });

  assert.equal(written.oracleComparison.matches, true);
  assert.equal(written.oracleComparison.byteLengthDelta, 0);
  assert.equal(written.oracleComparison.generatedSha256, expected.sha256);
  assert.equal(written.oracleComparison.oracleSha256, expected.sha256);
});

test("file writer rejects non-template PMM layouts", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmm-template-guard-"));
  const invalidTemplateFile = join(dir, "invalid-template.pmm");
  const outFile = join(dir, "generated.pmm");
  await writeFile(invalidTemplateFile, Buffer.alloc(0x600));

  await assert.rejects(
    () =>
      writePmmUnittestBoneKeys({
        template: invalidTemplateFile,
        out: outFile,
        keys: [{ frame: 30, position: [1, 2, 3] }],
      }),
    /PMM unittest writer requires/,
  );
});

test("maps generated four-key unittest PMM position candidates", () => {
  const template = Buffer.alloc(0x600);
  const keys = [
    { frame: 30, position: [1, 2, 3] },
    { frame: 60, position: [4, 5, 6] },
    { frame: 90, position: [7, 8, 9] },
    { frame: 120, position: [10, 11, 12] },
  ];

  const patched = patchPmmUnittestBoneKeys(template, { keys });
  const pmm = extractPmmMotionRecords(patched.bytes, { markerHex: "14146b6b", recordByteLength: 62, limit: 16 });
  const report = mapVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: keys.map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] })),
    },
    pmm,
    { boneName: "bone", includeRotationEvidence: false },
  );

  assert.equal(patched.keyCount, 4);
  assert.equal(patched.byteLengthDelta, 186);
  assert.equal(pmm.recordTotal, 4);
  assert.deepEqual(report.coverage.exactFrameRecordOffsets, ["0x1d4", "0x212", "0x250", "0x28e"]);
  assert.equal(report.coverage.framesWithExactFrameRecord, 4);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 4);
});

test("maps generated eight-key unittest PMM position candidates", () => {
  const template = Buffer.alloc(0x900);
  const keys = createPositionKeys(8);

  const patched = patchPmmUnittestBoneKeys(template, { keys });
  const pmm = extractPmmMotionRecords(patched.bytes, { markerHex: "14146b6b", recordByteLength: 62, limit: 32 });
  const report = mapVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: keys.map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] })),
    },
    pmm,
    { boneName: "bone", includeRotationEvidence: false },
  );

  assert.equal(patched.keyCount, 8);
  assert.equal(patched.byteLengthDelta, 7 * 62);
  assert.equal(pmm.summary.contiguousRuns[0].count, 8);
  assert.equal(report.coverage.framesWithExactFrameRecord, 8);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 8);
  assert.deepEqual(report.coverage.exactFrameRecordOffsets, ["0x1d4", "0x212", "0x250", "0x28e", "0x2cc", "0x30a", "0x348", "0x386"]);
});

test("maps generated thirty-two-key unittest PMM position candidates", () => {
  const template = Buffer.alloc(0x1200);
  const keys = createPositionKeys(32);

  const patched = patchPmmUnittestBoneKeys(template, { keys });
  const pmm = extractPmmMotionRecords(patched.bytes, { markerHex: "14146b6b", recordByteLength: 62, limit: 48 });
  const report = mapVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: keys.map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] })),
    },
    pmm,
    { boneName: "bone", includeRotationEvidence: false },
  );

  assert.equal(patched.keyCount, 32);
  assert.equal(patched.byteLengthDelta, 31 * 62);
  assert.equal(pmm.summary.contiguousRuns[0].count, 32);
  assert.equal(report.coverage.framesWithExactFrameRecord, 32);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 32);
  assert.deepEqual(report.coverage.exactFrameRecordOffsets, createExpectedRecordOffsets(32));
});

test("maps generated one-hundred-twenty-eight-key unittest PMM position candidates", () => {
  const template = Buffer.alloc(0x3000);
  const keys = createPositionKeys(128);

  const patched = patchPmmUnittestBoneKeys(template, { keys });
  const pmm = extractPmmMotionRecords(patched.bytes, { markerHex: "14146b6b", recordByteLength: 62, limit: 144 });
  const report = mapVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: keys.map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] })),
    },
    pmm,
    { boneName: "bone", includeRotationEvidence: false },
  );

  assert.equal(patched.keyCount, 128);
  assert.equal(patched.byteLengthDelta, 127 * 62);
  assert.equal(pmm.summary.contiguousRuns[0].count, 128);
  assert.equal(report.coverage.framesWithExactFrameRecord, 128);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 128);
  assert.deepEqual(report.coverage.exactFrameRecordOffsets, createExpectedRecordOffsets(128));
});

test("maps generated five-hundred-twelve-key unittest PMM position candidates", () => {
  const template = Buffer.alloc(0x9000);
  const keys = createPositionKeys(512);

  const patched = patchPmmUnittestBoneKeys(template, { keys });
  const pmm = extractPmmMotionRecords(patched.bytes, { markerHex: "14146b6b", recordByteLength: 62, limit: 528 });
  const report = mapVmdBoneFramesToPmmRecords(
    {
      modelName: "model",
      bones: keys.map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] })),
    },
    pmm,
    { boneName: "bone", includeRotationEvidence: false },
  );

  assert.equal(patched.keyCount, 512);
  assert.equal(patched.byteLengthDelta, 511 * 62);
  assert.equal(pmm.summary.contiguousRuns[0].count, 512);
  assert.equal(report.coverage.framesWithExactFrameRecord, 512);
  assert.equal(report.coverage.framesWithLocalPositionEvidence, 512);
  assert.deepEqual(report.coverage.exactFrameRecordOffsets, createExpectedRecordOffsets(512));
});

test("reports generated mapping coverage for VMD-driven unittest PMM keys", () => {
  const template = Buffer.alloc(0x600);
  const bones = [
    { name: "bone", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
    { name: "bone", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
    { name: "bone", frame: 90, position: [7, 8, 9], rotation: [0, 0, 0, 1] },
    { name: "bone", frame: 120, position: [10, 11, 12], rotation: [0, 0, 0, 1] },
  ];

  const patched = patchPmmUnittestVmdBoneKeys(template, {
    modelName: "model",
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones,
  });

  assert.equal(patched.keyCount, 4);
  assert.equal(patched.generatedMapping.structurallyVerified, true);
  assert.equal(patched.generatedMapping.coverage.framesWithExactFrameRecord, 4);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalPositionEvidence, 4);
});

test("reports generated mapping coverage for VMD-driven unittest rotation keys", () => {
  const template = Buffer.alloc(0x600);
  const bones = [
    { name: "bone", frame: 30, position: [0, 0, 0], rotation: [0.382683, 0, 0, 0.92388] },
    { name: "bone", frame: 60, position: [0, 0, 0], rotation: [0, 0, 0.382683, 0.92388] },
  ];

  const patched = patchPmmUnittestVmdBoneKeys(
    template,
    {
      modelName: "model",
      counts: {
        boneFrames: bones.length,
        morphFrames: 0,
        cameraFrames: 0,
        lightFrames: 0,
        selfShadowFrames: 0,
        propertyFrames: 0,
      },
      bones,
    },
    { allowNonIdentityRotation: true, requireGeneratedMapping: true },
  );

  assert.equal(patched.mode, "unittest-bone-transform-keys");
  assert.equal(patched.generatedMapping.layoutRecordByteLength, 62);
  assert.equal(patched.generatedMapping.layoutRecordTotal, 2);
  assert.equal(patched.generatedMapping.structurallyVerified, true);
  assert.equal(patched.generatedMapping.coverage.framesWithExactFrameRecord, 2);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalPositionEvidence, 2);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalRotationEvidence, 2);
});

test("reports generated mapping coverage for three mixed VMD-driven unittest transform keys", () => {
  const template = Buffer.alloc(0x700);
  const bones = [
    { name: "bone", frame: 30, position: [1, 2, 3], rotation: [0.382683, 0, 0, 0.92388] },
    { name: "bone", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0.382683, 0.92388] },
    { name: "bone", frame: 90, position: [7, 8, 9], rotation: [0, 0.382683, 0, 0.92388] },
  ];

  const patched = patchPmmUnittestVmdBoneKeys(
    template,
    {
      modelName: "model",
      counts: {
        boneFrames: bones.length,
        morphFrames: 0,
        cameraFrames: 0,
        lightFrames: 0,
        selfShadowFrames: 0,
        propertyFrames: 0,
      },
      bones,
    },
    { allowNonIdentityRotation: true, requireGeneratedMapping: true },
  );

  assert.equal(patched.mode, "unittest-bone-transform-keys");
  assert.equal(patched.keyCount, 3);
  assert.equal(patched.byteLengthDelta, 124);
  assert.equal(patched.generatedMapping.layoutRecordByteLength, 62);
  assert.equal(patched.generatedMapping.layoutRecordTotal, 3);
  assert.equal(patched.generatedMapping.structurallyVerified, true);
  assert.equal(patched.generatedMapping.coverage.framesWithExactFrameRecord, 3);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalPositionEvidence, 3);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalRotationEvidence, 3);
  assert.deepEqual(patched.generatedMapping.coverage.exactFrameRecordOffsets, ["0x1ce", "0x20e", "0x24c"]);
});

test("reports generated mapping coverage for eight VMD-driven unittest PMM keys", () => {
  const template = Buffer.alloc(0x900);
  const bones = createPositionKeys(8).map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] }));

  const patched = patchPmmUnittestVmdBoneKeys(template, {
    modelName: "model",
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones,
  });

  assert.equal(patched.keyCount, 8);
  assert.equal(patched.byteLengthDelta, 7 * 62);
  assert.equal(patched.generatedMapping.structurallyVerified, true);
  assert.equal(patched.generatedMapping.coverage.framesWithExactFrameRecord, 8);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalPositionEvidence, 8);
});

test("reports generated mapping coverage for thirty-two VMD-driven unittest PMM keys", () => {
  const template = Buffer.alloc(0x1200);
  const bones = createPositionKeys(32).map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] }));

  const patched = patchPmmUnittestVmdBoneKeys(template, {
    modelName: "model",
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones,
  });

  assert.equal(patched.keyCount, 32);
  assert.equal(patched.byteLengthDelta, 31 * 62);
  assert.equal(patched.generatedMapping.structurallyVerified, true);
  assert.equal(patched.generatedMapping.coverage.framesWithExactFrameRecord, 32);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalPositionEvidence, 32);
});

test("reports generated mapping coverage for one-hundred-twenty-eight VMD-driven unittest PMM keys", () => {
  const template = Buffer.alloc(0x3000);
  const bones = createPositionKeys(128).map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] }));

  const patched = patchPmmUnittestVmdBoneKeys(template, {
    modelName: "model",
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones,
  });

  assert.equal(patched.keyCount, 128);
  assert.equal(patched.byteLengthDelta, 127 * 62);
  assert.equal(patched.generatedMapping.structurallyVerified, true);
  assert.equal(patched.generatedMapping.coverage.framesWithExactFrameRecord, 128);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalPositionEvidence, 128);
});

test("reports generated mapping coverage for five-hundred-twelve VMD-driven unittest PMM keys", () => {
  const template = Buffer.alloc(0x9000);
  const bones = createPositionKeys(512).map((key) => ({ name: "bone", frame: key.frame, position: key.position, rotation: [0, 0, 0, 1] }));

  const patched = patchPmmUnittestVmdBoneKeys(template, {
    modelName: "model",
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones,
  });

  assert.equal(patched.keyCount, 512);
  assert.equal(patched.byteLengthDelta, 511 * 62);
  assert.equal(patched.generatedMapping.structurallyVerified, true);
  assert.equal(patched.generatedMapping.coverage.framesWithExactFrameRecord, 512);
  assert.equal(patched.generatedMapping.coverage.framesWithLocalPositionEvidence, 512);
});

test("can require generated mapping coverage for VMD-driven unittest PMM keys", () => {
  const template = Buffer.alloc(0x600);
  const vmd = {
    modelName: "model",
    counts: {
      boneFrames: 2,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones: [
      { name: "bone", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
      { name: "bone", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
    ],
  };

  assert.equal(patchPmmUnittestVmdBoneKeys(template, vmd, { requireGeneratedMapping: true }).generatedMapping.structurallyVerified, true);
  assert.throws(
    () => patchPmmUnittestVmdBoneKeys(template, vmd, { requireGeneratedMapping: true, generatedMarkerHex: "deadbeef" }),
    /Generated PMM key mapping verification failed/,
  );
});

test("preserves an existing fixture-specific unittest one-bone key when it already matches", () => {
  const template = Buffer.alloc(0x400);
  template.writeUInt32LE(30, 0x1d6);
  template.writeFloatLE(1, 0x1f2);
  template.writeFloatLE(2, 0x1f6);
  template.writeFloatLE(3, 0x1fa);

  const patched = patchPmmUnittestBoneKeys(template, {
    keys: [{ frame: 30, position: [1, 2, 3] }],
  });

  assert.equal(patched.preservedExistingKey, true);
  assert.equal(patched.replacementCount, 0);
  assert.equal(Buffer.compare(patched.bytes, template), 0);
});

test("extracts fixture-specific PMM keys from a position-only VMD inventory", () => {
  const keys = extractUnittestBonePositionKeysFromVmd({
    counts: {
      boneFrames: 3,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones: [
      { name: "全ての親", frame: 60, position: [4, 5, 6], rotation: [0, 0, 0, 1] },
      { name: "全ての親", frame: 30, position: [1, 2, 3], rotation: [0, 0, 0, 1] },
      { name: "全ての親", frame: 90, position: [7, 8, 9], rotation: [0, 0, 0, 1] },
    ],
  });

  assert.deepEqual(keys, [
    { name: "全ての親", frame: 30, position: [1, 2, 3] },
    { name: "全ての親", frame: 60, position: [4, 5, 6] },
    { name: "全ての親", frame: 90, position: [7, 8, 9] },
  ]);
});

test("rejects unsupported VMD channels and rotations for fixture-specific PMM keys", () => {
  const vmd = {
    counts: {
      boneFrames: 1,
      morphFrames: 1,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones: [{ name: "全ての親", frame: 30, position: [1, 2, 3], rotation: [0.1, 0, 0, 0.99] }],
  };

  assert.throws(() => extractUnittestBonePositionKeysFromVmd(vmd), /unsupported VMD channels/);
  assert.throws(() => extractUnittestBonePositionKeysFromVmd(vmd, { ignoreUnsupported: true }), /non-identity rotation/);
  assert.throws(
    () => extractUnittestBonePositionKeysFromVmd(vmd, { ignoreUnsupported: true, allowNonIdentityRotation: true }),
    /does not enable writing rotation keys/,
  );
});

function createCandidateRecord(frame) {
  const record = Buffer.alloc(62);
  record.writeUInt32LE(frame * 65536, 12);
  Buffer.from("14146b6b14146b6b14146b6b14146b6b", "hex").copy(record, 26);
  return record;
}

function createPositionKeys(count) {
  return Array.from({ length: count }, (_, index) => ({
    frame: 30 * (index + 1),
    position: [index * 3 + 1, index * 3 + 2, index * 3 + 3],
  }));
}

function createExpectedRecordOffsets(count) {
  return Array.from({ length: count }, (_, index) => `0x${(0x1d4 + index * 62).toString(16)}`);
}

function uint32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value, 0);
  return bytes;
}

function float32le(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeFloatLE(value, 0);
  return bytes;
}
