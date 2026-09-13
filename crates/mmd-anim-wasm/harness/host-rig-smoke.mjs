import assert from 'node:assert/strict';

// Same four-bone fixture and analytic oracle as host_rig_tests.rs (native ABI).
export function runHostRigSmoke(wasm) {
  const model = wasm.WasmMmdModel.withAppend(
    new Int32Array([-1, 0, 1, -1]),
    new Float32Array([0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0]),
    new Uint32Array([1, 0, 1, 2, 1, 1, 3, 2, 1]),
    new Float32Array([0.5, 0.7, 1]),
  );
  const runtime = wasm.WasmMmdRuntimeInstance.forModel(model);
  const rig = new wasm.WasmMmdHostRig(model, new Uint32Array([0, 2]), new Uint32Array());
  assert.throws(() => new wasm.WasmMmdHostRig(model, new Uint32Array([99]), new Uint32Array()));
  assert.throws(() => new wasm.WasmMmdHostRig(model, new Uint32Array([0, 0]), new Uint32Array()));
  assert.throws(() => new wasm.WasmMmdHostRig(model, new Uint32Array(), new Uint32Array([0])));
  const otherModel = new wasm.WasmMmdModel(new Int32Array([-1]), new Float32Array(3));
  const otherRuntime = wasm.WasmMmdRuntimeInstance.forModel(otherModel);
  model.free(); // Model storage is retained independently by rig and runtime.
  const positions = new Float32Array(12);
  const rotations = new Float32Array(16);
  const scales = new Float32Array(12).fill(1);
  const morphs = new Float32Array();
  const ik = new Uint8Array();
  const out = new Float32Array(64);
  for (let i = 0; i < 4; i++) rotations[i * 4 + 3] = 1;
  rotations[2] = Math.sin(0.2); rotations[3] = Math.cos(0.2);
  rotations[10] = Math.sin(0.1); rotations[11] = Math.cos(0.1);
  const evaluate = (q = rotations) => rig.evaluate(runtime, positions, q, scales, morphs, ik, 1e-4, 0);
  const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-5, `${a} != ${b}`);
  try {
    evaluate();
    assert.equal(runtime.copyWorldMatrices(out), true);
    near(out[44], -2 * Math.sin(0.4));
    near(out[45], 2 * Math.cos(0.4));
    near(out[32], Math.cos(0.6));
    near(out[48], Math.cos(0.2));
    const first = out.slice();
    for (let i = 0; i < 300; i++) {
      evaluate();
      runtime.copyWorldMatrices(out);
      assert.deepEqual(out, first);
    }
    assert.throws(() => evaluate(new Float32Array(15)));
    const bad = rotations.slice(); bad[0] = NaN;
    assert.throws(() => evaluate(bad));
    assert.throws(() => rig.evaluate(otherRuntime, positions, rotations, scales, morphs, ik, 1e-4, 0));
    runtime.copyWorldMatrices(out);
    assert.deepEqual(out, first, 'invalid input preserves output cache');
    evaluate(); // Recover after rejection.
    runtime.copyWorldMatrices(out);
    assert.deepEqual(out, first);
  } finally {
    rig.free(); runtime.free(); otherRuntime.free(); otherModel.free();
  }

  runExternalPhysicsSmoke(wasm);
}

function runExternalPhysicsSmoke(wasm) {
  const model = new wasm.WasmMmdModel(
    new Int32Array([-1, 0]),
    new Float32Array([0, 0, 0, 0, 1, 0]),
  );
  const runtime = wasm.WasmMmdRuntimeInstance.forModel(model);
  const rig = new wasm.WasmMmdHostRig(model, new Uint32Array([0]), new Uint32Array());
  const positions = new Float32Array([2, 0, 0, 0, 0, 0]);
  const rotations = new Float32Array([0, 0, 0, 1, 0, 0, 0, 1]);
  const scales = new Float32Array(6).fill(1);
  const morphs = new Float32Array();
  const ik = new Uint8Array();
  const out = new Float32Array(32);
  let calls = 0;
  const apply = () => rig.evaluateWithExternalPhysics(
    runtime, positions, rotations, scales, morphs, ik, 1e-4, 0,
    (before, physics, mask) => {
      calls++;
      assert.ok(before instanceof Float32Array);
      assert.ok(physics instanceof Float32Array);
      assert.ok(mask instanceof Uint8Array);
      assert.notEqual(before.buffer, physics.buffer);
      assert.deepEqual(physics, before);
      physics[12] = 99;
      physics[16 + 13] -= 0.25;
      mask.set([1, 1]);
      return true;
    },
  );
  try {
    apply();
    assert.equal(calls, 1);
    assert.equal(runtime.copyWorldMatrices(out), true);
    assert.ok(Math.abs(out[12] - 2) < 1e-5, `driven root moved: ${out[12]}`);
    assert.ok(Math.abs(out[16 + 12] - 2) < 1e-5, `child x changed: ${out[28]}`);
    assert.ok(Math.abs(out[16 + 13] - 0.75) < 1e-5, `child y not written: ${out[29]}`);

    assert.throws(() => rig.evaluateWithExternalPhysics(
      runtime, positions, rotations, scales, morphs, ik, 1e-4, 0,
      () => { throw new Error('external solver failed'); },
    ));
    assert.equal(runtime.copyWorldMatrices(out), true);
    assert.ok(Math.abs(out[12] - 2) < 1e-5, 'callback failure refreshes before-pose cache');
    assert.ok(Math.abs(out[16 + 13] - 1) < 1e-5, 'callback failure skips writeback');
    assert.throws(() => rig.evaluateWithExternalPhysics(
      runtime, positions, rotations, scales, morphs, ik, 1e-4, 0,
      async () => true,
    ));
    assert.throws(() => rig.evaluateWithExternalPhysics(
      runtime, positions, rotations, scales, morphs, ik, 1e-4, 0,
      () => false,
    ));
    assert.throws(() => rig.evaluateWithExternalPhysics(
      runtime, positions, rotations, scales, morphs, ik, 1e-4, 0,
      (_before, physics, mask) => {
        physics[16] = NaN;
        mask[1] = 1;
        return true;
      },
    ));
    apply();
    assert.equal(calls, 2, 'guard recovers after callback rejection');
    for (const outputIndex of [1, 2]) {
      assert.throws(() => rig.evaluateWithExternalPhysics(
        runtime, positions, rotations, scales, morphs, ik, 1e-4, 0,
        (...buffers) => {
          const buffer = buffers[outputIndex].buffer;
          structuredClone(buffer, { transfer: [buffer] });
          return true;
        },
      ), /detached an output buffer/);
      apply();
      assert.equal(runtime.copyWorldMatrices(out), true);
      assert.ok(Math.abs(out[29] - 0.75) < 1e-5, 'guard recovers after buffer transfer');
    }
  } finally {
    rig.free(); runtime.free(); model.free();
  }
}
