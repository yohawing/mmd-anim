use glam::{Mat4, Quat, Vec3A};

use crate::IkAngleLimit;

#[derive(Clone, Debug, PartialEq)]
pub struct IkChainLinkDefinition {
    pub bone_slot: usize,
    pub angle_limit: Option<IkAngleLimit>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IkChainDefinition {
    pub parent_slots: Vec<Option<usize>>,
    pub rest_positions: Vec<Vec3A>,
    /// Per-bone fixed axis used to constrain the CCD rotation axis during IK
    /// link steps. Does not project ordinary pose rotations.
    pub fixed_axes: Vec<Option<Vec3A>>,
    pub target_slot: usize,
    pub links: Vec<IkChainLinkDefinition>,
    pub iteration_count: u32,
    pub limit_angle: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IkChainPoseInput<'a> {
    pub parent_world_matrix: Option<Mat4>,
    pub local_position_offsets: &'a [Vec3A],
    pub local_rotations: &'a [Quat],
    pub goal_position: Vec3A,
    pub tolerance: f32,
    pub max_iterations_cap: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IkChainSolveOutput {
    pub solved_link_rotations: Vec<Quat>,
    pub final_distance: f32,
    pub executed_iterations: u32,
    pub link_steps: u32,
}

#[derive(Debug)]
pub struct IkChainSolver {
    definition: IkChainDefinition,
    /// Per-bone local-axis basis for angle-limit evaluation. Stored privately so
    /// existing `IkChainDefinition` struct literals stay source-compatible.
    local_axis_bases: Vec<Option<Quat>>,
    world_matrices: Vec<Mat4>,
    local_rotations: Vec<Quat>,
    base_rotations: Vec<Quat>,
    ik_rotations: Vec<Quat>,
    best_ik_rotations: Vec<Quat>,
    chain_states: Vec<ChainLinkState>,
}

impl IkChainSolver {
    /// Create a solver with no local-axis bases (unit XYZ angle-limit frames).
    pub fn new(definition: IkChainDefinition) -> Self {
        let bone_count = definition.rest_positions.len();
        Self::new_with_local_axis_bases(definition, vec![None; bone_count])
    }

    /// Additive constructor: attach per-bone local-axis bases used only as the
    /// angle-limit evaluation frame. Shorter lists are padded with `None`;
    /// longer lists are truncated to the definition bone count.
    pub fn new_with_local_axis_bases(
        definition: IkChainDefinition,
        local_axis_bases: Vec<Option<Quat>>,
    ) -> Self {
        let bone_count = definition.rest_positions.len();
        let link_count = definition.links.len();
        let mut bases = local_axis_bases
            .into_iter()
            .map(|basis| {
                basis.filter(|basis| basis.is_finite() && basis.length_squared() > f32::EPSILON)
            })
            .collect::<Vec<_>>();
        bases.resize(bone_count, None);
        bases.truncate(bone_count);
        Self {
            definition,
            local_axis_bases: bases,
            world_matrices: vec![Mat4::IDENTITY; bone_count],
            local_rotations: vec![Quat::IDENTITY; bone_count],
            base_rotations: Vec::with_capacity(link_count),
            ik_rotations: Vec::with_capacity(link_count),
            best_ik_rotations: Vec::with_capacity(link_count),
            chain_states: Vec::with_capacity(link_count),
        }
    }

    pub fn solve(&mut self, input: IkChainPoseInput<'_>) -> IkChainSolveOutput {
        let tolerance = input.tolerance.max(0.0);
        let iteration_count = input
            .max_iterations_cap
            .map(|cap| self.definition.iteration_count.min(cap))
            .unwrap_or(self.definition.iteration_count)
            .max(1) as usize;
        let limit_angle = self.definition.limit_angle.max(0.0);
        let link_count = self.definition.links.len();

        self.local_rotations.copy_from_slice(input.local_rotations);
        self.update_world_matrices(input);

        self.base_rotations.clear();
        self.base_rotations.extend(
            self.definition
                .links
                .iter()
                .map(|link| self.local_rotations[link.bone_slot]),
        );
        self.ik_rotations.clear();
        self.ik_rotations.resize(link_count, Quat::IDENTITY);
        self.best_ik_rotations.clear();
        self.best_ik_rotations.resize(link_count, Quat::IDENTITY);
        self.chain_states.clear();
        self.chain_states
            .resize_with(link_count, ChainLinkState::default);

        self.apply_link_rotations();
        self.update_world_matrices(input);

        let mut final_distance = f32::MAX;
        let mut best_distance = f32::MAX;
        let mut executed_iterations = 0u32;
        let mut link_steps = 0u32;

        for iteration in 0..iteration_count {
            let eff_pos = translation(self.world_matrices[self.definition.target_slot]);
            final_distance = (eff_pos - input.goal_position).length();
            if final_distance <= tolerance {
                break;
            }
            executed_iterations += 1;

            for link_index in 0..link_count {
                let link = &self.definition.links[link_index];
                let link_slot = link.bone_slot;

                if link_slot == self.definition.target_slot {
                    continue;
                }

                let link_world = self.world_matrices[link_slot];
                let link_pos = translation(link_world);
                let eff_pos = translation(self.world_matrices[self.definition.target_slot]);
                let link_world_rot = rotation(link_world);
                let local_effector = link_world_rot.inverse().mul_vec3a(eff_pos - link_pos);
                let local_target = link_world_rot
                    .inverse()
                    .mul_vec3a(input.goal_position - link_pos);

                if local_effector.length_squared() <= f32::EPSILON
                    || local_target.length_squared() <= f32::EPSILON
                {
                    continue;
                }

                let bone_slot = link.bone_slot;
                let fixed_axis = self.definition.fixed_axes.get(bone_slot).copied().flatten();
                let local_axis_basis = self.local_axis_bases.get(bone_slot).copied().flatten();
                solve_link_step(LinkStepInput {
                    local_effector: &local_effector,
                    local_target: &local_target,
                    link_index,
                    base_rotations: &self.base_rotations,
                    ik_rotations: &mut self.ik_rotations,
                    chain_states: &mut self.chain_states,
                    angle_limit: link.angle_limit,
                    iteration,
                    limit_angle,
                    local_axis_basis,
                    fixed_axis,
                });

                self.apply_link_rotations();
                self.update_world_matrices(input);
                link_steps += 1;
            }

            let current_distance = {
                let eff = translation(self.world_matrices[self.definition.target_slot]);
                (eff - input.goal_position).length()
            };
            final_distance = current_distance;
            if current_distance < best_distance {
                best_distance = current_distance;
                self.best_ik_rotations.copy_from_slice(&self.ik_rotations);
                if current_distance <= tolerance {
                    break;
                }
            } else {
                self.ik_rotations.copy_from_slice(&self.best_ik_rotations);
                self.apply_link_rotations();
                self.update_world_matrices(input);
                break;
            }
        }

        self.ik_rotations.copy_from_slice(&self.best_ik_rotations);
        self.apply_link_rotations();
        self.update_world_matrices(input);

        let solved_link_rotations = self
            .definition
            .links
            .iter()
            .map(|link| self.local_rotations[link.bone_slot])
            .collect();

        IkChainSolveOutput {
            solved_link_rotations,
            final_distance,
            executed_iterations,
            link_steps,
        }
    }

    pub(crate) fn update_world_matrices(&mut self, input: IkChainPoseInput<'_>) {
        update_mini_chain_world_matrices(
            &self.definition,
            input.parent_world_matrix.unwrap_or(Mat4::IDENTITY),
            input.local_position_offsets,
            &self.local_rotations,
            &mut self.world_matrices,
        );
    }

    fn apply_link_rotations(&mut self) {
        for (i, link) in self.definition.links.iter().enumerate() {
            let effective = (self.ik_rotations[i] * self.base_rotations[i]).normalize();
            // Fixed-axis is applied only during the CCD link step, not as a
            // post-projection of ordinary / base pose rotations.
            self.local_rotations[link.bone_slot] = effective;
        }
    }
}

pub(crate) fn update_mini_chain_world_matrices(
    definition: &IkChainDefinition,
    parent_world_matrix: Mat4,
    local_position_offsets: &[Vec3A],
    local_rotations: &[Quat],
    world_matrices: &mut [Mat4],
) {
    for slot in 0..definition.rest_positions.len() {
        let local_position = definition.rest_positions[slot] + local_position_offsets[slot];
        let local_rotation = local_rotations[slot];
        let local_matrix = Mat4::from_scale_rotation_translation(
            Vec3A::ONE.into(),
            local_rotation,
            local_position.into(),
        );
        world_matrices[slot] = match definition.parent_slots[slot] {
            Some(parent) => world_matrices[parent] * local_matrix,
            None => parent_world_matrix * local_matrix,
        };
    }
}

pub(crate) fn solve_link_step(input: LinkStepInput<'_>) {
    // fixedAxis is a hard CCD constraint. When present it owns the free
    // rotation axis; angle limits (single- or multi-axis) are then applied as a
    // post-step clamp so both constraints compose on every solver path.
    if let Some(fixed_axis) = input.fixed_axis {
        let prior_ik_rotation = input.ik_rotations[input.link_index];
        let prior_chain_state = input.chain_states[input.link_index];
        solve_unconstrained_link_step(UnconstrainedLinkStepInput {
            local_effector: input.local_effector,
            local_target: input.local_target,
            link_index: input.link_index,
            base_rotations: input.base_rotations,
            ik_rotations: input.ik_rotations,
            limit_angle: input.limit_angle,
            fixed_axis: Some(fixed_axis),
        });
        if let Some(angle_limit) = input.angle_limit {
            clamp_link_rotation_to_angle_limits(ClampAngleLimitInput {
                link_index: input.link_index,
                base_rotations: input.base_rotations,
                ik_rotations: input.ik_rotations,
                chain_states: input.chain_states,
                limits: angle_limit,
                local_axis_basis: input.local_axis_basis,
            });
            // Euler clamping can reintroduce non-twist components (and is
            // singular near ±π/2); re-project so fixedAxis remains hard.
            project_link_rotation_onto_fixed_axis(input.link_index, input.ik_rotations, fixed_axis);
            // A twist about an arbitrary fixed axis may leave the Euler box
            // after projection. Keep the previously accepted IK step rather
            // than emitting a rotation that violates the PMX link limits.
            if !link_rotation_within_angle_limits(
                input.link_index,
                input.base_rotations,
                input.ik_rotations,
                angle_limit,
                input.local_axis_basis,
            ) {
                input.ik_rotations[input.link_index] = prior_ik_rotation;
                input.chain_states[input.link_index] = prior_chain_state;
            }
        }
        return;
    }

    let single_axis = get_single_axis_limit(input.angle_limit);
    if let (Some(angle_limit), Some(axis_index)) = (input.angle_limit, single_axis) {
        solve_plane_link_step(PlaneLinkStepInput {
            local_effector: input.local_effector,
            local_target: input.local_target,
            link_index: input.link_index,
            base_rotations: input.base_rotations,
            ik_rotations: input.ik_rotations,
            chain_states: input.chain_states,
            axis_index,
            limits: angle_limit,
            iteration: input.iteration,
            limit_angle: input.limit_angle,
            local_axis_basis: input.local_axis_basis,
        });
    } else if let Some(angle_limit) = input.angle_limit {
        solve_limited_axes_link_step(LimitedAxesLinkStepInput {
            local_effector: input.local_effector,
            local_target: input.local_target,
            link_index: input.link_index,
            base_rotations: input.base_rotations,
            ik_rotations: input.ik_rotations,
            chain_states: input.chain_states,
            limits: angle_limit,
            limit_angle: input.limit_angle,
            local_axis_basis: input.local_axis_basis,
        });
    } else {
        solve_unconstrained_link_step(UnconstrainedLinkStepInput {
            local_effector: input.local_effector,
            local_target: input.local_target,
            link_index: input.link_index,
            base_rotations: input.base_rotations,
            ik_rotations: input.ik_rotations,
            limit_angle: input.limit_angle,
            fixed_axis: input.fixed_axis,
        });
    }
}

fn link_rotation_within_angle_limits(
    link_index: usize,
    base_rotations: &[Quat],
    ik_rotations: &[Quat],
    limits: IkAngleLimit,
    local_axis_basis: Option<Quat>,
) -> bool {
    let chain = ik_rotations[link_index] * base_rotations[link_index];
    if !chain.is_finite() || chain.length_squared() <= f32::EPSILON {
        return false;
    }
    let basis =
        local_axis_basis.filter(|basis| basis.is_finite() && basis.length_squared() > f32::EPSILON);
    let (q_b, q_b_inv) = basis.map_or((Quat::IDENTITY, Quat::IDENTITY), |basis| {
        let basis = basis.normalize();
        (basis, basis.inverse())
    });
    let local = (q_b_inv * chain.normalize() * q_b).normalize();
    if !local.is_finite() {
        return false;
    }
    let euler = decompose_euler_xyz(&quat_to_rotation_mat3(local), &[0.0; 3]);
    euler.iter().enumerate().all(|(axis, value)| {
        let (lower, upper) = limit_axis_bounds(limits, axis);
        *value >= lower - 1.0e-5 && *value <= upper + 1.0e-5
    })
}

struct ClampAngleLimitInput<'a> {
    link_index: usize,
    base_rotations: &'a [Quat],
    ik_rotations: &'a mut [Quat],
    chain_states: &'a mut [ChainLinkState],
    limits: IkAngleLimit,
    local_axis_basis: Option<Quat>,
}

/// Clamp the current chain rotation into angle limits without adding free CCD
/// motion. Used after a fixed-axis step so both constraints compose.
fn clamp_link_rotation_to_angle_limits(input: ClampAngleLimitInput<'_>) {
    let base = input.base_rotations[input.link_index];
    let current = input.ik_rotations[input.link_index] * base;
    if !current.is_finite() || current.length_squared() <= f32::EPSILON {
        return;
    }
    let current = current.normalize();
    let (q_b, q_b_inv) = match input.local_axis_basis {
        Some(basis) if basis.is_finite() => (basis.normalize(), basis.normalize().inverse()),
        _ => (Quat::IDENTITY, Quat::IDENTITY),
    };
    let current_la = q_b_inv * current * q_b;
    if !current_la.is_finite() || current_la.length_squared() <= f32::EPSILON {
        return;
    }
    let current_la = current_la.normalize();
    let current_mat = quat_to_rotation_mat3(current_la);
    let state = &mut input.chain_states[input.link_index];
    let mut euler = decompose_euler_xyz(&current_mat, &state.previous_euler);
    if !euler.iter().all(|v| v.is_finite()) {
        return;
    }
    for (axis_index, value) in euler.iter_mut().enumerate() {
        let (lower, upper) = limit_axis_bounds(input.limits, axis_index);
        *value = value.clamp(lower, upper);
    }
    state.previous_euler = euler;
    if let Some(axis_index) = get_single_axis_limit(Some(input.limits)) {
        state.plane_mode_angle = euler[axis_index];
    }
    let chain_rotation_la = euler_xyz_to_quat(&euler);
    if !chain_rotation_la.is_finite() || chain_rotation_la.length_squared() <= f32::EPSILON {
        return;
    }
    let chain_rotation = q_b * chain_rotation_la.normalize() * q_b_inv;
    if !chain_rotation.is_finite() || chain_rotation.length_squared() <= f32::EPSILON {
        return;
    }
    let chain_rotation = chain_rotation.normalize();
    let next_ik = chain_rotation * base.inverse();
    if !next_ik.is_finite() || next_ik.length_squared() <= f32::EPSILON {
        return;
    }
    input.ik_rotations[input.link_index] = next_ik.normalize();
}

fn project_link_rotation_onto_fixed_axis(
    link_index: usize,
    ik_rotations: &mut [Quat],
    fixed_axis: Vec3A,
) {
    if fixed_axis.length_squared() <= f32::EPSILON || !fixed_axis.is_finite() {
        return;
    }
    let ik_rotation = ik_rotations[link_index];
    if !ik_rotation.is_finite() || ik_rotation.length_squared() <= f32::EPSILON {
        return;
    }
    let constrained = constrain_rotation_to_axis(ik_rotation.normalize(), fixed_axis);
    if !constrained.is_finite() || constrained.length_squared() <= f32::EPSILON {
        return;
    }
    ik_rotations[link_index] = constrained.normalize();
}

pub(crate) struct LinkStepInput<'a> {
    pub local_effector: &'a Vec3A,
    pub local_target: &'a Vec3A,
    pub link_index: usize,
    pub base_rotations: &'a [Quat],
    pub ik_rotations: &'a mut [Quat],
    pub chain_states: &'a mut [ChainLinkState],
    pub angle_limit: Option<IkAngleLimit>,
    pub iteration: usize,
    pub limit_angle: f32,
    /// Optional PMX local-axis basis for angle-limit evaluation.
    pub local_axis_basis: Option<Quat>,
    /// Optional fixed axis that constrains the unconstrained CCD rotation axis.
    pub fixed_axis: Option<Vec3A>,
}

struct UnconstrainedLinkStepInput<'a> {
    local_effector: &'a Vec3A,
    local_target: &'a Vec3A,
    link_index: usize,
    base_rotations: &'a [Quat],
    ik_rotations: &'a mut [Quat],
    limit_angle: f32,
    fixed_axis: Option<Vec3A>,
}

fn solve_unconstrained_link_step(input: UnconstrainedLinkStepInput<'_>) {
    let local_eff_n = input.local_effector.normalize();
    let local_tgt_n = input.local_target.normalize();

    let tiny_angle = 1e-3 * std::f32::consts::PI / 180.0;

    let (axis_vec, mut angle) = if let Some(fixed) = input.fixed_axis {
        let axis = if fixed.length_squared() > f32::EPSILON && fixed.is_finite() {
            fixed.normalize()
        } else {
            return;
        };
        let signed = signed_projected_angle(local_eff_n, local_tgt_n, axis);
        if signed.abs() < tiny_angle {
            return;
        }
        (axis, signed)
    } else {
        let dot = local_eff_n.dot(local_tgt_n).clamp(-1.0, 1.0);
        let angle = dot.acos();
        if angle < tiny_angle {
            return;
        }
        let axis = local_eff_n.cross(local_tgt_n);
        let axis_vec = if axis.length() < 1e-5 {
            if dot > -1.0 + 1e-5 {
                return;
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
        (axis_vec, angle)
    };

    if input.limit_angle > 0.0 {
        angle = angle.clamp(-input.limit_angle, input.limit_angle);
    }

    let delta = Quat::from_axis_angle(axis_vec.into(), angle);
    let base = input.base_rotations[input.link_index];
    let ik = input.ik_rotations[input.link_index];
    let chain_rotation = (ik * base * delta).normalize();

    input.ik_rotations[input.link_index] = (chain_rotation * base.inverse()).normalize();
}

pub(crate) fn translation(matrix: Mat4) -> Vec3A {
    Vec3A::from_vec4(matrix.w_axis)
}

pub(crate) fn rotation(matrix: Mat4) -> Quat {
    matrix.to_scale_rotation_translation().1
}

pub(crate) fn constrain_rotation_to_axis(rotation: Quat, axis: Vec3A) -> Quat {
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

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ChainLinkState {
    pub previous_euler: [f32; 3],
    pub plane_mode_angle: f32,
}

pub(crate) fn get_single_axis_limit(limit: Option<IkAngleLimit>) -> Option<usize> {
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

pub(crate) fn quat_to_rotation_mat3(rotation: Quat) -> [f32; 9] {
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

pub(crate) fn decompose_euler_xyz(mat: &[f32; 9], before: &[f32; 3]) -> [f32; 3] {
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

pub(crate) fn euler_xyz_to_quat(euler: &[f32; 3]) -> Quat {
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

pub(crate) struct LimitedAxesLinkStepInput<'a> {
    pub local_effector: &'a Vec3A,
    pub local_target: &'a Vec3A,
    pub link_index: usize,
    pub base_rotations: &'a [Quat],
    pub ik_rotations: &'a mut [Quat],
    pub chain_states: &'a mut [ChainLinkState],
    pub limits: IkAngleLimit,
    pub limit_angle: f32,
    pub local_axis_basis: Option<Quat>,
}

pub(crate) fn solve_limited_axes_link_step(input: LimitedAxesLinkStepInput<'_>) {
    let state = &mut input.chain_states[input.link_index];
    let base = input.base_rotations[input.link_index];
    let current = (input.ik_rotations[input.link_index] * base).normalize();
    // Evaluate Euler / axis limits in the optional local-axis frame, then map
    // the clamped rotation back to bone-local space.
    let (q_b, q_b_inv) = match input.local_axis_basis {
        Some(basis) if basis.is_finite() => (basis.normalize(), basis.normalize().inverse()),
        _ => (Quat::IDENTITY, Quat::IDENTITY),
    };
    let current_la = (q_b_inv * current * q_b).normalize();
    let current_mat = quat_to_rotation_mat3(current_la);
    let mut total_euler = decompose_euler_xyz(&current_mat, &state.previous_euler);
    let mut working_effector = q_b_inv.mul_vec3a(*input.local_effector);
    let target = q_b_inv.mul_vec3a(*input.local_target).normalize();

    for axis_index in [2usize, 1, 0] {
        let (lower, upper) = limit_axis_bounds(input.limits, axis_index);
        if lower == 0.0 && upper == 0.0 {
            let next = total_euler[axis_index].clamp(lower, upper);
            let applied = next - total_euler[axis_index];
            total_euler[axis_index] = next;
            if applied.abs() > 0.0 {
                working_effector = Quat::from_axis_angle(axis_vec(axis_index).into(), applied)
                    .mul_vec3a(working_effector);
            }
            continue;
        }

        let axis = axis_vec(axis_index);
        let signed_angle = signed_projected_angle(working_effector, target, axis);
        if signed_angle.abs() <= 1.0e-6 {
            continue;
        }
        let step = if input.limit_angle > 0.0 {
            signed_angle.clamp(-input.limit_angle, input.limit_angle)
        } else {
            signed_angle
        };
        let next = (total_euler[axis_index] + step).clamp(lower, upper);
        let applied = next - total_euler[axis_index];
        total_euler[axis_index] = next;
        if applied.abs() > 0.0 {
            working_effector =
                Quat::from_axis_angle(axis.into(), applied).mul_vec3a(working_effector);
        }
    }

    state.previous_euler = total_euler;
    let chain_rotation_la = euler_xyz_to_quat(&total_euler).normalize();
    let chain_rotation = (q_b * chain_rotation_la * q_b_inv).normalize();
    input.ik_rotations[input.link_index] = (chain_rotation * base.inverse()).normalize();
}

pub(crate) fn limit_axis_bounds(limits: IkAngleLimit, axis_index: usize) -> (f32, f32) {
    match axis_index {
        0 => (limits.min.x, limits.max.x),
        1 => (limits.min.y, limits.max.y),
        _ => (limits.min.z, limits.max.z),
    }
}

pub(crate) fn axis_vec(axis_index: usize) -> Vec3A {
    match axis_index {
        0 => Vec3A::new(1.0, 0.0, 0.0),
        1 => Vec3A::new(0.0, 1.0, 0.0),
        _ => Vec3A::new(0.0, 0.0, 1.0),
    }
}

pub(crate) fn signed_projected_angle(from: Vec3A, to: Vec3A, axis: Vec3A) -> f32 {
    let projected_from = from - axis * from.dot(axis);
    let projected_to = to - axis * to.dot(axis);
    if projected_from.length_squared() <= f32::EPSILON
        || projected_to.length_squared() <= f32::EPSILON
    {
        return 0.0;
    }
    let from_n = projected_from.normalize();
    let to_n = projected_to.normalize();
    let dot = from_n.dot(to_n).clamp(-1.0, 1.0);
    let angle = dot.acos();
    let sign = axis.dot(from_n.cross(to_n)).signum();
    angle * if sign == 0.0 { 1.0 } else { sign }
}

pub(crate) struct PlaneLinkStepInput<'a> {
    pub local_effector: &'a Vec3A,
    pub local_target: &'a Vec3A,
    pub link_index: usize,
    pub base_rotations: &'a [Quat],
    pub ik_rotations: &'a mut [Quat],
    pub chain_states: &'a mut [ChainLinkState],
    pub axis_index: usize,
    pub limits: IkAngleLimit,
    pub iteration: usize,
    pub limit_angle: f32,
    pub local_axis_basis: Option<Quat>,
}

pub(crate) fn solve_plane_link_step(input: PlaneLinkStepInput<'_>) {
    let rotate_axis_la = match input.axis_index {
        0 => Vec3A::new(1.0, 0.0, 0.0),
        1 => Vec3A::new(0.0, 1.0, 0.0),
        _ => Vec3A::new(0.0, 0.0, 1.0),
    };
    let (q_b, q_b_inv) = match input.local_axis_basis {
        Some(basis) if basis.is_finite() => (basis.normalize(), basis.normalize().inverse()),
        _ => (Quat::IDENTITY, Quat::IDENTITY),
    };
    // Solve the plane limit in local-axis space, then conjugate back.
    let local_eff_n = q_b_inv.mul_vec3a(*input.local_effector).normalize();
    let local_tgt_n = q_b_inv.mul_vec3a(*input.local_target).normalize();

    let dot = local_eff_n.dot(local_tgt_n).clamp(-1.0, 1.0);
    let raw_angle = dot.acos();
    let capped_angle = if input.limit_angle > 0.0 {
        raw_angle.min(input.limit_angle)
    } else {
        raw_angle
    };

    let target_vec1 =
        Quat::from_axis_angle(rotate_axis_la.into(), capped_angle).mul_vec3a(local_eff_n);
    let target_vec2 =
        Quat::from_axis_angle(rotate_axis_la.into(), -capped_angle).mul_vec3a(local_eff_n);
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

    if input.iteration == 0 && (next_angle < lower || next_angle > upper) {
        if -next_angle > lower && -next_angle < upper {
            next_angle = -next_angle;
        } else {
            let half = (lower + upper) * 0.5;
            if (half - next_angle).abs() > (half + next_angle).abs() {
                next_angle = -next_angle;
            }
        }
    }

    state.plane_mode_angle = next_angle.clamp(lower, upper);
    let chain_rotation_la = Quat::from_axis_angle(rotate_axis_la.into(), state.plane_mode_angle);
    let chain_rotation = (q_b * chain_rotation_la * q_b_inv).normalize();
    input.ik_rotations[input.link_index] = (chain_rotation * base.inverse()).normalize();
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{BoneIndex, BoneInit, IkLinkInit, IkSolverInit, ModelArena, RuntimeInstance};

    fn assert_vec3a_near(actual: Vec3A, expected: Vec3A) {
        let delta = (actual - expected).abs();
        assert!(
            delta.x < 1.0e-5 && delta.y < 1.0e-5 && delta.z < 1.0e-5,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    fn assert_quat_near(actual: Quat, expected: Quat) {
        let actual = actual.to_array();
        let expected = expected.to_array();
        let delta = [
            (actual[0] - expected[0]).abs(),
            (actual[1] - expected[1]).abs(),
            (actual[2] - expected[2]).abs(),
            (actual[3] - expected[3]).abs(),
        ];
        assert!(
            delta[0] < 1.0e-5 && delta[1] < 1.0e-5 && delta[2] < 1.0e-5 && delta[3] < 1.0e-5,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    fn one_link_definition(angle_limit: Option<IkAngleLimit>) -> IkChainDefinition {
        IkChainDefinition {
            parent_slots: vec![None, Some(0)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::X],
            fixed_axes: vec![None, None],
            target_slot: 1,
            links: vec![IkChainLinkDefinition {
                bone_slot: 0,
                angle_limit,
            }],
            iteration_count: 1,
            limit_angle: 0.0,
        }
    }

    #[test]
    fn mini_chain_world_update_uses_identity_when_parent_world_is_unspecified() {
        let definition = one_link_definition(None);
        let mut world = vec![Mat4::IDENTITY; 2];
        update_mini_chain_world_matrices(
            &definition,
            Mat4::IDENTITY,
            &[Vec3A::ZERO, Vec3A::new(0.0, 2.0, 0.0)],
            &[
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                Quat::IDENTITY,
            ],
            &mut world,
        );

        assert_vec3a_near(translation(world[1]), Vec3A::new(-2.0, 1.0, 0.0));
    }

    #[test]
    fn primitive_matches_full_runtime_for_unconstrained_chain() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                    BoneInit::new(None, Vec3A::Y),
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

        let mut solver = IkChainSolver::new(one_link_definition(None));
        let local_position_offsets = [Vec3A::ZERO; 2];
        let local_rotations = [Quat::IDENTITY; 2];
        let output = solver.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &local_position_offsets,
            local_rotations: &local_rotations,
            goal_position: Vec3A::Y,
            tolerance: 1.0e-2,
            max_iterations_cap: None,
        });

        assert_quat_near(
            output.solved_link_rotations[0],
            runtime.pose().local_rotation(BoneIndex(0)),
        );
    }

    #[test]
    fn primitive_matches_full_runtime_for_two_link_unconstrained_chain() {
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                    BoneInit::new(Some(BoneIndex(1)), Vec3A::X),
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

        let definition = IkChainDefinition {
            parent_slots: vec![None, Some(0), Some(1)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::X, Vec3A::X],
            fixed_axes: vec![None, None, None],
            target_slot: 2,
            links: vec![
                IkChainLinkDefinition {
                    bone_slot: 1,
                    angle_limit: None,
                },
                IkChainLinkDefinition {
                    bone_slot: 0,
                    angle_limit: None,
                },
            ],
            iteration_count: 4,
            limit_angle: 0.0,
        };
        let mut solver = IkChainSolver::new(definition);
        let local_position_offsets = [Vec3A::ZERO; 3];
        let local_rotations = [Quat::IDENTITY; 3];
        let output = solver.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &local_position_offsets,
            local_rotations: &local_rotations,
            goal_position: Vec3A::new(1.0, 1.0, 0.0),
            tolerance: 1.0e-2,
            max_iterations_cap: None,
        });

        assert_quat_near(
            output.solved_link_rotations[0],
            runtime.pose().local_rotation(BoneIndex(1)),
        );
        assert_quat_near(
            output.solved_link_rotations[1],
            runtime.pose().local_rotation(BoneIndex(0)),
        );
    }

    #[test]
    fn deform_order_characterization_keeps_known_ik_delta_bounded() {
        fn solve_one_pass(
            definition: &IkChainDefinition,
            goal_position: Vec3A,
            strict: bool,
        ) -> Vec<Quat> {
            let local_position_offsets = vec![Vec3A::ZERO; definition.rest_positions.len()];
            let mut local_rotations = vec![Quat::IDENTITY; definition.rest_positions.len()];
            let base_rotations = vec![Quat::IDENTITY; definition.links.len()];
            let mut ik_rotations = vec![Quat::IDENTITY; definition.links.len()];
            let mut chain_states = vec![ChainLinkState::default(); definition.links.len()];
            let mut world_matrices = vec![Mat4::IDENTITY; definition.rest_positions.len()];

            update_mini_chain_world_matrices(
                definition,
                Mat4::IDENTITY,
                &local_position_offsets,
                &local_rotations,
                &mut world_matrices,
            );

            for link_index in 0..definition.links.len() {
                let link = &definition.links[link_index];
                let link_slot = link.bone_slot;
                let link_world = world_matrices[link_slot];
                let link_pos = translation(link_world);
                let eff_pos = translation(world_matrices[definition.target_slot]);
                let link_world_rot = rotation(link_world);
                let local_effector = link_world_rot.inverse().mul_vec3a(eff_pos - link_pos);
                let local_target = link_world_rot.inverse().mul_vec3a(goal_position - link_pos);

                let fixed_axis = definition.fixed_axes.get(link_slot).copied().flatten();
                solve_link_step(LinkStepInput {
                    local_effector: &local_effector,
                    local_target: &local_target,
                    link_index,
                    base_rotations: &base_rotations,
                    ik_rotations: &mut ik_rotations,
                    chain_states: &mut chain_states,
                    angle_limit: link.angle_limit,
                    iteration: 0,
                    limit_angle: definition.limit_angle,
                    local_axis_basis: None,
                    fixed_axis,
                });

                if strict {
                    local_rotations[link_slot] = ik_rotations[link_index].normalize();
                    update_mini_chain_world_matrices(
                        definition,
                        Mat4::IDENTITY,
                        &local_position_offsets,
                        &local_rotations,
                        &mut world_matrices,
                    );
                }
            }

            if !strict {
                for (link_index, link) in definition.links.iter().enumerate() {
                    local_rotations[link.bone_slot] = ik_rotations[link_index].normalize();
                }
                update_mini_chain_world_matrices(
                    definition,
                    Mat4::IDENTITY,
                    &local_position_offsets,
                    &local_rotations,
                    &mut world_matrices,
                );
            }

            definition
                .links
                .iter()
                .map(|link| local_rotations[link.bone_slot])
                .collect()
        }

        let definition = IkChainDefinition {
            parent_slots: vec![None, Some(0), Some(1)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::X, Vec3A::X],
            fixed_axes: vec![None, None, None],
            target_slot: 2,
            links: vec![
                IkChainLinkDefinition {
                    bone_slot: 1,
                    angle_limit: None,
                },
                IkChainLinkDefinition {
                    bone_slot: 0,
                    angle_limit: None,
                },
            ],
            iteration_count: 1,
            limit_angle: 0.0,
        };
        let goal_position = Vec3A::new(1.0, 1.0, 0.0);

        let correct_order = solve_one_pass(&definition, goal_position, true);
        let dependency_order = solve_one_pass(&definition, goal_position, false);
        let max_angular_delta = correct_order
            .iter()
            .zip(&dependency_order)
            .map(|(correct, dependency)| correct.angle_between(*dependency))
            .fold(0.0f32, f32::max);

        assert!(
            max_angular_delta > 0.0,
            "fixture must characterize a non-zero strict-order vs dependency-order IK delta"
        );
        assert!(
            max_angular_delta <= 0.79,
            "characterization budget widened unexpectedly: max_angular_delta={max_angular_delta}"
        );
    }

    #[test]
    fn primitive_matches_full_runtime_for_knee_plane_limit() {
        let limit = IkAngleLimit::new(
            Vec3A::new(0.0, 0.0, 0.0),
            Vec3A::new(0.0, 0.0, std::f32::consts::FRAC_PI_4),
        );
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                    BoneInit::new(None, Vec3A::Y),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0)).with_angle_limit(limit)],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.evaluate_current_pose();

        let mut solver = IkChainSolver::new(one_link_definition(Some(limit)));
        let local_position_offsets = [Vec3A::ZERO; 2];
        let local_rotations = [Quat::IDENTITY; 2];
        let output = solver.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &local_position_offsets,
            local_rotations: &local_rotations,
            goal_position: Vec3A::Y,
            tolerance: 1.0e-2,
            max_iterations_cap: None,
        });

        assert_quat_near(
            output.solved_link_rotations[0],
            runtime.pose().local_rotation(BoneIndex(0)),
        );
    }

    #[test]
    fn primitive_matches_full_runtime_for_limited_axes_chain() {
        let limit = IkAngleLimit::new(Vec3A::new(0.0, -0.6, -0.6), Vec3A::new(0.0, 0.6, 0.6));
        let goal = Vec3A::new(0.25, 0.55, 0.80).normalize();
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                    BoneInit::new(None, goal),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0)).with_angle_limit(limit)],
                    iteration_count: 1,
                    limit_angle: 0.0,
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.evaluate_current_pose();

        let mut solver = IkChainSolver::new(one_link_definition(Some(limit)));
        let local_position_offsets = [Vec3A::ZERO; 2];
        let local_rotations = [Quat::IDENTITY; 2];
        let output = solver.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &local_position_offsets,
            local_rotations: &local_rotations,
            goal_position: goal,
            tolerance: 1.0e-2,
            max_iterations_cap: None,
        });

        assert_quat_near(
            output.solved_link_rotations[0],
            runtime.pose().local_rotation(BoneIndex(0)),
        );
    }

    #[test]
    fn primitive_is_bit_deterministic_in_current_process_profile() {
        // The workspace currently enables glam fast-math; this test asserts
        // bit identity only across repeated solves in this same build profile.
        let definition = IkChainDefinition {
            parent_slots: vec![None, Some(0), Some(1)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::X, Vec3A::X],
            fixed_axes: vec![None, None, None],
            target_slot: 2,
            links: vec![
                IkChainLinkDefinition {
                    bone_slot: 1,
                    angle_limit: None,
                },
                IkChainLinkDefinition {
                    bone_slot: 0,
                    angle_limit: None,
                },
            ],
            iteration_count: 4,
            limit_angle: 0.0,
        };
        let local_position_offsets = [Vec3A::ZERO; 3];
        let local_rotations = [Quat::IDENTITY; 3];
        let input = IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &local_position_offsets,
            local_rotations: &local_rotations,
            goal_position: Vec3A::new(1.0, 1.0, 0.0),
            tolerance: 1.0e-2,
            max_iterations_cap: None,
        };
        let mut baseline_solver = IkChainSolver::new(definition.clone());
        let expected = baseline_solver.solve(input);

        for _ in 0..32 {
            let mut solver = IkChainSolver::new(definition.clone());
            let actual = solver.solve(input);
            assert_eq!(
                actual.final_distance.to_bits(),
                expected.final_distance.to_bits()
            );
            assert_eq!(actual.executed_iterations, expected.executed_iterations);
            assert_eq!(actual.link_steps, expected.link_steps);
            let actual_bits: Vec<_> = actual
                .solved_link_rotations
                .iter()
                .map(|q| q.to_array().map(f32::to_bits))
                .collect();
            let expected_bits: Vec<_> = expected
                .solved_link_rotations
                .iter()
                .map(|q| q.to_array().map(f32::to_bits))
                .collect();
            assert_eq!(actual_bits, expected_bits);
        }
    }

    #[test]
    fn existing_ik_chain_definition_struct_literal_has_no_local_axes() {
        // Source-compatible: no local_axis_bases field required on the public
        // definition. IkChainSolver::new leaves all bases as None.
        let definition = IkChainDefinition {
            parent_slots: vec![None, Some(0)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::X],
            fixed_axes: vec![None, None],
            target_slot: 1,
            links: vec![IkChainLinkDefinition {
                bone_slot: 0,
                angle_limit: None,
            }],
            iteration_count: 1,
            limit_angle: 0.0,
        };
        let mut solver = IkChainSolver::new(definition);
        let output = solver.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &[Vec3A::ZERO; 2],
            local_rotations: &[Quat::IDENTITY; 2],
            goal_position: Vec3A::Y,
            tolerance: 0.0,
            max_iterations_cap: None,
        });
        assert_eq!(output.solved_link_rotations.len(), 1);
    }

    #[test]
    fn fixed_axis_composes_with_single_axis_angle_limit() {
        // Without fixed-axis, pure Z plane limit can rotate X toward Y.
        // With fixed-axis = Y, CCD may only twist about Y, so X stays on XZ.
        let limit = IkAngleLimit::new(
            Vec3A::new(0.0, 0.0, -std::f32::consts::FRAC_PI_2),
            Vec3A::new(0.0, 0.0, std::f32::consts::FRAC_PI_2),
        );
        let definition = IkChainDefinition {
            parent_slots: vec![None, Some(0)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::X],
            fixed_axes: vec![Some(Vec3A::Y), None],
            target_slot: 1,
            links: vec![IkChainLinkDefinition {
                bone_slot: 0,
                angle_limit: Some(limit),
            }],
            iteration_count: 4,
            limit_angle: 0.0,
        };
        let mut solver = IkChainSolver::new(definition);
        let output = solver.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &[Vec3A::ZERO; 2],
            local_rotations: &[Quat::IDENTITY; 2],
            goal_position: Vec3A::Y,
            tolerance: 0.0,
            max_iterations_cap: None,
        });
        let child = output.solved_link_rotations[0].mul_vec3a(Vec3A::X);
        assert!(
            child.y.abs() < 1.0e-3,
            "fixed Y + Z-plane limit must not lift child off XZ; child={child:?}"
        );
        // Twist about Y is still free within limits → can move toward +Z goal component.
        // Goal is +Y which is unreachable under Y-fixed; stay near rest +X.
        assert!(
            child.x > 0.9,
            "unreachable +Y goal under Y fixed-axis keeps child near +X; child={child:?}"
        );
    }

    #[test]
    fn fixed_axis_composes_with_multi_axis_angle_limit() {
        let limit = IkAngleLimit::new(Vec3A::new(-1.0, -1.0, 0.0), Vec3A::new(1.0, 1.0, 0.0));
        let definition = IkChainDefinition {
            parent_slots: vec![None, Some(0)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::X],
            fixed_axes: vec![Some(Vec3A::Y), None],
            target_slot: 1,
            links: vec![IkChainLinkDefinition {
                bone_slot: 0,
                angle_limit: Some(limit),
            }],
            iteration_count: 4,
            limit_angle: 0.0,
        };
        let mut solver = IkChainSolver::new(definition);
        let output = solver.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &[Vec3A::ZERO; 2],
            local_rotations: &[Quat::IDENTITY; 2],
            // Mild XZ goal avoids Euler singularities near ±π/2 while still
            // requiring a non-trivial Y twist under multi-axis limits.
            goal_position: Vec3A::new(0.7, 0.0, 0.7),
            tolerance: 0.0,
            max_iterations_cap: None,
        });
        let solved = output.solved_link_rotations[0];
        let child = solved.mul_vec3a(Vec3A::X);
        assert!(
            child.y.abs() < 1.0e-3,
            "fixed Y + multi-axis limit must keep child on XZ; child={child:?}"
        );
        // Pure Y twist of the chain rotation: rotation vector parallel to Y.
        let rot_vec = Vec3A::new(solved.x, solved.y, solved.z);
        if rot_vec.length_squared() > 1.0e-8 {
            let axis = rot_vec.normalize();
            assert!(
                axis.x.abs() < 1.0e-3 && axis.z.abs() < 1.0e-3,
                "IK delta must remain pure Y twist; axis={axis:?} quat={solved:?}"
            );
        }
        assert!(
            child.z > 0.2,
            "fixed-axis multi-limit IK should still approach +Z; child={child:?}"
        );
    }

    #[test]
    fn additive_local_axis_bases_change_limited_solve() {
        let limits = IkAngleLimit::new(
            Vec3A::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0),
            Vec3A::new(std::f32::consts::FRAC_PI_2, 0.0, 0.0),
        );
        let definition = one_link_definition(Some(limits));
        let local_position_offsets = [Vec3A::ZERO; 2];
        let local_rotations = [Quat::IDENTITY; 2];
        let input = IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &local_position_offsets,
            local_rotations: &local_rotations,
            goal_position: Vec3A::Y,
            tolerance: 0.0,
            max_iterations_cap: None,
        };

        let mut unit = IkChainSolver::new(definition.clone());
        let unit_out = unit.solve(input);

        let basis = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2);
        let mut la = IkChainSolver::new_with_local_axis_bases(definition, vec![Some(basis), None]);
        let la_out = la.solve(input);

        let unit_dir = unit_out.solved_link_rotations[0]
            .mul_vec3a(Vec3A::X)
            .normalize();
        let la_dir = la_out.solved_link_rotations[0]
            .mul_vec3a(Vec3A::X)
            .normalize();
        assert!(
            (unit_dir - la_dir).length() > 0.2,
            "additive local-axis bases must change limited solve; unit={unit_dir:?} la={la_dir:?}"
        );
    }
}
