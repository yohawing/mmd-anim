use glam::{Mat4, Quat, Vec3A};

use crate::{MorphIndex, model::BoneIndex};

#[derive(Debug)]
pub struct PoseArena {
    local_position_offsets: Box<[Vec3A]>,
    local_rotations: Box<[Quat]>,
    local_scales: Box<[Vec3A]>,
    append_position_offsets: Box<[Vec3A]>,
    append_rotations: Box<[Quat]>,
    morph_weights: Box<[f32]>,
    ik_enabled: Box<[u8]>,
    world_matrices: Box<[Mat4]>,
    skinning_matrices: Box<[Mat4]>,
}

impl PoseArena {
    pub fn new(bone_count: usize) -> Self {
        Self::new_with_counts(bone_count, 0, 0)
    }

    pub fn new_with_morphs(bone_count: usize, morph_count: usize) -> Self {
        Self::new_with_counts(bone_count, morph_count, 0)
    }

    pub fn new_with_counts(bone_count: usize, morph_count: usize, ik_count: usize) -> Self {
        Self {
            local_position_offsets: vec![Vec3A::ZERO; bone_count].into_boxed_slice(),
            local_rotations: vec![Quat::IDENTITY; bone_count].into_boxed_slice(),
            local_scales: vec![Vec3A::ONE; bone_count].into_boxed_slice(),
            append_position_offsets: vec![Vec3A::ZERO; bone_count].into_boxed_slice(),
            append_rotations: vec![Quat::IDENTITY; bone_count].into_boxed_slice(),
            morph_weights: vec![0.0; morph_count].into_boxed_slice(),
            ik_enabled: vec![1; ik_count].into_boxed_slice(),
            world_matrices: vec![Mat4::IDENTITY; bone_count].into_boxed_slice(),
            skinning_matrices: vec![Mat4::IDENTITY; bone_count].into_boxed_slice(),
        }
    }

    pub fn reset_local_pose(&mut self) {
        self.local_position_offsets.fill(Vec3A::ZERO);
        self.local_rotations.fill(Quat::IDENTITY);
        self.local_scales.fill(Vec3A::ONE);
        self.reset_append_transforms();
        self.morph_weights.fill(0.0);
        self.ik_enabled.fill(1);
    }

    pub(crate) fn reset_append_transforms(&mut self) {
        self.append_position_offsets.fill(Vec3A::ZERO);
        self.append_rotations.fill(Quat::IDENTITY);
    }

    pub(crate) fn reset_append_transform(&mut self, bone: BoneIndex) {
        self.append_position_offsets[bone.as_usize()] = Vec3A::ZERO;
        self.append_rotations[bone.as_usize()] = Quat::IDENTITY;
    }

    #[inline]
    pub fn set_local_position_offset(&mut self, bone: BoneIndex, value: Vec3A) {
        self.local_position_offsets[bone.as_usize()] = value;
    }

    #[inline]
    pub fn set_local_rotation(&mut self, bone: BoneIndex, value: Quat) {
        self.local_rotations[bone.as_usize()] = value;
    }

    #[inline]
    pub fn set_local_scale(&mut self, bone: BoneIndex, value: Vec3A) {
        self.local_scales[bone.as_usize()] = value;
    }

    #[inline]
    pub fn local_position_offset(&self, bone: BoneIndex) -> Vec3A {
        self.local_position_offsets[bone.as_usize()]
    }

    #[inline]
    pub fn local_rotation(&self, bone: BoneIndex) -> Quat {
        self.local_rotations[bone.as_usize()]
    }

    #[inline]
    pub fn local_scale(&self, bone: BoneIndex) -> Vec3A {
        self.local_scales[bone.as_usize()]
    }

    #[inline]
    pub(crate) fn append_position_offset(&self, bone: BoneIndex) -> Vec3A {
        self.append_position_offsets[bone.as_usize()]
    }

    #[inline]
    pub(crate) fn append_rotation(&self, bone: BoneIndex) -> Quat {
        self.append_rotations[bone.as_usize()]
    }

    #[inline]
    pub(crate) fn set_append_position_offset(&mut self, bone: BoneIndex, value: Vec3A) {
        self.append_position_offsets[bone.as_usize()] = value;
    }

    #[inline]
    pub(crate) fn set_append_rotation(&mut self, bone: BoneIndex, value: Quat) {
        self.append_rotations[bone.as_usize()] = value;
    }

    #[inline]
    pub(crate) fn set_world_matrix(&mut self, bone: BoneIndex, value: Mat4) {
        self.world_matrices[bone.as_usize()] = value;
    }

    #[inline]
    pub(crate) fn set_skinning_matrix(&mut self, bone: BoneIndex, value: Mat4) {
        self.skinning_matrices[bone.as_usize()] = value;
    }

    #[inline]
    pub fn set_morph_weight(&mut self, morph: MorphIndex, value: f32) {
        self.morph_weights[morph.as_usize()] = value;
    }

    #[inline]
    pub fn morph_weight(&self, morph: MorphIndex) -> f32 {
        self.morph_weights[morph.as_usize()]
    }

    #[inline]
    pub fn morph_weights(&self) -> &[f32] {
        &self.morph_weights
    }

    #[inline]
    pub fn set_ik_enabled(&mut self, ik_index: usize, enabled: bool) {
        self.ik_enabled[ik_index] = u8::from(enabled);
    }

    #[inline]
    pub fn ik_enabled(&self) -> &[u8] {
        &self.ik_enabled
    }

    #[inline]
    pub fn world_matrices(&self) -> &[Mat4] {
        &self.world_matrices
    }

    #[inline]
    pub fn skinning_matrices(&self) -> &[Mat4] {
        &self.skinning_matrices
    }
}
