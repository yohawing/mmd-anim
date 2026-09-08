use std::sync::Arc;

use glam::Mat4;

use crate::{BoneIndex, ModelArena};

use super::{HostPoseError, HostPoseView, IkSolveOptions, PhysicsMode, RuntimeInstance};

/// Model-bound ownership for a retargeted, pre-morph MMD local pose.
///
/// Driven bones retain their post-morph, pre-Append/IK model-space transform.
/// Other bones retain ordinary MMD evaluation. IK is opt-in: list the PMX
/// controller bones whose current input poses supply explicit goals, and enable
/// their chains in each HostPoseView. No goals are inferred from FK feet.
#[derive(Debug)]
pub struct HostRigDefinition {
    model: Arc<ModelArena>,
    driven: Box<[bool]>,
    has_driven_bones: bool,
    allowed_ik: Box<[bool]>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum HostRigError {
    #[error("host rig bone index {0} is outside the model")]
    InvalidBone(u32),
    #[error("host rig contains duplicate bone index {0}")]
    DuplicateBone(u32),
    #[error("bone {0} is not a PMX IK controller")]
    InvalidGoal(u32),
    #[error("IK chain {chain} can move host-driven bone {bone}")]
    IkConflict { chain: usize, bone: u32 },
    #[error("IK chain {0} has no declared input goal")]
    MissingGoal(usize),
    #[error("host rig belongs to a different model")]
    ModelMismatch,
    #[error("host rig evaluation is already active")]
    EvaluationActive,
    #[error("host rig evaluation currently requires PhysicsMode::Off")]
    PhysicsEnabled,
    #[error("IK tolerance must be finite and non-negative")]
    InvalidTolerance,
    #[error("host rig scale at bone {0} must be positive and uniform")]
    InvalidScale(usize),
    #[error(transparent)]
    Pose(#[from] HostPoseError),
}

impl HostRigDefinition {
    pub fn new(
        model: Arc<ModelArena>,
        driven_bones: &[BoneIndex],
        goal_bones: &[BoneIndex],
    ) -> Result<Self, HostRigError> {
        let driven = bone_mask(model.bone_count(), driven_bones)?;
        let goals = bone_mask(model.bone_count(), goal_bones)?;
        for goal in goal_bones {
            if !model.ik_solvers().iter().any(|s| s.ik_bone == *goal) {
                return Err(HostRigError::InvalidGoal(goal.0));
            }
        }
        let mut allowed_ik = vec![false; model.ik_count()];
        for (chain, solver) in model.ik_solvers().iter().enumerate() {
            if !goals[solver.ik_bone.as_usize()] {
                continue;
            }
            // Reject indirect parent writes as well as direct link writes.
            for bone in driven_bones {
                let mut ancestor = Some(*bone);
                while let Some(current) = ancestor {
                    if solver.links.iter().any(|link| link.bone == current) {
                        return Err(HostRigError::IkConflict {
                            chain,
                            bone: bone.0,
                        });
                    }
                    ancestor = model.parent_index(current);
                }
            }
            allowed_ik[chain] = true;
        }
        Ok(Self {
            model,
            driven,
            has_driven_bones: !driven_bones.is_empty(),
            allowed_ik: allowed_ik.into_boxed_slice(),
        })
    }
}

fn bone_mask(count: usize, bones: &[BoneIndex]) -> Result<Box<[bool]>, HostRigError> {
    let mut mask = vec![false; count];
    for bone in bones {
        let Some(entry) = mask.get_mut(bone.as_usize()) else {
            return Err(HostRigError::InvalidBone(bone.0));
        };
        if *entry {
            return Err(HostRigError::DuplicateBone(bone.0));
        }
        *entry = true;
    }
    Ok(mask.into_boxed_slice())
}

#[derive(Debug)]
pub(super) struct HostRigScratch {
    pub driven: Box<[bool]>,
    pub reference_world: Box<[Mat4]>,
    pub active: bool,
}

/// A scoped host-rig evaluation context.
///
/// The context keeps the protected model-space transforms active while a
/// caller runs the before-physics phase, lets the physics bridge write back
/// unprotected bones, and evaluates the after-physics phase. Dropping the
/// context always clears the active flag, including when the bridge returns
/// an error or unwinds through an FFI panic guard.
pub struct HostRigEvaluation<'a> {
    runtime: &'a mut RuntimeInstance,
    active: bool,
}

impl HostRigEvaluation<'_> {
    /// Evaluate the current pose through the before-physics phase.
    pub fn evaluate_before_physics_with_ik_options(&mut self, options: IkSolveOptions) {
        self.runtime
            .evaluate_current_pose_before_physics_with_ik_options(options);
    }

    /// Evaluate the current pose through the after-physics phase.
    pub fn evaluate_after_physics_with_ik_options(&mut self, options: IkSolveOptions) {
        self.runtime
            .evaluate_current_pose_after_physics_with_ik_options(options);
    }

    /// Borrow the runtime for a physics bridge operation while the host-rig
    /// ownership context remains active.
    pub fn runtime_mut(&mut self) -> &mut RuntimeInstance {
        self.runtime
    }
}

impl Drop for HostRigEvaluation<'_> {
    fn drop(&mut self) {
        if self.active {
            if let Some(scratch) = self.runtime.host_rig.as_mut() {
                scratch.active = false;
            }
        }
    }
}

impl RuntimeInstance {
    /// Apply a fresh pre-morph host pose and open a scoped host-rig context.
    ///
    /// The returned context owns the active lifetime of the protected driven
    /// transforms. It must stay alive across physics writeback and the
    /// after-physics evaluation so that a physics body cannot overwrite a
    /// host-driven bone.
    pub fn begin_host_rig_evaluation<'a>(
        &'a mut self,
        rig: &HostRigDefinition,
        input: &HostPoseView<'_>,
        options: IkSolveOptions,
    ) -> Result<HostRigEvaluation<'a>, HostRigError> {
        self.validate_host_rig_input(rig, input, options)?;
        self.apply_validated_host_pose(input);
        if !rig.has_driven_bones {
            return Ok(HostRigEvaluation {
                runtime: self,
                active: false,
            });
        }

        let scratch = self.host_rig.get_or_insert_with(|| HostRigScratch {
            driven: vec![false; rig.driven.len()].into_boxed_slice(),
            reference_world: vec![Mat4::IDENTITY; rig.driven.len()].into_boxed_slice(),
            active: false,
        });
        scratch.driven.copy_from_slice(&rig.driven);
        // Model eval_order is parent-first. Freeze pure FK after morphs and
        // before any Append, fixed-axis projection, or IK has run.
        for &bone in self.model.eval_order() {
            let local = Mat4::from_scale_rotation_translation(
                self.pose.local_scale(bone).into(),
                self.pose.local_rotation(bone),
                (self.model.rest_position(bone) + self.pose.local_position_offset(bone)).into(),
            );
            scratch.reference_world[bone.as_usize()] = match self.model.parent_index(bone) {
                Some(parent) => scratch.reference_world[parent.as_usize()] * local,
                None => local,
            };
        }
        scratch.active = true;
        Ok(HostRigEvaluation {
            runtime: self,
            active: true,
        })
    }

    fn validate_host_rig_input(
        &self,
        rig: &HostRigDefinition,
        input: &HostPoseView<'_>,
        options: IkSolveOptions,
    ) -> Result<(), HostRigError> {
        if self.host_rig.as_ref().is_some_and(|scratch| scratch.active) {
            return Err(HostRigError::EvaluationActive);
        }
        if !Arc::ptr_eq(&self.model, &rig.model) {
            return Err(HostRigError::ModelMismatch);
        }
        if !options.tolerance.is_finite() || options.tolerance < 0.0 {
            return Err(HostRigError::InvalidTolerance);
        }
        self.validate_host_pose(input)?;
        for (index, scale) in input.local_scales.iter().enumerate() {
            if scale.x <= 0.0 || scale.x != scale.y || scale.x != scale.z {
                return Err(HostRigError::InvalidScale(index));
            }
        }
        for (chain, &enabled) in input
            .ik_enabled
            .iter()
            .take(self.model.ik_count())
            .enumerate()
        {
            if enabled != 0 && !rig.allowed_ik[chain] {
                return Err(HostRigError::MissingGoal(chain));
            }
        }
        Ok(())
    }

    /// Evaluate a complete retargeted input through both MMD phases, without
    /// physics. Inputs have the same pre-morph meaning as `apply_host_pose`.
    ///
    /// Start every frame from a fresh base pose (including helpers); never feed
    /// previous output matrices back as base transforms. Driven bones bypass
    /// Append and fixed-axis projection. Their input local deltas, including
    /// bone morphs, are authoritative Append sources even when PMX declares
    /// an incoming Append on those bones. Descendants use the protected world
    /// transforms during evaluation, not a post-evaluation matrix patch.
    ///
    /// All returned errors leave the previous pose intact. Existing clip and
    /// current-pose evaluation methods retain their original semantics.
    pub fn evaluate_host_rig_pose(
        &mut self,
        rig: &HostRigDefinition,
        input: &HostPoseView<'_>,
        options: IkSolveOptions,
    ) -> Result<(), HostRigError> {
        if self.physics_mode != PhysicsMode::Off {
            return Err(HostRigError::PhysicsEnabled);
        }
        let mut evaluation = self.begin_host_rig_evaluation(rig, input, options)?;
        evaluation.evaluate_before_physics_with_ik_options(options);
        evaluation.evaluate_after_physics_with_ik_options(options);
        Ok(())
    }

    pub(super) fn is_host_driven(&self, bone: BoneIndex) -> bool {
        self.host_rig
            .as_ref()
            .is_some_and(|rig| rig.active && rig.driven[bone.as_usize()])
    }
}
