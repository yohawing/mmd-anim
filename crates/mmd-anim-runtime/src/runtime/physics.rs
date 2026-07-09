use glam::{Mat4, Vec3A};

use super::{IkSolveOptions, RuntimeInstance};
use crate::{BoneIndex, ik_primitive::constrain_rotation_to_axis};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhysicsMode {
    #[default]
    Off,
    Trace,
    Live,
}

impl PhysicsMode {
    pub fn steps_backend(self) -> bool {
        matches!(self, Self::Trace | Self::Live)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicsTickConfig {
    pub fixed_substep_seconds: f32,
    pub max_substeps_per_tick: u32,
}

impl Default for PhysicsTickConfig {
    fn default() -> Self {
        Self {
            fixed_substep_seconds: 1.0 / 120.0,
            max_substeps_per_tick: 8,
        }
    }
}

impl PhysicsTickConfig {
    pub fn sanitized(self) -> Self {
        let default = Self::default();
        let fixed_substep_seconds =
            if self.fixed_substep_seconds.is_finite() && self.fixed_substep_seconds > 0.0 {
                self.fixed_substep_seconds
            } else {
                default.fixed_substep_seconds
            };
        let max_substeps_per_tick = self.max_substeps_per_tick.max(1);
        Self {
            fixed_substep_seconds,
            max_substeps_per_tick,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicsStepStats {
    pub input_dt_seconds: f32,
    pub clamped_dt_seconds: f32,
    pub substeps: u32,
    pub accumulator_seconds: f32,
}

impl RuntimeInstance {
    #[inline]
    pub fn physics_mode(&self) -> PhysicsMode {
        self.physics_mode
    }

    pub fn set_physics_mode(&mut self, mode: PhysicsMode) {
        if mode == PhysicsMode::Off {
            self.reset_physics_tick();
        }
        self.physics_mode = mode;
    }

    #[inline]
    pub fn physics_tick_config(&self) -> PhysicsTickConfig {
        self.physics_tick_config
    }

    pub fn set_physics_tick_config(&mut self, config: PhysicsTickConfig) {
        self.physics_tick_config = config.sanitized();
        self.physics_accumulator_seconds = self
            .physics_accumulator_seconds
            .min(self.max_physics_dt_seconds());
    }

    #[inline]
    pub fn physics_accumulator_seconds(&self) -> f32 {
        self.physics_accumulator_seconds
    }

    pub fn reset_physics_tick(&mut self) {
        self.physics_accumulator_seconds = 0.0;
    }

    pub fn apply_physics_world_matrices(
        &mut self,
        physics_world_matrices: &[Option<Mat4>],
    ) -> usize {
        let mut updated = 0;
        let mut earliest_eval_order_position = None;
        let mut target_world_matrices = self.pose.world_matrices().to_vec();
        let mut has_physics_target = vec![false; self.model.bone_count()];

        for (bone_index, target_world_matrix) in physics_world_matrices.iter().enumerate() {
            let Some(target_world_matrix) = target_world_matrix else {
                continue;
            };
            let Some(slot) = target_world_matrices.get_mut(bone_index) else {
                continue;
            };
            *slot = *target_world_matrix;
            has_physics_target[bone_index] = true;
        }

        for bone in self.model.eval_order() {
            let bone_index = bone.as_usize();
            if has_physics_target[bone_index] {
                continue;
            }
            let local_matrix = self.current_local_matrix_for_physics_scratch(*bone);
            target_world_matrices[bone_index] = self
                .model
                .parent_index(*bone)
                .map(|parent| target_world_matrices[parent.as_usize()] * local_matrix)
                .unwrap_or(local_matrix);
        }

        for bone_index in 0..self.model.bone_count() {
            if !has_physics_target[bone_index] {
                continue;
            }

            let bone = BoneIndex(bone_index as u32);
            let parent_inverse_world = self
                .model
                .parent_index(bone)
                .map(|parent| target_world_matrices[parent.as_usize()].inverse())
                .unwrap_or(Mat4::IDENTITY);
            let local_matrix = parent_inverse_world * target_world_matrices[bone_index];
            let (scale, rotation, translation) = local_matrix.to_scale_rotation_translation();

            self.pose.set_local_position_offset(
                bone,
                Vec3A::from(translation) - self.model.rest_position(bone),
            );
            self.pose.set_local_rotation(bone, rotation.normalize());
            self.pose.set_local_scale(bone, Vec3A::from(scale));

            let eval_order_position = self.model.eval_order_position(bone);
            earliest_eval_order_position = Some(
                earliest_eval_order_position.map_or(eval_order_position, |current: usize| {
                    current.min(eval_order_position)
                }),
            );
            updated += 1;
        }

        if let Some(start) = earliest_eval_order_position {
            self.update_world_matrices_from_eval_order_position(start);
        }

        updated
    }

    fn current_local_matrix_for_physics_scratch(&self, bone: BoneIndex) -> Mat4 {
        let mut local_position =
            self.model.rest_position(bone) + self.pose.local_position_offset(bone);
        let mut local_rotation = self.pose.local_rotation(bone);
        let local_scale = self.pose.local_scale(bone);

        if let Some(append_index) = self.model.append_transform_index(bone) {
            let append = self.model.append_transform(append_index);
            if append.affect_rotation {
                local_rotation = (local_rotation * self.pose.append_rotation(bone)).normalize();
            }
            if append.affect_translation {
                local_position += self.pose.append_position_offset(bone);
            }
        }

        if let Some(axis) = self.model.fixed_axis_constraint(bone) {
            local_rotation = constrain_rotation_to_axis(local_rotation, axis);
        }

        Mat4::from_scale_rotation_translation(
            local_scale.into(),
            local_rotation,
            local_position.into(),
        )
    }

    /// Advance the physics clock independently from animation sampling.
    ///
    /// This is a no-backend implementation for now: it consumes fixed 120 Hz
    /// substeps and refreshes the after-physics evaluation phase, but does not
    /// simulate rigid bodies until a physics backend is attached.
    pub fn step_physics(&mut self, dt_seconds: f32) -> PhysicsStepStats {
        self.step_physics_with_ik_options(dt_seconds, IkSolveOptions::default())
    }

    pub fn step_physics_with_ik_options(
        &mut self,
        dt_seconds: f32,
        options: IkSolveOptions,
    ) -> PhysicsStepStats {
        let stats = self.advance_physics_tick_clock(dt_seconds);
        self.evaluate_current_pose_after_physics_with_ik_options(options);
        stats
    }

    pub fn advance_physics_tick_clock(&mut self, dt_seconds: f32) -> PhysicsStepStats {
        let input_dt_seconds = dt_seconds;
        let clamped_dt_seconds = self.clamped_physics_dt(dt_seconds);
        self.physics_accumulator_seconds += clamped_dt_seconds;

        let mut substeps = 0;
        while self.physics_accumulator_seconds + f32::EPSILON
            >= self.physics_tick_config.fixed_substep_seconds
            && substeps < self.physics_tick_config.max_substeps_per_tick
        {
            self.physics_accumulator_seconds -= self.physics_tick_config.fixed_substep_seconds;
            substeps += 1;
        }
        if substeps == self.physics_tick_config.max_substeps_per_tick {
            self.physics_accumulator_seconds = self
                .physics_accumulator_seconds
                .min(self.physics_tick_config.fixed_substep_seconds);
        }

        PhysicsStepStats {
            input_dt_seconds,
            clamped_dt_seconds,
            substeps,
            accumulator_seconds: self.physics_accumulator_seconds,
        }
    }

    fn clamped_physics_dt(&self, dt_seconds: f32) -> f32 {
        if !dt_seconds.is_finite() || dt_seconds <= 0.0 {
            return 0.0;
        }
        dt_seconds.min(self.max_physics_dt_seconds())
    }

    fn max_physics_dt_seconds(&self) -> f32 {
        self.physics_tick_config.fixed_substep_seconds
            * self.physics_tick_config.max_substeps_per_tick as f32
    }
}
