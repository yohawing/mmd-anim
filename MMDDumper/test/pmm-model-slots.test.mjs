import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import iconv from "iconv-lite";
import { readPmmModelSlotReport } from "../src/pmm-model-slots.mjs";

test("inspects PMM model slots with PMX bone inventory and collisions", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmm-slots-"));
  const modelA = join(dir, "model-a.pmx");
  const modelB = join(dir, "model-b.pmx");
  const pmm = join(dir, "scene.pmm");
  await writeFile(modelA, makeMinimalPmx("モデルA", ["センター", "左足"]));
  await writeFile(modelB, makeMinimalPmx("モデルB", ["センター", "右足"]));
  await writeFile(pmm, makePmmBytes([modelA, modelB]));

  const report = await readPmmModelSlotReport(pmm, { limit: 8 });

  assert.equal(report.modelSlotCount, 2);
  assert.equal(report.modelSlots[0].readable, true);
  assert.equal(report.modelSlots[0].inventory.counts.bones, 2);
  assert.deepEqual(
    report.boneNameCollisions.map((collision) => collision.name),
    ["センター"],
  );
  assert.deepEqual(
    report.boneNameCollisions[0].entries.map((entry) => entry.slot),
    [0, 1],
  );
});

test("keeps duplicate model path references as separate PMM slots", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-pmm-duplicate-slots-"));
  const model = join(dir, "model-a.pmx");
  const pmm = join(dir, "scene.pmm");
  await writeFile(model, makeMinimalPmx("モデルA", ["センター"]));
  await writeFile(pmm, makePmmBytes([model, model]));

  const report = await readPmmModelSlotReport(pmm, { limit: 8 });

  assert.equal(report.modelSlotCount, 2);
  assert.deepEqual(
    report.modelSlots.map((slot) => slot.slot),
    [0, 1],
  );
  assert.deepEqual(
    report.modelSlots.map((slot) => slot.path),
    [model.replaceAll("\\", "/"), model.replaceAll("\\", "/")],
  );
  assert.deepEqual(
    report.boneNameCollisions.map((collision) => collision.name),
    ["センター"],
  );
});

function makePmmBytes(paths) {
  return Buffer.concat([
    Buffer.from("Polygon Movie maker 9.32\0", "ascii"),
    ...paths.flatMap((path) => [Buffer.from([1, 2, 3, 0]), iconv.encode(path, "cp932"), Buffer.from([0])]),
  ]);
}

function makeMinimalPmx(modelName, boneNames) {
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
