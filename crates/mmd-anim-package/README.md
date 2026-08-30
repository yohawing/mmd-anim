# mmd-anim-package

Experimental, bounded native reader for the draft `.mmdpack` container.

This crate deliberately implements only the low-level read boundary: fixed-header
validation, authenticated manifest parsing, entry layout checks, and bounded
per-entry decrypt/decompress. It is not a stable V1 format promise and does not
yet expose packing, WASM, FFI, or high-level PMX/VMD loading.
