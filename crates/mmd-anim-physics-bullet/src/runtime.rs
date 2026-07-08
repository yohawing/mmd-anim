use mmd_anim_runtime::RuntimeInstance;

use crate::{BulletError, PmxBulletWorld, Transform};

pub trait RuntimePhysicsBridgeExt {
    fn feed_runtime_kinematic_rigidbodies(
        &mut self,
        runtime: &RuntimeInstance,
    ) -> Result<usize, BulletError>;

    fn apply_readback_to_runtime(
        &self,
        runtime: &mut RuntimeInstance,
    ) -> Result<usize, BulletError>;
}

impl RuntimePhysicsBridgeExt for PmxBulletWorld {
    fn feed_runtime_kinematic_rigidbodies(
        &mut self,
        runtime: &RuntimeInstance,
    ) -> Result<usize, BulletError> {
        let bone_world_transforms = runtime
            .world_matrices()
            .iter()
            .copied()
            .map(Transform::from_mat4)
            .collect::<Vec<_>>();
        self.feed_kinematic_rigidbodies(&bone_world_transforms)
    }

    fn apply_readback_to_runtime(
        &self,
        runtime: &mut RuntimeInstance,
    ) -> Result<usize, BulletError> {
        let readback = self.readback_bone_world_transforms(runtime.world_matrices().len())?;
        let physics_world_matrices = readback
            .into_iter()
            .map(|transform| transform.map(Transform::to_mat4))
            .collect::<Vec<_>>();
        Ok(runtime.apply_physics_world_matrices(&physics_world_matrices))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Vec3A;
    use mmd_anim_format::{PmxPartsDescriptor, PmxPartsInput, build_pmx_model_from_parts};
    use mmd_anim_runtime::{BoneIndex, BoneInit, ModelArena, RuntimeInstance};
    use serde_json::json;

    use super::*;
    use crate::build_bullet_world_from_pmx;

    fn translation(matrix: glam::Mat4) -> Vec3A {
        Vec3A::from_vec4(matrix.w_axis)
    }

    #[test]
    fn bridge_feeds_and_applies_bullet_readback_to_runtime() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "physics", "position": [0.0, 8.0, 0.0]}],
            "rigidBodies": [
                {
                    "name": "physicsBody",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 8.0, 0.0],
                    "mass": 1.0,
                    "mode": "dynamic"
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
        let mut bullet = build_bullet_world_from_pmx(&model).unwrap();
        let runtime_model = Arc::new(
            ModelArena::new(vec![BoneInit::new(None, Vec3A::new(0.0, 8.0, 0.0))]).unwrap(),
        );
        let mut runtime = RuntimeInstance::new(runtime_model);
        runtime.evaluate_rest_pose();

        assert_eq!(
            bullet.feed_runtime_kinematic_rigidbodies(&runtime).unwrap(),
            0
        );
        bullet.world.step(1.0 / 30.0, 10).unwrap();
        assert_eq!(bullet.apply_readback_to_runtime(&mut runtime).unwrap(), 1);

        assert!(
            translation(runtime.world_matrices()[BoneIndex(0).as_usize()]).y < 8.0,
            "runtime bone should receive simulated Bullet readback"
        );
    }
}
