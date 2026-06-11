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

typedef struct mmd_runtime_model_t         mmd_runtime_model_t;
typedef struct mmd_runtime_instance_t      mmd_runtime_instance_t;
typedef struct mmd_runtime_clip_t          mmd_runtime_clip_t;
typedef struct mmd_runtime_parsed_model_t  mmd_runtime_parsed_model_t;

/* ------------------------------------------------------------------ */
/*  Flag constants                                                    */
/* ------------------------------------------------------------------ */

/* Append-transform flags  (bitmask) */
#define MMD_RUNTIME_APPEND_ROTATION    (1u << 0)
#define MMD_RUNTIME_APPEND_TRANSLATION (1u << 1)
#define MMD_RUNTIME_APPEND_LOCAL       (1u << 2)

/* IK link flags           (bitmask) */
#define MMD_RUNTIME_IK_LINK_ANGLE_LIMIT (1u << 0)

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

/* ------------------------------------------------------------------ */
/*  Parsed model (full PMX metadata)                                   */
/* ------------------------------------------------------------------ */

mmd_runtime_parsed_model_t* mmd_runtime_parsed_model_create_from_pmx_bytes(
    const uint8_t* data,
    size_t         len);

void mmd_runtime_parsed_model_free(
    mmd_runtime_parsed_model_t* model);

size_t mmd_runtime_parsed_model_vertex_count(
    const mmd_runtime_parsed_model_t* model);

size_t mmd_runtime_parsed_model_index_count(
    const mmd_runtime_parsed_model_t* model);

size_t mmd_runtime_parsed_model_material_group_count(
    const mmd_runtime_parsed_model_t* model);

/*  Pointer accessors: returned pointers are valid until free.        */
/*  Null model or empty array returns NULL.                           */

/*  vertex_count * 3 f32 values */
const float* mmd_runtime_parsed_model_positions(
    const mmd_runtime_parsed_model_t* model);

/*  vertex_count * 3 f32 values */
const float* mmd_runtime_parsed_model_normals(
    const mmd_runtime_parsed_model_t* model);

/*  vertex_count * 2 f32 values */
const float* mmd_runtime_parsed_model_uvs(
    const mmd_runtime_parsed_model_t* model);

/*  vertex_count f32 values */
const float* mmd_runtime_parsed_model_edge_scale(
    const mmd_runtime_parsed_model_t* model);

/*  index_count u32 values (three per triangle) */
const uint32_t* mmd_runtime_parsed_model_indices(
    const mmd_runtime_parsed_model_t* model);

/*  vertex_count * 4 u32 values */
const uint32_t* mmd_runtime_parsed_model_skin_indices(
    const mmd_runtime_parsed_model_t* model);

/*  vertex_count * 4 f32 values */
const float* mmd_runtime_parsed_model_skin_weights(
    const mmd_runtime_parsed_model_t* model);

/*  material_group_count * 3 u32 values: (start, count, material_index) */
const uint32_t* mmd_runtime_parsed_model_material_groups(
    const mmd_runtime_parsed_model_t* model);

/*  Returns a JSON byte buffer with non-hot metadata (everything except
    large geometry arrays). Must be freed with mmd_runtime_byte_buffer_free. */
mmd_runtime_ffi_byte_buffer_t mmd_runtime_parsed_model_metadata_json(
    const mmd_runtime_parsed_model_t* model);

#ifdef __cplusplus
}
#endif

#endif /* MMD_RUNTIME_H */
