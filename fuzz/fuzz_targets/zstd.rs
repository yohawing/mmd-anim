#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mmd_anim_package::{MmdPackageLimits, fuzz_support};

const MAX_DECODED_SIZE: usize = 64 * 1024;

#[derive(Arbitrary, Debug)]
struct ZstdInput {
    bytes: Vec<u8>,
    expected_size: u16,
}

fuzz_target!(|input: ZstdInput| {
    let limits = MmdPackageLimits {
        max_entry_decoded_bytes: MAX_DECODED_SIZE as u64,
        ..MmdPackageLimits::default()
    };
    let source = &input.bytes[..input.bytes.len().min(MAX_DECODED_SIZE)];

    // Generate one valid bounded frame from arbitrary input so every run can
    // reach the profile decoder's frame/header/content-size checks. The raw
    // input is also sent directly to exercise malformed-frame handling.
    if let Ok(frame) = zstd::bulk::compress(source, 1) {
        let _ = fuzz_support::decode_zstd_profile(&frame, source.len() as u64, &limits);
    }
    let expected = (input.expected_size as usize).min(MAX_DECODED_SIZE) as u64;
    let _ = fuzz_support::decode_zstd_profile(source, expected, &limits);
});
