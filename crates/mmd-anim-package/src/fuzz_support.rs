//! Internal validator entry points for the nested cargo-fuzz workspace.
//!
//! This module is available only with the experimental `fuzzing` feature. It
//! deliberately exposes no package construction or authentication bypasses.

use crate::{
    MmdPackageEntry, MmdPackageError, MmdPackageLimits, MmdPackageManifest, decode_zstd,
    parse_manifest, validate_defaults, validate_layout,
};

/// Parses strict JSON and runs the complete low-level manifest validation.
pub fn validate_manifest_json(
    bytes: &[u8],
    limits: &MmdPackageLimits,
) -> std::result::Result<MmdPackageManifest, MmdPackageError> {
    if bytes.len() as u64 > limits.max_manifest_cipher_bytes {
        return Err(MmdPackageError::LimitExceeded {
            what: "fuzz manifest bytes",
            actual: bytes.len() as u64,
            limit: limits.max_manifest_cipher_bytes,
        });
    }
    let value = crate::strict_json::parse(bytes)
        .map_err(|error| MmdPackageError::InvalidJson(error.to_string()))?;
    let manifest = parse_manifest(&value, limits)?;
    validate_defaults(&value, &manifest)?;
    Ok(manifest)
}

/// Runs the checked contiguous entry-layout validator without authentication.
pub fn validate_entry_layout(
    entries: &[MmdPackageEntry],
    package_len: usize,
    payload_base: usize,
    limits: &MmdPackageLimits,
) -> std::result::Result<(), MmdPackageError> {
    validate_layout(entries, package_len, payload_base, limits)
}

/// Runs the bounded Draft Zstd profile decoder without authentication.
pub fn decode_zstd_profile(
    compressed: &[u8],
    decoded_size: u64,
    limits: &MmdPackageLimits,
) -> std::result::Result<Vec<u8>, MmdPackageError> {
    if decoded_size > limits.max_entry_decoded_bytes {
        return Err(MmdPackageError::LimitExceeded {
            what: "fuzz decoded bytes",
            actual: decoded_size,
            limit: limits.max_entry_decoded_bytes,
        });
    }
    decode_zstd(compressed, decoded_size)
}
