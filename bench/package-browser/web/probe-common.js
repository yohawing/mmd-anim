export const decodeBase64 = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
export const sha256 = async bytes => [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
  .map(value => value.toString(16).padStart(2, '0')).join('');
export const parseTexture = (loader, bytes) => new Promise((resolve, reject) => loader.parse(bytes, resolve, reject));
const STAGES = ['fetch_ms', 'metadata_decode_ms', 'decrypt_ms', 'transcode_ms', 'upload_render_finish_ms', 'entry_gpu_complete_ms'];

function summarize(samples) {
  const ordered = [...samples];
  const sorted = [...ordered].sort((a, b) => a - b);
  if (sorted.length !== 5 || sorted.some(value => !Number.isFinite(value) || value < 0)) throw Error('invalid timing samples');
  return { samples_ms: ordered, p50_ms: sorted[2], p95_ms: sorted[4] };
}

function summarizeSigned(samples) {
  const ordered = [...samples];
  const sorted = [...ordered].sort((a, b) => a - b);
  if (sorted.length !== 5 || sorted.some(value => !Number.isFinite(value))) throw Error('invalid signed timing samples');
  return { samples_ms: ordered, p50_ms: sorted[2], p95_ms: sorted[4] };
}

export async function verifyCryptoRejections(key, encryptedEntry) {
  const ciphertext = new Uint8Array(encryptedEntry.ciphertext);
  const nonce = decodeBase64(encryptedEntry.nonce);
  const aad = decodeBase64(encryptedEntry.aad);
  const tampered = ciphertext.slice();
  tampered[0] ^= 1;
  const wrongKeyBytes = crypto.getRandomValues(new Uint8Array(32));
  let wrongKey;
  try {
    wrongKey = await crypto.subtle.importKey('raw', wrongKeyBytes, { name: 'AES-GCM' }, false, ['decrypt']);
  } finally {
    wrongKeyBytes.fill(0);
  }
  const rejected = async (candidateKey, candidateNonce, candidateAad, wire) => {
    try {
      await crypto.subtle.decrypt({ name: 'AES-GCM', iv: candidateNonce, additionalData: candidateAad, tagLength: 128 }, candidateKey, wire);
      return false;
    } catch {
      return true;
    }
  };
  return {
    wrong_key: await rejected(wrongKey, nonce, aad, ciphertext),
    wrong_aad: await rejected(key, nonce, crypto.getRandomValues(new Uint8Array(aad.length)), ciphertext),
    tamper: await rejected(key, nonce, aad, tampered),
    truncation: await rejected(key, nonce, aad, ciphertext.slice(0, -1)),
  };
}

async function measureOnce({ loader, id, encrypted, key, drawTexture, drawContext }) {
  const totalStart = performance.now();
  const fetchStart = performance.now();
  const response = await fetch(encrypted ? `/api/encrypted/${id}` : `/api/plain/${id}`, { cache: 'no-store' });
  if (!response.ok) throw Error(`${id}: input fetch failed`);
  const source = await response.arrayBuffer();
  const fetchMs = performance.now() - fetchStart;
  let bytes = source;
  let metadataDecodeMs = 0;
  let decryptMs = 0;
  let inputBytes = source.byteLength;
  if (encrypted) {
    const decodeStart = performance.now();
    const nonce = decodeBase64(response.headers.get('x-mmdpack-nonce'));
    const aad = decodeBase64(response.headers.get('x-mmdpack-aad'));
    inputBytes = source.byteLength;
    metadataDecodeMs = performance.now() - decodeStart;
    const decryptStart = performance.now();
    bytes = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: nonce, additionalData: aad, tagLength: 128 }, key, source);
    decryptMs = performance.now() - decryptStart;
  }
  const outputBytes = bytes.byteLength;
  const verificationCopyStart = performance.now();
  const verificationBytes = bytes.slice(0);
  const verificationCopyMs = performance.now() - verificationCopyStart;
  const draw = await drawTexture(loader, bytes, drawContext);
  const plaintextSha256 = await sha256(verificationBytes);
  return {
    result: {
      sha256: plaintextSha256, mips: draw.mips, pixels_sha256: draw.pixels_sha256,
      pixels_non_clear: draw.pixels_non_clear, format: draw.format, type: draw.type,
      color_space: draw.color_space, is_compressed_texture: draw.is_compressed_texture,
      input_bytes: inputBytes, output_bytes: outputBytes,
    },
    timing: {
      fetch_ms: fetchMs, metadata_decode_ms: metadataDecodeMs, decrypt_ms: decryptMs,
      transcode_ms: draw.transcode_ms, upload_render_finish_ms: draw.upload_render_finish_ms,
      entry_gpu_complete_ms: draw.gpu_complete_at - totalStart - verificationCopyMs,
    },
  };
}

function finishLane(last, rows, encrypted) {
  const timing = {};
  for (const stage of STAGES) timing[stage] = summarize(rows.map(row => row[stage]));
  return {
    ...last,
    timing,
    observed_calls: { warmup: 1, measured: 5, fetch: 6, decrypt: encrypted ? 6 : 0, transcode: 6, gpu_complete: 6 },
    observed_bytes: {
      measured_input_total: rows.reduce((sum, row) => sum + row.input_bytes, 0),
      measured_output_total: rows.reduce((sum, row) => sum + row.output_bytes, 0),
    },
  };
}

const isValidMeasuredResult = (result, descriptor) => result.sha256 === descriptor.expected_sha256
  && result.pixels_non_clear === true && result.is_compressed_texture === true;

export async function measureCase({ loader, descriptor, caseIndex, key, drawTexture, drawContext, validateResult = () => true }) {
  const rows = { baseline: [], encrypted: [] };
  const pairedOverhead = [];
  let lastBaseline;
  let lastEncrypted;
  let reference;
  const resultIdentity = result => JSON.stringify({
    sha256: result.sha256, mips: result.mips, pixels_sha256: result.pixels_sha256,
    pixels_non_clear: result.pixels_non_clear, format: result.format, type: result.type,
    color_space: result.color_space, is_compressed_texture: result.is_compressed_texture,
    output_bytes: result.output_bytes,
  });
  for (let iteration = 0; iteration < 6; iteration += 1) {
    const encryptedFirst = (caseIndex + iteration) % 2 === 1;
    const iterationTiming = {};
    for (const encrypted of encryptedFirst ? [true, false] : [false, true]) {
      const measured = await measureOnce({ loader, id: descriptor.id, encrypted, key, drawTexture, drawContext });
      if (!isValidMeasuredResult(measured.result, descriptor) || !validateResult(measured.result, descriptor)) throw Error(`${descriptor.id}: measured result invariant failed`);
      const identity = resultIdentity(measured.result);
      if (reference === undefined) reference = identity;
      else if (identity !== reference) throw Error(`${descriptor.id}: measured result drift at iteration ${iteration}`);
      iterationTiming[encrypted ? 'encrypted' : 'baseline'] = measured.timing.entry_gpu_complete_ms;
      if (encrypted) lastEncrypted = measured.result;
      else lastBaseline = measured.result;
      if (iteration > 0) rows[encrypted ? 'encrypted' : 'baseline'].push({ ...measured.timing, input_bytes: measured.result.input_bytes, output_bytes: measured.result.output_bytes });
    }
    if (iteration > 0) pairedOverhead.push(iterationTiming.encrypted - iterationTiming.baseline);
  }
  const baseline = finishLane(lastBaseline, rows.baseline, false);
  const encrypted = finishLane(lastEncrypted, rows.encrypted, true);
  const equality = {
    baseline_input: baseline.sha256 === descriptor.expected_sha256,
    encrypted_input: encrypted.sha256 === descriptor.expected_sha256,
    pixels: baseline.pixels_sha256 === encrypted.pixels_sha256,
    baseline_non_clear: baseline.pixels_non_clear,
    encrypted_non_clear: encrypted.pixels_non_clear,
    mips: JSON.stringify(baseline.mips) === JSON.stringify(encrypted.mips),
    format: baseline.format === encrypted.format && baseline.type === encrypted.type && baseline.color_space === encrypted.color_space,
  };
  if (Object.values(equality).some(value => !value)) throw Error(`${descriptor.id}: equality ${JSON.stringify(equality)}`);
  return { baseline, encrypted, paired_overhead: summarizeSigned(pairedOverhead), equality };
}
