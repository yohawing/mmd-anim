import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { normalizeFixture } from "../src/fixture.mjs";
import { validateFixtureInputs } from "../src/mmd-paths.mjs";

test("normalizes fixture-relative paths", () => {
  const fixture = normalizeFixture(
    {
      name: "sample",
      project: "scene.pmm",
      frames: [0, 30],
      output: "oracle.actual.jsonl",
    },
    "C:/work/MMDDumper/fixtures/sample/fixture.json",
  );

  assert.equal(fixture.name, "sample");
  assert.deepEqual(fixture.frames, [0, 30]);
  assert.match(fixture.project, /fixtures[\\/]sample[\\/]scene\.pmm$/);
  assert.match(fixture.output, /fixtures[\\/]sample[\\/]oracle\.actual\.jsonl$/);
  assert.match(fixture.done, /fixtures[\\/]sample[\\/]oracle\.actual\.jsonl\.done$/);
  assert.match(fixture.mmdExe, /MMDDumper[\\/]MikuMikuDance_v932x64[\\/]MikuMikuDance\.exe$/);
});

test("expands fixture frame ranges", () => {
  const fixture = normalizeFixture(
    {
      name: "sample",
      project: "scene.pmm",
      framesRange: { start: 0, end: 5, step: 2 },
      output: "oracle.actual.jsonl",
      jumpFrameIntervalMs: 40,
      jumpFrames: false,
      playback: true,
    },
    "C:/work/MMDDumper/fixtures/sample/fixture.json",
  );

  assert.deepEqual(fixture.frames, [0, 2, 4]);
  assert.equal(fixture.jumpFrameIntervalMs, 40);
  assert.equal(fixture.jumpFrames, false);
  assert.equal(fixture.playback, true);
  assert.deepEqual(fixture.framesRange, { start: 0, end: 5, step: 2 });
});

test("fixture preflight reports missing PMM model references before MMD launch", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-fixture-preflight-"));
  const project = join(dir, "scene.pmm");
  await writeFile(project, Buffer.from(`Polygon Movie maker 0002\0F:\\missing\\Model.pmx\0`, "utf8"));

  await assert.rejects(
    () =>
      validateFixtureInputs({
        mmdExe: process.execPath,
        project,
      }),
    /PMM model reference: F:\/missing\/Model\.pmx/,
  );
});
