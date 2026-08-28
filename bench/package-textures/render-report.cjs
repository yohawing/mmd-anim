const fs = require('fs');
const crypto = require('crypto');
const sha256 = b => crypto.createHash('sha256').update(b).digest('hex');
const value = (args, flag) => { const i = args.indexOf(flag); if (i < 0 || !args[i + 1]) throw new Error(`missing ${flag}`); return args[i + 1]; };
const safeCell = value => typeof value === 'string' && !/[|`\r\n]/.test(value);
const hash = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
function validate(native, wasm, expectedManifest, expectedLock, expectedSource) {
  if (native.schema !== 1 || wasm.schema !== 1 || native.lane !== 'native' || wasm.lane !== 'wasm') throw new Error('lane schema mismatch');
  if (native.status !== 'ok') throw new Error('native lane is not green');
  if (native.run_id !== wasm.run_id || native.manifest_sha256 !== expectedManifest || wasm.manifest_sha256 !== expectedManifest || native.lock_sha256 !== expectedLock || wasm.lock_sha256 !== expectedLock || native.source_digest !== expectedSource || wasm.source_digest !== expectedSource || JSON.stringify(native.config) !== JSON.stringify(wasm.config)) throw new Error('run provenance or codec configuration drift');
  if (!Array.isArray(native.cases) || !Array.isArray(wasm.cases) || native.cases.length !== wasm.cases.length || native.cases.length < 10 || native.cases.length > 20) throw new Error('lane case count mismatch');
  for (let i = 0; i < native.cases.length; i += 1) {
    const n = native.cases[i], w = wasm.cases[i];
    if (!w || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(n.id) || n.id === '.' || n.id === '..' || !safeCell(n.id) || !safeCell(n.class) || !safeCell(n.color_space) || !['toon', 'alpha_cutout', 'diffuse', 'normal'].includes(n.class) || !['srgb', 'linear'].includes(n.color_space) || !hash(n.source_sha256) || n.id !== w.id || n.source_sha256 !== w.source_sha256 || !hash(n.candidate_a?.reconstructed_ktx2_sha256) || !hash(n.candidate_b?.ktx2_sha256) || !n.uastc_levels_equal || n.native_a?.ok !== true || n.native_b?.ok !== true || w.candidate_level_hashes_match !== true || w.candidate_file_hashes_match !== true || w.wasm_a?.input_bytes !== n.candidate_a.reconstructed_ktx2_bytes || w.wasm_b?.input_bytes !== n.candidate_b.ktx2_bytes || w.wasm_a?.input_sha256 !== n.candidate_a.reconstructed_ktx2_sha256 || w.wasm_b?.input_sha256 !== n.candidate_b.ktx2_sha256) throw new Error(`lane comparability failed for case index ${i}`);
    if (w.decoded_hashes_match !== null && typeof w.decoded_hashes_match !== 'boolean') throw new Error(`decoded hash state invalid for case index ${i}`);
    if (w.wasm_a?.ok && w.wasm_b?.ok && w.decoded_hashes_match !== true) throw new Error(`decoded candidate mismatch for case index ${i}`);
  }
  if (wasm.boundary_checks.candidate_file_hashes_match !== true) throw new Error('candidate file hash boundary check failed');
  if (wasm.status === 'ok' && wasm.boundary_checks.decoded_hashes_match !== true) throw new Error('decoded hash boundary check failed');
  if (wasm.status === 'incompatible' && wasm.boundary_checks.decoded_hashes_match !== null) throw new Error('incompatible decode state must be unavailable');
  if (wasm.status !== 'ok' && wasm.status !== 'incompatible') throw new Error('unexpected WASM lane status');
}
function formatMs(x) { return Number.isFinite(x) ? x.toFixed(3) : 'unavailable'; }
function run(args) {
  const native = JSON.parse(fs.readFileSync(value(args, '--native'), 'utf8'));
  const wasm = JSON.parse(fs.readFileSync(value(args, '--wasm'), 'utf8'));
  const expectedManifest = value(args, '--expected-manifest-sha256');
  const expectedLock = value(args, '--expected-lock-sha256');
  const expectedSource = value(args, '--source-digest');
  validate(native, wasm, expectedManifest, expectedLock, expectedSource);
  const blocked = wasm.status !== 'ok';
  const lines = [];
  lines.push('# MMDPACK Texture Phase 0 Decision');
  lines.push('');
  lines.push('This is maintainer-only Phase 0 evidence. It is not a frozen V1 codec or `.mmdpack` container decision.');
  lines.push('');
  lines.push(`- Run ID: \`${native.run_id}\``);
  lines.push('- Timing policy: single measured iteration; OS/file caches uncontrolled; no warmup or repeat; timings are directional.');
  lines.push(`- Manifest SHA-256: \`${expectedManifest}\``);
  lines.push(`- Standalone Cargo.lock SHA-256: \`${expectedLock}\``);
  lines.push(`- Harness source digest: \`${expectedSource}\``);
  lines.push(`- Native Basis Universal: v2.50.0, executable SHA-256 \`${native.basisu_sha256}\``);
  lines.push(`- Three.js transcoder revision: \`${wasm.transcoder_revision}\`, JS SHA-256 \`${wasm.transcoder_js_sha256}\`, WASM SHA-256 \`${wasm.transcoder_wasm_sha256}\``);
  lines.push('- Environment: Windows native lane and Node ' + process.version + '; MSRV 1.87 build not verified on this host; measured with Rust 1.98.0.');
  lines.push('- Encoding config: UASTC LDR 4x4, `uastc_level=2`, RDO lambda `1.0`, mipmaps enabled, Candidate A KTX2 without Zstandard plus probe-level Zstandard 3, Candidate B KTX2 internal Zstandard 3.');
  lines.push('- Observable input evidence: each candidate was supplied through one WASM input call, and input bytes are counted; JS/WASM copy counts, heap/RSS peaks are unavailable and not claimed.');
  lines.push('');
  if (blocked) {
    lines.push('## Decision'); lines.push('');
    lines.push('Decision blocked: the configured Three.js transcoder and pinned Basis Universal v2.50 encoder output were not compatible (`KTX2File.isValid() == false`). A known-good control was not run, so adapter/tool-version causes were not isolated; the smallest missing evidence is a compatible, reviewed transcode control. No candidate is recommended.');
  } else {
    lines.push('## Decision'); lines.push('');
    lines.push('Recommendation: KTX2 UASTC with internal Zstandard level 3, because all per-mip raw UASTC and decoded RGBA32 hashes agree across candidates. This remains Phase 0 evidence and does not freeze V1.');
  }
  lines.push('');
  lines.push('## Case measurements'); lines.push('');
  lines.push('| Case | Class | Source | Dimensions | A raw payload | A KTX2 reconstructed | B KTX2 | A/B internal compressed | Native A/B ms | WASM A/B ms | A/B decoded | Quality evidence |');
  lines.push('|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|');
  for (const n of native.cases) {
    const w = wasm.cases.find(x => x.id === n.id);
    const a = n.candidate_a, b = n.candidate_b;
    const wasmTime = w.wasm_a?.ok && w.wasm_b?.ok ? `${formatMs(w.wasm_a.elapsed_ms)} / ${formatMs(w.wasm_b.elapsed_ms)}` : 'unavailable';
    const decoded = w.decoded_hashes_match === true ? 'match' : w.decoded_hashes_match === false ? 'mismatch' : 'unavailable';
    lines.push(`| \`${n.id}\` | ${n.class} | ${n.source_bytes} B / \`${n.source_sha256.slice(0, 12)}…\` | ${n.width}×${n.height} | ${a.raw_payload_bytes} B | ${a.reconstructed_ktx2_bytes} B | ${b.ktx2_bytes} B | ${a.raw_zstd_bytes} / ${b.internal_compressed_bytes} B | ${formatMs(n.native_a.elapsed_ms)} / ${formatMs(n.native_b.elapsed_ms)} | ${wasmTime} | ${decoded} | same raw UASTC mip bytes; source metric N/A |`);
  }
  lines.push('');
  lines.push('## Evidence limits'); lines.push('');
  lines.push('- Candidate A is a conservative probe payload: JSON metadata plus compressed raw blocks are counted together. This is not an estimate of a future minimal production representation; reconstruction retains no uncounted KTX2 template bytes.');
  lines.push('- Candidate A and B raw UASTC levels were byte-identical for every native case. This proves candidate decode equivalence, not source-image quality. Source-quality metrics were unavailable in this bounded probe because a PNG decoder/quality reference was not added.');
  lines.push('- Native output hashes are RGBA32 DDS mip payload hashes. WASM output hashes use Three.js Basis transcoder format id 13 (RGBA32) when compatible.');
  lines.push('- Raw JSON and Markdown are published atomically per file through same-volume replacement; the pair is not transactional.');
  lines.push('- All reported paths are manifest labels/IDs only; local asset paths and detailed command errors remain in ignored run artifacts.');
  fs.writeFileSync(value(args, '--output'), lines.join('\n') + '\n');
}
try { run(process.argv.slice(2)); } catch (error) { console.error(error.message || error); process.exitCode = 1; }
