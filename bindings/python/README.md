# Experimental Python binding

This directory contains the in-repository, dependency-free `ctypes` bridge for
the experimental mmd-anim C ABI v2.  The implementation deliberately lives in
the internal module `mmd_anim._runtime`; it is not yet a stable Python API.

Run the tests from the repository root:

```powershell
python -m unittest discover -s bindings/python/tests -v
```

The native lifecycle smoke uses `MMD_RUNTIME_LIBRARY` when set, otherwise it
looks for the platform cdylib under `target/release`.  It creates a one-bone
model and empty owned clip, evaluates a nonzero clip frame, copies the world
matrix, and frees clip/instance/model handles.  Without either library, only
that native test is skipped; pure ABI/ownership tests still run.
