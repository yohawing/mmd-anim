# Experimental Python binding

This directory contains a dependency-free `ctypes` bridge to the experimental
mmd-anim C ABI v2. It is an internal integration layer, not a stable public API
or a PyPI package.

## Quick start

Build the native library and make this directory importable:

```powershell
cargo build -p mmd-anim-ffi --release
$env:PYTHONPATH = "bindings/python"
```

Load a PMX model and VMD motion, evaluate one frame, and read the resulting
world matrices:

```python
from pathlib import Path

from mmd_anim._runtime import RuntimeLibrary


runtime = RuntimeLibrary()

with runtime.create_model_from_pmx_bytes(Path("model.pmx").read_bytes()) as model:
    with runtime.create_clip_from_vmd_bytes(
        model, Path("motion.vmd").read_bytes()
    ) as clip:
        with model.create_instance_for_model() as instance:
            instance.evaluate_clip_frame(clip, 30.0)
            world_matrices = instance.world_matrices_f32()

print(f"evaluated {len(world_matrices) // 16} bones")
```

`RuntimeLibrary()` loads the release library from `target/release`. Set
`MMD_RUNTIME_LIBRARY` to use a library from another location.

## Tests

From the repository root:

```powershell
python -m unittest discover -s bindings/python/tests -v
```

## Advanced usage and ABI details

This README intentionally covers only the basic path. For exact behavior and
advanced DCC integration, read the code and tests:

- [`mmd_anim/_runtime.py`](mmd_anim/_runtime.py): low-level runtime API
- [`mmd_anim/_model_descriptor.py`](mmd_anim/_model_descriptor.py): model construction without PMX bytes
- [`mmd_anim/_live_reload.py`](mmd_anim/_live_reload.py): live model replacement
- [`tests/`](tests/): usage examples and behavioral contracts
- [`mmd_runtime.h`](../../crates/mmd-anim-ffi/include/mmd_runtime.h): C ABI
- [`model_descriptor_v1.json`](../../crates/mmd-anim-ffi/abi/model_descriptor_v1.json): ABI layout

After changing the C ABI, run:

```powershell
python tools/check_python_abi_drift.py
python tools/check_model_descriptor_abi.py
```
