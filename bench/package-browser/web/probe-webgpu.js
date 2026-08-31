import * as THREE from 'three';
import { KTX2Loader } from '/three/examples/jsm/loaders/KTX2Loader.js';
import {
  decodeBase64, measureCase, parseTexture, sha256, verifyCryptoRejections,
} from '/probe-common.js';

const statusNode = document.querySelector('#status');

function boundedAdapterText(value) {
  return typeof value === 'string' && value.length > 0 && value.length <= 512 && !/[\u0000-\u001f\u007f]/.test(value)
    ? value : 'unavailable';
}

function adapterInfo(adapter) {
  const info = adapter.info || {};
  return {
    vendor: boundedAdapterText(info.vendor),
    architecture: boundedAdapterText(info.architecture),
    device: boundedAdapterText(info.device),
    description: boundedAdapterText(info.description),
    is_fallback_adapter: adapter.isFallbackAdapter === true,
  };
}

function featureList(features) {
  return [...features].map(String).sort();
}

function deviceLimits(device) {
  const limits = device.limits;
  return {
    maxTextureDimension2D: Number(limits.maxTextureDimension2D),
    maxTextureArrayLayers: Number(limits.maxTextureArrayLayers),
    maxBindGroups: Number(limits.maxBindGroups),
    maxBufferSize: Number(limits.maxBufferSize),
  };
}

function classifyAdapter(info) {
  const description = `${info.vendor} ${info.architecture} ${info.device} ${info.description}`;
  if (info.is_fallback_adapter || /software|swiftshader|llvmpipe|basic rasterizer/i.test(description)) return 'software';
  if (info.vendor === 'unavailable' && info.device === 'unavailable' && info.description === 'unavailable') return 'unknown';
  return 'hardware';
}

async function createRenderer() {
  if (!navigator.gpu) {
    const error = Error('WebGPU unavailable');
    error.code = 'WEBGPU_BLOCKED';
    throw error;
  }
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'high-performance' });
  if (!adapter) {
    const error = Error('WebGPU adapter unavailable');
    error.code = 'WEBGPU_BLOCKED';
    throw error;
  }
  const compressionFeatures = ['texture-compression-bc', 'texture-compression-astc', 'texture-compression-etc2']
    .filter(feature => adapter.features.has(feature));
  if (compressionFeatures.length === 0) {
    const error = Error('WebGPU adapter has no KTX2 GPU compression target');
    error.code = 'WEBGPU_BLOCKED';
    throw error;
  }
  const device = await adapter.requestDevice({ requiredFeatures: compressionFeatures });
  let renderer;
  try {
    renderer = new THREE.WebGPURenderer({ device, antialias: false, powerPreference: 'high-performance' });
    renderer._getFallback = null;
    renderer.setSize(16, 16, false);
    await renderer.init();
    if (renderer._getFallback !== null || renderer.backend?.isWebGPUBackend !== true || renderer.backend.device !== device) throw Error('WebGPU renderer did not retain the requested device without fallback');
    return { adapter, device, renderer, info: adapterInfo(adapter) };
  } catch (error) {
    renderer?.dispose();
    device.destroy();
    throw error;
  }
}

function packReadback(readback, width, height) {
  const rowBytes = width * 4;
  const paddedRowBytes = Math.ceil(rowBytes / 256) * 256;
  const minimumLength = (height - 1) * paddedRowBytes + rowBytes;
  if (!(readback instanceof Uint8Array) || readback.byteLength < minimumLength) throw Error('invalid WebGPU RenderTarget readback');
  const packed = new Uint8Array(rowBytes * height);
  for (let row = 0; row < height; row += 1) packed.set(readback.subarray(row * paddedRowBytes, row * paddedRowBytes + rowBytes), row * rowBytes);
  return packed;
}

async function drawTexture(loader, bytes, { renderer, device }) {
  const transcodeStart = performance.now();
  const texture = await parseTexture(loader, bytes);
  const transcodeMs = performance.now() - transcodeStart;
  let geometry;
  let material;
  let scene;
  let target;
  try {
    scene = new THREE.Scene();
    geometry = new THREE.PlaneGeometry(2, 2);
    material = new THREE.MeshBasicMaterial({ map: texture, transparent: false, depthTest: false });
    scene.add(new THREE.Mesh(geometry, material));
    const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 10);
    camera.position.z = 1;
    target = new THREE.RenderTarget(16, 16, { depthBuffer: false, stencilBuffer: false });
    if (target.texture.format !== THREE.RGBAFormat || target.texture.type !== THREE.UnsignedByteType) throw Error('WebGPU RenderTarget format is not RGBA8');
    const gpuStart = performance.now();
    renderer.setClearColor(0xff00ff, 1);
    renderer.setRenderTarget(target);
    renderer.render(scene, camera);
    await device.queue.onSubmittedWorkDone();
    const gpuCompleteAt = performance.now();
    renderer.setRenderTarget(null);

    const readback = await renderer.readRenderTargetPixelsAsync(target, 0, 0, 16, 16);
    const pixels = packReadback(readback, 16, 16);
    const clear = [255, 0, 255, 255];
    const mips = [];
    for (const mip of texture.mipmaps) {
      const data = mip.data ?? null;
      mips.push({ width: mip.width, height: mip.height, bytes: data?.byteLength ?? data?.length ?? 0, sha256: data ? await sha256(data) : null });
    }
    if (!mips.length || mips.some(mip => !mip.width || !mip.height || !mip.bytes)) throw Error('invalid mip metadata');
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
    renderer.setRenderTarget(null);
    target?.dispose();
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
  let device;
  try {
    statusNode.textContent = 'Loading WebGPU configuration…';
    if (!crossOriginIsolated || !isSecureContext) throw Error('secure isolated context required');
    const config = await (await fetch('/api/config', { cache: 'no-store' })).json();
    const officialFirefox = config.firefox_execution_surface === 'official_firefox';
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
    const created = await createRenderer();
    renderer = created.renderer;
    device = created.device;
    const adapterInfoValue = created.info;
    const adapter = created.adapter;
    loader = new KTX2Loader().setTranscoderPath('/three/examples/jsm/libs/basis/').setWorkerLimit(1);
    loader.detectSupport(renderer);
    const controlStart = performance.now();
    const controlResponse = await fetch('/three/examples/textures/ktx2/2d_uastc.ktx2', { cache: 'no-store' });
    if (!controlResponse.ok) throw Error('known-good control fetch failed');
    const control = await drawTexture(loader, await controlResponse.arrayBuffer(), { renderer, device });
    if (control.is_compressed_texture !== true || control.format === THREE.RGBAFormat || control.pixels_non_clear !== true) throw Error('known-good control warmup failed');
    const controlWarmupMs = performance.now() - controlStart;
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
      const measured = await measureCase({
        loader, descriptor, caseIndex: index, key, drawTexture, drawContext: { renderer, device },
        validateResult: result => result.format !== THREE.RGBAFormat,
      });
      cases.push({ id: descriptor.id, expected_sha256: descriptor.expected_sha256, baseline: measured.baseline,
        encrypted: measured.encrypted, paired_overhead: measured.paired_overhead,
        crypto_rejections: cryptoRejections, equality: measured.equality, lanes_equal: true });
    }
    const gpuClassification = classifyAdapter(adapterInfoValue);
    const memory = performance.memory ? {
      js_heap_size_limit: performance.memory.jsHeapSizeLimit,
      total_js_heap_size: performance.memory.totalJSHeapSize,
      used_js_heap_size: performance.memory.usedJSHeapSize,
      authority: 'directional_snapshot_not_peak',
    } : 'unavailable';
    const report = {
      schema: 1, run_id: config.run_id, browser_run_id: `${officialFirefox ? 'firefox-webgpu' : 'chrome-webgpu'}-${Date.now()}`, provenance: config.provenance,
      environment: {
        ua: navigator.userAgent, execution_surface: officialFirefox ? 'official_firefox' : config.execution_surface,
        cross_origin_isolated: crossOriginIsolated, secure_context: isSecureContext, webgpu: true,
        adapter_info: adapterInfoValue, adapter_features: featureList(adapter.features), device_features: featureList(device.features),
        limits: deviceLimits(device), gpu_classification: gpuClassification,
        performance_blocked: gpuClassification !== 'hardware', directional_memory: memory,
        unavailable_metrics: ['true_peak_js_heap', 'basis_wasm_linear_memory', 'physical_copy_count', 'gpu_resident_memory', 'compositor_presentation_latency'],
      },
      setup: { key_import_ms: keyImportMs, control_warmup_ms: controlWarmupMs }, cases,
    };
    const reportUrl = officialFirefox ? '/api/firefox/webgpu/report' : '/api/webgpu/report';
    const response = await fetch(reportUrl, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(report) });
    if (!response.ok) throw Error(await response.text());
    statusNode.textContent = `Published ${report.browser_run_id}: ${cases.length} cases`;
  } catch (error) {
    statusNode.textContent = error.code === 'WEBGPU_BLOCKED' ? `BLOCKED: ${error.message}` : `FAILED: ${error.stack || error}`;
  } finally {
    loader?.dispose();
    renderer?.dispose();
    if (device) device.destroy();
  }
};

if (new URLSearchParams(location.search).get('autorun') === '1') queueMicrotask(() => runButton.click());
