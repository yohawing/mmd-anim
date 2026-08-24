import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { access, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { promisify } from "node:util";
import test from "node:test";

const execFileAsync = promisify(execFile);
const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const cliPath = resolve(packageRoot, "src", "cli.mjs");

test("stage-pmx converts a UTF-8 PMX to an MMD-compatible UTF-16 PMX", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-stage-pmx-"));
  const input = join(dir, "source.pmx");
  const output = join(dir, "model.mmd-utf16.pmx");
  try {
    await writeFile(input, createMinimalPmx(1, "テストモデル"));

    const result = await runCli(["stage-pmx", "--input", input, "--output", output]);

    assert.deepEqual(
      { ok: result.ok, converted: result.converted, encoding: result.encoding },
      { ok: true, converted: true, encoding: "utf-8" },
    );
    assert.equal(result.input, input);
    assert.equal(result.output, output);
    const staged = await readFile(output);
    assert.equal(staged[9], 0);
    assert.equal(staged.includes(Buffer.from("テストモデル", "utf16le")), true);
    assert.equal((await readFile(input))[9], 1);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("stage-pmx reports an already UTF-16 PMX without rewriting it", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-stage-pmx-"));
  const input = join(dir, "source.pmx");
  const output = join(dir, "model.mmd-utf16.pmx");
  try {
    const source = createMinimalPmx(0, "テストモデル");
    await writeFile(input, source);

    const result = await runCli(["stage-pmx", "--input", input, "--output", output]);

    assert.deepEqual(
      { ok: result.ok, converted: result.converted, encoding: result.encoding },
      { ok: true, converted: false, encoding: "utf-16le" },
    );
    assert.equal(result.input, input);
    assert.equal(result.output, input);
    assert.deepEqual(await readFile(input), source);
    await assert.rejects(access(output));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("stage-pmx rejects non-PMX input", async () => {
  const input = resolve(packageRoot, "test", "missing-source.pmd");
  await assert.rejects(runCli(["stage-pmx", "--input", input, "--output", `${input}.pmx`]), /requires a \.pmx input/);
});

test("stage-pmx rejects malformed PMX bytes before reporting a no-op", async () => {
  const dir = await mkdtemp(join(tmpdir(), "mmddumper-stage-pmx-"));
  const input = join(dir, "source.pmx");
  try {
    await writeFile(input, Buffer.alloc(10));

    await assert.rejects(runCli(["stage-pmx", "--input", input, "--output", join(dir, "out.pmx")]), /Invalid PMX header/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

async function runCli(args) {
  const { stdout } = await execFileAsync(process.execPath, [cliPath, ...args], { cwd: packageRoot });
  return JSON.parse(stdout);
}

function createMinimalPmx(encoding, modelName) {
  const parts = [];
  parts.push(Buffer.from("PMX ", "ascii"));
  pushFloat32(parts, 2);
  pushUInt8(parts, 8);
  parts.push(Buffer.from([encoding, 0, 4, 4, 4, 4, 4, 4]));

  for (const text of [modelName, "test model", "", ""]) {
    pushText(parts, text, encoding);
  }
  for (let i = 0; i < 9; i += 1) {
    pushInt32(parts, 0);
  }
  return Buffer.concat(parts);
}

function pushText(parts, text, encoding) {
  const bytes = Buffer.from(text, encoding === 0 ? "utf16le" : "utf8");
  pushInt32(parts, bytes.byteLength);
  parts.push(bytes);
}

function pushUInt8(parts, value) {
  parts.push(Buffer.from([value]));
}

function pushInt32(parts, value) {
  const buffer = Buffer.alloc(4);
  buffer.writeInt32LE(value);
  parts.push(buffer);
}

function pushFloat32(parts, value) {
  const buffer = Buffer.alloc(4);
  buffer.writeFloatLE(value);
  parts.push(buffer);
}
