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
    /// expansion follows the graph in the same depth-first order as the former
    /// recursive implementation, using a reusable heap scratch stack.  The
    /// model has already rejected cycles, so the stack depth is bounded by the
    /// morph count.
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

        let expanded_weights = &mut self.morph_scratch.expanded_weights;
        let group_stack = &mut self.morph_scratch.group_stack;
        group_stack.clear();
        for (morph_idx, &w) in self.pose.morph_weights()[..mc].iter().enumerate() {
            if w == 0.0 {
                continue;
            }
            group_stack.push(super::GroupMorphFrame {
                morph_idx,
                weight: w,
                next_offset: 0,
            });
            while let Some(frame) = group_stack.last_mut() {
                let span = spans[frame.morph_idx];
                if frame.next_offset >= span.count {
                    group_stack.pop();
                    continue;
                }

                let offset_index = span.start as usize + frame.next_offset as usize;
                frame.next_offset += 1;
                let offset = offsets[offset_index];
                let child = offset.child_morph.as_usize();
                let contribution = frame.weight * offset.ratio;
                // Keep this addition in the exact order used by the
                // recursive implementation; callers may depend on f32
                // rounding for overlapping group paths.
                expanded_weights[child] += contribution;
                if spans[child].count > 0 {
                    group_stack.push(super::GroupMorphFrame {
                        morph_idx: child,
                        weight: contribution,
                        next_offset: 0,
                    });
                }
            }
        }
        for (i, &w) in expanded_weights.iter().enumerate() {
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
