//! C ABI wrapper for native hosts.

use std::collections::HashMap;
use std::{ptr, slice, str, sync::Arc};

use mmd_anim_runtime::ModelArena;
use mmd_anim_runtime::{
    solve_append_transform, AnimationClip, AppendPrimitiveInput, AppendTransformInit,
    BoneAnimationBinding, BoneIndex, BoneInit, BoneMorphOffset, GroupMorphOffset, IkAngleLimit,
    IkChainDefinition, IkChainLinkDefinition, IkChainPoseInput, IkChainSolver, IkLinkInit,
    IkSolveOptions, IkSolverInit, MorphAnimationBinding, MorphIndex, MorphInit, MorphKeyframe,
    MorphOffsetSpan, MorphTrack, MovableBoneKeyframe, MovableBoneTrack, PropertyAnimationBinding,
    PropertyKeyframe, RuntimeInstance,
};

pub const ABI_VERSION: u32 = 1;

pub struct MmdRuntimeModel {
    model: Arc<ModelArena>,
    bone_name_to_index: HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: HashMap<Vec<u8>, usize>,
}

pub struct MmdRuntimeInstance {
    runtime: RuntimeInstance,
    cached_world_matrices: Vec<f32>,
    cached_skinning_matrices: Vec<f32>,
}

impl MmdRuntimeInstance {
    fn refresh_matrix_caches(&mut self) {
        flatten_into(
            &mut self.cached_world_matrices,
            self.runtime.world_matrices(),
        );
        flatten_into(
            &mut self.cached_skinning_matrices,
            self.runtime.skinning_matrices(),
        );
    }
}

fn flatten_into(dst: &mut Vec<f32>, matrices: &[glam::Mat4]) {
    let expected = matrices.len() * 16;
    dst.clear();
    dst.reserve(expected);
    for m in matrices {
        dst.extend_from_slice(&m.to_cols_array());
    }
}

pub struct MmdRuntimeClip {
    clip: AnimationClip,
}

pub struct MmdRuntimePmxMaterialSplit {
    split: mmd_anim_format::PmxMaterialSplitResult,
    manifest_json: Vec<u8>,
}

pub struct MmdRuntimePmxRigSpec {
    spec: mmd_anim_format::PmxRigSpec,
    manifest_json: Vec<u8>,
}

pub struct MmdRuntimeIkChain {
    solver: IkChainSolver,
    bone_count: usize,
    link_count: usize,
}

pub struct MmdRuntimeAppendSolver {
    ratio: f32,
    affect_rotation: bool,
    affect_translation: bool,
}

#[repr(C)]
pub struct MmdRuntimeFfiBoneTrack {
    pub bone_index: u32,
    pub keyframe_offset: usize,
    pub keyframe_count: usize,
}

#[repr(C)]
pub struct MmdRuntimeFfiBoneKeyframe {
    pub frame: u32,
    pub position_xyz: [f32; 3],
    pub rotation_xyzw: [f32; 4],
}

#[repr(C)]
pub struct MmdRuntimeFfiMorphTrack {
    pub morph_index: u32,
    pub keyframe_offset: usize,
    pub keyframe_count: usize,
}

#[repr(C)]
pub struct MmdRuntimeFfiMorphKeyframe {
    pub frame: u32,
    pub weight: f32,
}

#[repr(C)]
pub struct MmdRuntimeFfiPropertyKeyframe {
    pub frame: u32,
    pub ik_enabled_offset: usize,
    pub ik_enabled_count: usize,
}

#[repr(C)]
pub struct MmdRuntimeFfiAppendTransform {
    pub target_bone_index: u32,
    pub source_bone_index: u32,
    pub ratio: f32,
    pub flags: u32,
}

#[repr(C)]
pub struct MmdRuntimeFfiIkSolver {
    pub ik_bone_index: u32,
    pub target_bone_index: u32,
    pub link_offset: usize,
    pub link_count: usize,
    pub iteration_count: u32,
    pub limit_angle: f32,
}

#[repr(C)]
pub struct MmdRuntimeFfiIkLink {
    pub bone_index: u32,
    pub flags: u32,
    pub angle_limit_min_xyz: [f32; 3],
    pub angle_limit_max_xyz: [f32; 3],
}

#[repr(C)]
pub struct MmdRuntimeFfiRigIkLink {
    pub bone_slot: u32,
    pub has_angle_limit: bool,
    pub angle_limit_min_xyz: [f32; 3],
    pub angle_limit_max_xyz: [f32; 3],
}

#[repr(C)]
pub struct MmdRuntimeFfiRigBone {
    pub parent_slot: i32,
    pub rest_position_xyz: [f32; 3],
    pub flags: u32,
    pub fixed_axis_xyz: [f32; 3],
}

#[repr(C)]
pub struct MmdRuntimeFfiIkSolveStats {
    pub executed_iterations: u32,
    pub link_steps: u32,
    pub final_distance: f32,
    pub break_reason: u32,
}

#[repr(C)]
pub struct MmdRuntimeFfiAppendConfig {
    pub ratio: f32,
    pub affect_rotation: bool,
    pub affect_translation: bool,
}

#[repr(C)]
pub struct MmdRuntimeFfiBoneMorphOffset {
    pub morph_index: u32,
    pub target_bone_index: u32,
    pub position_offset_xyz: [f32; 3],
    pub rotation_offset_xyzw: [f32; 4],
}

#[repr(C)]
pub struct MmdRuntimeFfiGroupMorphOffset {
    pub morph_index: u32,
    pub child_morph_index: u32,
    pub ratio: f32,
}

#[repr(C)]
pub struct MmdRuntimeFfiByteBuffer {
    pub data: *mut u8,
    pub len: usize,
}

const APPEND_FLAG_ROTATION: u32 = 1;
const APPEND_FLAG_TRANSLATION: u32 = 1 << 1;
const APPEND_FLAG_LOCAL: u32 = 1 << 2;
const IK_LINK_FLAG_ANGLE_LIMIT: u32 = 1;
const RIG_BONE_FIXED_AXIS: u32 = 1;

#[unsafe(no_mangle)]
pub extern "C" fn mmd_runtime_abi_version() -> u32 {
    ABI_VERSION
}

fn empty_byte_buffer() -> MmdRuntimeFfiByteBuffer {
    MmdRuntimeFfiByteBuffer {
        data: ptr::null_mut(),
        len: 0,
    }
}

fn byte_buffer_from_vec(bytes: Vec<u8>) -> MmdRuntimeFfiByteBuffer {
    if bytes.is_empty() {
        return empty_byte_buffer();
    }
    let mut bytes = bytes.into_boxed_slice();
    let data = bytes.as_mut_ptr();
    let len = bytes.len();
    let _ = Box::leak(bytes);
    MmdRuntimeFfiByteBuffer { data, len }
}

/// Frees a byte buffer returned by an export function.
///
/// # Safety
///
/// `buffer` must be either an empty buffer or a value returned by this library.
/// Passing an arbitrary pointer, or freeing the same buffer twice, is undefined
/// behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_byte_buffer_free(buffer: MmdRuntimeFfiByteBuffer) {
    if buffer.data.is_null() || buffer.len == 0 {
        return;
    }
    unsafe {
        drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
            buffer.data,
            buffer.len,
        )));
    }
}

/// Creates a stateful per-chain IK primitive solver.
///
/// # Safety
///
/// `bones` must point to `bone_count` readable entries and `links` must point
/// to `link_count` readable entries when their counts are non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_ik_chain_create(
    bones: *const MmdRuntimeFfiRigBone,
    bone_count: usize,
    target_bone_slot: u32,
    links: *const MmdRuntimeFfiRigIkLink,
    link_count: usize,
    iteration_count: u32,
    limit_angle: f32,
) -> *mut MmdRuntimeIkChain {
    let definition = unsafe {
        build_ik_chain_definition(
            bones,
            bone_count,
            target_bone_slot,
            links,
            link_count,
            iteration_count,
            limit_angle,
        )
    };
    let Some(definition) = definition else {
        return ptr::null_mut();
    };
    let solver = IkChainSolver::new(definition);
    Box::into_raw(Box::new(MmdRuntimeIkChain {
        solver,
        bone_count,
        link_count,
    }))
}

/// Frees an IK primitive solver handle.
///
/// # Safety
///
/// `chain` must be null or a pointer returned by `mmd_runtime_ik_chain_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_ik_chain_free(chain: *mut MmdRuntimeIkChain) {
    if chain.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(chain));
    }
}

/// Solves one IK chain into caller-owned link-rotation output.
///
/// # Safety
///
/// Required input and output pointers must point to readable/writable arrays
/// matching the documented C header lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_ik_chain_solve(
    chain: *mut MmdRuntimeIkChain,
    parent_world_matrix: *const f32,
    local_position_offsets_xyz: *const f32,
    local_rotations_xyzw: *const f32,
    goal_position_xyz: *const f32,
    tolerance: f32,
    max_iterations_cap: u32,
    out_link_rotations_xyzw: *mut f32,
    out_link_rotation_f32_len: usize,
    out_stats: *mut MmdRuntimeFfiIkSolveStats,
) -> bool {
    if chain.is_null()
        || local_rotations_xyzw.is_null()
        || goal_position_xyz.is_null()
        || out_link_rotations_xyzw.is_null()
    {
        return false;
    }
    let chain = unsafe { &mut *chain };
    let required_output_len = match chain.link_count.checked_mul(4) {
        Some(len) => len,
        None => return false,
    };
    if out_link_rotation_f32_len < required_output_len {
        return false;
    }

    let parent_world_matrix = if parent_world_matrix.is_null() {
        None
    } else {
        let raw = unsafe { slice::from_raw_parts(parent_world_matrix, 16) };
        if !all_finite(raw) {
            return false;
        }
        let Ok(raw) = raw.try_into() else {
            return false;
        };
        Some(glam::Mat4::from_cols_array(raw))
    };

    let position_offsets = if local_position_offsets_xyz.is_null() {
        vec![glam::Vec3A::ZERO; chain.bone_count]
    } else {
        let Some(len) = chain.bone_count.checked_mul(3) else {
            return false;
        };
        let raw = unsafe { slice::from_raw_parts(local_position_offsets_xyz, len) };
        if !all_finite(raw) {
            return false;
        }
        raw.chunks_exact(3)
            .map(|v| glam::Vec3A::new(v[0], v[1], v[2]))
            .collect()
    };

    let Some(rotation_len) = chain.bone_count.checked_mul(4) else {
        return false;
    };
    let raw_rotations = unsafe { slice::from_raw_parts(local_rotations_xyzw, rotation_len) };
    if !all_finite(raw_rotations) {
        return false;
    }
    let mut rotations = Vec::with_capacity(chain.bone_count);
    for q in raw_rotations.chunks_exact(4) {
        let rotation = glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]);
        if rotation.length_squared() <= f32::EPSILON {
            return false;
        }
        rotations.push(rotation.normalize());
    }

    let goal = unsafe { slice::from_raw_parts(goal_position_xyz, 3) };
    if !all_finite(goal) || !tolerance.is_finite() {
        return false;
    }
    let max_iterations_cap = if max_iterations_cap == 0 {
        None
    } else {
        Some(max_iterations_cap)
    };

    let output = chain.solver.solve(IkChainPoseInput {
        parent_world_matrix,
        local_position_offsets: &position_offsets,
        local_rotations: &rotations,
        goal_position: glam::Vec3A::new(goal[0], goal[1], goal[2]),
        tolerance,
        max_iterations_cap,
    });
    if output.solved_link_rotations.len() != chain.link_count {
        return false;
    }

    let out = unsafe { slice::from_raw_parts_mut(out_link_rotations_xyzw, required_output_len) };
    for (rotation, dst) in output
        .solved_link_rotations
        .iter()
        .zip(out.chunks_exact_mut(4))
    {
        dst.copy_from_slice(&rotation.to_array());
    }
    if !out_stats.is_null() {
        unsafe {
            *out_stats = MmdRuntimeFfiIkSolveStats {
                executed_iterations: output.executed_iterations,
                link_steps: output.link_steps,
                final_distance: output.final_distance,
                break_reason: if output.final_distance <= tolerance.max(0.0) {
                    0
                } else {
                    1
                },
            };
        }
    }
    true
}

/// Creates a per-bone append/grant primitive solver.
///
/// # Safety
///
/// `config` must point to a readable config struct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_append_solver_create(
    config: *const MmdRuntimeFfiAppendConfig,
) -> *mut MmdRuntimeAppendSolver {
    if config.is_null() {
        return ptr::null_mut();
    }
    let config = unsafe { &*config };
    if !config.ratio.is_finite() {
        return ptr::null_mut();
    }
    Box::into_raw(Box::new(MmdRuntimeAppendSolver {
        ratio: config.ratio,
        affect_rotation: config.affect_rotation,
        affect_translation: config.affect_translation,
    }))
}

/// Frees an append primitive solver handle.
///
/// # Safety
///
/// `solver` must be null or a pointer returned by
/// `mmd_runtime_append_solver_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_append_solver_free(solver: *mut MmdRuntimeAppendSolver) {
    if solver.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(solver));
    }
}

/// Solves one append/grant primitive into caller-owned output arrays.
///
/// # Safety
///
/// Pointers must reference readable/writable arrays matching the documented
/// C header lengths.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_append_solver_solve(
    solver: *const MmdRuntimeAppendSolver,
    source_position_offset_xyz: *const f32,
    source_rotation_xyzw: *const f32,
    out_position_offset_xyz: *mut f32,
    out_rotation_xyzw: *mut f32,
) -> bool {
    if solver.is_null()
        || source_position_offset_xyz.is_null()
        || source_rotation_xyzw.is_null()
        || out_position_offset_xyz.is_null()
        || out_rotation_xyzw.is_null()
    {
        return false;
    }
    let solver = unsafe { &*solver };
    let position = unsafe { slice::from_raw_parts(source_position_offset_xyz, 3) };
    let rotation = unsafe { slice::from_raw_parts(source_rotation_xyzw, 4) };
    if !all_finite(position) || !all_finite(rotation) {
        return false;
    }
    let source_rotation = glam::Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
    if source_rotation.length_squared() <= f32::EPSILON {
        return false;
    }

    let output = solve_append_transform(AppendPrimitiveInput {
        source_position_offset: glam::Vec3A::new(position[0], position[1], position[2]),
        source_rotation: source_rotation.normalize(),
        ratio: solver.ratio,
        affect_rotation: solver.affect_rotation,
        affect_translation: solver.affect_translation,
    });
    let out_position = unsafe { slice::from_raw_parts_mut(out_position_offset_xyz, 3) };
    out_position.copy_from_slice(&output.position_offset.to_array());
    let out_rotation = unsafe { slice::from_raw_parts_mut(out_rotation_xyzw, 4) };
    out_rotation.copy_from_slice(&output.rotation.to_array());
    true
}

/// Parses VMD bytes and returns the serialized `VmdParsedAnimation` JSON.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero.
/// The returned buffer is owned by the caller and must be freed with
/// `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_vmd_json(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }

    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
        Ok(parsed) => parsed,
        Err(_) => return empty_byte_buffer(),
    };

    match serde_json::to_vec(&parsed) {
        Ok(json) => byte_buffer_from_vec(json),
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns JSON for model fields except geometry.
///
/// The JSON includes metadata, materials, skeleton, morphs, display frames,
/// rigid bodies, joints, soft bodies, and diagnostics. Large geometry arrays
/// are intentionally omitted so Unity can consume them through typed buffers.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero.
/// The returned buffer is owned by the caller and must be freed with
/// `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_non_geometry_json(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }

    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let parsed = match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => parsed,
        Err(_) => return empty_byte_buffer(),
    };

    let mut object = serde_json::Map::with_capacity(9);
    macro_rules! push_json_field {
        ($key:expr, $value:expr) => {
            match serde_json::to_value($value) {
                Ok(value) => {
                    object.insert($key.to_owned(), value);
                }
                Err(_) => return empty_byte_buffer(),
            }
        };
    }

    push_json_field!("metadata", &parsed.metadata);
    match unity_pmx_materials_json(&parsed.materials) {
        Ok(value) => {
            object.insert("materials".to_owned(), value);
        }
        Err(_) => return empty_byte_buffer(),
    }
    match unity_pmx_skeleton_json(&parsed.skeleton) {
        Ok(value) => {
            object.insert("skeleton".to_owned(), value);
        }
        Err(_) => return empty_byte_buffer(),
    }
    push_json_field!("morphs", &parsed.morphs);
    push_json_field!("displayFrames", &parsed.display_frames);
    push_json_field!("rigidBodies", &parsed.rigid_bodies);
    push_json_field!("joints", &parsed.joints);
    push_json_field!("softBodies", &parsed.soft_bodies);
    push_json_field!("diagnostics", &parsed.diagnostics);

    match serde_json::to_vec(&serde_json::Value::Object(object)) {
        Ok(json) => byte_buffer_from_vec(json),
        Err(_) => empty_byte_buffer(),
    }
}

fn unity_pmx_materials_json(
    materials: &[mmd_anim_format::pmx::PmxParsedMaterial],
) -> Result<serde_json::Value, serde_json::Error> {
    let mut value = serde_json::to_value(materials)?;
    if let Some(items) = value.as_array_mut() {
        for item in items {
            if item
                .get("sharedToonIndex")
                .is_some_and(serde_json::Value::is_null)
            {
                item["sharedToonIndex"] = serde_json::Value::from(-1);
            }
        }
    }
    Ok(value)
}

fn unity_pmx_skeleton_json(
    skeleton: &mmd_anim_format::pmx::PmxParsedSkeleton,
) -> Result<serde_json::Value, serde_json::Error> {
    let mut value = serde_json::to_value(skeleton)?;
    if let Some(bones) = value
        .get_mut("bones")
        .and_then(serde_json::Value::as_array_mut)
    {
        for bone in bones {
            if bone
                .get("externalParentKey")
                .is_some_and(serde_json::Value::is_null)
            {
                bone["externalParentKey"] = serde_json::Value::from(-1);
            }
        }
    }
    Ok(value)
}

// ---------------------------------------------------------------------------
//  PMX geometry typed-buffer API
//
//  Each function parses PMX bytes and returns one geometry array as a raw
//  native-endian byte buffer.  Parsing is duplicated per call; no handle is
//  retained.  The caller must free each buffer with `mmd_runtime_byte_buffer_free`.
// ---------------------------------------------------------------------------

/// Parses PMX bytes and returns vertex positions as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 3` f32 values (x, y, z per vertex).
/// Returns an empty buffer on parse failure, null input, or zero length.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_positions_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .positions
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns vertex normals as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 3` f32 values (x, y, z per vertex).
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_normals_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .normals
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns vertex UVs as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 2` f32 values (u, v per vertex).
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_uvs_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .uvs
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns the number of additional UV channels.
///
/// Returns zero on parse failure, null input, or zero length.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_additional_uv_count(
    data: *const u8,
    len: usize,
) -> usize {
    if data.is_null() || len == 0 {
        return 0;
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => parsed.geometry.additional_uvs.len(),
        Err(_) => 0,
    }
}

/// Parses PMX bytes and returns one additional-UV channel as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 4` f32 values for the requested channel.
/// Returns an empty buffer on parse failure, null input, zero length, or
/// out-of-range `uv_index`.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_additional_uvs_buffer(
    data: *const u8,
    len: usize,
    uv_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let Some(values) = parsed.geometry.additional_uvs.get(uv_index) else {
                return empty_byte_buffer();
            };
            byte_buffer_from_f32_slice(values)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns face indices as a native-endian byte buffer.
///
/// The buffer contains `index_count` u32 values.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_indices_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .indices
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns material groups as a native-endian byte buffer.
///
/// The buffer contains `group_count * 3` u32 values as
/// `[start, count, material_index]` triples.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_material_groups_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let groups: Vec<u32> = parsed
                .geometry
                .material_groups
                .iter()
                .flat_map(|group| {
                    [
                        group.start as u32,
                        group.count as u32,
                        group.material_index as u32,
                    ]
                })
                .collect();
            byte_buffer_from_u32_slice(&groups)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns skin bone indices as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 4` u32 values (4 bone indices per vertex).
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_skin_indices_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .skin_indices
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns skin weights as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 4` f32 values (4 weights per vertex).
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_skin_weights_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .skin_weights
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns per-vertex edge scale as a native-endian byte buffer.
///
/// The buffer contains `vertex_count` f32 values.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_edge_scale_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => byte_buffer_from_f32_slice(&parsed.geometry.edge_scale),
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns SDEF-enabled flags as a byte buffer.
///
/// Each byte is `1` when SDEF is enabled for that vertex, or `0` otherwise.
/// The buffer length equals `vertex_count`.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_sdef_enabled_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .sdef
                .enabled
                .iter()
                .map(|&v| if v > 0.5 { 1u8 } else { 0u8 })
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns SDEF C vectors as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 3` f32 values.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_sdef_c_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .sdef
                .c
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns SDEF R0 vectors as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 3` f32 values.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_sdef_r0_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .sdef
                .r0
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns SDEF R1 vectors as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 3` f32 values.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_sdef_r1_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let buf: Vec<u8> = parsed
                .geometry
                .sdef
                .r1
                .iter()
                .flat_map(|v| v.to_ne_bytes())
                .collect();
            byte_buffer_from_vec(buf)
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns derived SDEF RW0 vectors as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 3` f32 values.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_sdef_rw0_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => byte_buffer_from_f32_slice(&parsed.geometry.sdef.rw0),
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns derived SDEF RW1 vectors as a native-endian byte buffer.
///
/// The buffer contains `vertex_count * 3` f32 values.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_sdef_rw1_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => byte_buffer_from_f32_slice(&parsed.geometry.sdef.rw1),
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns QDEF-enabled flags as a byte buffer.
///
/// Each byte is `1` when QDEF is enabled for that vertex, or `0` otherwise.
/// The buffer length equals `vertex_count`.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_qdef_enabled_buffer(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => byte_buffer_from_vec(
            parsed
                .geometry
                .qdef
                .enabled
                .iter()
                .map(|&v| if v > 0.5 { 1u8 } else { 0u8 })
                .collect(),
        ),
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes and returns skinning mode names as a JSON object.
///
/// The returned JSON has the shape `{"skinningModes": ["bdef1", ...]}`.
/// Returns an empty buffer on parse failure, null input, or zero length.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_parse_pmx_skinning_modes_json(
    data: *const u8,
    len: usize,
) -> MmdRuntimeFfiByteBuffer {
    if data.is_null() || len == 0 {
        return empty_byte_buffer();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(parsed) => {
            let vertex_count = parsed.geometry.positions.len() / 3;
            let modes: Vec<&str> = (0..vertex_count)
                .map(|i| {
                    if parsed.geometry.sdef.enabled.get(i).copied().unwrap_or(0.0) > 0.5 {
                        "sdef"
                    } else if parsed.geometry.qdef.enabled.get(i).copied().unwrap_or(0.0) > 0.5 {
                        "qdef"
                    } else {
                        let w2 = parsed
                            .geometry
                            .skin_weights
                            .get(i * 4 + 2)
                            .copied()
                            .unwrap_or(0.0);
                        let w3 = parsed
                            .geometry
                            .skin_weights
                            .get(i * 4 + 3)
                            .copied()
                            .unwrap_or(0.0);
                        let w1 = parsed
                            .geometry
                            .skin_weights
                            .get(i * 4 + 1)
                            .copied()
                            .unwrap_or(0.0);
                        if w2 != 0.0 || w3 != 0.0 {
                            "bdef4"
                        } else if w1 != 0.0 {
                            "bdef2"
                        } else {
                            "bdef1"
                        }
                    }
                })
                .collect();
            let wrapper = serde_json::json!({ "skinningModes": modes });
            match serde_json::to_vec(&wrapper) {
                Ok(json) => byte_buffer_from_vec(json),
                Err(_) => empty_byte_buffer(),
            }
        }
        Err(_) => empty_byte_buffer(),
    }
}

/// Parses PMX bytes once and creates an opaque material-split handle.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_create(
    data: *const u8,
    len: usize,
    flags: u32,
) -> *mut MmdRuntimePmxMaterialSplit {
    if data.is_null() || len == 0 {
        return ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let split = match mmd_anim_format::parse_pmx_material_split(bytes, flags) {
        Ok(split) => split,
        Err(_) => return ptr::null_mut(),
    };
    let manifest_json = match serde_json::to_vec(&split.manifest) {
        Ok(json) => json,
        Err(_) => return ptr::null_mut(),
    };
    Box::into_raw(Box::new(MmdRuntimePmxMaterialSplit {
        split,
        manifest_json,
    }))
}

/// Parses PMX bytes once and creates an opaque rig-spec handle.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_rig_spec_create(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimePmxRigSpec {
    if data.is_null() || len == 0 {
        return ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let spec = match mmd_anim_format::parse_pmx_rig_spec(bytes) {
        Ok(spec) => spec,
        Err(_) => return ptr::null_mut(),
    };
    let manifest_json = match serde_json::to_vec(&spec) {
        Ok(json) => json,
        Err(_) => return ptr::null_mut(),
    };
    Box::into_raw(Box::new(MmdRuntimePmxRigSpec {
        spec,
        manifest_json,
    }))
}

/// Frees a PMX material-split handle.
///
/// # Safety
/// `split` must be null or a handle returned by
/// `mmd_runtime_pmx_material_split_create` that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_free(
    split: *mut MmdRuntimePmxMaterialSplit,
) {
    if split.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(split));
    }
}

/// Frees a PMX rig-spec handle.
///
/// # Safety
/// `spec` must be null or a handle returned by
/// `mmd_runtime_pmx_rig_spec_create` that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_rig_spec_free(spec: *mut MmdRuntimePmxRigSpec) {
    if spec.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(spec));
    }
}

/// Returns the number of material-split meshes owned by a split handle.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. A null handle returns zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_mesh_count(
    split: *const MmdRuntimePmxMaterialSplit,
) -> usize {
    let Some(split) = (unsafe { split.as_ref() }) else {
        return 0;
    };
    split.split.meshes.len()
}

/// Returns the serialized material-split manifest JSON.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_manifest_json(
    split: *const MmdRuntimePmxMaterialSplit,
) -> MmdRuntimeFfiByteBuffer {
    let Some(split) = (unsafe { split.as_ref() }) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(split.manifest_json.clone())
}

/// Returns the serialized rig-spec manifest JSON.
///
/// # Safety
/// `spec` must be null or a valid handle returned by
/// `mmd_runtime_pmx_rig_spec_create`. Passing any other pointer is undefined
/// behavior. The returned buffer is owned by the caller and must be freed with
/// `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_rig_spec_manifest_json(
    spec: *const MmdRuntimePmxRigSpec,
) -> MmdRuntimeFfiByteBuffer {
    let Some(spec) = (unsafe { spec.as_ref() }) else {
        return empty_byte_buffer();
    };
    debug_assert_eq!(spec.spec.bone_count, spec.spec.bones.len());
    byte_buffer_from_vec(spec.manifest_json.clone())
}

/// Returns split mesh vertex positions as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_positions_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.positions)
}

/// Returns split mesh vertex normals as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_normals_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.normals)
}

/// Returns split mesh vertex UVs as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_uvs_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.uvs)
}

/// Returns one split mesh additional-UV layer as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_additional_uvs_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
    uv_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(split) = (unsafe { split.as_ref() }) else {
        return empty_byte_buffer();
    };
    let Some(mesh) = split.split.meshes.get(mesh_index) else {
        return empty_byte_buffer();
    };
    let Some(values) = mesh.geometry.additional_uvs.get(uv_index) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_f32_slice(values)
}

/// Returns split mesh triangle indices as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_indices_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_u32_buffer(split, mesh_index, |mesh| &mesh.geometry.indices)
}

/// Returns split mesh skin bone indices as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_skin_indices_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_u32_buffer(split, mesh_index, |mesh| &mesh.geometry.skin_indices)
}

/// Returns split mesh skin weights as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_skin_weights_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.skin_weights)
}

/// Returns split mesh edge scale values as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_edge_scale_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.edge_scale)
}

/// Returns split mesh SDEF-enabled flags as a byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_sdef_enabled_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(split) = (unsafe { split.as_ref() }) else {
        return empty_byte_buffer();
    };
    let Some(mesh) = split.split.meshes.get(mesh_index) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(
        mesh.geometry
            .sdef
            .enabled
            .iter()
            .map(|&v| if v > 0.5 { 1u8 } else { 0u8 })
            .collect(),
    )
}

/// Returns split mesh SDEF C vectors as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_sdef_c_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.c)
}

/// Returns split mesh SDEF R0 vectors as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_sdef_r0_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.r0)
}

/// Returns split mesh SDEF R1 vectors as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_sdef_r1_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.r1)
}

/// Returns split mesh derived SDEF RW0 vectors as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_sdef_rw0_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.rw0)
}

/// Returns split mesh derived SDEF RW1 vectors as a native-endian byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_sdef_rw1_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.rw1)
}

/// Returns split mesh QDEF-enabled flags as a byte buffer.
///
/// # Safety
/// `split` must be null or a valid handle returned by
/// `mmd_runtime_pmx_material_split_create`. Passing any other pointer is
/// undefined behavior. The returned buffer is owned by the caller and must be
/// freed with `mmd_runtime_byte_buffer_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_material_split_qdef_enabled_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(split) = (unsafe { split.as_ref() }) else {
        return empty_byte_buffer();
    };
    let Some(mesh) = split.split.meshes.get(mesh_index) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(
        mesh.geometry
            .qdef
            .enabled
            .iter()
            .map(|&v| if v > 0.5 { 1u8 } else { 0u8 })
            .collect(),
    )
}

fn pmx_material_split_f32_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
    accessor: fn(&mmd_anim_format::PmxMaterialSplitMesh) -> &Vec<f32>,
) -> MmdRuntimeFfiByteBuffer {
    let Some(split) = (unsafe { split.as_ref() }) else {
        return empty_byte_buffer();
    };
    let Some(mesh) = split.split.meshes.get(mesh_index) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_f32_slice(accessor(mesh))
}

fn pmx_material_split_u32_buffer(
    split: *const MmdRuntimePmxMaterialSplit,
    mesh_index: usize,
    accessor: fn(&mmd_anim_format::PmxMaterialSplitMesh) -> &Vec<u32>,
) -> MmdRuntimeFfiByteBuffer {
    let Some(split) = (unsafe { split.as_ref() }) else {
        return empty_byte_buffer();
    };
    let Some(mesh) = split.split.meshes.get(mesh_index) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_u32_slice(accessor(mesh))
}

fn byte_buffer_from_f32_slice(values: &[f32]) -> MmdRuntimeFfiByteBuffer {
    byte_buffer_from_vec(values.iter().flat_map(|v| v.to_ne_bytes()).collect())
}

fn byte_buffer_from_u32_slice(values: &[u32]) -> MmdRuntimeFfiByteBuffer {
    byte_buffer_from_vec(values.iter().flat_map(|v| v.to_ne_bytes()).collect())
}

/// Exports a minimal PMX model from flat geometry arrays and a JSON descriptor.
///
/// The descriptor is a UTF-8 JSON object matching the WASM `exportPmxFromParts`
/// metadata shape. This initial C ABI slice creates a default root bone and
/// default material when richer sections are not provided.
///
/// # Safety
///
/// `metadata_json` must point to `metadata_json_len` readable UTF-8 bytes.
/// `positions_xyz`, `normals_xyz`, and `uvs_xy` must point to
/// `vertex_count * 3`, `vertex_count * 3`, and `vertex_count * 2` readable
/// values respectively. `indices` must point to `index_count` readable values
/// when `index_count` is non-zero. `skin_indices` and `skin_weights` must be
/// both null, or both point to `vertex_count * 4` readable values. `edge_scale`
/// may be null, or must point to `vertex_count` readable values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_export_pmx_from_parts(
    metadata_json: *const u8,
    metadata_json_len: usize,
    positions_xyz: *const f32,
    vertex_count: usize,
    normals_xyz: *const f32,
    uvs_xy: *const f32,
    indices: *const u32,
    index_count: usize,
    skin_indices: *const u32,
    skin_weights: *const f32,
    edge_scale: *const f32,
) -> MmdRuntimeFfiByteBuffer {
    if metadata_json.is_null()
        || metadata_json_len == 0
        || positions_xyz.is_null()
        || normals_xyz.is_null()
        || uvs_xy.is_null()
        || vertex_count == 0
    {
        return empty_byte_buffer();
    }
    if index_count > 0 && indices.is_null() {
        return empty_byte_buffer();
    }
    if skin_indices.is_null() != skin_weights.is_null() {
        return empty_byte_buffer();
    }

    let Some(positions_len) = vertex_count.checked_mul(3) else {
        return empty_byte_buffer();
    };
    let Some(normals_len) = vertex_count.checked_mul(3) else {
        return empty_byte_buffer();
    };
    let Some(uvs_len) = vertex_count.checked_mul(2) else {
        return empty_byte_buffer();
    };
    let Some(skin_len) = vertex_count.checked_mul(4) else {
        return empty_byte_buffer();
    };

    let metadata_bytes = unsafe { slice::from_raw_parts(metadata_json, metadata_json_len) };
    let metadata_json = match str::from_utf8(metadata_bytes) {
        Ok(json) => json,
        Err(_) => return empty_byte_buffer(),
    };
    let descriptor: mmd_anim_format::PmxPartsDescriptor = match serde_json::from_str(metadata_json)
    {
        Ok(descriptor) => descriptor,
        Err(_) => return empty_byte_buffer(),
    };

    let positions_xyz = unsafe { slice::from_raw_parts(positions_xyz, positions_len) };
    let normals_xyz = unsafe { slice::from_raw_parts(normals_xyz, normals_len) };
    let uvs_xy = unsafe { slice::from_raw_parts(uvs_xy, uvs_len) };
    let indices = if index_count == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(indices, index_count) }
    };
    let (skin_indices, skin_weights) = if skin_indices.is_null() {
        (&[][..], &[][..])
    } else {
        (
            unsafe { slice::from_raw_parts(skin_indices, skin_len) },
            unsafe { slice::from_raw_parts(skin_weights, skin_len) },
        )
    };
    let edge_scale = if edge_scale.is_null() {
        &[]
    } else {
        unsafe { slice::from_raw_parts(edge_scale, vertex_count) }
    };

    let model = match mmd_anim_format::build_pmx_model_from_parts(mmd_anim_format::PmxPartsInput {
        descriptor,
        positions_xyz,
        normals_xyz,
        uvs_xy,
        indices,
        skin_indices,
        skin_weights,
        edge_scale,
    }) {
        Ok(model) => model,
        Err(_) => return empty_byte_buffer(),
    };
    byte_buffer_from_vec(mmd_anim_format::export_pmx_model(&model))
}

/// Creates a model from parent indices and rest-position triples.
///
/// # Safety
///
/// `parent_indices` must point to `bone_count` readable `i32` values.
/// `rest_positions_xyz` must point to `bone_count * 3` readable `f32` values.
/// Both pointers must remain valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create(
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    bone_count: usize,
) -> *mut MmdRuntimeModel {
    if parent_indices.is_null() || rest_positions_xyz.is_null() || bone_count == 0 {
        return ptr::null_mut();
    }

    let Some(model) = (unsafe {
        build_model_from_ffi(RawModelInput {
            parent_indices,
            rest_positions_xyz,
            bone_count,
            inverse_bind_matrices: ptr::null(),
            transform_orders: ptr::null(),
            ik_solvers: ptr::null(),
            ik_solver_count: 0,
            ik_links: ptr::null(),
            ik_link_count: 0,
            append_transforms: ptr::null(),
            append_transform_count: 0,
            morph_count: 0,
            bone_morph_offsets: ptr::null(),
            bone_morph_offset_count: 0,
            group_morph_offsets: ptr::null(),
            group_morph_offset_count: 0,
        })
    }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
}

/// Creates a model from parent indices, rest-position triples, and inverse bind matrices.
///
/// # Safety
///
/// `parent_indices` must point to `bone_count` readable `i32` values.
/// `rest_positions_xyz` must point to `bone_count * 3` readable `f32` values.
/// `inverse_bind_matrices` must point to `bone_count * 16` readable `f32` values.
/// All pointers must remain valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_with_inverse_bind(
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    inverse_bind_matrices: *const f32,
    bone_count: usize,
) -> *mut MmdRuntimeModel {
    if parent_indices.is_null()
        || rest_positions_xyz.is_null()
        || inverse_bind_matrices.is_null()
        || bone_count == 0
    {
        return ptr::null_mut();
    }

    let Some(model) = (unsafe {
        build_model_from_ffi(RawModelInput {
            parent_indices,
            rest_positions_xyz,
            bone_count,
            inverse_bind_matrices,
            transform_orders: ptr::null(),
            ik_solvers: ptr::null(),
            ik_solver_count: 0,
            ik_links: ptr::null(),
            ik_link_count: 0,
            append_transforms: ptr::null(),
            append_transform_count: 0,
            morph_count: 0,
            bone_morph_offsets: ptr::null(),
            bone_morph_offset_count: 0,
            group_morph_offsets: ptr::null(),
            group_morph_offset_count: 0,
        })
    }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
}

/// Creates a model from parent indices, rest-position triples, and append transforms.
///
/// # Safety
///
/// `parent_indices` must point to `bone_count` readable `i32` values.
/// `rest_positions_xyz` must point to `bone_count * 3` readable `f32` values.
/// `append_transforms` must be null when `append_transform_count` is zero, or
/// point to `append_transform_count` readable descriptors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_with_append(
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    bone_count: usize,
    append_transforms: *const MmdRuntimeFfiAppendTransform,
    append_transform_count: usize,
) -> *mut MmdRuntimeModel {
    if parent_indices.is_null() || rest_positions_xyz.is_null() || bone_count == 0 {
        return ptr::null_mut();
    }

    let Some(model) = (unsafe {
        build_model_from_ffi(RawModelInput {
            parent_indices,
            rest_positions_xyz,
            bone_count,
            inverse_bind_matrices: ptr::null(),
            transform_orders: ptr::null(),
            ik_solvers: ptr::null(),
            ik_solver_count: 0,
            ik_links: ptr::null(),
            ik_link_count: 0,
            append_transforms,
            append_transform_count,
            morph_count: 0,
            bone_morph_offsets: ptr::null(),
            bone_morph_offset_count: 0,
            group_morph_offsets: ptr::null(),
            group_morph_offset_count: 0,
        })
    }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
}

/// Creates a model from parent indices, rest positions, inverse bind matrices,
/// and append transforms.
///
/// # Safety
///
/// `parent_indices` must point to `bone_count` readable `i32` values.
/// `rest_positions_xyz` must point to `bone_count * 3` readable `f32` values.
/// `inverse_bind_matrices` must point to `bone_count * 16` readable `f32` values.
/// `append_transforms` must be null when `append_transform_count` is zero, or
/// point to `append_transform_count` readable descriptors.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_with_append_and_inverse_bind(
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    inverse_bind_matrices: *const f32,
    bone_count: usize,
    append_transforms: *const MmdRuntimeFfiAppendTransform,
    append_transform_count: usize,
) -> *mut MmdRuntimeModel {
    if parent_indices.is_null()
        || rest_positions_xyz.is_null()
        || inverse_bind_matrices.is_null()
        || bone_count == 0
    {
        return ptr::null_mut();
    }

    let Some(model) = (unsafe {
        build_model_from_ffi(RawModelInput {
            parent_indices,
            rest_positions_xyz,
            bone_count,
            inverse_bind_matrices,
            transform_orders: ptr::null(),
            ik_solvers: ptr::null(),
            ik_solver_count: 0,
            ik_links: ptr::null(),
            ik_link_count: 0,
            append_transforms,
            append_transform_count,
            morph_count: 0,
            bone_morph_offsets: ptr::null(),
            bone_morph_offset_count: 0,
            group_morph_offsets: ptr::null(),
            group_morph_offset_count: 0,
        })
    }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
}

/// Creates a model from all currently supported flat descriptor arrays.
///
/// `inverse_bind_matrices` may be null to use identity inverse bind values.
/// `ik_solvers`, `ik_links`, and `append_transforms` may be null when their
/// associated count is zero.
///
/// # Safety
///
/// `parent_indices` must point to `bone_count` readable `i32` values.
/// `rest_positions_xyz` must point to `bone_count * 3` readable `f32` values.
/// `inverse_bind_matrices`, when non-null, must point to `bone_count * 16`
/// readable `f32` values. Descriptor pointers must match their supplied counts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_full(
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    inverse_bind_matrices: *const f32,
    bone_count: usize,
    ik_solvers: *const MmdRuntimeFfiIkSolver,
    ik_solver_count: usize,
    ik_links: *const MmdRuntimeFfiIkLink,
    ik_link_count: usize,
    append_transforms: *const MmdRuntimeFfiAppendTransform,
    append_transform_count: usize,
) -> *mut MmdRuntimeModel {
    if parent_indices.is_null() || rest_positions_xyz.is_null() || bone_count == 0 {
        return ptr::null_mut();
    }

    let Some(model) = (unsafe {
        build_model_from_ffi(RawModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders: ptr::null(),
            bone_count,
            ik_solvers,
            ik_solver_count,
            ik_links,
            ik_link_count,
            append_transforms,
            append_transform_count,
            morph_count: 0,
            bone_morph_offsets: ptr::null(),
            bone_morph_offset_count: 0,
            group_morph_offsets: ptr::null(),
            group_morph_offset_count: 0,
        })
    }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
}

/// Creates a full model with explicit PMX-style transform order values.
///
/// # Safety
///
/// `transform_orders` must point to `bone_count` readable `i32` values. Other
/// pointer contracts are the same as `mmd_runtime_model_create_full`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_full_with_transform_order(
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    inverse_bind_matrices: *const f32,
    transform_orders: *const i32,
    bone_count: usize,
    ik_solvers: *const MmdRuntimeFfiIkSolver,
    ik_solver_count: usize,
    ik_links: *const MmdRuntimeFfiIkLink,
    ik_link_count: usize,
    append_transforms: *const MmdRuntimeFfiAppendTransform,
    append_transform_count: usize,
) -> *mut MmdRuntimeModel {
    if parent_indices.is_null()
        || rest_positions_xyz.is_null()
        || transform_orders.is_null()
        || bone_count == 0
    {
        return ptr::null_mut();
    }

    let Some(model) = (unsafe {
        build_model_from_ffi(RawModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders,
            bone_count,
            ik_solvers,
            ik_solver_count,
            ik_links,
            ik_link_count,
            append_transforms,
            append_transform_count,
            morph_count: 0,
            bone_morph_offsets: ptr::null(),
            bone_morph_offset_count: 0,
            group_morph_offsets: ptr::null(),
            group_morph_offset_count: 0,
        })
    }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
}

/// Creates a full model with PMX-style transform order, IK, append transforms,
/// and morph descriptor arrays.
///
/// `morph_count` is the total number of morph slots. Each entry in
/// `bone_morph_offsets` and `group_morph_offsets` carries its own `morph_index`
/// field; the implementation groups entries by morph index internally to build
/// the morph definition tables.
///
/// # Safety
///
/// Pointer contracts for the first 11 parameters are the same as
/// `mmd_runtime_model_create_full_with_transform_order`.
/// `bone_morph_offsets` must be null when `bone_morph_offset_count` is zero, or
/// point to `bone_morph_offset_count` readable descriptors. The same applies to
/// `group_morph_offsets` and `group_morph_offset_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_full_with_morphs(
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    inverse_bind_matrices: *const f32,
    transform_orders: *const i32,
    bone_count: usize,
    ik_solvers: *const MmdRuntimeFfiIkSolver,
    ik_solver_count: usize,
    ik_links: *const MmdRuntimeFfiIkLink,
    ik_link_count: usize,
    append_transforms: *const MmdRuntimeFfiAppendTransform,
    append_transform_count: usize,
    morph_count: u32,
    bone_morph_offsets: *const MmdRuntimeFfiBoneMorphOffset,
    bone_morph_offset_count: usize,
    group_morph_offsets: *const MmdRuntimeFfiGroupMorphOffset,
    group_morph_offset_count: usize,
) -> *mut MmdRuntimeModel {
    if parent_indices.is_null() || rest_positions_xyz.is_null() || bone_count == 0 {
        return ptr::null_mut();
    }

    let Some(model) = (unsafe {
        build_model_from_ffi(RawModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders,
            bone_count,
            ik_solvers,
            ik_solver_count,
            ik_links,
            ik_link_count,
            append_transforms,
            append_transform_count,
            morph_count,
            bone_morph_offsets,
            bone_morph_offset_count,
            group_morph_offsets,
            group_morph_offset_count,
        })
    }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
}

/// Creates a model by importing a PMX binary from byte slice, keeping only
/// runtime-required data alive. The resulting model carries name maps needed
/// to build VMD clips via `mmd_runtime_clip_create_from_vmd_bytes_for_model`.
///
/// # Safety
///
/// `data` must point to `len` readable bytes. Both must remain valid for the
/// duration of the call. Null pointer or zero length returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_from_pmx_bytes(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimeModel {
    if data.is_null() || len == 0 {
        return ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let import = match mmd_anim_format::import_pmx_runtime(bytes) {
        Ok(imp) => imp,
        Err(_) => return ptr::null_mut(),
    };
    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(import.model),
        bone_name_to_index: import.bone_name_to_index,
        morph_name_to_index: import.morph_name_to_index,
        ik_solver_bone_name_to_index: import.ik_solver_bone_name_to_index,
    }))
}

/// Creates an animation clip by importing a VMD motion binary and resolving
/// bone/morph/property IK names through the imported model's name maps.
///
/// The model must have been created via
/// `mmd_runtime_model_create_from_pmx_bytes` (which populates the required
/// name maps). Flat-array-constructed models carry empty name maps and
/// always return null from this function.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create
/// function. `data` must point to `len` readable bytes. A null or zero-length
/// byte input returns null. Import or clip-building failure returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_clip_create_from_vmd_bytes_for_model(
    model: *const MmdRuntimeModel,
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimeClip {
    let Some(model) = (unsafe { model.as_ref() }) else {
        return ptr::null_mut();
    };
    if data.is_null() || len == 0 {
        return ptr::null_mut();
    }
    if model.bone_name_to_index.is_empty() && model.morph_name_to_index.is_empty() {
        return ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let motion = match mmd_anim_format::import_vmd_motion(bytes) {
        Ok(m) => m,
        Err(_) => return ptr::null_mut(),
    };
    let solver_count = model.model.ik_count();
    let clip = mmd_anim_format::build_pair_clip(
        &motion,
        &model.bone_name_to_index,
        &model.morph_name_to_index,
        &model.ik_solver_bone_name_to_index,
        solver_count,
    );
    Box::into_raw(Box::new(MmdRuntimeClip { clip }))
}

/// Returns the number of bones in a model handle, or 0 for null.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_bone_count(model: *const MmdRuntimeModel) -> usize {
    let Some(model) = (unsafe { model.as_ref() }) else {
        return 0;
    };
    model.model.bone_count()
}

/// Returns the number of morph slots in a model handle, or 0 for null.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_morph_count(model: *const MmdRuntimeModel) -> usize {
    let Some(model) = (unsafe { model.as_ref() }) else {
        return 0;
    };
    model.model.morph_count() as usize
}

/// Returns the number of IK solvers in a model handle, or 0 for null.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_ik_count(model: *const MmdRuntimeModel) -> usize {
    let Some(model) = (unsafe { model.as_ref() }) else {
        return 0;
    };
    model.model.ik_count()
}

/// Frees a model created by `mmd_runtime_model_create`.
///
/// # Safety
///
/// `model` must be null or a pointer returned by `mmd_runtime_model_create`
/// that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_free(model: *mut MmdRuntimeModel) {
    if !model.is_null() {
        unsafe {
            drop(Box::from_raw(model));
        }
    }
}

/// Creates a runtime instance sharing the immutable model arena.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by
/// `mmd_runtime_model_create`. A null pointer returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_create(
    model: *const MmdRuntimeModel,
    morph_count: usize,
) -> *mut MmdRuntimeInstance {
    let Some(model) = (unsafe { model.as_ref() }) else {
        return ptr::null_mut();
    };
    let mut inst = MmdRuntimeInstance {
        runtime: RuntimeInstance::new_with_morph_count(Arc::clone(&model.model), morph_count),
        cached_world_matrices: Vec::new(),
        cached_skinning_matrices: Vec::new(),
    };
    inst.refresh_matrix_caches();
    Box::into_raw(Box::new(inst))
}

/// Creates a runtime instance sized from the model's own morph and IK counts.
///
/// This is the preferred constructor for PMX-byte-imported models because the
/// host does not need to preserve a full PMX representation only to learn
/// runtime state sizes.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create function.
/// A null pointer returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_create_for_model(
    model: *const MmdRuntimeModel,
) -> *mut MmdRuntimeInstance {
    let Some(model) = (unsafe { model.as_ref() }) else {
        return ptr::null_mut();
    };
    let mut inst = MmdRuntimeInstance {
        runtime: RuntimeInstance::new(Arc::clone(&model.model)),
        cached_world_matrices: Vec::new(),
        cached_skinning_matrices: Vec::new(),
    };
    inst.refresh_matrix_caches();
    Box::into_raw(Box::new(inst))
}

/// Creates a runtime instance with explicit morph and IK state counts.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by
/// `mmd_runtime_model_create`. A null pointer returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_create_with_counts(
    model: *const MmdRuntimeModel,
    morph_count: usize,
    ik_count: usize,
) -> *mut MmdRuntimeInstance {
    let Some(model) = (unsafe { model.as_ref() }) else {
        return ptr::null_mut();
    };
    let mut inst = MmdRuntimeInstance {
        runtime: RuntimeInstance::new_with_counts(Arc::clone(&model.model), morph_count, ik_count),
        cached_world_matrices: Vec::new(),
        cached_skinning_matrices: Vec::new(),
    };
    inst.refresh_matrix_caches();
    Box::into_raw(Box::new(inst))
}

/// Frees a runtime instance created by `mmd_runtime_instance_create`.
///
/// # Safety
///
/// `instance` must be null or a pointer returned by
/// `mmd_runtime_instance_create` that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_free(instance: *mut MmdRuntimeInstance) {
    if !instance.is_null() {
        unsafe {
            drop(Box::from_raw(instance));
        }
    }
}

/// Evaluates the instance rest pose.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns `false`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_rest_pose(
    instance: *mut MmdRuntimeInstance,
) -> bool {
    let Some(instance) = (unsafe { instance.as_mut() }) else {
        return false;
    };
    instance.runtime.evaluate_rest_pose();
    instance.refresh_matrix_caches();
    true
}

/// Evaluates a clip at `frame`.
///
/// # Safety
///
/// `instance` and `clip` must be null or valid pointers returned by their
/// respective create functions. A null pointer returns `false`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_clip_frame(
    instance: *mut MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    frame: f32,
) -> bool {
    let Some(instance) = (unsafe { instance.as_mut() }) else {
        return false;
    };
    let Some(clip) = (unsafe { clip.as_ref() }) else {
        return false;
    };
    instance.runtime.evaluate_clip_frame(&clip.clip, frame);
    instance.refresh_matrix_caches();
    true
}

/// Evaluates a clip at `frame` with custom IK solver options.
///
/// `ik_tolerance` is clamped to a non-negative finite value; invalid values
/// return `false`. `ik_max_iterations_cap == 0` means no cap.
///
/// # Safety
///
/// `instance` and `clip` must be null or valid pointers returned by their
/// respective create functions. A null pointer returns `false`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_clip_frame_with_ik_options(
    instance: *mut MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    frame: f32,
    ik_tolerance: f32,
    ik_max_iterations_cap: u32,
) -> bool {
    let Some(instance) = (unsafe { instance.as_mut() }) else {
        return false;
    };
    let Some(clip) = (unsafe { clip.as_ref() }) else {
        return false;
    };
    if !ik_tolerance.is_finite() || ik_tolerance < 0.0 {
        return false;
    }
    instance.runtime.evaluate_clip_frame_with_ik_options(
        &clip.clip,
        frame,
        IkSolveOptions {
            tolerance: ik_tolerance,
            max_iterations_cap: if ik_max_iterations_cap == 0 {
                None
            } else {
                Some(ik_max_iterations_cap)
            },
        },
    );
    instance.refresh_matrix_caches();
    true
}

/// Evaluates a clip at `frame` without solving IK.
///
/// This follows the same clip application and morph expansion path as
/// `mmd_runtime_instance_evaluate_clip_frame`, then stops after world matrix
/// update. It is intended for managed/native parity diagnostics.
///
/// # Safety
///
/// `instance` and `clip` must be null or valid pointers returned by their
/// respective create functions. A null pointer returns `false`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_clip_frame_without_ik(
    instance: *mut MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    frame: f32,
) -> bool {
    let Some(instance) = (unsafe { instance.as_mut() }) else {
        return false;
    };
    let Some(clip) = (unsafe { clip.as_ref() }) else {
        return false;
    };
    instance
        .runtime
        .evaluate_clip_frame_without_ik(&clip.clip, frame);
    instance.refresh_matrix_caches();
    true
}

/// Returns the required `f32` count for copying world matrices.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_world_matrix_f32_len(
    instance: *const MmdRuntimeInstance,
) -> usize {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return 0;
    };
    instance.runtime.world_matrices().len() * 16
}

/// Copies world matrices as column-major `f32[16]` values.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. `out_f32` must point to at least
/// `out_f32_len` writable `f32` values. The output region must not alias
/// memory owned by the runtime instance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_copy_world_matrices(
    instance: *const MmdRuntimeInstance,
    out_f32: *mut f32,
    out_f32_len: usize,
) -> bool {
    if out_f32.is_null() {
        return false;
    }
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return false;
    };
    let matrices = instance.runtime.world_matrices();
    let required_len = matrices.len() * 16;
    if out_f32_len < required_len {
        return false;
    }

    let out = unsafe { slice::from_raw_parts_mut(out_f32, required_len) };
    for (matrix_index, matrix) in matrices.iter().enumerate() {
        out[matrix_index * 16..matrix_index * 16 + 16].copy_from_slice(&matrix.to_cols_array());
    }
    true
}

/// Returns the required `f32` count for copying skinning matrices.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_skinning_matrix_f32_len(
    instance: *const MmdRuntimeInstance,
) -> usize {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return 0;
    };
    instance.runtime.skinning_matrices().len() * 16
}

/// Copies skinning matrices as column-major `f32[16]` values.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. `out_f32` must point to at least
/// `out_f32_len` writable `f32` values. The output region must not alias memory
/// owned by the runtime instance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_copy_skinning_matrices(
    instance: *const MmdRuntimeInstance,
    out_f32: *mut f32,
    out_f32_len: usize,
) -> bool {
    if out_f32.is_null() {
        return false;
    }
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return false;
    };
    let matrices = instance.runtime.skinning_matrices();
    let required_len = matrices.len() * 16;
    if out_f32_len < required_len {
        return false;
    }

    let out = unsafe { slice::from_raw_parts_mut(out_f32, required_len) };
    for (matrix_index, matrix) in matrices.iter().enumerate() {
        out[matrix_index * 16..matrix_index * 16 + 16].copy_from_slice(&matrix.to_cols_array());
    }
    true
}

/// Returns the number of bones in the instance's model.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_bone_count(
    instance: *const MmdRuntimeInstance,
) -> usize {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return 0;
    };
    instance.runtime.model().bone_count()
}

/// Returns a pointer to the cached world matrices array.
///
/// The array contains `bone_count * 16` column-major `f32` values (one `Mat4`
/// per bone). The returned pointer is valid until the next evaluation or free
/// call on this instance.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_world_matrices(
    instance: *const MmdRuntimeInstance,
) -> *const f32 {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return ptr::null();
    };
    instance.cached_world_matrices.as_ptr()
}

/// Returns a pointer to the cached skinning matrices array.
///
/// The array contains `bone_count * 16` column-major `f32` values (one `Mat4`
/// per bone). The returned pointer is valid until the next evaluation or free
/// call on this instance.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_skinning_matrices(
    instance: *const MmdRuntimeInstance,
) -> *const f32 {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return ptr::null();
    };
    instance.cached_skinning_matrices.as_ptr()
}

/// Returns the morph weight count for an instance.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_morph_weight_len(
    instance: *const MmdRuntimeInstance,
) -> usize {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return 0;
    };
    instance.runtime.morph_weights().len()
}

/// Copies morph weights into caller-owned memory.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. `out_f32` must point to at least
/// `out_f32_len` writable `f32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_copy_morph_weights(
    instance: *const MmdRuntimeInstance,
    out_f32: *mut f32,
    out_f32_len: usize,
) -> bool {
    if out_f32.is_null() {
        return false;
    }
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return false;
    };
    let weights = instance.runtime.morph_weights();
    if out_f32_len < weights.len() {
        return false;
    }
    let out = unsafe { slice::from_raw_parts_mut(out_f32, weights.len()) };
    out.copy_from_slice(weights);
    true
}

/// Returns the IK enabled-state count for an instance.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_ik_enabled_len(
    instance: *const MmdRuntimeInstance,
) -> usize {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return 0;
    };
    instance.runtime.ik_enabled().len()
}

/// Copies IK enabled states into caller-owned memory.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. `out_u8` must point to at least
/// `out_u8_len` writable `u8` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_copy_ik_enabled(
    instance: *const MmdRuntimeInstance,
    out_u8: *mut u8,
    out_u8_len: usize,
) -> bool {
    if out_u8.is_null() {
        return false;
    }
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return false;
    };
    let states = instance.runtime.ik_enabled();
    if out_u8_len < states.len() {
        return false;
    }
    let out = unsafe { slice::from_raw_parts_mut(out_u8, states.len()) };
    out.copy_from_slice(states);
    true
}

/// Returns a direct pointer to the morph weights array.
///
/// The array contains `morph_weight_len` `f32` values (one per morph). The
/// returned pointer is valid until the next evaluation or free call on this
/// instance.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_morph_weights(
    instance: *const MmdRuntimeInstance,
) -> *const f32 {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return ptr::null();
    };
    instance.runtime.morph_weights().as_ptr()
}

/// Returns a direct pointer to the IK enabled states array.
///
/// The array contains `ik_enabled_len` `u8` values (0 = disabled, 1 = enabled).
/// The returned pointer is valid until the next evaluation or free call on this
/// instance.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_ik_enabled(
    instance: *const MmdRuntimeInstance,
) -> *const u8 {
    let Some(instance) = (unsafe { instance.as_ref() }) else {
        return ptr::null();
    };
    instance.runtime.ik_enabled().as_ptr()
}

/// Creates an animation clip from flat track/keyframe arrays.
///
/// # Safety
///
/// Every non-null pointer must reference the element count supplied beside it.
/// Track keyframe ranges and property IK-state ranges must stay inside their
/// corresponding arrays. All pointers only need to live for this call; the clip
/// copies the data into owned Rust arenas.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_clip_create(
    bone_tracks: *const MmdRuntimeFfiBoneTrack,
    bone_track_count: usize,
    bone_keyframes: *const MmdRuntimeFfiBoneKeyframe,
    bone_keyframe_count: usize,
    morph_tracks: *const MmdRuntimeFfiMorphTrack,
    morph_track_count: usize,
    morph_keyframes: *const MmdRuntimeFfiMorphKeyframe,
    morph_keyframe_count: usize,
    property_keyframes: *const MmdRuntimeFfiPropertyKeyframe,
    property_keyframe_count: usize,
    property_ik_enabled: *const u8,
    property_ik_enabled_count: usize,
) -> *mut MmdRuntimeClip {
    let input = RawClipInput {
        bone_tracks,
        bone_track_count,
        bone_keyframes,
        bone_keyframe_count,
        morph_tracks,
        morph_track_count,
        morph_keyframes,
        morph_keyframe_count,
        property_keyframes,
        property_keyframe_count,
        property_ik_enabled,
        property_ik_enabled_count,
    };
    let Some(clip) = (unsafe { build_clip_from_ffi(input) }) else {
        return ptr::null_mut();
    };
    Box::into_raw(Box::new(MmdRuntimeClip { clip }))
}

/// Writes the inclusive first and last keyed frames for a clip.
///
/// Returns false when `clip`, `out_first_frame`, or `out_last_frame` is null,
/// or when the clip has no keyframes.
///
/// # Safety
///
/// `clip` must be null or a valid pointer returned by a clip create function.
/// Output pointers must be valid for writes when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_clip_frame_range(
    clip: *const MmdRuntimeClip,
    out_first_frame: *mut u32,
    out_last_frame: *mut u32,
) -> bool {
    let Some(clip) = (unsafe { clip.as_ref() }) else {
        return false;
    };
    if out_first_frame.is_null() || out_last_frame.is_null() {
        return false;
    }
    let Some((first, last)) = clip.clip.frame_range() else {
        return false;
    };
    unsafe {
        *out_first_frame = first;
        *out_last_frame = last;
    }
    true
}

/// Frees a clip created by `mmd_runtime_clip_create`.
///
/// # Safety
///
/// `clip` must be null or a pointer returned by `mmd_runtime_clip_create` that
/// has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_clip_free(clip: *mut MmdRuntimeClip) {
    if !clip.is_null() {
        unsafe {
            drop(Box::from_raw(clip));
        }
    }
}

struct RawClipInput {
    bone_tracks: *const MmdRuntimeFfiBoneTrack,
    bone_track_count: usize,
    bone_keyframes: *const MmdRuntimeFfiBoneKeyframe,
    bone_keyframe_count: usize,
    morph_tracks: *const MmdRuntimeFfiMorphTrack,
    morph_track_count: usize,
    morph_keyframes: *const MmdRuntimeFfiMorphKeyframe,
    morph_keyframe_count: usize,
    property_keyframes: *const MmdRuntimeFfiPropertyKeyframe,
    property_keyframe_count: usize,
    property_ik_enabled: *const u8,
    property_ik_enabled_count: usize,
}

unsafe fn build_clip_from_ffi(input: RawClipInput) -> Option<AnimationClip> {
    let bone_tracks = unsafe { checked_slice(input.bone_tracks, input.bone_track_count) }?;
    let bone_keyframes = unsafe { checked_slice(input.bone_keyframes, input.bone_keyframe_count) }?;
    let morph_tracks = unsafe { checked_slice(input.morph_tracks, input.morph_track_count) }?;
    let morph_keyframes =
        unsafe { checked_slice(input.morph_keyframes, input.morph_keyframe_count) }?;
    let property_keyframes =
        unsafe { checked_slice(input.property_keyframes, input.property_keyframe_count) }?;
    let property_ik_enabled =
        unsafe { checked_slice(input.property_ik_enabled, input.property_ik_enabled_count) }?;

    let mut bone_bindings = Vec::with_capacity(bone_tracks.len());
    for track in bone_tracks {
        let keyframes = checked_range(bone_keyframes, track.keyframe_offset, track.keyframe_count)?
            .iter()
            .map(|keyframe| {
                MovableBoneKeyframe::new(
                    keyframe.frame,
                    glam::Vec3A::new(
                        keyframe.position_xyz[0],
                        keyframe.position_xyz[1],
                        keyframe.position_xyz[2],
                    ),
                    glam::Quat::from_xyzw(
                        keyframe.rotation_xyzw[0],
                        keyframe.rotation_xyzw[1],
                        keyframe.rotation_xyzw[2],
                        keyframe.rotation_xyzw[3],
                    ),
                )
            })
            .collect::<Vec<_>>();
        bone_bindings.push(BoneAnimationBinding {
            bone: BoneIndex(track.bone_index),
            track: MovableBoneTrack::from_keyframes(keyframes),
        });
    }

    let mut morph_bindings = Vec::with_capacity(morph_tracks.len());
    for track in morph_tracks {
        let keyframes =
            checked_range(morph_keyframes, track.keyframe_offset, track.keyframe_count)?
                .iter()
                .map(|keyframe| MorphKeyframe::new(keyframe.frame, keyframe.weight))
                .collect::<Vec<_>>();
        morph_bindings.push(MorphAnimationBinding {
            morph: MorphIndex(track.morph_index),
            track: MorphTrack::from_keyframes(keyframes),
        });
    }

    let property_track = if property_keyframes.is_empty() {
        None
    } else {
        let mut keyframes = Vec::with_capacity(property_keyframes.len());
        for keyframe in property_keyframes {
            let ik_enabled = checked_range(
                property_ik_enabled,
                keyframe.ik_enabled_offset,
                keyframe.ik_enabled_count,
            )?
            .iter()
            .map(|state| *state != 0)
            .collect::<Vec<_>>();
            keyframes.push(PropertyKeyframe::new(keyframe.frame, ik_enabled));
        }
        Some(PropertyAnimationBinding::from_keyframes(keyframes))
    };

    Some(AnimationClip::new_full(
        bone_bindings,
        morph_bindings,
        property_track,
    ))
}

unsafe fn checked_slice<'a, T>(ptr: *const T, len: usize) -> Option<&'a [T]> {
    if len == 0 {
        return Some(&[]);
    }
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { slice::from_raw_parts(ptr, len) })
}

fn all_finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

unsafe fn build_ik_chain_definition(
    bones: *const MmdRuntimeFfiRigBone,
    bone_count: usize,
    target_bone_slot: u32,
    links: *const MmdRuntimeFfiRigIkLink,
    link_count: usize,
    iteration_count: u32,
    limit_angle: f32,
) -> Option<IkChainDefinition> {
    if bone_count == 0
        || link_count == 0
        || target_bone_slot as usize >= bone_count
        || !limit_angle.is_finite()
    {
        return None;
    }
    let bones = unsafe { checked_slice(bones, bone_count) }?;
    let links = unsafe { checked_slice(links, link_count) }?;
    let mut parent_slots = Vec::with_capacity(bone_count);
    let mut rest_positions = Vec::with_capacity(bone_count);
    let mut fixed_axes = Vec::with_capacity(bone_count);
    for (slot, bone) in bones.iter().enumerate() {
        if !all_finite(&bone.rest_position_xyz) {
            return None;
        }
        let parent = match bone.parent_slot {
            -1 => None,
            parent if parent >= 0 && (parent as usize) < slot => Some(parent as usize),
            _ => return None,
        };
        let fixed_axis = if bone.flags & RIG_BONE_FIXED_AXIS != 0 {
            if !all_finite(&bone.fixed_axis_xyz) {
                return None;
            }
            let axis = glam::Vec3A::new(
                bone.fixed_axis_xyz[0],
                bone.fixed_axis_xyz[1],
                bone.fixed_axis_xyz[2],
            );
            if axis.length_squared() <= f32::EPSILON {
                return None;
            }
            Some(axis.normalize())
        } else {
            None
        };
        parent_slots.push(parent);
        rest_positions.push(glam::Vec3A::new(
            bone.rest_position_xyz[0],
            bone.rest_position_xyz[1],
            bone.rest_position_xyz[2],
        ));
        fixed_axes.push(fixed_axis);
    }

    let links = links
        .iter()
        .map(|link| {
            if link.bone_slot as usize >= bone_count {
                return None;
            }
            let angle_limit = if link.has_angle_limit {
                if !all_finite(&link.angle_limit_min_xyz) || !all_finite(&link.angle_limit_max_xyz)
                {
                    return None;
                }
                Some(IkAngleLimit::new(
                    glam::Vec3A::new(
                        link.angle_limit_min_xyz[0],
                        link.angle_limit_min_xyz[1],
                        link.angle_limit_min_xyz[2],
                    ),
                    glam::Vec3A::new(
                        link.angle_limit_max_xyz[0],
                        link.angle_limit_max_xyz[1],
                        link.angle_limit_max_xyz[2],
                    ),
                ))
            } else {
                None
            };
            Some(IkChainLinkDefinition {
                bone_slot: link.bone_slot as usize,
                angle_limit,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    Some(IkChainDefinition {
        parent_slots,
        rest_positions,
        fixed_axes,
        target_slot: target_bone_slot as usize,
        links,
        iteration_count,
        limit_angle,
    })
}

fn checked_range<T>(slice: &[T], offset: usize, count: usize) -> Option<&[T]> {
    let end = offset.checked_add(count)?;
    slice.get(offset..end)
}

struct RawModelInput {
    parent_indices: *const i32,
    rest_positions_xyz: *const f32,
    inverse_bind_matrices: *const f32,
    transform_orders: *const i32,
    bone_count: usize,
    ik_solvers: *const MmdRuntimeFfiIkSolver,
    ik_solver_count: usize,
    ik_links: *const MmdRuntimeFfiIkLink,
    ik_link_count: usize,
    append_transforms: *const MmdRuntimeFfiAppendTransform,
    append_transform_count: usize,
    morph_count: u32,
    bone_morph_offsets: *const MmdRuntimeFfiBoneMorphOffset,
    bone_morph_offset_count: usize,
    group_morph_offsets: *const MmdRuntimeFfiGroupMorphOffset,
    group_morph_offset_count: usize,
}

unsafe fn build_model_from_ffi(input: RawModelInput) -> Option<ModelArena> {
    let parents = unsafe { slice::from_raw_parts(input.parent_indices, input.bone_count) };
    let positions =
        unsafe { slice::from_raw_parts(input.rest_positions_xyz, input.bone_count * 3) };
    let inverse_bind_matrices = if input.inverse_bind_matrices.is_null() {
        &[]
    } else {
        unsafe { slice::from_raw_parts(input.inverse_bind_matrices, input.bone_count * 16) }
    };
    let transform_orders = if input.transform_orders.is_null() {
        &[]
    } else {
        unsafe { slice::from_raw_parts(input.transform_orders, input.bone_count) }
    };
    let ik_solvers = unsafe { checked_slice(input.ik_solvers, input.ik_solver_count) }?;
    let ik_links = unsafe { checked_slice(input.ik_links, input.ik_link_count) }?;
    let append_transforms =
        unsafe { checked_slice(input.append_transforms, input.append_transform_count) }?;
    let mut bones = Vec::with_capacity(input.bone_count);

    for (bone_index, parent_index) in parents.iter().enumerate() {
        let parent = match *parent_index {
            -1 => None,
            parent if parent >= 0 => Some(BoneIndex(parent as u32)),
            _ => return None,
        };
        let position_offset = bone_index * 3;
        let mut bone = BoneInit::new(
            parent,
            glam::Vec3A::new(
                positions[position_offset],
                positions[position_offset + 1],
                positions[position_offset + 2],
            ),
        );
        if !inverse_bind_matrices.is_empty() {
            let inverse_bind_offset = bone_index * 16;
            let inverse_bind_matrix = inverse_bind_matrices
                [inverse_bind_offset..inverse_bind_offset + 16]
                .try_into()
                .ok()?;
            bone.inverse_bind_matrix = glam::Mat4::from_cols_array(inverse_bind_matrix);
        }
        if !transform_orders.is_empty() {
            bone.transform_order = transform_orders[bone_index];
        }
        bones.push(bone);
    }

    let ik_solvers = ik_solvers
        .iter()
        .map(|solver| {
            let links = checked_range(ik_links, solver.link_offset, solver.link_count)?
                .iter()
                .map(|link| {
                    let mut init = IkLinkInit::new(BoneIndex(link.bone_index));
                    if link.flags & IK_LINK_FLAG_ANGLE_LIMIT != 0 {
                        init = init.with_angle_limit(IkAngleLimit::new(
                            glam::Vec3A::new(
                                link.angle_limit_min_xyz[0],
                                link.angle_limit_min_xyz[1],
                                link.angle_limit_min_xyz[2],
                            ),
                            glam::Vec3A::new(
                                link.angle_limit_max_xyz[0],
                                link.angle_limit_max_xyz[1],
                                link.angle_limit_max_xyz[2],
                            ),
                        ));
                    }
                    Some(init)
                })
                .collect::<Option<Vec<_>>>()?;
            Some(IkSolverInit {
                ik_bone: BoneIndex(solver.ik_bone_index),
                target_bone: BoneIndex(solver.target_bone_index),
                links,
                iteration_count: solver.iteration_count,
                limit_angle: solver.limit_angle,
            })
        })
        .collect::<Option<Vec<_>>>()?;

    let append_transforms = append_transforms
        .iter()
        .map(|append| {
            let mut init = AppendTransformInit::new(
                BoneIndex(append.target_bone_index),
                BoneIndex(append.source_bone_index),
                append.ratio,
            );
            if append.flags & APPEND_FLAG_ROTATION != 0 {
                init = init.with_rotation();
            }
            if append.flags & APPEND_FLAG_TRANSLATION != 0 {
                init = init.with_translation();
            }
            if append.flags & APPEND_FLAG_LOCAL != 0 {
                init = init.with_local();
            }
            init
        })
        .collect::<Vec<_>>();

    let bone_morph_offsets =
        unsafe { checked_slice(input.bone_morph_offsets, input.bone_morph_offset_count) }?;
    let group_morph_offsets =
        unsafe { checked_slice(input.group_morph_offsets, input.group_morph_offset_count) }?;
    let morph =
        build_morph_init_from_ffi(input.morph_count, bone_morph_offsets, group_morph_offsets)?;

    ModelArena::new_with_morphs(bones, ik_solvers, append_transforms, morph).ok()
}

fn build_morph_init_from_ffi(
    morph_count: u32,
    bone_morph_offsets: &[MmdRuntimeFfiBoneMorphOffset],
    group_morph_offsets: &[MmdRuntimeFfiGroupMorphOffset],
) -> Option<MorphInit> {
    if morph_count == 0 {
        return if bone_morph_offsets.is_empty() && group_morph_offsets.is_empty() {
            Some(MorphInit::default())
        } else {
            None
        };
    }
    let mc = morph_count as usize;

    let (bone_offsets, bone_spans) = if bone_morph_offsets.is_empty() {
        (Vec::new(), vec![MorphOffsetSpan::default(); mc])
    } else {
        let mut sorted: Vec<_> = bone_morph_offsets.iter().collect();
        sorted.sort_by_key(|a| a.morph_index);
        if sorted.last().unwrap().morph_index as usize >= mc {
            return None;
        }
        let mut offsets = Vec::with_capacity(bone_morph_offsets.len());
        let mut spans = vec![MorphOffsetSpan::default(); mc];
        let mut i = 0;
        while i < sorted.len() {
            let morph = sorted[i].morph_index as usize;
            let start = offsets.len() as u32;
            let mut count = 0u32;
            while i < sorted.len() && sorted[i].morph_index == morph as u32 {
                let entry = sorted[i];
                offsets.push(BoneMorphOffset {
                    target_bone: BoneIndex(entry.target_bone_index),
                    position_offset: glam::Vec3A::new(
                        entry.position_offset_xyz[0],
                        entry.position_offset_xyz[1],
                        entry.position_offset_xyz[2],
                    ),
                    rotation_offset: glam::Quat::from_xyzw(
                        entry.rotation_offset_xyzw[0],
                        entry.rotation_offset_xyzw[1],
                        entry.rotation_offset_xyzw[2],
                        entry.rotation_offset_xyzw[3],
                    ),
                });
                count += 1;
                i += 1;
            }
            spans[morph] = MorphOffsetSpan { start, count };
        }
        (offsets, spans)
    };

    let (group_offsets, group_spans) = if group_morph_offsets.is_empty() {
        (Vec::new(), vec![MorphOffsetSpan::default(); mc])
    } else {
        let mut sorted: Vec<_> = group_morph_offsets.iter().collect();
        sorted.sort_by_key(|a| a.morph_index);
        if sorted.last().unwrap().morph_index as usize >= mc {
            return None;
        }
        let mut offsets = Vec::with_capacity(group_morph_offsets.len());
        let mut spans = vec![MorphOffsetSpan::default(); mc];
        let mut i = 0;
        while i < sorted.len() {
            let morph = sorted[i].morph_index as usize;
            let start = offsets.len() as u32;
            let mut count = 0u32;
            while i < sorted.len() && sorted[i].morph_index == morph as u32 {
                let entry = sorted[i];
                offsets.push(GroupMorphOffset {
                    child_morph: MorphIndex(entry.child_morph_index),
                    ratio: entry.ratio,
                });
                count += 1;
                i += 1;
            }
            spans[morph] = MorphOffsetSpan { start, count };
        }
        (offsets, spans)
    };

    Some(MorphInit {
        morph_count,
        bone_offsets,
        bone_spans,
        group_offsets,
        group_spans,
        ..MorphInit::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: f32, expected: f32, tolerance: f32) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "actual={actual} expected={expected} tolerance={tolerance}"
        );
    }

    fn assert_slice_near(actual: &[f32], expected: &[f32], tolerance: f32) {
        assert_eq!(actual.len(), expected.len());
        for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (*actual - *expected).abs() <= tolerance,
                "index={index} actual={actual} expected={expected} tolerance={tolerance}"
            );
        }
    }

    fn simple_ik_chain() -> *mut MmdRuntimeIkChain {
        let bones = [
            MmdRuntimeFfiRigBone {
                parent_slot: -1,
                rest_position_xyz: [0.0, 0.0, 0.0],
                flags: 0,
                fixed_axis_xyz: [0.0, 0.0, 0.0],
            },
            MmdRuntimeFfiRigBone {
                parent_slot: 0,
                rest_position_xyz: [1.0, 0.0, 0.0],
                flags: 0,
                fixed_axis_xyz: [0.0, 0.0, 0.0],
            },
        ];
        let links = [MmdRuntimeFfiRigIkLink {
            bone_slot: 0,
            has_angle_limit: false,
            angle_limit_min_xyz: [0.0, 0.0, 0.0],
            angle_limit_max_xyz: [0.0, 0.0, 0.0],
        }];
        unsafe {
            mmd_runtime_ik_chain_create(
                bones.as_ptr(),
                bones.len(),
                1,
                links.as_ptr(),
                links.len(),
                4,
                0.0,
            )
        }
    }

    #[test]
    fn append_solver_lifecycle_and_expected_output_use_xyzw_quaternion() {
        let config = MmdRuntimeFfiAppendConfig {
            ratio: 0.5,
            affect_rotation: true,
            affect_translation: true,
        };
        let solver = unsafe { mmd_runtime_append_solver_create(&config) };
        assert!(!solver.is_null());

        let source_position = [2.0, 4.0, -6.0];
        let source_rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let mut out_position = [0.0; 3];
        let mut out_rotation = [0.0; 4];
        assert!(unsafe {
            mmd_runtime_append_solver_solve(
                solver,
                source_position.as_ptr(),
                source_rotation.as_ptr(),
                out_position.as_mut_ptr(),
                out_rotation.as_mut_ptr(),
            )
        });

        assert_slice_near(&out_position, &[1.0, 2.0, -3.0], 1.0e-5);
        let solved = glam::Quat::from_xyzw(
            out_rotation[0],
            out_rotation[1],
            out_rotation[2],
            out_rotation[3],
        );
        let rotated_x = solved.mul_vec3(glam::Vec3::X);
        assert_near(rotated_x.x, std::f32::consts::FRAC_1_SQRT_2, 1.0e-5);
        assert_near(rotated_x.y, std::f32::consts::FRAC_1_SQRT_2, 1.0e-5);
        assert_near(out_rotation[3], solved.w, 0.0);

        unsafe { mmd_runtime_append_solver_free(solver) };
    }

    #[test]
    fn append_solver_rejects_null_inputs() {
        let config = MmdRuntimeFfiAppendConfig {
            ratio: 1.0,
            affect_rotation: true,
            affect_translation: true,
        };
        assert!(unsafe { mmd_runtime_append_solver_create(ptr::null()) }.is_null());
        let solver = unsafe { mmd_runtime_append_solver_create(&config) };
        assert!(!solver.is_null());

        let source_position = [0.0; 3];
        let source_rotation = [0.0, 0.0, 0.0, 1.0];
        let mut out_position = [0.0; 3];
        let mut out_rotation = [0.0; 4];
        assert!(!unsafe {
            mmd_runtime_append_solver_solve(
                ptr::null(),
                source_position.as_ptr(),
                source_rotation.as_ptr(),
                out_position.as_mut_ptr(),
                out_rotation.as_mut_ptr(),
            )
        });
        assert!(!unsafe {
            mmd_runtime_append_solver_solve(
                solver,
                ptr::null(),
                source_rotation.as_ptr(),
                out_position.as_mut_ptr(),
                out_rotation.as_mut_ptr(),
            )
        });
        assert!(!unsafe {
            mmd_runtime_append_solver_solve(
                solver,
                source_position.as_ptr(),
                ptr::null(),
                out_position.as_mut_ptr(),
                out_rotation.as_mut_ptr(),
            )
        });

        unsafe { mmd_runtime_append_solver_free(solver) };
    }

    #[test]
    fn ik_chain_lifecycle_solve_converges_and_uses_column_major_parent_matrix() {
        let chain = simple_ik_chain();
        assert!(!chain.is_null());

        let parent_world =
            glam::Mat4::from_translation(glam::Vec3::new(2.0, 0.0, 0.0)).to_cols_array();
        let local_rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let goal = [2.0, 1.0, 0.0];
        let mut out_rotations = [0.0; 4];
        let mut stats = MmdRuntimeFfiIkSolveStats {
            executed_iterations: 0,
            link_steps: 0,
            final_distance: f32::MAX,
            break_reason: u32::MAX,
        };

        assert!(unsafe {
            mmd_runtime_ik_chain_solve(
                chain,
                parent_world.as_ptr(),
                ptr::null(),
                local_rotations.as_ptr(),
                goal.as_ptr(),
                1.0e-3,
                0,
                out_rotations.as_mut_ptr(),
                out_rotations.len(),
                &mut stats,
            )
        });
        assert!(
            stats.final_distance <= 1.0e-3,
            "IK should converge to the goal, stats={:?}",
            (
                stats.executed_iterations,
                stats.link_steps,
                stats.final_distance,
                stats.break_reason
            )
        );
        assert_eq!(stats.break_reason, 0);

        let solved = glam::Quat::from_xyzw(
            out_rotations[0],
            out_rotations[1],
            out_rotations[2],
            out_rotations[3],
        );
        let rotated_x = solved.mul_vec3(glam::Vec3::X);
        assert_near(rotated_x.x, 0.0, 1.0e-3);
        assert_near(rotated_x.y, 1.0, 1.0e-3);
        assert_near(out_rotations[3], solved.w, 0.0);

        unsafe { mmd_runtime_ik_chain_free(chain) };
    }

    #[test]
    fn ik_chain_rejects_null_and_short_buffer_inputs() {
        let chain = simple_ik_chain();
        assert!(!chain.is_null());

        let local_rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
        let goal = [0.0, 1.0, 0.0];
        let mut out_rotations = [0.0; 4];

        assert!(
            unsafe { mmd_runtime_ik_chain_create(ptr::null(), 2, 1, ptr::null(), 1, 1, 0.0) }
                .is_null()
        );
        assert!(!unsafe {
            mmd_runtime_ik_chain_solve(
                ptr::null_mut(),
                ptr::null(),
                ptr::null(),
                local_rotations.as_ptr(),
                goal.as_ptr(),
                1.0e-3,
                0,
                out_rotations.as_mut_ptr(),
                out_rotations.len(),
                ptr::null_mut(),
            )
        });
        assert!(!unsafe {
            mmd_runtime_ik_chain_solve(
                chain,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                goal.as_ptr(),
                1.0e-3,
                0,
                out_rotations.as_mut_ptr(),
                out_rotations.len(),
                ptr::null_mut(),
            )
        });
        assert!(!unsafe {
            mmd_runtime_ik_chain_solve(
                chain,
                ptr::null(),
                ptr::null(),
                local_rotations.as_ptr(),
                goal.as_ptr(),
                1.0e-3,
                0,
                out_rotations.as_mut_ptr(),
                out_rotations.len() - 1,
                ptr::null_mut(),
            )
        });

        unsafe { mmd_runtime_ik_chain_free(chain) };
    }

    #[test]
    fn exports_pmx_from_parts_through_c_abi() {
        let metadata = serde_json::json!({
            "name": "ffi-parts-model",
            "englishName": "ffi-parts-model-en",
            "comment": "built through C ABI",
            "encoding": "utf-8",
            "indexSizes": {
                "vertex": 1,
                "texture": 1,
                "material": 1,
                "bone": 1,
                "morph": 1,
                "rigidBody": 1
            },
            "materialName": "ffi-default-mat"
        })
        .to_string();
        let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let normals = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let uvs = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let indices = [0, 1, 2];

        let buffer = unsafe {
            mmd_runtime_export_pmx_from_parts(
                metadata.as_ptr(),
                metadata.len(),
                positions.as_ptr(),
                3,
                normals.as_ptr(),
                uvs.as_ptr(),
                indices.as_ptr(),
                indices.len(),
                ptr::null(),
                ptr::null(),
                ptr::null(),
            )
        };
        assert!(!buffer.data.is_null());
        assert!(buffer.len > 0);

        let bytes = unsafe { slice::from_raw_parts(buffer.data, buffer.len) };
        let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
        assert_eq!(parsed.metadata.name, "ffi-parts-model");
        assert_eq!(parsed.metadata.english_name, "ffi-parts-model-en");
        assert_eq!(parsed.metadata.counts.vertices, 3);
        assert_eq!(parsed.metadata.counts.faces, 1);
        assert_eq!(parsed.metadata.counts.materials, 1);
        assert_eq!(parsed.metadata.counts.bones, 1);
        assert_eq!(parsed.materials[0].name, "ffi-default-mat");
        assert_eq!(parsed.geometry.indices, vec![0, 1, 2]);

        unsafe {
            mmd_runtime_byte_buffer_free(buffer);
        }
    }

    #[test]
    fn export_pmx_from_parts_rejects_invalid_c_abi_input() {
        let metadata = "{}";
        let positions = [0.0, 0.0, 0.0];
        let normals = [0.0, 0.0, 1.0];
        let uvs = [0.0, 0.0];
        let skin_indices = [0, 0, 0, 0];

        let partial_skin = unsafe {
            mmd_runtime_export_pmx_from_parts(
                metadata.as_ptr(),
                metadata.len(),
                positions.as_ptr(),
                1,
                normals.as_ptr(),
                uvs.as_ptr(),
                ptr::null(),
                0,
                skin_indices.as_ptr(),
                ptr::null(),
                ptr::null(),
            )
        };
        assert!(partial_skin.data.is_null());
        assert_eq!(partial_skin.len, 0);

        let null_metadata = unsafe {
            mmd_runtime_export_pmx_from_parts(
                ptr::null(),
                0,
                positions.as_ptr(),
                1,
                normals.as_ptr(),
                uvs.as_ptr(),
                ptr::null(),
                0,
                ptr::null(),
                ptr::null(),
                ptr::null(),
            )
        };
        assert!(null_metadata.data.is_null());
        assert_eq!(null_metadata.len, 0);
    }

    #[test]
    fn evaluates_rest_pose_through_c_abi() {
        let parents = [-1, 0];
        let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
        assert_eq!(
            unsafe { mmd_runtime_instance_world_matrix_f32_len(instance) },
            32
        );

        let mut matrices = [0.0f32; 32];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                matrices.as_mut_ptr(),
                matrices.len(),
            )
        });
        assert_eq!(matrices[12], 1.0);
        assert_eq!(matrices[16 + 12], 1.0);
        assert_eq!(matrices[16 + 13], 2.0);

        let mut skinning_matrices = [0.0f32; 32];
        assert_eq!(
            unsafe { mmd_runtime_instance_skinning_matrix_f32_len(instance) },
            32
        );
        assert!(unsafe {
            mmd_runtime_instance_copy_skinning_matrices(
                instance,
                skinning_matrices.as_mut_ptr(),
                skinning_matrices.len(),
            )
        });
        assert_eq!(skinning_matrices, matrices);

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn applies_inverse_bind_through_c_abi() {
        let parents = [-1];
        let rest_positions = [2.0, 0.0, 0.0];
        let inverse_bind =
            glam::Mat4::from_translation(glam::Vec3::new(-2.0, 0.0, 0.0)).to_cols_array();
        let model = unsafe {
            mmd_runtime_model_create_with_inverse_bind(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                inverse_bind.as_ptr(),
                1,
            )
        };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

        let mut world_matrices = [0.0f32; 16];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                world_matrices.as_mut_ptr(),
                world_matrices.len(),
            )
        });
        assert_eq!(world_matrices[12], 2.0);

        let mut skinning_matrices = [0.0f32; 16];
        assert!(unsafe {
            mmd_runtime_instance_copy_skinning_matrices(
                instance,
                skinning_matrices.as_mut_ptr(),
                skinning_matrices.len(),
            )
        });
        assert_eq!(skinning_matrices[12], 0.0);

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn creates_ik_solver_through_full_c_abi() {
        let parents = [-1, 0, 1];
        let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let ik_links = [MmdRuntimeFfiIkLink {
            bone_index: 1,
            flags: IK_LINK_FLAG_ANGLE_LIMIT,
            angle_limit_min_xyz: [-1.0, -0.5, -0.25],
            angle_limit_max_xyz: [1.0, 0.5, 0.25],
        }];
        let ik_solvers = [MmdRuntimeFfiIkSolver {
            ik_bone_index: 0,
            target_bone_index: 2,
            link_offset: 0,
            link_count: 1,
            iteration_count: 2,
            limit_angle: 0.5,
        }];
        let model = unsafe {
            mmd_runtime_model_create_full(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                ptr::null(),
                3,
                ik_solvers.as_ptr(),
                ik_solvers.len(),
                ik_links.as_ptr(),
                ik_links.len(),
                ptr::null(),
                0,
            )
        };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        assert_eq!(unsafe { mmd_runtime_instance_ik_enabled_len(instance) }, 1);
        let mut ik_enabled = [0u8; 1];
        assert!(unsafe {
            mmd_runtime_instance_copy_ik_enabled(
                instance,
                ik_enabled.as_mut_ptr(),
                ik_enabled.len(),
            )
        });
        assert_eq!(ik_enabled[0], 1);

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn evaluates_clip_frame_through_c_abi() {
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create_with_counts(model, 1, 1) };
        assert!(!instance.is_null());

        let bone_tracks = [MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 2,
        }];
        let bone_keyframes = [
            MmdRuntimeFfiBoneKeyframe {
                frame: 0,
                position_xyz: [0.0, 0.0, 0.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            },
            MmdRuntimeFfiBoneKeyframe {
                frame: 60,
                position_xyz: [2.0, 0.0, 0.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            },
        ];
        let morph_tracks = [MmdRuntimeFfiMorphTrack {
            morph_index: 0,
            keyframe_offset: 0,
            keyframe_count: 2,
        }];
        let morph_keyframes = [
            MmdRuntimeFfiMorphKeyframe {
                frame: 0,
                weight: 0.0,
            },
            MmdRuntimeFfiMorphKeyframe {
                frame: 60,
                weight: 1.0,
            },
        ];
        let property_keyframes = [
            MmdRuntimeFfiPropertyKeyframe {
                frame: 0,
                ik_enabled_offset: 0,
                ik_enabled_count: 1,
            },
            MmdRuntimeFfiPropertyKeyframe {
                frame: 30,
                ik_enabled_offset: 1,
                ik_enabled_count: 1,
            },
        ];
        let property_ik_enabled = [1u8, 0u8];
        let clip = unsafe {
            mmd_runtime_clip_create(
                bone_tracks.as_ptr(),
                bone_tracks.len(),
                bone_keyframes.as_ptr(),
                bone_keyframes.len(),
                morph_tracks.as_ptr(),
                morph_tracks.len(),
                morph_keyframes.as_ptr(),
                morph_keyframes.len(),
                property_keyframes.as_ptr(),
                property_keyframes.len(),
                property_ik_enabled.as_ptr(),
                property_ik_enabled.len(),
            )
        };
        assert!(!clip.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 30.0) });

        let mut matrices = [0.0f32; 16];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                matrices.as_mut_ptr(),
                matrices.len(),
            )
        });
        assert_eq!(matrices[12], 1.0);

        let mut morph_weights = [0.0f32; 1];
        assert_eq!(
            unsafe { mmd_runtime_instance_morph_weight_len(instance) },
            1
        );
        assert!(unsafe {
            mmd_runtime_instance_copy_morph_weights(
                instance,
                morph_weights.as_mut_ptr(),
                morph_weights.len(),
            )
        });
        assert_eq!(morph_weights[0], 0.5);

        let mut ik_enabled = [1u8; 1];
        assert_eq!(unsafe { mmd_runtime_instance_ik_enabled_len(instance) }, 1);
        assert!(unsafe {
            mmd_runtime_instance_copy_ik_enabled(
                instance,
                ik_enabled.as_mut_ptr(),
                ik_enabled.len(),
            )
        });
        assert_eq!(ik_enabled[0], 0);

        unsafe {
            mmd_runtime_clip_free(clip);
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn evaluates_clip_frame_without_ik_through_c_abi() {
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        let bone_tracks = [MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 2,
        }];
        let bone_keyframes = [
            MmdRuntimeFfiBoneKeyframe {
                frame: 0,
                position_xyz: [0.0, 0.0, 0.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            },
            MmdRuntimeFfiBoneKeyframe {
                frame: 60,
                position_xyz: [2.0, 0.0, 0.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            },
        ];
        let clip = unsafe {
            mmd_runtime_clip_create(
                bone_tracks.as_ptr(),
                bone_tracks.len(),
                bone_keyframes.as_ptr(),
                bone_keyframes.len(),
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
            )
        };
        assert!(!clip.is_null());

        assert!(unsafe {
            mmd_runtime_instance_evaluate_clip_frame_without_ik(instance, clip, 30.0)
        });
        let mut matrices = [0.0f32; 16];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                matrices.as_mut_ptr(),
                matrices.len(),
            )
        });
        assert_eq!(matrices[12], 1.0);

        unsafe {
            mmd_runtime_clip_free(clip);
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn evaluates_append_rotation_through_c_abi() {
        let parents = [-1, -1, 1];
        let rest_positions = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        let append = [MmdRuntimeFfiAppendTransform {
            target_bone_index: 1,
            source_bone_index: 0,
            ratio: 1.0,
            flags: APPEND_FLAG_ROTATION,
        }];
        let model = unsafe {
            mmd_runtime_model_create_with_append(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                3,
                append.as_ptr(),
                append.len(),
            )
        };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        let bone_tracks = [MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 1,
        }];
        let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: rotation,
        }];
        let clip = unsafe {
            mmd_runtime_clip_create(
                bone_tracks.as_ptr(),
                bone_tracks.len(),
                bone_keyframes.as_ptr(),
                bone_keyframes.len(),
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
            )
        };
        assert!(!clip.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 0.0) });
        let mut matrices = [0.0f32; 48];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                matrices.as_mut_ptr(),
                matrices.len(),
            )
        });
        assert!(matrices[32 + 12].abs() < 1.0e-5);
        assert!((matrices[32 + 13] - 1.0).abs() < 1.0e-5);

        unsafe {
            mmd_runtime_clip_free(clip);
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn copy_functions_reject_short_buffer() {
        let parents = [-1, 0];
        let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create_with_counts(model, 1, 1) };
        assert!(!instance.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

        let mut buf32 = [0.0f32; 32];
        assert!(!unsafe {
            mmd_runtime_instance_copy_world_matrices(instance, buf32.as_mut_ptr(), 31)
        });
        assert!(!unsafe {
            mmd_runtime_instance_copy_world_matrices(instance, buf32.as_mut_ptr(), 0)
        });

        assert!(!unsafe {
            mmd_runtime_instance_copy_skinning_matrices(instance, buf32.as_mut_ptr(), 31)
        });
        assert!(!unsafe {
            mmd_runtime_instance_copy_skinning_matrices(instance, buf32.as_mut_ptr(), 0)
        });

        let mut buf_f32 = [0.0f32; 1];
        assert!(!unsafe {
            mmd_runtime_instance_copy_morph_weights(instance, buf_f32.as_mut_ptr(), 0)
        });

        let mut buf_u8 = [0u8; 1];
        assert!(!unsafe { mmd_runtime_instance_copy_ik_enabled(instance, buf_u8.as_mut_ptr(), 0) });

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn applies_transform_order_to_append_chain_through_c_abi() {
        let parents = [-1, -1, -1, 1];
        let rest_positions = [
            0.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, //
            0.0, 0.0, 0.0, //
            1.0, 0.0, 0.0,
        ];
        let transform_orders = [0, 2, 1, 3];
        let append = [
            MmdRuntimeFfiAppendTransform {
                target_bone_index: 2,
                source_bone_index: 0,
                ratio: 1.0,
                flags: APPEND_FLAG_ROTATION,
            },
            MmdRuntimeFfiAppendTransform {
                target_bone_index: 1,
                source_bone_index: 2,
                ratio: 1.0,
                flags: APPEND_FLAG_ROTATION,
            },
        ];
        let model = unsafe {
            mmd_runtime_model_create_full_with_transform_order(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                ptr::null(),
                transform_orders.as_ptr(),
                4,
                ptr::null(),
                0,
                ptr::null(),
                0,
                append.as_ptr(),
                append.len(),
            )
        };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        let bone_tracks = [MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 1,
        }];
        let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: rotation,
        }];
        let clip = unsafe {
            mmd_runtime_clip_create(
                bone_tracks.as_ptr(),
                bone_tracks.len(),
                bone_keyframes.as_ptr(),
                bone_keyframes.len(),
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
            )
        };
        assert!(!clip.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 0.0) });
        let mut matrices = [0.0f32; 64];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                matrices.as_mut_ptr(),
                matrices.len(),
            )
        });
        assert!(matrices[48 + 12].abs() < 1.0e-5);
        assert!((matrices[48 + 13] - 1.0).abs() < 1.0e-5);

        unsafe {
            mmd_runtime_clip_free(clip);
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn creates_bone_morph_through_c_abi() {
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
            morph_index: 0,
            target_bone_index: 0,
            position_offset_xyz: [2.0, 0.0, 0.0],
            rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
        }];
        let model = unsafe {
            mmd_runtime_model_create_full_with_morphs(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                1,
                bone_morphs.as_ptr(),
                bone_morphs.len(),
                ptr::null(),
                0,
            )
        };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 1) };
        assert!(!instance.is_null());

        let bone_tracks = [MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 1,
        }];
        let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: rotation,
        }];
        let morph_tracks = [MmdRuntimeFfiMorphTrack {
            morph_index: 0,
            keyframe_offset: 0,
            keyframe_count: 2,
        }];
        let morph_keyframes = [
            MmdRuntimeFfiMorphKeyframe {
                frame: 0,
                weight: 0.0,
            },
            MmdRuntimeFfiMorphKeyframe {
                frame: 60,
                weight: 1.0,
            },
        ];
        let clip = unsafe {
            mmd_runtime_clip_create(
                bone_tracks.as_ptr(),
                bone_tracks.len(),
                bone_keyframes.as_ptr(),
                bone_keyframes.len(),
                morph_tracks.as_ptr(),
                morph_tracks.len(),
                morph_keyframes.as_ptr(),
                morph_keyframes.len(),
                ptr::null(),
                0,
                ptr::null(),
                0,
            )
        };
        assert!(!clip.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 60.0) });
        let mut morph_weights = [0.0f32; 1];
        assert!(unsafe {
            mmd_runtime_instance_copy_morph_weights(
                instance,
                morph_weights.as_mut_ptr(),
                morph_weights.len(),
            )
        });
        assert_eq!(morph_weights[0], 1.0);

        let mut matrices = [0.0f32; 16];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                matrices.as_mut_ptr(),
                matrices.len(),
            )
        });
        assert!((matrices[12] - 2.0).abs() < 1.0e-5);

        unsafe {
            mmd_runtime_clip_free(clip);
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn rejects_null_bone_morph_with_nonzero_count() {
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let model = unsafe {
            mmd_runtime_model_create_full_with_morphs(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                1,
                ptr::null(),
                1,
                ptr::null(),
                0,
            )
        };
        assert!(model.is_null());
    }

    #[test]
    fn rejects_morph_count_zero_with_bone_data() {
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
            morph_index: 0,
            target_bone_index: 0,
            position_offset_xyz: [1.0, 0.0, 0.0],
            rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
        }];
        let model = unsafe {
            mmd_runtime_model_create_full_with_morphs(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                0,
                bone_morphs.as_ptr(),
                bone_morphs.len(),
                ptr::null(),
                0,
            )
        };
        assert!(model.is_null());
    }

    // -----------------------------------------------------------------------
    // Phase 6: direct output view tests
    // -----------------------------------------------------------------------

    #[test]
    fn bone_count_returns_correct_value() {
        let parents = [-1, 0, 1];
        let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 3) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        assert_eq!(unsafe { mmd_runtime_instance_bone_count(instance) }, 3);

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn model_count_accessors_return_expected_values() {
        let parents = [-1, 0, 1];
        let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let transform_orders = [0, 1, 2];
        let ik_links = [MmdRuntimeFfiIkLink {
            bone_index: 1,
            flags: 0,
            angle_limit_min_xyz: [0.0, 0.0, 0.0],
            angle_limit_max_xyz: [0.0, 0.0, 0.0],
        }];
        let ik_solvers = [MmdRuntimeFfiIkSolver {
            ik_bone_index: 2,
            target_bone_index: 0,
            link_offset: 0,
            link_count: 1,
            iteration_count: 1,
            limit_angle: 1.0,
        }];
        let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
            morph_index: 1,
            target_bone_index: 0,
            position_offset_xyz: [1.0, 0.0, 0.0],
            rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
        }];
        let model = unsafe {
            mmd_runtime_model_create_full_with_morphs(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                ptr::null(),
                transform_orders.as_ptr(),
                3,
                ik_solvers.as_ptr(),
                ik_solvers.len(),
                ik_links.as_ptr(),
                ik_links.len(),
                ptr::null(),
                0,
                2,
                bone_morphs.as_ptr(),
                bone_morphs.len(),
                ptr::null(),
                0,
            )
        };
        assert!(!model.is_null());

        assert_eq!(unsafe { mmd_runtime_model_bone_count(model) }, 3);
        assert_eq!(unsafe { mmd_runtime_model_morph_count(model) }, 2);
        assert_eq!(unsafe { mmd_runtime_model_ik_count(model) }, 1);

        unsafe {
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn model_count_accessors_return_zero_for_null() {
        assert_eq!(unsafe { mmd_runtime_model_bone_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_model_morph_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_model_ik_count(ptr::null()) }, 0);
    }

    #[test]
    fn instance_create_for_model_uses_model_counts() {
        let parents = [-1, 0, 1];
        let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0];
        let transform_orders = [0, 1, 2];
        let ik_links = [MmdRuntimeFfiIkLink {
            bone_index: 1,
            flags: 0,
            angle_limit_min_xyz: [0.0, 0.0, 0.0],
            angle_limit_max_xyz: [0.0, 0.0, 0.0],
        }];
        let ik_solvers = [MmdRuntimeFfiIkSolver {
            ik_bone_index: 2,
            target_bone_index: 0,
            link_offset: 0,
            link_count: 1,
            iteration_count: 1,
            limit_angle: 1.0,
        }];
        let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
            morph_index: 1,
            target_bone_index: 0,
            position_offset_xyz: [1.0, 0.0, 0.0],
            rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
        }];
        let model = unsafe {
            mmd_runtime_model_create_full_with_morphs(
                parents.as_ptr(),
                rest_positions.as_ptr(),
                ptr::null(),
                transform_orders.as_ptr(),
                3,
                ik_solvers.as_ptr(),
                ik_solvers.len(),
                ik_links.as_ptr(),
                ik_links.len(),
                ptr::null(),
                0,
                2,
                bone_morphs.as_ptr(),
                bone_morphs.len(),
                ptr::null(),
                0,
            )
        };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create_for_model(model) };
        assert!(!instance.is_null());

        assert_eq!(unsafe { mmd_runtime_instance_bone_count(instance) }, 3);
        assert_eq!(
            unsafe { mmd_runtime_instance_morph_weight_len(instance) },
            2
        );
        assert_eq!(unsafe { mmd_runtime_instance_ik_enabled_len(instance) }, 1);

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn instance_create_for_model_returns_null_for_null() {
        assert!(unsafe { mmd_runtime_instance_create_for_model(ptr::null()) }.is_null());
    }

    #[test]
    fn bone_count_returns_zero_for_null() {
        assert_eq!(unsafe { mmd_runtime_instance_bone_count(ptr::null()) }, 0);
    }

    #[test]
    fn pointer_view_returns_non_null_after_evaluation() {
        let parents = [-1, 0];
        let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

        let world_ptr = unsafe { mmd_runtime_instance_world_matrices(instance) };
        assert!(!world_ptr.is_null());

        let skin_ptr = unsafe { mmd_runtime_instance_skinning_matrices(instance) };
        assert!(!skin_ptr.is_null());

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn pointer_view_returns_null_for_null_instance() {
        assert!(unsafe { mmd_runtime_instance_world_matrices(ptr::null()) }.is_null());
        assert!(unsafe { mmd_runtime_instance_skinning_matrices(ptr::null()) }.is_null());
    }

    #[test]
    fn pointer_view_contains_expected_translation() {
        let parents = [-1, 0];
        let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

        let world_ptr = unsafe { mmd_runtime_instance_world_matrices(instance) };
        assert!(!world_ptr.is_null());

        // column-major: translation is at indices [12, 13, 14]
        unsafe {
            assert_eq!(*world_ptr.add(12), 1.0);
            assert_eq!(*world_ptr.add(16 + 12), 1.0);
            assert_eq!(*world_ptr.add(16 + 13), 2.0);
        }

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn pointer_view_consistent_with_copy_api() {
        let parents = [-1, 0];
        let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create(model, 0) };
        assert!(!instance.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

        // Read via pointer view
        let world_ptr = unsafe { mmd_runtime_instance_world_matrices(instance) };
        let world_slice = unsafe { std::slice::from_raw_parts(world_ptr, 32) };

        // Read via copy API
        let mut copy_buf = [0.0f32; 32];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                instance,
                copy_buf.as_mut_ptr(),
                copy_buf.len(),
            )
        });

        assert_eq!(world_slice, &copy_buf);

        // Same for skinning
        let skin_ptr = unsafe { mmd_runtime_instance_skinning_matrices(instance) };
        let skin_slice = unsafe { std::slice::from_raw_parts(skin_ptr, 32) };

        let mut skin_copy = [0.0f32; 32];
        assert!(unsafe {
            mmd_runtime_instance_copy_skinning_matrices(
                instance,
                skin_copy.as_mut_ptr(),
                skin_copy.len(),
            )
        });

        assert_eq!(skin_slice, &skin_copy);

        unsafe {
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    // -----------------------------------------------------------------------
    // Phase 6b: morph/IK direct pointer view tests
    // -----------------------------------------------------------------------

    #[test]
    fn morph_ik_direct_pointer_returns_null_for_null_instance() {
        assert!(unsafe { mmd_runtime_instance_morph_weights(ptr::null()) }.is_null());
        assert!(unsafe { mmd_runtime_instance_ik_enabled(ptr::null()) }.is_null());
    }

    #[test]
    fn morph_ik_direct_pointer_consistent_with_copy_api() {
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
        assert!(!model.is_null());
        let instance = unsafe { mmd_runtime_instance_create_with_counts(model, 1, 1) };
        assert!(!instance.is_null());

        let bone_tracks = [MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 1,
        }];
        let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        }];
        let morph_tracks = [MmdRuntimeFfiMorphTrack {
            morph_index: 0,
            keyframe_offset: 0,
            keyframe_count: 2,
        }];
        let morph_keyframes = [
            MmdRuntimeFfiMorphKeyframe {
                frame: 0,
                weight: 0.0,
            },
            MmdRuntimeFfiMorphKeyframe {
                frame: 60,
                weight: 1.0,
            },
        ];
        let property_keyframes = [MmdRuntimeFfiPropertyKeyframe {
            frame: 0,
            ik_enabled_offset: 0,
            ik_enabled_count: 1,
        }];
        let property_ik_enabled = [1u8];
        let clip = unsafe {
            mmd_runtime_clip_create(
                bone_tracks.as_ptr(),
                bone_tracks.len(),
                bone_keyframes.as_ptr(),
                bone_keyframes.len(),
                morph_tracks.as_ptr(),
                morph_tracks.len(),
                morph_keyframes.as_ptr(),
                morph_keyframes.len(),
                property_keyframes.as_ptr(),
                property_keyframes.len(),
                property_ik_enabled.as_ptr(),
                property_ik_enabled.len(),
            )
        };
        assert!(!clip.is_null());

        assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 30.0) });

        // Direct pointer read
        let morph_ptr = unsafe { mmd_runtime_instance_morph_weights(instance) };
        assert!(!morph_ptr.is_null());
        let morph_slice = unsafe { std::slice::from_raw_parts(morph_ptr, 1) };

        let ik_ptr = unsafe { mmd_runtime_instance_ik_enabled(instance) };
        assert!(!ik_ptr.is_null());
        let ik_slice = unsafe { std::slice::from_raw_parts(ik_ptr, 1) };

        // Copy API read
        let mut morph_copy = [0.0f32; 1];
        assert!(unsafe {
            mmd_runtime_instance_copy_morph_weights(instance, morph_copy.as_mut_ptr(), 1)
        });

        let mut ik_copy = [0u8; 1];
        assert!(unsafe { mmd_runtime_instance_copy_ik_enabled(instance, ik_copy.as_mut_ptr(), 1) });

        assert_eq!(morph_slice, &morph_copy);
        assert_eq!(ik_slice, &ik_copy);

        unsafe {
            mmd_runtime_clip_free(clip);
            mmd_runtime_instance_free(instance);
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn clip_frame_range_reports_all_track_frames() {
        let bone_tracks = [MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 1,
        }];
        let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
            frame: 30,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        }];
        let morph_tracks = [MmdRuntimeFfiMorphTrack {
            morph_index: 0,
            keyframe_offset: 0,
            keyframe_count: 2,
        }];
        let morph_keyframes = [
            MmdRuntimeFfiMorphKeyframe {
                frame: 10,
                weight: 0.0,
            },
            MmdRuntimeFfiMorphKeyframe {
                frame: 60,
                weight: 1.0,
            },
        ];
        let property_keyframes = [MmdRuntimeFfiPropertyKeyframe {
            frame: 5,
            ik_enabled_offset: 0,
            ik_enabled_count: 1,
        }];
        let property_ik_enabled = [1_u8];
        let clip = unsafe {
            mmd_runtime_clip_create(
                bone_tracks.as_ptr(),
                bone_tracks.len(),
                bone_keyframes.as_ptr(),
                bone_keyframes.len(),
                morph_tracks.as_ptr(),
                morph_tracks.len(),
                morph_keyframes.as_ptr(),
                morph_keyframes.len(),
                property_keyframes.as_ptr(),
                property_keyframes.len(),
                property_ik_enabled.as_ptr(),
                property_ik_enabled.len(),
            )
        };
        assert!(!clip.is_null());

        let mut first = 0;
        let mut last = 0;
        assert!(unsafe { mmd_runtime_clip_frame_range(clip, &mut first, &mut last) });
        assert_eq!((first, last), (5, 60));

        unsafe {
            mmd_runtime_clip_free(clip);
        }
    }

    #[test]
    fn clip_frame_range_rejects_null_or_empty() {
        let mut first = 99;
        let mut last = 99;
        assert!(!unsafe { mmd_runtime_clip_frame_range(ptr::null(), &mut first, &mut last) });
        assert_eq!((first, last), (99, 99));

        let empty_clip = unsafe {
            mmd_runtime_clip_create(
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
                ptr::null(),
                0,
            )
        };
        assert!(!empty_clip.is_null());
        assert!(!unsafe { mmd_runtime_clip_frame_range(empty_clip, &mut first, &mut last) });
        assert!(!unsafe { mmd_runtime_clip_frame_range(empty_clip, ptr::null_mut(), &mut last) });
        assert!(!unsafe { mmd_runtime_clip_frame_range(empty_clip, &mut first, ptr::null_mut()) });

        unsafe {
            mmd_runtime_clip_free(empty_clip);
        }
    }

    // -----------------------------------------------------------------------
    // PMX/VMD byte-import ABI tests (Phase 9)
    // -----------------------------------------------------------------------

    #[test]
    fn import_pmx_bytes_rejects_null() {
        assert!(unsafe { mmd_runtime_model_create_from_pmx_bytes(ptr::null(), 0) }.is_null());
        assert!(unsafe { mmd_runtime_model_create_from_pmx_bytes(ptr::null(), 100) }.is_null());
        let dummy = 0u8;
        assert!(
            unsafe { mmd_runtime_model_create_from_pmx_bytes(&dummy as *const u8, 0) }.is_null()
        );
    }

    #[test]
    fn import_pmx_bytes_rejects_garbage() {
        let garbage = [0u8; 32];
        let model =
            unsafe { mmd_runtime_model_create_from_pmx_bytes(garbage.as_ptr(), garbage.len()) };
        assert!(model.is_null());
    }

    #[test]
    fn import_vmd_bytes_for_model_rejects_null_and_empty() {
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
        assert!(!model.is_null());

        // Null model
        assert!(
            unsafe {
                mmd_runtime_clip_create_from_vmd_bytes_for_model(ptr::null(), ptr::null(), 0)
            }
            .is_null()
        );
        // Null bytes
        assert!(
            unsafe { mmd_runtime_clip_create_from_vmd_bytes_for_model(model, ptr::null(), 100) }
                .is_null()
        );
        // Zero length
        let dummy = 0u8;
        assert!(
            unsafe {
                mmd_runtime_clip_create_from_vmd_bytes_for_model(model, &dummy as *const u8, 0)
            }
            .is_null()
        );

        unsafe {
            mmd_runtime_model_free(model);
        }
    }

    #[test]
    fn flat_array_model_returns_null_from_vmd_import() {
        // Flat-array constructed models have empty name maps, so VMD import
        // should return null.
        let parents = [-1];
        let rest_positions = [0.0, 0.0, 0.0];
        let model =
            unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
        assert!(!model.is_null());

        let garbage = [0u8; 32];
        assert!(
            unsafe {
                mmd_runtime_clip_create_from_vmd_bytes_for_model(
                    model,
                    garbage.as_ptr(),
                    garbage.len(),
                )
            }
            .is_null()
        );

        unsafe {
            mmd_runtime_model_free(model);
        }
    }

    // -----------------------------------------------------------------------
    //  JSON / geometry buffer API tests
    // -----------------------------------------------------------------------

    #[test]
    fn vmd_json_rejects_null_empty_invalid() {
        let null_empty = unsafe { mmd_runtime_parse_vmd_json(ptr::null(), 0) };
        assert!(null_empty.data.is_null());
        assert_eq!(null_empty.len, 0);

        let null_nonempty = unsafe { mmd_runtime_parse_vmd_json(ptr::null(), 10) };
        assert!(null_nonempty.data.is_null());
        assert_eq!(null_nonempty.len, 0);

        let d = 0u8;
        let empty = unsafe { mmd_runtime_parse_vmd_json(&d as *const u8, 0) };
        assert!(empty.data.is_null());
        assert_eq!(empty.len, 0);

        let garbage = [0u8; 16];
        let invalid = unsafe { mmd_runtime_parse_vmd_json(garbage.as_ptr(), garbage.len()) };
        assert!(invalid.data.is_null());
        assert_eq!(invalid.len, 0);
    }

    #[test]
    fn vmd_json_serializes_camera_fixture() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
        let json_buf = unsafe { mmd_runtime_parse_vmd_json(bytes.as_ptr(), bytes.len()) };
        assert!(!json_buf.data.is_null());
        assert!(json_buf.len > 0);

        let json_str =
            unsafe { str::from_utf8(slice::from_raw_parts(json_buf.data, json_buf.len)) }.unwrap();
        let v: serde_json::Value = serde_json::from_str(json_str).unwrap();
        assert!(v.is_object(), "vmd json must be an object");

        unsafe { mmd_runtime_byte_buffer_free(json_buf) };
    }

    #[test]
    fn pmx_non_geometry_json_rejects_null_empty_invalid() {
        let null_empty = unsafe { mmd_runtime_parse_pmx_non_geometry_json(ptr::null(), 0) };
        assert!(null_empty.data.is_null());
        assert_eq!(null_empty.len, 0);

        let null_nonempty = unsafe { mmd_runtime_parse_pmx_non_geometry_json(ptr::null(), 10) };
        assert!(null_nonempty.data.is_null());
        assert_eq!(null_nonempty.len, 0);

        let d = 0u8;
        let empty = unsafe { mmd_runtime_parse_pmx_non_geometry_json(&d as *const u8, 0) };
        assert!(empty.data.is_null());
        assert_eq!(empty.len, 0);

        let garbage = [0u8; 16];
        let invalid =
            unsafe { mmd_runtime_parse_pmx_non_geometry_json(garbage.as_ptr(), garbage.len()) };
        assert!(invalid.data.is_null());
        assert_eq!(invalid.len, 0);
    }

    #[test]
    fn pmx_non_geometry_json_omits_geometry_and_normalizes_fields() {
        let bytes: &[u8] =
            include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let json_buf =
            unsafe { mmd_runtime_parse_pmx_non_geometry_json(bytes.as_ptr(), bytes.len()) };
        assert!(!json_buf.data.is_null());
        assert!(json_buf.len > 0);

        let json_str =
            unsafe { str::from_utf8(slice::from_raw_parts(json_buf.data, json_buf.len)) }.unwrap();
        let v: serde_json::Value = serde_json::from_str(json_str).unwrap();

        // geometry field must not be present
        assert!(v.get("geometry").is_none(), "geometry must be omitted");

        // required non-geometry fields must be present
        assert!(v.get("metadata").is_some());
        assert!(v.get("materials").is_some());
        assert!(v.get("skeleton").is_some());
        assert!(v.get("morphs").is_some());

        // sharedToonIndex null -> -1
        if let Some(mats) = v.get("materials").and_then(|m| m.as_array()) {
            for mat in mats {
                if let Some(idx) = mat.get("sharedToonIndex") {
                    assert!(
                        !idx.is_null(),
                        "sharedToonIndex must not be null in output JSON"
                    );
                }
            }
        }

        // externalParentKey null -> -1
        if let Some(bones) = v
            .get("skeleton")
            .and_then(|s| s.get("bones"))
            .and_then(|b| b.as_array())
        {
            for bone in bones {
                if let Some(key) = bone.get("externalParentKey") {
                    assert!(
                        !key.is_null(),
                        "externalParentKey must not be null in output JSON"
                    );
                }
            }
        }

        unsafe { mmd_runtime_byte_buffer_free(json_buf) };
    }

    #[test]
    fn pmx_geometry_buffers_reject_null_empty_invalid() {
        macro_rules! check_rejects {
            ($fn:ident) => {{
                let null = unsafe { $fn(ptr::null(), 0) };
                assert!(null.data.is_null(), stringify!($fn null));
                assert_eq!(null.len, 0, stringify!($fn null len));

                let d = 0u8;
                let empty = unsafe { $fn(&d as *const u8, 0) };
                assert!(empty.data.is_null(), stringify!($fn empty));

                let garbage = [0u8; 16];
                let invalid = unsafe { $fn(garbage.as_ptr(), garbage.len()) };
                assert!(invalid.data.is_null(), stringify!($fn invalid));
            }};
        }

        check_rejects!(mmd_runtime_parse_pmx_positions_buffer);
        check_rejects!(mmd_runtime_parse_pmx_normals_buffer);
        check_rejects!(mmd_runtime_parse_pmx_uvs_buffer);
        check_rejects!(mmd_runtime_parse_pmx_indices_buffer);
        check_rejects!(mmd_runtime_parse_pmx_material_groups_buffer);
        check_rejects!(mmd_runtime_parse_pmx_skin_indices_buffer);
        check_rejects!(mmd_runtime_parse_pmx_skin_weights_buffer);
        check_rejects!(mmd_runtime_parse_pmx_edge_scale_buffer);
        check_rejects!(mmd_runtime_parse_pmx_sdef_enabled_buffer);
        check_rejects!(mmd_runtime_parse_pmx_sdef_c_buffer);
        check_rejects!(mmd_runtime_parse_pmx_sdef_r0_buffer);
        check_rejects!(mmd_runtime_parse_pmx_sdef_r1_buffer);
        check_rejects!(mmd_runtime_parse_pmx_sdef_rw0_buffer);
        check_rejects!(mmd_runtime_parse_pmx_sdef_rw1_buffer);
        check_rejects!(mmd_runtime_parse_pmx_qdef_enabled_buffer);
        check_rejects!(mmd_runtime_parse_pmx_skinning_modes_json);

        assert_eq!(
            unsafe { mmd_runtime_parse_pmx_additional_uv_count(ptr::null(), 0) },
            0
        );
        let d = 0u8;
        assert_eq!(
            unsafe { mmd_runtime_parse_pmx_additional_uv_count(&d as *const u8, 0) },
            0
        );
        let garbage = [0u8; 16];
        assert_eq!(
            unsafe { mmd_runtime_parse_pmx_additional_uv_count(garbage.as_ptr(), garbage.len()) },
            0
        );

        let null = unsafe { mmd_runtime_parse_pmx_additional_uvs_buffer(ptr::null(), 0, 0) };
        assert!(null.data.is_null(), "additional UV null");
        assert_eq!(null.len, 0, "additional UV null len");

        let empty = unsafe { mmd_runtime_parse_pmx_additional_uvs_buffer(&d as *const u8, 0, 0) };
        assert!(empty.data.is_null(), "additional UV empty");

        let invalid = unsafe {
            mmd_runtime_parse_pmx_additional_uvs_buffer(garbage.as_ptr(), garbage.len(), 0)
        };
        assert!(invalid.data.is_null(), "additional UV invalid");
    }

    fn ffi_buffer_to_vec(buffer: MmdRuntimeFfiByteBuffer) -> Vec<u8> {
        let bytes = if buffer.data.is_null() || buffer.len == 0 {
            Vec::new()
        } else {
            unsafe { slice::from_raw_parts(buffer.data, buffer.len).to_vec() }
        };
        unsafe { mmd_runtime_byte_buffer_free(buffer) };
        bytes
    }

    fn assert_empty_ffi_buffer(buffer: MmdRuntimeFfiByteBuffer, context: &str) {
        assert!(buffer.data.is_null(), "{context}: data must be null");
        assert_eq!(buffer.len, 0, "{context}: len must be zero");
    }

    fn assert_material_split_geometry_invariants(
        split: *mut MmdRuntimePmxMaterialSplit,
        manifest: &serde_json::Value,
        context: &str,
    ) {
        let mesh_count = unsafe { mmd_runtime_pmx_material_split_mesh_count(split) };
        assert!(mesh_count > 0, "{context}: mesh_count must be positive");
        assert_eq!(
            manifest.get("meshCount").and_then(|v| v.as_u64()),
            Some(mesh_count as u64),
            "{context}: manifest meshCount must match mesh_count"
        );

        let meshes = manifest
            .get("meshes")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("{context}: manifest meshes must be an array"));
        assert_eq!(
            meshes.len(),
            mesh_count,
            "{context}: manifest mesh array length must match mesh_count"
        );

        for mesh_index in 0..mesh_count {
            let mesh_context = format!("{context}: mesh {mesh_index}");
            let mesh_manifest = meshes
                .iter()
                .find(|mesh| {
                    mesh.get("meshIndex").and_then(|v| v.as_u64()) == Some(mesh_index as u64)
                })
                .unwrap_or_else(|| panic!("{mesh_context}: manifest mesh entry missing"));

            let positions = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_positions_buffer(split, mesh_index)
            });
            assert_eq!(
                positions.len() % (3 * 4),
                0,
                "{mesh_context}: positions len must be xyz f32 aligned"
            );
            let vertex_count = positions.len() / (3 * 4);

            let normals = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_normals_buffer(split, mesh_index)
            });
            assert_eq!(
                normals.len(),
                positions.len(),
                "{mesh_context}: normals len"
            );

            let uvs = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_uvs_buffer(split, mesh_index)
            });
            assert_eq!(uvs.len(), vertex_count * 2 * 4, "{mesh_context}: uvs len");

            let skin_indices = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_skin_indices_buffer(split, mesh_index)
            });
            assert_eq!(
                skin_indices.len(),
                vertex_count * 4 * 4,
                "{mesh_context}: skin_indices len"
            );

            let skin_weights = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_skin_weights_buffer(split, mesh_index)
            });
            assert_eq!(
                skin_weights.len(),
                vertex_count * 4 * 4,
                "{mesh_context}: skin_weights len"
            );

            let edge_scale = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_edge_scale_buffer(split, mesh_index)
            });
            assert_eq!(
                edge_scale.len(),
                vertex_count * 4,
                "{mesh_context}: edge_scale len"
            );

            let sdef_enabled = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_sdef_enabled_buffer(split, mesh_index)
            });
            assert_eq!(
                sdef_enabled.len(),
                vertex_count,
                "{mesh_context}: sdef_enabled len"
            );

            let qdef_enabled = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_qdef_enabled_buffer(split, mesh_index)
            });
            assert_eq!(
                qdef_enabled.len(),
                vertex_count,
                "{mesh_context}: qdef_enabled len"
            );

            macro_rules! check_vec3_f32_buffer {
                ($fn:ident, $name:literal) => {{
                    let buf = ffi_buffer_to_vec(unsafe { $fn(split, mesh_index) });
                    assert_eq!(
                        buf.len(),
                        vertex_count * 3 * 4,
                        "{}: {} len",
                        mesh_context,
                        $name
                    );
                }};
            }

            check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_c_buffer, "sdef_c");
            check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_r0_buffer, "sdef_r0");
            check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_r1_buffer, "sdef_r1");
            check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_rw0_buffer, "sdef_rw0");
            check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_rw1_buffer, "sdef_rw1");

            let indices = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_indices_buffer(split, mesh_index)
            });
            assert_eq!(
                indices.len() % 4,
                0,
                "{mesh_context}: indices len must be u32 aligned"
            );
            for (index_offset, index_bytes) in indices.chunks_exact(4).enumerate() {
                let index = u32::from_ne_bytes(index_bytes.try_into().unwrap()) as usize;
                assert!(
                    index < vertex_count,
                    "{mesh_context}: index {index_offset} value {index} must be < vertex_count {vertex_count}"
                );
            }

            for uv_index in 0..4 {
                let additional_uvs = ffi_buffer_to_vec(unsafe {
                    mmd_runtime_pmx_material_split_additional_uvs_buffer(
                        split, mesh_index, uv_index,
                    )
                });
                if !additional_uvs.is_empty() {
                    assert_eq!(
                        additional_uvs.len(),
                        vertex_count * 4 * 4,
                        "{mesh_context}: additional_uvs[{uv_index}] len"
                    );
                }
            }

            let original_vertex_indices = mesh_manifest
                .get("originalVertexIndices")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| {
                    panic!("{mesh_context}: originalVertexIndices must be an array")
                });
            assert_eq!(
                original_vertex_indices.len(),
                vertex_count,
                "{mesh_context}: originalVertexIndices len"
            );

            let morph_index_map = mesh_manifest
                .get("morphIndexMap")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("{mesh_context}: morphIndexMap must be an array"));
            let mut seen_local_indices = vec![false; morph_index_map.len()];
            for entry in morph_index_map {
                let local_index = entry
                    .get("localIndex")
                    .and_then(|v| v.as_u64())
                    .unwrap_or_else(|| panic!("{mesh_context}: localIndex missing"))
                    as usize;
                assert!(
                    local_index < morph_index_map.len(),
                    "{mesh_context}: localIndex {local_index} out of range"
                );
                assert!(
                    !seen_local_indices[local_index],
                    "{mesh_context}: duplicate localIndex {local_index}"
                );
                seen_local_indices[local_index] = true;
            }
            assert!(
                seen_local_indices.iter().all(|seen| *seen),
                "{mesh_context}: localIndex values must be contiguous from zero"
            );
        }
    }

    fn assert_material_split_rejects_null_and_out_of_range(
        split: *mut MmdRuntimePmxMaterialSplit,
        mesh_count: usize,
    ) {
        assert_eq!(
            unsafe { mmd_runtime_pmx_material_split_mesh_count(ptr::null()) },
            0
        );
        assert_empty_ffi_buffer(
            unsafe { mmd_runtime_pmx_material_split_manifest_json(ptr::null()) },
            "null material split manifest",
        );

        macro_rules! check_empty_getter {
            ($fn:ident) => {{
                assert_empty_ffi_buffer(
                    unsafe { $fn(ptr::null(), 0) },
                    concat!(stringify!($fn), " null"),
                );
                assert_empty_ffi_buffer(
                    unsafe { $fn(split, mesh_count) },
                    concat!(stringify!($fn), " out of range"),
                );
            }};
        }

        check_empty_getter!(mmd_runtime_pmx_material_split_positions_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_normals_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_uvs_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_indices_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_skin_indices_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_skin_weights_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_edge_scale_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_sdef_enabled_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_sdef_c_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_sdef_r0_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_sdef_r1_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_sdef_rw0_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_sdef_rw1_buffer);
        check_empty_getter!(mmd_runtime_pmx_material_split_qdef_enabled_buffer);

        assert_empty_ffi_buffer(
            unsafe { mmd_runtime_pmx_material_split_additional_uvs_buffer(ptr::null(), 0, 0) },
            "additional_uvs null",
        );
        assert_empty_ffi_buffer(
            unsafe { mmd_runtime_pmx_material_split_additional_uvs_buffer(split, mesh_count, 0) },
            "additional_uvs out of range",
        );
    }

    fn material_split_manifest_json(
        split: *mut MmdRuntimePmxMaterialSplit,
        context: &str,
    ) -> serde_json::Value {
        let manifest_bytes =
            ffi_buffer_to_vec(unsafe { mmd_runtime_pmx_material_split_manifest_json(split) });
        assert!(
            !manifest_bytes.is_empty(),
            "{context}: manifest_json must not be empty"
        );
        serde_json::from_slice(&manifest_bytes)
            .unwrap_or_else(|err| panic!("{context}: manifest_json parse failed: {err}"))
    }

    fn rig_spec_manifest_json(
        spec: *mut MmdRuntimePmxRigSpec,
        context: &str,
    ) -> serde_json::Value {
        let manifest_bytes =
            ffi_buffer_to_vec(unsafe { mmd_runtime_pmx_rig_spec_manifest_json(spec) });
        assert!(
            !manifest_bytes.is_empty(),
            "{context}: manifest_json must not be empty"
        );
        serde_json::from_slice(&manifest_bytes)
            .unwrap_or_else(|err| panic!("{context}: manifest_json parse failed: {err}"))
    }

    fn assert_json_array3(value: &serde_json::Value, context: &str) {
        let array = value
            .as_array()
            .unwrap_or_else(|| panic!("{context}: must be an array"));
        assert_eq!(array.len(), 3, "{context}: must have three elements");
        assert!(
            array.iter().all(|item| item.is_number()),
            "{context}: elements must be numbers"
        );
    }

    #[test]
    fn rig_spec_manifest_json_has_expected_shape() {
        let bytes: &[u8] =
            include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let spec = unsafe { mmd_runtime_pmx_rig_spec_create(bytes.as_ptr(), bytes.len()) };
        assert!(!spec.is_null(), "rig spec handle must not be null");

        let manifest = rig_spec_manifest_json(spec, "fixture rig spec");
        let bone_count = manifest
            .get("boneCount")
            .and_then(|v| v.as_u64())
            .expect("fixture rig spec: boneCount must be a number");
        let ik_chain_count = manifest
            .get("ikChainCount")
            .and_then(|v| v.as_u64())
            .expect("fixture rig spec: ikChainCount must be a number");
        let grant_count = manifest
            .get("grantCount")
            .and_then(|v| v.as_u64())
            .expect("fixture rig spec: grantCount must be a number");

        let bones = manifest
            .get("bones")
            .and_then(|v| v.as_array())
            .expect("fixture rig spec: bones must be an array");
        let ik_chains = manifest
            .get("ikChains")
            .and_then(|v| v.as_array())
            .expect("fixture rig spec: ikChains must be an array");
        let grants = manifest
            .get("grants")
            .and_then(|v| v.as_array())
            .expect("fixture rig spec: grants must be an array");

        assert_eq!(bones.len(), bone_count as usize, "boneCount mismatch");
        assert_eq!(
            ik_chains.len(),
            ik_chain_count as usize,
            "ikChainCount mismatch"
        );
        assert_eq!(grants.len(), grant_count as usize, "grantCount mismatch");
        assert!(bone_count > 0, "fixture rig spec: boneCount must be positive");
        assert!(
            ik_chain_count > 0,
            "fixture rig spec: ikChainCount must be positive"
        );

        for (bone_index, bone) in bones.iter().enumerate() {
            let context = format!("fixture rig spec: bone {bone_index}");
            assert!(bone.get("name").is_some_and(|v| v.is_string()), "{context}: name");
            assert!(
                bone.get("nameBytes").is_some_and(|v| v.is_string()),
                "{context}: nameBytes"
            );
            assert!(
                bone.get("parentIndex").is_some_and(|v| v.is_number()),
                "{context}: parentIndex"
            );
            assert_json_array3(
                bone.get("restPosition")
                    .unwrap_or_else(|| panic!("{context}: restPosition missing")),
                &format!("{context}: restPosition"),
            );
            assert!(
                bone.get("deformLayer").is_some_and(|v| v.is_number()),
                "{context}: deformLayer"
            );
            assert!(bone.get("fixedAxis").is_some(), "{context}: fixedAxis missing");
            assert!(bone.get("localAxis").is_some(), "{context}: localAxis missing");
            assert!(
                bone.get("transformAfterPhysics")
                    .is_some_and(|v| v.is_boolean()),
                "{context}: transformAfterPhysics"
            );
            if let Some(local_axis) = bone.get("localAxis").filter(|v| !v.is_null()) {
                assert_json_array3(
                    local_axis
                        .get("x")
                        .unwrap_or_else(|| panic!("{context}: localAxis.x missing")),
                    &format!("{context}: localAxis.x"),
                );
                assert_json_array3(
                    local_axis
                        .get("z")
                        .unwrap_or_else(|| panic!("{context}: localAxis.z missing")),
                    &format!("{context}: localAxis.z"),
                );
            }
        }

        for (chain_index, chain) in ik_chains.iter().enumerate() {
            let context = format!("fixture rig spec: ik chain {chain_index}");
            assert!(
                chain
                    .get("controllerBoneIndex")
                    .is_some_and(|v| v.is_number()),
                "{context}: controllerBoneIndex"
            );
            assert!(
                chain.get("targetBoneIndex").is_some_and(|v| v.is_number()),
                "{context}: targetBoneIndex"
            );
            assert!(
                chain.get("iterationCount").is_some_and(|v| v.is_number()),
                "{context}: iterationCount"
            );
            assert!(
                chain.get("limitAngle").is_some_and(|v| v.is_number()),
                "{context}: limitAngle"
            );
            let links = chain
                .get("links")
                .and_then(|v| v.as_array())
                .unwrap_or_else(|| panic!("{context}: links must be an array"));
            for (link_index, link) in links.iter().enumerate() {
                let context = format!("{context}: link {link_index}");
                assert!(
                    link.get("boneIndex").is_some_and(|v| v.is_number()),
                    "{context}: boneIndex"
                );
                assert!(
                    link.get("hasAngleLimit").is_some_and(|v| v.is_boolean()),
                    "{context}: hasAngleLimit"
                );
                assert_json_array3(
                    link.get("angleLimitMin")
                        .unwrap_or_else(|| panic!("{context}: angleLimitMin missing")),
                    &format!("{context}: angleLimitMin"),
                );
                assert_json_array3(
                    link.get("angleLimitMax")
                        .unwrap_or_else(|| panic!("{context}: angleLimitMax missing")),
                    &format!("{context}: angleLimitMax"),
                );
            }
        }

        for (grant_index, grant) in grants.iter().enumerate() {
            let context = format!("fixture rig spec: grant {grant_index}");
            assert!(
                grant.get("targetBoneIndex").is_some_and(|v| v.is_number()),
                "{context}: targetBoneIndex"
            );
            assert!(
                grant.get("sourceBoneIndex").is_some_and(|v| v.is_number()),
                "{context}: sourceBoneIndex"
            );
            assert!(
                grant.get("ratio").is_some_and(|v| v.is_number()),
                "{context}: ratio"
            );
            assert!(
                grant.get("affectRotation").is_some_and(|v| v.is_boolean()),
                "{context}: affectRotation"
            );
            assert!(
                grant
                    .get("affectTranslation")
                    .is_some_and(|v| v.is_boolean()),
                "{context}: affectTranslation"
            );
            assert!(
                grant.get("local").is_some_and(|v| v.is_boolean()),
                "{context}: local"
            );
        }

        unsafe { mmd_runtime_pmx_rig_spec_free(spec) };
    }

    #[test]
    fn rig_spec_rejects_null_and_invalid_input() {
        let null_spec = unsafe { mmd_runtime_pmx_rig_spec_create(ptr::null(), 1) };
        assert!(null_spec.is_null(), "null input must return null handle");

        let byte = 0_u8;
        let empty_spec = unsafe { mmd_runtime_pmx_rig_spec_create(&byte as *const u8, 0) };
        assert!(empty_spec.is_null(), "empty input must return null handle");

        let garbage = b"not a pmx";
        let invalid_spec =
            unsafe { mmd_runtime_pmx_rig_spec_create(garbage.as_ptr(), garbage.len()) };
        assert!(
            invalid_spec.is_null(),
            "invalid input must return null handle"
        );

        assert_empty_ffi_buffer(
            unsafe { mmd_runtime_pmx_rig_spec_manifest_json(ptr::null()) },
            "null rig spec manifest",
        );
        unsafe { mmd_runtime_pmx_rig_spec_free(ptr::null_mut()) };
    }

    #[test]
    fn pmx_material_split_buffers_have_consistent_dimensions() {
        let bytes: &[u8] =
            include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let split =
            unsafe { mmd_runtime_pmx_material_split_create(bytes.as_ptr(), bytes.len(), 0) };
        assert!(!split.is_null(), "material split handle must not be null");

        let mesh_count = unsafe { mmd_runtime_pmx_material_split_mesh_count(split) };
        let manifest = material_split_manifest_json(split, "fixture material split");
        assert_material_split_geometry_invariants(split, &manifest, "fixture material split");
        assert_material_split_rejects_null_and_out_of_range(split, mesh_count);

        unsafe { mmd_runtime_pmx_material_split_free(split) };
    }

    #[test]
    fn pmx_geometry_buffers_have_correct_dimensions() {
        let bytes: &[u8] =
            include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
        let vertex_count = parsed.metadata.counts.vertices as usize;
        let index_count = parsed.metadata.counts.faces as usize * 3;
        let additional_uv_count = parsed.geometry.additional_uvs.len();
        let material_group_count = parsed.geometry.material_groups.len();

        macro_rules! check_buf {
            ($fn:ident, $expected_bytes:expr) => {{
                let buf = unsafe { $fn(bytes.as_ptr(), bytes.len()) };
                assert!(!buf.data.is_null(), stringify!($fn must not be null));
                assert_eq!(
                    buf.len,
                    $expected_bytes,
                    stringify!($fn dimension mismatch)
                );
                unsafe { mmd_runtime_byte_buffer_free(buf) };
            }};
        }

        check_buf!(mmd_runtime_parse_pmx_positions_buffer, vertex_count * 3 * 4);
        check_buf!(mmd_runtime_parse_pmx_normals_buffer, vertex_count * 3 * 4);
        check_buf!(mmd_runtime_parse_pmx_uvs_buffer, vertex_count * 2 * 4);
        check_buf!(mmd_runtime_parse_pmx_indices_buffer, index_count * 4);
        check_buf!(
            mmd_runtime_parse_pmx_material_groups_buffer,
            material_group_count * 3 * 4
        );
        check_buf!(
            mmd_runtime_parse_pmx_skin_indices_buffer,
            vertex_count * 4 * 4
        );
        check_buf!(
            mmd_runtime_parse_pmx_skin_weights_buffer,
            vertex_count * 4 * 4
        );
        check_buf!(mmd_runtime_parse_pmx_edge_scale_buffer, vertex_count * 4);
        check_buf!(mmd_runtime_parse_pmx_sdef_enabled_buffer, vertex_count);
        check_buf!(mmd_runtime_parse_pmx_sdef_c_buffer, vertex_count * 3 * 4);
        check_buf!(mmd_runtime_parse_pmx_sdef_r0_buffer, vertex_count * 3 * 4);
        check_buf!(mmd_runtime_parse_pmx_sdef_r1_buffer, vertex_count * 3 * 4);
        check_buf!(mmd_runtime_parse_pmx_sdef_rw0_buffer, vertex_count * 3 * 4);
        check_buf!(mmd_runtime_parse_pmx_sdef_rw1_buffer, vertex_count * 3 * 4);
        check_buf!(mmd_runtime_parse_pmx_qdef_enabled_buffer, vertex_count);

        assert_eq!(
            unsafe { mmd_runtime_parse_pmx_additional_uv_count(bytes.as_ptr(), bytes.len()) },
            additional_uv_count
        );
        for uv_index in 0..additional_uv_count {
            let buf = unsafe {
                mmd_runtime_parse_pmx_additional_uvs_buffer(bytes.as_ptr(), bytes.len(), uv_index)
            };
            assert!(
                !buf.data.is_null(),
                "additional UV channel {uv_index} must not be null"
            );
            assert_eq!(
                buf.len,
                vertex_count * 4 * 4,
                "additional UV channel {uv_index} dimension mismatch"
            );
            unsafe { mmd_runtime_byte_buffer_free(buf) };
        }
        let invalid_uv = unsafe {
            mmd_runtime_parse_pmx_additional_uvs_buffer(
                bytes.as_ptr(),
                bytes.len(),
                additional_uv_count,
            )
        };
        assert!(invalid_uv.data.is_null(), "invalid additional UV index");
        assert_eq!(invalid_uv.len, 0, "invalid additional UV index len");
    }

    #[test]
    fn pmx_skinning_modes_json_has_correct_shape() {
        let bytes: &[u8] =
            include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
        let vertex_count = parsed.metadata.counts.vertices as usize;

        let buf = unsafe { mmd_runtime_parse_pmx_skinning_modes_json(bytes.as_ptr(), bytes.len()) };
        assert!(!buf.data.is_null());
        assert!(buf.len > 0);

        let json_str = unsafe { str::from_utf8(slice::from_raw_parts(buf.data, buf.len)) }.unwrap();
        let v: serde_json::Value = serde_json::from_str(json_str).unwrap();

        let modes = v
            .get("skinningModes")
            .and_then(|m| m.as_array())
            .expect("skinningModes array must be present");
        assert_eq!(modes.len(), vertex_count);
        for mode in modes {
            let s = mode.as_str().expect("each skinning mode must be a string");
            assert!(
                matches!(s, "bdef1" | "bdef2" | "bdef4" | "sdef" | "qdef"),
                "unexpected skinning mode: {s}"
            );
        }

        unsafe { mmd_runtime_byte_buffer_free(buf) };
    }
}
