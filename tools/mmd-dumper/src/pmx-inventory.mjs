import { readFile } from "node:fs/promises";

export async function readPmxInventory(path, options = {}) {
  const buffer = await readFile(path);
  return parsePmxInventory(buffer, { ...options, path });
}

export function parsePmxInventory(buffer, options = {}) {
  const reader = new PmxReader(buffer);
  const magic = reader.ascii(4);
  if (magic !== "PMX ") {
    throw new Error(`${options.path ?? "PMX"}: invalid PMX magic`);
  }

  const version = reader.float32();
  const globalCount = reader.uint8();
  const globals = Array.from({ length: globalCount }, () => reader.uint8());
  const settings = {
    encoding: globals[0] === 0 ? "utf-16le" : "utf-8",
    additionalUvCount: globals[1] ?? 0,
    vertexIndexSize: globals[2] ?? 4,
    textureIndexSize: globals[3] ?? 4,
    materialIndexSize: globals[4] ?? 4,
    boneIndexSize: globals[5] ?? 4,
    morphIndexSize: globals[6] ?? 4,
    rigidBodyIndexSize: globals[7] ?? 4,
  };

  const modelName = reader.text(settings.encoding);
  const modelNameEnglish = reader.text(settings.encoding);
  reader.text(settings.encoding);
  reader.text(settings.encoding);

  skipVertices(reader, settings);
  reader.skip(reader.int32() * settings.vertexIndexSize);
  skipTextures(reader, settings);
  skipMaterials(reader, settings);
  const bones = readBones(reader, settings);
  const morphs = readMorphs(reader, settings);
  const displayFrames = readDisplayFrames(reader, settings);

  return {
    path: options.path,
    version,
    modelName,
    modelNameEnglish,
    counts: {
      bones: bones.length,
      morphs: morphs.length,
    },
    bones: limitRows(bones, options.limit ?? 20),
    morphs: limitRows(morphs, options.limit ?? 20),
    ikBoneIndices: bones.filter((bone) => (bone.flags & 0x0020) !== 0).map((bone) => bone.index),
    displayFrames: limitRows(displayFrames, options.limit ?? 20),
  };
}

class PmxReader {
  constructor(buffer) {
    this.bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
    this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
    this.offset = 0;
  }

  ascii(length) {
    this.require(length);
    const value = String.fromCharCode(...this.bytes.subarray(this.offset, this.offset + length));
    this.offset += length;
    return value;
  }

  text(encoding) {
    const length = this.int32();
    this.require(length);
    const bytes = this.bytes.subarray(this.offset, this.offset + length);
    this.offset += length;
    return new TextDecoder(encoding).decode(bytes).replace(/\0+$/u, "");
  }

  uint8() {
    this.require(1);
    return this.view.getUint8(this.offset++);
  }

  int8() {
    this.require(1);
    return this.view.getInt8(this.offset++);
  }

  uint16() {
    this.require(2);
    const value = this.view.getUint16(this.offset, true);
    this.offset += 2;
    return value;
  }

  int16() {
    this.require(2);
    const value = this.view.getInt16(this.offset, true);
    this.offset += 2;
    return value;
  }

  int32() {
    this.require(4);
    const value = this.view.getInt32(this.offset, true);
    this.offset += 4;
    return value;
  }

  float32() {
    this.require(4);
    const value = this.view.getFloat32(this.offset, true);
    this.offset += 4;
    return value;
  }

  index(size) {
    if (size === 1) {
      return this.int8();
    }
    if (size === 2) {
      return this.int16();
    }
    if (size === 4) {
      return this.int32();
    }
    throw new Error(`unsupported PMX index size: ${size}`);
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

function skipVertices(reader, settings) {
  const startOffset = reader.offset;
  const candidateBoneIndexSizes = uniqueNumbers([settings.boneIndexSize, 1, 2, 4]);
  const vertexCount = reader.int32();
  let lastError;
  for (const boneIndexSize of candidateBoneIndexSizes) {
    reader.offset = startOffset + 4;
    try {
      for (let i = 0; i < vertexCount; i += 1) {
        skipVertex(reader, settings, boneIndexSize);
      }
      if (isPostVertexSectionPlausible(reader, settings)) {
        return;
      }
    } catch (error) {
      lastError = error;
    }
  }
  reader.offset = startOffset;
  if (lastError instanceof Error) {
    throw lastError;
  }
  throw new Error("unable to skip PMX vertex payload");
}

function skipVertex(reader, settings, boneIndexSize) {
  reader.skip(12 + 12 + 8 + settings.additionalUvCount * 16);
  const weightType = reader.uint8();
  if (weightType === 0) {
    reader.index(boneIndexSize);
  } else if (weightType === 1) {
    reader.index(boneIndexSize);
    reader.index(boneIndexSize);
    reader.skip(4);
  } else if (weightType === 2 || weightType === 4) {
    for (let j = 0; j < 4; j += 1) {
      reader.index(boneIndexSize);
    }
    reader.skip(16);
  } else if (weightType === 3) {
    reader.index(boneIndexSize);
    reader.index(boneIndexSize);
    reader.skip(4 + 36);
  } else {
    throw new Error(`unsupported PMX vertex weight type: ${weightType}`);
  }
  reader.skip(4);
}

function isPostVertexSectionPlausible(reader, settings) {
  const vertexIndexCount = peekInt32(reader, reader.offset);
  if (vertexIndexCount === undefined || vertexIndexCount < 0) {
    return false;
  }
  const vertexIndexBytes = vertexIndexCount * settings.vertexIndexSize;
  if (!Number.isSafeInteger(vertexIndexBytes)) {
    return false;
  }
  const textureCountOffset = reader.offset + 4 + vertexIndexBytes;
  const textureCount = peekInt32(reader, textureCountOffset);
  return textureCount !== undefined && textureCount >= 0;
}

function peekInt32(reader, offset) {
  if (offset < 0 || offset + 4 > reader.bytes.byteLength) {
    return undefined;
  }
  return reader.view.getInt32(offset, true);
}

function uniqueNumbers(values) {
  return [...new Set(values.filter((value) => Number.isInteger(value)))];
}

function skipTextures(reader, settings) {
  const textureCount = reader.int32();
  for (let i = 0; i < textureCount; i += 1) {
    reader.text(settings.encoding);
  }
}

function skipMaterials(reader, settings) {
  const materialCount = reader.int32();
  for (let i = 0; i < materialCount; i += 1) {
    reader.text(settings.encoding);
    reader.text(settings.encoding);
    reader.skip(4 * 4 + 3 * 4 + 4 + 3 * 4 + 1 + 4 * 4 + 4);
    reader.index(settings.textureIndexSize);
    reader.index(settings.textureIndexSize);
    reader.skip(1);
    const toonFlag = reader.uint8();
    if (toonFlag === 0) {
      reader.index(settings.textureIndexSize);
    } else {
      reader.skip(1);
    }
    reader.text(settings.encoding);
    reader.skip(4);
  }
}

function readBones(reader, settings) {
  const boneCount = reader.int32();
  const bones = [];
  for (let i = 0; i < boneCount; i += 1) {
    const name = reader.text(settings.encoding);
    const nameEnglish = reader.text(settings.encoding);
    reader.skip(12);
    const parentIndex = reader.index(settings.boneIndexSize);
    const deformLayer = reader.int32();
    const flags = reader.uint16();
    if ((flags & 0x0001) !== 0) {
      reader.index(settings.boneIndexSize);
    } else {
      reader.skip(12);
    }
    if ((flags & 0x0100) !== 0 || (flags & 0x0200) !== 0) {
      reader.index(settings.boneIndexSize);
      reader.skip(4);
    }
    if ((flags & 0x0400) !== 0) {
      reader.skip(12);
    }
    if ((flags & 0x0800) !== 0) {
      reader.skip(24);
    }
    if ((flags & 0x2000) !== 0) {
      reader.skip(4);
    }
    if ((flags & 0x0020) !== 0) {
      reader.index(settings.boneIndexSize);
      reader.skip(4 + 4);
      const linkCount = reader.int32();
      for (let link = 0; link < linkCount; link += 1) {
        reader.index(settings.boneIndexSize);
        const hasLimit = reader.uint8();
        if (hasLimit !== 0) {
          reader.skip(24);
        }
      }
    }
    bones.push({ index: i, name, nameEnglish, parentIndex, deformLayer, flags });
  }
  return bones;
}

function readMorphs(reader, settings) {
  const morphCount = reader.int32();
  const morphs = [];
  for (let i = 0; i < morphCount; i += 1) {
    const name = reader.text(settings.encoding);
    const nameEnglish = reader.text(settings.encoding);
    const panel = reader.uint8();
    const type = reader.uint8();
    const offsetCount = reader.int32();
    skipMorphOffsets(reader, settings, type, offsetCount);
    morphs.push({ index: i, name, nameEnglish, panel, type });
  }
  return morphs;
}

function skipMorphOffsets(reader, settings, type, offsetCount) {
  for (let i = 0; i < offsetCount; i += 1) {
    if (type === 0 || type === 9) {
      reader.index(settings.morphIndexSize);
      reader.skip(4);
    } else if (type === 1) {
      reader.index(settings.vertexIndexSize);
      reader.skip(12);
    } else if (type === 2) {
      reader.index(settings.boneIndexSize);
      reader.skip(12 + 16);
    } else if (type >= 3 && type <= 7) {
      reader.index(settings.vertexIndexSize);
      reader.skip(16);
    } else if (type === 8) {
      reader.index(settings.materialIndexSize);
      reader.skip(1 + 16 + 12 + 4 + 12 + 16 + 4 + 16 + 16 + 16);
    } else if (type === 10) {
      reader.index(settings.rigidBodyIndexSize);
      reader.skip(1 + 12 + 12);
    } else {
      throw new Error(`unsupported PMX morph type: ${type}`);
    }
  }
}

function readDisplayFrames(reader, settings) {
  const displayFrameCount = reader.int32();
  const displayFrames = [];
  for (let i = 0; i < displayFrameCount; i += 1) {
    const name = reader.text(settings.encoding);
    const nameEnglish = reader.text(settings.encoding);
    const special = reader.uint8();
    const itemCount = reader.int32();
    const items = [];
    for (let itemIndex = 0; itemIndex < itemCount; itemIndex += 1) {
      const type = reader.uint8();
      const index = type === 0 ? reader.index(settings.boneIndexSize) : reader.index(settings.morphIndexSize);
      items.push({ type, index });
    }
    displayFrames.push({ index: i, name, nameEnglish, special, items });
  }
  return displayFrames;
}

function limitRows(rows, limit) {
  if (limit === 0) {
    return [];
  }
  return rows.slice(0, limit);
}
