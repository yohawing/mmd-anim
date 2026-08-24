import { readFile, writeFile } from "node:fs/promises";
import { basename } from "node:path";
import iconv from "iconv-lite";

const asciiDecoder = new TextDecoder("ascii");
const pmmHeaderPrefix = "Polygon Movie maker ";
const assetPathPattern =
  /([A-Za-z]:[\\/][^\0\r\n]*?\.(?:pmd|pmx|vmd|vac|x|wav|bmp|tga)|(?:UserFile|Model|Accessory|Motion|Wave|BackGround)[\\/][^\0\r\n]*?\.(?:pmd|pmx|vmd|vac|x|wav|bmp|tga))/gi;

export async function readPmmManifest(file) {
  return parsePmmManifest(await readFile(file));
}

export function parsePmmManifest(bytes) {
  const header = asciiDecoder.decode(bytes.subarray(0, Math.min(bytes.byteLength, 32)));
  if (!header.startsWith(pmmHeaderPrefix)) {
    throw new Error("PMM_HEADER_NOT_FOUND");
  }
  const version = header.slice(pmmHeaderPrefix.length).split("\0")[0]?.trim() ?? "";
  const references = extractPmmAssetReferences(bytes);
  return {
    signature: "Polygon Movie maker",
    version,
    byteLength: bytes.byteLength,
    assetReferences: references,
    modelSlots: createModelSlots(references),
    modelPaths: pathsByKind(references, "model"),
    motionPaths: pathsByKind(references, "motion"),
  };
}

export async function writePatchedPmmFromTemplate({ template, out, model, motion }) {
  const bytes = await readFile(template);
  const result = patchPmmAssetReferences(bytes, { model, motion });
  await writeFile(out, result.bytes);
  return {
    out,
    replacements: result.replacements,
    manifest: parsePmmManifest(result.bytes),
  };
}

export function patchPmmAssetReferences(bytes, replacements) {
  const output = Buffer.from(bytes);
  const manifest = parsePmmManifest(output);
  const applied = [];

  if (replacements.model) {
    applied.push(replaceFirstReference(output, manifest.assetReferences, "model", replacements.model));
  }
  if (replacements.motion) {
    applied.push(replaceFirstReference(output, manifest.assetReferences, "motion", replacements.motion));
  }

  return { bytes: output, replacements: applied };
}

function replaceFirstReference(bytes, references, kind, replacementPath) {
  const reference = references.find((candidate) => candidate.kind === kind);
  if (!reference) {
    throw new Error(`Template PMM does not contain a ${kind} path to replace.`);
  }
  const encoded = encodeShiftJis(replacementPath);
  if (encoded.byteLength > reference.byteLength) {
    throw new Error(
      `Replacement ${kind} path is too long for safe in-place PMM patching: ${encoded.byteLength} > ${reference.byteLength} bytes.`,
    );
  }
  bytes.fill(0, reference.offset, reference.offset + reference.byteLength);
  Buffer.from(encoded).copy(bytes, reference.offset);
  return {
    kind,
    from: reference.path,
    to: replacementPath,
    offset: reference.offset,
    byteLength: reference.byteLength,
    replacementByteLength: encoded.byteLength,
  };
}

function extractPmmAssetReferences(bytes) {
  const references = [];
  let chunkStart = 0;
  for (let index = 0; index <= bytes.byteLength; index += 1) {
    if (index < bytes.byteLength && bytes[index] !== 0) {
      continue;
    }
    if (index > chunkStart) {
      const chunk = bytes.subarray(chunkStart, index);
      const text = decodeShiftJis(chunk);
      for (const match of text.matchAll(assetPathPattern)) {
        if (match.index === undefined) {
          continue;
        }
        const rawPath = stripLeadingBinaryJunk(match[1] ?? "");
        if (!rawPath.includes("\\") && !rawPath.includes("/")) {
          continue;
        }
        const normalizedPath = normalizePmmAssetPath(rawPath);
        const pathOffset = chunkStart + encodeShiftJis(text.slice(0, match.index)).byteLength;
        const pathByteLength = encodeShiftJis(rawPath).byteLength;
        references.push(createAssetReference(rawPath, normalizedPath, pathOffset, pathByteLength));
      }
    }
    chunkStart = index + 1;
  }
  return references.sort((left, right) => left.offset - right.offset);
}

function stripLeadingBinaryJunk(value) {
  return value.replace(/^[^A-Za-z0-9_ぁ-んァ-ヶ一-龠（）()]+/, "");
}

function normalizePmmAssetPath(value) {
  const normalized = value.replaceAll("\\", "/").replace(/\/+/g, "/");
  const userFileIndex = normalized.toLowerCase().lastIndexOf("userfile/");
  return userFileIndex >= 0 ? normalized.slice(userFileIndex) : normalized;
}

function createAssetReference(path, normalizedPath, offset, byteLength) {
  const fileName = basename(normalizedPath);
  const extensionSeparatorIndex = fileName.lastIndexOf(".");
  const extension =
    extensionSeparatorIndex >= 0 ? fileName.slice(extensionSeparatorIndex + 1).toLowerCase() : "";
  return {
    path,
    normalizedPath,
    fileName,
    extension,
    kind: classifyAssetKind(extension),
    offset,
    byteLength,
  };
}

function classifyAssetKind(extension) {
  switch (extension) {
    case "pmd":
    case "pmx":
      return "model";
    case "vmd":
      return "motion";
    case "x":
    case "vac":
      return "accessory";
    case "wav":
      return "audio";
    case "bmp":
    case "tga":
      return "image";
    default:
      return "unknown";
  }
}

function pathsByKind(references, kind) {
  return references.filter((reference) => reference.kind === kind).map((reference) => reference.normalizedPath);
}

function createModelSlots(references) {
  return references
    .filter((reference) => reference.kind === "model")
    .map((reference, index) => ({
      slot: index,
      path: reference.normalizedPath,
      fileName: reference.fileName,
      offset: reference.offset,
      offsetHex: `0x${reference.offset.toString(16)}`,
      byteLength: reference.byteLength,
      note: "Provisional PMM model slot inferred from model path order.",
    }));
}

function decodeShiftJis(bytes) {
  return iconv.decode(Buffer.from(bytes), "cp932");
}

function encodeShiftJis(value) {
  return iconv.encode(value, "cp932");
}
