#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use mmd_anim_package::{
    MmdPackageCompression, MmdPackageEntry, MmdPackageEntryKind, MmdPackageLimits, fuzz_support,
};

#[derive(Arbitrary, Debug)]
struct LayoutInput {
    entries: Vec<LayoutEntry>,
    package_len: u32,
    payload_base: u32,
}

#[derive(Arbitrary, Debug)]
struct LayoutEntry {
    id: u32,
    offset: u64,
    cipher_size: u64,
    decoded_size: u64,
}

fuzz_target!(|input: LayoutInput| {
    let limits = MmdPackageLimits {
        max_entries: 64,
        max_entry_cipher_bytes: 8 * 1024 * 1024,
        max_entry_decoded_bytes: 8 * 1024 * 1024,
        max_total_decoded_bytes: 32 * 1024 * 1024,
        ..MmdPackageLimits::default()
    };
    let entries: Vec<_> = input
        .entries
        .into_iter()
        .take(limits.max_entries)
        .enumerate()
        .map(|(index, value)| MmdPackageEntry {
            id: value.id,
            path: format!("binary/{index:08x}.bin"),
            kind: MmdPackageEntryKind::Binary,
            codec: "opaque".to_owned(),
            compression: MmdPackageCompression::None,
            offset: value.offset,
            cipher_size: value.cipher_size,
            decoded_size: value.decoded_size,
        })
        .collect();
    let package_len = input.package_len as usize;
    let payload_base = input.payload_base as usize;
    let _ = fuzz_support::validate_entry_layout(&entries, package_len, payload_base, &limits);
});
