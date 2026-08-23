import { readFile } from "node:fs/promises";
import iconv from "iconv-lite";

export async function readPmdInventory(path, options = {}) {
  const buffer = await readFile(path);
  return parsePmdInventory(buffer, { ...options, path });
}

export function parsePmdInventory(buffer, options = {}) {
  const reader = new PmdReader(buffer);
  const magic = reader.ascii(3);
  if (magic !== "Pmd") {
    throw new Error(`${options.path ?? "PMD"}: invalid PMD magic`);
  }

  const version = reader.float32();
  const modelName = reader.fixedText(20);
  reader.fixedText(256); // comment

  skipVertices(reader);
  reader.skip(reader.uint32() * 2); // face vertex indices
  skipMaterials(reader);
  const bones = readBones(reader);
  skipIk(reader);
  const morphs = readMorphs(reader);

  return {
    path: options.path,
    format: "pmd",
    version,
    modelName,
    modelNameEnglish: "",
    counts: {
      bones: bones.length,
      morphs: morphs.length,
    },
    bones: limitRows(bones, options.limit ?? 20),
    morphs: limitRows(morphs, options.limit ?? 20),
  };
}

class PmdReader {
  constructor(buffer) {
    this.bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
    this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
    this.offset = 0;
  }

  ascii(length) {
    this.require(length);
    const bytes = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return String.fromCharCode(...bytes);
  }

  fixedText(length) {
    this.require(length);
    const bytes = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    const nul = bytes.indexOf(0);
    const textBytes = nul >= 0 ? bytes.subarray(0, nul) : bytes;
    return iconv.decode(Buffer.from(textBytes), "cp932").replace(/\0+$/u, "");
  }

  uint8() {
    this.require(1);
    return this.view.getUint8(this.offset++);
  }

  uint16() {
    this.require(2);
    const value = this.view.getUint16(this.offset, true);
    this.offset += 2;
    return value;
  }

  uint32() {
    this.require(4);
    const value = this.view.getUint32(this.offset, true);
    this.offset += 4;
    return value;
  }

  float32() {
    this.require(4);
    const value = this.view.getFloat32(this.offset, true);
    this.offset += 4;
    return value;
  }

  skip(length) {
    this.require(length);
    this.offset += length;
  }

  require(length) {
    if (length < 0 || this.offset + length > this.bytes.byteLength) {
      throw new Error(`unexpected EOF at byte ${this.offset}`);
    }
  }
}

function skipVertices(reader) {
  const vertexCount = reader.uint32();
  reader.skip(vertexCount * 38);
}

function skipMaterials(reader) {
  const materialCount = reader.uint32();
  reader.skip(materialCount * 70);
}

function readBones(reader) {
  const boneCount = reader.uint16();
  const bones = [];
  for (let index = 0; index < boneCount; index += 1) {
    const name = reader.fixedText(20);
    reader.skip(2 + 2 + 1 + 2 + 12);
    bones.push({ index, name, nameEnglish: "" });
  }
  return bones;
}

function skipIk(reader) {
  const ikCount = reader.uint16();
  for (let index = 0; index < ikCount; index += 1) {
    reader.skip(2 + 2);
    const linkCount = reader.uint8();
    reader.skip(2 + 4 + linkCount * 2);
  }
}

function readMorphs(reader) {
  const morphCount = reader.uint16();
  const morphs = [];
  for (let index = 0; index < morphCount; index += 1) {
    const name = reader.fixedText(20);
    const vertexCount = reader.uint32();
    const type = reader.uint8();
    reader.skip(vertexCount * 16);
    morphs.push({ index, name, nameEnglish: "", panel: type, type });
  }
  return morphs;
}

function limitRows(rows, limit) {
  if (limit === Number.MAX_SAFE_INTEGER) {
    return rows;
  }
  return rows.slice(0, limit);
}
