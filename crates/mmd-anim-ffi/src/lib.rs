//! C ABI wrapper for native hosts.

use std::cell::RefCell;
use std::collections::HashMap;
#[cfg(feature = "physics-bullet-native")]
use std::collections::{BTreeMap, HashSet};
use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::{ptr, slice, str, sync::Arc};

#[cfg(feature = "physics-bullet-native")]
use serde::{Deserialize, Deserializer, Serialize};

use mmd_anim_format::fbx::{
    UnityAnimationClipDto, UnityMorphBinding, UnityReducedPoseBindings,
    reduced_pose_to_unity_animation_clip_with_fps,
};
use mmd_anim_runtime::ModelArena;
use mmd_anim_runtime::{
    AnimationClip, AppendPrimitiveInput, BoneAnimationBinding, BoneIndex, DensePoseSequenceView,
    FlatAppendTransformInput, FlatBoneInput, FlatBoneMorphInput, FlatGroupMorphInput,
    FlatIkLinkInput, FlatIkSolverInput, HostPoseView, IkAngleLimit, IkChainDefinition,
    IkChainLinkDefinition, IkChainPoseInput, IkChainSolver, IkSolveOptions, LocalAxis,
    MorphAnimationBinding, MorphIndex, MorphInit, MorphKeyframe, MorphTrack, MovableBoneKeyframe,
    MovableBoneTrack, PhysicsMode, PhysicsStepStats, PhysicsTickConfig, PoseReductionReport,
    PropertyAnimationBinding, PropertyKeyframe, ReducedPoseSequence, ReductionTarget,
    ReductionTolerances, RuntimeAppendTransformDescriptorV1, RuntimeBoneDescriptorV1,
    RuntimeBoneMorphOffsetDescriptorV1, RuntimeGroupMorphOffsetDescriptorV1,
    RuntimeIkLinkDescriptorV1, RuntimeIkSolverDescriptorV1, RuntimeInstance,
    RuntimeModelDescriptorV1, RuntimeMorphDescriptorV1, SkeletonSnapshot,
    build_append_transforms_from_flat_iter, build_bones_from_flat, build_ik_solvers_from_flat_iter,
    build_morph_init_from_flat_iter, compile_runtime_model_descriptor_v1, solve_append_transform,
};

pub const ABI_VERSION: u32 = 2;
const FEATURE_SPLIT_PHYSICS_EVALUATION: u32 = 1 << 0;
const FEATURE_PHYSICS_BULLET_NATIVE: u32 = 1 << 1;
pub const MMD_RUNTIME_FEATURE_MODEL_DESCRIPTOR: u32 = 1 << 2;
pub const MMD_RUNTIME_FEATURE_HOST_POSE_NATIVE_MORPHS: u32 = 1 << 3;
pub const MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1: u32 =
    mmd_anim_runtime::RUNTIME_MODEL_DESCRIPTOR_VERSION_V1;
pub const MMD_RUNTIME_MODEL_DESCRIPTOR_FLAGS_NONE: u32 = 0;
const FEATURE_MODEL_DESCRIPTOR: u32 = MMD_RUNTIME_FEATURE_MODEL_DESCRIPTOR;
const FEATURE_HOST_POSE_NATIVE_MORPHS: u32 = MMD_RUNTIME_FEATURE_HOST_POSE_NATIVE_MORPHS;

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

pub struct MmdRuntimeReducedPose {
    sequence: ReducedPoseSequence,
    unity_curve_cache: RefCell<Option<MmdRuntimeUnityCurveCache>>,
}

struct MmdRuntimeUnityCurveCache {
    frames_per_second_bits: u32,
    flip_z: bool,
    clip: UnityAnimationClipDto,
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

pub struct MmdRuntimePhysicsWorld {
    #[cfg(feature = "physics-bullet-native")]
    world: mmd_anim_physics_bullet::PmxBulletWorld,
    /// The name-bearing rebuild source is available only for PMX-created
    /// worlds. Descriptor-created worlds intentionally remain unnamed.
    #[cfg(feature = "physics-bullet-native")]
    pmx_model: Option<mmd_anim_format::PmxParsedModel>,
    /// When true, the next bake sample reseeds the Bullet world from the
    /// evaluated pose and copies outputs without advancing either the solver or
    /// the normal forward physics clock.
    ///
    /// Armed on world creation and on a successful
    /// `mmd_runtime_physics_world_reset`. Disarmed after the seed-only bake
    /// sample, or after a successful explicit
    /// `mmd_runtime_physics_world_step_runtime`.
    #[cfg(feature = "physics-bullet-native")]
    next_bake_sample_is_seed_only: bool,
}

#[cfg(feature = "physics-bullet-native")]
const PHYSICS_PARAMS_SCHEMA_VERSION: u32 = 1;

#[cfg(feature = "physics-bullet-native")]
#[derive(Debug, Serialize)]
struct PhysicsParamsSnapshot {
    schema_version: u32,
    rigid_bodies: BTreeMap<String, PhysicsRigidBodyParams>,
    joints: BTreeMap<String, PhysicsJointParams>,
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Debug, Serialize)]
struct PhysicsRigidBodyParams {
    mass: f32,
    linear_damping: f32,
    angular_damping: f32,
    friction: f32,
    restitution: f32,
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Debug, Serialize)]
struct PhysicsJointParams {
    translation_lower_limit: [f32; 3],
    translation_upper_limit: [f32; 3],
    rotation_lower_limit: [f32; 3],
    rotation_upper_limit: [f32; 3],
    spring_translation_factor: [f32; 3],
    spring_rotation_factor: [f32; 3],
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicsParamsUpdate {
    schema_version: u32,
    #[serde(default)]
    rigid_bodies: BTreeMap<String, PhysicsRigidBodyParamsUpdate>,
    #[serde(default)]
    joints: BTreeMap<String, PhysicsJointParamsUpdate>,
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicsRigidBodyParamsUpdate {
    #[serde(default, deserialize_with = "deserialize_present_option")]
    mass: Option<f32>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    linear_damping: Option<f32>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    angular_damping: Option<f32>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    friction: Option<f32>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    restitution: Option<f32>,
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PhysicsJointParamsUpdate {
    #[serde(default, deserialize_with = "deserialize_present_option")]
    translation_lower_limit: Option<[f32; 3]>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    translation_upper_limit: Option<[f32; 3]>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    rotation_lower_limit: Option<[f32; 3]>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    rotation_upper_limit: Option<[f32; 3]>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    spring_translation_factor: Option<[f32; 3]>,
    #[serde(default, deserialize_with = "deserialize_present_option")]
    spring_rotation_factor: Option<[f32; 3]>,
}

#[cfg(feature = "physics-bullet-native")]
fn deserialize_present_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
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

/// Complete per-bone v1 descriptor for payload-free runtime model construction.
/// `rest_position_xyz` is the absolute PMX-space rest position; the runtime
/// derives the parent-relative rest translation and inverse bind matrix.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MmdRuntimeModelBoneDescriptor {
    pub parent_index: i32,
    pub rest_position_xyz: [f32; 3],
    pub transform_order: i32,
    pub flags: u32,
    pub fixed_axis_xyz: [f32; 3],
    pub local_axis_x_xyz: [f32; 3],
    pub local_axis_z_xyz: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MmdRuntimeModelIkSolverDescriptor {
    pub ik_bone_index: u32,
    pub target_bone_index: u32,
    pub link_offset: usize,
    pub link_count: usize,
    pub iteration_count: u32,
    pub limit_angle: f32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MmdRuntimeModelIkLinkDescriptor {
    pub bone_index: u32,
    pub flags: u32,
    pub angle_limit_min_xyz: [f32; 3],
    pub angle_limit_max_xyz: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MmdRuntimeModelAppendDescriptor {
    pub target_bone_index: u32,
    pub source_bone_index: u32,
    pub ratio: f32,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MmdRuntimeModelBoneMorphOffsetDescriptor {
    pub morph_index: u32,
    pub target_bone_index: u32,
    pub position_offset_xyz: [f32; 3],
    pub rotation_offset_xyzw: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MmdRuntimeModelGroupMorphOffsetDescriptor {
    pub morph_index: u32,
    pub child_morph_index: u32,
    pub ratio: f32,
}

/// Versioned, self-describing model descriptor.  All pointed-to records are
/// borrowed only for the duration of the constructor call.
#[repr(C)]
pub struct MmdRuntimeModelDescriptor {
    pub struct_size: u32,
    pub descriptor_version: u32,
    pub flags: u32,
    pub reserved: u32,
    pub bones: *const MmdRuntimeModelBoneDescriptor,
    pub bone_count: usize,
    pub ik_solvers: *const MmdRuntimeModelIkSolverDescriptor,
    pub ik_solver_count: usize,
    pub ik_links: *const MmdRuntimeModelIkLinkDescriptor,
    pub ik_link_count: usize,
    pub append_transforms: *const MmdRuntimeModelAppendDescriptor,
    pub append_transform_count: usize,
    pub morph_count: u32,
    pub bone_morph_offsets: *const MmdRuntimeModelBoneMorphOffsetDescriptor,
    pub bone_morph_offset_count: usize,
    pub group_morph_offsets: *const MmdRuntimeModelGroupMorphOffsetDescriptor,
    pub group_morph_offset_count: usize,
}

#[repr(C)]
pub struct MmdRuntimeFfiRigIkLink {
    pub bone_slot: u32,
    pub has_angle_limit: u8,
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
    pub has_local_axis: u8,
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
    pub affect_rotation: u8,
    pub affect_translation: u8,
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

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MmdRuntimeFfiReductionTolerances {
    pub local_position: f32,
    pub local_rotation_radians: f32,
    pub world_position: f32,
    pub world_rotation_radians: f32,
    pub morph_weight: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MmdRuntimeFfiPoseReductionReport {
    pub source_bone_key_count: usize,
    pub reduced_bone_key_count: usize,
    pub source_morph_key_count: usize,
    pub reduced_morph_key_count: usize,
    pub max_local_position_error: f32,
    pub max_local_rotation_error_radians: f32,
    pub max_world_position_error: f32,
    pub max_world_rotation_error_radians: f32,
    pub max_morph_weight_error: f32,
}

pub const MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_TRANSLATION: u32 = 0;
pub const MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_EULER: u32 = 1;
pub const MMD_RUNTIME_UNITY_CURVE_MORPH_WEIGHT: u32 = 2;

pub const MMD_RUNTIME_UNITY_CURVE_AXIS_X: u32 = 0;
pub const MMD_RUNTIME_UNITY_CURVE_AXIS_Y: u32 = 1;
pub const MMD_RUNTIME_UNITY_CURVE_AXIS_Z: u32 = 2;
pub const MMD_RUNTIME_UNITY_CURVE_AXIS_NONE: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MmdRuntimeFfiUnityCurveDescriptor {
    pub semantic: u32,
    pub target_index: u32,
    pub axis: u32,
    pub key_count: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MmdRuntimeFfiUnityCurveKey {
    pub time_seconds: f32,
    pub value: f32,
    pub in_tangent: f32,
    pub out_tangent: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmdRuntimeStatus {
    Ok = 0,
    InvalidInput = 1,
    Unsupported = 2,
    BufferTooSmall = 3,
    Error = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmdRuntimeFfiPhysicsMode {
    Off = 0,
    Trace = 1,
    Live = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmdRuntimeFfiPhysicsRigidBodyShape {
    Sphere = 0,
    Box = 1,
    Capsule = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmdRuntimeFfiPhysicsRigidBodyMode {
    Static = 0,
    Dynamic = 1,
    DynamicBone = 2,
    Unknown = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmdRuntimeFfiPhysicsJointKind {
    Generic6DofSpring = 0,
    Unsupported = 1,
}

/// Selects the physics action performed by
/// `mmd_runtime_evaluate_host_frame`: reseed the Bullet world from the
/// evaluated pose without advancing the solver, or advance the runtime's
/// fixed-step physics clock forward.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmdRuntimePhysicsFrameAction {
    Seed = 0,
    Step = 1,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MmdRuntimeFfiPhysicsTickConfig {
    pub fixed_substep_seconds: f32,
    pub max_substeps_per_tick: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MmdRuntimeFfiPhysicsStepStats {
    pub input_dt_seconds: f32,
    pub clamped_dt_seconds: f32,
    pub substeps: u32,
    pub accumulator_seconds: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MmdRuntimeFfiPhysicsRigidBodyDesc {
    pub shape: u32,
    pub shape_size: [f32; 3],
    pub position_xyz: [f32; 3],
    pub rotation_euler_xyz: [f32; 3],
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    pub collision_group: u16,
    pub collision_mask: u16,
    pub bone_index: i32,
    pub mode: u32,
    pub body_from_bone_position_xyz: [f32; 3],
    pub body_from_bone_rotation_xyzw: [f32; 4],
    pub bone_from_body_position_xyz: [f32; 3],
    pub bone_from_body_rotation_xyzw: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MmdRuntimeFfiPhysicsJointDesc {
    pub kind: u32,
    pub rigidbody_a: usize,
    pub rigidbody_b: usize,
    pub position_xyz: [f32; 3],
    pub rotation_euler_xyz: [f32; 3],
    pub translation_lower_limit_xyz: [f32; 3],
    pub translation_upper_limit_xyz: [f32; 3],
    pub rotation_lower_limit_xyz: [f32; 3],
    pub rotation_upper_limit_xyz: [f32; 3],
    pub spring_translation_factor_xyz: [f32; 3],
    pub spring_rotation_factor_xyz: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MmdRuntimeFfiPhysicsWorldStepReport {
    pub tick: MmdRuntimeFfiPhysicsStepStats,
    pub kinematic_rigidbodies_fed: usize,
    pub bones_written_back: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MmdRuntimeFfiPhysicsRigidBodyBinding {
    pub bone_index: i32,
    pub mode: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MmdRuntimeFfiHostPoseView {
    pub local_position_offsets_xyz: *const f32,
    pub local_rotation_xyzw: *const f32,
    pub local_scales_xyz: *const f32,
    pub bone_count: usize,
    pub morph_weights: *const f32,
    pub morph_count: usize,
    pub ik_enabled: *const u8,
    pub ik_count: usize,
}

const APPEND_FLAG_ROTATION: u32 = 1;
const APPEND_FLAG_TRANSLATION: u32 = 1 << 1;
const APPEND_FLAG_LOCAL: u32 = 1 << 2;
const IK_LINK_FLAG_ANGLE_LIMIT: u32 = 1;
const MODEL_BONE_FLAG_TRANSFORM_AFTER_PHYSICS: u32 = 1;
const MODEL_BONE_FLAG_FIXED_AXIS: u32 = 1 << 1;
const MODEL_BONE_FLAG_LOCAL_AXIS: u32 = 1 << 2;
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
        // C strings cannot carry an embedded NUL.  Preserve the diagnostic
        // instead of silently dropping it when a PMX name (or another input
        // field) contains one.  The escaped form remains recognizable while
        // keeping the pointer valid for the documented C-string API.
        let sanitized = message.as_ref().replace('\0', "\\0");
        *cell.borrow_mut() =
            Some(CString::new(sanitized).expect("NUL is escaped before CString construction"));
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

fn status_failure(status: MmdRuntimeStatus, message: &str) -> MmdRuntimeStatus {
    set_last_error(message);
    status
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

#[unsafe(no_mangle)]
pub extern "C" fn mmd_runtime_feature_flags() -> u32 {
    ffi_guard(FEATURE_SPLIT_PHYSICS_EVALUATION, runtime_feature_flags)
}

fn runtime_feature_flags() -> u32 {
    FEATURE_SPLIT_PHYSICS_EVALUATION
        | FEATURE_MODEL_DESCRIPTOR
        | FEATURE_HOST_POSE_NATIVE_MORPHS
        | if cfg!(feature = "physics-bullet-native") {
            FEATURE_PHYSICS_BULLET_NATIVE
        } else {
            0
        }
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
        let Some(affect_rotation) = parse_ffi_bool(config.affect_rotation) else {
            return ptr::null_mut();
        };
        let Some(affect_translation) = parse_ffi_bool(config.affect_translation) else {
            return ptr::null_mut();
        };
        if !config.ratio.is_finite() {
            return ptr::null_mut();
        }
        Box::into_raw(Box::new(MmdRuntimeAppendSolver {
            ratio: config.ratio,
            affect_rotation,
            affect_translation,
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

/// Creates a complete runtime model from a versioned typed descriptor.
///
/// All input records are copied while this call is active. The returned model
/// owns its normalized runtime storage and does not retain any input pointer.
/// Invalid descriptor metadata or compiler validation failures return NULL and
/// set a concrete indexed thread-local error message.
/// Every descriptor pointer must be null exactly when its paired count is zero;
/// otherwise it must reference that many readable records.
///
/// # Safety
///
/// `descriptor` must point to a readable, correctly aligned
/// `MmdRuntimeModelDescriptor` whose pointed-to arrays remain readable for the
/// duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_model_create_from_descriptor(
    descriptor: *const MmdRuntimeModelDescriptor,
) -> *mut MmdRuntimeModel {
    ffi_guard(ptr::null_mut(), || {
        let Some(model) = (unsafe { build_model_from_descriptor_ffi(descriptor) }) else {
            return ptr::null_mut();
        };
        Box::into_raw(Box::new(model))
    })
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

fn physics_mode_to_ffi(mode: PhysicsMode) -> MmdRuntimeFfiPhysicsMode {
    match mode {
        PhysicsMode::Off => MmdRuntimeFfiPhysicsMode::Off,
        PhysicsMode::Trace => MmdRuntimeFfiPhysicsMode::Trace,
        PhysicsMode::Live => MmdRuntimeFfiPhysicsMode::Live,
    }
}

fn physics_mode_from_ffi(mode: u32) -> Option<PhysicsMode> {
    match mode {
        0 => Some(PhysicsMode::Off),
        1 => Some(PhysicsMode::Trace),
        2 => Some(PhysicsMode::Live),
        _ => None,
    }
}

fn physics_tick_config_to_ffi(config: PhysicsTickConfig) -> MmdRuntimeFfiPhysicsTickConfig {
    MmdRuntimeFfiPhysicsTickConfig {
        fixed_substep_seconds: config.fixed_substep_seconds,
        max_substeps_per_tick: config.max_substeps_per_tick,
    }
}

fn physics_step_stats_to_ffi(stats: PhysicsStepStats) -> MmdRuntimeFfiPhysicsStepStats {
    MmdRuntimeFfiPhysicsStepStats {
        input_dt_seconds: stats.input_dt_seconds,
        clamped_dt_seconds: stats.clamped_dt_seconds,
        substeps: stats.substeps,
        accumulator_seconds: stats.accumulator_seconds,
    }
}

/// Returns the current split-physics evaluation mode.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function. `out_mode` must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_get_physics_mode(
    instance: *const MmdRuntimeInstance,
    out_mode: *mut MmdRuntimeFfiPhysicsMode,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_mode.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        unsafe {
            *out_mode = physics_mode_to_ffi(instance.runtime.physics_mode());
        }
        MmdRuntimeStatus::Ok
    })
}

/// Sets the split-physics evaluation mode.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_set_physics_mode(
    instance: *mut MmdRuntimeInstance,
    mode: u32,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(mode) = physics_mode_from_ffi(mode) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        instance.runtime.set_physics_mode(mode);
        MmdRuntimeStatus::Ok
    })
}

/// Returns the current fixed-step physics clock config.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function. `out_config` must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_get_physics_tick_config(
    instance: *const MmdRuntimeInstance,
    out_config: *mut MmdRuntimeFfiPhysicsTickConfig,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_config.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let Some(instance) = (unsafe { instance.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        unsafe {
            *out_config = physics_tick_config_to_ffi(instance.runtime.physics_tick_config());
        }
        MmdRuntimeStatus::Ok
    })
}

/// Sets the fixed-step physics clock config.
///
/// Invalid values are sanitized the same way the Rust runtime does.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function. `config` must point to a readable config struct.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_set_physics_tick_config(
    instance: *mut MmdRuntimeInstance,
    config: *const MmdRuntimeFfiPhysicsTickConfig,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if config.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let config = unsafe { *config };
        instance.runtime.set_physics_tick_config(PhysicsTickConfig {
            fixed_substep_seconds: config.fixed_substep_seconds,
            max_substeps_per_tick: config.max_substeps_per_tick,
        });
        MmdRuntimeStatus::Ok
    })
}

/// Resets the split-physics fixed-step accumulator.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_reset_physics_tick(
    instance: *mut MmdRuntimeInstance,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        instance.runtime.reset_physics_tick();
        MmdRuntimeStatus::Ok
    })
}

/// Evaluates a clip frame through the pre-physics phase.
///
/// # Safety
///
/// `instance` and `clip` must be null or valid pointers returned by their
/// respective create functions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_clip_frame_before_physics(
    instance: *mut MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    frame: f32,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(clip) = (unsafe { clip.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        instance
            .runtime
            .evaluate_clip_frame_before_physics(&clip.clip, frame);
        instance.refresh_matrix_caches();
        MmdRuntimeStatus::Ok
    })
}

/// Evaluates a clip frame through the pre-physics phase with IK options.
///
/// # Safety
///
/// `instance` and `clip` must be null or valid pointers returned by their
/// respective create functions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_clip_frame_before_physics_with_ik_options(
    instance: *mut MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    frame: f32,
    ik_tolerance: f32,
    ik_max_iterations_cap: u32,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(clip) = (unsafe { clip.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if !ik_tolerance.is_finite() || ik_tolerance < 0.0 {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        instance
            .runtime
            .evaluate_clip_frame_before_physics_with_ik_options(
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
        MmdRuntimeStatus::Ok
    })
}

/// Evaluates the current pose through the pre-physics phase.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_current_pose_before_physics(
    instance: *mut MmdRuntimeInstance,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        instance.runtime.evaluate_current_pose_before_physics();
        instance.refresh_matrix_caches();
        MmdRuntimeStatus::Ok
    })
}

/// Applies a host-provided local pose to the runtime instance.
///
/// The local arrays are the pre-morph base pose. The runtime validates counts
/// and finiteness, applies atomically - no partial mutation on failure - then
/// expands group/bone morph weights natively. Hosts must not preapply morph
/// bone deltas. After success the pose is set but world matrices are NOT yet
/// evaluated; call evaluate_current_pose_before_physics next.
///
/// # Safety
///
/// `instance` must be a valid instance handle. `view` must be a valid
/// pointer to a `MmdRuntimeFfiHostPoseView` whose data pointers are valid
/// for reads of the indicated counts.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_apply_host_pose(
    instance: *mut MmdRuntimeInstance,
    view: *const MmdRuntimeFfiHostPoseView,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(view) = (unsafe { view.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        apply_host_pose_impl(instance, view)
    })
}

/// Applies a host pose and evaluates the before-physics phase in one call.
///
/// The local arrays follow the pre-morph base-pose contract of
/// `mmd_runtime_instance_apply_host_pose`; group/bone morph expansion is
/// native and hosts must not preapply morph bone deltas. Equivalent to
/// apply_host_pose followed by evaluate_current_pose_before_physics. On
/// failure, neither the pose nor the evaluation is applied.
///
/// # Safety
///
/// Same as `mmd_runtime_instance_apply_host_pose`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(
    instance: *mut MmdRuntimeInstance,
    view: *const MmdRuntimeFfiHostPoseView,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(view) = (unsafe { view.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let status = apply_host_pose_impl(instance, view);
        if status != MmdRuntimeStatus::Ok {
            return status;
        }
        instance.runtime.evaluate_current_pose_before_physics();
        instance.refresh_matrix_caches();
        MmdRuntimeStatus::Ok
    })
}

fn apply_host_pose_impl(
    instance: &mut MmdRuntimeInstance,
    view: &MmdRuntimeFfiHostPoseView,
) -> MmdRuntimeStatus {
    let bone_count = view.bone_count;

    let Some(position_len) = bone_count.checked_mul(3) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(rotation_len) = bone_count.checked_mul(4) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(scale_len) = bone_count.checked_mul(3) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };

    let Some(position_f32) =
        (unsafe { checked_slice(view.local_position_offsets_xyz, position_len) })
    else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(rotation_f32) = (unsafe { checked_slice(view.local_rotation_xyzw, rotation_len) })
    else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(scale_f32) = (unsafe { checked_slice(view.local_scales_xyz, scale_len) }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(morph_weights) = (unsafe { checked_slice(view.morph_weights, view.morph_count) })
    else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(ik_enabled) = (unsafe { checked_slice(view.ik_enabled, view.ik_count) }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };

    let local_position_offsets: Vec<glam::Vec3A> = position_f32
        .chunks_exact(3)
        .map(|p| glam::Vec3A::new(p[0], p[1], p[2]))
        .collect();
    let local_rotations: Vec<glam::Quat> = rotation_f32
        .chunks_exact(4)
        .map(|q| glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]))
        .collect();
    let local_scales: Vec<glam::Vec3A> = scale_f32
        .chunks_exact(3)
        .map(|s| glam::Vec3A::new(s[0], s[1], s[2]))
        .collect();

    let host_pose = HostPoseView {
        local_position_offsets: &local_position_offsets,
        local_rotations: &local_rotations,
        local_scales: &local_scales,
        morph_weights,
        ik_enabled,
    };

    match instance.runtime.apply_host_pose(&host_pose) {
        Ok(()) => MmdRuntimeStatus::Ok,
        Err(err) => status_failure(MmdRuntimeStatus::InvalidInput, &err.to_string()),
    }
}

/// Evaluates the current pose through the post-physics phase.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_current_pose_after_physics(
    instance: *mut MmdRuntimeInstance,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        instance.runtime.evaluate_current_pose_after_physics();
        instance.refresh_matrix_caches();
        MmdRuntimeStatus::Ok
    })
}

/// Evaluates the current pose through the post-physics phase with IK options.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_evaluate_current_pose_after_physics_with_ik_options(
    instance: *mut MmdRuntimeInstance,
    ik_tolerance: f32,
    ik_max_iterations_cap: u32,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if !ik_tolerance.is_finite() || ik_tolerance < 0.0 {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        instance
            .runtime
            .evaluate_current_pose_after_physics_with_ik_options(IkSolveOptions {
                tolerance: ik_tolerance,
                max_iterations_cap: if ik_max_iterations_cap == 0 {
                    None
                } else {
                    Some(ik_max_iterations_cap)
                },
            });
        instance.refresh_matrix_caches();
        MmdRuntimeStatus::Ok
    })
}

/// Advances the split-physics fixed-step clock without a physics backend.
///
/// # Safety
///
/// `instance` must be null or a valid pointer returned by an instance create
/// function. `out_stats` must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_advance_physics_tick_clock(
    instance: *mut MmdRuntimeInstance,
    dt_seconds: f32,
    out_stats: *mut MmdRuntimeFfiPhysicsStepStats,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_stats.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let stats = instance.runtime.advance_physics_tick_clock(dt_seconds);
        unsafe {
            *out_stats = physics_step_stats_to_ffi(stats);
        }
        MmdRuntimeStatus::Ok
    })
}

/// Applies external physics world matrices to the current pose.
///
/// # Safety
///
/// `physics_world_matrices_f32` must point to `bone_count * 16` readable f32
/// values. If `physics_world_matrix_mask_u8` is null, every matrix is applied;
/// otherwise it must point to `bone_count` readable u8 values and non-zero
/// entries select applied bones. `out_updated_bone_count` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_instance_apply_physics_world_matrices(
    instance: *mut MmdRuntimeInstance,
    physics_world_matrices_f32: *const f32,
    physics_world_matrices_f32_len: usize,
    physics_world_matrix_mask_u8: *const u8,
    physics_world_matrix_mask_u8_len: usize,
    out_updated_bone_count: *mut usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let bone_count = instance.runtime.model().bone_count();
        let Some(required_matrix_len) = bone_count.checked_mul(16) else {
            return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT);
        };
        if physics_world_matrices_f32.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        if physics_world_matrices_f32_len < required_matrix_len {
            return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT);
        }
        if !physics_world_matrix_mask_u8.is_null() && physics_world_matrix_mask_u8_len < bone_count
        {
            return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT);
        }

        let matrix_values =
            unsafe { slice::from_raw_parts(physics_world_matrices_f32, required_matrix_len) };
        let mask = if physics_world_matrix_mask_u8.is_null() {
            None
        } else {
            Some(unsafe { slice::from_raw_parts(physics_world_matrix_mask_u8, bone_count) })
        };

        let mut physics_world_matrices = Vec::with_capacity(bone_count);
        for bone_index in 0..bone_count {
            let apply = mask.is_none_or(|mask| mask[bone_index] != 0);
            if apply {
                let start = bone_index * 16;
                let raw = <[f32; 16]>::try_from(&matrix_values[start..start + 16])
                    .expect("slice length checked");
                if !all_finite(&raw) {
                    return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
                }
                physics_world_matrices.push(Some(glam::Mat4::from_cols_array(&raw)));
            } else {
                physics_world_matrices.push(None);
            }
        }

        let updated = instance
            .runtime
            .apply_physics_world_matrices(&physics_world_matrices);
        instance.refresh_matrix_caches();
        if !out_updated_bone_count.is_null() {
            unsafe {
                *out_updated_bone_count = updated;
            }
        }
        MmdRuntimeStatus::Ok
    })
}

/// Creates a feature-gated native physics world from typed descriptors.
///
/// # Safety
///
/// Descriptor pointers must be null only when their counts are zero. `out_world`
/// must be valid for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_create(
    rigidbodies: *const MmdRuntimeFfiPhysicsRigidBodyDesc,
    rigidbody_count: usize,
    joints: *const MmdRuntimeFfiPhysicsJointDesc,
    joint_count: usize,
    out_world: *mut *mut MmdRuntimePhysicsWorld,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_world.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        unsafe {
            *out_world = ptr::null_mut();
        }
        physics_world_create_impl(rigidbodies, rigidbody_count, joints, joint_count, out_world)
    })
}

/// Creates a feature-gated native physics world from PMX bytes.
///
/// # Safety
///
/// `pmx_data` must point to `pmx_len` readable bytes. `out_world` must be valid
/// for writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_create_from_pmx_bytes(
    pmx_data: *const u8,
    pmx_len: usize,
    out_world: *mut *mut MmdRuntimePhysicsWorld,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_world.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        unsafe {
            *out_world = ptr::null_mut();
        }
        physics_world_create_from_pmx_bytes_impl(pmx_data, pmx_len, out_world)
    })
}

/// Frees a physics world handle.
///
/// # Safety
///
/// `world` must be null or a pointer returned by a physics world create
/// function that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_free(world: *mut MmdRuntimePhysicsWorld) {
    ffi_guard_void(|| {
        if !world.is_null() {
            unsafe {
                drop(Box::from_raw(world));
            }
        }
    })
}

/// Returns a deterministic schema-v1 snapshot of editable PMX physics parameters.
///
/// The returned UTF-8 JSON buffer is Rust-owned and must be freed with
/// `mmd_runtime_byte_buffer_free`. Only worlds created from PMX bytes have the
/// original names required by this API; descriptor-created worlds are unsupported.
///
/// # Safety
///
/// `world` must be a valid physics-world handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_params_get_json(
    world: *const MmdRuntimePhysicsWorld,
) -> MmdRuntimeFfiByteBuffer {
    ffi_guard(empty_byte_buffer(), || {
        let Some(world) = (unsafe { world.as_ref() }) else {
            return empty_byte_buffer_failure(FFI_ERR_INVALID_INPUT);
        };
        physics_params_get_json_impl(world)
    })
}

/// Applies a partial schema-v1 named PMX physics-parameter update.
///
/// A successful update rebuilds the whole Bullet world, preserves gravity,
/// resets simulation state, and re-arms seed-only behavior for the next
/// seed/bake sample. Any parse, validation, or rebuild failure leaves the
/// original world untouched.
///
/// # Safety
///
/// `world` must be a valid physics-world handle. `data` must point to `len`
/// readable UTF-8 JSON bytes, and `len` must be non-zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_params_set_json(
    world: *mut MmdRuntimePhysicsWorld,
    data: *const u8,
    len: usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(world) = (unsafe { world.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(data) = (unsafe { checked_slice(data, len) }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if data.is_empty() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        physics_params_set_json_impl(world, data)
    })
}

/// Resets the physics world and reseeds it from the runtime pose.
///
/// Reset includes one fixed 1/60 second solver settle, static-body re-pinning,
/// transient-state cleanup, and settled dynamic-body readback into the runtime
/// pose. The returned count still describes bodies seeded from the runtime
/// pose, not solver substeps.
///
/// A successful reset also arms seed-only behavior for the next
/// `mmd_runtime_physics_world_bake_clip_frames` sample: that sample evaluates,
/// reseeds without a solver step, and copies without advancing the normal
/// forward physics clock.
///
/// # Safety
///
/// `world` and `instance` must be valid handles. `out_seeded_rigidbody_count`
/// may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_reset(
    world: *mut MmdRuntimePhysicsWorld,
    instance: *mut MmdRuntimeInstance,
    out_seeded_rigidbody_count: *mut usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        physics_world_reset_impl(world, instance, out_seeded_rigidbody_count)
    })
}

/// Steps a physics world using the runtime's fixed-step physics clock.
///
/// This live-evaluation path feeds static bodies before the step. DynamicBone
/// bodies are seeded during reset but remain solver-owned during forward steps.
///
/// A successful step disarms bake seed-only state: the next
/// `mmd_runtime_physics_world_bake_clip_frames` sample advances physics
/// normally rather than reseeding without a step.
///
/// # Safety
///
/// `world` and `instance` must be valid handles. `out_report` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_step_runtime(
    world: *mut MmdRuntimePhysicsWorld,
    instance: *mut MmdRuntimeInstance,
    dt_seconds: f32,
    out_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        physics_world_step_runtime_impl(world, instance, dt_seconds, out_report)
    })
}

/// Evaluates one atomic host frame: applies a validated host pose, evaluates
/// the before-physics phase, seeds or steps the physics world, and evaluates
/// the after-physics phase.
///
/// This combines the sequence a host would otherwise chain across
/// `mmd_runtime_instance_apply_host_pose`,
/// `mmd_runtime_instance_evaluate_current_pose_before_physics_with_ik_options`,
/// either `mmd_runtime_physics_world_reset` or
/// `mmd_runtime_physics_world_step_runtime`, and
/// `mmd_runtime_instance_evaluate_current_pose_after_physics_with_ik_options`
/// into a single call, guaranteeing the correct ordering.
///
/// On failure applying the host pose, no mutation occurs (fail-atomic). Once
/// the pose has been applied, before-physics evaluation and the physics
/// action always run; a physics failure still leaves the applied pose and its
/// before-physics evaluation in place.
///
/// For `MmdRuntimePhysicsFrameAction::Seed`, `dt_seconds` is ignored and
/// `out_report` (when non-null) is zeroed, since a seed does not advance the
/// solver and produces no meaningful step statistics.
///
/// `MmdRuntimePhysicsFrameAction::Step` requires the instance's physics mode
/// to be `Trace` or `Live`; it returns `INVALID_INPUT` when the mode is
/// `Off`.
///
/// Returns `INVALID_INPUT` when the physics world's rigidbody bindings
/// reference bone indices outside the instance's bone range.
///
/// # Safety
///
/// `instance` and `world` must be valid handles. `pose` must be a valid
/// pointer to a `MmdRuntimeFfiHostPoseView` whose data pointers are valid for
/// reads of the indicated counts. `out_report` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_evaluate_host_frame(
    instance: *mut MmdRuntimeInstance,
    world: *mut MmdRuntimePhysicsWorld,
    pose: *const MmdRuntimeFfiHostPoseView,
    action: u32,
    dt_seconds: f32,
    ik_tolerance: f32,
    ik_max_iterations_cap: u32,
    out_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let action = match action {
            0 => MmdRuntimePhysicsFrameAction::Seed,
            1 => MmdRuntimePhysicsFrameAction::Step,
            _ => {
                return status_failure(
                    MmdRuntimeStatus::InvalidInput,
                    "unknown physics frame action",
                );
            }
        };

        let Some(instance) = (unsafe { instance.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if world.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let Some(pose) = (unsafe { pose.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if !ik_tolerance.is_finite() || ik_tolerance < 0.0 {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        if action == MmdRuntimePhysicsFrameAction::Step
            && (!dt_seconds.is_finite() || dt_seconds < 0.0)
        {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }

        // Validate physics preconditions before mutating pose.
        let pre_status = validate_host_frame_physics_impl(world, instance, action);
        if pre_status != MmdRuntimeStatus::Ok {
            return pre_status;
        }

        let status = apply_host_pose_impl(instance, pose);
        if status != MmdRuntimeStatus::Ok {
            return status;
        }

        // IK options apply to the before-physics phase only; the bridge
        // functions (reset/step) evaluate after-physics with defaults.
        let ik_options = IkSolveOptions {
            tolerance: ik_tolerance,
            max_iterations_cap: if ik_max_iterations_cap == 0 {
                None
            } else {
                Some(ik_max_iterations_cap)
            },
        };
        instance
            .runtime
            .evaluate_current_pose_before_physics_with_ik_options(ik_options);

        evaluate_host_frame_physics_impl(world, instance, action, dt_seconds, out_report)
    })
}

/// Returns the number of rigid bodies in a physics world.
///
/// # Safety
///
/// `world` must be a valid handle and `out_rigidbody_count` must be valid for
/// writes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_rigidbody_count(
    world: *const MmdRuntimePhysicsWorld,
    out_rigidbody_count: *mut usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        physics_world_rigidbody_count_impl(world, out_rigidbody_count)
    })
}

/// Returns a physics world's current gravity vector.
///
/// # Safety
///
/// `world` must be a valid handle and `out_gravity_xyz` must be valid for
/// writes of 3 `f32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_get_gravity(
    world: *const MmdRuntimePhysicsWorld,
    out_gravity_xyz: *mut f32,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(world) = (unsafe { world.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if out_gravity_xyz.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        physics_world_get_gravity_impl(world, out_gravity_xyz)
    })
}

/// Sets a physics world's gravity vector.
///
/// # Safety
///
/// `world` must be a valid handle and `gravity_xyz` must be valid for reads
/// of 3 `f32` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_set_gravity(
    world: *mut MmdRuntimePhysicsWorld,
    gravity_xyz: *const f32,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(world) = (unsafe { world.as_mut() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if gravity_xyz.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        physics_world_set_gravity_impl(world, gravity_xyz)
    })
}

/// Copies rigid body diagnostics as `[body][position_xyz, rotation_xyzw]`.
///
/// # Safety
///
/// `out_transforms_f32` must point to at least `rigidbody_count * 7` writable
/// f32 values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_copy_rigidbody_states(
    world: *const MmdRuntimePhysicsWorld,
    out_transforms_f32: *mut f32,
    out_transforms_f32_len: usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        physics_world_copy_rigidbody_states_impl(world, out_transforms_f32, out_transforms_f32_len)
    })
}

/// Copies rigidbody-to-bone binding metadata into a caller-owned buffer.
///
/// # Safety
///
/// `out_bindings` must point to at least `capacity` writable elements.
/// `out_count` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_copy_rigidbody_bindings(
    world: *const MmdRuntimePhysicsWorld,
    out_bindings: *mut MmdRuntimeFfiPhysicsRigidBodyBinding,
    capacity: usize,
    out_count: *mut usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        physics_world_copy_rigidbody_bindings_impl(world, out_bindings, capacity, out_count)
    })
}

/// Writes a per-bone mask where non-zero entries indicate bones driven by physics.
///
/// A bone is physics-driven if any rigidbody with `writes_back_to_bone()` mode
/// (Dynamic or DynamicBone) is bound to it.
///
/// Returns `BUFFER_TOO_SMALL` when `bone_count` is smaller than the physics
/// world's required bone count (the highest bound bone index plus one),
/// since the mask could not represent all bindings in that case.
///
/// # Safety
///
/// `out_mask` must point to at least `bone_count` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_physics_driven_bone_mask(
    world: *const MmdRuntimePhysicsWorld,
    out_mask: *mut u8,
    bone_count: usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        physics_world_physics_driven_bone_mask_impl(world, out_mask, bone_count)
    })
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_create_impl(
    _rigidbodies: *const MmdRuntimeFfiPhysicsRigidBodyDesc,
    _rigidbody_count: usize,
    _joints: *const MmdRuntimeFfiPhysicsJointDesc,
    _joint_count: usize,
    _out_world: *mut *mut MmdRuntimePhysicsWorld,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_create_from_pmx_bytes_impl(
    _pmx_data: *const u8,
    _pmx_len: usize,
    _out_world: *mut *mut MmdRuntimePhysicsWorld,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_params_get_json_impl(_world: &MmdRuntimePhysicsWorld) -> MmdRuntimeFfiByteBuffer {
    empty_byte_buffer_failure("physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_params_set_json_impl(
    _world: &mut MmdRuntimePhysicsWorld,
    _data: &[u8],
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_reset_impl(
    _world: *mut MmdRuntimePhysicsWorld,
    _instance: *mut MmdRuntimeInstance,
    _out_seeded_rigidbody_count: *mut usize,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_step_runtime_impl(
    _world: *mut MmdRuntimePhysicsWorld,
    _instance: *mut MmdRuntimeInstance,
    _dt_seconds: f32,
    _out_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(feature = "physics-bullet-native")]
fn validate_host_frame_physics_impl(
    world: *mut MmdRuntimePhysicsWorld,
    instance: &MmdRuntimeInstance,
    action: MmdRuntimePhysicsFrameAction,
) -> MmdRuntimeStatus {
    let Some(world) = (unsafe { world.as_ref() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let instance_bone_count = instance.runtime.world_matrices().len();
    for (rigidbody_index, binding) in world.world.rigidbody_bindings.iter().enumerate() {
        let Some(bone_index) = binding.bone_index else {
            continue;
        };
        if bone_index >= instance_bone_count {
            return status_failure(
                MmdRuntimeStatus::InvalidInput,
                &format!(
                    "physics_world.rigidbodies[{rigidbody_index}].bone_index: {bone_index} exceeds instance bone_count {instance_bone_count}"
                ),
            );
        }
    }
    if action == MmdRuntimePhysicsFrameAction::Step
        && !instance.runtime.physics_mode().steps_backend()
    {
        return status_failure(
            MmdRuntimeStatus::InvalidInput,
            "physics mode is Off; set Trace or Live before stepping",
        );
    }
    MmdRuntimeStatus::Ok
}

#[cfg(not(feature = "physics-bullet-native"))]
fn validate_host_frame_physics_impl(
    _world: *mut MmdRuntimePhysicsWorld,
    _instance: &MmdRuntimeInstance,
    _action: MmdRuntimePhysicsFrameAction,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn evaluate_host_frame_physics_impl(
    _world: *mut MmdRuntimePhysicsWorld,
    _instance: &mut MmdRuntimeInstance,
    _action: MmdRuntimePhysicsFrameAction,
    _dt_seconds: f32,
    _out_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_rigidbody_count_impl(
    _world: *const MmdRuntimePhysicsWorld,
    _out_rigidbody_count: *mut usize,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_copy_rigidbody_states_impl(
    _world: *const MmdRuntimePhysicsWorld,
    _out_transforms_f32: *mut f32,
    _out_transforms_f32_len: usize,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_copy_rigidbody_bindings_impl(
    _world: *const MmdRuntimePhysicsWorld,
    _out_bindings: *mut MmdRuntimeFfiPhysicsRigidBodyBinding,
    _capacity: usize,
    _out_count: *mut usize,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_physics_driven_bone_mask_impl(
    _world: *const MmdRuntimePhysicsWorld,
    _out_mask: *mut u8,
    _bone_count: usize,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_get_gravity_impl(
    _world: &MmdRuntimePhysicsWorld,
    _out_gravity_xyz: *mut f32,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics feature not enabled")
}

#[cfg(not(feature = "physics-bullet-native"))]
fn physics_world_set_gravity_impl(
    _world: &mut MmdRuntimePhysicsWorld,
    _gravity_xyz: *const f32,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics feature not enabled")
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_create_impl(
    rigidbodies: *const MmdRuntimeFfiPhysicsRigidBodyDesc,
    rigidbody_count: usize,
    joints: *const MmdRuntimeFfiPhysicsJointDesc,
    joint_count: usize,
    out_world: *mut *mut MmdRuntimePhysicsWorld,
) -> MmdRuntimeStatus {
    let Some(rigidbodies) = (unsafe { checked_slice(rigidbodies, rigidbody_count) }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(joints) = (unsafe { checked_slice(joints, joint_count) }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(rigidbodies) = rigidbodies
        .iter()
        .map(physics_rigidbody_desc_from_ffi)
        .collect::<Option<Vec<_>>>()
    else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(joints) = joints
        .iter()
        .map(physics_joint_desc_from_ffi)
        .collect::<Option<Vec<_>>>()
    else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    match mmd_anim_physics_bullet::build_bullet_world_from_descriptors(&rigidbodies, &joints) {
        Ok(world) => {
            unsafe {
                *out_world = Box::into_raw(Box::new(MmdRuntimePhysicsWorld {
                    world,
                    pmx_model: None,
                    next_bake_sample_is_seed_only: true,
                }));
            }
            MmdRuntimeStatus::Ok
        }
        Err(err) => status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_create_from_pmx_bytes_impl(
    pmx_data: *const u8,
    pmx_len: usize,
    out_world: *mut *mut MmdRuntimePhysicsWorld,
) -> MmdRuntimeStatus {
    let Some(bytes) = (unsafe { checked_slice(pmx_data, pmx_len) }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Ok(model) = mmd_anim_format::parse_pmx_model(bytes) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_PMX_PARSE_FAILED);
    };
    match mmd_anim_physics_bullet::build_bullet_world_from_pmx(&model) {
        Ok(world) => {
            unsafe {
                *out_world = Box::into_raw(Box::new(MmdRuntimePhysicsWorld {
                    world,
                    pmx_model: Some(model),
                    next_bake_sample_is_seed_only: true,
                }));
            }
            MmdRuntimeStatus::Ok
        }
        Err(err) => status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
    }
}

#[cfg(feature = "physics-bullet-native")]
fn validate_physics_param_names(model: &mmd_anim_format::PmxParsedModel) -> Result<(), String> {
    fn validate<'a>(kind: &str, names: impl Iterator<Item = &'a str>) -> Result<(), String> {
        let mut seen = HashSet::new();
        for name in names {
            if name.is_empty() {
                return Err(format!("PMX {kind} has an empty name"));
            }
            if !seen.insert(name) {
                return Err(format!("PMX {kind} name is duplicated: {name}"));
            }
        }
        Ok(())
    }

    validate(
        "rigid body",
        model.rigid_bodies.iter().map(|body| body.name.as_str()),
    )?;
    validate(
        "joint",
        model.joints.iter().map(|joint| joint.name.as_str()),
    )
}

#[cfg(feature = "physics-bullet-native")]
fn physics_params_get_json_impl(world: &MmdRuntimePhysicsWorld) -> MmdRuntimeFfiByteBuffer {
    let Some(model) = world.pmx_model.as_ref() else {
        return empty_byte_buffer_failure("physics parameters require a PMX-created world");
    };
    if let Err(message) = validate_physics_param_names(model) {
        set_last_error(message);
        return empty_byte_buffer();
    }

    let rigid_bodies = model
        .rigid_bodies
        .iter()
        .map(|body| {
            (
                body.name.clone(),
                PhysicsRigidBodyParams {
                    mass: body.mass,
                    linear_damping: body.linear_damping,
                    angular_damping: body.angular_damping,
                    friction: body.friction,
                    restitution: body.restitution,
                },
            )
        })
        .collect();
    let joints = model
        .joints
        .iter()
        .map(|joint| {
            (
                joint.name.clone(),
                PhysicsJointParams {
                    translation_lower_limit: joint.translation_lower_limit,
                    translation_upper_limit: joint.translation_upper_limit,
                    rotation_lower_limit: joint.rotation_lower_limit,
                    rotation_upper_limit: joint.rotation_upper_limit,
                    spring_translation_factor: joint.spring_translation_factor,
                    spring_rotation_factor: joint.spring_rotation_factor,
                },
            )
        })
        .collect();
    let snapshot = PhysicsParamsSnapshot {
        schema_version: PHYSICS_PARAMS_SCHEMA_VERSION,
        rigid_bodies,
        joints,
    };
    match serde_json::to_vec(&snapshot) {
        Ok(bytes) => byte_buffer_from_vec(bytes),
        Err(error) => {
            set_last_error(format!("physics parameter JSON encode failed: {error}"));
            empty_byte_buffer()
        }
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_params_set_json_impl(
    world: &mut MmdRuntimePhysicsWorld,
    data: &[u8],
) -> MmdRuntimeStatus {
    let Some(source_model) = world.pmx_model.as_ref() else {
        return status_failure(
            MmdRuntimeStatus::Unsupported,
            "physics parameters require a PMX-created world",
        );
    };
    if let Err(message) = validate_physics_param_names(source_model) {
        set_last_error(message);
        return MmdRuntimeStatus::InvalidInput;
    }
    let update: PhysicsParamsUpdate = match serde_json::from_slice(data) {
        Ok(update) => update,
        Err(error) => {
            set_last_error(format!("invalid physics parameter JSON: {error}"));
            return MmdRuntimeStatus::InvalidInput;
        }
    };
    if update.schema_version != PHYSICS_PARAMS_SCHEMA_VERSION {
        return status_failure(
            MmdRuntimeStatus::InvalidInput,
            "unsupported physics parameter schema_version",
        );
    }

    let mut replacement_model = source_model.clone();
    for (name, values) in update.rigid_bodies {
        let Some(body) = replacement_model
            .rigid_bodies
            .iter_mut()
            .find(|body| body.name == name)
        else {
            set_last_error(format!("unknown PMX rigid body name: {name}"));
            return MmdRuntimeStatus::InvalidInput;
        };
        if let Some(value) = values.mass {
            if !value.is_finite() || value < 0.0 {
                return invalid_physics_param("mass must be finite and >= 0");
            }
            body.mass = value;
        }
        if let Some(value) = values.linear_damping {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return invalid_physics_param("linear_damping must be finite and in [0, 1]");
            }
            body.linear_damping = value;
        }
        if let Some(value) = values.angular_damping {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return invalid_physics_param("angular_damping must be finite and in [0, 1]");
            }
            body.angular_damping = value;
        }
        if let Some(value) = values.friction {
            if !value.is_finite() || value < 0.0 {
                return invalid_physics_param("friction must be finite and >= 0");
            }
            body.friction = value;
        }
        if let Some(value) = values.restitution {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return invalid_physics_param("restitution must be finite and in [0, 1]");
            }
            body.restitution = value;
        }
    }

    for (name, values) in update.joints {
        let Some(joint) = replacement_model
            .joints
            .iter_mut()
            .find(|joint| joint.name == name)
        else {
            set_last_error(format!("unknown PMX joint name: {name}"));
            return MmdRuntimeStatus::InvalidInput;
        };
        if let Some(value) = values.translation_lower_limit {
            joint.translation_lower_limit = value;
        }
        if let Some(value) = values.translation_upper_limit {
            joint.translation_upper_limit = value;
        }
        if let Some(value) = values.rotation_lower_limit {
            joint.rotation_lower_limit = value;
        }
        if let Some(value) = values.rotation_upper_limit {
            joint.rotation_upper_limit = value;
        }
        if let Some(value) = values.spring_translation_factor {
            joint.spring_translation_factor = value;
        }
        if let Some(value) = values.spring_rotation_factor {
            joint.spring_rotation_factor = value;
        }
        for (field, value) in [
            ("translation_lower_limit", joint.translation_lower_limit),
            ("translation_upper_limit", joint.translation_upper_limit),
            ("rotation_lower_limit", joint.rotation_lower_limit),
            ("rotation_upper_limit", joint.rotation_upper_limit),
        ] {
            if !all_finite(&value) {
                return invalid_physics_param(&format!("{field} must contain finite values"));
            }
        }
        for (field, value) in [
            ("spring_translation_factor", joint.spring_translation_factor),
            ("spring_rotation_factor", joint.spring_rotation_factor),
        ] {
            if !all_finite(&value) || value.iter().any(|component| *component < 0.0) {
                return invalid_physics_param(&format!("{field} must contain finite values >= 0"));
            }
        }
        if joint
            .translation_lower_limit
            .iter()
            .zip(joint.translation_upper_limit)
            .any(|(lower, upper)| *lower > upper)
        {
            return invalid_physics_param("translation lower limits must be <= upper limits");
        }
        if joint
            .rotation_lower_limit
            .iter()
            .zip(joint.rotation_upper_limit)
            .any(|(lower, upper)| *lower > upper)
        {
            return invalid_physics_param("rotation lower limits must be <= upper limits");
        }
    }

    let gravity = match world.world.world.gravity() {
        Ok(gravity) => gravity,
        Err(error) => return status_failure(MmdRuntimeStatus::Error, &error.to_string()),
    };
    let mut replacement_world =
        match mmd_anim_physics_bullet::build_bullet_world_from_pmx(&replacement_model) {
            Ok(world) => world,
            Err(error) => return status_failure(MmdRuntimeStatus::Error, &error.to_string()),
        };
    if let Err(error) = replacement_world.world.set_gravity(gravity) {
        return status_failure(MmdRuntimeStatus::Error, &error.to_string());
    }

    world.world = replacement_world;
    world.pmx_model = Some(replacement_model);
    world.next_bake_sample_is_seed_only = true;
    MmdRuntimeStatus::Ok
}

#[cfg(feature = "physics-bullet-native")]
fn invalid_physics_param(message: &str) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::InvalidInput, message)
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_reset_impl(
    world: *mut MmdRuntimePhysicsWorld,
    instance: *mut MmdRuntimeInstance,
    out_seeded_rigidbody_count: *mut usize,
) -> MmdRuntimeStatus {
    use mmd_anim_physics_bullet::RuntimePhysicsBridgeExt;

    let Some(world) = (unsafe { world.as_mut() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(instance) = (unsafe { instance.as_mut() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    match world.world.reset_runtime_physics(&mut instance.runtime) {
        Ok(seeded) => {
            // Successful reset re-arms seed-only behavior for the next bake sample.
            world.next_bake_sample_is_seed_only = true;
            instance.refresh_matrix_caches();
            if !out_seeded_rigidbody_count.is_null() {
                unsafe {
                    *out_seeded_rigidbody_count = seeded;
                }
            }
            MmdRuntimeStatus::Ok
        }
        Err(err) => status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_step_runtime_impl(
    world: *mut MmdRuntimePhysicsWorld,
    instance: *mut MmdRuntimeInstance,
    dt_seconds: f32,
    out_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    use mmd_anim_physics_bullet::RuntimePhysicsBridgeExt;

    if !dt_seconds.is_finite() || dt_seconds < 0.0 {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }
    let Some(world) = (unsafe { world.as_mut() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(instance) = (unsafe { instance.as_mut() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    match world.world.step_runtime_physics_with_runtime_clock_options(
        &mut instance.runtime,
        dt_seconds,
        false,
    ) {
        Ok(report) => {
            // Explicit physics advance disarms seed-only so the next bake sample steps.
            world.next_bake_sample_is_seed_only = false;
            instance.refresh_matrix_caches();
            if !out_report.is_null() {
                unsafe {
                    *out_report = MmdRuntimeFfiPhysicsWorldStepReport {
                        tick: physics_step_stats_to_ffi(report.tick),
                        kinematic_rigidbodies_fed: report.kinematic_rigidbodies_fed,
                        bones_written_back: report.bones_written_back,
                    };
                }
            }
            MmdRuntimeStatus::Ok
        }
        Err(err) => status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
    }
}

/// Seeds or steps the physics world as part of an atomic host frame
/// evaluation.
///
/// Seed uses `initialize_runtime_physics_bake` (reset tick, reset world, seed
/// rigidbodies from bones, settle) followed by a manual readback and
/// after-physics evaluation; it never advances the Bullet solver. Step
/// requires the runtime's physics mode to be `Trace` or `Live` and uses
/// `step_runtime_physics_with_runtime_clock_options`, which already evaluates
/// the runtime's after-physics phase internally (with the default,
/// non-IK-option evaluation), so this function must not double-evaluate
/// after-physics on top of it.
#[cfg(feature = "physics-bullet-native")]
fn evaluate_host_frame_physics_impl(
    world: *mut MmdRuntimePhysicsWorld,
    instance: &mut MmdRuntimeInstance,
    action: MmdRuntimePhysicsFrameAction,
    dt_seconds: f32,
    out_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    use mmd_anim_physics_bullet::RuntimePhysicsBridgeExt;

    // Safety: world was validated non-null and compatible by
    // validate_host_frame_physics_impl before pose was applied.
    let world = unsafe { &mut *world };

    match action {
        MmdRuntimePhysicsFrameAction::Seed => {
            match world
                .world
                .initialize_runtime_physics_bake(&mut instance.runtime)
            {
                Ok(_seeded) => {
                    if let Err(err) = world.world.apply_readback_to_runtime(&mut instance.runtime) {
                        return status_failure(MmdRuntimeStatus::Error, err.to_string().as_str());
                    }
                    instance.runtime.evaluate_current_pose_after_physics();
                    // Successful seed re-arms seed-only behavior for the next bake sample.
                    world.next_bake_sample_is_seed_only = true;
                    instance.refresh_matrix_caches();
                    // A seed does not advance the solver, so the step report
                    // carries no meaningful statistics.
                    if !out_report.is_null() {
                        unsafe {
                            *out_report = MmdRuntimeFfiPhysicsWorldStepReport {
                                tick: MmdRuntimeFfiPhysicsStepStats {
                                    input_dt_seconds: 0.0,
                                    clamped_dt_seconds: 0.0,
                                    substeps: 0,
                                    accumulator_seconds: 0.0,
                                },
                                kinematic_rigidbodies_fed: 0,
                                bones_written_back: 0,
                            };
                        }
                    }
                    MmdRuntimeStatus::Ok
                }
                Err(err) => status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
            }
        }
        MmdRuntimePhysicsFrameAction::Step => {
            match world.world.step_runtime_physics_with_runtime_clock_options(
                &mut instance.runtime,
                dt_seconds,
                false,
            ) {
                Ok(report) => {
                    // Explicit physics advance disarms seed-only so the next
                    // bake sample steps.
                    world.next_bake_sample_is_seed_only = false;
                    instance.refresh_matrix_caches();
                    if !out_report.is_null() {
                        unsafe {
                            *out_report = MmdRuntimeFfiPhysicsWorldStepReport {
                                tick: physics_step_stats_to_ffi(report.tick),
                                kinematic_rigidbodies_fed: report.kinematic_rigidbodies_fed,
                                bones_written_back: report.bones_written_back,
                            };
                        }
                    }
                    MmdRuntimeStatus::Ok
                }
                Err(err) => status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
            }
        }
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_rigidbody_count_impl(
    world: *const MmdRuntimePhysicsWorld,
    out_rigidbody_count: *mut usize,
) -> MmdRuntimeStatus {
    if out_rigidbody_count.is_null() {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }
    let Some(world) = (unsafe { world.as_ref() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    match world.world.world.rigidbody_count() {
        Ok(count) => {
            unsafe {
                *out_rigidbody_count = count;
            }
            MmdRuntimeStatus::Ok
        }
        Err(err) => status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_copy_rigidbody_states_impl(
    world: *const MmdRuntimePhysicsWorld,
    out_transforms_f32: *mut f32,
    out_transforms_f32_len: usize,
) -> MmdRuntimeStatus {
    let Some(world) = (unsafe { world.as_ref() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let required_len = match world.world.rigidbody_handles.len().checked_mul(7) {
        Some(len) => len,
        None => return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT),
    };
    if out_transforms_f32.is_null() {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }
    if out_transforms_f32_len < required_len {
        return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT);
    }
    let out = unsafe { slice::from_raw_parts_mut(out_transforms_f32, required_len) };
    for (index, handle) in world.world.rigidbody_handles.iter().copied().enumerate() {
        let transform = match world.world.world.rigidbody_transform(handle) {
            Ok(transform) => transform,
            Err(err) => return status_failure(MmdRuntimeStatus::Error, err.to_string().as_str()),
        };
        let start = index * 7;
        out[start..start + 3].copy_from_slice(&transform.position);
        out[start + 3..start + 7].copy_from_slice(&transform.rotation_xyzw);
    }
    MmdRuntimeStatus::Ok
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_copy_rigidbody_bindings_impl(
    world: *const MmdRuntimePhysicsWorld,
    out_bindings: *mut MmdRuntimeFfiPhysicsRigidBodyBinding,
    capacity: usize,
    out_count: *mut usize,
) -> MmdRuntimeStatus {
    let Some(world) = (unsafe { world.as_ref() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let bindings = &world.world.rigidbody_bindings;
    if !out_count.is_null() {
        unsafe {
            *out_count = bindings.len();
        }
    }
    if bindings.len() > capacity {
        return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT);
    }
    if capacity > 0 {
        if out_bindings.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let out = unsafe { slice::from_raw_parts_mut(out_bindings, bindings.len()) };
        for (slot, binding) in out.iter_mut().zip(bindings.iter()) {
            *slot = MmdRuntimeFfiPhysicsRigidBodyBinding {
                bone_index: binding.bone_index.map(|i| i as i32).unwrap_or(-1),
                mode: rigidbody_mode_to_ffi(binding.mode),
            };
        }
    }
    MmdRuntimeStatus::Ok
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_physics_driven_bone_mask_impl(
    world: *const MmdRuntimePhysicsWorld,
    out_mask: *mut u8,
    bone_count: usize,
) -> MmdRuntimeStatus {
    let Some(world) = (unsafe { world.as_ref() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let required = world.world.required_bone_count();
    if bone_count < required {
        return status_failure(
            MmdRuntimeStatus::BufferTooSmall,
            "bone mask buffer too short for physics world bindings",
        );
    }
    if bone_count == 0 {
        return MmdRuntimeStatus::Ok;
    }
    if out_mask.is_null() {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }
    let out = unsafe { slice::from_raw_parts_mut(out_mask, bone_count) };
    out.fill(0);
    for binding in &world.world.rigidbody_bindings {
        if !binding.mode.writes_back_to_bone() {
            continue;
        }
        let Some(bone_index) = binding.bone_index else {
            continue;
        };
        if bone_index < bone_count {
            out[bone_index] = 1;
        }
    }
    MmdRuntimeStatus::Ok
}

#[cfg(feature = "physics-bullet-native")]
fn rigidbody_mode_to_ffi(mode: mmd_anim_physics_bullet::PmxRigidBodyMode) -> u32 {
    use mmd_anim_physics_bullet::PmxRigidBodyMode;

    match mode {
        PmxRigidBodyMode::Static => 0,
        PmxRigidBodyMode::Dynamic => 1,
        PmxRigidBodyMode::DynamicBone => 2,
        PmxRigidBodyMode::Unknown => 3,
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_get_gravity_impl(
    world: &MmdRuntimePhysicsWorld,
    out_gravity_xyz: *mut f32,
) -> MmdRuntimeStatus {
    match world.world.world.gravity() {
        Ok(gravity) => {
            unsafe {
                let out = slice::from_raw_parts_mut(out_gravity_xyz, 3);
                out.copy_from_slice(&gravity);
            }
            MmdRuntimeStatus::Ok
        }
        Err(e) => status_failure(MmdRuntimeStatus::Error, &e.to_string()),
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_set_gravity_impl(
    world: &mut MmdRuntimePhysicsWorld,
    gravity_xyz: *const f32,
) -> MmdRuntimeStatus {
    let gravity = unsafe { slice::from_raw_parts(gravity_xyz, 3) };
    if !all_finite(gravity) {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }
    match world
        .world
        .world
        .set_gravity([gravity[0], gravity[1], gravity[2]])
    {
        Ok(()) => MmdRuntimeStatus::Ok,
        Err(e) => status_failure(MmdRuntimeStatus::Error, &e.to_string()),
    }
}

#[cfg(feature = "physics-bullet-native")]
fn physics_rigidbody_desc_from_ffi(
    desc: &MmdRuntimeFfiPhysicsRigidBodyDesc,
) -> Option<mmd_anim_physics_bullet::PhysicsRigidBodyDescriptor> {
    use mmd_anim_physics_bullet::{
        PhysicsRigidBodyDescriptor, PmxRigidBodyBinding, PmxRigidBodyMode, RigidBodyDesc,
        RigidBodyShape, Transform,
    };

    let scalar_values = [
        desc.mass,
        desc.linear_damping,
        desc.angular_damping,
        desc.friction,
        desc.restitution,
    ];
    if !all_finite(&desc.shape_size)
        || !all_finite(&desc.position_xyz)
        || !all_finite(&desc.rotation_euler_xyz)
        || !all_finite(&scalar_values)
        || !all_finite(&desc.body_from_bone_position_xyz)
        || !all_finite(&desc.body_from_bone_rotation_xyzw)
        || !all_finite(&desc.bone_from_body_position_xyz)
        || !all_finite(&desc.bone_from_body_rotation_xyzw)
    {
        return None;
    }

    let shape = match desc.shape {
        0 if desc.shape_size[0] >= 0.0 => RigidBodyShape::Sphere {
            radius: desc.shape_size[0],
        },
        1 if desc.shape_size.iter().all(|value| *value >= 0.0) => RigidBodyShape::Box {
            half_extents: desc.shape_size,
        },
        2 if desc.shape_size[0] >= 0.0 && desc.shape_size[1] >= 0.0 => RigidBodyShape::Capsule {
            radius: desc.shape_size[0],
            height: desc.shape_size[1],
        },
        _ => return None,
    };
    let mode = match desc.mode {
        0 => PmxRigidBodyMode::Static,
        1 => PmxRigidBodyMode::Dynamic,
        2 => PmxRigidBodyMode::DynamicBone,
        3 => PmxRigidBodyMode::Unknown,
        _ => return None,
    };

    Some(PhysicsRigidBodyDescriptor {
        rigidbody: RigidBodyDesc {
            shape,
            position: desc.position_xyz,
            rotation_euler: desc.rotation_euler_xyz,
            mass: if mode == PmxRigidBodyMode::Static {
                0.0
            } else {
                desc.mass
            },
            linear_damping: desc.linear_damping,
            angular_damping: desc.angular_damping,
            friction: desc.friction,
            restitution: desc.restitution,
            collision_group: desc.collision_group,
            collision_mask: desc.collision_mask,
        },
        binding: PmxRigidBodyBinding {
            bone_index: if desc.bone_index >= 0 {
                Some(desc.bone_index as usize)
            } else {
                None
            },
            mode,
            body_from_bone: Transform {
                position: desc.body_from_bone_position_xyz,
                rotation_xyzw: desc.body_from_bone_rotation_xyzw,
            },
            bone_from_body: Transform {
                position: desc.bone_from_body_position_xyz,
                rotation_xyzw: desc.bone_from_body_rotation_xyzw,
            },
        },
    })
}

#[cfg(feature = "physics-bullet-native")]
fn physics_joint_desc_from_ffi(
    desc: &MmdRuntimeFfiPhysicsJointDesc,
) -> Option<mmd_anim_physics_bullet::PhysicsJointDescriptor> {
    use mmd_anim_physics_bullet::{PhysicsJointDescriptor, PhysicsJointKind};

    if !all_finite(&desc.position_xyz)
        || !all_finite(&desc.rotation_euler_xyz)
        || !all_finite(&desc.translation_lower_limit_xyz)
        || !all_finite(&desc.translation_upper_limit_xyz)
        || !all_finite(&desc.rotation_lower_limit_xyz)
        || !all_finite(&desc.rotation_upper_limit_xyz)
        || !all_finite(&desc.spring_translation_factor_xyz)
        || !all_finite(&desc.spring_rotation_factor_xyz)
    {
        return None;
    }
    let kind = match desc.kind {
        0 => PhysicsJointKind::Generic6DofSpring,
        1 => PhysicsJointKind::Unsupported,
        _ => return None,
    };
    Some(PhysicsJointDescriptor {
        kind,
        rigidbody_a: desc.rigidbody_a,
        rigidbody_b: desc.rigidbody_b,
        position: desc.position_xyz,
        rotation_euler: desc.rotation_euler_xyz,
        translation_lower_limit: desc.translation_lower_limit_xyz,
        translation_upper_limit: desc.translation_upper_limit_xyz,
        rotation_lower_limit: desc.rotation_lower_limit_xyz,
        rotation_upper_limit: desc.rotation_upper_limit_xyz,
        spring_translation_factor: desc.spring_translation_factor_xyz,
        spring_rotation_factor: desc.spring_rotation_factor_xyz,
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

        let Some(world_range) = checked_pointer_range(out_world_matrices_f32, required_world_len)
        else {
            return false;
        };
        let Some(morph_range) = checked_pointer_range(out_morph_weights_f32, required_morph_len)
        else {
            return false;
        };
        if pointer_ranges_overlap(world_range, morph_range) {
            return false;
        }
        let out_world = unsafe { checked_mut_slice(out_world_matrices_f32, required_world_len) }
            .expect("output range was validated");
        let out_morph = unsafe { checked_mut_slice(out_morph_weights_f32, required_morph_len) }
            .expect("output range was validated");

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

fn reduction_target_from_u32(value: u32) -> Option<ReductionTarget> {
    match value {
        0 => Some(ReductionTarget::LinearSlerp),
        1 => Some(ReductionTarget::VmdBezier),
        2 => Some(ReductionTarget::DccCubic),
        _ => None,
    }
}

fn ffi_reduction_report(report: PoseReductionReport) -> MmdRuntimeFfiPoseReductionReport {
    MmdRuntimeFfiPoseReductionReport {
        source_bone_key_count: report.source_bone_key_count,
        reduced_bone_key_count: report.reduced_bone_key_count,
        source_morph_key_count: report.source_morph_key_count,
        reduced_morph_key_count: report.reduced_morph_key_count,
        max_local_position_error: report.max_local_position_error,
        max_local_rotation_error_radians: report.max_local_rotation_error_radians,
        max_world_position_error: report.max_world_position_error,
        max_world_rotation_error_radians: report.max_world_rotation_error_radians,
        max_morph_weight_error: report.max_morph_weight_error,
    }
}

/// Reduces caller-owned dense batch output into an opaque sparse pose handle.
///
/// `world_matrices_f32` uses `[frame][bone][16]` column-major layout and
/// `morph_weights_f32` uses `[frame][morph]`. The model supplies the immutable
/// skeleton snapshot. On failure `*out_reduced_pose` is set to null.
///
/// # Safety
///
/// All non-empty input regions must be readable for their declared lengths.
/// `out_reduced_pose` must point to writable handle storage. The returned
/// handle must be released with `mmd_runtime_reduced_pose_free`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_create_from_dense(
    model: *const MmdRuntimeModel,
    model_identity: u64,
    world_matrices_f32: *const f32,
    world_matrices_f32_len: usize,
    morph_weights_f32: *const f32,
    morph_weights_f32_len: usize,
    frame_count: usize,
    start_frame: f32,
    frame_step: f32,
    target: u32,
    tolerances: MmdRuntimeFfiReductionTolerances,
    out_reduced_pose: *mut *mut MmdRuntimeReducedPose,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_reduced_pose.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        unsafe { ptr::write(out_reduced_pose, ptr::null_mut()) };
        let Some(model) = (unsafe { model.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(target) = reduction_target_from_u32(target) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let bone_count = model.model.bone_count();
        if frame_count == 0 || !morph_weights_f32_len.is_multiple_of(frame_count) {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let morph_count = morph_weights_f32_len / frame_count;
        let Some(required_world_len) = frame_count
            .checked_mul(bone_count)
            .and_then(|len| len.checked_mul(16))
        else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        let Some(required_morph_len) = frame_count.checked_mul(morph_count) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if world_matrices_f32_len != required_world_len
            || morph_weights_f32_len != required_morph_len
            || (required_world_len > 0 && world_matrices_f32.is_null())
            || (required_morph_len > 0 && morph_weights_f32.is_null())
        {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        let world_values = if required_world_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(world_matrices_f32, required_world_len) }
        };
        let morph_weights = if required_morph_len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(morph_weights_f32, required_morph_len) }
        };
        let world_matrices = world_values
            .chunks_exact(16)
            .map(glam::Mat4::from_cols_slice)
            .collect::<Vec<_>>();
        let snapshot = match SkeletonSnapshot::from_model_with_morph_count(
            &model.model,
            model_identity,
            morph_count,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return status_failure(MmdRuntimeStatus::InvalidInput, &error.to_string());
            }
        };
        let dense = match DensePoseSequenceView::new(
            &world_matrices,
            morph_weights,
            frame_count,
            bone_count,
            morph_count,
            start_frame,
            frame_step,
        ) {
            Ok(dense) => dense,
            Err(error) => {
                return status_failure(MmdRuntimeStatus::InvalidInput, &error.to_string());
            }
        };
        let tolerances = ReductionTolerances {
            local_position: tolerances.local_position,
            local_rotation_radians: tolerances.local_rotation_radians,
            world_position: tolerances.world_position,
            world_rotation_radians: tolerances.world_rotation_radians,
            morph_weight: tolerances.morph_weight,
        };
        let sequence =
            match mmd_anim_runtime::reduce_dense_pose_sequence(dense, snapshot, tolerances, target)
            {
                Ok(sequence) => sequence,
                Err(error) => {
                    return status_failure(MmdRuntimeStatus::InvalidInput, &error.to_string());
                }
            };
        unsafe {
            ptr::write(
                out_reduced_pose,
                Box::into_raw(Box::new(MmdRuntimeReducedPose {
                    sequence,
                    unity_curve_cache: RefCell::new(None),
                })),
            )
        };
        MmdRuntimeStatus::Ok
    })
}

/// Releases a reduced-pose handle. Null is accepted.
///
/// # Safety
///
/// `pose` must be null or a live handle returned by the create function, and
/// each non-null handle may be freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_free(pose: *mut MmdRuntimeReducedPose) {
    ffi_guard_void(|| {
        if !pose.is_null() {
            drop(unsafe { Box::from_raw(pose) });
        }
    });
}

/// Returns the reduced pose bone count, or zero for null.
///
/// # Safety
///
/// `pose` must be null or a live reduced-pose handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_bone_count(
    pose: *const MmdRuntimeReducedPose,
) -> usize {
    ffi_guard(0, || {
        unsafe { pose.as_ref() }
            .map(|pose| pose.sequence.snapshot().bone_count())
            .unwrap_or(0)
    })
}

/// Returns the reduced pose morph count, or zero for null.
///
/// # Safety
///
/// `pose` must be null or a live reduced-pose handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_morph_count(
    pose: *const MmdRuntimeReducedPose,
) -> usize {
    ffi_guard(0, || {
        unsafe { pose.as_ref() }
            .map(|pose| pose.sequence.snapshot().morph_count())
            .unwrap_or(0)
    })
}

/// Copies the immutable reduction report.
///
/// # Safety
///
/// `pose` must be a live reduced-pose handle and `out_report` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_report(
    pose: *const MmdRuntimeReducedPose,
    out_report: *mut MmdRuntimeFfiPoseReductionReport,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        let Some(pose) = (unsafe { pose.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if out_report.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        unsafe { ptr::write(out_report, ffi_reduction_report(pose.sequence.report())) };
        MmdRuntimeStatus::Ok
    })
}

fn validate_unity_curve_request(
    pose: &MmdRuntimeReducedPose,
    frames_per_second: f32,
) -> Result<(), MmdRuntimeStatus> {
    if !frames_per_second.is_finite() || frames_per_second <= 0.0 {
        return Err(status_failure(
            MmdRuntimeStatus::InvalidInput,
            "frames per second must be finite and greater than zero",
        ));
    }
    if pose.sequence.target() != ReductionTarget::DccCubic {
        return Err(status_failure(
            MmdRuntimeStatus::Unsupported,
            "Unity curve enumeration requires a DccCubic reduced pose",
        ));
    }
    Ok(())
}

fn parse_unity_curve_flip_z(raw: u8) -> Result<bool, MmdRuntimeStatus> {
    match raw {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(status_failure(
            MmdRuntimeStatus::InvalidInput,
            "flip_z must be 0 or 1",
        )),
    }
}

fn unity_curve_count(sequence: &ReducedPoseSequence) -> Option<usize> {
    sequence
        .bone_tracks()
        .len()
        .checked_mul(6)
        .and_then(|bone_curves| bone_curves.checked_add(sequence.morph_tracks().len()))
}

fn unity_curve_descriptor(
    sequence: &ReducedPoseSequence,
    curve_index: usize,
) -> Option<MmdRuntimeFfiUnityCurveDescriptor> {
    let bone_curve_count = sequence.bone_tracks().len().checked_mul(6)?;
    if curve_index < bone_curve_count {
        let target = curve_index / 6;
        let channel = curve_index % 6;
        let target_index = u32::try_from(target).ok()?;
        let key_count = sequence.bone_tracks()[target].keys().len();
        let (semantic, axis) = if channel < 3 {
            (MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_TRANSLATION, channel)
        } else {
            (MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_EULER, channel - 3)
        };
        return Some(MmdRuntimeFfiUnityCurveDescriptor {
            semantic,
            target_index,
            axis: axis as u32,
            key_count,
        });
    }
    let target = curve_index.checked_sub(bone_curve_count)?;
    let track = sequence.morph_tracks().get(target)?;
    Some(MmdRuntimeFfiUnityCurveDescriptor {
        semantic: MMD_RUNTIME_UNITY_CURVE_MORPH_WEIGHT,
        target_index: u32::try_from(target).ok()?,
        axis: MMD_RUNTIME_UNITY_CURVE_AXIS_NONE,
        key_count: track.keys().len(),
    })
}

fn unity_clip_for_reduced_pose(
    sequence: &ReducedPoseSequence,
    frames_per_second: f32,
    flip_z: bool,
) -> Result<UnityAnimationClipDto, MmdRuntimeStatus> {
    let bindings = UnityReducedPoseBindings {
        model_identity: sequence.snapshot().model_identity(),
        bone_paths: vec![String::new(); sequence.snapshot().bone_count()],
        morph_bindings: (0..sequence.snapshot().morph_count())
            .map(|_| {
                Some(UnityMorphBinding {
                    path: String::new(),
                    property: String::new(),
                })
            })
            .collect(),
    };
    reduced_pose_to_unity_animation_clip_with_fps(sequence, &bindings, frames_per_second, flip_z)
        .map_err(|error| status_failure(MmdRuntimeStatus::InvalidInput, &error.to_string()))
}

fn ensure_unity_curve_cache(
    pose: &MmdRuntimeReducedPose,
    frames_per_second: f32,
    flip_z: bool,
) -> Result<(), MmdRuntimeStatus> {
    validate_unity_curve_request(pose, frames_per_second)?;
    let cache_matches = pose
        .unity_curve_cache
        .borrow()
        .as_ref()
        .is_some_and(|cache| {
            cache.frames_per_second_bits == frames_per_second.to_bits() && cache.flip_z == flip_z
        });
    if cache_matches {
        return Ok(());
    }
    let clip = unity_clip_for_reduced_pose(&pose.sequence, frames_per_second, flip_z)?;
    *pose.unity_curve_cache.borrow_mut() = Some(MmdRuntimeUnityCurveCache {
        frames_per_second_bits: frames_per_second.to_bits(),
        flip_z,
        clip,
    });
    Ok(())
}

/// Returns the number of target-native Unity scalar curves.
///
/// # Safety
///
/// `pose` must be a live reduced-pose handle, `flip_z` must be 0 or 1, and
/// `out_curve_count` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_unity_curve_count(
    pose: *const MmdRuntimeReducedPose,
    frames_per_second: f32,
    flip_z: u8,
    out_curve_count: *mut usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_curve_count.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        unsafe { ptr::write(out_curve_count, 0) };
        let flip_z = match parse_unity_curve_flip_z(flip_z) {
            Ok(flip_z) => flip_z,
            Err(status) => return status,
        };
        let Some(pose) = (unsafe { pose.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if let Err(status) = ensure_unity_curve_cache(pose, frames_per_second, flip_z) {
            return status;
        }
        let Some(count) = unity_curve_count(&pose.sequence) else {
            return status_failure(MmdRuntimeStatus::Error, "Unity curve count overflow");
        };
        unsafe { ptr::write(out_curve_count, count) };
        MmdRuntimeStatus::Ok
    })
}

/// Copies one Unity scalar-curve descriptor.
///
/// Curves are ordered as translation XYZ then Euler XYZ for every bone,
/// followed by one weight curve for every morph.
///
/// # Safety
///
/// `pose` must be a live reduced-pose handle, `flip_z` must be 0 or 1, and
/// `out_descriptor` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_unity_curve_descriptor(
    pose: *const MmdRuntimeReducedPose,
    frames_per_second: f32,
    flip_z: u8,
    curve_index: usize,
    out_descriptor: *mut MmdRuntimeFfiUnityCurveDescriptor,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_descriptor.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        unsafe { ptr::write(out_descriptor, MmdRuntimeFfiUnityCurveDescriptor::default()) };
        let flip_z = match parse_unity_curve_flip_z(flip_z) {
            Ok(flip_z) => flip_z,
            Err(status) => return status,
        };
        let Some(pose) = (unsafe { pose.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if let Err(status) = ensure_unity_curve_cache(pose, frames_per_second, flip_z) {
            return status;
        }
        let Some(descriptor) = unity_curve_descriptor(&pose.sequence, curve_index) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, "curve index out of range");
        };
        unsafe { ptr::write(out_descriptor, descriptor) };
        MmdRuntimeStatus::Ok
    })
}

/// Copies one Unity scalar curve into a caller-owned key buffer.
///
/// `out_required_count` is always written after request validation. A null or
/// short key buffer returns `BUFFER_TOO_SMALL` with the required count, which
/// provides the first stage of the two-call retrieval pattern.
///
/// # Safety
///
/// `pose` must be a live reduced-pose handle and `flip_z` must be 0 or 1.
/// `out_required_count` must be writable. A non-empty key output region must
/// be writable and must not alias `out_required_count`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_reduced_pose_unity_curve_keys(
    pose: *const MmdRuntimeReducedPose,
    frames_per_second: f32,
    flip_z: u8,
    curve_index: usize,
    out_keys: *mut MmdRuntimeFfiUnityCurveKey,
    out_key_capacity: usize,
    out_required_count: *mut usize,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        if out_required_count.is_null() {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        }
        unsafe { ptr::write(out_required_count, 0) };
        let flip_z = match parse_unity_curve_flip_z(flip_z) {
            Ok(flip_z) => flip_z,
            Err(status) => return status,
        };
        let Some(pose) = (unsafe { pose.as_ref() }) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
        };
        if let Err(status) = ensure_unity_curve_cache(pose, frames_per_second, flip_z) {
            return status;
        }
        let Some(descriptor) = unity_curve_descriptor(&pose.sequence, curve_index) else {
            return status_failure(MmdRuntimeStatus::InvalidInput, "curve index out of range");
        };
        unsafe { ptr::write(out_required_count, descriptor.key_count) };
        if out_key_capacity < descriptor.key_count || out_keys.is_null() {
            return status_failure(MmdRuntimeStatus::BufferTooSmall, "output buffer too small");
        }
        let cache = pose.unity_curve_cache.borrow();
        let Some(curve) = cache
            .as_ref()
            .and_then(|cache| cache.clip.curves.get(curve_index))
        else {
            return status_failure(MmdRuntimeStatus::Error, "Unity curve mapping mismatch");
        };
        if curve.keys.len() != descriptor.key_count {
            return status_failure(MmdRuntimeStatus::Error, "Unity curve key count mismatch");
        }
        for (index, key) in curve.keys.iter().enumerate() {
            unsafe {
                out_keys.add(index).write(MmdRuntimeFfiUnityCurveKey {
                    time_seconds: key.time_seconds,
                    value: key.value,
                    in_tangent: key.in_tangent,
                    out_tangent: key.out_tangent,
                });
            }
        }
        MmdRuntimeStatus::Ok
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

/// Sequentially bakes clip frames through a stateful physics world.
///
/// Unlike `mmd_runtime_instance_evaluate_clip_frame_batch`, this mutates the
/// supplied runtime instance and physics world in frame order. `frame_step`
/// advances clip sampling; `dt_seconds` advances the physics clock.
///
/// # Seed-only first sample
///
/// After physics world creation or a successful
/// `mmd_runtime_physics_world_reset`, the next bake sample is **seed-only**:
/// the clip frame is evaluated, the Bullet world is reset/reseeded from that
/// pose (including a physics tick reset, but no solver settle), outputs are
/// copied, and the normal forward physics clock is **not** advanced. That
/// sample disarms the seed-only state. Later samples in
/// the same or subsequent bake calls use evaluate → step → copy.
///
/// A continuation bake call without an intervening successful reset (or after
/// a successful `mmd_runtime_physics_world_step_runtime`) does **not** skip its
/// first sample. `frame_count == 0` does not consume or disarm the seed-only
/// state. `out_last_report` for a one-sample seed-only bake remains the default
/// zero report; for multi-sample bakes it reports the final actual physics step.
///
/// # Safety
///
/// `world`, `instance`, and `clip` must be valid handles. Non-empty output
/// regions must point to writable buffers of at least the corresponding `*_len`
/// count and must not alias each other. `out_last_report` may be null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mmd_runtime_physics_world_bake_clip_frames(
    world: *mut MmdRuntimePhysicsWorld,
    instance: *mut MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    start_frame: f32,
    frame_step: f32,
    dt_seconds: f32,
    frame_count: usize,
    out_world_matrices_f32: *mut f32,
    out_world_matrices_f32_len: usize,
    out_morph_weights_f32: *mut f32,
    out_morph_weights_f32_len: usize,
    out_last_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    ffi_guard(MmdRuntimeStatus::Error, || {
        physics_world_bake_clip_frames_impl(
            world,
            instance,
            clip,
            start_frame,
            frame_step,
            dt_seconds,
            frame_count,
            out_world_matrices_f32,
            out_world_matrices_f32_len,
            out_morph_weights_f32,
            out_morph_weights_f32_len,
            out_last_report,
        )
    })
}

#[cfg(not(feature = "physics-bullet-native"))]
#[allow(clippy::too_many_arguments)]
fn physics_world_bake_clip_frames_impl(
    _world: *mut MmdRuntimePhysicsWorld,
    _instance: *mut MmdRuntimeInstance,
    _clip: *const MmdRuntimeClip,
    _start_frame: f32,
    _frame_step: f32,
    _dt_seconds: f32,
    _frame_count: usize,
    _out_world_matrices_f32: *mut f32,
    _out_world_matrices_f32_len: usize,
    _out_morph_weights_f32: *mut f32,
    _out_morph_weights_f32_len: usize,
    _out_last_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    status_failure(MmdRuntimeStatus::Unsupported, "physics backend unsupported")
}

#[cfg(feature = "physics-bullet-native")]
#[allow(clippy::too_many_arguments)]
fn physics_world_bake_clip_frames_impl(
    world: *mut MmdRuntimePhysicsWorld,
    instance: *mut MmdRuntimeInstance,
    clip: *const MmdRuntimeClip,
    start_frame: f32,
    frame_step: f32,
    dt_seconds: f32,
    frame_count: usize,
    out_world_matrices_f32: *mut f32,
    out_world_matrices_f32_len: usize,
    out_morph_weights_f32: *mut f32,
    out_morph_weights_f32_len: usize,
    out_last_report: *mut MmdRuntimeFfiPhysicsWorldStepReport,
) -> MmdRuntimeStatus {
    use mmd_anim_physics_bullet::RuntimePhysicsBridgeExt;

    let Some(world) = (unsafe { world.as_mut() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(instance) = (unsafe { instance.as_mut() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(clip) = (unsafe { clip.as_ref() }) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    if !start_frame.is_finite()
        || !frame_step.is_finite()
        || !dt_seconds.is_finite()
        || dt_seconds < 0.0
    {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }

    let world_frame_len = match instance.runtime.world_matrices().len().checked_mul(16) {
        Some(len) => len,
        None => return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT),
    };
    let morph_frame_len = instance.runtime.morph_weights().len();
    let required_world_len = match world_frame_len.checked_mul(frame_count) {
        Some(len) => len,
        None => return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT),
    };
    let required_morph_len = match morph_frame_len.checked_mul(frame_count) {
        Some(len) => len,
        None => return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT),
    };

    if out_world_matrices_f32_len < required_world_len
        || out_morph_weights_f32_len < required_morph_len
    {
        return status_failure(MmdRuntimeStatus::BufferTooSmall, FFI_ERR_INVALID_INPUT);
    }
    if required_world_len > 0 && out_world_matrices_f32.is_null() {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }
    if required_morph_len > 0 && out_morph_weights_f32.is_null() {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }

    let Some(world_range) = checked_pointer_range(out_world_matrices_f32, required_world_len)
    else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    let Some(morph_range) = checked_pointer_range(out_morph_weights_f32, required_morph_len) else {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    };
    if pointer_ranges_overlap(world_range, morph_range) {
        return status_failure(MmdRuntimeStatus::InvalidInput, FFI_ERR_INVALID_INPUT);
    }
    let out_world = unsafe { checked_mut_slice(out_world_matrices_f32, required_world_len) }
        .expect("output range was validated");
    let out_morph = unsafe { checked_mut_slice(out_morph_weights_f32, required_morph_len) }
        .expect("output range was validated");

    let mut last_report = MmdRuntimeFfiPhysicsWorldStepReport {
        tick: physics_step_stats_to_ffi(PhysicsStepStats::default()),
        kinematic_rigidbodies_fed: 0,
        bones_written_back: 0,
    };
    for frame_index in 0..frame_count {
        let frame = start_frame + frame_step * frame_index as f32;
        instance
            .runtime
            .evaluate_clip_frame_before_physics(&clip.clip, frame);

        if world.next_bake_sample_is_seed_only {
            // Initial seed-only sample: reset/reseed Bullet from the evaluated
            // pose and reset the physics tick, then copy without advancing the
            // solver or the normal forward physics clock.
            if let Err(err) = world
                .world
                .initialize_runtime_physics_bake(&mut instance.runtime)
            {
                return status_failure(MmdRuntimeStatus::Error, err.to_string().as_str());
            }
            world.next_bake_sample_is_seed_only = false;
            // Keep last_report as the default zero report for this sample.
        } else {
            let report = match world.world.step_runtime_physics_with_runtime_clock_options(
                &mut instance.runtime,
                dt_seconds,
                false,
            ) {
                Ok(report) => report,
                Err(err) => {
                    return status_failure(MmdRuntimeStatus::Error, err.to_string().as_str());
                }
            };
            last_report = MmdRuntimeFfiPhysicsWorldStepReport {
                tick: physics_step_stats_to_ffi(report.tick),
                kinematic_rigidbodies_fed: report.kinematic_rigidbodies_fed,
                bones_written_back: report.bones_written_back,
            };
        }

        let world_start = frame_index * world_frame_len;
        let world_end = world_start + world_frame_len;
        flatten_matrices_into_slice(
            &mut out_world[world_start..world_end],
            instance.runtime.world_matrices(),
        );
        if morph_frame_len > 0 {
            let morph_start = frame_index * morph_frame_len;
            let morph_end = morph_start + morph_frame_len;
            out_morph[morph_start..morph_end].copy_from_slice(instance.runtime.morph_weights());
        }
    }

    instance.refresh_matrix_caches();
    if !out_last_report.is_null() {
        unsafe {
            *out_last_report = last_report;
        }
    }
    MmdRuntimeStatus::Ok
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
        if output_overlaps_instance_views(instance, out_f32, required_len) {
            return false;
        }

        let Some(out) = (unsafe { checked_mut_slice(out_f32, required_len) }) else {
            return false;
        };
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
        if output_overlaps_instance_views(instance, out_f32, required_len) {
            return false;
        }

        let Some(out) = (unsafe { checked_mut_slice(out_f32, required_len) }) else {
            return false;
        };
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
        if output_overlaps_instance_views(instance, out_f32, weights.len()) {
            return false;
        }
        let Some(out) = (unsafe { checked_mut_slice(out_f32, weights.len()) }) else {
            return false;
        };
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
        if output_overlaps_instance_views(instance, out_u8, states.len()) {
            return false;
        }
        let Some(out) = (unsafe { checked_mut_slice(out_u8, states.len()) }) else {
            return false;
        };
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
    checked_pointer_range(ptr, len)?;
    if len == 0 {
        return Some(&[]);
    }
    Some(unsafe { slice::from_raw_parts(ptr, len) })
}

unsafe fn checked_mut_slice<'a, T>(ptr: *mut T, len: usize) -> Option<&'a mut [T]> {
    checked_pointer_range(ptr, len)?;
    if len == 0 {
        return Some(&mut []);
    }
    Some(unsafe { slice::from_raw_parts_mut(ptr, len) })
}

fn checked_pointer_range<T>(ptr: *const T, len: usize) -> Option<(usize, usize)> {
    if len == 0 {
        return Some((0, 0));
    }
    if ptr.is_null() {
        return None;
    }
    let start = ptr as usize;
    if !start.is_multiple_of(std::mem::align_of::<T>()) {
        return None;
    }
    let byte_len = len.checked_mul(std::mem::size_of::<T>())?;
    if byte_len > isize::MAX as usize {
        return None;
    }
    Some((start, start.checked_add(byte_len)?))
}

fn pointer_ranges_overlap(left: (usize, usize), right: (usize, usize)) -> bool {
    left.0 < left.1 && right.0 < right.1 && left.0 < right.1 && right.0 < left.1
}

fn output_overlaps_instance_views<T>(
    instance: &MmdRuntimeInstance,
    output: *const T,
    output_len: usize,
) -> bool {
    let Some(output_range) = checked_pointer_range(output, output_len) else {
        return true;
    };
    let view_ranges = [
        checked_pointer_range(
            instance.cached_world_matrices.as_ptr(),
            instance.cached_world_matrices.len(),
        ),
        checked_pointer_range(
            instance.cached_skinning_matrices.as_ptr(),
            instance.cached_skinning_matrices.len(),
        ),
        checked_pointer_range(
            instance.runtime.morph_weights().as_ptr(),
            instance.runtime.morph_weights().len(),
        ),
        checked_pointer_range(
            instance.runtime.ik_enabled().as_ptr(),
            instance.runtime.ik_enabled().len(),
        ),
    ];
    view_ranges
        .into_iter()
        .flatten()
        .any(|view_range| pointer_ranges_overlap(output_range, view_range))
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
            let has_angle_limit = parse_ffi_bool(link.has_angle_limit)?;
            let angle_limit = if has_angle_limit {
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
        let has_local_axis = parse_ffi_bool(axis.has_local_axis)?;
        if !has_local_axis {
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

fn parse_ffi_bool(value: u8) -> Option<bool> {
    match value {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

fn descriptor_failure<T>(message: impl AsRef<str>) -> Option<T> {
    set_last_error(message);
    None
}

unsafe fn copy_descriptor_array<T: Copy>(
    path: &str,
    ptr: *const T,
    count: usize,
) -> Option<Vec<T>> {
    if ptr.is_null() != (count == 0) {
        return descriptor_failure(format!(
            "{path}: pointer/count mismatch (pointer is {}, count is {count})",
            if ptr.is_null() { "null" } else { "non-null" }
        ));
    }
    if count > u32::MAX as usize {
        return descriptor_failure(format!("{path}: count exceeds u32::MAX"));
    }
    let Some(bytes) = count.checked_mul(std::mem::size_of::<T>()) else {
        return descriptor_failure(format!("{path}: count byte-size overflow"));
    };
    if bytes > isize::MAX as usize {
        return descriptor_failure(format!("{path}: count byte-size exceeds isize::MAX"));
    }
    if !ptr.is_null() {
        let address = ptr as usize;
        if !address.is_multiple_of(std::mem::align_of::<T>()) {
            return descriptor_failure(format!("{path}: pointer is misaligned"));
        }
        if address.checked_add(bytes).is_none() {
            return descriptor_failure(format!("{path}: pointer range overflows usize"));
        }
    }
    if count == 0 {
        return Some(Vec::new());
    }
    Some(unsafe { slice::from_raw_parts(ptr, count) }.to_vec())
}

unsafe fn build_model_from_descriptor_ffi(
    descriptor: *const MmdRuntimeModelDescriptor,
) -> Option<MmdRuntimeModel> {
    if descriptor.is_null() {
        return descriptor_failure("descriptor: null pointer");
    }
    let address = descriptor as usize;
    if !address.is_multiple_of(std::mem::align_of::<MmdRuntimeModelDescriptor>()) {
        return descriptor_failure("descriptor: pointer is misaligned");
    }
    let descriptor = unsafe { &*descriptor };
    let expected_size = std::mem::size_of::<MmdRuntimeModelDescriptor>();
    if descriptor.struct_size as usize != expected_size {
        return descriptor_failure(format!(
            "descriptor.struct_size: expected {expected_size}, got {}",
            descriptor.struct_size
        ));
    }
    if descriptor.descriptor_version != MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1 {
        return descriptor_failure(format!(
            "descriptor.descriptor_version: expected {}, got {}",
            MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1, descriptor.descriptor_version
        ));
    }
    if descriptor.flags != 0 {
        return descriptor_failure(format!(
            "descriptor.flags: unknown bits 0x{:08x}",
            descriptor.flags
        ));
    }
    if descriptor.reserved != 0 {
        return descriptor_failure(format!(
            "descriptor.reserved: expected zero, got {}",
            descriptor.reserved
        ));
    }

    let bones = unsafe {
        copy_descriptor_array("descriptor.bones", descriptor.bones, descriptor.bone_count)
    }?;
    let ik_solvers = unsafe {
        copy_descriptor_array(
            "descriptor.ik_solvers",
            descriptor.ik_solvers,
            descriptor.ik_solver_count,
        )
    }?;
    let ik_links = unsafe {
        copy_descriptor_array(
            "descriptor.ik_links",
            descriptor.ik_links,
            descriptor.ik_link_count,
        )
    }?;
    let append_transforms = unsafe {
        copy_descriptor_array(
            "descriptor.append_transforms",
            descriptor.append_transforms,
            descriptor.append_transform_count,
        )
    }?;
    let bone_morph_offsets = unsafe {
        copy_descriptor_array(
            "descriptor.bone_morph_offsets",
            descriptor.bone_morph_offsets,
            descriptor.bone_morph_offset_count,
        )
    }?;
    let group_morph_offsets = unsafe {
        copy_descriptor_array(
            "descriptor.group_morph_offsets",
            descriptor.group_morph_offsets,
            descriptor.group_morph_offset_count,
        )
    }?;

    for (link_index, link) in ik_links.iter().enumerate() {
        if link.flags & !IK_LINK_FLAG_ANGLE_LIMIT != 0 {
            return descriptor_failure(format!(
                "descriptor.ik_links[{link_index}].flags: unknown bits 0x{:08x}",
                link.flags
            ));
        }
    }

    for (solver_index, solver) in ik_solvers.iter().enumerate() {
        let Some(end) = solver.link_offset.checked_add(solver.link_count) else {
            return descriptor_failure(format!(
                "descriptor.ik_solvers[{solver_index}].link_offset: offset + count overflows usize"
            ));
        };
        if end > ik_links.len() {
            return descriptor_failure(format!(
                "descriptor.ik_solvers[{solver_index}].links: range {}..{} exceeds link_count {}",
                solver.link_offset,
                end,
                ik_links.len()
            ));
        }
    }

    let mut runtime_bones = Vec::with_capacity(bones.len());
    for (bone_index, bone) in bones.iter().enumerate() {
        if bone.flags
            & !(MODEL_BONE_FLAG_TRANSFORM_AFTER_PHYSICS
                | MODEL_BONE_FLAG_FIXED_AXIS
                | MODEL_BONE_FLAG_LOCAL_AXIS)
            != 0
        {
            return descriptor_failure(format!(
                "descriptor.bones[{bone_index}].flags: unknown bits 0x{:08x}",
                bone.flags
            ));
        }
        let parent = match bone.parent_index {
            -1 => None,
            parent if parent >= 0 => Some(BoneIndex(parent as u32)),
            _ => {
                return descriptor_failure(format!(
                    "descriptor.bones[{bone_index}].parent_index: expected -1 or non-negative index"
                ));
            }
        };
        runtime_bones.push(RuntimeBoneDescriptorV1 {
            parent,
            rest_position: glam::Vec3A::from_array(bone.rest_position_xyz),
            transform_order: bone.transform_order,
            transform_after_physics: bone.flags & MODEL_BONE_FLAG_TRANSFORM_AFTER_PHYSICS != 0,
            fixed_axis: (bone.flags & MODEL_BONE_FLAG_FIXED_AXIS != 0)
                .then(|| glam::Vec3A::from_array(bone.fixed_axis_xyz)),
            local_axis: (bone.flags & MODEL_BONE_FLAG_LOCAL_AXIS != 0).then(|| LocalAxis {
                x: glam::Vec3A::from_array(bone.local_axis_x_xyz),
                z: glam::Vec3A::from_array(bone.local_axis_z_xyz),
            }),
        });
    }

    let mut runtime_solvers = Vec::with_capacity(ik_solvers.len());
    for solver in ik_solvers.iter() {
        let links = &ik_links[solver.link_offset..solver.link_offset + solver.link_count];
        let mut runtime_links = Vec::with_capacity(links.len());
        for link in links {
            runtime_links.push(RuntimeIkLinkDescriptorV1 {
                bone: BoneIndex(link.bone_index),
                angle_limit: (link.flags & IK_LINK_FLAG_ANGLE_LIMIT != 0).then(|| IkAngleLimit {
                    min: glam::Vec3A::from_array(link.angle_limit_min_xyz),
                    max: glam::Vec3A::from_array(link.angle_limit_max_xyz),
                }),
            });
        }
        runtime_solvers.push(RuntimeIkSolverDescriptorV1 {
            ik_bone: BoneIndex(solver.ik_bone_index),
            target_bone: BoneIndex(solver.target_bone_index),
            links: runtime_links,
            iteration_count: solver.iteration_count,
            limit_angle: solver.limit_angle,
        });
    }

    let mut runtime_appends = Vec::with_capacity(append_transforms.len());
    for (append_index, append) in append_transforms.iter().enumerate() {
        if append.flags & !(APPEND_FLAG_ROTATION | APPEND_FLAG_TRANSLATION | APPEND_FLAG_LOCAL) != 0
        {
            return descriptor_failure(format!(
                "descriptor.append_transforms[{append_index}].flags: unknown bits 0x{:08x}",
                append.flags
            ));
        }
        runtime_appends.push(RuntimeAppendTransformDescriptorV1 {
            target_bone: BoneIndex(append.target_bone_index),
            source_bone: BoneIndex(append.source_bone_index),
            ratio: append.ratio,
            affect_rotation: append.flags & APPEND_FLAG_ROTATION != 0,
            affect_translation: append.flags & APPEND_FLAG_TRANSLATION != 0,
            local: append.flags & APPEND_FLAG_LOCAL != 0,
        });
    }

    let runtime_bone_morph_offsets = bone_morph_offsets
        .iter()
        .map(|offset| RuntimeBoneMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(offset.morph_index),
            target_bone: BoneIndex(offset.target_bone_index),
            position_offset: glam::Vec3A::from_array(offset.position_offset_xyz),
            rotation_offset: glam::Quat::from_array(offset.rotation_offset_xyzw),
        })
        .collect();
    let runtime_group_morph_offsets = group_morph_offsets
        .iter()
        .map(|offset| RuntimeGroupMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(offset.morph_index),
            child_morph: MorphIndex(offset.child_morph_index),
            ratio: offset.ratio,
        })
        .collect();

    let runtime_descriptor = RuntimeModelDescriptorV1 {
        descriptor_version: MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1,
        bones: runtime_bones,
        ik_solvers: runtime_solvers,
        append_transforms: runtime_appends,
        morphs: RuntimeMorphDescriptorV1 {
            morph_count: descriptor.morph_count,
            bone_offsets: runtime_bone_morph_offsets,
            group_offsets: runtime_group_morph_offsets,
        },
    };
    let model = match compile_runtime_model_descriptor_v1(&runtime_descriptor) {
        Ok(model) => model,
        Err(error) => return descriptor_failure(error.to_string()),
    };
    Some(MmdRuntimeModel {
        model: Arc::new(model),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    })
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
    let inverse_bind_matrices = if input.inverse_bind_matrices.is_null() {
        &[]
    } else {
        let len = input.bone_count.checked_mul(16)?;
        unsafe { checked_slice(input.inverse_bind_matrices, len) }?
    };
    let transform_orders = if input.transform_orders.is_null() {
        &[]
    } else {
        unsafe { checked_slice(input.transform_orders, input.bone_count) }?
    };
    let ik_solvers = unsafe { checked_slice(input.ik_solvers, input.ik_solver_count) }?;
    let ik_links = unsafe { checked_slice(input.ik_links, input.ik_link_count) }?;
    let append_transforms =
        unsafe { checked_slice(input.append_transforms, input.append_transform_count) }?;
    let parents = unsafe { checked_slice(input.parent_indices, input.bone_count) }?;
    let position_len = input.bone_count.checked_mul(3)?;
    let positions = unsafe { checked_slice(input.rest_positions_xyz, position_len) }?;
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
