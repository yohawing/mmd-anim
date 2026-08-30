use std::sync::Arc;

use mmd_anim_package::{
    MmdModelBinding, MmdPackage, MmdPackageCompression, MmdPackageEntryKind, MmdPackageError,
    MmdPackageLimits, MmdPackageMotionMetadata, MmdPackageMotionRole, MmdPackagePackCompression,
    MmdPackagePackEntry, MmdPackagePackError, MmdPackagePackInput, MmdPackagePacker,
};

fn model(compression: MmdPackagePackCompression, decoded: Vec<u8>) -> MmdPackagePackEntry {
    MmdPackagePackEntry {
        id: 1,
        path: "model/model.pmx".into(),
        kind: MmdPackageEntryKind::Model,
        codec: "pmx".into(),
        compression,
        decoded,
        media_type: None,
        motion: None,
        texture: None,
    }
}

fn input(entries: Vec<MmdPackagePackEntry>) -> MmdPackagePackInput {
    MmdPackagePackInput {
        default_model_entry_id: 1,
        default_motion_entry_id: None,
        entries,
        model_bindings: vec![MmdModelBinding {
            model_entry_id: 1,
            texture_bindings: vec![],
        }],
    }
}

fn reopen(packed: &mmd_anim_package::MmdPackedPackage) -> MmdPackage {
    MmdPackage::open_bytes(
        Arc::from(packed.bytes()),
        *packed.key(),
        MmdPackageLimits::default(),
    )
    .unwrap()
}

#[test]
fn packs_and_reads_model_motion_and_metadata() {
    let model_bytes = vec![0x4d; 32 * 1024];
    let motion_bytes = vec![0x56; 4096];
    let metadata_bytes = br#"{"name":"sample"}"#.to_vec();
    let mut package_input = input(vec![
        model(MmdPackagePackCompression::ZstdV1, model_bytes.clone()),
        MmdPackagePackEntry {
            id: 2,
            path: "motion/default.vmd".into(),
            kind: MmdPackageEntryKind::Motion,
            codec: "vmd".into(),
            compression: MmdPackagePackCompression::ZstdV1,
            decoded: motion_bytes.clone(),
            media_type: None,
            motion: Some(MmdPackageMotionMetadata {
                role: MmdPackageMotionRole::Model,
                target_model_entry_id: Some(1),
            }),
            texture: None,
        },
        MmdPackagePackEntry {
            id: 3,
            path: "meta/info.json".into(),
            kind: MmdPackageEntryKind::Metadata,
            codec: "json".into(),
            compression: MmdPackagePackCompression::None,
            decoded: metadata_bytes.clone(),
            media_type: Some("application/json".into()),
            motion: None,
            texture: None,
        },
    ]);
    package_input.default_motion_entry_id = Some(2);

    let packed = MmdPackagePacker::pack(package_input, MmdPackageLimits::default()).unwrap();
    let package = reopen(&packed);
    assert_eq!(package.read_entry(1).unwrap(), model_bytes);
    assert_eq!(package.read_entry(2).unwrap(), motion_bytes);
    assert_eq!(package.read("meta/info.json").unwrap(), metadata_bytes);
    assert_eq!(
        package.manifest().entries[0].compression,
        MmdPackageCompression::ZstdV1
    );
}

#[test]
fn generates_fresh_identity_key_and_ciphertext_each_time() {
    let source = input(vec![model(
        MmdPackagePackCompression::None,
        b"PMX".to_vec(),
    )]);
    let first = MmdPackagePacker::pack(source.clone(), MmdPackageLimits::default()).unwrap();
    let second = MmdPackagePacker::pack(source, MmdPackageLimits::default()).unwrap();
    assert_ne!(first.package_id(), second.package_id());
    assert_ne!(first.key(), second.key());
    assert_ne!(first.bytes(), second.bytes());
}

#[test]
fn auto_zstd_selects_only_smaller_output() {
    let tiny = MmdPackagePacker::pack(
        input(vec![model(MmdPackagePackCompression::AutoZstdV1, vec![1])]),
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert_eq!(
        reopen(&tiny).manifest().entries[0].compression,
        MmdPackageCompression::None
    );

    let repeated = MmdPackagePacker::pack(
        input(vec![model(
            MmdPackagePackCompression::AutoZstdV1,
            vec![7; 64 * 1024],
        )]),
        MmdPackageLimits::default(),
    )
    .unwrap();
    assert_eq!(
        reopen(&repeated).manifest().entries[0].compression,
        MmdPackageCompression::ZstdV1
    );
}

#[test]
fn rejects_invalid_input_and_limits() {
    let invalid = input(vec![MmdPackagePackEntry {
        path: "../model.pmx".into(),
        ..model(MmdPackagePackCompression::None, b"PMX".to_vec())
    }]);
    assert!(matches!(
        MmdPackagePacker::pack(invalid, MmdPackageLimits::default()),
        Err(MmdPackagePackError::Package(MmdPackageError::InvalidManifest(message)))
            if message.contains("path")
    ));

    let limited = MmdPackageLimits {
        max_entry_decoded_bytes: 2,
        ..MmdPackageLimits::default()
    };
    assert!(matches!(
        MmdPackagePacker::pack(
            input(vec![model(
                MmdPackagePackCompression::None,
                b"PMX".to_vec()
            )]),
            limited
        ),
        Err(MmdPackagePackError::Package(
            MmdPackageError::LimitExceeded {
                what: "entry decoded bytes",
                actual: 3,
                limit: 2
            }
        ))
    ));
}

#[test]
fn generated_entry_tampering_is_rejected() {
    let packed = MmdPackagePacker::pack(
        input(vec![model(
            MmdPackagePackCompression::None,
            b"PMX".to_vec(),
        )]),
        MmdPackageLimits::default(),
    )
    .unwrap();
    let (bytes, key) = packed.into_parts();
    let mut bytes = bytes.to_vec();
    *bytes.last_mut().unwrap() ^= 1;
    let package =
        MmdPackage::open_bytes(Arc::from(bytes), key, MmdPackageLimits::default()).unwrap();
    assert!(matches!(
        package.read_entry(1),
        Err(MmdPackageError::AuthenticationFailed("entry"))
    ));
}
