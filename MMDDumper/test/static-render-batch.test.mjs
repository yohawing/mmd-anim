import test from "node:test";
import assert from "node:assert/strict";
import { existsSync } from "node:fs";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { prepareStaticRenderBatch } from "../src/static-render-batch.mjs";

test("prepares a static render batch from a template PMM without launching MMD", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-static-render-"));
  const pmx = join(dir, "model.pmx");
  const vmd = join(dir, "motion.vmd");
  const templatePmm = join(dir, "template.pmm");
  const outDir = join(dir, "out");
  const manifestPath = join(dir, "static-render.json");
  await mkdir(outDir, { recursive: true });
  await writeFile(pmx, "");
  await writeFile(vmd, "");
  await writeFile(templatePmm, "template-pmm");
  await writeFile(
    manifestPath,
    `${JSON.stringify(
      {
        defaults: { outDir, timeoutMs: 45000 },
        cases: [
          {
            name: "template-camera",
            pmx,
            vmd,
            templatePmm,
            frames: [600, 0, 251, 251],
          },
        ],
      },
      null,
      2,
    )}\n`,
    "utf8",
  );

  const result = await prepareStaticRenderBatch({ manifest: manifestPath });

  assert.equal(result.ok, true);
  assert.equal(result.mode, "static-render-prepare");
  assert.equal(result.cases, 1);
  assert.deepEqual(result.results[0].frames, [0, 251, 600]);
  assert.equal(result.results[0].projectSource, "template-pmm");
  assert.equal(existsSync(result.results[0].project), true);
  assert.equal(await readFile(result.results[0].project, "utf8"), "template-pmm");
  const fixture = JSON.parse(await readFile(result.results[0].fixturePath, "utf8"));
  assert.deepEqual(fixture.frames, [0, 251, 600]);
  assert.equal(fixture.timeoutMs, 45000);
  assert.match(result.results[0].warnings[0], /templatePmm was used/);
});

test("applies static render image settings to copied template PMM headers", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-static-render-resolution-"));
  const pmx = join(dir, "model.pmx");
  const templatePmm = join(dir, "template.pmm");
  const outDir = join(dir, "out");
  const manifestPath = join(dir, "static-render.json");
  const template = Buffer.alloc(64);
  Buffer.from("Polygon Movie maker 0002", "latin1").copy(template, 0);
  template.writeInt32LE(640, 30);
  template.writeInt32LE(360, 34);
  await mkdir(outDir, { recursive: true });
  await writeFile(pmx, "");
  await writeFile(templatePmm, template);
  await writeFile(
    manifestPath,
    `${JSON.stringify(
      {
        defaults: {
          outDir,
          image: { width: 1024, height: 768, format: "png", cropContent: false },
        },
        cases: [{ name: "resolution", pmx, templatePmm, frame: 0 }],
      },
      null,
      2,
    )}\n`,
    "utf8",
  );

  const result = await prepareStaticRenderBatch({ manifest: manifestPath });
  const copied = await readFile(result.results[0].project);

  assert.equal(copied.readInt32LE(30), 1024);
  assert.equal(copied.readInt32LE(34), 768);
  assert.deepEqual(result.results[0].image, { width: 1024, height: 768, format: "png", cropContent: false });
});
