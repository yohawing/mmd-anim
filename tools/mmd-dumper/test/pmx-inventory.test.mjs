import test from "node:test";
import assert from "node:assert/strict";
import { parsePmxInventory } from "../src/pmx-inventory.mjs";

test("parses PMX bone and morph inventory", () => {
  const inventory = parsePmxInventory(makeMinimalPmx(), { path: "minimal.pmx", limit: 10 });

  assert.equal(inventory.version, 2);
  assert.equal(inventory.modelName, "テストモデル");
  assert.equal(inventory.counts.bones, 2);
  assert.equal(inventory.counts.morphs, 1);
  assert.deepEqual(
    inventory.bones.map((bone) => bone.name),
    ["センター", "上半身"],
  );
  assert.equal(inventory.morphs[0].name, "まばたき");
  assert.deepEqual(inventory.ikBoneIndices, []);
  assert.deepEqual(inventory.displayFrames, []);
});

function makeMinimalPmx() {
  const parts = [];
  const push = (buffer) => parts.push(Buffer.from(buffer));
  const textEncoder = new TextEncoder();

  push(Buffer.from("PMX ", "ascii"));
  pushFloat32(parts, 2.0);
  pushUInt8(parts, 8);
  push(Buffer.from([1, 0, 4, 4, 4, 4, 4, 4]));

  pushText(parts, textEncoder, "テストモデル");
  pushText(parts, textEncoder, "test model");
  pushText(parts, textEncoder, "");
  pushText(parts, textEncoder, "");

  pushInt32(parts, 0); // vertices
  pushInt32(parts, 0); // vertex indices
  pushInt32(parts, 0); // textures
  pushInt32(parts, 0); // materials

  pushInt32(parts, 2); // bones
  pushBone(parts, textEncoder, "センター", "center");
  pushBone(parts, textEncoder, "上半身", "upper body");

  pushInt32(parts, 1); // morphs
  pushText(parts, textEncoder, "まばたき");
  pushText(parts, textEncoder, "blink");
  pushUInt8(parts, 2); // eye panel
  pushUInt8(parts, 1); // vertex morph
  pushInt32(parts, 0); // offsets
  pushInt32(parts, 0); // display frames

  return Buffer.concat(parts);
}

function pushBone(parts, textEncoder, name, englishName) {
  pushText(parts, textEncoder, name);
  pushText(parts, textEncoder, englishName);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
  pushInt32(parts, -1); // parent bone
  pushInt32(parts, 0); // layer
  pushUInt16(parts, 0); // flags: offset connection
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
