# Experimental Python binding

This directory contains a dependency-free `ctypes` bridge to the experimental
mmd-anim C ABI v2. It is an internal integration layer, not a stable public API
or a PyPI package.

The implementation, tests, C header, and ABI manifest are the source of truth.
Read those files for exact behavior; this README is only a starting point.

## Entry points

- [`mmd_anim/_runtime.py`](mmd_anim/_runtime.py): low-level C ABI wrapper
- [`mmd_anim/_model_descriptor.py`](mmd_anim/_model_descriptor.py): typed model definitions
- [`mmd_anim/_live_reload.py`](mmd_anim/_live_reload.py): managed DCC live reload

Prefer `LiveRuntime` when possible. If you use the low-level API directly, you
must keep related handles in the same generation, serialize access, and keep
them alive while native calls are running. A physics failure requires a
successful reload before evaluation or readback can continue.

`HostPose` values are pre-morph. Hosts must not apply Group or Bone morphs
before passing them to native code, and must require
`MMD_RUNTIME_FEATURE_HOST_POSE_NATIVE_MORPHS` (feature bit 3).

## Tests and ABI checks

From the repository root:

```powershell
python -m unittest discover -s bindings/python/tests -v
python tools/check_python_abi_drift.py
python tools/check_model_descriptor_abi.py
```

Native tests use `MMD_RUNTIME_LIBRARY` or the release library under
`target/release`; native-only tests are skipped when neither is available.

For details, read [`tests/`](tests/),
[`mmd_runtime.h`](../../crates/mmd-anim-ffi/include/mmd_runtime.h), and the
[`model descriptor ABI manifest`](../../crates/mmd-anim-ffi/abi/model_descriptor_v1.json).
