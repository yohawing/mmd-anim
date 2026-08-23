import { mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import test from "node:test";
import assert from "node:assert/strict";
import { writePmmInvestigationKit } from "../src/pmm-investigation-kit.mjs";

test("writes the standard PMM investigation VMD kit", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-pmm-kit-"));
  try {
    const kit = await writePmmInvestigationKit(dir, {
      modelName: "Tda",
      boneName: "センター",
      morphName: "まばたき",
    });

    assert.equal(kit.motions.length, 3);
    assert.deepEqual(kit.suggestedSearch, { int32: [30], float32: [1, 2, 3, 0.5] });
    const manifest = JSON.parse(await readFile(kit.manifestFile, "utf8"));
    assert.equal(manifest.kind, "pmm-investigation-kit");
    assert.equal(manifest.motions[0].boneFrames, 1);
    assert.equal(manifest.motions[1].morphFrames, 1);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
