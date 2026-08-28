const fs = require('node:fs');
const path = require('node:path');
const CONFORMANCE_VECTOR = {
  id: 'aes-gcm-phase0-fixed-v1',
  inputBytes: 42,
  inputSha256: 'c97d7f2d08368e7e338cda518af14a6fcd96d2c5868db8b8752903cca7d3f95f',
  wireSha256: 'bc1bcc8bff0bea5d4359cae7f394dca42c4b9faafe939f8a50df5a20a0d415bb',
};

function value(name) {
  const index = process.argv.indexOf(name);
  if (index < 0 || !process.argv[index + 1]) throw new Error('missing ' + name);
  return process.argv[index + 1];
}

function safeId(id) {
  return typeof id === 'string' && /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(id) && id !== '.' && id !== '..';
}

function safeCell(text) {
  return typeof text === 'string' && text.length > 0 && !/[\r\n|]/.test(text)
    && !/[\u0000-\u001f\u007f]/.test(text) && !text.includes(String.fromCharCode(96));
}

function safePathLabel(label) {
  return safeCell(label) && !label.startsWith('/') && !label.startsWith('\\') && !/^[A-Za-z]:[\\/]/.test(label);
}

function hash64(value) {
  return typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
}

function cell(value) {
  return String(value).replaceAll('|', '\\|').replace(/[\r\n]/g, ' ');
}

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

function finiteTiming(timing) {
  if (!timing || timing.warmup !== 1 || timing.repeats !== 5 || !Array.isArray(timing.samples_ms) || timing.samples_ms.length !== 5) return false;
  return [timing.p50_ms, timing.p95_ms, timing.throughput_mib_s, ...timing.samples_ms].every(Number.isFinite)
    && timing.p95_ms >= timing.p50_ms;
}

function aesChecksPass(checks) {
  return checks && ['round_trip', 'wrong_key_rejected', 'wrong_aad_rejected', 'tamper_rejected', 'truncation_rejected'].every((name) => checks[name] === true);
}

function zstdChecksPass(checks) {
  return checks && ['round_trip', 'size_limit_rejected', 'truncation_rejected'].every((name) => checks[name] === true);
}

function validateAesBackend(backend, expectedLabel, compressedBytes, compressedSha256) {
  return !!backend && backend.backend === expectedLabel && Number.isSafeInteger(backend.wire_bytes)
    && backend.wire_bytes === compressedBytes + 16 && hash64(backend.wire_sha256)
    && hash64(backend.plaintext_sha256) && backend.plaintext_sha256 === compressedSha256
    && finiteTiming(backend.encrypt) && finiteTiming(backend.decrypt)
    && aesChecksPass(backend.checks);
}

function validateZstdBackend(backend, expectedLabel, inputBytes, inputSha256) {
  return !!backend && backend.backend === expectedLabel && Number.isSafeInteger(backend.decoded_bytes)
    && backend.decoded_bytes === inputBytes && hash64(backend.decoded_sha256)
    && backend.decoded_sha256 === inputSha256 && finiteTiming(backend.decode) && zstdChecksPass(backend.checks);
}

function validateConformanceBackend(backend, expectedLabel, inputBytes, inputSha256) {
  return !!backend && backend.backend === expectedLabel && Number.isSafeInteger(backend.wire_bytes)
    && backend.wire_bytes === inputBytes + 16 && hash64(backend.wire_sha256)
    && hash64(backend.plaintext_sha256) && backend.plaintext_sha256 === inputSha256 && aesChecksPass(backend.checks);
}

function validateConformance(report, expectedLane) {
  const vector = report.aes_conformance;
  if (!vector || vector.vector_id !== CONFORMANCE_VECTOR.id || vector.input_bytes !== CONFORMANCE_VECTOR.inputBytes || vector.input_sha256 !== CONFORMANCE_VECTOR.inputSha256) throw new Error(expectedLane + ' AES canonical conformance vector mismatch');
  const labels = expectedLane === 'native'
    ? ['RustCrypto aes-gcm 0.10.3 AES-256-GCM', 'ring 0.17.14 AES-256-GCM']
    : ['RustCrypto aes-gcm 0.10.3 AES-256-GCM (WASM)', 'Node WebCrypto SubtleCrypto AES-GCM (Node stand-in; browser unmeasured)'];
  const first = vector.rustcrypto;
  const second = expectedLane === 'native' ? vector.ring : vector.webcrypto;
  if (!validateConformanceBackend(first, labels[0], vector.input_bytes, vector.input_sha256)
    || !validateConformanceBackend(second, labels[1], vector.input_bytes, vector.input_sha256)
    || first.wire_bytes !== second.wire_bytes || first.wire_sha256 !== CONFORMANCE_VECTOR.wireSha256 || second.wire_sha256 !== CONFORMANCE_VECTOR.wireSha256) throw new Error(expectedLane + ' AES conformance A/B wire parity missing');
  if (expectedLane === 'native' && vector.webcrypto !== undefined) throw new Error('native conformance has unexpected WebCrypto field');
  if (expectedLane === 'wasm-node' && vector.ring !== undefined) throw new Error('WASM conformance has unexpected ring field');
}

function validateReport(report, expectedLane, expectedRun, expectedManifest, expectedLock, expectedSource) {
  if (!report || report.schema !== 'mmdpack-phase0-backends/1' || report.lane !== expectedLane
    || report.campaign_schema !== 'mmdpack-phase0-campaign/1' || report.status !== 'ok'
    || report.run_id !== expectedRun || report.environment.campaign_manifest_sha256 !== expectedManifest
    || report.environment.cargo_lock_sha256 !== expectedLock || report.environment.harness_source_digest !== expectedSource
    || !Array.isArray(report.failures) || report.failures.length !== 0) throw new Error(expectedLane + ' metadata/status mismatch');
  if (report.config.timing_policy !== 'one warmup plus five measured iterations; OS/file caches uncontrolled; p50/p95 are directional') throw new Error(expectedLane + ' timing policy mismatch');
  if (report.config.zstd_frame_policy !== 'single frame; declared content size; no dictionary/checksum; window <= 64 MiB; decoded <= 128 MiB') throw new Error(expectedLane + ' Zstandard policy mismatch');
  if (!Array.isArray(report.cases) || report.cases.length < 10 || report.cases.length > 20) throw new Error(expectedLane + ' case count mismatch');
  const environment = report.environment;
  if (!Number.isSafeInteger(environment.native_binary_bytes)
    || (expectedLane === 'wasm-node' && (!Number.isSafeInteger(environment.wasm_wrapper_bytes) || !Number.isSafeInteger(environment.wasm_module_bytes) || !hash64(environment.wasm_wrapper_sha256) || !hash64(environment.wasm_module_sha256)))) throw new Error(expectedLane + ' binary size observation missing');
  if (!report.observable_copies || !Number.isSafeInteger(report.observable_copies.input_calls) || !Number.isSafeInteger(report.observable_copies.input_bytes)) throw new Error(expectedLane + ' copy observation missing');
  validateConformance(report, expectedLane);
  const ids = new Set();
  const aesLabels = expectedLane === 'native'
    ? ['RustCrypto aes-gcm 0.10.3 AES-256-GCM', 'ring 0.17.14 AES-256-GCM']
    : ['RustCrypto aes-gcm 0.10.3 AES-256-GCM (WASM)', 'Node WebCrypto SubtleCrypto AES-GCM (Node stand-in; browser unmeasured)'];
  const zstdLabels = expectedLane === 'native'
    ? ['zstd 0.13.3 / libzstd 1.5.7', 'ruzstd 0.8.3']
    : ['zstd 0.13.3 / libzstd 1.5.7 (WASM)', 'ruzstd 0.8.3 (WASM)'];
  for (const item of report.cases) {
    if (!safeId(item.id) || ids.has(item.id) || !['pmx', 'vmd'].includes(item.kind) || !['small', 'medium', 'large'].includes(item.size_class) || !safePathLabel(item.path_label)) throw new Error(expectedLane + ' unsafe case metadata');
    ids.add(item.id);
    if (!Number.isSafeInteger(item.input_bytes) || !Number.isSafeInteger(item.compressed_bytes) || !hash64(item.input_sha256) || !hash64(item.compressed_sha256)) throw new Error(expectedLane + ' case sizes/hashes missing');
    if (!item.aes || !validateAesBackend(item.aes.rustcrypto, aesLabels[0], item.compressed_bytes, item.compressed_sha256)) throw new Error(expectedLane + ' AES RustCrypto evidence missing');
    const secondAes = expectedLane === 'native' ? item.aes.ring : item.aes.webcrypto;
    if (!validateAesBackend(secondAes, aesLabels[1], item.compressed_bytes, item.compressed_sha256)) throw new Error(expectedLane + ' AES candidate evidence missing');
    if (expectedLane === 'native' && item.aes.webcrypto !== undefined) throw new Error('native case has unexpected WebCrypto field');
    if (expectedLane === 'wasm-node' && item.aes.ring !== undefined) throw new Error('WASM case has unexpected ring field');
    if (!item.zstd || !validateZstdBackend(item.zstd.libzstd, zstdLabels[0], item.input_bytes, item.input_sha256) || !validateZstdBackend(item.zstd.ruzstd, zstdLabels[1], item.input_bytes, item.input_sha256)) throw new Error(expectedLane + ' Zstandard A/B evidence missing');
  }
}

function validatePair(native, wasm) {
  if (native.measured_at_utc !== wasm.measured_at_utc || native.campaign_schema !== wasm.campaign_schema) throw new Error('Native/WASM run metadata drift');
  const fields = ['aes_wire', 'aes_reference', 'aes_candidate', 'zstd_encoder', 'zstd_level', 'zstd_reference_decoder', 'zstd_candidate_decoder', 'zstd_frame_policy', 'max_window_bytes', 'max_decoded_bytes', 'vector_policy', 'timing_policy'];
  if (fields.some((field) => native.config[field] !== wasm.config[field])) throw new Error('Native/WASM configuration drift');
  if (native.cases.length !== wasm.cases.length) throw new Error('Native/WASM case count drift');
  if (native.aes_conformance.input_bytes !== wasm.aes_conformance.input_bytes || native.aes_conformance.input_sha256 !== wasm.aes_conformance.input_sha256 || native.aes_conformance.rustcrypto.wire_sha256 !== wasm.aes_conformance.rustcrypto.wire_sha256 || native.aes_conformance.ring.wire_sha256 !== wasm.aes_conformance.webcrypto.wire_sha256 || native.aes_conformance.rustcrypto.wire_sha256 !== native.aes_conformance.ring.wire_sha256) throw new Error('AES conformance wire drift');
  for (let index = 0; index < native.cases.length; index += 1) {
    const a = native.cases[index]; const b = wasm.cases[index];
    if (a.id !== b.id || a.kind !== b.kind || a.size_class !== b.size_class || a.input_bytes !== b.input_bytes || a.input_sha256 !== b.input_sha256 || a.compressed_bytes !== b.compressed_bytes || a.compressed_sha256 !== b.compressed_sha256) throw new Error('Native/WASM case drift at ' + index);
    if (a.zstd.libzstd.decoded_bytes !== b.zstd.libzstd.decoded_bytes || a.zstd.ruzstd.decoded_bytes !== b.zstd.ruzstd.decoded_bytes || a.zstd.libzstd.decoded_sha256 !== b.zstd.libzstd.decoded_sha256 || a.zstd.ruzstd.decoded_sha256 !== b.zstd.ruzstd.decoded_sha256) throw new Error('Native/WASM Zstandard drift for ' + a.id);
  }
}

function summary(a, b) {
  return a.p50_ms.toFixed(3) + '/' + a.p95_ms.toFixed(3) + '; ' + b.p50_ms.toFixed(3) + '/' + b.p95_ms.toFixed(3) + ' ms; ' + a.throughput_mib_s.toFixed(1) + '/' + b.throughput_mib_s.toFixed(1) + ' MiB/s';
}

function backendRow(item, native, wasm) {
  const n = native.aes; const w = wasm.aes;
  return '| ' + cell(item.id) + ' | ' + cell(item.kind) + ' | ' + cell(item.size_class) + ' | ' + item.input_bytes + ' | ' + item.input_sha256 + ' | ' + item.compressed_bytes + ' | ' + item.compressed_sha256 + ' | ' + n.rustcrypto.wire_bytes + ' | ' + summary(n.rustcrypto.encrypt, n.ring.encrypt) + ' | ' + summary(w.rustcrypto.encrypt, w.webcrypto.encrypt) + ' | ' + summary(native.zstd.libzstd.decode, native.zstd.ruzstd.decode) + ' | ' + summary(wasm.zstd.libzstd.decode, wasm.zstd.ruzstd.decode) + ' | A/B OK |\\n';
}

function render(native, wasm) {
  const env = native.environment;
  const vector = native.aes_conformance;
  let output = '# MMDPACK Phase 0 backend A/B decision\\n\\n';
  output += '> Measurement evidence only. The MMDPACK V1 backend and container profile remain unfrozen.\\n\\n';
  output += '## Result\\n\\n**Decision blocked.** Windows Native and Node/WASM stand-in compatibility evidence is insufficient for a production backend. macOS Native, real browser Web Crypto, first-render, memory, and copy hard-gate evidence remain deferred.\\n\\n';
  output += '## Reproducibility and configuration\\n\\n';
  output += '- Run ID: ' + cell(native.run_id) + '; measured at: ' + cell(native.measured_at_utc) + '\\n- Campaign manifest SHA-256: ' + env.campaign_manifest_sha256 + '\\n- Standalone Cargo.lock SHA-256: ' + env.cargo_lock_sha256 + '\\n- Harness source digest: ' + env.harness_source_digest + '\\n';
  output += '- Native: ' + cell(env.platform) + ', ' + cell(env.rustc) + ', ' + cell(env.native_runner) + '; binary ' + env.native_binary_bytes + ' bytes.\\n';
  output += '- WASM/Node: ' + cell(wasm.environment.platform) + '; JS wrapper ' + wasm.environment.wasm_wrapper_bytes + ' bytes / ' + wasm.environment.wasm_wrapper_sha256 + '; _bg.wasm ' + wasm.environment.wasm_module_bytes + ' bytes / ' + wasm.environment.wasm_module_sha256 + '. Node WebCrypto is a stand-in, not browser evidence.\\n';
  output += '- MSRV: ' + cell(env.msrv_statement) + '\\n- AES wire: ' + cell(native.config.aes_wire) + '; Zstd encoder: ' + cell(native.config.zstd_encoder) + '\\n- Decoder A/B: ' + cell(native.config.zstd_reference_decoder) + ' vs ' + cell(native.config.zstd_candidate_decoder) + '; level ' + native.config.zstd_level + '\\n- Frame limits: ' + cell(native.config.zstd_frame_policy) + '\\n- Vector policy: ' + cell(native.config.vector_policy) + '\\n- Timing: ' + cell(native.config.timing_policy) + '\\n- Fixed conformance vector: ' + vector.input_bytes + ' bytes / ' + vector.input_sha256 + '; all four Native/WASM AES backends match wire SHA-256 ' + vector.rustcrypto.wire_sha256 + '.\\n\\n';
  output += '## Per-case measurements\\n\\n| ID | Kind | Class | Input bytes | Source SHA-256 | Frame bytes | Frame SHA-256 | AES wire bytes | Native AES A/B (p50/p95 ms; MiB/s) | WASM AES A/B (p50/p95 ms; MiB/s) | Native Zstd A/B (p50/p95 ms; MiB/s) | WASM Zstd A/B (p50/p95 ms; MiB/s) | Checks |\\n|---|---|---|---:|---|---:|---|---:|---:|---:|---:|---:|---|\\n';
  for (let index = 0; index < native.cases.length; index += 1) output += backendRow(native.cases[index], native.cases[index], wasm.cases[index]);
  output += '\\nRows share source/frame hashes across lanes. AES performance wires are not cross-lane compared because campaign nonce/AAD domains differ; fixed conformance wire parity is the cross-backend compatibility vector. Each lane A/B and each Zstd decoder matched its required plaintext/source hash.\\n\\n';
  output += '## Boundary and observability limits\\n\\n';
  output += '- Wrong-key, wrong-AAD, one-byte tamper, truncation, and declared-size checks were green for every case and conformance backend. Parser tests cover reserved/dictionary/checksum/trailing-frame and structurally complete single-frame over-window boundaries before decoder allocation.\\n';
  output += '- Native input: ' + native.observable_copies.input_calls + ' calls / ' + native.observable_copies.input_bytes + ' bytes. WASM/Node reads: ' + wasm.observable_copies.input_calls + ' calls / ' + wasm.observable_copies.input_bytes + ' bytes. JS/WASM copy count/bytes, RSS, JS heap peak, WASM linear-memory peak, and largest allocation are unavailable.\\n';
  output += '- Warmup/repeat distributions are directional; OS/file caches were uncontrolled. Native JSON, WASM JSON, and this Markdown are separate file-atomic publications, not a transactional set.\\n\\n';
  output += '## Bounded conclusion and next task\\n\\nNo backend is selected for V1. Keep RustCrypto/ring AES and libzstd/ruzstd as explicit candidates. Next, measure macOS arm64 Native, real Chrome/Firefox/Safari Web Crypto, WebGL2/WebGPU first-render paths, and reliable memory/copy metrics; production choice remains blocked until each target hard-gate metric is observed.\\n';
  return output.replaceAll(String.fromCharCode(92) + 'n', '\n');
}

function main() {
  const native = readJson(value('--native-json'));
  const wasm = readJson(value('--wasm-json'));
  const expectedRun = value('--expected-run-id');
  const expectedManifest = value('--expected-manifest-sha256');
  const expectedLock = value('--expected-lock-sha256');
  const expectedSource = value('--expected-source-digest');
  validateReport(native, 'native', expectedRun, expectedManifest, expectedLock, expectedSource);
  validateReport(wasm, 'wasm-node', expectedRun, expectedManifest, expectedLock, expectedSource);
  validatePair(native, wasm);
  const outputPath = path.resolve(value('--output'));
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  const temporary = outputPath + '.' + process.pid + '.tmp';
  fs.writeFileSync(temporary, render(native, wasm));
  fs.renameSync(temporary, outputPath);
  process.stdout.write('backend decision report candidate generated: ' + outputPath + '\\n');
}

try { main(); } catch (error) { process.stderr.write('render-report: ' + error.message + '\\n'); process.exitCode = 1; }
