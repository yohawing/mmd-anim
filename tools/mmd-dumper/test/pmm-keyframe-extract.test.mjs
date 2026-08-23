import test from "node:test";
import assert from "node:assert/strict";
import {
  checkPmmKeyframeProfile,
  checkPmmKeyframeProfileRegistry,
  comparePmmKeyframesWithProfile,
  comparePmmVmdKeyframes,
  extractPmmKeyframesWithProfile,
  extractPmmKeyframesWithProfileRegistry,
  extractPmmVmdKeyframes,
} from "../src/pmm-keyframe-extract.mjs";

test("extracts verified PMM keyframe records using a VMD-shaped motion block", () => {
  const prefix = Buffer.alloc(100);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] }),
    makeKeyBytes({ frame: 61, position: [104, 105, 106], rotation: [0, 0, -0.382683, 0.92388] }),
    suffix,
  ]);
  const vmd = makeVmd([
    ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["センター", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
  ]);

  const report = extractPmmVmdKeyframes(base, variant, vmd, {
    modelSlots: [{ slot: 0, path: "tda.pmx", fileName: "tda.pmx", offset: 20 }],
  });

  assert.equal(report.profile.recordByteLength, 62);
  assert.equal(report.profile.recordCount, 2);
  assert.equal(report.profile.blockStart, 100);
  assert.equal(report.profile.positionOffsetInRecord, 36);
  assert.equal(report.profile.rotationOffsetInRecord, 20);
  assert.equal(report.profile.positionVerified, true);
  assert.equal(report.profile.rotationVerified, true);
  assert.equal(report.profile.modelSlotContext.slot, 0);
  assert.deepEqual(
    report.records.map((record) => [record.name, record.frame, record.position, record.rotation]),
    [
      ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
      ["センター", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
    ],
  );
});

test("extracts PMM keyframes from a verified profile without a VMD oracle", () => {
  const prefix = Buffer.alloc(100);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] }),
    makeKeyBytes({ frame: 61, position: [104, 105, 106], rotation: [0, 0, -0.382683, 0.92388] }),
    suffix,
  ]);
  const vmd = makeVmd([
    ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["センター", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
  ]);
  const profile = extractPmmVmdKeyframes(base, variant, vmd).profile;
  const decoded = extractPmmKeyframesWithProfile(variant, profile);

  assert.equal(decoded.records.length, 2);
  assert.deepEqual(
    decoded.records.map((record) => [record.name, record.frame, record.position, record.rotation]),
    [
      ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
      ["センター", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
    ],
  );
});

test("compares extracted PMM keyframes against VMD bone frames", () => {
  const prefix = Buffer.alloc(100);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] }),
    makeKeyBytes({ frame: 61, position: [104, 105, 106], rotation: [0, 0, -0.382683, 0.92388] }),
    suffix,
  ]);
  const matching = makeVmd([
    ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["センター", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
  ]);
  const mismatching = makeVmd([
    ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["センター", 61, [104, 105, 107], [0, 0, -0.382683, 0.92388]],
  ]);

  const ok = comparePmmVmdKeyframes(base, variant, matching);
  const ng = comparePmmVmdKeyframes(base, variant, mismatching);

  assert.equal(ok.ok, true);
  assert.equal(ok.mismatchCount, 0);
  assert.equal(ng.ok, false);
  assert.equal(ng.mismatchCount, 2);
  assert.deepEqual(
    ng.mismatches.map((mismatch) => mismatch.kind),
    ["position-missing", "position-missing"],
  );
  assert.deepEqual(ng.mismatches[0].expected, [101, 102, 103]);
});

test("compares profile-decoded PMM keyframes against VMD bone frames", () => {
  const prefix = Buffer.alloc(100);
  const suffix = Buffer.from([9, 8, 7, 6]);
  const base = Buffer.concat([prefix, Buffer.from([0xaa, 0xbb]), suffix]);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] }),
    makeKeyBytes({ frame: 61, position: [104, 105, 106], rotation: [0, 0, -0.382683, 0.92388] }),
    suffix,
  ]);
  const matching = makeVmd([
    ["全ての親", 31, [101, 102, 103], [0, 0.382683, 0, 0.92388]],
    ["センター", 61, [104, 105, 106], [0, 0, -0.382683, 0.92388]],
  ]);
  const profile = extractPmmVmdKeyframes(base, variant, matching, {
    positionOffsetInRecord: 36,
    rotationOffsetInRecord: 20,
  }).profile;
  const report = comparePmmKeyframesWithProfile(variant, profile, matching);

  assert.equal(report.ok, true);
  assert.equal(report.mismatchCount, 0);
  assert.deepEqual(report.counts, { pmmRecords: 2, vmdBoneFrames: 2 });
});

test("checks profile record offsets before profile-based decode", () => {
  const prefix = Buffer.alloc(100);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] }),
  ]);
  const profile = {
    verified: true,
    recordByteLength: 62,
    recordCount: 1,
    blockStart: 100,
    blockEnd: 162,
    frameOffsetInRecord: 8,
    positionOffsetInRecord: 36,
    rotationOffsetInRecord: 20,
    records: [{ index: 0, name: "全ての親", recordStart: 100 }],
  };
  const ok = checkPmmKeyframeProfile(variant, profile);
  const outOfRange = checkPmmKeyframeProfile(variant.subarray(0, 120), profile);

  assert.equal(ok.ok, true);
  assert.equal(ok.recordsChecked, 1);
  assert.deepEqual(ok.reasons, []);
  assert.equal(outOfRange.ok, false);
  assert.match(outOfRange.reasons.join("\n"), /blockEnd is outside PMM bytes|position offset is out/);
});

test("ranks reusable PMM keyframe profiles from a registry", () => {
  const prefix = Buffer.alloc(100);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] }),
  ]);
  const validProfile = {
    verified: true,
    recordByteLength: 62,
    recordCount: 1,
    blockStart: 100,
    blockEnd: 162,
    frameOffsetInRecord: 8,
    positionOffsetInRecord: 36,
    rotationOffsetInRecord: 20,
    records: [{ index: 0, name: "全ての親", recordStart: 100 }],
  };
  const invalidProfile = {
    ...validProfile,
    recordCount: 2,
  };
  const report = checkPmmKeyframeProfileRegistry(variant, {
    profiles: [
      { id: "invalid-count", profile: invalidProfile },
      { id: "valid-one-key", profile: validProfile },
    ],
  });

  assert.equal(report.ok, true);
  assert.equal(report.profileCount, 2);
  assert.equal(report.candidates[0].id, "valid-one-key");
  assert.equal(report.candidates[0].ok, true);
  assert.equal(report.candidates[1].ok, false);
  assert.match(report.candidates[1].reasons.join("\n"), /recordCount 2/);
});

test("extracts PMM keyframes with the best matching registry profile", () => {
  const prefix = Buffer.alloc(100);
  const variant = Buffer.concat([
    prefix,
    makeKeyBytes({ frame: 31, position: [101, 102, 103], rotation: [0, 0.382683, 0, 0.92388] }),
  ]);
  const validProfile = {
    verified: true,
    recordByteLength: 62,
    recordCount: 1,
    blockStart: 100,
    blockEnd: 162,
    frameOffsetInRecord: 8,
    positionOffsetInRecord: 36,
    rotationOffsetInRecord: 20,
    records: [{ index: 0, name: "全ての親", recordStart: 100 }],
  };
  const report = extractPmmKeyframesWithProfileRegistry(variant, {
    profiles: [
      { id: "bad", profile: { ...validProfile, recordCount: 2 } },
      { id: "good", profile: validProfile },
    ],
  });

  assert.equal(report.ok, true);
  assert.equal(report.selectedProfile.id, "good");
  assert.deepEqual(report.records.map((record) => [record.name, record.frame, record.position]), [
    ["全ての親", 31, [101, 102, 103]],
  ]);
});

function makeVmd(entries) {
  const bones = entries.map(([name, frame, position, rotation = [0, 0, 0, 1]]) => ({ name, frame, position, rotation }));
  return {
    modelName: "Tda",
    maxFrame: Math.max(...bones.map((bone) => bone.frame)),
    counts: {
      boneFrames: bones.length,
      morphFrames: 0,
      cameraFrames: 0,
      lightFrames: 0,
      selfShadowFrames: 0,
      propertyFrames: 0,
    },
    bones,
  };
}

function makeKeyBytes({ frame, position, rotation = [0, 0, 0, 1] }) {
  const bytes = Buffer.alloc(62);
  bytes.writeUInt32LE(frame, 8);
  bytes.writeFloatLE(rotation[0], 20);
  bytes.writeFloatLE(rotation[1], 24);
  bytes.writeFloatLE(rotation[2], 28);
  bytes.writeFloatLE(rotation[3], 32);
  bytes.writeFloatLE(position[0], 36);
  bytes.writeFloatLE(position[1], 40);
  bytes.writeFloatLE(position[2], 44);
  return bytes;
}
