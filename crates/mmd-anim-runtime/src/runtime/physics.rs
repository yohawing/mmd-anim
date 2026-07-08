use super::{IkSolveOptions, RuntimeInstance};

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

        self.evaluate_current_pose_after_physics_with_ik_options(options);

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
