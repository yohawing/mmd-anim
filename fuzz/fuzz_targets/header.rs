#![no_main]

use libfuzzer_sys::fuzz_target;
use mmd_anim_package::MmdPackageHeader;

fuzz_target!(|data: &[u8]| {
    let _ = MmdPackageHeader::parse_prefix(data);
});
