use glam::Quat;

use crate::MorphIndex;

use super::RuntimeInstance;

impl RuntimeInstance {
    /// Expand group morphs and apply bone morph offsets.
    ///
    /// Called automatically from [`Self::evaluate_clip_frame`]. Exposed publicly so
    /// that hosts manually driving [`crate::PoseArena`] can trigger morph expansion
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
    pub fn morph_weights(&self) -> &[f32] {
        self.pose.morph_weights()
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
