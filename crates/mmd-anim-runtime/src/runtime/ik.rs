use glam::Quat;

use crate::ik_primitive::{
    ChainLinkState, LinkStepInput, effective_iteration_count, rotation, solve_link_step,
    translation,
};

use super::{IkSolveOptions, RuntimeInstance};

impl RuntimeInstance {
    pub(super) fn evaluation_layers(model: &crate::ModelArena) -> Vec<i32> {
        let mut layers = vec![0; model.bone_count()];
        // Keep the runtime's parent-before-child guarantee when authored layers
        // are reversed. PMX also inherits the parent's layer during loading.
        for &bone in model.eval_order() {
            layers[bone.as_usize()] = model
                .parent_index(bone)
                .map_or(model.transform_order(bone), |parent| {
                    model.transform_order(bone).max(layers[parent.as_usize()])
                });
        }
        layers
    }

    pub(super) fn initial_ik_target_history(
        model: &crate::ModelArena,
    ) -> Vec<(crate::BoneIndex, glam::Mat4)> {
        let mut rest = vec![glam::Vec3A::ZERO; model.bone_count()];
        for &bone in model.eval_order() {
            rest[bone.as_usize()] = model.rest_position(bone)
                + model
                    .parent_index(bone)
                    .map_or(glam::Vec3A::ZERO, |p| rest[p.as_usize()]);
        }
        let mut history = Vec::new();
        let layers = Self::evaluation_layers(model);
        for solver in model.ik_solvers() {
            let bone = solver.target_bone;
            if layers[bone.as_usize()] > layers[solver.ik_bone.as_usize()]
                && !history.iter().any(|(existing, _)| *existing == bone)
            {
                history.push((
                    bone,
                    glam::Mat4::from_translation(rest[bone.as_usize()].into()),
                ));
            }
        }
        history
    }

    pub(super) fn initial_ik_history(model: &crate::ModelArena) -> Vec<Option<Quat>> {
        let (under_ik, dependent) = Self::ik_dependency_roles(model);
        dependent
            .into_iter()
            .zip(under_ik)
            .map(|(dependent, under_ik)| (dependent && under_ik).then_some(Quat::IDENTITY))
            .collect()
    }

    pub(super) fn ik_deferred_append_bones(model: &crate::ModelArena) -> Vec<bool> {
        let (under_ik, mut dependent) = Self::ik_dependency_roles(model);
        for (i, value) in dependent.iter_mut().enumerate() {
            *value &= !under_ik[i]
                && model
                    .ik_solvers()
                    .iter()
                    .any(|s| s.target_bone.as_usize() == i);
        }
        dependent
    }

    fn ik_dependency_roles(model: &crate::ModelArena) -> (Vec<bool>, Vec<bool>) {
        let mut under_ik = vec![false; model.bone_count()];
        for solver in model.ik_solvers() {
            under_ik[solver.ik_bone.as_usize()] = false;
            under_ik[solver.target_bone.as_usize()] = false;
            for link in &solver.links {
                under_ik[link.bone.as_usize()] = true;
            }
        }
        // MMD performs this inheritance scan in authored bone order, even when
        // the hierarchy contains a forward parent reference. The append weight
        // does not affect the reset path: a zero-weight grant still selects it.
        let mut dependent = vec![false; under_ik.len()];
        for i in 0..dependent.len() {
            let bone = crate::BoneIndex(i as u32);
            dependent[i] = model
                .parent_index(bone)
                .is_some_and(|p| dependent[p.as_usize()])
                || model.append_transform_index(bone).is_some_and(|index| {
                    let append = model.append_transform(index);
                    (append.affect_rotation || append.affect_translation)
                        && under_ik[append.source_bone.as_usize()]
                });
        }
        (under_ik, dependent)
    }

    pub(super) fn solve_ik_solver(
        &mut self,
        ik_index: usize,
        options: IkSolveOptions,
        _after_physics: bool,
    ) {
        if self.pose.ik_enabled()[ik_index] == 0 {
            return;
        }

        let tolerance = options.tolerance.max(0.0);
        let mut links = std::mem::take(&mut self.ik_scratch.links);
        let mut base_rotations = std::mem::take(&mut self.ik_scratch.base_rotations);
        let mut base_ik_rotations = std::mem::take(&mut self.ik_scratch.base_ik_rotations);
        let mut ik_rotations = std::mem::take(&mut self.ik_scratch.ik_rotations);
        let mut chain_states = std::mem::take(&mut self.ik_scratch.chain_states);

        {
            let solver = &self.model.ik_solvers()[ik_index];
            let ik_bone = solver.ik_bone;
            let target_bone = solver.target_bone;
            let iteration_count = effective_iteration_count(
                solver.iteration_count,
                solver.limit_angle,
                options.max_iterations_cap,
            ) as usize;
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
            chain_states.clear();
            chain_states.resize_with(link_count, || ChainLinkState {
                previous_euler: [0.0; 3],
            });

            self.apply_ik_link_rotations(
                &links,
                &base_rotations,
                &base_ik_rotations,
                &ik_rotations,
            );
            // CcdIk rebuilds the target's local matrix, but reads its existing
            // world matrix for the first step. Refresh only after a link moves.

            let mut broke_early = false;
            let mut final_distance = {
                let eff = translation(self.pose.world_matrices()[target_bone.as_usize()]);
                let ik = translation(self.pose.world_matrices()[ik_bone.as_usize()]);
                (eff - ik).length()
            };
            'iterations: for _iteration in 0..iteration_count {
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

                    let link_world_rot = rotation(link_world);
                    let mut local_effector = link_world_rot.inverse().mul_vec3a(eff_pos - link_pos);
                    let mut local_target = link_world_rot.inverse().mul_vec3a(ik_pos - link_pos);

                    if local_effector.length_squared() <= f32::EPSILON
                        || local_target.length_squared() <= f32::EPSILON
                    {
                        continue;
                    }

                    if (local_effector.normalize() - local_target.normalize()).length_squared()
                        < 1e-7
                    {
                        final_distance = (eff_pos - ik_pos).length();
                        broke_early = true;
                        break 'iterations;
                    }

                    // MMD resets the link's world pose but keeps this accumulator
                    // on the append-dependent path. Seed it only after measuring
                    // the first step from that reset pose. Express the directions
                    // in the seeded frame so the shared step still computes the
                    // same parent-frame axis and clamps the accumulated rotation.
                    if _iteration == 0 {
                        if let Some(previous) = self.ik_history[link_bone.as_usize()] {
                            ik_rotations[link_index] = previous;
                            let base = base_rotations[link_index];
                            let adjustment = (previous * base).inverse() * base;
                            local_effector = adjustment.mul_vec3a(local_effector);
                            local_target = adjustment.mul_vec3a(local_target);
                        }
                    }
                    // Fixed-axis constrains the CCD rotation axis only.
                    let fixed_axis = self.model.fixed_axis(link_bone);
                    solve_link_step(LinkStepInput {
                        local_effector: &local_effector,
                        local_target: &local_target,
                        link_index,
                        base_rotations: &base_rotations,
                        ik_rotations: &mut ik_rotations,
                        chain_states: &mut chain_states,
                        angle_limit: link.angle_limit,
                        iteration: _iteration,
                        iteration_count,
                        limit_angle,
                        // PMX local axes describe manipulation handles. MMD applies
                        // IK limits in the authored bone frame, independent of them.
                        local_axis_basis: None,
                        fixed_axis,
                    });

                    if self.ik_history[link_bone.as_usize()].is_some() {
                        self.ik_history[link_bone.as_usize()] = Some(
                            (ik_rotations[link_index] * base_rotations[link_index]).normalize(),
                        );
                    }
                    self.apply_ik_link_rotations(
                        &links,
                        &base_rotations,
                        &base_ik_rotations,
                        &ik_rotations,
                    );
                    self.update_world_matrices_after_ik_link_change(ik_index);
                    self.ik_stats[ik_index].link_steps += 1;
                }

                let current_distance = {
                    let eff = translation(self.pose.world_matrices()[target_bone.as_usize()]);
                    let ik = translation(self.pose.world_matrices()[ik_bone.as_usize()]);
                    (eff - ik).length()
                };
                final_distance = current_distance;

                if current_distance <= tolerance {
                    self.ik_stats[ik_index].tolerance_post_iteration_breaks += 1;
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

            self.apply_ik_link_rotations(
                &links,
                &base_rotations,
                &base_ik_rotations,
                &ik_rotations,
            );
            self.update_world_matrices_after_ik_link_change(ik_index);
        }

        self.ik_scratch.links = links;
        self.ik_scratch.base_rotations = base_rotations;
        self.ik_scratch.base_ik_rotations = base_ik_rotations;
        self.ik_scratch.ik_rotations = ik_rotations;
        self.ik_scratch.chain_states = chain_states;
    }

    fn update_world_matrices_after_ik_link_change(&mut self, ik_index: usize) {
        if self.ik_link_change_update_bones[ik_index].is_none() {
            self.ik_link_change_update_bones[ik_index] =
                Some(self.compute_ik_link_change_update_bones(ik_index));
        }
        let update_bone_count = self.ik_link_change_update_bones[ik_index]
            .as_ref()
            .map_or(0, Vec::len);
        #[cfg(test)]
        let previous_category = self.world_matrix_bone_update_category;
        #[cfg(test)]
        self.set_world_matrix_bone_update_category(
            super::WorldMatrixBoneUpdateCategory::IkLinkChange,
        );
        for update_index in 0..update_bone_count {
            let bone = self.ik_link_change_update_bones[ik_index]
                .as_ref()
                .expect("ik link-change update bones are computed lazily")[update_index];
            self.update_append_transform_for_bone(bone);
            self.update_world_matrix_for_bone(bone);
        }
        #[cfg(test)]
        self.set_world_matrix_bone_update_category(previous_category);
    }

    fn compute_ik_link_change_update_bones(&self, ik_index: usize) -> Vec<crate::BoneIndex> {
        let solver = &self.model.ik_solvers()[ik_index];
        let links = &solver.links;
        let ik_bone = solver.ik_bone;
        let target_bone = solver.target_bone;

        let start_position =
            self.min_ik_dependency_eval_order_position(links, ik_bone, target_bone);
        let start_position = self.expand_update_start_for_append_dependencies(start_position, None);
        let append_dependency_roots =
            self.ik_update_scope_append_dependency_roots(links, ik_bone, target_bone);

        let mut update_bones = Vec::new();
        for position in start_position..self.model.eval_order().len() {
            let bone = self.model.eval_order()[position];
            // Another chain's effector remains at its layer's last world pass.
            // Refreshing it here changes the next solver's initial direction.
            if bone != target_bone
                && !links.iter().any(|link| link.bone == bone)
                && self
                    .model
                    .ik_solvers()
                    .iter()
                    .any(|other| other.target_bone == bone)
            {
                continue;
            }
            if self.bone_is_in_ik_update_scope(bone, links, ik_bone, target_bone)
                || self.bone_depends_on_ik_update_scope_append_roots(bone, &append_dependency_roots)
            {
                update_bones.push(bone);
            }
        }
        update_bones
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
        if self.bone_is_ancestor_of(target_bone, bone) {
            return true;
        }
        links.iter().any(|link| {
            self.bone_is_ancestor_of(bone, link.bone) || self.bone_is_ancestor_of(link.bone, bone)
        })
    }

    fn ik_update_scope_append_dependency_roots(
        &self,
        links: &[crate::IkLink],
        ik_bone: crate::BoneIndex,
        target_bone: crate::BoneIndex,
    ) -> Vec<crate::BoneIndex> {
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
    }

    fn bone_depends_on_ik_update_scope_append_roots(
        &self,
        bone: crate::BoneIndex,
        append_dependency_roots: &[crate::BoneIndex],
    ) -> bool {
        append_dependency_roots
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
}
