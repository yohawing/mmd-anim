import { readFile } from "node:fs/promises";
import iconv from "iconv-lite";

const signature = "Vocaloid Motion Data 0002";

export async function readVmdInventory(file, options = {}) {
  return inspectVmd(await readFile(file), options);
}

export function inspectVmd(bytes, options = {}) {
  const reader = new BinaryReader(bytes);
  const header = reader.fixedText(30);
  if (!header.startsWith(signature)) {
    throw new Error(`Invalid VMD signature: ${header}`);
  }
  const modelName = reader.fixedText(20);
  const limit = options.limit ?? 16;

  const boneFrames = readFixedFrames(reader, 111, readBoneFrame, limit);
  const morphFrames = readFixedFrames(reader, 23, readMorphFrame, limit);
  const cameraFrames = readFixedFrames(reader, 61, readCameraFrame, limit);
  const lightFrames = readFixedFrames(reader, 28, readLightFrame, limit);
  const selfShadowFrames = readFixedFrames(reader, 9, readSelfShadowFrame, limit);
  const propertyFrames = reader.remaining >= 4 ? readPropertyFrames(reader, limit) : emptyFrameSet(0);

  return {
    header,
    modelName,
    byteLength: bytes.byteLength,
    bytesRead: reader.offset,
    trailingBytes: bytes.byteLength - reader.offset,
    counts: {
      boneFrames: boneFrames.count,
      morphFrames: morphFrames.count,
      cameraFrames: cameraFrames.count,
      lightFrames: lightFrames.count,
      selfShadowFrames: selfShadowFrames.count,
      propertyFrames: propertyFrames.count,
    },
    maxFrame: Math.max(
      boneFrames.maxFrame,
      morphFrames.maxFrame,
      cameraFrames.maxFrame,
      lightFrames.maxFrame,
      selfShadowFrames.maxFrame,
      propertyFrames.maxFrame,
      0,
    ),
    bones: boneFrames.samples,
    morphs: morphFrames.samples,
    cameraFrames: cameraFrames.samples,
    lightFrames: lightFrames.samples,
    selfShadowFrames: selfShadowFrames.samples,
    propertyFrames: propertyFrames.samples,
    boneNameCounts: countBy(boneFrames.all, (frame) => frame.name),
    morphNameCounts: countBy(morphFrames.all, (frame) => frame.name),
  };
}

function readFixedFrames(reader, recordByteLength, readRecord, limit) {
  const count = reader.u32();
  const samples = [];
  const all = [];
  let maxFrame = 0;
  for (let index = 0; index < count; index += 1) {
    const recordStart = reader.offset;
    const record = readRecord(reader);
    if (reader.offset !== recordStart + recordByteLength) {
      throw new Error(`VMD parser bug: expected ${recordByteLength} byte record, consumed ${reader.offset - recordStart}`);
    }
    maxFrame = Math.max(maxFrame, record.frame ?? 0);
    all.push(record);
    if (samples.length < limit) {
      samples.push(record);
    }
  }
  return { count, samples, all, maxFrame };
}

function readBoneFrame(reader) {
  return {
    name: reader.fixedText(15),
    frame: reader.u32(),
    position: [reader.f32(), reader.f32(), reader.f32()].map(roundFloat),
    rotation: [reader.f32(), reader.f32(), reader.f32(), reader.f32()].map(roundFloat),
    interpolationHex: reader.bytes(64).toString("hex"),
  };
}

function readMorphFrame(reader) {
  return {
    name: reader.fixedText(15),
    frame: reader.u32(),
    weight: roundFloat(reader.f32()),
  };
}

function readCameraFrame(reader) {
  return {
    frame: reader.u32(),
    distance: roundFloat(reader.f32()),
    position: [reader.f32(), reader.f32(), reader.f32()].map(roundFloat),
    rotation: [reader.f32(), reader.f32(), reader.f32()].map(roundFloat),
    interpolationHex: reader.bytes(24).toString("hex"),
    fov: reader.u32(),
    perspective: reader.u8(),
  };
}

function readLightFrame(reader) {
  return {
    frame: reader.u32(),
    color: [reader.f32(), reader.f32(), reader.f32()].map(roundFloat),
    direction: [reader.f32(), reader.f32(), reader.f32()].map(roundFloat),
  };
}

function readSelfShadowFrame(reader) {
  return {
    frame: reader.u32(),
    mode: reader.u8(),
    distance: roundFloat(reader.f32()),
  };
}

function readPropertyFrames(reader, limit) {
  const count = reader.u32();
  const samples = [];
  let maxFrame = 0;
  for (let index = 0; index < count; index += 1) {
    const frame = reader.u32();
    const visible = reader.u8();
    const ikCount = reader.u32();
    const iks = [];
    for (let ikIndex = 0; ikIndex < ikCount; ikIndex += 1) {
      const ik = { name: reader.fixedText(20), enabled: reader.u8() };
      if (samples.length < limit) {
        iks.push(ik);
      }
    }
    maxFrame = Math.max(maxFrame, frame);
    if (samples.length < limit) {
      samples.push({ frame, visible, ikCount, iks });
    }
  }
  return { count, samples, all: [], maxFrame };
}

function emptyFrameSet(count) {
  return { count, samples: [], all: [], maxFrame: 0 };
}

function countBy(records, keyOf) {
  const counts = new Map();
  for (const record of records) {
    const key = keyOf(record);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([name, count]) => ({ name, count }))
    .sort((left, right) => right.count - left.count || left.name.localeCompare(right.name, "ja"))
    .slice(0, 32);
}

function roundFloat(value) {
  if (!Number.isFinite(value)) {
    return value;
  }
  return Math.round(value * 1_000_000) / 1_000_000;
}

class BinaryReader {
  constructor(bytes) {
    this.buffer = bytes;
    this.offset = 0;
  }

  get remaining() {
    return this.buffer.byteLength - this.offset;
  }

  u8() {
    this.ensure(1);
    return this.buffer[this.offset++];
  }

  u32() {
    this.ensure(4);
    const value = this.buffer.readUInt32LE(this.offset);
    this.offset += 4;
    return value;
  }

  f32() {
    this.ensure(4);
    const value = this.buffer.readFloatLE(this.offset);
    this.offset += 4;
    return value;
  }

  bytes(byteLength) {
    this.ensure(byteLength);
    const value = this.buffer.subarray(this.offset, this.offset + byteLength);
    this.offset += byteLength;
    return value;
  }

  fixedText(byteLength) {
    const raw = this.bytes(byteLength);
    const end = raw.indexOf(0);
    return iconv.decode(end >= 0 ? raw.subarray(0, end) : raw, "cp932");
  }

  ensure(byteLength) {
    if (this.offset + byteLength > this.buffer.byteLength) {
      throw new Error(`Unexpected end of VMD at offset ${this.offset}, need ${byteLength} byte(s).`);
    }
  }
}
