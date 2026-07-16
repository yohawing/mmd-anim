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

The ctypes signature manifest is `mmd_anim/_abi.py`. After changing
`mmd_runtime.h`, update that manifest for every wrapped declaration, then run
`python tools/check_python_abi_drift.py`.
