use std::sync::Arc;

use glam::{Mat4, Quat};

use crate::{AnimationClip, ModelArena, PoseArena};
use crate::{
    append_primitive::{AppendPrimitiveInput, solve_append_transform},
    ik_primitive::{
        ChainLinkState, LinkStepInput, constrain_rotation_to_axis, rotation, solve_link_step,
        translation,
    },
};

mod morph;

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

#[cfg(test)]
mod tests;
