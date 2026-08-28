const fs = require('fs');
const crypto = require('crypto');
const sha256 = b => crypto.createHash('sha256').update(b).digest('hex');
const value = (args, flag) => { const i = args.indexOf(flag); if (i < 0 || !args[i + 1]) throw new Error(`missing ${flag}`); return args[i + 1]; };
const safeCell = value => typeof value === 'string' && !/[|`\r\n]/.test(value);
const hash = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
const positiveInt = value => Number.isSafeInteger(value) && value > 0;
const nonNegativeInt = value => Number.isSafeInteger(value) && value >= 0;
const dimensions = value => Array.isArray(value) && value.length === 2 && positiveInt(value[0]) && positiveInt(value[1]);
const validMip = value => value && positiveInt(value.width) && positiveInt(value.height) && positiveInt(value.bytes) && hash(value.sha256);
const validMips = value => Array.isArray(value) && value.length > 0 && value.every(validMip);
const validHashes = value => Array.isArray(value) && value.length > 0 && value.every(hash);
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);

function validate(native, wasm, control, expectedManifest, expectedLock, expectedSource, expectedControlInput) {
  if (native.schema !== 1 || wasm.schema !== 1 || native.lane !== 'native' || wasm.lane !== 'wasm') throw new Error('lane schema mismatch');
  if (native.status !== 'ok') throw new Error('native lane is not green');
  if (typeof native.run_id !== 'string' || native.run_id.length === 0 || native.run_id !== wasm.run_id) throw new Error('native/WASM run ID mismatch');
  if (native.manifest_sha256 !== expectedManifest || wasm.manifest_sha256 !== expectedManifest || native.lock_sha256 !== expectedLock || wasm.lock_sha256 !== expectedLock || native.source_digest !== expectedSource || wasm.source_digest !== expectedSource || JSON.stringify(native.config) !== JSON.stringify(wasm.config)) throw new Error('run provenance or codec configuration drift');
  if (!control || control.schema !== 1 || control.lane !== 'wasm-control' || control.status !== 'ok' || control.result?.ok !== true || control.run_id !== native.run_id || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(control.input_label) || !hash(control.input_sha256) || control.input_sha256 !== expectedControlInput || control.result.input_sha256 !== control.input_sha256 || control.result.input_bytes !== control.input_bytes || control.transcoder_js_sha256 !== wasm.transcoder_js_sha256 || control.transcoder_wasm_sha256 !== wasm.transcoder_wasm_sha256 || control.fixture_repo_relative !== 'examples/textures/ktx2/2d_uastc.ktx2' || control.fixture_revision !== 'three.js b5673888ec8b7ff279a93135c50bdb07f1900dba' || !positiveInt(control.result.levels) || control.result.levels !== control.result.mips?.length || !validMips(control.result.mips)) throw new Error('known-good WASM control is not green or provenance drifted');
  if (!Array.isArray(native.cases) || !Array.isArray(wasm.cases) || native.cases.length !== wasm.cases.length || native.cases.length < 10 || native.cases.length > 20) throw new Error('lane case count mismatch');
  const aggregate = { cases: native.cases.length, native_a_mips: 0, native_b_mips: 0, wasm_a_mips: 0, wasm_b_mips: 0 };
  for (let i = 0; i < native.cases.length; i += 1) {
    const n = native.cases[i], w = wasm.cases[i];
    const na = n?.native_a, nb = n?.native_b, wa = w?.wasm_a, wb = w?.wasm_b;
    const candidateA = n?.candidate_a, candidateB = n?.candidate_b;
    if (!w || !/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(n.id) || n.id === '.' || n.id === '..' || !safeCell(n.id) || !safeCell(n.class) || !safeCell(n.color_space) || !['toon', 'alpha_cutout', 'diffuse', 'normal'].includes(n.class) || !['srgb', 'linear'].includes(n.color_space) || !hash(n.source_sha256) || n.id !== w.id || n.source_sha256 !== w.source_sha256 || !hash(candidateA?.reconstructed_ktx2_sha256) || !hash(candidateB?.ktx2_sha256) || !nonNegativeInt(candidateA?.reconstructed_ktx2_bytes) || !nonNegativeInt(candidateB?.ktx2_bytes) || candidateB.ktx2_bytes >= candidateA.reconstructed_ktx2_bytes || n.uastc_levels_equal !== true || na?.ok !== true || nb?.ok !== true || !validHashes(candidateA.level_hashes) || !validHashes(candidateB.level_hashes) || candidateA.level_hashes.length !== candidateB.level_hashes.length || candidateA.level_hashes.length !== na.mip_hashes?.length || !equal(candidateA.level_hashes, candidateB.level_hashes) || !validHashes(na.mip_hashes) || !validHashes(nb.mip_hashes) || na.mip_hashes.length !== nb.mip_hashes.length || !equal(na.mip_hashes, nb.mip_hashes) || !Array.isArray(na.mip_dimensions) || !Array.isArray(nb.mip_dimensions) || na.mip_dimensions.length !== na.mip_hashes.length || nb.mip_dimensions.length !== nb.mip_hashes.length || !na.mip_dimensions.every(dimensions) || !nb.mip_dimensions.every(dimensions) || !equal(na.mip_dimensions, nb.mip_dimensions) || w.candidate_level_hashes_match !== true || w.candidate_file_hashes_match !== true || wa?.ok !== true || wb?.ok !== true || !positiveInt(wa.levels) || !positiveInt(wb.levels) || !validMips(wa.mips) || !validMips(wb.mips) || wa.levels !== wa.mips.length || wb.levels !== wb.mips.length || wa.levels !== wb.levels || wa.levels !== candidateA.level_hashes.length || !equal(wa.mips.map(m => [m.width, m.height]), wb.mips.map(m => [m.width, m.height])) || !equal(wa.mips.map(m => m.sha256), wb.mips.map(m => m.sha256)) || w.wasm_a?.input_bytes !== candidateA.reconstructed_ktx2_bytes || w.wasm_b?.input_bytes !== candidateB.ktx2_bytes || w.wasm_a?.input_sha256 !== candidateA.reconstructed_ktx2_sha256 || w.wasm_b?.input_sha256 !== candidateB.ktx2_sha256) throw new Error(`lane comparability failed for case index ${i}`);
    if (w.decoded_hashes_match !== null && typeof w.decoded_hashes_match !== 'boolean') throw new Error(`decoded hash state invalid for case index ${i}`);
    if (w.decoded_hashes_match !== true) throw new Error(`decoded candidate mismatch or unavailable for case index ${i}`);
    aggregate.native_a_mips += na.mip_hashes.length;
    aggregate.native_b_mips += nb.mip_hashes.length;
    aggregate.wasm_a_mips += wa.mips.length;
    aggregate.wasm_b_mips += wb.mips.length;
  }
  if (wasm.boundary_checks.candidate_file_hashes_match !== true || wasm.boundary_checks.candidate_level_hashes_match !== true || wasm.boundary_checks.case_order_match !== true || wasm.boundary_checks.source_hashes_match !== true || wasm.boundary_checks.candidate_sizes_match !== true) throw new Error('WASM comparability boundary check failed');
  if (wasm.boundary_checks.decoded_hashes_match !== true) throw new Error('decoded hash boundary check failed');
  if (wasm.status !== 'ok') throw new Error('WASM lane is not green');
  return aggregate;
}
function formatMs(x) { return Number.isFinite(x) ? x.toFixed(3) : 'unavailable'; }
function run(args) {
  const native = JSON.parse(fs.readFileSync(value(args, '--native'), 'utf8'));
  const wasm = JSON.parse(fs.readFileSync(value(args, '--wasm'), 'utf8'));
  const control = JSON.parse(fs.readFileSync(value(args, '--control'), 'utf8'));
  const expectedManifest = value(args, '--expected-manifest-sha256');
  const expectedLock = value(args, '--expected-lock-sha256');
  const expectedSource = value(args, '--source-digest');
  const expectedControlInput = value(args, '--expected-control-input-sha256');
  const aggregate = validate(native, wasm, control, expectedManifest, expectedLock, expectedSource, expectedControlInput);
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
  lines.push(`- Known-good control: \`${control.input_label}\`, ${control.result.mips.length} RGBA32 mips, input SHA-256 \`${control.input_sha256}\`.`);
  lines.push(`- Fixture provenance: Three.js revision \`${control.fixture_revision}\`; repo-relative label \`${control.fixture_repo_relative}\`.`);
  lines.push(`- Aggregate coverage: ${aggregate.cases} cases; Native A/B ${aggregate.native_a_mips}/${aggregate.native_b_mips} mips (${aggregate.native_a_mips + aggregate.native_b_mips} total); WASM A/B ${aggregate.wasm_a_mips}/${aggregate.wasm_b_mips} mips (${aggregate.wasm_a_mips + aggregate.wasm_b_mips} total).`);
  lines.push('- Environment: Windows native lane and Node ' + process.version + '; MSRV 1.87 build not verified on this host; measured with Rust 1.98.0.');
  lines.push('- Encoding config: UASTC LDR 4x4, `uastc_level=2`, RDO lambda `1.0`, mipmaps enabled, Candidate A KTX2 without Zstandard plus probe-level Zstandard 3, Candidate B KTX2 internal Zstandard 3.');
  lines.push('- Observable input evidence: each candidate was supplied through one WASM input call, and input bytes are counted; JS/WASM copy counts, heap/RSS peaks are unavailable and not claimed.');
  lines.push('');
  lines.push('## Decision'); lines.push('');
  lines.push(`Recommendation: KTX2 UASTC with internal Zstandard level 3. The pinned Three.js known-good control (${control.result.mips.length} mips) succeeded, both candidates decoded through the same lane-local A/B checks, and all candidate per-mip raw UASTC hashes agree. Candidate B is the standard KTX2 path and was smaller than Candidate A's reconstructed KTX2 in every case; Candidate A also requires custom probe metadata/reconstruction. Candidate A remains a conservative probe payload; this is Phase 0 evidence and does not freeze V1.`);
  lines.push('');
  lines.push('## Case measurements'); lines.push('');
  lines.push('| Case | Class | Source | Dimensions | A raw payload | A KTX2 reconstructed | B KTX2 | A/B internal compressed | Native A/B ms | WASM A/B ms | A/B decoded | Quality evidence |');
  lines.push('|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|');
  for (const n of native.cases) {
    const w = wasm.cases.find(x => x.id === n.id);
    const a = n.candidate_a, b = n.candidate_b;
    const wasmTime = `${formatMs(w.wasm_a.elapsed_ms)} / ${formatMs(w.wasm_b.elapsed_ms)}`;
    lines.push(`| \`${n.id}\` | ${n.class} | ${n.source_bytes} B / \`${n.source_sha256.slice(0, 12)}…\` | ${n.width}×${n.height} | ${a.raw_payload_bytes} B | ${a.reconstructed_ktx2_bytes} B | ${b.ktx2_bytes} B | ${a.raw_zstd_bytes} / ${b.internal_compressed_bytes} B | ${formatMs(n.native_a.elapsed_ms)} / ${formatMs(n.native_b.elapsed_ms)} | ${wasmTime} | match | same raw UASTC mip bytes; source metric N/A |`);
  }
  lines.push('');
  lines.push('## Evidence limits'); lines.push('');
  lines.push('- Candidate A is a conservative probe payload: JSON metadata plus compressed raw blocks are counted together. This is not an estimate of a future minimal production representation; reconstruction retains no uncounted KTX2 template bytes.');
  lines.push('- Candidate A and B raw UASTC levels were byte-identical for every native case. This proves candidate decode equivalence, not source-image quality. Source-quality metrics were unavailable in this bounded probe because a PNG decoder/quality reference was not added.');
  lines.push('- The size comparison favors standard Candidate B KTX2 files over the reconstructed Candidate A files in this campaign; Candidate A raw payload bytes are not a minimal-production estimate. Standard-loader control success and the extra custom A reconstruction path are the bounded interoperability/complexity evidence.');
  lines.push('- Native A/B decoded DDS hashes agree within the Native lane; WASM A/B decoded RGBA32 hashes agree within the WASM lane. These are lane-local checks, not cross-lane byte parity claims.');
  lines.push('- Raw JSON, decision Markdown, and control Markdown are published atomically per file through same-volume replacement; the three published artifacts are not a transactional set.');
  lines.push('- All reported paths are manifest labels/IDs only; local asset paths and detailed command errors remain in ignored run artifacts.');
  fs.writeFileSync(value(args, '--output'), lines.join('\n') + '\n');
}
try { run(process.argv.slice(2)); } catch (error) { console.error(error.message || error); process.exitCode = 1; }
