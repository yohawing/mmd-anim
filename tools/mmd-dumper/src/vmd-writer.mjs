import { mkdir, writeFile } from "node:fs/promises";
import { dirname } from "node:path";
import iconv from "iconv-lite";

const vmdHeaderText = "Vocaloid Motion Data 0002";

export async function writeSyntheticVmd(file, options = {}) {
  const bytes = createSyntheticVmd(options);
  await mkdir(dirname(file), { recursive: true });
  await writeFile(file, bytes);
  return {
    file,
    byteLength: bytes.byteLength,
    boneFrames: normalizeBoneFrames(options).length,
    morphFrames: options.morphName ? 1 : 0,
    cameraFrames: normalizeCameraFrames(options).length,
    propertyFrames: normalizePropertyFrames(options).length,
  };
}

export function createSyntheticVmd(options = {}) {
  const boneFrames = normalizeBoneFrames(options);
  const cameraFrames = normalizeCameraFrames(options);
  const propertyFrames = normalizePropertyFrames(options);
  const chunks = [
    fixedText(vmdHeaderText, 30),
    fixedText(options.modelName ?? "MMDDumper", 20),
    uint32le(boneFrames.length),
  ];
  for (const frame of boneFrames) {
    chunks.push(createBoneFrame(frame));
  }

  chunks.push(uint32le(options.morphName ? 1 : 0));
  if (options.morphName) {
    chunks.push(
      createMorphFrame({
        name: options.morphName,
        frame: options.frame ?? 30,
        weight: options.weight ?? 0.5,
      }),
    );
  }

  chunks.push(uint32le(cameraFrames.length));
  for (const frame of cameraFrames) {
    chunks.push(createCameraFrame(frame));
  }
  chunks.push(uint32le(0)); // light frames
  chunks.push(uint32le(0)); // self-shadow frames
  chunks.push(uint32le(propertyFrames.length));
  for (const frame of propertyFrames) {
    chunks.push(createPropertyFrame(frame));
  }
  return Buffer.concat(chunks);
}

function normalizeBoneFrames(options) {
  if (options.boneFrames) {
    return options.boneFrames.map((frame) => ({
      name: frame.name ?? options.boneName,
      frame: frame.frame,
      position: frame.position,
      rotation: frame.rotation ?? [0, 0, 0, 1],
    }));
  }
  if (!options.boneName) {
    return [];
  }
  return [
    {
      name: options.boneName,
      frame: options.frame ?? 30,
      position: options.position ?? [1, 2, 3],
      rotation: options.rotation ?? [0, 0, 0, 1],
    },
  ];
}

function normalizeCameraFrames(options) {
  return (options.cameraFrames ?? []).map((frame) => ({
    frame: frame.frame,
    distance: frame.distance ?? -45,
    position: frame.position ?? [0, 10, 0],
    rotation: frame.rotation ?? [0, 0, 0],
    interpolation: frame.interpolation ?? defaultCameraInterpolation(),
    fov: frame.fov ?? 30,
    perspective: frame.perspective ?? 0,
  }));
}

function normalizePropertyFrames(options) {
  return (options.propertyFrames ?? []).map((frame) => ({
    frame: frame.frame,
    visible: frame.visible ?? 1,
    iks: (frame.iks ?? []).map((ik) => ({
      name: ik.name,
      enabled: ik.enabled ? 1 : 0,
    })),
  }));
}

function createBoneFrame(options) {
  return Buffer.concat([
    fixedText(options.name, 15),
    uint32le(options.frame),
    float32le(options.position[0]),
    float32le(options.position[1]),
    float32le(options.position[2]),
    float32le(options.rotation[0]),
    float32le(options.rotation[1]),
    float32le(options.rotation[2]),
    float32le(options.rotation[3]),
    defaultInterpolation(),
  ]);
}

function createMorphFrame(options) {
  return Buffer.concat([fixedText(options.name, 15), uint32le(options.frame), float32le(options.weight)]);
}

function createCameraFrame(options) {
  return Buffer.concat([
    uint32le(options.frame),
    float32le(options.distance),
    float32le(options.position[0]),
    float32le(options.position[1]),
    float32le(options.position[2]),
    float32le(options.rotation[0]),
    float32le(options.rotation[1]),
    float32le(options.rotation[2]),
    Buffer.from(options.interpolation),
    uint32le(options.fov),
    Buffer.from([options.perspective & 0xff]),
  ]);
}

function createPropertyFrame(options) {
  return Buffer.concat([
    uint32le(options.frame),
    Buffer.from([options.visible & 0xff]),
    uint32le(options.iks.length),
    ...options.iks.flatMap((ik) => [fixedText(ik.name, 20), Buffer.from([ik.enabled & 0xff])]),
  ]);
}

function defaultInterpolation() {
  const bytes = Buffer.alloc(64);
  for (let index = 0; index < 4; index += 1) {
    const offset = index * 4;
    bytes[offset] = 20;
    bytes[offset + 1] = 20;
    bytes[offset + 2] = 107;
    bytes[offset + 3] = 107;
  }
  return bytes;
}

function defaultCameraInterpolation() {
  return Buffer.concat(Array.from({ length: 6 }, () => Buffer.from([20, 20, 107, 107])));
}

function fixedText(value, byteLength) {
  const encoded = iconv.encode(value, "cp932");
  if (encoded.byteLength > byteLength) {
    throw new Error(`Text is too long for VMD field: ${value}`);
  }
  const bytes = Buffer.alloc(byteLength);
  encoded.copy(bytes, 0);
  return bytes;
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
