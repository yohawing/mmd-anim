import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { promisify } from "node:util";
import test from "node:test";

const execFileAsync = promisify(execFile);
const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cliPath = resolve(packageRoot, "src", "cli.mjs");

test("verify-coverage preserves fixture camera defaults and accepts explicit boolean values", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-cli-"));
  try {
    const fixture = join(dir, "fixture.json");
    const actual = join(dir, "actual.jsonl");
    await writeFixture(fixture, actual);
    await writeFile(actual, `${JSON.stringify(createModelRecord())}\n`, "utf8");

    const defaultResult = await runCliFailure(["verify-coverage", "--fixture", fixture, "--actual", actual]);
    assert.equal(JSON.parse(defaultResult.stdout).ok, false);
    const explicitFalse = await runCli(["verify-coverage", "--fixture", fixture, "--actual", actual, "--require-camera", "false"]);
    assert.equal(explicitFalse.ok, true);
    const explicitTrue = await runCliFailure(["verify-coverage", "--fixture", fixture, "--actual", actual, "--require-camera", "true"]);
    assert.equal(JSON.parse(explicitTrue.stdout).ok, false);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("verify-coverage rejects non-boolean require-camera values", async () => {
  const error = await runCliFailure(["verify-coverage", "--fixture", "missing-fixture.json", "--require-camera", "yes"]);
  assert.match(error.stderr, /--require-camera must be true or false/);
});

test("record rejects non-boolean accept-dialog values", async () => {
  const error = await runCliFailure(["record", "--fixture", "missing-fixture.json", "--accept-dialog", "yes"]);
  assert.match(error.stderr, /--accept-dialog must be true or false/);
});

async function writeFixture(fixture, actual) {
  await writeFile(
    fixture,
    `${JSON.stringify({
      name: "cli-test",
      mmdExe: "MikuMikuDance.exe",
      project: "scene.pmm",
      frames: [0],
      output: actual,
      dump: { bones: false, morphs: false, camera: true },
    })}\n`,
    "utf8",
  );
}

function createModelRecord() {
  return {
    schemaVersion: 1,
    source: { mmdVersion: "9.32-x64", dumperVersion: "0.1.0" },
    frame: 0,
    models: [
      {
        index: 0,
        name: "model.pmx",
        filename: "model.pmx",
        visible: true,
        bones: [{ index: 0, name: "bone", worldMatrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1] }],
        morphs: [{ index: 0, name: "morph", weight: 0 }],
      },
    ],
  };
}

async function runCli(args, env = process.env) {
  const { stdout } = await execFileAsync(process.execPath, [cliPath, ...args], { cwd: packageRoot, env });
  return JSON.parse(stdout);
}

async function runCliFailure(args, env = process.env) {
  try {
    await execFileAsync(process.execPath, [cliPath, ...args], { cwd: packageRoot, env });
  } catch (error) {
    return error;
  }
  throw new Error("expected CLI failure");
}
