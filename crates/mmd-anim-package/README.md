# mmd-anim-package

Experimental Rust package core for the draft `.mmdpack` container.

This crate is currently workspace-private and is not published to crates.io
while the Draft 0.2 format evolves.

This crate provides bounded packing of already codec-encoded payloads, fixed
header validation, authenticated manifest parsing, entry layout checks, and
bounded per-entry decrypt/decompress. Manifest and KTX2 UASTC payload checks
are fail-closed where the current draft defines them.

The wire format is Draft 0.2 and is not a stable MMDPACK V1 contract. Packing
accepts codec-ready payloads; PNG/JPEG decoding, mip generation, UASTC/KTX2
encoding, WASM/FFI bindings, and high-level PMX/VMD loading are outside this
crate's current scope.
