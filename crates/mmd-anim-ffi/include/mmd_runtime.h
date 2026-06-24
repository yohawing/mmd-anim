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

#define MMD_RUNTIME_ABI_VERSION 1

/* ------------------------------------------------------------------ */
/*  Opaque handle types                                               */
/* ------------------------------------------------------------------ */

typedef struct mmd_runtime_model_t    mmd_runtime_model_t;
typedef struct mmd_runtime_instance_t mmd_runtime_instance_t;
typedef struct mmd_runtime_clip_t     mmd_runtime_clip_t;
typedef struct mmd_runtime_pmx_material_split_t mmd_runtime_pmx_material_split_t;
typedef struct mmd_runtime_pmx_rig_spec_t mmd_runtime_pmx_rig_spec_t;
typedef struct mmd_runtime_ik_chain_t mmd_runtime_ik_chain_t;
typedef struct mmd_runtime_append_solver_t mmd_runtime_append_solver_t;

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

/* ------------------------------------------------------------------ */
/*  Model lifecycle                                                   */
/* ------------------------------------------------------------------ */

uint32_t mmd_runtime_abi_version(void);

void mmd_runtime_byte_buffer_free(
    mmd_runtime_ffi_byte_buffer_t buffer);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_vmd_json(
    const uint8_t* data,
    size_t         len);

mmd_runtime_ffi_byte_buffer_t mmd_runtime_parse_pmx_non_geometry_json(
    const uint8_t* data,
    size_t         len);

/* PMX geometry typed-buffer API.
   Each function returns one geometry array as a native-endian byte buffer.
   The caller must free each buffer with mmd_runtime_byte_buffer_free.
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
   indices are out of range, values are non-finite, or counts are invalid. */
mmd_runtime_ik_chain_t* mmd_runtime_ik_chain_create(
    const mmd_runtime_ffi_rig_bone_t*    bones,
    size_t                               bone_count,
    uint32_t                             target_bone_slot,
    const mmd_runtime_ffi_rig_ik_link_t* links,
    size_t                               link_count,
    uint32_t                             iteration_count,
    float                                limit_angle);

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

bool mmd_runtime_instance_evaluate_clip_frame_without_ik(
    mmd_runtime_instance_t*       instance,
    const mmd_runtime_clip_t*     clip,
    float                         frame);

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
