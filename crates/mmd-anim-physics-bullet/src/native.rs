use std::ffi::CStr;
use std::ptr::{self, NonNull};

use thiserror::Error;

#[derive(Debug, Error)]
#[error("Bullet backend error: {message}")]
pub struct BulletError {
    message: String,
}

impl BulletError {
    fn last() -> Self {
        let message = unsafe {
            let ptr = ffi::mmd_anim_bullet_get_last_error();
            if ptr.is_null() {
                "unknown Bullet error".to_owned()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };
        Self { message }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RigidBodyHandle(i32);

impl RigidBodyHandle {
    pub fn index(self) -> i32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RigidBodyShape {
    Sphere { radius: f32 },
    Box { half_extents: [f32; 3] },
    Capsule { radius: f32, height: f32 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidBodyDesc {
    pub shape: RigidBodyShape,
    pub position: [f32; 3],
    pub rotation_euler: [f32; 3],
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub friction: f32,
    pub restitution: f32,
    pub collision_group: u16,
    pub collision_mask: u16,
}

impl RigidBodyDesc {
    pub fn dynamic_sphere(radius: f32, position: [f32; 3], mass: f32) -> Self {
        Self {
            shape: RigidBodyShape::Sphere { radius },
            position,
            rotation_euler: [0.0; 3],
            mass,
            linear_damping: 0.0,
            angular_damping: 0.0,
            friction: 0.5,
            restitution: 0.0,
            collision_group: 0,
            collision_mask: 0xffff,
        }
    }

    fn to_ffi(self) -> ffi::RigidBodyDesc {
        let (shape_type, shape_size) = match self.shape {
            RigidBodyShape::Sphere { radius } => (ffi::SHAPE_SPHERE, [radius, 0.0, 0.0]),
            RigidBodyShape::Box { half_extents } => (ffi::SHAPE_BOX, half_extents),
            RigidBodyShape::Capsule { radius, height } => {
                (ffi::SHAPE_CAPSULE, [radius, height, 0.0])
            }
        };

        ffi::RigidBodyDesc {
            shape_type,
            shape_size,
            position: self.position,
            rotation_euler: self.rotation_euler,
            mass: self.mass,
            linear_damping: self.linear_damping,
            angular_damping: self.angular_damping,
            friction: self.friction,
            restitution: self.restitution,
            collision_group: self.collision_group,
            collision_mask: self.collision_mask,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation_xyzw: [f32; 4],
}

pub struct BulletWorld {
    raw: NonNull<ffi::World>,
}

impl BulletWorld {
    pub fn new() -> Result<Self, BulletError> {
        let mut raw = ptr::null_mut();
        check(unsafe { ffi::mmd_anim_bullet_world_create(&mut raw) })?;
        let raw = NonNull::new(raw).ok_or_else(BulletError::last)?;
        Ok(Self { raw })
    }

    pub fn reset(&mut self) -> Result<(), BulletError> {
        check(unsafe { ffi::mmd_anim_bullet_world_reset(self.raw.as_ptr()) })
    }

    pub fn step(&mut self, delta_time: f32, max_sub_steps: i32) -> Result<(), BulletError> {
        check(unsafe {
            ffi::mmd_anim_bullet_world_step(self.raw.as_ptr(), delta_time, max_sub_steps)
        })
    }

    pub fn add_rigidbody(&mut self, desc: RigidBodyDesc) -> Result<RigidBodyHandle, BulletError> {
        let ffi_desc = desc.to_ffi();
        let mut index = -1;
        check(unsafe {
            ffi::mmd_anim_bullet_world_add_rigidbody(self.raw.as_ptr(), &ffi_desc, &mut index)
        })?;
        Ok(RigidBodyHandle(index))
    }

    pub fn rigidbody_count(&self) -> Result<usize, BulletError> {
        let count = unsafe { ffi::mmd_anim_bullet_world_get_rigidbody_count(self.raw.as_ptr()) };
        if count < 0 {
            return Err(BulletError::last());
        }
        Ok(count as usize)
    }

    pub fn rigidbody_transform(&self, handle: RigidBodyHandle) -> Result<Transform, BulletError> {
        let mut position = [0.0; 3];
        let mut rotation_xyzw = [0.0; 4];
        check(unsafe {
            ffi::mmd_anim_bullet_world_get_rigidbody_transform(
                self.raw.as_ptr(),
                handle.0,
                position.as_mut_ptr(),
                rotation_xyzw.as_mut_ptr(),
            )
        })?;
        Ok(Transform {
            position,
            rotation_xyzw,
        })
    }

    pub fn set_rigidbody_transform(
        &mut self,
        handle: RigidBodyHandle,
        transform: Transform,
    ) -> Result<(), BulletError> {
        check(unsafe {
            ffi::mmd_anim_bullet_world_set_rigidbody_transform(
                self.raw.as_ptr(),
                handle.0,
                transform.position.as_ptr(),
                transform.rotation_xyzw.as_ptr(),
            )
        })
    }
}

impl Drop for BulletWorld {
    fn drop(&mut self) {
        unsafe {
            ffi::mmd_anim_bullet_world_destroy(self.raw.as_ptr());
        }
    }
}

fn check(status: i32) -> Result<(), BulletError> {
    if status == ffi::STATUS_OK {
        Ok(())
    } else {
        Err(BulletError::last())
    }
}

mod ffi {
    use std::ffi::c_char;

    pub enum World {}

    pub const STATUS_OK: i32 = 0;
    pub const SHAPE_SPHERE: i32 = 0;
    pub const SHAPE_BOX: i32 = 1;
    pub const SHAPE_CAPSULE: i32 = 2;

    #[repr(C)]
    pub struct RigidBodyDesc {
        pub shape_type: i32,
        pub shape_size: [f32; 3],
        pub position: [f32; 3],
        pub rotation_euler: [f32; 3],
        pub mass: f32,
        pub linear_damping: f32,
        pub angular_damping: f32,
        pub friction: f32,
        pub restitution: f32,
        pub collision_group: u16,
        pub collision_mask: u16,
    }

    unsafe extern "C" {
        pub fn mmd_anim_bullet_get_last_error() -> *const c_char;
        pub fn mmd_anim_bullet_world_create(out_world: *mut *mut World) -> i32;
        pub fn mmd_anim_bullet_world_destroy(world: *mut World);
        pub fn mmd_anim_bullet_world_reset(world: *mut World) -> i32;
        pub fn mmd_anim_bullet_world_step(
            world: *mut World,
            delta_time: f32,
            max_sub_steps: i32,
        ) -> i32;
        pub fn mmd_anim_bullet_world_add_rigidbody(
            world: *mut World,
            desc: *const RigidBodyDesc,
            out_index: *mut i32,
        ) -> i32;
        pub fn mmd_anim_bullet_world_get_rigidbody_count(world: *const World) -> i32;
        pub fn mmd_anim_bullet_world_get_rigidbody_transform(
            world: *const World,
            index: i32,
            out_position: *mut f32,
            out_rotation_xyzw: *mut f32,
        ) -> i32;
        pub fn mmd_anim_bullet_world_set_rigidbody_transform(
            world: *mut World,
            index: i32,
            position: *const f32,
            rotation_xyzw: *const f32,
        ) -> i32;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_rigidbody_falls_under_mmd_gravity() {
        let mut world = BulletWorld::new().unwrap();
        let body = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(1.0, [0.0, 10.0, 0.0], 1.0))
            .unwrap();

        let before = world.rigidbody_transform(body).unwrap();
        world.step(1.0 / 30.0, 10).unwrap();
        let after = world.rigidbody_transform(body).unwrap();

        assert_eq!(world.rigidbody_count().unwrap(), 1);
        assert!(
            after.position[1] < before.position[1],
            "expected y to decrease: before={before:?}, after={after:?}"
        );
    }

    #[test]
    fn reset_restores_initial_transform() {
        let mut world = BulletWorld::new().unwrap();
        let body = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(1.0, [0.0, 10.0, 0.0], 1.0))
            .unwrap();

        world.step(1.0 / 10.0, 10).unwrap();
        world.reset().unwrap();
        let transform = world.rigidbody_transform(body).unwrap();

        assert!((transform.position[1] - 10.0).abs() < 1.0e-4);
    }
}
