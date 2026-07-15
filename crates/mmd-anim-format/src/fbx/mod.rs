#![cfg(feature = "fbx")]

use std::{
    collections::{BTreeMap, HashSet},
    io::{Cursor, Seek, Write},
    sync::Arc,
};

use fbxcel::{
    low::{FbxVersion, v7400::ArrayAttributeEncoding},
    writer::v7400::binary::{AttributesWriter, FbxFooter, Writer},
};
use glam::Mat4;
use mmd_anim_runtime::{
    AnimationClip, BoneIndex, DensePoseSequenceView, ModelArena, MorphIndex, PoseReductionError,
    PoseReductionReport, ReducedBoneKey, ReducedMorphKey, ReducedPoseSequence, ReductionTarget,
    ReductionTimings, ReductionTolerances, ReductionWorkStats, RuntimeInstance, SkeletonSnapshot,
    reduce_dense_pose_sequence,
};

use crate::{
    pmx::{PmxParsedBone, PmxParsedMaterial, PmxParsedModel, PmxParsedMorph},
    vmd::{VmdParsedAnimation, VmdParsedBoneFrame, VmdParsedMorphFrame},
};

mod skin_diff;
pub use skin_diff::{
    FbxSkinBoneDiff, FbxSkinClusterData, FbxSkinDiffOptions, FbxSkinDiffReport, FbxSkinReadError,
    FbxSkinVertexWeight, FbxSkinWeightDiff, diff_fbx_skin_clusters, read_fbx_skin_clusters,
};

const ROOT_NODE_ID: i64 = 0;
const DOCUMENT_ID: i64 = 100;
const MODEL_ID: i64 = 200;
const GEOMETRY_ID: i64 = 300;
const MATERIAL_ID_BASE: i64 = 1000;
const TEXTURE_ID_BASE: i64 = 5000;
const VIDEO_ID_BASE: i64 = 7000;
const BONE_MODEL_ID_BASE: i64 = 10_000;
const BONE_ATTR_ID_BASE: i64 = 20_000;
const SKIN_ID: i64 = 30_000;
const CLUSTER_ID_BASE: i64 = 40_000;
const POSE_ID: i64 = 50_000;
const BLEND_SHAPE_ID_BASE: i64 = 90_000;
const BLEND_SHAPE_CHANNEL_ID_BASE: i64 = 91_000;
const SHAPE_GEOMETRY_ID_BASE: i64 = 92_000;
const ANIM_STACK_ID: i64 = 60_000;
const ANIM_LAYER_ID: i64 = 60_001;
const ANIM_CURVENODE_ROT_BASE: i64 = 70_000;
const ANIM_CURVENODE_TRANS_BASE: i64 = 80_000;
const ANIM_CURVENODE_MORPH_BASE: i64 = 95_000;
const ANIM_CURVE_BASE: i64 = 100_000;
const ANIM_CURVE_MORPH_BASE: i64 = 110_000;
const FBX_TIME_ONE_SECOND: i64 = 46_186_158_000;
const FBX_FRAME_DURATION: i64 = FBX_TIME_ONE_SECOND / 30;
const STATIC_BONE_EPSILON: f32 = 1.0e-5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FbxBoneNamePolicy {
    #[default]
    LegacyHex,
    Readable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FbxBoneNameSource {
    LegacyHex,
    PmxEnglish,
    AsciiName,
    StandardDictionary,
    HexFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FbxBoneNameMapEntry {
    pub index: usize,
    pub pmx_name: String,
    pub pmx_english_name: String,
    pub fbx_name: String,
    pub source: FbxBoneNameSource,
    pub collision_suffix: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FbxExportOptions {
    pub model_name: String,
    pub flip_z: bool,
    pub diffuse_texture_paths: Vec<String>,
    pub bones_only: bool,
    pub bone_name_policy: FbxBoneNamePolicy,
}

impl Default for FbxExportOptions {
    fn default() -> Self {
        Self {
            model_name: "PMX Model".to_owned(),
            flip_z: true,
            diffuse_texture_paths: Vec::new(),
            bones_only: false,
            bone_name_policy: FbxBoneNamePolicy::default(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FbxExportError {
    #[error("PMX position buffer length must be divisible by 3, got {0}")]
    InvalidPositionBuffer(usize),
    #[error("PMX normal buffer length must be divisible by 3, got {0}")]
    InvalidNormalBuffer(usize),
    #[error("PMX UV buffer length must be divisible by 2, got {0}")]
    InvalidUvBuffer(usize),
    #[error("PMX index buffer length must be divisible by 3, got {0}")]
    InvalidIndexBuffer(usize),
    #[error("PMX index {index} references missing vertex {vertex} (vertex count {vertex_count})")]
    IndexOutOfRange {
        index: usize,
        vertex: u32,
        vertex_count: usize,
    },
    #[error("FBX writer error: {0}")]
    Writer(#[from] fbxcel::writer::v7400::binary::Error),
    #[error("FBX pose source failed at frame {frame}: {message}")]
    PoseSource { frame: u32, message: String },
    #[error(
        "FBX pose source returned {actual} bones at frame {frame}, expected at least {expected}"
    )]
    PoseBoneCount {
        frame: u32,
        expected: usize,
        actual: usize,
    },
    #[error("reduced pose must use the DccCubic target")]
    ReducedPoseTarget,
    #[error("reduced pose model identity or skeleton/morph counts do not match the FBX model")]
    ReducedPoseBinding,
    #[error("reduced pose frame {frame} cannot be represented as FBX time")]
    ReducedPoseTime { frame: f32 },
    #[error("pose reduction failed: {0}")]
    PoseReduction(#[from] PoseReductionError),
}

#[derive(Debug, Clone)]
pub struct FbxReducedPoseExport {
    pub bytes: Vec<u8>,
    pub report: PoseReductionReport,
    pub work_stats: ReductionWorkStats,
    pub timings: ReductionTimings,
}

impl PartialEq for FbxReducedPoseExport {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
            && self.report == other.report
            && self.work_stats == other.work_stats
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnityReducedPoseBindings {
    pub model_identity: u64,
    pub bone_paths: Vec<String>,
    pub morph_bindings: Vec<Option<UnityMorphBinding>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnityMorphBinding {
    pub path: String,
    pub property: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnityAnimationClipDto {
    pub frame_rate: f32,
    pub curves: Vec<UnityAnimationCurveDto>,
    pub source_key_count: usize,
    pub reduced_key_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnityAnimationCurveDto {
    pub path: String,
    pub property: String,
    pub keys: Vec<UnityAnimationKeyDto>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnityAnimationKeyDto {
    pub time_seconds: f32,
    pub value: f32,
    pub in_tangent: f32,
    pub out_tangent: f32,
}

/// Supplies one evaluated set of runtime bone world matrices per FBX frame.
pub trait FbxPoseSource {
    fn world_matrices(&mut self, frame: u32) -> Result<&[Mat4], String>;
    fn morph_weights(&self) -> Option<&[f32]> {
        None
    }
}

struct RuntimeBakePoseSource<'a> {
    runtime: RuntimeInstance,
    clip: &'a AnimationClip,
}

impl FbxPoseSource for RuntimeBakePoseSource<'_> {
    fn world_matrices(&mut self, frame: u32) -> Result<&[Mat4], String> {
        self.runtime.evaluate_clip_frame(self.clip, frame as f32);
        Ok(self.runtime.world_matrices())
    }

    fn morph_weights(&self) -> Option<&[f32]> {
        Some(self.runtime.morph_weights())
    }
}

/// Exports a PMX model to FBX 7.4 binary.
///
/// When `vmd` is provided this uses the raw VMD bone-frame path. That path does
/// not run the runtime evaluator, so IK, append/grant transforms, fixed-axis
/// constraints, and VMD per-axis translation Bezier curves are not represented
/// with the same semantics as `convert-fbx`.
///
/// For reference exports that should match the CLI path, prefer
/// [`export_pmx_fbx_binary_with_runtime_bake`].
pub fn export_pmx_fbx_binary(
    model: &PmxParsedModel,
    vmd: Option<&VmdParsedAnimation>,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let animation = vmd.map(|vmd| FbxAnimationData::from_vmd(model, vmd, options));
    export_pmx_fbx_binary_with_animation(model, animation, options)
}

/// Exports a PMX model to FBX 7.4 binary with runtime-baked bone animation.
///
/// This is the preferred path for DCC reference FBX output because it samples
/// the same runtime evaluation surface used by `mmd-anim convert-fbx`.
pub fn export_pmx_fbx_binary_with_runtime_bake(
    model: &PmxParsedModel,
    runtime_model: Arc<ModelArena>,
    clip: &AnimationClip,
    last_frame: u32,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let mut pose_source = RuntimeBakePoseSource {
        runtime: RuntimeInstance::new(Arc::clone(&runtime_model)),
        clip,
    };
    export_pmx_fbx_binary_with_pose_source(
        model,
        runtime_model,
        clip,
        last_frame,
        options,
        &mut pose_source,
    )
}

pub fn export_pmx_fbx_binary_with_reduced_runtime_bake(
    model: &PmxParsedModel,
    runtime_model: Arc<ModelArena>,
    clip: &AnimationClip,
    last_frame: u32,
    tolerances: ReductionTolerances,
    options: &FbxExportOptions,
) -> Result<FbxReducedPoseExport, FbxExportError> {
    let mut pose_source = RuntimeBakePoseSource {
        runtime: RuntimeInstance::new(Arc::clone(&runtime_model)),
        clip,
    };
    export_pmx_fbx_binary_with_reduced_pose_source(
        model,
        runtime_model,
        last_frame,
        0,
        tolerances,
        options,
        &mut pose_source,
    )
}

/// Exports runtime-baked animation using caller-supplied world matrices.
///
/// The source is called exactly once for every integer frame in `0..=last_frame`.
/// This keeps FBX conversion independent from a particular physics backend.
pub fn export_pmx_fbx_binary_with_pose_source<P>(
    model: &PmxParsedModel,
    runtime_model: Arc<ModelArena>,
    clip: &AnimationClip,
    last_frame: u32,
    options: &FbxExportOptions,
    pose_source: &mut P,
) -> Result<Vec<u8>, FbxExportError>
where
    P: FbxPoseSource,
{
    let animation = Some(FbxAnimationData::from_pose_source(
        model,
        runtime_model,
        clip,
        last_frame,
        options,
        pose_source,
    )?);
    export_pmx_fbx_binary_with_animation(model, animation, options)
}

/// Collects a dense final-pose source, reduces it with `DccCubic`, and exports sparse FBX curves.
pub fn export_pmx_fbx_binary_with_reduced_pose_source<P>(
    model: &PmxParsedModel,
    runtime_model: Arc<ModelArena>,
    last_frame: u32,
    model_identity: u64,
    tolerances: ReductionTolerances,
    options: &FbxExportOptions,
    pose_source: &mut P,
) -> Result<FbxReducedPoseExport, FbxExportError>
where
    P: FbxPoseSource,
{
    let frame_count = last_frame as usize + 1;
    let bone_count = runtime_model.bone_count();
    let morph_count = runtime_model.morph_count() as usize;
    let mut world_matrices = Vec::with_capacity(frame_count.saturating_mul(bone_count));
    let mut morph_weights = Vec::with_capacity(frame_count.saturating_mul(morph_count));
    for frame in 0..=last_frame {
        let matrices = pose_source
            .world_matrices(frame)
            .map_err(|message| FbxExportError::PoseSource { frame, message })?;
        if matrices.len() < bone_count {
            return Err(FbxExportError::PoseBoneCount {
                frame,
                expected: bone_count,
                actual: matrices.len(),
            });
        }
        world_matrices.extend_from_slice(&matrices[..bone_count]);
        let weights = pose_source
            .morph_weights()
            .ok_or_else(|| FbxExportError::PoseSource {
                frame,
                message: "reduced pose source must expose morph weights".to_owned(),
            })?;
        if weights.len() < morph_count {
            return Err(FbxExportError::PoseSource {
                frame,
                message: format!(
                    "reduced pose source returned {} morphs, expected {morph_count}",
                    weights.len()
                ),
            });
        }
        morph_weights.extend_from_slice(&weights[..morph_count]);
    }
    let snapshot = SkeletonSnapshot::from_model(&runtime_model, model_identity)?;
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(
            &world_matrices,
            &morph_weights,
            frame_count,
            bone_count,
            morph_count,
            0.0,
            1.0,
        )?,
        snapshot,
        tolerances,
        ReductionTarget::DccCubic,
    )?;
    let report = reduced.report();
    let work_stats = reduced.work_stats().clone();
    let timings = reduced.timings();
    let bytes = export_pmx_fbx_binary_with_reduced_pose(model, &reduced, model_identity, options)?;
    Ok(FbxReducedPoseExport {
        bytes,
        report,
        work_stats,
        timings,
    })
}

/// Exports a target-native sparse DCC pose result as FBX user/broken tangent curves.
pub fn export_pmx_fbx_binary_with_reduced_pose(
    model: &PmxParsedModel,
    reduced: &ReducedPoseSequence,
    model_identity: u64,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let animation = FbxAnimationData::from_reduced_pose(model, reduced, model_identity, options)?;
    export_pmx_fbx_binary_with_animation(model, Some(animation), options)
}

/// Projects a sparse DCC pose result into flat Unity `AnimationCurve` bindings.
///
/// Euler XYZ curves are emitted deliberately; quaternion-component and FBX-import
/// resampling profiles are separate concerns and are not implied by this DTO.
pub fn reduced_pose_to_unity_animation_clip(
    reduced: &ReducedPoseSequence,
    bindings: &UnityReducedPoseBindings,
    flip_z: bool,
) -> Result<UnityAnimationClipDto, FbxExportError> {
    validate_reduced_pose(
        reduced,
        bindings.model_identity,
        bindings.bone_paths.len(),
        bindings.morph_bindings.len(),
    )?;
    let mut curves = Vec::new();
    let position_sign = [1.0, 1.0, if flip_z { -1.0 } else { 1.0 }];
    let rotation_sign = [
        if flip_z { -1.0 } else { 1.0 },
        if flip_z { -1.0 } else { 1.0 },
        1.0,
    ];
    for (bone, track) in reduced.bone_tracks().iter().enumerate() {
        for (axis, sign) in position_sign.iter().copied().enumerate() {
            let values = track
                .keys()
                .iter()
                .map(|key| key.translation.to_array()[axis] * sign)
                .collect::<Vec<_>>();
            let tangents = bone_segment_tangents(track.keys(), axis, false, sign);
            curves.push(UnityAnimationCurveDto {
                path: bindings.bone_paths[bone].clone(),
                property: format!("localPosition.{}", axis_name(axis)),
                keys: unity_keys(reduced, track.keys(), &values, &tangents)?,
            });
        }
        let mut previous = None;
        let euler_values = track
            .keys()
            .iter()
            .map(|key| {
                let q = key.rotation;
                let converted = convert_quat_to_fbx(
                    [q.x as f64, q.y as f64, q.z as f64, q.w as f64],
                    &FbxExportOptions {
                        flip_z,
                        ..FbxExportOptions::default()
                    },
                );
                let euler = quat_to_euler_xyz(converted);
                let filtered = previous
                    .map(|value| euler_filter(euler, value))
                    .unwrap_or(euler);
                previous = Some(filtered);
                filtered
            })
            .collect::<Vec<_>>();
        for (axis, sign) in rotation_sign.iter().copied().enumerate() {
            let values = euler_values
                .iter()
                .map(|value| value[axis] as f32)
                .collect::<Vec<_>>();
            let tangents = bone_segment_tangents(track.keys(), axis, true, sign);
            curves.push(UnityAnimationCurveDto {
                path: bindings.bone_paths[bone].clone(),
                property: format!("localEulerAnglesRaw.{}", axis_name(axis)),
                keys: unity_keys(reduced, track.keys(), &values, &tangents)?,
            });
        }
    }
    for (morph, track) in reduced.morph_tracks().iter().enumerate() {
        let Some(binding) = &bindings.morph_bindings[morph] else {
            continue;
        };
        let values = track
            .keys()
            .iter()
            .map(|key| key.weight * 100.0)
            .collect::<Vec<_>>();
        let tangents = morph_segment_tangents(track.keys());
        curves.push(UnityAnimationCurveDto {
            path: binding.path.clone(),
            property: binding.property.clone(),
            keys: unity_morph_keys(reduced, track.keys(), &values, &tangents)?,
        });
    }
    let report = reduced.report();
    Ok(UnityAnimationClipDto {
        frame_rate: 30.0,
        curves,
        source_key_count: report.source_bone_key_count + report.source_morph_key_count,
        reduced_key_count: report.reduced_bone_key_count + report.reduced_morph_key_count,
    })
}

fn export_pmx_fbx_binary_with_animation(
    model: &PmxParsedModel,
    animation: Option<FbxAnimationData>,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let mesh = if options.bones_only {
        None
    } else {
        Some(MeshData::from_pmx(model, options)?)
    };
    let sink = Cursor::new(Vec::new());
    let mut writer = Writer::new(sink, FbxVersion::V7_4)?;

    write_fbx_header_extension(&mut writer)?;
    write_top_level_fields(&mut writer)?;
    write_global_settings(&mut writer, animation.as_ref())?;
    write_documents(&mut writer, animation.is_some())?;
    write_references(&mut writer)?;
    let vertex_morph_count = if options.bones_only {
        0
    } else {
        vertex_morph_count(model)
    };
    write_definitions(
        &mut writer,
        if options.bones_only {
            0
        } else {
            model.materials.len()
        },
        if options.bones_only {
            0
        } else {
            diffuse_texture_records(model, options).len()
        },
        model.skeleton.bones.len(),
        vertex_morph_count,
        animation.as_ref(),
        !options.bones_only,
    )?;
    write_objects(
        &mut writer,
        model,
        options,
        mesh.as_ref(),
        animation.as_ref(),
    )?;
    write_connections(
        &mut writer,
        model,
        options,
        &model.skeleton.bones,
        vertex_morph_count,
        animation.as_ref(),
        !options.bones_only,
    )?;
    if let Some(animation) = animation.as_ref() {
        write_takes(&mut writer, animation.last_time())?;
    }

    let footer_code: [u8; 16] = [
        0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26,
        0x7e,
    ];
    let footer = FbxFooter {
        unknown1: Some(&footer_code),
        ..Default::default()
    };
    let sink = writer.finalize_and_flush(&footer)?;
    Ok(sink.into_inner())
}

/// Compatibility wrapper around [`export_pmx_fbx_binary`].
///
/// Passing `Some(vmd)` uses the raw VMD bone-frame path. Prefer
/// [`export_fbx_with_runtime_bake`] for CLI-equivalent reference output.
pub fn export_fbx(
    model: &PmxParsedModel,
    vmd: Option<&VmdParsedAnimation>,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    export_pmx_fbx_binary(model, vmd, options)
}

/// Compatibility wrapper around [`export_pmx_fbx_binary_with_runtime_bake`].
pub fn export_fbx_with_runtime_bake(
    model: &PmxParsedModel,
    runtime_model: Arc<ModelArena>,
    clip: &AnimationClip,
    last_frame: u32,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    export_pmx_fbx_binary_with_runtime_bake(model, runtime_model, clip, last_frame, options)
}

struct MeshData {
    vertices: Vec<f64>,
    normals: Vec<f64>,
    uvs: Vec<f64>,
    polygon_vertex_indices: Vec<i32>,
    polygon_uv_indices: Vec<i32>,
    polygon_material_indices: Vec<i32>,
}

struct VertexMorphExport {
    name: String,
    indexes: Vec<i32>,
    vertices: Vec<f64>,
}

impl MeshData {
    fn from_pmx(
        model: &PmxParsedModel,
        options: &FbxExportOptions,
    ) -> Result<Self, FbxExportError> {
        let vertex_count = model.geometry.positions.len() / 3;
        if !model.geometry.positions.len().is_multiple_of(3) {
            return Err(FbxExportError::InvalidPositionBuffer(
                model.geometry.positions.len(),
            ));
        }
        if !model.geometry.normals.len().is_multiple_of(3) {
            return Err(FbxExportError::InvalidNormalBuffer(
                model.geometry.normals.len(),
            ));
        }
        if !model.geometry.uvs.len().is_multiple_of(2) {
            return Err(FbxExportError::InvalidUvBuffer(model.geometry.uvs.len()));
        }
        if !model.geometry.indices.len().is_multiple_of(3) {
            return Err(FbxExportError::InvalidIndexBuffer(
                model.geometry.indices.len(),
            ));
        }

        let z_sign = if options.flip_z { -1.0 } else { 1.0 };
        let mut vertices = Vec::with_capacity(model.geometry.positions.len());
        for position in model.geometry.positions.chunks_exact(3) {
            vertices.push(position[0] as f64);
            vertices.push(position[1] as f64);
            vertices.push(position[2] as f64 * z_sign);
        }

        let mut normals = Vec::with_capacity(model.geometry.normals.len());
        for normal in model.geometry.normals.chunks_exact(3) {
            normals.push(normal[0] as f64);
            normals.push(normal[1] as f64);
            normals.push(normal[2] as f64 * z_sign);
        }

        let mut uvs = Vec::with_capacity(model.geometry.uvs.len());
        for uv in model.geometry.uvs.chunks_exact(2) {
            uvs.push(uv[0] as f64);
            uvs.push((1.0 - uv[1]) as f64);
        }

        let mut polygon_vertex_indices = Vec::with_capacity(model.geometry.indices.len());
        let mut polygon_uv_indices = Vec::with_capacity(model.geometry.indices.len());
        let mut polygon_material_indices =
            Vec::with_capacity(model.geometry.indices.len().saturating_div(3));
        for (triangle_index, triangle) in model.geometry.indices.chunks_exact(3).enumerate() {
            for (local_index, vertex_index) in triangle_indices_for_handedness(triangle, options)
                .into_iter()
                .enumerate()
            {
                if vertex_index as usize >= vertex_count {
                    return Err(FbxExportError::IndexOutOfRange {
                        index: triangle_index * 3 + local_index,
                        vertex: vertex_index,
                        vertex_count,
                    });
                }
                let raw = vertex_index as i32;
                if local_index == 2 {
                    polygon_vertex_indices.push(-raw - 1);
                } else {
                    polygon_vertex_indices.push(raw);
                }
                polygon_uv_indices.push(raw);
            }
            polygon_material_indices.push(material_index_for_triangle(model, triangle_index));
        }

        Ok(Self {
            vertices,
            normals,
            uvs,
            polygon_vertex_indices,
            polygon_uv_indices,
            polygon_material_indices,
        })
    }
}

fn vertex_morph_count(model: &PmxParsedModel) -> usize {
    model
        .morphs
        .iter()
        .filter(|morph| is_exportable_vertex_morph(morph))
        .count()
}

fn is_exportable_vertex_morph(morph: &PmxParsedMorph) -> bool {
    morph.kind == "vertex" && !morph.vertex_offsets.is_empty()
}

fn collect_vertex_morph_exports(
    model: &PmxParsedModel,
    mesh: &MeshData,
    options: &FbxExportOptions,
) -> Vec<VertexMorphExport> {
    let vertex_count = mesh.vertices.len() / 3;
    let z_sign = if options.flip_z { -1.0 } else { 1.0 };
    let mut exports = Vec::new();

    for morph in model
        .morphs
        .iter()
        .filter(|morph| is_exportable_vertex_morph(morph))
    {
        let mut vertex_deltas = BTreeMap::<usize, [f64; 3]>::new();
        for offset in &morph.vertex_offsets {
            let vertex_index = offset.vertex_index as usize;
            if vertex_index >= vertex_count {
                continue;
            }
            let delta = vertex_deltas.entry(vertex_index).or_insert([0.0; 3]);
            delta[0] += offset.position[0] as f64;
            delta[1] += offset.position[1] as f64;
            delta[2] += offset.position[2] as f64 * z_sign;
        }

        let morph_name = if morph.english_name.is_empty() {
            morph.name.as_str()
        } else {
            morph.english_name.as_str()
        };
        let mut indexes = Vec::with_capacity(vertex_deltas.len());
        let mut vertices = Vec::with_capacity(vertex_deltas.len() * 3);
        for (vertex_index, delta) in vertex_deltas {
            indexes.push(vertex_index as i32);
            vertices.extend_from_slice(&delta);
        }
        exports.push(VertexMorphExport {
            name: japanese_to_ascii(morph_name),
            indexes,
            vertices,
        });
    }

    exports
}

struct FbxAnimationData {
    max_frame: u32,
    tracks: Vec<FbxAnimationTrack>,
    morph_tracks: Vec<FbxMorphAnimationTrack>,
}

struct FbxAnimationTrack {
    bone_index: usize,
    frame_times: Vec<i64>,
    rotation_values: [Vec<f32>; 3],
    translation_values: [Vec<f32>; 3],
    rotation_attributes: [Option<FbxCurveAttributes>; 3],
    translation_attributes: [Option<FbxCurveAttributes>; 3],
}

struct FbxMorphAnimationTrack {
    export_index: usize,
    frame_times: Vec<i64>,
    weight_values: Vec<f32>,
    attributes: Option<FbxCurveAttributes>,
}

#[derive(Clone)]
struct FbxCurveAttributes {
    flags: Vec<i32>,
    data: Vec<f32>,
    ref_counts: Vec<i32>,
}

struct RuntimeBakeTrack {
    bone_index: usize,
    rotation_values: [Vec<f32>; 3],
    translation_values: [Vec<f32>; 3],
    previous_euler: Option<[f64; 3]>,
    changed_from_rest: bool,
}

impl RuntimeBakeTrack {
    fn new(bone_index: usize, frame_count: usize) -> Self {
        Self {
            bone_index,
            rotation_values: [
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
            ],
            translation_values: [
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
            ],
            previous_euler: None,
            changed_from_rest: false,
        }
    }
}

struct BoneTrack {
    bone_index: usize,
    keyframes: Vec<SortedKeyframe>,
}

#[derive(Clone)]
struct SortedKeyframe {
    frame: u32,
    translation: [f64; 3],
    rotation: [f64; 4],
    rot_interp: [u8; 4],
}

impl FbxAnimationData {
    fn from_reduced_pose(
        model: &PmxParsedModel,
        reduced: &ReducedPoseSequence,
        model_identity: u64,
        options: &FbxExportOptions,
    ) -> Result<Self, FbxExportError> {
        validate_reduced_pose(
            reduced,
            model_identity,
            model.skeleton.bones.len(),
            model.morphs.len(),
        )?;
        let last_frame = *reduced.sample_frames().last().unwrap();
        if last_frame < 0.0 || last_frame > u32::MAX as f32 {
            return Err(FbxExportError::ReducedPoseTime { frame: last_frame });
        }
        let position_sign = [1.0, 1.0, if options.flip_z { -1.0 } else { 1.0 }];
        let rotation_sign = [
            if options.flip_z { -1.0 } else { 1.0 },
            if options.flip_z { -1.0 } else { 1.0 },
            1.0,
        ];
        let mut tracks = Vec::new();
        for (bone_index, track) in reduced.bone_tracks().iter().enumerate() {
            let rest_translation = reduced.snapshot().rest_local_translations()[bone_index];
            let rest_rotation = reduced.snapshot().rest_local_rotations()[bone_index];
            if track.keys().iter().all(|key| {
                key.translation.distance(rest_translation) <= STATIC_BONE_EPSILON
                    && key.rotation.dot(rest_rotation).abs() >= 1.0 - STATIC_BONE_EPSILON
            }) {
                continue;
            }
            let frame_times = track
                .keys()
                .iter()
                .map(|key| reduced_key_time(reduced, key.sample_index))
                .collect::<Result<Vec<_>, _>>()?;
            let mut translation_values: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::new());
            let mut rotation_values: [Vec<f32>; 3] = std::array::from_fn(|_| Vec::new());
            let mut previous_euler = None;
            for key in track.keys() {
                for axis in 0..3 {
                    translation_values[axis]
                        .push(key.translation.to_array()[axis] * position_sign[axis]);
                }
                let q = key.rotation;
                let converted =
                    convert_quat_to_fbx([q.x as f64, q.y as f64, q.z as f64, q.w as f64], options);
                let euler = quat_to_euler_xyz(converted);
                let filtered = previous_euler
                    .map(|previous| euler_filter(euler, previous))
                    .unwrap_or(euler);
                previous_euler = Some(filtered);
                for axis in 0..3 {
                    rotation_values[axis].push(filtered[axis] as f32);
                }
            }
            let rotation_attributes = std::array::from_fn(|axis| {
                Some(curve_attributes(&bone_segment_tangents(
                    track.keys(),
                    axis,
                    true,
                    rotation_sign[axis],
                )))
            });
            let translation_attributes = std::array::from_fn(|axis| {
                Some(curve_attributes(&bone_segment_tangents(
                    track.keys(),
                    axis,
                    false,
                    position_sign[axis],
                )))
            });
            tracks.push(FbxAnimationTrack {
                bone_index,
                frame_times,
                rotation_values,
                translation_values,
                rotation_attributes,
                translation_attributes,
            });
        }

        let exported_morphs = exported_vertex_morph_indices(model);
        let mut morph_tracks = Vec::new();
        if !options.bones_only {
            for (morph_index, track) in reduced.morph_tracks().iter().enumerate() {
                let Some(export_index) = exported_morphs
                    .iter()
                    .position(|candidate| *candidate == morph_index)
                else {
                    continue;
                };
                if !track
                    .keys()
                    .iter()
                    .any(|key| key.weight.abs() > STATIC_BONE_EPSILON)
                {
                    continue;
                }
                let frame_times = track
                    .keys()
                    .iter()
                    .map(|key| reduced_key_time(reduced, key.sample_index))
                    .collect::<Result<Vec<_>, _>>()?;
                let weight_values = track.keys().iter().map(|key| key.weight * 100.0).collect();
                let attributes = Some(curve_attributes(&morph_segment_tangents(track.keys())));
                morph_tracks.push(FbxMorphAnimationTrack {
                    export_index,
                    frame_times,
                    weight_values,
                    attributes,
                });
            }
        }
        Ok(Self {
            max_frame: last_frame.ceil() as u32,
            tracks,
            morph_tracks,
        })
    }

    fn from_pose_source<P>(
        model: &PmxParsedModel,
        runtime_model: Arc<ModelArena>,
        clip: &AnimationClip,
        max_frame: u32,
        options: &FbxExportOptions,
        pose_source: &mut P,
    ) -> Result<Self, FbxExportError>
    where
        P: FbxPoseSource,
    {
        let bone_count = model.skeleton.bones.len().min(runtime_model.bone_count());
        let required_matrix_count = (0..bone_count)
            .flat_map(|index| {
                let bone = BoneIndex(index as u32);
                [
                    index,
                    runtime_model
                        .parent_index(bone)
                        .map(BoneIndex::as_usize)
                        .unwrap_or(index),
                ]
            })
            .max()
            .map(|index| index + 1)
            .unwrap_or(0);
        let frame_count = max_frame as usize + 1;
        let frame_times: Vec<i64> = (0..=max_frame)
            .map(|frame| frame as i64 * FBX_FRAME_DURATION)
            .collect();
        let rest_translations: Vec<_> = (0..bone_count)
            .map(|index| runtime_model.rest_position(BoneIndex(index as u32)))
            .collect();
        let mut tracks: Vec<RuntimeBakeTrack> = (0..bone_count)
            .map(|index| RuntimeBakeTrack::new(index, frame_count))
            .collect();

        for frame in 0..=max_frame {
            let world_matrices = pose_source
                .world_matrices(frame)
                .map_err(|message| FbxExportError::PoseSource { frame, message })?;
            if world_matrices.len() < required_matrix_count {
                return Err(FbxExportError::PoseBoneCount {
                    frame,
                    expected: required_matrix_count,
                    actual: world_matrices.len(),
                });
            }

            for track in &mut tracks {
                let bone = BoneIndex(track.bone_index as u32);
                let bone_world = world_matrices[track.bone_index];
                let local_matrix = match runtime_model.parent_index(bone) {
                    Some(parent) => world_matrices[parent.as_usize()].inverse() * bone_world,
                    None => bone_world,
                };
                let (_scale, rotation, translation) = local_matrix.to_scale_rotation_translation();

                if translation_changed(translation, rest_translations[track.bone_index])
                    || rotation_changed(rotation)
                {
                    track.changed_from_rest = true;
                }

                let converted_translation = convert_position_to_fbx(
                    [
                        translation.x as f64,
                        translation.y as f64,
                        translation.z as f64,
                    ],
                    options,
                );
                let rotation = rotation.normalize();
                let converted_rotation = convert_quat_to_fbx(
                    [
                        rotation.x as f64,
                        rotation.y as f64,
                        rotation.z as f64,
                        rotation.w as f64,
                    ],
                    options,
                );
                let euler = quat_to_euler_xyz(converted_rotation);
                let filtered_euler = track
                    .previous_euler
                    .map(|previous| euler_filter(euler, previous))
                    .unwrap_or(euler);
                track.previous_euler = Some(filtered_euler);

                for axis in 0..3 {
                    track.rotation_values[axis].push(filtered_euler[axis] as f32);
                    track.translation_values[axis].push(converted_translation[axis] as f32);
                }
            }
        }

        let tracks = tracks
            .into_iter()
            .filter(|track| track.changed_from_rest)
            .map(|track| FbxAnimationTrack {
                bone_index: track.bone_index,
                frame_times: frame_times.clone(),
                rotation_values: track.rotation_values,
                translation_values: track.translation_values,
                rotation_attributes: [None, None, None],
                translation_attributes: [None, None, None],
            })
            .collect();
        let morph_tracks = if options.bones_only {
            Vec::new()
        } else {
            collect_runtime_bake_morph_tracks(model, clip, max_frame, &frame_times)
        };

        Ok(Self {
            max_frame,
            tracks,
            morph_tracks,
        })
    }

    fn from_vmd(
        model: &PmxParsedModel,
        vmd: &VmdParsedAnimation,
        options: &FbxExportOptions,
    ) -> Self {
        let bone_tracks = collect_bone_tracks(model, vmd);
        let morph_tracks = if options.bones_only {
            Vec::new()
        } else {
            collect_vmd_morph_tracks(model, vmd)
        };
        let bone_max_frame = bone_tracks
            .iter()
            .filter_map(|track| track.keyframes.last().map(|keyframe| keyframe.frame))
            .max()
            .unwrap_or(0);
        let morph_max_frame = morph_tracks
            .iter()
            .filter_map(|track| {
                track
                    .frame_times
                    .last()
                    .map(|time| (time / FBX_FRAME_DURATION) as u32)
            })
            .max()
            .unwrap_or(0);
        let max_frame = bone_max_frame
            .max(morph_max_frame)
            .max(vmd.metadata.max_frame);
        let frame_count = max_frame as usize + 1;
        let frame_times: Vec<i64> = (0..=max_frame)
            .map(|frame| frame as i64 * FBX_FRAME_DURATION)
            .collect();
        let mut tracks = Vec::with_capacity(bone_tracks.len());

        for track in bone_tracks {
            let bone = &model.skeleton.bones[track.bone_index];
            let rest_translation = bone_local_translation(bone, &model.skeleton.bones, options);
            let mut rotation_values = [
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
            ];
            let mut translation_values = [
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
                Vec::with_capacity(frame_count),
            ];
            let mut previous_euler = None;

            for frame in 0..=max_frame {
                let (translation, rotation) = evaluate_bone_at_frame(&track.keyframes, frame);
                let converted_rotation = convert_quat_to_fbx(rotation, options);
                let euler = quat_to_euler_xyz(converted_rotation);
                let filtered_euler = previous_euler
                    .map(|previous| euler_filter(euler, previous))
                    .unwrap_or(euler);
                previous_euler = Some(filtered_euler);

                let converted_translation = convert_position_to_fbx(translation, options);

                for axis in 0..3 {
                    rotation_values[axis].push(filtered_euler[axis] as f32);
                    translation_values[axis]
                        .push((rest_translation[axis] + converted_translation[axis]) as f32);
                }
            }

            tracks.push(FbxAnimationTrack {
                bone_index: track.bone_index,
                frame_times: frame_times.clone(),
                rotation_values,
                translation_values,
                rotation_attributes: [None, None, None],
                translation_attributes: [None, None, None],
            });
        }

        Self {
            max_frame,
            tracks,
            morph_tracks,
        }
    }

    fn last_time(&self) -> i64 {
        self.max_frame as i64 * FBX_FRAME_DURATION
    }
}

#[derive(Debug, Clone, Copy)]
struct SegmentTangents {
    out_tangent: f32,
    next_in_tangent: f32,
}

fn validate_reduced_pose(
    reduced: &ReducedPoseSequence,
    model_identity: u64,
    bone_count: usize,
    morph_count: usize,
) -> Result<(), FbxExportError> {
    if reduced.target() != ReductionTarget::DccCubic {
        return Err(FbxExportError::ReducedPoseTarget);
    }
    if !reduced.validate_model(model_identity, bone_count, morph_count) {
        return Err(FbxExportError::ReducedPoseBinding);
    }
    Ok(())
}

fn reduced_key_time(
    reduced: &ReducedPoseSequence,
    sample_index: usize,
) -> Result<i64, FbxExportError> {
    let frame = reduced.sample_frames()[sample_index];
    let time = frame as f64 * FBX_TIME_ONE_SECOND as f64 / 30.0;
    if frame < 0.0 || !time.is_finite() || time < i64::MIN as f64 || time > i64::MAX as f64 {
        Err(FbxExportError::ReducedPoseTime { frame })
    } else {
        Ok(time.round() as i64)
    }
}

fn bone_segment_tangents(
    keys: &[ReducedBoneKey],
    axis: usize,
    rotation: bool,
    sign: f32,
) -> Vec<SegmentTangents> {
    (0..keys.len())
        .map(|key| {
            let Some(next) = keys.get(key + 1) else {
                return SegmentTangents {
                    out_tangent: 0.0,
                    next_in_tangent: 0.0,
                };
            };
            let segment = next.dcc_segment;
            let (out_tangent, next_in_tangent, scale) = if rotation {
                (
                    segment.rotation_out_tangent.to_array()[axis],
                    segment.rotation_in_tangent.to_array()[axis],
                    sign * 30.0 * 180.0 / std::f32::consts::PI,
                )
            } else {
                (
                    segment.translation_out_tangent.to_array()[axis],
                    segment.translation_in_tangent.to_array()[axis],
                    sign * 30.0,
                )
            };
            SegmentTangents {
                out_tangent: out_tangent * scale,
                next_in_tangent: next_in_tangent * scale,
            }
        })
        .collect()
}

fn morph_segment_tangents(keys: &[ReducedMorphKey]) -> Vec<SegmentTangents> {
    (0..keys.len())
        .map(|key| {
            let Some(next) = keys.get(key + 1) else {
                return SegmentTangents {
                    out_tangent: 0.0,
                    next_in_tangent: 0.0,
                };
            };
            SegmentTangents {
                out_tangent: next.dcc_segment.out_tangent * 3_000.0,
                next_in_tangent: next.dcc_segment.in_tangent * 3_000.0,
            }
        })
        .collect()
}

fn curve_attributes(tangents: &[SegmentTangents]) -> FbxCurveAttributes {
    const CUBIC_USER: i32 = 0x0000_0408;
    const LINEAR_USER: i32 = 0x0000_0404;
    const DEFAULT_WEIGHT_TOKEN: f32 = f32::from_bits(218_434_821);
    let mut flags = Vec::with_capacity(tangents.len());
    let mut data = Vec::with_capacity(tangents.len() * 4);
    for (index, tangent) in tangents.iter().enumerate() {
        flags.push(if index + 1 < tangents.len() {
            CUBIC_USER
        } else {
            LINEAR_USER
        });
        data.extend_from_slice(&[
            tangent.out_tangent,
            tangent.next_in_tangent,
            DEFAULT_WEIGHT_TOKEN,
            0.0,
        ]);
    }
    FbxCurveAttributes {
        flags,
        data,
        ref_counts: vec![1; tangents.len()],
    }
}

fn unity_keys(
    reduced: &ReducedPoseSequence,
    source_keys: &[ReducedBoneKey],
    values: &[f32],
    tangents: &[SegmentTangents],
) -> Result<Vec<UnityAnimationKeyDto>, FbxExportError> {
    unity_keys_from_indices(
        reduced,
        source_keys.iter().map(|key| key.sample_index),
        values,
        tangents,
    )
}

fn unity_morph_keys(
    reduced: &ReducedPoseSequence,
    source_keys: &[ReducedMorphKey],
    values: &[f32],
    tangents: &[SegmentTangents],
) -> Result<Vec<UnityAnimationKeyDto>, FbxExportError> {
    unity_keys_from_indices(
        reduced,
        source_keys.iter().map(|key| key.sample_index),
        values,
        tangents,
    )
}

fn unity_keys_from_indices(
    reduced: &ReducedPoseSequence,
    sample_indices: impl Iterator<Item = usize>,
    values: &[f32],
    tangents: &[SegmentTangents],
) -> Result<Vec<UnityAnimationKeyDto>, FbxExportError> {
    sample_indices
        .enumerate()
        .map(|(index, sample_index)| {
            let frame = reduced.sample_frames()[sample_index];
            if frame < 0.0 || !frame.is_finite() {
                return Err(FbxExportError::ReducedPoseTime { frame });
            }
            Ok(UnityAnimationKeyDto {
                time_seconds: frame / 30.0,
                value: values[index],
                in_tangent: index
                    .checked_sub(1)
                    .map_or(0.0, |previous| tangents[previous].next_in_tangent),
                out_tangent: tangents[index].out_tangent,
            })
        })
        .collect()
}

fn axis_name(axis: usize) -> char {
    ['x', 'y', 'z'][axis]
}

fn translation_changed(translation: glam::Vec3, rest: glam::Vec3A) -> bool {
    (translation.x - rest.x).abs() > STATIC_BONE_EPSILON
        || (translation.y - rest.y).abs() > STATIC_BONE_EPSILON
        || (translation.z - rest.z).abs() > STATIC_BONE_EPSILON
}

fn rotation_changed(rotation: glam::Quat) -> bool {
    let rotation = rotation.normalize();
    let identity_dot = rotation.w.abs().clamp(0.0, 1.0);
    1.0 - identity_dot > STATIC_BONE_EPSILON
}

fn collect_bone_tracks(model: &PmxParsedModel, vmd: &VmdParsedAnimation) -> Vec<BoneTrack> {
    let mut grouped_frames = Vec::<(usize, Vec<&VmdParsedBoneFrame>)>::new();
    for frame in &vmd.bone_frames {
        let Some(bone_index) = find_bone_index(model, &frame.bone_name) else {
            continue;
        };
        if let Some((_, frames)) = grouped_frames
            .iter_mut()
            .find(|(index, _)| *index == bone_index)
        {
            frames.push(frame);
        } else {
            grouped_frames.push((bone_index, vec![frame]));
        }
    }

    grouped_frames
        .into_iter()
        .map(|(bone_index, mut frames)| {
            frames.sort_by_key(|frame| frame.frame);
            let mut keyframes = Vec::<SortedKeyframe>::with_capacity(frames.len());
            for frame in frames {
                let keyframe = sorted_keyframe_from_vmd(frame);
                if keyframes
                    .last()
                    .is_some_and(|previous| previous.frame == keyframe.frame)
                {
                    keyframes.pop();
                }
                keyframes.push(keyframe);
            }
            BoneTrack {
                bone_index,
                keyframes,
            }
        })
        .filter(|track| !track.keyframes.is_empty())
        .collect()
}

fn sorted_keyframe_from_vmd(frame: &VmdParsedBoneFrame) -> SortedKeyframe {
    SortedKeyframe {
        frame: frame.frame,
        translation: [
            frame.translation[0] as f64,
            frame.translation[1] as f64,
            frame.translation[2] as f64,
        ],
        rotation: quat_normalize([
            frame.rotation[0] as f64,
            frame.rotation[1] as f64,
            frame.rotation[2] as f64,
            frame.rotation[3] as f64,
        ]),
        rot_interp: decode_rotation_interpolation(&frame.interpolation),
    }
}

fn find_bone_index(model: &PmxParsedModel, bone_name: &str) -> Option<usize> {
    model
        .skeleton
        .bones
        .iter()
        .position(|bone| bone.name == bone_name || bone.english_name == bone_name)
}

fn exported_vertex_morph_export_index(model: &PmxParsedModel, morph_name: &str) -> Option<usize> {
    model
        .morphs
        .iter()
        .filter(|morph| is_exportable_vertex_morph(morph))
        .position(|morph| morph.name == morph_name || morph.english_name == morph_name)
}

fn morph_weight_changes_from_zero(weights: &[f32]) -> bool {
    weights
        .iter()
        .any(|weight| weight.abs() > STATIC_BONE_EPSILON)
}

fn exported_vertex_morph_indices(model: &PmxParsedModel) -> Vec<usize> {
    model
        .morphs
        .iter()
        .enumerate()
        .filter(|(_, morph)| is_exportable_vertex_morph(morph))
        .map(|(index, _)| index)
        .collect()
}

fn collect_runtime_bake_morph_tracks(
    model: &PmxParsedModel,
    clip: &AnimationClip,
    max_frame: u32,
    frame_times: &[i64],
) -> Vec<FbxMorphAnimationTrack> {
    let mut morph_tracks = Vec::new();
    for (export_index, pmx_morph_index) in
        exported_vertex_morph_indices(model).into_iter().enumerate()
    {
        let morph_index = MorphIndex(pmx_morph_index as u32);
        let Some(binding) = clip
            .morph_tracks()
            .iter()
            .find(|binding| binding.morph == morph_index)
        else {
            continue;
        };

        let mut weight_values = Vec::with_capacity(frame_times.len());
        for frame in 0..=max_frame {
            let weight = binding.track.sample(frame as f32).unwrap_or(0.0);
            weight_values.push(weight * 100.0);
        }
        if !morph_weight_changes_from_zero(&weight_values) {
            continue;
        }

        morph_tracks.push(FbxMorphAnimationTrack {
            export_index,
            frame_times: frame_times.to_vec(),
            weight_values,
            attributes: None,
        });
    }
    morph_tracks
}

fn collect_vmd_morph_tracks(
    model: &PmxParsedModel,
    vmd: &VmdParsedAnimation,
) -> Vec<FbxMorphAnimationTrack> {
    let mut grouped_frames = std::collections::HashMap::<usize, Vec<&VmdParsedMorphFrame>>::new();
    for frame in &vmd.morph_frames {
        let Some(export_index) = exported_vertex_morph_export_index(model, &frame.morph_name)
        else {
            continue;
        };
        grouped_frames.entry(export_index).or_default().push(frame);
    }

    let mut morph_tracks = Vec::with_capacity(grouped_frames.len());
    for (export_index, mut frames) in grouped_frames {
        frames.sort_by_key(|frame| frame.frame);
        let mut keyframes = Vec::<&VmdParsedMorphFrame>::with_capacity(frames.len());
        for frame in frames {
            if keyframes
                .last()
                .is_some_and(|previous| previous.frame == frame.frame)
            {
                keyframes.pop();
            }
            keyframes.push(frame);
        }

        let frame_times: Vec<i64> = keyframes
            .iter()
            .map(|frame| frame.frame as i64 * FBX_FRAME_DURATION)
            .collect();
        let weight_values: Vec<f32> = keyframes.iter().map(|frame| frame.weight * 100.0).collect();
        if !morph_weight_changes_from_zero(&weight_values) {
            continue;
        }

        morph_tracks.push(FbxMorphAnimationTrack {
            export_index,
            frame_times,
            weight_values,
            attributes: None,
        });
    }
    morph_tracks.sort_by_key(|track| track.export_index);
    morph_tracks
}

fn evaluate_bone_at_frame(keyframes: &[SortedKeyframe], frame: u32) -> ([f64; 3], [f64; 4]) {
    let first = &keyframes[0];
    if frame <= first.frame {
        return (first.translation, first.rotation);
    }
    let last = &keyframes[keyframes.len() - 1];
    if frame >= last.frame {
        return (last.translation, last.rotation);
    }

    let next_index = keyframes.partition_point(|keyframe| keyframe.frame <= frame);
    let prev = &keyframes[next_index - 1];
    let next = &keyframes[next_index];
    let linear_t = (frame - prev.frame) as f64 / (next.frame - prev.frame) as f64;
    let bezier_t = evaluate_bezier(linear_t, next.rot_interp);
    (
        lerp3(prev.translation, next.translation, linear_t),
        quat_slerp(prev.rotation, next.rotation, bezier_t),
    )
}

fn lerp3(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [
        a[0] + t * (b[0] - a[0]),
        a[1] + t * (b[1] - a[1]),
        a[2] + t * (b[2] - a[2]),
    ]
}

fn convert_quat_to_fbx(q: [f64; 4], options: &FbxExportOptions) -> [f64; 4] {
    if options.flip_z {
        [-q[0], -q[1], q[2], q[3]]
    } else {
        q
    }
}

fn convert_position_to_fbx(p: [f64; 3], options: &FbxExportOptions) -> [f64; 3] {
    if options.flip_z {
        [p[0], p[1], -p[2]]
    } else {
        p
    }
}

fn quat_to_euler_xyz(q: [f64; 4]) -> [f64; 3] {
    let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
    let sin_beta = (2.0 * (w * y - x * z)).clamp(-1.0, 1.0);
    let beta = sin_beta.asin();
    let (alpha, gamma) = if beta.cos().abs() < 1e-6 {
        let a = (2.0 * (x * y + w * z)).atan2(1.0 - 2.0 * (y * y + z * z));
        (a, 0.0)
    } else {
        let a = (2.0 * (y * z + w * x)).atan2(1.0 - 2.0 * (x * x + y * y));
        let g = (2.0 * (x * y + w * z)).atan2(1.0 - 2.0 * (y * y + z * z));
        (a, g)
    };
    [alpha.to_degrees(), beta.to_degrees(), gamma.to_degrees()]
}

fn euler_filter(current: [f64; 3], previous: [f64; 3]) -> [f64; 3] {
    let mut result = current;
    for value in 0..3 {
        while result[value] - previous[value] > 180.0 {
            result[value] -= 360.0;
        }
        while result[value] - previous[value] < -180.0 {
            result[value] += 360.0;
        }
    }
    result
}

fn quat_slerp(a: [f64; 4], b: [f64; 4], t: f64) -> [f64; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if dot < 0.0 {
        b = [-b[0], -b[1], -b[2], -b[3]];
        dot = -dot;
    }
    if dot > 0.9995 {
        let result = [
            a[0] + t * (b[0] - a[0]),
            a[1] + t * (b[1] - a[1]),
            a[2] + t * (b[2] - a[2]),
            a[3] + t * (b[3] - a[3]),
        ];
        return quat_normalize(result);
    }
    let theta = dot.acos();
    let sin_theta = theta.sin();
    let wa = ((1.0 - t) * theta).sin() / sin_theta;
    let wb = (t * theta).sin() / sin_theta;
    [
        wa * a[0] + wb * b[0],
        wa * a[1] + wb * b[1],
        wa * a[2] + wb * b[2],
        wa * a[3] + wb * b[3],
    ]
}

fn quat_normalize(q: [f64; 4]) -> [f64; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len == 0.0 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    [q[0] / len, q[1] / len, q[2] / len, q[3] / len]
}

fn evaluate_bezier(x: f64, cp: [u8; 4]) -> f64 {
    let scale = 1.0 / 127.0;
    let x1 = cp[0] as f64 * scale;
    let y1 = cp[1] as f64 * scale;
    let x2 = cp[2] as f64 * scale;
    let y2 = cp[3] as f64 * scale;
    let mut t = x;
    for _ in 0..15 {
        let bx = 3.0 * (1.0 - t) * (1.0 - t) * t * x1 + 3.0 * (1.0 - t) * t * t * x2 + t * t * t;
        let dx = 3.0 * (1.0 - t) * (1.0 - t) * x1
            + 6.0 * (1.0 - t) * t * (x2 - x1)
            + 3.0 * t * t * (1.0 - x2);
        if dx.abs() < 1e-10 {
            break;
        }
        t -= (bx - x) / dx;
        t = t.clamp(0.0, 1.0);
    }
    3.0 * (1.0 - t) * (1.0 - t) * t * y1 + 3.0 * (1.0 - t) * t * t * y2 + t * t * t
}

fn decode_rotation_interpolation(interp: &[u8]) -> [u8; 4] {
    if interp.len() > 15 {
        [interp[3], interp[7], interp[11], interp[15]]
    } else {
        [0, 0, 127, 127]
    }
}

fn triangle_indices_for_handedness(triangle: &[u32], options: &FbxExportOptions) -> [u32; 3] {
    if options.flip_z {
        [triangle[0], triangle[2], triangle[1]]
    } else {
        [triangle[0], triangle[1], triangle[2]]
    }
}

fn material_index_for_triangle(model: &PmxParsedModel, triangle_index: usize) -> i32 {
    let index_start = triangle_index * 3;
    model
        .geometry
        .material_groups
        .iter()
        .find(|group| index_start >= group.start && index_start < group.start + group.count)
        .map(|group| group.material_index as i32)
        .unwrap_or(0)
}

fn write_fbx_header_extension<W: Write + Seek>(
    writer: &mut Writer<W>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "FBXHeaderExtension", |_| Ok(()))?;
    write_i32_node(writer, "FBXHeaderVersion", 1003)?;
    write_i32_node(writer, "FBXVersion", 7400)?;
    write_i32_node(writer, "EncryptionType", 0)?;
    begin_node(writer, "CreationTimeStamp", |_| Ok(()))?;
    write_i32_node(writer, "Version", 1000)?;
    write_i32_node(writer, "Year", 2026)?;
    write_i32_node(writer, "Month", 6)?;
    write_i32_node(writer, "Day", 25)?;
    write_i32_node(writer, "Hour", 0)?;
    write_i32_node(writer, "Minute", 0)?;
    write_i32_node(writer, "Second", 0)?;
    write_i32_node(writer, "Millisecond", 0)?;
    writer.close_node()?;
    write_string_node(writer, "Creator", "mmd-anim fbx exporter")?;
    writer.close_node()?;
    Ok(())
}

fn write_top_level_fields<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "FileId", |attrs| {
        attrs.append_binary_direct(&[
            0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b,
            0xfc, 0xf1,
        ])?;
        Ok(())
    })?;
    writer.close_node()?;
    write_string_node(writer, "CreationTime", "1970-01-01 10:00:00:000")?;
    write_string_node(writer, "Creator", "mmd-anim fbx exporter")?;
    Ok(())
}

fn write_global_settings<W: Write + Seek>(
    writer: &mut Writer<W>,
    animation: Option<&FbxAnimationData>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "GlobalSettings", |_| Ok(()))?;
    write_i32_node(writer, "Version", 1000)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_i32(writer, "UpAxis", "int", "Integer", "", 1)?;
    write_property_i32(writer, "UpAxisSign", "int", "Integer", "", 1)?;
    write_property_i32(writer, "FrontAxis", "int", "Integer", "", 2)?;
    write_property_i32(writer, "FrontAxisSign", "int", "Integer", "", 1)?;
    write_property_i32(writer, "CoordAxis", "int", "Integer", "", 0)?;
    write_property_i32(writer, "CoordAxisSign", "int", "Integer", "", 1)?;
    write_property_i32(writer, "TimeMode", "enum", "", "", 6)?;
    write_property_f64(writer, "UnitScaleFactor", "double", "Number", "", 1.0)?;
    write_property_f64(
        writer,
        "OriginalUnitScaleFactor",
        "double",
        "Number",
        "",
        1.0,
    )?;
    if let Some(anim) = animation {
        let last_time = anim.last_time();
        write_property_i64(writer, "TimeSpanStart", "KTime", "Time", "", 0)?;
        write_property_i64(writer, "TimeSpanStop", "KTime", "Time", "", last_time)?;
        write_property_f64(writer, "CustomFrameRate", "double", "Number", "", 30.0)?;
    }
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_documents<W: Write + Seek>(
    writer: &mut Writer<W>,
    has_animation: bool,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Documents", |_| Ok(()))?;
    write_i32_node(writer, "Count", 1)?;
    begin_node(writer, "Document", |attrs| {
        attrs.append_i64(DOCUMENT_ID)?;
        attrs.append_string_direct("")?;
        attrs.append_string_direct("Scene")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_compound(writer, "SourceObject", "object", "", "")?;
    let active_anim_stack_name = if has_animation { "Take 001" } else { "" };
    write_property_string(
        writer,
        "ActiveAnimStackName",
        "KString",
        "",
        active_anim_stack_name,
    )?;
    writer.close_node()?;
    write_i64_node(writer, "RootNode", ROOT_NODE_ID)?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_references<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "References", |_| Ok(()))?;
    writer.close_node()?;
    Ok(())
}

fn write_definitions<W: Write + Seek>(
    writer: &mut Writer<W>,
    material_count: usize,
    texture_count: usize,
    bone_count: usize,
    vertex_morph_count: usize,
    animation: Option<&FbxAnimationData>,
    include_mesh_assets: bool,
) -> Result<(), FbxExportError> {
    let bone_animation_track_count = animation.map(|data| data.tracks.len()).unwrap_or(0);
    let morph_animation_track_count = animation.map(|data| data.morph_tracks.len()).unwrap_or(0);
    let animation_object_count = if animation.is_some() {
        2 + bone_animation_track_count * 8 + morph_animation_track_count * 2
    } else {
        0
    };
    begin_node(writer, "Definitions", |_| Ok(()))?;
    write_i32_node(writer, "Version", 100)?;
    let mesh_object_count = if include_mesh_assets {
        3 + material_count + texture_count * 2 + bone_count + vertex_morph_count * 3
    } else {
        0
    };
    let object_count =
        1 + (1 + bone_count) + bone_count + mesh_object_count + animation_object_count;
    write_i32_node(writer, "Count", object_count as i32)?;

    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("GlobalSettings")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    writer.close_node()?;

    write_model_object_type(writer, bone_count as i32)?;
    if include_mesh_assets {
        write_geometry_object_type(writer, 1 + vertex_morph_count as i32)?;
        write_material_object_type(writer, material_count as i32)?;
        if texture_count > 0 {
            write_simple_object_type(writer, "Texture", texture_count as i32)?;
            write_simple_object_type(writer, "Video", texture_count as i32)?;
        }
    }
    write_node_attribute_object_type(writer, bone_count as i32)?;
    if include_mesh_assets {
        write_deformer_object_type(writer, bone_count as i32, vertex_morph_count as i32)?;
        write_pose_object_type(writer)?;
    }
    if animation.is_some() {
        write_animation_object_types(
            writer,
            bone_animation_track_count as i32,
            morph_animation_track_count as i32,
        )?;
    }
    writer.close_node()?;
    Ok(())
}

fn write_model_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    bone_count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Model")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1 + bone_count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxNode")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_vec3(
        writer,
        "Lcl Translation",
        "Lcl Translation",
        "",
        "A",
        [0.0; 3],
    )?;
    write_property_vec3(writer, "Lcl Rotation", "Lcl Rotation", "", "A", [0.0; 3])?;
    write_property_vec3(writer, "Lcl Scaling", "Lcl Scaling", "", "A", [1.0; 3])?;
    write_property_f64(writer, "Visibility", "Visibility", "", "A", 1.0)?;
    write_property_i32(writer, "Visibility Inheritance", "bool", "", "", 1)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_node_attribute_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("NodeAttribute")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxSkeleton")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_color(writer, "Color", "ColorRGB", "Color", "", [0.8, 0.8, 0.8])?;
    write_property_f64(writer, "Size", "double", "Number", "", 33.333333333333336)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_geometry_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Geometry")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxMesh")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_color(writer, "Color", "ColorRGB", "Color", "", [0.8, 0.8, 0.8])?;
    write_property_vec3(writer, "BBoxMin", "Vector3D", "Vector", "", [0.0; 3])?;
    write_property_vec3(writer, "BBoxMax", "Vector3D", "Vector", "", [0.0; 3])?;
    write_property_i32(writer, "Primary Visibility", "bool", "", "", 1)?;
    write_property_i32(writer, "Casts Shadows", "bool", "", "", 1)?;
    write_property_i32(writer, "Receive Shadows", "bool", "", "", 1)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_material_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Material")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxSurfacePhong")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_string(writer, "ShadingModel", "KString", "", "Phong")?;
    write_property_i32(writer, "MultiLayer", "bool", "", "", 0)?;
    write_property_color(writer, "EmissiveColor", "Color", "", "A", [0.0, 0.0, 0.0])?;
    write_property_f64(writer, "EmissiveFactor", "Number", "", "A", 1.0)?;
    write_property_color(writer, "AmbientColor", "Color", "", "A", [0.2, 0.2, 0.2])?;
    write_property_f64(writer, "AmbientFactor", "Number", "", "A", 1.0)?;
    write_property_color(writer, "DiffuseColor", "Color", "", "A", [0.8, 0.8, 0.8])?;
    write_property_f64(writer, "DiffuseFactor", "Number", "", "A", 1.0)?;
    write_property_color(
        writer,
        "TransparentColor",
        "Color",
        "",
        "A",
        [0.0, 0.0, 0.0],
    )?;
    write_property_f64(writer, "TransparencyFactor", "Number", "", "A", 0.0)?;
    write_property_f64(writer, "Opacity", "Number", "", "A", 1.0)?;
    write_property_color(writer, "SpecularColor", "Color", "", "A", [0.2, 0.2, 0.2])?;
    write_property_f64(writer, "SpecularFactor", "Number", "", "A", 1.0)?;
    write_property_f64(writer, "Shininess", "Number", "", "A", 20.0)?;
    write_property_f64(writer, "ShininessExponent", "Number", "", "A", 20.0)?;
    write_property_color(writer, "ReflectionColor", "Color", "", "A", [0.0, 0.0, 0.0])?;
    write_property_f64(writer, "ReflectionFactor", "Number", "", "A", 1.0)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_deformer_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    bone_count: i32,
    vertex_morph_count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Deformer")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1 + bone_count + vertex_morph_count * 2)?;
    writer.close_node()?;
    Ok(())
}

fn write_pose_object_type<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Pose")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    writer.close_node()?;
    Ok(())
}

fn write_animation_object_types<W: Write + Seek>(
    writer: &mut Writer<W>,
    bone_track_count: i32,
    morph_track_count: i32,
) -> Result<(), FbxExportError> {
    write_animation_stack_object_type(writer)?;
    write_animation_layer_object_type(writer)?;
    write_animation_curve_node_object_type(writer, bone_track_count * 2 + morph_track_count)?;
    write_simple_object_type(
        writer,
        "AnimationCurve",
        bone_track_count * 6 + morph_track_count,
    )?;
    Ok(())
}

fn write_animation_stack_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("AnimationStack")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxAnimStack")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_string(writer, "Description", "KString", "", "")?;
    write_property_i64(writer, "LocalStart", "KTime", "Time", "", 0)?;
    write_property_i64(writer, "LocalStop", "KTime", "Time", "", 0)?;
    write_property_i64(writer, "ReferenceStart", "KTime", "Time", "", 0)?;
    write_property_i64(writer, "ReferenceStop", "KTime", "Time", "", 0)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_animation_layer_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("AnimationLayer")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxAnimLayer")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_f64(writer, "Weight", "Number", "", "A", 100.0)?;
    write_property_i32(writer, "Mute", "bool", "", "", 0)?;
    write_property_i32(writer, "Solo", "bool", "", "", 0)?;
    write_property_i32(writer, "Lock", "bool", "", "", 0)?;
    write_property_color(writer, "Color", "ColorRGB", "Color", "", [0.8, 0.8, 0.8])?;
    write_property_i32(writer, "BlendMode", "enum", "", "", 0)?;
    write_property_i32(writer, "RotationAccumulationMode", "enum", "", "", 0)?;
    write_property_i32(writer, "ScaleAccumulationMode", "enum", "", "", 0)?;
    write_property_i64(writer, "BlendModeBypass", "ULongLong", "", "", 0)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_animation_curve_node_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("AnimationCurveNode")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxAnimCurveNode")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_compound(writer, "d", "Compound", "", "")?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_simple_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct(name)?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", count)?;
    writer.close_node()?;
    Ok(())
}

fn write_objects<W: Write + Seek>(
    writer: &mut Writer<W>,
    model: &PmxParsedModel,
    options: &FbxExportOptions,
    mesh: Option<&MeshData>,
    animation: Option<&FbxAnimationData>,
) -> Result<(), FbxExportError> {
    let bone_names = build_bone_names(&model.skeleton.bones, options.bone_name_policy);
    begin_node(writer, "Objects", |_| Ok(()))?;
    let vertex_morph_exports = if let Some(mesh) = mesh {
        let vertex_morph_exports = collect_vertex_morph_exports(model, mesh, options);
        write_geometry(writer, mesh)?;
        for (export_index, morph_export) in vertex_morph_exports.iter().enumerate() {
            write_shape_geometry(writer, export_index, morph_export)?;
        }
        Some(vertex_morph_exports)
    } else {
        None
    };
    write_model(writer, options)?;
    write_skeleton(writer, &model.skeleton.bones, &bone_names, options)?;
    if let Some(vertex_morph_exports) = vertex_morph_exports.as_ref() {
        let texture_records = diffuse_texture_records(model, options);
        write_skin_deformers(writer, model, options)?;
        for (export_index, morph_export) in vertex_morph_exports.iter().enumerate() {
            write_blend_shape_deformer(writer, export_index, morph_export)?;
            write_blend_shape_channel_deformer(writer, export_index, morph_export)?;
        }
        write_bind_pose(writer, &model.skeleton.bones, options)?;
        for (index, material) in model.materials.iter().enumerate() {
            write_material(writer, material, MATERIAL_ID_BASE + index as i64)?;
        }
        for record in &texture_records {
            write_texture(writer, record)?;
            write_video(writer, record)?;
        }
    }
    if let Some(animation) = animation {
        write_animation(writer, animation)?;
    }
    writer.close_node()?;
    Ok(())
}

fn write_geometry<W: Write + Seek>(
    writer: &mut Writer<W>,
    mesh: &MeshData,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Geometry", |attrs| {
        attrs.append_i64(GEOMETRY_ID)?;
        attrs.append_string_direct("\x00\x01Geometry")?;
        attrs.append_string_direct("Mesh")?;
        Ok(())
    })?;
    write_i32_node(writer, "GeometryVersion", 124)?;
    write_arr_f64_node(writer, "Vertices", &mesh.vertices)?;
    write_arr_i32_node(writer, "PolygonVertexIndex", &mesh.polygon_vertex_indices)?;

    begin_node(writer, "LayerElementNormal", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_string_node(writer, "Name", "")?;
    write_string_node(writer, "MappingInformationType", "ByVertice")?;
    write_string_node(writer, "ReferenceInformationType", "Direct")?;
    write_arr_f64_node(writer, "Normals", &mesh.normals)?;
    writer.close_node()?;

    begin_node(writer, "LayerElementUV", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_string_node(writer, "Name", "UVSet")?;
    write_string_node(writer, "MappingInformationType", "ByPolygonVertex")?;
    write_string_node(writer, "ReferenceInformationType", "IndexToDirect")?;
    write_arr_f64_node(writer, "UV", &mesh.uvs)?;
    write_arr_i32_node(writer, "UVIndex", &mesh.polygon_uv_indices)?;
    writer.close_node()?;

    begin_node(writer, "LayerElementMaterial", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_string_node(writer, "Name", "")?;
    write_string_node(writer, "MappingInformationType", "ByPolygon")?;
    write_string_node(writer, "ReferenceInformationType", "IndexToDirect")?;
    write_arr_i32_node(writer, "Materials", &mesh.polygon_material_indices)?;
    writer.close_node()?;

    begin_node(writer, "Layer", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 100)?;
    write_layer_element(writer, "LayerElementNormal", 0)?;
    write_layer_element(writer, "LayerElementUV", 0)?;
    write_layer_element(writer, "LayerElementMaterial", 0)?;
    writer.close_node()?;

    writer.close_node()?;
    Ok(())
}

fn write_shape_geometry<W: Write + Seek>(
    writer: &mut Writer<W>,
    export_index: usize,
    morph_export: &VertexMorphExport,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01Geometry", morph_export.name);
    begin_node(writer, "Geometry", |attrs| {
        attrs.append_i64(shape_geometry_id(export_index))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("Shape")?;
        Ok(())
    })?;
    write_i32_node(writer, "GeometryVersion", 124)?;
    write_arr_i32_node(writer, "Indexes", &morph_export.indexes)?;
    write_arr_f64_node(writer, "Vertices", &morph_export.vertices)?;
    writer.close_node()?;
    Ok(())
}

fn write_blend_shape_deformer<W: Write + Seek>(
    writer: &mut Writer<W>,
    export_index: usize,
    morph_export: &VertexMorphExport,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01Deformer", morph_export.name);
    begin_node(writer, "Deformer", |attrs| {
        attrs.append_i64(blend_shape_id(export_index))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("BlendShape")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 100)?;
    writer.close_node()?;
    Ok(())
}

fn write_blend_shape_channel_deformer<W: Write + Seek>(
    writer: &mut Writer<W>,
    export_index: usize,
    morph_export: &VertexMorphExport,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01SubDeformer", morph_export.name);
    begin_node(writer, "Deformer", |attrs| {
        attrs.append_i64(blend_shape_channel_id(export_index))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("BlendShapeChannel")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 100)?;
    write_f64_node(writer, "DeformPercent", 0.0)?;
    write_arr_f64_node(writer, "FullWeights", &[100.0])?;
    writer.close_node()?;
    Ok(())
}

fn write_layer_element<W: Write + Seek>(
    writer: &mut Writer<W>,
    element_type: &str,
    typed_index: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "LayerElement", |_| Ok(()))?;
    write_string_node(writer, "Type", element_type)?;
    write_i32_node(writer, "TypedIndex", typed_index)?;
    writer.close_node()?;
    Ok(())
}

fn write_model<W: Write + Seek>(
    writer: &mut Writer<W>,
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01Model", options.model_name);
    let subtype = if options.bones_only { "Null" } else { "Mesh" };
    begin_node(writer, "Model", |attrs| {
        attrs.append_i64(MODEL_ID)?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct(subtype)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 232)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_vec3(
        writer,
        "Lcl Translation",
        "Lcl Translation",
        "",
        "A",
        [0.0; 3],
    )?;
    write_property_vec3(writer, "Lcl Rotation", "Lcl Rotation", "", "A", [0.0; 3])?;
    write_property_vec3(writer, "Lcl Scaling", "Lcl Scaling", "", "A", [1.0; 3])?;
    write_property_i32(writer, "DefaultAttributeIndex", "int", "Integer", "", 0)?;
    writer.close_node()?;
    write_bool_node(writer, "Shading", true)?;
    write_string_node(writer, "Culling", "CullingOff")?;
    writer.close_node()?;
    Ok(())
}

fn write_skeleton<W: Write + Seek>(
    writer: &mut Writer<W>,
    bones: &[PmxParsedBone],
    bone_names: &[String],
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    for (index, bone) in bones.iter().enumerate() {
        write_bone_node_attribute(writer, index, &bone_names[index])?;
        write_bone_model(writer, index, bone, &bone_names[index], bones, options)?;
    }
    Ok(())
}

fn write_bone_node_attribute<W: Write + Seek>(
    writer: &mut Writer<W>,
    index: usize,
    bone_name: &str,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01NodeAttribute", bone_name);
    begin_node(writer, "NodeAttribute", |attrs| {
        attrs.append_i64(bone_attr_id(index))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("LimbNode")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_f64(writer, "Size", "double", "Number", "", 33.333333333333336)?;
    writer.close_node()?;
    write_string_node(writer, "TypeFlags", "Skeleton")?;
    writer.close_node()?;
    Ok(())
}

fn write_bone_model<W: Write + Seek>(
    writer: &mut Writer<W>,
    index: usize,
    bone: &PmxParsedBone,
    bone_name: &str,
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01Model", bone_name);
    begin_node(writer, "Model", |attrs| {
        attrs.append_i64(bone_model_id(index))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("LimbNode")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 232)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_i32(writer, "RotationActive", "bool", "", "", 1)?;
    write_property_i32(writer, "InheritType", "enum", "", "", 1)?;
    write_property_vec3(writer, "ScalingMax", "Vector3D", "Vector", "", [0.0; 3])?;
    write_property_i32(writer, "DefaultAttributeIndex", "int", "Integer", "", 0)?;
    write_property_vec3(
        writer,
        "Lcl Translation",
        "Lcl Translation",
        "",
        "A+",
        bone_local_translation(bone, bones, options),
    )?;
    writer.close_node()?;
    write_bool_node(writer, "Shading", true)?;
    write_string_node(writer, "Culling", "CullingOff")?;
    writer.close_node()?;
    Ok(())
}

fn bone_local_translation(
    bone: &PmxParsedBone,
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> [f64; 3] {
    let position = converted_bone_position(bone, options);
    if bone.parent_index >= 0 {
        if let Some(parent) = bones.get(bone.parent_index as usize) {
            let parent_position = converted_bone_position(parent, options);
            return [
                position[0] - parent_position[0],
                position[1] - parent_position[1],
                position[2] - parent_position[2],
            ];
        }
    }
    position
}

fn converted_bone_position(bone: &PmxParsedBone, options: &FbxExportOptions) -> [f64; 3] {
    let z_sign = if options.flip_z { -1.0 } else { 1.0 };
    [
        bone.position[0] as f64,
        bone.position[1] as f64,
        bone.position[2] as f64 * z_sign,
    ]
}

fn identity_matrix() -> [f64; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn bone_world_transform(bone: &PmxParsedBone, options: &FbxExportOptions) -> [f64; 16] {
    let position = converted_bone_position(bone, options);
    [
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        position[0],
        position[1],
        position[2],
        1.0,
    ]
}

fn bone_world_transform_inverse(bone: &PmxParsedBone, options: &FbxExportOptions) -> [f64; 16] {
    let position = converted_bone_position(bone, options);
    [
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        -position[0],
        -position[1],
        -position[2],
        1.0,
    ]
}

fn build_bone_names(bones: &[PmxParsedBone], policy: FbxBoneNamePolicy) -> Vec<String> {
    build_bone_name_map(bones, policy)
        .into_iter()
        .map(|entry| entry.fbx_name)
        .collect()
}

pub fn build_bone_name_map(
    bones: &[PmxParsedBone],
    policy: FbxBoneNamePolicy,
) -> Vec<FbxBoneNameMapEntry> {
    match policy {
        FbxBoneNamePolicy::LegacyHex => bones
            .iter()
            .enumerate()
            .map(|(index, bone)| FbxBoneNameMapEntry {
                index,
                pmx_name: bone.name.clone(),
                pmx_english_name: bone.english_name.clone(),
                fbx_name: japanese_to_ascii(&bone.name),
                source: FbxBoneNameSource::LegacyHex,
                collision_suffix: None,
            })
            .collect(),
        FbxBoneNamePolicy::Readable => {
            let candidates = bones
                .iter()
                .map(resolve_readable_bone_name)
                .collect::<Vec<_>>();
            deduplicate_bone_names(bones, candidates)
        }
    }
}

struct BoneNameCandidate {
    name: String,
    source: FbxBoneNameSource,
}

fn resolve_readable_bone_name(bone: &PmxParsedBone) -> BoneNameCandidate {
    if let Some(name) = sanitize_fbx_identifier(bone.english_name.trim()) {
        return BoneNameCandidate {
            name,
            source: FbxBoneNameSource::PmxEnglish,
        };
    }
    if let Some(name) = sanitize_fbx_identifier(&bone.name) {
        return BoneNameCandidate {
            name,
            source: FbxBoneNameSource::AsciiName,
        };
    }
    if let Some(name) = standard_mmd_bone_dictionary_name(&bone.name) {
        return BoneNameCandidate {
            name: name.to_owned(),
            source: FbxBoneNameSource::StandardDictionary,
        };
    }
    BoneNameCandidate {
        name: japanese_to_ascii(&bone.name),
        source: FbxBoneNameSource::HexFallback,
    }
}

fn sanitize_fbx_identifier(raw: &str) -> Option<String> {
    let normalized = normalize_fullwidth_ascii(raw);
    if !normalized.is_ascii() {
        return None;
    }

    let mut result = String::with_capacity(normalized.len());
    for ch in normalized.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            result.push(ch);
        } else {
            result.push('_');
        }
    }
    if result.is_empty() {
        return None;
    }
    if result.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        result.insert_str(0, "b_");
    }
    Some(result)
}

fn normalize_fullwidth_ascii(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ('\u{FF01}'..='\u{FF5E}').contains(&ch) {
                char::from_u32(ch as u32 - 0xFEE0).unwrap_or('_')
            } else {
                ch
            }
        })
        .collect()
}

fn standard_mmd_bone_dictionary_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "全ての親" => "master",
        "センター" => "center",
        "グルーブ" => "groove",
        "腰" => "waist",
        "下半身" => "lower_body",
        "上半身" => "upper_body",
        "上半身2" | "上半身２" => "upper_body_2",
        "首" => "neck",
        "頭" => "head",
        "左肩" => "left_shoulder",
        "左腕" => "left_arm",
        "左ひじ" | "左肘" => "left_elbow",
        "左手首" => "left_wrist",
        "右肩" => "right_shoulder",
        "右腕" => "right_arm",
        "右ひじ" | "右肘" => "right_elbow",
        "右手首" => "right_wrist",
        "左足" => "left_leg",
        "左ひざ" | "左膝" => "left_knee",
        "左足首" => "left_ankle",
        "左足ＩＫ" | "左足IK" => "left_leg_ik",
        "左つま先ＩＫ" | "左つま先IK" => "left_toe_ik",
        "右足" => "right_leg",
        "右ひざ" | "右膝" => "right_knee",
        "右足首" => "right_ankle",
        "右足ＩＫ" | "右足IK" => "right_leg_ik",
        "右つま先ＩＫ" | "右つま先IK" => "right_toe_ik",
        _ => return None,
    })
}

fn deduplicate_bone_names(
    bones: &[PmxParsedBone],
    candidates: Vec<BoneNameCandidate>,
) -> Vec<FbxBoneNameMapEntry> {
    let mut used = HashSet::<String>::new();
    let mut entries = Vec::with_capacity(candidates.len());
    for (index, candidate) in candidates.into_iter().enumerate() {
        let mut fbx_name = candidate.name.clone();
        let mut collision_suffix = None;
        if !used.insert(fbx_name.clone()) {
            let suffix = format!("_{index}");
            fbx_name = format!("{}{suffix}", candidate.name);
            if used.insert(fbx_name.clone()) {
                collision_suffix = Some(suffix);
            } else {
                let mut suffix_index = 2usize;
                loop {
                    let suffix = format!("_{index}_{suffix_index}");
                    fbx_name = format!("{}{suffix}", candidate.name);
                    if used.insert(fbx_name.clone()) {
                        collision_suffix = Some(suffix);
                        break;
                    }
                    suffix_index += 1;
                }
            }
        }
        let bone = &bones[index];
        entries.push(FbxBoneNameMapEntry {
            index,
            pmx_name: bone.name.clone(),
            pmx_english_name: bone.english_name.clone(),
            fbx_name,
            source: candidate.source,
            collision_suffix,
        });
    }
    entries
}

fn japanese_to_ascii(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 6);
    for ch in s.chars() {
        if ch.is_ascii() {
            result.push(ch);
        } else if ('\u{FF01}'..='\u{FF5E}').contains(&ch) {
            result.push(char::from_u32(ch as u32 - 0xFEE0).unwrap_or('_'));
        } else {
            for byte in ch.to_string().as_bytes() {
                result.push_str(&format!("{:02X}", byte));
            }
        }
    }
    result
}

fn write_material<W: Write + Seek>(
    writer: &mut Writer<W>,
    material: &PmxParsedMaterial,
    id: i64,
) -> Result<(), FbxExportError> {
    let material_name = if material.name.is_empty() {
        material.english_name.as_str()
    } else {
        material.name.as_str()
    };
    let name = format!("{}\x00\x01Material", material_name);
    begin_node(writer, "Material", |attrs| {
        attrs.append_i64(id)?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 102)?;
    write_string_node(writer, "ShadingModel", "phong")?;
    write_i32_node(writer, "MultiLayer", 0)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_color(
        writer,
        "DiffuseColor",
        "Color",
        "",
        "A",
        [
            material.diffuse[0] as f64,
            material.diffuse[1] as f64,
            material.diffuse[2] as f64,
        ],
    )?;
    write_property_f64(writer, "DiffuseFactor", "double", "Number", "", 1.0)?;
    write_property_color(
        writer,
        "SpecularColor",
        "Color",
        "",
        "A",
        [
            material.specular[0] as f64,
            material.specular[1] as f64,
            material.specular[2] as f64,
        ],
    )?;
    write_property_f64(
        writer,
        "SpecularFactor",
        "double",
        "Number",
        "",
        material.specular_power as f64,
    )?;
    write_property_color(
        writer,
        "AmbientColor",
        "Color",
        "",
        "A",
        [
            material.ambient[0] as f64,
            material.ambient[1] as f64,
            material.ambient[2] as f64,
        ],
    )?;
    write_property_f64(
        writer,
        "TransparencyFactor",
        "double",
        "Number",
        "",
        (1.0 - material.diffuse[3]).clamp(0.0, 1.0) as f64,
    )?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

#[derive(Clone)]
struct DiffuseTextureRecord {
    material_index: usize,
    path: String,
}

fn diffuse_texture_records(
    model: &PmxParsedModel,
    options: &FbxExportOptions,
) -> Vec<DiffuseTextureRecord> {
    model
        .materials
        .iter()
        .enumerate()
        .filter_map(|(material_index, material)| {
            let option_path = options
                .diffuse_texture_paths
                .get(material_index)
                .map(String::as_str)
                .unwrap_or("");
            let path = if option_path.is_empty() {
                material.texture_path.as_str()
            } else {
                option_path
            };
            if path.is_empty() {
                None
            } else {
                Some(DiffuseTextureRecord {
                    material_index,
                    path: path.replace('\\', "/"),
                })
            }
        })
        .collect()
}

fn texture_id(record: &DiffuseTextureRecord) -> i64 {
    TEXTURE_ID_BASE + record.material_index as i64
}

fn video_id(record: &DiffuseTextureRecord) -> i64 {
    VIDEO_ID_BASE + record.material_index as i64
}

fn write_texture<W: Write + Seek>(
    writer: &mut Writer<W>,
    record: &DiffuseTextureRecord,
) -> Result<(), FbxExportError> {
    let name = format!("DiffuseTexture_{}\x00\x01Texture", record.material_index);
    begin_node(writer, "Texture", |attrs| {
        attrs.append_i64(texture_id(record))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("TextureVideoClip")?;
        Ok(())
    })?;
    write_string_node(writer, "Type", "TextureVideoClip")?;
    write_i32_node(writer, "Version", 202)?;
    write_string_node(writer, "TextureName", &name)?;
    write_string_node(writer, "Media", &record.path)?;
    write_string_node(writer, "FileName", &record.path)?;
    write_string_node(writer, "RelativeFilename", &record.path)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_string(writer, "UVSet", "KString", "", "UVSet")?;
    write_property_i32(writer, "UseMaterial", "bool", "", "", 1)?;
    write_property_i32(writer, "UseMipMap", "bool", "", "", 0)?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_video<W: Write + Seek>(
    writer: &mut Writer<W>,
    record: &DiffuseTextureRecord,
) -> Result<(), FbxExportError> {
    let name = format!("DiffuseVideo_{}\x00\x01Video", record.material_index);
    begin_node(writer, "Video", |attrs| {
        attrs.append_i64(video_id(record))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("Clip")?;
        Ok(())
    })?;
    write_string_node(writer, "Type", "Clip")?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_string(writer, "Path", "KString", "XRefUrl", &record.path)?;
    writer.close_node()?;
    write_string_node(writer, "FileName", &record.path)?;
    write_string_node(writer, "RelativeFilename", &record.path)?;
    writer.close_node()?;
    Ok(())
}

fn write_skin_deformers<W: Write + Seek>(
    writer: &mut Writer<W>,
    model: &PmxParsedModel,
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    write_skin_deformer(writer)?;
    let vertex_count = model.geometry.positions.len() / 3;
    for (index, bone) in model.skeleton.bones.iter().enumerate() {
        write_cluster_deformer(
            writer,
            index,
            bone,
            &model.geometry.skin_indices,
            &model.geometry.skin_weights,
            vertex_count,
            options,
        )?;
    }
    Ok(())
}

fn write_skin_deformer<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "Deformer", |attrs| {
        attrs.append_i64(SKIN_ID)?;
        attrs.append_string_direct("\x00\x01Deformer")?;
        attrs.append_string_direct("Skin")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_f64_node(writer, "Link_DeformAcuracy", 50.0)?;
    write_string_node(writer, "SkinningType", "Linear")?;
    writer.close_node()?;
    Ok(())
}

fn write_cluster_deformer<W: Write + Seek>(
    writer: &mut Writer<W>,
    index: usize,
    bone: &PmxParsedBone,
    skin_indices: &[u32],
    skin_weights: &[f32],
    vertex_count: usize,
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    let (indices, weights) =
        collect_bone_skin_data(index, skin_indices, skin_weights, vertex_count);
    begin_node(writer, "Deformer", |attrs| {
        attrs.append_i64(cluster_id(index))?;
        attrs.append_string_direct("\x00\x01SubDeformer")?;
        attrs.append_string_direct("Cluster")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 100)?;
    begin_node(writer, "UserData", |attrs| {
        attrs.append_string_direct("")?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    writer.close_node()?;
    if !indices.is_empty() {
        write_arr_i32_node(writer, "Indexes", &indices)?;
        write_arr_f64_node(writer, "Weights", &weights)?;
    }
    // Maya imports Cluster Transform as the inverse bind matrix. Keeping this
    // as identity makes every bindPreMatrix identity and applies the bind-joint
    // translation to already model-space PMX vertices a second time.
    write_arr_f64_node(
        writer,
        "Transform",
        &bone_world_transform_inverse(bone, options),
    )?;
    write_arr_f64_node(
        writer,
        "TransformLink",
        &bone_world_transform(bone, options),
    )?;
    writer.close_node()?;
    Ok(())
}

fn collect_bone_skin_data(
    bone_index: usize,
    skin_indices: &[u32],
    skin_weights: &[f32],
    vertex_count: usize,
) -> (Vec<i32>, Vec<f64>) {
    let mut indices = Vec::new();
    let mut weights = Vec::new();
    for vertex_index in 0..vertex_count {
        let mut merged_weight = 0.0f64;
        for slot in 0..4 {
            let skin_offset = vertex_index * 4 + slot;
            if skin_indices[skin_offset] as usize == bone_index && skin_weights[skin_offset] > 0.0 {
                merged_weight += skin_weights[skin_offset] as f64;
            }
        }
        if merged_weight > 0.0 {
            indices.push(vertex_index as i32);
            weights.push(merged_weight);
        }
    }
    (indices, weights)
}

fn write_bind_pose<W: Write + Seek>(
    writer: &mut Writer<W>,
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Pose", |attrs| {
        attrs.append_i64(POSE_ID)?;
        attrs.append_string_direct("BindPose\x00\x01Pose")?;
        attrs.append_string_direct("BindPose")?;
        Ok(())
    })?;
    write_string_node(writer, "Type", "BindPose")?;
    write_i32_node(writer, "Version", 100)?;
    write_i32_node(writer, "NbPoseNodes", (bones.len() + 1) as i32)?;
    write_pose_node(writer, MODEL_ID, &identity_matrix())?;
    for (index, bone) in bones.iter().enumerate() {
        write_pose_node(
            writer,
            bone_model_id(index),
            &bone_world_transform(bone, options),
        )?;
    }
    writer.close_node()?;
    Ok(())
}

fn write_pose_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    node_id: i64,
    matrix: &[f64; 16],
) -> Result<(), FbxExportError> {
    begin_node(writer, "PoseNode", |_| Ok(()))?;
    write_i64_node(writer, "Node", node_id)?;
    write_arr_f64_node(writer, "Matrix", matrix)?;
    writer.close_node()?;
    Ok(())
}

fn write_animation<W: Write + Seek>(
    writer: &mut Writer<W>,
    animation: &FbxAnimationData,
) -> Result<(), FbxExportError> {
    write_animation_stack(writer, animation.last_time())?;
    write_animation_layer(writer)?;
    for track in &animation.tracks {
        write_animation_curve_node(
            writer,
            animation_curvenode_rotation_id(track.bone_index),
            "R",
        )?;
        write_animation_curve_node(
            writer,
            animation_curvenode_translation_id(track.bone_index),
            "T",
        )?;
        for channel in 0..3 {
            write_animation_curve(
                writer,
                animation_curve_id(track.bone_index, channel),
                &track.frame_times,
                &track.rotation_values[channel],
                track.rotation_attributes[channel].as_ref(),
            )?;
        }
        for channel in 0..3 {
            write_animation_curve(
                writer,
                animation_curve_id(track.bone_index, channel + 3),
                &track.frame_times,
                &track.translation_values[channel],
                track.translation_attributes[channel].as_ref(),
            )?;
        }
    }
    for (track_index, track) in animation.morph_tracks.iter().enumerate() {
        write_morph_animation_curve_node(writer, animation_curvenode_morph_id(track_index))?;
        write_animation_curve(
            writer,
            animation_curve_morph_id(track_index),
            &track.frame_times,
            &track.weight_values,
            track.attributes.as_ref(),
        )?;
    }
    Ok(())
}

fn write_animation_stack<W: Write + Seek>(
    writer: &mut Writer<W>,
    last_time: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, "AnimationStack", |attrs| {
        attrs.append_i64(ANIM_STACK_ID)?;
        attrs.append_string_direct("Take 001\x00\x01AnimStack")?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_i64(writer, "LocalStop", "KTime", "Time", "", last_time)?;
    write_property_i64(writer, "ReferenceStop", "KTime", "Time", "", last_time)?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_animation_layer<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "AnimationLayer", |attrs| {
        attrs.append_i64(ANIM_LAYER_ID)?;
        attrs.append_string_direct("BaseLayer\x00\x01AnimLayer")?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_morph_animation_curve_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    id: i64,
) -> Result<(), FbxExportError> {
    let typed_name = "DeformPercent\x00\x01AnimCurveNode";
    begin_node(writer, "AnimationCurveNode", |attrs| {
        attrs.append_i64(id)?;
        attrs.append_string_direct(typed_name)?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_f64(writer, "d|DeformPercent", "Number", "", "A", 0.0)?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_animation_curve_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    id: i64,
    name: &str,
) -> Result<(), FbxExportError> {
    let typed_name = format!("{name}\x00\x01AnimCurveNode");
    begin_node(writer, "AnimationCurveNode", |attrs| {
        attrs.append_i64(id)?;
        attrs.append_string_direct(&typed_name)?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_f64(writer, "d|X", "Number", "", "A", 0.0)?;
    write_property_f64(writer, "d|Y", "Number", "", "A", 0.0)?;
    write_property_f64(writer, "d|Z", "Number", "", "A", 0.0)?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_animation_curve<W: Write + Seek>(
    writer: &mut Writer<W>,
    id: i64,
    frame_times: &[i64],
    values: &[f32],
    attributes: Option<&FbxCurveAttributes>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "AnimationCurve", |attrs| {
        attrs.append_i64(id)?;
        attrs.append_string_direct("\x00\x01AnimCurve")?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    write_f64_node(writer, "Default", 0.0)?;
    write_i32_node(writer, "KeyVer", 4009)?;
    write_arr_i64_node(writer, "KeyTime", frame_times)?;
    write_arr_f32_node(writer, "KeyValueFloat", values)?;
    if let Some(attributes) = attributes {
        write_arr_i32_node(writer, "KeyAttrFlags", &attributes.flags)?;
        write_arr_f32_node(writer, "KeyAttrDataFloat", &attributes.data)?;
        write_arr_i32_node(writer, "KeyAttrRefCount", &attributes.ref_counts)?;
    } else {
        write_arr_i32_node(writer, "KeyAttrFlags", &[0x00006108_i32])?;
        write_arr_f32_node(
            writer,
            "KeyAttrDataFloat",
            &[0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32],
        )?;
        write_arr_i32_node(writer, "KeyAttrRefCount", &[values.len() as i32])?;
    }
    writer.close_node()?;
    Ok(())
}

fn write_connections<W: Write + Seek>(
    writer: &mut Writer<W>,
    model: &PmxParsedModel,
    options: &FbxExportOptions,
    bones: &[PmxParsedBone],
    vertex_morph_count: usize,
    animation: Option<&FbxAnimationData>,
    include_mesh_assets: bool,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Connections", |_| Ok(()))?;
    write_oo_connection(writer, MODEL_ID, ROOT_NODE_ID)?;
    if include_mesh_assets {
        write_oo_connection(writer, GEOMETRY_ID, MODEL_ID)?;
        write_oo_connection(writer, SKIN_ID, GEOMETRY_ID)?;
        for export_index in 0..vertex_morph_count {
            write_oo_connection(
                writer,
                shape_geometry_id(export_index),
                blend_shape_channel_id(export_index),
            )?;
            write_oo_connection(
                writer,
                blend_shape_channel_id(export_index),
                blend_shape_id(export_index),
            )?;
            write_oo_connection(writer, blend_shape_id(export_index), GEOMETRY_ID)?;
        }
        for index in 0..model.materials.len() {
            write_oo_connection(writer, MATERIAL_ID_BASE + index as i64, MODEL_ID)?;
        }
        for record in diffuse_texture_records(model, options) {
            write_oo_connection(writer, video_id(&record), texture_id(&record))?;
            write_op_connection(
                writer,
                texture_id(&record),
                MATERIAL_ID_BASE + record.material_index as i64,
                "DiffuseColor",
            )?;
        }
    }
    for (index, bone) in bones.iter().enumerate() {
        let model_id = bone_model_id(index);
        write_oo_connection(writer, bone_attr_id(index), model_id)?;
        if include_mesh_assets {
            write_oo_connection(writer, cluster_id(index), SKIN_ID)?;
            write_oo_connection(writer, model_id, cluster_id(index))?;
        }
        let parent_id = if bone.parent_index >= 0 {
            bone_model_id(bone.parent_index as usize)
        } else {
            ROOT_NODE_ID
        };
        write_oo_connection(writer, model_id, parent_id)?;
    }
    if let Some(animation) = animation {
        write_animation_connections(writer, animation)?;
    }
    writer.close_node()?;
    Ok(())
}

fn write_takes<W: Write + Seek>(
    writer: &mut Writer<W>,
    last_time: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Takes", |_| Ok(()))?;
    write_string_node(writer, "Current", "Take 001")?;
    begin_node(writer, "Take", |attrs| {
        attrs.append_string_direct("Take 001")?;
        Ok(())
    })?;
    write_string_node(writer, "FileName", "Take_001.tak")?;
    write_time_span_node(writer, "LocalTime", last_time)?;
    write_time_span_node(writer, "ReferenceTime", last_time)?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_time_span_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    last_time: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_i64(0)?;
        attrs.append_i64(last_time)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_animation_connections<W: Write + Seek>(
    writer: &mut Writer<W>,
    animation: &FbxAnimationData,
) -> Result<(), FbxExportError> {
    write_oo_connection(writer, ANIM_LAYER_ID, ANIM_STACK_ID)?;
    for track in &animation.tracks {
        let bone_model = bone_model_id(track.bone_index);
        let rotation_node = animation_curvenode_rotation_id(track.bone_index);
        let translation_node = animation_curvenode_translation_id(track.bone_index);

        write_oo_connection(writer, rotation_node, ANIM_LAYER_ID)?;
        write_op_connection(writer, rotation_node, bone_model, "Lcl Rotation")?;
        for (channel, property) in ["d|X", "d|Y", "d|Z"].into_iter().enumerate() {
            write_op_connection(
                writer,
                animation_curve_id(track.bone_index, channel),
                rotation_node,
                property,
            )?;
        }

        write_oo_connection(writer, translation_node, ANIM_LAYER_ID)?;
        write_op_connection(writer, translation_node, bone_model, "Lcl Translation")?;
        for (channel, property) in ["d|X", "d|Y", "d|Z"].into_iter().enumerate() {
            write_op_connection(
                writer,
                animation_curve_id(track.bone_index, channel + 3),
                translation_node,
                property,
            )?;
        }
    }
    for (track_index, track) in animation.morph_tracks.iter().enumerate() {
        let morph_node = animation_curvenode_morph_id(track_index);
        let morph_curve = animation_curve_morph_id(track_index);
        let blend_shape_channel = blend_shape_channel_id(track.export_index);

        write_oo_connection(writer, morph_node, ANIM_LAYER_ID)?;
        write_op_connection(writer, morph_node, blend_shape_channel, "DeformPercent")?;
        write_op_connection(writer, morph_curve, morph_node, "d|DeformPercent")?;
    }
    Ok(())
}

fn bone_model_id(index: usize) -> i64 {
    BONE_MODEL_ID_BASE + index as i64
}

fn bone_attr_id(index: usize) -> i64 {
    BONE_ATTR_ID_BASE + index as i64
}

fn cluster_id(index: usize) -> i64 {
    CLUSTER_ID_BASE + index as i64
}

fn blend_shape_id(export_index: usize) -> i64 {
    BLEND_SHAPE_ID_BASE + export_index as i64
}

fn blend_shape_channel_id(export_index: usize) -> i64 {
    BLEND_SHAPE_CHANNEL_ID_BASE + export_index as i64
}

fn shape_geometry_id(export_index: usize) -> i64 {
    SHAPE_GEOMETRY_ID_BASE + export_index as i64
}

fn animation_curvenode_rotation_id(bone_index: usize) -> i64 {
    ANIM_CURVENODE_ROT_BASE + bone_index as i64
}

fn animation_curvenode_translation_id(bone_index: usize) -> i64 {
    ANIM_CURVENODE_TRANS_BASE + bone_index as i64
}

fn animation_curve_id(bone_index: usize, channel: usize) -> i64 {
    ANIM_CURVE_BASE + (bone_index * 6 + channel) as i64
}

fn animation_curvenode_morph_id(track_index: usize) -> i64 {
    ANIM_CURVENODE_MORPH_BASE + track_index as i64
}

fn animation_curve_morph_id(track_index: usize) -> i64 {
    ANIM_CURVE_MORPH_BASE + track_index as i64
}

fn write_oo_connection<W: Write + Seek>(
    writer: &mut Writer<W>,
    child_id: i64,
    parent_id: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, "C", |attrs| {
        attrs.append_string_direct("OO")?;
        attrs.append_i64(child_id)?;
        attrs.append_i64(parent_id)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_op_connection<W: Write + Seek>(
    writer: &mut Writer<W>,
    child_id: i64,
    parent_id: i64,
    property: &str,
) -> Result<(), FbxExportError> {
    begin_node(writer, "C", |attrs| {
        attrs.append_string_direct("OP")?;
        attrs.append_i64(child_id)?;
        attrs.append_i64(parent_id)?;
        attrs.append_string_direct(property)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn begin_node<W, F>(
    writer: &mut Writer<W>,
    name: &str,
    append_attrs: F,
) -> Result<(), FbxExportError>
where
    W: Write + Seek,
    F: FnOnce(&mut AttributesWriter<'_, W>) -> Result<(), FbxExportError>,
{
    {
        let mut attrs = writer.new_node(name)?;
        append_attrs(&mut attrs)?;
    }
    Ok(())
}

fn write_bool_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: bool,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_bool(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_i32_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_i32(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_i64_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_i64(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_f64_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: f64,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_f64(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_string_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: &str,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_string_direct(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_arr_i32_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    values: &[i32],
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs
            .append_arr_i32_from_iter(Some(ArrayAttributeEncoding::Zlib), values.iter().copied())?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_arr_i64_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    values: &[i64],
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs
            .append_arr_i64_from_iter(Some(ArrayAttributeEncoding::Zlib), values.iter().copied())?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_arr_f32_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    values: &[f32],
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs
            .append_arr_f32_from_iter(Some(ArrayAttributeEncoding::Zlib), values.iter().copied())?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_arr_f64_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    values: &[f64],
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs
            .append_arr_f64_from_iter(Some(ArrayAttributeEncoding::Zlib), values.iter().copied())?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_i32<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        attrs.append_i32(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_i64<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        attrs.append_i64(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_f64<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: f64,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        attrs.append_f64(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_compound<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_string<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    value: &str,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct("")?;
        attrs.append_string_direct(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_vec3<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: [f64; 3],
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        attrs.append_f64(value[0])?;
        attrs.append_f64(value[1])?;
        attrs.append_f64(value[2])?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_color<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: [f64; 3],
) -> Result<(), FbxExportError> {
    write_property_vec3(writer, name, type_name, label, flags, value)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use fbxcel::{low::v7400::AttributeValue, tree::any::AnyTree};
    use glam::Vec3;
    use mmd_anim_runtime::{
        DensePoseSequenceView, ReductionTolerances, SkeletonSnapshot, reduce_dense_pose_sequence,
    };

    use super::*;

    #[test]
    fn reduced_export_equality_ignores_nondeterministic_timings() {
        let first = FbxReducedPoseExport {
            bytes: vec![1, 2, 3],
            report: PoseReductionReport::default(),
            work_stats: ReductionWorkStats::default(),
            timings: ReductionTimings {
                candidate_build: Duration::from_millis(1),
                error_measure: Duration::from_millis(2),
                dcc_fit: Duration::from_millis(1),
            },
        };
        let mut second = first.clone();
        second.timings = ReductionTimings {
            candidate_build: Duration::from_secs(1),
            error_measure: Duration::from_secs(2),
            dcc_fit: Duration::from_secs(1),
        };
        assert_eq!(first, second);
    }

    fn runtime_baked_fixture_fbx() -> (PmxParsedModel, Vec<u8>) {
        let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
        let vmd_data = include_bytes!("../../fixtures/vmd/ik_multi_bone_nondefault.vmd");
        let model = crate::parse_pmx_model(pmx_data).expect("PMX fixture should parse");
        let runtime_import =
            crate::import_pmx_runtime(pmx_data).expect("PMX runtime fixture should import");
        let runtime_motion =
            crate::import_vmd_motion(vmd_data).expect("VMD runtime fixture should import");
        let parsed_motion = crate::parse_vmd_animation(vmd_data).expect("VMD fixture should parse");
        let clip = crate::build_pair_clip(
            &runtime_motion,
            &runtime_import.bone_name_to_index,
            &runtime_import.morph_name_to_index,
            &runtime_import.ik_solver_bone_name_to_index,
            runtime_import.model.ik_count(),
        );
        let last_frame = parsed_motion
            .bone_frames
            .iter()
            .map(|frame| frame.frame)
            .max()
            .unwrap_or(0);
        let fbx = export_fbx_with_runtime_bake(
            &model,
            Arc::new(runtime_import.model),
            &clip,
            last_frame,
            &FbxExportOptions::default(),
        )
        .expect("runtime-baked FBX should export");
        (model, fbx)
    }

    fn reduced_fbx_fixture() -> (PmxParsedModel, ReducedPoseSequence, usize) {
        let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
        let model = crate::parse_pmx_model(pmx_data).unwrap();
        let runtime = crate::import_pmx_runtime(pmx_data).unwrap().model;
        let snapshot = SkeletonSnapshot::from_model(&runtime, 123).unwrap();
        let root = snapshot
            .parent_indices()
            .iter()
            .position(|parent| *parent < 0)
            .unwrap();
        let frame_count = 9;
        let mut world = Vec::with_capacity(frame_count * snapshot.bone_count());
        for frame in 0..frame_count {
            let t = frame as f32 / (frame_count - 1) as f32;
            let peak = 4.0 * t * (1.0 - t);
            let mut frame_world = vec![Mat4::IDENTITY; snapshot.bone_count()];
            let mut resolved = vec![false; snapshot.bone_count()];
            fn resolve(
                bone: usize,
                root: usize,
                peak: f32,
                snapshot: &SkeletonSnapshot,
                world: &mut [Mat4],
                resolved: &mut [bool],
            ) {
                if resolved[bone] {
                    return;
                }
                let parent = snapshot.parent_indices()[bone];
                if parent >= 0 {
                    resolve(parent as usize, root, peak, snapshot, world, resolved);
                }
                let mut translation = snapshot.rest_local_translations()[bone];
                if bone == root {
                    translation.x += peak;
                }
                let local = Mat4::from_rotation_translation(
                    snapshot.rest_local_rotations()[bone],
                    Vec3::from(translation),
                );
                world[bone] = if parent < 0 {
                    local
                } else {
                    world[parent as usize] * local
                };
                resolved[bone] = true;
            }
            for bone in 0..snapshot.bone_count() {
                resolve(bone, root, peak, &snapshot, &mut frame_world, &mut resolved);
            }
            world.extend(frame_world);
        }
        let morphs = vec![0.0; frame_count * snapshot.morph_count()];
        let reduced = reduce_dense_pose_sequence(
            DensePoseSequenceView::new(
                &world,
                &morphs,
                frame_count,
                snapshot.bone_count(),
                snapshot.morph_count(),
                0.0,
                1.0,
            )
            .unwrap(),
            snapshot,
            ReductionTolerances {
                local_position: 0.01,
                world_position: 0.01,
                ..Default::default()
            },
            ReductionTarget::DccCubic,
        )
        .unwrap();
        (model, reduced, root)
    }

    #[test]
    fn reduced_pose_writes_sparse_per_key_user_tangents_and_replays_samples() {
        let (model, reduced, root) = reduced_fbx_fixture();
        let fbx = export_pmx_fbx_binary_with_reduced_pose(
            &model,
            &reduced,
            123,
            &FbxExportOptions {
                flip_z: false,
                ..Default::default()
            },
        )
        .unwrap();
        let tree = load_tree(&fbx);
        let root_node = tree.root();
        let objects = root_node.first_child_by_name("Objects").unwrap();
        let curve = objects
            .children_by_name("AnimationCurve")
            .find(|node| {
                node.attributes().first().and_then(AttributeValue::get_i64)
                    == Some(animation_curve_id(root, 3))
            })
            .unwrap();
        let times = child_arr_i64(curve, "KeyTime");
        let values = child_arr_f32(curve, "KeyValueFloat");
        let flags = child_arr_i32(curve, "KeyAttrFlags");
        let data = child_arr_f32(curve, "KeyAttrDataFloat");
        let refs = child_arr_i32(curve, "KeyAttrRefCount");
        assert_eq!(times.len(), 3);
        assert_eq!(times.len(), values.len());
        assert_eq!(flags.len(), values.len());
        assert_eq!(data.len(), values.len() * 4);
        assert_eq!(refs, vec![1; values.len()]);
        assert!(flags[..flags.len() - 1].iter().all(|flag| *flag == 0x408));
        assert_eq!(flags[flags.len() - 1], 0x404);

        for frame in 0..9 {
            let time = frame as f32 / 30.0;
            let upper = times
                .partition_point(|key| (*key as f64 / FBX_TIME_ONE_SECOND as f64) <= time as f64);
            let actual = if upper == 0 {
                values[0]
            } else if upper == values.len() {
                values[values.len() - 1]
            } else {
                let left = upper - 1;
                let right = upper;
                let left_time = times[left] as f32 / FBX_TIME_ONE_SECOND as f32;
                let right_time = times[right] as f32 / FBX_TIME_ONE_SECOND as f32;
                let amount = (time - left_time) / (right_time - left_time);
                let duration = right_time - left_time;
                let t2 = amount * amount;
                let t3 = t2 * amount;
                (2.0 * t3 - 3.0 * t2 + 1.0) * values[left]
                    + (t3 - 2.0 * t2 + amount) * duration * data[left * 4]
                    + (-2.0 * t3 + 3.0 * t2) * values[right]
                    + (t3 - t2) * duration * data[left * 4 + 1]
            };
            let t = frame as f32 / 8.0;
            let expected =
                reduced.snapshot().rest_local_translations()[root].x + 4.0 * t * (1.0 - t);
            assert!(
                (actual - expected).abs() <= 0.01,
                "{frame}: {actual} {expected}"
            );
        }
    }

    #[test]
    fn unity_direct_clip_dto_keeps_sparse_times_and_tangents_separate_from_fbx_import() {
        let (_model, reduced, root) = reduced_fbx_fixture();
        let mut bone_paths = (0..reduced.snapshot().bone_count())
            .map(|bone| format!("root/bone{bone}"))
            .collect::<Vec<_>>();
        bone_paths[root] = "root/moving".to_owned();
        let dto = reduced_pose_to_unity_animation_clip(
            &reduced,
            &UnityReducedPoseBindings {
                model_identity: 123,
                bone_paths,
                morph_bindings: vec![None; reduced.snapshot().morph_count()],
            },
            false,
        )
        .unwrap();
        let curve = dto
            .curves
            .iter()
            .find(|curve| curve.path == "root/moving" && curve.property == "localPosition.x")
            .unwrap();
        assert_eq!(curve.keys.len(), 3);
        assert_eq!(curve.keys[0].time_seconds, 0.0);
        assert!((curve.keys[2].time_seconds - 8.0 / 30.0).abs() <= f32::EPSILON);
        assert!(curve.keys.iter().all(|key| {
            key.in_tangent.is_finite() && key.out_tangent.is_finite() && key.value.is_finite()
        }));
        assert!(dto.reduced_key_count < dto.source_key_count);
    }

    #[test]
    fn pose_source_seam_preserves_non_physics_runtime_bake_bytes() {
        struct RecordingPoseSource<'a> {
            runtime: RuntimeInstance,
            clip: &'a AnimationClip,
            frames: Vec<u32>,
            matrix_buffer_address: Option<usize>,
        }

        impl FbxPoseSource for RecordingPoseSource<'_> {
            fn world_matrices(&mut self, frame: u32) -> Result<&[Mat4], String> {
                self.frames.push(frame);
                self.runtime.evaluate_clip_frame(self.clip, frame as f32);
                let matrices = self.runtime.world_matrices();
                let address = matrices.as_ptr() as usize;
                assert_eq!(*self.matrix_buffer_address.get_or_insert(address), address);
                Ok(matrices)
            }
        }

        let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
        let vmd_data = include_bytes!("../../fixtures/vmd/ik_multi_bone_nondefault.vmd");
        let model = crate::parse_pmx_model(pmx_data).unwrap();
        let runtime_import = crate::import_pmx_runtime(pmx_data).unwrap();
        let runtime_motion = crate::import_vmd_motion(vmd_data).unwrap();
        let clip = crate::build_pair_clip(
            &runtime_motion,
            &runtime_import.bone_name_to_index,
            &runtime_import.morph_name_to_index,
            &runtime_import.ik_solver_bone_name_to_index,
            runtime_import.model.ik_count(),
        );
        let runtime_model = Arc::new(runtime_import.model);
        let expected = export_fbx_with_runtime_bake(
            &model,
            Arc::clone(&runtime_model),
            &clip,
            2,
            &FbxExportOptions::default(),
        )
        .unwrap();
        let mut pose_source = RecordingPoseSource {
            runtime: RuntimeInstance::new(Arc::clone(&runtime_model)),
            clip: &clip,
            frames: Vec::new(),
            matrix_buffer_address: None,
        };
        let actual = export_pmx_fbx_binary_with_pose_source(
            &model,
            runtime_model,
            &clip,
            2,
            &FbxExportOptions::default(),
            &mut pose_source,
        )
        .unwrap();

        assert_eq!(pose_source.frames, vec![0, 1, 2]);
        assert_eq!(actual, expected);
    }

    #[test]
    fn pose_source_validates_parent_matrix_outside_exported_bone_subset() {
        use mmd_anim_runtime::BoneInit;

        struct ShortPoseSource {
            matrices: Vec<Mat4>,
        }

        impl FbxPoseSource for ShortPoseSource {
            fn world_matrices(&mut self, _frame: u32) -> Result<&[Mat4], String> {
                Ok(&self.matrices)
            }
        }

        let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
        let mut model = crate::parse_pmx_model(pmx_data).unwrap();
        model.skeleton.bones.truncate(1);
        let runtime_model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(Some(BoneIndex(1)), glam::Vec3A::ZERO),
                BoneInit::new(None, glam::Vec3A::ZERO),
            ])
            .unwrap(),
        );
        let clip = AnimationClip::new(Vec::new());
        let mut source = ShortPoseSource {
            matrices: vec![Mat4::IDENTITY],
        };
        let options = FbxExportOptions {
            bones_only: true,
            ..FbxExportOptions::default()
        };

        let error = export_pmx_fbx_binary_with_pose_source(
            &model,
            runtime_model,
            &clip,
            0,
            &options,
            &mut source,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            FbxExportError::PoseBoneCount {
                frame: 0,
                expected: 2,
                actual: 1,
            }
        ));
    }

    fn load_tree(bytes: &[u8]) -> fbxcel::tree::v7400::Tree {
        match AnyTree::from_seekable_reader(Cursor::new(bytes)).expect("FBX should parse") {
            AnyTree::V7400(version, tree, footer) => {
                assert_eq!(version, FbxVersion::V7_4);
                assert_eq!(
                    footer.expect("FBX footer should parse").fbx_version,
                    FbxVersion::V7_4
                );
                tree
            }
            _ => panic!("FBX should be parsed as a v7400 tree"),
        }
    }

    fn child_i32(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> i32 {
        node.first_child_by_name(name)
            .and_then(|child| child.attributes().first())
            .and_then(AttributeValue::get_i32)
            .unwrap_or_else(|| panic!("missing i32 child node {name}"))
    }

    fn child_string<'a>(node: fbxcel::tree::v7400::NodeHandle<'a>, name: &str) -> &'a str {
        node.first_child_by_name(name)
            .and_then(|child| child.attributes().first())
            .and_then(AttributeValue::get_string)
            .unwrap_or_else(|| panic!("missing string child node {name}"))
    }

    fn child_arr_f32(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<f32> {
        node.first_child_by_name(name)
            .and_then(|child| child.attributes().first())
            .and_then(AttributeValue::get_arr_f32)
            .map(|values| values.to_vec())
            .unwrap_or_else(|| panic!("missing f32 array child node {name}"))
    }

    fn child_arr_i32(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<i32> {
        node.first_child_by_name(name)
            .and_then(|child| child.attributes().first())
            .and_then(AttributeValue::get_arr_i32)
            .map(|values| values.to_vec())
            .unwrap_or_else(|| panic!("missing i32 array child node {name}"))
    }

    fn child_arr_i64(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<i64> {
        node.first_child_by_name(name)
            .and_then(|child| child.attributes().first())
            .and_then(AttributeValue::get_arr_i64)
            .map(|values| values.to_vec())
            .unwrap_or_else(|| panic!("missing i64 array child node {name}"))
    }

    fn child_arr_f64(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<f64> {
        node.first_child_by_name(name)
            .and_then(|child| child.attributes().first())
            .and_then(AttributeValue::get_arr_f64)
            .map(|values| values.to_vec())
            .unwrap_or_else(|| panic!("missing f64 array child node {name}"))
    }

    fn object_type_count(definitions: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> i32 {
        definitions
            .children_by_name("ObjectType")
            .find(|node| {
                node.attributes()
                    .first()
                    .and_then(AttributeValue::get_string)
                    == Some(name)
            })
            .map(|node| child_i32(node, "Count"))
            .unwrap_or_else(|| panic!("missing ObjectType {name}"))
    }

    fn optional_object_type_count(
        definitions: fbxcel::tree::v7400::NodeHandle<'_>,
        name: &str,
    ) -> i32 {
        definitions
            .children_by_name("ObjectType")
            .find(|node| {
                node.attributes()
                    .first()
                    .and_then(AttributeValue::get_string)
                    == Some(name)
            })
            .map(|node| child_i32(node, "Count"))
            .unwrap_or(0)
    }

    fn descendants_by_name<'a>(
        tree: &'a fbxcel::tree::v7400::Tree,
        name: &str,
    ) -> Vec<fbxcel::tree::v7400::NodeHandle<'a>> {
        let mut traversal = tree.root().node_id().traverse_depth_first();
        let mut nodes = Vec::new();
        while let Some(node_id) = traversal.next_open_forward(tree) {
            let node = node_id.to_handle(tree);
            if node.name() == name {
                nodes.push(node);
            }
        }
        nodes
    }

    #[test]
    fn runtime_bake_export_writes_parseable_fbx_structure() {
        let (model, fbx) = runtime_baked_fixture_fbx();
        let tree = load_tree(&fbx);
        let root = tree.root();
        let definitions = root
            .first_child_by_name("Definitions")
            .expect("Definitions node should exist");
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        let connections = root
            .first_child_by_name("Connections")
            .expect("Connections node should exist");

        assert_eq!(objects.children_by_name("Geometry").count(), 1);
        assert_eq!(objects.children_by_name("Pose").count(), 1);
        assert_eq!(
            objects.children_by_name("Model").count(),
            model.skeleton.bones.len() + 1
        );
        assert_eq!(
            objects.children_by_name("NodeAttribute").count(),
            model.skeleton.bones.len()
        );
        assert_eq!(
            objects.children_by_name("Deformer").count(),
            model.skeleton.bones.len() + 1
        );

        let animation_curves = objects.children_by_name("AnimationCurve").count();
        let animation_curve_nodes = objects.children_by_name("AnimationCurveNode").count();
        let morph_animation_curve_nodes = objects
            .children_by_name("AnimationCurveNode")
            .filter(|node| {
                node.attributes()
                    .get(1)
                    .and_then(AttributeValue::get_string)
                    .is_some_and(|name| name.contains("DeformPercent"))
            })
            .count();
        let bone_animation_curve_nodes =
            animation_curve_nodes.saturating_sub(morph_animation_curve_nodes);
        assert!(animation_curves > 0);
        assert!(animation_curve_nodes > 0);
        assert!(bone_animation_curve_nodes.is_multiple_of(2));
        assert_eq!(
            animation_curves,
            bone_animation_curve_nodes / 2 * 6 + morph_animation_curve_nodes
        );
        assert_eq!(objects.children_by_name("AnimationStack").count(), 1);
        assert_eq!(objects.children_by_name("AnimationLayer").count(), 1);

        assert_eq!(
            child_i32(definitions, "Count") as usize,
            objects.children().count() + 1
        );
        assert_eq!(
            object_type_count(definitions, "Model") as usize,
            model.skeleton.bones.len() + 1
        );
        assert_eq!(
            object_type_count(definitions, "AnimationCurve") as usize,
            animation_curves
        );
        assert_eq!(
            object_type_count(definitions, "AnimationCurveNode") as usize,
            animation_curve_nodes
        );

        let clusters: Vec<_> = objects
            .children_by_name("Deformer")
            .filter(|node| {
                node.attributes()
                    .get(2)
                    .and_then(AttributeValue::get_string)
                    == Some("Cluster")
            })
            .collect();
        assert_eq!(clusters.len(), model.skeleton.bones.len());
        for cluster in clusters {
            let cluster_id = cluster
                .attributes()
                .first()
                .and_then(AttributeValue::get_i64)
                .expect("Cluster should have an object id");
            let bone_index = usize::try_from(cluster_id - CLUSTER_ID_BASE)
                .expect("Cluster id should map to a bone index");
            let bone = &model.skeleton.bones[bone_index];
            assert_eq!(
                child_arr_f64(cluster, "Transform"),
                bone_world_transform_inverse(bone, &FbxExportOptions::default()),
                "Cluster Transform should be the inverse bone bind matrix"
            );
            assert_eq!(
                child_arr_f64(cluster, "TransformLink"),
                bone_world_transform(bone, &FbxExportOptions::default()),
                "Cluster TransformLink should be the bone bind matrix"
            );
        }

        let pose = objects
            .first_child_by_name("Pose")
            .expect("Bind pose should exist");
        assert_eq!(child_string(pose, "Type"), "BindPose");
        assert_eq!(
            child_i32(pose, "NbPoseNodes") as usize,
            model.skeleton.bones.len() + 1
        );
        assert_eq!(
            pose.children_by_name("PoseNode").count(),
            model.skeleton.bones.len() + 1
        );

        let connection_kinds: Vec<_> = connections
            .children_by_name("C")
            .filter_map(|node| {
                node.attributes()
                    .first()
                    .and_then(AttributeValue::get_string)
            })
            .collect();
        assert!(connection_kinds.contains(&"OO"));
        assert!(connection_kinds.contains(&"OP"));
    }

    #[test]
    fn runtime_bake_export_writes_monotonic_animation_keys() {
        let (_model, fbx) = runtime_baked_fixture_fbx();
        let tree = load_tree(&fbx);
        let curves = descendants_by_name(&tree, "AnimationCurve");
        assert!(!curves.is_empty());

        for curve in curves {
            let key_times = curve
                .first_child_by_name("KeyTime")
                .and_then(|node| node.attributes().first())
                .and_then(AttributeValue::get_arr_i64)
                .expect("AnimationCurve should have KeyTime values");
            let key_values = curve
                .first_child_by_name("KeyValueFloat")
                .and_then(|node| node.attributes().first())
                .and_then(AttributeValue::get_arr_f32)
                .expect("AnimationCurve should have KeyValueFloat values");
            assert_eq!(key_times.len(), key_values.len());
            assert!(key_times.len() >= 2);
            assert!(
                key_times.windows(2).all(|window| window[0] < window[1]),
                "FBX KeyTime values should be strictly monotonic"
            );
        }
    }

    #[test]
    fn bones_only_export_omits_mesh_skin_material_pose_and_morphs() {
        let (model, _fbx) = runtime_baked_fixture_fbx();
        let options = FbxExportOptions {
            bones_only: true,
            ..FbxExportOptions::default()
        };
        let fbx = export_fbx(&model, None, &options).expect("bones-only FBX should export");
        let tree = load_tree(&fbx);
        let root = tree.root();
        let definitions = root
            .first_child_by_name("Definitions")
            .expect("Definitions node should exist");
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        let connections = root
            .first_child_by_name("Connections")
            .expect("Connections node should exist");

        assert_eq!(objects.children_by_name("Geometry").count(), 0);
        assert_eq!(objects.children_by_name("Material").count(), 0);
        assert_eq!(objects.children_by_name("Texture").count(), 0);
        assert_eq!(objects.children_by_name("Video").count(), 0);
        assert_eq!(objects.children_by_name("Deformer").count(), 0);
        assert_eq!(objects.children_by_name("Pose").count(), 0);
        assert_eq!(
            objects.children_by_name("Model").count(),
            model.skeleton.bones.len() + 1
        );
        assert_eq!(
            objects.children_by_name("NodeAttribute").count(),
            model.skeleton.bones.len()
        );
        assert_eq!(optional_object_type_count(definitions, "Geometry"), 0);
        assert_eq!(optional_object_type_count(definitions, "Material"), 0);
        assert_eq!(optional_object_type_count(definitions, "Texture"), 0);
        assert_eq!(optional_object_type_count(definitions, "Video"), 0);
        assert_eq!(optional_object_type_count(definitions, "Deformer"), 0);
        assert_eq!(optional_object_type_count(definitions, "Pose"), 0);
        assert_eq!(
            child_i32(definitions, "Count") as usize,
            objects.children().count() + 1
        );

        let root_model = objects
            .children_by_name("Model")
            .find(|node| {
                node.attributes().first().and_then(AttributeValue::get_i64) == Some(MODEL_ID)
            })
            .expect("root model should exist");
        assert_eq!(
            root_model
                .attributes()
                .get(2)
                .and_then(AttributeValue::get_string),
            Some("Null")
        );
        assert!(has_oo_connection(connections, MODEL_ID, ROOT_NODE_ID));
        assert!(!has_oo_connection(connections, GEOMETRY_ID, MODEL_ID));
        assert!(!has_oo_connection(connections, SKIN_ID, GEOMETRY_ID));
        assert!(!has_oo_connection(connections, cluster_id(0), SKIN_ID));
        assert!(has_oo_connection(
            connections,
            bone_attr_id(0),
            bone_model_id(0)
        ));
    }

    #[test]
    fn bones_only_runtime_bake_keeps_bone_animation_without_morph_curves() {
        let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
        let vmd_data = include_bytes!("../../fixtures/vmd/ik_multi_bone_nondefault.vmd");
        let model = crate::parse_pmx_model(pmx_data).expect("PMX fixture should parse");
        let runtime_import =
            crate::import_pmx_runtime(pmx_data).expect("PMX runtime fixture should import");
        let runtime_motion =
            crate::import_vmd_motion(vmd_data).expect("VMD runtime fixture should import");
        let parsed_motion = crate::parse_vmd_animation(vmd_data).expect("VMD fixture should parse");
        let clip = crate::build_pair_clip(
            &runtime_motion,
            &runtime_import.bone_name_to_index,
            &runtime_import.morph_name_to_index,
            &runtime_import.ik_solver_bone_name_to_index,
            runtime_import.model.ik_count(),
        );
        let last_frame = parsed_motion
            .bone_frames
            .iter()
            .map(|frame| frame.frame)
            .max()
            .unwrap_or(0);
        let options = FbxExportOptions {
            bones_only: true,
            ..FbxExportOptions::default()
        };
        let fbx = export_fbx_with_runtime_bake(
            &model,
            Arc::new(runtime_import.model),
            &clip,
            last_frame,
            &options,
        )
        .expect("bones-only runtime-baked FBX should export");
        let tree = load_tree(&fbx);
        let root = tree.root();
        let definitions = root
            .first_child_by_name("Definitions")
            .expect("Definitions node should exist");
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        let connections = root
            .first_child_by_name("Connections")
            .expect("Connections node should exist");

        assert_eq!(objects.children_by_name("AnimationStack").count(), 1);
        assert_eq!(objects.children_by_name("AnimationLayer").count(), 1);
        let animation_curves = objects.children_by_name("AnimationCurve").count();
        let animation_curve_nodes = objects.children_by_name("AnimationCurveNode").count();
        assert!(animation_curves > 0);
        assert!(animation_curve_nodes > 0);
        assert_eq!(animation_curves, animation_curve_nodes / 2 * 6);
        assert_eq!(
            optional_object_type_count(definitions, "AnimationCurve") as usize,
            animation_curves
        );
        assert_eq!(
            objects
                .children_by_name("AnimationCurveNode")
                .filter(|node| {
                    node.attributes()
                        .get(1)
                        .and_then(AttributeValue::get_string)
                        .is_some_and(|name| name.contains("DeformPercent"))
                })
                .count(),
            0
        );
        assert!(connections.children_by_name("C").any(|node| {
            let attrs = node.attributes();
            attrs.first().and_then(AttributeValue::get_string) == Some("OP")
                && attrs.get(3).and_then(AttributeValue::get_string) == Some("Lcl Rotation")
        }));
    }

    #[test]
    fn fbx_export_writes_diffuse_texture_references() {
        let (mut model, _fbx) = runtime_baked_fixture_fbx();
        model.materials[0].texture_path = "textures/diffuse.png".to_owned();
        let fbx =
            export_fbx(&model, None, &FbxExportOptions::default()).expect("FBX should export");
        let tree = load_tree(&fbx);
        let root = tree.root();
        let definitions = root
            .first_child_by_name("Definitions")
            .expect("Definitions node should exist");
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        let connections = root
            .first_child_by_name("Connections")
            .expect("Connections node should exist");

        assert_eq!(object_type_count(definitions, "Texture"), 1);
        assert_eq!(object_type_count(definitions, "Video"), 1);

        let texture = objects
            .children_by_name("Texture")
            .next()
            .expect("Texture object should exist");
        let video = objects
            .children_by_name("Video")
            .next()
            .expect("Video object should exist");
        assert_eq!(child_string(texture, "FileName"), "textures/diffuse.png");
        assert_eq!(
            child_string(texture, "RelativeFilename"),
            "textures/diffuse.png"
        );
        assert_eq!(child_string(video, "FileName"), "textures/diffuse.png");
        assert_eq!(
            child_string(video, "RelativeFilename"),
            "textures/diffuse.png"
        );

        let has_texture_to_material = connections.children_by_name("C").any(|node| {
            let attrs = node.attributes();
            attrs.first().and_then(AttributeValue::get_string) == Some("OP")
                && attrs.get(1).and_then(AttributeValue::get_i64) == Some(TEXTURE_ID_BASE)
                && attrs.get(2).and_then(AttributeValue::get_i64) == Some(MATERIAL_ID_BASE)
                && attrs.get(3).and_then(AttributeValue::get_string) == Some("DiffuseColor")
        });
        let has_video_to_texture = connections.children_by_name("C").any(|node| {
            let attrs = node.attributes();
            attrs.first().and_then(AttributeValue::get_string) == Some("OO")
                && attrs.get(1).and_then(AttributeValue::get_i64) == Some(VIDEO_ID_BASE)
                && attrs.get(2).and_then(AttributeValue::get_i64) == Some(TEXTURE_ID_BASE)
        });
        assert!(has_texture_to_material);
        assert!(has_video_to_texture);
    }

    fn has_oo_connection(
        connections: fbxcel::tree::v7400::NodeHandle<'_>,
        child_id: i64,
        parent_id: i64,
    ) -> bool {
        connections.children_by_name("C").any(|node| {
            let attrs = node.attributes();
            attrs.first().and_then(AttributeValue::get_string) == Some("OO")
                && attrs.get(1).and_then(AttributeValue::get_i64) == Some(child_id)
                && attrs.get(2).and_then(AttributeValue::get_i64) == Some(parent_id)
        })
    }

    fn deformer_type(node: fbxcel::tree::v7400::NodeHandle<'_>) -> Option<&str> {
        node.attributes()
            .get(2)
            .and_then(AttributeValue::get_string)
    }

    #[test]
    fn fbx_export_writes_vertex_morph_blendshape() {
        use crate::pmx::{PmxParsedMorph, PmxParsedVertexMorphOffset};

        let (mut model, _fbx) = runtime_baked_fixture_fbx();
        model.geometry.positions = vec![
            0.0, 0.0, 0.0, //
            1.0, 0.0, 0.0, //
            0.0, 1.0, 0.0,
        ];
        model.geometry.normals = vec![0.0; 9];
        model.geometry.uvs = vec![0.0; 6];
        model.geometry.indices = vec![0, 1, 2];
        model.geometry.skin_indices = vec![0; 12];
        model.geometry.skin_weights = vec![1.0; 12];
        model.metadata.counts.vertices = 3;
        model.metadata.counts.faces = 1;
        model.morphs = vec![
            PmxParsedMorph {
                name: "Smile".to_owned(),
                english_name: "smile".to_owned(),
                kind: "vertex".to_owned(),
                vertex_offsets: vec![PmxParsedVertexMorphOffset {
                    vertex_index: 1,
                    position: [0.0, 0.5, 0.25],
                }],
                group_offsets: Vec::new(),
                bone_offsets: Vec::new(),
                uv_offsets: Vec::new(),
                additional_uv_offsets: Vec::new(),
                material_offsets: Vec::new(),
                flip_offsets: Vec::new(),
                impulse_offsets: Vec::new(),
            },
            PmxParsedMorph {
                name: "BoneMorph".to_owned(),
                english_name: "bone".to_owned(),
                kind: "bone".to_owned(),
                vertex_offsets: Vec::new(),
                group_offsets: Vec::new(),
                bone_offsets: Vec::new(),
                uv_offsets: Vec::new(),
                additional_uv_offsets: Vec::new(),
                material_offsets: Vec::new(),
                flip_offsets: Vec::new(),
                impulse_offsets: Vec::new(),
            },
        ];
        model.metadata.counts.morphs = model.morphs.len();

        let fbx =
            export_fbx(&model, None, &FbxExportOptions::default()).expect("FBX should export");
        let tree = load_tree(&fbx);
        let root = tree.root();
        let definitions = root
            .first_child_by_name("Definitions")
            .expect("Definitions node should exist");
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        let connections = root
            .first_child_by_name("Connections")
            .expect("Connections node should exist");

        assert_eq!(objects.children_by_name("Geometry").count(), 2);
        assert_eq!(
            objects
                .children_by_name("Deformer")
                .filter(|node| deformer_type(*node) == Some("BlendShape"))
                .count(),
            1
        );
        assert_eq!(
            objects
                .children_by_name("Deformer")
                .filter(|node| deformer_type(*node) == Some("BlendShapeChannel"))
                .count(),
            1
        );
        assert_eq!(object_type_count(definitions, "Geometry"), 2);
        assert_eq!(
            object_type_count(definitions, "Deformer") as usize,
            model.skeleton.bones.len() + 1 + 2
        );
        assert_eq!(
            child_i32(definitions, "Count") as usize,
            objects.children().count() + 1
        );

        let shape = objects
            .children_by_name("Geometry")
            .find(|node| {
                node.attributes()
                    .get(2)
                    .and_then(AttributeValue::get_string)
                    == Some("Shape")
            })
            .expect("Shape geometry should exist");
        let shape_vertices = child_arr_f64(shape, "Vertices");
        assert_eq!(child_arr_i32(shape, "Indexes"), vec![1]);
        assert_eq!(shape_vertices, vec![0.0, 0.5, -0.25]);

        assert!(has_oo_connection(
            connections,
            shape_geometry_id(0),
            blend_shape_channel_id(0)
        ));
        assert!(has_oo_connection(
            connections,
            blend_shape_channel_id(0),
            blend_shape_id(0)
        ));
        assert!(has_oo_connection(
            connections,
            blend_shape_id(0),
            GEOMETRY_ID
        ));
    }

    fn has_op_connection(
        connections: fbxcel::tree::v7400::NodeHandle<'_>,
        child_id: i64,
        parent_id: i64,
        property: &str,
    ) -> bool {
        connections.children_by_name("C").any(|node| {
            let attrs = node.attributes();
            attrs.first().and_then(AttributeValue::get_string) == Some("OP")
                && attrs.get(1).and_then(AttributeValue::get_i64) == Some(child_id)
                && attrs.get(2).and_then(AttributeValue::get_i64) == Some(parent_id)
                && attrs.get(3).and_then(AttributeValue::get_string) == Some(property)
        })
    }

    #[test]
    fn vmd_export_writes_vertex_morph_weight_animation() {
        use crate::pmx::{PmxParsedMorph, PmxParsedVertexMorphOffset};
        use crate::vmd::{
            VmdParsedAnimation, VmdParsedCounts, VmdParsedMetadata, VmdParsedMorphFrame,
        };

        let (mut model, _fbx) = runtime_baked_fixture_fbx();
        model.geometry.positions = vec![
            0.0, 0.0, 0.0, //
            1.0, 0.0, 0.0, //
            0.0, 1.0, 0.0,
        ];
        model.geometry.normals = vec![0.0; 9];
        model.geometry.uvs = vec![0.0; 6];
        model.geometry.indices = vec![0, 1, 2];
        model.geometry.skin_indices = vec![0; 12];
        model.geometry.skin_weights = vec![1.0; 12];
        model.metadata.counts.vertices = 3;
        model.metadata.counts.faces = 1;
        model.morphs = vec![PmxParsedMorph {
            name: "Smile".to_owned(),
            english_name: "smile".to_owned(),
            kind: "vertex".to_owned(),
            vertex_offsets: vec![PmxParsedVertexMorphOffset {
                vertex_index: 1,
                position: [0.0, 0.5, 0.25],
            }],
            group_offsets: Vec::new(),
            bone_offsets: Vec::new(),
            uv_offsets: Vec::new(),
            additional_uv_offsets: Vec::new(),
            material_offsets: Vec::new(),
            flip_offsets: Vec::new(),
            impulse_offsets: Vec::new(),
        }];
        model.metadata.counts.morphs = model.morphs.len();

        let vmd = VmdParsedAnimation {
            kind: "vmd",
            metadata: VmdParsedMetadata {
                format: "vmd",
                model_name: "fixture".to_owned(),
                model_name_bytes: Vec::new(),
                counts: VmdParsedCounts {
                    bones: 0,
                    morphs: 2,
                    cameras: 0,
                    lights: 0,
                    self_shadows: 0,
                    properties: 0,
                },
                max_frame: 30,
            },
            bone_frames: Vec::new(),
            morph_frames: vec![
                VmdParsedMorphFrame {
                    morph_name: "smile".to_owned(),
                    morph_name_bytes: Vec::new(),
                    frame: 0,
                    weight: 0.0,
                },
                VmdParsedMorphFrame {
                    morph_name: "smile".to_owned(),
                    morph_name_bytes: Vec::new(),
                    frame: 30,
                    weight: 0.75,
                },
            ],
            camera_frames: Vec::new(),
            light_frames: Vec::new(),
            self_shadow_frames: Vec::new(),
            property_frames: Vec::new(),
        };

        let fbx = export_fbx(&model, Some(&vmd), &FbxExportOptions::default())
            .expect("VMD morph FBX should export");
        let tree = load_tree(&fbx);
        let root = tree.root();
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        let connections = root
            .first_child_by_name("Connections")
            .expect("Connections node should exist");

        let morph_curve_nodes: Vec<_> = objects
            .children_by_name("AnimationCurveNode")
            .filter(|node| {
                node.attributes()
                    .get(1)
                    .and_then(AttributeValue::get_string)
                    .is_some_and(|name| name.contains("DeformPercent"))
            })
            .collect();
        assert_eq!(morph_curve_nodes.len(), 1);

        let morph_curves: Vec<_> = objects
            .children_by_name("AnimationCurve")
            .filter(|node| {
                node.attributes().first().and_then(AttributeValue::get_i64)
                    == Some(animation_curve_morph_id(0))
            })
            .collect();
        assert_eq!(morph_curves.len(), 1);

        let morph_curve = morph_curves[0];
        assert_eq!(child_arr_f32(morph_curve, "KeyValueFloat"), vec![0.0, 75.0]);

        let morph_node_id = animation_curvenode_morph_id(0);
        let morph_curve_id = animation_curve_morph_id(0);
        assert!(has_oo_connection(connections, morph_node_id, ANIM_LAYER_ID));
        assert!(has_op_connection(
            connections,
            morph_node_id,
            blend_shape_channel_id(0),
            "DeformPercent"
        ));
        assert!(has_op_connection(
            connections,
            morph_curve_id,
            morph_node_id,
            "d|DeformPercent"
        ));
    }

    #[test]
    fn runtime_bake_export_writes_vertex_morph_weight_animation() {
        use crate::pmx::{PmxParsedMorph, PmxParsedVertexMorphOffset};
        use mmd_anim_runtime::{
            AnimationClip, BoneInit, MorphAnimationBinding, MorphIndex, MorphInit, MorphKeyframe,
            MorphOffsetSpan, MorphTrack,
        };

        let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
        let mut model = crate::parse_pmx_model(pmx_data).expect("PMX fixture should parse");
        let runtime_model = mmd_anim_runtime::ModelArena::new_with_morphs(
            vec![BoneInit::new(None, glam::Vec3A::ZERO)],
            Vec::new(),
            Vec::new(),
            MorphInit {
                morph_count: 1,
                vertex_spans: vec![MorphOffsetSpan::default()],
                bone_spans: vec![MorphOffsetSpan::default()],
                group_spans: vec![MorphOffsetSpan::default()],
                ..MorphInit::default()
            },
        )
        .expect("runtime model with one morph slot should build");

        model.geometry.positions = vec![
            0.0, 0.0, 0.0, //
            1.0, 0.0, 0.0, //
            0.0, 1.0, 0.0,
        ];
        model.geometry.normals = vec![0.0; 9];
        model.geometry.uvs = vec![0.0; 6];
        model.geometry.indices = vec![0, 1, 2];
        model.geometry.skin_indices = vec![0; 12];
        model.geometry.skin_weights = vec![1.0; 12];
        model.metadata.counts.vertices = 3;
        model.metadata.counts.faces = 1;
        model.morphs = vec![PmxParsedMorph {
            name: "Smile".to_owned(),
            english_name: "smile".to_owned(),
            kind: "vertex".to_owned(),
            vertex_offsets: vec![PmxParsedVertexMorphOffset {
                vertex_index: 1,
                position: [0.0, 0.5, 0.25],
            }],
            group_offsets: Vec::new(),
            bone_offsets: Vec::new(),
            uv_offsets: Vec::new(),
            additional_uv_offsets: Vec::new(),
            material_offsets: Vec::new(),
            flip_offsets: Vec::new(),
            impulse_offsets: Vec::new(),
        }];
        model.metadata.counts.morphs = model.morphs.len();

        let clip = AnimationClip::new_full(
            Vec::new(),
            vec![MorphAnimationBinding {
                morph: MorphIndex(0),
                track: MorphTrack::from_keyframes(vec![
                    MorphKeyframe::new(0, 0.0),
                    MorphKeyframe::new(2, 0.5),
                ]),
            }],
            None,
        );
        let fbx = export_fbx_with_runtime_bake(
            &model,
            Arc::new(runtime_model),
            &clip,
            2,
            &FbxExportOptions::default(),
        )
        .expect("runtime-baked morph FBX should export");
        let tree = load_tree(&fbx);
        let root = tree.root();
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        let connections = root
            .first_child_by_name("Connections")
            .expect("Connections node should exist");

        let morph_curve = objects
            .children_by_name("AnimationCurve")
            .find(|node| {
                node.attributes().first().and_then(AttributeValue::get_i64)
                    == Some(animation_curve_morph_id(0))
            })
            .expect("runtime-baked morph AnimationCurve should exist");
        assert_eq!(
            child_arr_f32(morph_curve, "KeyValueFloat"),
            vec![0.0, 25.0, 50.0]
        );

        let morph_node_id = animation_curvenode_morph_id(0);
        let morph_curve_id = animation_curve_morph_id(0);
        assert!(has_oo_connection(connections, morph_node_id, ANIM_LAYER_ID));
        assert!(has_op_connection(
            connections,
            morph_node_id,
            blend_shape_channel_id(0),
            "DeformPercent"
        ));
        assert!(has_op_connection(
            connections,
            morph_curve_id,
            morph_node_id,
            "d|DeformPercent"
        ));
    }

    fn test_bone(name: &str, english_name: &str) -> PmxParsedBone {
        use crate::pmx::PmxParsedBoneFlags;

        PmxParsedBone {
            name: name.to_owned(),
            english_name: english_name.to_owned(),
            parent_index: -1,
            layer: 0,
            position: [0.0, 0.0, 0.0],
            tail_index: -1,
            tail_position: Some([0.0, 1.0, 0.0]),
            flags: PmxParsedBoneFlags {
                indexed_tail: false,
                rotatable: true,
                translatable: true,
                visible: true,
                enabled: true,
                ik: false,
                append_local: false,
                append_rotate: false,
                append_translate: false,
                fixed_axis: false,
                local_axis: false,
                transform_after_physics: false,
                external_parent_transform: false,
            },
            append_transform: None,
            fixed_axis: None,
            local_axis: None,
            external_parent_key: None,
            ik: None,
        }
    }

    fn fbx_object_label(name_attr: &str) -> &str {
        name_attr.split('\0').next().unwrap_or(name_attr)
    }

    fn limb_joint_object_labels(tree: &fbxcel::tree::v7400::Tree, kind: &str) -> Vec<String> {
        let objects = tree
            .root()
            .first_child_by_name("Objects")
            .expect("Objects node should exist");
        objects
            .children_by_name(kind)
            .filter_map(|node| {
                let id = node
                    .attributes()
                    .first()
                    .and_then(AttributeValue::get_i64)?;
                if id < BONE_MODEL_ID_BASE {
                    return None;
                }
                let label = node
                    .attributes()
                    .get(1)
                    .and_then(AttributeValue::get_string)
                    .map(fbx_object_label)?;
                Some(label.to_owned())
            })
            .collect()
    }

    #[test]
    fn legacy_bone_name_policy_hex_encodes_japanese_names() {
        let bones = vec![test_bone("センター", "")];
        let names = build_bone_names(&bones, FbxBoneNamePolicy::LegacyHex);
        assert_eq!(names[0], japanese_to_ascii("センター"));
        assert_ne!(names[0], "center");
    }

    #[test]
    fn readable_bone_name_policy_maps_standard_mmd_names() {
        let bones = vec![
            test_bone("センター", ""),
            test_bone("左足", ""),
            test_bone("全ての親", ""),
        ];
        let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
        assert_eq!(names, vec!["center", "left_leg", "master"]);
    }

    #[test]
    fn readable_bone_name_policy_uses_standard_dictionary_before_partial_ascii() {
        let bones = vec![test_bone("左足ＩＫ", "")];
        let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
        assert_eq!(names[0], "left_leg_ik");
    }

    #[test]
    fn readable_bone_name_policy_uses_ascii_pmx_name() {
        let bones = vec![test_bone("left arm.01", "")];
        let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
        assert_eq!(names[0], "left_arm_01");
    }

    #[test]
    fn readable_bone_name_policy_prefers_pmx_english_name() {
        let bones = vec![test_bone("センター", "RootCenter")];
        let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
        assert_eq!(names[0], "RootCenter");
    }

    #[test]
    fn readable_bone_name_policy_sanitizes_english_names() {
        let bones = vec![test_bone("センター", "Left Arm-01")];
        let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
        assert_eq!(names[0], "Left_Arm_01");
    }

    #[test]
    fn readable_bone_name_policy_deduplicates_collisions() {
        let bones = vec![
            test_bone("custom1", "left_arm"),
            test_bone("custom2", "left_arm"),
            test_bone("custom3", "left_arm"),
        ];
        let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
        assert_eq!(names, vec!["left_arm", "left_arm_1", "left_arm_2"]);
    }

    #[test]
    fn readable_bone_name_policy_writes_readable_fbx_joint_names() {
        let (mut model, _) = runtime_baked_fixture_fbx();
        model.skeleton.bones[0].name = "センター".to_owned();
        model.skeleton.bones[0].english_name.clear();

        let legacy_fbx = export_fbx(
            &model,
            None,
            &FbxExportOptions {
                bones_only: true,
                ..FbxExportOptions::default()
            },
        )
        .expect("legacy FBX should export");
        let legacy_tree = load_tree(&legacy_fbx);
        let legacy_models = limb_joint_object_labels(&legacy_tree, "Model");
        let legacy_attrs = limb_joint_object_labels(&legacy_tree, "NodeAttribute");
        assert!(legacy_models.iter().any(|name| name.contains("E382")));
        assert!(legacy_attrs.iter().any(|name| name.contains("E382")));

        let readable_fbx = export_fbx(
            &model,
            None,
            &FbxExportOptions {
                bones_only: true,
                bone_name_policy: FbxBoneNamePolicy::Readable,
                ..FbxExportOptions::default()
            },
        )
        .expect("readable FBX should export");
        let readable_tree = load_tree(&readable_fbx);
        let readable_models = limb_joint_object_labels(&readable_tree, "Model");
        let readable_attrs = limb_joint_object_labels(&readable_tree, "NodeAttribute");
        assert!(readable_models.contains(&"center".to_owned()));
        assert!(readable_attrs.contains(&"center".to_owned()));
        assert!(!readable_models.iter().any(|name| name.contains("E382")));
        assert!(!readable_attrs.iter().any(|name| name.contains("E382")));
    }

    #[derive(Debug, PartialEq, Eq)]
    struct FbxStructuralAnimationCounts {
        definitions_object_count: i32,
        objects_child_count: usize,
        geometry_count: usize,
        pose_count: usize,
        model_count: usize,
        node_attribute_count: usize,
        deformer_count: usize,
        animation_curve_count: usize,
        animation_curve_node_count: usize,
        morph_animation_curve_node_count: usize,
        animation_stack_count: usize,
        animation_layer_count: usize,
        definitions_model_type_count: i32,
        definitions_animation_curve_type_count: i32,
        definitions_animation_curve_node_type_count: i32,
    }

    fn structural_animation_counts(
        tree: &fbxcel::tree::v7400::Tree,
    ) -> FbxStructuralAnimationCounts {
        let root = tree.root();
        let definitions = root
            .first_child_by_name("Definitions")
            .expect("Definitions node should exist");
        let objects = root
            .first_child_by_name("Objects")
            .expect("Objects node should exist");

        let animation_curve_node_count = objects.children_by_name("AnimationCurveNode").count();
        let morph_animation_curve_node_count = objects
            .children_by_name("AnimationCurveNode")
            .filter(|node| {
                node.attributes()
                    .get(1)
                    .and_then(AttributeValue::get_string)
                    .is_some_and(|name| name.contains("DeformPercent"))
            })
            .count();

        FbxStructuralAnimationCounts {
            definitions_object_count: child_i32(definitions, "Count"),
            objects_child_count: objects.children().count(),
            geometry_count: objects.children_by_name("Geometry").count(),
            pose_count: objects.children_by_name("Pose").count(),
            model_count: objects.children_by_name("Model").count(),
            node_attribute_count: objects.children_by_name("NodeAttribute").count(),
            deformer_count: objects.children_by_name("Deformer").count(),
            animation_curve_count: objects.children_by_name("AnimationCurve").count(),
            animation_curve_node_count,
            morph_animation_curve_node_count,
            animation_stack_count: objects.children_by_name("AnimationStack").count(),
            animation_layer_count: objects.children_by_name("AnimationLayer").count(),
            definitions_model_type_count: object_type_count(definitions, "Model"),
            definitions_animation_curve_type_count: object_type_count(
                definitions,
                "AnimationCurve",
            ),
            definitions_animation_curve_node_type_count: object_type_count(
                definitions,
                "AnimationCurveNode",
            ),
        }
    }

    fn fbx_connection_signatures(
        tree: &fbxcel::tree::v7400::Tree,
    ) -> Vec<(String, i64, i64, Option<String>)> {
        let connections = tree
            .root()
            .first_child_by_name("Connections")
            .expect("Connections node should exist");
        let mut signatures = connections
            .children_by_name("C")
            .map(|node| {
                let attrs = node.attributes();
                let kind = attrs
                    .first()
                    .and_then(AttributeValue::get_string)
                    .expect("connection kind should exist")
                    .to_owned();
                let child_id = attrs
                    .get(1)
                    .and_then(AttributeValue::get_i64)
                    .expect("connection child id should exist");
                let parent_id = attrs
                    .get(2)
                    .and_then(AttributeValue::get_i64)
                    .expect("connection parent id should exist");
                let property = attrs
                    .get(3)
                    .and_then(AttributeValue::get_string)
                    .map(str::to_owned);
                (kind, child_id, parent_id, property)
            })
            .collect::<Vec<_>>();
        signatures.sort();
        signatures
    }

    fn id_based_bone_animation_op_connections(
        tree: &fbxcel::tree::v7400::Tree,
    ) -> Vec<(i64, i64, String)> {
        let connections = tree
            .root()
            .first_child_by_name("Connections")
            .expect("Connections node should exist");
        let mut links = connections
            .children_by_name("C")
            .filter_map(|node| {
                let attrs = node.attributes();
                if attrs.first().and_then(AttributeValue::get_string) != Some("OP") {
                    return None;
                }
                let curve_node_id = attrs.get(1).and_then(AttributeValue::get_i64)?;
                let bone_model_id = attrs.get(2).and_then(AttributeValue::get_i64)?;
                let property = attrs.get(3).and_then(AttributeValue::get_string)?;
                if bone_model_id < BONE_MODEL_ID_BASE {
                    return None;
                }
                if property != "Lcl Rotation" && property != "Lcl Translation" {
                    return None;
                }
                Some((curve_node_id, bone_model_id, property.to_owned()))
            })
            .collect::<Vec<_>>();
        links.sort();
        links
    }

    #[test]
    fn readable_bone_name_policy_preserves_runtime_baked_structure_and_animation_ids() {
        let (mut model, _) = runtime_baked_fixture_fbx();
        model.skeleton.bones[0].name = "センター".to_owned();
        model.skeleton.bones[0].english_name.clear();

        let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
        let vmd_data = include_bytes!("../../fixtures/vmd/ik_multi_bone_nondefault.vmd");
        let runtime_import =
            crate::import_pmx_runtime(pmx_data).expect("PMX runtime fixture should import");
        let runtime_motion =
            crate::import_vmd_motion(vmd_data).expect("VMD runtime fixture should import");
        let parsed_motion = crate::parse_vmd_animation(vmd_data).expect("VMD fixture should parse");
        let clip = crate::build_pair_clip(
            &runtime_motion,
            &runtime_import.bone_name_to_index,
            &runtime_import.morph_name_to_index,
            &runtime_import.ik_solver_bone_name_to_index,
            runtime_import.model.ik_count(),
        );
        let last_frame = parsed_motion
            .bone_frames
            .iter()
            .map(|frame| frame.frame)
            .max()
            .unwrap_or(0);

        let runtime_model = Arc::new(runtime_import.model);
        let legacy_fbx = export_fbx_with_runtime_bake(
            &model,
            Arc::clone(&runtime_model),
            &clip,
            last_frame,
            &FbxExportOptions::default(),
        )
        .expect("legacy runtime-baked FBX should export");
        let readable_fbx = export_fbx_with_runtime_bake(
            &model,
            runtime_model,
            &clip,
            last_frame,
            &FbxExportOptions {
                bone_name_policy: FbxBoneNamePolicy::Readable,
                ..FbxExportOptions::default()
            },
        )
        .expect("readable runtime-baked FBX should export");

        let legacy_tree = load_tree(&legacy_fbx);
        let readable_tree = load_tree(&readable_fbx);

        assert_eq!(
            structural_animation_counts(&legacy_tree),
            structural_animation_counts(&readable_tree),
            "readable policy should not change FBX structural or animation object counts"
        );
        assert_eq!(
            fbx_connection_signatures(&legacy_tree),
            fbx_connection_signatures(&readable_tree),
            "readable policy should not change FBX connection topology"
        );
        assert_eq!(
            id_based_bone_animation_op_connections(&legacy_tree),
            id_based_bone_animation_op_connections(&readable_tree),
            "readable policy should not change model-id animation connections"
        );
        assert!(!id_based_bone_animation_op_connections(&legacy_tree).is_empty());

        let legacy_models = limb_joint_object_labels(&legacy_tree, "Model");
        let readable_models = limb_joint_object_labels(&readable_tree, "Model");
        let legacy_attrs = limb_joint_object_labels(&legacy_tree, "NodeAttribute");
        let readable_attrs = limb_joint_object_labels(&readable_tree, "NodeAttribute");
        assert!(legacy_models.iter().any(|name| name.contains("E382")));
        assert!(readable_models.contains(&"center".to_owned()));
        assert!(legacy_attrs.iter().any(|name| name.contains("E382")));
        assert!(readable_attrs.contains(&"center".to_owned()));
        assert_ne!(legacy_models, readable_models);
        assert_ne!(legacy_attrs, readable_attrs);
    }
}
