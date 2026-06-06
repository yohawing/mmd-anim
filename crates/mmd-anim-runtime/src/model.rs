use glam::{Mat4, Quat, Vec3A};
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

impl BoneIndex {
    #[inline]
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

#[derive(Clone, Debug)]
pub struct BoneInit {
    pub parent: Option<BoneIndex>,
    pub rest_position: Vec3A,
    pub inverse_bind_matrix: Mat4,
    pub transform_order: i32,
    pub fixed_axis: Option<Vec3A>,
}

impl BoneInit {
    pub fn new(parent: Option<BoneIndex>, rest_position: Vec3A) -> Self {
        Self {
            parent,
            rest_position,
            inverse_bind_matrix: Mat4::IDENTITY,
            transform_order: 0,
            fixed_axis: None,
        }
    }

    pub fn with_fixed_axis(mut self, axis: Vec3A) -> Self {
        self.fixed_axis = Some(axis);
        self
    }
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
}

#[derive(Debug)]
pub struct ModelArena {
    parent_indices: Box<[i32]>,
    rest_positions: Box<[Vec3A]>,
    inverse_bind_matrices: Box<[Mat4]>,
    transform_orders: Box<[i32]>,
    fixed_axis_flags: Box<[u8]>,
    fixed_axes: Box<[Vec3A]>,
    eval_order: Box<[BoneIndex]>,
    eval_order_positions: Box<[usize]>,
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
        let mut fixed_axis_flags = Vec::with_capacity(bone_count);
        let mut fixed_axes = Vec::with_capacity(bone_count);

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
            match bone.fixed_axis {
                Some(axis) if axis.length_squared() > f32::EPSILON => {
                    fixed_axis_flags.push(1);
                    fixed_axes.push(axis.normalize());
                }
                _ => {
                    fixed_axis_flags.push(0);
                    fixed_axes.push(Vec3A::X);
                }
            }
        }

        let eval_order = build_eval_order(&parent_indices, &transform_orders)?;
        let eval_order_positions = build_eval_order_positions(&eval_order, bone_count);
        let ik_solvers = build_ik_solvers(ik_solvers, bone_count)?;
        let (append_transforms, append_transform_indices) =
            build_append_transforms(append_transforms, bone_count)?;
        validate_morph_init(&morph, bone_count)?;

        Ok(Self {
            parent_indices: parent_indices.into_boxed_slice(),
            rest_positions: rest_positions.into_boxed_slice(),
            inverse_bind_matrices: inverse_bind_matrices.into_boxed_slice(),
            transform_orders: transform_orders.into_boxed_slice(),
            fixed_axis_flags: fixed_axis_flags.into_boxed_slice(),
            fixed_axes: fixed_axes.into_boxed_slice(),
            eval_order,
            eval_order_positions,
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
    pub fn fixed_axis_count(&self) -> usize {
        self.fixed_axis_flags
            .iter()
            .filter(|&&flag| flag != 0)
            .count()
    }

    #[inline]
    pub fn eval_order(&self) -> &[BoneIndex] {
        &self.eval_order
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

fn build_ik_solvers(
    ik_solvers: Vec<IkSolverInit>,
    bone_count: usize,
) -> Result<Box<[IkSolver]>, ModelBuildError> {
    let mut solvers = Vec::with_capacity(ik_solvers.len());

    for (solver_index, solver) in ik_solvers.into_iter().enumerate() {
        validate_ik_bone(solver_index, "ik", solver.ik_bone, bone_count)?;
        validate_ik_bone(solver_index, "target", solver.target_bone, bone_count)?;

        let mut links = Vec::with_capacity(solver.links.len());
        for link in solver.links {
            validate_ik_bone(solver_index, "link", link.bone, bone_count)?;
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

    Ok(solvers.into_boxed_slice())
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
    match state[bone] {
        VisitState::Visited => return Ok(()),
        VisitState::Visiting => return Err(ModelBuildError::ParentCycle { bone }),
        VisitState::Unvisited => {}
    }

    state[bone] = VisitState::Visiting;

    let parent = parent_indices[bone];
    if parent >= 0 {
        visit_bone(parent as usize, parent_indices, state, order)?;
    }

    state[bone] = VisitState::Visited;
    order.push(BoneIndex(bone as u32));
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
    let mut state = vec![VisitState::Unvisited; morph.morph_count as usize];
    for morph_index in 0..morph.morph_count as usize {
        visit_group_morph(morph_index, morph, &mut state)?;
    }
    Ok(())
}

fn visit_group_morph(
    morph_index: usize,
    morph: &MorphInit,
    state: &mut [VisitState],
) -> Result<(), ModelBuildError> {
    match state[morph_index] {
        VisitState::Visited => return Ok(()),
        VisitState::Visiting => {
            return Err(ModelBuildError::GroupMorphCycle { morph: morph_index });
        }
        VisitState::Unvisited => {}
    }

    state[morph_index] = VisitState::Visiting;
    let span = morph.group_spans[morph_index];
    for offset_index in span.start..span.start + span.count {
        let child = morph.group_offsets[offset_index as usize]
            .child_morph
            .as_usize();
        if morph.group_spans[child].count > 0 {
            visit_group_morph(child, morph, state)?;
        }
    }
    state[morph_index] = VisitState::Visited;
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
