use glam::{Mat3, Mat4, Quat, Vec3, Vec3A};
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BoneIndex(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MorphIndex(pub u32);

impl MorphIndex {
    #[inline]
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneMorphOffset {
    pub target_bone: BoneIndex,
    pub position_offset: Vec3A,
    pub rotation_offset: Quat,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VertexMorphOffset {
    pub vertex_index: u32,
    pub position_offset: Vec3A,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupMorphOffset {
    pub child_morph: MorphIndex,
    pub ratio: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MorphOffsetSpan {
    pub start: u32,
    pub count: u32,
}

#[derive(Clone, Debug, Default)]
pub struct MorphInit {
    pub morph_count: u32,
    pub vertex_offsets: Vec<VertexMorphOffset>,
    pub vertex_spans: Vec<MorphOffsetSpan>,
    pub bone_offsets: Vec<BoneMorphOffset>,
    pub bone_spans: Vec<MorphOffsetSpan>,
    pub group_offsets: Vec<GroupMorphOffset>,
    pub group_spans: Vec<MorphOffsetSpan>,
}

/// Build the canonical grouped morph tables used by every runtime input
/// adapter.  Inputs retain their source order within each morph bucket while
/// this function owns span construction, index checking, and cycle traversal.
pub fn build_morph_init_from_offsets(
    morph_count: u32,
    bone_offsets: Vec<(MorphIndex, BoneMorphOffset)>,
    group_offsets: Vec<(MorphIndex, GroupMorphOffset)>,
) -> Result<MorphInit, ModelBuildError> {
    let bone_offsets = index_morph_offsets(bone_offsets)?;
    let group_offsets = index_morph_offsets(group_offsets)?;
    build_morph_init_from_indexed_offsets(morph_count, bone_offsets, group_offsets).map_err(
        |error| match error {
            ModelBuildError::GroupMorphCycleAt { morph, .. } => {
                ModelBuildError::GroupMorphCycle { morph }
            }
            other => other,
        },
    )
}

pub(crate) fn build_morph_init_from_indexed_offsets(
    morph_count: u32,
    bone_offsets: Vec<(usize, MorphIndex, BoneMorphOffset)>,
    group_offsets: Vec<(usize, MorphIndex, GroupMorphOffset)>,
) -> Result<MorphInit, ModelBuildError> {
    let count = morph_count as usize;
    if count == 0 && (!bone_offsets.is_empty() || !group_offsets.is_empty()) {
        return Err(ModelBuildError::MorphCountZeroWithData);
    }

    let mut bone_counts = try_zeroed_vec(count)?;
    for (_, index, _) in &bone_offsets {
        let count = bone_counts
            .get_mut(index.as_usize())
            .ok_or(ModelBuildError::InvalidBoneMorphMorph { offset: index.0 })?;
        *count = count
            .checked_add(1)
            .ok_or(ModelBuildError::MorphStorageAllocation)?;
    }
    let mut bone_buckets = try_empty_buckets(count)?;
    reserve_bucket_storage(&mut bone_buckets, &bone_counts)?;
    for (_, index, offset) in bone_offsets {
        let bucket = bone_buckets
            .get_mut(index.as_usize())
            .ok_or(ModelBuildError::InvalidBoneMorphMorph { offset: index.0 })?;
        bucket.push(offset);
    }
    let mut group_counts = try_zeroed_vec(count)?;
    for (_, index, offset) in &group_offsets {
        if offset.child_morph.as_usize() >= count {
            return Err(ModelBuildError::InvalidGroupMorphChild {
                morph: index.as_usize(),
                child: offset.child_morph.0,
            });
        }
        let count = group_counts
            .get_mut(index.as_usize())
            .ok_or(ModelBuildError::InvalidGroupMorph { morph: index.0 })?;
        *count = count
            .checked_add(1)
            .ok_or(ModelBuildError::MorphStorageAllocation)?;
    }
    let mut group_buckets = try_empty_buckets(count)?;
    let mut group_sources = try_empty_buckets(count)?;
    reserve_bucket_storage(&mut group_buckets, &group_counts)?;
    reserve_bucket_storage(&mut group_sources, &group_counts)?;
    for (source, index, offset) in group_offsets {
        let bucket = group_buckets
            .get_mut(index.as_usize())
            .ok_or(ModelBuildError::InvalidGroupMorph { morph: index.0 })?;
        bucket.push(offset);
        group_sources[index.as_usize()].push(source);
    }

    let mut bone_flat = try_vec_with_capacity(bone_counts.iter().sum())?;
    let mut bone_spans = try_vec_with_capacity(count)?;
    for bucket in bone_buckets {
        let start =
            u32::try_from(bone_flat.len()).map_err(|_| ModelBuildError::MorphStorageOverflow)?;
        bone_flat.extend(bucket);
        let count = u32::try_from(bone_flat.len() - start as usize)
            .map_err(|_| ModelBuildError::MorphStorageOverflow)?;
        bone_spans.push(MorphOffsetSpan { start, count });
    }

    let mut group_flat = try_vec_with_capacity(group_counts.iter().sum())?;
    let mut group_source_flat = try_vec_with_capacity(group_counts.iter().sum())?;
    let mut group_spans = try_vec_with_capacity(count)?;
    for (bucket, sources) in group_buckets.into_iter().zip(group_sources) {
        let start =
            u32::try_from(group_flat.len()).map_err(|_| ModelBuildError::MorphStorageOverflow)?;
        group_flat.extend(bucket);
        group_source_flat.extend(sources);
        let count = u32::try_from(group_flat.len() - start as usize)
            .map_err(|_| ModelBuildError::MorphStorageOverflow)?;
        group_spans.push(MorphOffsetSpan { start, count });
    }
    let morph = MorphInit {
        morph_count,
        bone_offsets: bone_flat,
        bone_spans,
        group_offsets: group_flat,
        group_spans,
        ..MorphInit::default()
    };
    validate_group_morph_cycles_with_sources(&morph, &group_source_flat)?;
    Ok(morph)
}

fn index_morph_offsets<T>(
    offsets: Vec<(MorphIndex, T)>,
) -> Result<Vec<(usize, MorphIndex, T)>, ModelBuildError> {
    let mut indexed = try_vec_with_capacity(offsets.len())?;
    indexed.extend(
        offsets
            .into_iter()
            .enumerate()
            .map(|(source, (morph, offset))| (source, morph, offset)),
    );
    Ok(indexed)
}

fn try_vec_with_capacity<T>(capacity: usize) -> Result<Vec<T>, ModelBuildError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ModelBuildError::MorphStorageAllocation)?;
    Ok(values)
}

fn try_zeroed_vec(length: usize) -> Result<Vec<usize>, ModelBuildError> {
    let mut values = try_vec_with_capacity(length)?;
    values.resize(length, 0);
    Ok(values)
}

fn try_empty_buckets<T>(length: usize) -> Result<Vec<Vec<T>>, ModelBuildError> {
    let mut buckets = try_vec_with_capacity(length)?;
    buckets.resize_with(length, Vec::new);
    Ok(buckets)
}

fn reserve_bucket_storage<T>(
    buckets: &mut [Vec<T>],
    counts: &[usize],
) -> Result<(), ModelBuildError> {
    for (bucket, &count) in buckets.iter_mut().zip(counts) {
        bucket
            .try_reserve_exact(count)
            .map_err(|_| ModelBuildError::MorphStorageAllocation)?;
    }
    Ok(())
}

impl BoneIndex {
    #[inline]
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// PMX bone local-axis descriptor (bone-local X/Z directions from the file).
///
/// The runtime builds a defensive right-handed orthonormal frame from these
/// vectors for IK angle-limit evaluation only. Ordinary pose evaluation does
/// not reorient bones by this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LocalAxis {
    pub x: Vec3A,
    pub z: Vec3A,
}

impl LocalAxis {
    pub fn new(x: Vec3A, z: Vec3A) -> Self {
        Self { x, z }
    }

    /// Build a right-handed orthonormal basis quaternion whose columns are
    /// `(x, y, z)` in bone-local space: `y = normalize(z × x)`, `z = x × y`.
    ///
    /// Returns `None` when inputs are non-finite or degenerate (near-zero /
    /// nearly parallel axes). Callers must treat `None` as "no local axis".
    pub fn basis_quat(self) -> Option<Quat> {
        build_local_axis_basis_quat(self.x, self.z)
    }
}

#[derive(Clone, Debug)]
pub struct BoneInit {
    pub parent: Option<BoneIndex>,
    pub rest_position: Vec3A,
    pub inverse_bind_matrix: Mat4,
    pub transform_order: i32,
    pub transform_after_physics: bool,
    pub fixed_axis: Option<Vec3A>,
    /// When true, ordinary world-pose evaluation projects local rotation onto
    /// `fixed_axis`. PMX import keeps this false so VMD/local rotations stay
    /// unprojected; fixed-axis still constrains CCD during IK steps via the
    /// stored `fixed_axis` descriptor.
    pub enforce_fixed_axis: bool,
}

impl BoneInit {
    pub fn new(parent: Option<BoneIndex>, rest_position: Vec3A) -> Self {
        Self {
            parent,
            rest_position,
            inverse_bind_matrix: Mat4::IDENTITY,
            transform_order: 0,
            transform_after_physics: false,
            fixed_axis: None,
            enforce_fixed_axis: false,
        }
    }

    pub fn with_fixed_axis(mut self, axis: Vec3A) -> Self {
        self.fixed_axis = Some(axis);
        self.enforce_fixed_axis = true;
        self
    }
}

/// Build a defensive right-handed local-axis basis quaternion.
///
/// Frame construction matches nanoem / Dayo style:
/// `x = normalize(local_x)`, `y = normalize(local_z × x)`, `z = x × y`.
fn build_local_axis_basis_quat(x: Vec3A, z: Vec3A) -> Option<Quat> {
    if !x.is_finite() || !z.is_finite() {
        return None;
    }
    let x_len_sq = x.length_squared();
    if x_len_sq <= f32::EPSILON {
        return None;
    }
    let x_n = x * x_len_sq.sqrt().recip();
    let y = z.cross(x_n);
    let y_len_sq = y.length_squared();
    if y_len_sq <= f32::EPSILON {
        return None;
    }
    let y_n = y * y_len_sq.sqrt().recip();
    let z_n = x_n.cross(y_n);
    let z_len_sq = z_n.length_squared();
    if z_len_sq <= f32::EPSILON {
        return None;
    }
    let z_n = z_n * z_len_sq.sqrt().recip();
    if !x_n.is_finite() || !y_n.is_finite() || !z_n.is_finite() {
        return None;
    }
    let mat = Mat3::from_cols(Vec3::from(x_n), Vec3::from(y_n), Vec3::from(z_n));
    let quat = Quat::from_mat3(&mat);
    if !quat.is_finite() || quat.length_squared() <= f32::EPSILON {
        return None;
    }
    Some(quat.normalize())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IkAngleLimit {
    pub min: Vec3A,
    pub max: Vec3A,
}

impl IkAngleLimit {
    pub fn new(min: Vec3A, max: Vec3A) -> Self {
        Self { min, max }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IkLinkInit {
    pub bone: BoneIndex,
    pub angle_limit: Option<IkAngleLimit>,
}

impl IkLinkInit {
    pub fn new(bone: BoneIndex) -> Self {
        Self {
            bone,
            angle_limit: None,
        }
    }

    pub fn with_angle_limit(mut self, angle_limit: IkAngleLimit) -> Self {
        self.angle_limit = Some(angle_limit);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IkSolverInit {
    pub ik_bone: BoneIndex,
    pub target_bone: BoneIndex,
    pub links: Vec<IkLinkInit>,
    pub iteration_count: u32,
    pub limit_angle: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AppendTransformInit {
    pub target_bone: BoneIndex,
    pub source_bone: BoneIndex,
    pub ratio: f32,
    pub affect_rotation: bool,
    pub affect_translation: bool,
    pub local: bool,
}

impl AppendTransformInit {
    pub fn new(target_bone: BoneIndex, source_bone: BoneIndex, ratio: f32) -> Self {
        Self {
            target_bone,
            source_bone,
            ratio,
            affect_rotation: false,
            affect_translation: false,
            local: false,
        }
    }

    pub fn with_rotation(mut self) -> Self {
        self.affect_rotation = true;
        self
    }

    pub fn with_translation(mut self) -> Self {
        self.affect_translation = true;
        self
    }

    pub fn with_local(mut self) -> Self {
        self.local = true;
        self
    }
}

impl IkSolverInit {
    pub fn new(ik_bone: BoneIndex, target_bone: BoneIndex, links: Vec<IkLinkInit>) -> Self {
        Self {
            ik_bone,
            target_bone,
            links,
            iteration_count: 1,
            limit_angle: 0.0,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ModelBuildError {
    #[error("model must contain at least one bone")]
    EmptyModel,
    #[error("runtime descriptor validation failed at {path}: {reason}")]
    InvalidRuntimeDescriptor { path: String, reason: String },
    #[error("bone {bone} references invalid parent {parent}")]
    InvalidParent { bone: usize, parent: u32 },
    #[error("bone hierarchy contains a cycle involving bone {bone}")]
    ParentCycle { bone: usize },
    #[error("ik solver {solver} references invalid {role} bone {bone}")]
    InvalidIkBone {
        solver: usize,
        role: &'static str,
        bone: u32,
    },
    #[error("append transform {append} references invalid {role} bone {bone}")]
    InvalidAppendBone {
        append: usize,
        role: &'static str,
        bone: u32,
    },
    #[error("bone {bone} has more than one append transform")]
    DuplicateAppendTransform { bone: u32 },
    #[error("bone morph offset {offset} references invalid target bone {bone}")]
    InvalidBoneMorphBone { offset: usize, bone: u32 },
    #[error("morph span list has length {actual}, expected {expected}")]
    InvalidMorphSpanCount { actual: usize, expected: usize },
    #[error("morph {morph} has invalid {kind} offset span")]
    InvalidMorphSpan { morph: usize, kind: &'static str },
    #[error("group morph {morph} references invalid child morph {child}")]
    InvalidGroupMorphChild { morph: usize, child: u32 },
    #[error("group morph cycle detected at morph {morph}")]
    GroupMorphCycle { morph: usize },
    #[error("group morph cycle detected at morph {morph} (offset {offset})")]
    GroupMorphCycleAt { morph: usize, offset: usize },
    #[error("morph count is zero but morph offsets are present")]
    MorphCountZeroWithData,
    #[error("bone morph references invalid morph {offset}")]
    InvalidBoneMorphMorph { offset: u32 },
    #[error("group morph references invalid morph {morph}")]
    InvalidGroupMorph { morph: u32 },
    #[error("morph storage exceeds u32 span capacity")]
    MorphStorageOverflow,
    #[error("morph storage allocation failed")]
    MorphStorageAllocation,
}

#[derive(Debug)]
pub struct ModelArena {
    parent_indices: Box<[i32]>,
    rest_positions: Box<[Vec3A]>,
    inverse_bind_matrices: Box<[Mat4]>,
    transform_orders: Box<[i32]>,
    fixed_axis_flags: Box<[u8]>,
    fixed_axis_constraint_flags: Box<[u8]>,
    fixed_axes: Box<[Vec3A]>,
    local_axis_flags: Box<[u8]>,
    local_axis_x: Box<[Vec3A]>,
    local_axis_z: Box<[Vec3A]>,
    local_axis_basis: Box<[Quat]>,
    transform_after_physics_flags: Box<[u8]>,
    ik_link_flags: Box<[u8]>,
    eval_order: Box<[BoneIndex]>,
    eval_order_before_physics: Box<[BoneIndex]>,
    eval_order_after_physics: Box<[BoneIndex]>,
    eval_order_positions: Box<[usize]>,
    ik_bone_solver_spans: Box<[MorphOffsetSpan]>,
    ik_bone_solver_indices: Box<[u32]>,
    ik_solvers: Box<[IkSolver]>,
    append_transforms: Box<[AppendTransform]>,
    append_transform_indices: Box<[i32]>,
    morph_count: u32,
    vertex_morph_offsets: Box<[VertexMorphOffset]>,
    vertex_morph_spans: Box<[MorphOffsetSpan]>,
    bone_morph_offsets: Box<[BoneMorphOffset]>,
    bone_morph_spans: Box<[MorphOffsetSpan]>,
    group_morph_offsets: Box<[GroupMorphOffset]>,
    group_morph_spans: Box<[MorphOffsetSpan]>,
}

impl ModelArena {
    pub fn new(bones: Vec<BoneInit>) -> Result<Self, ModelBuildError> {
        Self::new_full(bones, Vec::new(), Vec::new())
    }

    pub fn new_with_ik(
        bones: Vec<BoneInit>,
        ik_solvers: Vec<IkSolverInit>,
    ) -> Result<Self, ModelBuildError> {
        Self::new_full(bones, ik_solvers, Vec::new())
    }

    pub fn new_full(
        bones: Vec<BoneInit>,
        ik_solvers: Vec<IkSolverInit>,
        append_transforms: Vec<AppendTransformInit>,
    ) -> Result<Self, ModelBuildError> {
        Self::new_with_morphs(bones, ik_solvers, append_transforms, MorphInit::default())
    }

    pub fn new_with_morphs(
        bones: Vec<BoneInit>,
        ik_solvers: Vec<IkSolverInit>,
        append_transforms: Vec<AppendTransformInit>,
        morph: MorphInit,
    ) -> Result<Self, ModelBuildError> {
        if bones.is_empty() {
            return Err(ModelBuildError::EmptyModel);
        }

        let bone_count = bones.len();
        let mut parent_indices = Vec::with_capacity(bone_count);
        let mut rest_positions = Vec::with_capacity(bone_count);
        let mut inverse_bind_matrices = Vec::with_capacity(bone_count);
        let mut transform_orders = Vec::with_capacity(bone_count);
        let mut transform_after_physics_flags = Vec::with_capacity(bone_count);
        let mut fixed_axis_flags = Vec::with_capacity(bone_count);
        let mut fixed_axis_constraint_flags = Vec::with_capacity(bone_count);
        let mut fixed_axes = Vec::with_capacity(bone_count);
        let mut local_axis_flags = Vec::with_capacity(bone_count);
        let mut local_axis_x = Vec::with_capacity(bone_count);
        let mut local_axis_z = Vec::with_capacity(bone_count);
        let mut local_axis_basis = Vec::with_capacity(bone_count);

        for (bone_index, bone) in bones.iter().enumerate() {
            let parent = match bone.parent {
                Some(parent) if parent.as_usize() < bone_count => parent.0 as i32,
                Some(parent) => {
                    return Err(ModelBuildError::InvalidParent {
                        bone: bone_index,
                        parent: parent.0,
                    });
                }
                None => -1,
            };

            parent_indices.push(parent);
            rest_positions.push(bone.rest_position);
            inverse_bind_matrices.push(bone.inverse_bind_matrix);
            transform_orders.push(bone.transform_order);
            transform_after_physics_flags.push(u8::from(bone.transform_after_physics));
            match bone.fixed_axis {
                Some(axis) if axis.length_squared() > f32::EPSILON && axis.is_finite() => {
                    fixed_axis_flags.push(1);
                    fixed_axis_constraint_flags.push(u8::from(bone.enforce_fixed_axis));
                    fixed_axes.push(axis.normalize());
                }
                _ => {
                    fixed_axis_flags.push(0);
                    fixed_axis_constraint_flags.push(0);
                    fixed_axes.push(Vec3A::X);
                }
            }
            // Local axes are applied via `with_local_axes` so existing BoneInit
            // struct literals stay source-compatible (no new required fields).
            local_axis_flags.push(0);
            local_axis_x.push(Vec3A::X);
            local_axis_z.push(Vec3A::Z);
            local_axis_basis.push(Quat::IDENTITY);
        }

        let eval_order = build_eval_order(&parent_indices, &transform_orders)?;
        let eval_order_before_physics =
            build_eval_order_for_phase(&eval_order, &transform_after_physics_flags, false);
        let eval_order_after_physics =
            build_eval_order_for_phase(&eval_order, &transform_after_physics_flags, true);
        let eval_order_positions = build_eval_order_positions(&eval_order, bone_count);
        let (ik_solvers, ik_link_flags) = build_ik_solvers(ik_solvers, bone_count)?;
        let (ik_bone_solver_spans, ik_bone_solver_indices) =
            build_ik_bone_solver_lookup(&ik_solvers, bone_count);
        let (append_transforms, append_transform_indices) =
            build_append_transforms(append_transforms, bone_count)?;
        validate_morph_init(&morph, bone_count)?;

        Ok(Self {
            parent_indices: parent_indices.into_boxed_slice(),
            rest_positions: rest_positions.into_boxed_slice(),
            inverse_bind_matrices: inverse_bind_matrices.into_boxed_slice(),
            transform_orders: transform_orders.into_boxed_slice(),
            fixed_axis_flags: fixed_axis_flags.into_boxed_slice(),
            fixed_axis_constraint_flags: fixed_axis_constraint_flags.into_boxed_slice(),
            fixed_axes: fixed_axes.into_boxed_slice(),
            local_axis_flags: local_axis_flags.into_boxed_slice(),
            local_axis_x: local_axis_x.into_boxed_slice(),
            local_axis_z: local_axis_z.into_boxed_slice(),
            local_axis_basis: local_axis_basis.into_boxed_slice(),
            transform_after_physics_flags: transform_after_physics_flags.into_boxed_slice(),
            ik_link_flags,
            eval_order,
            eval_order_before_physics,
            eval_order_after_physics,
            eval_order_positions,
            ik_bone_solver_spans,
            ik_bone_solver_indices,
            ik_solvers,
            append_transforms,
            append_transform_indices,
            morph_count: morph.morph_count,
            vertex_morph_offsets: morph.vertex_offsets.into_boxed_slice(),
            vertex_morph_spans: morph.vertex_spans.into_boxed_slice(),
            bone_morph_offsets: morph.bone_offsets.into_boxed_slice(),
            bone_morph_spans: morph.bone_spans.into_boxed_slice(),
            group_morph_offsets: morph.group_offsets.into_boxed_slice(),
            group_morph_spans: morph.group_spans.into_boxed_slice(),
        })
    }

    #[inline]
    pub fn bone_count(&self) -> usize {
        self.parent_indices.len()
    }

    #[inline]
    pub fn parent_index(&self, bone: BoneIndex) -> Option<BoneIndex> {
        let parent = self.parent_indices[bone.as_usize()];
        if parent < 0 {
            None
        } else {
            Some(BoneIndex(parent as u32))
        }
    }

    #[inline]
    pub fn rest_position(&self, bone: BoneIndex) -> Vec3A {
        self.rest_positions[bone.as_usize()]
    }

    #[inline]
    pub fn inverse_bind_matrix(&self, bone: BoneIndex) -> Mat4 {
        self.inverse_bind_matrices[bone.as_usize()]
    }

    #[inline]
    pub fn transform_order(&self, bone: BoneIndex) -> i32 {
        self.transform_orders[bone.as_usize()]
    }

    #[inline]
    pub fn fixed_axis(&self, bone: BoneIndex) -> Option<Vec3A> {
        if self.fixed_axis_flags[bone.as_usize()] != 0 {
            Some(self.fixed_axes[bone.as_usize()])
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn fixed_axis_constraint(&self, bone: BoneIndex) -> Option<Vec3A> {
        if self.fixed_axis_constraint_flags[bone.as_usize()] != 0 {
            Some(self.fixed_axes[bone.as_usize()])
        } else {
            None
        }
    }

    #[inline]
    pub fn fixed_axis_count(&self) -> usize {
        self.fixed_axis_flags
            .iter()
            .filter(|&&flag| flag != 0)
            .count()
    }

    /// Overlay per-bone PMX local-axis descriptors without changing existing
    /// constructors. Entries beyond `bone_count` are ignored; shorter lists
    /// leave trailing bones without a local axis. Degenerate axes are dropped.
    pub fn with_local_axes(
        mut self,
        local_axes: impl IntoIterator<Item = Option<LocalAxis>>,
    ) -> Self {
        for (bone_index, axis) in local_axes.into_iter().enumerate() {
            if bone_index >= self.bone_count() {
                break;
            }
            match axis.and_then(|axis| {
                let basis = axis.basis_quat()?;
                Some((axis.x, axis.z, basis))
            }) {
                Some((x, z, basis)) => {
                    self.local_axis_flags[bone_index] = 1;
                    self.local_axis_x[bone_index] = x;
                    self.local_axis_z[bone_index] = z;
                    self.local_axis_basis[bone_index] = basis;
                }
                None => {
                    self.local_axis_flags[bone_index] = 0;
                    self.local_axis_x[bone_index] = Vec3A::X;
                    self.local_axis_z[bone_index] = Vec3A::Z;
                    self.local_axis_basis[bone_index] = Quat::IDENTITY;
                }
            }
        }
        self
    }

    #[inline]
    pub fn local_axis(&self, bone: BoneIndex) -> Option<LocalAxis> {
        if self.local_axis_flags[bone.as_usize()] != 0 {
            Some(LocalAxis {
                x: self.local_axis_x[bone.as_usize()],
                z: self.local_axis_z[bone.as_usize()],
            })
        } else {
            None
        }
    }

    /// Orthonormal local-axis basis as a quaternion (columns of the bone-local
    /// frame). Used only as the IK angle-limit evaluation frame.
    #[inline]
    pub fn local_axis_basis(&self, bone: BoneIndex) -> Option<Quat> {
        if self.local_axis_flags[bone.as_usize()] != 0 {
            Some(self.local_axis_basis[bone.as_usize()])
        } else {
            None
        }
    }

    #[inline]
    pub fn local_axis_count(&self) -> usize {
        self.local_axis_flags
            .iter()
            .filter(|&&flag| flag != 0)
            .count()
    }

    #[inline]
    pub fn transform_after_physics(&self, bone: BoneIndex) -> bool {
        self.transform_after_physics_flags[bone.as_usize()] != 0
    }

    #[inline]
    pub(crate) fn is_ik_link_bone(&self, bone: BoneIndex) -> bool {
        self.ik_link_flags[bone.as_usize()] != 0
    }

    #[inline]
    pub fn eval_order(&self) -> &[BoneIndex] {
        &self.eval_order
    }

    #[inline]
    pub(crate) fn eval_order_for_phase(&self, after_physics: bool) -> &[BoneIndex] {
        if after_physics {
            &self.eval_order_after_physics
        } else {
            &self.eval_order_before_physics
        }
    }

    #[inline]
    pub(crate) fn eval_order_position(&self, bone: BoneIndex) -> usize {
        self.eval_order_positions[bone.as_usize()]
    }

    #[inline]
    pub fn ik_count(&self) -> usize {
        self.ik_solvers.len()
    }

    #[inline]
    pub fn ik_solvers(&self) -> &[IkSolver] {
        &self.ik_solvers
    }

    #[inline]
    pub(crate) fn ik_solver_count_for_bone(&self, bone: BoneIndex) -> usize {
        self.ik_bone_solver_spans[bone.as_usize()].count as usize
    }

    #[inline]
    pub(crate) fn ik_solver_index_for_bone(&self, bone: BoneIndex, local_index: usize) -> usize {
        let span = &self.ik_bone_solver_spans[bone.as_usize()];
        self.ik_bone_solver_indices[span.start as usize + local_index] as usize
    }

    #[inline]
    pub fn append_transform_index(&self, bone: BoneIndex) -> Option<usize> {
        let index = self.append_transform_indices[bone.as_usize()];
        if index < 0 {
            None
        } else {
            Some(index as usize)
        }
    }

    #[inline]
    pub fn append_transform(&self, append_index: usize) -> &AppendTransform {
        &self.append_transforms[append_index]
    }

    #[inline]
    pub fn append_transforms(&self) -> &[AppendTransform] {
        &self.append_transforms
    }

    #[inline]
    pub fn morph_count(&self) -> u32 {
        self.morph_count
    }

    #[inline]
    pub fn vertex_morph_offsets(&self) -> &[VertexMorphOffset] {
        &self.vertex_morph_offsets
    }

    #[inline]
    pub fn vertex_morph_spans(&self) -> &[MorphOffsetSpan] {
        &self.vertex_morph_spans
    }

    #[inline]
    pub fn bone_morph_offsets(&self) -> &[BoneMorphOffset] {
        &self.bone_morph_offsets
    }

    #[inline]
    pub fn bone_morph_spans(&self) -> &[MorphOffsetSpan] {
        &self.bone_morph_spans
    }

    #[inline]
    pub fn group_morph_offsets(&self) -> &[GroupMorphOffset] {
        &self.group_morph_offsets
    }

    #[inline]
    pub fn group_morph_spans(&self) -> &[MorphOffsetSpan] {
        &self.group_morph_spans
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct IkSolver {
    pub ik_bone: BoneIndex,
    pub target_bone: BoneIndex,
    pub links: Box<[IkLink]>,
    pub iteration_count: u32,
    pub limit_angle: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IkLink {
    pub bone: BoneIndex,
    pub angle_limit: Option<IkAngleLimit>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppendTransform {
    pub target_bone: BoneIndex,
    pub source_bone: BoneIndex,
    pub ratio: f32,
    pub affect_rotation: bool,
    pub affect_translation: bool,
    pub local: bool,
}

type AppendTransformBuildOutput = (Box<[AppendTransform]>, Box<[i32]>);
type IkSolverBuildOutput = (Box<[IkSolver]>, Box<[u8]>);

fn build_ik_solvers(
    ik_solvers: Vec<IkSolverInit>,
    bone_count: usize,
) -> Result<IkSolverBuildOutput, ModelBuildError> {
    let mut solvers = Vec::with_capacity(ik_solvers.len());
    let mut ik_link_flags = vec![0; bone_count];

    for (solver_index, solver) in ik_solvers.into_iter().enumerate() {
        validate_ik_bone(solver_index, "ik", solver.ik_bone, bone_count)?;
        validate_ik_bone(solver_index, "target", solver.target_bone, bone_count)?;

        let mut links = Vec::with_capacity(solver.links.len());
        for link in solver.links {
            validate_ik_bone(solver_index, "link", link.bone, bone_count)?;
            ik_link_flags[link.bone.as_usize()] = 1;
            links.push(IkLink {
                bone: link.bone,
                angle_limit: link.angle_limit,
            });
        }

        solvers.push(IkSolver {
            ik_bone: solver.ik_bone,
            target_bone: solver.target_bone,
            links: links.into_boxed_slice(),
            iteration_count: solver.iteration_count,
            limit_angle: solver.limit_angle,
        });
    }

    Ok((solvers.into_boxed_slice(), ik_link_flags.into_boxed_slice()))
}

type IkBoneSolverLookup = (Box<[MorphOffsetSpan]>, Box<[u32]>);

fn build_ik_bone_solver_lookup(ik_solvers: &[IkSolver], bone_count: usize) -> IkBoneSolverLookup {
    let mut counts = vec![0u32; bone_count];
    for solver in ik_solvers {
        counts[solver.ik_bone.as_usize()] += 1;
    }

    let mut spans = Vec::with_capacity(bone_count);
    let mut next_offset = 0u32;
    for count in counts {
        spans.push(MorphOffsetSpan {
            start: next_offset,
            count,
        });
        next_offset += count;
    }

    let mut indices = vec![0u32; ik_solvers.len()];
    let mut write_positions: Vec<u32> = spans.iter().map(|span| span.start).collect();
    for (solver_index, solver) in ik_solvers.iter().enumerate() {
        let bone = solver.ik_bone.as_usize();
        let write_index = write_positions[bone] as usize;
        indices[write_index] = solver_index as u32;
        write_positions[bone] += 1;
    }

    (spans.into_boxed_slice(), indices.into_boxed_slice())
}

fn validate_ik_bone(
    solver: usize,
    role: &'static str,
    bone: BoneIndex,
    bone_count: usize,
) -> Result<(), ModelBuildError> {
    if bone.as_usize() < bone_count {
        Ok(())
    } else {
        Err(ModelBuildError::InvalidIkBone {
            solver,
            role,
            bone: bone.0,
        })
    }
}

fn build_append_transforms(
    append_transforms: Vec<AppendTransformInit>,
    bone_count: usize,
) -> Result<AppendTransformBuildOutput, ModelBuildError> {
    let mut transforms = Vec::with_capacity(append_transforms.len());
    let mut indices = vec![-1; bone_count];

    for (append_index, append) in append_transforms.into_iter().enumerate() {
        validate_append_bone(append_index, "target", append.target_bone, bone_count)?;
        validate_append_bone(append_index, "source", append.source_bone, bone_count)?;

        let target = append.target_bone.as_usize();
        if indices[target] >= 0 {
            return Err(ModelBuildError::DuplicateAppendTransform {
                bone: append.target_bone.0,
            });
        }
        indices[target] = append_index as i32;
        transforms.push(AppendTransform {
            target_bone: append.target_bone,
            source_bone: append.source_bone,
            ratio: append.ratio,
            affect_rotation: append.affect_rotation,
            affect_translation: append.affect_translation,
            local: append.local,
        });
    }

    Ok((transforms.into_boxed_slice(), indices.into_boxed_slice()))
}

fn validate_append_bone(
    append: usize,
    role: &'static str,
    bone: BoneIndex,
    bone_count: usize,
) -> Result<(), ModelBuildError> {
    if bone.as_usize() < bone_count {
        Ok(())
    } else {
        Err(ModelBuildError::InvalidAppendBone {
            append,
            role,
            bone: bone.0,
        })
    }
}

fn build_eval_order(
    parent_indices: &[i32],
    transform_orders: &[i32],
) -> Result<Box<[BoneIndex]>, ModelBuildError> {
    let mut state = vec![VisitState::Unvisited; parent_indices.len()];
    let mut order = Vec::with_capacity(parent_indices.len());
    let mut start_order = Vec::with_capacity(parent_indices.len());
    for bone in 0..parent_indices.len() {
        start_order.push(bone);
    }
    start_order.sort_by_key(|bone| (transform_orders[*bone], *bone));

    for bone in start_order {
        visit_bone(bone, parent_indices, &mut state, &mut order)?;
    }

    Ok(order.into_boxed_slice())
}

fn build_eval_order_for_phase(
    eval_order: &[BoneIndex],
    transform_after_physics_flags: &[u8],
    after_physics: bool,
) -> Box<[BoneIndex]> {
    eval_order
        .iter()
        .copied()
        .filter(|bone| {
            let bone_after_physics = transform_after_physics_flags[bone.as_usize()] != 0;
            bone_after_physics == after_physics
        })
        .collect()
}

fn build_eval_order_positions(eval_order: &[BoneIndex], bone_count: usize) -> Box<[usize]> {
    let mut positions = vec![0; bone_count];
    for (position, bone) in eval_order.iter().enumerate() {
        positions[bone.as_usize()] = position;
    }
    positions.into_boxed_slice()
}

fn visit_bone(
    bone: usize,
    parent_indices: &[i32],
    state: &mut [VisitState],
    order: &mut Vec<BoneIndex>,
) -> Result<(), ModelBuildError> {
    if state[bone] == VisitState::Visited {
        return Ok(());
    }

    // Explicit post-order stack keeps malformed/deep host descriptors from
    // consuming the native call stack.  `expanded` is the return edge from a
    // child to its parent in the former recursive implementation.
    let mut stack = vec![(bone, false)];
    while let Some((current, expanded)) = stack.pop() {
        if expanded {
            state[current] = VisitState::Visited;
            order.push(BoneIndex(current as u32));
            continue;
        }

        match state[current] {
            VisitState::Visited => continue,
            VisitState::Visiting => {
                return Err(ModelBuildError::ParentCycle { bone: current });
            }
            VisitState::Unvisited => {}
        }

        state[current] = VisitState::Visiting;
        stack.push((current, true));
        let parent = parent_indices[current];
        if parent >= 0 {
            stack.push((parent as usize, false));
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisitState {
    Unvisited,
    Visiting,
    Visited,
}

fn validate_morph_init(morph: &MorphInit, bone_count: usize) -> Result<(), ModelBuildError> {
    let morph_count = morph.morph_count as usize;
    if !morph.vertex_spans.is_empty() || !morph.vertex_offsets.is_empty() {
        validate_morph_spans(
            "vertex",
            &morph.vertex_spans,
            morph_count,
            morph.vertex_offsets.len(),
        )?;
    }
    validate_morph_spans(
        "bone",
        &morph.bone_spans,
        morph_count,
        morph.bone_offsets.len(),
    )?;
    validate_morph_spans(
        "group",
        &morph.group_spans,
        morph_count,
        morph.group_offsets.len(),
    )?;

    for (offset_index, offset) in morph.bone_offsets.iter().enumerate() {
        if offset.target_bone.as_usize() >= bone_count {
            return Err(ModelBuildError::InvalidBoneMorphBone {
                offset: offset_index,
                bone: offset.target_bone.0,
            });
        }
    }

    for (morph_index, span) in morph.group_spans.iter().enumerate() {
        for offset_index in span.start..span.start + span.count {
            let child = morph.group_offsets[offset_index as usize].child_morph;
            if child.as_usize() >= morph_count {
                return Err(ModelBuildError::InvalidGroupMorphChild {
                    morph: morph_index,
                    child: child.0,
                });
            }
        }
    }
    validate_group_morph_cycles(morph)?;

    Ok(())
}

fn validate_group_morph_cycles(morph: &MorphInit) -> Result<(), ModelBuildError> {
    let mut sources = try_vec_with_capacity(morph.group_offsets.len())?;
    sources.extend(0..morph.group_offsets.len());
    validate_group_morph_cycles_with_sources(morph, &sources).map_err(|error| match error {
        ModelBuildError::GroupMorphCycleAt { morph, .. } => {
            ModelBuildError::GroupMorphCycle { morph }
        }
        other => other,
    })
}

fn validate_group_morph_cycles_with_sources(
    morph: &MorphInit,
    sources: &[usize],
) -> Result<(), ModelBuildError> {
    let mut state = try_vec_with_capacity(morph.morph_count as usize)?;
    state.resize(morph.morph_count as usize, VisitState::Unvisited);
    let mut stack = try_vec_with_capacity(morph.morph_count as usize)?;
    for morph_index in 0..morph.morph_count as usize {
        visit_group_morph(morph_index, morph, &mut state, sources, &mut stack)?;
    }
    Ok(())
}

fn visit_group_morph(
    morph_index: usize,
    morph: &MorphInit,
    state: &mut [VisitState],
    sources: &[usize],
    stack: &mut Vec<(usize, usize)>,
) -> Result<(), ModelBuildError> {
    if state[morph_index] == VisitState::Visited {
        return Ok(());
    }

    // Iterative DFS equivalent to the previous recursive walk.  The second
    // tuple member is the next offset in this morph's span to inspect.
    stack.clear();
    stack.push((morph_index, 0usize));
    state[morph_index] = VisitState::Visiting;
    while let Some((current, next_offset)) = stack.last_mut() {
        let span = morph.group_spans[*current];
        if *next_offset >= span.count as usize {
            state[*current] = VisitState::Visited;
            stack.pop();
            continue;
        }

        let offset_index = span.start as usize + *next_offset;
        *next_offset += 1;
        let child = morph.group_offsets[offset_index].child_morph.as_usize();
        if morph.group_spans[child].count == 0 {
            continue;
        }
        match state[child] {
            VisitState::Visited => {}
            VisitState::Visiting => {
                return Err(ModelBuildError::GroupMorphCycleAt {
                    morph: child,
                    offset: sources[offset_index],
                });
            }
            VisitState::Unvisited => {
                state[child] = VisitState::Visiting;
                stack.push((child, 0));
            }
        }
    }

    Ok(())
}

fn validate_morph_spans(
    kind: &'static str,
    spans: &[MorphOffsetSpan],
    morph_count: usize,
    offset_count: usize,
) -> Result<(), ModelBuildError> {
    if spans.len() != morph_count {
        return Err(ModelBuildError::InvalidMorphSpanCount {
            actual: spans.len(),
            expected: morph_count,
        });
    }

    for (morph_index, span) in spans.iter().enumerate() {
        let start = span.start as usize;
        let count = span.count as usize;
        if start
            .checked_add(count)
            .is_none_or(|end| end > offset_count)
        {
            return Err(ModelBuildError::InvalidMorphSpan {
                morph: morph_index,
                kind,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_capacity_overflow_as_morph_storage_allocation_error() {
        assert_eq!(
            try_vec_with_capacity::<u8>(usize::MAX).unwrap_err(),
            ModelBuildError::MorphStorageAllocation
        );
    }

    #[test]
    fn rejects_invalid_parent() {
        let error =
            ModelArena::new(vec![BoneInit::new(Some(BoneIndex(10)), Vec3A::ZERO)]).unwrap_err();

        assert_eq!(
            error,
            ModelBuildError::InvalidParent {
                bone: 0,
                parent: 10
            }
        );
    }

    #[test]
    fn eval_order_for_phase_preserves_relative_order_within_each_phase() {
        let mut root = BoneInit::new(None, Vec3A::ZERO);
        root.transform_order = 0;
        let mut pre_child = BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO);
        pre_child.transform_order = 2;
        let mut after_parent = BoneInit::new(None, Vec3A::ZERO);
        after_parent.transform_order = 1;
        after_parent.transform_after_physics = true;
        let mut after_child = BoneInit::new(Some(BoneIndex(2)), Vec3A::ZERO);
        after_child.transform_order = 3;
        after_child.transform_after_physics = true;
        let mut pre_sibling = BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO);
        pre_sibling.transform_order = 4;

        let model = ModelArena::new(vec![
            root,
            pre_child,
            after_parent,
            after_child,
            pre_sibling,
        ])
        .unwrap();

        let expected_before_physics: Vec<_> = model
            .eval_order()
            .iter()
            .copied()
            .filter(|bone| !model.transform_after_physics(*bone))
            .collect();
        let expected_after_physics: Vec<_> = model
            .eval_order()
            .iter()
            .copied()
            .filter(|bone| model.transform_after_physics(*bone))
            .collect();

        assert_eq!(
            model.eval_order_for_phase(false),
            expected_before_physics.as_slice()
        );
        assert_eq!(
            model.eval_order_for_phase(true),
            expected_after_physics.as_slice()
        );
        assert_eq!(
            model.eval_order_for_phase(false),
            &[BoneIndex(0), BoneIndex(1), BoneIndex(4)]
        );
        assert_eq!(
            model.eval_order_for_phase(true),
            &[BoneIndex(2), BoneIndex(3)]
        );
    }

    #[test]
    fn parent_is_ordered_before_child_even_if_input_order_is_not_transform_order() {
        let mut root = BoneInit::new(None, Vec3A::ZERO);
        root.transform_order = 10;
        let child = BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO);

        let model = ModelArena::new(vec![root, child]).unwrap();

        assert_eq!(model.eval_order(), &[BoneIndex(0), BoneIndex(1)]);
    }

    #[test]
    fn stores_ik_solver_descriptors() {
        let solver = IkSolverInit {
            ik_bone: BoneIndex(2),
            target_bone: BoneIndex(1),
            links: vec![
                IkLinkInit::new(BoneIndex(0))
                    .with_angle_limit(IkAngleLimit::new(Vec3A::splat(-1.0), Vec3A::splat(1.0))),
            ],
            iteration_count: 4,
            limit_angle: 0.5,
        };

        let model = ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(1)), Vec3A::ZERO),
            ],
            vec![solver],
        )
        .unwrap();

        assert_eq!(model.ik_count(), 1);
        assert_eq!(model.ik_solvers()[0].ik_bone, BoneIndex(2));
        assert_eq!(model.ik_solvers()[0].target_bone, BoneIndex(1));
        assert_eq!(model.ik_solvers()[0].links[0].bone, BoneIndex(0));
        assert_eq!(model.ik_solvers()[0].iteration_count, 4);
        assert_eq!(model.ik_solvers()[0].limit_angle, 0.5);
    }

    #[test]
    fn ik_bone_solver_lookup_maps_multiple_bones_and_preserves_registration_order() {
        let model = ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(1)), Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
            ],
            vec![
                IkSolverInit::new(
                    BoneIndex(2),
                    BoneIndex(1),
                    vec![IkLinkInit::new(BoneIndex(0))],
                ),
                IkSolverInit::new(
                    BoneIndex(4),
                    BoneIndex(3),
                    vec![IkLinkInit::new(BoneIndex(0))],
                ),
                IkSolverInit::new(
                    BoneIndex(2),
                    BoneIndex(1),
                    vec![IkLinkInit::new(BoneIndex(0))],
                ),
            ],
        )
        .unwrap();

        assert_eq!(model.ik_solver_count_for_bone(BoneIndex(0)), 0);
        assert_eq!(model.ik_solver_count_for_bone(BoneIndex(1)), 0);
        assert_eq!(model.ik_solver_count_for_bone(BoneIndex(2)), 2);
        assert_eq!(model.ik_solver_count_for_bone(BoneIndex(3)), 0);
        assert_eq!(model.ik_solver_count_for_bone(BoneIndex(4)), 1);
        assert_eq!(model.ik_solver_index_for_bone(BoneIndex(2), 0), 0);
        assert_eq!(model.ik_solver_index_for_bone(BoneIndex(2), 1), 2);
        assert_eq!(model.ik_solver_index_for_bone(BoneIndex(4), 0), 1);
    }

    #[test]
    fn stores_normalized_fixed_axis_descriptors() {
        let model = ModelArena::new(vec![
            BoneInit::new(None, Vec3A::ZERO).with_fixed_axis(Vec3A::new(0.0, 2.0, 0.0)),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
        ])
        .unwrap();

        assert_eq!(model.fixed_axis(BoneIndex(0)), Some(Vec3A::Y));
        assert_eq!(model.fixed_axis(BoneIndex(1)), None);
    }

    #[test]
    fn stores_local_axis_descriptors_and_basis() {
        let model = ModelArena::new(vec![
            BoneInit::new(None, Vec3A::ZERO),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
        ])
        .unwrap()
        .with_local_axes([
            Some(LocalAxis::new(
                Vec3A::new(0.0, 1.0, 0.0),
                Vec3A::new(0.0, 0.0, 1.0),
            )),
            Some(LocalAxis::new(Vec3A::ZERO, Vec3A::Z)),
            None,
        ]);

        let axis = model.local_axis(BoneIndex(0)).expect("local axis retained");
        assert_eq!(axis.x, Vec3A::new(0.0, 1.0, 0.0));
        assert_eq!(axis.z, Vec3A::new(0.0, 0.0, 1.0));
        let basis = model
            .local_axis_basis(BoneIndex(0))
            .expect("local axis basis");
        // Right-handed rebuild: x=(0,1,0), y=z×x=(0,0,1)×(0,1,0)=(-1,0,0), z=x×y=(0,0,1).
        let x_dir = basis * Vec3A::X;
        let y_dir = basis * Vec3A::Y;
        let z_dir = basis * Vec3A::Z;
        assert!((x_dir - Vec3A::Y).length() < 1.0e-5);
        assert!((y_dir - (-Vec3A::X)).length() < 1.0e-5);
        assert!((z_dir - Vec3A::Z).length() < 1.0e-5);
        assert!(model.local_axis(BoneIndex(1)).is_none());
        assert!(model.local_axis_basis(BoneIndex(1)).is_none());
        assert!(model.local_axis(BoneIndex(2)).is_none());
        assert_eq!(model.local_axis_count(), 1);
    }

    #[test]
    fn existing_bone_init_constructors_have_no_local_axes() {
        // Source-compatible construction: BoneInit::new and full struct literal
        // without any local-axis field retain empty local-axis storage.
        let via_new = ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap();
        assert_eq!(via_new.local_axis_count(), 0);
        assert!(via_new.local_axis(BoneIndex(0)).is_none());

        let via_literal = ModelArena::new(vec![BoneInit {
            parent: None,
            rest_position: Vec3A::ZERO,
            inverse_bind_matrix: Mat4::IDENTITY,
            transform_order: 0,
            transform_after_physics: false,
            fixed_axis: None,
            enforce_fixed_axis: false,
        }])
        .unwrap();
        assert_eq!(via_literal.local_axis_count(), 0);
    }

    #[test]
    fn rejects_invalid_ik_link_bone() {
        let error = ModelArena::new_with_ik(
            vec![BoneInit::new(None, Vec3A::ZERO)],
            vec![IkSolverInit::new(
                BoneIndex(0),
                BoneIndex(0),
                vec![IkLinkInit::new(BoneIndex(10))],
            )],
        )
        .unwrap_err();

        assert_eq!(
            error,
            ModelBuildError::InvalidIkBone {
                solver: 0,
                role: "link",
                bone: 10,
            }
        );
    }

    #[test]
    fn stores_append_transform_descriptors() {
        let model = ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
            ],
            Vec::new(),
            vec![
                AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 0.5)
                    .with_rotation()
                    .with_translation(),
            ],
        )
        .unwrap();

        let append_index = model.append_transform_index(BoneIndex(1)).unwrap();
        let append = model.append_transform(append_index);
        assert_eq!(append.source_bone, BoneIndex(0));
        assert_eq!(append.ratio, 0.5);
        assert!(append.affect_rotation);
        assert!(append.affect_translation);
    }

    #[test]
    fn rejects_duplicate_append_transform_target() {
        let error = ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
            ],
            Vec::new(),
            vec![
                AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0),
                AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0),
            ],
        )
        .unwrap_err();

        assert_eq!(error, ModelBuildError::DuplicateAppendTransform { bone: 1 });
    }

    #[test]
    fn rejects_invalid_bone_morph_target_bone() {
        let error = ModelArena::new_with_morphs(
            vec![BoneInit::new(None, Vec3A::ZERO)],
            Vec::new(),
            Vec::new(),
            MorphInit {
                morph_count: 1,
                bone_offsets: vec![BoneMorphOffset {
                    target_bone: BoneIndex(10),
                    position_offset: Vec3A::ZERO,
                    rotation_offset: Quat::IDENTITY,
                }],
                bone_spans: vec![MorphOffsetSpan { start: 0, count: 1 }],
                group_offsets: Vec::new(),
                group_spans: vec![MorphOffsetSpan::default()],
                ..MorphInit::default()
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            ModelBuildError::InvalidBoneMorphBone {
                offset: 0,
                bone: 10
            }
        );
    }

    #[test]
    fn accepts_group_morph_child_that_is_later() {
        let model = ModelArena::new_with_morphs(
            vec![BoneInit::new(None, Vec3A::ZERO)],
            Vec::new(),
            Vec::new(),
            MorphInit {
                morph_count: 2,
                bone_offsets: Vec::new(),
                bone_spans: vec![MorphOffsetSpan::default(), MorphOffsetSpan::default()],
                group_offsets: vec![GroupMorphOffset {
                    child_morph: MorphIndex(1),
                    ratio: 1.0,
                }],
                group_spans: vec![
                    MorphOffsetSpan { start: 0, count: 1 },
                    MorphOffsetSpan::default(),
                ],
                ..MorphInit::default()
            },
        )
        .unwrap();

        assert_eq!(model.morph_count(), 2);
    }

    #[test]
    fn rejects_group_morph_child_out_of_range() {
        let error = ModelArena::new_with_morphs(
            vec![BoneInit::new(None, Vec3A::ZERO)],
            Vec::new(),
            Vec::new(),
            MorphInit {
                morph_count: 2,
                bone_offsets: Vec::new(),
                bone_spans: vec![MorphOffsetSpan::default(), MorphOffsetSpan::default()],
                group_offsets: vec![GroupMorphOffset {
                    child_morph: MorphIndex(2),
                    ratio: 1.0,
                }],
                group_spans: vec![
                    MorphOffsetSpan { start: 0, count: 1 },
                    MorphOffsetSpan::default(),
                ],
                ..MorphInit::default()
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            ModelBuildError::InvalidGroupMorphChild { morph: 0, child: 2 }
        );
    }

    #[test]
    fn rejects_group_morph_cycle() {
        let error = ModelArena::new_with_morphs(
            vec![BoneInit::new(None, Vec3A::ZERO)],
            Vec::new(),
            Vec::new(),
            MorphInit {
                morph_count: 2,
                bone_offsets: Vec::new(),
                bone_spans: vec![MorphOffsetSpan::default(), MorphOffsetSpan::default()],
                group_offsets: vec![
                    GroupMorphOffset {
                        child_morph: MorphIndex(1),
                        ratio: 1.0,
                    },
                    GroupMorphOffset {
                        child_morph: MorphIndex(0),
                        ratio: 1.0,
                    },
                ],
                group_spans: vec![
                    MorphOffsetSpan { start: 0, count: 1 },
                    MorphOffsetSpan { start: 1, count: 1 },
                ],
                ..MorphInit::default()
            },
        )
        .unwrap_err();

        assert_eq!(error, ModelBuildError::GroupMorphCycle { morph: 0 });
    }

    #[test]
    fn stores_vertex_morph_offsets() {
        let model = ModelArena::new_with_morphs(
            vec![BoneInit::new(None, Vec3A::ZERO)],
            Vec::new(),
            Vec::new(),
            MorphInit {
                morph_count: 1,
                vertex_offsets: vec![VertexMorphOffset {
                    vertex_index: 7,
                    position_offset: Vec3A::new(1.0, 2.0, 3.0),
                }],
                vertex_spans: vec![MorphOffsetSpan { start: 0, count: 1 }],
                bone_spans: vec![MorphOffsetSpan::default()],
                group_spans: vec![MorphOffsetSpan::default()],
                ..MorphInit::default()
            },
        )
        .unwrap();

        assert_eq!(
            model.vertex_morph_offsets(),
            &[VertexMorphOffset {
                vertex_index: 7,
                position_offset: Vec3A::new(1.0, 2.0, 3.0),
            }]
        );
        assert_eq!(
            model.vertex_morph_spans(),
            &[MorphOffsetSpan { start: 0, count: 1 }]
        );
    }
}
