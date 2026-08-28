use aes_gcm::{
    aead::{AeadInPlace, KeyInit},
    Aes256Gcm, Key, Nonce, Tag,
};
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read, Write};

pub const AES_KEY_BYTES: usize = 32;
pub const AES_NONCE_BYTES: usize = 12;
pub const AES_TAG_BYTES: usize = 16;
pub const ZSTD_LEVEL: i32 = 3;
pub const MAX_DECODED_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_WINDOW_LOG: u32 = 26;

/// Public material used only by this deterministic Phase 0 test vector.
/// Production code must never use this key.
pub const TEST_VECTOR_KEY: [u8; AES_KEY_BYTES] = [0x42; AES_KEY_BYTES];
pub const TEST_VECTOR_NONCE: [u8; AES_NONCE_BYTES] = [0x24; AES_NONCE_BYTES];
pub const TEST_VECTOR_AAD: &[u8] = b"mmdpack-phase0-backends/conformance/v1";
pub const TEST_VECTOR_INPUT: &[u8] = b"mmdpack Phase 0 backend conformance vector";

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn conformance_material() -> ([u8; AES_KEY_BYTES], [u8; AES_NONCE_BYTES], Vec<u8>) {
    (TEST_VECTOR_KEY, TEST_VECTOR_NONCE, TEST_VECTOR_AAD.to_vec())
}

/// Derive per-run campaign material. The key is supplied by the orchestrator
/// through an ephemeral process environment value and is never serialized.
pub fn campaign_material(
    run_key: &[u8; AES_KEY_BYTES],
    case_id: &str,
    domain: &str,
) -> ([u8; AES_KEY_BYTES], [u8; AES_NONCE_BYTES], Vec<u8>) {
    let mut nonce_hash = Sha256::new();
    nonce_hash.update(b"mmdpack-phase0-backends/nonce/v2/");
    nonce_hash.update(run_key);
    nonce_hash.update(case_id.as_bytes());
    nonce_hash.update(domain.as_bytes());
    let nonce_digest = nonce_hash.finalize();
    let mut nonce = [0_u8; AES_NONCE_BYTES];
    nonce.copy_from_slice(&nonce_digest[..AES_NONCE_BYTES]);
    let mut aad = b"mmdpack-phase0-backends/aad/v2/".to_vec();
    aad.extend_from_slice(case_id.as_bytes());
    aad.extend_from_slice(b"/");
    aad.extend_from_slice(domain.as_bytes());
    (*run_key, nonce, aad)
}

pub fn aes_encrypt_rustcrypto(
    plaintext: &[u8],
    key: &[u8; AES_KEY_BYTES],
    nonce: &[u8; AES_NONCE_BYTES],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let mut wire = plaintext.to_vec();
    let tag = cipher
        .encrypt_in_place_detached(Nonce::from_slice(nonce), aad, &mut wire)
        .map_err(|_| "RustCrypto AES-GCM encryption failed".to_string())?;
    wire.extend_from_slice(&tag);
    Ok(wire)
}

pub fn aes_decrypt_rustcrypto(
    wire: &[u8],
    key: &[u8; AES_KEY_BYTES],
    nonce: &[u8; AES_NONCE_BYTES],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    if wire.len() < AES_TAG_BYTES {
        return Err("AES-GCM wire payload is shorter than its tag".to_string());
    }
    let split = wire.len() - AES_TAG_BYTES;
    let mut plaintext = wire[..split].to_vec();
    let tag = Tag::from_slice(&wire[split..]);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher
        .decrypt_in_place_detached(Nonce::from_slice(nonce), aad, &mut plaintext, tag)
        .map_err(|_| "RustCrypto AES-GCM authentication failed".to_string())?;
    Ok(plaintext)
}

fn preflight_zstd(frame: &[u8], expected_size: usize) -> Result<(), String> {
    if expected_size > MAX_DECODED_BYTES {
        return Err("Zstandard decoded size exceeds the harness limit".to_string());
    }
    let (declared, window_size, frame_end) = parse_zstd_frame(frame)?;
    let expected_size_u64 = u64::try_from(expected_size)
        .map_err(|_| "Zstandard expected size does not fit u64".to_string())?;
    if declared != expected_size_u64 || window_size > (1_u64 << MAX_WINDOW_LOG) {
        return Err("Zstandard frame size or window exceeds the harness limit".to_string());
    }
    if frame_end != frame.len() {
        return Err("Zstandard input contains trailing bytes after one frame".to_string());
    }
    if declared > MAX_DECODED_BYTES as u64 {
        return Err("Zstandard declared size exceeds the harness limit".to_string());
    }
    let declared = usize::try_from(declared)
        .map_err(|_| "Zstandard declared size does not fit usize".to_string())?;
    if declared != expected_size {
        return Err("Zstandard declared size does not match the input size".to_string());
    }
    Ok(())
}

fn parse_zstd_frame(frame: &[u8]) -> Result<(u64, u64, usize), String> {
    const MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
    if frame.len() < 5 || frame[..4] != MAGIC {
        return Err("Zstandard frame magic is invalid".to_string());
    }
    let descriptor = frame[4];
    if descriptor & 0x08 != 0 {
        return Err("Zstandard frame descriptor has reserved bits set".to_string());
    }
    let single_segment = descriptor & 0x20 != 0;
    let fcs_flag = descriptor >> 6;
    let dictionary_flag = descriptor & 0x03;
    if dictionary_flag != 0 {
        return Err("Zstandard dictionary frames are not supported by this profile".to_string());
    }
    let dictionary_bytes = match dictionary_flag {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => 4,
    };
    let fcs_bytes = match (fcs_flag, single_segment) {
        (0, false) => 0,
        (0, true) => 1,
        (1, _) => 2,
        (2, _) => 4,
        _ => 8,
    };
    let mut at = 5_usize;
    let window_size = if single_segment {
        0
    } else {
        let window_descriptor = *frame
            .get(at)
            .ok_or_else(|| "Zstandard window descriptor is truncated".to_string())?;
        at = at
            .checked_add(1)
            .ok_or_else(|| "Zstandard header offset overflow".to_string())?;
        let exponent = u64::from(window_descriptor >> 3);
        let mantissa = u64::from(window_descriptor & 0x07);
        let base = 1_u64
            .checked_shl(
                u32::try_from(exponent)
                    .unwrap_or(u32::MAX)
                    .saturating_add(10),
            )
            .ok_or_else(|| "Zstandard window shift overflow".to_string())?;
        let add = (base / 8)
            .checked_mul(mantissa)
            .ok_or_else(|| "Zstandard window size overflow".to_string())?;
        base.checked_add(add)
            .ok_or_else(|| "Zstandard window size overflow".to_string())?
    };
    at = at
        .checked_add(dictionary_bytes)
        .ok_or_else(|| "Zstandard dictionary offset overflow".to_string())?;
    let fcs_end = at
        .checked_add(fcs_bytes)
        .ok_or_else(|| "Zstandard content-size offset overflow".to_string())?;
    if fcs_end > frame.len() {
        return Err("Zstandard frame header is truncated".to_string());
    }
    let declared = if fcs_bytes == 0 {
        return Err("Zstandard frame has no declared content size".to_string());
    } else {
        let mut value = 0_u64;
        for (shift, byte) in frame[at..fcs_end].iter().enumerate() {
            value |= u64::from(*byte)
                .checked_shl(u32::try_from(shift * 8).unwrap_or(u32::MAX))
                .ok_or_else(|| "Zstandard content-size shift overflow".to_string())?;
        }
        if fcs_bytes == 2 {
            value
                .checked_add(256)
                .ok_or_else(|| "Zstandard content-size overflow".to_string())?
        } else {
            value
        }
    };
    let window_size = if single_segment {
        declared
    } else {
        window_size
    };
    at = fcs_end;
    let checksum_bytes = if descriptor & 0x04 != 0 { 4 } else { 0 };
    if checksum_bytes != 0 {
        return Err("Zstandard checksum frames are not supported by this profile".to_string());
    }
    loop {
        let header_end = at
            .checked_add(3)
            .ok_or_else(|| "Zstandard block-header offset overflow".to_string())?;
        if header_end > frame.len() {
            return Err("Zstandard block header is truncated".to_string());
        }
        let header = u32::from_le_bytes([frame[at], frame[at + 1], frame[at + 2], 0]);
        let last = header & 1 != 0;
        let block_type = (header >> 1) & 0x03;
        let block_size = usize::try_from(header >> 3)
            .map_err(|_| "Zstandard block size does not fit usize".to_string())?;
        if block_type == 3 {
            return Err("Zstandard reserved block type is invalid".to_string());
        }
        let payload_size = if block_type == 1 { 1 } else { block_size };
        at = header_end
            .checked_add(payload_size)
            .ok_or_else(|| "Zstandard block range overflow".to_string())?;
        if at > frame.len() {
            return Err("Zstandard block payload is truncated".to_string());
        }
        if last {
            let end = at
                .checked_add(checksum_bytes)
                .ok_or_else(|| "Zstandard checksum range overflow".to_string())?;
            if end > frame.len() {
                return Err("Zstandard checksum is truncated".to_string());
            }
            return Ok((declared, window_size, end));
        }
    }
}

fn read_bounded<R: Read>(reader: &mut R, expected_size: usize) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(expected_size);
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| "Zstandard decoder read failed".to_string())?;
        if count == 0 {
            break;
        }
        let next = output
            .len()
            .checked_add(count)
            .ok_or_else(|| "Zstandard output size overflow".to_string())?;
        if next > expected_size || next > MAX_DECODED_BYTES {
            return Err("Zstandard output exceeds the expected size limit".to_string());
        }
        output.extend_from_slice(&buffer[..count]);
    }
    if output.len() != expected_size {
        return Err("Zstandard decoded size does not match the expected size".to_string());
    }
    Ok(output)
}

pub fn zstd_encode(input: &[u8]) -> Result<Vec<u8>, String> {
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), ZSTD_LEVEL)
        .map_err(|_| "libzstd encoder initialization failed".to_string())?;
    encoder
        .set_pledged_src_size(Some(
            u64::try_from(input.len()).map_err(|_| "input size does not fit u64".to_string())?,
        ))
        .map_err(|_| "libzstd content-size setup failed".to_string())?;
    encoder
        .include_contentsize(true)
        .map_err(|_| "libzstd content-size setup failed".to_string())?;
    encoder
        .write_all(input)
        .map_err(|_| "libzstd encoding failed".to_string())?;
    encoder
        .finish()
        .map_err(|_| "libzstd encoding failed".to_string())
}

pub fn zstd_decode_baseline(frame: &[u8], expected_size: usize) -> Result<Vec<u8>, String> {
    preflight_zstd(frame, expected_size)?;
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(frame))
        .map_err(|_| "libzstd decoder initialization failed".to_string())?
        .single_frame();
    decoder
        .window_log_max(MAX_WINDOW_LOG)
        .map_err(|_| "libzstd window limit rejected the frame".to_string())?;
    read_bounded(&mut decoder, expected_size)
}

pub fn zstd_decode_ruzstd(frame: &[u8], expected_size: usize) -> Result<Vec<u8>, String> {
    preflight_zstd(frame, expected_size)?;
    let mut decoder = ruzstd::decoding::StreamingDecoder::new(Cursor::new(frame))
        .map_err(|_| "ruzstd decoder initialization failed".to_string())?;
    read_bounded(&mut decoder, expected_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes_wire_round_trip_and_boundaries() {
        let input = TEST_VECTOR_INPUT;
        let (key, nonce, aad) = conformance_material();
        let wire = aes_encrypt_rustcrypto(input, &key, &nonce, &aad).unwrap();
        assert_eq!(
            aes_decrypt_rustcrypto(&wire, &key, &nonce, &aad).unwrap(),
            input
        );

        let mut wrong_key = key;
        wrong_key[0] ^= 1;
        assert!(aes_decrypt_rustcrypto(&wire, &wrong_key, &nonce, &aad).is_err());
        let mut tampered = wire.clone();
        tampered[0] ^= 1;
        assert!(aes_decrypt_rustcrypto(&tampered, &key, &nonce, &aad).is_err());
        assert!(aes_decrypt_rustcrypto(&wire[..wire.len() - 1], &key, &nonce, &aad).is_err());
    }

    #[test]
    fn zstd_decoders_match_and_reject_size_drift() {
        let input = vec![0x5a; 8192];
        let frame = zstd_encode(&input).unwrap();
        assert_eq!(zstd_decode_baseline(&frame, input.len()).unwrap(), input);
        assert_eq!(zstd_decode_ruzstd(&frame, input.len()).unwrap(), input);
        assert!(zstd_decode_baseline(&frame, input.len() - 1).is_err());
        assert!(zstd_decode_ruzstd(&frame[..frame.len() - 1], input.len()).is_err());
    }

    #[test]
    fn zstd_profile_rejects_dictionary_checksum_and_trailing_frames() {
        let input = vec![0x31; 8192];
        let frame = zstd_encode(&input).unwrap();

        let mut dictionary = frame.clone();
        dictionary[4] |= 1;
        assert!(zstd_decode_baseline(&dictionary, input.len()).is_err());

        let mut checksum = frame.clone();
        checksum[4] |= 0x04;
        assert!(zstd_decode_ruzstd(&checksum, input.len()).is_err());

        let mut concatenated = frame.clone();
        concatenated.extend_from_slice(&frame);
        assert!(zstd_decode_baseline(&concatenated, input.len()).is_err());
        assert!(zstd_decode_ruzstd(&concatenated, input.len()).is_err());
    }

    #[test]
    fn zstd_profile_rejects_reserved_and_oversized_headers() {
        let input = vec![0x24; 256];
        let frame = zstd_encode(&input).unwrap();

        let mut reserved = frame.clone();
        reserved[4] |= 0x08;
        assert!(zstd_decode_baseline(&reserved, input.len()).is_err());

        // Structurally complete single frame: declared content size is 256,
        // while the window descriptor requests 128 MiB. Both decoders must
        // reject this before allocation; malformed/incomplete headers are not
        // sufficient evidence for the window boundary.
        let mut over_window = vec![
            0x28, 0xb5, 0x2f, 0xfd, 0x40, // FCS=2 bytes, non-single segment
            0x88, // exponent 17 => 128 MiB window
            0x00, 0x00, // declared content size = 256 (FCS value + 256)
            0x01, 0x08, 0x00, // last raw block, 256-byte payload
        ];
        over_window.extend(std::iter::repeat_n(0_u8, 256));
        assert!(zstd_decode_baseline(&over_window, input.len()).is_err());
        assert!(zstd_decode_ruzstd(&over_window, input.len()).is_err());
    }
}
