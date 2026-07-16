//! Optional Bullet backend for mmd-anim physics.
//!
//! The default build has no native dependency. Enabling the `native` feature
//! builds the C++ backend from the vendored Bullet3 source. Maintainers can set
//! `MMD_ANIM_BULLET3_DIR` to test against another Bullet3 checkout.

pub const NATIVE_FEATURE_ENABLED: bool = cfg!(feature = "native");

#[cfg(feature = "native")]
mod native;
#[cfg(feature = "native")]
mod pmx;
#[cfg(all(feature = "native", feature = "runtime"))]
mod runtime;
#[cfg(all(test, feature = "native", feature = "pmx-format"))]
mod test_support;

#[cfg(feature = "native")]
pub use native::{
    BulletError, BulletWorld, ConstraintHandle, ContactPoint, RigidBodyDesc, RigidBodyHandle,
    RigidBodyShape, SixDofSpringJointDesc, Transform,
};
#[cfg(all(feature = "native", feature = "pmx-format"))]
pub use pmx::build_bullet_world_from_pmx;
#[cfg(feature = "native")]
pub use pmx::{
    PhysicsJointDescriptor, PhysicsJointKind, PhysicsRigidBodyDescriptor, PmxBulletBuildReport,
    PmxBulletWorld, PmxRigidBodyBinding, PmxRigidBodyMode, build_bullet_world_from_descriptors,
};
#[cfg(all(feature = "native", feature = "runtime"))]
pub use runtime::RuntimePhysicsBridgeExt;
