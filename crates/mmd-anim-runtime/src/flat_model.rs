use std::fmt;

use crate::{
    AppendTransformInit, BoneIndex, BoneInit, BoneMorphOffset, GroupMorphOffset, IkAngleLimit,
    IkLinkInit, IkSolverInit, MorphIndex, MorphInit, MorphOffsetSpan,
};

pub struct FlatBoneInput<'a> {
    pub parent_indices: &'a [i32],
    pub rest_positions_xyz: &'a [f32],
    pub inverse_bind_matrices: &'a [f32],
    pub transform_orders: &'a [i32],
}

#[derive(Debug, Clone, Copy)]
pub struct FlatIkLinkInput {
    pub bone_index: u32,
    pub has_angle_limit: bool,
    pub angle_limit_min_xyz: [f32; 3],
    pub angle_limit_max_xyz: [f32; 3],
}

#[derive(Debug, Clone, Copy)]
pub struct FlatIkSolverInput {
    pub ik_bone_index: u32,
    pub target_bone_index: u32,
    pub link_offset: usize,
    pub link_count: usize,
    pub iteration_count: u32,
    pub limit_angle: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct FlatAppendTransformInput {
    pub target_bone_index: u32,
    pub source_bone_index: u32,
    pub ratio: f32,
    pub affect_rotation: bool,
    pub affect_translation: bool,
    pub local: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct FlatBoneMorphInput {
    pub morph_index: u32,
    pub target_bone_index: u32,
    pub position_offset_xyz: [f32; 3],
    pub rotation_offset_xyzw: [f32; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct FlatGroupMorphInput {
    pub morph_index: u32,
    pub child_morph_index: u32,
    pub ratio: f32,
}

pub struct FlatMorphInput<'a> {
    pub morph_count: u32,
    pub bone_morphs: &'a [FlatBoneMorphInput],
    pub group_morphs: &'a [FlatGroupMorphInput],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatModelInputError {
    EmptyBoneSet,
    RestPositionsLen,
    InverseBindMatricesLen,
    TransformOrdersLen,
    InvalidParentIndex,
    RangeOverflow,
    RangeOutOfBounds,
    MorphCountZeroWithData,
    BoneMorphIndexOutOfRange,
    GroupMorphIndexOutOfRange,
}

impl fmt::Display for FlatModelInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyBoneSet => "model must contain at least one bone",
            Self::RestPositionsLen => "rest_positions_xyz must contain bone_count * 3 values",
            Self::InverseBindMatricesLen => {
                "inverse_bind_matrices must contain bone_count * 16 values"
            }
            Self::TransformOrdersLen => "transform_orders must contain bone_count values",
            Self::InvalidParentIndex => "parent index must be -1 or non-negative",
            Self::RangeOverflow => "range overflow",
            Self::RangeOutOfBounds => "track keyframe range is out of bounds",
            Self::MorphCountZeroWithData => {
                "morph_count must be non-zero when morph data is provided"
            }
            Self::BoneMorphIndexOutOfRange => "bone morph index is out of range",
            Self::GroupMorphIndexOutOfRange => "group morph index is out of range",
        };
        f.write_str(message)
    }
}

impl std::error::Error for FlatModelInputError {}

pub fn build_bones_from_flat(
    input: FlatBoneInput<'_>,
) -> Result<Vec<BoneInit>, FlatModelInputError> {
    if input.parent_indices.is_empty() {
        return Err(FlatModelInputError::EmptyBoneSet);
    }
    if input.rest_positions_xyz.len() != input.parent_indices.len() * 3 {
        return Err(FlatModelInputError::RestPositionsLen);
    }
    if !input.inverse_bind_matrices.is_empty()
        && input.inverse_bind_matrices.len() != input.parent_indices.len() * 16
    {
        return Err(FlatModelInputError::InverseBindMatricesLen);
    }
    if !input.transform_orders.is_empty()
        && input.transform_orders.len() != input.parent_indices.len()
    {
        return Err(FlatModelInputError::TransformOrdersLen);
    }

    let mut bones = Vec::with_capacity(input.parent_indices.len());
    for (bone_index, parent_index) in input.parent_indices.iter().enumerate() {
        let parent = match *parent_index {
            -1 => None,
            parent if parent >= 0 => Some(BoneIndex(parent as u32)),
            _ => return Err(FlatModelInputError::InvalidParentIndex),
        };
        let position_offset = bone_index * 3;
        let mut bone = BoneInit::new(
            parent,
            glam::Vec3A::new(
                input.rest_positions_xyz[position_offset],
                input.rest_positions_xyz[position_offset + 1],
                input.rest_positions_xyz[position_offset + 2],
            ),
        );
        if !input.inverse_bind_matrices.is_empty() {
            let inverse_bind_offset = bone_index * 16;
            let inverse_bind_matrix = input.inverse_bind_matrices
                [inverse_bind_offset..inverse_bind_offset + 16]
                .try_into()
                .expect("validated inverse bind matrix slice length");
            bone.inverse_bind_matrix = glam::Mat4::from_cols_array(inverse_bind_matrix);
        }
        if !input.transform_orders.is_empty() {
            bone.transform_order = input.transform_orders[bone_index];
        }
        bones.push(bone);
    }

    Ok(bones)
}

pub fn build_ik_solvers_from_flat(
    solvers: &[FlatIkSolverInput],
    links: &[FlatIkLinkInput],
) -> Result<Vec<IkSolverInit>, FlatModelInputError> {
    solvers
        .iter()
        .map(|solver| {
            let link_end = solver
                .link_offset
                .checked_add(solver.link_count)
                .ok_or(FlatModelInputError::RangeOverflow)?;
            let solver_links = links
                .get(solver.link_offset..link_end)
                .ok_or(FlatModelInputError::RangeOutOfBounds)?
                .iter()
                .map(|link| {
                    let mut init = IkLinkInit::new(BoneIndex(link.bone_index));
                    if link.has_angle_limit {
                        init = init.with_angle_limit(IkAngleLimit::new(
                            glam::Vec3A::new(
                                link.angle_limit_min_xyz[0],
                                link.angle_limit_min_xyz[1],
                                link.angle_limit_min_xyz[2],
                            ),
                            glam::Vec3A::new(
                                link.angle_limit_max_xyz[0],
                                link.angle_limit_max_xyz[1],
                                link.angle_limit_max_xyz[2],
                            ),
                        ));
                    }
                    init
                })
                .collect();

            Ok(IkSolverInit {
                ik_bone: BoneIndex(solver.ik_bone_index),
                target_bone: BoneIndex(solver.target_bone_index),
                links: solver_links,
                iteration_count: solver.iteration_count,
                limit_angle: solver.limit_angle,
            })
        })
        .collect()
}

pub fn build_morph_init_from_flat(
    input: FlatMorphInput<'_>,
) -> Result<MorphInit, FlatModelInputError> {
    if input.morph_count == 0 {
        if input.bone_morphs.is_empty() && input.group_morphs.is_empty() {
            return Ok(MorphInit::default());
        }
        return Err(FlatModelInputError::MorphCountZeroWithData);
    }
    let morph_count = input.morph_count as usize;
    let (bone_offsets, bone_spans) =
        build_bone_morph_offset_tables(morph_count, input.bone_morphs)?;
    let (group_offsets, group_spans) =
        build_group_morph_offset_tables(morph_count, input.group_morphs)?;
    Ok(MorphInit {
        morph_count: input.morph_count,
        bone_offsets,
        bone_spans,
        group_offsets,
        group_spans,
        ..MorphInit::default()
    })
}

fn build_bone_morph_offset_tables(
    morph_count: usize,
    bone_morphs: &[FlatBoneMorphInput],
) -> Result<(Vec<BoneMorphOffset>, Vec<MorphOffsetSpan>), FlatModelInputError> {
    if bone_morphs.is_empty() {
        return Ok((Vec::new(), vec![MorphOffsetSpan::default(); morph_count]));
    }

    let mut sorted: Vec<&FlatBoneMorphInput> = bone_morphs.iter().collect();
    sorted.sort_by_key(|entry| entry.morph_index);
    if sorted.last().unwrap().morph_index as usize >= morph_count {
        return Err(FlatModelInputError::BoneMorphIndexOutOfRange);
    }

    let mut offsets = Vec::with_capacity(bone_morphs.len());
    let mut spans = vec![MorphOffsetSpan::default(); morph_count];
    let mut index = 0;
    while index < sorted.len() {
        let morph = sorted[index].morph_index as usize;
        let start = offsets.len() as u32;
        let mut count = 0u32;
        while index < sorted.len() && sorted[index].morph_index as usize == morph {
            let entry = sorted[index];
            offsets.push(BoneMorphOffset {
                target_bone: BoneIndex(entry.target_bone_index),
                position_offset: glam::Vec3A::new(
                    entry.position_offset_xyz[0],
                    entry.position_offset_xyz[1],
                    entry.position_offset_xyz[2],
                ),
                rotation_offset: glam::Quat::from_xyzw(
                    entry.rotation_offset_xyzw[0],
                    entry.rotation_offset_xyzw[1],
                    entry.rotation_offset_xyzw[2],
                    entry.rotation_offset_xyzw[3],
                ),
            });
            count += 1;
            index += 1;
        }
        spans[morph] = MorphOffsetSpan { start, count };
    }

    Ok((offsets, spans))
}

fn build_group_morph_offset_tables(
    morph_count: usize,
    group_morphs: &[FlatGroupMorphInput],
) -> Result<(Vec<GroupMorphOffset>, Vec<MorphOffsetSpan>), FlatModelInputError> {
    if group_morphs.is_empty() {
        return Ok((Vec::new(), vec![MorphOffsetSpan::default(); morph_count]));
    }

    let mut sorted: Vec<&FlatGroupMorphInput> = group_morphs.iter().collect();
    sorted.sort_by_key(|entry| entry.morph_index);
    if sorted.last().unwrap().morph_index as usize >= morph_count {
        return Err(FlatModelInputError::GroupMorphIndexOutOfRange);
    }

    let mut offsets = Vec::with_capacity(group_morphs.len());
    let mut spans = vec![MorphOffsetSpan::default(); morph_count];
    let mut index = 0;
    while index < sorted.len() {
        let morph = sorted[index].morph_index as usize;
        let start = offsets.len() as u32;
        let mut count = 0u32;
        while index < sorted.len() && sorted[index].morph_index as usize == morph {
            let entry = sorted[index];
            offsets.push(GroupMorphOffset {
                child_morph: MorphIndex(entry.child_morph_index),
                ratio: entry.ratio,
            });
            count += 1;
            index += 1;
        }
        spans[morph] = MorphOffsetSpan { start, count };
    }

    Ok((offsets, spans))
}

pub fn build_append_transforms_from_flat(
    append_transforms: &[FlatAppendTransformInput],
) -> Vec<AppendTransformInit> {
    append_transforms
        .iter()
        .map(|append| {
            let mut init = AppendTransformInit::new(
                BoneIndex(append.target_bone_index),
                BoneIndex(append.source_bone_index),
                append.ratio,
            );
            if append.affect_rotation {
                init = init.with_rotation();
            }
            if append.affect_translation {
                init = init.with_translation();
            }
            if append.local {
                init = init.with_local();
            }
            init
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_bones_from_flat_arrays() {
        let bones = build_bones_from_flat(FlatBoneInput {
            parent_indices: &[-1, 0],
            rest_positions_xyz: &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            inverse_bind_matrices: &[],
            transform_orders: &[2, 1],
        })
        .unwrap();

        assert_eq!(bones.len(), 2);
        assert_eq!(bones[0].parent, None);
        assert_eq!(bones[1].parent, Some(BoneIndex(0)));
        assert_eq!(bones[1].rest_position.to_array(), [3.0, 4.0, 5.0]);
        assert_eq!(bones[0].transform_order, 2);
        assert_eq!(bones[1].transform_order, 1);
    }

    #[test]
    fn rejects_invalid_flat_bone_arrays() {
        let error = build_bones_from_flat(FlatBoneInput {
            parent_indices: &[0],
            rest_positions_xyz: &[0.0, 1.0],
            inverse_bind_matrices: &[],
            transform_orders: &[],
        })
        .unwrap_err();

        assert_eq!(error, FlatModelInputError::RestPositionsLen);
        assert_eq!(
            error.to_string(),
            "rest_positions_xyz must contain bone_count * 3 values"
        );
    }

    #[test]
    fn builds_ik_solvers_from_flat_arrays() {
        let solvers = build_ik_solvers_from_flat(
            &[FlatIkSolverInput {
                ik_bone_index: 3,
                target_bone_index: 2,
                link_offset: 0,
                link_count: 1,
                iteration_count: 10,
                limit_angle: 0.5,
            }],
            &[FlatIkLinkInput {
                bone_index: 1,
                has_angle_limit: true,
                angle_limit_min_xyz: [-1.0, -2.0, -3.0],
                angle_limit_max_xyz: [1.0, 2.0, 3.0],
            }],
        )
        .unwrap();

        assert_eq!(solvers.len(), 1);
        assert_eq!(solvers[0].ik_bone, BoneIndex(3));
        assert_eq!(solvers[0].target_bone, BoneIndex(2));
        assert_eq!(solvers[0].links.len(), 1);
        assert!(solvers[0].links[0].angle_limit.is_some());
    }

    #[test]
    fn builds_append_transforms_from_flat_arrays() {
        let append_transforms = build_append_transforms_from_flat(&[FlatAppendTransformInput {
            target_bone_index: 2,
            source_bone_index: 1,
            ratio: 0.25,
            affect_rotation: true,
            affect_translation: false,
            local: true,
        }]);

        assert_eq!(append_transforms.len(), 1);
        assert_eq!(append_transforms[0].target_bone, BoneIndex(2));
        assert_eq!(append_transforms[0].source_bone, BoneIndex(1));
        assert_eq!(append_transforms[0].ratio, 0.25);
        assert!(append_transforms[0].affect_rotation);
        assert!(!append_transforms[0].affect_translation);
        assert!(append_transforms[0].local);
    }

    #[test]
    fn builds_morph_init_from_flat_arrays() {
        let morph = build_morph_init_from_flat(FlatMorphInput {
            morph_count: 2,
            bone_morphs: &[FlatBoneMorphInput {
                morph_index: 1,
                target_bone_index: 0,
                position_offset_xyz: [1.0, 2.0, 3.0],
                rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
            }],
            group_morphs: &[],
        })
        .unwrap();

        assert_eq!(morph.morph_count, 2);
        assert_eq!(morph.bone_offsets.len(), 1);
        assert_eq!(morph.bone_spans.len(), 2);
        assert_eq!(morph.bone_spans[0], MorphOffsetSpan::default());
        assert_eq!(morph.bone_spans[1], MorphOffsetSpan { start: 0, count: 1 });
        assert_eq!(morph.bone_offsets[0].target_bone, BoneIndex(0));
    }

    #[test]
    fn rejects_out_of_range_bone_morph_index() {
        let error = build_morph_init_from_flat(FlatMorphInput {
            morph_count: 1,
            bone_morphs: &[FlatBoneMorphInput {
                morph_index: 1,
                target_bone_index: 0,
                position_offset_xyz: [0.0, 0.0, 0.0],
                rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
            }],
            group_morphs: &[],
        })
        .unwrap_err();

        assert_eq!(error, FlatModelInputError::BoneMorphIndexOutOfRange);
    }

    #[test]
    fn rejects_zero_morph_count_with_data() {
        let error = build_morph_init_from_flat(FlatMorphInput {
            morph_count: 0,
            bone_morphs: &[FlatBoneMorphInput {
                morph_index: 0,
                target_bone_index: 0,
                position_offset_xyz: [0.0, 0.0, 0.0],
                rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
            }],
            group_morphs: &[],
        })
        .unwrap_err();

        assert_eq!(error, FlatModelInputError::MorphCountZeroWithData);
        assert_eq!(
            error.to_string(),
            "morph_count must be non-zero when morph data is provided"
        );
    }
}
