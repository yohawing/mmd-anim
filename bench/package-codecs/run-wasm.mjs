import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const root = path.dirname(fileURLToPath(import.meta.url));

function valueAfter(name, fallback) {
  const index = process.argv.indexOf(name);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

const manifestPath = valueAfter('--manifest', path.resolve(root, '../../../.ai/mmdpack/campaign.json'));
const outputPath = valueAfter('--output', path.resolve(root, '../../../.ai/mmdpack/wasm.json'));
const cargoLockPath = valueAfter('--cargo-lock', path.resolve(root, 'Cargo.lock'));
const runId = valueAfter('--run-id', 'manual-local');
const measuredAt = valueAfter('--measured-at', 'not-recorded');
const harnessSourceDigest = valueAfter('--harness-source-digest', 'untracked local harness; digest not supplied');
const expectedManifestHash = valueAfter('--manifest-sha256', null);
const expectedCargoLockHash = valueAfter('--cargo-lock-sha256', null);
const wasmModulePath = path.resolve(root, 'pkg/package_codecs_wasm.js');
const manifestBytes = await readFile(manifestPath);
const manifestHash = sha256(manifestBytes);
if (expectedManifestHash !== null && expectedManifestHash !== manifestHash) {
  throw new Error('manifest hash does not match bytes read for this run');
}
const cargoLockBytes = await readFile(cargoLockPath);
const cargoLockHash = sha256(cargoLockBytes);
if (expectedCargoLockHash !== null && expectedCargoLockHash !== cargoLockHash) {
  throw new Error('Cargo.lock hash does not match bytes read for this run');
}
const manifest = JSON.parse(manifestBytes.toString('utf8'));
const wasm = await import(pathToFileURL(wasmModulePath));

function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function rustcVersion() {
  try {
    return execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim();
  } catch {
    return 'unavailable';
  }
}

function wasmPackVersion() {
  try {
    return execFileSync('wasm-pack', ['--version'], { encoding: 'utf8' }).trim();
  } catch {
    return 'unavailable';
  }
}

const rustc = rustcVersion();

const cases = [];
const failures = [];
for (const item of manifest.cases) {
  try {
    const input = new Uint8Array(await readFile(item.path));
    const result = JSON.parse(wasm.run_case(input, item.id));
    const checks = result.checks;
    if (!checks.round_trip || !checks.wrong_key_rejected || !checks.tamper_rejected || !checks.truncation_rejected) {
      throw new Error(`boundary check failed: ${JSON.stringify(checks)}`);
    }
    cases.push({
      id: item.id,
      kind: item.kind,
      size_class: item.size_class,
      path_label: item.path_label,
      input_bytes: input.byteLength,
      compressed_bytes: result.sizes.compressed_bytes,
      ciphertext_bytes: result.sizes.ciphertext_bytes,
      compressed_ratio: result.sizes.compressed_bytes / Math.max(1, input.byteLength),
      sha256: sha256(input),
      timings: result.timings,
      checks: result.checks,
    });
  } catch (error) {
    failures.push({ id: item.id, path_label: item.path_label, error: String(error) });
  }
}

const report = {
  schema: 'mmdpack-phase0-benchmark/1',
  campaign_schema: manifest.schema,
  description: manifest.description,
  run_id: runId,
  measured_at_utc: measuredAt,
  environment: {
    platform: `${process.platform}-${process.arch}`,
    rustc,
    wasm_runner: `Node.js ${process.version}; package-codecs-wasm 0.1.0`,
    native_runner: 'not-run (WASM JSON lane)',
    campaign_manifest_sha256: manifestHash,
    cargo_lock_sha256: cargoLockHash,
    wasm_pack: wasmPackVersion(),
    harness_source_digest: harnessSourceDigest,
    msrv_statement: `MSRV 1.87 build not verified on this host; measured with ${rustc}`,
  },
  config: {
    compression: 'zstd crate 0.13.3 / zstd 1.5.7',
    zstd_level: 3,
    encryption: 'RustCrypto aes-gcm 0.10.3 (AES-256-GCM)',
    key_bytes: 32,
    nonce_bytes: 12,
    tag_bytes: 16,
    aad: 'mmdpack-phase0/<case id>',
    key_policy: 'fresh random key and nonce per encrypt call; never serialized',
    timing_policy: 'single measured iteration; OS/file caches uncontrolled; no warmup or repeat; timings are directional',
  },
  cases,
  failures,
};
await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`);
console.log(`wasm cases=${cases.length} failures=${failures.length} raw=${outputPath}`);
if (failures.length > 0) {
  process.exitCode = 1;
}
