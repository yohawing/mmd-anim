use mmd_anim_format::{
    PmxParsedModel,
    pmx::{PmxParsedJoint, PmxParsedRigidBody},
};
use thiserror::Error;

use crate::{
    BulletError, BulletWorld, RigidBodyDesc, RigidBodyHandle, RigidBodyShape, SixDofSpringJointDesc,
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
    pub report: PmxBulletBuildReport,
}

pub fn build_bullet_world_from_pmx(
    model: &PmxParsedModel,
) -> Result<PmxBulletWorld, PmxBulletBuildError> {
    let mut world = BulletWorld::new()?;
    let mut rigidbody_handles = Vec::with_capacity(model.rigid_bodies.len());
    let mut report = PmxBulletBuildReport::default();

    for (index, body) in model.rigid_bodies.iter().enumerate() {
        let desc = rigidbody_desc_from_pmx(index, body)?;
        let handle = world.add_rigidbody(desc)?;
        rigidbody_handles.push(handle);
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
        report,
    })
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
        collision_mask: body.mask,
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
    use mmd_anim_format::{PmxPartsDescriptor, PmxPartsInput, build_pmx_model_from_parts};
    use serde_json::json;

    use super::*;

    #[test]
    fn builds_bullet_world_from_pmx_physics_metadata() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "rigidBodies": [
                {
                    "name": "anchor",
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 10.0, 0.0],
                    "mode": "static"
                },
                {
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
        let positions_xyz = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let normals_xyz = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let uvs_xy = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
        let indices = [0, 1, 2];
        let model = build_pmx_model_from_parts(PmxPartsInput {
            descriptor,
            positions_xyz: &positions_xyz,
            normals_xyz: &normals_xyz,
            uvs_xy: &uvs_xy,
            indices: &indices,
            skin_indices: &[],
            skin_weights: &[],
            edge_scale: &[],
        })
        .unwrap();

        let mut built = build_bullet_world_from_pmx(&model).unwrap();
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
        assert!(
            bob.position[1] > 6.0,
            "bob should remain constrained: {bob:?}"
        );
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
