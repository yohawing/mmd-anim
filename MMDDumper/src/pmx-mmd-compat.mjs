import { readFile, writeFile } from "node:fs/promises";
import { extname } from "node:path";

const textDecoders = {
  0: new TextDecoder("utf-16le"),
  1: new TextDecoder("utf-8"),
};

export async function stageMmdCompatiblePmx(input, output) {
  if (extname(input).toLowerCase() !== ".pmx") {
    return { input, output: input, converted: false, encoding: "not-pmx" };
  }
  const bytes = await readFile(input);
  validatePmxHeader(bytes);
  const encoding = bytes[9];
  if (encoding === 0) {
    return { input, output: input, converted: false, encoding: "utf-16le" };
  }
  if (encoding !== 1) {
    throw new Error(`Unsupported PMX text encoding byte: ${encoding}`);
  }
  const converted = convertPmxUtf8ToUtf16(bytes);
  await writeFile(output, converted);
  return { input, output, converted: true, encoding: "utf-8" };
}

function validatePmxHeader(bytes) {
  if (bytes.byteLength < 9 || bytes.toString("ascii", 0, 4) !== "PMX ") {
    throw new Error("Invalid PMX header.");
  }
  const headerSize = bytes[8];
  if (headerSize < 8 || bytes.byteLength < 9 + headerSize) {
    throw new Error(`Invalid PMX header size: ${headerSize}.`);
  }
}

export function convertPmxUtf8ToUtf16(bytes) {
  const reader = new PmxReader(bytes);
  const chunks = [];
  copy(reader, chunks, 4); // magic
  copy(reader, chunks, 4); // version
  const headerSize = reader.uint8();
  chunks.push(Buffer.from([headerSize]));
  if (headerSize < 8) {
    throw new Error(`Unsupported PMX header size: ${headerSize}`);
  }
  const encoding = reader.uint8();
  if (encoding !== 1) {
    throw new Error(`Expected UTF-8 PMX encoding byte, got ${encoding}`);
  }
  chunks.push(Buffer.from([0]));
  const settingsBytes = reader.take(headerSize - 1);
  chunks.push(settingsBytes);
  const settings = {
    encoding,
    additionalUvCount: settingsBytes[0] ?? 0,
    vertexIndexSize: settingsBytes[1] ?? 4,
    textureIndexSize: settingsBytes[2] ?? 4,
    materialIndexSize: settingsBytes[3] ?? 4,
    boneIndexSize: settingsBytes[4] ?? 4,
    morphIndexSize: settingsBytes[5] ?? 4,
    rigidBodyIndexSize: settingsBytes[6] ?? 4,
  };

  for (let i = 0; i < 4; i += 1) {
    convertText(reader, chunks, settings.encoding);
  }
  copyVertices(reader, chunks, settings);
  copyCountedFixedSection(reader, chunks, settings.vertexIndexSize);
  copyTextSection(reader, chunks, settings);
  copyMaterials(reader, chunks, settings);
  copyBones(reader, chunks, settings);
  copyMorphs(reader, chunks, settings);
  copyDisplayFrames(reader, chunks, settings);
  copyRigidBodies(reader, chunks, settings);
  copyJoints(reader, chunks, settings);
  if (reader.remaining > 0) {
    copySoftBodies(reader, chunks, settings);
  }
  if (reader.remaining > 0) {
    copy(reader, chunks, reader.remaining);
  }
  return Buffer.concat(chunks);
}

class PmxReader {
  constructor(bytes) {
    this.bytes = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
    this.offset = 0;
  }

  get remaining() {
    return this.bytes.byteLength - this.offset;
  }

  require(length) {
    if (length < 0 || this.offset + length > this.bytes.byteLength) {
      throw new Error(`unexpected EOF at byte ${this.offset}`);
    }
  }

  take(length) {
    this.require(length);
    const chunk = Buffer.from(this.bytes.subarray(this.offset, this.offset + length));
    this.offset += length;
    return chunk;
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

  int32() {
    this.require(4);
    const value = this.view.getInt32(this.offset, true);
    this.offset += 4;
    return value;
  }
}

function copy(reader, chunks, length) {
  chunks.push(reader.take(length));
}

function copyInt32(reader, chunks) {
  const start = reader.offset;
  const value = reader.int32();
  chunks.push(Buffer.from(reader.bytes.subarray(start, start + 4)));
  return value;
}

function copyUint8(reader, chunks) {
  const value = reader.uint8();
  chunks.push(Buffer.from([value]));
  return value;
}

function copyUint16(reader, chunks) {
  const start = reader.offset;
  const value = reader.uint16();
  chunks.push(Buffer.from(reader.bytes.subarray(start, start + 2)));
  return value;
}

function convertText(reader, chunks, encoding) {
  const byteLength = reader.int32();
  reader.require(byteLength);
  const source = reader.bytes.subarray(reader.offset, reader.offset + byteLength);
  reader.offset += byteLength;
  const text = textDecoders[encoding].decode(source);
  const converted = Buffer.from(text, "utf16le");
  const length = Buffer.alloc(4);
  length.writeInt32LE(converted.byteLength);
  chunks.push(length, converted);
}

function copyVertices(reader, chunks, settings) {
  const startOffset = reader.offset;
  const vertexCount = reader.int32();
  const candidates = uniqueNumbers([settings.boneIndexSize, 1, 2, 4]);
  let endOffset = undefined;
  let lastError = undefined;
  for (const boneIndexSize of candidates) {
    reader.offset = startOffset + 4;
    try {
      for (let i = 0; i < vertexCount; i += 1) {
        skipVertex(reader, settings, boneIndexSize);
      }
      if (isPostVertexSectionPlausible(reader, settings)) {
        endOffset = reader.offset;
        break;
      }
    } catch (error) {
      lastError = error;
    }
  }
  if (endOffset === undefined) {
    reader.offset = startOffset;
    if (lastError instanceof Error) {
      throw lastError;
    }
    throw new Error("unable to skip PMX vertex payload");
  }
  chunks.push(Buffer.from(reader.bytes.subarray(startOffset, endOffset)));
  reader.offset = endOffset;
}

function skipVertex(reader, settings, boneIndexSize) {
  reader.require(12 + 12 + 8 + settings.additionalUvCount * 16);
  reader.offset += 12 + 12 + 8 + settings.additionalUvCount * 16;
  const weightType = reader.uint8();
  if (weightType === 0) {
    reader.offset += boneIndexSize;
  } else if (weightType === 1) {
    reader.offset += boneIndexSize * 2 + 4;
  } else if (weightType === 2 || weightType === 4) {
    reader.offset += boneIndexSize * 4 + 16;
  } else if (weightType === 3) {
    reader.offset += boneIndexSize * 2 + 4 + 36;
  } else {
    throw new Error(`unsupported PMX vertex weight type: ${weightType}`);
  }
  reader.offset += 4;
  reader.require(0);
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

function copyCountedFixedSection(reader, chunks, indexSize) {
  const count = copyInt32(reader, chunks);
  copy(reader, chunks, count * indexSize);
}

function copyTextSection(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
  }
}

function copyMaterials(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, 16 + 12 + 4 + 12 + 1 + 16 + 4);
    copy(reader, chunks, settings.textureIndexSize);
    copy(reader, chunks, settings.textureIndexSize);
    copy(reader, chunks, 1);
    const toonFlag = copyUint8(reader, chunks);
    copy(reader, chunks, toonFlag === 0 ? settings.textureIndexSize : 1);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, 4);
  }
}

function copyBones(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, 12 + settings.boneIndexSize + 4);
    const flags = copyUint16(reader, chunks);
    if ((flags & 0x0001) !== 0) {
      copy(reader, chunks, settings.boneIndexSize);
    } else {
      copy(reader, chunks, 12);
    }
    if ((flags & 0x0100) !== 0 || (flags & 0x0200) !== 0) {
      copy(reader, chunks, settings.boneIndexSize + 4);
    }
    if ((flags & 0x0400) !== 0) {
      copy(reader, chunks, 12);
    }
    if ((flags & 0x0800) !== 0) {
      copy(reader, chunks, 24);
    }
    if ((flags & 0x2000) !== 0) {
      copy(reader, chunks, 4);
    }
    if ((flags & 0x0020) !== 0) {
      copy(reader, chunks, settings.boneIndexSize + 8);
      const linkCount = copyInt32(reader, chunks);
      for (let link = 0; link < linkCount; link += 1) {
        copy(reader, chunks, settings.boneIndexSize);
        const hasLimit = copyUint8(reader, chunks);
        if (hasLimit !== 0) {
          copy(reader, chunks, 24);
        }
      }
    }
  }
}

function copyMorphs(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, 1);
    const type = copyUint8(reader, chunks);
    const offsetCount = copyInt32(reader, chunks);
    for (let offset = 0; offset < offsetCount; offset += 1) {
      copyMorphOffset(reader, chunks, settings, type);
    }
  }
}

function copyMorphOffset(reader, chunks, settings, type) {
  if (type === 0 || type === 9) {
    copy(reader, chunks, settings.morphIndexSize + 4);
  } else if (type === 1) {
    copy(reader, chunks, settings.vertexIndexSize + 12);
  } else if (type === 2) {
    copy(reader, chunks, settings.boneIndexSize + 28);
  } else if (type >= 3 && type <= 7) {
    copy(reader, chunks, settings.vertexIndexSize + 16);
  } else if (type === 8) {
    copy(reader, chunks, settings.materialIndexSize + 1 + 16 + 12 + 4 + 12 + 16 + 4 + 16 + 16 + 16);
  } else if (type === 10) {
    copy(reader, chunks, settings.rigidBodyIndexSize + 1 + 12 + 12);
  } else {
    throw new Error(`unsupported PMX morph type: ${type}`);
  }
}

function copyDisplayFrames(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, 1);
    const itemCount = copyInt32(reader, chunks);
    for (let item = 0; item < itemCount; item += 1) {
      const type = copyUint8(reader, chunks);
      copy(reader, chunks, type === 0 ? settings.boneIndexSize : settings.morphIndexSize);
    }
  }
}

function copyRigidBodies(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, settings.boneIndexSize + 1 + 2 + 1 + 36 + 20 + 1);
  }
}

function copyJoints(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, 1 + settings.rigidBodyIndexSize * 2 + 96);
  }
}

function copySoftBodies(reader, chunks, settings) {
  const count = copyInt32(reader, chunks);
  for (let i = 0; i < count; i += 1) {
    convertText(reader, chunks, settings.encoding);
    convertText(reader, chunks, settings.encoding);
    copy(reader, chunks, 1 + settings.materialIndexSize + 1 + 2 + 1 + 4 + 4 + 4 + 4 + 4 + 48 + 24 + 16 + 12);
    const anchorCount = copyInt32(reader, chunks);
    for (let anchor = 0; anchor < anchorCount; anchor += 1) {
      copy(reader, chunks, settings.rigidBodyIndexSize + settings.vertexIndexSize + 1);
    }
    const pinnedCount = copyInt32(reader, chunks);
    copy(reader, chunks, pinnedCount * settings.vertexIndexSize);
  }
}

function uniqueNumbers(values) {
  return [...new Set(values.filter((value) => Number.isInteger(value)))];
}
