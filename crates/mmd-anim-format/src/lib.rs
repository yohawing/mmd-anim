//! Runtime-only PMX/VMD binary importer.
//!
//! This crate scans PMX/VMD sections and extracts only the runtime-required
//! data into `mmd-anim-runtime` arenas and clips. It does not retain a full
//! PMX/VMD object graph — mesh, material, texture, toon, display-frame UI,
//! rigid body, joint, and soft-body metadata are dropped during import.

pub mod error;
pub mod format;
pub mod nmd;
pub mod normalize;
pub mod pmd;
pub mod pmm;
pub mod pmx;
pub mod vmd;
pub mod vpd;
pub mod xfile;

pub use format::{MmdFormatKind, detect_mmd_format, sniff};
pub use nmd::{NmdParsedManifest, parse_nmd_manifest};
pub use normalize::normalize_vmd_name;
pub use pmd::{
    PmdParsedModel, PmdRuntimeImport, export_pmd_model, import_pmd_runtime, parse_pmd_model,
};
pub use pmm::{
    PmmParsedManifest, PmmSceneExportOptions, PmmSceneExportReport, export_pmm_manifest,
    export_pmm_scene_from_pmx_vmd, parse_pmm_manifest,
};
pub use pmx::{
    PmxBoneImport, PmxMaterialSplit, PmxMaterialSplitManifest, PmxMaterialSplitManifestMesh,
    PmxMaterialSplitMesh, PmxMaterialSplitMorphIndexMap, PmxMaterialSplitResult, PmxMorphNames,
    PmxParsedModel, PmxPartsBoneDescriptor, PmxPartsDescriptor, PmxPartsDisplayFrameDescriptor,
    PmxPartsDisplayFrameItem, PmxPartsGroupMorphOffset, PmxPartsIndexSizes, PmxPartsInput,
    PmxPartsJointDescriptor, PmxPartsMaterialDescriptor, PmxPartsMaterialFlags,
    PmxPartsMorphDescriptor, PmxPartsRigidBodyDescriptor, PmxPartsVertexMorphOffset, PmxRigSpec,
    PmxRigSpecBone, PmxRigSpecGrant, PmxRigSpecIkChain, PmxRigSpecIkLink, PmxRigSpecLocalAxis,
    PmxRuntimeImport, build_pmx_model_from_parts, export_pmx_model, import_pmx_model,
    import_pmx_runtime, parse_pmx_material_split, parse_pmx_model, parse_pmx_rig_spec,
    split_pmx_model_by_material, validate_pmx_export_model,
};
pub use vmd::{
    VmdCameraState, VmdClipBuildOptions, VmdIkEntry, VmdImportResult, VmdLightState,
    VmdParsedAnimation, VmdPropertyIkFrame, VmdSelfShadowState, build_clip_from_import,
    build_pair_clip, build_pair_clip_with_options, build_property_binding_with_ik_resolver,
    export_vmd_animation, import_vmd_motion, parse_vmd_animation, sample_vmd_camera_frames,
    sample_vmd_light_frames, sample_vmd_self_shadow_frames,
};
pub use vpd::{VpdParsedPose, export_vpd_pose, parse_vpd_pose};
pub use xfile::{AccessoryParsedManifest, export_accessory_manifest, parse_accessory_manifest};
