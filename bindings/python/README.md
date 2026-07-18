# Experimental Python binding

This directory contains the in-repository, dependency-free `ctypes` bridge for
the experimental mmd-anim C ABI v2.  The implementation deliberately lives in
the internal module `mmd_anim._runtime`; it is not yet a stable Python API.

Run the tests from the repository root:

```powershell
python -m unittest discover -s bindings/python/tests -v
```

The native lifecycle and parser/geometry/IK smokes use `MMD_RUNTIME_LIBRARY`
when set; otherwise they look for the platform cdylib under `target/release`.
They create a one-bone model and empty owned clip, evaluate a nonzero clip
frame, copy the world matrix, parse tracked PMX/VMD fixtures, read an owned
geometry buffer, solve an IK primitive, and free all handles. Without either
library, native tests are skipped; pure ABI/ownership and header-drift tests
still run.

The shared version 1 model-descriptor ABI manifest is
`crates/mmd-anim-ffi/abi/model_descriptor_v1.json`. It fixes the C/Rust/ctypes
record names, field order/types, `sizeof`, `alignof`, and every field offset on
64-bit Windows and Ubuntu, as well as ABI/version/feature/flag constants and
the `mmd_runtime_model_create_from_descriptor` prototype. The ctypes bridge
loads this manifest directly; Rust layout tests and the drift checker validate
the other two surfaces. Run both checks after changing `mmd_runtime.h`:

```powershell
python tools/check_python_abi_drift.py
python tools/check_model_descriptor_abi.py
```

`test_model_descriptor_abi.py` loads the release cdylib with raw ctypes when
available and checks the version/feature guard, create/instance/rest-evaluate,
world and skinning copies, deterministic fresh creation, and indexed
thread-local errors for invalid descriptors.

The optional typed descriptor helper is kept internal (there is no package-root
re-export).  Build an immutable `ModelDefinition` from `Bone`, `IkSolver`,
`IkLink`, append, and morph records, then call
`RuntimeLibrary.create_model_from_descriptor`.  Bone list position is the
native index and `Bone.rest_position_pmx_xyz` is an absolute PMX-space rest
position.  The helper keeps all six contiguous ctypes arrays alive through the
constructor; the native model owns its copy after the call returns.  A
descriptor-created model intentionally has no bone/morph name map, so passing
it to `create_clip_from_vmd_bytes` preserves the explicit native name-resolution
error.

`mmd_anim._live_reload` adds the host-neutral `HostPose`,
`RuntimeHandleSet`, and `LiveRuntime` helpers for DCC integrations.  A
`LiveRuntime.reload(definition, current_pose, physics_definition)` builds a new
model and instance, applies the caller-owned current pose, and (when physics is
provided) always creates a fresh typed physics world and validates/reseeds it
with `SEED` before atomically swapping the handle set.  The old world, instance,
and model are then closed in that order.  Per-frame pose/morph/IK updates use
the host-pose API and do not rebuild descriptors.  `HostPose` local arrays are
pre-morph base values; native runtime expansion applies Group/Bone morph
deltas, so DCC hosts must not preapply those deltas.  Hosts must check
`MMD_RUNTIME_FEATURE_HOST_POSE_NATIVE_MORPHS` (bit 3) before relying on this
semantic correction; `LiveRuntime` rejects older ABI-v2 libraries that do not
advertise it.

The current pose remains host-owned and must be supplied by the DCC; no pose
getter or live in-place model mutation is provided.  Physics continuity,
mid-drag physics parameter updates, automatic name-map generation, and
package-root re-exports are intentionally out of scope.
