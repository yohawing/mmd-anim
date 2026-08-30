//! Experimental Draft 0.2 golden package vector.
//!
//! The vector is intentionally test-only. It documents the current draft wire
//! bytes without freezing the eventual MMDPACK V1 format.

use std::sync::Arc;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use mmd_anim_package::{
    MMDPACK_HEADER_LEN, MmdPackage, MmdPackageCompression, MmdPackageEntryKind, MmdPackageError,
    MmdPackageLimits, MmdPackageVerifyOptions,
};

const KEY: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const PACKAGE_ID: [u8; 16] = [
    0x46, 0x21, 0x22, 0x23, 0x24, 0x25, 0x46, 0x27, 0xa8, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
];
const NONCE_PREFIX: [u8; 8] = [0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37];

// These bytes are the complete deterministic Draft 0.2 fixture components.
// The package ID is in RFC 9562/network byte order, while nonce IDs and all
// integer fields use little endian as specified by the current draft.
const HEADER_HEX: &str = "4d4d445041434b0001000000000000004621222324254627a8292a2b2c2d2e2f3031323334353637040100000000000000000000000000000000000000000000";
const MANIFEST_PLAINTEXT: &[u8] = br#"{"schema":"mmdpack/1","defaultModelEntryId":1,"entries":[{"id":1,"path":"model/model.pmx","kind":"model","codec":"pmx","compression":"none","offset":0,"cipherSize":19,"decodedSize":3}],"modelBindings":[{"modelEntryId":1,"textureBindings":[]}] }"#;
const MANIFEST_CIPHERTEXT_HEX: &str = "c9642f94e45ebdee516d5dc3cdc770d1a7bcbbc09b0b6fefc7c4c16ede3d1960f986461d32dcd9fc08a6d5682e92d8b0d19a884b368f812390c01ed31081211e66c79c5dc31fddad4a8fdba130249482eec44d80bf9fed63067324c121a7605b7b21b7992c2f7f62a7b400e97ec06dc2c3a1cd2739d8eb1d52a845089c7bf684e7f175d220d14771ec9d6b71c0342df66392a39152fbbb17b64b0e7c36d21b1ee28a31fcf011d34c4530a38cf6d8c7a6b911abb4765dabd633d8e8f163454f734529191600455574e02559c9c1478c96fc9db4b2e77de2cb1e4fd4cbf6eb9332864d98ed9d85e71bfc44ab67448677816c22b6c575e7e2bfeafccf592aca6407a6151ed2";
const ENTRY_AAD_HEX: &str =
    "4d4d44502d4141442d314621222324254627a8292a2b2c2d2e2f010000000300000000000000";
const ENTRY_PLAINTEXT: &[u8] = b"PMX";
const ENTRY_CIPHERTEXT_HEX: &str = "80460dcf0b83ab5dc319e75359a1316ff31e7c";

fn nonce(nonce_prefix: &[u8; 8], id: u32) -> [u8; 12] {
    let mut value = [0; 12];
    value[..8].copy_from_slice(nonce_prefix);
    value[8..].copy_from_slice(&id.to_le_bytes());
    value
}

fn entry_aad(package_id: &[u8; 16], id: u32, decoded_size: u64) -> [u8; 38] {
    let mut value = [0; 38];
    value[..10].copy_from_slice(b"MMDP-AAD-1");
    value[10..26].copy_from_slice(package_id);
    value[26..30].copy_from_slice(&id.to_le_bytes());
    value[30..].copy_from_slice(&decoded_size.to_le_bytes());
    value
}

fn build_derived_vector(key: &[u8; 32], package_id: &[u8; 16], nonce_prefix: &[u8; 8]) -> Vec<u8> {
    let manifest_cipher_size = MANIFEST_PLAINTEXT.len() as u64 + 16;
    let mut header = [0; MMDPACK_HEADER_LEN];
    header[..8].copy_from_slice(b"MMDPACK\0");
    header[8..10].copy_from_slice(&1_u16.to_le_bytes());
    header[16..32].copy_from_slice(package_id);
    header[32..40].copy_from_slice(nonce_prefix);
    header[40..48].copy_from_slice(&manifest_cipher_size.to_le_bytes());

    let cipher = Aes256Gcm::new_from_slice(key).unwrap();
    let manifest = cipher
        .encrypt(
            Nonce::from_slice(&nonce(nonce_prefix, 0)),
            Payload {
                msg: MANIFEST_PLAINTEXT,
                aad: &header,
            },
        )
        .unwrap();
    let entry = cipher
        .encrypt(
            Nonce::from_slice(&nonce(nonce_prefix, 1)),
            Payload {
                msg: ENTRY_PLAINTEXT,
                aad: &entry_aad(package_id, 1, ENTRY_PLAINTEXT.len() as u64),
            },
        )
        .unwrap();

    let mut package = header.to_vec();
    package.extend_from_slice(&manifest);
    package.extend_from_slice(&entry);
    package
}

fn build_vector() -> Vec<u8> {
    let mut package = hex_decode(HEADER_HEX);
    package.extend_from_slice(&hex_decode(MANIFEST_CIPHERTEXT_HEX));
    package.extend_from_slice(&hex_decode(ENTRY_CIPHERTEXT_HEX));
    package
}

fn build_other_package() -> Vec<u8> {
    // Keep the key and nonce prefix equal so a swapped entry exercises the
    // package_id component of Entry AAD rather than only a different nonce.
    let mut package_id = PACKAGE_ID;
    package_id[15] ^= 1;
    build_derived_vector(&KEY, &package_id, &NONCE_PREFIX)
}

fn open(bytes: Vec<u8>) -> Result<MmdPackage, MmdPackageError> {
    MmdPackage::open_bytes(Arc::from(bytes), KEY, MmdPackageLimits::default())
}

fn hex_decode(value: &str) -> Vec<u8> {
    assert!(value.len().is_multiple_of(2));
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn draft_0_2_vector_is_byte_stable() {
    let fixture = build_vector();
    assert_eq!(hex(&fixture[..MMDPACK_HEADER_LEN]), HEADER_HEX);
    assert_eq!(
        hex(&fixture
            [MMDPACK_HEADER_LEN..MMDPACK_HEADER_LEN + hex_decode(MANIFEST_CIPHERTEXT_HEX).len()]),
        MANIFEST_CIPHERTEXT_HEX
    );
    assert_eq!(hex(&fixture[fixture.len() - 19..]), ENTRY_CIPHERTEXT_HEX);
    assert_eq!(
        fixture,
        build_derived_vector(&KEY, &PACKAGE_ID, &NONCE_PREFIX),
        "Draft 0.2 golden bytes drifted"
    );
}

#[test]
fn draft_0_2_vector_opens_reads_and_verifies() {
    let package = open(build_vector()).unwrap();
    assert_eq!(package.header().major, 1);
    assert_eq!(package.header().minor, 0);
    assert_eq!(package.header().package_id, PACKAGE_ID);
    assert_eq!(package.header().nonce_prefix, NONCE_PREFIX);
    assert_eq!(package.header().manifest_cipher_size, 260);

    let manifest = package.manifest();
    assert_eq!(manifest.schema, "mmdpack/1");
    assert_eq!(manifest.default_model_entry_id, 1);
    assert_eq!(manifest.default_motion_entry_id, None);
    assert_eq!(manifest.entries.len(), 1);
    assert_eq!(manifest.entries[0].id, 1);
    assert_eq!(manifest.entries[0].path, "model/model.pmx");
    assert_eq!(manifest.entries[0].kind, MmdPackageEntryKind::Model);
    assert_eq!(manifest.entries[0].codec, "pmx");
    assert_eq!(manifest.entries[0].compression, MmdPackageCompression::None);
    assert_eq!(manifest.entries[0].offset, 0);
    assert_eq!(manifest.entries[0].cipher_size, 19);
    assert_eq!(manifest.entries[0].decoded_size, 3);
    assert_eq!(package.read_entry(1).unwrap(), ENTRY_PLAINTEXT);
    assert_eq!(package.read("model/model.pmx").unwrap(), ENTRY_PLAINTEXT);

    let report = package.verify(MmdPackageVerifyOptions::default()).unwrap();
    assert_eq!(report.entry_count, 1);
    assert_eq!(report.authenticated_entry_count, 1);
    assert_eq!(report.total_decoded_bytes, 3);
    assert!(report.unknown_codec_entry_ids.is_empty());
}

#[test]
fn draft_0_2_vector_documents_aad_and_nonce_bytes() {
    let fixture = build_vector();
    let header = &fixture[..MMDPACK_HEADER_LEN];
    assert_eq!(&header[..8], b"MMDPACK\0");
    assert_eq!(&header[16..32], PACKAGE_ID);
    assert_eq!(&header[32..40], NONCE_PREFIX);
    assert_eq!(hex(header), HEADER_HEX);
    assert_eq!(hex(&nonce(&NONCE_PREFIX, 0)), "303132333435363700000000");
    assert_eq!(hex(&nonce(&NONCE_PREFIX, 1)), "303132333435363701000000");
    assert_eq!(hex(&entry_aad(&PACKAGE_ID, 1, 3)), ENTRY_AAD_HEX);
}

#[test]
fn draft_0_2_failure_vectors_reject_wrong_key_and_tampering() {
    let fixture = build_vector();

    assert!(matches!(
        MmdPackage::open_bytes(
            Arc::from(fixture.clone()),
            [0xff; 32],
            MmdPackageLimits::default()
        ),
        Err(MmdPackageError::AuthenticationFailed("manifest"))
    ));

    let mut header_tampered = fixture.clone();
    header_tampered[16] ^= 1;
    assert!(matches!(
        open(header_tampered),
        Err(MmdPackageError::AuthenticationFailed("manifest"))
    ));

    let mut manifest_tampered = fixture.clone();
    manifest_tampered[MMDPACK_HEADER_LEN] ^= 1;
    assert!(matches!(
        open(manifest_tampered),
        Err(MmdPackageError::AuthenticationFailed("manifest"))
    ));

    let mut entry_tampered = fixture.clone();
    *entry_tampered.last_mut().unwrap() ^= 1;
    assert!(matches!(
        open(entry_tampered).unwrap().read_entry(1),
        Err(MmdPackageError::AuthenticationFailed("entry"))
    ));

    let truncated = &fixture[..fixture.len() - 1];
    assert!(matches!(
        open(truncated.to_vec()),
        Err(MmdPackageError::InvalidManifest(message)) if message.contains("entry table covers")
    ));
}

#[test]
fn draft_0_2_failure_vector_rejects_cross_package_entry_swap() {
    let package_a = build_vector();
    let package_b = build_other_package();
    let parsed_a = open(package_a.clone()).unwrap();
    let parsed_b = open(package_b.clone()).unwrap();
    let payload_base_a = MMDPACK_HEADER_LEN + parsed_a.header().manifest_cipher_size as usize;
    let payload_base_b = MMDPACK_HEADER_LEN + parsed_b.header().manifest_cipher_size as usize;
    let entry_size = parsed_a.manifest().entries[0].cipher_size as usize;
    assert_eq!(
        entry_size,
        parsed_b.manifest().entries[0].cipher_size as usize
    );

    let mut swapped = package_a;
    swapped[payload_base_a..payload_base_a + entry_size]
        .copy_from_slice(&package_b[payload_base_b..payload_base_b + entry_size]);
    assert!(matches!(
        open(swapped).unwrap().read_entry(1),
        Err(MmdPackageError::AuthenticationFailed("entry"))
    ));
}
