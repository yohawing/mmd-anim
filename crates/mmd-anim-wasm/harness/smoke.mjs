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

let passed = 0;
let failed = 0;

function assert(cond, msg) {
  if (cond) {
    console.log(`  PASS: ${msg}`);
    passed++;
  } else {
    console.error(`  FAIL: ${msg}`);
    failed++;
  }
}

function assertClose(actual, expected, tol, msg) {
  assert(Math.abs(actual - expected) < tol, `${msg} (got ${actual}, expected ${expected})`);
}

console.log('=== Wasm Smoke Test ===\n');

console.log('1. wasm_wrapper_version');
const version = wasm.wasm_wrapper_version();
assert(typeof version === 'number' && version > 0, `wrapper version = ${version}`);

console.log('\n2. Construct WasmMmdModel (2 bones)');
const parentIndices = new Int32Array([-1, 0]);
const restPositions = new Float32Array([
  1.0, 0.0, 0.0,
  0.0, 2.0, 0.0,
]);
const model = new wasm.WasmMmdModel(parentIndices, restPositions);
assert(model.boneCount() === 2, 'boneCount() === 2');

console.log('\n3. Create RuntimeInstance and evaluate rest pose');
const runtime = new wasm.WasmMmdRuntimeInstance(model, 0);
runtime.evaluateRestPose();

const len = runtime.worldMatrixF32Len();
assert(len === 32, `worldMatrixF32Len() === 32 (got ${len})`);

const matrices = runtime.worldMatrices();
assert(matrices.length === 32, 'worldMatrices() returns f32[32]');

console.log('\n4. worldMatricesView() direct view');
const view = runtime.worldMatricesView();
assert(view instanceof Float32Array, 'worldMatricesView returns Float32Array');
assert(view.length === 32, `worldMatricesView length = ${view.length}`);
assertClose(view[12], 1.0, 1e-6, 'view bone 0 world x = 1.0');
assertClose(view[28], 1.0, 1e-6, 'view bone 1 world x = 1.0');
assertClose(view[29], 2.0, 1e-6, 'view bone 1 world y = 2.0');

console.log('\n5. copyWorldMatrices into caller-owned buffer');
const buf = new Float32Array(len);
const ok = runtime.copyWorldMatrices(buf);
assert(ok === true, 'copyWorldMatrices returns true');

assertClose(buf[12], 1.0, 1e-6, 'bone 0 world x = 1.0');
assertClose(buf[28], 1.0, 1e-6, 'bone 1 world x = 1.0');
assertClose(buf[29], 2.0, 1e-6, 'bone 1 world y = 2.0');

console.log('\n6. copyWorldMatrices rejects short buffer');
const shortBuf = new Float32Array(15);
assert(runtime.copyWorldMatrices(shortBuf) === false, 'copy returns false on short buffer');

console.log('\n6b. RuntimeInstance.forModel');
const autoRuntime = wasm.WasmMmdRuntimeInstance.forModel(model);
assert(autoRuntime.worldMatrixF32Len() === 32, 'forModel worldMatrixF32Len() === 32');
assert(autoRuntime.morphWeightLen() === 0, 'forModel morphWeightLen() === 0');
assert(autoRuntime.ikEnabledLen() === 0, 'forModel ikEnabledLen() === 0');

console.log('\n7. Create WasmMmdClip and evaluate frame');
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
assert(clip.hasFrames(), 'clip hasFrames() returns true');
assert(clip.firstFrame() === 0, 'clip firstFrame() === 0');
assert(clip.lastFrame() === 60, 'clip lastFrame() === 60');

runtime.evaluateClipFrame(clip, 30.0);
runtime.copyWorldMatrices(buf);
assertClose(buf[12], 2.0, 1e-6, 'bone 0 world x after clip frame 30 = 2.0');
assertClose(buf[28], 2.0, 1e-6, 'bone 1 world x after clip frame 30 = 2.0');
assertClose(buf[29], 2.0, 1e-6, 'bone 1 world y after clip frame 30 = 2.0');

console.log('\n8. skinningMatricesView()');
const skinView = runtime.skinningMatricesView();
assert(skinView instanceof Float32Array, 'skinningMatricesView returns Float32Array');
assert(skinView.length === 32, `skinningMatricesView length = ${skinView.length}`);

console.log('\n9. copySkinningMatrices');
const skinLen = runtime.skinningMatrixF32Len();
assert(skinLen === 32, `skinningMatrixF32Len() === 32 (got ${skinLen})`);
const skinBuf = new Float32Array(skinLen);
assert(runtime.copySkinningMatrices(skinBuf) === true, 'copySkinningMatrices returns true');

console.log('\n10. morph and IK accessors and views');
const model2 = new wasm.WasmMmdModel(new Int32Array([-1]), new Float32Array([0, 0, 0]));
const runtime2 = new wasm.WasmMmdRuntimeInstance(model2, 3);
const morphView = runtime2.morphWeightsView();
assert(morphView instanceof Float32Array, 'morphWeightsView returns Float32Array');
assert(morphView.length === 3, 'morphWeightsView.length === 3');

assert(runtime2.morphWeightLen() === 3, 'morphWeightLen === 3');
assert(runtime2.morphWeights().length === 3, 'morphWeights().length === 3');
const morphBuf = new Float32Array(3);
assert(runtime2.copyMorphWeights(morphBuf) === true, 'copyMorphWeights returns true');
assert(runtime2.ikEnabledLen() === 0, 'ikEnabledLen === 0');
const ikView = runtime2.ikEnabledView();
assert(ikView instanceof Uint8Array, 'ikEnabledView returns Uint8Array');
assert(ikView.length === 0, 'ikEnabledView.length === 0');

console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
if (failed > 0) process.exit(1);
