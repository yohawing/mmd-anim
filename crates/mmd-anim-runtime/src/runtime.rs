use std::sync::Arc;

use glam::{Mat4, Quat};

use crate::{AnimationClip, ModelArena, MorphIndex, PoseArena};
use crate::{
    append_primitive::{AppendPrimitiveInput, solve_append_transform},
    ik_primitive::{
        ChainLinkState, LinkStepInput, constrain_rotation_to_axis, rotation, solve_link_step,
        translation,
    },
};

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

impl Default for IkSolveOptions {
    fn default() -> Self {
        Self {
            tolerance: 0.0,
            max_iterations_cap: None,
        }
    }
}

#[derive(Debug)]
pub struct RuntimeInstance {
    model: Arc<ModelArena>,
    pose: PoseArena,
    ik_scratch: IkScratch,
    morph_scratch: MorphScratch,
    ik_stats: Vec<IkSolverRuntimeStats>,
    #[cfg(test)]
    world_matrix_bone_update_count: usize,
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
        Self {
            model,
            pose,
            ik_scratch,
            morph_scratch,
            ik_stats,
            #[cfg(test)]
            world_matrix_bone_update_count: 0,
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

    fn update_world_matrices(&mut self) {
        self.update_world_matrices_from_eval_order_position(0);
    }

    fn update_world_matrices_from_eval_order_position(&mut self, start_position: usize) {
        self.update_world_matrices_from_eval_order_position_for_phase(start_position, None);
    }

    fn update_world_matrices_from_eval_order_position_for_phase(
        &mut self,
        start_position: usize,
        phase: Option<bool>,
    ) {
        let start_position =
            self.expand_update_start_for_append_dependencies(start_position, phase);
        for bone in &self.model.eval_order()[start_position..] {
            if !self.bone_matches_phase(*bone, phase) {
                continue;
            }
            self.pose.reset_append_transform(*bone);
        }
        for position in start_position..self.model.eval_order().len() {
            let bone = self.model.eval_order()[position];
            if !self.bone_matches_phase(bone, phase) {
                continue;
            }
            self.update_append_transform_for_bone(bone);
            self.update_world_matrix_for_bone(bone);
        }
    }

    fn update_world_matrices_using_current_append_from_eval_order_position(
        &mut self,
        start_position: usize,
    ) {
        self.update_world_matrices_using_current_append_from_eval_order_position_for_phase(
            start_position,
            None,
        );
    }

    fn update_world_matrices_using_current_append_from_eval_order_position_for_phase(
        &mut self,
        start_position: usize,
        phase: Option<bool>,
    ) {
        for position in start_position..self.model.eval_order().len() {
            let bone = self.model.eval_order()[position];
            if !self.bone_matches_phase(bone, phase) {
                continue;
            }
            self.update_world_matrix_for_bone(bone);
        }
    }

    #[inline]
    fn bone_matches_phase(&self, bone: crate::BoneIndex, phase: Option<bool>) -> bool {
        phase.is_none_or(|after_physics| self.model.transform_after_physics(bone) == after_physics)
    }

    fn update_append_transform_for_bone(&mut self, bone: crate::BoneIndex) {
        let Some(append_index) = self.model.append_transform_index(bone) else {
            return;
        };
        let append = self.model.append_transform(append_index);
        let use_source_append = !append.local
            && self
                .model
                .append_transform_index(append.source_bone)
                .is_some();
        let mut source_rotation = if use_source_append {
            self.pose.append_rotation(append.source_bone)
        } else {
            self.pose.local_rotation(append.source_bone)
        };
        if use_source_append && self.model.is_ik_link_bone(append.source_bone) {
            source_rotation =
                (self.pose.ik_rotation(append.source_bone) * source_rotation).normalize();
        }
        let source_position_offset = if use_source_append {
            self.pose.append_position_offset(append.source_bone)
        } else {
            self.pose.local_position_offset(append.source_bone)
        };
        let append_output = solve_append_transform(AppendPrimitiveInput {
            source_position_offset,
            source_rotation,
            ratio: append.ratio,
            affect_rotation: append.affect_rotation,
            affect_translation: append.affect_translation,
        });
        self.pose.set_append_rotation(bone, append_output.rotation);
        self.pose
            .set_append_position_offset(bone, append_output.position_offset);
    }

    fn update_world_matrix_for_bone(&mut self, bone: crate::BoneIndex) {
        #[cfg(test)]
        {
            self.world_matrix_bone_update_count += 1;
        }
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

        let local_matrix = Mat4::from_scale_rotation_translation(
            local_scale.into(),
            local_rotation,
            local_position.into(),
        );

        let world_matrix = match self.model.parent_index(bone) {
            Some(parent) => self.pose.world_matrices()[parent.as_usize()] * local_matrix,
            None => local_matrix,
        };

        self.pose.set_world_matrix(bone, world_matrix);
        self.pose
            .set_skinning_matrix(bone, world_matrix * self.model.inverse_bind_matrix(bone));
    }

    fn expand_update_start_for_append_dependencies(
        &self,
        start_position: usize,
        phase: Option<bool>,
    ) -> usize {
        let mut start = start_position;
        loop {
            let mut changed = false;
            for append in self.model.append_transforms() {
                if !self.bone_matches_phase(append.target_bone, phase) {
                    continue;
                }
                let source_position = self.model.eval_order_position(append.source_bone);
                let target_position = self.model.eval_order_position(append.target_bone);
                if source_position >= start && target_position < start {
                    start = target_position;
                    changed = true;
                }
            }
            if !changed {
                return start;
            }
        }
    }

    fn evaluate_current_pose_ordered(&mut self, options: IkSolveOptions) {
        self.pose.reset_append_transforms();
        self.update_world_matrices_using_current_append_from_eval_order_position(0);

        for after_physics in [false, true] {
            for position in 0..self.model.eval_order().len() {
                let bone = self.model.eval_order()[position];
                if self.model.transform_after_physics(bone) != after_physics {
                    continue;
                }

                if self.model.append_transform_index(bone).is_some() {
                    self.pose.reset_append_transform(bone);
                    self.update_append_transform_for_bone(bone);
                }
                self.update_world_matrix_for_bone(bone);

                for ik_index in 0..self.model.ik_count() {
                    if self.model.ik_solvers()[ik_index].ik_bone == bone {
                        self.solve_ik_solver(ik_index, options, after_physics);
                    }
                }
            }
        }
        self.update_world_matrices_using_current_append_from_eval_order_position(0);
    }

    fn solve_ik_solver(&mut self, ik_index: usize, options: IkSolveOptions, after_physics: bool) {
        if self.pose.ik_enabled()[ik_index] == 0 {
            return;
        }

        let tolerance = options.tolerance.max(0.0);
        let mut links = std::mem::take(&mut self.ik_scratch.links);
        let mut base_rotations = std::mem::take(&mut self.ik_scratch.base_rotations);
        let mut base_ik_rotations = std::mem::take(&mut self.ik_scratch.base_ik_rotations);
        let mut ik_rotations = std::mem::take(&mut self.ik_scratch.ik_rotations);
        let mut best_ik_rotations = std::mem::take(&mut self.ik_scratch.best_ik_rotations);
        let mut chain_states = std::mem::take(&mut self.ik_scratch.chain_states);

        {
            let solver = &self.model.ik_solvers()[ik_index];
            let ik_bone = solver.ik_bone;
            let target_bone = solver.target_bone;
            let iteration_count = options
                .max_iterations_cap
                .map(|cap| solver.iteration_count.min(cap))
                .unwrap_or(solver.iteration_count)
                .max(1) as usize;
            let limit_angle = solver.limit_angle.max(0.0);
            let link_count = solver.links.len();

            links.clear();
            links.extend(solver.links.iter().cloned());
            self.ik_stats[ik_index].solver_evaluations += 1;
            self.ik_stats[ik_index].configured_iterations += iteration_count as u64;

            base_rotations.clear();
            base_rotations.extend(links.iter().map(|l| self.pose.local_rotation(l.bone)));
            base_ik_rotations.clear();
            base_ik_rotations.extend(links.iter().map(|l| self.pose.ik_rotation(l.bone)));
            ik_rotations.clear();
            ik_rotations.resize(link_count, Quat::IDENTITY);
            best_ik_rotations.clear();
            best_ik_rotations.resize(link_count, Quat::IDENTITY);
            chain_states.clear();
            chain_states.resize_with(link_count, || ChainLinkState {
                previous_euler: [0.0; 3],
                plane_mode_angle: 0.0,
            });

            // Always start from base rotations (IK deltas start at identity).
            self.apply_ik_link_rotations(
                &links,
                &base_rotations,
                &base_ik_rotations,
                &ik_rotations,
            );
            self.update_world_matrices_after_ik_link_change(
                &links,
                ik_bone,
                target_bone,
                Some(after_physics),
            );

            let mut broke_early = false;
            let mut final_distance = f32::MAX;
            let mut best_distance = f32::MAX;
            for _iteration in 0..iteration_count {
                // Tolerance early exit
                let eff_pos = translation(self.pose.world_matrices()[target_bone.as_usize()]);
                let ik_pos = translation(self.pose.world_matrices()[ik_bone.as_usize()]);
                final_distance = (eff_pos - ik_pos).length();
                if final_distance <= tolerance {
                    self.ik_stats[ik_index].tolerance_precheck_breaks += 1;
                    broke_early = true;
                    break;
                }
                self.ik_stats[ik_index].executed_iterations += 1;

                for link_index in 0..link_count {
                    let link = &links[link_index];
                    let link_bone = link.bone;
                    self.ik_stats[ik_index].link_visits += 1;

                    if link_bone == target_bone {
                        continue;
                    }

                    let link_world = self.pose.world_matrices()[link_bone.as_usize()];
                    let link_pos = translation(link_world);
                    let eff_pos = translation(self.pose.world_matrices()[target_bone.as_usize()]);
                    let ik_pos = translation(self.pose.world_matrices()[ik_bone.as_usize()]);

                    // Transform direction vectors to link-local space
                    let link_world_rot = rotation(link_world);
                    let local_effector = link_world_rot.inverse().mul_vec3a(eff_pos - link_pos);
                    let local_target = link_world_rot.inverse().mul_vec3a(ik_pos - link_pos);

                    if local_effector.length_squared() <= f32::EPSILON
                        || local_target.length_squared() <= f32::EPSILON
                    {
                        continue;
                    }

                    solve_link_step(LinkStepInput {
                        local_effector: &local_effector,
                        local_target: &local_target,
                        link_index,
                        base_rotations: &base_rotations,
                        ik_rotations: &mut ik_rotations,
                        chain_states: &mut chain_states,
                        angle_limit: link.angle_limit,
                        iteration: _iteration,
                        limit_angle,
                    });

                    self.apply_ik_link_rotations(
                        &links,
                        &base_rotations,
                        &base_ik_rotations,
                        &ik_rotations,
                    );
                    self.update_world_matrices_after_ik_link_change(
                        &links,
                        ik_bone,
                        target_bone,
                        Some(after_physics),
                    );
                    self.ik_stats[ik_index].link_steps += 1;
                }

                // Best rotations tracking
                let current_distance = {
                    let eff = translation(self.pose.world_matrices()[target_bone.as_usize()]);
                    let ik = translation(self.pose.world_matrices()[ik_bone.as_usize()]);
                    (eff - ik).length()
                };
                final_distance = current_distance;

                if current_distance < best_distance {
                    best_distance = current_distance;
                    best_ik_rotations.copy_from_slice(&ik_rotations);
                    if current_distance <= tolerance {
                        self.ik_stats[ik_index].tolerance_post_iteration_breaks += 1;
                        broke_early = true;
                        break;
                    }
                } else {
                    self.ik_stats[ik_index].rollback_breaks += 1;
                    ik_rotations.copy_from_slice(&best_ik_rotations);
                    self.apply_ik_link_rotations(
                        &links,
                        &base_rotations,
                        &base_ik_rotations,
                        &ik_rotations,
                    );
                    self.update_world_matrices_after_ik_link_change(
                        &links,
                        ik_bone,
                        target_bone,
                        Some(after_physics),
                    );
                    broke_early = true;
                    break;
                }
            }
            self.ik_stats[ik_index].final_distance_sum += f64::from(final_distance);
            self.ik_stats[ik_index].final_distance_max = self.ik_stats[ik_index]
                .final_distance_max
                .max(final_distance);
            if !broke_early {
                self.ik_stats[ik_index].max_iteration_exhaustions += 1;
                self.ik_stats[ik_index].exhausted_final_distance_sum += f64::from(final_distance);
                self.ik_stats[ik_index].exhausted_final_distance_max = self.ik_stats[ik_index]
                    .exhausted_final_distance_max
                    .max(final_distance);
            }

            // Apply final best effective rotations
            self.apply_ik_link_rotations(
                &links,
                &base_rotations,
                &base_ik_rotations,
                &best_ik_rotations,
            );
            self.update_world_matrices_after_ik_link_change(
                &links,
                ik_bone,
                target_bone,
                Some(after_physics),
            );
        }

        self.ik_scratch.links = links;
        self.ik_scratch.base_rotations = base_rotations;
        self.ik_scratch.base_ik_rotations = base_ik_rotations;
        self.ik_scratch.ik_rotations = ik_rotations;
        self.ik_scratch.best_ik_rotations = best_ik_rotations;
        self.ik_scratch.chain_states = chain_states;
    }

    fn update_world_matrices_after_ik_link_change(
        &mut self,
        links: &[crate::IkLink],
        ik_bone: crate::BoneIndex,
        target_bone: crate::BoneIndex,
        phase: Option<bool>,
    ) {
        let start_position =
            self.min_ik_dependency_eval_order_position(links, ik_bone, target_bone);
        let start_position = self.expand_update_start_for_append_dependencies(start_position, None);
        for position in start_position..self.model.eval_order().len() {
            let bone = self.model.eval_order()[position];
            if self.bone_matches_phase(bone, phase)
                || self.bone_is_in_ik_update_scope(bone, links, ik_bone, target_bone)
                || self.bone_depends_on_ik_update_scope_append_source(
                    bone,
                    links,
                    ik_bone,
                    target_bone,
                )
            {
                self.update_append_transform_for_bone(bone);
                self.update_world_matrix_for_bone(bone);
            }
        }
    }

    fn min_ik_dependency_eval_order_position(
        &self,
        links: &[crate::IkLink],
        ik_bone: crate::BoneIndex,
        target_bone: crate::BoneIndex,
    ) -> usize {
        let mut min_position = self.model.eval_order_position(ik_bone);
        min_position = min_position.min(self.model.eval_order_position(target_bone));
        for link in links {
            min_position = min_position.min(self.model.eval_order_position(link.bone));
        }
        for bone in [ik_bone, target_bone]
            .into_iter()
            .chain(links.iter().map(|link| link.bone))
        {
            let mut current = Some(bone);
            while let Some(parent) = current {
                min_position = min_position.min(self.model.eval_order_position(parent));
                current = self.model.parent_index(parent);
            }
        }
        min_position
    }

    fn bone_is_in_ik_update_scope(
        &self,
        bone: crate::BoneIndex,
        links: &[crate::IkLink],
        ik_bone: crate::BoneIndex,
        target_bone: crate::BoneIndex,
    ) -> bool {
        if bone == ik_bone || bone == target_bone || links.iter().any(|link| link.bone == bone) {
            return true;
        }
        if self.bone_is_ancestor_of(bone, ik_bone) || self.bone_is_ancestor_of(bone, target_bone) {
            return true;
        }
        links.iter().any(|link| {
            self.bone_is_ancestor_of(bone, link.bone) || self.bone_is_ancestor_of(link.bone, bone)
        })
    }

    fn bone_depends_on_ik_update_scope_append_source(
        &self,
        bone: crate::BoneIndex,
        links: &[crate::IkLink],
        ik_bone: crate::BoneIndex,
        target_bone: crate::BoneIndex,
    ) -> bool {
        let mut changed_append_roots = Vec::new();
        loop {
            let mut changed = false;
            for append in self.model.append_transforms() {
                let source_changed = self.bone_is_in_ik_update_scope(
                    append.source_bone,
                    links,
                    ik_bone,
                    target_bone,
                ) || changed_append_roots.iter().any(|root| {
                    append.source_bone == *root
                        || self.bone_is_ancestor_of(*root, append.source_bone)
                });
                if source_changed && !changed_append_roots.contains(&append.target_bone) {
                    changed_append_roots.push(append.target_bone);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        changed_append_roots
            .iter()
            .any(|root| bone == *root || self.bone_is_ancestor_of(*root, bone))
    }

    fn bone_is_ancestor_of(&self, ancestor: crate::BoneIndex, bone: crate::BoneIndex) -> bool {
        let mut current = self.model.parent_index(bone);
        while let Some(parent) = current {
            if parent == ancestor {
                return true;
            }
            current = self.model.parent_index(parent);
        }
        false
    }

    fn apply_ik_link_rotations(
        &mut self,
        links: &[crate::IkLink],
        base_rotations: &[Quat],
        base_ik_rotations: &[Quat],
        ik_rotations: &[Quat],
    ) {
        for (i, link) in links.iter().enumerate() {
            let effective = (ik_rotations[i] * base_rotations[i]).normalize();
            let total_ik = (ik_rotations[i] * base_ik_rotations[i]).normalize();
            self.pose.set_ik_rotation(link.bone, total_ik);
            self.pose.set_local_rotation(link.bone, effective);
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

    /// Expand group morphs and apply bone morph offsets.
    ///
    /// Called automatically from [`Self::evaluate_clip_frame`]. Exposed publicly so
    /// that hosts manually driving [`PoseArena`] can trigger morph expansion
    /// before calling [`Self::evaluate_current_pose`].
    pub fn expand_morphs(&mut self) {
        self.expand_group_morphs();
        self.apply_bone_morphs();
    }

    /// Pass 1: expand all group morph weights (updates morph_weights in-place).
    /// Group morph children may appear before or after their parents in PMX, so
    /// expansion follows the graph recursively using the model-validated
    /// cycle-free group morph spans.
    fn expand_group_morphs(&mut self) {
        let spans = self.model.group_morph_spans();
        let offsets = self.model.group_morph_offsets();
        if spans.is_empty() || offsets.is_empty() {
            return;
        }
        let mc = self.model.morph_count() as usize;
        self.morph_scratch.expanded_weights.clear();
        self.morph_scratch
            .expanded_weights
            .extend_from_slice(&self.pose.morph_weights()[..mc]);

        for (morph_idx, &w) in self.pose.morph_weights()[..mc].iter().enumerate() {
            if w == 0.0 {
                continue;
            }
            expand_group_morph_weight(
                morph_idx,
                w,
                spans,
                offsets,
                &mut self.morph_scratch.expanded_weights,
            );
        }
        for (i, &w) in self.morph_scratch.expanded_weights.iter().enumerate() {
            self.pose.set_morph_weight(MorphIndex(i as u32), w);
        }
    }

    /// Pass 2: apply bone morph offsets using the final (expanded) morph
    /// weights.
    fn apply_bone_morphs(&mut self) {
        let spans = self.model.bone_morph_spans();
        let offsets = self.model.bone_morph_offsets();
        if spans.is_empty() || offsets.is_empty() {
            return;
        }
        for (morph_idx, span) in spans.iter().enumerate() {
            let weight = self.pose.morph_weight(MorphIndex(morph_idx as u32));
            if weight == 0.0 {
                continue;
            }
            for i in span.start..span.start + span.count {
                let off = &offsets[i as usize];
                let pos = self.pose.local_position_offset(off.target_bone);
                self.pose
                    .set_local_position_offset(off.target_bone, pos + off.position_offset * weight);
                let rot = self.pose.local_rotation(off.target_bone);
                let scaled = Quat::IDENTITY.slerp(off.rotation_offset, weight);
                self.pose
                    .set_local_rotation(off.target_bone, (rot * scaled).normalize());
            }
        }
    }

    #[inline]
    pub fn world_matrices(&self) -> &[Mat4] {
        self.pose.world_matrices()
    }

    #[cfg(test)]
    fn reset_world_matrix_bone_update_count(&mut self) {
        self.world_matrix_bone_update_count = 0;
    }

    #[cfg(test)]
    fn world_matrix_bone_update_count(&self) -> usize {
        self.world_matrix_bone_update_count
    }

    #[inline]
    pub fn skinning_matrices(&self) -> &[Mat4] {
        self.pose.skinning_matrices()
    }

    #[inline]
    pub fn morph_weights(&self) -> &[f32] {
        self.pose.morph_weights()
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
    pub fn ik_enabled(&self) -> &[u8] {
        self.pose.ik_enabled()
    }
}

fn expand_group_morph_weight(
    morph_idx: usize,
    weight: f32,
    spans: &[crate::MorphOffsetSpan],
    offsets: &[crate::GroupMorphOffset],
    expanded_weights: &mut [f32],
) {
    let span = spans[morph_idx];
    for i in span.start..span.start + span.count {
        let off = &offsets[i as usize];
        let child = off.child_morph.as_usize();
        let contribution = weight * off.ratio;
        expanded_weights[child] += contribution;
        if spans[child].count > 0 {
            expand_group_morph_weight(child, contribution, spans, offsets, expanded_weights);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use glam::{Quat, Vec3A};

    use crate::{
        AnimationClip, AppendTransformInit, BoneAnimationBinding, BoneIndex, BoneInit,
        IkAngleLimit, IkLinkInit, IkSolverInit, ModelArena, MovableBoneKeyframe, MovableBoneTrack,
        RuntimeInstance,
    };

    fn translation(matrix: glam::Mat4) -> Vec3A {
        Vec3A::from_vec4(matrix.w_axis)
    }

    fn assert_vec3a_near(actual: Vec3A, expected: Vec3A) {
        let delta = (actual - expected).abs();
        assert!(
            delta.x < 1.0e-5 && delta.y < 1.0e-5 && delta.z < 1.0e-5,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    #[test]
    fn evaluates_rest_pose_world_matrices() {
        let model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
            ])
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_rest_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[0]),
            Vec3A::new(1.0, 0.0, 0.0),
        );
        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(1.0, 2.0, 0.0),
        );
    }

    #[test]
    fn evaluates_current_pose_with_parent_rotation() {
        let model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
            ])
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.pose_mut().set_local_rotation(
            BoneIndex(0),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );
        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(-1.0, 0.0, 0.0),
        );
    }

    #[test]
    fn fixed_axis_bone_rotation_keeps_only_axis_twist() {
        let model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::ZERO).with_fixed_axis(Vec3A::Y),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
            ])
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.pose_mut().set_local_rotation(
            BoneIndex(0),
            (Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
                * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
            .normalize(),
        );
        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(0.0, 0.0, -1.0),
        );
    }

    #[test]
    fn evaluates_current_pose_with_local_position_offset() {
        let model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
            ])
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(1), Vec3A::new(0.0, 0.0, 3.0));
        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(1.0, 2.0, 3.0),
        );
    }

    #[test]
    fn evaluates_clip_frame_into_world_matrices() {
        let model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
            ])
            .unwrap(),
        );
        let clip = AnimationClip::new(vec![BoneAnimationBinding {
            bone: BoneIndex(1),
            track: MovableBoneTrack::from_keyframes(vec![
                MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                MovableBoneKeyframe::new(10, Vec3A::new(0.0, 0.0, 4.0), Quat::IDENTITY),
            ]),
        }]);
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_clip_frame(&clip, 5.0);

        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(1.0, 2.0, 2.0),
        );
    }

    #[test]
    fn applies_append_rotation_before_world_matrix_output() {
        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
                ],
                Vec::new(),
                vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0).with_rotation()],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.pose_mut().set_local_rotation(
            BoneIndex(0),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );
        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[2]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn applies_append_translation_before_world_matrix_output() {
        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                Vec::new(),
                vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 0.5).with_translation()],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime
            .pose_mut()
            .set_local_position_offset(BoneIndex(0), Vec3A::new(2.0, 0.0, 0.0));
        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(1.0, 1.0, 0.0),
        );
    }

    #[test]
    fn initializes_ik_enabled_from_model_solvers() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
                ],
                vec![IkSolverInit::new(
                    BoneIndex(1),
                    BoneIndex(0),
                    vec![IkLinkInit::new(BoneIndex(0))],
                )],
            )
            .unwrap(),
        );

        let runtime = RuntimeInstance::new(model);

        assert_eq!(runtime.ik_enabled(), &[1]);
    }

    #[test]
    fn solves_one_link_ik_toward_controller_bone() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn skips_disabled_ik_solver() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.pose_mut().set_ik_enabled(0, false);
        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(1.0, 0.0, 0.0),
        );
    }

    #[test]
    fn solves_two_link_ik_chain_toward_controller_bone() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(3),
                    target_bone: BoneIndex(2),
                    links: vec![IkLinkInit::new(BoneIndex(1)), IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 4,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[2]),
            Vec3A::new(1.0, 1.0, 0.0),
        );
    }

    #[test]
    fn evaluates_all_solvers_attached_to_same_ik_bone() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![
                    IkSolverInit {
                        ik_bone: BoneIndex(2),
                        target_bone: BoneIndex(1),
                        links: vec![IkLinkInit::new(BoneIndex(0))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                    IkSolverInit {
                        ik_bone: BoneIndex(2),
                        target_bone: BoneIndex(1),
                        links: vec![IkLinkInit::new(BoneIndex(0))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                ],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_eq!(runtime.ik_runtime_stats()[0].solver_evaluations, 1);
        assert_eq!(runtime.ik_runtime_stats()[1].solver_evaluations, 1);
    }

    #[test]
    fn ik_updates_only_affected_eval_suffix_for_late_chain() {
        let unrelated_count = 96usize;
        let chain_root = BoneIndex(unrelated_count as u32);
        let chain_mid = BoneIndex(unrelated_count as u32 + 1);
        let chain_tip = BoneIndex(unrelated_count as u32 + 2);
        let controller = BoneIndex(unrelated_count as u32 + 3);

        let mut bones = Vec::new();
        for i in 0..unrelated_count {
            bones.push(BoneInit::new(None, Vec3A::new(i as f32 * 10.0, -10.0, 0.0)));
        }
        bones.push(BoneInit::new(None, Vec3A::ZERO));
        bones.push(BoneInit::new(Some(chain_root), Vec3A::new(1.0, 0.0, 0.0)));
        bones.push(BoneInit::new(Some(chain_mid), Vec3A::new(1.0, 0.0, 0.0)));
        bones.push(BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)));

        let model = Arc::new(
            ModelArena::new_with_ik(
                bones,
                vec![IkSolverInit {
                    ik_bone: controller,
                    target_bone: chain_tip,
                    links: vec![IkLinkInit::new(chain_mid), IkLinkInit::new(chain_root)],
                    iteration_count: 4,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.reset_world_matrix_bone_update_count();
        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[chain_tip.as_usize()]),
            Vec3A::new(1.0, 1.0, 0.0),
        );
        assert!(
            runtime.world_matrix_bone_update_count() < 360,
            "IK should not recompute unrelated prefix bones repeatedly; updated {} bones",
            runtime.world_matrix_bone_update_count()
        );
    }

    #[test]
    fn clamps_ik_rotation_by_solver_limit_angle() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: std::f32::consts::FRAC_PI_4,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        let expected = Vec3A::new(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        );
        assert_vec3a_near(translation(runtime.world_matrices()[1]), expected);
    }

    #[test]
    fn applies_constant_limit_angle_per_iteration() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(1)), IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: std::f32::consts::FRAC_PI_4,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        // With constant limit_angle = PI/4 (not scaled by link_index), only the root
        // (link 1, bone 0) rotates at most PI/4. The effector bone is skipped.
        // The child bone ends up at (cos(PI/4)*1, sin(PI/4)*1, 0)
        let expected = Vec3A::new(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        );
        assert_vec3a_near(translation(runtime.world_matrices()[1]), expected);
    }

    #[test]
    fn clip_frame_produces_deterministic_world_translations() {
        let model = Arc::new(
            ModelArena::new(vec![
                BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
            ])
            .unwrap(),
        );
        let clip = AnimationClip::new(vec![BoneAnimationBinding {
            bone: BoneIndex(1),
            track: MovableBoneTrack::from_keyframes(vec![
                MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                MovableBoneKeyframe::new(10, Vec3A::new(0.0, 0.0, 4.0), Quat::IDENTITY),
            ]),
        }]);
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_clip_frame(&clip, 5.0);

        let matrices = runtime.world_matrices();
        assert_eq!(matrices.len(), 2);
        assert_vec3a_near(translation(matrices[0]), Vec3A::new(1.0, 0.0, 0.0));
        assert_vec3a_near(translation(matrices[1]), Vec3A::new(1.0, 2.0, 2.0));
    }

    #[test]
    fn evaluate_clip_frame_without_ik_leaves_ik_unsolved() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let clip = AnimationClip::new(vec![]);

        let mut without_ik = RuntimeInstance::new(Arc::clone(&model));
        let mut with_ik = RuntimeInstance::new(model);

        without_ik.evaluate_clip_frame_without_ik(&clip, 0.0);
        with_ik.evaluate_clip_frame(&clip, 0.0);

        // Without IK: effector bone stays at rest position (1, 0, 0)
        assert_vec3a_near(
            translation(without_ik.world_matrices()[1]),
            Vec3A::new(1.0, 0.0, 0.0),
        );
        // With IK: effector bone rotates toward target at (0, 1, 0)
        assert_vec3a_near(
            translation(with_ik.world_matrices()[1]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn ik_options_cap_configured_iterations() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 100,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.reset_ik_runtime_stats();
        runtime.evaluate_current_pose_with_ik_options(super::IkSolveOptions {
            tolerance: 0.0,
            max_iterations_cap: Some(5),
        });

        assert_eq!(runtime.ik_runtime_stats()[0].configured_iterations, 5);
    }

    // ---- morph expansion tests ----

    fn assert_near(actual: f32, expected: f32) {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1.0e-5,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    #[test]
    fn bone_morph_position_offset_drives_world_position() {
        let model = Arc::new(
            ModelArena::new_with_morphs(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
                ],
                Vec::new(),
                Vec::new(),
                crate::MorphInit {
                    morph_count: 1,
                    bone_offsets: vec![crate::BoneMorphOffset {
                        target_bone: BoneIndex(1),
                        position_offset: Vec3A::new(0.0, 0.0, 2.0),
                        rotation_offset: Quat::IDENTITY,
                    }],
                    bone_spans: vec![crate::MorphOffsetSpan { start: 0, count: 1 }],
                    group_offsets: vec![],
                    group_spans: vec![crate::MorphOffsetSpan::default()],
                    ..crate::MorphInit::default()
                },
            )
            .unwrap(),
        );
        let clip = AnimationClip::new_with_morphs(
            Vec::new(),
            vec![crate::MorphAnimationBinding {
                morph: crate::MorphIndex(0),
                track: crate::MorphTrack::from_keyframes(vec![
                    crate::MorphKeyframe::new(0, 0.0),
                    crate::MorphKeyframe::new(10, 1.0),
                ]),
            }],
        );
        let mut runtime = RuntimeInstance::new_with_morph_count(model, 1);

        runtime.evaluate_clip_frame(&clip, 5.0);

        // weight = 0.5: bone offset = (0,0,2) * 0.5 = (0,0,1)
        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(0.0, 1.0, 1.0),
        );
        assert_near(runtime.morph_weights()[0], 0.5);
    }

    #[test]
    fn bone_morph_rotation_offset_affects_child_position() {
        let model = Arc::new(
            ModelArena::new_with_morphs(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
                ],
                Vec::new(),
                Vec::new(),
                crate::MorphInit {
                    morph_count: 1,
                    bone_offsets: vec![crate::BoneMorphOffset {
                        target_bone: BoneIndex(1),
                        position_offset: Vec3A::ZERO,
                        rotation_offset: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    }],
                    bone_spans: vec![crate::MorphOffsetSpan { start: 0, count: 1 }],
                    group_offsets: vec![],
                    group_spans: vec![crate::MorphOffsetSpan::default()],
                    ..crate::MorphInit::default()
                },
            )
            .unwrap(),
        );
        let clip = AnimationClip::new_with_morphs(
            Vec::new(),
            vec![crate::MorphAnimationBinding {
                morph: crate::MorphIndex(0),
                track: crate::MorphTrack::from_keyframes(vec![
                    crate::MorphKeyframe::new(0, 0.0),
                    crate::MorphKeyframe::new(10, 1.0),
                ]),
            }],
        );
        let mut runtime = RuntimeInstance::new_with_morph_count(model, 1);

        runtime.evaluate_clip_frame(&clip, 10.0);

        // weight = 1.0: bone 1 (rest 1,0,0) rotated Z-90 by morph (position unchanged)
        // bone 2 at (1,0,0) relative to bone 1: world = (1,0,0) + (0,1,0)
        assert_vec3a_near(
            translation(runtime.world_matrices()[2]),
            Vec3A::new(1.0, 1.0, 0.0),
        );
    }

    #[test]
    fn group_morph_contributes_to_bone_morph_weight() {
        // PMX order: child (bone morph) has smaller index than parent (group morph)
        // Morph 0 = bone morph, Morph 1 = group morph with MorphIndex(0) as child.
        let model = Arc::new(
            ModelArena::new_with_morphs(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
                ],
                Vec::new(),
                Vec::new(),
                crate::MorphInit {
                    morph_count: 2,
                    bone_offsets: vec![crate::BoneMorphOffset {
                        target_bone: BoneIndex(1),
                        position_offset: Vec3A::new(0.0, 0.0, 2.0),
                        rotation_offset: Quat::IDENTITY,
                    }],
                    bone_spans: vec![
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                        crate::MorphOffsetSpan::default(),
                    ],
                    group_offsets: vec![crate::GroupMorphOffset {
                        child_morph: crate::MorphIndex(0),
                        ratio: 0.5,
                    }],
                    group_spans: vec![
                        crate::MorphOffsetSpan::default(),
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                    ],
                    ..crate::MorphInit::default()
                },
            )
            .unwrap(),
        );
        // VMD track only on group morph (index 1), weight = 1.0
        let clip = AnimationClip::new_with_morphs(
            Vec::new(),
            vec![crate::MorphAnimationBinding {
                morph: crate::MorphIndex(1),
                track: crate::MorphTrack::from_keyframes(vec![
                    crate::MorphKeyframe::new(0, 0.0),
                    crate::MorphKeyframe::new(10, 1.0),
                ]),
            }],
        );
        let mut runtime = RuntimeInstance::new_with_morph_count(model, 2);

        runtime.evaluate_clip_frame(&clip, 10.0);

        // Group expansion: morph_weights[0] += 1.0 * 0.5 = 0.5
        // Bone morph applies: (0,0,2) * 0.5 = (0,0,1)
        assert_near(runtime.morph_weights()[0], 0.5);
        assert_near(runtime.morph_weights()[1], 1.0);
        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(0.0, 1.0, 1.0),
        );
    }

    #[test]
    fn group_morph_can_reference_later_child_morph() {
        let model = Arc::new(
            ModelArena::new_with_morphs(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
                ],
                Vec::new(),
                Vec::new(),
                crate::MorphInit {
                    morph_count: 2,
                    bone_offsets: vec![crate::BoneMorphOffset {
                        target_bone: BoneIndex(1),
                        position_offset: Vec3A::new(0.0, 0.0, 2.0),
                        rotation_offset: Quat::IDENTITY,
                    }],
                    bone_spans: vec![
                        crate::MorphOffsetSpan::default(),
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                    ],
                    group_offsets: vec![crate::GroupMorphOffset {
                        child_morph: crate::MorphIndex(1),
                        ratio: 0.5,
                    }],
                    group_spans: vec![
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                        crate::MorphOffsetSpan::default(),
                    ],
                    ..crate::MorphInit::default()
                },
            )
            .unwrap(),
        );
        let clip = AnimationClip::new_with_morphs(
            Vec::new(),
            vec![crate::MorphAnimationBinding {
                morph: crate::MorphIndex(0),
                track: crate::MorphTrack::from_keyframes(vec![crate::MorphKeyframe::new(0, 1.0)]),
            }],
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_clip_frame(&clip, 0.0);

        assert_near(runtime.morph_weights()[0], 1.0);
        assert_near(runtime.morph_weights()[1], 0.5);
        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(0.0, 1.0, 1.0),
        );
    }

    #[test]
    fn chained_group_morphs_descend_to_bone_morph_weight() {
        let model = Arc::new(
            ModelArena::new_with_morphs(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
                ],
                Vec::new(),
                Vec::new(),
                crate::MorphInit {
                    morph_count: 3,
                    bone_offsets: vec![crate::BoneMorphOffset {
                        target_bone: BoneIndex(1),
                        position_offset: Vec3A::new(0.0, 0.0, 2.0),
                        rotation_offset: Quat::IDENTITY,
                    }],
                    bone_spans: vec![
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                        crate::MorphOffsetSpan::default(),
                        crate::MorphOffsetSpan::default(),
                    ],
                    group_offsets: vec![
                        crate::GroupMorphOffset {
                            child_morph: crate::MorphIndex(0),
                            ratio: 0.25,
                        },
                        crate::GroupMorphOffset {
                            child_morph: crate::MorphIndex(1),
                            ratio: 0.5,
                        },
                    ],
                    group_spans: vec![
                        crate::MorphOffsetSpan::default(),
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                        crate::MorphOffsetSpan { start: 1, count: 1 },
                    ],
                    ..crate::MorphInit::default()
                },
            )
            .unwrap(),
        );
        let clip = AnimationClip::new_with_morphs(
            Vec::new(),
            vec![crate::MorphAnimationBinding {
                morph: crate::MorphIndex(2),
                track: crate::MorphTrack::from_keyframes(vec![crate::MorphKeyframe::new(0, 1.0)]),
            }],
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_clip_frame(&clip, 0.0);

        assert_near(runtime.morph_weights()[2], 1.0);
        assert_near(runtime.morph_weights()[1], 0.5);
        assert_near(runtime.morph_weights()[0], 0.125);
        assert_vec3a_near(
            translation(runtime.world_matrices()[1]),
            Vec3A::new(0.0, 1.0, 0.25),
        );
    }

    #[test]
    fn expand_morphs_noop_when_no_morph_defs() {
        let model = Arc::new(ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap());
        let mut runtime = RuntimeInstance::new_with_morph_count(model, 1);
        runtime
            .pose_mut()
            .set_morph_weight(crate::MorphIndex(0), 1.0);
        runtime.expand_morphs();
        // No crash = pass
        assert_near(runtime.morph_weights()[0], 1.0);
    }

    #[test]
    fn clamps_link_local_rotation_to_angle_limit() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![
                        IkLinkInit::new(BoneIndex(0)).with_angle_limit(IkAngleLimit::new(
                            Vec3A::new(0.0, 0.0, 0.0),
                            Vec3A::new(0.0, 0.0, std::f32::consts::FRAC_PI_4),
                        )),
                    ],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        let expected = Vec3A::new(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        );
        assert_vec3a_near(translation(runtime.world_matrices()[1]), expected);
    }

    #[test]
    fn multi_axis_limited_link_solves_before_clamping() {
        let local_effector = Vec3A::X;
        let local_target = Vec3A::new(0.25, 0.55, 0.80).normalize();
        let limits = IkAngleLimit::new(Vec3A::new(0.0, -1.0, -1.0), Vec3A::new(0.0, 1.0, 1.0));
        let base_rotations = vec![Quat::IDENTITY];
        let mut ik_rotations = vec![Quat::IDENTITY];
        let mut chain_states = vec![super::ChainLinkState {
            previous_euler: [0.0; 3],
            plane_mode_angle: 0.0,
        }];

        super::solve_limited_axes_link_step(super::LimitedAxesLinkStepInput {
            local_effector: &local_effector,
            local_target: &local_target,
            link_index: 0,
            base_rotations: &base_rotations,
            ik_rotations: &mut ik_rotations,
            chain_states: &mut chain_states,
            limits,
            limit_angle: 0.0,
        });

        let current_direction = ik_rotations[0].mul_vec3a(local_effector).normalize();
        let legacy_direction =
            legacy_clamp_only_limited_direction(local_effector, local_target, limits);
        let current_error = (current_direction - local_target).length();
        let legacy_error = (legacy_direction - local_target).length();

        assert!(
            current_error < legacy_error - 0.015,
            "current_error={current_error:.6} legacy_error={legacy_error:.6} current={current_direction:?} legacy={legacy_direction:?} target={local_target:?}"
        );
        assert!(
            chain_states[0].previous_euler[1].abs() > 0.1
                && chain_states[0].previous_euler[2].abs() > 0.1,
            "multi-axis limited IK should use both Y and Z axes; euler={:?}",
            chain_states[0].previous_euler
        );
    }

    #[test]
    fn multi_axis_limited_link_applies_limits_to_total_rotation() {
        let local_effector = Vec3A::new(0.25, 0.45, 0.85).normalize();
        let local_target = Vec3A::new(0.55, 0.15, 0.80).normalize();
        let limits = IkAngleLimit::new(Vec3A::new(-1.0, -1.0, 0.0), Vec3A::new(1.0, 1.0, 0.0));
        let base_rotations = vec![Quat::from_rotation_z(0.45)];
        let mut ik_rotations = vec![Quat::IDENTITY];
        let mut chain_states = vec![super::ChainLinkState {
            previous_euler: [0.0; 3],
            plane_mode_angle: 0.0,
        }];

        super::solve_limited_axes_link_step(super::LimitedAxesLinkStepInput {
            local_effector: &local_effector,
            local_target: &local_target,
            link_index: 0,
            base_rotations: &base_rotations,
            ik_rotations: &mut ik_rotations,
            chain_states: &mut chain_states,
            limits,
            limit_angle: 0.0,
        });

        let base_direction = base_rotations[0].mul_vec3a(local_effector).normalize();
        let effective = (ik_rotations[0] * base_rotations[0]).normalize();
        let stale_direction = limited_direction_without_fixed_axis_working_update(
            local_effector,
            local_target,
            base_rotations[0],
            limits,
        );
        let solved_direction = effective.mul_vec3a(local_effector).normalize();
        assert_near(chain_states[0].previous_euler[2], 0.0);
        assert!(
            (solved_direction - stale_direction).length() > 0.05,
            "fixed axis clamp should affect later axis solve; solved={solved_direction:?} stale={stale_direction:?}"
        );
        assert!(
            (solved_direction - local_target).length() < (base_direction - local_target).length(),
            "non-identity base should still solve toward target; base={base_direction:?} solved={solved_direction:?} target={local_target:?}"
        );
    }

    fn limited_direction_without_fixed_axis_working_update(
        local_effector: Vec3A,
        local_target: Vec3A,
        base: Quat,
        limits: IkAngleLimit,
    ) -> Vec3A {
        let mut total_euler =
            super::decompose_euler_xyz(&super::quat_to_rotation_mat3(base), &[0.0; 3]);
        let mut working_effector = local_effector;
        let target = local_target.normalize();

        for axis_index in [2usize, 1, 0] {
            let (lower, upper) = super::limit_axis_bounds(limits, axis_index);
            if lower == 0.0 && upper == 0.0 {
                total_euler[axis_index] = total_euler[axis_index].clamp(lower, upper);
                continue;
            }

            let axis = super::axis_vec(axis_index);
            let signed_angle = super::signed_projected_angle(working_effector, target, axis);
            if signed_angle.abs() <= 1.0e-6 {
                continue;
            }
            let next = (total_euler[axis_index] + signed_angle).clamp(lower, upper);
            let applied = next - total_euler[axis_index];
            total_euler[axis_index] = next;
            if applied.abs() > 0.0 {
                working_effector =
                    Quat::from_axis_angle(axis.into(), applied).mul_vec3a(working_effector);
            }
        }

        super::euler_xyz_to_quat(&total_euler)
            .normalize()
            .mul_vec3a(local_effector)
            .normalize()
    }

    fn legacy_clamp_only_limited_direction(
        local_effector: Vec3A,
        local_target: Vec3A,
        limits: IkAngleLimit,
    ) -> Vec3A {
        let local_eff_n = local_effector.normalize();
        let local_tgt_n = local_target.normalize();
        let dot = local_eff_n.dot(local_tgt_n).clamp(-1.0, 1.0);
        let angle = dot.acos();
        let axis = local_eff_n.cross(local_tgt_n);
        let axis_vec = if axis.length() < 1e-5 {
            if dot > -1.0 + 1e-5 {
                return local_eff_n;
            }
            let basis = if local_eff_n.x.abs() < 0.9 {
                Vec3A::new(1.0, 0.0, 0.0)
            } else {
                Vec3A::new(0.0, 1.0, 0.0)
            };
            local_eff_n.cross(basis).normalize()
        } else {
            axis.normalize()
        };
        let rotation = Quat::from_axis_angle(axis_vec.into(), angle).normalize();
        let euler = super::decompose_euler_xyz(&super::quat_to_rotation_mat3(rotation), &[0.0; 3]);
        let clamped = [
            euler[0].clamp(limits.min.x, limits.max.x),
            euler[1].clamp(limits.min.y, limits.max.y),
            euler[2].clamp(limits.min.z, limits.max.z),
        ];
        super::euler_xyz_to_quat(&clamped)
            .normalize()
            .mul_vec3a(local_effector)
            .normalize()
    }

    #[test]
    fn plane_link_step_matches_saba_total_axis_rotation() {
        let base = Quat::from_rotation_x(0.3);
        let base_rotations = vec![base];
        let mut ik_rotations = vec![Quat::IDENTITY];
        let mut chain_states = vec![super::ChainLinkState {
            previous_euler: [0.0; 3],
            plane_mode_angle: 0.0,
        }];
        let local_effector = Vec3A::X;
        let local_target = Vec3A::Y;

        super::solve_plane_link_step(super::PlaneLinkStepInput {
            local_effector: &local_effector,
            local_target: &local_target,
            link_index: 0,
            base_rotations: &base_rotations,
            ik_rotations: &mut ik_rotations,
            chain_states: &mut chain_states,
            axis_index: 2,
            limits: IkAngleLimit::new(
                Vec3A::new(-std::f32::consts::PI, 0.0, -std::f32::consts::PI),
                Vec3A::new(std::f32::consts::PI, 0.0, std::f32::consts::PI),
            ),
            iteration: 0,
            limit_angle: 0.0,
        });

        let effective = (ik_rotations[0] * base_rotations[0]).normalize();
        assert_near(
            chain_states[0].plane_mode_angle,
            std::f32::consts::FRAC_PI_2,
        );
        assert_vec3a_near(
            effective.mul_vec3a(Vec3A::X),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).mul_vec3a(Vec3A::X),
        );
        assert_vec3a_near(effective.mul_vec3a(Vec3A::Z), Vec3A::Z);
    }

    #[test]
    fn append_rotation_propagates_post_ik_link_rotation() {
        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(3)), Vec3A::new(1.0, 0.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
                vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[4]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn append_source_with_own_append_includes_ik_link_rotation() {
        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
                vec![
                    AppendTransformInit::new(BoneIndex(0), BoneIndex(3), 1.0).with_rotation(),
                    AppendTransformInit::new(BoneIndex(4), BoneIndex(0), 1.0).with_rotation(),
                ],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.pose_mut().set_local_rotation(
            BoneIndex(3),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
        );

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[5]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn without_ik_evaluation_clears_previous_ik_link_rotation_for_append_sources() {
        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
                vec![
                    AppendTransformInit::new(BoneIndex(0), BoneIndex(3), 1.0).with_rotation(),
                    AppendTransformInit::new(BoneIndex(4), BoneIndex(0), 1.0).with_rotation(),
                ],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.pose_mut().set_local_rotation(
            BoneIndex(3),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
        );

        runtime.evaluate_current_pose();
        runtime.evaluate_current_pose_without_ik();

        assert_vec3a_near(
            translation(runtime.world_matrices()[5]),
            Vec3A::new(
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
            ),
        );
    }

    #[test]
    fn shared_ik_link_preserves_accumulated_rotation_for_later_append_source() {
        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
                ],
                vec![
                    IkSolverInit {
                        ik_bone: BoneIndex(2),
                        target_bone: BoneIndex(1),
                        links: vec![IkLinkInit::new(BoneIndex(0))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                    IkSolverInit {
                        ik_bone: BoneIndex(2),
                        target_bone: BoneIndex(2),
                        links: vec![IkLinkInit::new(BoneIndex(0))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                ],
                vec![
                    AppendTransformInit::new(BoneIndex(0), BoneIndex(3), 1.0).with_rotation(),
                    AppendTransformInit::new(BoneIndex(4), BoneIndex(0), 1.0).with_rotation(),
                ],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.pose_mut().set_local_rotation(
            BoneIndex(3),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
        );

        runtime.evaluate_current_pose();

        assert_eq!(runtime.ik_runtime_stats()[1].tolerance_precheck_breaks, 1);
        assert_vec3a_near(
            translation(runtime.world_matrices()[5]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn earlier_append_target_updates_after_later_ik_link_rotation() {
        let mut append_target = BoneInit::new(None, Vec3A::ZERO);
        append_target.transform_order = 0;
        let mut append_child = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);
        append_child.transform_order = 1;
        let mut link = BoneInit::new(None, Vec3A::ZERO);
        link.transform_order = 10;
        let mut effector = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
        effector.transform_order = 11;
        let mut controller = BoneInit::new(None, Vec3A::Y);
        controller.transform_order = 12;

        let model = Arc::new(
            ModelArena::new_full(
                vec![link, effector, controller, append_target, append_child],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
                vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[4]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn earlier_append_target_preserves_later_ik_link_source_append_rotation() {
        let mut append_target = BoneInit::new(None, Vec3A::ZERO);
        append_target.transform_order = 0;
        let mut append_child = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);
        append_child.transform_order = 1;
        let mut append_driver = BoneInit::new(None, Vec3A::ZERO);
        append_driver.transform_order = 9;
        let mut link = BoneInit::new(None, Vec3A::ZERO);
        link.transform_order = 10;
        let mut effector = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
        effector.transform_order = 11;
        let mut controller = BoneInit::new(None, Vec3A::Y);
        controller.transform_order = 12;

        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    link,
                    effector,
                    controller,
                    append_target,
                    append_child,
                    append_driver,
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
                vec![
                    AppendTransformInit::new(BoneIndex(0), BoneIndex(5), 1.0).with_rotation(),
                    AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation(),
                ],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.pose_mut().set_local_rotation(
            BoneIndex(5),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
        );

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[4]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn transitive_append_target_recomputes_after_opposite_phase_ik_source_rotation() {
        let mut append_a = BoneInit::new(None, Vec3A::ZERO);
        append_a.transform_order = 0;
        let mut append_b = BoneInit::new(None, Vec3A::ZERO);
        append_b.transform_order = 1;
        let mut append_b_child = BoneInit::new(Some(BoneIndex(4)), Vec3A::X);
        append_b_child.transform_order = 2;

        let mut link = BoneInit::new(None, Vec3A::ZERO);
        link.transform_order = 10;
        link.transform_after_physics = true;
        let mut effector = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
        effector.transform_order = 11;
        effector.transform_after_physics = true;
        let mut controller = BoneInit::new(None, Vec3A::Y);
        controller.transform_order = 12;
        controller.transform_after_physics = true;

        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    link,
                    effector,
                    controller,
                    append_a,
                    append_b,
                    append_b_child,
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
                vec![
                    AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation(),
                    AppendTransformInit::new(BoneIndex(4), BoneIndex(3), 1.0).with_rotation(),
                ],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[5]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn mixed_phase_ik_updates_opposite_phase_controller_dependency() {
        let mut link_a = BoneInit::new(None, Vec3A::ZERO);
        link_a.transform_order = 0;
        let mut effector_a = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
        effector_a.transform_order = 1;
        let mut controller_a = BoneInit::new(None, Vec3A::Y);
        controller_a.transform_order = 2;
        let mut after_append = BoneInit::new(None, Vec3A::ZERO);
        after_append.transform_order = 3;
        after_append.transform_after_physics = true;
        let mut link_b = BoneInit::new(None, Vec3A::ZERO);
        link_b.transform_order = 4;
        let mut effector_b = BoneInit::new(Some(BoneIndex(4)), Vec3A::X);
        effector_b.transform_order = 5;
        let mut controller_b = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);
        controller_b.transform_order = 6;

        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    link_a,
                    effector_a,
                    controller_a,
                    after_append,
                    link_b,
                    effector_b,
                    controller_b,
                ],
                vec![
                    IkSolverInit {
                        ik_bone: BoneIndex(2),
                        target_bone: BoneIndex(1),
                        links: vec![IkLinkInit::new(BoneIndex(0))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                    IkSolverInit {
                        ik_bone: BoneIndex(6),
                        target_bone: BoneIndex(5),
                        links: vec![IkLinkInit::new(BoneIndex(4))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                ],
                vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[5]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
        assert_vec3a_near(
            translation(runtime.world_matrices()[6]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
        assert_vec3a_near(
            translation(runtime.world_matrices()[3]),
            Vec3A::new(0.0, 0.0, 0.0),
        );
    }

    #[test]
    fn after_physics_plain_child_recomputes_after_pre_physics_parent_ik() {
        let mut after_child = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
        after_child.transform_order = 3;
        after_child.transform_after_physics = true;

        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                    BoneInit::new(None, Vec3A::Y),
                    after_child,
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[3]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn pre_physics_child_recomputes_after_after_physics_append_parent() {
        let mut after_parent = BoneInit::new(None, Vec3A::ZERO);
        after_parent.transform_order = 1;
        after_parent.transform_after_physics = true;
        let mut pre_child = BoneInit::new(Some(BoneIndex(1)), Vec3A::X);
        pre_child.transform_order = 2;

        let model = Arc::new(
            ModelArena::new_full(
                vec![BoneInit::new(None, Vec3A::ZERO), after_parent, pre_child],
                Vec::new(),
                vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0).with_rotation()],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.pose_mut().set_local_rotation(
            BoneIndex(0),
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        );

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[2]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn append_target_recomputes_after_opposite_phase_ik_source_rotation() {
        let mut after_controller = BoneInit::new(None, Vec3A::Y);
        after_controller.transform_order = 2;
        after_controller.transform_after_physics = true;
        let mut append_target = BoneInit::new(None, Vec3A::ZERO);
        append_target.transform_order = 3;
        let append_child = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);

        let model = Arc::new(
            ModelArena::new_full(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                    after_controller,
                    append_target,
                    append_child,
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
                vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        assert_vec3a_near(
            translation(runtime.world_matrices()[4]),
            Vec3A::new(0.0, 1.0, 0.0),
        );
    }

    #[test]
    fn scratch_ik_capacities_stable_after_repeated_evaluate() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(3)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
                    BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                ],
                vec![
                    IkSolverInit {
                        ik_bone: BoneIndex(2),
                        target_bone: BoneIndex(1),
                        links: vec![IkLinkInit::new(BoneIndex(0))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                    IkSolverInit {
                        ik_bone: BoneIndex(6),
                        target_bone: BoneIndex(5),
                        links: vec![IkLinkInit::new(BoneIndex(3)), IkLinkInit::new(BoneIndex(4))],
                        iteration_count: 1,
                        limit_angle: 0.0,
                    },
                ],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);

        runtime.evaluate_current_pose();

        let cap_links = runtime.ik_scratch.links.capacity();
        let cap_base = runtime.ik_scratch.base_rotations.capacity();
        let cap_base_ik = runtime.ik_scratch.base_ik_rotations.capacity();
        let cap_ik = runtime.ik_scratch.ik_rotations.capacity();
        let cap_best = runtime.ik_scratch.best_ik_rotations.capacity();
        let cap_chain = runtime.ik_scratch.chain_states.capacity();

        for _ in 0..10 {
            runtime.evaluate_current_pose();
        }

        assert_eq!(runtime.ik_scratch.links.capacity(), cap_links);
        assert_eq!(runtime.ik_scratch.base_rotations.capacity(), cap_base);
        assert_eq!(runtime.ik_scratch.base_ik_rotations.capacity(), cap_base_ik);
        assert_eq!(runtime.ik_scratch.ik_rotations.capacity(), cap_ik);
        assert_eq!(runtime.ik_scratch.best_ik_rotations.capacity(), cap_best);
        assert_eq!(runtime.ik_scratch.chain_states.capacity(), cap_chain);
    }

    #[test]
    fn scratch_morph_capacity_stable_after_repeated_clip_frame() {
        let model = Arc::new(
            ModelArena::new_with_morphs(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
                ],
                Vec::new(),
                Vec::new(),
                crate::MorphInit {
                    morph_count: 2,
                    bone_offsets: vec![crate::BoneMorphOffset {
                        target_bone: BoneIndex(1),
                        position_offset: Vec3A::new(0.0, 0.0, 2.0),
                        rotation_offset: Quat::IDENTITY,
                    }],
                    bone_spans: vec![
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                        crate::MorphOffsetSpan::default(),
                    ],
                    group_offsets: vec![crate::GroupMorphOffset {
                        child_morph: crate::MorphIndex(0),
                        ratio: 0.5,
                    }],
                    group_spans: vec![
                        crate::MorphOffsetSpan::default(),
                        crate::MorphOffsetSpan { start: 0, count: 1 },
                    ],
                    ..crate::MorphInit::default()
                },
            )
            .unwrap(),
        );
        let clip = AnimationClip::new_with_morphs(
            Vec::new(),
            vec![crate::MorphAnimationBinding {
                morph: crate::MorphIndex(1),
                track: crate::MorphTrack::from_keyframes(vec![
                    crate::MorphKeyframe::new(0, 0.0),
                    crate::MorphKeyframe::new(10, 1.0),
                ]),
            }],
        );
        let mut runtime = RuntimeInstance::new_with_morph_count(model, 2);

        runtime.evaluate_clip_frame(&clip, 5.0);

        let cap_expanded = runtime.morph_scratch.expanded_weights.capacity();

        for _ in 0..10 {
            runtime.evaluate_clip_frame(&clip, 5.0);
        }

        assert_eq!(
            runtime.morph_scratch.expanded_weights.capacity(),
            cap_expanded
        );
    }
}
