//! Optional Bullet backend for mmd-anim physics.
//!
//! The default build has no native dependency. Enable the `native` feature and
//! point `MMD_ANIM_BULLET3_DIR` at a Bullet checkout to build the C++ backend.

pub const NATIVE_FEATURE_ENABLED: bool = cfg!(feature = "native");

#[cfg(feature = "native")]
mod native;
#[cfg(all(feature = "native", feature = "pmx-format"))]
mod pmx;
#[cfg(all(feature = "native", feature = "pmx-format", feature = "runtime"))]
mod runtime;
#[cfg(all(test, feature = "native", feature = "pmx-format"))]
mod test_support;

#[cfg(feature = "native")]
pub use native::{
    BulletError, BulletWorld, ConstraintHandle, ContactPoint, RigidBodyDesc, RigidBodyHandle,
    RigidBodyShape, SixDofSpringJointDesc, Transform,
};
#[cfg(all(feature = "native", feature = "pmx-format"))]
pub use pmx::{
    PmxBulletBuildReport, PmxBulletWorld, PmxRigidBodyBinding, PmxRigidBodyMode,
    build_bullet_world_from_pmx,
};
#[cfg(all(feature = "native", feature = "pmx-format", feature = "runtime"))]
pub use runtime::RuntimePhysicsBridgeExt;
