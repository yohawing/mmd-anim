//! C ABI wrapper for native hosts.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{ptr, slice, str, sync::Arc};

use mmd_anim_runtime::ModelArena;
use mmd_anim_runtime::{
    AnimationClip, AppendPrimitiveInput, BoneAnimationBinding, BoneIndex, FlatAppendTransformInput,
    FlatBoneInput, FlatBoneMorphInput, FlatGroupMorphInput, FlatIkLinkInput, FlatIkSolverInput,
    IkAngleLimit, IkChainDefinition, IkChainLinkDefinition, IkChainPoseInput, IkChainSolver,
    IkSolveOptions, MorphAnimationBinding, MorphIndex, MorphInit, MorphKeyframe, MorphTrack,
    MovableBoneKeyframe, MovableBoneTrack, PropertyAnimationBinding, PropertyKeyframe,
    RuntimeInstance, build_append_transforms_from_flat_iter, build_bones_from_flat,
    build_ik_solvers_from_flat_iter, build_morph_init_from_flat_iter, solve_append_transform,
};

pub const ABI_VERSION: u32 = 2;

pub struct MmdRuntimeModel {
    model: Arc<ModelArena>,
    bone_name_to_index: HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: HashMap<Vec<u8>, usize>,
}

pub struct MmdRuntimeInstance {
    model: Arc<ModelArena>,
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

fn flatten_matrices_into_slice(dst: &mut [f32], matrices: &[glam::Mat4]) {
    debug_assert_eq!(dst.len(), matrices.len() * 16);
    for (matrix_index, matrix) in matrices.iter().enumerate() {
        dst[matrix_index * 16..matrix_index * 16 + 16].copy_from_slice(&matrix.to_cols_array());
    }
}

pub struct MmdRuntimeClip {
    clip: AnimationClip,
}

pub struct MmdRuntimeVmdCameraTrack {
    frames: Vec<mmd_anim_format::vmd::VmdParsedCameraFrame>,
}

pub struct MmdRuntimeVmdLightTrack {
    frames: Vec<mmd_anim_format::vmd::VmdParsedLightFrame>,
}

pub struct MmdRuntimeVmdSelfShadowTrack {
    frames: Vec<mmd_anim_format::vmd::VmdParsedSelfShadowFrame>,
}

pub struct MmdRuntimePmxMaterialSplit {
    split: mmd_anim_format::PmxMaterialSplitResult,
    manifest_json: Vec<u8>,
}

pub struct MmdRuntimePmxGeometry {
    parsed: mmd_anim_format::PmxParsedModel,
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

/// Additive v2 per-bone local-axis descriptor for primitive IK-chain creation.
///
/// Existing `MmdRuntimeFfiRigBone` layout is unchanged. Hosts that need PMX
/// localAxis angle-limit frames pass a parallel array of these into
/// `mmd_runtime_ik_chain_create_v2`.
#[repr(C)]
pub struct MmdRuntimeFfiRigBoneLocalAxisV2 {
    pub has_local_axis: bool,
    pub local_axis_x_xyz: [f32; 3],
    pub local_axis_z_xyz: [f32; 3],
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
const FFI_PANIC_ERROR_MESSAGE: &str = "internal panic in mmd-anim-ffi";
const FFI_ERR_INVALID_INPUT: &str = "invalid input";
const FFI_ERR_VMD_PARSE_FAILED: &str = "vmd parse failed";
const FFI_ERR_PMX_PARSE_FAILED: &str = "pmx parse failed";
const FFI_ERR_PMX_IMPORT_FAILED: &str = "pmx import failed";
const FFI_ERR_VMD_IMPORT_FAILED: &str = "vmd import failed";
const FFI_ERR_CLIP_CREATE_FAILED: &str = "clip create failed";
const FFI_ERR_PMX_EXPORT_FAILED: &str = "pmx export failed";
const FFI_ERR_JSON_ENCODE_FAILED: &str = "json encode failed";
const FFI_ERR_WORKER_PANIC: &str = "worker panic";

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn clear_last_error() {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

fn set_last_error(message: impl AsRef<str>) {
    LAST_ERROR.with(|cell| {
        *cell.borrow_mut() = CString::new(message.as_ref()).ok();
    });
}

fn ffi_guard<T, F>(default: T, f: F) -> T
where
    F: FnOnce() -> T,
{
    ffi_guard_impl(default, true, f)
}

fn ffi_guard_preserve_last_error<T, F>(default: T, f: F) -> T
where
    F: FnOnce() -> T,
{
    ffi_guard_impl(default, false, f)
}

fn ffi_guard_impl<T, F>(default: T, clear_before_call: bool, f: F) -> T
where
    F: FnOnce() -> T,
{
    if clear_before_call {
        clear_last_error();
    }
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => {
            set_last_error(FFI_PANIC_ERROR_MESSAGE);
            default
        }
    }
}

fn ffi_guard_void<F>(f: F)
where
    F: FnOnce(),
{
    ffi_guard((), f);
}

fn empty_byte_buffer_failure(message: &str) -> MmdRuntimeFfiByteBuffer {
    set_last_error(message);
    empty_byte_buffer()
}

fn null_mut_failure<T>(message: &str) -> *mut T {
    set_last_error(message);
    ptr::null_mut()
}

fn null_failure<T>(message: &str) -> *const T {
    set_last_error(message);
    ptr::null()
}

fn false_failure(message: &str) -> bool {
    set_last_error(message);
    false
}

/// Returns the most recent FFI error message for the calling thread.
///
/// The pointer remains valid until the next FFI call on the same thread.
/// Returns null when no message is available.
#[unsafe(no_mangle)]
pub extern "C" fn mmd_runtime_last_error_message() -> *const c_char {
    ffi_guard_preserve_last_error(ptr::null(), || {
        LAST_ERROR.with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|message| message.as_ptr())
                .unwrap_or(ptr::null())
        })
    })
}

#[cfg(test)]
#[unsafe(no_mangle)]
pub extern "C" fn mmd_runtime_test_trigger_panic_guard() -> bool {
    ffi_guard(false, || {
        panic!("test-only panic injection");
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn mmd_runtime_abi_version() -> u32 {
    ffi_guard(ABI_VERSION, || ABI_VERSION)
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
    ffi_guard_void(|| {
        if buffer.data.is_null() || buffer.len == 0 {
            return;
        }
        unsafe {
            drop(Box::from_raw(ptr::slice_from_raw_parts_mut(
                buffer.data,
                buffer.len,
            )));
        }
    })
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
    unsafe {
        mmd_runtime_ik_chain_create_v2(
            bones,
            bone_count,
            ptr::null(),
            target_bone_slot,
            links,
            link_count,
            iteration_count,
            limit_angle,
        )
    }
}

/// Creates a stateful per-chain IK primitive solver with optional local-axis
/// bases (additive v2 entry point).
///
/// When `local_axes` is null, behavior matches `mmd_runtime_ik_chain_create`
/// (no local-axis angle-limit frames). When non-null, it must point to
/// `bone_count` readable entries; degenerate axes are ignored, while non-finite
/// axes cause this function to return null.
///
/// # Safety
///
/// Same pointer requirements as `mmd_runtime_ik_chain_create`. `local_axes`,
/// when non-null, must be readable for `bone_count` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_ik_chain_create_v2(
    bones: *const MmdRuntimeFfiRigBone,
    bone_count: usize,
    local_axes: *const MmdRuntimeFfiRigBoneLocalAxisV2,
    target_bone_slot: u32,
    links: *const MmdRuntimeFfiRigIkLink,
    link_count: usize,
    iteration_count: u32,
    limit_angle: f32,
) -> *mut MmdRuntimeIkChain {
    ffi_guard(ptr::null_mut(), || {
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
        let local_axis_bases = unsafe { build_ik_chain_local_axis_bases(local_axes, bone_count) };
        let Some(local_axis_bases) = local_axis_bases else {
            return ptr::null_mut();
        };
        let solver = IkChainSolver::new_with_local_axis_bases(definition, local_axis_bases);
        Box::into_raw(Box::new(MmdRuntimeIkChain {
            solver,
            bone_count,
            link_count,
        }))
    })
}

/// Frees an IK primitive solver handle.
///
/// # Safety
///
/// `chain` must be null or a pointer returned by `mmd_runtime_ik_chain_create`
/// or `mmd_runtime_ik_chain_create_v2`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_ik_chain_free(chain: *mut MmdRuntimeIkChain) {
    ffi_guard_void(|| {
        if chain.is_null() {
            return;
        }
        unsafe {
            drop(Box::from_raw(chain));
        }
    })
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
    ffi_guard(false, || {
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

        let out =
            unsafe { slice::from_raw_parts_mut(out_link_rotations_xyzw, required_output_len) };
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
    })
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
    ffi_guard(ptr::null_mut(), || {
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
    })
}

/// Frees an append primitive solver handle.
///
/// # Safety
///
/// `solver` must be null or a pointer returned by
/// `mmd_runtime_append_solver_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_append_solver_free(solver: *mut MmdRuntimeAppendSolver) {
    ffi_guard_void(|| {
        if solver.is_null() {
            return;
        }
        unsafe {
            drop(Box::from_raw(solver));
        }
    })
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
    ffi_guard(false, || {
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
        let source_rotation =
            glam::Quat::from_xyzw(rotation[0], rotation[1], rotation[2], rotation[3]);
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
    })
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
    ffi_guard(empty_byte_buffer(), || {
        if data.is_null() || len == 0 {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        }

        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return empty_byte_buffer_failure(FFI_ERR_VMD_PARSE_FAILED),
        };

        match serde_json::to_vec(&parsed) {
            Ok(json) => byte_buffer_from_vec(json),
            Err(_) => empty_byte_buffer_failure(FFI_ERR_JSON_ENCODE_FAILED),
        }
    })
}

/// Parses VMD bytes and returns an owned camera-track handle.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero.
/// The returned track is owned by the caller and must be freed with
/// `mmd_runtime_vmd_camera_track_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_camera_track_create_from_vmd_bytes(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimeVmdCameraTrack {
    ffi_guard(ptr::null_mut(), || {
        if data.is_null() || len == 0 {
            return ptr::null_mut();
        }

        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return ptr::null_mut(),
        };
        if parsed.camera_frames.is_empty() {
            return ptr::null_mut();
        }

        Box::into_raw(Box::new(MmdRuntimeVmdCameraTrack {
            frames: parsed.camera_frames,
        }))
    })
}

/// Returns the number of camera keyframes in a VMD camera track.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by
/// `mmd_runtime_vmd_camera_track_create_from_vmd_bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_camera_track_frame_count(
    track: *const MmdRuntimeVmdCameraTrack,
) -> usize {
    ffi_guard(0, || {
        let Some(track) = (unsafe { track.as_ref() }) else {
            return 0;
        };
        track.frames.len()
    })
}

/// Samples an owned VMD camera track into a flat array.
///
/// Writes `[distance, position.x, position.y, position.z, rotation.x,
/// rotation.y, rotation.z, fov, perspective]` to `out_values`.
/// `perspective` is encoded as `1.0` when enabled, otherwise `0.0`.
///
/// Returns false when `track` or `out_values` is null, when `out_len` is less
/// than 9, when `frame` is not finite, or when the track has no camera
/// keyframes.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by
/// `mmd_runtime_vmd_camera_track_create_from_vmd_bytes`. `out_values` must be
/// valid for writes of at least `out_len` floats when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_camera_track_sample(
    track: *const MmdRuntimeVmdCameraTrack,
    frame: f32,
    out_values: *mut f32,
    out_len: usize,
) -> bool {
    ffi_guard(false, || {
        if !frame.is_finite() || out_values.is_null() || out_len < 9 {
            return false;
        }
        let Some(track) = (unsafe { track.as_ref() }) else {
            return false;
        };
        let Some(camera) = mmd_anim_format::sample_vmd_camera_frames(&track.frames, frame) else {
            return false;
        };
        unsafe {
            write_camera_state_array(camera, out_values);
        }
        true
    })
}

/// Samples camera motion directly from VMD bytes.
///
/// This one-shot helper reparses the VMD on each call. Hosts that evaluate
/// multiple frames should use the camera-track handle API instead.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero.
/// `out_values` must be valid for writes of at least `out_len` floats when
/// non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_sample_camera(
    data: *const u8,
    len: usize,
    frame: f32,
    out_values: *mut f32,
    out_len: usize,
) -> bool {
    ffi_guard(false, || {
        if data.is_null() || len == 0 || !frame.is_finite() || out_values.is_null() || out_len < 9 {
            return false;
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };
        let Some(camera) = mmd_anim_format::sample_vmd_camera_frames(&parsed.camera_frames, frame)
        else {
            return false;
        };
        unsafe {
            write_camera_state_array(camera, out_values);
        }
        true
    })
}

/// Frees a VMD camera track created by
/// `mmd_runtime_vmd_camera_track_create_from_vmd_bytes`.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by this library that has
/// not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_camera_track_free(track: *mut MmdRuntimeVmdCameraTrack) {
    ffi_guard_void(|| {
        if !track.is_null() {
            unsafe {
                drop(Box::from_raw(track));
            }
        }
    })
}

/// Parses VMD bytes and returns an owned light-track handle.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero.
/// The returned track is owned by the caller and must be freed with
/// `mmd_runtime_vmd_light_track_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_light_track_create_from_vmd_bytes(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimeVmdLightTrack {
    ffi_guard(ptr::null_mut(), || {
        if data.is_null() || len == 0 {
            return ptr::null_mut();
        }

        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return ptr::null_mut(),
        };
        if parsed.light_frames.is_empty() {
            return ptr::null_mut();
        }

        Box::into_raw(Box::new(MmdRuntimeVmdLightTrack {
            frames: parsed.light_frames,
        }))
    })
}

/// Returns the number of light keyframes in a VMD light track.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by
/// `mmd_runtime_vmd_light_track_create_from_vmd_bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_light_track_frame_count(
    track: *const MmdRuntimeVmdLightTrack,
) -> usize {
    ffi_guard(0, || {
        let Some(track) = (unsafe { track.as_ref() }) else {
            return 0;
        };
        track.frames.len()
    })
}

/// Samples an owned VMD light track into a flat array.
///
/// Writes `[color.r, color.g, color.b, direction.x, direction.y,
/// direction.z]` to `out_values`.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by
/// `mmd_runtime_vmd_light_track_create_from_vmd_bytes`. `out_values` must be
/// valid for writes of at least `out_len` floats when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_light_track_sample(
    track: *const MmdRuntimeVmdLightTrack,
    frame: f32,
    out_values: *mut f32,
    out_len: usize,
) -> bool {
    ffi_guard(false, || {
        if !frame.is_finite() || out_values.is_null() || out_len < 6 {
            return false;
        }
        let Some(track) = (unsafe { track.as_ref() }) else {
            return false;
        };
        let Some(light) = mmd_anim_format::sample_vmd_light_frames(&track.frames, frame) else {
            return false;
        };
        unsafe {
            write_light_state_array(light, out_values);
        }
        true
    })
}

/// Samples light motion directly from VMD bytes.
///
/// This one-shot helper reparses the VMD on each call. Hosts that evaluate
/// multiple frames should use the light-track handle API instead.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero.
/// `out_values` must be valid for writes of at least `out_len` floats when
/// non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_sample_light(
    data: *const u8,
    len: usize,
    frame: f32,
    out_values: *mut f32,
    out_len: usize,
) -> bool {
    ffi_guard(false, || {
        if data.is_null() || len == 0 || !frame.is_finite() || out_values.is_null() || out_len < 6 {
            return false;
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };
        let Some(light) = mmd_anim_format::sample_vmd_light_frames(&parsed.light_frames, frame)
        else {
            return false;
        };
        unsafe {
            write_light_state_array(light, out_values);
        }
        true
    })
}

/// Frees a VMD light track created by
/// `mmd_runtime_vmd_light_track_create_from_vmd_bytes`.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by this library that has
/// not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_light_track_free(track: *mut MmdRuntimeVmdLightTrack) {
    ffi_guard_void(|| {
        if !track.is_null() {
            unsafe {
                drop(Box::from_raw(track));
            }
        }
    })
}

/// Parses VMD bytes and returns an owned self-shadow-track handle.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero`.
/// The returned track is owned by the caller and must be freed with
/// `mmd_runtime_vmd_self_shadow_track_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_self_shadow_track_create_from_vmd_bytes(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimeVmdSelfShadowTrack {
    ffi_guard(ptr::null_mut(), || {
        if data.is_null() || len == 0 {
            return ptr::null_mut();
        }

        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return ptr::null_mut(),
        };
        if parsed.self_shadow_frames.is_empty() {
            return ptr::null_mut();
        }

        Box::into_raw(Box::new(MmdRuntimeVmdSelfShadowTrack {
            frames: parsed.self_shadow_frames,
        }))
    })
}

/// Returns the number of self-shadow keyframes in a VMD self-shadow track.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by
/// `mmd_runtime_vmd_self_shadow_track_create_from_vmd_bytes`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_self_shadow_track_frame_count(
    track: *const MmdRuntimeVmdSelfShadowTrack,
) -> usize {
    ffi_guard(0, || {
        let Some(track) = (unsafe { track.as_ref() }) else {
            return 0;
        };
        track.frames.len()
    })
}

/// Samples an owned VMD self-shadow track into a flat array.
///
/// Writes `[mode, distance]` to `out_values`. `mode` is encoded as a float.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by
/// `mmd_runtime_vmd_self_shadow_track_create_from_vmd_bytes`. `out_values`
/// must be valid for writes of at least `out_len` floats when non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_self_shadow_track_sample(
    track: *const MmdRuntimeVmdSelfShadowTrack,
    frame: f32,
    out_values: *mut f32,
    out_len: usize,
) -> bool {
    ffi_guard(false, || {
        if !frame.is_finite() || out_values.is_null() || out_len < 2 {
            return false;
        }
        let Some(track) = (unsafe { track.as_ref() }) else {
            return false;
        };
        let Some(self_shadow) =
            mmd_anim_format::sample_vmd_self_shadow_frames(&track.frames, frame)
        else {
            return false;
        };
        unsafe {
            write_self_shadow_state_array(self_shadow, out_values);
        }
        true
    })
}

/// Samples self-shadow motion directly from VMD bytes.
///
/// This one-shot helper reparses the VMD on each call. Hosts that evaluate
/// multiple frames should use the self-shadow-track handle API instead.
///
/// # Safety
///
/// `data` must point to `len` readable bytes when `len` is non-zero.
/// `out_values` must be valid for writes of at least `out_len` floats when
/// non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_sample_self_shadow(
    data: *const u8,
    len: usize,
    frame: f32,
    out_values: *mut f32,
    out_len: usize,
) -> bool {
    ffi_guard(false, || {
        if data.is_null() || len == 0 || !frame.is_finite() || out_values.is_null() || out_len < 2 {
            return false;
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_vmd_animation(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };
        let Some(self_shadow) =
            mmd_anim_format::sample_vmd_self_shadow_frames(&parsed.self_shadow_frames, frame)
        else {
            return false;
        };
        unsafe {
            write_self_shadow_state_array(self_shadow, out_values);
        }
        true
    })
}

/// Frees a VMD self-shadow track created by
/// `mmd_runtime_vmd_self_shadow_track_create_from_vmd_bytes`.
///
/// # Safety
///
/// `track` must be null or a valid pointer returned by this library that has
/// not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_vmd_self_shadow_track_free(
    track: *mut MmdRuntimeVmdSelfShadowTrack,
) {
    ffi_guard_void(|| {
        if !track.is_null() {
            unsafe {
                drop(Box::from_raw(track));
            }
        }
    })
}

fn camera_state_array(camera: mmd_anim_format::VmdCameraState) -> [f32; 9] {
    [
        camera.distance,
        camera.position[0],
        camera.position[1],
        camera.position[2],
        camera.rotation[0],
        camera.rotation[1],
        camera.rotation[2],
        camera.fov,
        if camera.perspective { 1.0 } else { 0.0 },
    ]
}

unsafe fn write_camera_state_array(camera: mmd_anim_format::VmdCameraState, out_values: *mut f32) {
    let values = camera_state_array(camera);
    unsafe {
        ptr::copy_nonoverlapping(values.as_ptr(), out_values, values.len());
    }
}

fn light_state_array(light: mmd_anim_format::VmdLightState) -> [f32; 6] {
    [
        light.color[0],
        light.color[1],
        light.color[2],
        light.direction[0],
        light.direction[1],
        light.direction[2],
    ]
}

unsafe fn write_light_state_array(light: mmd_anim_format::VmdLightState, out_values: *mut f32) {
    let values = light_state_array(light);
    unsafe {
        ptr::copy_nonoverlapping(values.as_ptr(), out_values, values.len());
    }
}

fn self_shadow_state_array(self_shadow: mmd_anim_format::VmdSelfShadowState) -> [f32; 2] {
    [self_shadow.mode as f32, self_shadow.distance]
}

unsafe fn write_self_shadow_state_array(
    self_shadow: mmd_anim_format::VmdSelfShadowState,
    out_values: *mut f32,
) {
    let values = self_shadow_state_array(self_shadow);
    unsafe {
        ptr::copy_nonoverlapping(values.as_ptr(), out_values, values.len());
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
    ffi_guard(empty_byte_buffer(), || {
        if data.is_null() || len == 0 {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        }

        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let parsed = match mmd_anim_format::parse_pmx_model(bytes) {
            Ok(parsed) => parsed,
            Err(_) => return empty_byte_buffer_failure(FFI_ERR_PMX_PARSE_FAILED),
        };

        let mut object = serde_json::Map::with_capacity(9);
        macro_rules! push_json_field {
            ($key:expr, $value:expr) => {
                match serde_json::to_value($value) {
                    Ok(value) => {
                        object.insert($key.to_owned(), value);
                    }
                    Err(_) => return empty_byte_buffer_failure(FFI_ERR_JSON_ENCODE_FAILED),
                }
            };
        }

        push_json_field!("metadata", &parsed.metadata);
        match unity_pmx_materials_json(&parsed.materials) {
            Ok(value) => {
                object.insert("materials".to_owned(), value);
            }
            Err(_) => return empty_byte_buffer_failure(FFI_ERR_JSON_ENCODE_FAILED),
        }
        match unity_pmx_skeleton_json(&parsed.skeleton) {
            Ok(value) => {
                object.insert("skeleton".to_owned(), value);
            }
            Err(_) => return empty_byte_buffer_failure(FFI_ERR_JSON_ENCODE_FAILED),
        }
        push_json_field!("morphs", &parsed.morphs);
        push_json_field!("displayFrames", &parsed.display_frames);
        push_json_field!("rigidBodies", &parsed.rigid_bodies);
        push_json_field!("joints", &parsed.joints);
        push_json_field!("softBodies", &parsed.soft_bodies);
        push_json_field!("diagnostics", &parsed.diagnostics);

        match serde_json::to_vec(&serde_json::Value::Object(object)) {
            Ok(json) => byte_buffer_from_vec(json),
            Err(_) => empty_byte_buffer_failure(FFI_ERR_JSON_ENCODE_FAILED),
        }
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.positions)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.normals)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.uvs)
    })
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
    ffi_guard(0, || {
        if data.is_null() || len == 0 {
            return 0;
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        match mmd_anim_format::parse_pmx_model(bytes) {
            Ok(parsed) => parsed.geometry.additional_uvs.len(),
            Err(_) => 0,
        }
    })
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
    ffi_guard(empty_byte_buffer(), || {
        match parse_pmx_model_from_raw(data, len) {
            Ok(parsed) => {
                let Some(values) = parsed.geometry.additional_uvs.get(uv_index) else {
                    return empty_byte_buffer();
                };
                byte_buffer_from_f32_slice(values)
            }
            Err(buffer) => buffer,
        }
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_u32_buffer(data, len, |geometry| &geometry.indices)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        match parse_pmx_model_from_raw(data, len) {
            Ok(parsed) => pmx_material_groups_buffer(&parsed.geometry.material_groups),
            Err(buffer) => buffer,
        }
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_u32_buffer(data, len, |geometry| &geometry.skin_indices)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.skin_weights)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.edge_scale)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_flags_buffer(data, len, |geometry| &geometry.sdef.enabled)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.sdef.c)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.sdef.r0)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.sdef.r1)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.sdef.rw0)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_buffer(data, len, |geometry| &geometry.sdef.rw1)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_raw_f32_flags_buffer(data, len, |geometry| &geometry.qdef.enabled)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        if data.is_null() || len == 0 {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        match mmd_anim_format::parse_pmx_model(bytes) {
            Ok(parsed) => pmx_skinning_modes_json_buffer(&parsed.geometry),
            Err(_) => empty_byte_buffer_failure(FFI_ERR_PMX_PARSE_FAILED),
        }
    })
}

/// Parses PMX bytes once and creates an opaque geometry handle.
///
/// Use the `mmd_runtime_pmx_geometry_*_buffer` accessors to fetch multiple
/// geometry arrays without reparsing the PMX bytes for each array.
///
/// # Safety
/// `data` must point to `len` readable bytes when `len` is non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_create(
    data: *const u8,
    len: usize,
) -> *mut MmdRuntimePmxGeometry {
    ffi_guard(ptr::null_mut(), || {
        if data.is_null() || len == 0 {
            return null_mut_failure(FFI_ERR_INVALID_INPUT);
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        match mmd_anim_format::parse_pmx_model(bytes) {
            Ok(parsed) => Box::into_raw(Box::new(MmdRuntimePmxGeometry { parsed })),
            Err(_) => null_mut_failure(FFI_ERR_PMX_PARSE_FAILED),
        }
    })
}

/// Frees a PMX geometry handle created by `mmd_runtime_pmx_geometry_create`.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`. Passing any other pointer is undefined
/// behavior.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_free(geometry: *mut MmdRuntimePmxGeometry) {
    ffi_guard_void(|| {
        if !geometry.is_null() {
            unsafe {
                drop(Box::from_raw(geometry));
            }
        }
    })
}

/// Returns the number of additional UV channels in a PMX geometry handle.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_additional_uv_count(
    geometry: *const MmdRuntimePmxGeometry,
) -> usize {
    ffi_guard(0, || {
        let Some(geometry) = (unsafe { geometry.as_ref() }) else {
            return 0;
        };
        geometry.parsed.geometry.additional_uvs.len()
    })
}

/// Returns handle-owned PMX vertex positions as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_positions_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.positions)
    })
}

/// Returns handle-owned PMX vertex normals as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_normals_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.normals)
    })
}

/// Returns handle-owned PMX vertex UVs as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_uvs_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.uvs)
    })
}

/// Returns one handle-owned PMX additional-UV channel as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_additional_uvs_buffer(
    geometry: *const MmdRuntimePmxGeometry,
    uv_index: usize,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        let Some(geometry) = (unsafe { geometry.as_ref() }) else {
            return empty_byte_buffer();
        };
        let Some(values) = geometry.parsed.geometry.additional_uvs.get(uv_index) else {
            return empty_byte_buffer();
        };
        byte_buffer_from_f32_slice(values)
    })
}

/// Returns handle-owned PMX face indices as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_indices_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_u32_buffer(geometry, |geometry| &geometry.indices)
    })
}

/// Returns handle-owned PMX material groups as `[start, count, material_index]` triples.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_material_groups_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        let Some(geometry) = (unsafe { geometry.as_ref() }) else {
            return empty_byte_buffer();
        };
        pmx_material_groups_buffer(&geometry.parsed.geometry.material_groups)
    })
}

/// Returns handle-owned PMX skin bone indices as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_skin_indices_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_u32_buffer(geometry, |geometry| &geometry.skin_indices)
    })
}

/// Returns handle-owned PMX skin weights as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_skin_weights_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.skin_weights)
    })
}

/// Returns handle-owned PMX edge scale values as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_edge_scale_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.edge_scale)
    })
}

/// Returns handle-owned PMX SDEF-enabled flags as one byte per vertex.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_sdef_enabled_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_flags_buffer(geometry, |geometry| &geometry.sdef.enabled)
    })
}

/// Returns handle-owned PMX SDEF C vectors as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_sdef_c_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.sdef.c)
    })
}

/// Returns handle-owned PMX SDEF R0 vectors as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_sdef_r0_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.sdef.r0)
    })
}

/// Returns handle-owned PMX SDEF R1 vectors as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_sdef_r1_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.sdef.r1)
    })
}

/// Returns handle-owned PMX derived SDEF RW0 vectors as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_sdef_rw0_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.sdef.rw0)
    })
}

/// Returns handle-owned PMX derived SDEF RW1 vectors as a native-endian byte buffer.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_sdef_rw1_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_buffer(geometry, |geometry| &geometry.sdef.rw1)
    })
}

/// Returns handle-owned PMX QDEF-enabled flags as one byte per vertex.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_qdef_enabled_buffer(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        pmx_geometry_f32_flags_buffer(geometry, |geometry| &geometry.qdef.enabled)
    })
}

/// Returns handle-owned PMX skinning mode names as a JSON object.
///
/// The returned JSON has the shape `{"skinningModes": ["bdef1", ...]}`.
///
/// # Safety
/// `geometry` must be null or a valid handle returned by
/// `mmd_runtime_pmx_geometry_create`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_geometry_skinning_modes_json(
    geometry: *const MmdRuntimePmxGeometry,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        let Some(geometry) = (unsafe { geometry.as_ref() }) else {
            return empty_byte_buffer();
        };
        pmx_skinning_modes_json_buffer(&geometry.parsed.geometry)
    })
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
    ffi_guard(ptr::null_mut(), || {
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
    })
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
    ffi_guard(ptr::null_mut(), || {
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
    })
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
    ffi_guard_void(|| {
        if split.is_null() {
            return;
        }
        unsafe {
            drop(Box::from_raw(split));
        }
    })
}

/// Frees a PMX rig-spec handle.
///
/// # Safety
/// `spec` must be null or a handle returned by
/// `mmd_runtime_pmx_rig_spec_create` that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_pmx_rig_spec_free(spec: *mut MmdRuntimePmxRigSpec) {
    ffi_guard_void(|| {
        if spec.is_null() {
            return;
        }
        unsafe {
            drop(Box::from_raw(spec));
        }
    })
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
    ffi_guard(0, || {
        let Some(split) = (unsafe { split.as_ref() }) else {
            return 0;
        };
        split.split.meshes.len()
    })
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
    ffi_guard(empty_byte_buffer(), || {
        let Some(split) = (unsafe { split.as_ref() }) else {
            return empty_byte_buffer();
        };
        byte_buffer_from_vec(split.manifest_json.clone())
    })
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
    ffi_guard(empty_byte_buffer(), || {
        let Some(spec) = (unsafe { spec.as_ref() }) else {
            return empty_byte_buffer();
        };
        debug_assert_eq!(spec.spec.bone_count, spec.spec.bones.len());
        byte_buffer_from_vec(spec.manifest_json.clone())
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.positions)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.normals)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.uvs)
    })
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
    ffi_guard(empty_byte_buffer(), || {
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
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_u32_buffer(split, mesh_index, |mesh| &mesh.geometry.indices)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_u32_buffer(split, mesh_index, |mesh| &mesh.geometry.skin_indices)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.skin_weights)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.edge_scale)
    })
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
    ffi_guard(empty_byte_buffer(), || {
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
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.c)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.r0)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.r1)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.rw0)
    })
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
    ffi_guard(empty_byte_buffer(), || {
        pmx_material_split_f32_buffer(split, mesh_index, |mesh| &mesh.geometry.sdef.rw1)
    })
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
    ffi_guard(empty_byte_buffer(), || {
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
    })
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

fn pmx_geometry_f32_buffer(
    geometry: *const MmdRuntimePmxGeometry,
    accessor: fn(&mmd_anim_format::pmx::PmxParsedGeometry) -> &Vec<f32>,
) -> MmdRuntimeFfiByteBuffer {
    let Some(geometry) = (unsafe { geometry.as_ref() }) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_f32_slice(accessor(&geometry.parsed.geometry))
}

fn pmx_geometry_u32_buffer(
    geometry: *const MmdRuntimePmxGeometry,
    accessor: fn(&mmd_anim_format::pmx::PmxParsedGeometry) -> &Vec<u32>,
) -> MmdRuntimeFfiByteBuffer {
    let Some(geometry) = (unsafe { geometry.as_ref() }) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_u32_slice(accessor(&geometry.parsed.geometry))
}

fn pmx_geometry_f32_flags_buffer(
    geometry: *const MmdRuntimePmxGeometry,
    accessor: fn(&mmd_anim_format::pmx::PmxParsedGeometry) -> &Vec<f32>,
) -> MmdRuntimeFfiByteBuffer {
    let Some(geometry) = (unsafe { geometry.as_ref() }) else {
        return empty_byte_buffer();
    };
    byte_buffer_from_vec(
        accessor(&geometry.parsed.geometry)
            .iter()
            .map(|&value| if value > 0.5 { 1u8 } else { 0u8 })
            .collect(),
    )
}

fn parse_pmx_model_from_raw(
    data: *const u8,
    len: usize,
) -> Result<mmd_anim_format::PmxParsedModel, MmdRuntimeFfiByteBuffer> {
    if data.is_null() || len == 0 {
        return Err(empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT));
    }
    let bytes = unsafe { slice::from_raw_parts(data, len) };
    mmd_anim_format::parse_pmx_model(bytes)
        .map_err(|_| empty_byte_buffer_failure(FFI_ERR_PMX_PARSE_FAILED))
}

fn pmx_raw_f32_buffer(
    data: *const u8,
    len: usize,
    accessor: fn(&mmd_anim_format::pmx::PmxParsedGeometry) -> &Vec<f32>,
) -> MmdRuntimeFfiByteBuffer {
    match parse_pmx_model_from_raw(data, len) {
        Ok(parsed) => byte_buffer_from_f32_slice(accessor(&parsed.geometry)),
        Err(buffer) => buffer,
    }
}

fn pmx_raw_u32_buffer(
    data: *const u8,
    len: usize,
    accessor: fn(&mmd_anim_format::pmx::PmxParsedGeometry) -> &Vec<u32>,
) -> MmdRuntimeFfiByteBuffer {
    match parse_pmx_model_from_raw(data, len) {
        Ok(parsed) => byte_buffer_from_u32_slice(accessor(&parsed.geometry)),
        Err(buffer) => buffer,
    }
}

fn pmx_raw_f32_flags_buffer(
    data: *const u8,
    len: usize,
    accessor: fn(&mmd_anim_format::pmx::PmxParsedGeometry) -> &Vec<f32>,
) -> MmdRuntimeFfiByteBuffer {
    match parse_pmx_model_from_raw(data, len) {
        Ok(parsed) => byte_buffer_from_vec(
            accessor(&parsed.geometry)
                .iter()
                .map(|&value| if value > 0.5 { 1u8 } else { 0u8 })
                .collect(),
        ),
        Err(buffer) => buffer,
    }
}

fn pmx_skinning_modes_json_buffer(
    geometry: &mmd_anim_format::pmx::PmxParsedGeometry,
) -> MmdRuntimeFfiByteBuffer {
    let vertex_count = geometry.positions.len() / 3;
    let modes: Vec<&str> = (0..vertex_count)
        .map(|i| {
            geometry
                .sdef
                .skinning_modes
                .get(i)
                .map(String::as_str)
                .unwrap_or("bdef1")
        })
        .collect();
    let wrapper = serde_json::json!({ "skinningModes": modes });
    match serde_json::to_vec(&wrapper) {
        Ok(json) => byte_buffer_from_vec(json),
        Err(_) => empty_byte_buffer_failure(FFI_ERR_JSON_ENCODE_FAILED),
    }
}

fn pmx_material_groups_buffer(
    material_groups: &[mmd_anim_format::pmx::PmxParsedMaterialGroup],
) -> MmdRuntimeFfiByteBuffer {
    let groups: Vec<u32> = material_groups
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
    ffi_guard(empty_byte_buffer(), || {
        if metadata_json.is_null()
            || metadata_json_len == 0
            || positions_xyz.is_null()
            || normals_xyz.is_null()
            || uvs_xy.is_null()
            || vertex_count == 0
        {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        }
        if index_count > 0 && indices.is_null() {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        }
        if skin_indices.is_null() != skin_weights.is_null() {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        }

        let Some(positions_len) = vertex_count.checked_mul(3) else {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        };
        let Some(normals_len) = vertex_count.checked_mul(3) else {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        };
        let Some(uvs_len) = vertex_count.checked_mul(2) else {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        };
        let Some(skin_len) = vertex_count.checked_mul(4) else {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        };

        let metadata_bytes = unsafe { slice::from_raw_parts(metadata_json, metadata_json_len) };
        let metadata_json = match str::from_utf8(metadata_bytes) {
            Ok(json) => json,
            Err(_) => return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT),
        };
        let descriptor: mmd_anim_format::PmxPartsDescriptor =
            match serde_json::from_str(metadata_json) {
                Ok(descriptor) => descriptor,
                Err(_) => return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT),
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

        let model =
            match mmd_anim_format::build_pmx_model_from_parts(mmd_anim_format::PmxPartsInput {
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
                Err(_) => return empty_byte_buffer_failure(FFI_ERR_PMX_EXPORT_FAILED),
            };
        byte_buffer_from_vec(mmd_anim_format::export_pmx_model(&model))
    })
}

macro_rules! create_runtime_model_ffi {
    (
        $parent_indices:expr,
        $rest_positions_xyz:expr,
        $bone_count:expr
        $(, required: [$($required:expr),* $(,)?])?
        $(, fields: { $($field:ident: $value:expr),* $(,)? })?
        $(,)?
    ) => {{
        ffi_guard(ptr::null_mut(), || {
            if $parent_indices.is_null()
                || $rest_positions_xyz.is_null()
                || $bone_count == 0
                $($(|| $required.is_null())*)?
            {
                return ptr::null_mut();
            }

            unsafe {
                create_runtime_model_from_ffi_input(RawModelInput {
                    $($($field: $value,)*)?
                    ..RawModelInput::with_bones(
                        $parent_indices,
                        $rest_positions_xyz,
                        $bone_count,
                    )
                })
            }
        })
    }};
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
    create_runtime_model_ffi!(parent_indices, rest_positions_xyz, bone_count)
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
    create_runtime_model_ffi!(
        parent_indices,
        rest_positions_xyz,
        bone_count,
        required: [inverse_bind_matrices],
        fields: { inverse_bind_matrices: inverse_bind_matrices },
    )
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
    create_runtime_model_ffi!(
        parent_indices,
        rest_positions_xyz,
        bone_count,
        fields: {
            append_transforms: append_transforms,
            append_transform_count: append_transform_count,
        },
    )
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
    create_runtime_model_ffi!(
        parent_indices,
        rest_positions_xyz,
        bone_count,
        required: [inverse_bind_matrices],
        fields: {
            inverse_bind_matrices: inverse_bind_matrices,
            append_transforms: append_transforms,
            append_transform_count: append_transform_count,
        },
    )
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
    create_runtime_model_ffi!(
        parent_indices,
        rest_positions_xyz,
        bone_count,
        fields: {
            inverse_bind_matrices: inverse_bind_matrices,
            ik_solvers: ik_solvers,
            ik_solver_count: ik_solver_count,
            ik_links: ik_links,
            ik_link_count: ik_link_count,
            append_transforms: append_transforms,
            append_transform_count: append_transform_count,
        },
    )
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
    create_runtime_model_ffi!(
        parent_indices,
        rest_positions_xyz,
        bone_count,
        required: [transform_orders],
        fields: {
            inverse_bind_matrices: inverse_bind_matrices,
            transform_orders: transform_orders,
            ik_solvers: ik_solvers,
            ik_solver_count: ik_solver_count,
            ik_links: ik_links,
            ik_link_count: ik_link_count,
            append_transforms: append_transforms,
            append_transform_count: append_transform_count,
        },
    )
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
    create_runtime_model_ffi!(
        parent_indices,
        rest_positions_xyz,
        bone_count,
        fields: {
            inverse_bind_matrices: inverse_bind_matrices,
            transform_orders: transform_orders,
            ik_solvers: ik_solvers,
            ik_solver_count: ik_solver_count,
            ik_links: ik_links,
            ik_link_count: ik_link_count,
            append_transforms: append_transforms,
            append_transform_count: append_transform_count,
            morph_count: morph_count,
            bone_morph_offsets: bone_morph_offsets,
            bone_morph_offset_count: bone_morph_offset_count,
            group_morph_offsets: group_morph_offsets,
            group_morph_offset_count: group_morph_offset_count,
        },
    )
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
    ffi_guard(ptr::null_mut(), || {
        if data.is_null() || len == 0 {
            return null_mut_failure(FFI_ERR_INVALID_INPUT);
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let import = match mmd_anim_format::import_pmx_runtime(bytes) {
            Ok(imp) => imp,
            Err(_) => return null_mut_failure(FFI_ERR_PMX_IMPORT_FAILED),
        };
        Box::into_raw(Box::new(MmdRuntimeModel {
            model: Arc::new(import.model),
            bone_name_to_index: import.bone_name_to_index,
            morph_name_to_index: import.morph_name_to_index,
            ik_solver_bone_name_to_index: import.ik_solver_bone_name_to_index,
        }))
    })
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
    ffi_guard(ptr::null_mut(), || {
        let Some(model) = (unsafe { model.as_ref() }) else {
            return null_mut_failure(FFI_ERR_INVALID_INPUT);
        };
        if data.is_null() || len == 0 {
            return null_mut_failure(FFI_ERR_INVALID_INPUT);
        }
        if model.bone_name_to_index.is_empty() && model.morph_name_to_index.is_empty() {
            return null_mut_failure(FFI_ERR_CLIP_CREATE_FAILED);
        }
        let bytes = unsafe { slice::from_raw_parts(data, len) };
        let motion = match mmd_anim_format::import_vmd_motion(bytes) {
            Ok(m) => m,
            Err(_) => return null_mut_failure(FFI_ERR_VMD_IMPORT_FAILED),
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
    })
}

/// Returns the number of bones in a model handle, or 0 for null.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_bone_count(model: *const MmdRuntimeModel) -> usize {
    ffi_guard(0, || {
        let Some(model) = (unsafe { model.as_ref() }) else {
            return 0;
        };
        model.model.bone_count()
    })
}

/// Returns the number of morph slots in a model handle, or 0 for null.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_morph_count(model: *const MmdRuntimeModel) -> usize {
    ffi_guard(0, || {
        let Some(model) = (unsafe { model.as_ref() }) else {
            return 0;
        };
        model.model.morph_count() as usize
    })
}

/// Returns the number of IK solvers in a model handle, or 0 for null.
///
/// # Safety
///
/// `model` must be null or a valid pointer returned by a model create function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_ik_count(model: *const MmdRuntimeModel) -> usize {
    ffi_guard(0, || {
        let Some(model) = (unsafe { model.as_ref() }) else {
            return 0;
        };
        model.model.ik_count()
    })
}

/// Frees a model created by `mmd_runtime_model_create`.
///
/// # Safety
///
/// `model` must be null or a pointer returned by `mmd_runtime_model_create`
/// that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_free(model: *mut MmdRuntimeModel) {
    ffi_guard_void(|| {
        if !model.is_null() {
            unsafe {
                drop(Box::from_raw(model));
            }
        }
    })
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
    ffi_guard(ptr::null_mut(), || {
        let Some(model) = (unsafe { model.as_ref() }) else {
            return ptr::null_mut();
        };
        let model_arena = Arc::clone(&model.model);
        let mut inst = MmdRuntimeInstance {
            model: Arc::clone(&model_arena),
            runtime: RuntimeInstance::new_with_morph_count(model_arena, morph_count),
            cached_world_matrices: Vec::new(),
            cached_skinning_matrices: Vec::new(),
        };
        inst.refresh_matrix_caches();
        Box::into_raw(Box::new(inst))
    })
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
    ffi_guard(ptr::null_mut(), || {
        let Some(model) = (unsafe { model.as_ref() }) else {
            return ptr::null_mut();
        };
        let model_arena = Arc::clone(&model.model);
        let mut inst = MmdRuntimeInstance {
            model: Arc::clone(&model_arena),
            runtime: RuntimeInstance::new(model_arena),
            cached_world_matrices: Vec::new(),
            cached_skinning_matrices: Vec::new(),
        };
        inst.refresh_matrix_caches();
        Box::into_raw(Box::new(inst))
    })
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
    ffi_guard(ptr::null_mut(), || {
        let Some(model) = (unsafe { model.as_ref() }) else {
            return ptr::null_mut();
        };
        let model_arena = Arc::clone(&model.model);
        let mut inst = MmdRuntimeInstance {
            model: Arc::clone(&model_arena),
            runtime: RuntimeInstance::new_with_counts(model_arena, morph_count, ik_count),
            cached_world_matrices: Vec::new(),
            cached_skinning_matrices: Vec::new(),
        };
        inst.refresh_matrix_caches();
        Box::into_raw(Box::new(inst))
    })
}

/// Frees a runtime instance created by `mmd_runtime_instance_create`.
///
/// # Safety
///
/// `instance` must be null or a pointer returned by
/// `mmd_runtime_instance_create` that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_free(instance: *mut MmdRuntimeInstance) {
    ffi_guard_void(|| {
        if !instance.is_null() {
            unsafe {
                drop(Box::from_raw(instance));
            }
        }
    })
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
    ffi_guard(false, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return false;
        };
        instance.runtime.evaluate_rest_pose();
        instance.refresh_matrix_caches();
        true
    })
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
    ffi_guard(false, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return false;
        };
        let Some(clip) = (unsafe { clip.as_ref() }) else {
            return false;
        };
        instance.runtime.evaluate_clip_frame(&clip.clip, frame);
        instance.refresh_matrix_caches();
        true
    })
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
    ffi_guard(false, || {
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
    })
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
    ffi_guard(false, || {
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
    })
}

/// Returns the required `f32` count for batch world matrix output.
///
/// The batch layout is `[frame][bone][16]`, with each matrix stored as
/// column-major `f32[16]`.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer or size overflow returns `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_clip_frame_batch_world_matrix_f32_len(
    instance: *const MmdRuntimeInstance,
    frame_count: usize,
) -> usize {
    ffi_guard(0, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return 0;
        };
        instance
            .runtime
            .world_matrices()
            .len()
            .checked_mul(16)
            .and_then(|frame_len| frame_len.checked_mul(frame_count))
            .unwrap_or(0)
    })
}

/// Returns the required `f32` count for batch morph weight output.
///
/// The batch layout is `[frame][morph]`.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by
/// `mmd_runtime_instance_create`. A null pointer or size overflow returns `0`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_clip_frame_batch_morph_weight_f32_len(
    instance: *const MmdRuntimeInstance,
    frame_count: usize,
) -> usize {
    ffi_guard(0, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return 0;
        };
        instance
            .runtime
            .morph_weights()
            .len()
            .checked_mul(frame_count)
            .unwrap_or(0)
    })
}

/// Evaluates a contiguous clip frame range into caller-owned batch buffers.
///
/// `worker_count == 0` uses the host's available parallelism. The source
/// `instance` is not evaluated or mutated; it only supplies the immutable model
/// arena plus the morph and IK state sizes. Each worker owns an independent
/// `RuntimeInstance`, so mutable pose and scratch buffers are never shared
/// across threads.
///
/// Output layouts:
/// - `out_world_matrices_f32`: `[frame][bone][16]`, column-major matrices
/// - `out_morph_weights_f32`: `[frame][morph]`
///
/// # Safety
///
/// `instance` and `clip` must be null or valid pointers returned by their
/// respective create functions. Non-empty output regions must point to writable
/// buffers of at least the corresponding `*_len` count and must not alias each
/// other.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_clip_frame_batch(
    instance: *const MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    start_frame: f32,
    frame_step: f32,
    frame_count: usize,
    worker_count: u32,
    out_world_matrices_f32: *mut f32,
    out_world_matrices_f32_len: usize,
    out_morph_weights_f32: *mut f32,
    out_morph_weights_f32_len: usize,
) -> bool {
    ffi_guard(false, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return false;
        };
        let Some(clip) = (unsafe { clip.as_ref() }) else {
            return false;
        };
        if !start_frame.is_finite() || !frame_step.is_finite() {
            return false;
        }

        let world_frame_len = match instance.runtime.world_matrices().len().checked_mul(16) {
            Some(len) => len,
            None => return false,
        };
        let morph_frame_len = instance.runtime.morph_weights().len();
        let required_world_len = match world_frame_len.checked_mul(frame_count) {
            Some(len) => len,
            None => return false,
        };
        let required_morph_len = match morph_frame_len.checked_mul(frame_count) {
            Some(len) => len,
            None => return false,
        };

        if out_world_matrices_f32_len < required_world_len
            || out_morph_weights_f32_len < required_morph_len
        {
            return false;
        }
        if required_world_len > 0 && out_world_matrices_f32.is_null() {
            return false;
        }
        if required_morph_len > 0 && out_morph_weights_f32.is_null() {
            return false;
        }
        if frame_count == 0 {
            return true;
        }

        let model = Arc::clone(&instance.model);
        let morph_count = morph_frame_len;
        let ik_count = instance.runtime.ik_enabled().len();
        let workers = resolve_batch_worker_count(worker_count, frame_count);

        let out_world = if required_world_len == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(out_world_matrices_f32, required_world_len) }
        };
        let out_morph = if required_morph_len == 0 {
            &mut []
        } else {
            unsafe { slice::from_raw_parts_mut(out_morph_weights_f32, required_morph_len) }
        };

        if workers <= 1 {
            let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, ik_count);
            evaluate_clip_frame_batch_chunk(
                &mut runtime,
                &clip.clip,
                start_frame,
                frame_step,
                0,
                frame_count,
                world_frame_len,
                morph_frame_len,
                out_world,
                out_morph,
            );
            return true;
        }

        let frames_per_chunk = frame_count.div_ceil(workers);
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(workers);
            let mut remaining_world = out_world;
            let mut remaining_morph = out_morph;
            for chunk_index in 0..workers {
                let first_frame_index = chunk_index * frames_per_chunk;
                if first_frame_index >= frame_count {
                    break;
                }
                let frames_in_chunk = (frame_count - first_frame_index).min(frames_per_chunk);
                let world_chunk_len = frames_in_chunk * world_frame_len;
                let morph_chunk_len = frames_in_chunk * morph_frame_len;
                let (world_chunk, next_world) = remaining_world.split_at_mut(world_chunk_len);
                let (morph_chunk, next_morph) = remaining_morph.split_at_mut(morph_chunk_len);
                remaining_world = next_world;
                remaining_morph = next_morph;
                let worker_model = Arc::clone(&model);
                let worker_clip = &clip.clip;
                handles.push(scope.spawn(move || {
                    let mut runtime =
                        RuntimeInstance::new_with_counts(worker_model, morph_count, ik_count);
                    evaluate_clip_frame_batch_chunk(
                        &mut runtime,
                        worker_clip,
                        start_frame,
                        frame_step,
                        first_frame_index,
                        frames_in_chunk,
                        world_frame_len,
                        morph_frame_len,
                        world_chunk,
                        morph_chunk,
                    );
                }));
            }

            for handle in handles {
                if handle.join().is_err() {
                    return false_failure(FFI_ERR_WORKER_PANIC);
                }
            }
            true
        })
    })
}

fn resolve_batch_worker_count(requested_worker_count: u32, frame_count: usize) -> usize {
    let requested = requested_worker_count as usize;
    let default_workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let workers = if requested == 0 {
        default_workers
    } else {
        requested
    };
    workers.clamp(1, frame_count.max(1))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_clip_frame_batch_chunk(
    runtime: &mut RuntimeInstance,
    clip: &AnimationClip,
    start_frame: f32,
    frame_step: f32,
    first_frame_index: usize,
    frame_count: usize,
    world_frame_len: usize,
    morph_frame_len: usize,
    out_world: &mut [f32],
    out_morph: &mut [f32],
) {
    for local_frame_index in 0..frame_count {
        let global_frame_index = first_frame_index + local_frame_index;
        let frame = start_frame + frame_step * global_frame_index as f32;
        runtime.evaluate_clip_frame(clip, frame);

        let world_start = local_frame_index * world_frame_len;
        let world_end = world_start + world_frame_len;
        flatten_matrices_into_slice(
            &mut out_world[world_start..world_end],
            runtime.world_matrices(),
        );

        if morph_frame_len > 0 {
            let morph_start = local_frame_index * morph_frame_len;
            let morph_end = morph_start + morph_frame_len;
            out_morph[morph_start..morph_end].copy_from_slice(runtime.morph_weights());
        }
    }
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
    ffi_guard(0, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return 0;
        };
        instance.runtime.world_matrices().len() * 16
    })
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
    ffi_guard(false, || {
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
        flatten_matrices_into_slice(out, matrices);
        true
    })
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
    ffi_guard(0, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return 0;
        };
        instance.runtime.skinning_matrices().len() * 16
    })
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
    ffi_guard(false, || {
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
        flatten_matrices_into_slice(out, matrices);
        true
    })
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
    ffi_guard(0, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return 0;
        };
        instance.runtime.model().bone_count()
    })
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
    ffi_guard(ptr::null(), || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return null_failure(FFI_ERR_INVALID_INPUT);
        };
        instance.cached_world_matrices.as_ptr()
    })
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
    ffi_guard(ptr::null(), || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return null_failure(FFI_ERR_INVALID_INPUT);
        };
        instance.cached_skinning_matrices.as_ptr()
    })
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
    ffi_guard(0, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return 0;
        };
        instance.runtime.morph_weights().len()
    })
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
    ffi_guard(false, || {
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
    })
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
    ffi_guard(0, || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return 0;
        };
        instance.runtime.ik_enabled().len()
    })
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
    ffi_guard(false, || {
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
    })
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
    ffi_guard(ptr::null(), || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return null_failure(FFI_ERR_INVALID_INPUT);
        };
        instance.runtime.morph_weights().as_ptr()
    })
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
    ffi_guard(ptr::null(), || {
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return null_failure(FFI_ERR_INVALID_INPUT);
        };
        instance.runtime.ik_enabled().as_ptr()
    })
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
    ffi_guard(ptr::null_mut(), || {
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
    })
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
    ffi_guard(false, || {
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
    })
}

/// Frees a clip created by `mmd_runtime_clip_create`.
///
/// # Safety
///
/// `clip` must be null or a pointer returned by `mmd_runtime_clip_create` that
/// has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_clip_free(clip: *mut MmdRuntimeClip) {
    ffi_guard_void(|| {
        if !clip.is_null() {
            unsafe {
                drop(Box::from_raw(clip));
            }
        }
    })
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

unsafe fn build_ik_chain_local_axis_bases(
    local_axes: *const MmdRuntimeFfiRigBoneLocalAxisV2,
    bone_count: usize,
) -> Option<Vec<Option<glam::Quat>>> {
    if local_axes.is_null() {
        return Some(vec![None; bone_count]);
    }
    let local_axes = unsafe { checked_slice(local_axes, bone_count) }?;
    let mut bases = Vec::with_capacity(bone_count);
    for axis in local_axes {
        if !axis.has_local_axis {
            bases.push(None);
            continue;
        }
        if !all_finite(&axis.local_axis_x_xyz) || !all_finite(&axis.local_axis_z_xyz) {
            return None;
        }
        let x = glam::Vec3A::new(
            axis.local_axis_x_xyz[0],
            axis.local_axis_x_xyz[1],
            axis.local_axis_x_xyz[2],
        );
        let z = glam::Vec3A::new(
            axis.local_axis_z_xyz[0],
            axis.local_axis_z_xyz[1],
            axis.local_axis_z_xyz[2],
        );
        // Degenerate axes are treated as "no local axis" rather than hard fail,
        // matching ModelArena::with_local_axes defensive behavior.
        bases.push(mmd_anim_runtime::LocalAxis::new(x, z).basis_quat());
    }
    Some(bases)
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

impl Default for RawModelInput {
    fn default() -> Self {
        Self {
            parent_indices: ptr::null(),
            rest_positions_xyz: ptr::null(),
            inverse_bind_matrices: ptr::null(),
            transform_orders: ptr::null(),
            bone_count: 0,
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
        }
    }
}

impl RawModelInput {
    fn with_bones(
        parent_indices: *const i32,
        rest_positions_xyz: *const f32,
        bone_count: usize,
    ) -> Self {
        Self {
            parent_indices,
            rest_positions_xyz,
            bone_count,
            ..Self::default()
        }
    }
}

unsafe fn create_runtime_model_from_ffi_input(input: RawModelInput) -> *mut MmdRuntimeModel {
    let Some(model) = (unsafe { build_model_from_ffi(input) }) else {
        return ptr::null_mut();
    };

    Box::into_raw(Box::new(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }))
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
    let bones = build_bones_from_flat(FlatBoneInput {
        parent_indices: parents,
        rest_positions_xyz: positions,
        inverse_bind_matrices,
        transform_orders,
    })
    .ok()?;

    let ik_links = ik_links
        .iter()
        .map(|link| FlatIkLinkInput {
            bone_index: link.bone_index,
            has_angle_limit: link.flags & IK_LINK_FLAG_ANGLE_LIMIT != 0,
            angle_limit_min_xyz: link.angle_limit_min_xyz,
            angle_limit_max_xyz: link.angle_limit_max_xyz,
        })
        .collect::<Vec<_>>();
    let ik_solvers = build_ik_solvers_from_flat_iter(
        ik_solvers.iter().map(|solver| FlatIkSolverInput {
            ik_bone_index: solver.ik_bone_index,
            target_bone_index: solver.target_bone_index,
            link_offset: solver.link_offset,
            link_count: solver.link_count,
            iteration_count: solver.iteration_count,
            limit_angle: solver.limit_angle,
        }),
        &ik_links,
    )
    .ok()?;

    let append_transforms =
        build_append_transforms_from_flat_iter(append_transforms.iter().map(|append| {
            FlatAppendTransformInput {
                target_bone_index: append.target_bone_index,
                source_bone_index: append.source_bone_index,
                ratio: append.ratio,
                affect_rotation: append.flags & APPEND_FLAG_ROTATION != 0,
                affect_translation: append.flags & APPEND_FLAG_TRANSLATION != 0,
                local: append.flags & APPEND_FLAG_LOCAL != 0,
            }
        }));

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
    build_morph_init_from_flat_iter(
        morph_count,
        bone_morph_offsets.iter().map(|entry| FlatBoneMorphInput {
            morph_index: entry.morph_index,
            target_bone_index: entry.target_bone_index,
            position_offset_xyz: entry.position_offset_xyz,
            rotation_offset_xyzw: entry.rotation_offset_xyzw,
        }),
        group_morph_offsets.iter().map(|entry| FlatGroupMorphInput {
            morph_index: entry.morph_index,
            child_morph_index: entry.child_morph_index,
            ratio: entry.ratio,
        }),
    )
    .ok()
}

#[cfg(test)]
mod tests;
