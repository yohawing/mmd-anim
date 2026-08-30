#![no_main]

use libfuzzer_sys::fuzz_target;
use mmd_anim_package::{MmdPackageLimits, fuzz_support};

const VALID_MANIFEST: &[u8] = br#"{"schema":"mmdpack/1","defaultModelEntryId":1,"entries":[{"id":1,"path":"model/model.pmx","kind":"model","codec":"pmx","compression":"none","offset":0,"cipherSize":19,"decodedSize":3}],"modelBindings":[{"modelEntryId":1,"textureBindings":[]}] }"#;

fuzz_target!(|data: &[u8]| {
    let limits = MmdPackageLimits {
        max_manifest_cipher_bytes: 64 * 1024,
        ..MmdPackageLimits::default()
    };

    // Always exercise the complete manifest validator once, then let the
    // arbitrary bytes explore strict JSON and all rejection boundaries.
    let _ = fuzz_support::validate_manifest_json(VALID_MANIFEST, &limits);
    let bounded = &data[..data.len().min(64 * 1024)];
    let _ = fuzz_support::validate_manifest_json(bounded, &limits);
});
