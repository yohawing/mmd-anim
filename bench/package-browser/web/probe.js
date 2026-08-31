import * as THREE from 'three';
import { KTX2Loader } from '/three/examples/jsm/loaders/KTX2Loader.js';
import {
  decodeBase64, measureCase, parseTexture, sha256, verifyCryptoRejections,
} from '/probe-common.js';

const statusNode = document.querySelector('#status');

function createRenderer() {
  const renderer = new THREE.WebGLRenderer({ preserveDrawingBuffer: true, antialias: false, powerPreference: 'high-performance' });
  renderer.setSize(16, 16, false);
  renderer.setClearColor(0xff00ff, 1);
  const gl = renderer.getContext();
  if (!(gl instanceof WebGL2RenderingContext)) throw Error('WebGL2 required');
  return { renderer, gl };
}

async function drawTexture(loader, bytes, { renderer, gl }) {
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
      const measured = await measureCase({ loader, descriptor, caseIndex: index, key, drawTexture, drawContext: { renderer, gl } });
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
