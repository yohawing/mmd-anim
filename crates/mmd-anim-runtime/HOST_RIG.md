# Retargeted host poses

Host rigs evaluate **Humanoid → MMD** poses while retaining host-driven bones
and evaluating PMX helper bones. Bone mapping, retargeting, rest-pose conversion,
units, handedness and engine root motion belong to the host.

## Per-frame flow

1. Retarget into fresh MMD-local base arrays. Translation is an offset from the
   model's parent-local rest position; quaternion order is XYZW. Supply every
   bone, morph and IK entry required by the instance. Unanimated helper input
   is zero translation, identity rotation and unit scale.
2. Evaluate the host rig. Inputs must not already include bone Morph, Append,
   fixed-axis projection or MMD IK. Group/bone morphs are expanded once.
3. Apply the returned model-space world matrices to the display skeleton once,
   or use the skinning matrices directly. Convert coordinates/units on the host.
   Do not run another host-side Append/IK pass on the result.

Keep retarget input separate from display output. Reconstruct each frame from
input, including helpers; recapturing the previous display pose can accumulate
Append. Playback, repeated frames and random seeks use the same stateless
evaluation entry point.

## Ownership

`HostRigDefinition` binds to a specific model allocation. Its driven list uses
MMD bone indices, not engine Humanoid slots. Driven bones retain their complete
model-space transform from pure FK after bone morphs. Incoming Append and fixed
axis projection do not alter those bones. A helper parent may move without
moving a driven child; descendants read that protected transform during the
normal evaluation, rather than receiving a final matrix patch.

For Append sourced from a driven bone, the authoritative motion is its input
parent-local delta after morphs, even when that bone declares incoming Append.
This remains distinct from the local transform reconstructed from final world
matrices for display. Non-driven sources retain existing PMX Append semantics.

IK is explicitly opt-in. At rig construction, list PMX IK **controller** bones
whose input transforms represent goals. A declared controller's chain may be
enabled or disabled in `HostPoseView.ik_enabled` each frame. Enabling undeclared
chains is an error. Declaration is rejected if any link writes a driven bone
or its ancestor; diagnostics identify the chain and protected bone. If multiple
chains share a controller, all must satisfy this rule. No FK foot goals are
inferred. For ordinary Humanoid FK playback, pass an empty goal list and zero
IK flags, retaining the Animator/retargeter's leg pose.

Local scales must be positive and uniform per bone. Keep engine object/world
scale outside these model-space arrays. The pose API supports Physics Off and
evaluates both before- and after-physics bone phases. The native Live physics
frame API keeps the same HostRig ownership through physics writeback; dynamic
bodies remain solver-owned, while writes to declared driven bones are
suppressed. Foot locking, inferred goals and FK/IK blending are separate work.

## Rust, C and JavaScript

Rust: construct `HostRigDefinition`, then call
`RuntimeInstance::evaluate_host_rig_pose`. See the runnable example:

```sh
cargo run -p mmd-anim-runtime --example host_rig
```

C: check `MMD_RUNTIME_FEATURE_HOST_RIG`, then use
`mmd_runtime_host_rig_create`, `mmd_runtime_instance_evaluate_host_rig_pose` and
`mmd_runtime_host_rig_free`. The input is the existing
`mmd_runtime_ffi_host_pose_view_t`; ABI 3 and existing APIs are unchanged.
Use existing instance copy functions for outputs. A null creation result or
non-OK evaluation status has details in `mmd_runtime_last_error_message()`.
The header documents array validity, ownership and exclusive handle access.
For native Bullet Live/Trace evaluation, also check
`MMD_RUNTIME_FEATURE_HOST_RIG_PHYSICS` and call
`mmd_runtime_evaluate_host_rig_frame`. Its tolerance and iteration cap apply
to before-physics; the existing bridge after-physics defaults remain in force.
Use `Seed` for a reset-style frame and `Step` for a fixed-clock advance.

JavaScript uses `WasmMmdHostRig` from the generated WASM module:

```js
const runtime = wasm.WasmMmdRuntimeInstance.forModel(model);
const rig = new wasm.WasmMmdHostRig(model, drivenBoneIndices, goalBoneIndices);
const output = new Float32Array(runtime.worldMatrixF32Len());
// Reuse base Float32Arrays and Uint8Array IK flags, populated by the host.
rig.evaluate(runtime, positions, rotations, scales, morphs, ikFlags, 1e-4, 0);
runtime.copyWorldMatrices(output);
// Apply output once; repeat evaluation from a fresh base pose next frame.
rig.free();
runtime.free();
```

IK cap `0` means authored iterations. Model storage is retained by rig and
instance; the model handle may be freed after creating them. Native and WASM
rigs reuse conversion buffers but must not be evaluated concurrently through
the same mutable handle. Invalid inputs leave instance outputs unchanged.
Existing runtime Append allocations and wasm-bindgen boundary copies remain;
this API does not claim zero allocations for the complete frame.

Unity integration should replace all-IK-enabled capture with an explicitly
configured rig and fresh base inputs, then apply native outputs once.
Three.js integration should pass retarget output to this WASM entry point
instead of requiring a VMD clip, then convert/apply matrices once.
Consumer integrations are not included in this change.

## Verification and physics handoff

Core tests cover parent helpers, protected incoming Append, downstream Append,
after-physics bones, group/bone morphs, explicit IK, rejection/recovery, 300
identical frames and seek order. Native ABI and the actual WASM harness use the
same four-bone arithmetic oracle, with native outputs additionally compared to
the Rust core. The WASM test runs under Node with a web-target binary; it is not
a Unity or Three.js GUI acceptance test.

```sh
cargo test -p mmd-anim-runtime --test host_rig
cargo test -p mmd-anim-ffi host_rig
cd crates/mmd-anim-wasm/harness
npm run smoke
```

The native Live route keeps the same input snapshot and ownership through
`base/morph → before-physics → physics writeback → after-physics`. Dynamic
bodies attached to protected driven bones stay in Bullet and may move
independently; their runtime writeback is suppressed for the protected bone.
Do not call the legacy physics evaluation after the full HostRig frame call: it
does not retain an active rig context for a later physics phase.
