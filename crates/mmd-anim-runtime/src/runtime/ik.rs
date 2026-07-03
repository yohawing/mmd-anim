use glam::Quat;

use crate::ik_primitive::{ChainLinkState, LinkStepInput, rotation, solve_link_step, translation};

use super::{IkSolveOptions, RuntimeInstance};

impl RuntimeInstance {
    pub(super) fn solve_ik_solver(
        &mut self,
        ik_index: usize,
        options: IkSolveOptions,
        after_physics: bool,
    ) {
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
}
