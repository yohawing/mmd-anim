import * as THREE from 'three';
import { KTX2Loader } from '/three/examples/jsm/loaders/KTX2Loader.js';

const statusNode = document.querySelector('#status');
const decodeBase64 = value => Uint8Array.from(atob(value), char => char.charCodeAt(0));
const sha256 = async bytes => [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))]
  .map(value => value.toString(16).padStart(2, '0')).join('');
const parseTexture = (loader, bytes) => new Promise((resolve, reject) => loader.parse(bytes, resolve, reject));
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

function createRenderer() {
  const renderer = new THREE.WebGLRenderer({ preserveDrawingBuffer: true, antialias: false, powerPreference: 'high-performance' });
  renderer.setSize(16, 16, false);
  renderer.setClearColor(0xff00ff, 1);
  const gl = renderer.getContext();
  if (!(gl instanceof WebGL2RenderingContext)) throw Error('WebGL2 required');
  return { renderer, gl };
}

async function drawTexture(loader, bytes, renderer, gl) {
  const transcodeStart = performance.now();
  const texture = await parseTexture(loader, bytes);
  const transcodeMs = performance.now() - transcodeStart;
  let geometry;
  let material;
  let scene;
  try {
    scene = new THREE.Scene();
    geometry = new THREE.PlaneGeometry(2, 2);
    material = new THREE.MeshBasicMaterial({ map: texture, transparent: false, depthTest: false });
    scene.add(new THREE.Mesh(geometry, material));
    const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 10);
    camera.position.z = 1;
    const gpuStart = performance.now();
    renderer.render(scene, camera);
    gl.finish();
    const gpuCompleteAt = performance.now();
    const pixels = new Uint8Array(16 * 16 * 4);
    gl.readPixels(0, 0, 16, 16, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
    if (gl.getError() !== gl.NO_ERROR) throw Error('WebGL error after readback');
    const mips = [];
    for (const mip of texture.mipmaps) {
      const data = mip.data ?? null;
      mips.push({ width: mip.width, height: mip.height, bytes: data?.byteLength ?? data?.length ?? 0, sha256: data ? await sha256(data) : null });
    }
    if (!mips.length || mips.some(mip => !mip.width || !mip.height || !mip.bytes)) throw Error('invalid mip metadata');
    const clear = [255, 0, 255, 255];
    return {
      transcode_ms: transcodeMs,
      upload_render_finish_ms: gpuCompleteAt - gpuStart,
      gpu_complete_at: gpuCompleteAt,
      pixels_sha256: await sha256(pixels),
      pixels_non_clear: pixels.some((value, index) => value !== clear[index % 4]),
      mips,
      format: texture.format,
      type: texture.type,
      color_space: texture.colorSpace,
      is_compressed_texture: texture.isCompressedTexture === true,
    };
  } finally {
    texture.dispose();
    material?.dispose();
    geometry?.dispose();
    scene?.clear();
  }
}

async function verifyCryptoRejections(key, encryptedEntry) {
  const ciphertext = new Uint8Array(encryptedEntry.ciphertext);
  const nonce = decodeBase64(encryptedEntry.nonce);
  const aad = decodeBase64(encryptedEntry.aad);
  const tampered = ciphertext.slice();
  tampered[0] ^= 1;
  const wrongKey = await crypto.subtle.importKey('raw', crypto.getRandomValues(new Uint8Array(32)), { name: 'AES-GCM' }, false, ['decrypt']);
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

async function measureOnce(loader, id, encrypted, key, renderer, gl) {
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
    const ciphertext = source;
    const nonce = decodeBase64(response.headers.get('x-mmdpack-nonce'));
    const aad = decodeBase64(response.headers.get('x-mmdpack-aad'));
    inputBytes = ciphertext.byteLength;
    metadataDecodeMs = performance.now() - decodeStart;
    const decryptStart = performance.now();
    bytes = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: nonce, additionalData: aad, tagLength: 128 }, key, ciphertext);
    decryptMs = performance.now() - decryptStart;
  }
  const outputBytes = bytes.byteLength;
  const verificationCopyStart = performance.now();
  const verificationBytes = bytes.slice(0);
  const verificationCopyMs = performance.now() - verificationCopyStart;
  const draw = await drawTexture(loader, bytes, renderer, gl);
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

async function measureCase(loader, descriptor, caseIndex, key, renderer, gl) {
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
      const measured = await measureOnce(loader, descriptor.id, encrypted, key, renderer, gl);
      if (measured.result.sha256 !== descriptor.expected_sha256 || measured.result.pixels_non_clear !== true || measured.result.is_compressed_texture !== true) throw Error(`${descriptor.id}: measured result invariant failed`);
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

const runButton = document.querySelector('#run');
runButton.onclick = async () => {
  let renderer;
  let loader;
  try {
    statusNode.textContent = 'Loading configuration…';
    if (!crossOriginIsolated || !isSecureContext) throw Error('secure isolated context required');
    const config = await (await fetch('/api/config', { cache: 'no-store' })).json();
    const officialFirefox = config.firefox_execution_surface === 'official_firefox';
    const zenDiagnostic = config.webgl2_execution_surface === 'zen_browser';
    const keyResponse = await (await fetch('/api/key', { cache: 'no-store' })).json();
    let rawKey = decodeBase64(keyResponse.key);
    const keyImportStart = performance.now();
    let key;
    try {
      key = await crypto.subtle.importKey('raw', rawKey, { name: 'AES-GCM' }, false, ['decrypt']);
    } finally {
      rawKey.fill(0);
    }
    const keyImportMs = performance.now() - keyImportStart;
    if (key.extractable || key.type !== 'secret' || key.usages.length !== 1 || key.usages[0] !== 'decrypt') throw Error('CryptoKey invariant failed');
    const created = createRenderer();
    renderer = created.renderer;
    const gl = created.gl;
    loader = new KTX2Loader().setTranscoderPath('/three/examples/jsm/libs/basis/').setWorkerLimit(1);
    loader.detectSupport(renderer);
    const controlStart = performance.now();
    const controlTexture = await loader.loadAsync('/three/examples/textures/ktx2/2d_uastc.ktx2');
    const controlWarmupMs = performance.now() - controlStart;
    controlTexture.dispose();
    const cases = [];
    for (let index = 0; index < config.cases.length; index += 1) {
      const descriptor = config.cases[index];
      statusNode.textContent = `Running ${descriptor.id}`;
      const encryptedResponse = await fetch(`/api/encrypted/${descriptor.id}`, { cache: 'no-store' });
      if (!encryptedResponse.ok) throw Error(`${descriptor.id}: encrypted rejection input failed`);
      const encryptedEntry = {
        ciphertext: await encryptedResponse.arrayBuffer(),
        nonce: encryptedResponse.headers.get('x-mmdpack-nonce'),
        aad: encryptedResponse.headers.get('x-mmdpack-aad'),
      };
      const cryptoRejections = await verifyCryptoRejections(key, encryptedEntry);
      if (Object.values(cryptoRejections).some(value => !value)) throw Error(`${descriptor.id}: crypto rejection gate failed`);
      const measured = await measureCase(loader, descriptor, index, key, renderer, gl);
      cases.push({ id: descriptor.id, expected_sha256: descriptor.expected_sha256, baseline: measured.baseline,
        encrypted: measured.encrypted, paired_overhead: measured.paired_overhead,
        crypto_rejections: cryptoRejections, equality: measured.equality, lanes_equal: true });
    }
    const debug = gl.getExtension('WEBGL_debug_renderer_info');
    const vendor = debug ? gl.getParameter(debug.UNMASKED_VENDOR_WEBGL) : 'unavailable';
    const gpuRenderer = debug ? gl.getParameter(debug.UNMASKED_RENDERER_WEBGL) : 'unavailable';
    const software = /swiftshader|software|llvmpipe/i.test(`${vendor} ${gpuRenderer}`);
    const gpuClassification = software ? 'software' : debug ? 'hardware' : 'unknown';
    const memory = performance.memory ? {
      js_heap_size_limit: performance.memory.jsHeapSizeLimit,
      total_js_heap_size: performance.memory.totalJSHeapSize,
      used_js_heap_size: performance.memory.usedJSHeapSize,
      authority: 'directional_snapshot_not_peak',
    } : 'unavailable';
    const report = {
      schema: 1, run_id: config.run_id, browser_run_id: `${officialFirefox ? 'firefox-webgl2' : zenDiagnostic ? 'zen-webgl2' : 'chrome'}-${Date.now()}`, provenance: config.provenance,
      environment: {
        ...(officialFirefox ? { execution_surface: 'official_firefox' } : zenDiagnostic ? { execution_surface: 'zen_browser' } : {}),
        ua: navigator.userAgent, cross_origin_isolated: crossOriginIsolated, secure_context: isSecureContext, webgl2: true,
        vendor, renderer: gpuRenderer, gpu_classification: gpuClassification,
        performance_blocked: gpuClassification !== 'hardware', extensions: gl.getSupportedExtensions().sort(), directional_memory: memory,
        unavailable_metrics: ['true_peak_js_heap', 'basis_wasm_linear_memory', 'physical_copy_count', 'gpu_resident_memory', 'compositor_presentation_latency'],
      },
      setup: { key_import_ms: keyImportMs, control_warmup_ms: controlWarmupMs }, cases,
    };
    const reportUrl = officialFirefox ? '/api/firefox/webgl2/report' : zenDiagnostic ? '/api/zen/webgl2/report' : '/api/report';
    const response = await fetch(reportUrl, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(report) });
    if (!response.ok) throw Error(await response.text());
    statusNode.textContent = `Published ${report.browser_run_id}: ${cases.length} cases`;
  } catch (error) {
    statusNode.textContent = `FAILED: ${error.stack || error}`;
  } finally {
    loader?.dispose();
    renderer?.dispose();
  }
};

if (new URLSearchParams(location.search).get('autorun') === '1') queueMicrotask(() => runButton.click());
