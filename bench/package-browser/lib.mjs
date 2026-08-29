import { mkdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { dirname, join } from 'node:path';

const HEX = /^[0-9a-f]{64}$/;
const SLUG = /^(?!\.{1,2}$)[A-Za-z0-9][A-Za-z0-9._-]*$/;
const STAGES = ['fetch_ms', 'metadata_decode_ms', 'decrypt_ms', 'transcode_ms', 'upload_render_finish_ms', 'entry_gpu_complete_ms'];
const UNAVAILABLE = ['true_peak_js_heap', 'basis_wasm_linear_memory', 'physical_copy_count', 'gpu_resident_memory', 'compositor_presentation_latency'];
const CLEAR_PIXELS_SHA256 = '2a717e567f5f6bde3a3e7959a7b114873c0e12c2cf9c6829c2e05a5823d7efca';
const THREE_RGBA_FORMAT = 1023;

function fail(message) { throw new Error(message); }
function object(value, label) { if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${label} must be an object`); return value; }
function exactKeys(value, keys, label) {
  object(value, label);
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) fail(`${label} fields are not canonical`);
}
function finite(value, label) { if (!Number.isFinite(value) || value < 0) fail(`${label} must be finite and nonnegative`); return value; }
function positiveInteger(value, label) { if (!Number.isInteger(value) || value <= 0) fail(`${label} must be a positive integer`); return value; }
function boundedText(value, label, maximum = 512) {
  if (typeof value !== 'string' || value.length === 0 || value.length > maximum || /[\u0000-\u001f\u007f]/.test(value)) fail(`${label} must be bounded text without controls`);
  return value;
}

function validateTiming(value, label) {
  exactKeys(value, ['samples_ms', 'p50_ms', 'p95_ms'], label);
  if (!Array.isArray(value.samples_ms) || value.samples_ms.length !== 5) fail(`${label}.samples_ms must contain five values`);
  value.samples_ms.forEach((sample, index) => finite(sample, `${label}.samples_ms[${index}]`));
  const sorted = [...value.samples_ms].sort((a, b) => a - b);
  if (value.p50_ms !== sorted[2] || value.p95_ms !== sorted[4]) fail(`${label} percentile mismatch`);
}

function validateSignedTiming(value, label) {
  exactKeys(value, ['samples_ms', 'p50_ms', 'p95_ms'], label);
  if (!Array.isArray(value.samples_ms) || value.samples_ms.length !== 5) fail(`${label}.samples_ms must contain five values`);
  if (value.samples_ms.some(sample => !Number.isFinite(sample))) fail(`${label}.samples_ms must be finite`);
  const sorted = [...value.samples_ms].sort((a, b) => a - b);
  if (value.p50_ms !== sorted[2] || value.p95_ms !== sorted[4]) fail(`${label} percentile mismatch`);
}

function validateLane(value, expectedHash, expected, encrypted, label) {
  exactKeys(value, ['sha256', 'mips', 'pixels_sha256', 'pixels_non_clear', 'format', 'type', 'color_space', 'is_compressed_texture', 'input_bytes', 'output_bytes', 'timing', 'observed_calls', 'observed_bytes'], label);
  if (value.sha256 !== expectedHash || !HEX.test(value.pixels_sha256) || value.pixels_sha256 === CLEAR_PIXELS_SHA256 || value.pixels_non_clear !== true) fail(`${label} hash/readback mismatch`);
  if (!Number.isInteger(value.format) || !Number.isInteger(value.type) || typeof value.color_space !== 'string' || value.is_compressed_texture !== true) fail(`${label} texture metadata invalid`);
  if (value.output_bytes !== expected.bytes || value.input_bytes !== expected.bytes + (encrypted ? 16 : 0)) fail(`${label} byte size mismatch`);
  if (!Array.isArray(value.mips) || value.mips.length !== expected.mips.length) fail(`${label}.mips coverage mismatch`);
  for (const [index, mip] of value.mips.entries()) {
    exactKeys(mip, ['width', 'height', 'bytes', 'sha256'], `${label}.mips[${index}]`);
    positiveInteger(mip.width, `${label}.mips[${index}].width`);
    positiveInteger(mip.height, `${label}.mips[${index}].height`);
    positiveInteger(mip.bytes, `${label}.mips[${index}].bytes`);
    if (mip.width !== expected.mips[index][0] || mip.height !== expected.mips[index][1]) fail(`${label}.mips[${index}] dimensions mismatch`);
    if (mip.sha256 !== null && !HEX.test(mip.sha256)) fail(`${label}.mips[${index}].sha256 invalid`);
  }
  exactKeys(value.timing, STAGES, `${label}.timing`);
  for (const stage of STAGES) validateTiming(value.timing[stage], `${label}.timing.${stage}`);
  exactKeys(value.observed_calls, ['warmup', 'measured', 'fetch', 'decrypt', 'transcode', 'gpu_complete'], `${label}.observed_calls`);
  if (value.observed_calls.warmup !== 1 || value.observed_calls.measured !== 5 || value.observed_calls.fetch !== 6 || value.observed_calls.decrypt !== (encrypted ? 6 : 0) || value.observed_calls.transcode !== 6 || value.observed_calls.gpu_complete !== 6) fail(`${label}.observed_calls mismatch`);
  exactKeys(value.observed_bytes, ['measured_input_total', 'measured_output_total'], `${label}.observed_bytes`);
  if (value.observed_bytes.measured_input_total !== value.input_bytes * 5 || value.observed_bytes.measured_output_total !== expected.bytes * 5) fail(`${label}.observed_bytes mismatch`);
}

function validateVariantReport(report, context, variant) {
  exactKeys(report, ['schema', 'run_id', 'browser_run_id', 'provenance', 'environment', 'setup', 'cases'], 'report');
  if (report.schema !== 1 || report.run_id !== context.runId || !SLUG.test(report.browser_run_id)) fail('report identity invalid');
  exactKeys(report.provenance, ['control_sha256', 'revision', 'latest_sha256', 'inputs_digest', 'three_digest', 'harness_digest'], 'provenance');
  if (JSON.stringify(report.provenance) !== JSON.stringify(context.provenance)) fail('provenance mismatch');
  const envKeys = variant === 'webgpu'
    ? ['ua', 'execution_surface', 'cross_origin_isolated', 'secure_context', 'webgpu', 'adapter_info', 'adapter_features', 'device_features', 'limits', 'gpu_classification', 'performance_blocked', 'directional_memory', 'unavailable_metrics']
    : [...(variant === 'zen-webgl2' ? ['execution_surface'] : []), 'ua', 'cross_origin_isolated', 'secure_context', 'webgl2', 'vendor', 'renderer', 'gpu_classification', 'performance_blocked', 'extensions', 'directional_memory', 'unavailable_metrics'];
  exactKeys(report.environment, envKeys, 'environment');
  boundedText(report.environment.ua, 'environment.ua');
  const browserUaValid = variant === 'zen-webgl2'
    ? /Firefox\/\d+/.test(report.environment.ua) && !/Chrome\/\d+/.test(report.environment.ua)
    : /Chrome\/\d+/.test(report.environment.ua);
  if (!browserUaValid || report.environment.cross_origin_isolated !== true || report.environment.secure_context !== true) fail(`${variant} environment invalid`);
  if (variant === 'webgpu') {
    if (report.environment.execution_surface !== context.executionSurface || report.environment.execution_surface !== 'external_chrome_extension') fail('WebGPU execution surface is not authoritative external Chrome');
    exactKeys(report.environment.adapter_info, ['vendor', 'architecture', 'device', 'description', 'is_fallback_adapter'], 'environment.adapter_info');
    for (const key of ['vendor', 'architecture', 'device', 'description']) boundedText(report.environment.adapter_info[key], `environment.adapter_info.${key}`);
    if (typeof report.environment.adapter_info.is_fallback_adapter !== 'boolean') fail('environment.adapter_info.is_fallback_adapter invalid');
    if (report.environment.webgpu !== true || !Array.isArray(report.environment.adapter_features) || !Array.isArray(report.environment.device_features)) fail('Chrome/WebGPU environment invalid');
    if (report.environment.adapter_features.length > 128 || report.environment.device_features.length > 128) fail('WebGPU features invalid');
    for (const [index, feature] of report.environment.adapter_features.entries()) boundedText(feature, `environment.adapter_features[${index}]`, 128);
    for (const [index, feature] of report.environment.device_features.entries()) boundedText(feature, `environment.device_features[${index}]`, 128);
    const compressionFeatures = ['texture-compression-bc', 'texture-compression-astc', 'texture-compression-etc2'];
    if (!compressionFeatures.some(feature => report.environment.device_features.includes(feature) && report.environment.adapter_features.includes(feature))) fail('WebGPU compression feature missing');
    exactKeys(report.environment.limits, ['maxTextureDimension2D', 'maxTextureArrayLayers', 'maxBindGroups', 'maxBufferSize'], 'environment.limits');
    for (const key of ['maxTextureDimension2D', 'maxTextureArrayLayers', 'maxBindGroups', 'maxBufferSize']) positiveInteger(report.environment.limits[key], `environment.limits.${key}`);
  } else {
    if (variant === 'zen-webgl2' && (report.environment.execution_surface !== context.executionSurface || report.environment.execution_surface !== 'zen_browser')) fail('Zen execution surface invalid');
    if (report.environment.webgl2 !== true) fail(`${variant} WebGL2 environment invalid`);
    boundedText(report.environment.vendor, 'environment.vendor');
    boundedText(report.environment.renderer, 'environment.renderer');
  }
  if (!['hardware', 'software', 'unknown'].includes(report.environment.gpu_classification)) fail('GPU classification invalid');
  if (report.environment.performance_blocked !== (report.environment.gpu_classification !== 'hardware')) fail('performance blocked classification mismatch');
  if (variant !== 'webgpu') {
    if (!Array.isArray(report.environment.extensions) || report.environment.extensions.length > 256) fail('extensions invalid');
    for (const [index, extension] of report.environment.extensions.entries()) boundedText(extension, `environment.extensions[${index}]`, 128);
  }
  if (JSON.stringify(report.environment.unavailable_metrics) !== JSON.stringify(UNAVAILABLE)) fail('unavailable metrics drift');
  if (report.environment.directional_memory !== 'unavailable') {
    exactKeys(report.environment.directional_memory, ['js_heap_size_limit', 'total_js_heap_size', 'used_js_heap_size', 'authority'], 'directional_memory');
    for (const key of ['js_heap_size_limit', 'total_js_heap_size', 'used_js_heap_size']) finite(report.environment.directional_memory[key], `directional_memory.${key}`);
    if (report.environment.directional_memory.authority !== 'directional_snapshot_not_peak') fail('memory authority invalid');
  }
  exactKeys(report.setup, ['key_import_ms', 'control_warmup_ms'], 'setup');
  finite(report.setup.key_import_ms, 'setup.key_import_ms');
  finite(report.setup.control_warmup_ms, 'setup.control_warmup_ms');
  if (!Array.isArray(report.cases) || report.cases.length !== context.cases.size) fail('case count mismatch');
  const seen = new Set();
  for (const [index, item] of report.cases.entries()) {
    const label = `cases[${index}]`;
    exactKeys(item, ['id', 'expected_sha256', 'baseline', 'encrypted', 'paired_overhead', 'crypto_rejections', 'equality', 'lanes_equal'], label);
    const expected = context.cases.get(item.id);
    if (!expected || seen.has(item.id) || item.expected_sha256 !== expected.sha256 || item.lanes_equal !== true) fail(`${label} identity invalid`);
    seen.add(item.id);
    exactKeys(item.crypto_rejections, ['wrong_key', 'wrong_aad', 'tamper', 'truncation'], `${label}.crypto_rejections`);
    if (Object.values(item.crypto_rejections).some(value => value !== true)) fail(`${label} crypto rejection failed`);
    exactKeys(item.equality, ['baseline_input', 'encrypted_input', 'pixels', 'baseline_non_clear', 'encrypted_non_clear', 'mips', 'format'], `${label}.equality`);
    if (Object.values(item.equality).some(value => value !== true)) fail(`${label} equality failed`);
    validateLane(item.baseline, expected.sha256, expected, false, `${label}.baseline`);
    validateLane(item.encrypted, expected.sha256, expected, true, `${label}.encrypted`);
    if (variant === 'webgpu' && (item.baseline.format === THREE_RGBA_FORMAT || item.encrypted.format === THREE_RGBA_FORMAT)) fail(`${label} WebGPU texture used RGBA fallback`);
    validateSignedTiming(item.paired_overhead, `${label}.paired_overhead`);
    const paired = item.encrypted.timing.entry_gpu_complete_ms.samples_ms.map((value, sample) => value - item.baseline.timing.entry_gpu_complete_ms.samples_ms[sample]);
    const pairedSorted = [...paired].sort((a, b) => a - b);
    if (JSON.stringify(item.paired_overhead.samples_ms) !== JSON.stringify(paired) || item.paired_overhead.p50_ms !== pairedSorted[2] || item.paired_overhead.p95_ms !== pairedSorted[4]) fail(`${label}.paired_overhead mismatch`);
    for (const field of ['mips', 'pixels_sha256', 'format', 'type', 'color_space', 'is_compressed_texture', 'output_bytes']) if (JSON.stringify(item.baseline[field]) !== JSON.stringify(item.encrypted[field])) fail(`${label} lane ${field} mismatch`);
  }
  if (seen.size !== context.cases.size) fail('case coverage mismatch');
  return JSON.parse(JSON.stringify(report));
}

export function validateReport(report, context) { return validateVariantReport(report, context, 'webgl2'); }
export function validateWebGPUReport(report, context) { return validateVariantReport(report, context, 'webgpu'); }
export function validateZenWebGL2Report(report, context) { return validateVariantReport(report, context, 'zen-webgl2'); }

export function renderMarkdown(report) {
  const blocked = report.environment.performance_blocked;
  const lines = [
    '# MMDPACK browser WebGL2 decision', '',
    `- Status: fixed ten-case Chrome/WebGL2 texture-entry functionality passed; performance is ${blocked ? `blocked because GPU classification is ${report.environment.gpu_classification}` : 'directional on the observed hardware GPU'}.`,
    '- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, textured-quad draw, and `gl.finish()`. This is not full-MMD first render or compositor presentation evidence.',
    `- Browser: ${report.environment.ua}`,
    `- GPU: ${report.environment.vendor}; ${report.environment.renderer}; classification: ${report.environment.gpu_classification}.`,
    `- Three.js revision label: ${report.provenance.revision}; control SHA-256: ${report.provenance.control_sha256}.`,
    '- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.',
    '- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.',
    '- Memory/copy limit: the Chrome heap value is a directional snapshot, not a peak. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.',
    '- Not run: Firefox, Safari, macOS, WebGPU, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.', '',
    `Run: ${report.browser_run_id} (local raw artifact is intentionally ignored).`, '',
    '| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |',
    '| --- | ---: | ---: | ---: | ---: | ---: |',
  ];
  for (const item of report.cases) {
    const base = item.baseline.timing.entry_gpu_complete_ms;
    const enc = item.encrypted.timing.entry_gpu_complete_ms;
    const overhead = item.paired_overhead;
    const decrypt = item.encrypted.timing.decrypt_ms;
    const transcode = item.encrypted.timing.transcode_ms;
    lines.push(`| ${item.id} | ${base.p50_ms.toFixed(3)} / ${base.p95_ms.toFixed(3)} | ${enc.p50_ms.toFixed(3)} / ${enc.p95_ms.toFixed(3)} | ${overhead.p50_ms.toFixed(3)} / ${overhead.p95_ms.toFixed(3)} | ${decrypt.p50_ms.toFixed(3)} / ${decrypt.p95_ms.toFixed(3)} | ${transcode.p50_ms.toFixed(3)} / ${transcode.p95_ms.toFixed(3)} |`);
  }
  lines.push('', '- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.', '- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear GPU readback are the bounded substitute.', '');
  return lines.join('\n');
}

export function renderWebGPUMarkdown(report) {
  const blocked = report.environment.performance_blocked;
  const lines = [
    '# MMDPACK browser WebGPU decision', '',
    `- Status: fixed ten-case Chrome/WebGPU texture-entry functionality passed; performance is ${blocked ? `blocked because GPU classification is ${report.environment.gpu_classification}` : 'directional on the observed hardware GPU'}.`,
    '- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, WebGPURenderer using the requested device, textured-quad draw into a 16x16 RenderTarget, `queue.onSubmittedWorkDone()`, and asynchronous RenderTarget readback. This is not full-MMD first render or compositor presentation evidence.',
    `- Browser: ${report.environment.ua}`,
    `- Execution surface: ${report.environment.execution_surface} (server-authorized).`,
    `- Adapter: ${report.environment.adapter_info.vendor}; ${report.environment.adapter_info.architecture}; ${report.environment.adapter_info.device}; classification: ${report.environment.gpu_classification}.`,
    `- Three.js revision label: ${report.provenance.revision}; control SHA-256: ${report.provenance.control_sha256}.`,
    '- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.',
    '- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.',
    '- Memory/copy limit: the Chrome heap value is a directional snapshot, not a peak. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.',
    '- Not run: Firefox, Safari, macOS, WebGL2, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.', '',
    `Run: ${report.browser_run_id} (local raw artifact is intentionally ignored).`, '',
    '| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |',
    '| --- | ---: | ---: | ---: | ---: | ---: |',
  ];
  for (const item of report.cases) {
    const base = item.baseline.timing.entry_gpu_complete_ms;
    const enc = item.encrypted.timing.entry_gpu_complete_ms;
    const overhead = item.paired_overhead;
    const decrypt = item.encrypted.timing.decrypt_ms;
    const transcode = item.encrypted.timing.transcode_ms;
    lines.push(`| ${item.id} | ${base.p50_ms.toFixed(3)} / ${base.p95_ms.toFixed(3)} | ${enc.p50_ms.toFixed(3)} / ${enc.p95_ms.toFixed(3)} | ${overhead.p50_ms.toFixed(3)} / ${overhead.p95_ms.toFixed(3)} | ${decrypt.p50_ms.toFixed(3)} / ${decrypt.p95_ms.toFixed(3)} | ${transcode.p50_ms.toFixed(3)} / ${transcode.p95_ms.toFixed(3)} |`);
  }
  lines.push('', '- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.', '- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear RenderTarget readback are the bounded substitute.', '');
  return lines.join('\n');
}

export function renderZenWebGL2Markdown(report) {
  const blocked = report.environment.performance_blocked;
  const lines = [
    '# MMDPACK Zen Browser WebGL2 diagnostic', '',
    `- Status: fixed ten-case Zen Browser/Firefox-engine WebGL2 texture-entry functionality passed; performance is ${blocked ? `blocked because GPU classification is ${report.environment.gpu_classification}` : 'directional on the observed hardware GPU'}. This is diagnostic evidence and is not official Firefox authority.`,
    '- Scope: plaintext or AES-256-GCM encrypted Candidate B KTX2 entry through Web Crypto, the pinned official Three.js KTX2Loader, textured-quad draw, and `gl.finish()`. This is not full-MMD first render or compositor presentation evidence.',
    `- Browser: ${report.environment.ua}`,
    `- Execution surface: ${report.environment.execution_surface} (server-authorized Zen diagnostic).`,
    `- GPU: ${report.environment.vendor}; ${report.environment.renderer}; classification: ${report.environment.gpu_classification}.`,
    `- Three.js revision label: ${report.provenance.revision}; control SHA-256: ${report.provenance.control_sha256}.`,
    '- Key policy: Web Crypto `extractable: false`, decrypt-only. Raw key bytes were zeroed after import and were not serialized.',
    '- Timing: one warmup and five measured repetitions per lane, with baseline/encrypted order alternated for every repetition. Paired overhead is encrypted minus plaintext within the same repetition; all p50/p95 values are directional.',
    '- Memory/copy limit: browser heap is unavailable unless explicitly exposed. True peak JS heap, Basis WASM live/capacity, physical copy count, GPU memory, and compositor presentation latency are unavailable.',
    '- Not run: official Firefox authority, Safari, macOS, WebGPU, `.mmdpack` Header/Manifest, PMX/VMD parse, and full-MMD rendering.', '',
    `Run: ${report.browser_run_id} (local raw artifact is intentionally ignored).`, '',
    '| Case | Baseline GPU-complete p50/p95 ms | Encrypted p50/p95 ms | Paired overhead p50/p95 ms | AES decrypt p50/p95 ms | Transcode p50/p95 ms |',
    '| --- | ---: | ---: | ---: | ---: | ---: |',
  ];
  for (const item of report.cases) {
    const base = item.baseline.timing.entry_gpu_complete_ms;
    const enc = item.encrypted.timing.entry_gpu_complete_ms;
    const overhead = item.paired_overhead;
    const decrypt = item.encrypted.timing.decrypt_ms;
    const transcode = item.encrypted.timing.transcode_ms;
    lines.push(`| ${item.id} | ${base.p50_ms.toFixed(3)} / ${base.p95_ms.toFixed(3)} | ${enc.p50_ms.toFixed(3)} / ${enc.p95_ms.toFixed(3)} | ${overhead.p50_ms.toFixed(3)} / ${overhead.p95_ms.toFixed(3)} | ${decrypt.p50_ms.toFixed(3)} / ${decrypt.p95_ms.toFixed(3)} | ${transcode.p50_ms.toFixed(3)} / ${transcode.p95_ms.toFixed(3)} |`);
  }
  lines.push('', '- All cases passed wrong-key, wrong-AAD, one-byte tamper, and tag truncation rejection.', '- The official KTX2Loader does not promise public per-mip payload bytes for every selected GPU target. Where unavailable, mip SHA-256 is `null`; plaintext hash, non-empty mip metadata, texture format/type/color space, and non-clear GPU readback are the bounded substitute.', '');
  return lines.join('\n');
}

export async function publishReport(report, { outputRoot, documentPath, variant = 'webgl2' }) {
  if (!SLUG.test(report.browser_run_id)) fail('unsafe browser run id');
  const runsRoot = join(outputRoot, 'runs');
  const finalRun = join(runsRoot, report.browser_run_id);
  try { await readFile(join(finalRun, 'report.json')); fail('browser run already exists'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  await mkdir(runsRoot, { recursive: true });
  await mkdir(dirname(documentPath), { recursive: true });
  const token = randomUUID();
  const runTemp = join(runsRoot, `.${report.browser_run_id}.${token}.tmp`);
  const docTemp = `${documentPath}.${token}.tmp`;
  await mkdir(runTemp, { recursive: false });
  try {
    await writeFile(join(runTemp, 'report.json'), `${JSON.stringify(report, null, 2)}\n`, { flag: 'wx' });
    const markdown = variant === 'webgpu' ? renderWebGPUMarkdown(report)
      : variant === 'zen-webgl2' ? renderZenWebGL2Markdown(report) : renderMarkdown(report);
    await writeFile(docTemp, markdown, { flag: 'wx' });
    await rename(runTemp, finalRun);
    try {
      await rename(docTemp, documentPath);
    } catch (error) {
      await rm(finalRun, { recursive: true, force: true });
      throw error;
    }
  } finally {
    await rm(runTemp, { recursive: true, force: true });
    await rm(docTemp, { force: true });
  }
}

export const validationConstants = { STAGES, UNAVAILABLE };
