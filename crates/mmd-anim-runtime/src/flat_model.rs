use std::fmt;

use crate::{BoneIndex, BoneInit};

pub struct FlatBoneInput<'a> {
    pub parent_indices: &'a [i32],
    pub rest_positions_xyz: &'a [f32],
    pub inverse_bind_matrices: &'a [f32],
    pub transform_orders: &'a [i32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlatModelInputError {
    EmptyBoneSet,
    RestPositionsLen,
    InverseBindMatricesLen,
    TransformOrdersLen,
    InvalidParentIndex,
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
}
