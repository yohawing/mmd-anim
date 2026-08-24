import test from "node:test";
import assert from "node:assert/strict";
import iconv from "iconv-lite";
import { parsePmdInventory } from "../src/pmd-inventory.mjs";

test("parses PMD bone and morph inventory", () => {
  const inventory = parsePmdInventory(makeMinimalPmd(), { path: "minimal.pmd", limit: 10 });

  assert.equal(inventory.format, "pmd");
  assert.equal(inventory.version, 1);
  assert.equal(inventory.modelName, "テストPMD");
  assert.equal(inventory.counts.bones, 2);
  assert.equal(inventory.counts.morphs, 1);
  assert.deepEqual(
    inventory.bones.map((bone) => bone.name),
    ["センター", "上半身"],
  );
  assert.equal(inventory.morphs[0].name, "まばたき");
});

function makeMinimalPmd() {
  const parts = [];
  parts.push(Buffer.from("Pmd", "ascii"));
  pushFloat32(parts, 1.0);
  pushFixedText(parts, "テストPMD", 20);
  pushFixedText(parts, "", 256);

  pushUInt32(parts, 0); // vertices
  pushUInt32(parts, 0); // face vertex indices
  pushUInt32(parts, 0); // materials

  pushUInt16(parts, 2); // bones
  pushBone(parts, "センター");
  pushBone(parts, "上半身");

  pushUInt16(parts, 0); // IK

  pushUInt16(parts, 1); // morphs
  pushFixedText(parts, "まばたき", 20);
  pushUInt32(parts, 0);
  pushUInt8(parts, 2);

  return Buffer.concat(parts);
}

function pushBone(parts, name) {
  pushFixedText(parts, name, 20);
  pushUInt16(parts, 0xffff);
  pushUInt16(parts, 0);
  pushUInt8(parts, 0);
  pushUInt16(parts, 0);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
  pushFloat32(parts, 0);
}

function pushFixedText(parts, value, byteLength) {
  const encoded = iconv.encode(value, "cp932");
  const bytes = Buffer.alloc(byteLength);
  encoded.copy(bytes, 0, 0, Math.min(encoded.byteLength, byteLength));
  parts.push(bytes);
}

function pushUInt8(parts, value) {
  parts.push(Buffer.from([value]));
}

function pushUInt16(parts, value) {
  const buffer = Buffer.alloc(2);
  buffer.writeUInt16LE(value);
  parts.push(buffer);
}

function pushUInt32(parts, value) {
  const buffer = Buffer.alloc(4);
  buffer.writeUInt32LE(value);
  parts.push(buffer);
}

function pushFloat32(parts, value) {
  const buffer = Buffer.alloc(4);
  buffer.writeFloatLE(value);
  parts.push(buffer);
}
