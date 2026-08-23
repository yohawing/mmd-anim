import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { compareVmdToPmmMotion } from "../src/vmd-pmm-compare.mjs";
import { createSyntheticVmd } from "../src/vmd-writer.mjs";

test("compares VMD frame counts to PMM motion candidates", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmd-vmd-pmm-"));
  const vmd = join(dir, "motion.vmd");
  const pmm = join(dir, "scene.pmm");
  await writeFile(vmd, createSyntheticVmd({ boneName: "センター", morphName: "まばたき" }));
  await writeFile(pmm, createPmmCandidateBytes());

  const report = await compareVmdToPmmMotion({ vmd, pmm });

  assert.equal(report.vmd.counts.boneFrames, 1);
  assert.equal(report.vmd.counts.morphFrames, 1);
  assert.equal(report.vmd.motionFrameTotal, 2);
  assert.equal(report.pmm.recordTotal, 2);
  assert.equal(report.ratios.pmmRecordsPerVmdMotionFrame, 1);
});

function createPmmCandidateBytes() {
  const record = Buffer.concat([
    Buffer.alloc(26),
    Buffer.from("14146b6b14146b6b14146b6b14146b6b", "hex"),
    Buffer.alloc(16),
  ]);
  return Buffer.concat([record, record]);
}
