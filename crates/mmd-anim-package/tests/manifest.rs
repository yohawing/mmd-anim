use std::sync::Arc;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use mmd_anim_package::{
    MMDPACK_HEADER_LEN, MmdModelBinding, MmdPackage, MmdPackageCompression, MmdPackageEntryKind,
    MmdPackageError, MmdPackageLimits, MmdPackagePackCompression, MmdPackagePackEntry,
    MmdPackagePackError, MmdPackagePackInput, MmdPackagePacker,
};
use serde_json::{Map, Value, json};

const KEY: [u8; 32] = [0x42; 32];
const PACKAGE_ID: [u8; 16] = [0x11; 16];
const NONCE_PREFIX: [u8; 8] = [0x22; 8];

fn model_entry() -> Value {
    json!({
        "id": 1,
        "path": "model/model.pmx",
        "kind": "model",
        "codec": "pmx",
        "compression": "none",
        "offset": 0,
        "cipherSize": 19,
        "decodedSize": 3,
    })
}

fn texture_entry(id: u32, codec: &str, metadata: Value) -> Value {
    json!({
        "id": id,
        "path": format!("texture/{id}.bin"),
        "kind": "texture",
        "codec": codec,
        "compression": "none",
        "offset": 0,
        "cipherSize": 32,
        "decodedSize": 16,
        "texture": metadata,
    })
}

fn raw_texture_metadata(mip_size: u64) -> Value {
    json!({
        "width": 1,
        "height": 1,
        "mipCount": 1,
        "colorSpace": "linear",
        "usage": "data",
        "channelModel": "rgba",
        "swizzle": "rgba",
        "alphaMode": "straight",
        "origin": "top-left",
        "blockOrder": "row-major-top-left",
        "mips": [{"width": 1, "height": 1, "offset": 0, "size": mip_size}],
    })
}

fn ktx2_texture_metadata() -> Value {
    json!({
        "width": 1,
        "height": 1,
        "mipCount": 1,
        "colorSpace": "srgb",
        "usage": "color",
        "channelModel": "rgba",
        "swizzle": "rgba",
        "alphaMode": "straight",
        "origin": "top-left",
    })
}

fn valid_ktx2_payload() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"\xabKTX 20\xbb\r\n\x1a\n");
    for value in [0_u32, 1, 1, 1, 0, 0, 1, 1, 0, 104, 44, 0, 0] {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    payload.extend_from_slice(&0_u64.to_le_bytes());
    payload.extend_from_slice(&0_u64.to_le_bytes());
    payload.extend_from_slice(&160_u64.to_le_bytes());
    payload.extend_from_slice(&16_u64.to_le_bytes());
    payload.extend_from_slice(&16_u64.to_le_bytes());

    payload.extend_from_slice(&44_u32.to_le_bytes());
    payload.extend_from_slice(&0_u32.to_le_bytes());
    payload.extend_from_slice(&0x0028_0002_u32.to_le_bytes());
    payload.extend_from_slice(&0x0002_01a6_u32.to_le_bytes());
    payload.extend_from_slice(&[3, 3, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0]);
    payload.extend_from_slice(&0x037f_0000_u32.to_le_bytes());
    payload.extend_from_slice(&[0; 4]);
    payload.extend_from_slice(&[0; 4]);
    payload.extend_from_slice(&[0xff; 4]);
    payload.extend_from_slice(&[0; 12]);
    payload.extend_from_slice(&[0; 16]);
    assert_eq!(payload.len(), 176);
    payload
}

fn valid_ktx2_zstd_payload(data: &[u8], bytes_plane: u8) -> Vec<u8> {
    let compressed = zstd::bulk::compress(data, 3).unwrap();
    let mut payload = valid_ktx2_payload();
    payload[44..48].copy_from_slice(&2_u32.to_le_bytes());
    payload[104 + 20] = bytes_plane;
    payload[88..96].copy_from_slice(&(compressed.len() as u64).to_le_bytes());
    payload.truncate(160);
    payload.extend_from_slice(&compressed);
    payload
}

fn valid_ktx2_payload_with_swizzle_kvd() -> Vec<u8> {
    let mut payload = valid_ktx2_payload();
    let level = payload.split_off(160);
    payload.truncate(148);
    payload[56..60].copy_from_slice(&148_u32.to_le_bytes());
    payload[60..64].copy_from_slice(&20_u32.to_le_bytes());
    payload.extend_from_slice(&16_u32.to_le_bytes());
    payload.extend_from_slice(b"KTXswizzle\0rgba\0");
    payload.extend_from_slice(&[0; 8]);
    assert_eq!(payload.len(), 176);
    payload[80..88].copy_from_slice(&176_u64.to_le_bytes());
    payload.extend_from_slice(&level);
    payload
}

fn incompressible_ktx2_zstd_payload() -> Vec<u8> {
    let compressed = zstd::bulk::compress(&[0; 16], 3).unwrap();
    let mut payload = valid_ktx2_payload();
    let mut state = 0x6d2b_79f5_u32;
    let mut pair = b"probe\0".to_vec();
    for _ in 0..(256 * 1024) {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        pair.push((state >> 24) as u8);
    }
    pair.push(0);

    payload.truncate(148);
    let kvd_length = (4 + pair.len() + 3) & !3;
    let level_offset = (148 + kvd_length + 15) & !15;
    payload[44..48].copy_from_slice(&2_u32.to_le_bytes());
    payload[104 + 20] = 16;
    payload[56..60].copy_from_slice(&148_u32.to_le_bytes());
    payload[60..64].copy_from_slice(&(kvd_length as u32).to_le_bytes());
    payload.extend_from_slice(&(pair.len() as u32).to_le_bytes());
    payload.extend_from_slice(&pair);
    payload.resize(level_offset, 0);
    payload[80..88].copy_from_slice(&(level_offset as u64).to_le_bytes());
    payload[88..96].copy_from_slice(&(compressed.len() as u64).to_le_bytes());
    payload.extend_from_slice(&compressed);
    payload
}

fn open_ktx2(payload: &[u8]) -> Result<MmdPackage, MmdPackageError> {
    open(
        manifest(vec![
            model_entry(),
            texture_entry(2, "ktx2-uastc-v1", ktx2_texture_metadata()),
        ]),
        &[b"PMX", payload],
        MmdPackageLimits::default(),
    )
}

fn pack_ktx2(
    payload: Vec<u8>,
    compression: MmdPackagePackCompression,
) -> Result<mmd_anim_package::MmdPackedPackage, MmdPackagePackError> {
    MmdPackagePacker::pack(
        MmdPackagePackInput {
            default_model_entry_id: 1,
            default_motion_entry_id: None,
            entries: vec![
                MmdPackagePackEntry {
                    id: 1,
                    path: "model/model.pmx".into(),
                    kind: MmdPackageEntryKind::Model,
                    codec: "pmx".into(),
                    compression: MmdPackagePackCompression::None,
                    decoded: b"PMX".to_vec(),
                    media_type: None,
                    motion: None,
                    texture: None,
                },
                MmdPackagePackEntry {
                    id: 2,
                    path: "texture/2.ktx2".into(),
                    kind: MmdPackageEntryKind::Texture,
                    codec: "ktx2-uastc-v1".into(),
                    compression,
                    decoded: payload,
                    media_type: None,
                    motion: None,
                    texture: Some(ktx2_texture_metadata()),
                },
            ],
            model_bindings: vec![MmdModelBinding {
                model_entry_id: 1,
                texture_bindings: vec![],
            }],
        },
        MmdPackageLimits::default(),
    )
}

fn manifest(entries: Vec<Value>) -> Value {
    json!({
        "schema": "mmdpack/1",
        "defaultModelEntryId": 1,
        "entries": entries,
        "modelBindings": [{"modelEntryId": 1, "textureBindings": []}],
    })
}

fn build_package(mut manifest: Value, payloads: &[&[u8]]) -> Vec<u8> {
    {
        let entries = manifest["entries"].as_array_mut().unwrap();
        assert_eq!(entries.len(), payloads.len());
        let mut offset = 0_u64;
        for (entry, payload) in entries.iter_mut().zip(payloads) {
            let cipher_size = payload.len() as u64 + 16;
            entry["offset"] = Value::from(offset);
            entry["cipherSize"] = Value::from(cipher_size);
            entry["decodedSize"] = Value::from(payload.len() as u64);
            offset += cipher_size;
        }
    }

    let manifest_plaintext = serde_json::to_vec(&manifest).unwrap();
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
    let entries = manifest["entries"].as_array().unwrap();
    for (entry, payload) in entries.iter().zip(payloads) {
        let id = entry["id"].as_u64().unwrap() as u32;
        let decoded_size = payload.len() as u64;
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce(id)),
                Payload {
                    msg: payload,
                    aad: &entry_aad(id, decoded_size),
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

fn open(
    manifest: Value,
    payloads: &[&[u8]],
    limits: MmdPackageLimits,
) -> Result<MmdPackage, MmdPackageError> {
    MmdPackage::open_bytes(Arc::from(build_package(manifest, payloads)), KEY, limits)
}

fn invalid_manifest_message(result: Result<MmdPackage, MmdPackageError>, needle: &str) {
    assert!(matches!(
        result,
        Err(MmdPackageError::InvalidManifest(message)) if message.contains(needle)
    ));
}

#[test]
fn accepts_valid_raw_uastc_and_ktx2_payload() {
    let raw = open(
        manifest(vec![
            model_entry(),
            texture_entry(2, "uastc-ldr-4x4-v1", raw_texture_metadata(16)),
        ]),
        &[b"PMX", &[0; 16]],
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert_eq!(raw.manifest().entries[1].kind, MmdPackageEntryKind::Texture);
    assert_eq!(raw.read_entry(2).unwrap(), [0; 16]);

    let ktx2 = open(
        manifest(vec![
            model_entry(),
            texture_entry(2, "ktx2-uastc-v1", ktx2_texture_metadata()),
        ]),
        &[b"PMX", &valid_ktx2_payload()],
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert_eq!(
        ktx2.manifest().entries[1].compression,
        MmdPackageCompression::None
    );
    assert_eq!(ktx2.read_entry(2).unwrap().len(), 176);
    assert_eq!(ktx2.read("texture/2.bin").unwrap().len(), 176);
}

#[test]
fn rejects_ktx2_payload_manifest_mismatch_and_truncation_on_read_and_verify() {
    let payload = valid_ktx2_payload();
    let mut mismatch = ktx2_texture_metadata();
    mismatch["width"] = 2.into();
    let package = open(
        manifest(vec![
            model_entry(),
            texture_entry(2, "ktx2-uastc-v1", mismatch),
        ]),
        &[b"PMX", &payload],
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("dimensions")
    ));
    assert!(matches!(
        package.read("texture/2.bin"),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("dimensions")
    ));
    assert!(matches!(
        package.verify(Default::default()),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("dimensions")
    ));

    let package = open(
        manifest(vec![
            model_entry(),
            texture_entry(2, "ktx2-uastc-v1", ktx2_texture_metadata()),
        ]),
        &[b"PMX", &payload[..payload.len() - 1]],
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("outside")
    ));
}

#[test]
fn rejects_ktx2_dfd_mismatch_and_bad_level_ranges() {
    let mut dfd_mismatch = valid_ktx2_payload();
    dfd_mismatch[104 + 14] = 1;
    let package = open_ktx2(&dfd_mismatch).unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("transfer function")
    ));

    let mut unaligned = valid_ktx2_payload();
    unaligned[80..88].copy_from_slice(&161_u64.to_le_bytes());
    let package = open_ktx2(&unaligned).unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("16-byte aligned")
    ));

    let mut overlap = valid_ktx2_zstd_payload(&[0; 16], 16);
    overlap[80..88].copy_from_slice(&104_u64.to_le_bytes());
    let package = open_ktx2(&overlap).unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("overlaps")
    ));
}

#[test]
fn rejects_ktx2_metadata_without_required_kvd_terminator() {
    let mut payload = valid_ktx2_payload_with_swizzle_kvd();
    payload[148 + 4 + 15] = b'x';
    let package = open_ktx2(&payload).unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("KTXswizzle must be NUL-terminated")
    ));
}

#[test]
fn validates_ktx2_internal_zstd_current_and_legacy_byte_plane_sizes() {
    for bytes_plane in [16, 0] {
        let payload = valid_ktx2_zstd_payload(&[0; 16], bytes_plane);
        let package = open_ktx2(&payload).unwrap();
        assert_eq!(package.read_entry(2).unwrap(), payload);
    }

    let payload = valid_ktx2_zstd_payload(&[0; 15], 16);
    let package = open_ktx2(&payload).unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(message)) if message.contains("expanded to 15 bytes")
    ));

    let mut payload = valid_ktx2_zstd_payload(&[0; 16], 16);
    payload[160] ^= 0xff;
    let package = open_ktx2(&payload).unwrap();
    assert!(matches!(
        package.read_entry(2),
        Err(MmdPackageError::InvalidKtx2(_))
    ));
}

#[test]
fn selects_effective_outer_compression_before_ktx2_validation() {
    let payload = incompressible_ktx2_zstd_payload();
    let packed = pack_ktx2(payload.clone(), MmdPackagePackCompression::AutoZstdV1).unwrap();
    let package = MmdPackage::open_bytes(
        Arc::from(packed.bytes()),
        *packed.key(),
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert_eq!(
        package.manifest().entries[1].compression,
        MmdPackageCompression::None
    );
    assert_eq!(package.read_entry(2).unwrap(), payload);

    let explicit = pack_ktx2(
        valid_ktx2_zstd_payload(&[0; 16], 16),
        MmdPackagePackCompression::None,
    )
    .unwrap();
    let package = MmdPackage::open_bytes(
        Arc::from(explicit.bytes()),
        *explicit.key(),
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert_eq!(
        package.manifest().entries[1].compression,
        MmdPackageCompression::None
    );
    assert!(package.read_entry(2).is_ok());

    let result = pack_ktx2(
        valid_ktx2_zstd_payload(&[0; 16], 16),
        MmdPackagePackCompression::AutoZstdV1,
    );
    assert!(matches!(
        result,
        Err(MmdPackagePackError::Package(
            MmdPackageError::InvalidKtx2(message)
        )) if message.contains("internal Zstd requires MMDPACK entry compression none")
    ));
}

#[test]
fn pack_rejects_ktx2_before_payload_is_consumed() {
    let mut payload = valid_ktx2_payload();
    payload[104 + 12] = 0;
    let result = MmdPackagePacker::pack(
        MmdPackagePackInput {
            default_model_entry_id: 1,
            default_motion_entry_id: None,
            entries: vec![
                MmdPackagePackEntry {
                    id: 1,
                    path: "model/model.pmx".into(),
                    kind: MmdPackageEntryKind::Model,
                    codec: "pmx".into(),
                    compression: MmdPackagePackCompression::None,
                    decoded: b"PMX".to_vec(),
                    media_type: None,
                    motion: None,
                    texture: None,
                },
                MmdPackagePackEntry {
                    id: 2,
                    path: "texture/2.ktx2".into(),
                    kind: MmdPackageEntryKind::Texture,
                    codec: "ktx2-uastc-v1".into(),
                    compression: MmdPackagePackCompression::None,
                    decoded: payload,
                    media_type: None,
                    motion: None,
                    texture: Some(ktx2_texture_metadata()),
                },
            ],
            model_bindings: vec![MmdModelBinding {
                model_entry_id: 1,
                texture_bindings: vec![],
            }],
        },
        MmdPackageLimits::default(),
    );
    assert!(matches!(
        result,
        Err(mmd_anim_package::MmdPackagePackError::Package(
            MmdPackageError::InvalidKtx2(message)
        )) if message.contains("color model")
    ));
}

#[test]
fn rejects_malformed_raw_uastc_mip_coverage() {
    invalid_manifest_message(
        open(
            manifest(vec![
                model_entry(),
                texture_entry(2, "uastc-ldr-4x4-v1", raw_texture_metadata(15)),
            ]),
            &[b"PMX", &[0; 16]],
            MmdPackageLimits::default(),
        ),
        "expected raw UASTC size",
    );
}

#[test]
fn enforces_known_kind_codec_metadata_rules() {
    let mut motion = json!({
        "id": 2, "path": "motion/scene.vmd", "kind": "motion", "codec": "vmd",
        "compression": "none", "offset": 0, "cipherSize": 19, "decodedSize": 3,
    });
    invalid_manifest_message(
        open(
            manifest(vec![model_entry(), motion.clone()]),
            &[b"PMX", b"VMD"],
            MmdPackageLimits::default(),
        ),
        "requires motion metadata",
    );
    motion["motion"] = json!({"role": "scene", "targetModelEntryId": 99});
    invalid_manifest_message(
        open(
            manifest(vec![model_entry(), motion]),
            &[b"PMX", b"VMD"],
            MmdPackageLimits::default(),
        ),
        "references missing entry",
    );

    let mut model_with_motion = model_entry();
    model_with_motion["motion"] = json!({"role": "scene"});
    invalid_manifest_message(
        open(
            manifest(vec![model_with_motion]),
            &[b"PMX"],
            MmdPackageLimits::default(),
        ),
        "non-motion entry",
    );

    let missing_texture = json!({
        "id": 2, "path": "texture/a.bin", "kind": "texture", "codec": "ktx2-uastc-v1",
        "compression": "none", "offset": 0, "cipherSize": 32, "decodedSize": 16,
    });
    invalid_manifest_message(
        open(
            manifest(vec![model_entry(), missing_texture]),
            &[b"PMX", &[0; 16]],
            MmdPackageLimits::default(),
        ),
        "requires texture metadata",
    );

    let mut model_with_texture = model_entry();
    model_with_texture["texture"] = ktx2_texture_metadata();
    invalid_manifest_message(
        open(
            manifest(vec![model_with_texture]),
            &[b"PMX"],
            MmdPackageLimits::default(),
        ),
        "non-texture entry",
    );

    let audio = json!({
        "id": 2, "path": "audio/a.bin", "kind": "audio", "codec": "opaque",
        "compression": "none", "offset": 0, "cipherSize": 19, "decodedSize": 3,
    });
    invalid_manifest_message(
        open(
            manifest(vec![model_entry(), audio]),
            &[b"PMX", b"AUD"],
            MmdPackageLimits::default(),
        ),
        "requires mediaType",
    );
}

#[test]
fn validates_license_references_and_metadata_shapes() {
    let mut declared = manifest(vec![model_entry()]);
    declared["licenses"] = json!({"model": {
        "name": "Example License",
        "url": "https://example.invalid/license",
        "text": "License text",
        "attribution": "Example Author",
        "notes": "Use as documented"
    }});
    declared["entries"][0]["licenseRefs"] = json!(["model"]);
    declared["credits"] = json!([{
        "subject": "model",
        "name": "Example Model",
        "author": "Example Author",
        "url": "https://example.invalid/credit"
    }]);
    open(declared, &[b"PMX"], MmdPackageLimits::default()).unwrap();

    let mut missing_name = manifest(vec![model_entry()]);
    missing_name["licenses"] = json!({"model": {"url": "https://example.invalid/license"}});
    invalid_manifest_message(
        open(missing_name, &[b"PMX"], MmdPackageLimits::default()),
        "missing required field \"name\"",
    );

    let mut wrong_license_field = manifest(vec![model_entry()]);
    wrong_license_field["licenses"] = json!({"model": {
        "name": "Example License",
        "url": 7
    }});
    invalid_manifest_message(
        open(wrong_license_field, &[b"PMX"], MmdPackageLimits::default()),
        "field \"url\" must be a string",
    );

    let mut missing = manifest(vec![model_entry()]);
    missing["entries"][0]["licenseRefs"] = json!(["missing"]);
    invalid_manifest_message(
        open(missing, &[b"PMX"], MmdPackageLimits::default()),
        "is not declared",
    );

    let mut wrong_shape = manifest(vec![model_entry()]);
    wrong_shape["licenses"] = json!({"model": "not-an-object"});
    invalid_manifest_message(
        open(wrong_shape, &[b"PMX"], MmdPackageLimits::default()),
        "must be an object",
    );

    let mut credits = manifest(vec![model_entry()]);
    credits["credits"] = json!(["not-an-object"]);
    invalid_manifest_message(
        open(credits, &[b"PMX"], MmdPackageLimits::default()),
        "credit entry must be an object",
    );

    let mut invalid_subject = manifest(vec![model_entry()]);
    invalid_subject["credits"] = json!([{"subject": "artist", "name": "Example"}]);
    invalid_manifest_message(
        open(invalid_subject, &[b"PMX"], MmdPackageLimits::default()),
        "unsupported credit subject",
    );

    let mut missing_credit_name = manifest(vec![model_entry()]);
    missing_credit_name["credits"] = json!([{"subject": "other"}]);
    invalid_manifest_message(
        open(missing_credit_name, &[b"PMX"], MmdPackageLimits::default()),
        "missing required field \"name\"",
    );

    let mut wrong_credit_field = manifest(vec![model_entry()]);
    wrong_credit_field["credits"] = json!([{
        "subject": "other",
        "name": "Example",
        "author": false
    }]);
    invalid_manifest_message(
        open(wrong_credit_field, &[b"PMX"], MmdPackageLimits::default()),
        "credit field \"author\" must be a string",
    );
}

#[test]
fn enforces_manifest_and_metadata_budgets() {
    let mut long_string = manifest(vec![model_entry()]);
    long_string["name"] = Value::String("x".repeat(64 * 1024 + 1));
    assert!(matches!(
        open(long_string, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "metadata string bytes",
            ..
        })
    ));

    let mut deep = manifest(vec![model_entry()]);
    let mut nested = Value::Bool(true);
    for _ in 0..20 {
        nested = json!({"nested": nested});
    }
    deep["extra"] = nested;
    assert!(matches!(
        open(deep, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "manifest depth",
            ..
        })
    ));

    let mut node_heavy = manifest(vec![model_entry()]);
    let mut node_values = Vec::with_capacity(4096);
    for _ in 0..4096 {
        let mut object = Map::new();
        for index in 0..16 {
            object.insert(format!("field{index}"), Value::Null);
        }
        node_values.push(Value::Object(object));
    }
    node_heavy["extra"] = Value::Array(node_values);
    assert!(matches!(
        open(node_heavy, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "manifest nodes",
            ..
        })
    ));

    let mut arrays = manifest(vec![model_entry()]);
    arrays["extra"] = Value::Array(vec![Value::Null; 8193]);
    assert!(matches!(
        open(arrays, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "manifest array items",
            ..
        })
    ));

    let mut object_heavy = manifest(vec![model_entry()]);
    let mut object_values = Map::new();
    for index in 0..8193 {
        object_values.insert(format!("field{index}"), Value::Null);
    }
    object_heavy["extra"] = Value::Object(object_values);
    assert!(matches!(
        open(object_heavy, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "manifest object fields",
            ..
        })
    ));

    let mut licenses = manifest(vec![model_entry()]);
    let mut license_values = Map::new();
    for index in 0..4097 {
        license_values.insert(format!("license{index}"), json!({}));
    }
    licenses["licenses"] = Value::Object(license_values);
    assert!(matches!(
        open(licenses, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "license count",
            ..
        })
    ));

    let mut credits = manifest(vec![model_entry()]);
    credits["credits"] = Value::Array(vec![json!({}); 4097]);
    assert!(matches!(
        open(credits, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "credit count",
            ..
        })
    ));

    let mut refs = manifest(vec![model_entry()]);
    let mut license_values = Map::new();
    let mut references = Vec::new();
    for index in 0..65 {
        let name = format!("license{index}");
        license_values.insert(name.clone(), json!({"name": name}));
        references.push(Value::String(name));
    }
    refs["licenses"] = Value::Object(license_values);
    refs["entries"][0]["licenseRefs"] = Value::Array(references);
    assert!(matches!(
        open(refs, &[b"PMX"], MmdPackageLimits::default()),
        Err(MmdPackageError::LimitExceeded {
            what: "license references",
            ..
        })
    ));
}

#[test]
fn enforces_texture_dimension_and_mip_limits() {
    let mut metadata = raw_texture_metadata(16);
    metadata["width"] = 16_385.into();
    assert!(matches!(
        open(
            manifest(vec![
                model_entry(),
                texture_entry(2, "uastc-ldr-4x4-v1", metadata)
            ]),
            &[b"PMX", &[0; 16]],
            MmdPackageLimits::default()
        ),
        Err(MmdPackageError::LimitExceeded {
            what: "texture dimension",
            ..
        })
    ));

    let mut mip_metadata = raw_texture_metadata(16);
    mip_metadata["mipCount"] = 33.into();
    assert!(matches!(
        open(
            manifest(vec![
                model_entry(),
                texture_entry(2, "uastc-ldr-4x4-v1", mip_metadata)
            ]),
            &[b"PMX", &[0; 16]],
            MmdPackageLimits::default()
        ),
        Err(MmdPackageError::LimitExceeded {
            what: "texture mip count",
            ..
        })
    ));
}
