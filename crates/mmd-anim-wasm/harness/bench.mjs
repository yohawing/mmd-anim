import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(__dirname, 'pkg');

if (!existsSync(pkgDir)) {
  console.error('ERROR: wasm package not found. Run build.mjs first or use `npm run build`.');
  process.exit(1);
}

const wasmModulePath = path.join(pkgDir, 'mmd_anim_wasm.js');
const wasmBinaryPath = path.join(pkgDir, 'mmd_anim_wasm_bg.wasm');
if (!existsSync(wasmBinaryPath)) {
  console.error('ERROR: wasm binary not found. Run build.mjs first or use `npm run build`.');
  process.exit(1);
}

const wasm = await import(`${pathToFileURL(wasmModulePath).href}?cacheBust=${Date.now()}`);
const wasmBytes = readFileSync(wasmBinaryPath);
await wasm.default({ module_or_path: wasmBytes });

const BONE_COUNT = 10;
const ITERATIONS = 10000;

console.log('=== Wasm Benchmark ===');
console.log(`Bones: ${BONE_COUNT}, Iterations: ${ITERATIONS}\n`);

const parentIndices = new Int32Array(BONE_COUNT);
const restPositions = new Float32Array(BONE_COUNT * 3);
for (let i = 0; i < BONE_COUNT; i++) {
  parentIndices[i] = i === 0 ? -1 : i - 1;
  restPositions[i * 3] = i * 1.0;
  restPositions[i * 3 + 1] = 0.0;
  restPositions[i * 3 + 2] = 0.0;
}

const model = new wasm.WasmMmdModel(parentIndices, restPositions);

const clip = new wasm.WasmMmdClip(
  new Uint32Array([0, 0, 2]),
  new Uint32Array([0, 60]),
  new Float32Array([
    0.0, 0.0, 0.0,  0.0, 0.0, 0.0, 1.0,
    2.0, 0.0, 0.0,  0.0, 0.0, 0.0, 1.0,
  ]),
  new Uint32Array([]),
  new Uint32Array([]),
  new Float32Array([]),
  new Uint32Array([]),
  new Uint8Array([]),
  0,
);

// ---------- copy path: evaluateClipFrame + copyWorldMatrices ----------

const copyRuntime = new wasm.WasmMmdRuntimeInstance(model, 0);
const len = copyRuntime.worldMatrixF32Len();
const copyBuf = new Float32Array(len);

console.log('Warming up copy path...');
for (let i = 0; i < 100; i++) {
  copyRuntime.evaluateClipFrame(clip, 30.0);
  copyRuntime.copyWorldMatrices(copyBuf);
}

console.log('Running copy path benchmark...');
const copyStart = performance.now();
let copyChecksum = 0;
for (let i = 0; i < ITERATIONS; i++) {
  copyRuntime.evaluateClipFrame(clip, 30.0);
  copyRuntime.copyWorldMatrices(copyBuf);
  for (let j = 0; j < len; j++) {
    copyChecksum += copyBuf[j] | 0;
  }
}
const copyElapsed = performance.now() - copyStart;
const copyPerFrame = copyElapsed / ITERATIONS;
const copyPerFrameUs = copyPerFrame * 1000;
const copyThroughput = ITERATIONS / (copyElapsed / 1000);

console.log(`\n--- Copy path (evaluateClipFrame + copyWorldMatrices) ---`);
console.log(`  Elapsed:    ${copyElapsed.toFixed(2)} ms`);
console.log(`  Per frame:  ${copyPerFrame.toFixed(4)} ms  (${copyPerFrameUs.toFixed(2)} µs)`);
console.log(`  Throughput: ${copyThroughput.toFixed(0)} frames/sec`);
console.log(`  Checksum:   ${copyChecksum}`);

// ---------- view path: evaluateClipFrame + worldMatricesView ----------

const viewRuntime = new wasm.WasmMmdRuntimeInstance(model, 0);

console.log('\nWarming up view path...');
for (let i = 0; i < 100; i++) {
  viewRuntime.evaluateClipFrame(clip, 30.0);
  const v = viewRuntime.worldMatricesView();
  for (let j = 0; j < len; j++) {
    v[j];
  }
}

console.log('Running view path benchmark...');
const viewStart = performance.now();
let viewChecksum = 0;
for (let i = 0; i < ITERATIONS; i++) {
  viewRuntime.evaluateClipFrame(clip, 30.0);
  const v = viewRuntime.worldMatricesView();
  for (let j = 0; j < len; j++) {
    viewChecksum += v[j] | 0;
  }
}
const viewElapsed = performance.now() - viewStart;
const viewPerFrame = viewElapsed / ITERATIONS;
const viewPerFrameUs = viewPerFrame * 1000;
const viewThroughput = ITERATIONS / (viewElapsed / 1000);

console.log(`\n--- View path (evaluateClipFrame + worldMatricesView) ---`);
console.log(`  Elapsed:    ${viewElapsed.toFixed(2)} ms`);
console.log(`  Per frame:  ${viewPerFrame.toFixed(4)} ms  (${viewPerFrameUs.toFixed(2)} µs)`);
console.log(`  Throughput: ${viewThroughput.toFixed(0)} frames/sec`);
console.log(`  Checksum:   ${viewChecksum}`);

// ---------- summary ----------

const ratio = (copyThroughput / viewThroughput);
console.log(`\n--- Summary ---`);
console.log(`  Copy: ${copyPerFrame.toFixed(4)} ms/frame  (${copyThroughput.toFixed(0)} fps)`);
console.log(`  View: ${viewPerFrame.toFixed(4)} ms/frame  (${viewThroughput.toFixed(0)} fps)`);
console.log(`  Speed ratio (copy / view): ${ratio.toFixed(2)}x`);
console.log(`  Checksum match:            ${copyChecksum === viewChecksum ? 'YES' : 'NO (data mismatch)'}`);
