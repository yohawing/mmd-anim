#![cfg(feature = "fbx")]

use std::{
    io::{Cursor, Seek, Write},
    sync::Arc,
};

use fbxcel::{
    low::{v7400::ArrayAttributeEncoding, FbxVersion},
    writer::v7400::binary::{AttributesWriter, FbxFooter, Writer},
};
use mmd_anim_runtime::{AnimationClip, BoneIndex, ModelArena, RuntimeInstance};

use crate::{
    pmx::{PmxParsedBone, PmxParsedMaterial, PmxParsedModel},
    vmd::{VmdParsedAnimation, VmdParsedBoneFrame},
};

const ROOT_NODE_ID: i64 = 0;
const DOCUMENT_ID: i64 = 100;
const MODEL_ID: i64 = 200;
const GEOMETRY_ID: i64 = 300;
const MATERIAL_ID_BASE: i64 = 1000;
const BONE_MODEL_ID_BASE: i64 = 10_000;
const BONE_ATTR_ID_BASE: i64 = 20_000;
const SKIN_ID: i64 = 30_000;
const CLUSTER_ID_BASE: i64 = 40_000;
const POSE_ID: i64 = 50_000;
const ANIM_STACK_ID: i64 = 60_000;
const ANIM_LAYER_ID: i64 = 60_001;
const ANIM_CURVENODE_ROT_BASE: i64 = 70_000;
const ANIM_CURVENODE_TRANS_BASE: i64 = 80_000;
const ANIM_CURVE_BASE: i64 = 100_000;
const FBX_TIME_ONE_SECOND: i64 = 46_186_158_000;
const FBX_FRAME_DURATION: i64 = FBX_TIME_ONE_SECOND / 30;
const STATIC_BONE_EPSILON: f32 = 1.0e-5;

#[derive(Debug, Clone)]
pub struct FbxExportOptions {
    pub model_name: String,
    pub flip_z: bool,
}

impl Default for FbxExportOptions {
    fn default() -> Self {
        Self {
            model_name: "PMX Model".to_owned(),
            flip_z: true,
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
}

pub fn export_pmx_fbx_binary(
    model: &PmxParsedModel,
    vmd: Option<&VmdParsedAnimation>,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let animation = vmd.map(|vmd| FbxAnimationData::from_vmd(model, vmd, options));
    export_pmx_fbx_binary_with_animation(model, animation, options)
}

pub fn export_pmx_fbx_binary_with_runtime_bake(
    model: &PmxParsedModel,
    runtime_model: Arc<ModelArena>,
    clip: &AnimationClip,
    last_frame: u32,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let animation =
        Some(FbxAnimationData::from_runtime_bake(model, runtime_model, clip, last_frame, options));
    export_pmx_fbx_binary_with_animation(model, animation, options)
}

fn export_pmx_fbx_binary_with_animation(
    model: &PmxParsedModel,
    animation: Option<FbxAnimationData>,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let mesh = MeshData::from_pmx(model, options)?;
    let sink = Cursor::new(Vec::new());
    let mut writer = Writer::new(sink, FbxVersion::V7_4)?;

    write_fbx_header_extension(&mut writer)?;
    write_top_level_fields(&mut writer)?;
    write_global_settings(&mut writer)?;
    write_documents(&mut writer, animation.is_some())?;
    write_references(&mut writer)?;
    write_definitions(
        &mut writer,
        model.materials.len(),
        model.skeleton.bones.len(),
        animation.as_ref(),
    )?;
    write_objects(&mut writer, model, options, &mesh, animation.as_ref())?;
    write_connections(
        &mut writer,
        model.materials.len(),
        &model.skeleton.bones,
        animation.as_ref(),
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

pub fn export_fbx(
    model: &PmxParsedModel,
    vmd: Option<&VmdParsedAnimation>,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    export_pmx_fbx_binary(model, vmd, options)
}

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

impl MeshData {
    fn from_pmx(
        model: &PmxParsedModel,
        options: &FbxExportOptions,
    ) -> Result<Self, FbxExportError> {
        let vertex_count = model.geometry.positions.len() / 3;
        if model.geometry.positions.len() % 3 != 0 {
            return Err(FbxExportError::InvalidPositionBuffer(
                model.geometry.positions.len(),
            ));
        }
        if model.geometry.normals.len() % 3 != 0 {
            return Err(FbxExportError::InvalidNormalBuffer(
                model.geometry.normals.len(),
            ));
        }
        if model.geometry.uvs.len() % 2 != 0 {
            return Err(FbxExportError::InvalidUvBuffer(model.geometry.uvs.len()));
        }
        if model.geometry.indices.len() % 3 != 0 {
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

struct FbxAnimationData {
    max_frame: u32,
    tracks: Vec<FbxAnimationTrack>,
}

struct FbxAnimationTrack {
    bone_index: usize,
    frame_times: Vec<i64>,
    rotation_values: [Vec<f32>; 3],
    translation_values: [Vec<f32>; 3],
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
    fn from_runtime_bake(
        model: &PmxParsedModel,
        runtime_model: Arc<ModelArena>,
        clip: &AnimationClip,
        max_frame: u32,
        options: &FbxExportOptions,
    ) -> Self {
        let bone_count = model.skeleton.bones.len().min(runtime_model.bone_count());
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
        let mut runtime = RuntimeInstance::new(Arc::clone(&runtime_model));

        for frame in 0..=max_frame {
            runtime.evaluate_clip_frame(clip, frame as f32);
            let world_matrices = runtime.world_matrices();

            for track in &mut tracks {
                let bone = BoneIndex(track.bone_index as u32);
                let bone_world = world_matrices[track.bone_index];
                let local_matrix = match runtime_model.parent_index(bone) {
                    Some(parent) => world_matrices[parent.as_usize()].inverse() * bone_world,
                    None => bone_world,
                };
                let (_scale, rotation, translation) =
                    local_matrix.to_scale_rotation_translation();

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
            })
            .collect();

        Self { max_frame, tracks }
    }

    fn from_vmd(
        model: &PmxParsedModel,
        vmd: &VmdParsedAnimation,
        options: &FbxExportOptions,
    ) -> Self {
        let bone_tracks = collect_bone_tracks(model, vmd);
        let max_frame = bone_tracks
            .iter()
            .filter_map(|track| track.keyframes.last().map(|keyframe| keyframe.frame))
            .max()
            .unwrap_or(vmd.metadata.max_frame);
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
            });
        }

        Self { max_frame, tracks }
    }

    fn last_time(&self) -> i64 {
        self.max_frame as i64 * FBX_FRAME_DURATION
    }
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
        let bx = 3.0 * (1.0 - t) * (1.0 - t) * t * x1
            + 3.0 * (1.0 - t) * t * t * x2
            + t * t * t;
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
    write_string_node(writer, "Creator", "mmd-anim fbx exporter spike")?;
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

fn write_global_settings<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
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
    bone_count: usize,
    animation: Option<&FbxAnimationData>,
) -> Result<(), FbxExportError> {
    let animation_track_count = animation.map(|data| data.tracks.len()).unwrap_or(0);
    let animation_object_count = if animation.is_some() {
        2 + animation_track_count * 8
    } else {
        0
    };
    begin_node(writer, "Definitions", |_| Ok(()))?;
    write_i32_node(writer, "Version", 100)?;
    write_i32_node(
        writer,
        "Count",
        (5 + material_count + bone_count * 3 + animation_object_count) as i32,
    )?;

    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("GlobalSettings")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    writer.close_node()?;

    write_model_object_type(writer, bone_count as i32)?;
    write_geometry_object_type(writer)?;
    write_material_object_type(writer, material_count as i32)?;
    write_node_attribute_object_type(writer, bone_count as i32)?;
    write_deformer_object_type(writer, bone_count as i32)?;
    write_pose_object_type(writer)?;
    if animation.is_some() {
        write_animation_object_types(writer, animation_track_count as i32)?;
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
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Geometry")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
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
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Deformer")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1 + bone_count)?;
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
    track_count: i32,
) -> Result<(), FbxExportError> {
    write_animation_stack_object_type(writer)?;
    write_animation_layer_object_type(writer)?;
    write_animation_curve_node_object_type(writer, track_count * 2)?;
    write_simple_object_type(writer, "AnimationCurve", track_count * 6)?;
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
    mesh: &MeshData,
    animation: Option<&FbxAnimationData>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Objects", |_| Ok(()))?;
    write_geometry(writer, mesh)?;
    write_model(writer, options)?;
    write_skeleton(writer, &model.skeleton.bones, options)?;
    write_skin_deformers(writer, model, options)?;
    write_bind_pose(writer, &model.skeleton.bones, options)?;
    for (index, material) in model.materials.iter().enumerate() {
        write_material(writer, material, MATERIAL_ID_BASE + index as i64)?;
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
    begin_node(writer, "Model", |attrs| {
        attrs.append_i64(MODEL_ID)?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("Mesh")?;
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
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    for (index, bone) in bones.iter().enumerate() {
        write_bone_node_attribute(writer, index, bone)?;
        write_bone_model(writer, index, bone, bones, options)?;
    }
    Ok(())
}

fn write_bone_node_attribute<W: Write + Seek>(
    writer: &mut Writer<W>,
    index: usize,
    bone: &PmxParsedBone,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01NodeAttribute", bone_name(bone));
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
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01Model", bone_name(bone));
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

fn bone_name(bone: &PmxParsedBone) -> &str {
    if bone.english_name.is_empty() {
        &bone.name
    } else {
        &bone.english_name
    }
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
    write_arr_f64_node(writer, "Transform", &identity_matrix())?;
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
        for slot in 0..4 {
            let skin_offset = vertex_index * 4 + slot;
            if skin_indices[skin_offset] as usize == bone_index && skin_weights[skin_offset] > 0.0 {
                indices.push(vertex_index as i32);
                weights.push(skin_weights[skin_offset] as f64);
            }
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
            )?;
        }
        for channel in 0..3 {
            write_animation_curve(
                writer,
                animation_curve_id(track.bone_index, channel + 3),
                &track.frame_times,
                &track.translation_values[channel],
            )?;
        }
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
    write_arr_i32_node(writer, "KeyAttrFlags", &[0x00006108_i32])?;
    write_arr_f32_node(
        writer,
        "KeyAttrDataFloat",
        &[0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32],
    )?;
    write_arr_i32_node(writer, "KeyAttrRefCount", &[values.len() as i32])?;
    writer.close_node()?;
    Ok(())
}

fn write_connections<W: Write + Seek>(
    writer: &mut Writer<W>,
    material_count: usize,
    bones: &[PmxParsedBone],
    animation: Option<&FbxAnimationData>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Connections", |_| Ok(()))?;
    write_oo_connection(writer, MODEL_ID, ROOT_NODE_ID)?;
    write_oo_connection(writer, GEOMETRY_ID, MODEL_ID)?;
    write_oo_connection(writer, SKIN_ID, GEOMETRY_ID)?;
    for index in 0..material_count {
        write_oo_connection(writer, MATERIAL_ID_BASE + index as i64, MODEL_ID)?;
    }
    for (index, bone) in bones.iter().enumerate() {
        let model_id = bone_model_id(index);
        write_oo_connection(writer, bone_attr_id(index), model_id)?;
        write_oo_connection(writer, cluster_id(index), SKIN_ID)?;
        write_oo_connection(writer, model_id, cluster_id(index))?;
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

fn animation_curvenode_rotation_id(bone_index: usize) -> i64 {
    ANIM_CURVENODE_ROT_BASE + bone_index as i64
}

fn animation_curvenode_translation_id(bone_index: usize) -> i64 {
    ANIM_CURVENODE_TRANS_BASE + bone_index as i64
}

fn animation_curve_id(bone_index: usize, channel: usize) -> i64 {
    ANIM_CURVE_BASE + (bone_index * 6 + channel) as i64
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
    let mut attrs = writer.new_node(name)?;
    append_attrs(&mut attrs)?;
    drop(attrs);
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
        attrs.append_arr_i32_from_iter(None::<ArrayAttributeEncoding>, values.iter().copied())?;
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
        attrs.append_arr_i64_from_iter(None::<ArrayAttributeEncoding>, values.iter().copied())?;
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
        attrs.append_arr_f32_from_iter(None::<ArrayAttributeEncoding>, values.iter().copied())?;
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
        attrs.append_arr_f64_from_iter(None::<ArrayAttributeEncoding>, values.iter().copied())?;
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
