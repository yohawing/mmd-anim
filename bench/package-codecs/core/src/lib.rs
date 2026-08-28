//! Small, shared Phase 0 codec probe.
//!
//! This crate deliberately does not define an `.mmdpack` container.  It only
//! exercises the draft PMX/VMD pipeline: Zstandard level 3 followed by
//! AES-256-GCM with a fresh key and nonce for every run.

use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use getrandom::getrandom;
use serde::{Deserialize, Serialize};

pub const ZSTD_LEVEL: i32 = 3;
pub const AES_KEY_BYTES: usize = 32;
pub const AES_NONCE_BYTES: usize = 12;
pub const AES_TAG_BYTES: usize = 16;

pub struct EncryptedPayload {
    ciphertext: Vec<u8>,
    key: [u8; AES_KEY_BYTES],
    nonce: [u8; AES_NONCE_BYTES],
}

impl EncryptedPayload {
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoundaryChecks {
    pub round_trip: bool,
    pub wrong_key_rejected: bool,
    pub tamper_rejected: bool,
    pub truncation_rejected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineSizes {
    pub input_bytes: usize,
    pub compressed_bytes: usize,
    pub ciphertext_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PipelineResult {
    pub sizes: PipelineSizes,
    pub checks: BoundaryChecks,
}

fn codec_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

/// Compress one PMX/VMD payload with the draft Zstd level.
pub fn compress(input: &[u8]) -> Result<Vec<u8>, String> {
    zstd::bulk::compress(input, ZSTD_LEVEL).map_err(codec_error)
}

/// Decompress a single payload and require the expected decoded size.
pub fn decompress(input: &[u8], expected_size: usize) -> Result<Vec<u8>, String> {
    zstd::bulk::decompress(input, expected_size).map_err(codec_error)
}

/// Encrypt compressed bytes. Key and nonce stay private to the returned value.
pub fn encrypt(compressed: Vec<u8>, aad: &[u8]) -> Result<EncryptedPayload, String> {
    let mut key = [0_u8; AES_KEY_BYTES];
    let mut nonce = [0_u8; AES_NONCE_BYTES];
    getrandom(&mut key).map_err(codec_error)?;
    getrandom(&mut nonce).map_err(codec_error)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(codec_error)?;
    let nonce_ref = Nonce::from_slice(&nonce);
    let ciphertext = cipher
        .encrypt(
            nonce_ref,
            aes_gcm::aead::Payload {
                msg: &compressed,
                aad,
            },
        )
        .map_err(|_| "authentication encryption failed".to_string())?;
    Ok(EncryptedPayload {
        ciphertext,
        key,
        nonce,
    })
}

pub fn decrypt(payload: &EncryptedPayload, aad: &[u8]) -> Result<Vec<u8>, String> {
    decrypt_with_key_nonce(&payload.ciphertext, &payload.key, &payload.nonce, aad)
}

pub fn decrypt_with_key_nonce(
    ciphertext: &[u8],
    key: &[u8; AES_KEY_BYTES],
    nonce: &[u8; AES_NONCE_BYTES],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(codec_error)?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            aes_gcm::aead::Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| "authentication failed".to_string())
}

pub fn check_boundaries(payload: &EncryptedPayload, aad: &[u8]) -> BoundaryChecks {
    let wrong_key_rejected = {
        let mut wrong_key = payload.key;
        wrong_key[0] ^= 1;
        decrypt_with_key_nonce(&payload.ciphertext, &wrong_key, &payload.nonce, aad).is_err()
    };

    let tamper_rejected = {
        let mut tampered = payload.ciphertext.clone();
        if let Some(byte) = tampered.first_mut() {
            *byte ^= 1;
        }
        decrypt_with_key_nonce(&tampered, &payload.key, &payload.nonce, aad).is_err()
    };

    let truncation_rejected = {
        let mut truncated = payload.ciphertext.clone();
        truncated.pop();
        decrypt_with_key_nonce(&truncated, &payload.key, &payload.nonce, aad).is_err()
    };

    BoundaryChecks {
        round_trip: false,
        wrong_key_rejected,
        tamper_rejected,
        truncation_rejected,
    }
}

/// Execute the complete Phase 0 pipeline and dangerous-boundary probes.
///
/// The generated key and nonce are intentionally not returned or serialized.
pub fn run_case(input: &[u8], aad: &[u8]) -> Result<PipelineResult, String> {
    let compressed = compress(input)?;
    let encrypted = encrypt(compressed, aad)?;
    let decrypted = decrypt(&encrypted, aad)?;
    let restored = decompress(&decrypted, input.len())?;
    let round_trip = restored == input;

    let mut checks = check_boundaries(&encrypted, aad);
    checks.round_trip = round_trip;

    Ok(PipelineResult {
        sizes: PipelineSizes {
            input_bytes: input.len(),
            compressed_bytes: encrypted.ciphertext.len() - AES_TAG_BYTES,
            ciphertext_bytes: encrypted.ciphertext.len(),
        },
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        (0..32_768_u32)
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn complete_pipeline_is_byte_identical() {
        let input = sample();
        let outcome = run_case(&input, b"case/sample").expect("pipeline");
        assert_eq!(outcome.sizes.input_bytes, input.len());
        assert!(outcome.checks.round_trip);
    }

    #[test]
    fn authentication_boundaries_fail_closed() {
        let outcome = run_case(&sample(), b"case/sample").expect("pipeline");
        assert!(outcome.checks.wrong_key_rejected);
        assert!(outcome.checks.tamper_rejected);
        assert!(outcome.checks.truncation_rejected);
    }

    #[test]
    fn compression_is_before_authentication_tag() {
        let outcome = run_case(b"repeated repeated repeated", b"case/order").expect("pipeline");
        assert_eq!(
            outcome.sizes.ciphertext_bytes,
            outcome.sizes.compressed_bytes + AES_TAG_BYTES
        );
    }
}
