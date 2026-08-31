import http from 'node:http';
import { createCipheriv, createHash, randomBytes } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { publishReport, validateFirefoxWebGL2Report, validateFirefoxWebGPUReport, validateReport, validateWebGPUReport, validateZenWebGL2Report } from './lib.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const harnessRoot = join(root, 'bench/package-browser');
const webRoot = join(harnessRoot, 'web');
// Only the self-test opts into temporary roots; normal runs retain fixed
// campaign inputs, external Three.js checkout, and pinned-source validation.
const selfTestRoot = process.env.MMDPACK_BROWSER_SELF_TEST === '1' && process.env.MMDPACK_BROWSER_SELF_TEST_ROOT
  ? resolve(process.env.MMDPACK_BROWSER_SELF_TEST_ROOT) : null;
const textureRoot = selfTestRoot ? join(selfTestRoot, 'textures') : join(root, '.ai/mmdpack/textures');
const threeRoot = selfTestRoot ? join(selfTestRoot, 'three') : resolve(root, '..', 'references/three.js');
const latestPath = selfTestRoot ? join(selfTestRoot, 'latest.json') : join(textureRoot, 'latest.json');
const outputRoot = selfTestRoot ? join(selfTestRoot, 'output') : join(root, '.ai/mmdpack/browser');
const documentRoot = selfTestRoot ? join(selfTestRoot, 'documents') : join(root, 'docs');
const documentPath = join(documentRoot, 'mmdpack-browser-webgl2-decision.md');
const webgpuDocumentPath = join(documentRoot, 'mmdpack-browser-webgpu-decision.md');
const zenDocumentPath = join(documentRoot, 'mmdpack-browser-zen-webgl2-diagnostic.md');
const firefoxDocumentPath = join(documentRoot, 'mmdpack-browser-firefox-webgl2-decision.md');
const firefoxWebgpuDocumentPath = join(documentRoot, 'mmdpack-browser-firefox-webgpu-decision.md');
const webgpuExecutionSurface = process.env.MMDPACK_BROWSER_AUTHORITY === 'external_chrome_extension'
  ? 'external_chrome_extension' : 'diagnostic';
const zenExecutionSurface = process.env.MMDPACK_BROWSER_AUTHORITY === 'zen_browser'
  ? 'zen_browser' : 'diagnostic';
const firefoxExecutionSurface = process.env.MMDPACK_BROWSER_AUTHORITY === 'official_firefox'
  ? 'official_firefox' : 'diagnostic';
const SLUG = /^(?!\.{1,2}$)[A-Za-z0-9][A-Za-z0-9._-]*$/;
const pins = {
  control: '21b6912cae1f074ae3eda1b751f43c36eafc7eb83f3af71f85bba2ccbafce125',
  basisJs: '9042facffa0b63ec1c897919c5db43f000c3dee03d9698b6b2465bb06446d298',
  basisWasm: '6cf17dc889352c42e9acf8897107978d127005fe3386c36a0e3845e27967630a',
  revision: 'b5673888ec8b7ff279a93135c50bdb07f1900dba',
};
const threeHashes = new Map([
  ['build/three.module.js', 'be571b5b87ebb742aa7a5317482c1ae568f28c8723c749860a583e5eaa702e25'],
  ['build/three.core.js', '28f286e4d242d45309f018544e471c6a912afd2015e9eb0d62ee1c4b723fb70e'],
  ['build/three.webgpu.js', '6c0a95ed368d5b97638665a19612bcf60d898dd4b208f406d72e58dad8dad811'],
  ['examples/jsm/loaders/KTX2Loader.js', '109773ed42979b66bdbfc82a1f4eacd61747015957aee91e721b0505365a61dd'],
  ['examples/jsm/utils/WorkerPool.js', '989ce70b268e2848626f48a43b9934d4f764aa92cb2d64bc4f6ce8c602449f3b'],
  ['examples/jsm/libs/ktx-parse.module.js', '48756a80f10fb5c00dbfd56de9c8e213b5fc22cb5edb29f4ccd7aeaaaee05f5f'],
  ['examples/jsm/libs/zstddec.module.js', 'a0213f6d252470085e7fec3a9c9e9147c84c277e7fd64e79f96a961c321c4b66'],
  ['examples/jsm/math/ColorSpaces.js', '12e39192f0151e264ca06f5525fd8219db28a57839b6b8c4a036424c5029a1d7'],
  ['examples/jsm/libs/basis/basis_transcoder.js', pins.basisJs],
  ['examples/jsm/libs/basis/basis_transcoder.wasm', pins.basisWasm],
  ['examples/textures/ktx2/2d_uastc.ktx2', pins.control],
]);
const threeRoutes = new Map([
  ['/three/build/three.module.js', 'build/three.module.js'],
  ['/three/build/three.core.js', 'build/three.core.js'],
  ['/three/build/three.webgpu.js', 'build/three.webgpu.js'],
  ['/three/examples/jsm/loaders/KTX2Loader.js', 'examples/jsm/loaders/KTX2Loader.js'],
  ['/three/examples/jsm/utils/WorkerPool.js', 'examples/jsm/utils/WorkerPool.js'],
  ['/three/examples/jsm/libs/ktx-parse.module.js', 'examples/jsm/libs/ktx-parse.module.js'],
  ['/three/examples/jsm/libs/zstddec.module.js', 'examples/jsm/libs/zstddec.module.js'],
  ['/three/examples/jsm/math/ColorSpaces.js', 'examples/jsm/math/ColorSpaces.js'],
  ['/three/examples/jsm/libs/basis/basis_transcoder.js', 'examples/jsm/libs/basis/basis_transcoder.js'],
  ['/three/examples/jsm/libs/basis/basis_transcoder.wasm', 'examples/jsm/libs/basis/basis_transcoder.wasm'],
  ['/three/examples/textures/ktx2/2d_uastc.ktx2', 'examples/textures/ktx2/2d_uastc.ktx2'],
]);
const harnessFiles = ['README.md', 'lib.mjs', 'self-test.mjs', 'server.mjs', 'web/index.html', 'web/index-firefox.html', 'web/index-firefox-webgpu.html', 'web/index-zen.html', 'web/probe.js', 'web/index-webgpu.html', 'web/probe-webgpu.js'];

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
function digestNamed(items) {
  const hash = createHash('sha256');
  for (const [name, bytes] of [...items].sort(([a], [b]) => a.localeCompare(b))) hash.update(`${name} ${sha256(bytes)} ${bytes.length}\n`);
  return hash.digest('hex');
}
function contained(base, candidate) {
  const path = relative(base, candidate);
  return path !== '' && !path.startsWith('..') && !resolve(candidate).startsWith('\\\\');
}
async function readContained(base, relativePath) {
  const canonicalBase = await realpath(base);
  const candidate = await realpath(join(base, relativePath));
  if (!contained(canonicalBase.toLowerCase(), candidate.toLowerCase())) throw Error(`path escaped configured root: ${relativePath}`);
  return { path: candidate, bytes: await readFile(candidate) };
}

async function loadSources() {
  const latestBytes = await readFile(latestPath);
  const latest = JSON.parse(latestBytes.toString('utf8'));
  const cases = latest.native?.cases;
  if (!SLUG.test(latest.run_id) || !Array.isArray(cases) || cases.length !== 10) throw Error('latest texture run identity/count invalid');
  const ids = new Set();
  const inputs = [];
  const caseMap = new Map();
  for (const item of cases) {
    if (!SLUG.test(item.id) || ids.has(item.id)) throw Error('unsafe or duplicate case id');
    ids.add(item.id);
    const base = join(textureRoot, 'runs', latest.run_id);
    const loaded = await readContained(base, join(item.id, 'candidate-b.ktx2'));
    if (loaded.bytes.length !== item.candidate_b?.ktx2_bytes || sha256(loaded.bytes) !== item.candidate_b?.ktx2_sha256) throw Error(`Candidate B drift: ${item.id}`);
    inputs.push([item.id, loaded.bytes]);
    const mips = item.native_b?.mip_dimensions;
    if (!Array.isArray(mips) || mips.length === 0 || mips.some(pair => !Array.isArray(pair) || pair.length !== 2 || pair.some(value => !Number.isInteger(value) || value <= 0))) throw Error(`Candidate B mip topology invalid: ${item.id}`);
    caseMap.set(item.id, { id: item.id, bytes: loaded.bytes.length, sha256: sha256(loaded.bytes), mips, plaintext: loaded.bytes });
  }
  const served = new Map();
  for (const [route, relativePath] of threeRoutes) {
    const bytes = (await readContained(threeRoot, relativePath)).bytes;
    if (!selfTestRoot && sha256(bytes) !== threeHashes.get(relativePath)) throw Error(`pinned Three.js source drift: ${relativePath}`);
    served.set(route, bytes);
  }
  if (!selfTestRoot && (latest.control?.input_sha256 !== pins.control || latest.control?.transcoder_js_sha256 !== pins.basisJs || latest.control?.transcoder_wasm_sha256 !== pins.basisWasm || latest.control?.transcoder_revision !== `three.js ${pins.revision}`)) throw Error('texture run control provenance drift');
  const harness = [];
  for (const name of harnessFiles) harness.push([name, (await readContained(harnessRoot, name)).bytes]);
  const provenance = {
    control_sha256: pins.control,
    revision: pins.revision,
    latest_sha256: sha256(latestBytes),
    inputs_digest: digestNamed(inputs),
    three_digest: digestNamed([...served].map(([route, bytes]) => [route, bytes])),
    harness_digest: digestNamed(harness),
  };
  return { latest, caseMap, served, provenance };
}

async function initialize() {
  const sources = await loadSources();
  const key = randomBytes(32);
  const entries = new Map();
  const nonces = new Set();
  for (const item of sources.caseMap.values()) {
    const nonce = randomBytes(12);
    const nonceHex = nonce.toString('hex');
    if (nonces.has(nonceHex)) throw Error('nonce reuse');
    nonces.add(nonceHex);
    const aad = randomBytes(24);
    const cipher = createCipheriv('aes-256-gcm', key, nonce);
    cipher.setAAD(aad);
    const encrypted = Buffer.concat([cipher.update(item.plaintext), cipher.final(), cipher.getAuthTag()]);
    entries.set(item.id, { ...item, nonce, aad, encrypted });
  }
  return { ...sources, key, entries };
}

const headers = (type = 'application/octet-stream') => ({
  'content-type': type,
  'cache-control': 'no-store',
  'cross-origin-opener-policy': 'same-origin',
  'cross-origin-embedder-policy': 'require-corp',
  'cross-origin-resource-policy': 'same-origin',
  'x-content-type-options': 'nosniff',
});
function sendJson(response, body, status = 200) {
  response.writeHead(status, headers('application/json'));
  response.end(JSON.stringify(body));
}
function assertNoSecrets(report, state) {
  const serialized = JSON.stringify(report);
  const secrets = [state.key, ...[...state.entries.values()].flatMap(entry => [entry.nonce, entry.aad, entry.encrypted])];
  for (const secret of secrets) {
    if (serialized.includes(secret.toString('base64')) || serialized.includes(secret.toString('hex'))) throw Error('report contains encryption material');
  }
}
function mime(route) {
  if (route.endsWith('.wasm')) return 'application/wasm';
  if (route.endsWith('.ktx2')) return 'application/octet-stream';
  return 'text/javascript; charset=utf-8';
}
async function collectJson(request, response) {
  let size = 0;
  const chunks = [];
  for await (const chunk of request) {
    size += chunk.length;
    if (size > 2_000_000) {
      sendJson(response, { error: 'report body too large' }, 413);
      return null;
    }
    chunks.push(chunk);
  }
  try { return JSON.parse(Buffer.concat(chunks).toString('utf8')); }
  catch { sendJson(response, { error: 'invalid JSON' }, 400); return null; }
}

const state = await initialize();
let publishing = false;
const server = http.createServer(async (request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (process.env.MMDPACK_BROWSER_DEBUG === '1') console.log(`${request.method} ${url.pathname}`);
    if (request.method === 'GET' && url.pathname === '/') {
      response.writeHead(200, headers('text/html; charset=utf-8'));
      return response.end(await readFile(join(webRoot, 'index.html')));
    }
    if (request.method === 'GET' && url.pathname === '/webgpu') {
      response.writeHead(200, headers('text/html; charset=utf-8'));
      return response.end(await readFile(join(webRoot, 'index-webgpu.html')));
    }
    if (request.method === 'GET' && url.pathname === '/zen') {
      response.writeHead(200, headers('text/html; charset=utf-8'));
      return response.end(await readFile(join(webRoot, 'index-zen.html')));
    }
    if (request.method === 'GET' && url.pathname === '/firefox') {
      response.writeHead(200, headers('text/html; charset=utf-8'));
      return response.end(await readFile(join(webRoot, 'index-firefox.html')));
    }
    if (request.method === 'GET' && url.pathname === '/firefox-webgpu') {
      response.writeHead(200, headers('text/html; charset=utf-8'));
      return response.end(await readFile(join(webRoot, 'index-firefox-webgpu.html')));
    }
    if (request.method === 'GET' && url.pathname === '/probe.js') {
      response.writeHead(200, headers('text/javascript; charset=utf-8'));
      return response.end(await readFile(join(webRoot, 'probe.js')));
    }
    if (request.method === 'GET' && url.pathname === '/probe-webgpu.js') {
      response.writeHead(200, headers('text/javascript; charset=utf-8'));
      return response.end(await readFile(join(webRoot, 'probe-webgpu.js')));
    }
    if (request.method === 'GET' && url.pathname === '/api/config') return sendJson(response, {
      run_id: state.latest.run_id,
      execution_surface: webgpuExecutionSurface,
      webgl2_execution_surface: zenExecutionSurface,
      firefox_execution_surface: firefoxExecutionSurface,
      cases: [...state.entries.values()].map(item => ({ id: item.id, expected_sha256: item.sha256, size: item.bytes })),
      provenance: state.provenance,
    });
    if (request.method === 'GET' && url.pathname === '/api/key') return sendJson(response, { key: state.key.toString('base64') });
    const entryMatch = url.pathname.match(/^\/api\/(plain|encrypted)\/([A-Za-z0-9][A-Za-z0-9._-]*)$/);
    if (request.method === 'GET' && entryMatch && state.entries.has(entryMatch[2])) {
      const entry = state.entries.get(entryMatch[2]);
      if (entryMatch[1] === 'plain') {
        response.writeHead(200, headers());
        return response.end(entry.plaintext);
      }
      response.writeHead(200, { ...headers(), 'x-mmdpack-nonce': entry.nonce.toString('base64'), 'x-mmdpack-aad': entry.aad.toString('base64') });
      return response.end(entry.encrypted);
    }
    if (request.method === 'GET' && state.served.has(url.pathname)) {
      response.writeHead(200, headers(mime(url.pathname)));
      return response.end(state.served.get(url.pathname));
    }
    if (request.method === 'POST' && (url.pathname === '/api/report' || url.pathname === '/api/webgpu/report' || url.pathname === '/api/zen/webgl2/report' || url.pathname === '/api/firefox/webgl2/report' || url.pathname === '/api/firefox/webgpu/report')) {
      const variant = url.pathname === '/api/webgpu/report' ? 'webgpu'
        : url.pathname === '/api/zen/webgl2/report' ? 'zen-webgl2'
          : url.pathname === '/api/firefox/webgl2/report' ? 'firefox-webgl2'
            : url.pathname === '/api/firefox/webgpu/report' ? 'firefox-webgpu' : 'webgl2';
      if (variant === 'webgpu' && webgpuExecutionSurface !== 'external_chrome_extension') return sendJson(response, { error: 'WebGPU publication requires external Chrome authority' }, 403);
      if (variant === 'zen-webgl2' && zenExecutionSurface !== 'zen_browser') return sendJson(response, { error: 'Zen publication requires explicit Zen diagnostic authority' }, 403);
      if (variant === 'firefox-webgl2' && firefoxExecutionSurface !== 'official_firefox') return sendJson(response, { error: 'Firefox publication requires explicit official Firefox authority' }, 403);
      if (variant === 'firefox-webgpu' && firefoxExecutionSurface !== 'official_firefox') return sendJson(response, { error: 'Firefox WebGPU publication requires explicit official Firefox authority' }, 403);
      if (publishing) return sendJson(response, { error: 'publication already in progress' }, 409);
      publishing = true;
      try {
        const candidate = await collectJson(request, response);
        if (candidate === null) return;
        let canonical;
        try {
          canonical = variant === 'webgpu'
            ? validateWebGPUReport(candidate, { runId: state.latest.run_id, provenance: state.provenance, cases: state.caseMap, executionSurface: webgpuExecutionSurface })
            : variant === 'zen-webgl2'
              ? validateZenWebGL2Report(candidate, { runId: state.latest.run_id, provenance: state.provenance, cases: state.caseMap, executionSurface: zenExecutionSurface })
              : variant === 'firefox-webgl2'
                ? validateFirefoxWebGL2Report(candidate, { runId: state.latest.run_id, provenance: state.provenance, cases: state.caseMap, executionSurface: firefoxExecutionSurface })
                : variant === 'firefox-webgpu'
                  ? validateFirefoxWebGPUReport(candidate, { runId: state.latest.run_id, provenance: state.provenance, cases: state.caseMap, executionSurface: firefoxExecutionSurface })
              : validateReport(candidate, { runId: state.latest.run_id, provenance: state.provenance, cases: state.caseMap });
          assertNoSecrets(canonical, state);
        } catch (error) {
          return sendJson(response, { error: error.message }, 400);
        }
        const current = await loadSources();
        if (JSON.stringify(current.provenance) !== JSON.stringify(state.provenance)) throw Error('source/tool/harness drift before publication');
        await publishReport(canonical, {
          outputRoot: variant === 'webgpu' ? join(outputRoot, 'webgpu') : variant === 'zen-webgl2' ? join(outputRoot, 'zen-webgl2') : variant === 'firefox-webgl2' ? join(outputRoot, 'firefox-webgl2') : variant === 'firefox-webgpu' ? join(outputRoot, 'firefox-webgpu') : outputRoot,
          documentPath: variant === 'webgpu' ? webgpuDocumentPath : variant === 'zen-webgl2' ? zenDocumentPath : variant === 'firefox-webgl2' ? firefoxDocumentPath : variant === 'firefox-webgpu' ? firefoxWebgpuDocumentPath : documentPath,
          variant,
        });
        return sendJson(response, { ok: true });
      } finally {
        publishing = false;
      }
    }
    return sendJson(response, { error: 'not found' }, 404);
  } catch (error) {
    return sendJson(response, { error: String(error) }, 500);
  }
});

server.listen(0, '127.0.0.1', () => console.log(`http://127.0.0.1:${server.address().port}/`));
