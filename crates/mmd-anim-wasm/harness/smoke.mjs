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

function buildMinimalVmdBytes() {
  const magicText = 'Vocaloid Motion Data 0002';
  const bytes = [];
  for (let i = 0; i < magicText.length; i++) bytes.push(magicText.charCodeAt(i));
  while (bytes.length < 30) bytes.push(0);
  const modelName = 'smoke';
  for (let i = 0; i < modelName.length; i++) bytes.push(modelName.charCodeAt(i));
  while (bytes.length < 50) bytes.push(0);
  for (let section = 0; section < 6; section++) {
    bytes.push(0, 0, 0, 0);
  }
  return new Uint8Array(bytes);
}

function buildFixedAxisIkPmxBytes() {
  const bytes = [];
  const pushU8 = (value) => bytes.push(value & 0xff);
  const pushI16 = (value) => {
    const view = new DataView(new ArrayBuffer(2));
    view.setInt16(0, value, true);
    bytes.push(...new Uint8Array(view.buffer));
  };
  const pushI32 = (value) => {
    const view = new DataView(new ArrayBuffer(4));
    view.setInt32(0, value, true);
    bytes.push(...new Uint8Array(view.buffer));
  };
  const pushF32 = (value) => {
    const view = new DataView(new ArrayBuffer(4));
    view.setFloat32(0, value, true);
    bytes.push(...new Uint8Array(view.buffer));
  };
  const pushVec3 = (x, y, z) => { pushF32(x); pushF32(y); pushF32(z); };
  const pushText = (value) => {
    const text = new TextEncoder().encode(value);
    pushI32(text.length);
    bytes.push(...text);
  };
  const pushBone = (name, position, parent, flags, extra) => {
    pushText(name); pushText(''); pushVec3(...position); pushI16(parent); pushI32(0); pushI16(flags);
    extra();
  };

  bytes.push(...new TextEncoder().encode('PMX '));
  pushF32(2.0); pushU8(8);
  bytes.push(1, 0, 2, 2, 2, 2, 2, 2); // UTF-8, no extra UVs, i16 indices
  pushText(''); pushText(''); pushText(''); pushText('');
  for (let section = 0; section < 4; section++) pushI32(0);
  pushI32(3);
  pushBone('link', [0, 0, 0], -1, 0x0400, () => { pushVec3(0, 0, 0); pushVec3(0, 0, 1); });
  pushBone('tip', [0, 1, 0], 0, 0, () => pushVec3(0, 0, 0));
  pushBone('ik', [0, 0, 1], -1, 0x0001 | 0x0020, () => {
    pushI16(1); pushI16(1); pushI32(16); pushF32(Math.PI); pushI32(1); pushI16(0); pushU8(0);
  });
  pushI32(0); // morph count
  return new Uint8Array(bytes);
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

console.log('\n6c. Import fixedAxis IK PMX bytes');
const axisModel = wasm.WasmMmdModel.fromPmxBytes(buildFixedAxisIkPmxBytes());
assert(axisModel.boneCount() === 3, 'fromPmxBytes fixedAxis model has 3 bones');
assert(axisModel.ikCount() === 1, 'fromPmxBytes fixedAxis model has 1 IK solver');
const axisRuntime = wasm.WasmMmdRuntimeInstance.forModel(axisModel);
axisRuntime.evaluateRestPose();
const axisWorld = axisRuntime.worldMatrices();
assertClose(axisWorld[28], 0.0, 1e-3, 'fixedAxis IK tip x remains 0');
assertClose(axisWorld[29], 1.0, 1e-3, 'fixedAxis IK tip y remains 1');
assertClose(axisWorld[30], 0.0, 1e-3, 'fixedAxis IK tip z remains 0');

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

console.log('\n7b. evaluateClipFrameBatch');
assert(runtime.clipFrameBatchWorldMatrixF32Len(3) === 96, 'batch world matrix len for 3 frames === 96');
assert(runtime.clipFrameBatchMorphWeightF32Len(3) === 0, 'batch morph weight len for 3 frames === 0');
const batch = runtime.evaluateClipFrameBatch(clip, 0.0, 30.0, 3, 0);
assert(batch.frameCount() === 3, 'batch frameCount() === 3');
assert(batch.boneCount() === 2, 'batch boneCount() === 2');
assert(batch.morphCount() === 0, 'batch morphCount() === 0');
const batchWorld = batch.worldMatrices();
assert(batchWorld instanceof Float32Array, 'batch worldMatrices() returns Float32Array');
assert(batchWorld.length === 96, 'batch worldMatrices length === 96');
assertClose(batchWorld[12], 1.0, 1e-6, 'batch frame 0 bone 0 world x = 1.0');
assertClose(batchWorld[32 + 12], 2.0, 1e-6, 'batch frame 30 bone 0 world x = 2.0');
assertClose(batchWorld[64 + 12], 3.0, 1e-6, 'batch frame 60 bone 0 world x = 3.0');
const batchWorldView = batch.worldMatricesView();
assert(batchWorldView instanceof Float32Array, 'batch worldMatricesView returns Float32Array');
assert(batchWorldView.length === 96, 'batch worldMatricesView length === 96');
const batchCopy = new Float32Array(96);
assert(batch.copyWorldMatrices(batchCopy) === true, 'batch copyWorldMatrices returns true');
assertClose(batchCopy[64 + 12], 3.0, 1e-6, 'batch copy frame 60 bone 0 world x = 3.0');
assert(batch.copyWorldMatrices(new Float32Array(95)) === false, 'batch copyWorldMatrices rejects short buffer');
runtime.copyWorldMatrices(buf);
assertClose(buf[12], 2.0, 1e-6, 'batch evaluation does not mutate source runtime frame');

console.log('\n7c. reduced pose handle parity, lifetime, short buffer, and free');
const reductionArgs = [0, 1e-4, 1e-4, 1e-4, 1e-4, 1e-4];
const batchReduced = batch.reducePose(...reductionArgs);
const hostReduced = wasm.reduceDensePose(
  model,
  batchWorld,
  new Float32Array([]),
  3,
  0.0,
  30.0,
  ...reductionArgs,
);
assert(batchReduced.reducedBoneKeyCount() === 4, 'batch reduced pose keeps two keys per bone');
assert(hostReduced.reducedBoneKeyCount() === 4, 'host dense reduced pose keeps two keys per bone');
batch.free();
model.free();
const reducedWorldA = new Float32Array(32);
const reducedWorldB = new Float32Array(32);
assert(batchReduced.sample(30.0, reducedWorldA, new Float32Array([])) === true,
  'batch reduced result samples after batch/model free');
assert(hostReduced.sample(30.0, reducedWorldB, new Float32Array([])) === true,
  'host reduced result samples after model free');
assert(batchReduced.sample(30.0, new Float32Array(31), new Float32Array([])) === false,
  'reduced pose sample rejects short buffer');
assert(reducedWorldA.every((value, index) => value === reducedWorldB[index]),
  'batch and host dense reduced samplers have parity');
assertClose(reducedWorldA[12], 2.0, 1e-6, 'reduced pose frame 30 bone 0 world x = 2.0');
batchReduced.free();
hostReduced.free();

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

console.log('\n11. parseVmdAnimationJson dedicated parser API');
const minimalVmd = buildMinimalVmdBytes();
const vmdJson = JSON.parse(wasm.parseVmdAnimationJson(minimalVmd));
assert(vmdJson.kind === 'vmd', 'parseVmdAnimationJson returns VMD DTO kind');
assert(vmdJson.metadata.format === 'vmd', 'parseVmdAnimationJson metadata.format === vmd');
assert(vmdJson.boneFrames.length === 0, 'parseVmdAnimationJson bone frame count === 0');
assert(vmdJson.metadata.counts.bones === 0, 'parseVmdAnimationJson metadata.counts.bones === 0');

clip.free();
runtime.free();
runtime2.free();
model2.free();

console.log(`\n=== Results: ${passed} passed, ${failed} failed ===`);
if (failed > 0) process.exit(1);
