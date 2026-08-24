import test from "node:test";
import assert from "node:assert/strict";
import iconv from "iconv-lite";
import { parsePmmManifest, patchPmmAssetReferences } from "../src/pmm-manifest.mjs";

test("extracts model and motion references from a PMM byte stream", () => {
  const bytes = makePmmBytes([
    "F:\\Develop\\MMDDev\\data\\pmx\\Model.pmx",
    "F:\\Develop\\MMDDev\\data\\vmd\\Motion.vmd",
  ]);

  const manifest = parsePmmManifest(bytes);

  assert.equal(manifest.version, "9.32");
  assert.deepEqual(manifest.modelPaths, ["F:/Develop/MMDDev/data/pmx/Model.pmx"]);
  assert.deepEqual(manifest.motionPaths, ["F:/Develop/MMDDev/data/vmd/Motion.vmd"]);
  assert.deepEqual(
    manifest.modelSlots.map((slot) => ({ slot: slot.slot, path: slot.path, fileName: slot.fileName })),
    [{ slot: 0, path: "F:/Develop/MMDDev/data/pmx/Model.pmx", fileName: "Model.pmx" }],
  );
  assert.equal(manifest.modelSlots[0].offsetHex.startsWith("0x"), true);
});

test("patches template PMM references in place", () => {
  const bytes = makePmmBytes([
    "F:\\Develop\\MMDDev\\data\\pmx\\LongModelName.pmx",
    "F:\\Develop\\MMDDev\\data\\vmd\\LongMotionName.vmd",
  ]);

  const result = patchPmmAssetReferences(bytes, {
    model: "F:\\D\\M.pmx",
    motion: "F:\\D\\A.vmd",
  });
  const manifest = parsePmmManifest(result.bytes);

  assert.deepEqual(manifest.modelPaths, ["F:/D/M.pmx"]);
  assert.deepEqual(manifest.motionPaths, ["F:/D/A.vmd"]);
  assert.equal(result.replacements.length, 2);
});

test("rejects longer replacement paths", () => {
  const bytes = makePmmBytes(["F:\\D\\M.pmx"]);

  assert.throws(
    () => patchPmmAssetReferences(bytes, { model: "F:\\Develop\\MMDDev\\data\\pmx\\TooLongForSlot.pmx" }),
    /too long/,
  );
});

function makePmmBytes(paths) {
  return Buffer.concat([
    Buffer.from("Polygon Movie maker 9.32\0", "ascii"),
    ...paths.flatMap((path) => [Buffer.from([1, 2, 3, 0]), iconv.encode(path, "cp932"), Buffer.from([0])]),
  ]);
}
