import assert from "node:assert/strict";
import { access, mkdtemp, readFile, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { test } from "node:test";
import { prepareOracleBatch, readOracleBatchManifest } from "../src/oracle-batch.mjs";
import { readPmmDocumentKeyframes } from "../src/pmm-document-keyframes.mjs";
import { readPmmManifest } from "../src/pmm-manifest.mjs";
import { readVmdInventory } from "../src/vmd-inventory.mjs";

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const manifestPath = resolve(packageRoot, "manifests", "m0.json");
const baselinePath = resolve(packageRoot, "fixtures", "m0", "expected-baseline.json");
const repositoryRoot = resolve(packageRoot, "..", "..");

test("M0 characterizes the three Node backend dry-run cases", async () => {
  const manifest = await readOracleBatchManifest(manifestPath);
  const baseline = JSON.parse(await readFile(baselinePath, "utf8"));
  const expectedDefaultOutDir = resolve(repositoryRoot, ".ai", "mmd-dumper", "m0");
  assert.equal(manifest.outDir, expectedDefaultOutDir);
  assert.deepEqual(
    manifest.cases.map((entry) => entry.name),
    ["body-only", "body-camera", "body-property-ik"],
  );

  const output = await mkdtemp(join(packageRoot, ".m0-test-"));
  try {
    const result = await prepareOracleBatch({ manifest: manifestPath, outDir: output });
    assert.equal(result.ok, true);
    assert.equal(result.mode, "oracle-batch-prepare");
    assert.equal(result.cases, 3);
    assert.equal(result.outDir, output);

    for (const actual of result.results) {
      const expected = baseline.cases[actual.name];
      assert.ok(expected, `missing expected baseline for ${actual.name}`);
      const fixture = JSON.parse(await readFile(actual.fixturePath, "utf8"));
      const pmm = await readPmmManifest(actual.project);
      const document = await readPmmDocumentKeyframes(actual.project);
      const modelReference = pmm.assetReferences.find((reference) => reference.kind === "model");
      const model = document.models[0];

      await access(actual.stagedPmx.output);
      assert.equal(resolve(modelReference.path), resolve(actual.stagedPmx.output));
      assert.equal(resolve(model.path), resolve(actual.stagedPmx.output));
      assert.deepEqual(actual.frames, expected.frames);
      assert.deepEqual(fixture.frames, expected.frames);
      assert.equal(fixture.project, actual.project);
      assert.equal(fixture.output, actual.output);
      assert.equal(actual.records, undefined);
      assert.equal(await exists(actual.output), false, `${actual.name} unexpectedly recorded oracle output`);
      assert.equal(await exists(`${actual.output}.done`), false, `${actual.name} unexpectedly wrote done marker`);
      assert.equal(fixture.dump?.camera ?? false, actual.name === "body-camera");
      assert.equal(fixture.dump?.bones, true);
      assert.equal(fixture.dump?.morphs, true);

      const sourceVmd = await readVmdInventory(actual.vmd, { limit: Number.MAX_SAFE_INTEGER });
      assert.deepEqual(selectCounts(sourceVmd.counts), expected.sourceVmdCounts);
      if (actual.cameraVmd) {
        const sourceCamera = await readVmdInventory(actual.cameraVmd, { limit: Number.MAX_SAFE_INTEGER });
        assert.deepEqual(
          { cameraFrames: sourceCamera.counts.cameraFrames, maxFrame: sourceCamera.maxFrame },
          expected.sourceCameraCounts,
        );
      } else {
        assert.equal(expected.sourceCameraCounts, undefined);
      }

      assert.deepEqual(selectCounts(document.counts, Object.keys(expected.pmmCounts)), expected.pmmCounts);
      if (expected.cameraPmmCounts) {
        assert.deepEqual(document.camera.counts, expected.cameraPmmCounts);
      }
      assert.deepEqual(actual.patch.counts, expected.patchCounts);
      assert.deepEqual(actual.filter.appliedCounts, expected.filter.appliedCounts);
      assert.deepEqual(actual.filter.skippedCounts, expected.filter.skippedCounts);
      assert.deepEqual(actual.filter.skippedBoneNames, expected.filter.skippedBoneNames);
      assert.deepEqual(actual.filter.skippedMorphNames, expected.filter.skippedMorphNames);
      assert.equal(actual.filter.droppedUnsupportedChannels?.propertyFrames, expected.filter.propertyFrames);

      assert.equal(actual.stagedPmx.converted, true);
    }
  } finally {
    await rm(output, { recursive: true, force: true });
  }
});

function selectCounts(counts, keys = ["boneFrames", "morphFrames", "cameraFrames", "propertyFrames"]) {
  return Object.fromEntries(keys.map((key) => [key, counts[key] ?? 0]));
}

async function exists(path) {
  try {
    await access(path);
    return true;
  } catch {
    return false;
  }
}
