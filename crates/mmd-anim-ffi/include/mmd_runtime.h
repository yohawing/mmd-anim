#ifndef MMD_RUNTIME_H
#define MMD_RUNTIME_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/*  Version                                                           */
/* ------------------------------------------------------------------ */

#define MMD_RUNTIME_ABI_VERSION 2

/* ------------------------------------------------------------------ */
/*  Host physics FFI surface contract                                 */
/* ------------------------------------------------------------------ */
/*
   Capability detection
   ---------------------
   mmd_runtime_feature_flags() returns a bitmask describing what this build
   supports: bit 0 (MMD_RUNTIME_FEATURE_SPLIT_PHYSICS_EVALUATION) is set when
   the before/after-physics split evaluation API is available; bit 1
   (MMD_RUNTIME_FEATURE_PHYSICS_BULLET_NATIVE) is set when the native Bullet
   physics world is available; bit 2 (MMD_RUNTIME_FEATURE_MODEL_DESCRIPTOR)
   is set when the version 1 typed model descriptor constructor is available.
   Check the relevant bit before calling any
   physics_world_* or evaluate_host_frame function; when the bit is unset
   those functions return MMD_RUNTIME_STATUS_UNSUPPORTED.

   Handle ownership
   ----------------
   mmd_runtime_model_t* is shared and read-only after creation (Arc-backed).
   The same model may be used to create any number of instances.
   mmd_runtime_instance_t* is exclusively owned by the caller and must be
   released with mmd_runtime_instance_free.
   mmd_runtime_physics_world_t* is exclusively owned and created
   independently from PMX bytes or rigidbody/joint descriptors — it does
   not hold a reference to the model handle. The caller is responsible for
   ensuring that the bone indices used by the physics world match those of
   the instance it is paired with; index mismatches within bounds silently
   drive the wrong bones.
   Free order: release instances before releasing the model they were
   created from. Physics worlds may be freed in any order relative to
   models. Freeing a model while instances still reference it is safe (Arc
   keeps storage alive), but using any handle after it has been freed is
   undefined behavior.

   Thread safety
   -------------
   Individual handles are not thread-safe: do not call FFI functions on the
   same instance or physics world from more than one thread at a time.
   Independent (model, instance, world) triples that share no handle may be
   driven concurrently from different threads. mmd_runtime_last_error_message
   is thread-local; the returned message is valid only until the next FFI
   call made on that same thread.

   Same-frame re-evaluation
   -------------------------
   mmd_runtime_evaluate_host_frame with action = STEP advances the physics
   world's fixed-step clock. Calling it more than once for the same logical
   frame accumulates physics time as if multiple frames had elapsed; the
   caller must not double-step a frame. STEP requires the instance's physics
   mode to be Trace or Live; it returns MMD_RUNTIME_STATUS_INVALID_INPUT
   when the mode is Off. action = SEED resets rigid bodies to their
   bone-derived positions, zeroes velocities, and evaluates after-physics —
   it does not advance the solver. SEED may be called at any time to
   reinitialize physics state.

   Error recovery
   --------------
   When any function returns a status other than MMD_RUNTIME_STATUS_OK, the
   handles it was given remain valid and may be used in subsequent calls,
   which may succeed. The only functions that never fail are the handle-free
   functions.
*/

/* ------------------------------------------------------------------ */
/*  Opaque handle types                                               */
/* ------------------------------------------------------------------ */

typedef struct mmd_runtime_model_t    mmd_runtime_model_t;
typedef struct mmd_runtime_instance_t mmd_runtime_instance_t;
typedef struct mmd_runtime_clip_t     mmd_runtime_clip_t;
typedef struct mmd_runtime_pmx_geometry_t mmd_runtime_pmx_geometry_t;
typedef struct mmd_runtime_pmx_material_split_t mmd_runtime_pmx_material_split_t;
typedef struct mmd_runtime_pmx_rig_spec_t mmd_runtime_pmx_rig_spec_t;
typedef struct mmd_runtime_ik_chain_t mmd_runtime_ik_chain_t;
typedef struct mmd_runtime_append_solver_t mmd_runtime_append_solver_t;
typedef struct mmd_runtime_vmd_camera_track_t mmd_runtime_vmd_camera_track_t;
typedef struct mmd_runtime_vmd_light_track_t mmd_runtime_vmd_light_track_t;
typedef struct mmd_runtime_vmd_self_shadow_track_t mmd_runtime_vmd_self_shadow_track_t;
typedef struct mmd_runtime_physics_world_t mmd_runtime_physics_world_t;
typedef struct mmd_runtime_reduced_pose_t mmd_runtime_reduced_pose_t;

/* ------------------------------------------------------------------ */
/*  Flag constants                                                    */
/* ------------------------------------------------------------------ */

/* Append-transform flags  (bitmask) */
#define MMD_RUNTIME_APPEND_ROTATION    (1u << 0)
#define MMD_RUNTIME_APPEND_TRANSLATION (1u << 1)
#define MMD_RUNTIME_APPEND_LOCAL       (1u << 2)

/* IK link flags           (bitmask) */
#define MMD_RUNTIME_IK_LINK_ANGLE_LIMIT (1u << 0)

/* Rig primitive bone flags (bitmask) */
#define MMD_RUNTIME_RIG_BONE_FIXED_AXIS (1u << 0)

/* Runtime feature flags (bitmask) */
#define MMD_RUNTIME_FEATURE_SPLIT_PHYSICS_EVALUATION (1u << 0)
#define MMD_RUNTIME_FEATURE_PHYSICS_BULLET_NATIVE    (1u << 1)
#define MMD_RUNTIME_FEATURE_MODEL_DESCRIPTOR         (1u << 2)
#define MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1      1u
#define MMD_RUNTIME_MODEL_DESCRIPTOR_FLAGS_NONE     0u

/* ------------------------------------------------------------------ */
/*  Status and mode enums                                             */
/* ------------------------------------------------------------------ */

typedef enum mmd_runtime_status {
    MMD_RUNTIME_STATUS_OK = 0,
    MMD_RUNTIME_STATUS_INVALID_INPUT = 1,
    MMD_RUNTIME_STATUS_UNSUPPORTED = 2,
    MMD_RUNTIME_STATUS_BUFFER_TOO_SMALL = 3,
    MMD_RUNTIME_STATUS_ERROR = 4
} mmd_runtime_status_t;

typedef enum mmd_runtime_reduction_target {
    MMD_RUNTIME_REDUCTION_TARGET_LINEAR_SLERP = 0,
    MMD_RUNTIME_REDUCTION_TARGET_VMD_BEZIER = 1,
    MMD_RUNTIME_REDUCTION_TARGET_DCC_CUBIC = 2
} mmd_runtime_reduction_target_t;

typedef enum mmd_runtime_unity_curve_semantic {
    MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_TRANSLATION = 0,
    MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_EULER = 1,
    MMD_RUNTIME_UNITY_CURVE_MORPH_WEIGHT = 2
} mmd_runtime_unity_curve_semantic_t;

typedef enum mmd_runtime_unity_curve_axis {
    MMD_RUNTIME_UNITY_CURVE_AXIS_X = 0,
    MMD_RUNTIME_UNITY_CURVE_AXIS_Y = 1,
    MMD_RUNTIME_UNITY_CURVE_AXIS_Z = 2,
    MMD_RUNTIME_UNITY_CURVE_AXIS_NONE = 3
} mmd_runtime_unity_curve_axis_t;

typedef struct mmd_runtime_ffi_reduction_tolerances {
    float local_position;
    float local_rotation_radians;
    float world_position;
    float world_rotation_radians;
    float morph_weight;
} mmd_runtime_ffi_reduction_tolerances_t;

typedef struct mmd_runtime_ffi_pose_reduction_report {
    size_t source_bone_key_count;
    size_t reduced_bone_key_count;
    size_t source_morph_key_count;
    size_t reduced_morph_key_count;
    float max_local_position_error;
    float max_local_rotation_error_radians;
    float max_world_position_error;
    float max_world_rotation_error_radians;
    float max_morph_weight_error;
} mmd_runtime_ffi_pose_reduction_report_t;

typedef struct mmd_runtime_ffi_unity_curve_descriptor {
    uint32_t semantic;    /* mmd_runtime_unity_curve_semantic_t */
    uint32_t target_index; /* bone index or morph index */
    uint32_t axis;        /* mmd_runtime_unity_curve_axis_t */
    size_t   key_count;
} mmd_runtime_ffi_unity_curve_descriptor_t;

typedef struct mmd_runtime_ffi_unity_curve_key {
    float time_seconds;
    float value;
    float in_tangent;
    float out_tangent;
} mmd_runtime_ffi_unity_curve_key_t;

typedef enum mmd_runtime_physics_mode {
    MMD_RUNTIME_PHYSICS_MODE_OFF = 0,
    MMD_RUNTIME_PHYSICS_MODE_TRACE = 1,
    MMD_RUNTIME_PHYSICS_MODE_LIVE = 2
} mmd_runtime_physics_mode_t;

typedef enum mmd_runtime_physics_rigidbody_shape {
    MMD_RUNTIME_PHYSICS_RIGIDBODY_SHAPE_SPHERE = 0,
    MMD_RUNTIME_PHYSICS_RIGIDBODY_SHAPE_BOX = 1,
    MMD_RUNTIME_PHYSICS_RIGIDBODY_SHAPE_CAPSULE = 2
} mmd_runtime_physics_rigidbody_shape_t;

typedef enum mmd_runtime_physics_rigidbody_mode {
    MMD_RUNTIME_PHYSICS_RIGIDBODY_MODE_STATIC = 0,
    MMD_RUNTIME_PHYSICS_RIGIDBODY_MODE_DYNAMIC = 1,
    MMD_RUNTIME_PHYSICS_RIGIDBODY_MODE_DYNAMIC_BONE = 2,
    MMD_RUNTIME_PHYSICS_RIGIDBODY_MODE_UNKNOWN = 3
} mmd_runtime_physics_rigidbody_mode_t;

typedef enum mmd_runtime_physics_joint_kind {
    MMD_RUNTIME_PHYSICS_JOINT_KIND_GENERIC_6DOF_SPRING = 0,
    MMD_RUNTIME_PHYSICS_JOINT_KIND_UNSUPPORTED = 1
} mmd_runtime_physics_joint_kind_t;

/* Selects the physics action performed by
   mmd_runtime_evaluate_host_frame: reseed the Bullet world from the
   evaluated pose without advancing the solver (SEED places all bodies at
   their bone-derived positions and zeroes velocities), or advance the
   runtime's fixed-step physics clock forward (STEP requires physics mode
   Trace or Live; returns INVALID_INPUT when mode is Off). */
typedef enum mmd_runtime_physics_frame_action {
    MMD_RUNTIME_PHYSICS_FRAME_ACTION_SEED = 0,
    MMD_RUNTIME_PHYSICS_FRAME_ACTION_STEP = 1
} mmd_runtime_physics_frame_action_t;

/* ------------------------------------------------------------------ */
/*  Descriptor structs                                                */
/* ------------------------------------------------------------------ */

typedef struct mmd_runtime_ffi_bone_track {
    uint32_t bone_index;
    size_t   keyframe_offset;
    size_t   keyframe_count;
} mmd_runtime_ffi_bone_track_t;

typedef struct mmd_runtime_ffi_bone_keyframe {
    uint32_t frame;
    float    position_xyz[3];
    float    rotation_xyzw[4];
} mmd_runtime_ffi_bone_keyframe_t;

typedef struct mmd_runtime_ffi_morph_track {
    uint32_t morph_index;
    size_t   keyframe_offset;
    size_t   keyframe_count;
} mmd_runtime_ffi_morph_track_t;

typedef struct mmd_runtime_ffi_morph_keyframe {
    uint32_t frame;
    float    weight;
} mmd_runtime_ffi_morph_keyframe_t;

typedef struct mmd_runtime_ffi_property_keyframe {
    uint32_t frame;
    size_t   ik_enabled_offset;
    size_t   ik_enabled_count;
} mmd_runtime_ffi_property_keyframe_t;

typedef struct mmd_runtime_ffi_append_transform {
    uint32_t target_bone_index;
    uint32_t source_bone_index;
    float    ratio;
    uint32_t flags;
} mmd_runtime_ffi_append_transform_t;

typedef struct mmd_runtime_ffi_ik_solver {
    uint32_t ik_bone_index;
    uint32_t target_bone_index;
    size_t   link_offset;
    size_t   link_count;
    uint32_t iteration_count;
    float    limit_angle;
} mmd_runtime_ffi_ik_solver_t;

typedef struct mmd_runtime_ffi_ik_link {
    uint32_t bone_index;
    uint32_t flags;
    float    angle_limit_min_xyz[3];
    float    angle_limit_max_xyz[3];
} mmd_runtime_ffi_ik_link_t;

/* Version 1 typed model descriptor records.  All flags are fixed-width
   integer bitmasks; input memory is borrowed only during the constructor call. */
#define MMD_RUNTIME_MODEL_BONE_TRANSFORM_AFTER_PHYSICS (1u << 0)
#define MMD_RUNTIME_MODEL_BONE_FIXED_AXIS              (1u << 1)
#define MMD_RUNTIME_MODEL_BONE_LOCAL_AXIS              (1u << 2)
typedef struct mmd_runtime_model_bone_descriptor {
    int32_t  parent_index;
    float    rest_position_xyz[3];
    int32_t  transform_order;
    uint32_t flags;
    float    fixed_axis_xyz[3];
    float    local_axis_x_xyz[3];
    float    local_axis_z_xyz[3];
} mmd_runtime_model_bone_descriptor_t;

typedef struct mmd_runtime_model_ik_solver_descriptor {
    uint32_t ik_bone_index;
    uint32_t target_bone_index;
    size_t   link_offset;
    size_t   link_count;
    uint32_t iteration_count;
    float    limit_angle;
} mmd_runtime_model_ik_solver_descriptor_t;

#define MMD_RUNTIME_MODEL_IK_LINK_ANGLE_LIMIT (1u << 0)
typedef struct mmd_runtime_model_ik_link_descriptor {
    uint32_t bone_index;
    uint32_t flags;
    float    angle_limit_min_xyz[3];
    float    angle_limit_max_xyz[3];
} mmd_runtime_model_ik_link_descriptor_t;

typedef struct mmd_runtime_model_append_descriptor {
    uint32_t target_bone_index;
    uint32_t source_bone_index;
    float    ratio;
    uint32_t flags;
} mmd_runtime_model_append_descriptor_t;

typedef struct mmd_runtime_model_bone_morph_offset_descriptor {
    uint32_t morph_index;
    uint32_t target_bone_index;
    float    position_offset_xyz[3];
    float    rotation_offset_xyzw[4];
} mmd_runtime_model_bone_morph_offset_descriptor_t;

typedef struct mmd_runtime_model_group_morph_offset_descriptor {
    uint32_t morph_index;
    uint32_t child_morph_index;
    float    ratio;
} mmd_runtime_model_group_morph_offset_descriptor_t;

typedef struct mmd_runtime_model_descriptor {
    uint32_t struct_size;
    uint32_t descriptor_version;
    uint32_t flags;
    uint32_t reserved;
    const mmd_runtime_model_bone_descriptor_t* bones;
    size_t bone_count;
    const mmd_runtime_model_ik_solver_descriptor_t* ik_solvers;
    size_t ik_solver_count;
    const mmd_runtime_model_ik_link_descriptor_t* ik_links;
    size_t ik_link_count;
    const mmd_runtime_model_append_descriptor_t* append_transforms;
    size_t append_transform_count;
    uint32_t morph_count;
    const mmd_runtime_model_bone_morph_offset_descriptor_t* bone_morph_offsets;
    size_t bone_morph_offset_count;
    const mmd_runtime_model_group_morph_offset_descriptor_t* group_morph_offsets;
    size_t group_morph_offset_count;
} mmd_runtime_model_descriptor_t;

typedef struct mmd_runtime_ffi_rig_ik_link {
    uint32_t bone_slot;
    bool     has_angle_limit;
    float    angle_limit_min_xyz[3];
    float    angle_limit_max_xyz[3];
} mmd_runtime_ffi_rig_ik_link_t;

typedef struct mmd_runtime_ffi_rig_bone {
    int32_t  parent_slot;
    float    rest_position_xyz[3];
    uint32_t flags;
    float    fixed_axis_xyz[3];
} mmd_runtime_ffi_rig_bone_t;

/* Additive v2 per-bone local-axis descriptor for primitive IK-chain creation.
   Existing mmd_runtime_ffi_rig_bone_t layout is intentionally unchanged.
   has_local_axis == false means unit XYZ angle-limit frames for that bone.
   When has_local_axis is true, local_axis_x_xyz / local_axis_z_xyz are the PMX
   bone-local X/Z directions used only as the IK angle-limit evaluation frame. */
typedef struct mmd_runtime_ffi_rig_bone_local_axis_v2 {
    bool  has_local_axis;
    float local_axis_x_xyz[3];
    float local_axis_z_xyz[3];
} mmd_runtime_ffi_rig_bone_local_axis_v2_t;

typedef struct mmd_runtime_ffi_ik_solve_stats {
    uint32_t executed_iterations;
    uint32_t link_steps;
    float    final_distance;
    uint32_t break_reason; /* 0=tolerance, 1=max_iterations, 2=rollback */
} mmd_runtime_ffi_ik_solve_stats_t;

typedef struct mmd_runtime_ffi_append_config {
    float ratio;
    bool  affect_rotation;
    bool  affect_translation;
} mmd_runtime_ffi_append_config_t;

typedef struct mmd_runtime_ffi_bone_morph_offset {
    uint32_t morph_index;
    uint32_t target_bone_index;
    float    position_offset_xyz[3];
    float    rotation_offset_xyzw[4];
} mmd_runtime_ffi_bone_morph_offset_t;

typedef struct mmd_runtime_ffi_group_morph_offset {
    uint32_t morph_index;
    uint32_t child_morph_index;
    float    ratio;
} mmd_runtime_ffi_group_morph_offset_t;

typedef struct mmd_runtime_ffi_byte_buffer {
    uint8_t* data;
    size_t   len;
} mmd_runtime_ffi_byte_buffer_t;

typedef struct mmd_runtime_ffi_physics_tick_config {
    float    fixed_substep_seconds;
    uint32_t max_substeps_per_tick;
} mmd_runtime_ffi_physics_tick_config_t;

typedef struct mmd_runtime_ffi_physics_step_stats {
    float    input_dt_seconds;
    float    clamped_dt_seconds;
    uint32_t substeps;
    float    accumulator_seconds;
} mmd_runtime_ffi_physics_step_stats_t;

typedef struct mmd_runtime_ffi_physics_rigidbody_desc {
    uint32_t shape;
    float    shape_size[3];
    float    position_xyz[3];
    float    rotation_euler_xyz[3];
    float    mass;
    float    linear_damping;
    float    angular_damping;
    float    friction;
    float    restitution;
    uint16_t collision_group;
    uint16_t collision_mask;
    int32_t  bone_index;
    uint32_t mode;
    float    body_from_bone_position_xyz[3];
    float    body_from_bone_rotation_xyzw[4];
    float    bone_from_body_position_xyz[3];
    float    bone_from_body_rotation_xyzw[4];
} mmd_runtime_ffi_physics_rigidbody_desc_t;

typedef struct mmd_runtime_ffi_physics_joint_desc {
    uint32_t kind;
    size_t   rigidbody_a;
    size_t   rigidbody_b;
    float    position_xyz[3];
    float    rotation_euler_xyz[3];
    float    translation_lower_limit_xyz[3];
    float    translation_upper_limit_xyz[3];
    float    rotation_lower_limit_xyz[3];
    float    rotation_upper_limit_xyz[3];
    float    spring_translation_factor_xyz[3];
    float    spring_rotation_factor_xyz[3];
} mmd_runtime_ffi_physics_joint_desc_t;

typedef struct mmd_runtime_ffi_physics_world_step_report {
    mmd_runtime_ffi_physics_step_stats_t tick;
    size_t kinematic_rigidbodies_fed;
    size_t bones_written_back;
} mmd_runtime_ffi_physics_world_step_report_t;

typedef struct mmd_runtime_ffi_physics_rigidbody_binding {
    int32_t  bone_index;  /* -1 if unbound */
    uint32_t mode;        /* mmd_runtime_physics_rigidbody_mode_t values */
} mmd_runtime_ffi_physics_rigidbody_binding_t;

typedef struct mmd_runtime_ffi_host_pose_view {
    const float*   local_position_offsets_xyz;
    const float*   local_rotation_xyzw;
    const float*   local_scales_xyz;
    size_t         bone_count;
    const float*   morph_weights;
    size_t         morph_count;
    const uint8_t* ik_enabled;
    size_t         ik_count;
} mmd_runtime_ffi_host_pose_view_t;

/* ------------------------------------------------------------------ */
/*  Model lifecycle                                                   */
/* ------------------------------------------------------------------ */

uint32_t mmd_runtime_abi_version(void);

uint32_t mmd_runtime_feature_flags(void);

/* Returns the most recent FFI error message for the calling thread, or NULL.
   The returned pointer is valid only until the next FFI call on the same
   thread. Do not store or free it. */
const char* mmd_runtime_last_error_message(void);

void mmd_runtime_byte_buffer_free(
    mmd_runtime_ffi_byte_buffer_t buffer);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_vmd_json(
    const uint8_t* data,
    size_t         len);

mmd_runtime_vmd_camera_track_t* mmd_runtime_vmd_camera_track_create_from_vmd_bytes(
    const uint8_t* data,
    size_t         len);

size_t mmd_runtime_vmd_camera_track_frame_count(
    const mmd_runtime_vmd_camera_track_t* track);

bool mmd_runtime_vmd_camera_track_sample(
    const mmd_runtime_vmd_camera_track_t* track,
    float                                 frame,
    float*                                out_values,
    size_t                                out_len);

bool mmd_runtime_vmd_sample_camera(
    const uint8_t* data,
    size_t         len,
    float          frame,
    float*         out_values,
    size_t         out_len);

void mmd_runtime_vmd_camera_track_free(
    mmd_runtime_vmd_camera_track_t* track);

mmd_runtime_vmd_light_track_t* mmd_runtime_vmd_light_track_create_from_vmd_bytes(
    const uint8_t* data,
    size_t         len);

size_t mmd_runtime_vmd_light_track_frame_count(
    const mmd_runtime_vmd_light_track_t* track);

bool mmd_runtime_vmd_light_track_sample(
    const mmd_runtime_vmd_light_track_t* track,
    float                                frame,
    float*                               out_values,
    size_t                               out_len);

bool mmd_runtime_vmd_sample_light(
    const uint8_t* data,
    size_t         len,
    float          frame,
    float*         out_values,
    size_t         out_len);

void mmd_runtime_vmd_light_track_free(
    mmd_runtime_vmd_light_track_t* track);

mmd_runtime_vmd_self_shadow_track_t* mmd_runtime_vmd_self_shadow_track_create_from_vmd_bytes(
    const uint8_t* data,
    size_t         len);

size_t mmd_runtime_vmd_self_shadow_track_frame_count(
    const mmd_runtime_vmd_self_shadow_track_t* track);

bool mmd_runtime_vmd_self_shadow_track_sample(
    const mmd_runtime_vmd_self_shadow_track_t* track,
    float                                      frame,
    float*                                     out_values,
    size_t                                     out_len);

bool mmd_runtime_vmd_sample_self_shadow(
    const uint8_t* data,
    size_t         len,
    float          frame,
    float*         out_values,
    size_t         out_len);

void mmd_runtime_vmd_self_shadow_track_free(
    mmd_runtime_vmd_self_shadow_track_t* track);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_non_geometry_json(
    const uint8_t* data,
    size_t         len);

/* PMX geometry typed-buffer API.
   Each function returns one geometry array as a native-endian byte buffer.
   The caller must free each buffer with mmd_runtime_byte_buffer_free.
   These legacy parse_pmx_* helpers reparse the whole PMX on every call; prefer
   the mmd_runtime_pmx_geometry_* handle API below when reading multiple arrays.
   Returns an empty buffer (data == NULL, len == 0) on any error. */

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_positions_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_normals_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_uvs_buffer(
    const uint8_t* data,
    size_t         len);

size_t mmd_runtime_parse_pmx_additional_uv_count(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_additional_uvs_buffer(
    const uint8_t* data,
    size_t         len,
    size_t         uv_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_indices_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_material_groups_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_skin_indices_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_skin_weights_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_edge_scale_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_sdef_enabled_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_sdef_c_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_sdef_r0_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_sdef_r1_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_sdef_rw0_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_sdef_rw1_buffer(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_qdef_enabled_buffer(
    const uint8_t* data,
    size_t         len);

/* Returns JSON: {"skinningModes": ["bdef1", ...]} */
mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_skinning_modes_json(
    const uint8_t* data,
    size_t         len);

/* PMX geometry handle API.
   mmd_runtime_pmx_geometry_create parses PMX bytes once and returns an owned
   opaque handle. Free it with mmd_runtime_pmx_geometry_free. Geometry buffers
   are native-endian flat arrays and must be freed with
   mmd_runtime_byte_buffer_free. Invalid input, invalid handles, or out-of-range
   UV indices return null handles, zero counts, or empty buffers. */

mmd_runtime_pmx_geometry_t* mmd_runtime_pmx_geometry_create(
    const uint8_t* data,
    size_t         len);

void mmd_runtime_pmx_geometry_free(
    mmd_runtime_pmx_geometry_t* geometry);

size_t mmd_runtime_pmx_geometry_additional_uv_count(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_positions_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_normals_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_uvs_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_additional_uvs_buffer(
    const mmd_runtime_pmx_geometry_t* geometry,
    size_t                            uv_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_indices_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_material_groups_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_skin_indices_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_skin_weights_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_edge_scale_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_sdef_enabled_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_sdef_c_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_sdef_r0_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_sdef_r1_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_sdef_rw0_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_sdef_rw1_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_qdef_enabled_buffer(
    const mmd_runtime_pmx_geometry_t* geometry);

/* Returns caller-owned JSON: {"skinningModes": ["bdef1", ...]}.
   Free the returned buffer with mmd_runtime_byte_buffer_free. */
mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_geometry_skinning_modes_json(
    const mmd_runtime_pmx_geometry_t* geometry);

/* Rig primitive API.
   Coordinates use the MMD convention: left-handed, Y-up, xyz vectors.
   Quaternions are xyzw. Matrices are column-major f32[16].
   Create functions return NULL on invalid input. Solve functions return false
   on invalid input, NULL required pointers, non-finite values, or short output
   buffers. Free functions accept NULL and otherwise release the owned opaque
   handle created by the matching create function. */

/* Creates an owned IK-chain primitive.
   bones is a bone_count-sized rig-bone array; parent_slot < 0 means no parent.
   target_bone_slot selects the effector bone in bones. links is a link_count-
   sized array ordered the same way PMX IK links are solved. iteration_count
   and limit_angle are the per-chain solve settings. bones and links are
   borrowed only for the call. Returns NULL if required arrays are NULL,
   indices are out of range, values are non-finite, or counts are invalid.
   Local-axis angle-limit frames are not provided by this entry point; use
   mmd_runtime_ik_chain_create_v2 when localAxis data is available. */
mmd_runtime_ik_chain_t* mmd_runtime_ik_chain_create(
    const mmd_runtime_ffi_rig_bone_t*    bones,
    size_t                               bone_count,
    uint32_t                             target_bone_slot,
    const mmd_runtime_ffi_rig_ik_link_t* links,
    size_t                               link_count,
    uint32_t                             iteration_count,
    float                                limit_angle);

/* Additive v2 IK-chain create with optional per-bone localAxis bases.
   Same arguments as mmd_runtime_ik_chain_create, plus local_axes:
   - local_axes may be NULL → identical to mmd_runtime_ik_chain_create.
   - When non-NULL, local_axes must point to bone_count entries. Degenerate
     axes (near-zero / parallel) are treated as no local axis for that bone.
   Non-finite local-axis vectors cause NULL. Existing create/solve/free
   contracts are otherwise unchanged. */
mmd_runtime_ik_chain_t* mmd_runtime_ik_chain_create_v2(
    const mmd_runtime_ffi_rig_bone_t*                 bones,
    size_t                                            bone_count,
    const mmd_runtime_ffi_rig_bone_local_axis_v2_t*  local_axes,
    uint32_t                                          target_bone_slot,
    const mmd_runtime_ffi_rig_ik_link_t*              links,
    size_t                                            link_count,
    uint32_t                                          iteration_count,
    float                                             limit_angle);

void mmd_runtime_ik_chain_free(
    mmd_runtime_ik_chain_t* chain);

/* Solves an IK-chain primitive.
   chain must be a live handle. parent_world_matrix may be NULL, which means
   identity; when provided it points to one column-major f32[16] matrix.
   local_position_offsets_xyz and local_rotations_xyzw are required arrays with
   bone_count * 3 and bone_count * 4 f32 values. goal_position_xyz is a required
   xyz vector in MMD coordinates. max_iterations_cap == 0 means no cap.
   out_link_rotations_xyzw receives link_count xyzw quaternions and
   out_link_rotation_f32_len must be at least link_count * 4. out_stats may be
   NULL; when provided it receives solve diagnostics. Input and output arrays
   are caller-owned and are not retained after the call. */
bool mmd_runtime_ik_chain_solve(
    mmd_runtime_ik_chain_t*              chain,
    const float*                         parent_world_matrix,
    const float*                         local_position_offsets_xyz,
    const float*                         local_rotations_xyzw,
    const float*                         goal_position_xyz,
    float                                tolerance,
    uint32_t                             max_iterations_cap,
    float*                               out_link_rotations_xyzw,
    size_t                               out_link_rotation_f32_len,
    mmd_runtime_ffi_ik_solve_stats_t*    out_stats);

/* Creates an owned append-transform primitive.
   config is borrowed for the call and must not be NULL. ratio and channel flags
   are copied into the handle. Returns NULL on invalid or non-finite input. */
mmd_runtime_append_solver_t* mmd_runtime_append_solver_create(
    const mmd_runtime_ffi_append_config_t* config);

void mmd_runtime_append_solver_free(
    mmd_runtime_append_solver_t* solver);

/* Solves an append-transform primitive.
   solver must be a live handle. source_position_offset_xyz and
   source_rotation_xyzw are required caller-owned inputs. out_position_offset_xyz
   and out_rotation_xyzw are required caller-owned outputs. The output rotation
   is an xyzw quaternion. Returns false on NULL pointers or non-finite input. */
bool mmd_runtime_append_solver_solve(
    const mmd_runtime_append_solver_t* solver,
    const float*                      source_position_offset_xyz,
    const float*                      source_rotation_xyzw,
    float*                            out_position_offset_xyz,
    float*                            out_rotation_xyzw);

/* PMX material split handle API.
   mmd_runtime_pmx_material_split_create parses PMX bytes once and returns an
   owned opaque handle. Free it with mmd_runtime_pmx_material_split_free.
   All returned byte buffers are owned by Rust and must be freed with
   mmd_runtime_byte_buffer_free. Geometry buffers are native-endian flat arrays.
   Invalid input, invalid handles, or out-of-range mesh/UV indices return null
   handles, zero counts, or empty buffers. */

mmd_runtime_pmx_material_split_t* mmd_runtime_pmx_material_split_create(
    const uint8_t* data,
    size_t         len,
    uint32_t       flags);

void mmd_runtime_pmx_material_split_free(
    mmd_runtime_pmx_material_split_t* split);

size_t mmd_runtime_pmx_material_split_mesh_count(
    const mmd_runtime_pmx_material_split_t* split);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_manifest_json(
    const mmd_runtime_pmx_material_split_t* split);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_positions_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_normals_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_uvs_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_additional_uvs_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index,
    size_t                                  uv_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_indices_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_skin_indices_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_skin_weights_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_edge_scale_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_sdef_enabled_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_sdef_c_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_sdef_r0_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_sdef_r1_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_sdef_rw0_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_sdef_rw1_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_material_split_qdef_enabled_buffer(
    const mmd_runtime_pmx_material_split_t* split,
    size_t                                  mesh_index);

/* PMX rig-spec handle API.
   PMX rig data is reported in MMD coordinates: left-handed, Y-up, xyz vectors.
   Quaternions in any rig payload are xyzw. Matrices in any rig payload are
   column-major f32[16].
   mmd_runtime_pmx_rig_spec_create parses PMX bytes once and returns an owned
   opaque handle. Free it with mmd_runtime_pmx_rig_spec_free. The manifest JSON
   byte buffer is owned by Rust and must be freed with
   mmd_runtime_byte_buffer_free. Invalid input or invalid handles return null
   handles or empty buffers. */

/* Creates an owned PMX rig-spec handle from data[0..len].
   data is borrowed only for the call and must not be NULL when len > 0.
   Returns NULL when the bytes are invalid, empty, or unsupported. */
mmd_runtime_pmx_rig_spec_t* mmd_runtime_pmx_rig_spec_create(
    const uint8_t* data,
    size_t         len);

/* Releases an owned PMX rig-spec handle. Passing NULL is allowed. */
void mmd_runtime_pmx_rig_spec_free(
    mmd_runtime_pmx_rig_spec_t* spec);

/* Returns the rig-spec manifest JSON for spec.
   spec must be a live handle. The returned byte buffer is Rust-owned and must
   be freed with mmd_runtime_byte_buffer_free. Invalid or NULL spec returns an
   empty buffer. */
mmd_runtime_ffi_byte_buffer_t mmd_runtime_pmx_rig_spec_manifest_json(
    const mmd_runtime_pmx_rig_spec_t* spec);

mmd_runtime_model_t* mmd_runtime_model_create(
    const int32_t* parent_indices,
    const float*   rest_positions_xyz,
    size_t         bone_count);

mmd_runtime_model_t* mmd_runtime_model_create_with_inverse_bind(
    const int32_t* parent_indices,
    const float*   rest_positions_xyz,
    const float*   inverse_bind_matrices,
    size_t         bone_count);

mmd_runtime_model_t* mmd_runtime_model_create_with_append(
    const int32_t*                       parent_indices,
    const float*                         rest_positions_xyz,
    size_t                               bone_count,
    const mmd_runtime_ffi_append_transform_t* append_transforms,
    size_t                               append_transform_count);

mmd_runtime_model_t* mmd_runtime_model_create_with_append_and_inverse_bind(
    const int32_t*                       parent_indices,
    const float*                         rest_positions_xyz,
    const float*                         inverse_bind_matrices,
    size_t                               bone_count,
    const mmd_runtime_ffi_append_transform_t* append_transforms,
    size_t                               append_transform_count);

mmd_runtime_model_t* mmd_runtime_model_create_full(
    const int32_t*                       parent_indices,
    const float*                         rest_positions_xyz,
    const float*                         inverse_bind_matrices,
    size_t                               bone_count,
    const mmd_runtime_ffi_ik_solver_t*   ik_solvers,
    size_t                               ik_solver_count,
    const mmd_runtime_ffi_ik_link_t*     ik_links,
    size_t                               ik_link_count,
    const mmd_runtime_ffi_append_transform_t* append_transforms,
    size_t                               append_transform_count);

mmd_runtime_model_t* mmd_runtime_model_create_full_with_transform_order(
    const int32_t*                       parent_indices,
    const float*                         rest_positions_xyz,
    const float*                         inverse_bind_matrices,
    const int32_t*                       transform_orders,
    size_t                               bone_count,
    const mmd_runtime_ffi_ik_solver_t*   ik_solvers,
    size_t                               ik_solver_count,
    const mmd_runtime_ffi_ik_link_t*     ik_links,
    size_t                               ik_link_count,
    const mmd_runtime_ffi_append_transform_t* append_transforms,
    size_t                               append_transform_count);

mmd_runtime_model_t* mmd_runtime_model_create_full_with_morphs(
    const int32_t*                           parent_indices,
    const float*                             rest_positions_xyz,
    const float*                             inverse_bind_matrices,
    const int32_t*                           transform_orders,
    size_t                                   bone_count,
    const mmd_runtime_ffi_ik_solver_t*       ik_solvers,
    size_t                                   ik_solver_count,
    const mmd_runtime_ffi_ik_link_t*         ik_links,
    size_t                                   ik_link_count,
    const mmd_runtime_ffi_append_transform_t* append_transforms,
    size_t                                   append_transform_count,
    uint32_t                                 morph_count,
    const mmd_runtime_ffi_bone_morph_offset_t*  bone_morph_offsets,
    size_t                                   bone_morph_offset_count,
    const mmd_runtime_ffi_group_morph_offset_t* group_morph_offsets,
    size_t                                   group_morph_offset_count);

mmd_runtime_model_t* mmd_runtime_model_create_from_descriptor(
    const mmd_runtime_model_descriptor_t* descriptor);

mmd_runtime_model_t* mmd_runtime_model_create_from_pmx_bytes(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_export_pmx_from_parts(
    const uint8_t* metadata_json,
    size_t         metadata_json_len,
    const float*   positions_xyz,
    size_t         vertex_count,
    const float*   normals_xyz,
    const float*   uvs_xy,
    const uint32_t* indices,
    size_t         index_count,
    const uint32_t* skin_indices,
    const float*   skin_weights,
    const float*   edge_scale);

mmd_runtime_clip_t* mmd_runtime_clip_create_from_vmd_bytes_for_model(
    const mmd_runtime_model_t* model,
    const uint8_t*             data,
    size_t                     len);

size_t mmd_runtime_model_bone_count(
    const mmd_runtime_model_t* model);

size_t mmd_runtime_model_morph_count(
    const mmd_runtime_model_t* model);

size_t mmd_runtime_model_ik_count(
    const mmd_runtime_model_t* model);

void mmd_runtime_model_free(mmd_runtime_model_t* model);

/* ------------------------------------------------------------------ */
/*  Instance lifecycle and evaluation                                  */
/* ------------------------------------------------------------------ */

mmd_runtime_instance_t* mmd_runtime_instance_create(
    const mmd_runtime_model_t* model,
    size_t                     morph_count);

mmd_runtime_instance_t* mmd_runtime_instance_create_for_model(
    const mmd_runtime_model_t* model);

mmd_runtime_instance_t* mmd_runtime_instance_create_with_counts(
    const mmd_runtime_model_t* model,
    size_t                     morph_count,
    size_t                     ik_count);

void mmd_runtime_instance_free(mmd_runtime_instance_t* instance);

bool mmd_runtime_instance_evaluate_rest_pose(
    mmd_runtime_instance_t* instance);

bool mmd_runtime_instance_evaluate_clip_frame(
    mmd_runtime_instance_t*       instance,
    const mmd_runtime_clip_t*     clip,
    float                         frame);

/* Evaluates with custom IK solver options.
   ik_max_iterations_cap == 0 means no cap. */
bool mmd_runtime_instance_evaluate_clip_frame_with_ik_options(
    mmd_runtime_instance_t*       instance,
    const mmd_runtime_clip_t*     clip,
    float                         frame,
    float                         ik_tolerance,
    uint32_t                      ik_max_iterations_cap);

mmd_runtime_status_t mmd_runtime_instance_get_physics_mode(
    const mmd_runtime_instance_t* instance,
    mmd_runtime_physics_mode_t*   out_mode);

mmd_runtime_status_t mmd_runtime_instance_set_physics_mode(
    mmd_runtime_instance_t*      instance,
    mmd_runtime_physics_mode_t   mode);

mmd_runtime_status_t mmd_runtime_instance_get_physics_tick_config(
    const mmd_runtime_instance_t*              instance,
    mmd_runtime_ffi_physics_tick_config_t*     out_config);

mmd_runtime_status_t mmd_runtime_instance_set_physics_tick_config(
    mmd_runtime_instance_t*                    instance,
    const mmd_runtime_ffi_physics_tick_config_t* config);

mmd_runtime_status_t mmd_runtime_instance_reset_physics_tick(
    mmd_runtime_instance_t* instance);

mmd_runtime_status_t mmd_runtime_instance_evaluate_clip_frame_before_physics(
    mmd_runtime_instance_t*   instance,
    const mmd_runtime_clip_t* clip,
    float                     frame);

mmd_runtime_status_t mmd_runtime_instance_evaluate_clip_frame_before_physics_with_ik_options(
    mmd_runtime_instance_t*   instance,
    const mmd_runtime_clip_t* clip,
    float                     frame,
    float                     ik_tolerance,
    uint32_t                  ik_max_iterations_cap);

mmd_runtime_status_t mmd_runtime_instance_evaluate_current_pose_before_physics(
    mmd_runtime_instance_t* instance);

mmd_runtime_status_t mmd_runtime_instance_apply_host_pose(
    mmd_runtime_instance_t *instance,
    const mmd_runtime_ffi_host_pose_view_t *view);

mmd_runtime_status_t mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(
    mmd_runtime_instance_t *instance,
    const mmd_runtime_ffi_host_pose_view_t *view);

mmd_runtime_status_t mmd_runtime_instance_evaluate_current_pose_after_physics(
    mmd_runtime_instance_t* instance);

mmd_runtime_status_t mmd_runtime_instance_evaluate_current_pose_after_physics_with_ik_options(
    mmd_runtime_instance_t* instance,
    float                   ik_tolerance,
    uint32_t                ik_max_iterations_cap);

mmd_runtime_status_t mmd_runtime_instance_advance_physics_tick_clock(
    mmd_runtime_instance_t*                   instance,
    float                                     dt_seconds,
    mmd_runtime_ffi_physics_step_stats_t*     out_stats);

mmd_runtime_status_t mmd_runtime_instance_apply_physics_world_matrices(
    mmd_runtime_instance_t* instance,
    const float*            physics_world_matrices_f32,
    size_t                  physics_world_matrices_f32_len,
    const uint8_t*          physics_world_matrix_mask_u8,
    size_t                  physics_world_matrix_mask_u8_len,
    size_t*                 out_updated_bone_count);

mmd_runtime_status_t mmd_runtime_physics_world_create(
    const mmd_runtime_ffi_physics_rigidbody_desc_t* rigidbodies,
    size_t                                          rigidbody_count,
    const mmd_runtime_ffi_physics_joint_desc_t*     joints,
    size_t                                          joint_count,
    mmd_runtime_physics_world_t**                   out_world);

mmd_runtime_status_t mmd_runtime_physics_world_create_from_pmx_bytes(
    const uint8_t*                  pmx_data,
    size_t                          pmx_len,
    mmd_runtime_physics_world_t**   out_world);

void mmd_runtime_physics_world_free(
    mmd_runtime_physics_world_t* world);

/* Returns a deterministic UTF-8 JSON snapshot of the editable physics
   parameters for a PMX-created world. The top-level schema_version is 1;
   rigid_bodies and joints are objects keyed by the original PMX names.
   Descriptor-created worlds return an empty buffer and UNSUPPORTED details in
   the thread-local last error. The returned Rust-owned buffer must be freed
   with mmd_runtime_byte_buffer_free. */
mmd_runtime_ffi_byte_buffer_t mmd_runtime_physics_params_get_json(
    const mmd_runtime_physics_world_t* world);

/* Applies a partial schema-version-1 named parameter update. A successful
   update rebuilds the physics world, preserves gravity, resets simulation
   state, and takes effect on the next seed/step. Validation or rebuild failure
   leaves the existing world unchanged. */
mmd_runtime_status_t mmd_runtime_physics_params_set_json(
    mmd_runtime_physics_world_t* world,
    const uint8_t*               data,
    size_t                       len);

/* Successful reset reseeds every bound body from the runtime pose, performs
   one fixed 1/60 solver settle, re-pins static bodies, cleans transient state,
   writes the settled dynamic bodies back to the runtime pose, and arms
   seed-only behavior for the next bake sample. */
mmd_runtime_status_t mmd_runtime_physics_world_reset(
    mmd_runtime_physics_world_t* world,
    mmd_runtime_instance_t*      instance,
    size_t*                      out_seeded_rigidbody_count);

/* Forward steps feed static bodies only; DynamicBone bodies are seeded by
   reset and remain solver-owned. A successful explicit step disarms bake
   seed-only state so the next bake sample advances physics normally. */
mmd_runtime_status_t mmd_runtime_physics_world_step_runtime(
    mmd_runtime_physics_world_t*                      world,
    mmd_runtime_instance_t*                           instance,
    float                                             dt_seconds,
    mmd_runtime_ffi_physics_world_step_report_t*      out_report);

/* Applies a validated host pose, evaluates the before-physics phase, seeds or
   steps the physics world (per `action`), and evaluates the after-physics
   phase, all as a single atomic call. On failure applying the host pose, no
   mutation occurs. For SEED, dt_seconds is ignored and out_report (when
   non-null) is zeroed, since a seed resets rigid bodies to their
   bone-derived positions without advancing the solver. For STEP, dt_seconds
   must be finite and >= 0, and the instance's physics mode must be Trace or
   Live; MMD_RUNTIME_STATUS_INVALID_INPUT is returned when the mode is Off.
   Unknown action values return MMD_RUNTIME_STATUS_INVALID_INPUT.
   MMD_RUNTIME_STATUS_INVALID_INPUT is also returned when the physics
   world's rigidbody bindings reference bone indices outside the instance's
   bone range. IK options apply to the before-physics phase; after-physics
   uses defaults. */
mmd_runtime_status_t mmd_runtime_evaluate_host_frame(
    mmd_runtime_instance_t*                     instance,
    mmd_runtime_physics_world_t*                world,
    const mmd_runtime_ffi_host_pose_view_t*     pose,
    mmd_runtime_physics_frame_action_t          action,
    float                                       dt_seconds,
    float                                       ik_tolerance,
    uint32_t                                    ik_max_iterations_cap,
    mmd_runtime_ffi_physics_world_step_report_t* out_report);

mmd_runtime_status_t mmd_runtime_physics_world_rigidbody_count(
    const mmd_runtime_physics_world_t* world,
    size_t*                            out_rigidbody_count);

mmd_runtime_status_t mmd_runtime_physics_world_get_gravity(
    const mmd_runtime_physics_world_t* world,
    float                               out_gravity_xyz[3]);

mmd_runtime_status_t mmd_runtime_physics_world_set_gravity(
    mmd_runtime_physics_world_t* world,
    const float                   gravity_xyz[3]);

mmd_runtime_status_t mmd_runtime_physics_world_copy_rigidbody_states(
    const mmd_runtime_physics_world_t* world,
    float*                             out_transforms_f32,
    size_t                             out_transforms_f32_len);

mmd_runtime_status_t mmd_runtime_physics_world_copy_rigidbody_bindings(
    const mmd_runtime_physics_world_t *world,
    mmd_runtime_ffi_physics_rigidbody_binding_t *out_bindings,
    size_t capacity,
    size_t *out_count);

/* Returns MMD_RUNTIME_STATUS_BUFFER_TOO_SMALL when bone_count is smaller than
   the physics world's required bone count (the highest bound bone index
   plus one), rather than silently ignoring out-of-range bindings. */
mmd_runtime_status_t mmd_runtime_physics_world_physics_driven_bone_mask(
    const mmd_runtime_physics_world_t *world,
    uint8_t *out_mask,
    size_t bone_count);

bool mmd_runtime_instance_evaluate_clip_frame_without_ik(
    mmd_runtime_instance_t*       instance,
    const mmd_runtime_clip_t*     clip,
    float                         frame);

size_t mmd_runtime_instance_clip_frame_batch_world_matrix_f32_len(
    const mmd_runtime_instance_t* instance,
    size_t                        frame_count);

size_t mmd_runtime_instance_clip_frame_batch_morph_weight_f32_len(
    const mmd_runtime_instance_t* instance,
    size_t                        frame_count);

/* Evaluates a frame range into caller-owned contiguous buffers.
   worker_count == 0 uses available host parallelism.
   The source instance is not mutated; worker-local runtime instances are used.
   out_world_matrices_f32 layout: [frame][bone][16] column-major f32 matrices.
   out_morph_weights_f32 layout: [frame][morph]. */
bool mmd_runtime_instance_evaluate_clip_frame_batch(
    const mmd_runtime_instance_t* instance,
    const mmd_runtime_clip_t*     clip,
    float                         start_frame,
    float                         frame_step,
    size_t                        frame_count,
    uint32_t                      worker_count,
    float*                        out_world_matrices_f32,
    size_t                        out_world_matrices_f32_len,
    float*                        out_morph_weights_f32,
    size_t                        out_morph_weights_f32_len);

/* Reduces dense batch output into an owned opaque sparse-pose handle.
   Dense layouts match evaluate_clip_frame_batch. On failure, *out_reduced_pose
   is set to NULL. */
mmd_runtime_status_t mmd_runtime_reduced_pose_create_from_dense(
    const mmd_runtime_model_t*                     model,
    uint64_t                                       model_identity,
    const float*                                   world_matrices_f32,
    size_t                                         world_matrices_f32_len,
    const float*                                   morph_weights_f32,
    size_t                                         morph_weights_f32_len,
    size_t                                         frame_count,
    float                                          start_frame,
    float                                          frame_step,
    uint32_t                                       target,
    mmd_runtime_ffi_reduction_tolerances_t         tolerances,
    mmd_runtime_reduced_pose_t**                   out_reduced_pose);

void mmd_runtime_reduced_pose_free(mmd_runtime_reduced_pose_t* pose);
size_t mmd_runtime_reduced_pose_bone_count(const mmd_runtime_reduced_pose_t* pose);
size_t mmd_runtime_reduced_pose_morph_count(const mmd_runtime_reduced_pose_t* pose);
mmd_runtime_status_t mmd_runtime_reduced_pose_report(
    const mmd_runtime_reduced_pose_t*               pose,
    mmd_runtime_ffi_pose_reduction_report_t*        out_report);

/* Enumerates target-native Unity scalar curves from a DCC_CUBIC reduced pose.
   Curves are translation XYZ then local Euler XYZ for each bone, followed by
   one weight curve for each morph. frames_per_second must be finite and > 0;
   flip_z selects Unity handedness conversion. LINEAR_SLERP and VMD_BEZIER
   reduced poses return MMD_RUNTIME_STATUS_UNSUPPORTED rather than being
   silently converted to Hermite curves. The reduced handle owns its skeleton
   snapshot, so these calls remain valid after the source model is freed. */
mmd_runtime_status_t mmd_runtime_reduced_pose_unity_curve_count(
    const mmd_runtime_reduced_pose_t* pose,
    float                             frames_per_second,
    bool                              flip_z,
    size_t*                           out_curve_count);

mmd_runtime_status_t mmd_runtime_reduced_pose_unity_curve_descriptor(
    const mmd_runtime_reduced_pose_t*             pose,
    float                                         frames_per_second,
    bool                                          flip_z,
    size_t                                        curve_index,
    mmd_runtime_ffi_unity_curve_descriptor_t*     out_descriptor);

/* Two-call caller-owned retrieval. Pass out_keys = NULL and capacity = 0 to
   receive MMD_RUNTIME_STATUS_BUFFER_TOO_SMALL plus out_required_count, then
   allocate that many keys and call again. Any short buffer returns the same
   status and required count. Euler filtering, degree conversion, and
   per-second tangent conversion are already applied by Rust. */
mmd_runtime_status_t mmd_runtime_reduced_pose_unity_curve_keys(
    const mmd_runtime_reduced_pose_t* pose,
    float                             frames_per_second,
    bool                              flip_z,
    size_t                            curve_index,
    mmd_runtime_ffi_unity_curve_key_t* out_keys,
    size_t                            out_key_capacity,
    size_t*                           out_required_count);

/* Stateful sequential physics bake.
   After world creation or a successful mmd_runtime_physics_world_reset, the
   first bake sample is seed-only: evaluate_clip_frame_before_physics at that
   sample, reset/reseed Bullet from the evaluated pose (physics tick reset, no
   solver settle), copy world/morph outputs, and do NOT advance the solver or
   normal forward-step clock. Later samples
   use evaluate -> step -> copy. A continuation bake without another successful
   reset does not skip its first sample. frame_count == 0 does not consume the
   seed-only state. A successful mmd_runtime_physics_world_step_runtime also
   disarms seed-only. out_last_report for a one-sample seed-only bake remains
   the default zero report; multi-sample bakes report the final actual step.
   Layout matches evaluate_clip_frame_batch: [frame][bone][16] and [frame][morph]. */
mmd_runtime_status_t mmd_runtime_physics_world_bake_clip_frames(
    mmd_runtime_physics_world_t*                      world,
    mmd_runtime_instance_t*                           instance,
    const mmd_runtime_clip_t*                         clip,
    float                                             start_frame,
    float                                             frame_step,
    float                                             dt_seconds,
    size_t                                            frame_count,
    float*                                            out_world_matrices_f32,
    size_t                                            out_world_matrices_f32_len,
    float*                                            out_morph_weights_f32,
    size_t                                            out_morph_weights_f32_len,
    mmd_runtime_ffi_physics_world_step_report_t*      out_last_report);

/* ------------------------------------------------------------------ */
/*  Output: world matrices                                             */
/* ------------------------------------------------------------------ */

size_t mmd_runtime_instance_world_matrix_f32_len(
    const mmd_runtime_instance_t* instance);

bool mmd_runtime_instance_copy_world_matrices(
    const mmd_runtime_instance_t* instance,
    float*                        out_f32,
    size_t                        out_f32_len);

/* ------------------------------------------------------------------ */
/*  Output: skinning matrices                                          */
/* ------------------------------------------------------------------ */

size_t mmd_runtime_instance_skinning_matrix_f32_len(
    const mmd_runtime_instance_t* instance);

bool mmd_runtime_instance_copy_skinning_matrices(
    const mmd_runtime_instance_t* instance,
    float*                        out_f32,
    size_t                        out_f32_len);

/* ------------------------------------------------------------------ */
/*  Output: direct matrix views (Phase 6)                               */
/* ------------------------------------------------------------------ */

size_t mmd_runtime_instance_bone_count(
    const mmd_runtime_instance_t* instance);

/* Returned matrix pointers contain bone_count * 16 column-major f32 values.
   They remain valid until the next evaluation call or instance free. */
const float* mmd_runtime_instance_world_matrices(
    const mmd_runtime_instance_t* instance);

const float* mmd_runtime_instance_skinning_matrices(
    const mmd_runtime_instance_t* instance);

/* ------------------------------------------------------------------ */
/*  Output: morph weights                                              */
/* ------------------------------------------------------------------ */

size_t mmd_runtime_instance_morph_weight_len(
    const mmd_runtime_instance_t* instance);

bool mmd_runtime_instance_copy_morph_weights(
    const mmd_runtime_instance_t* instance,
    float*                        out_f32,
    size_t                        out_f32_len);

/* Returned pointer contains morph_weight_len f32 values.
   Remains valid until the next evaluation call or instance free. */
const float* mmd_runtime_instance_morph_weights(
    const mmd_runtime_instance_t* instance);

/* ------------------------------------------------------------------ */
/*  Output: IK enabled states                                          */
/* ------------------------------------------------------------------ */

size_t mmd_runtime_instance_ik_enabled_len(
    const mmd_runtime_instance_t* instance);

bool mmd_runtime_instance_copy_ik_enabled(
    const mmd_runtime_instance_t* instance,
    uint8_t*                      out_u8,
    size_t                        out_u8_len);

/* Returned pointer contains ik_enabled_len uint8_t values (0/1).
   Remains valid until the next evaluation call or instance free. */
const uint8_t* mmd_runtime_instance_ik_enabled(
    const mmd_runtime_instance_t* instance);

/* ------------------------------------------------------------------ */
/*  Clip lifecycle                                                     */
/* ------------------------------------------------------------------ */

mmd_runtime_clip_t* mmd_runtime_clip_create(
    const mmd_runtime_ffi_bone_track_t*     bone_tracks,
    size_t                                  bone_track_count,
    const mmd_runtime_ffi_bone_keyframe_t*  bone_keyframes,
    size_t                                  bone_keyframe_count,
    const mmd_runtime_ffi_morph_track_t*    morph_tracks,
    size_t                                  morph_track_count,
    const mmd_runtime_ffi_morph_keyframe_t* morph_keyframes,
    size_t                                  morph_keyframe_count,
    const mmd_runtime_ffi_property_keyframe_t* property_keyframes,
    size_t                                  property_keyframe_count,
    const uint8_t*                          property_ik_enabled,
    size_t                                  property_ik_enabled_count);

bool mmd_runtime_clip_frame_range(
    const mmd_runtime_clip_t* clip,
    uint32_t*                 out_first_frame,
    uint32_t*                 out_last_frame);

void mmd_runtime_clip_free(mmd_runtime_clip_t* clip);

#ifdef __cplusplus
}
#endif

#endif /* MMD_RUNTIME_H */
