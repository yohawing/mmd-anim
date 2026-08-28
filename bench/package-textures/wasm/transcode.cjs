const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const vm = require('vm');

const value = (args, flag) => {
  const i = args.indexOf(flag);
  if (i < 0 || !args[i + 1]) throw new Error(`missing ${flag}`);
  return args[i + 1];
};
const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const nowMs = () => Number(process.hrtime.bigint()) / 1e6;
const readJsonOnce = file => {
  const bytes = fs.readFileSync(file);
  return { bytes, value: JSON.parse(bytes.toString('utf8')), hash: sha256(bytes) };
};
function loadCommonJsFactory(file, sourceBytes) {
  const module = { exports: {} };
  const context = {
    module, exports: module.exports, require, __filename: file, __dirname: path.dirname(file),
    console, process, setTimeout, clearTimeout, TextDecoder, WebAssembly, Uint8Array, Uint8ClampedArray,
    ArrayBuffer, Int8Array, Uint32Array, Int32Array, Float32Array, Float64Array, globalThis,
  };
  vm.runInNewContext(sourceBytes.toString('utf8'), context, { filename: file });
  if (typeof module.exports !== 'function') throw new Error('configured basis transcoder is not a CommonJS factory');
  return module.exports;
}
function validateManifest(manifest) {
  if (manifest.schema !== 1 || !Array.isArray(manifest.cases) || manifest.cases.length < 10 || manifest.cases.length > 20) throw new Error('manifest must contain 10-20 cases with schema 1');
  const ids = new Set();
  for (const c of manifest.cases) {
    if (typeof c.id !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(c.id) || c.id === '.' || c.id === '..' || ids.has(c.id)) throw new Error('manifest case IDs must be safe and unique');
    if (typeof c.path !== 'string' || c.path.trim() === '' || !Number.isInteger(c.width) || !Number.isInteger(c.height) || c.width <= 0 || c.height <= 0) throw new Error(`invalid manifest dimensions/path for ${c.id}`);
    if (!['toon', 'alpha_cutout', 'diffuse', 'normal'].includes(c.class)) throw new Error(`unsupported case class for ${c.id}`);
    if (!['srgb', 'linear'].includes(c.color_space) || c.mipmaps !== true) throw new Error(`unsupported case config for ${c.id}`);
    ids.add(c.id);
  }
}
function transcode(module, bytes) {
  const file = new module.KTX2File(new Uint8Array(bytes));
  try {
    if (!file.isValid()) return { ok: false, error: 'configured Three.js Basis transcoder rejected KTX2 (compatibility result)', input_bytes: bytes.length, input_calls: 1, input_sha256: sha256(bytes) };
    const width = file.getWidth();
    const height = file.getHeight();
    const levels = file.getLevels();
    if (!file.startTranscoding()) return { ok: false, error: 'startTranscoding returned false', input_bytes: bytes.length, input_calls: 1, input_sha256: sha256(bytes) };
    const mips = [];
    const start = nowMs();
    for (let level = 0; level < levels; level += 1) {
      const info = file.getImageLevelInfo(0, level, 0);
      const size = file.getImageTranscodedSizeInBytes(0, level, 13);
      const output = new Uint8Array(size);
      if (!file.transcodeImage(output, 0, level, 13, 0, 0)) throw new Error(`RGBA32 transcode failed at mip ${level}`);
      mips.push({ width: info.origWidth, height: info.origHeight, bytes: size, sha256: sha256(output) });
    }
    return { ok: true, elapsed_ms: nowMs() - start, width, height, levels, mips, input_bytes: bytes.length, input_calls: 1, input_sha256: sha256(bytes) };
  } catch (error) {
    return { ok: false, error: String(error), input_bytes: bytes.length, input_calls: 1, input_sha256: sha256(bytes) };
  } finally {
    file.close();
    file.delete();
  }
}
async function run(args) {
  const manifestPath = value(args, '--manifest');
  const nativePath = value(args, '--native');
  const runDir = value(args, '--run-dir');
  const output = value(args, '--output');
  const jsPath = value(args, '--transcoder-js');
  const wasmPath = value(args, '--transcoder-wasm');
  const lockPath = value(args, '--lock');
  const expectedManifest = value(args, '--expected-manifest-sha256');
  const expectedLock = value(args, '--expected-lock-sha256');
  const expectedSource = value(args, '--source-digest');
  const expectedJs = value(args, '--expected-transcoder-js-sha256');
  const expectedWasm = value(args, '--expected-transcoder-wasm-sha256');
  const native = JSON.parse(fs.readFileSync(nativePath, 'utf8'));
  const manifestRead = readJsonOnce(manifestPath);
  validateManifest(manifestRead.value);
  const lockRead = fs.readFileSync(lockPath);
  const jsBytes = fs.readFileSync(jsPath);
  const wasmBytes = fs.readFileSync(wasmPath);
  const jsHash = sha256(jsBytes);
  const wasmHash = sha256(wasmBytes);
  if (manifestRead.hash !== expectedManifest || manifestRead.hash !== native.manifest_sha256 || native.source_digest !== expectedSource) throw new Error('manifest/source provenance drift before WASM lane');
  if (sha256(lockRead) !== expectedLock || sha256(lockRead) !== native.lock_sha256) throw new Error('Cargo.lock provenance drift before WASM lane');
  if (jsHash !== expectedJs || wasmHash !== expectedWasm) throw new Error('transcoder bytes do not match expected hashes');
  const factory = loadCommonJsFactory(jsPath, jsBytes);
  const module = await factory({ wasmBinary: wasmBytes });
  const report = { schema: 1, lane: 'wasm', status: 'ok', run_id: native.run_id, manifest_sha256: manifestRead.hash, lock_sha256: sha256(lockRead), source_digest: expectedSource, config: native.config, transcoder_js_sha256: jsHash, transcoder_wasm_sha256: wasmHash, transcoder_revision: 'three.js b5673888ec8b7ff279a93135c50bdb07f1900dba', format: 13, cases: [], failures: [], boundary_checks: { manifest_native_match: true, lock_native_match: true, case_order_match: true, source_hashes_match: true, candidate_sizes_match: true, candidate_file_hashes_match: true, candidate_level_hashes_match: true, decoded_hashes_match: null } };
  let allDecoded = true;
  let allDecodedMatch = true;
  for (let i = 0; i < manifestRead.value.cases.length; i += 1) {
    const c = manifestRead.value.cases[i];
    const n = native.cases[i];
    if (!n || n.id !== c.id) { report.boundary_checks.case_order_match = false; report.failures.push(`case order drift at index ${i}`); continue; }
    const sourceHash = sha256(fs.readFileSync(c.path));
    if (sourceHash !== n.source_sha256) report.boundary_checks.source_hashes_match = false;
    if (c.id === '.' || c.id === '..' || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(c.id)) throw new Error(`unsafe case ID ${c.id}`);
    const root = fs.realpathSync(runDir);
    const dir = path.resolve(root, c.id);
    if (dir === root || !dir.startsWith(`${root}${path.sep}`)) throw new Error(`case directory escaped run directory for ${c.id}`);
    const resolvedDir = fs.realpathSync(dir);
    if (resolvedDir === root || !resolvedDir.startsWith(`${root}${path.sep}`)) throw new Error(`resolved case directory escaped run directory for ${c.id}`);
    const a = fs.readFileSync(path.join(resolvedDir, 'candidate-a-reconstructed.ktx2'));
    const b = fs.readFileSync(path.join(resolvedDir, 'candidate-b.ktx2'));
    if (a.length !== n.candidate_a.reconstructed_ktx2_bytes || b.length !== n.candidate_b.ktx2_bytes) report.boundary_checks.candidate_sizes_match = false;
    const aHash = sha256(a);
    const bHash = sha256(b);
    const filesMatch = aHash === n.candidate_a.reconstructed_ktx2_sha256 && bHash === n.candidate_b.ktx2_sha256;
    if (!filesMatch) report.boundary_checks.candidate_file_hashes_match = false;
    const wa = transcode(module, a);
    const wb = transcode(module, b);
    const decodedMatch = wa.ok && wb.ok ? JSON.stringify(wa.mips) === JSON.stringify(wb.mips) : null;
    if (!wa.ok || !wb.ok) report.status = 'incompatible';
    if (!wa.ok || !wb.ok) allDecoded = false;
    else if (!decodedMatch) allDecodedMatch = false;
    report.cases.push({ id: c.id, source_sha256: sourceHash, native_source_sha256: n.source_sha256, candidate_level_hashes_match: n.uastc_levels_equal === true, candidate_file_hashes_match: filesMatch, wasm_a: wa, wasm_b: wb, decoded_hashes_match: decodedMatch });
  }
  report.boundary_checks.decoded_hashes_match = allDecoded ? allDecodedMatch : null;
  if (report.status === 'incompatible') report.failures.push('Three.js transcoder compatibility blocked: at least one KTX2File rejected the Basis v2.50 output');
  if (!report.boundary_checks.source_hashes_match || !report.boundary_checks.case_order_match || !report.boundary_checks.candidate_sizes_match || !report.boundary_checks.candidate_file_hashes_match) throw new Error('WASM comparability validation failed');
  fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
}
run(process.argv.slice(2)).catch(error => { console.error(error.stack || error); process.exitCode = 1; });
