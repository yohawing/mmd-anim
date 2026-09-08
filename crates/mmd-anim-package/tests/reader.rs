use std::sync::Arc;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use mmd_anim_package::{
    MMDPACK_HEADER_LEN, MmdPackage, MmdPackageError, MmdPackageHeader, MmdPackageLimits,
};
use serde_json::{Value, json};

const KEY: [u8; 32] = [0x42; 32];
const PACKAGE_ID: [u8; 16] = [0x11; 16];
const NONCE_PREFIX: [u8; 8] = [0x22; 8];

#[derive(Clone)]
struct InputEntry {
    id: u32,
    path: &'static str,
    kind: &'static str,
    codec: &'static str,
    compression: &'static str,
    decoded: Vec<u8>,
}

fn model(decoded: &[u8]) -> InputEntry {
    InputEntry {
        id: 1,
        path: "model/model.pmx",
        kind: "model",
        codec: "pmx",
        compression: "none",
        decoded: decoded.to_vec(),
    }
}

fn build(entries: &[InputEntry]) -> Vec<u8> {
    let (manifest_entries, encoded_entries) = encode_entries(entries);
    let manifest = json!({
        "schema": "mmdpack/1",
        "defaultModelEntryId": 1,
        "entries": manifest_entries,
        "modelBindings": [{"modelEntryId": 1, "textureBindings": []}],
    });
    build_parts(serde_json::to_vec(&manifest).unwrap(), &encoded_entries)
}

fn encode_entries(entries: &[InputEntry]) -> (Vec<Value>, Vec<(InputEntry, Vec<u8>)>) {
    let mut offset = 0_u64;
    let mut encoded_entries = Vec::new();
    let manifest_entries: Vec<_> = entries
        .iter()
        .map(|entry| {
            let encoded = match entry.compression {
                "none" => entry.decoded.clone(),
                "zstd-v1" => zstd::bulk::compress(&entry.decoded, 3).unwrap(),
                other => panic!("unsupported test compression {other}"),
            };
            let cipher_size = encoded.len() as u64 + 16;
            let value = json!({
                "id": entry.id,
                "path": entry.path,
                "kind": entry.kind,
                "codec": entry.codec,
                "compression": entry.compression,
                "offset": offset,
                "cipherSize": cipher_size,
                "decodedSize": entry.decoded.len(),
            });
            offset += cipher_size;
            encoded_entries.push((entry.clone(), encoded));
            value
        })
        .collect();
    (manifest_entries, encoded_entries)
}

fn build_parts(manifest_plaintext: Vec<u8>, entries: &[(InputEntry, Vec<u8>)]) -> Vec<u8> {
    let manifest_cipher_size = manifest_plaintext.len() as u64 + 16;
    let mut header = [0_u8; MMDPACK_HEADER_LEN];
    header[..8].copy_from_slice(b"MMDPACK\0");
    header[8..10].copy_from_slice(&1_u16.to_le_bytes());
    header[16..32].copy_from_slice(&PACKAGE_ID);
    header[32..40].copy_from_slice(&NONCE_PREFIX);
    header[40..48].copy_from_slice(&manifest_cipher_size.to_le_bytes());

    let cipher = Aes256Gcm::new_from_slice(&KEY).unwrap();
    let manifest_ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce(0)),
            Payload {
                msg: &manifest_plaintext,
                aad: &header,
            },
        )
        .unwrap();
    let mut package = header.to_vec();
    package.extend_from_slice(&manifest_ciphertext);
    for (entry, encoded) in entries {
        let aad = entry_aad(entry.id, entry.decoded.len() as u64);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce(entry.id)),
                Payload {
                    msg: encoded,
                    aad: &aad,
                },
            )
            .unwrap();
        package.extend_from_slice(&ciphertext);
    }
    package
}

fn nonce(id: u32) -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    nonce[..8].copy_from_slice(&NONCE_PREFIX);
    nonce[8..].copy_from_slice(&id.to_le_bytes());
    nonce
}

fn entry_aad(id: u32, decoded_size: u64) -> [u8; 38] {
    let mut aad = [0_u8; 38];
    aad[..10].copy_from_slice(b"MMDP-AAD-1");
    aad[10..26].copy_from_slice(&PACKAGE_ID);
    aad[26..30].copy_from_slice(&id.to_le_bytes());
    aad[30..].copy_from_slice(&decoded_size.to_le_bytes());
    aad
}

fn open(bytes: Vec<u8>) -> Result<MmdPackage, MmdPackageError> {
    MmdPackage::open_bytes(Arc::from(bytes), KEY, MmdPackageLimits::default())
}

#[test]
fn parses_fixed_header_without_decrypting() {
    let bytes = build(&[model(b"PMX")]);
    let header = MmdPackageHeader::parse_prefix(&bytes).unwrap();
    assert_eq!(header.major, 1);
    assert_eq!(header.minor, 0);
    assert_eq!(header.package_id, PACKAGE_ID);
    assert_eq!(header.nonce_prefix, NONCE_PREFIX);
}

#[test]
fn rejects_invalid_fixed_header_fields() {
    assert!(matches!(
        MmdPackageHeader::parse_prefix(&[]),
        Err(MmdPackageError::HeaderTooShort)
    ));
    let mut bytes = build(&[model(b"PMX")]);
    bytes[0] ^= 1;
    assert!(matches!(
        MmdPackageHeader::parse_prefix(&bytes),
        Err(MmdPackageError::InvalidMagic)
    ));
    let mut bytes = build(&[model(b"PMX")]);
    bytes[12] = 1;
    assert!(matches!(
        MmdPackageHeader::parse_prefix(&bytes),
        Err(MmdPackageError::UnsupportedFlags(1))
    ));
    let mut bytes = build(&[model(b"PMX")]);
    bytes[63] = 1;
    assert!(matches!(
        MmdPackageHeader::parse_prefix(&bytes),
        Err(MmdPackageError::NonZeroReserved)
    ));
}

#[test]
fn reads_only_requested_uncompressed_entry_by_id_or_path() {
    let bytes = build(&[
        model(b"model bytes"),
        InputEntry {
            id: 2,
            path: "meta/readme.txt",
            kind: "metadata",
            codec: "utf8",
            compression: "none",
            decoded: b"hello".to_vec(),
        },
    ]);
    let package = open(bytes).unwrap();
    assert_eq!(package.read_entry(1).unwrap(), b"model bytes");
    assert_eq!(package.read("meta/readme.txt").unwrap(), b"hello");
    assert!(matches!(
        package.read("META/readme.txt"),
        Err(MmdPackageError::PathNotFound(_))
    ));
}

#[test]
fn decodes_one_bounded_zstd_frame() {
    let mut compressed_model = model(&vec![0x5a; 64 * 1024]);
    compressed_model.compression = "zstd-v1";
    let package = open(build(&[compressed_model])).unwrap();
    assert_eq!(package.read_entry(1).unwrap(), vec![0x5a; 64 * 1024]);
}

#[test]
fn rejects_wrong_key_and_ciphertext_tampering() {
    let bytes = build(&[model(b"PMX")]);
    assert!(matches!(
        MmdPackage::open_bytes(
            Arc::from(bytes.clone()),
            [0x99; 32],
            MmdPackageLimits::default()
        ),
        Err(MmdPackageError::AuthenticationFailed("manifest"))
    ));

    let mut manifest_tampered = bytes.clone();
    manifest_tampered[MMDPACK_HEADER_LEN] ^= 1;
    assert!(matches!(
        open(manifest_tampered),
        Err(MmdPackageError::AuthenticationFailed("manifest"))
    ));

    let package = open(bytes.clone()).unwrap();
    let payload_base = MMDPACK_HEADER_LEN + package.header().manifest_cipher_size as usize;
    drop(package);
    let mut entry_tampered = bytes;
    entry_tampered[payload_base] ^= 1;
    assert!(matches!(
        open(entry_tampered).unwrap().read_entry(1),
        Err(MmdPackageError::AuthenticationFailed("entry"))
    ));
}

#[test]
fn enforces_limits_before_decoding() {
    let bytes = build(&[model(b"12345")]);
    let limits = MmdPackageLimits {
        max_entry_decoded_bytes: 4,
        ..MmdPackageLimits::default()
    };
    assert!(matches!(
        MmdPackage::open_bytes(Arc::from(bytes), KEY, limits),
        Err(MmdPackageError::LimitExceeded {
            what: "entry decoded bytes",
            actual: 5,
            limit: 4
        })
    ));
}

#[test]
fn rejects_non_contiguous_or_trailing_payload_layout() {
    let entry = model(b"PMX");
    let encoded = entry.decoded.clone();
    let invalid_manifest = json!({
        "schema": "mmdpack/1",
        "defaultModelEntryId": 1,
        "entries": [{
            "id": 1,
            "path": entry.path,
            "kind": entry.kind,
            "codec": entry.codec,
            "compression": "none",
            "offset": 1,
            "cipherSize": encoded.len() + 16,
            "decodedSize": entry.decoded.len(),
        }],
        "modelBindings": [{"modelEntryId": 1, "textureBindings": []}],
    });
    let bytes = build_parts(
        serde_json::to_vec(&invalid_manifest).unwrap(),
        &[(entry, encoded)],
    );
    assert!(matches!(
        open(bytes),
        Err(MmdPackageError::InvalidManifest(message))
            if message.contains("contiguous offset")
    ));

    let mut bytes = build(&[model(b"PMX")]);
    bytes.push(0);
    assert!(matches!(
        open(bytes),
        Err(MmdPackageError::InvalidManifest(message))
            if message.contains("entry table covers")
    ));
}

#[test]
fn rejects_duplicate_json_keys_everywhere() {
    let manifest = br#"{
        "schema":"mmdpack/1",
        "defaultModelEntryId":1,
        "entries":[],
        "modelBindings":[{"modelEntryId":1,"modelEntryId":1,"textureBindings":[]}]
    }"#;
    let bytes = build_parts(manifest.to_vec(), &[]);
    assert!(matches!(
        open(bytes),
        Err(MmdPackageError::InvalidJson(message)) if message.contains("duplicate key")
    ));
}

#[test]
fn rejects_integers_outside_javascript_safe_range() {
    let manifest = br#"{
        "schema":"mmdpack/1",
        "defaultModelEntryId":1,
        "entries":[],
        "modelBindings":[],
        "futureValue":9007199254740992
    }"#;
    assert!(matches!(
        open(build_parts(manifest.to_vec(), &[])),
        Err(MmdPackageError::InvalidJson(message)) if message.contains("safe range")
    ));
}

#[test]
fn rejects_non_nfc_paths() {
    let decomposed = InputEntry {
        path: "model/cafe\u{301}.pmx",
        ..model(b"PMX")
    };
    assert!(matches!(
        open(build(&[decomposed])),
        Err(MmdPackageError::InvalidManifest(message)) if message.contains("not normalized")
    ));
}

#[test]
fn validates_default_motion_metadata() {
    let motion = InputEntry {
        id: 2,
        path: "motion/default.vmd",
        kind: "motion",
        codec: "vmd",
        compression: "none",
        decoded: b"VMD".to_vec(),
    };
    let (manifest_entries, encoded_entries) = encode_entries(&[model(b"PMX"), motion]);
    let manifest = json!({
        "schema": "mmdpack/1",
        "defaultModelEntryId": 1,
        "defaultMotionEntryId": 2,
        "entries": manifest_entries,
        "modelBindings": [{"modelEntryId": 1, "textureBindings": []}],
    });
    assert!(matches!(
        open(build_parts(serde_json::to_vec(&manifest).unwrap(), &encoded_entries)),
        Err(MmdPackageError::InvalidManifest(message)) if message.contains("motion")
    ));
}

#[test]
fn validates_texture_binding_shape_and_target() {
    let (manifest_entries, encoded_entries) = encode_entries(&[model(b"PMX")]);
    let manifest = json!({
        "schema": "mmdpack/1",
        "defaultModelEntryId": 1,
        "entries": manifest_entries,
        "modelBindings": [{"modelEntryId": 1}],
    });
    assert!(matches!(
        open(build_parts(serde_json::to_vec(&manifest).unwrap(), &encoded_entries)),
        Err(MmdPackageError::InvalidManifest(message)) if message.contains("textureBindings")
    ));
}

#[test]
fn preserves_unknown_codec_but_refuses_to_read_it() {
    let unknown = InputEntry {
        id: 2,
        path: "binary/future.bin",
        kind: "binary",
        codec: "future-codec-v2",
        ..model(b"future")
    };
    let package = open(build(&[model(b"PMX"), unknown])).unwrap();
    assert_eq!(package.manifest().entries[1].codec, "future-codec-v2");
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::UnsupportedCodec(codec)) if codec == "future-codec-v2"
    ));
}

#[test]
fn rejects_entry_metadata_swaps_through_aad() {
    let entries = [
        model(b"first"),
        InputEntry {
            id: 2,
            path: "meta/second.bin",
            kind: "binary",
            codec: "opaque",
            compression: "none",
            decoded: b"other".to_vec(),
        },
    ];
    let mut bytes = build(&entries);
    let package = open(bytes.clone()).unwrap();
    let payload_base = MMDPACK_HEADER_LEN + package.header().manifest_cipher_size as usize;
    let first_size = package.manifest().entries[0].cipher_size as usize;
    let second_size = package.manifest().entries[1].cipher_size as usize;
    assert_eq!(first_size, second_size);
    drop(package);
    let second_start = payload_base + first_size;
    let second = bytes[second_start..second_start + second_size].to_vec();
    bytes[payload_base..payload_base + first_size].copy_from_slice(&second);
    assert!(matches!(
        open(bytes).unwrap().read_entry(1),
        Err(MmdPackageError::AuthenticationFailed("entry"))
    ));
}
