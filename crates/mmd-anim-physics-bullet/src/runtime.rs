use mmd_anim_runtime::{PhysicsStepStats, RuntimeInstance};

use crate::{BulletError, PmxBulletWorld, Transform};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuntimePhysicsStepReport {
    pub kinematic_rigidbodies_fed: usize,
    pub bones_written_back: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RuntimePhysicsClockStepReport {
    pub tick: PhysicsStepStats,
    pub kinematic_rigidbodies_fed: usize,
    pub bones_written_back: usize,
}

pub trait RuntimePhysicsBridgeExt {
    fn reset_runtime_physics(
        &mut self,
        runtime: &mut RuntimeInstance,
    ) -> Result<usize, BulletError>;

    fn seed_runtime_physics(&mut self, runtime: &RuntimeInstance) -> Result<usize, BulletError>;

    fn feed_runtime_kinematic_rigidbodies(
        &mut self,
        runtime: &RuntimeInstance,
    ) -> Result<usize, BulletError>;

    fn apply_readback_to_runtime(
        &self,
        runtime: &mut RuntimeInstance,
    ) -> Result<usize, BulletError>;

    fn step_runtime_physics(
        &mut self,
        runtime: &mut RuntimeInstance,
        delta_time: f32,
        max_sub_steps: i32,
    ) -> Result<RuntimePhysicsStepReport, BulletError>;

    fn step_runtime_physics_with_runtime_clock(
        &mut self,
        runtime: &mut RuntimeInstance,
        delta_time: f32,
    ) -> Result<RuntimePhysicsClockStepReport, BulletError>;
}

impl RuntimePhysicsBridgeExt for PmxBulletWorld {
    fn reset_runtime_physics(
        &mut self,
        runtime: &mut RuntimeInstance,
    ) -> Result<usize, BulletError> {
        runtime.reset_physics_tick();
        self.world.reset()?;
        self.seed_runtime_physics(runtime)
    }

    fn seed_runtime_physics(&mut self, runtime: &RuntimeInstance) -> Result<usize, BulletError> {
        let fed = self.feed_runtime_kinematic_rigidbodies(runtime)?;
        self.settle_to_current()?;
        Ok(fed)
    }

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

    fn step_runtime_physics(
        &mut self,
        runtime: &mut RuntimeInstance,
        delta_time: f32,
        max_sub_steps: i32,
    ) -> Result<RuntimePhysicsStepReport, BulletError> {
        let kinematic_rigidbodies_fed = self.feed_runtime_kinematic_rigidbodies(runtime)?;
        self.world.step(delta_time, max_sub_steps)?;
        let bones_written_back = self.apply_readback_to_runtime(runtime)?;
        Ok(RuntimePhysicsStepReport {
            kinematic_rigidbodies_fed,
            bones_written_back,
        })
    }

    fn step_runtime_physics_with_runtime_clock(
        &mut self,
        runtime: &mut RuntimeInstance,
        delta_time: f32,
    ) -> Result<RuntimePhysicsClockStepReport, BulletError> {
        if !runtime.physics_mode().steps_backend() {
            return Ok(RuntimePhysicsClockStepReport::default());
        }

        let tick = runtime.advance_physics_tick_clock(delta_time);
        let kinematic_rigidbodies_fed = self.feed_runtime_kinematic_rigidbodies(runtime)?;

        if tick.substeps > 0 {
            let simulated_dt =
                runtime.physics_tick_config().fixed_substep_seconds * tick.substeps as f32;
            self.world.step(simulated_dt, tick.substeps as i32)?;
        }

        let bones_written_back = self.apply_readback_to_runtime(runtime)?;
        runtime.evaluate_current_pose_after_physics();

        Ok(RuntimePhysicsClockStepReport {
            tick,
            kinematic_rigidbodies_fed,
            bones_written_back,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::Vec3A;
    use mmd_anim_format::{PmxPartsDescriptor, PmxPartsInput, build_pmx_model_from_parts};
    use mmd_anim_runtime::{BoneIndex, BoneInit, ModelArena, PhysicsMode, RuntimeInstance};
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
        runtime.set_physics_mode(PhysicsMode::Live);

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

    #[test]
    fn bridge_step_runs_feed_bullet_step_and_readback() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [
                {"name": "anchor", "position": [0.0, 10.0, 0.0]},
                {"name": "physics", "position": [0.0, 8.0, 0.0]}
            ],
            "rigidBodies": [
                {
                    "name": "anchorBody",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 10.0, 0.0],
                    "mode": "static"
                },
                {
                    "name": "physicsBody",
                    "boneIndex": 1,
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
        let mut bullet = build_bullet_world_from_pmx(&model).unwrap();
        let runtime_model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::new(0.0, 10.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 8.0, 0.0)),
            ])
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(runtime_model);
        runtime.evaluate_rest_pose();
        runtime.set_physics_mode(PhysicsMode::Live);

        assert_eq!(bullet.seed_runtime_physics(&runtime).unwrap(), 1);
        let report = bullet
            .step_runtime_physics(&mut runtime, 1.0 / 30.0, 10)
            .unwrap();

        assert_eq!(
            report,
            RuntimePhysicsStepReport {
                kinematic_rigidbodies_fed: 1,
                bones_written_back: 1,
            }
        );
        assert!(
            translation(runtime.world_matrices()[BoneIndex(1).as_usize()]).y < 8.0,
            "dynamic runtime bone should move after bridge step"
        );
    }

    #[test]
    fn bridge_step_uses_runtime_fixed_substep_clock() {
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
        runtime.set_physics_mode(PhysicsMode::Live);

        let report = bullet
            .step_runtime_physics_with_runtime_clock(&mut runtime, 1.0 / 60.0)
            .unwrap();

        assert_eq!(report.tick.substeps, 2);
        assert_eq!(report.bones_written_back, 1);
        assert!(
            translation(runtime.world_matrices()[BoneIndex(0).as_usize()]).y < 8.0,
            "runtime-clock bridge should step Bullet and write back"
        );
    }

    #[test]
    fn bridge_runtime_clock_respects_off_mode() {
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

        let report = bullet
            .step_runtime_physics_with_runtime_clock(&mut runtime, 1.0 / 60.0)
            .unwrap();

        assert_eq!(runtime.physics_mode(), PhysicsMode::Off);
        assert_eq!(report, RuntimePhysicsClockStepReport::default());
        assert_eq!(runtime.physics_accumulator_seconds(), 0.0);
        assert_eq!(translation(runtime.world_matrices()[0]).y, 8.0);
    }

    #[test]
    fn bridge_reset_reseeds_bullet_world_from_runtime_pose() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "anchor", "position": [0.0, 10.0, 0.0]}],
            "rigidBodies": [
                {
                    "name": "anchorBody",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 10.0, 0.0],
                    "mode": "static"
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
            ModelArena::new(vec![BoneInit::new(None, Vec3A::new(0.0, 10.0, 0.0))]).unwrap(),
        );
        let mut runtime = RuntimeInstance::new(runtime_model);
        runtime.evaluate_rest_pose();
        runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(0), Vec3A::new(0.0, 5.0, 0.0));
        runtime.evaluate_current_pose();

        assert_eq!(bullet.reset_runtime_physics(&mut runtime).unwrap(), 1);
        let body = bullet
            .world
            .rigidbody_transform(bullet.rigidbody_handles[0])
            .unwrap();

        assert!((body.position[1] - 15.0).abs() < 1.0e-4);
    }
}
