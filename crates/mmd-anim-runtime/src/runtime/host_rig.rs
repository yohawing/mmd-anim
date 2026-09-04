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

impl RuntimeInstance {
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
        if !Arc::ptr_eq(&self.model, &rig.model) {
            return Err(HostRigError::ModelMismatch);
        }
        if self.physics_mode != PhysicsMode::Off {
            return Err(HostRigError::PhysicsEnabled);
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
        self.apply_validated_host_pose(input);
        if !rig.has_driven_bones {
            self.evaluate_current_pose_with_ik_options(options);
            return Ok(());
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
        self.evaluate_current_pose_with_ik_options(options);
        self.host_rig.as_mut().unwrap().active = false;
        Ok(())
    }

    pub(super) fn is_host_driven(&self, bone: BoneIndex) -> bool {
        self.host_rig
            .as_ref()
            .is_some_and(|rig| rig.active && rig.driven[bone.as_usize()])
    }
}
