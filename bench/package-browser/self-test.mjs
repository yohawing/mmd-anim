import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawn } from 'node:child_process';
import { request as httpRequest } from 'node:http';
import { setTimeout as delay } from 'node:timers/promises';
import { publishReport, validateReport, validateWebGPUReport, validationConstants } from './lib.mjs';

const hex = character => character.repeat(64);
const timing = () => ({ samples_ms: [1, 2, 3, 4, 5], p50_ms: 3, p95_ms: 5 });
const signedTiming = () => ({ samples_ms: [0, 0, 0, 0, 0], p50_ms: 0, p95_ms: 0 });
const timings = () => Object.fromEntries(validationConstants.STAGES.map(stage => [stage, timing()]));
function lane(expected, encrypted) {
  return {
    sha256: expected.sha256,
    mips: [{ width: 1, height: 1, bytes: 16, sha256: null }],
    pixels_sha256: hex('b'), pixels_non_clear: true, format: 36492, type: 1009,
    color_space: 'srgb', is_compressed_texture: true,
    input_bytes: expected.bytes + (encrypted ? 16 : 0), output_bytes: expected.bytes,
    timing: timings(),
    observed_calls: { warmup: 1, measured: 5, fetch: 6, decrypt: encrypted ? 6 : 0, transcode: 6, gpu_complete: 6 },
    observed_bytes: { measured_input_total: (expected.bytes + (encrypted ? 16 : 0)) * 5, measured_output_total: expected.bytes * 5 },
  };
}
const provenance = { control_sha256: hex('1'), revision: hex('2'), latest_sha256: hex('3'), inputs_digest: hex('4'), three_digest: hex('5'), harness_digest: hex('6') };
const expectedCases = new Map(Array.from({ length: 10 }, (_, index) => [`case-${index}`, { id: `case-${index}`, bytes: 100 + index, mips: [[1, 1]], sha256: createHash('sha256').update(`case-${index}`).digest('hex') }]));
function report(run = 'chrome-test') {
  return {
    schema: 1, run_id: 'source-run', browser_run_id: run, provenance,
    environment: {
      ua: 'Mozilla/5.0 Chrome/152.0.0.0', cross_origin_isolated: true, secure_context: true, webgl2: true,
      vendor: 'Test Vendor', renderer: 'Test GPU', gpu_classification: 'hardware', performance_blocked: false,
      extensions: ['EXT_test'], directional_memory: 'unavailable', unavailable_metrics: validationConstants.UNAVAILABLE,
    },
    setup: { key_import_ms: 1, control_warmup_ms: 2 },
    cases: [...expectedCases.values()].map(expected => ({
      id: expected.id, expected_sha256: expected.sha256,
      baseline: lane(expected, false), encrypted: lane(expected, true),
      paired_overhead: signedTiming(),
      crypto_rejections: { wrong_key: true, wrong_aad: true, tamper: true, truncation: true },
      equality: { baseline_input: true, encrypted_input: true, pixels: true, baseline_non_clear: true, encrypted_non_clear: true, mips: true, format: true },
      lanes_equal: true,
    })),
  };
}
function webgpuReport(run = 'chrome-webgpu-test') {
  const value = report(run);
  value.environment = {
    ua: value.environment.ua, execution_surface: 'external_chrome_extension', cross_origin_isolated: true, secure_context: true, webgpu: true,
    adapter_info: { vendor: 'Test Vendor', architecture: 'Test Architecture', device: 'Test GPU', description: 'Test Adapter', is_fallback_adapter: false },
    adapter_features: ['texture-compression-bc'], device_features: ['texture-compression-bc'],
    limits: { maxTextureDimension2D: 8192, maxTextureArrayLayers: 256, maxBindGroups: 4, maxBufferSize: 268435456 },
    gpu_classification: 'hardware', performance_blocked: false, directional_memory: 'unavailable', unavailable_metrics: validationConstants.UNAVAILABLE,
  };
  return value;
}
const context = { runId: 'source-run', provenance, cases: expectedCases };
const canonical = validateReport(report(), context);
const webgpuContext = { ...context, executionSurface: 'external_chrome_extension' };
const canonicalWebGPU = validateWebGPUReport(webgpuReport(), webgpuContext);
const secret = report();
secret.secret_key = 'must-not-publish';
assert.throws(() => validateReport(secret, context), /canonical/);
const badTiming = report();
badTiming.cases[0].encrypted.timing.decrypt_ms.p95_ms = 4;
assert.throws(() => validateReport(badTiming, context), /percentile/);
const badPairedTiming = report();
badPairedTiming.cases[0].paired_overhead.samples_ms[0] = Number.NaN;
assert.throws(() => validateReport(badPairedTiming, context), /finite/);
const forgedPairedTiming = report();
forgedPairedTiming.cases[0].paired_overhead = { samples_ms: [1, 1, 1, 1, 1], p50_ms: 1, p95_ms: 1 };
assert.throws(() => validateReport(forgedPairedTiming, context), /paired_overhead mismatch/);
const webgpuWrongSurface = webgpuReport();
webgpuWrongSurface.environment.webgl2 = true;
assert.throws(() => validateWebGPUReport(webgpuWrongSurface, context), /canonical/);
const missingCompression = webgpuReport();
missingCompression.environment.adapter_features = [];
missingCompression.environment.device_features = [];
assert.throws(() => validateWebGPUReport(missingCompression, webgpuContext), /compression feature missing/);
const rgbaFallback = webgpuReport();
rgbaFallback.cases[0].baseline.format = 1023;
rgbaFallback.cases[0].encrypted.format = 1023;
assert.throws(() => validateWebGPUReport(rgbaFallback, webgpuContext), /RGBA fallback/);

const temp = await mkdtemp(join(tmpdir(), 'mmdpack-browser-test-'));
const documentPath = join(temp, 'decision.md');
await writeFile(documentPath, 'old publication');
await publishReport(canonical, { outputRoot: join(temp, 'raw'), documentPath });
const published = await readFile(documentPath, 'utf8');
assert.match(published, /chrome-test/);
assert.doesNotMatch(published, /Execution surface:/);
await assert.rejects(publishReport(canonical, { outputRoot: join(temp, 'raw'), documentPath }), /already exists/);
assert.equal(await readFile(documentPath, 'utf8'), published);
const webgpuTemp = await mkdtemp(join(tmpdir(), 'mmdpack-webgpu-test-'));
const webgpuDocumentPath = join(webgpuTemp, 'decision.md');
await publishReport(canonicalWebGPU, { outputRoot: join(webgpuTemp, 'raw'), documentPath: webgpuDocumentPath, variant: 'webgpu' });
const webgpuPublished = await readFile(webgpuDocumentPath, 'utf8');
assert.match(webgpuPublished, /Chrome\/WebGPU/);
assert.match(webgpuPublished, /Execution surface: external_chrome_extension \(server-authorized\)/);
await assert.rejects(publishReport(canonicalWebGPU, { outputRoot: join(webgpuTemp, 'raw'), documentPath: webgpuDocumentPath, variant: 'webgpu' }), /already exists/);
assert.equal(await readFile(webgpuDocumentPath, 'utf8'), webgpuPublished);

for (const path of ['bench/package-browser/lib.mjs', 'bench/package-browser/server.mjs', 'bench/package-browser/self-test.mjs', 'bench/package-browser/web/probe.js', 'bench/package-browser/web/probe-webgpu.js']) {
  await new Promise((resolve, reject) => {
    const child = spawn(process.execPath, ['--check', path]);
    child.on('exit', code => code ? reject(Error(`${path}: syntax exit ${code}`)) : resolve());
  });
}

const existingDoc = await readFile('docs/mmdpack-browser-webgl2-decision.md').catch(() => null);
const server = spawn(process.execPath, ['bench/package-browser/server.mjs'], { stdio: ['ignore', 'pipe', 'pipe'] });
const url = await new Promise((resolve, reject) => {
  let output = '';
  const timeout = setTimeout(() => reject(Error('server startup timeout')), 15_000);
  server.stdout.on('data', chunk => {
    output += chunk;
    const match = output.match(/http:\/\/127\.0\.0\.1:\d+\//);
    if (match) { clearTimeout(timeout); resolve(match[0]); }
  });
  server.on('exit', code => reject(Error(`server exited ${code}`)));
});
try {
  const html = await (await fetch(url)).text();
  assert.match(html, /id="run"/);
  assert.match(html, /id="status"/);
  assert.match(html, /type="importmap"/);
  assert.match(html, /type="module"/);
  const webgpuHtml = await (await fetch(`${url}webgpu`)).text();
  assert.match(webgpuHtml, /Chrome WebGPU/);
  assert.match(webgpuHtml, /three\.webgpu\.js/);
  assert.match(webgpuHtml, /probe-webgpu\.js/);
  assert.equal((await fetch(`${url}three/.git/config`)).status, 404);
  assert.equal((await fetch(`${url}three/build/three.webgpu.js`)).status, 200);
  assert.equal((await fetch(`${url}api/config`)).status, 200);
  const invalid = await fetch(`${url}api/report`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{"schema":1}' });
  assert.equal(invalid.status, 400);
  const invalidWebGPU = await fetch(`${url}api/webgpu/report`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{"schema":1}' });
  assert.equal(invalidWebGPU.status, 403);
  let slowRequest;
  const slowStatus = new Promise((resolve, reject) => {
    slowRequest = httpRequest(`${url}api/report`, { method: 'POST', headers: { 'content-type': 'application/json' } }, response => {
      response.resume();
      response.on('end', () => resolve(response.statusCode));
    });
    slowRequest.on('error', reject);
    slowRequest.write('{"schema":');
  });
  await delay(50);
  assert.equal((await fetch(`${url}api/report`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: '{}' })).status, 409);
  slowRequest.end('1}');
  assert.equal(await slowStatus, 400);
  assert.deepEqual(await readFile('docs/mmdpack-browser-webgl2-decision.md').catch(() => null), existingDoc);
} finally {
  server.kill();
}

console.log('package-browser self-test: validation, atomic publication, whitelist, and invalid-result preservation passed');
