use std::ffi::CStr;
use std::ptr::{self, NonNull};

use glam::{Mat4, Quat, Vec3};
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstraintHandle(i32);

impl ConstraintHandle {
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
pub struct SixDofSpringJointDesc {
    pub rigidbody_a: RigidBodyHandle,
    pub rigidbody_b: RigidBodyHandle,
    pub position: [f32; 3],
    pub rotation_euler: [f32; 3],
    pub translation_lower_limit: [f32; 3],
    pub translation_upper_limit: [f32; 3],
    pub rotation_lower_limit: [f32; 3],
    pub rotation_upper_limit: [f32; 3],
    pub spring_translation_factor: [f32; 3],
    pub spring_rotation_factor: [f32; 3],
}

impl SixDofSpringJointDesc {
    pub fn locked(
        rigidbody_a: RigidBodyHandle,
        rigidbody_b: RigidBodyHandle,
        position: [f32; 3],
    ) -> Self {
        Self {
            rigidbody_a,
            rigidbody_b,
            position,
            rotation_euler: [0.0; 3],
            translation_lower_limit: [0.0; 3],
            translation_upper_limit: [0.0; 3],
            rotation_lower_limit: [0.0; 3],
            rotation_upper_limit: [0.0; 3],
            spring_translation_factor: [0.0; 3],
            spring_rotation_factor: [0.0; 3],
        }
    }

    fn to_ffi(self) -> ffi::SixDofSpringJointDesc {
        ffi::SixDofSpringJointDesc {
            rigidbody_index_a: self.rigidbody_a.0,
            rigidbody_index_b: self.rigidbody_b.0,
            position: self.position,
            rotation_euler: self.rotation_euler,
            translation_lower_limit: self.translation_lower_limit,
            translation_upper_limit: self.translation_upper_limit,
            rotation_lower_limit: self.rotation_lower_limit,
            rotation_upper_limit: self.rotation_upper_limit,
            spring_translation_factor: self.spring_translation_factor,
            spring_rotation_factor: self.spring_rotation_factor,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation_xyzw: [f32; 4],
}

impl Transform {
    pub const IDENTITY: Self = Self {
        position: [0.0; 3],
        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
    };

    pub fn from_translation(position: [f32; 3]) -> Self {
        Self {
            position,
            rotation_xyzw: Self::IDENTITY.rotation_xyzw,
        }
    }

    pub fn to_mat4(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::ONE,
            Quat::from_xyzw(
                self.rotation_xyzw[0],
                self.rotation_xyzw[1],
                self.rotation_xyzw[2],
                self.rotation_xyzw[3],
            ),
            Vec3::from_array(self.position),
        )
    }

    pub fn from_mat4(matrix: Mat4) -> Self {
        let (_scale, rotation, translation) = matrix.to_scale_rotation_translation();
        Self {
            position: translation.to_array(),
            rotation_xyzw: rotation.to_array(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactPoint {
    pub rigidbody_a: RigidBodyHandle,
    pub rigidbody_b: RigidBodyHandle,
    pub distance: f32,
    pub position_world_on_a: [f32; 3],
    pub position_world_on_b: [f32; 3],
    pub normal_world_on_b: [f32; 3],
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

    pub fn settle_to_current(&mut self) -> Result<(), BulletError> {
        check(unsafe { ffi::mmd_anim_bullet_world_settle_to_current(self.raw.as_ptr()) })
    }

    pub fn step(&mut self, delta_time: f32, max_sub_steps: i32) -> Result<(), BulletError> {
        self.step_with_fixed_substep(delta_time, max_sub_steps, 1.0 / 120.0)
    }

    pub fn step_with_fixed_substep(
        &mut self,
        delta_time: f32,
        max_sub_steps: i32,
        fixed_substep_seconds: f32,
    ) -> Result<(), BulletError> {
        check(unsafe {
            ffi::mmd_anim_bullet_world_step(
                self.raw.as_ptr(),
                delta_time,
                max_sub_steps,
                fixed_substep_seconds,
            )
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

    pub fn add_6dof_spring_joint(
        &mut self,
        desc: SixDofSpringJointDesc,
    ) -> Result<ConstraintHandle, BulletError> {
        let ffi_desc = desc.to_ffi();
        let mut index = -1;
        check(unsafe {
            ffi::mmd_anim_bullet_world_add_6dof_spring_joint(
                self.raw.as_ptr(),
                &ffi_desc,
                &mut index,
            )
        })?;
        Ok(ConstraintHandle(index))
    }

    pub fn constraint_count(&self) -> Result<usize, BulletError> {
        let count = unsafe { ffi::mmd_anim_bullet_world_get_constraint_count(self.raw.as_ptr()) };
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

    pub fn contact_points(&self) -> Result<Vec<ContactPoint>, BulletError> {
        let mut count = 0;
        check(unsafe {
            ffi::mmd_anim_bullet_world_collect_contacts(
                self.raw.as_ptr(),
                ptr::null_mut(),
                0,
                &mut count,
            )
        })?;
        if count <= 0 {
            return Ok(Vec::new());
        }

        let mut contacts = vec![ffi::ContactPoint::default(); count as usize];
        let mut written = 0;
        check(unsafe {
            ffi::mmd_anim_bullet_world_collect_contacts(
                self.raw.as_ptr(),
                contacts.as_mut_ptr(),
                contacts.len() as i32,
                &mut written,
            )
        })?;
        contacts.truncate(written.max(0) as usize);
        Ok(contacts
            .into_iter()
            .map(|contact| ContactPoint {
                rigidbody_a: RigidBodyHandle(contact.rigidbody_index_a),
                rigidbody_b: RigidBodyHandle(contact.rigidbody_index_b),
                distance: contact.distance,
                position_world_on_a: contact.position_world_on_a,
                position_world_on_b: contact.position_world_on_b,
                normal_world_on_b: contact.normal_world_on_b,
            })
            .collect())
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

    #[repr(C)]
    pub struct SixDofSpringJointDesc {
        pub rigidbody_index_a: i32,
        pub rigidbody_index_b: i32,
        pub position: [f32; 3],
        pub rotation_euler: [f32; 3],
        pub translation_lower_limit: [f32; 3],
        pub translation_upper_limit: [f32; 3],
        pub rotation_lower_limit: [f32; 3],
        pub rotation_upper_limit: [f32; 3],
        pub spring_translation_factor: [f32; 3],
        pub spring_rotation_factor: [f32; 3],
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct ContactPoint {
        pub rigidbody_index_a: i32,
        pub rigidbody_index_b: i32,
        pub distance: f32,
        pub position_world_on_a: [f32; 3],
        pub position_world_on_b: [f32; 3],
        pub normal_world_on_b: [f32; 3],
    }

    unsafe extern "C" {
        pub fn mmd_anim_bullet_get_last_error() -> *const c_char;
        pub fn mmd_anim_bullet_world_create(out_world: *mut *mut World) -> i32;
        pub fn mmd_anim_bullet_world_destroy(world: *mut World);
        pub fn mmd_anim_bullet_world_reset(world: *mut World) -> i32;
        pub fn mmd_anim_bullet_world_settle_to_current(world: *mut World) -> i32;
        pub fn mmd_anim_bullet_world_step(
            world: *mut World,
            delta_time: f32,
            max_sub_steps: i32,
            fixed_substep_seconds: f32,
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
        pub fn mmd_anim_bullet_world_add_6dof_spring_joint(
            world: *mut World,
            desc: *const SixDofSpringJointDesc,
            out_index: *mut i32,
        ) -> i32;
        pub fn mmd_anim_bullet_world_get_constraint_count(world: *const World) -> i32;
        pub fn mmd_anim_bullet_world_collect_contacts(
            world: *const World,
            out_contacts: *mut ContactPoint,
            capacity: i32,
            out_count: *mut i32,
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
    fn custom_fixed_substep_is_used_by_bullet() {
        let mut world = BulletWorld::new().unwrap();
        let body = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(1.0, [0.0, 10.0, 0.0], 1.0))
            .unwrap();

        for _ in 0..2 {
            world
                .step_with_fixed_substep(1.0 / 60.0, 1, 1.0 / 60.0)
                .unwrap();
        }
        let after = world.rigidbody_transform(body).unwrap();

        assert!(
            after.position[1] < 9.999,
            "1/60 fixed step should consume the full interval: {after:?}"
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

    #[test]
    fn locked_6dof_joint_constrains_dynamic_body() {
        let mut world = BulletWorld::new().unwrap();
        let anchor = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(0.5, [0.0, 10.0, 0.0], 0.0))
            .unwrap();
        let bob = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(0.5, [0.0, 8.0, 0.0], 1.0))
            .unwrap();

        let joint = world
            .add_6dof_spring_joint(SixDofSpringJointDesc::locked(anchor, bob, [0.0, 9.0, 0.0]))
            .unwrap();
        world.step(0.5, 60).unwrap();
        let bob_after = world.rigidbody_transform(bob).unwrap();

        assert_eq!(joint.index(), 0);
        assert_eq!(world.constraint_count().unwrap(), 1);
        assert!(
            bob_after.position[1] > 6.0,
            "expected locked joint to prevent free fall: bob_after={bob_after:?}"
        );
    }

    #[test]
    fn unity_parity_dynamic_body_rests_on_static_floor() {
        let mut world = BulletWorld::new().unwrap();
        let _floor = world
            .add_rigidbody(RigidBodyDesc {
                shape: RigidBodyShape::Box {
                    half_extents: [20.0, 1.0, 20.0],
                },
                position: [0.0, 0.0, 0.0],
                rotation_euler: [0.0; 3],
                mass: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                friction: 0.5,
                restitution: 0.0,
                collision_group: 0,
                collision_mask: 0xffff,
            })
            .unwrap();
        let sphere = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(1.0, [0.0, 10.0, 0.0], 1.0))
            .unwrap();

        for _ in 0..180 {
            world.step(1.0 / 60.0, 2).unwrap();
        }
        let sphere_y = world.rigidbody_transform(sphere).unwrap().position[1];
        eprintln!("rust floor sphere_y {sphere_y}");

        assert!(sphere_y > 1.0, "sphere fell through floor: y={sphere_y}");
        assert!(
            sphere_y < 9.0,
            "sphere did not fall meaningfully: y={sphere_y}"
        );
    }

    #[test]
    fn contact_points_report_resting_body_contact() {
        let mut world = BulletWorld::new().unwrap();
        let _floor = world
            .add_rigidbody(RigidBodyDesc {
                shape: RigidBodyShape::Box {
                    half_extents: [20.0, 1.0, 20.0],
                },
                position: [0.0, 0.0, 0.0],
                rotation_euler: [0.0; 3],
                mass: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                friction: 0.5,
                restitution: 0.0,
                collision_group: 0,
                collision_mask: 0xffff,
            })
            .unwrap();
        let sphere = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(1.0, [0.0, 10.0, 0.0], 1.0))
            .unwrap();

        for _ in 0..180 {
            world.step(1.0 / 60.0, 2).unwrap();
        }

        let contacts = world.contact_points().unwrap();
        assert!(
            contacts
                .iter()
                .any(|contact| { contact.rigidbody_a == sphere || contact.rigidbody_b == sphere }),
            "expected contact involving resting sphere: {contacts:?}"
        );
    }

    #[test]
    fn unity_parity_kinematic_floor_drags_resting_dynamic_body() {
        let mut world = BulletWorld::new().unwrap();
        let floor = world
            .add_rigidbody(RigidBodyDesc {
                shape: RigidBodyShape::Box {
                    half_extents: [50.0, 1.0, 50.0],
                },
                position: [0.0, -1.0, 0.0],
                rotation_euler: [0.0; 3],
                mass: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                friction: 1.0,
                restitution: 0.0,
                collision_group: 0,
                collision_mask: 0xffff,
            })
            .unwrap();
        let block = world
            .add_rigidbody(RigidBodyDesc {
                shape: RigidBodyShape::Box {
                    half_extents: [1.0, 1.0, 1.0],
                },
                position: [0.0, 3.0, 0.0],
                rotation_euler: [0.0; 3],
                mass: 1.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                friction: 1.0,
                restitution: 0.0,
                collision_group: 0,
                collision_mask: 0xffff,
            })
            .unwrap();

        for _ in 0..60 {
            world.step(1.0 / 60.0, 2).unwrap();
        }
        let resting_x = world.rigidbody_transform(block).unwrap().position[0];

        let mut floor_x = 0.0;
        for _ in 0..60 {
            floor_x += 0.25;
            world
                .set_rigidbody_transform(
                    floor,
                    Transform {
                        position: [floor_x, -1.0, 0.0],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    },
                )
                .unwrap();
            world.step(1.0 / 60.0, 2).unwrap();
        }

        let block_after = world.rigidbody_transform(block).unwrap();
        eprintln!(
            "rust drag dx block_y {} {}",
            block_after.position[0] - resting_x,
            block_after.position[1]
        );
        assert!(
            block_after.position[1] > -1.0,
            "block fell through floor: {block_after:?}"
        );
        assert!(
            block_after.position[0] - resting_x > 1.0,
            "kinematic floor did not drag block: resting_x={resting_x}, after={block_after:?}"
        );
    }

    #[test]
    fn unity_parity_kinematic_anchor_drags_jointed_dynamic_body() {
        let mut world = BulletWorld::new().unwrap();
        let anchor = world
            .add_rigidbody(RigidBodyDesc {
                shape: RigidBodyShape::Box {
                    half_extents: [0.5, 0.5, 0.5],
                },
                position: [0.0, 10.0, 0.0],
                rotation_euler: [0.0; 3],
                mass: 0.0,
                linear_damping: 0.0,
                angular_damping: 0.0,
                friction: 0.5,
                restitution: 0.0,
                collision_group: 0,
                collision_mask: 0xffff,
            })
            .unwrap();
        let hair = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(0.5, [0.0, 8.0, 0.0], 1.0))
            .unwrap();
        world
            .add_6dof_spring_joint(SixDofSpringJointDesc {
                rigidbody_a: anchor,
                rigidbody_b: hair,
                position: [0.0, 9.0, 0.0],
                rotation_euler: [0.0; 3],
                translation_lower_limit: [0.0; 3],
                translation_upper_limit: [0.0; 3],
                rotation_lower_limit: [-std::f32::consts::PI; 3],
                rotation_upper_limit: [std::f32::consts::PI; 3],
                spring_translation_factor: [0.0; 3],
                spring_rotation_factor: [0.0; 3],
            })
            .unwrap();

        for _ in 0..60 {
            world.step(1.0 / 60.0, 2).unwrap();
        }
        let rest_x = world.rigidbody_transform(hair).unwrap().position[0];

        let mut anchor_x = 0.0;
        for _ in 0..80 {
            anchor_x += 0.25;
            world
                .set_rigidbody_transform(
                    anchor,
                    Transform {
                        position: [anchor_x, 10.0, 0.0],
                        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                    },
                )
                .unwrap();
            world.step(1.0 / 60.0, 2).unwrap();
        }

        let dragged = world.rigidbody_transform(hair).unwrap();
        eprintln!(
            "rust anchor dx hair_y {} {}",
            dragged.position[0] - rest_x,
            dragged.position[1]
        );
        assert!(
            dragged.position[1] > 0.0,
            "joint did not hold dynamic body: {dragged:?}"
        );
        assert!(
            dragged.position[0] - rest_x > 5.0,
            "jointed dynamic body did not follow anchor: rest_x={rest_x}, dragged={dragged:?}"
        );
    }

    #[test]
    fn settle_to_current_keeps_teleported_body_stable_until_stepped() {
        let mut world = BulletWorld::new().unwrap();
        let body = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(1.0, [0.0, 10.0, 0.0], 1.0))
            .unwrap();

        world
            .set_rigidbody_transform(
                body,
                Transform {
                    position: [0.0, 20.0, 0.0],
                    rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                },
            )
            .unwrap();
        world.settle_to_current().unwrap();
        let settled = world.rigidbody_transform(body).unwrap();

        assert!((settled.position[1] - 20.0).abs() < 1.0e-4);
    }
}
