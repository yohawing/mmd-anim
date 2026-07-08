use std::sync::Arc;

use glam::Quat;

use crate::ik_primitive::ChainLinkState;
use crate::{AnimationClip, ModelArena, PoseArena};

mod ik;
mod morph;
mod physics;
mod world;

#[cfg(test)]
use crate::ik_primitive::{
    LimitedAxesLinkStepInput, PlaneLinkStepInput, axis_vec, decompose_euler_xyz, euler_xyz_to_quat,
    limit_axis_bounds, quat_to_rotation_mat3, signed_projected_angle, solve_limited_axes_link_step,
    solve_plane_link_step,
};

#[derive(Debug)]
struct IkScratch {
    links: Vec<crate::IkLink>,
    base_rotations: Vec<Quat>,
    base_ik_rotations: Vec<Quat>,
    ik_rotations: Vec<Quat>,
    best_ik_rotations: Vec<Quat>,
    chain_states: Vec<ChainLinkState>,
}

impl IkScratch {
    fn new(model: &ModelArena) -> Self {
        let max_links = model
            .ik_solvers()
            .iter()
            .map(|s| s.links.len())
            .max()
            .unwrap_or(0);
        IkScratch {
            links: Vec::with_capacity(max_links),
            base_rotations: Vec::with_capacity(max_links),
            base_ik_rotations: Vec::with_capacity(max_links),
            ik_rotations: Vec::with_capacity(max_links),
            best_ik_rotations: Vec::with_capacity(max_links),
            chain_states: Vec::with_capacity(max_links),
        }
    }
}

#[derive(Debug)]
struct MorphScratch {
    expanded_weights: Vec<f32>,
}

impl MorphScratch {
    fn new(morph_count: usize) -> Self {
        Self {
            expanded_weights: vec![0.0; morph_count],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IkSolverRuntimeStats {
    pub solver_evaluations: u64,
    pub configured_iterations: u64,
    pub executed_iterations: u64,
    pub tolerance_precheck_breaks: u64,
    pub tolerance_post_iteration_breaks: u64,
    pub rollback_breaks: u64,
    pub max_iteration_exhaustions: u64,
    pub link_visits: u64,
    pub link_steps: u64,
    pub final_distance_sum: f64,
    pub final_distance_max: f32,
    pub exhausted_final_distance_sum: f64,
    pub exhausted_final_distance_max: f32,
}

impl IkSolverRuntimeStats {
    fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IkSolveOptions {
    pub tolerance: f32,
    pub max_iterations_cap: Option<u32>,
}

pub use physics::{PhysicsStepStats, PhysicsTickConfig};

impl Default for IkSolveOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.0,
            max_iterations_cap: None,
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WorldMatrixBoneUpdateCategory {
    LeadingBookend,
    PhaseLoop,
    TrailingBookend,
    IkLinkChange,
    #[default]
    Other,
}

#[derive(Debug)]
pub struct RuntimeInstance {
    model: Arc<ModelArena>,
    pose: PoseArena,
    physics_tick_config: PhysicsTickConfig,
    physics_accumulator_seconds: f32,
    ik_scratch: IkScratch,
    morph_scratch: MorphScratch,
    ik_stats: Vec<IkSolverRuntimeStats>,
    ik_link_change_update_bones: Vec<Option<Vec<crate::BoneIndex>>>,
    #[cfg(test)]
    world_matrix_bone_update_count: usize,
    #[cfg(test)]
    world_matrix_bone_update_category: WorldMatrixBoneUpdateCategory,
    #[cfg(test)]
    world_matrix_bone_update_leading_bookend_count: usize,
    #[cfg(test)]
    world_matrix_bone_update_phase_loop_count: usize,
    #[cfg(test)]
    world_matrix_bone_update_trailing_bookend_count: usize,
    #[cfg(test)]
    world_matrix_bone_update_ik_link_change_count: usize,
    #[cfg(test)]
    world_matrix_bone_update_other_count: usize,
}

impl RuntimeInstance {
    pub fn new(model: Arc<ModelArena>) -> Self {
        let morph_count = model.morph_count() as usize;
        Self::new_with_morph_count(model, morph_count)
    }

    pub fn new_with_morph_count(model: Arc<ModelArena>, morph_count: usize) -> Self {
        let ik_count = model.ik_count();
        Self::new_with_counts(model, morph_count, ik_count)
    }

    pub fn new_with_counts(model: Arc<ModelArena>, morph_count: usize, ik_count: usize) -> Self {
        let morph_count = morph_count.max(model.morph_count() as usize);
        let pose = PoseArena::new_with_counts(model.bone_count(), morph_count, ik_count);
        let ik_scratch = IkScratch::new(&model);
        let morph_scratch = MorphScratch::new(morph_count);
        let ik_stats = vec![IkSolverRuntimeStats::default(); model.ik_count()];
        let ik_link_change_update_bones = vec![None; model.ik_count()];
        Self {
            model,
            pose,
            physics_tick_config: PhysicsTickConfig::default(),
            physics_accumulator_seconds: 0.0,
            ik_scratch,
            morph_scratch,
            ik_stats,
            ik_link_change_update_bones,
            #[cfg(test)]
            world_matrix_bone_update_count: 0,
            #[cfg(test)]
            world_matrix_bone_update_category: WorldMatrixBoneUpdateCategory::default(),
            #[cfg(test)]
            world_matrix_bone_update_leading_bookend_count: 0,
            #[cfg(test)]
            world_matrix_bone_update_phase_loop_count: 0,
            #[cfg(test)]
            world_matrix_bone_update_trailing_bookend_count: 0,
            #[cfg(test)]
            world_matrix_bone_update_ik_link_change_count: 0,
            #[cfg(test)]
            world_matrix_bone_update_other_count: 0,
        }
    }

    #[inline]
    pub fn model(&self) -> &ModelArena {
        &self.model
    }

    #[inline]
    pub fn pose(&self) -> &PoseArena {
        &self.pose
    }

    #[inline]
    pub fn pose_mut(&mut self) -> &mut PoseArena {
        &mut self.pose
    }

    pub fn evaluate_current_pose(&mut self) {
        self.pose.reset_ik_rotations();
        self.evaluate_current_pose_ordered(IkSolveOptions::default());
    }

    pub fn evaluate_current_pose_with_ik_options(&mut self, options: IkSolveOptions) {
        self.pose.reset_ik_rotations();
        self.evaluate_current_pose_ordered(options);
    }

    /// Evaluate the current pose by updating world matrices only, without
    /// running any IK solver. This is useful for diagnostics that need to
    /// inspect clip/VMD state before IK is applied.
    pub fn evaluate_current_pose_without_ik(&mut self) {
        self.pose.reset_ik_rotations();
        self.update_world_matrices();
    }

    fn evaluate_current_pose_ordered(&mut self, options: IkSolveOptions) {
        self.begin_current_pose_evaluation();
        let mut earliest_after_physics_eval_order_position = None;
        self.evaluate_current_pose_phase(
            false,
            options,
            &mut earliest_after_physics_eval_order_position,
        );
        self.evaluate_current_pose_phase(
            true,
            options,
            &mut earliest_after_physics_eval_order_position,
        );
        self.finish_current_pose_evaluation(earliest_after_physics_eval_order_position);
    }

    pub fn evaluate_current_pose_before_physics(&mut self) {
        self.evaluate_current_pose_before_physics_with_ik_options(IkSolveOptions::default());
    }

    pub fn evaluate_current_pose_before_physics_with_ik_options(
        &mut self,
        options: IkSolveOptions,
    ) {
        self.pose.reset_ik_rotations();
        self.begin_current_pose_evaluation();
        let mut earliest_after_physics_eval_order_position = None;
        self.evaluate_current_pose_phase(
            false,
            options,
            &mut earliest_after_physics_eval_order_position,
        );
    }

    pub fn evaluate_current_pose_after_physics(&mut self) {
        self.evaluate_current_pose_after_physics_with_ik_options(IkSolveOptions::default());
    }

    pub fn evaluate_current_pose_after_physics_with_ik_options(&mut self, options: IkSolveOptions) {
        let mut earliest_after_physics_eval_order_position = None;
        self.evaluate_current_pose_phase(
            true,
            options,
            &mut earliest_after_physics_eval_order_position,
        );
        self.finish_current_pose_evaluation(earliest_after_physics_eval_order_position);
    }

    fn begin_current_pose_evaluation(&mut self) {
        self.pose.reset_append_transforms();
        #[cfg(test)]
        self.set_world_matrix_bone_update_category(WorldMatrixBoneUpdateCategory::LeadingBookend);
        self.update_world_matrices_using_current_append_from_eval_order_position(0);
    }

    fn evaluate_current_pose_phase(
        &mut self,
        after_physics: bool,
        options: IkSolveOptions,
        earliest_after_physics_eval_order_position: &mut Option<usize>,
    ) {
        let phase_bone_count = self.model.eval_order_for_phase(after_physics).len();
        for phase_index in 0..phase_bone_count {
            let bone = self.model.eval_order_for_phase(after_physics)[phase_index];
            if after_physics {
                let position = self.model.eval_order_position(bone);
                *earliest_after_physics_eval_order_position = Some(
                    (*earliest_after_physics_eval_order_position)
                        .map_or(position, |earliest| earliest.min(position)),
                );
            }
            if self.model.append_transform_index(bone).is_some() {
                self.pose.reset_append_transform(bone);
                self.update_append_transform_for_bone(bone);
            }
            #[cfg(test)]
            self.set_world_matrix_bone_update_category(WorldMatrixBoneUpdateCategory::PhaseLoop);
            self.update_world_matrix_for_bone(bone);

            let ik_solver_count = self.model.ik_solver_count_for_bone(bone);
            for local_index in 0..ik_solver_count {
                let ik_index = self.model.ik_solver_index_for_bone(bone, local_index);
                self.solve_ik_solver(ik_index, options, after_physics);
            }
        }
    }

    fn finish_current_pose_evaluation(
        &mut self,
        earliest_after_physics_eval_order_position: Option<usize>,
    ) {
        let mut trailing_refresh_start = earliest_after_physics_eval_order_position;
        for append in self.model.append_transforms() {
            let source_position = self.model.eval_order_position(append.source_bone);
            let target_position = self.model.eval_order_position(append.target_bone);
            if target_position < source_position {
                trailing_refresh_start = Some(
                    trailing_refresh_start
                        .map_or(target_position, |start| start.min(target_position)),
                );
            }
        }

        if let Some(start_position) = trailing_refresh_start {
            let start_position =
                self.expand_update_start_for_append_dependencies(start_position, None);
            #[cfg(test)]
            self.set_world_matrix_bone_update_category(
                WorldMatrixBoneUpdateCategory::TrailingBookend,
            );
            self.update_world_matrices_from_eval_order_position(start_position);
        }
    }

    pub fn evaluate_rest_pose(&mut self) {
        self.pose.reset_local_pose();
        self.evaluate_current_pose();
    }

    pub fn evaluate_clip_frame(&mut self, clip: &AnimationClip, frame: f32) {
        clip.apply_to_pose(frame, &mut self.pose);
        self.expand_morphs();
        self.evaluate_current_pose();
    }

    pub fn evaluate_clip_frame_with_ik_options(
        &mut self,
        clip: &AnimationClip,
        frame: f32,
        options: IkSolveOptions,
    ) {
        clip.apply_to_pose(frame, &mut self.pose);
        self.expand_morphs();
        self.evaluate_current_pose_with_ik_options(options);
    }

    pub fn evaluate_clip_frame_before_physics(&mut self, clip: &AnimationClip, frame: f32) {
        self.evaluate_clip_frame_before_physics_with_ik_options(
            clip,
            frame,
            IkSolveOptions::default(),
        );
    }

    pub fn evaluate_clip_frame_before_physics_with_ik_options(
        &mut self,
        clip: &AnimationClip,
        frame: f32,
        options: IkSolveOptions,
    ) {
        clip.apply_to_pose(frame, &mut self.pose);
        self.expand_morphs();
        self.evaluate_current_pose_before_physics_with_ik_options(options);
    }

    /// Evaluate a clip frame but stop before solving IK. Applies the clip to
    /// the pose, expands morphs, and updates world matrices - the same setup
    /// as [`Self::evaluate_clip_frame`] but without calling `solve_enabled_ik`.
    /// Useful for diagnostics that need to inspect pre-IK runtime state.
    pub fn evaluate_clip_frame_without_ik(&mut self, clip: &AnimationClip, frame: f32) {
        clip.apply_to_pose(frame, &mut self.pose);
        self.expand_morphs();
        self.pose.reset_ik_rotations();
        self.update_world_matrices();
    }

    pub fn reset_ik_runtime_stats(&mut self) {
        for stats in &mut self.ik_stats {
            stats.reset();
        }
    }

    pub fn ik_runtime_stats(&self) -> &[IkSolverRuntimeStats] {
        &self.ik_stats
    }

    #[inline]
    pub fn append_position_offset(&self, bone: crate::BoneIndex) -> glam::Vec3A {
        self.pose.append_position_offset(bone)
    }

    #[inline]
    pub fn append_rotation(&self, bone: crate::BoneIndex) -> glam::Quat {
        self.pose.append_rotation(bone)
    }

    #[inline]
    pub fn ik_enabled(&self) -> &[u8] {
        self.pose.ik_enabled()
    }
}

#[cfg(test)]
mod tests;
