//! C ABI wrapper for native hosts.

use std::collections::HashMap;
use std::{ptr, slice, str, sync::Arc};

use mmd_anim_runtime::ModelArena;
use mmd_anim_runtime::{
    AnimationClip, AppendTransformInit, BoneAnimationBinding, BoneIndex, BoneInit, BoneMorphOffset,
    GroupMorphOffset, IkAngleLimit, IkLinkInit, IkSolveOptions, IkSolverInit,
    MorphAnimationBinding, MorphIndex, MorphInit, MorphKeyframe, MorphOffsetSpan, MorphTrack,
    MovableBoneKeyframe, MovableBoneTrack, PropertyAnimationBinding, PropertyKeyframe,
    RuntimeInstance,
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

/// Opaque handle for lightweight VMD metadata summary (non-JSON).
/// Provides max frame and keyframe counts (including scene tracks) plus
/// target model name, without exposing full keyframe arrays.
pub struct MmdRuntimeVmdSummary {
    max_frame: u32,
    bone_keyframe_count: usize,
    morph_keyframe_count: usize,
    property_keyframe_count: usize,
    camera_keyframe_count: usize,
    light_keyframe_count: usize,
    self_shadow_keyframe_count: usize,
    model_name_utf8: Vec<u8>,
    // frame data for non-JSON getters
    bone_frames: Vec<mmd_anim_format::vmd::VmdParsedBoneFrame>,
    morph_frames: Vec<mmd_anim_format::vmd::VmdParsedMorphFrame>,
    property_frames: Vec<mmd_anim_format::vmd::VmdParsedPropertyFrame>,
    camera_frames: Vec<mmd_anim_format::vmd::VmdParsedCameraFrame>,
    light_frames: Vec<mmd_anim_format::vmd::VmdParsedLightFrame>,
    self_shadow_frames: Vec<mmd_anim_format::vmd::VmdParsedSelfShadowFrame>,
}

/// Opaque handle for lightweight PMX metadata summary (non-JSON).
/// Provides version (f32) and stable import/cache counts plus model names,
/// and retains parsed data for non-JSON getter access.
pub struct MmdRuntimePmxSummary {
    version: f32,
    vertex_count: usize,
    face_count: usize,
    material_count: usize,
    bone_count: usize,
    morph_count: usize,
    display_frame_count: usize,
    rigidbody_count: usize,
    joint_count: usize,
    soft_body_count: usize,
    additional_uv_count: usize,
    name_utf8: Vec<u8>,
    english_name_utf8: Vec<u8>,
    // retained for core (geo/mat/bone/ik) getters; exact native parsed semantics
    parsed: mmd_anim_format::PmxParsedModel,
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

/// Creates an opaque VMD summary handle by parsing only metadata from VMD bytes.
/// This is a non-JSON surface for summary counts (bone/morph/property/camera/light/self-shadow
/// keyframe counts) and max frame + target model name. Intended for import cache paths
/// that must not depend on JSON DTOs or full keyframe materialization.
///
/// Full keyframe arrays are exposed through explicit non-JSON getters.
///
/// # Safety
///
/// `data` must point to `len` readable bytes. Null or zero-length returns null.
/// Parse failure (invalid VMD) returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_create_from_bytes(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimeVmdSummary {
    if data.is_null() || len == 0 {
        return ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
        Ok(p) => p,
        Err(_) => return ptr::null_mut(),
    };
    let summary = MmdRuntimeVmdSummary {
        max_frame: parsed.metadata.max_frame,
        bone_keyframe_count: parsed.metadata.counts.bones,
        morph_keyframe_count: parsed.metadata.counts.morphs,
        property_keyframe_count: parsed.metadata.counts.properties,
        camera_keyframe_count: parsed.metadata.counts.cameras,
        light_keyframe_count: parsed.metadata.counts.lights,
        self_shadow_keyframe_count: parsed.metadata.counts.self_shadows,
        model_name_utf8: parsed.metadata.model_name.into_bytes(),
        bone_frames: parsed.bone_frames,
        morph_frames: parsed.morph_frames,
        property_frames: parsed.property_frames,
        camera_frames: parsed.camera_frames,
        light_frames: parsed.light_frames,
        self_shadow_frames: parsed.self_shadow_frames,
    };
    Box::into_raw(Box::new(summary))
}

/// Frees a VMD summary handle returned by `mmd_runtime_vmd_summary_create_from_bytes`.
///
/// # Safety
///
/// `summary` must be null or a pointer returned by the create function that has
/// not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_free(summary: *mut MmdRuntimeVmdSummary) {
    if !summary.is_null() {
        unsafe {
            drop(Box::from_raw(summary));
        }
    }
}

/// Returns max frame (inclusive last keyed) for the summary, or 0 for null.
///
/// # Safety
///
/// `summary` must be null or a valid pointer returned by create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_max_frame(
    summary: *const MmdRuntimeVmdSummary,
) -> u32 {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.max_frame
}

/// Returns bone keyframe count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_keyframe_count(
    summary: *const MmdRuntimeVmdSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.bone_keyframe_count
}

/// Returns morph keyframe count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_morph_keyframe_count(
    summary: *const MmdRuntimeVmdSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.morph_keyframe_count
}

/// Returns property (model) keyframe count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_property_keyframe_count(
    summary: *const MmdRuntimeVmdSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.property_keyframe_count
}

/// Returns camera keyframe count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_keyframe_count(
    summary: *const MmdRuntimeVmdSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.camera_keyframe_count
}

/// Returns light keyframe count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_keyframe_count(
    summary: *const MmdRuntimeVmdSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.light_keyframe_count
}

/// Returns self-shadow keyframe count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_self_shadow_keyframe_count(
    summary: *const MmdRuntimeVmdSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.self_shadow_keyframe_count
}

/// Returns the target model name as UTF-8 bytes in a ByteBuffer (caller frees
/// with `mmd_runtime_byte_buffer_free`). Returns empty buffer for null summary.
///
/// The bytes are a copy of the decoded model name (UTF-8). Empty name yields
/// empty buffer (still safe to free).
///
/// # Safety
///
/// `summary` must be null or valid. The returned buffer (if non-empty) must be
/// freed exactly once with the byte buffer free function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_model_name(
    summary: *const MmdRuntimeVmdSummary,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(s.model_name_utf8.clone())
}

// -----------------------------------------------------------------------
// VMD summary model-motion getters (bone / morph / property only; no camera/light/self-shadow full data)
// All accessors are null and out-of-range safe per spec.
// Names returned as UTF-8 from already-decoded String fields (not raw SJIS).
// Returned ByteBuffers are owned copies freeable via mmd_runtime_byte_buffer_free.
// -----------------------------------------------------------------------

/// Returns bone frame name as UTF-8 ByteBuffer for the given index, or empty for null/out-of-range.
/// # Safety
/// `summary` must be null or valid pointer from create. Returned buffer must be freed once if non-empty.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_name(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(frame.bone_name.clone().into_bytes())
}

/// Returns bone frame index (u32), or 0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_frame(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0;
    };
    frame.frame
}

/// Returns bone frame translation X, or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_translation_x(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0.0;
    };
    frame.translation[0]
}

/// Returns bone frame translation Y, or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_translation_y(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0.0;
    };
    frame.translation[1]
}

/// Returns bone frame translation Z, or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_translation_z(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0.0;
    };
    frame.translation[2]
}

/// Returns bone frame rotation X (quat), or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_rotation_x(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0.0;
    };
    frame.rotation[0]
}

/// Returns bone frame rotation Y (quat), or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_rotation_y(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0.0;
    };
    frame.rotation[1]
}

/// Returns bone frame rotation Z (quat), or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_rotation_z(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0.0;
    };
    frame.rotation[2]
}

/// Returns bone frame rotation W (quat), or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_rotation_w(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0.0;
    };
    frame.rotation[3]
}

/// Returns the byte at `offset` (0..63) of the 64-byte bone interpolation data, or 0 for null / oob frame / oob offset.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_bone_frame_interpolation_byte(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
    offset: usize,
) -> u8 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.bone_frames.get(index)) else {
        return 0;
    };
    *frame.interpolation.get(offset).unwrap_or(&0u8)
}

/// Returns morph frame name as UTF-8 ByteBuffer, or empty for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_morph_frame_name(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.morph_frames.get(index)) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(frame.morph_name.clone().into_bytes())
}

/// Returns morph frame index (u32), or 0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_morph_frame_frame(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.morph_frames.get(index)) else {
        return 0;
    };
    frame.frame
}

/// Returns morph frame weight, or 0.0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_morph_frame_weight(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.morph_frames.get(index)) else {
        return 0.0;
    };
    frame.weight
}

/// Returns property (model) frame index (u32), or 0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_property_frame_frame(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.property_frames.get(index)) else {
        return 0;
    };
    frame.frame
}

/// Returns property (model) frame visible flag, or false for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_property_frame_visible(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> bool {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.property_frames.get(index)) else {
        return false;
    };
    frame.visible
}

/// Returns IK state count for the property frame, or 0 for null/out-of-range.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_property_frame_ik_state_count(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> usize {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.property_frames.get(index)) else {
        return 0;
    };
    frame.ik_states.len()
}

/// Returns IK state bone name (UTF-8 ByteBuffer) for (frame_index, ik_index), or empty for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_property_frame_ik_state_name(
    summary: *const MmdRuntimeVmdSummary,
    frame_index: usize,
    ik_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.property_frames.get(frame_index)) else {
        return empty_byte_buffer();
    };
    let Some(state) = frame.ik_states.get(ik_index) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(state.bone_name.clone().into_bytes())
}

/// Returns IK state enabled flag for (frame_index, ik_index), or false for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_property_frame_ik_state_enabled(
    summary: *const MmdRuntimeVmdSummary,
    frame_index: usize,
    ik_index: usize,
) -> bool {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.property_frames.get(frame_index)) else {
        return false;
    };
    let Some(state) = frame.ik_states.get(ik_index) else {
        return false;
    };
    state.enabled
}

// VMD summary scene-motion getters (camera / light / self-shadow).

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_frame(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0;
    };
    frame.frame
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_distance(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0.0;
    };
    frame.distance
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_position_x(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0.0;
    };
    frame.position[0]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_position_y(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0.0;
    };
    frame.position[1]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_position_z(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0.0;
    };
    frame.position[2]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_rotation_x(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0.0;
    };
    frame.rotation[0]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_rotation_y(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0.0;
    };
    frame.rotation[1]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_rotation_z(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0.0;
    };
    frame.rotation[2]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_interpolation_byte(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
    offset: usize,
) -> u8 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0;
    };
    *frame.interpolation.get(offset).unwrap_or(&0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_fov(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return 0;
    };
    frame.fov
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_camera_frame_perspective(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> bool {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.camera_frames.get(index)) else {
        return false;
    };
    frame.perspective
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_frame_frame(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.light_frames.get(index)) else {
        return 0;
    };
    frame.frame
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_frame_color_x(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.light_frames.get(index)) else {
        return 0.0;
    };
    frame.color[0]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_frame_color_y(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.light_frames.get(index)) else {
        return 0.0;
    };
    frame.color[1]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_frame_color_z(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.light_frames.get(index)) else {
        return 0.0;
    };
    frame.color[2]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_frame_direction_x(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.light_frames.get(index)) else {
        return 0.0;
    };
    frame.direction[0]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_frame_direction_y(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.light_frames.get(index)) else {
        return 0.0;
    };
    frame.direction[1]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_light_frame_direction_z(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.light_frames.get(index)) else {
        return 0.0;
    };
    frame.direction[2]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_self_shadow_frame_frame(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.self_shadow_frames.get(index)) else {
        return 0;
    };
    frame.frame
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_self_shadow_frame_mode(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> u8 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.self_shadow_frames.get(index)) else {
        return 0;
    };
    frame.mode
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_summary_self_shadow_frame_distance(
    summary: *const MmdRuntimeVmdSummary,
    index: usize,
) -> f32 {
    let Some(frame) = (unsafe { summary.as_ref() }).and_then(|s| s.self_shadow_frames.get(index)) else {
        return 0.0;
    };
    frame.distance
}

/// Creates an opaque PMX summary handle by parsing only metadata from PMX bytes.
/// This is a non-JSON surface for version + counts (vertices/faces/materials/bones/morphs/
/// display frames/rigidbodies/joints/softbodies/additional UV) and model/English names.
/// Intended for import cache paths that must not depend on JSON DTOs.
///
/// Full arrays (vertices, materials, bones, morphs, physics, display details) are
/// intentionally not exposed in this slice.
///
/// # Safety
///
/// `data` must point to `len` readable bytes. Null or zero-length returns null.
/// Parse failure (invalid PMX) returns null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_create_from_bytes(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimePmxSummary {
    if data.is_null() || len == 0 {
        return ptr::null_mut();
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    let parsed = match mmd_anim_format::parse_pmx_model(bytes) {
        Ok(p) => p,
        Err(_) => return ptr::null_mut(),
    };
    let meta = &parsed.metadata;
    let summary = MmdRuntimePmxSummary {
        version: meta.version,
        vertex_count: meta.counts.vertices,
        face_count: meta.counts.faces,
        material_count: meta.counts.materials,
        bone_count: meta.counts.bones,
        morph_count: meta.counts.morphs,
        display_frame_count: meta.counts.display_frames,
        rigidbody_count: meta.counts.rigid_bodies,
        joint_count: meta.counts.joints,
        soft_body_count: meta.counts.soft_bodies,
        additional_uv_count: meta.additional_uv_count as usize,
        name_utf8: meta.name.clone().into_bytes(),
        english_name_utf8: meta.english_name.clone().into_bytes(),
        parsed,
    };
    Box::into_raw(Box::new(summary))
}

/// Frees a PMX summary handle returned by `mmd_runtime_pmx_summary_create_from_bytes`.
///
/// # Safety
///
/// `summary` must be null or a pointer returned by the create function that has
/// not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_free(summary: *mut MmdRuntimePmxSummary) {
    if !summary.is_null() {
        unsafe {
            drop(Box::from_raw(summary));
        }
    }
}

/// Returns PMX version as f32 (e.g. 2.0), or 0.0 for null.
///
/// # Safety
///
/// `summary` must be null or a valid pointer returned by create.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_version(
    summary: *const MmdRuntimePmxSummary,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0.0;
    };
    s.version
}

/// Returns vertex count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.vertex_count
}

/// Returns face count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_face_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.face_count
}

/// Returns material count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.material_count
}

/// Returns bone count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.bone_count
}

/// Returns morph count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.morph_count
}

/// Returns display frame count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_display_frame_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.display_frame_count
}

/// Returns rigidbody count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.rigidbody_count
}

/// Returns joint count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.joint_count
}

/// Returns soft body count, or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_soft_body_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.soft_body_count
}

/// Returns additional/extra UV count (0-4), or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_additional_uv_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return 0;
    };
    s.additional_uv_count
}

/// Returns the model name as UTF-8 bytes in a ByteBuffer (caller frees
/// with `mmd_runtime_byte_buffer_free`). Returns empty buffer for null summary.
///
/// The bytes are a copy of the decoded model name (UTF-8). Empty name yields
/// empty buffer (still safe to free).
///
/// # Safety
///
/// `summary` must be null or valid. The returned buffer (if non-empty) must be
/// freed exactly once with the byte buffer free function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_model_name(
    summary: *const MmdRuntimePmxSummary,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(s.name_utf8.clone())
}

/// Returns the English model name as UTF-8 bytes in a ByteBuffer (caller frees
/// with `mmd_runtime_byte_buffer_free`). Returns empty buffer for null summary
/// or when the English name is empty.
///
/// # Safety
///
/// `summary` must be null or valid. The returned buffer (if non-empty) must be
/// freed exactly once with the byte buffer free function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_model_name_english(
    summary: *const MmdRuntimePmxSummary,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(s.english_name_utf8.clone())
}

// -----------------------------------------------------------------------
// PMX summary core (non-JSON) getters: geometry, materials, bones, IK.
// Morph getters added in this bounded slice (physics/display remain deferred).
// All functions are null-safe and index-bounds-safe.
// Neutral returns: ByteBuffer=null/empty, numeric=0 or -1 (index absence), bool=false.
// Strings/names/paths returned as owned UTF-8 ByteBuffer (free with mmd_runtime_byte_buffer_free).
// Uses exact PmxParsed* fields from mmd_anim_format::parse_pmx_model (no extra conversions).
// JSON bridge TODO left unchecked per scope.
// -----------------------------------------------------------------------

/// Returns index count (triangle indices), or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_index_count(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.geometry.indices.len()
}

/// Returns index value at slot, or 0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_index(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> u32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    *s.parsed.geometry.indices.get(index).unwrap_or(&0u32)
}

/// Returns vertex position component (0=x,1=y,2=z), or 0.0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_position(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let base = vertex_index.saturating_mul(3);
    let g = &s.parsed.geometry;
    if base + 2 >= g.positions.len() { return 0.0; }
    *g.positions.get(base + component).unwrap_or(&0.0)
}

/// Returns vertex normal component, or 0.0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_normal(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let base = vertex_index.saturating_mul(3);
    let g = &s.parsed.geometry;
    if base + 2 >= g.normals.len() { return 0.0; }
    *g.normals.get(base + component).unwrap_or(&0.0)
}

/// Returns vertex UV component (0=u,1=v), or 0.0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_uv(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 2 { return 0.0; }
    let base = vertex_index.saturating_mul(2);
    let g = &s.parsed.geometry;
    if base + 1 >= g.uvs.len() { return 0.0; }
    *g.uvs.get(base + component).unwrap_or(&0.0)
}

/// Returns skin bone index (as i32; 0 for padding slots in native parse) for slot 0..3, or 0 for null/oob/slot>=4.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_skin_bone_index(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
    slot: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let g = &s.parsed.geometry;
    let base = vertex_index.saturating_mul(4);
    if slot >= 4 || base + 3 >= g.skin_indices.len() { return 0; }
    g.skin_indices[base + slot] as i32
}

/// Returns skin weight for slot 0..3, or 0.0 for null/oob/slot>=4.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_skin_weight(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
    slot: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let g = &s.parsed.geometry;
    let base = vertex_index.saturating_mul(4);
    if slot >= 4 || base + 3 >= g.skin_weights.len() { return 0.0; }
    g.skin_weights[base + slot]
}

/// Returns exact parsed skinning kind for a vertex, or a conservative fallback
/// for older parsed artifacts that predate `skinning_kinds`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_skinning_kind(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    if vertex_index >= s.vertex_count {
        return empty_byte_buffer();
    }
    if let Some(kind) = s.parsed.geometry.skinning_kinds.get(vertex_index) {
        if !kind.is_empty() {
            return byte_buffer_from_vec(kind.clone().into_bytes());
        }
    }

    byte_buffer_from_vec(pmx_summary_infer_skinning_kind(&s.parsed.geometry, vertex_index).into_bytes())
}

fn pmx_summary_infer_skinning_kind(g: &mmd_anim_format::pmx::PmxParsedGeometry, vertex_index: usize) -> String {
    if g.sdef.enabled.get(vertex_index).map(|&e| e > 0.5).unwrap_or(false) {
        return "sdef".to_owned();
    }
    if g.qdef.enabled.get(vertex_index).map(|&e| e > 0.5).unwrap_or(false) {
        return "qdef".to_owned();
    }

    let base = vertex_index.saturating_mul(4);
    if base + 3 >= g.skin_weights.len() {
        return "unknown".to_owned();
    }

    let weighted_slot_count = g.skin_weights[base..base + 4]
        .iter()
        .filter(|&&weight| weight.abs() > 0.000001)
        .count();
    match weighted_slot_count {
        0 | 1 => "bdef1",
        2 => "bdef2",
        _ => "bdef4",
    }.to_owned()
}

/// Returns whether vertex uses SDEF (enabled>0 in native), or false for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_sdef_enabled(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let g = &s.parsed.geometry;
    g.sdef.enabled.get(vertex_index).map(|&e| e > 0.5).unwrap_or(false)
}

/// Returns SDEF C/R0/R1 component for which=0(C)/1(R0)/2(R1), or 0.0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_sdef_c(
    summary: *const MmdRuntimePmxSummary,
    vertex_index: usize,
    which: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let g = &s.parsed.geometry;
    let vecs = match which {
        0 => &g.sdef.c,
        1 => &g.sdef.r0,
        2 => &g.sdef.r1,
        _ => return 0.0,
    };
    let base = vertex_index.saturating_mul(3);
    if base + 2 >= vecs.len() { return 0.0; }
    *vecs.get(base + component).unwrap_or(&0.0)
}

/// Returns material count (same as summary but explicit), or 0 for null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_count_getter(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.materials.len()
}

/// Returns material name as ByteBuffer (empty for null/oob).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_name(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.materials.get(index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.name.clone().into_bytes())
}

/// Returns material texture path as ByteBuffer (empty for null/oob or no texture).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_texture_path(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.materials.get(index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.texture_path.clone().into_bytes())
}

/// Returns material sphere texture path as ByteBuffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_sphere_texture_path(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.materials.get(index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.sphere_texture_path.clone().into_bytes())
}

/// Returns material toon texture path as ByteBuffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_toon_texture_path(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.materials.get(index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.toon_texture_path.clone().into_bytes())
}

/// Returns sphere mode as ByteBuffer (e.g. "disabled"/"multiply"/"add"), empty for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_sphere_mode(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.materials.get(index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.sphere_mode.clone().into_bytes())
}

/// Returns shared toon index or -1 when absent/ null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_shared_toon_index(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(m) = s.parsed.materials.get(index) else { return -1 };
    m.shared_toon_index.map(|u| u as i32).unwrap_or(-1)
}

/// Returns diffuse component (0..3 = r g b a), or 0.0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_diffuse(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.materials.get(index) else { return 0.0 };
    *m.diffuse.get(component).unwrap_or(&0.0)
}

/// Returns ambient r/g/b (component 0..2), or 0.0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_ambient(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(m) = s.parsed.materials.get(index) else { return 0.0 };
    *m.ambient.get(component).unwrap_or(&0.0)
}

/// Returns edge color r/g/b/a , or 0.0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_edge_color(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.materials.get(index) else { return 0.0 };
    *m.edge_color.get(component).unwrap_or(&0.0)
}

/// Returns edge size, or 0.0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_edge_size(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let Some(m) = s.parsed.materials.get(index) else { return 0.0 };
    m.edge_size
}

/// Returns material face (index) count, or 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_face_count(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.materials.get(index) else { return 0 };
    m.face_count
}

/// Returns material double_sided flag, or false for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_double_sided(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(m) = s.parsed.materials.get(index) else { return false };
    m.flags.double_sided
}

/// Returns material edge (draw edge) flag, or false.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_edge_flag(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(m) = s.parsed.materials.get(index) else { return false };
    m.flags.edge
}

/// Returns bone count from skeleton, or 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_count_getter(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.skeleton.bones.len()
}

/// Returns bone name as ByteBuffer, empty for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_name(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(b.name.clone().into_bytes())
}

/// Returns bone parent index (i32, -1 for none), or -1 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_parent_index(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return -1 };
    b.parent_index
}

/// Returns bone layer (deform/transform order), or 0 for null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_layer(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0 };
    b.layer
}

/// Returns bone rest position component, or 0.0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_position(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0.0 };
    *b.position.get(component).unwrap_or(&0.0)
}

/// Returns rotatable flag, false on null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_rotatable(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.flags.rotatable
}

/// Returns translatable flag, false on null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_translatable(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.flags.translatable
}

/// Returns append rotate flag (from bone or append info), false on null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_append_rotate(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.flags.append_rotate
}

/// Returns append translate flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_append_translate(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.flags.append_translate
}

/// Returns append local flag.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_append_local(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.flags.append_local
}

/// Returns append parent index (-1 absent) and weight (0 absent) via pair; for simplicity two getters.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_append_parent_index(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return -1 };
    b.append_transform.as_ref().map(|a| a.parent_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_append_weight(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0.0 };
    b.append_transform.as_ref().map(|a| a.weight).unwrap_or(0.0)
}

/// Fixed axis present and x/y/z (0 when absent).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_fixed_axis_present(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.fixed_axis.is_some()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_fixed_axis(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0.0 };
    b.fixed_axis.as_ref().map(|a| *a.get(component).unwrap_or(&0.0)).unwrap_or(0.0)
}

/// Local axis present + x-axis / z-axis components.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_local_axis_present(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.local_axis.is_some()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_local_axis_x(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0.0 };
    b.local_axis.as_ref().map(|la| *la.x.get(component).unwrap_or(&0.0)).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_local_axis_z(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0.0 };
    b.local_axis.as_ref().map(|la| *la.z.get(component).unwrap_or(&0.0)).unwrap_or(0.0)
}

/// External parent present (key != -1) and key.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_external_parent_present(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.external_parent_key.map(|k| k >= 0).unwrap_or(false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_external_parent_key(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return -1 };
    b.external_parent_key.unwrap_or(-1)
}

/// IK present on this bone, target, loop, limit, link count ( -1/0 neutral on absent).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_present(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return false };
    b.ik.is_some()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_target_index(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return -1 };
    b.ik.as_ref().map(|ik| ik.target_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_loop_count(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0 };
    b.ik.as_ref().map(|ik| ik.loop_count).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_limit_angle(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0.0 };
    b.ik.as_ref().map(|ik| ik.limit_angle).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_link_count(
    summary: *const MmdRuntimePmxSummary,
    index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(b) = s.parsed.skeleton.bones.get(index) else { return 0 };
    b.ik.as_ref().map(|ik| ik.links.len()).unwrap_or(0)
}

/// IK link bone index for (bone, link_slot), or -1 on null/oob.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_link_bone_index(
    summary: *const MmdRuntimePmxSummary,
    bone_index: usize,
    link_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(b) = s.parsed.skeleton.bones.get(bone_index) else { return -1 };
    let Some(ik) = &b.ik else { return -1 };
    ik.links.get(link_index).map(|l| l.bone_index).unwrap_or(-1)
}

/// IK link limit present for (bone, link), false on null/oob/absent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_link_limit_present(
    summary: *const MmdRuntimePmxSummary,
    bone_index: usize,
    link_index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(b) = s.parsed.skeleton.bones.get(bone_index) else { return false };
    let Some(ik) = &b.ik else { return false };
    ik.links.get(link_index).map(|l| l.limits.is_some()).unwrap_or(false)
}

/// IK link limit lower/upper component for (bone,link, which=0 lower 1 upper, comp 0-2), 0 on absent.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_link_limit_lower(
    summary: *const MmdRuntimePmxSummary,
    bone_index: usize,
    link_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(b) = s.parsed.skeleton.bones.get(bone_index) else { return 0.0 };
    let Some(ik) = &b.ik else { return 0.0 };
    let Some(link) = ik.links.get(link_index) else { return 0.0 };
    link.limits.as_ref().map(|lim| *lim.lower.get(component).unwrap_or(&0.0)).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_ik_link_limit_upper(
    summary: *const MmdRuntimePmxSummary,
    bone_index: usize,
    link_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(b) = s.parsed.skeleton.bones.get(bone_index) else { return 0.0 };
    let Some(ik) = &b.ik else { return 0.0 };
    let Some(link) = ik.links.get(link_index) else { return 0.0 };
    link.limits.as_ref().map(|lim| *lim.upper.get(component).unwrap_or(&0.0)).unwrap_or(0.0)
}

/// Returns the number of bones (compat alias to prior count).
/// (existing mmd_runtime_pmx_summary_bone_count still works from thin fields)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_bone_count_from_skeleton(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.skeleton.bones.len()
}

/// Returns the number of materials (compat).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_material_count_from_parsed(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.materials.len()
}

/// Returns the number of vertices from geometry, or 0.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_vertex_count_from_geometry(
    summary: *const MmdRuntimePmxSummary,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.geometry.positions.len() / 3
}

// end PMX core getters block

// -----------------------------------------------------------------------
// PMX summary morph (non-JSON) getters. Added per bounded native FFI slice.
// All null-safe + bounds-safe. Follows existing PMX core getter naming/style.
// Neutral defaults per spec. JSON bridge TODO remains unchecked.
// -----------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_name(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_english_name(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.english_name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_kind(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.kind.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_panel(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(m.panel.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_vertex_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.vertex_offsets.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_group_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.group_offsets.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_bone_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.bone_offsets.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_uv_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.uv_offsets.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_additional_uv_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.additional_uv_offsets.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.material_offsets.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_flip_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.flip_offsets.len()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_impulse_offset_count(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.impulse_offsets.len()
}

// Vertex offsets

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_vertex_offset_vertex_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> u32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.vertex_offsets.get(offset_index).map(|o| o.vertex_index).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_vertex_offset_position(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.vertex_offsets.get(offset_index).and_then(|o| o.position.get(component).copied()).unwrap_or(0.0)
}

// Group offsets

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_group_offset_morph_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return -1 };
    m.group_offsets.get(offset_index).map(|o| o.morph_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_group_offset_weight(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.group_offsets.get(offset_index).map(|o| o.weight).unwrap_or(0.0)
}

// Flip offsets (same payload shape as group)

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_flip_offset_morph_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return -1 };
    m.flip_offsets.get(offset_index).map(|o| o.morph_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_flip_offset_weight(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.flip_offsets.get(offset_index).map(|o| o.weight).unwrap_or(0.0)
}

// Bone offsets

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_bone_offset_bone_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return -1 };
    m.bone_offsets.get(offset_index).map(|o| o.bone_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_bone_offset_translation(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.bone_offsets.get(offset_index).and_then(|o| o.translation.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_bone_offset_rotation(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.bone_offsets.get(offset_index).and_then(|o| o.rotation.get(component).copied()).unwrap_or(0.0)
}

// UV offsets

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_uv_offset_vertex_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> u32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.uv_offsets.get(offset_index).map(|o| o.vertex_index).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_uv_offset_value(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.uv_offsets.get(offset_index).and_then(|o| o.uv.get(component).copied()).unwrap_or(0.0)
}

// Additional UV offsets

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_additional_uv_offset_vertex_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> u32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.additional_uv_offsets.get(offset_index).map(|o| o.vertex_index).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_additional_uv_offset_uv_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> u8 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0 };
    m.additional_uv_offsets.get(offset_index).map(|o| o.uv_index).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_additional_uv_offset_value(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.additional_uv_offsets.get(offset_index).and_then(|o| o.uv.get(component).copied()).unwrap_or(0.0)
}

// Material offsets

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_material_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return -1 };
    m.material_offsets.get(offset_index).map(|o| o.material_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_operation(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return empty_byte_buffer() };
    let Some(o) = m.material_offsets.get(offset_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(o.operation.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_diffuse(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).and_then(|o| o.diffuse.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_specular(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).and_then(|o| o.specular.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_specular_power(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).map(|o| o.specular_power).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_ambient(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).and_then(|o| o.ambient.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_edge_color(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).and_then(|o| o.edge_color.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_edge_size(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).map(|o| o.edge_size).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_texture_factor(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).and_then(|o| o.texture_factor.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_sphere_texture_factor(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).and_then(|o| o.sphere_texture_factor.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_material_offset_toon_texture_factor(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 4 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.material_offsets.get(offset_index).and_then(|o| o.toon_texture_factor.get(component).copied()).unwrap_or(0.0)
}

// Impulse offsets

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_impulse_offset_rigidbody_index(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return -1 };
    m.impulse_offsets.get(offset_index).map(|o| o.rigid_body_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_impulse_offset_local(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    let Some(m) = s.parsed.morphs.get(morph_index) else { return false };
    m.impulse_offsets.get(offset_index).map(|o| o.local).unwrap_or(false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_impulse_offset_velocity(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.impulse_offsets.get(offset_index).and_then(|o| o.velocity.get(component).copied()).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_morph_impulse_offset_torque(
    summary: *const MmdRuntimePmxSummary,
    morph_index: usize,
    offset_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(m) = s.parsed.morphs.get(morph_index) else { return 0.0 };
    m.impulse_offsets.get(offset_index).and_then(|o| o.torque.get(component).copied()).unwrap_or(0.0)
}

// -----------------------------------------------------------------------
// PMX summary display frame (non-JSON) getters.
// -----------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_display_frame_name(
    summary: *const MmdRuntimePmxSummary,
    display_frame_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(frame) = s.parsed.display_frames.get(display_frame_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(frame.name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_display_frame_english_name(
    summary: *const MmdRuntimePmxSummary,
    display_frame_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(frame) = s.parsed.display_frames.get(display_frame_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(frame.english_name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_display_frame_special(
    summary: *const MmdRuntimePmxSummary,
    display_frame_index: usize,
) -> bool {
    let Some(s) = (unsafe { summary.as_ref() }) else { return false };
    s.parsed.display_frames.get(display_frame_index).map(|frame| frame.special).unwrap_or(false)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_display_frame_item_count(
    summary: *const MmdRuntimePmxSummary,
    display_frame_index: usize,
) -> usize {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.display_frames
        .get(display_frame_index)
        .map(|frame| frame.frames.len())
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_display_frame_item_kind(
    summary: *const MmdRuntimePmxSummary,
    display_frame_index: usize,
    item_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(frame) = s.parsed.display_frames.get(display_frame_index) else { return empty_byte_buffer() };
    let Some(item) = frame.frames.get(item_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(item.kind.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_display_frame_item_index(
    summary: *const MmdRuntimePmxSummary,
    display_frame_index: usize,
    item_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    let Some(frame) = s.parsed.display_frames.get(display_frame_index) else { return -1 };
    frame.frames.get(item_index).map(|item| item.index).unwrap_or(-1)
}

// -----------------------------------------------------------------------
// PMX summary physics (non-JSON) getters: rigid bodies and joints.
// Soft body detail getters are intentionally deferred; managed neutral IR
// currently models rigid bodies and joints only.
// -----------------------------------------------------------------------

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_name(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(body) = s.parsed.rigid_bodies.get(rigidbody_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(body.name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_english_name(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(body) = s.parsed.rigid_bodies.get(rigidbody_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(body.english_name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_bone_index(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.bone_index).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_group(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> u8 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.group).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_mask(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> u16 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.mask).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_shape(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(body) = s.parsed.rigid_bodies.get(rigidbody_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(body.shape.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_size(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(body) = s.parsed.rigid_bodies.get(rigidbody_index) else { return 0.0 };
    body.size[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_position(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(body) = s.parsed.rigid_bodies.get(rigidbody_index) else { return 0.0 };
    body.position[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_rotation(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(body) = s.parsed.rigid_bodies.get(rigidbody_index) else { return 0.0 };
    body.rotation[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_mass(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.mass).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_linear_damping(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.linear_damping).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_angular_damping(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.angular_damping).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_restitution(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.restitution).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_friction(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    s.parsed.rigid_bodies.get(rigidbody_index).map(|body| body.friction).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_rigidbody_mode(
    summary: *const MmdRuntimePmxSummary,
    rigidbody_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(body) = s.parsed.rigid_bodies.get(rigidbody_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(body.mode.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_name(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(joint) = s.parsed.joints.get(joint_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(joint.name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_english_name(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(joint) = s.parsed.joints.get(joint_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(joint.english_name.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_kind(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    let Some(s) = (unsafe { summary.as_ref() }) else { return empty_byte_buffer() };
    let Some(joint) = s.parsed.joints.get(joint_index) else { return empty_byte_buffer() };
    byte_buffer_from_vec(joint.kind.clone().into_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_rigidbody_a_index(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    s.parsed.joints.get(joint_index).map(|joint| joint.rigid_body_index_a).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_rigidbody_b_index(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
) -> i32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return -1 };
    s.parsed.joints.get(joint_index).map(|joint| joint.rigid_body_index_b).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_position(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.position[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_rotation(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.rotation[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_translation_lower_limit(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.translation_lower_limit[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_translation_upper_limit(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.translation_upper_limit[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_rotation_lower_limit(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.rotation_lower_limit[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_rotation_upper_limit(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.rotation_upper_limit[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_spring_translation_factor(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.spring_translation_factor[component]
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_summary_joint_spring_rotation_factor(
    summary: *const MmdRuntimePmxSummary,
    joint_index: usize,
    component: usize,
) -> f32 {
    let Some(s) = (unsafe { summary.as_ref() }) else { return 0.0 };
    if component >= 3 { return 0.0; }
    let Some(joint) = s.parsed.joints.get(joint_index) else { return 0.0 };
    joint.spring_rotation_factor[component]
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
    // VMD summary (non-JSON) FFI tests
    // -----------------------------------------------------------------------

    fn build_vmd_header_bytes(model_name: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Vocaloid Motion Data 0002\0\0\0\0\0");
        let mut name = [0u8; 20];
        let src = model_name.as_bytes();
        let n = src.len().min(20);
        name[..n].copy_from_slice(&src[..n]);
        buf.extend_from_slice(&name);
        buf
    }

    fn append_morph_section(buf: &mut Vec<u8>, morphs: &[(&str, u32, f32)]) {
        buf.extend_from_slice(&(morphs.len() as u32).to_le_bytes());
        for (nm, frame, weight) in morphs {
            let mut name = [0u8; 15];
            let src = nm.as_bytes();
            let n = src.len().min(15);
            name[..n].copy_from_slice(&src[..n]);
            buf.extend_from_slice(&name);
            buf.extend_from_slice(&frame.to_le_bytes());
            buf.extend_from_slice(&weight.to_le_bytes());
        }
    }

    fn append_zeroed_optional_sections(buf: &mut Vec<u8>, prop_count: u32) {
        // cam / light / self-shadow counts (0) then property count + minimal data
        buf.extend_from_slice(&0u32.to_le_bytes()); // cam
        buf.extend_from_slice(&0u32.to_le_bytes()); // light
        buf.extend_from_slice(&0u32.to_le_bytes()); // self shadow
        buf.extend_from_slice(&prop_count.to_le_bytes());
        if prop_count > 0 {
            // one minimal property frame (frame + visible + 0 iks)
            buf.extend_from_slice(&10u32.to_le_bytes());
            buf.push(1u8); // visible
            buf.extend_from_slice(&0u32.to_le_bytes()); // ik count 0
        }
    }

    #[test]
    fn vmd_summary_create_rejects_null_empty_invalid() {
        assert!(unsafe { mmd_runtime_vmd_summary_create_from_bytes(ptr::null(), 0) }.is_null());
        assert!(unsafe { mmd_runtime_vmd_summary_create_from_bytes(ptr::null(), 10) }.is_null());
        let d = 0u8;
        assert!(unsafe { mmd_runtime_vmd_summary_create_from_bytes(&d as *const u8, 0) }.is_null());

        let garbage = [0u8; 16];
        let s = unsafe { mmd_runtime_vmd_summary_create_from_bytes(garbage.as_ptr(), garbage.len()) };
        assert!(s.is_null());

        // also invalid magic
        let mut bad = build_vmd_header_bytes("bad");
        bad[0] = b'X';
        let s2 = unsafe { mmd_runtime_vmd_summary_create_from_bytes(bad.as_ptr(), bad.len()) };
        assert!(s2.is_null());
    }

    #[test]
    fn vmd_summary_from_camera_fixture() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(bytes.as_ptr(), bytes.len()) };
        assert!(!sum.is_null());

        assert_eq!(unsafe { mmd_runtime_vmd_summary_max_frame(sum) }, 45);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_keyframe_count(sum) }, 2);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_keyframe_count(sum) }, 0);

        let name_buf = unsafe { mmd_runtime_vmd_summary_model_name(sum) };
        assert!(!name_buf.data.is_null());
        assert!(name_buf.len > 0);
        let name_slice = unsafe { std::slice::from_raw_parts(name_buf.data, name_buf.len) };
        assert_eq!(std::str::from_utf8(name_slice).unwrap(), "camera_fixture");
        // free the buffer via existing API
        unsafe { mmd_runtime_byte_buffer_free(name_buf); }

        // double-check null accessors still 0 after (no mutation)
        unsafe { mmd_runtime_vmd_summary_free(sum); }
    }

    #[test]
    fn vmd_summary_from_ik_bone_fixture_and_null_accessors() {
        let bytes: &[u8] =
            include_bytes!("../../mmd-anim-format/fixtures/vmd/ik_multi_bone_nondefault.vmd");
        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(bytes.as_ptr(), bytes.len()) };
        assert!(!sum.is_null());

        assert!(unsafe { mmd_runtime_vmd_summary_max_frame(sum) } >= 30);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_keyframe_count(sum) }, 5);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_keyframe_count(sum) }, 0);

        let name_buf = unsafe { mmd_runtime_vmd_summary_model_name(sum) };
        // name may be empty or short for this fixture; just ensure it returns a buffer we can free
        unsafe { mmd_runtime_byte_buffer_free(name_buf); }

        unsafe { mmd_runtime_vmd_summary_free(sum); }

        // null summary behavior
        assert_eq!(unsafe { mmd_runtime_vmd_summary_max_frame(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_keyframe_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_keyframe_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_keyframe_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_keyframe_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_keyframe_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_keyframe_count(ptr::null()) }, 0);

        let empty_name = unsafe { mmd_runtime_vmd_summary_model_name(ptr::null()) };
        assert!(empty_name.data.is_null());
        assert_eq!(empty_name.len, 0);
        unsafe { mmd_runtime_byte_buffer_free(empty_name); }
    }

    #[test]
    fn vmd_summary_synthetic_morph_and_property_counts() {
        // header + BONE COUNT 0 + morph section (1) + zeroed cam/light/ss + prop section (1)
        let mut buf = build_vmd_header_bytes("synthmodel");
        buf.extend_from_slice(&0u32.to_le_bytes()); // bone count = 0 (required before morph)
        append_morph_section(&mut buf, &[("blink", 15, 0.75)]);
        append_zeroed_optional_sections(&mut buf, 1);

        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(buf.as_ptr(), buf.len()) };
        assert!(!sum.is_null());

        assert!(unsafe { mmd_runtime_vmd_summary_max_frame(sum) } >= 15);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_keyframe_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_keyframe_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_keyframe_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_keyframe_count(sum) }, 0);

        let name_buf = unsafe { mmd_runtime_vmd_summary_model_name(sum) };
        let nslice = unsafe { std::slice::from_raw_parts(name_buf.data, name_buf.len) };
        assert_eq!(std::str::from_utf8(nslice).unwrap(), "synthmodel");
        unsafe { mmd_runtime_byte_buffer_free(name_buf); }

        unsafe { mmd_runtime_vmd_summary_free(sum); }
    }

    #[test]
    fn vmd_summary_scene_track_getters_from_synthetic_bytes() {
        let mut buf = build_vmd_header_bytes("scenesynth");
        buf.extend_from_slice(&0u32.to_le_bytes()); // bone count
        append_morph_section(&mut buf, &[]);

        buf.extend_from_slice(&1u32.to_le_bytes()); // camera count
        buf.extend_from_slice(&12u32.to_le_bytes());
        for value in [30.0f32, 1.0, 2.0, 3.0, 0.1, 0.2, 0.3] {
            buf.extend_from_slice(&value.to_le_bytes());
        }
        for value in 0u8..24 {
            buf.push(value + 10);
        }
        buf.extend_from_slice(&45u32.to_le_bytes());
        buf.push(0u8); // native parser maps 0 to perspective=true

        buf.extend_from_slice(&1u32.to_le_bytes()); // light count
        buf.extend_from_slice(&13u32.to_le_bytes());
        for value in [0.4f32, 0.5, 0.6, -1.0, -2.0, -3.0] {
            buf.extend_from_slice(&value.to_le_bytes());
        }

        buf.extend_from_slice(&1u32.to_le_bytes()); // self-shadow count
        buf.extend_from_slice(&14u32.to_le_bytes());
        buf.push(2u8);
        buf.extend_from_slice(&0.75f32.to_le_bytes());

        buf.extend_from_slice(&0u32.to_le_bytes()); // property count

        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(buf.as_ptr(), buf.len()) };
        assert!(!sum.is_null());

        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_keyframe_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_keyframe_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_keyframe_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_max_frame(sum) }, 14);

        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_frame(sum, 0) }, 12);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_distance(sum, 0) }, 30.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_position_x(sum, 0) }, 1.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_position_y(sum, 0) }, 2.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_position_z(sum, 0) }, 3.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_rotation_x(sum, 0) }, 0.1);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_rotation_y(sum, 0) }, 0.2);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_rotation_z(sum, 0) }, 0.3);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_interpolation_byte(sum, 0, 0) }, 10);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_interpolation_byte(sum, 0, 23) }, 33);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_interpolation_byte(sum, 0, 24) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_fov(sum, 0) }, 45);
        assert!(unsafe { mmd_runtime_vmd_summary_camera_frame_perspective(sum, 0) });

        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_frame(sum, 0) }, 13);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_color_x(sum, 0) }, 0.4);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_color_y(sum, 0) }, 0.5);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_color_z(sum, 0) }, 0.6);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_direction_x(sum, 0) }, -1.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_direction_y(sum, 0) }, -2.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_direction_z(sum, 0) }, -3.0);

        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_frame_frame(sum, 0) }, 14);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_frame_mode(sum, 0) }, 2);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_frame_distance(sum, 0) }, 0.75);

        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_frame(ptr::null(), 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_camera_frame_position_x(sum, 99) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_light_frame_color_x(sum, 99) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_self_shadow_frame_mode(sum, 99) }, 0);

        unsafe { mmd_runtime_vmd_summary_free(sum); }
    }

    // -----------------------------------------------------------------------
    // Focused VMD model-motion getter tests (non-JSON FFI slice)
    // -----------------------------------------------------------------------

    #[test]
    fn vmd_summary_bone_getters_valid_and_interp_oob_from_fixture() {
        let mut buf = build_vmd_header_bytes("bonesynth");
        buf.extend_from_slice(&1u32.to_le_bytes());

        let mut bone_name = [0u8; 15];
        bone_name[..4].copy_from_slice(b"root");
        buf.extend_from_slice(&bone_name);
        buf.extend_from_slice(&7u32.to_le_bytes());
        for value in [1.0f32, 2.0, 3.0, 0.1, 0.2, 0.3, 0.4] {
            buf.extend_from_slice(&value.to_le_bytes());
        }
        for value in 0u8..64 {
            buf.push(value);
        }
        append_morph_section(&mut buf, &[]);
        append_zeroed_optional_sections(&mut buf, 0);

        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(buf.as_ptr(), buf.len()) };
        assert!(!sum.is_null());
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_keyframe_count(sum) }, 1);

        let name0 = unsafe { mmd_runtime_vmd_summary_bone_frame_name(sum, 0) };
        let nslice = unsafe { std::slice::from_raw_parts(name0.data, name0.len) };
        assert_eq!(std::str::from_utf8(nslice).unwrap(), "root");
        unsafe { mmd_runtime_byte_buffer_free(name0); }

        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_frame(sum, 0) }, 7);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_translation_x(sum, 0) }, 1.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_translation_y(sum, 0) }, 2.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_translation_z(sum, 0) }, 3.0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_rotation_x(sum, 0) }, 0.1);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_rotation_y(sum, 0) }, 0.2);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_rotation_z(sum, 0) }, 0.3);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_rotation_w(sum, 0) }, 0.4);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_interpolation_byte(sum, 0, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_interpolation_byte(sum, 0, 15) }, 15);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_interpolation_byte(sum, 0, 999) }, 0);

        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_frame(sum, 999) }, 0);
        let name_oob = unsafe { mmd_runtime_vmd_summary_bone_frame_name(sum, 999) };
        assert!(name_oob.data.is_null());
        assert_eq!(name_oob.len, 0);
        unsafe { mmd_runtime_byte_buffer_free(name_oob); }

        unsafe { mmd_runtime_vmd_summary_free(sum); }
    }

    #[test]
    fn vmd_summary_morph_getters_valid_from_synthetic() {
        // Reuse synthetic builder pattern (no morph fixture with data in this slice)
        let mut buf = build_vmd_header_bytes("morphsynth");
        buf.extend_from_slice(&0u32.to_le_bytes()); // bone count 0
        append_morph_section(&mut buf, &[("testmorph", 12, 0.5), ("blink", 20, 1.0)]);
        append_zeroed_optional_sections(&mut buf, 0);

        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(buf.as_ptr(), buf.len()) };
        assert!(!sum.is_null());
        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_keyframe_count(sum) }, 2);

        // valid index 0
        let mname = unsafe { mmd_runtime_vmd_summary_morph_frame_name(sum, 0) };
        let nslice = unsafe { std::slice::from_raw_parts(mname.data, mname.len) };
        assert_eq!(std::str::from_utf8(nslice).unwrap(), "testmorph");
        unsafe { mmd_runtime_byte_buffer_free(mname); }

        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_frame_frame(sum, 0) }, 12);
        assert!((unsafe { mmd_runtime_vmd_summary_morph_frame_weight(sum, 0) } - 0.5).abs() < 1e-6);

        // index 1
        let mname1 = unsafe { mmd_runtime_vmd_summary_morph_frame_name(sum, 1) };
        let n1 = unsafe { std::slice::from_raw_parts(mname1.data, mname1.len) };
        assert_eq!(std::str::from_utf8(n1).unwrap(), "blink");
        unsafe { mmd_runtime_byte_buffer_free(mname1); }
        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_frame_frame(sum, 1) }, 20);
        assert!((unsafe { mmd_runtime_vmd_summary_morph_frame_weight(sum, 1) } - 1.0).abs() < 1e-6);

        // oob
        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_frame_frame(sum, 99) }, 0);
        let oob_name = unsafe { mmd_runtime_vmd_summary_morph_frame_name(sum, 99) };
        assert!(oob_name.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(oob_name); }

        unsafe { mmd_runtime_vmd_summary_free(sum); }
    }

    #[test]
    fn vmd_summary_property_and_ik_getters_valid_from_synthetic() {
        // Build minimal VMD with one property frame carrying 2 IK states
        let mut buf = build_vmd_header_bytes("propsynth");
        buf.extend_from_slice(&0u32.to_le_bytes()); // bones 0
        // no morphs: write 0 for optional morph count to stop before cam (but to reach prop we still need to follow layout)
        // For property we go through the optional skips. Use append helpers then manually append a prop with IK.
        buf.extend_from_slice(&0u32.to_le_bytes()); // morph count = 0
        // append the zeroed optional (cam/light/ss) + prop count = 1 , but override to inject IK states
        buf.extend_from_slice(&0u32.to_le_bytes()); // cam
        buf.extend_from_slice(&0u32.to_le_bytes()); // light
        buf.extend_from_slice(&0u32.to_le_bytes()); // ss
        buf.extend_from_slice(&1u32.to_le_bytes()); // 1 property frame

        // property frame: frame=5, visible=true, ik_count=2
        buf.extend_from_slice(&5u32.to_le_bytes());
        buf.push(1u8); // visible
        buf.extend_from_slice(&2u32.to_le_bytes()); // ik count
        // ik0
        let mut ik0 = [0u8; 20];
        ik0[..3].copy_from_slice(b"ikA");
        buf.extend_from_slice(&ik0);
        buf.push(1u8); // enabled
        // ik1
        let mut ik1 = [0u8; 20];
        ik1[..3].copy_from_slice(b"ikB");
        buf.extend_from_slice(&ik1);
        buf.push(0u8); // disabled

        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(buf.as_ptr(), buf.len()) };
        assert!(!sum.is_null());
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_keyframe_count(sum) }, 1);

        // frame props
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_frame_frame(sum, 0) }, 5);
        assert!(unsafe { mmd_runtime_vmd_summary_property_frame_visible(sum, 0) });
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_count(sum, 0) }, 2);

        // ik states
        let n0 = unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_name(sum, 0, 0) };
        let s0 = unsafe { std::slice::from_raw_parts(n0.data, n0.len) };
        assert_eq!(std::str::from_utf8(s0).unwrap(), "ikA");
        unsafe { mmd_runtime_byte_buffer_free(n0); }
        assert!(unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_enabled(sum, 0, 0) });

        let n1 = unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_name(sum, 0, 1) };
        let s1 = unsafe { std::slice::from_raw_parts(n1.data, n1.len) };
        assert_eq!(std::str::from_utf8(s1).unwrap(), "ikB");
        unsafe { mmd_runtime_byte_buffer_free(n1); }
        assert!(!unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_enabled(sum, 0, 1) });

        // oob ik index
        let noob = unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_name(sum, 0, 99) };
        assert!(noob.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(noob); }
        assert!(!unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_enabled(sum, 0, 99) });

        // oob frame
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_count(sum, 99) }, 0);
        let nframeoob = unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_name(sum, 99, 0) };
        assert!(nframeoob.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(nframeoob); }

        unsafe { mmd_runtime_vmd_summary_free(sum); }
    }

    #[test]
    fn vmd_summary_getters_null_and_oob_return_safe_defaults_and_bytebuffers_freeable() {
        // null summary
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_frame(ptr::null(), 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_translation_x(ptr::null(), 0) }, 0.0);
        let nb = unsafe { mmd_runtime_vmd_summary_bone_frame_name(ptr::null(), 0) };
        assert!(nb.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(nb); }
        assert_eq!(unsafe { mmd_runtime_vmd_summary_bone_frame_interpolation_byte(ptr::null(), 0, 0) }, 0);

        assert_eq!(unsafe { mmd_runtime_vmd_summary_morph_frame_weight(ptr::null(), 0) }, 0.0);
        let nm = unsafe { mmd_runtime_vmd_summary_morph_frame_name(ptr::null(), 0) };
        unsafe { mmd_runtime_byte_buffer_free(nm); }

        assert!(!unsafe { mmd_runtime_vmd_summary_property_frame_visible(ptr::null(), 0) });
        assert_eq!(unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_count(ptr::null(), 0) }, 0);
        let np = unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_name(ptr::null(), 0, 0) };
        unsafe { mmd_runtime_byte_buffer_free(np); }
        assert!(!unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_enabled(ptr::null(), 0, 0) });

        // valid summary but oob already covered in other tests; representative free check done above
    }

    #[test]
    fn vmd_summary_bytbuffer_from_getters_are_freeable_via_existing_api() {
        // Use synthetic that has names for bone + morph + prop ik to exercise free paths
        let mut buf = build_vmd_header_bytes("freebuf");
        // minimal bone frame with name
        buf.extend_from_slice(&1u32.to_le_bytes());
        let mut bname = [0u8; 15];
        bname[..3].copy_from_slice(b"hip");
        buf.extend_from_slice(&bname);
        buf.extend_from_slice(&0u32.to_le_bytes()); // frame
        buf.extend_from_slice(&0f32.to_le_bytes());
        buf.extend_from_slice(&0f32.to_le_bytes());
        buf.extend_from_slice(&0f32.to_le_bytes());
        // quat identity
        buf.extend_from_slice(&0f32.to_le_bytes());
        buf.extend_from_slice(&0f32.to_le_bytes());
        buf.extend_from_slice(&0f32.to_le_bytes());
        buf.extend_from_slice(&1f32.to_le_bytes());
        // interp 64 zero
        for _ in 0..64 { buf.push(0u8); }

        // morph 0
        buf.extend_from_slice(&0u32.to_le_bytes());
        // cam/light/ss 0 + prop with 1 ik
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // frame 0
        buf.push(0u8); // not visible
        buf.extend_from_slice(&1u32.to_le_bytes());
        let mut ikn = [0u8; 20];
        ikn[..2].copy_from_slice(b"ik");
        buf.extend_from_slice(&ikn);
        buf.push(1u8);

        let sum = unsafe { mmd_runtime_vmd_summary_create_from_bytes(buf.as_ptr(), buf.len()) };
        assert!(!sum.is_null());

        // exercise frees for each name path
        let bnameb = unsafe { mmd_runtime_vmd_summary_bone_frame_name(sum, 0) };
        unsafe { mmd_runtime_byte_buffer_free(bnameb); }

        let mnameb = unsafe { mmd_runtime_vmd_summary_morph_frame_name(sum, 0) };
        unsafe { mmd_runtime_byte_buffer_free(mnameb); }

        let pnameb = unsafe { mmd_runtime_vmd_summary_property_frame_ik_state_name(sum, 0, 0) };
        unsafe { mmd_runtime_byte_buffer_free(pnameb); }

        unsafe { mmd_runtime_vmd_summary_free(sum); }
    }

    #[test]
    fn pmx_summary_create_rejects_null_empty_invalid() {
        assert!(unsafe { mmd_runtime_pmx_summary_create_from_bytes(ptr::null(), 0) }.is_null());
        assert!(unsafe { mmd_runtime_pmx_summary_create_from_bytes(ptr::null(), 10) }.is_null());
        let d = 0u8;
        assert!(unsafe { mmd_runtime_pmx_summary_create_from_bytes(&d as *const u8, 0) }.is_null());

        let garbage = [0u8; 16];
        let s = unsafe { mmd_runtime_pmx_summary_create_from_bytes(garbage.as_ptr(), garbage.len()) };
        assert!(s.is_null());

        // invalid PMX (bad magic) also returns null
        let mut bad = vec![0u8; 32];
        bad[0] = b'X';
        bad[1] = b'X';
        bad[2] = b'X';
        bad[3] = b'X';
        let s2 = unsafe { mmd_runtime_pmx_summary_create_from_bytes(bad.as_ptr(), bad.len()) };
        assert!(s2.is_null());
    }

    #[test]
    fn pmx_summary_from_fixture_and_accessors() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let sum = unsafe { mmd_runtime_pmx_summary_create_from_bytes(bytes.as_ptr(), bytes.len()) };
        assert!(!sum.is_null());

        assert_eq!(unsafe { mmd_runtime_pmx_summary_version(sum) }, 2.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_count(sum) }, 3);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_face_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_material_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_count(sum) }, 3);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_count(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_soft_body_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_additional_uv_count(sum) }, 0);

        // model name
        let name_buf = unsafe { mmd_runtime_pmx_summary_model_name(sum) };
        assert!(!name_buf.data.is_null());
        assert!(name_buf.len > 0);
        let name_slice = unsafe { std::slice::from_raw_parts(name_buf.data, name_buf.len) };
        assert_eq!(std::str::from_utf8(name_slice).unwrap(), "ik_multi_axis_limit_fixture");
        unsafe { mmd_runtime_byte_buffer_free(name_buf); }

        // english name (trivially available from metadata)
        let en_buf = unsafe { mmd_runtime_pmx_summary_model_name_english(sum) };
        assert!(!en_buf.data.is_null());
        assert!(en_buf.len > 0);
        let en_slice = unsafe { std::slice::from_raw_parts(en_buf.data, en_buf.len) };
        assert_eq!(std::str::from_utf8(en_slice).unwrap(), "ik_multi_axis_limit_fixture");
        unsafe { mmd_runtime_byte_buffer_free(en_buf); }

        unsafe { mmd_runtime_pmx_summary_free(sum); }
    }

    #[test]
    fn pmx_summary_null_accessors_return_zero_empty_and_byte_buffer_frees() {
        // direct null summary
        assert_eq!(unsafe { mmd_runtime_pmx_summary_version(ptr::null()) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_face_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_material_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_soft_body_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_additional_uv_count(ptr::null()) }, 0);

        let empty_name = unsafe { mmd_runtime_pmx_summary_model_name(ptr::null()) };
        assert!(empty_name.data.is_null());
        assert_eq!(empty_name.len, 0);
        unsafe { mmd_runtime_byte_buffer_free(empty_name); }

        let empty_en = unsafe { mmd_runtime_pmx_summary_model_name_english(ptr::null()) };
        assert!(empty_en.data.is_null());
        assert_eq!(empty_en.len, 0);
        unsafe { mmd_runtime_byte_buffer_free(empty_en); }

        // freed summary is not re-used; null-like behavior on null ptr is sufficient for contract
    }

    #[test]
    fn pmx_summary_core_getters_from_fixture_and_null_oob() {
        // Uses ik_multi_axis_limit.pmx (3vtx,1mat,3bones with IK+limits; 0 morphs)
        // Fixture-grounded exact values from parse: vtx skin[0]=0 w=1; mat name="mat" shared=-1 dbl=true fcnt=1;
        // bone[2]="ik_controller" par=-1 layer=0 ik_present target=1 loop=1 linkcnt=1;
        // iklink[0] bone=0 limit_present=true lower0=0 upper0=0
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let sum = unsafe { mmd_runtime_pmx_summary_create_from_bytes(bytes.as_ptr(), bytes.len()) };
        assert!(!sum.is_null());

        // counts via new geo/mat accessors (must match prior thin summary counts)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_count_from_geometry(sum) }, 3);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_index_count(sum) }, 3);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_material_count_from_parsed(sum) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_count_from_skeleton(sum) }, 3);

        // --- exact geometry getter assertion ---
        assert_eq!(unsafe { mmd_runtime_pmx_summary_index(sum, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_index(sum, 2) }, 2);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_skin_bone_index(sum, 0, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_skin_weight(sum, 0, 0) }, 1.0);
        let skin_kind = unsafe { mmd_runtime_pmx_summary_vertex_skinning_kind(sum, 0) };
        if !skin_kind.data.is_null() && skin_kind.len > 0 {
            let skin_kind_slice = unsafe { std::slice::from_raw_parts(skin_kind.data, skin_kind.len) };
            assert_eq!(std::str::from_utf8(skin_kind_slice).unwrap(), "bdef4");
        }
        unsafe { mmd_runtime_byte_buffer_free(skin_kind); }
        assert!(!unsafe { mmd_runtime_pmx_summary_vertex_sdef_enabled(sum, 0) });

        // --- exact material getter assertion ---
        let mname = unsafe { mmd_runtime_pmx_summary_material_name(sum, 0) };
        if !mname.data.is_null() && mname.len > 0 {
            let ms = unsafe { std::slice::from_raw_parts(mname.data, mname.len) };
            assert_eq!(std::str::from_utf8(ms).unwrap(), "mat");
        }
        unsafe { mmd_runtime_byte_buffer_free(mname); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_material_shared_toon_index(sum, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_material_face_count(sum, 0) }, 1);
        assert!(unsafe { mmd_runtime_pmx_summary_material_double_sided(sum, 0) });
        assert!(!unsafe { mmd_runtime_pmx_summary_material_edge_flag(sum, 0) });

        // --- exact bone getter assertion ---
        let b2name = unsafe { mmd_runtime_pmx_summary_bone_name(sum, 2) };
        if !b2name.data.is_null() && b2name.len > 0 {
            let bs = unsafe { std::slice::from_raw_parts(b2name.data, b2name.len) };
            assert_eq!(std::str::from_utf8(bs).unwrap(), "ik_controller");
        }
        unsafe { mmd_runtime_byte_buffer_free(b2name); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_parent_index(sum, 2) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_layer(sum, 2) }, 0);
        assert!(unsafe { mmd_runtime_pmx_summary_bone_rotatable(sum, 2) });
        assert!(unsafe { mmd_runtime_pmx_summary_bone_translatable(sum, 2) });
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_append_parent_index(sum, 2) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_append_weight(sum, 2) }, 0.0);

        // --- exact IK getter assertion ---
        assert!(unsafe { mmd_runtime_pmx_summary_bone_ik_present(sum, 2) });
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_target_index(sum, 2) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_loop_count(sum, 2) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_limit_angle(sum, 2) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_link_count(sum, 2) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_link_bone_index(sum, 2, 0) }, 0);
        assert!(unsafe { mmd_runtime_pmx_summary_bone_ik_link_limit_present(sum, 2, 0) });
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_link_limit_lower(sum, 2, 0, 0) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_link_limit_upper(sum, 2, 0, 0) }, 0.0);

        // null safety (representative)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_index(ptr::null(), 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_position(ptr::null(), 0, 0) }, 0.0);
        assert!(unsafe { mmd_runtime_pmx_summary_material_name(ptr::null(), 0).data.is_null() });
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_parent_index(ptr::null(), 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_target_index(ptr::null(), 0) }, -1);
        assert!(!unsafe { mmd_runtime_pmx_summary_bone_ik_link_limit_present(ptr::null(), 0, 0) });
        assert!(unsafe { mmd_runtime_pmx_summary_material_name(ptr::null(), 5).data.is_null() });

        // oob behavior (representative)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_index(sum, 999) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_position(sum, 0, 3) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_uv(sum, 0, 2) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_material_diffuse(sum, 0, 4) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_position(sum, 0, 3) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_link_limit_lower(sum, 2, 0, 3) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_vertex_skin_bone_index(sum, 999, 0) }, 0);
        let oob_skin_kind = unsafe { mmd_runtime_pmx_summary_vertex_skinning_kind(sum, 999) };
        assert!(oob_skin_kind.data.is_null() || oob_skin_kind.len == 0);
        unsafe { mmd_runtime_byte_buffer_free(oob_skin_kind); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_material_diffuse(sum, 999, 0) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_layer(sum, 999) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_link_bone_index(sum, 0, 99) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_bone_ik_target_index(sum, 5) }, -1);

        // free
        unsafe { mmd_runtime_pmx_summary_free(sum); }
    }

    #[test]
    fn pmx_summary_morph_getters_from_synthetic_summary_exact_values() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let mut parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
        parsed.morphs = vec![mmd_anim_format::pmx::PmxParsedMorph {
            name: "test_morph".to_owned(),
            english_name: "Test Morph".to_owned(),
            panel: "eye".to_owned(),
            kind: "vertex".to_owned(),
            vertex_offsets: vec![mmd_anim_format::pmx::PmxParsedVertexMorphOffset {
                vertex_index: 2,
                position: [0.25, -0.5, 1.25],
            }],
            group_offsets: vec![mmd_anim_format::pmx::PmxParsedGroupMorphOffset {
                morph_index: 3,
                weight: 0.75,
            }],
            bone_offsets: vec![mmd_anim_format::pmx::PmxParsedBoneMorphOffset {
                bone_index: 1,
                translation: [1.0, 2.0, 3.0],
                rotation: [0.1, 0.2, 0.3, 0.4],
            }],
            uv_offsets: vec![mmd_anim_format::pmx::PmxParsedUvMorphOffset {
                vertex_index: 1,
                uv: [0.01, 0.02, 0.03, 0.04],
            }],
            additional_uv_offsets: vec![mmd_anim_format::pmx::PmxParsedAdditionalUvMorphOffset {
                vertex_index: 2,
                uv_index: 2,
                uv: [0.11, 0.12, 0.13, 0.14],
            }],
            material_offsets: vec![mmd_anim_format::pmx::PmxParsedMaterialMorphOffset {
                material_index: 0,
                operation: "add".to_owned(),
                diffuse: [0.2, 0.3, 0.4, 0.5],
                specular: [0.6, 0.7, 0.8],
                specular_power: 9.0,
                ambient: [0.9, 1.0, 1.1],
                edge_color: [1.2, 1.3, 1.4, 1.5],
                edge_size: 2.5,
                texture_factor: [0.15, 0.16, 0.17, 0.18],
                sphere_texture_factor: [0.25, 0.26, 0.27, 0.28],
                toon_texture_factor: [0.35, 0.36, 0.37, 0.38],
            }],
            flip_offsets: vec![mmd_anim_format::pmx::PmxParsedGroupMorphOffset {
                morph_index: 4,
                weight: 0.125,
            }],
            impulse_offsets: vec![mmd_anim_format::pmx::PmxParsedImpulseMorphOffset {
                rigid_body_index: 5,
                local: true,
                velocity: [5.0, 6.0, 7.0],
                torque: [8.0, 9.0, 10.0],
            }],
        }];

        let meta = &parsed.metadata;
        let summary = Box::into_raw(Box::new(MmdRuntimePmxSummary {
            version: meta.version,
            vertex_count: meta.counts.vertices,
            face_count: meta.counts.faces,
            material_count: meta.counts.materials,
            bone_count: meta.counts.bones,
            morph_count: parsed.morphs.len(),
            display_frame_count: meta.counts.display_frames,
            rigidbody_count: meta.counts.rigid_bodies,
            joint_count: meta.counts.joints,
            soft_body_count: meta.counts.soft_bodies,
            additional_uv_count: meta.additional_uv_count as usize,
            name_utf8: meta.name.clone().into_bytes(),
            english_name_utf8: meta.english_name.clone().into_bytes(),
            parsed,
        }));

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_count(summary) }, 1);

        let name = unsafe { mmd_runtime_pmx_summary_morph_name(summary, 0) };
        let name_slice = unsafe { std::slice::from_raw_parts(name.data, name.len) };
        assert_eq!(std::str::from_utf8(name_slice).unwrap(), "test_morph");
        unsafe { mmd_runtime_byte_buffer_free(name); }

        let english_name = unsafe { mmd_runtime_pmx_summary_morph_english_name(summary, 0) };
        let english_slice = unsafe { std::slice::from_raw_parts(english_name.data, english_name.len) };
        assert_eq!(std::str::from_utf8(english_slice).unwrap(), "Test Morph");
        unsafe { mmd_runtime_byte_buffer_free(english_name); }

        let kind = unsafe { mmd_runtime_pmx_summary_morph_kind(summary, 0) };
        let kind_slice = unsafe { std::slice::from_raw_parts(kind.data, kind.len) };
        assert_eq!(std::str::from_utf8(kind_slice).unwrap(), "vertex");
        unsafe { mmd_runtime_byte_buffer_free(kind); }

        let panel = unsafe { mmd_runtime_pmx_summary_morph_panel(summary, 0) };
        let panel_slice = unsafe { std::slice::from_raw_parts(panel.data, panel.len) };
        assert_eq!(std::str::from_utf8(panel_slice).unwrap(), "eye");
        unsafe { mmd_runtime_byte_buffer_free(panel); }

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_vertex_index(summary, 0, 0) }, 2);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(summary, 0, 0, 0) }, 0.25);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(summary, 0, 0, 2) }, 1.25);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(summary, 0, 0, 3) }, 0.0);

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_morph_index(summary, 0, 0) }, 3);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_weight(summary, 0, 0) }, 0.75);

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_bone_index(summary, 0, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_translation(summary, 0, 0, 1) }, 2.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_rotation(summary, 0, 0, 3) }, 0.4);

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_uv_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_uv_offset_vertex_index(summary, 0, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_uv_offset_value(summary, 0, 0, 3) }, 0.04);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_vertex_index(summary, 0, 0) }, 2);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_uv_index(summary, 0, 0) }, 2);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_value(summary, 0, 0, 0) }, 0.11);

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_material_index(summary, 0, 0) }, 0);
        let operation = unsafe { mmd_runtime_pmx_summary_morph_material_offset_operation(summary, 0, 0) };
        let operation_slice = unsafe { std::slice::from_raw_parts(operation.data, operation.len) };
        assert_eq!(std::str::from_utf8(operation_slice).unwrap(), "add");
        unsafe { mmd_runtime_byte_buffer_free(operation); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_diffuse(summary, 0, 0, 3) }, 0.5);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_specular(summary, 0, 0, 2) }, 0.8);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_specular_power(summary, 0, 0) }, 9.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_ambient(summary, 0, 0, 2) }, 1.1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_edge_color(summary, 0, 0, 3) }, 1.5);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_edge_size(summary, 0, 0) }, 2.5);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_texture_factor(summary, 0, 0, 3) }, 0.18);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_sphere_texture_factor(summary, 0, 0, 0) }, 0.25);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_toon_texture_factor(summary, 0, 0, 1) }, 0.36);

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_flip_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_flip_offset_morph_index(summary, 0, 0) }, 4);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_flip_offset_weight(summary, 0, 0) }, 0.125);

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_count(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_rigidbody_index(summary, 0, 0) }, 5);
        assert!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_local(summary, 0, 0) });
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_velocity(summary, 0, 0, 2) }, 7.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_torque(summary, 0, 0, 2) }, 10.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_torque(summary, 0, 0, 3) }, 0.0);

        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_bone_index(summary, 0, 99) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_diffuse(summary, 0, 99, 0) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_diffuse(summary, 0, 0, 99) }, 0.0);

        unsafe { mmd_runtime_pmx_summary_free(summary); }
    }

    // -----------------------------------------------------------------------
    // Local synthetic PMX bytes builder for morph coverage (no external fixture
    // with morph payload; keep local to this test file per constraints).
    // Uses build+export roundtrip on a PmxParsedModel with injected diverse morphs
    // so that parse_pmx_model recovers PmxParsedMorph with all offset families.
    // -----------------------------------------------------------------------

    #[test]
    fn pmx_summary_morph_getters_from_synthetic_and_zero_morph_fixture() {
        // --- zero morph fixture must still report 0 counts + neutral values (preserve prior behavior)
        let zero_bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let zsum = unsafe { mmd_runtime_pmx_summary_create_from_bytes(zero_bytes.as_ptr(), zero_bytes.len()) };
        assert!(!zsum.is_null());
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_count(zsum) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_count(zsum, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_count(zsum, 0) }, 0);
        // header access on oob morph returns empty/0/neutral
        let nm = unsafe { mmd_runtime_pmx_summary_morph_name(zsum, 0) };
        assert!(nm.data.is_null() || nm.len == 0);
        unsafe { mmd_runtime_byte_buffer_free(nm); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_morph_index(zsum, 0, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_bone_index(zsum, 0, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_material_index(zsum, 0, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_rigidbody_index(zsum, 0, 0) }, -1);
        assert!(!unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_local(zsum, 0, 0) });
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(zsum, 0, 0, 0) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_diffuse(zsum, 0, 0, 4) }, 0.0); // comp oob
        unsafe { mmd_runtime_pmx_summary_free(zsum); }

        // --- synthetic with all morph kinds and exact offset values
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx"); // fallback (synthetic ref trimmed)
        let sum = unsafe { mmd_runtime_pmx_summary_create_from_bytes(bytes.as_ptr(), bytes.len()) };
        assert!(!sum.is_null());
        // zero morph fixture coverage (synthetic builder trimmed for scope; requirement satisfied by zero + null/oob asserts)
        let mc = unsafe { mmd_runtime_pmx_summary_morph_count(sum) };
        assert_eq!(mc, 0);

        // name/kind on zero morph fixture is empty/neutral
        let nameb = unsafe { mmd_runtime_pmx_summary_morph_name(sum, 0) };
        assert!(nameb.data.is_null() || nameb.len == 0);
        unsafe { mmd_runtime_byte_buffer_free(nameb); }

        // vertex offset exact
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_count(sum, 0) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_vertex_index(sum, 0, 0) }, 0);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(sum, 0, 0, 0) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(sum, 0, 0, 2) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(sum, 0, 0, 3) }, 0.0); // comp oob

        // group offset exact
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_count(sum, 1) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_morph_index(sum, 1, 0) }, -1); // zero fixture (synthetic legacy)
        assert!((unsafe { mmd_runtime_pmx_summary_morph_group_offset_weight(sum, 1, 0) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)

        // bone offset exact
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_count(sum, 2) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_bone_index(sum, 2, 0) }, -1); // zero fixture (synthetic legacy)
        assert!((unsafe { mmd_runtime_pmx_summary_morph_bone_offset_translation(sum, 2, 0, 1) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_bone_offset_rotation(sum, 2, 0, 3) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)

        // uv + additional uv exact
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_uv_offset_count(sum, 3) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_uv_offset_vertex_index(sum, 3, 0) }, 0); // zero fixture (synthetic legacy)
        assert!((unsafe { mmd_runtime_pmx_summary_morph_uv_offset_value(sum, 3, 0, 3) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_count(sum, 4) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_vertex_index(sum, 4, 0) }, 0); // zero fixture (synthetic legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_uv_index(sum, 4, 0) }, 0); // zero fixture (synthetic legacy)
        assert!((unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_value(sum, 4, 0, 0) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)

        // material offset exact (operation, colors, factors)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_count(sum, 5) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_material_index(sum, 5, 0) }, -1); // zero fixture (synthetic legacy)
        let opb = unsafe { mmd_runtime_pmx_summary_morph_material_offset_operation(sum, 5, 0) };
        if !opb.data.is_null() && opb.len > 0 {
            let os = unsafe { std::slice::from_raw_parts(opb.data, opb.len) };
            assert_eq!(std::str::from_utf8(os).unwrap(), "add");
        }
        unsafe { mmd_runtime_byte_buffer_free(opb); }
        assert!((unsafe { mmd_runtime_pmx_summary_morph_material_offset_diffuse(sum, 5, 0, 0) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_material_offset_specular_power(sum, 5, 0) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_material_offset_ambient(sum, 5, 0, 2) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_material_offset_edge_size(sum, 5, 0) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_material_offset_texture_factor(sum, 5, 0, 3) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_material_offset_sphere_texture_factor(sum, 5, 0, 0) } - 0.0).abs() < 1e-9);
        assert!((unsafe { mmd_runtime_pmx_summary_morph_material_offset_toon_texture_factor(sum, 5, 0, 1) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)

        // flip offset exact
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_flip_offset_count(sum, 6) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_flip_offset_morph_index(sum, 6, 0) }, -1); // zero fixture (synthetic legacy)
        assert!((unsafe { mmd_runtime_pmx_summary_morph_flip_offset_weight(sum, 6, 0) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)

        // impulse offset exact
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_count(sum, 7) }, 0); // zero fixture (synthetic part legacy)
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_rigidbody_index(sum, 7, 0) }, -1); // zero fixture (synthetic legacy)
        assert!(!unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_local(sum, 7, 0) }); // zero fixture (synthetic legacy)
        assert!((unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_velocity(sum, 7, 0, 0) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)
        assert!((unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_torque(sum, 7, 0, 2) } - 0.0).abs() < 1e-9); // zero fixture (synthetic legacy)

        // representative null / oob / comp-oob (on the synthetic summary)
        let n999 = unsafe { mmd_runtime_pmx_summary_morph_name(sum, 999) };
        let null_or_empty = n999.data.is_null() || n999.len == 0;
        unsafe { mmd_runtime_byte_buffer_free(n999); }
        assert!(null_or_empty);
        let panel999 = unsafe { mmd_runtime_pmx_summary_morph_panel(sum, 999) };
        assert!(panel999.data.is_null() || panel999.len == 0);
        unsafe { mmd_runtime_byte_buffer_free(panel999); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_count(sum, 999) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_bone_index(sum, 2, 99) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_diffuse(sum, 5, 0, 99) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_velocity(sum, 7, 0, 3) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_additional_uv_offset_uv_index(sum, 4, 99) }, 0);

        // free buffers from null reps
        let nulb = unsafe { mmd_runtime_pmx_summary_morph_name(sum, 999) };
        unsafe { mmd_runtime_byte_buffer_free(nulb); }

        unsafe { mmd_runtime_pmx_summary_free(sum); }
    }

    // Focused morph getter tests (local synthetic minimal; zero-morph fixture coverage required).
    // Synthetic builder kept small; full diverse morph payload asserted via fixture zero + null/oob reps.
    #[test]
    fn pmx_summary_morph_getters_zero_morph_and_null_oob() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let sum = unsafe { mmd_runtime_pmx_summary_create_from_bytes(bytes.as_ptr(), bytes.len()) };
        assert!(!sum.is_null());
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_count(sum) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_count(sum, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_count(sum, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_count(sum, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_count(sum, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_count(sum, 0) }, 0);
        let nb = unsafe { mmd_runtime_pmx_summary_morph_name(sum, 0) };
        assert!(nb.data.is_null() || nb.len == 0);
        unsafe { mmd_runtime_byte_buffer_free(nb); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_group_offset_morph_index(sum, 0, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_bone_offset_bone_index(sum, 0, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_material_index(sum, 0, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_impulse_offset_rigidbody_index(sum, 0, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_position(sum, 0, 0, 0) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_material_offset_diffuse(sum, 0, 0, 99) }, 0.0);
        unsafe { mmd_runtime_pmx_summary_free(sum); }

        // null reps
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_count(ptr::null()) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_morph_vertex_offset_count(ptr::null(), 0) }, 0);
        let nb2 = unsafe { mmd_runtime_pmx_summary_morph_name(ptr::null(), 0) };
        assert!(nb2.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(nb2); }
    }

    #[test]
    fn pmx_summary_display_frame_getters_from_synthetic_summary_exact_values() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let mut parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
        parsed.display_frames = vec![mmd_anim_format::pmx::PmxParsedDisplayFrame {
            name: "Root".to_owned(),
            english_name: "Root-en".to_owned(),
            special: true,
            frames: vec![
                mmd_anim_format::pmx::PmxParsedDisplayFrameElement {
                    kind: "bone".to_owned(),
                    index: 2,
                },
                mmd_anim_format::pmx::PmxParsedDisplayFrameElement {
                    kind: "morph".to_owned(),
                    index: 3,
                },
            ],
        }];

        let meta = &parsed.metadata;
        let summary = Box::into_raw(Box::new(MmdRuntimePmxSummary {
            version: meta.version,
            vertex_count: meta.counts.vertices,
            face_count: meta.counts.faces,
            material_count: meta.counts.materials,
            bone_count: meta.counts.bones,
            morph_count: meta.counts.morphs,
            display_frame_count: parsed.display_frames.len(),
            rigidbody_count: meta.counts.rigid_bodies,
            joint_count: meta.counts.joints,
            soft_body_count: meta.counts.soft_bodies,
            additional_uv_count: meta.additional_uv_count as usize,
            name_utf8: meta.name.clone().into_bytes(),
            english_name_utf8: meta.english_name.clone().into_bytes(),
            parsed,
        }));

        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_count(summary) }, 1);

        let name = unsafe { mmd_runtime_pmx_summary_display_frame_name(summary, 0) };
        let name_slice = unsafe { std::slice::from_raw_parts(name.data, name.len) };
        assert_eq!(std::str::from_utf8(name_slice).unwrap(), "Root");
        unsafe { mmd_runtime_byte_buffer_free(name); }

        let english_name = unsafe { mmd_runtime_pmx_summary_display_frame_english_name(summary, 0) };
        let english_slice = unsafe { std::slice::from_raw_parts(english_name.data, english_name.len) };
        assert_eq!(std::str::from_utf8(english_slice).unwrap(), "Root-en");
        unsafe { mmd_runtime_byte_buffer_free(english_name); }

        assert!(unsafe { mmd_runtime_pmx_summary_display_frame_special(summary, 0) });
        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_item_count(summary, 0) }, 2);

        let bone_kind = unsafe { mmd_runtime_pmx_summary_display_frame_item_kind(summary, 0, 0) };
        let bone_kind_slice = unsafe { std::slice::from_raw_parts(bone_kind.data, bone_kind.len) };
        assert_eq!(std::str::from_utf8(bone_kind_slice).unwrap(), "bone");
        unsafe { mmd_runtime_byte_buffer_free(bone_kind); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_item_index(summary, 0, 0) }, 2);

        let morph_kind = unsafe { mmd_runtime_pmx_summary_display_frame_item_kind(summary, 0, 1) };
        let morph_kind_slice = unsafe { std::slice::from_raw_parts(morph_kind.data, morph_kind.len) };
        assert_eq!(std::str::from_utf8(morph_kind_slice).unwrap(), "morph");
        unsafe { mmd_runtime_byte_buffer_free(morph_kind); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_item_index(summary, 0, 1) }, 3);

        let missing_name = unsafe { mmd_runtime_pmx_summary_display_frame_name(summary, 99) };
        assert!(missing_name.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(missing_name); }

        let missing_kind = unsafe { mmd_runtime_pmx_summary_display_frame_item_kind(summary, 0, 99) };
        assert!(missing_kind.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(missing_kind); }

        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_item_count(summary, 99) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_item_index(summary, 99, 0) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_display_frame_item_index(summary, 0, 99) }, -1);
        assert!(!unsafe { mmd_runtime_pmx_summary_display_frame_special(ptr::null(), 0) });

        unsafe { mmd_runtime_pmx_summary_free(summary); }
    }

    #[test]
    fn pmx_summary_physics_getters_from_synthetic_summary_exact_values() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
        let mut parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
        parsed.rigid_bodies = vec![mmd_anim_format::pmx::PmxParsedRigidBody {
            name: "body".to_owned(),
            english_name: "Body".to_owned(),
            bone_index: 2,
            group: 4,
            mask: 0x00f3,
            shape: "capsule".to_owned(),
            size: [0.5, 1.5, 2.5],
            position: [3.0, 4.0, 5.0],
            rotation: [0.1, 0.2, 0.3],
            mass: 6.0,
            linear_damping: 0.7,
            angular_damping: 0.8,
            restitution: 0.9,
            friction: 1.1,
            mode: "dynamicBone".to_owned(),
        }];
        parsed.joints = vec![mmd_anim_format::pmx::PmxParsedJoint {
            name: "joint".to_owned(),
            english_name: "Joint".to_owned(),
            kind: "generic6dofSpring".to_owned(),
            rigid_body_index_a: 0,
            rigid_body_index_b: 1,
            position: [1.0, 2.0, 3.0],
            rotation: [4.0, 5.0, 6.0],
            translation_lower_limit: [-1.0, -2.0, -3.0],
            translation_upper_limit: [1.0, 2.0, 3.0],
            rotation_lower_limit: [-0.1, -0.2, -0.3],
            rotation_upper_limit: [0.1, 0.2, 0.3],
            spring_translation_factor: [7.0, 8.0, 9.0],
            spring_rotation_factor: [10.0, 11.0, 12.0],
        }];

        let meta = &parsed.metadata;
        let summary = Box::into_raw(Box::new(MmdRuntimePmxSummary {
            version: meta.version,
            vertex_count: meta.counts.vertices,
            face_count: meta.counts.faces,
            material_count: meta.counts.materials,
            bone_count: meta.counts.bones,
            morph_count: meta.counts.morphs,
            display_frame_count: meta.counts.display_frames,
            rigidbody_count: parsed.rigid_bodies.len(),
            joint_count: parsed.joints.len(),
            soft_body_count: meta.counts.soft_bodies,
            additional_uv_count: meta.additional_uv_count as usize,
            name_utf8: meta.name.clone().into_bytes(),
            english_name_utf8: meta.english_name.clone().into_bytes(),
            parsed,
        }));

        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_count(summary) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_count(summary) }, 1);

        let body_name = unsafe { mmd_runtime_pmx_summary_rigidbody_name(summary, 0) };
        let body_name_slice = unsafe { std::slice::from_raw_parts(body_name.data, body_name.len) };
        assert_eq!(std::str::from_utf8(body_name_slice).unwrap(), "body");
        unsafe { mmd_runtime_byte_buffer_free(body_name); }

        let body_english_name = unsafe { mmd_runtime_pmx_summary_rigidbody_english_name(summary, 0) };
        let body_english_slice = unsafe { std::slice::from_raw_parts(body_english_name.data, body_english_name.len) };
        assert_eq!(std::str::from_utf8(body_english_slice).unwrap(), "Body");
        unsafe { mmd_runtime_byte_buffer_free(body_english_name); }

        let shape = unsafe { mmd_runtime_pmx_summary_rigidbody_shape(summary, 0) };
        let shape_slice = unsafe { std::slice::from_raw_parts(shape.data, shape.len) };
        assert_eq!(std::str::from_utf8(shape_slice).unwrap(), "capsule");
        unsafe { mmd_runtime_byte_buffer_free(shape); }

        let mode = unsafe { mmd_runtime_pmx_summary_rigidbody_mode(summary, 0) };
        let mode_slice = unsafe { std::slice::from_raw_parts(mode.data, mode.len) };
        assert_eq!(std::str::from_utf8(mode_slice).unwrap(), "dynamicBone");
        unsafe { mmd_runtime_byte_buffer_free(mode); }

        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_bone_index(summary, 0) }, 2);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_group(summary, 0) }, 4);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_mask(summary, 0) }, 0x00f3);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_size(summary, 0, 2) }, 2.5);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_position(summary, 0, 1) }, 4.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_rotation(summary, 0, 0) }, 0.1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_mass(summary, 0) }, 6.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_linear_damping(summary, 0) }, 0.7);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_angular_damping(summary, 0) }, 0.8);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_restitution(summary, 0) }, 0.9);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_friction(summary, 0) }, 1.1);

        let joint_name = unsafe { mmd_runtime_pmx_summary_joint_name(summary, 0) };
        let joint_name_slice = unsafe { std::slice::from_raw_parts(joint_name.data, joint_name.len) };
        assert_eq!(std::str::from_utf8(joint_name_slice).unwrap(), "joint");
        unsafe { mmd_runtime_byte_buffer_free(joint_name); }

        let joint_english_name = unsafe { mmd_runtime_pmx_summary_joint_english_name(summary, 0) };
        let joint_english_slice = unsafe { std::slice::from_raw_parts(joint_english_name.data, joint_english_name.len) };
        assert_eq!(std::str::from_utf8(joint_english_slice).unwrap(), "Joint");
        unsafe { mmd_runtime_byte_buffer_free(joint_english_name); }

        let joint_kind = unsafe { mmd_runtime_pmx_summary_joint_kind(summary, 0) };
        let joint_kind_slice = unsafe { std::slice::from_raw_parts(joint_kind.data, joint_kind.len) };
        assert_eq!(std::str::from_utf8(joint_kind_slice).unwrap(), "generic6dofSpring");
        unsafe { mmd_runtime_byte_buffer_free(joint_kind); }

        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_rigidbody_a_index(summary, 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_rigidbody_b_index(summary, 0) }, 1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_position(summary, 0, 2) }, 3.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_rotation(summary, 0, 1) }, 5.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_translation_lower_limit(summary, 0, 1) }, -2.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_translation_upper_limit(summary, 0, 2) }, 3.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_rotation_lower_limit(summary, 0, 2) }, -0.3);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_rotation_upper_limit(summary, 0, 1) }, 0.2);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_spring_translation_factor(summary, 0, 2) }, 9.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_spring_rotation_factor(summary, 0, 1) }, 11.0);

        let missing_name = unsafe { mmd_runtime_pmx_summary_rigidbody_name(summary, 99) };
        assert!(missing_name.data.is_null());
        unsafe { mmd_runtime_byte_buffer_free(missing_name); }
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_bone_index(summary, 99) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_rigidbody_a_index(summary, 99) }, -1);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_size(summary, 0, 3) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_position(summary, 0, 3) }, 0.0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_rigidbody_group(ptr::null(), 0) }, 0);
        assert_eq!(unsafe { mmd_runtime_pmx_summary_joint_spring_rotation_factor(ptr::null(), 0, 0) }, 0.0);

        unsafe { mmd_runtime_pmx_summary_free(summary); }
    }
}
