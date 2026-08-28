const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');

const WARMUP = 1;
const REPEATS = 5;
const REPORT_SCHEMA = 'mmdpack-phase0-backends/1';
const AES_REFERENCE = 'AES-256-GCM fixed profile (ciphertext || 16-byte tag)';
const AES_CANDIDATE = 'Native RustCrypto/ring or WASM RustCrypto/Node WebCrypto';
const ZSTD_ENCODER = 'zstd crate 0.13.3 / libzstd 1.5.7, level 3 (one frame per case)';
const RUSTCRYPTO_BACKEND = 'RustCrypto aes-gcm 0.10.3 AES-256-GCM (WASM)';
const WEBCRYPTO_BACKEND = 'Node WebCrypto SubtleCrypto AES-GCM (Node stand-in; browser unmeasured)';
const TEST_VECTOR_INPUT = Buffer.from('mmdpack Phase 0 backend conformance vector');
const TEST_VECTOR_KEY = Buffer.alloc(32, 0x42);
const TEST_VECTOR_NONCE = Buffer.alloc(12, 0x24);
const TEST_VECTOR_AAD = Buffer.from('mmdpack-phase0-backends/conformance/v1');
let CAMPAIGN_KEY = null;
const CONFIG = {
  aes_wire: AES_REFERENCE,
  aes_reference: AES_REFERENCE,
  aes_candidate: AES_CANDIDATE,
  zstd_encoder: ZSTD_ENCODER,
  zstd_level: 3,
  zstd_reference_decoder: 'zstd crate 0.13.3 / libzstd 1.5.7',
  zstd_candidate_decoder: 'ruzstd 0.8.3',
  zstd_frame_policy: 'single frame; declared content size; no dictionary/checksum; window <= 64 MiB; decoded <= 128 MiB',
  max_window_bytes: 64 * 1024 * 1024,
  max_decoded_bytes: 128 * 1024 * 1024,
  vector_policy: 'campaign uses a fresh run-scoped 32-byte key from an ephemeral environment value; every warmup/repeat encryption has a backend/domain/iteration-unique nonce and AAD; one fixed public key/nonce/AAD vector is conformance-only and excluded from campaign performance; no secrets serialized',
  timing_policy: 'one warmup plus five measured iterations; OS/file caches uncontrolled; p50/p95 are directional',
};

function value(name, fallback = undefined) {
  const index = process.argv.indexOf(name);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

function required(name) {
  const result = value(name);
  if (result === undefined) throw new Error(`missing ${name}`);
  return result;
}

function sha256(bytes) {
  return crypto.createHash('sha256').update(bytes).digest('hex');
}

function safeId(id) {
  return /^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(id) && id !== '.' && id !== '..';
}

function safeLabel(label) {
  return typeof label === 'string' && label.length > 0 && !/[\r\n|`]/.test(label) && !/[\u0000-\u001f\u007f]/.test(label);
}

function validateManifest(manifest) {
  if (manifest.schema !== 'mmdpack-phase0-campaign/1' || !Array.isArray(manifest.cases) || manifest.cases.length < 10 || manifest.cases.length > 20) {
    throw new Error('campaign schema or case count is invalid');
  }
  const ids = new Set();
  for (const item of manifest.cases) {
    if (!safeId(item.id) || ids.has(item.id)) throw new Error(`invalid or duplicate campaign case id: ${item.id}`);
    ids.add(item.id);
    if (!['pmx', 'vmd'].includes(item.kind) || !['small', 'medium', 'large'].includes(item.size_class)) {
      throw new Error(`unsupported campaign class for ${item.id}`);
    }
    if (!safeLabel(item.path_label) || typeof item.path !== 'string' || item.path.length === 0) {
      throw new Error(`invalid campaign path metadata for ${item.id}`);
    }
  }
}

function campaignMaterial(id, domain) {
  if (!CAMPAIGN_KEY) throw new Error('campaign key is not initialized');
  const nonce = crypto.createHash('sha256').update(Buffer.concat([
    Buffer.from('mmdpack-phase0-backends/nonce/v2/'), CAMPAIGN_KEY,
    Buffer.from(id), Buffer.from(domain),
  ])).digest().subarray(0, 12);
  const aad = Buffer.from(`mmdpack-phase0-backends/aad/v2/${id}/${domain}`);
  return { key: Buffer.from(CAMPAIGN_KEY), nonce, aad };
}

function timing(bytes, operation) {
  for (let i = 0; i < WARMUP; i += 1) operation(i);
  const samples = [];
  let last;
  for (let i = 0; i < REPEATS; i += 1) {
    const start = process.hrtime.bigint();
    last = operation(WARMUP + i);
    samples.push(Number(process.hrtime.bigint() - start) / 1e6);
  }
  const sorted = [...samples].sort((a, b) => a - b);
  const p50 = sorted[Math.floor(REPEATS / 2)];
  const p95 = sorted[REPEATS - 1];
  return {
    value: last,
    timing: {
      warmup: WARMUP,
      repeats: REPEATS,
      samples_ms: samples,
      p50_ms: p50,
      p95_ms: p95,
      throughput_mib_s: p50 > 0 ? bytes / (1024 * 1024) / (p50 / 1000) : 0,
    },
  };
}

async function timingAsync(bytes, operation) {
  for (let i = 0; i < WARMUP; i += 1) await operation(i);
  const samples = [];
  let last;
  for (let i = 0; i < REPEATS; i += 1) {
    const start = process.hrtime.bigint();
    last = await operation(WARMUP + i);
    samples.push(Number(process.hrtime.bigint() - start) / 1e6);
  }
  const sorted = [...samples].sort((a, b) => a - b);
  const p50 = sorted[Math.floor(REPEATS / 2)];
  const p95 = sorted[REPEATS - 1];
  return {
    value: last,
    timing: {
      warmup: WARMUP,
      repeats: REPEATS,
      samples_ms: samples,
      p50_ms: p50,
      p95_ms: p95,
      throughput_mib_s: p50 > 0 ? bytes / (1024 * 1024) / (p50 / 1000) : 0,
    },
  };
}

function fixedChecks(wire, plaintext, key, nonce, aad, decrypt) {
  const wrongKey = Buffer.from(key);
  wrongKey[0] ^= 1;
  const wrongAad = Buffer.concat([Buffer.from(aad), Buffer.from([1])]);
  const tampered = Buffer.from(wire);
  tampered[0] ^= 1;
  const rejected = (candidate, candidateKey, candidateNonce, candidateAad) => {
    try { decrypt(candidate, candidateKey, candidateNonce, candidateAad); return false; } catch { return true; }
  };
  return {
    round_trip: Buffer.from(decrypt(wire, key, nonce, aad)).equals(plaintext),
    wrong_key_rejected: rejected(wire, wrongKey, nonce, aad),
    wrong_aad_rejected: rejected(wire, key, nonce, wrongAad),
    tamper_rejected: rejected(tampered, key, nonce, aad),
    truncation_rejected: rejected(wire.subarray(0, wire.length - 1), key, nonce, aad),
  };
}

async function fixedChecksAsync(wire, plaintext, key, nonce, aad, decrypt) {
  const wrongKey = Buffer.from(key);
  wrongKey[0] ^= 1;
  const wrongAad = Buffer.concat([Buffer.from(aad), Buffer.from([1])]);
  const tampered = Buffer.from(wire);
  tampered[0] ^= 1;
  const rejected = async (candidate, candidateKey, candidateNonce, candidateAad) => {
    try { await decrypt(candidate, candidateKey, candidateNonce, candidateAad); return false; } catch { return true; }
  };
  return {
    round_trip: (await decrypt(wire, key, nonce, aad)).equals(plaintext),
    wrong_key_rejected: await rejected(wire, wrongKey, nonce, aad),
    wrong_aad_rejected: await rejected(wire, key, nonce, wrongAad),
    tamper_rejected: await rejected(tampered, key, nonce, aad),
    truncation_rejected: await rejected(wire.subarray(0, wire.length - 1), key, nonce, aad),
  };
}

function rustcryptoDecryptExplicit(wasm, wire, key, nonce, aad) {
  return Buffer.from(wasm.rustcrypto_decrypt_explicit(
    new Uint8Array(wire), new Uint8Array(key), new Uint8Array(nonce), new Uint8Array(aad),
  ));
}

function makeRustcryptoConformance(wasm) {
  const wire = Buffer.from(wasm.rustcrypto_encrypt_conformance());
  const decrypt = (candidate, key, nonce, aad) => rustcryptoDecryptExplicit(wasm, candidate, key, nonce, aad);
  const checks = fixedChecks(wire, TEST_VECTOR_INPUT, TEST_VECTOR_KEY, TEST_VECTOR_NONCE, TEST_VECTOR_AAD, decrypt);
  if (!Object.values(checks).every((value) => value === true)) throw new Error('WASM RustCrypto conformance checks failed');
  return {
    backend: RUSTCRYPTO_BACKEND,
    wire_bytes: wire.length,
    wire_sha256: sha256(wire),
    plaintext_sha256: sha256(decrypt(wire, TEST_VECTOR_KEY, TEST_VECTOR_NONCE, TEST_VECTOR_AAD)),
    checks,
  };
}

async function webcryptoEncrypt(input, material) {
  const key = await crypto.webcrypto.subtle.importKey('raw', material.key, 'AES-GCM', false, ['encrypt', 'decrypt']);
  return Buffer.from(await crypto.webcrypto.subtle.encrypt({ name: 'AES-GCM', iv: material.nonce, additionalData: material.aad, tagLength: 128 }, key, input));
}

async function webcryptoDecrypt(wire, material) {
  const key = await crypto.webcrypto.subtle.importKey('raw', material.key, 'AES-GCM', false, ['decrypt']);
  return Buffer.from(await crypto.webcrypto.subtle.decrypt({ name: 'AES-GCM', iv: material.nonce, additionalData: material.aad, tagLength: 128 }, key, wire));
}

async function makeWebcryptoConformance() {
  const material = { key: TEST_VECTOR_KEY, nonce: TEST_VECTOR_NONCE, aad: TEST_VECTOR_AAD };
  const wire = await webcryptoEncrypt(TEST_VECTOR_INPUT, material);
  const decrypt = (candidate, key, nonce, aad) => webcryptoDecrypt(candidate, { key, nonce, aad });
  const checks = await fixedChecksAsync(wire, TEST_VECTOR_INPUT, material.key, material.nonce, material.aad, decrypt);
  if (!Object.values(checks).every((value) => value === true)) throw new Error('Node WebCrypto conformance checks failed');
  return {
    backend: 'Node WebCrypto SubtleCrypto AES-GCM (Node stand-in; browser unmeasured)',
    wire_bytes: wire.length,
    wire_sha256: sha256(wire),
    plaintext_sha256: sha256(await webcryptoDecrypt(wire, material)),
    checks,
  };
}

function makeRustcryptoReport(wasm, frame, id) {
  let finalMaterial;
  const measuredEncrypt = timing(frame.length, (iteration) => {
    const domain = `wasm/rustcrypto/encrypt/${iteration}`;
    finalMaterial = campaignMaterial(id, domain);
    return Buffer.from(wasm.rustcrypto_encrypt_campaign(new Uint8Array(frame), new Uint8Array(finalMaterial.key), id, domain));
  });
  const wire = measuredEncrypt.value;
  const decrypt = (candidate, key, nonce, aad) => rustcryptoDecryptExplicit(wasm, candidate, key, nonce, aad);
  const measuredDecrypt = timing(frame.length, () => decrypt(wire, finalMaterial.key, finalMaterial.nonce, finalMaterial.aad));
  const checks = fixedChecks(wire, frame, finalMaterial.key, finalMaterial.nonce, finalMaterial.aad, decrypt);
  if (!Object.values(checks).every((value) => value === true)) throw new Error(`WASM RustCrypto AES boundary failure for ${id}`);
  return {
    backend: RUSTCRYPTO_BACKEND,
    wire_bytes: wire.length,
    wire_sha256: sha256(wire),
    plaintext_sha256: sha256(measuredDecrypt.value),
    encrypt: measuredEncrypt.timing,
    decrypt: measuredDecrypt.timing,
    checks,
  };
}

async function makeWebcryptoReport(frame, id) {
  let finalMaterial;
  const measuredEncrypt = await timingAsync(frame.length, (iteration) => {
    finalMaterial = campaignMaterial(id, `wasm/webcrypto/encrypt/${iteration}`);
    return webcryptoEncrypt(frame, finalMaterial);
  });
  const wire = measuredEncrypt.value;
  const measuredDecrypt = await timingAsync(frame.length, () => webcryptoDecrypt(wire, finalMaterial));
  const decrypt = (candidate, key, nonce, aad) => webcryptoDecrypt(candidate, { key, nonce, aad });
  const checks = await fixedChecksAsync(wire, frame, finalMaterial.key, finalMaterial.nonce, finalMaterial.aad, decrypt);
  if (!Object.values(checks).every((value) => value === true)) throw new Error(`Node WebCrypto AES boundary failure for ${id}`);
  return {
    backend: WEBCRYPTO_BACKEND,
    wire_bytes: wire.length,
    wire_sha256: sha256(wire),
    plaintext_sha256: sha256(measuredDecrypt.value),
    encrypt: measuredEncrypt.timing,
    decrypt: measuredDecrypt.timing,
    checks,
  };
}

function makeZstdReport(wasm, frame, input, id, decoder, label) {
  const measured = timing(input.length, () => Buffer.from(decoder(new Uint8Array(frame), input.length)));
  const decoded = measured.value;
  const rejects = (operation) => { try { operation(); return false; } catch { return true; } };
  const checks = {
    round_trip: decoded.equals(input),
    size_limit_rejected: rejects(() => decoder(new Uint8Array(frame), Math.max(0, input.length - 1))),
    truncation_rejected: rejects(() => decoder(new Uint8Array(frame.subarray(0, frame.length - 1)), input.length)),
  };
  if (!Object.values(checks).every((value) => value === true)) throw new Error(`WASM ${label} Zstandard boundary failure for ${id}`);
  return {
    backend: `${label} (WASM)`,
    decoded_bytes: decoded.length,
    decoded_sha256: sha256(decoded),
    decode: measured.timing,
    checks,
  };
}

async function main() {
  const root = __dirname;
  const manifestPath = required('--manifest');
  const lockPath = required('--cargo-lock');
  const runDir = path.resolve(required('--run-dir'));
  const outputPath = path.resolve(required('--output'));
  const wrapperPath = path.resolve(required('--wasm-wrapper'));
  const modulePath = path.resolve(required('--wasm-module'));
  const boundModulePath = path.join(path.dirname(wrapperPath), `${path.basename(wrapperPath, '.js')}_bg.wasm`);
  if (modulePath !== boundModulePath) throw new Error('WASM wrapper/module paths are not the generated binding pair');
  const manifestBytes = fs.readFileSync(manifestPath);
  const manifestHash = sha256(manifestBytes);
  if (required('--expected-manifest-sha256') !== manifestHash) throw new Error('manifest hash does not match bytes read for this run');
  const lockBytes = fs.readFileSync(lockPath);
  const lockHash = sha256(lockBytes);
  if (required('--expected-lock-sha256') !== lockHash) throw new Error('Cargo.lock hash does not match bytes read for this run');
  const manifest = JSON.parse(manifestBytes.toString('utf8'));
  validateManifest(manifest);
  const campaignKeyHex = process.env.MMDPACK_BACKENDS_CAMPAIGN_KEY_HEX;
  if (typeof campaignKeyHex !== 'string' || !/^[0-9a-fA-F]{64}$/.test(campaignKeyHex)) throw new Error('campaign key environment value must contain 64 hex characters');
  CAMPAIGN_KEY = Buffer.from(campaignKeyHex, 'hex');
  const wrapperBytesBefore = fs.readFileSync(wrapperPath);
  const moduleBytesBefore = fs.readFileSync(modulePath);
  const wrapperHashBefore = sha256(wrapperBytesBefore);
  const moduleHashBefore = sha256(moduleBytesBefore);
  const wasm = require(wrapperPath);
  const wrapperBytes = fs.readFileSync(wrapperPath);
  const moduleBytes = fs.readFileSync(modulePath);
  if (wrapperBytes.length !== wrapperBytesBefore.length || sha256(wrapperBytes) !== wrapperHashBefore
    || moduleBytes.length !== moduleBytesBefore.length || sha256(moduleBytes) !== moduleHashBefore) {
    throw new Error('WASM wrapper/module bytes changed while loading');
  }
  const aesRustcryptoConformance = makeRustcryptoConformance(wasm);
  const aesWebcryptoConformance = await makeWebcryptoConformance();
  const aesConformance = {
    vector_id: 'aes-gcm-phase0-fixed-v1',
    input_bytes: TEST_VECTOR_INPUT.length,
    input_sha256: sha256(TEST_VECTOR_INPUT),
    rustcrypto: aesRustcryptoConformance,
    webcrypto: aesWebcryptoConformance,
  };
  if (aesRustcryptoConformance.wire_sha256 !== aesWebcryptoConformance.wire_sha256) throw new Error('WASM/Node WebCrypto conformance wire drift detected');
  const cases = [];
  let sourceBytes = 0;
  for (const item of manifest.cases) {
    const input = fs.readFileSync(item.path);
    const caseDir = path.resolve(runDir, 'cases', item.id);
    const rootDir = path.resolve(runDir);
    if (!caseDir.startsWith(`${rootDir}${path.sep}`)) throw new Error(`case directory escaped run directory for ${item.id}`);
    const frame = fs.readFileSync(path.join(caseDir, 'frame.zst'));
    const rustcrypto = makeRustcryptoReport(wasm, frame, item.id);
    const webcrypto = await makeWebcryptoReport(frame, item.id);
    const libzstd = makeZstdReport(wasm, frame, input, item.id, wasm.zstd_decode_libzstd, 'zstd 0.13.3 / libzstd 1.5.7');
    const ruzstd = makeZstdReport(wasm, frame, input, item.id, wasm.zstd_decode_ruzstd_wasm, 'ruzstd 0.8.3');
    const aes = { rustcrypto, webcrypto };
    const zstd = { libzstd, ruzstd, decoded_equal: libzstd.decoded_sha256 === ruzstd.decoded_sha256 };
    if (!zstd.decoded_equal) throw new Error(`WASM backend output drift detected for ${item.id}`);
    sourceBytes += input.length + frame.length;
    cases.push({
      id: item.id, kind: item.kind, size_class: item.size_class, path_label: item.path_label,
      input_bytes: input.length, input_sha256: sha256(input), compressed_bytes: frame.length,
      compressed_sha256: sha256(frame), aes, zstd,
    });
  }
  const rustc = (() => { try { return execFileSync('rustc', ['--version'], { encoding: 'utf8' }).trim(); } catch { return 'unavailable'; } })();
  const report = {
    schema: REPORT_SCHEMA, lane: 'wasm-node', campaign_schema: manifest.schema, description: manifest.description,
    status: 'ok', run_id: required('--run-id'), measured_at_utc: required('--measured-at'), aes_conformance: aesConformance,
    environment: {
      platform: `${process.platform}-${process.arch}`, rustc, native_runner: 'not-run (WASM/Node JSON lane)',
      native_binary_bytes: 0,
      wasm_wrapper_bytes: wrapperBytes.length, wasm_wrapper_sha256: sha256(wrapperBytes),
      wasm_module_bytes: moduleBytes.length, wasm_module_sha256: sha256(moduleBytes),
      campaign_manifest_sha256: manifestHash, cargo_lock_sha256: lockHash,
      wasm_pack: value('--wasm-pack', 'not-provided'), harness_source_digest: required('--harness-source-digest'),
      msrv_statement: `MSRV 1.87 build not verified on this host; measured with ${rustc}`,
      webcrypto_mode: 'Node.js WebCrypto stand-in; actual browser host unmeasured',
    },
    config: CONFIG, cases, failures: [],
    observable_copies: {
      input_calls: manifest.cases.length * 2, input_bytes: sourceBytes,
      js_wasm_copy_count: null, js_wasm_copy_bytes: null,
      note: 'Node source/frame reads and WASM input calls are counted; JS↔WASM copy count/bytes and heap peaks are unavailable',
    },
  };
  fs.mkdirSync(path.dirname(outputPath), { recursive: true });
  const temporary = `${outputPath}.${process.pid}.tmp`;
  fs.writeFileSync(temporary, `${JSON.stringify(report, null, 2)}\n`);
  fs.renameSync(temporary, outputPath);
  process.stdout.write(`wasm/node backend lane complete: cases=${cases.length} output=${outputPath}\n`);
}

main().catch((error) => {
  process.stderr.write(`package-backends-wasm: ${error.message}\n`);
  process.exitCode = 1;
});
