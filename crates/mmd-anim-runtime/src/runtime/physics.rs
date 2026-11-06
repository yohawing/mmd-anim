use glam::{Mat4, Vec3A};

use super::{IkSolveOptions, RuntimeInstance};
use crate::BoneIndex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhysicsMode {
    /// Physics is disabled. Switching to this mode resets the accumulated
    /// physics tick state.
    #[default]
    Off,
    /// Host-synchronized physics. The host is expected to evaluate animation
    /// frames before stepping physics, so random seeks should reset/reseed the
    /// backend before continuing.
    Trace,
    /// Wall-clock physics. The host may keep stepping physics while animation
    /// sampling is paused, using the latest evaluated pose as the kinematic
    /// target.
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
        let updated = physics_world_matrices
            .iter()
            .take(self.model.bone_count())
            .enumerate()
            .filter(|(index, target)| {
                target.is_some() && !self.is_host_driven(BoneIndex(*index as u32))
            })
            .count();
        if updated == 0 {
            return 0;
        }

        // A parent can inherit append from its own descendant. Publish supplied
        // world transforms before resolving local channels so that descendant
        // inversion never uses the parent's previous pose on this back edge.
        for (index, target) in physics_world_matrices
            .iter()
            .take(self.model.bone_count())
            .enumerate()
        {
            let bone = BoneIndex(index as u32);
            if self.is_host_driven(bone) {
                continue;
            }
            if let Some(world) = target {
                self.pose.set_world_matrix(bone, *world);
                self.transform_matrices[index] =
                    *world * Mat4::from_translation((-self.model.rest_world_position(bone)).into());
            }
        }

        // A supplied world matrix already includes its append transform.
        // Resolve parents and append donors before inverting it into base local
        // channels, including donors ordered after their recipients in PMX.
        let mut visited = vec![false; self.model.bone_count()];
        for position in 0..self.model.eval_order().len() {
            self.apply_physics_world_matrix_with_dependencies(
                self.model.eval_order()[position],
                physics_world_matrices,
                &mut visited,
            );
        }
        // Dependency traversal may visit descendants before their parent's local
        // channels are finalized. Rebuild world and skinning in parent-first order.
        self.update_world_matrices();
        updated
    }

    fn apply_physics_world_matrix_with_dependencies(
        &mut self,
        bone: BoneIndex,
        targets: &[Option<Mat4>],
        visited: &mut [bool],
    ) {
        let index = bone.as_usize();
        // Append graphs can contain cycles; retain the existing evaluator's
        // finite traversal behavior rather than recursing indefinitely.
        if visited[index] {
            return;
        }
        visited[index] = true;
        if self.is_host_driven(bone) {
            // HostRig references remain authoritative, even if Bullet also
            // supplied this bone. Its normal update maintains skinning output.
            self.update_world_matrix_for_bone(bone);
            return;
        }
        if let Some(parent) = self.model.parent_index(bone) {
            self.apply_physics_world_matrix_with_dependencies(parent, targets, visited);
        }
        if let Some(append_index) = self.model.append_transform_index(bone) {
            let source = self.model.append_transform(append_index).source_bone;
            self.apply_physics_world_matrix_with_dependencies(source, targets, visited);
        }
        self.update_append_transform_for_bone(bone);

        if let Some(Some(target)) = targets.get(index) {
            let parent_inverse = self
                .model
                .parent_index(bone)
                .map_or(Mat4::IDENTITY, |parent| {
                    self.pose.world_matrices()[parent.as_usize()].inverse()
                });
            let (scale, mut rotation, translation) =
                (parent_inverse * *target).to_scale_rotation_translation();
            let mut offset = Vec3A::from(translation) - self.model.rest_position(bone);
            if let Some(append_index) = self.model.append_transform_index(bone) {
                let append = self.model.append_transform(append_index);
                if append.affect_rotation {
                    // Runtime world evaluation composes base * append.
                    rotation *= self.pose.append_rotation(bone).conjugate();
                }
                if append.affect_translation {
                    offset -= self.pose.append_position_offset(bone);
                }
            }
            self.pose.set_local_position_offset(bone, offset);
            self.pose.set_local_rotation(bone, rotation.normalize());
            self.pose.set_local_scale(bone, Vec3A::from(scale));
        }
        // Keep fixed-axis constraints, un-targeted descendants and skinning
        // matrices on the same path used by ordinary pose evaluation.
        self.update_world_matrix_for_bone(bone);
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
