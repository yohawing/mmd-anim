use glam::{EulerRot, Mat4, Quat, Vec3};
use mmd_anim_format::{
    PmxParsedModel,
    pmx::{PmxParsedJoint, PmxParsedRigidBody},
};
use thiserror::Error;

use crate::{
    BulletError, BulletWorld, RigidBodyDesc, RigidBodyHandle, RigidBodyShape,
    SixDofSpringJointDesc, Transform,
};

#[derive(Debug, Error)]
pub enum PmxBulletBuildError {
    #[error("unsupported PMX rigid body shape `{shape}` at index {index}")]
    UnsupportedRigidBodyShape { index: usize, shape: String },
    #[error(transparent)]
    Bullet(#[from] BulletError),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PmxBulletBuildReport {
    pub rigidbodies_added: usize,
    pub joints_added: usize,
    pub joints_skipped_invalid_body: usize,
    pub joints_skipped_unsupported_type: usize,
}

pub struct PmxBulletWorld {
    pub world: BulletWorld,
    pub rigidbody_handles: Vec<RigidBodyHandle>,
    pub rigidbody_bindings: Vec<PmxRigidBodyBinding>,
    pub report: PmxBulletBuildReport,
}

impl PmxBulletWorld {
    pub fn settle_to_current(&mut self) -> Result<(), BulletError> {
        self.world.settle_to_current()
    }

    pub fn feed_kinematic_rigidbodies(
        &mut self,
        bone_world_transforms: &[Transform],
    ) -> Result<usize, BulletError> {
        self.feed_kinematic_rigidbodies_with_options(bone_world_transforms, true)
    }

    pub fn feed_kinematic_rigidbodies_with_options(
        &mut self,
        bone_world_transforms: &[Transform],
        include_dynamic_bone: bool,
    ) -> Result<usize, BulletError> {
        let mut fed = 0;
        for (handle, binding) in self
            .rigidbody_handles
            .iter()
            .copied()
            .zip(self.rigidbody_bindings.iter())
        {
            if !binding.mode.follows_bone_before_step(include_dynamic_bone) {
                continue;
            }
            let Some(bone_index) = binding.bone_index else {
                continue;
            };
            let Some(transform) = bone_world_transforms.get(bone_index).copied() else {
                continue;
            };
            let body_world = transform.to_mat4() * binding.body_from_bone.to_mat4();
            self.world
                .set_rigidbody_transform(handle, Transform::from_mat4(body_world))?;
            fed += 1;
        }
        Ok(fed)
    }

    pub fn seed_rigidbodies_from_bones(
        &mut self,
        bone_world_transforms: &[Transform],
    ) -> Result<usize, BulletError> {
        let mut seeded = 0;
        for (handle, binding) in self
            .rigidbody_handles
            .iter()
            .copied()
            .zip(self.rigidbody_bindings.iter())
        {
            let Some(bone_index) = binding.bone_index else {
                continue;
            };
            let Some(transform) = bone_world_transforms.get(bone_index).copied() else {
                continue;
            };
            let body_world = transform.to_mat4() * binding.body_from_bone.to_mat4();
            self.world
                .set_rigidbody_transform(handle, Transform::from_mat4(body_world))?;
            seeded += 1;
        }
        Ok(seeded)
    }

    pub fn readback_bone_world_transforms(
        &self,
        bone_count: usize,
    ) -> Result<Vec<Option<Transform>>, BulletError> {
        let mut bone_transforms = vec![None; bone_count];
        for (handle, binding) in self
            .rigidbody_handles
            .iter()
            .copied()
            .zip(self.rigidbody_bindings.iter())
        {
            if !binding.mode.writes_back_to_bone() {
                continue;
            }
            let Some(bone_index) = binding.bone_index else {
                continue;
            };
            let Some(slot) = bone_transforms.get_mut(bone_index) else {
                continue;
            };
            let body_world = self.world.rigidbody_transform(handle)?.to_mat4();
            *slot = Some(Transform::from_mat4(
                body_world * binding.bone_from_body.to_mat4(),
            ));
        }
        Ok(bone_transforms)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PmxRigidBodyMode {
    Static,
    Dynamic,
    DynamicBone,
    Unknown,
}

impl PmxRigidBodyMode {
    pub fn follows_bone_before_step(self, include_dynamic_bone: bool) -> bool {
        matches!(self, Self::Static) || (include_dynamic_bone && matches!(self, Self::DynamicBone))
    }

    pub fn writes_back_to_bone(self) -> bool {
        matches!(self, Self::Dynamic | Self::DynamicBone)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PmxRigidBodyBinding {
    pub bone_index: Option<usize>,
    pub mode: PmxRigidBodyMode,
    pub body_from_bone: Transform,
    pub bone_from_body: Transform,
}

pub fn build_bullet_world_from_pmx(
    model: &PmxParsedModel,
) -> Result<PmxBulletWorld, PmxBulletBuildError> {
    let mut world = BulletWorld::new()?;
    let mut rigidbody_handles = Vec::with_capacity(model.rigid_bodies.len());
    let mut rigidbody_bindings = Vec::with_capacity(model.rigid_bodies.len());
    let mut report = PmxBulletBuildReport::default();

    for (index, body) in model.rigid_bodies.iter().enumerate() {
        let desc = rigidbody_desc_from_pmx(index, body)?;
        let handle = world.add_rigidbody(desc)?;
        rigidbody_handles.push(handle);
        rigidbody_bindings.push(PmxRigidBodyBinding {
            bone_index: if body.bone_index >= 0 {
                Some(body.bone_index as usize)
            } else {
                None
            },
            mode: rigidbody_mode_from_pmx(body.mode.as_str()),
            body_from_bone: body_from_bone_transform(model, body),
            bone_from_body: bone_from_body_transform(model, body),
        });
        report.rigidbodies_added += 1;
    }

    for joint in &model.joints {
        match joint_desc_from_pmx(joint, &rigidbody_handles) {
            JointMapping::Mapped(desc) => {
                world.add_6dof_spring_joint(desc)?;
                report.joints_added += 1;
            }
            JointMapping::InvalidBody => report.joints_skipped_invalid_body += 1,
            JointMapping::UnsupportedType => report.joints_skipped_unsupported_type += 1,
        }
    }

    Ok(PmxBulletWorld {
        world,
        rigidbody_handles,
        rigidbody_bindings,
        report,
    })
}

fn rigidbody_mode_from_pmx(mode: &str) -> PmxRigidBodyMode {
    match mode {
        "static" => PmxRigidBodyMode::Static,
        "dynamic" => PmxRigidBodyMode::Dynamic,
        "dynamicBone" => PmxRigidBodyMode::DynamicBone,
        _ => PmxRigidBodyMode::Unknown,
    }
}

fn body_from_bone_transform(model: &PmxParsedModel, body: &PmxParsedRigidBody) -> Transform {
    let body_bind = rigidbody_bind_transform(body);
    let Some(bone_index) = valid_bone_index(model, body.bone_index) else {
        return Transform::from_mat4(body_bind);
    };
    let bone_bind = bone_bind_transform(model, bone_index);
    Transform::from_mat4(bone_bind.inverse() * body_bind)
}

fn bone_from_body_transform(model: &PmxParsedModel, body: &PmxParsedRigidBody) -> Transform {
    let body_bind = rigidbody_bind_transform(body);
    let Some(bone_index) = valid_bone_index(model, body.bone_index) else {
        return Transform::from_mat4(body_bind.inverse());
    };
    let bone_bind = bone_bind_transform(model, bone_index);
    Transform::from_mat4(body_bind.inverse() * bone_bind)
}

fn valid_bone_index(model: &PmxParsedModel, bone_index: i32) -> Option<usize> {
    if bone_index < 0 {
        return None;
    }
    let bone_index = bone_index as usize;
    (bone_index < model.skeleton.bones.len()).then_some(bone_index)
}

fn rigidbody_bind_transform(body: &PmxParsedRigidBody) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::ONE,
        Quat::from_euler(
            EulerRot::ZYX,
            body.rotation[2],
            body.rotation[1],
            body.rotation[0],
        ),
        Vec3::from_array(body.position),
    )
}

fn bone_bind_transform(model: &PmxParsedModel, bone_index: usize) -> Mat4 {
    Mat4::from_translation(Vec3::from_array(model.skeleton.bones[bone_index].position))
}

fn rigidbody_desc_from_pmx(
    index: usize,
    body: &PmxParsedRigidBody,
) -> Result<RigidBodyDesc, PmxBulletBuildError> {
    let shape = match body.shape.as_str() {
        "sphere" => RigidBodyShape::Sphere {
            radius: body.size[0],
        },
        "box" => RigidBodyShape::Box {
            half_extents: body.size,
        },
        "capsule" => RigidBodyShape::Capsule {
            radius: body.size[0],
            height: body.size[1],
        },
        shape => {
            return Err(PmxBulletBuildError::UnsupportedRigidBodyShape {
                index,
                shape: shape.to_owned(),
            });
        }
    };

    Ok(RigidBodyDesc {
        shape,
        position: body.position,
        rotation_euler: body.rotation,
        mass: if body.mode == "static" {
            0.0
        } else {
            body.mass
        },
        linear_damping: body.linear_damping,
        angular_damping: body.angular_damping,
        friction: body.friction,
        restitution: body.restitution,
        collision_group: body.group as u16,
        collision_mask: !body.mask,
    })
}

enum JointMapping {
    Mapped(SixDofSpringJointDesc),
    InvalidBody,
    UnsupportedType,
}

fn joint_desc_from_pmx(joint: &PmxParsedJoint, bodies: &[RigidBodyHandle]) -> JointMapping {
    if joint.kind != "generic6dofSpring" {
        return JointMapping::UnsupportedType;
    }

    let Some(&rigidbody_a) = body_handle(bodies, joint.rigid_body_index_a) else {
        return JointMapping::InvalidBody;
    };
    let Some(&rigidbody_b) = body_handle(bodies, joint.rigid_body_index_b) else {
        return JointMapping::InvalidBody;
    };

    JointMapping::Mapped(SixDofSpringJointDesc {
        rigidbody_a,
        rigidbody_b,
        position: joint.position,
        rotation_euler: joint.rotation,
        translation_lower_limit: joint.translation_lower_limit,
        translation_upper_limit: joint.translation_upper_limit,
        rotation_lower_limit: joint.rotation_lower_limit,
        rotation_upper_limit: joint.rotation_upper_limit,
        spring_translation_factor: joint.spring_translation_factor,
        spring_rotation_factor: joint.spring_rotation_factor,
    })
}

fn body_handle(bodies: &[RigidBodyHandle], index: i32) -> Option<&RigidBodyHandle> {
    if index < 0 {
        return None;
    }
    bodies.get(index as usize)
}

#[cfg(test)]
mod tests {
    use mmd_anim_format::PmxPartsDescriptor;
    use serde_json::json;

    use super::*;

    #[test]
    fn builds_bullet_world_from_pmx_physics_metadata() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "root"}],
            "rigidBodies": [
                {
                    "name": "anchor",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 10.0, 0.0],
                    "mode": "static"
                },
                {
                    "boneIndex": 0,
                    "name": "bob",
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 8.0, 0.0],
                    "mass": 1.0,
                    "mode": "dynamic"
                }
            ],
            "joints": [
                {
                    "name": "joint",
                    "type": "generic6dofSpring",
                    "rigidBodyIndexA": 0,
                    "rigidBodyIndexB": 1,
                    "position": [0.0, 9.0, 0.0]
                }
            ]
        }))
        .unwrap();
        let model = crate::test_support::build_test_pmx_model(descriptor);

        let mut built = build_bullet_world_from_pmx(&model).unwrap();
        let fed = built
            .feed_kinematic_rigidbodies(&[Transform {
                position: [0.0, 10.0, 0.0],
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            }])
            .unwrap();
        built.settle_to_current().unwrap();
        built.world.step(0.5, 60).unwrap();
        let bob = built
            .world
            .rigidbody_transform(built.rigidbody_handles[1])
            .unwrap();

        assert_eq!(
            built.report,
            PmxBulletBuildReport {
                rigidbodies_added: 2,
                joints_added: 1,
                joints_skipped_invalid_body: 0,
                joints_skipped_unsupported_type: 0,
            }
        );
        assert_eq!(built.rigidbody_bindings[0].mode, PmxRigidBodyMode::Static);
        assert_eq!(built.rigidbody_bindings[0].bone_index, Some(0));
        assert_eq!(fed, 1);
        assert!(
            bob.position[1] > 6.0,
            "bob should remain constrained: {bob:?}"
        );
    }

    #[test]
    fn pmx_joint_mapping_preserves_limits_and_spring_factors() {
        let mut world = BulletWorld::new().unwrap();
        let body_a = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(0.5, [0.0, 10.0, 0.0], 0.0))
            .unwrap();
        let body_b = world
            .add_rigidbody(RigidBodyDesc::dynamic_sphere(0.5, [0.0, 8.0, 0.0], 1.0))
            .unwrap();
        let joint = PmxParsedJoint {
            name: "joint".to_owned(),
            english_name: "joint".to_owned(),
            kind: "generic6dofSpring".to_owned(),
            rigid_body_index_a: 0,
            rigid_body_index_b: 1,
            position: [1.0, 2.0, 3.0],
            rotation: [0.1, 0.2, 0.3],
            translation_lower_limit: [-0.1, -0.2, -0.3],
            translation_upper_limit: [0.4, 0.5, 0.6],
            rotation_lower_limit: [-0.7, -0.8, -0.9],
            rotation_upper_limit: [1.0, 1.1, 1.2],
            spring_translation_factor: [10.0, 20.0, 30.0],
            spring_rotation_factor: [40.0, 50.0, 60.0],
        };

        let desc = match joint_desc_from_pmx(&joint, &[body_a, body_b]) {
            JointMapping::Mapped(desc) => desc,
            JointMapping::InvalidBody | JointMapping::UnsupportedType => {
                panic!("expected generic6dofSpring joint to map")
            }
        };

        assert_eq!(desc.rigidbody_a, body_a);
        assert_eq!(desc.rigidbody_b, body_b);
        assert_eq!(desc.position, joint.position);
        assert_eq!(desc.rotation_euler, joint.rotation);
        assert_eq!(desc.translation_lower_limit, joint.translation_lower_limit);
        assert_eq!(desc.translation_upper_limit, joint.translation_upper_limit);
        assert_eq!(desc.rotation_lower_limit, joint.rotation_lower_limit);
        assert_eq!(desc.rotation_upper_limit, joint.rotation_upper_limit);
        assert_eq!(
            desc.spring_translation_factor,
            joint.spring_translation_factor
        );
        assert_eq!(desc.spring_rotation_factor, joint.spring_rotation_factor);
    }

    #[test]
    fn readback_returns_dynamic_body_transform_for_bound_bone() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "root"}, {"name": "physics"}],
            "rigidBodies": [
                {
                    "name": "physicsBody",
                    "boneIndex": 1,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 8.0, 0.0],
                    "mass": 1.0,
                    "mode": "dynamic"
                }
            ]
        }))
        .unwrap();
        let model = crate::test_support::build_test_pmx_model(descriptor);

        let mut built = build_bullet_world_from_pmx(&model).unwrap();
        built.world.step(1.0 / 30.0, 10).unwrap();
        let readback = built.readback_bone_world_transforms(2).unwrap();

        assert!(readback[0].is_none());
        let physics_bone = readback[1].unwrap();
        assert!(
            physics_bone.position[1] < 8.0,
            "dynamic body should write back simulated transform: {physics_bone:?}"
        );
    }

    #[test]
    fn feed_and_readback_apply_rigidbody_bone_offset() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "root", "position": [0.0, 10.0, 0.0]}],
            "rigidBodies": [
                {
                    "name": "offsetBody",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 9.0, 0.0],
                    "mode": "static"
                }
            ]
        }))
        .unwrap();
        let model = crate::test_support::build_test_pmx_model(descriptor);

        let mut built = build_bullet_world_from_pmx(&model).unwrap();
        built
            .feed_kinematic_rigidbodies(&[Transform::from_translation([0.0, 20.0, 0.0])])
            .unwrap();
        let body = built
            .world
            .rigidbody_transform(built.rigidbody_handles[0])
            .unwrap();

        assert!((built.rigidbody_bindings[0].body_from_bone.position[1] + 1.0).abs() < 1.0e-4);
        assert!((body.position[1] - 19.0).abs() < 1.0e-4);
        assert!((built.rigidbody_bindings[0].bone_from_body.position[1] - 1.0).abs() < 1.0e-4);
    }

    #[test]
    fn feed_options_can_skip_dynamic_bone_bodies() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "root", "position": [0.0, 5.0, 0.0]}],
            "rigidBodies": [
                {
                    "name": "dynamicBoneBody",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 5.0, 0.0],
                    "mass": 1.0,
                    "mode": "dynamicBone"
                }
            ]
        }))
        .unwrap();
        let model = crate::test_support::build_test_pmx_model(descriptor);

        let mut built = build_bullet_world_from_pmx(&model).unwrap();
        let moved_bone = [Transform::from_translation([0.0, 20.0, 0.0])];
        let skipped = built
            .feed_kinematic_rigidbodies_with_options(&moved_bone, false)
            .unwrap();
        let skipped_body = built
            .world
            .rigidbody_transform(built.rigidbody_handles[0])
            .unwrap();
        let included = built
            .feed_kinematic_rigidbodies_with_options(&moved_bone, true)
            .unwrap();
        let included_body = built
            .world
            .rigidbody_transform(built.rigidbody_handles[0])
            .unwrap();

        assert_eq!(skipped, 0);
        assert!((skipped_body.position[1] - 5.0).abs() < 1.0e-4);
        assert_eq!(included, 1);
        assert!((included_body.position[1] - 20.0).abs() < 1.0e-4);
    }

    #[test]
    fn pmx_collision_mask_controls_bullet_contacts() {
        fn has_contact(dynamic_non_collision_mask: u16) -> bool {
            let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
                "bones": [{"name": "root"}],
                "rigidBodies": [
                    {
                        "name": "floor",
                        "boneIndex": 0,
                        "shape": "box",
                        "size": [5.0, 1.0, 5.0],
                        "position": [0.0, -1.0, 0.0],
                        "group": 1,
                        "mask": 0,
                        "mode": "static"
                    },
                    {
                        "name": "ball",
                        "boneIndex": 0,
                        "shape": "sphere",
                        "size": [0.5, 0.0, 0.0],
                        "position": [0.0, 0.25, 0.0],
                        "group": 1,
                        "mask": dynamic_non_collision_mask,
                        "mass": 1.0,
                        "mode": "dynamic"
                    }
                ]
            }))
            .unwrap();
            let model = crate::test_support::build_test_pmx_model(descriptor);
            let mut built = build_bullet_world_from_pmx(&model).unwrap();
            for _ in 0..10 {
                built.world.step(1.0 / 60.0, 2).unwrap();
            }
            built.world.contact_points().unwrap().iter().any(|contact| {
                contact.rigidbody_a == built.rigidbody_handles[1]
                    || contact.rigidbody_b == built.rigidbody_handles[1]
            })
        }

        assert!(has_contact(0));
        assert!(!has_contact(1 << 1));
    }

    #[test]
    #[ignore = "set MMD_ANIM_REAL_PMX to a local PMX file for manual physics smoke"]
    fn builds_bullet_world_from_real_pmx() {
        let path = std::env::var("MMD_ANIM_REAL_PMX").expect("MMD_ANIM_REAL_PMX must be set");
        let data = std::fs::read(&path).unwrap();
        let model = mmd_anim_format::parse_pmx_model(&data).unwrap();
        let mut built = build_bullet_world_from_pmx(&model).unwrap();
        built.world.step(1.0 / 30.0, 10).unwrap();

        assert_eq!(built.report.rigidbodies_added, model.rigid_bodies.len());
        assert_eq!(
            built.world.rigidbody_count().unwrap(),
            model.rigid_bodies.len()
        );
    }
}
