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

    fn feed_runtime_kinematic_rigidbodies_with_options(
        &mut self,
        runtime: &RuntimeInstance,
        include_dynamic_bone: bool,
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

    fn step_runtime_physics_with_runtime_clock_options(
        &mut self,
        runtime: &mut RuntimeInstance,
        delta_time: f32,
        include_dynamic_bone_before_step: bool,
    ) -> Result<RuntimePhysicsClockStepReport, BulletError>;
}

impl RuntimePhysicsBridgeExt for PmxBulletWorld {
    fn reset_runtime_physics(
        &mut self,
        runtime: &mut RuntimeInstance,
    ) -> Result<usize, BulletError> {
        runtime.reset_physics_tick();
        self.world.reset()?;
        let seeded = self.seed_runtime_physics(runtime)?;

        // Match the Unity/saba reset sequence. Relax the constraints for one
        // 1/60 second update. Bullet's fixed step is 1/120, so two substeps are
        // required to consume the whole settle interval.
        self.world.step(1.0 / 60.0, 2)?;

        // The settle may move dynamic bodies, but static bodies remain driven
        // by the current runtime pose. Re-pin only those bodies before the
        // final cleanup; DynamicBone must remain solver-owned until readback.
        let bone_world_transforms = runtime
            .world_matrices()
            .iter()
            .copied()
            .map(Transform::from_mat4)
            .collect::<Vec<_>>();
        self.feed_kinematic_rigidbodies_with_options(&bone_world_transforms, false)?;
        self.settle_to_current()?;
        self.apply_readback_to_runtime(runtime)?;
        runtime.evaluate_current_pose_after_physics();
        Ok(seeded)
    }

    fn seed_runtime_physics(&mut self, runtime: &RuntimeInstance) -> Result<usize, BulletError> {
        let bone_world_transforms = runtime
            .world_matrices()
            .iter()
            .copied()
            .map(Transform::from_mat4)
            .collect::<Vec<_>>();
        let fed = self.seed_rigidbodies_from_bones(&bone_world_transforms)?;
        self.settle_to_current()?;
        Ok(fed)
    }

    fn feed_runtime_kinematic_rigidbodies(
        &mut self,
        runtime: &RuntimeInstance,
    ) -> Result<usize, BulletError> {
        self.feed_runtime_kinematic_rigidbodies_with_options(runtime, true)
    }

    fn feed_runtime_kinematic_rigidbodies_with_options(
        &mut self,
        runtime: &RuntimeInstance,
        include_dynamic_bone: bool,
    ) -> Result<usize, BulletError> {
        let bone_world_transforms = runtime
            .world_matrices()
            .iter()
            .copied()
            .map(Transform::from_mat4)
            .collect::<Vec<_>>();
        self.feed_kinematic_rigidbodies_with_options(&bone_world_transforms, include_dynamic_bone)
    }

    fn apply_readback_to_runtime(
        &self,
        runtime: &mut RuntimeInstance,
    ) -> Result<usize, BulletError> {
        let readback = self.readback_bone_world_transforms(runtime.world_matrices().len())?;
        let physics_world_matrices = readback
            .into_iter()
            .map(|transform| transform.map(|transform| transform.to_mat4()))
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
        self.step_runtime_physics_with_runtime_clock_options(runtime, delta_time, true)
    }

    fn step_runtime_physics_with_runtime_clock_options(
        &mut self,
        runtime: &mut RuntimeInstance,
        delta_time: f32,
        include_dynamic_bone_before_step: bool,
    ) -> Result<RuntimePhysicsClockStepReport, BulletError> {
        if !runtime.physics_mode().steps_backend() {
            return Ok(RuntimePhysicsClockStepReport::default());
        }

        let tick = runtime.advance_physics_tick_clock(delta_time);
        let kinematic_rigidbodies_fed = self.feed_runtime_kinematic_rigidbodies_with_options(
            runtime,
            include_dynamic_bone_before_step,
        )?;

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

#[cfg(all(test, feature = "pmx-format"))]
mod tests {
    use std::sync::Arc;

    use glam::Vec3A;
    use mmd_anim_format::PmxPartsDescriptor;
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
        let model = crate::test_support::build_test_pmx_model(descriptor);
        let mut bullet = build_bullet_world_from_pmx(&model).unwrap();
        let runtime_model = Arc::new(
            ModelArena::new(vec![BoneInit::new(None, Vec3A::new(0.0, 8.0, 0.0))]).unwrap(),
        );
        let mut runtime = RuntimeInstance::new(Arc::clone(&runtime_model));
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
        let model = crate::test_support::build_test_pmx_model(descriptor);
        let mut bullet = build_bullet_world_from_pmx(&model).unwrap();
        let runtime_model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::new(0.0, 10.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 8.0, 0.0)),
            ])
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(Arc::clone(&runtime_model));
        runtime.evaluate_rest_pose();
        runtime.set_physics_mode(PhysicsMode::Live);

        assert_eq!(bullet.seed_runtime_physics(&runtime).unwrap(), 2);
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
        let model = crate::test_support::build_test_pmx_model(descriptor);
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
    fn runtime_clock_default_keeps_dynamic_bone_feed_compatible() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "dynamicBone", "position": [0.0, 8.0, 0.0]}],
            "rigidBodies": [
                {
                    "name": "dynamicBoneBody",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 8.0, 0.0],
                    "mass": 1.0,
                    "mode": "dynamicBone"
                }
            ]
        }))
        .unwrap();
        let model = crate::test_support::build_test_pmx_model(descriptor);
        let runtime_model = Arc::new(
            ModelArena::new(vec![BoneInit::new(None, Vec3A::new(0.0, 8.0, 0.0))]).unwrap(),
        );
        let mut runtime = RuntimeInstance::new(Arc::clone(&runtime_model));
        runtime.evaluate_rest_pose();
        runtime.set_physics_mode(PhysicsMode::Live);

        let mut default_bullet = build_bullet_world_from_pmx(&model).unwrap();
        let default_report = default_bullet
            .step_runtime_physics_with_runtime_clock(&mut runtime, 1.0 / 60.0)
            .unwrap();
        assert_eq!(default_report.kinematic_rigidbodies_fed, 1);
        assert!(
            translation(runtime.world_matrices()[BoneIndex(0).as_usize()]).y < 8.0,
            "dynamicBone default runtime-clock readback should remain full-write compatible"
        );

        let mut static_only_runtime = RuntimeInstance::new(runtime_model);
        static_only_runtime.evaluate_rest_pose();
        static_only_runtime.set_physics_mode(PhysicsMode::Live);
        let mut static_only_bullet = build_bullet_world_from_pmx(&model).unwrap();
        let static_only_report = static_only_bullet
            .step_runtime_physics_with_runtime_clock_options(
                &mut static_only_runtime,
                1.0 / 60.0,
                false,
            )
            .unwrap();
        assert_eq!(static_only_report.kinematic_rigidbodies_fed, 0);
    }

    #[test]
    fn runtime_clock_static_only_option_does_not_pin_dynamic_bone_before_step() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(json!({
            "bones": [{"name": "dynamicBone", "position": [0.0, 8.0, 0.0]}],
            "rigidBodies": [
                {
                    "name": "dynamicBoneBody",
                    "boneIndex": 0,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 8.0, 0.0],
                    "mass": 1.0,
                    "mode": "dynamicBone"
                }
            ]
        }))
        .unwrap();
        let model = crate::test_support::build_test_pmx_model(descriptor);
        let runtime_model = Arc::new(
            ModelArena::new(vec![BoneInit::new(None, Vec3A::new(0.0, 8.0, 0.0))]).unwrap(),
        );

        let mut static_only_runtime = RuntimeInstance::new(Arc::clone(&runtime_model));
        static_only_runtime.evaluate_rest_pose();
        static_only_runtime.set_physics_mode(PhysicsMode::Live);
        let mut static_only_bullet = build_bullet_world_from_pmx(&model).unwrap();
        assert_eq!(
            static_only_bullet
                .seed_runtime_physics(&static_only_runtime)
                .unwrap(),
            1
        );
        static_only_runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(0), Vec3A::new(0.0, 12.0, 0.0));
        static_only_runtime.evaluate_current_pose();
        let static_only_report = static_only_bullet
            .step_runtime_physics_with_runtime_clock_options(
                &mut static_only_runtime,
                1.0 / 60.0,
                false,
            )
            .unwrap();
        let static_only_y = translation(static_only_runtime.world_matrices()[0]).y;

        let mut pinned_runtime = RuntimeInstance::new(runtime_model);
        pinned_runtime.evaluate_rest_pose();
        pinned_runtime.set_physics_mode(PhysicsMode::Live);
        let mut pinned_bullet = build_bullet_world_from_pmx(&model).unwrap();
        assert_eq!(
            pinned_bullet.seed_runtime_physics(&pinned_runtime).unwrap(),
            1
        );
        pinned_runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(0), Vec3A::new(0.0, 12.0, 0.0));
        pinned_runtime.evaluate_current_pose();
        let pinned_report = pinned_bullet
            .step_runtime_physics_with_runtime_clock_options(&mut pinned_runtime, 1.0 / 60.0, true)
            .unwrap();
        let pinned_y = translation(pinned_runtime.world_matrices()[0]).y;

        assert_eq!(static_only_report.kinematic_rigidbodies_fed, 0);
        assert_eq!(static_only_report.bones_written_back, 1);
        assert!(
            static_only_y < 10.0,
            "static-only step should not teleport DynamicBone to moved runtime pose: y={static_only_y}"
        );
        assert_eq!(pinned_report.kinematic_rigidbodies_fed, 1);
        assert!(
            pinned_y > 15.0,
            "pinned step should follow moved runtime pose before Bullet step: y={pinned_y}"
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
        let model = crate::test_support::build_test_pmx_model(descriptor);
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
    fn bridge_reset_settles_first_step_and_repeats_from_runtime_pose() {
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
        let model = crate::test_support::build_test_pmx_model(descriptor);
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
        runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(0), Vec3A::new(0.0, 5.0, 0.0));
        runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(1), Vec3A::new(0.0, 5.0, 0.0));
        runtime.evaluate_current_pose();

        let mut seed_only_bullet = build_bullet_world_from_pmx(&model).unwrap();
        assert_eq!(seed_only_bullet.seed_runtime_physics(&runtime).unwrap(), 2);
        let dynamic_after_direct_seed = seed_only_bullet
            .world
            .rigidbody_transform(seed_only_bullet.rigidbody_handles[1])
            .unwrap();
        assert!(
            (dynamic_after_direct_seed.position[1] - 13.0).abs() < 1.0e-4,
            "direct seed must synchronize without integrating: {dynamic_after_direct_seed:?}"
        );

        assert_eq!(bullet.reset_runtime_physics(&mut runtime).unwrap(), 2);
        let anchor_after_reset = bullet
            .world
            .rigidbody_transform(bullet.rigidbody_handles[0])
            .unwrap();
        let dynamic_after_reset = bullet
            .world
            .rigidbody_transform(bullet.rigidbody_handles[1])
            .unwrap();

        assert!((anchor_after_reset.position[1] - 15.0).abs() < 1.0e-4);
        assert!(
            dynamic_after_reset.position[1] < 13.0,
            "reset must include the 1/60 settle: {dynamic_after_reset:?}"
        );
        assert!(
            (translation(runtime.world_matrices()[1]).y - dynamic_after_reset.position[1]).abs()
                < 1.0e-4,
            "reset must expose the settled body transform through the runtime bone"
        );

        let first_step = bullet
            .step_runtime_physics_with_runtime_clock_options(&mut runtime, 1.0 / 60.0, false)
            .unwrap();
        assert_eq!(first_step.tick.substeps, 2);
        assert_eq!(first_step.kinematic_rigidbodies_fed, 1);
        assert_eq!(first_step.bones_written_back, 1);
        let first_step_dynamic = bullet
            .world
            .rigidbody_transform(bullet.rigidbody_handles[1])
            .unwrap();
        assert!(
            (first_step_dynamic.position[1] - dynamic_after_reset.position[1]).abs() > 1.0e-6,
            "first forward step must advance the settled body: reset={dynamic_after_reset:?}, first={first_step_dynamic:?}"
        );
        assert!(
            (translation(runtime.world_matrices()[1]).y - first_step_dynamic.position[1]).abs()
                < 1.0e-4,
            "first forward step must write the body transform back to the runtime bone"
        );

        runtime.evaluate_rest_pose();
        runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(0), Vec3A::new(0.0, 5.0, 0.0));
        runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(1), Vec3A::new(0.0, 5.0, 0.0));
        runtime.evaluate_current_pose();

        assert_eq!(bullet.reset_runtime_physics(&mut runtime).unwrap(), 2);
        let repeated_reset_dynamic = bullet
            .world
            .rigidbody_transform(bullet.rigidbody_handles[1])
            .unwrap();
        assert!(
            (repeated_reset_dynamic.position[1] - dynamic_after_reset.position[1]).abs() < 1.0e-5,
            "reset settle must be repeatable: first={dynamic_after_reset:?}, repeated={repeated_reset_dynamic:?}"
        );
        assert_eq!(runtime.physics_accumulator_seconds(), 0.0);
    }
}
