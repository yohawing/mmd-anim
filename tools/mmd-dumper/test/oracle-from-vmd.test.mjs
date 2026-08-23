import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { resolve } from "node:path";
import { deriveOracleFrames, prepareOracleFromVmd } from "../src/oracle-from-vmd.mjs";

test("derives sorted unique oracle frames from VMD inventory", () => {
  assert.deepEqual(
    deriveOracleFrames({
      maxFrame: 60,
      bones: [
        { frame: 30 },
        { frame: 60 },
        { frame: 30 },
      ],
      morphs: [{ frame: 15 }],
    }),
    [0, 15, 30, 60],
  );
});

test("prepares a PMM-patched oracle fixture without launching MMD", async () => {
  const template = resolve("..", "data", "pmm", "tda_base_no_motion.pmm");
  const targetVmd = resolve("out", "pmm-analysis", "tda-parent-center-groove-transform-keys-target.vmd");
  if (!existsSync(template) || !existsSync(targetVmd)) {
    return;
  }
  const outDir = resolve("out", "test-oracle-from-vmd");
  await rm(outDir, { recursive: true, force: true });

  const result = await prepareOracleFromVmd({
    templatePmm: template,
    targetVmd,
    outDir,
    targetSlot: 0,
  });

  assert.equal(result.ok, true);
  assert.equal(result.patch.ok, true);
  assert.equal(result.patch.comparison.ok, true);
  assert.equal(result.patch.comparison.counts.mismatches, 0);
  assert.deepEqual(result.frames, [0, 31, 61, 91]);
  assert.equal(existsSync(result.project), true);
  assert.equal(existsSync(result.fixturePath), true);
});
