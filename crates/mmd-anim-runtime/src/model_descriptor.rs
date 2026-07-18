//! C-layout-independent version 1 runtime model descriptors.
//!
//! This module is deliberately made up of ordinary Rust values (`Vec`,
//! `Option`, and glam types).  FFI layers can copy their own records into this
//! representation, while format importers can construct it without adopting a
//! C ABI.  The compiler is the single normalization point for absolute PMX
//! rest positions, metadata and offset tables.

use std::fmt;

use glam::{Quat, Vec3A};
use thiserror::Error;

use crate::{
    AppendTransformInit, BoneIndex, BoneInit, BoneMorphOffset, GroupMorphOffset, IkAngleLimit,
    IkLinkInit, IkSolverInit, LocalAxis, ModelArena, MorphIndex, MorphInit, MorphOffsetSpan,
};

/// The only descriptor version understood by this crate.
pub const RUNTIME_MODEL_DESCRIPTOR_VERSION_V1: u32 = 1;

/// A host-independent snapshot of all runtime model data needed by v1.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeModelDescriptorV1 {
    /// Must be [`RUNTIME_MODEL_DESCRIPTOR_VERSION_V1`].
    pub descriptor_version: u32,
    pub bones: Vec<RuntimeBoneDescriptorV1>,
    pub ik_solvers: Vec<RuntimeIkSolverDescriptorV1>,
    pub append_transforms: Vec<RuntimeAppendTransformDescriptorV1>,
    pub morphs: RuntimeMorphDescriptorV1,
}

impl Default for RuntimeModelDescriptorV1 {
    fn default() -> Self {
        Self {
            descriptor_version: RUNTIME_MODEL_DESCRIPTOR_VERSION_V1,
            bones: Vec::new(),
            ik_solvers: Vec::new(),
            append_transforms: Vec::new(),
            morphs: RuntimeMorphDescriptorV1::default(),
        }
    }
}

impl RuntimeModelDescriptorV1 {
    pub fn new(bones: Vec<RuntimeBoneDescriptorV1>) -> Self {
        Self {
            bones,
            ..Self::default()
        }
    }
}

/// Absolute PMX/MMD-space rest position and per-bone metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeBoneDescriptorV1 {
    pub parent: Option<BoneIndex>,
    pub rest_position: Vec3A,
    pub transform_order: i32,
    pub transform_after_physics: bool,
    pub fixed_axis: Option<Vec3A>,
    pub local_axis: Option<LocalAxis>,
}

impl RuntimeBoneDescriptorV1 {
    pub fn new(parent: Option<BoneIndex>, rest_position: Vec3A) -> Self {
        Self {
            parent,
            rest_position,
            transform_order: 0,
            transform_after_physics: false,
            fixed_axis: None,
            local_axis: None,
        }
    }
}

/// One link in an IK chain.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeIkLinkDescriptorV1 {
    pub bone: BoneIndex,
    pub angle_limit: Option<IkAngleLimit>,
}

impl RuntimeIkLinkDescriptorV1 {
    pub fn new(bone: BoneIndex) -> Self {
        Self {
            bone,
            angle_limit: None,
        }
    }
}

/// An IK solver attached to one IK bone.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeIkSolverDescriptorV1 {
    pub ik_bone: BoneIndex,
    pub target_bone: BoneIndex,
    pub links: Vec<RuntimeIkLinkDescriptorV1>,
    pub iteration_count: u32,
    pub limit_angle: f32,
}

impl RuntimeIkSolverDescriptorV1 {
    pub fn new(
        ik_bone: BoneIndex,
        target_bone: BoneIndex,
        links: Vec<RuntimeIkLinkDescriptorV1>,
    ) -> Self {
        Self {
            ik_bone,
            target_bone,
            links,
            iteration_count: 1,
            limit_angle: 0.0,
        }
    }
}

/// An append transform (rotation and/or translation, optionally in local
/// space).  A target may have at most one append transform.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeAppendTransformDescriptorV1 {
    pub target_bone: BoneIndex,
    pub source_bone: BoneIndex,
    pub ratio: f32,
    pub affect_rotation: bool,
    pub affect_translation: bool,
    pub local: bool,
}

impl RuntimeAppendTransformDescriptorV1 {
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
}

/// Bone and group morph offsets.  Offsets are grouped by `morph_index` by the
/// compiler and exposed in `ModelArena` through spans.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeMorphDescriptorV1 {
    pub morph_count: u32,
    pub bone_offsets: Vec<RuntimeBoneMorphOffsetDescriptorV1>,
    pub group_offsets: Vec<RuntimeGroupMorphOffsetDescriptorV1>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeBoneMorphOffsetDescriptorV1 {
    pub morph_index: MorphIndex,
    pub target_bone: BoneIndex,
    pub position_offset: Vec3A,
    pub rotation_offset: Quat,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeGroupMorphOffsetDescriptorV1 {
    pub morph_index: MorphIndex,
    pub child_morph: MorphIndex,
    pub ratio: f32,
}

/// Detailed validation failure.  `path` always identifies the offending
/// descriptor field, including its zero-based array index where applicable.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{path}: {kind}")]
pub struct RuntimeModelDescriptorError {
    pub path: String,
    pub kind: RuntimeModelDescriptorErrorKind,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RuntimeModelDescriptorErrorKind {
    #[error("descriptor version must be {expected}, got {actual}")]
    UnsupportedVersion { expected: u32, actual: u32 },
    #[error("model must contain at least one bone")]
    EmptyBones,
    #[error("index {value} is out of range for length {length}")]
    IndexOutOfRange { value: u32, length: usize },
    #[error("parent cannot reference itself")]
    SelfParent,
    #[error("parent hierarchy contains a cycle")]
    ParentCycle,
    #[error("value is not finite")]
    NonFinite,
    #[error("axis is zero-length or otherwise degenerate")]
    DegenerateAxis,
    #[error("quaternion is zero-length or otherwise degenerate")]
    DegenerateQuaternion,
    #[error("minimum must not exceed maximum")]
    InvalidRange,
    #[error("iteration count must be greater than zero")]
    InvalidIterationCount,
    #[error("limit angle must be finite and non-negative")]
    InvalidLimitAngle,
    #[error("append ratio must be finite")]
    InvalidAppendRatio,
    #[error("append target is already used by another append transform")]
    DuplicateAppendTarget,
    #[error("append requires rotation and/or translation")]
    InvalidAppendFlags,
    #[error("morph count is zero but morph offsets are present")]
    EmptyMorphSet,
    #[error("morph group graph contains a cycle")]
    GroupMorphCycle,
    #[error("model arena rejected normalized payload: {0}")]
    ModelBuild(String),
    #[error("descriptor storage allocation failed")]
    AllocationFailed,
}

impl RuntimeModelDescriptorError {
    fn new(path: impl Into<String>, kind: RuntimeModelDescriptorErrorKind) -> Self {
        Self {
            path: path.into(),
            kind,
        }
    }
}

/// Compile a validated v1 descriptor into the immutable runtime arena.
pub fn compile_runtime_model_descriptor_v1(
    descriptor: &RuntimeModelDescriptorV1,
) -> Result<ModelArena, RuntimeModelDescriptorError> {
    if descriptor.descriptor_version != RUNTIME_MODEL_DESCRIPTOR_VERSION_V1 {
        return Err(RuntimeModelDescriptorError::new(
            "descriptor_version",
            RuntimeModelDescriptorErrorKind::UnsupportedVersion {
                expected: RUNTIME_MODEL_DESCRIPTOR_VERSION_V1,
                actual: descriptor.descriptor_version,
            },
        ));
    }
    if descriptor.bones.is_empty() {
        return Err(RuntimeModelDescriptorError::new(
            "bones",
            RuntimeModelDescriptorErrorKind::EmptyBones,
        ));
    }

    let bone_count = descriptor.bones.len();
    validate_bones(&descriptor.bones)?;
    validate_ik_solvers(&descriptor.ik_solvers, bone_count)?;
    validate_append_transforms(&descriptor.append_transforms, bone_count)?;

    let mut absolute_positions = Vec::with_capacity(bone_count);
    for (bone_index, bone) in descriptor.bones.iter().enumerate() {
        let path = format!("bones[{bone_index}].rest_position");
        validate_vec3(path, bone.rest_position)?;
        absolute_positions.push(bone.rest_position);
    }

    let mut bones = Vec::with_capacity(bone_count);
    let mut local_axes = Vec::with_capacity(bone_count);
    for (bone_index, descriptor_bone) in descriptor.bones.iter().enumerate() {
        let parent = descriptor_bone.parent;
        let absolute_position = absolute_positions[bone_index];
        let rest_position = parent
            .map(|index| absolute_position - absolute_positions[index.as_usize()])
            .unwrap_or(absolute_position);
        validate_vec3(format!("bones[{bone_index}].rest_position"), rest_position)?;
        bones.push(BoneInit {
            parent,
            rest_position,
            inverse_bind_matrix: glam::Mat4::from_translation((-absolute_position).into()),
            transform_order: descriptor_bone.transform_order,
            transform_after_physics: descriptor_bone.transform_after_physics,
            fixed_axis: descriptor_bone.fixed_axis,
            // PMX fixed axis is metadata and an IK constraint; ordinary local
            // pose evaluation must not project rotations onto it.
            enforce_fixed_axis: false,
        });
        local_axes.push(descriptor_bone.local_axis);
    }

    let ik_solvers = descriptor
        .ik_solvers
        .iter()
        .map(|solver| IkSolverInit {
            ik_bone: solver.ik_bone,
            target_bone: solver.target_bone,
            links: solver
                .links
                .iter()
                .map(|link| IkLinkInit {
                    bone: link.bone,
                    angle_limit: link.angle_limit,
                })
                .collect(),
            iteration_count: solver.iteration_count,
            limit_angle: solver.limit_angle,
        })
        .collect();

    let append_transforms = descriptor
        .append_transforms
        .iter()
        .map(|append| AppendTransformInit {
            target_bone: append.target_bone,
            source_bone: append.source_bone,
            ratio: append.ratio,
            affect_rotation: append.affect_rotation,
            affect_translation: append.affect_translation,
            local: append.local,
        })
        .collect();

    let morph = compile_morphs(&descriptor.morphs, bone_count)?;
    let model = ModelArena::new_with_morphs(bones, ik_solvers, append_transforms, morph).map_err(
        |error| match error {
            crate::ModelBuildError::ParentCycle { bone } => RuntimeModelDescriptorError::new(
                format!("bones[{bone}].parent"),
                RuntimeModelDescriptorErrorKind::ParentCycle,
            ),
            crate::ModelBuildError::GroupMorphCycle { morph } => RuntimeModelDescriptorError::new(
                format!("morphs.group_offsets[{morph}].child_morph"),
                RuntimeModelDescriptorErrorKind::GroupMorphCycle,
            ),
            other => RuntimeModelDescriptorError::new(
                "model",
                RuntimeModelDescriptorErrorKind::ModelBuild(other.to_string()),
            ),
        },
    )?;
    Ok(model.with_local_axes(local_axes))
}

fn validate_bones(bones: &[RuntimeBoneDescriptorV1]) -> Result<(), RuntimeModelDescriptorError> {
    for (bone_index, bone) in bones.iter().enumerate() {
        if let Some(parent) = bone.parent {
            if parent.as_usize() >= bones.len() {
                return Err(RuntimeModelDescriptorError::new(
                    format!("bones[{bone_index}].parent"),
                    RuntimeModelDescriptorErrorKind::IndexOutOfRange {
                        value: parent.0,
                        length: bones.len(),
                    },
                ));
            }
            if parent.as_usize() == bone_index {
                return Err(RuntimeModelDescriptorError::new(
                    format!("bones[{bone_index}].parent"),
                    RuntimeModelDescriptorErrorKind::SelfParent,
                ));
            }
        }
        validate_vec3(
            format!("bones[{bone_index}].rest_position"),
            bone.rest_position,
        )?;
        if let Some(axis) = bone.fixed_axis {
            validate_axis(format!("bones[{bone_index}].fixed_axis"), axis)?;
        }
        if let Some(axis) = bone.local_axis {
            validate_axis(format!("bones[{bone_index}].local_axis.x"), axis.x)?;
            validate_axis(format!("bones[{bone_index}].local_axis.z"), axis.z)?;
            if axis.basis_quat().is_none() {
                return Err(RuntimeModelDescriptorError::new(
                    format!("bones[{bone_index}].local_axis"),
                    RuntimeModelDescriptorErrorKind::DegenerateAxis,
                ));
            }
        }
    }

    Ok(())
}

fn validate_ik_solvers(
    solvers: &[RuntimeIkSolverDescriptorV1],
    bone_count: usize,
) -> Result<(), RuntimeModelDescriptorError> {
    for (solver_index, solver) in solvers.iter().enumerate() {
        validate_bone_index(
            format!("ik_solvers[{solver_index}].ik_bone"),
            solver.ik_bone,
            bone_count,
        )?;
        validate_bone_index(
            format!("ik_solvers[{solver_index}].target_bone"),
            solver.target_bone,
            bone_count,
        )?;
        if solver.iteration_count == 0 {
            return Err(RuntimeModelDescriptorError::new(
                format!("ik_solvers[{solver_index}].iteration_count"),
                RuntimeModelDescriptorErrorKind::InvalidIterationCount,
            ));
        }
        if !solver.limit_angle.is_finite() || solver.limit_angle < 0.0 {
            return Err(RuntimeModelDescriptorError::new(
                format!("ik_solvers[{solver_index}].limit_angle"),
                RuntimeModelDescriptorErrorKind::InvalidLimitAngle,
            ));
        }
        for (link_index, link) in solver.links.iter().enumerate() {
            validate_bone_index(
                format!("ik_solvers[{solver_index}].links[{link_index}].bone"),
                link.bone,
                bone_count,
            )?;
            if let Some(limit) = link.angle_limit {
                let min_path =
                    format!("ik_solvers[{solver_index}].links[{link_index}].angle_limit.min");
                let max_path =
                    format!("ik_solvers[{solver_index}].links[{link_index}].angle_limit.max");
                validate_vec3(min_path, limit.min)?;
                validate_vec3(max_path, limit.max)?;
                if (limit.min.cmple(limit.max)).bitmask() != 0b111 {
                    return Err(RuntimeModelDescriptorError::new(
                        format!("ik_solvers[{solver_index}].links[{link_index}].angle_limit"),
                        RuntimeModelDescriptorErrorKind::InvalidRange,
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_append_transforms(
    appends: &[RuntimeAppendTransformDescriptorV1],
    bone_count: usize,
) -> Result<(), RuntimeModelDescriptorError> {
    let mut targets = std::collections::HashSet::with_capacity(appends.len());
    for (append_index, append) in appends.iter().enumerate() {
        validate_bone_index(
            format!("append_transforms[{append_index}].target_bone"),
            append.target_bone,
            bone_count,
        )?;
        validate_bone_index(
            format!("append_transforms[{append_index}].source_bone"),
            append.source_bone,
            bone_count,
        )?;
        if !append.ratio.is_finite() {
            return Err(RuntimeModelDescriptorError::new(
                format!("append_transforms[{append_index}].ratio"),
                RuntimeModelDescriptorErrorKind::InvalidAppendRatio,
            ));
        }
        if !append.affect_rotation && !append.affect_translation {
            return Err(RuntimeModelDescriptorError::new(
                format!("append_transforms[{append_index}]"),
                RuntimeModelDescriptorErrorKind::InvalidAppendFlags,
            ));
        }
        if !targets.insert(append.target_bone) {
            return Err(RuntimeModelDescriptorError::new(
                format!("append_transforms[{append_index}].target_bone"),
                RuntimeModelDescriptorErrorKind::DuplicateAppendTarget,
            ));
        }
    }
    Ok(())
}

fn compile_morphs(
    descriptor: &RuntimeMorphDescriptorV1,
    bone_count: usize,
) -> Result<MorphInit, RuntimeModelDescriptorError> {
    let morph_count = descriptor.morph_count as usize;
    if morph_count == 0
        && (!descriptor.bone_offsets.is_empty() || !descriptor.group_offsets.is_empty())
    {
        return Err(RuntimeModelDescriptorError::new(
            "morphs.morph_count",
            RuntimeModelDescriptorErrorKind::EmptyMorphSet,
        ));
    }

    let mut bone_counts = try_zeroed_usize_vec(morph_count, "morphs.morph_count")?;
    for (offset_index, offset) in descriptor.bone_offsets.iter().enumerate() {
        validate_morph_index(
            format!("morphs.bone_offsets[{offset_index}].morph_index"),
            offset.morph_index,
            morph_count,
        )?;
        bone_counts[offset.morph_index.as_usize()] = bone_counts[offset.morph_index.as_usize()]
            .checked_add(1)
            .ok_or_else(|| allocation_error("morphs.bone_offsets"))?;
    }
    let mut bone_buckets = try_empty_buckets::<(usize, &RuntimeBoneMorphOffsetDescriptorV1)>(
        bone_counts.len(),
        "morphs.bone_offsets",
    )?;
    for (bucket, count) in bone_buckets.iter_mut().zip(&bone_counts) {
        bucket
            .try_reserve_exact(*count)
            .map_err(|_| allocation_error("morphs.bone_offsets"))?;
    }
    for (offset_index, offset) in descriptor.bone_offsets.iter().enumerate() {
        bone_buckets[offset.morph_index.as_usize()].push((offset_index, offset));
    }
    let mut bone_offsets = Vec::new();
    bone_offsets
        .try_reserve_exact(descriptor.bone_offsets.len())
        .map_err(|_| allocation_error("morphs.bone_offsets"))?;
    let mut bone_spans = try_spans(morph_count, "morphs.bone_spans")?;
    for (morph_index, offsets) in bone_buckets.into_iter().enumerate() {
        let start = checked_offset_start(bone_offsets.len(), morph_index)?;
        for (offset_index, offset) in offsets {
            validate_bone_index(
                format!("morphs.bone_offsets[{offset_index}].target_bone"),
                offset.target_bone,
                bone_count,
            )?;
            validate_vec3(
                format!("morphs.bone_offsets[{offset_index}].position_offset"),
                offset.position_offset,
            )?;
            let rotation_offset = validate_quaternion(
                format!("morphs.bone_offsets[{offset_index}].rotation_offset"),
                offset.rotation_offset,
            )?;
            bone_offsets.push(BoneMorphOffset {
                target_bone: offset.target_bone,
                position_offset: offset.position_offset,
                rotation_offset,
            });
        }
        bone_spans[morph_index] = span_from_range(start, bone_offsets.len())?;
    }

    let mut group_counts = try_zeroed_usize_vec(morph_count, "morphs.morph_count")?;
    for (offset_index, offset) in descriptor.group_offsets.iter().enumerate() {
        validate_morph_index(
            format!("morphs.group_offsets[{offset_index}].morph_index"),
            offset.morph_index,
            morph_count,
        )?;
        validate_morph_index(
            format!("morphs.group_offsets[{offset_index}].child_morph"),
            offset.child_morph,
            morph_count,
        )?;
        if !offset.ratio.is_finite() {
            return Err(RuntimeModelDescriptorError::new(
                format!("morphs.group_offsets[{offset_index}].ratio"),
                RuntimeModelDescriptorErrorKind::NonFinite,
            ));
        }
        group_counts[offset.morph_index.as_usize()] = group_counts[offset.morph_index.as_usize()]
            .checked_add(1)
            .ok_or_else(|| allocation_error("morphs.group_offsets"))?;
    }
    let mut group_buckets = try_empty_buckets::<(usize, &RuntimeGroupMorphOffsetDescriptorV1)>(
        group_counts.len(),
        "morphs.group_offsets",
    )?;
    for (bucket, count) in group_buckets.iter_mut().zip(&group_counts) {
        bucket
            .try_reserve_exact(*count)
            .map_err(|_| allocation_error("morphs.group_offsets"))?;
    }
    for (offset_index, offset) in descriptor.group_offsets.iter().enumerate() {
        group_buckets[offset.morph_index.as_usize()].push((offset_index, offset));
    }
    let mut group_offsets = Vec::new();
    group_offsets
        .try_reserve_exact(descriptor.group_offsets.len())
        .map_err(|_| allocation_error("morphs.group_offsets"))?;
    let mut group_source_indices = Vec::new();
    group_source_indices
        .try_reserve_exact(descriptor.group_offsets.len())
        .map_err(|_| allocation_error("morphs.group_offsets"))?;
    let mut group_spans = try_spans(morph_count, "morphs.group_spans")?;
    for (morph_index, offsets) in group_buckets.into_iter().enumerate() {
        let start = checked_offset_start(group_offsets.len(), morph_index)?;
        for (offset_index, offset) in offsets {
            group_offsets.push(GroupMorphOffset {
                child_morph: offset.child_morph,
                ratio: offset.ratio,
            });
            group_source_indices.push(offset_index);
        }
        group_spans[morph_index] = span_from_range(start, group_offsets.len())?;
    }

    validate_group_morph_cycles(&group_spans, &group_offsets, &group_source_indices)?;

    Ok(MorphInit {
        morph_count: descriptor.morph_count,
        bone_offsets,
        bone_spans,
        group_offsets,
        group_spans,
        ..MorphInit::default()
    })
}

fn validate_group_morph_cycles(
    group_spans: &[MorphOffsetSpan],
    group_offsets: &[GroupMorphOffset],
    group_source_indices: &[usize],
) -> Result<(), RuntimeModelDescriptorError> {
    let mut states = try_zeroed_u8_vec(group_spans.len(), "morphs.group_offsets")?;
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(group_spans.len())
        .map_err(|_| allocation_error("morphs.group_offsets"))?;

    for root in 0..group_spans.len() {
        if states[root] != 0 {
            continue;
        }
        states[root] = 1;
        stack.push((root, 0usize));
        while let Some((morph, next_offset)) = stack.last_mut() {
            let span = group_spans[*morph];
            if *next_offset >= span.count as usize {
                states[*morph] = 2;
                stack.pop();
                continue;
            }
            let offset_index = span.start as usize + *next_offset;
            *next_offset += 1;
            let child = group_offsets[offset_index].child_morph.as_usize();
            match states[child] {
                0 => {
                    states[child] = 1;
                    stack.push((child, 0));
                }
                1 => {
                    let source_index = group_source_indices[offset_index];
                    return Err(RuntimeModelDescriptorError::new(
                        format!("morphs.group_offsets[{source_index}].child_morph"),
                        RuntimeModelDescriptorErrorKind::GroupMorphCycle,
                    ));
                }
                2 => {}
                _ => unreachable!("group morph traversal state is invalid"),
            }
        }
    }
    Ok(())
}

fn allocation_error(path: &str) -> RuntimeModelDescriptorError {
    RuntimeModelDescriptorError::new(path, RuntimeModelDescriptorErrorKind::AllocationFailed)
}

fn try_zeroed_usize_vec(
    length: usize,
    path: &str,
) -> Result<Vec<usize>, RuntimeModelDescriptorError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| allocation_error(path))?;
    values.resize(length, 0);
    Ok(values)
}

fn try_zeroed_u8_vec(length: usize, path: &str) -> Result<Vec<u8>, RuntimeModelDescriptorError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(length)
        .map_err(|_| allocation_error(path))?;
    values.resize(length, 0);
    Ok(values)
}

fn try_empty_buckets<T>(
    length: usize,
    path: &str,
) -> Result<Vec<Vec<T>>, RuntimeModelDescriptorError> {
    let mut buckets = Vec::new();
    buckets
        .try_reserve_exact(length)
        .map_err(|_| allocation_error(path))?;
    buckets.resize_with(length, Vec::new);
    Ok(buckets)
}

fn try_spans(
    length: usize,
    path: &str,
) -> Result<Vec<MorphOffsetSpan>, RuntimeModelDescriptorError> {
    let mut spans = Vec::new();
    spans
        .try_reserve_exact(length)
        .map_err(|_| allocation_error(path))?;
    spans.resize(length, MorphOffsetSpan::default());
    Ok(spans)
}

fn checked_offset_start(
    offset_count: usize,
    morph_index: usize,
) -> Result<u32, RuntimeModelDescriptorError> {
    u32::try_from(offset_count)
        .map_err(|_| allocation_error(&format!("morphs[{morph_index}].offsets")))
}

fn span_from_range(start: u32, end: usize) -> Result<MorphOffsetSpan, RuntimeModelDescriptorError> {
    let end = u32::try_from(end).map_err(|_| allocation_error("morphs.offsets"))?;
    let count = end
        .checked_sub(start)
        .ok_or_else(|| allocation_error("morphs.offsets"))?;
    Ok(MorphOffsetSpan { start, count })
}

fn validate_bone_index(
    path: String,
    value: BoneIndex,
    length: usize,
) -> Result<(), RuntimeModelDescriptorError> {
    if value.as_usize() >= length {
        return Err(RuntimeModelDescriptorError::new(
            path,
            RuntimeModelDescriptorErrorKind::IndexOutOfRange {
                value: value.0,
                length,
            },
        ));
    }
    Ok(())
}

fn validate_morph_index(
    path: String,
    value: MorphIndex,
    length: usize,
) -> Result<(), RuntimeModelDescriptorError> {
    if value.as_usize() >= length {
        return Err(RuntimeModelDescriptorError::new(
            path,
            RuntimeModelDescriptorErrorKind::IndexOutOfRange {
                value: value.0,
                length,
            },
        ));
    }
    Ok(())
}

fn validate_vec3(path: String, value: Vec3A) -> Result<(), RuntimeModelDescriptorError> {
    if !value.is_finite() {
        return Err(RuntimeModelDescriptorError::new(
            path,
            RuntimeModelDescriptorErrorKind::NonFinite,
        ));
    }
    Ok(())
}

fn validate_axis(path: String, value: Vec3A) -> Result<(), RuntimeModelDescriptorError> {
    validate_vec3(path.clone(), value)?;
    let length_squared = value.length_squared();
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return Err(RuntimeModelDescriptorError::new(
            path.clone(),
            RuntimeModelDescriptorErrorKind::DegenerateAxis,
        ));
    }
    let normalized = value.normalize();
    if !normalized.is_finite()
        || !normalized.length_squared().is_finite()
        || normalized.length_squared() <= f32::EPSILON
    {
        return Err(RuntimeModelDescriptorError::new(
            path,
            RuntimeModelDescriptorErrorKind::DegenerateAxis,
        ));
    }
    Ok(())
}

fn validate_quaternion(path: String, value: Quat) -> Result<Quat, RuntimeModelDescriptorError> {
    if !value.is_finite() {
        return Err(RuntimeModelDescriptorError::new(
            path,
            RuntimeModelDescriptorErrorKind::NonFinite,
        ));
    }
    let length_squared = value.length_squared();
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return Err(RuntimeModelDescriptorError::new(
            path,
            RuntimeModelDescriptorErrorKind::DegenerateQuaternion,
        ));
    }
    let normalized = value.normalize();
    if !normalized.is_finite() || normalized.length_squared() <= f32::EPSILON {
        return Err(RuntimeModelDescriptorError::new(
            path,
            RuntimeModelDescriptorErrorKind::DegenerateQuaternion,
        ));
    }
    Ok(normalized)
}

impl fmt::Display for RuntimeModelDescriptorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RuntimeModelDescriptorV1")
            .field("descriptor_version", &self.descriptor_version)
            .field("bones", &self.bones.len())
            .field("ik_solvers", &self.ik_solvers.len())
            .field("append_transforms", &self.append_transforms.len())
            .field("morph_count", &self.morphs.morph_count)
            .finish()
    }
}
