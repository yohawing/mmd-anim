use super::{IkSolveOptions, RuntimeInstance};
use crate::{AnimationClip, MorphIndex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MorphOverrideError {
    LengthMismatch,
    InvalidIndex(u32),
    NonFiniteWeight(usize),
    NonFiniteFrame,
}

impl std::fmt::Display for MorphOverrideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LengthMismatch => write!(
                f,
                "morph override indices and weights must have equal lengths"
            ),
            Self::InvalidIndex(index) => write!(f, "morph override index {index} is out of range"),
            Self::NonFiniteWeight(index) => {
                write!(f, "morph override weight {index} must be finite")
            }
            Self::NonFiniteFrame => write!(f, "morph override frame must be finite"),
        }
    }
}

impl std::error::Error for MorphOverrideError {}

impl RuntimeInstance {
    /// Evaluate a fresh clip (or rest) pose with sparse direct morph overrides.
    /// Overrides replace sampled weights before group expansion and bone morphs,
    /// then Append/IK run normally. Duplicate indices use the last value; weights
    /// are not clamped. Inputs are validated before any output is changed.
    /// Overrides only affect this call, so an empty input restores sampled values.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_with_morph_overrides(
        &mut self,
        clip: Option<&AnimationClip>,
        frame: f32,
        indices: &[u32],
        weights: &[f32],
        options: IkSolveOptions,
        ik_enabled: bool,
    ) -> Result<(), MorphOverrideError> {
        if !frame.is_finite() {
            return Err(MorphOverrideError::NonFiniteFrame);
        }
        if indices.len() != weights.len() {
            return Err(MorphOverrideError::LengthMismatch);
        }
        for (i, (&index, &weight)) in indices.iter().zip(weights).enumerate() {
            if index as usize >= self.pose.morph_weights().len() {
                return Err(MorphOverrideError::InvalidIndex(index));
            }
            if !weight.is_finite() {
                return Err(MorphOverrideError::NonFiniteWeight(i));
            }
        }
        if let Some(clip) = clip {
            clip.apply_to_pose(frame, &mut self.pose);
        } else {
            self.pose.reset_local_pose();
        }
        for (&index, &weight) in indices.iter().zip(weights) {
            self.pose.set_morph_weight(MorphIndex(index), weight);
        }
        self.expand_morphs();
        if !ik_enabled {
            for index in 0..self.pose.ik_enabled().len() {
                self.pose.set_ik_enabled(index, false);
            }
        }
        self.evaluate_current_pose_with_ik_options(options);
        Ok(())
    }
}
