#pragma once

#include <stdint.h>

#ifdef _WIN32
#define MMD_ANIM_BULLET_API __declspec(dllexport)
#else
#define MMD_ANIM_BULLET_API
#endif

typedef struct mmd_anim_bullet_world mmd_anim_bullet_world;

typedef enum mmd_anim_bullet_status {
    MMD_ANIM_BULLET_OK = 0,
    MMD_ANIM_BULLET_NULL_POINTER = 1,
    MMD_ANIM_BULLET_INVALID_ARGUMENT = 2,
    MMD_ANIM_BULLET_INTERNAL_ERROR = 3,
} mmd_anim_bullet_status;

typedef enum mmd_anim_bullet_shape_type {
    MMD_ANIM_BULLET_SHAPE_SPHERE = 0,
    MMD_ANIM_BULLET_SHAPE_BOX = 1,
    MMD_ANIM_BULLET_SHAPE_CAPSULE = 2,
} mmd_anim_bullet_shape_type;

typedef struct mmd_anim_bullet_rigidbody_desc {
    int32_t shape_type;
    float shape_size[3];
    float position[3];
    float rotation_euler[3];
    float mass;
    float linear_damping;
    float angular_damping;
    float friction;
    float restitution;
    uint16_t collision_group;
    uint16_t collision_mask;
} mmd_anim_bullet_rigidbody_desc;

typedef struct mmd_anim_bullet_6dof_spring_joint_desc {
    int32_t rigidbody_index_a;
    int32_t rigidbody_index_b;
    float position[3];
    float rotation_euler[3];
    float translation_lower_limit[3];
    float translation_upper_limit[3];
    float rotation_lower_limit[3];
    float rotation_upper_limit[3];
    float spring_translation_factor[3];
    float spring_rotation_factor[3];
} mmd_anim_bullet_6dof_spring_joint_desc;

typedef struct mmd_anim_bullet_contact_point {
    int32_t rigidbody_index_a;
    int32_t rigidbody_index_b;
    float distance;
    float position_world_on_a[3];
    float position_world_on_b[3];
    float normal_world_on_b[3];
} mmd_anim_bullet_contact_point;

#ifdef __cplusplus
extern "C" {
#endif

MMD_ANIM_BULLET_API uint32_t mmd_anim_bullet_get_version(void);
MMD_ANIM_BULLET_API const char *mmd_anim_bullet_get_last_error(void);

MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_create(mmd_anim_bullet_world **out_world);
MMD_ANIM_BULLET_API void
mmd_anim_bullet_world_destroy(mmd_anim_bullet_world *world);
MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_reset(mmd_anim_bullet_world *world);
MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_settle_to_current(mmd_anim_bullet_world *world);
MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_step(
    mmd_anim_bullet_world *world,
    float delta_time,
    int32_t max_sub_steps,
    float fixed_substep_seconds);

MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_add_rigidbody(
    mmd_anim_bullet_world *world,
    const mmd_anim_bullet_rigidbody_desc *desc,
    int32_t *out_index);
MMD_ANIM_BULLET_API int32_t
mmd_anim_bullet_world_get_rigidbody_count(const mmd_anim_bullet_world *world);
MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_get_rigidbody_transform(
    const mmd_anim_bullet_world *world,
    int32_t index,
    float out_position[3],
    float out_rotation_xyzw[4]);
MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_set_rigidbody_transform(
    mmd_anim_bullet_world *world,
    int32_t index,
    const float position[3],
    const float rotation_xyzw[4]);
MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_add_6dof_spring_joint(
    mmd_anim_bullet_world *world,
    const mmd_anim_bullet_6dof_spring_joint_desc *desc,
    int32_t *out_index);
MMD_ANIM_BULLET_API int32_t
mmd_anim_bullet_world_get_constraint_count(const mmd_anim_bullet_world *world);
MMD_ANIM_BULLET_API mmd_anim_bullet_status
mmd_anim_bullet_world_collect_contacts(
    const mmd_anim_bullet_world *world,
    mmd_anim_bullet_contact_point *out_contacts,
    int32_t capacity,
    int32_t *out_count);

#ifdef __cplusplus
}
#endif
