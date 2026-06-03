use std::sync::Arc;

use glam::{Mat4, Quat, Vec3A};

use crate::{AnimationClip, ModelArena, MorphIndex, PoseArena};

#[derive(Debug)]
struct IkScratch {
    links: Vec<crate::IkLink>,
    base_rotations: Vec<Quat>,
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

#[derive(Debug)]
pub struct RuntimeInstance {
    model: Arc<ModelArena>,
    pose: PoseArena,
    ik_scratch: IkScratch,
    morph_scratch: MorphScratch,
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
        Self {
            model,
            pose,
            ik_scratch,
            morph_scratch,
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
        self.update_world_matrices();
        self.solve_enabled_ik();
    }

    /// Evaluate the current pose by updating world matrices only, without
    /// running any IK solver. This is useful for diagnostics that need to
    /// inspect clip/VMD state before IK is applied.
    pub fn evaluate_current_pose_without_ik(&mut self) {
        self.update_world_matrices();
    }

    fn update_world_matrices(&mut self) {
        self.pose.reset_append_transforms();
        for bone in self.model.eval_order() {
            let mut local_position =
                self.model.rest_position(*bone) + self.pose.local_position_offset(*bone);
            let mut local_rotation = self.pose.local_rotation(*bone);
            let local_scale = self.pose.local_scale(*bone);

            if let Some(append_index) = self.model.append_transform_index(*bone) {
                let append = self.model.append_transform(append_index);
                if append.affect_rotation {
                    let source_rotation = if !append.local
                        && self
                            .model
                            .append_transform_index(append.source_bone)
                            .is_some()
                    {
                        self.pose.append_rotation(append.source_bone)
                    } else {
                        self.pose.local_rotation(append.source_bone)
                    };
                    let append_rotation = Quat::IDENTITY
                        .slerp(source_rotation, append.ratio)
                        .normalize();
                    self.pose.set_append_rotation(*bone, append_rotation);
                    local_rotation = (local_rotation * append_rotation).normalize();
                }

                if append.affect_translation {
                    let source_position_offset = if !append.local
                        && self
                            .model
                            .append_transform_index(append.source_bone)
                            .is_some()
                    {
                        self.pose.append_position_offset(append.source_bone)
                    } else {
                        self.pose.local_position_offset(append.source_bone)
                    };
                    let append_position_offset = source_position_offset * append.ratio;
                    self.pose
                        .set_append_position_offset(*bone, append_position_offset);
                    local_position += append_position_offset;
                }
            }

            if let Some(axis) = self.model.fixed_axis(*bone) {
                local_rotation = constrain_rotation_to_axis(local_rotation, axis);
            }

            let local_matrix = Mat4::from_scale_rotation_translation(
                local_scale.into(),
                local_rotation,
                local_position.into(),
            );

            let world_matrix = match self.model.parent_index(*bone) {
                Some(parent) => self.pose.world_matrices()[parent.as_usize()] * local_matrix,
                None => local_matrix,
            };

            self.pose.set_world_matrix(*bone, world_matrix);
            self.pose
                .set_skinning_matrix(*bone, world_matrix * self.model.inverse_bind_matrix(*bone));
        }
    }

    fn solve_enabled_ik(&mut self) {
        const DEFAULT_TOLERANCE: f32 = 1e-4;

        let mut links = std::mem::take(&mut self.ik_scratch.links);
        let mut base_rotations = std::mem::take(&mut self.ik_scratch.base_rotations);
        let mut ik_rotations = std::mem::take(&mut self.ik_scratch.ik_rotations);
        let mut best_ik_rotations = std::mem::take(&mut self.ik_scratch.best_ik_rotations);
        let mut chain_states = std::mem::take(&mut self.ik_scratch.chain_states);

        for ik_index in 0..self.model.ik_count() {
            if self.pose.ik_enabled()[ik_index] == 0 {
                continue;
            }

            let solver = &self.model.ik_solvers()[ik_index];
            let ik_bone = solver.ik_bone;
            let target_bone = solver.target_bone;
            let iteration_count = solver.iteration_count.max(1) as usize;
            let limit_angle = solver.limit_angle.max(0.0);
            let link_count = solver.links.len();

            links.clear();
            links.extend(solver.links.iter().cloned());

            base_rotations.clear();
            base_rotations.extend(links.iter().map(|l| self.pose.local_rotation(l.bone)));
            ik_rotations.clear();
            ik_rotations.resize(link_count, Quat::IDENTITY);
            best_ik_rotations.clear();
            best_ik_rotations.resize(link_count, Quat::IDENTITY);
            chain_states.clear();
            chain_states.resize_with(link_count, || ChainLinkState {
                previous_euler: [0.0; 3],
                plane_mode_angle: 0.0,
            });

            let mut best_distance = f32::MAX;

            // Initial world matrix state
            self.apply_ik_link_rotations(&links, &base_rotations, &ik_rotations);
            self.update_world_matrices();

            for _iteration in 0..iteration_count {
                // Tolerance early exit
                let eff_pos = translation(self.pose.world_matrices()[target_bone.as_usize()]);
                let ik_pos = translation(self.pose.world_matrices()[ik_bone.as_usize()]);
                if (eff_pos - ik_pos).length() <= DEFAULT_TOLERANCE {
                    break;
                }

                for link_index in 0..link_count {
                    let link = &links[link_index];

                    if link.bone == target_bone {
                        continue;
                    }

                    let link_world = self.pose.world_matrices()[link.bone.as_usize()];
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

                    let single_axis = get_single_axis_limit(link.angle_limit);

                    if let (Some(angle_limit), Some(axis_index)) = (link.angle_limit, single_axis) {
                        solve_plane_link_step(PlaneLinkStepInput {
                            local_effector: &local_effector,
                            local_target: &local_target,
                            link_index,
                            base_rotations: &base_rotations,
                            ik_rotations: &mut ik_rotations,
                            chain_states: &mut chain_states,
                            axis_index,
                            limits: angle_limit,
                            iteration: _iteration,
                            limit_angle,
                        });
                    } else {
                        let local_eff_n = local_effector.normalize();
                        let local_tgt_n = local_target.normalize();
                        let dot = local_eff_n.dot(local_tgt_n).clamp(-1.0, 1.0);
                        let mut angle = dot.acos();

                        let tiny_angle = 1e-3 * std::f32::consts::PI / 180.0;
                        if angle < tiny_angle {
                            continue;
                        }

                        if limit_angle > 0.0 {
                            angle = angle.min(limit_angle);
                        }

                        let axis = local_eff_n.cross(local_tgt_n);
                        let axis_vec = if axis.length() < 1e-5 {
                            if dot > -1.0 + 1e-5 {
                                continue;
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

                        let delta = Quat::from_axis_angle(axis_vec.into(), angle);
                        let base = base_rotations[link_index];
                        let ik = ik_rotations[link_index];
                        let mut chain_rotation = (ik * base * delta).normalize();

                        if let Some(angle_limit) = link.angle_limit {
                            chain_rotation = clamp_limited_rotation(
                                chain_rotation,
                                angle_limit,
                                &mut chain_states[link_index],
                                limit_angle,
                            );
                        }

                        ik_rotations[link_index] = (chain_rotation * base.inverse()).normalize();
                    }

                    self.apply_ik_link_rotations(&links, &base_rotations, &ik_rotations);
                    self.update_world_matrices();
                }

                // Best rotations tracking
                let current_distance = {
                    let eff = translation(self.pose.world_matrices()[target_bone.as_usize()]);
                    let ik = translation(self.pose.world_matrices()[ik_bone.as_usize()]);
                    (eff - ik).length()
                };

                if current_distance < best_distance {
                    best_distance = current_distance;
                    best_ik_rotations.copy_from_slice(&ik_rotations);
                    if current_distance <= DEFAULT_TOLERANCE {
                        break;
                    }
                } else {
                    ik_rotations.copy_from_slice(&best_ik_rotations);
                    self.apply_ik_link_rotations(&links, &base_rotations, &ik_rotations);
                    self.update_world_matrices();
                    break;
                }
            }

            // Apply final best effective rotations
            self.apply_ik_link_rotations(&links, &base_rotations, &best_ik_rotations);
            self.update_world_matrices();
        }

        self.ik_scratch.links = links;
        self.ik_scratch.base_rotations = base_rotations;
        self.ik_scratch.ik_rotations = ik_rotations;
        self.ik_scratch.best_ik_rotations = best_ik_rotations;
        self.ik_scratch.chain_states = chain_states;
    }

    fn apply_ik_link_rotations(
        &mut self,
        links: &[crate::IkLink],
        base_rotations: &[Quat],
        ik_rotations: &[Quat],
    ) {
        for (i, link) in links.iter().enumerate() {
            let effective = (ik_rotations[i] * base_rotations[i]).normalize();
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

    /// Evaluate a clip frame but stop before solving IK. Applies the clip to
    /// the pose, expands morphs, and updates world matrices - the same setup
    /// as [`Self::evaluate_clip_frame`] but without calling `solve_enabled_ik`.
    /// Useful for diagnostics that need to inspect pre-IK runtime state.
    pub fn evaluate_clip_frame_without_ik(&mut self, clip: &AnimationClip, frame: f32) {
        clip.apply_to_pose(frame, &mut self.pose);
        self.expand_morphs();
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

    #[inline]
    pub fn skinning_matrices(&self) -> &[Mat4] {
        self.pose.skinning_matrices()
    }

    #[inline]
    pub fn morph_weights(&self) -> &[f32] {
        self.pose.morph_weights()
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

fn translation(matrix: Mat4) -> Vec3A {
    Vec3A::from_vec4(matrix.w_axis)
}

fn rotation(matrix: Mat4) -> Quat {
    matrix.to_scale_rotation_translation().1
}

fn constrain_rotation_to_axis(rotation: Quat, axis: Vec3A) -> Quat {
    let axis = axis.normalize();
    let vector = Vec3A::new(rotation.x, rotation.y, rotation.z);
    let projected = axis * vector.dot(axis);
    let twist = Quat::from_xyzw(projected.x, projected.y, projected.z, rotation.w);
    if twist.length_squared() <= f32::EPSILON {
        Quat::IDENTITY
    } else {
        twist.normalize()
    }
}

#[derive(Debug)]
struct ChainLinkState {
    previous_euler: [f32; 3],
    plane_mode_angle: f32,
}

fn get_single_axis_limit(limit: Option<crate::IkAngleLimit>) -> Option<usize> {
    let limit = limit?;
    let has = [
        limit.min.x != 0.0 || limit.max.x != 0.0,
        limit.min.y != 0.0 || limit.max.y != 0.0,
        limit.min.z != 0.0 || limit.max.z != 0.0,
    ];
    if has[0]
        && limit.min.y == 0.0
        && limit.max.y == 0.0
        && limit.min.z == 0.0
        && limit.max.z == 0.0
    {
        return Some(0);
    }
    if has[1]
        && limit.min.x == 0.0
        && limit.max.x == 0.0
        && limit.min.z == 0.0
        && limit.max.z == 0.0
    {
        return Some(1);
    }
    if has[2]
        && limit.min.x == 0.0
        && limit.max.x == 0.0
        && limit.min.y == 0.0
        && limit.max.y == 0.0
    {
        return Some(2);
    }
    None
}

fn clamp_limited_rotation(
    rotation: Quat,
    limits: crate::IkAngleLimit,
    state: &mut ChainLinkState,
    limit_angle: f32,
) -> Quat {
    let mat = quat_to_rotation_mat3(rotation);
    let euler = decompose_euler_xyz(&mat, &state.previous_euler);
    let clamped: [f32; 3] = [
        euler[0].clamp(limits.min.x, limits.max.x),
        euler[1].clamp(limits.min.y, limits.max.y),
        euler[2].clamp(limits.min.z, limits.max.z),
    ];
    let mut limited_step: [f32; 3] = [0.0; 3];
    for i in 0..3 {
        let delta = clamped[i] - state.previous_euler[i];
        limited_step[i] = if limit_angle > 0.0 {
            delta.clamp(-limit_angle, limit_angle) + state.previous_euler[i]
        } else {
            clamped[i]
        };
    }
    state.previous_euler = limited_step;
    euler_xyz_to_quat(&limited_step)
}

fn quat_to_rotation_mat3(rotation: Quat) -> [f32; 9] {
    let [x, y, z, w] = rotation.normalize().to_array();
    let x2 = x + x;
    let y2 = y + y;
    let z2 = z + z;
    let xx = x * x2;
    let xy = x * y2;
    let xz = x * z2;
    let yy = y * y2;
    let yz = y * z2;
    let zz = z * z2;
    let wx = w * x2;
    let wy = w * y2;
    let wz = w * z2;
    [
        1.0 - (yy + zz),
        xy + wz,
        xz - wy,
        xy - wz,
        1.0 - (xx + zz),
        yz + wx,
        xz + wy,
        yz - wx,
        1.0 - (xx + yy),
    ]
}

fn decompose_euler_xyz(mat: &[f32; 9], before: &[f32; 3]) -> [f32; 3] {
    let sy = -mat[2];
    let mut result: [f32; 3];
    if 1.0 - sy.abs() < 1e-6 {
        let y = sy.asin();
        let sx = before[0].sin();
        let sz = before[2].sin();
        if sx.abs() < sz.abs() {
            let cx = before[0].cos();
            result = if cx > 0.0 {
                [0.0, y, (-mat[3]).asin()]
            } else {
                [std::f32::consts::PI, y, mat[3].asin()]
            };
        } else {
            let cz = before[2].cos();
            result = if cz > 0.0 {
                [(-mat[7]).asin(), y, 0.0]
            } else {
                [mat[7].asin(), y, std::f32::consts::PI]
            };
        }
    } else {
        result = [mat[5].atan2(mat[8]), (-mat[2]).asin(), mat[1].atan2(mat[0])];
    }

    let pi = std::f32::consts::PI;
    let candidates: [[f32; 3]; 8] = [
        [result[0] + pi, pi - result[1], result[2] + pi],
        [result[0] + pi, pi - result[1], result[2] - pi],
        [result[0] + pi, -pi - result[1], result[2] + pi],
        [result[0] + pi, -pi - result[1], result[2] - pi],
        [result[0] - pi, pi - result[1], result[2] + pi],
        [result[0] - pi, pi - result[1], result[2] - pi],
        [result[0] - pi, -pi - result[1], result[2] + pi],
        [result[0] - pi, -pi - result[1], result[2] - pi],
    ];
    let mut min_error = diff_angle(result[0], before[0]).abs()
        + diff_angle(result[1], before[1]).abs()
        + diff_angle(result[2], before[2]).abs();
    for candidate in &candidates {
        let error = diff_angle(candidate[0], before[0]).abs()
            + diff_angle(candidate[1], before[1]).abs()
            + diff_angle(candidate[2], before[2]).abs();
        if error < min_error {
            min_error = error;
            result = *candidate;
        }
    }
    result
}

fn diff_angle(a: f32, b: f32) -> f32 {
    let diff = normalize_angle(a) - normalize_angle(b);
    if diff > std::f32::consts::PI {
        diff - std::f32::consts::TAU
    } else if diff < -std::f32::consts::PI {
        diff + std::f32::consts::TAU
    } else {
        diff
    }
}

fn normalize_angle(angle: f32) -> f32 {
    let mut result = angle;
    while result >= std::f32::consts::TAU {
        result -= std::f32::consts::TAU;
    }
    while result < 0.0 {
        result += std::f32::consts::TAU;
    }
    result
}

fn euler_xyz_to_quat(euler: &[f32; 3]) -> Quat {
    let [x, y, z] = *euler;
    let c1 = (x / 2.0).cos();
    let c2 = (y / 2.0).cos();
    let c3 = (z / 2.0).cos();
    let s1 = (x / 2.0).sin();
    let s2 = (y / 2.0).sin();
    let s3 = (z / 2.0).sin();
    Quat::from_xyzw(
        s1 * c2 * c3 + c1 * s2 * s3,
        c1 * s2 * c3 - s1 * c2 * s3,
        c1 * c2 * s3 + s1 * s2 * c3,
        c1 * c2 * c3 - s1 * s2 * s3,
    )
}

struct PlaneLinkStepInput<'a> {
    local_effector: &'a Vec3A,
    local_target: &'a Vec3A,
    link_index: usize,
    base_rotations: &'a [Quat],
    ik_rotations: &'a mut [Quat],
    chain_states: &'a mut [ChainLinkState],
    axis_index: usize,
    limits: crate::IkAngleLimit,
    iteration: usize,
    limit_angle: f32,
}

fn solve_plane_link_step(input: PlaneLinkStepInput<'_>) {
    let rotate_axis = match input.axis_index {
        0 => Vec3A::new(1.0, 0.0, 0.0),
        1 => Vec3A::new(0.0, 1.0, 0.0),
        _ => Vec3A::new(0.0, 0.0, 1.0),
    };
    let local_eff_n = input.local_effector.normalize();
    let local_tgt_n = input.local_target.normalize();

    let dot = local_eff_n.dot(local_tgt_n).clamp(-1.0, 1.0);
    let raw_angle = dot.acos();
    let capped_angle = if input.limit_angle > 0.0 {
        raw_angle.min(input.limit_angle)
    } else {
        raw_angle
    };

    let target_vec1 =
        Quat::from_axis_angle(rotate_axis.into(), capped_angle).mul_vec3a(local_eff_n);
    let target_vec2 =
        Quat::from_axis_angle(rotate_axis.into(), -capped_angle).mul_vec3a(local_eff_n);
    let signed_angle = if target_vec1.dot(local_tgt_n) > target_vec2.dot(local_tgt_n) {
        capped_angle
    } else {
        -capped_angle
    };

    let state = &mut input.chain_states[input.link_index];
    let mut next_angle = state.plane_mode_angle + signed_angle;
    let (lower, upper) = match input.axis_index {
        0 => (input.limits.min.x, input.limits.max.x),
        1 => (input.limits.min.y, input.limits.max.y),
        _ => (input.limits.min.z, input.limits.max.z),
    };
    let base = input.base_rotations[input.link_index];
    // Extract the base rotation angle on the limited axis so we can clamp the
    // *total* (base + IK) rotation to the prescribed limits rather than
    // clamping only the IK contribution.
    let base_mat = quat_to_rotation_mat3(base);
    let base_euler = decompose_euler_xyz(&base_mat, &[0.0; 3]);
    let base_axis_angle = base_euler[input.axis_index];
    let effective_min = lower - base_axis_angle;
    let effective_max = upper - base_axis_angle;

    if input.iteration == 0 && (next_angle < effective_min || next_angle > effective_max) {
        if -next_angle > effective_min && -next_angle < effective_max {
            next_angle = -next_angle;
        } else {
            let half = (effective_min + effective_max) * 0.5;
            if (half - next_angle).abs() > (half + next_angle).abs() {
                next_angle = -next_angle;
            }
        }
    }

    state.plane_mode_angle = next_angle.clamp(effective_min, effective_max);
    // Preserve the base (VMD) rotation by combining it with the IK adjustment,
    // matching the convention used by the general (non-plane) path.
    let ik_adj = Quat::from_axis_angle(rotate_axis.into(), state.plane_mode_angle);
    input.ik_rotations[input.link_index] = (base * ik_adj * base.inverse()).normalize();
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
    fn plane_link_step_preserves_base_rotation() {
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
        assert_vec3a_near(effective.mul_vec3a(Vec3A::Z), base.mul_vec3a(Vec3A::Z));
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
        let cap_ik = runtime.ik_scratch.ik_rotations.capacity();
        let cap_best = runtime.ik_scratch.best_ik_rotations.capacity();
        let cap_chain = runtime.ik_scratch.chain_states.capacity();

        for _ in 0..10 {
            runtime.evaluate_current_pose();
        }

        assert_eq!(runtime.ik_scratch.links.capacity(), cap_links);
        assert_eq!(runtime.ik_scratch.base_rotations.capacity(), cap_base);
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
