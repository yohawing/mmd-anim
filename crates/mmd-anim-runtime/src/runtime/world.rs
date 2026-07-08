use glam::Mat4;

use crate::{
    append_primitive::{AppendPrimitiveInput, solve_append_transform},
    ik_primitive::constrain_rotation_to_axis,
};

#[cfg(test)]
use super::WorldMatrixBoneUpdateCategory;

use super::RuntimeInstance;

impl RuntimeInstance {
    pub(super) fn update_world_matrices(&mut self) {
        self.update_world_matrices_from_eval_order_position(0);
    }

    pub(super) fn update_world_matrices_from_eval_order_position(&mut self, start_position: usize) {
        self.update_world_matrices_from_eval_order_position_for_phase(start_position, None);
    }

    pub(super) fn update_world_matrices_from_eval_order_position_for_phase(
        &mut self,
        start_position: usize,
        phase: Option<bool>,
    ) {
        let start_position =
            self.expand_update_start_for_append_dependencies(start_position, phase);
        match phase {
            None => {
                for bone in &self.model.eval_order()[start_position..] {
                    self.pose.reset_append_transform(*bone);
                }
                for position in start_position..self.model.eval_order().len() {
                    let bone = self.model.eval_order()[position];
                    self.update_append_transform_for_bone(bone);
                    self.update_world_matrix_for_bone(bone);
                }
            }
            Some(after_physics) => {
                let phase_bone_count = self.model.eval_order_for_phase(after_physics).len();
                for phase_index in 0..phase_bone_count {
                    let bone = self.model.eval_order_for_phase(after_physics)[phase_index];
                    if self.model.eval_order_position(bone) < start_position {
                        continue;
                    }
                    self.pose.reset_append_transform(bone);
                }
                for phase_index in 0..phase_bone_count {
                    let bone = self.model.eval_order_for_phase(after_physics)[phase_index];
                    if self.model.eval_order_position(bone) < start_position {
                        continue;
                    }
                    self.update_append_transform_for_bone(bone);
                    self.update_world_matrix_for_bone(bone);
                }
            }
        }
    }

    pub(super) fn update_world_matrices_using_current_append_from_eval_order_position(
        &mut self,
        start_position: usize,
    ) {
        self.update_world_matrices_using_current_append_from_eval_order_position_for_phase(
            start_position,
            None,
        );
    }

    pub(super) fn update_world_matrices_using_current_append_from_eval_order_position_for_phase(
        &mut self,
        start_position: usize,
        phase: Option<bool>,
    ) {
        match phase {
            None => {
                for position in start_position..self.model.eval_order().len() {
                    let bone = self.model.eval_order()[position];
                    self.update_world_matrix_for_bone(bone);
                }
            }
            Some(after_physics) => {
                let phase_bone_count = self.model.eval_order_for_phase(after_physics).len();
                for phase_index in 0..phase_bone_count {
                    let bone = self.model.eval_order_for_phase(after_physics)[phase_index];
                    if self.model.eval_order_position(bone) < start_position {
                        continue;
                    }
                    self.update_world_matrix_for_bone(bone);
                }
            }
        }
    }

    #[inline]
    pub(super) fn bone_matches_phase(&self, bone: crate::BoneIndex, phase: Option<bool>) -> bool {
        phase.is_none_or(|after_physics| self.model.transform_after_physics(bone) == after_physics)
    }

    pub(super) fn update_append_transform_for_bone(&mut self, bone: crate::BoneIndex) {
        let mut visiting = Vec::new();
        self.update_append_transform_for_bone_inner(bone, &mut visiting);
    }

    fn update_append_transform_for_bone_inner(
        &mut self,
        bone: crate::BoneIndex,
        visiting: &mut Vec<crate::BoneIndex>,
    ) {
        let Some(append_index) = self.model.append_transform_index(bone) else {
            return;
        };
        if visiting.contains(&bone) {
            return;
        }
        visiting.push(bone);
        let append = self.model.append_transform(append_index);
        let source_bone = append.source_bone;
        let ratio = append.ratio;
        let affect_rotation = append.affect_rotation;
        let affect_translation = append.affect_translation;
        let use_source_append =
            !append.local && self.model.append_transform_index(source_bone).is_some();
        if use_source_append {
            self.update_append_transform_for_bone_inner(source_bone, visiting);
        }
        let mut source_rotation = if use_source_append {
            self.pose.append_rotation(source_bone)
        } else {
            self.pose.local_rotation(source_bone)
        };
        if use_source_append && self.model.is_ik_link_bone(source_bone) {
            source_rotation = (self.pose.ik_rotation(source_bone) * source_rotation).normalize();
        }
        let source_position_offset = if use_source_append {
            self.pose.append_position_offset(source_bone)
        } else {
            self.pose.local_position_offset(source_bone)
        };
        let append_output = solve_append_transform(AppendPrimitiveInput {
            source_position_offset,
            source_rotation,
            ratio,
            affect_rotation,
            affect_translation,
        });
        self.pose.set_append_rotation(bone, append_output.rotation);
        self.pose
            .set_append_position_offset(bone, append_output.position_offset);
        visiting.pop();
    }

    pub(super) fn update_world_matrix_for_bone(&mut self, bone: crate::BoneIndex) {
        #[cfg(test)]
        {
            self.world_matrix_bone_update_count += 1;
            match self.world_matrix_bone_update_category {
                WorldMatrixBoneUpdateCategory::LeadingBookend => {
                    self.world_matrix_bone_update_leading_bookend_count += 1;
                }
                WorldMatrixBoneUpdateCategory::PhaseLoop => {
                    self.world_matrix_bone_update_phase_loop_count += 1;
                }
                WorldMatrixBoneUpdateCategory::TrailingBookend => {
                    self.world_matrix_bone_update_trailing_bookend_count += 1;
                }
                WorldMatrixBoneUpdateCategory::IkLinkChange => {
                    self.world_matrix_bone_update_ik_link_change_count += 1;
                }
                WorldMatrixBoneUpdateCategory::Other => {
                    self.world_matrix_bone_update_other_count += 1;
                }
            }
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

    pub(super) fn expand_update_start_for_append_dependencies(
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

    #[inline]
    pub fn world_matrices(&self) -> &[Mat4] {
        self.pose.world_matrices()
    }

    #[cfg(test)]
    pub(super) fn set_world_matrix_bone_update_category(
        &mut self,
        category: WorldMatrixBoneUpdateCategory,
    ) {
        self.world_matrix_bone_update_category = category;
    }

    #[cfg(test)]
    pub(super) fn reset_world_matrix_bone_update_count(&mut self) {
        self.world_matrix_bone_update_count = 0;
        self.world_matrix_bone_update_leading_bookend_count = 0;
        self.world_matrix_bone_update_phase_loop_count = 0;
        self.world_matrix_bone_update_trailing_bookend_count = 0;
        self.world_matrix_bone_update_ik_link_change_count = 0;
        self.world_matrix_bone_update_other_count = 0;
        self.world_matrix_bone_update_category = WorldMatrixBoneUpdateCategory::Other;
    }

    #[cfg(test)]
    pub(super) fn world_matrix_bone_update_count(&self) -> usize {
        self.world_matrix_bone_update_count
    }

    #[cfg(test)]
    pub(super) fn world_matrix_bone_update_leading_bookend_count(&self) -> usize {
        self.world_matrix_bone_update_leading_bookend_count
    }

    #[cfg(test)]
    pub(super) fn world_matrix_bone_update_phase_loop_count(&self) -> usize {
        self.world_matrix_bone_update_phase_loop_count
    }

    #[cfg(test)]
    pub(super) fn world_matrix_bone_update_trailing_bookend_count(&self) -> usize {
        self.world_matrix_bone_update_trailing_bookend_count
    }

    #[cfg(test)]
    pub(super) fn world_matrix_bone_update_ik_link_change_count(&self) -> usize {
        self.world_matrix_bone_update_ik_link_change_count
    }

    #[cfg(test)]
    pub(super) fn world_matrix_bone_update_other_count(&self) -> usize {
        self.world_matrix_bone_update_other_count
    }

    #[inline]
    pub fn skinning_matrices(&self) -> &[Mat4] {
        self.pose.skinning_matrices()
    }
}
