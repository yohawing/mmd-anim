use std::sync::Arc;

use mmd_anim_package::{
    MmdModelBinding, MmdPackage, MmdPackageEntryKind, MmdPackageError, MmdPackageLimits,
    MmdPackagePackCompression, MmdPackagePackEntry, MmdPackagePackInput, MmdPackagePacker,
    MmdTextureBinding,
};
use serde_json::json;

fn package_with_bindings(texture_bindings: Vec<MmdTextureBinding>) -> MmdPackage {
    let mut entries = vec![MmdPackagePackEntry {
        id: 1,
        path: "model/model.pmx".into(),
        kind: MmdPackageEntryKind::Model,
        codec: "pmx".into(),
        compression: MmdPackagePackCompression::None,
        decoded: b"PMX".to_vec(),
        media_type: None,
        motion: None,
        texture: None,
    }];
    if !texture_bindings.is_empty() {
        entries.push(MmdPackagePackEntry {
            id: 2,
            path: "texture/diffuse.bin".into(),
            kind: MmdPackageEntryKind::Texture,
            codec: "uastc-ldr-4x4-v1".into(),
            compression: MmdPackagePackCompression::None,
            decoded: vec![0; 16],
            media_type: None,
            motion: None,
            texture: Some(json!({
                "width": 1,
                "height": 1,
                "mipCount": 1,
                "colorSpace": "srgb",
                "usage": "color",
                "channelModel": "rgba",
                "swizzle": "rgba",
                "alphaMode": "straight",
                "origin": "top-left",
                "blockOrder": "row-major-top-left",
                "mips": [{"width": 1, "height": 1, "offset": 0, "size": 16}]
            })),
        });
    }

    let packed = MmdPackagePacker::pack(
        MmdPackagePackInput {
            default_model_entry_id: 1,
            default_motion_entry_id: None,
            entries,
            model_bindings: vec![MmdModelBinding {
                model_entry_id: 1,
                texture_bindings,
            }],
        },
        MmdPackageLimits::default(),
    )
    .unwrap();
    MmdPackage::open_bytes(
        Arc::from(packed.bytes()),
        *packed.key(),
        MmdPackageLimits::default(),
    )
    .unwrap()
}

#[test]
fn accepts_texture_binding_inside_pmx_texture_table() {
    let package = package_with_bindings(vec![MmdTextureBinding {
        texture_index: 0,
        entry_id: 2,
    }]);

    package
        .validate_texture_bindings_against_table(1, 1)
        .unwrap();
}

#[test]
fn rejects_texture_binding_outside_pmx_texture_table() {
    let package = package_with_bindings(vec![MmdTextureBinding {
        texture_index: 1,
        entry_id: 2,
    }]);

    assert!(matches!(
        package.validate_texture_bindings_against_table(1, 1),
        Err(MmdPackageError::TextureIndexOutOfRange {
            model_entry_id: 1,
            texture_index: 1,
            texture_table_len: 1,
        })
    ));
}

#[test]
fn does_not_require_every_pmx_texture_slot_to_be_bound() {
    let package = package_with_bindings(vec![MmdTextureBinding {
        texture_index: 0,
        entry_id: 2,
    }]);

    package
        .validate_texture_bindings_against_table(1, 3)
        .unwrap();
}

#[test]
fn empty_bindings_allow_zero_pmx_textures_for_builtin_toon_slots() {
    let package = package_with_bindings(Vec::new());

    package
        .validate_texture_bindings_against_table(1, 0)
        .unwrap();
}
