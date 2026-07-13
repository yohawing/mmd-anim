#include "mmd_bullet_api.h"

#include <btBulletDynamicsCommon.h>

#include <cmath>
#include <memory>
#include <string>
#include <vector>

thread_local std::string g_last_error;

struct RigidBodyEntry {
    std::unique_ptr<btCollisionShape> shape;
    std::unique_ptr<btDefaultMotionState> motion_state;
    std::unique_ptr<btRigidBody> body;
    btTransform initial_transform;
};

struct mmd_anim_bullet_world {
    std::unique_ptr<btDefaultCollisionConfiguration> collision_configuration;
    std::unique_ptr<btCollisionDispatcher> dispatcher;
    std::unique_ptr<btDbvtBroadphase> broadphase;
    std::unique_ptr<btSequentialImpulseConstraintSolver> solver;
    std::unique_ptr<btDiscreteDynamicsWorld> dynamics_world;
    std::vector<RigidBodyEntry> rigidbodies;
    std::vector<std::unique_ptr<btTypedConstraint>> constraints;
};

static mmd_anim_bullet_status fail(mmd_anim_bullet_status status, const char *message) {
    g_last_error = message;
    return status;
}

static btTransform make_transform(const float position[3], const float euler[3]) {
    btTransform transform;
    transform.setIdentity();
    transform.setOrigin(btVector3(position[0], position[1], position[2]));
    btQuaternion rotation;
    rotation.setEulerZYX(euler[2], euler[1], euler[0]);
    transform.setRotation(rotation);
    return transform;
}

static void set_vec3_limit(btGeneric6DofSpringConstraint &constraint, const float lower[3], const float upper[3]) {
    constraint.setLinearLowerLimit(btVector3(lower[0], lower[1], lower[2]));
    constraint.setLinearUpperLimit(btVector3(upper[0], upper[1], upper[2]));
}

static void set_angular_limit(btGeneric6DofSpringConstraint &constraint, const float lower[3], const float upper[3]) {
    constraint.setAngularLowerLimit(btVector3(lower[0], lower[1], lower[2]));
    constraint.setAngularUpperLimit(btVector3(upper[0], upper[1], upper[2]));
}

static void configure_spring_axis(btGeneric6DofSpringConstraint &constraint, int axis, float stiffness) {
    if (stiffness > 0.0f) {
        constraint.enableSpring(axis, true);
        constraint.setStiffness(axis, stiffness);
        constraint.setEquilibriumPoint(axis);
    }
}

static btCollisionShape *make_shape(const mmd_anim_bullet_rigidbody_desc &desc) {
    switch (desc.shape_type) {
    case MMD_ANIM_BULLET_SHAPE_SPHERE:
        return new btSphereShape(btMax(desc.shape_size[0], 0.0001f));
    case MMD_ANIM_BULLET_SHAPE_BOX:
        return new btBoxShape(btVector3(
            btMax(desc.shape_size[0], 0.0001f),
            btMax(desc.shape_size[1], 0.0001f),
            btMax(desc.shape_size[2], 0.0001f)));
    case MMD_ANIM_BULLET_SHAPE_CAPSULE:
        return new btCapsuleShape(btMax(desc.shape_size[0], 0.0001f), btMax(desc.shape_size[1], 0.0001f));
    default:
        return nullptr;
    }
}

static int32_t rigidbody_index_for_collision_object(
    const mmd_anim_bullet_world *world,
    const btCollisionObject *object) {
    if (!world || !object) {
        return -1;
    }
    for (size_t i = 0; i < world->rigidbodies.size(); ++i) {
        if (world->rigidbodies[i].body.get() == object) {
            return static_cast<int32_t>(i);
        }
    }
    return -1;
}

static void copy_vec3(const btVector3 &source, float target[3]) {
    target[0] = source.x();
    target[1] = source.y();
    target[2] = source.z();
}

extern "C" {

uint32_t mmd_anim_bullet_get_version(void) {
    return 1;
}

const char *mmd_anim_bullet_get_last_error(void) {
    return g_last_error.c_str();
}

mmd_anim_bullet_status mmd_anim_bullet_world_create(mmd_anim_bullet_world **out_world) {
    if (!out_world) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "out_world is null");
    }

    try {
        auto world = std::make_unique<mmd_anim_bullet_world>();
        world->collision_configuration = std::make_unique<btDefaultCollisionConfiguration>();
        world->dispatcher = std::make_unique<btCollisionDispatcher>(world->collision_configuration.get());
        world->broadphase = std::make_unique<btDbvtBroadphase>();
        world->solver = std::make_unique<btSequentialImpulseConstraintSolver>();
        world->dynamics_world = std::make_unique<btDiscreteDynamicsWorld>(
            world->dispatcher.get(),
            world->broadphase.get(),
            world->solver.get(),
            world->collision_configuration.get());
        world->dynamics_world->setGravity(btVector3(0.0f, -98.0f, 0.0f));
        *out_world = world.release();
        g_last_error.clear();
        return MMD_ANIM_BULLET_OK;
    } catch (const std::exception &err) {
        return fail(MMD_ANIM_BULLET_INTERNAL_ERROR, err.what());
    }
}

void mmd_anim_bullet_world_destroy(mmd_anim_bullet_world *world) {
    if (!world) {
        return;
    }
    if (world->dynamics_world) {
        for (auto it = world->constraints.rbegin(); it != world->constraints.rend(); ++it) {
            world->dynamics_world->removeConstraint(it->get());
        }
        for (auto it = world->rigidbodies.rbegin(); it != world->rigidbodies.rend(); ++it) {
            world->dynamics_world->removeRigidBody(it->body.get());
        }
    }
    delete world;
}

mmd_anim_bullet_status mmd_anim_bullet_world_reset(mmd_anim_bullet_world *world) {
    if (!world) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world is null");
    }

    for (auto &entry : world->rigidbodies) {
        entry.body->setWorldTransform(entry.initial_transform);
        entry.body->setInterpolationWorldTransform(entry.initial_transform);
        entry.body->setLinearVelocity(btVector3(0.0f, 0.0f, 0.0f));
        entry.body->setAngularVelocity(btVector3(0.0f, 0.0f, 0.0f));
        entry.body->clearForces();
        if (entry.motion_state) {
            entry.motion_state->setWorldTransform(entry.initial_transform);
        }
    }
    for (auto &constraint : world->constraints) {
        constraint->setEnabled(true);
    }
    world->dynamics_world->getBroadphase()->getOverlappingPairCache()->cleanProxyFromPairs(nullptr, world->dynamics_world->getDispatcher());
    g_last_error.clear();
    return MMD_ANIM_BULLET_OK;
}

mmd_anim_bullet_status mmd_anim_bullet_world_settle_to_current(mmd_anim_bullet_world *world) {
    if (!world) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world is null");
    }

    world->dynamics_world->clearForces();
    btOverlappingPairCache *pair_cache = world->dynamics_world->getPairCache();
    btDispatcher *dispatcher = world->dynamics_world->getDispatcher();

    for (auto &entry : world->rigidbodies) {
        btRigidBody *body = entry.body.get();
        body->setInterpolationWorldTransform(body->getWorldTransform());
        body->setLinearVelocity(btVector3(0.0f, 0.0f, 0.0f));
        body->setAngularVelocity(btVector3(0.0f, 0.0f, 0.0f));
        body->setInterpolationLinearVelocity(btVector3(0.0f, 0.0f, 0.0f));
        body->setInterpolationAngularVelocity(btVector3(0.0f, 0.0f, 0.0f));
        body->clearForces();
        body->activate(true);

        if (pair_cache && dispatcher && body->getBroadphaseHandle()) {
            pair_cache->cleanProxyFromPairs(body->getBroadphaseHandle(), dispatcher);
        }
    }

    g_last_error.clear();
    return MMD_ANIM_BULLET_OK;
}

mmd_anim_bullet_status mmd_anim_bullet_world_step(
    mmd_anim_bullet_world *world,
    float delta_time,
    int32_t max_sub_steps,
    float fixed_substep_seconds) {
    if (!world) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world is null");
    }
    if (!std::isfinite(delta_time) || delta_time < 0.0f || max_sub_steps < 0 ||
        !std::isfinite(fixed_substep_seconds) || fixed_substep_seconds <= 0.0f) {
        return fail(
            MMD_ANIM_BULLET_INVALID_ARGUMENT,
            "delta_time and max_sub_steps must be non-negative and fixed_substep_seconds must be positive");
    }

    world->dynamics_world->stepSimulation(delta_time, max_sub_steps, fixed_substep_seconds);
    g_last_error.clear();
    return MMD_ANIM_BULLET_OK;
}

mmd_anim_bullet_status mmd_anim_bullet_world_add_rigidbody(
    mmd_anim_bullet_world *world,
    const mmd_anim_bullet_rigidbody_desc *desc,
    int32_t *out_index) {
    if (!world || !desc || !out_index) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world, desc, or out_index is null");
    }

    try {
        std::unique_ptr<btCollisionShape> shape(make_shape(*desc));
        if (!shape) {
            return fail(MMD_ANIM_BULLET_INVALID_ARGUMENT, "unknown shape type");
        }

        btTransform initial_transform = make_transform(desc->position, desc->rotation_euler);
        btVector3 inertia(0.0f, 0.0f, 0.0f);
        const btScalar mass = btMax(desc->mass, 0.0f);
        if (mass > 0.0f) {
            shape->calculateLocalInertia(mass, inertia);
        }

        auto motion_state = std::make_unique<btDefaultMotionState>(initial_transform);
        btRigidBody::btRigidBodyConstructionInfo info(mass, motion_state.get(), shape.get(), inertia);
        info.m_linearDamping = btMax(desc->linear_damping, 0.0f);
        info.m_angularDamping = btMax(desc->angular_damping, 0.0f);
        info.m_friction = btMax(desc->friction, 0.0f);
        info.m_restitution = btMax(desc->restitution, 0.0f);

        auto body = std::make_unique<btRigidBody>(info);
        if (mass == 0.0f) {
            body->setCollisionFlags(body->getCollisionFlags() | btCollisionObject::CF_KINEMATIC_OBJECT);
        }
        body->setActivationState(DISABLE_DEACTIVATION);

        const int group = 1 << btMin<uint16_t>(desc->collision_group, 15);
        const int mask = static_cast<int>(desc->collision_mask);
        world->dynamics_world->addRigidBody(body.get(), group, mask);

        RigidBodyEntry entry;
        entry.shape = std::move(shape);
        entry.motion_state = std::move(motion_state);
        entry.body = std::move(body);
        entry.initial_transform = initial_transform;
        world->rigidbodies.push_back(std::move(entry));
        *out_index = static_cast<int32_t>(world->rigidbodies.size() - 1);
        g_last_error.clear();
        return MMD_ANIM_BULLET_OK;
    } catch (const std::exception &err) {
        return fail(MMD_ANIM_BULLET_INTERNAL_ERROR, err.what());
    }
}

int32_t mmd_anim_bullet_world_get_rigidbody_count(const mmd_anim_bullet_world *world) {
    if (!world) {
        g_last_error = "world is null";
        return -1;
    }
    g_last_error.clear();
    return static_cast<int32_t>(world->rigidbodies.size());
}

mmd_anim_bullet_status mmd_anim_bullet_world_get_rigidbody_transform(
    const mmd_anim_bullet_world *world,
    int32_t index,
    float out_position[3],
    float out_rotation_xyzw[4]) {
    if (!world || !out_position || !out_rotation_xyzw) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world or output buffer is null");
    }
    if (index < 0 || static_cast<size_t>(index) >= world->rigidbodies.size()) {
        return fail(MMD_ANIM_BULLET_INVALID_ARGUMENT, "rigidbody index out of range");
    }

    btTransform transform;
    const auto &entry = world->rigidbodies[static_cast<size_t>(index)];
    entry.body->getMotionState()->getWorldTransform(transform);
    const btVector3 origin = transform.getOrigin();
    const btQuaternion rotation = transform.getRotation();
    out_position[0] = origin.x();
    out_position[1] = origin.y();
    out_position[2] = origin.z();
    out_rotation_xyzw[0] = rotation.x();
    out_rotation_xyzw[1] = rotation.y();
    out_rotation_xyzw[2] = rotation.z();
    out_rotation_xyzw[3] = rotation.w();
    g_last_error.clear();
    return MMD_ANIM_BULLET_OK;
}

mmd_anim_bullet_status mmd_anim_bullet_world_set_rigidbody_transform(
    mmd_anim_bullet_world *world,
    int32_t index,
    const float position[3],
    const float rotation_xyzw[4]) {
    if (!world || !position || !rotation_xyzw) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world or input buffer is null");
    }
    if (index < 0 || static_cast<size_t>(index) >= world->rigidbodies.size()) {
        return fail(MMD_ANIM_BULLET_INVALID_ARGUMENT, "rigidbody index out of range");
    }

    btTransform transform;
    transform.setIdentity();
    transform.setOrigin(btVector3(position[0], position[1], position[2]));
    transform.setRotation(btQuaternion(rotation_xyzw[0], rotation_xyzw[1], rotation_xyzw[2], rotation_xyzw[3]));

    auto &entry = world->rigidbodies[static_cast<size_t>(index)];
    entry.body->setWorldTransform(transform);
    if (entry.motion_state) {
        entry.motion_state->setWorldTransform(transform);
    }
    g_last_error.clear();
    return MMD_ANIM_BULLET_OK;
}

mmd_anim_bullet_status mmd_anim_bullet_world_add_6dof_spring_joint(
    mmd_anim_bullet_world *world,
    const mmd_anim_bullet_6dof_spring_joint_desc *desc,
    int32_t *out_index) {
    if (!world || !desc || !out_index) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world, desc, or out_index is null");
    }
    if (desc->rigidbody_index_a < 0 || desc->rigidbody_index_b < 0 ||
        static_cast<size_t>(desc->rigidbody_index_a) >= world->rigidbodies.size() ||
        static_cast<size_t>(desc->rigidbody_index_b) >= world->rigidbodies.size()) {
        return fail(MMD_ANIM_BULLET_INVALID_ARGUMENT, "joint rigidbody index out of range");
    }

    try {
        auto &body_a = *world->rigidbodies[static_cast<size_t>(desc->rigidbody_index_a)].body;
        auto &body_b = *world->rigidbodies[static_cast<size_t>(desc->rigidbody_index_b)].body;
        btTransform joint_transform = make_transform(desc->position, desc->rotation_euler);
        btTransform frame_a = body_a.getWorldTransform().inverse() * joint_transform;
        btTransform frame_b = body_b.getWorldTransform().inverse() * joint_transform;

        auto constraint = std::make_unique<btGeneric6DofSpringConstraint>(body_a, body_b, frame_a, frame_b, true);
        set_vec3_limit(*constraint, desc->translation_lower_limit, desc->translation_upper_limit);
        set_angular_limit(*constraint, desc->rotation_lower_limit, desc->rotation_upper_limit);
        for (int axis = 0; axis < 3; ++axis) {
            configure_spring_axis(*constraint, axis, desc->spring_translation_factor[axis]);
            configure_spring_axis(*constraint, axis + 3, desc->spring_rotation_factor[axis]);
        }

        world->dynamics_world->addConstraint(constraint.get(), true);
        world->constraints.push_back(std::move(constraint));
        *out_index = static_cast<int32_t>(world->constraints.size() - 1);
        g_last_error.clear();
        return MMD_ANIM_BULLET_OK;
    } catch (const std::exception &err) {
        return fail(MMD_ANIM_BULLET_INTERNAL_ERROR, err.what());
    }
}

int32_t mmd_anim_bullet_world_get_constraint_count(const mmd_anim_bullet_world *world) {
    if (!world) {
        g_last_error = "world is null";
        return -1;
    }
    g_last_error.clear();
    return static_cast<int32_t>(world->constraints.size());
}

mmd_anim_bullet_status mmd_anim_bullet_world_collect_contacts(
    const mmd_anim_bullet_world *world,
    mmd_anim_bullet_contact_point *out_contacts,
    int32_t capacity,
    int32_t *out_count) {
    if (!world || !out_count) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "world or out_count is null");
    }
    if (capacity < 0) {
        return fail(MMD_ANIM_BULLET_INVALID_ARGUMENT, "capacity must be non-negative");
    }
    if (capacity > 0 && !out_contacts) {
        return fail(MMD_ANIM_BULLET_NULL_POINTER, "out_contacts is null with non-zero capacity");
    }

    int32_t count = 0;
    btDispatcher *dispatcher = world->dynamics_world->getDispatcher();
    const int manifold_count = dispatcher->getNumManifolds();
    for (int manifold_index = 0; manifold_index < manifold_count; ++manifold_index) {
        btPersistentManifold *manifold = dispatcher->getManifoldByIndexInternal(manifold_index);
        if (!manifold) {
            continue;
        }
        const int32_t body_a = rigidbody_index_for_collision_object(
            world,
            static_cast<const btCollisionObject *>(manifold->getBody0()));
        const int32_t body_b = rigidbody_index_for_collision_object(
            world,
            static_cast<const btCollisionObject *>(manifold->getBody1()));
        if (body_a < 0 || body_b < 0) {
            continue;
        }
        const int contact_count = manifold->getNumContacts();
        for (int contact_index = 0; contact_index < contact_count; ++contact_index) {
            const btManifoldPoint &point = manifold->getContactPoint(contact_index);
            if (count < capacity) {
                auto &out = out_contacts[count];
                out.rigidbody_index_a = body_a;
                out.rigidbody_index_b = body_b;
                out.distance = point.getDistance();
                copy_vec3(point.getPositionWorldOnA(), out.position_world_on_a);
                copy_vec3(point.getPositionWorldOnB(), out.position_world_on_b);
                copy_vec3(point.m_normalWorldOnB, out.normal_world_on_b);
            }
            ++count;
        }
    }

    *out_count = count;
    g_last_error.clear();
    return MMD_ANIM_BULLET_OK;
}

}
