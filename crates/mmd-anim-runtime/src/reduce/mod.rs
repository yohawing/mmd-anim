use std::cmp::Ordering;
use std::time::{Duration, Instant};

use glam::{EulerRot, Mat3, Mat4, Quat, Vec3, Vec3A};
#[cfg(not(target_family = "wasm"))]
#[cfg(not(target_family = "wasm"))]
use rayon::prelude::*;
#[cfg(not(target_family = "wasm"))]
use rayon::{ThreadPool, ThreadPoolBuilder};
use thiserror::Error;

use crate::{BoneIndex, InterpolationScalar, ModelArena};

const AFFINE_EPSILON: f32 = 1.0e-4;
// Below this point the one-time prefit costs more than the global passes it removes.
const DCC_LOCAL_PREFIT_MIN_FRAMES: usize = 90;

#[derive(Debug, Clone, Copy)]
pub struct DensePoseSequenceView<'a> {
    world_matrices: &'a [Mat4],
    morph_weights: &'a [f32],
    frame_count: usize,
    bone_count: usize,
    morph_count: usize,
    start_frame: f32,
    frame_step: f32,
}

impl<'a> DensePoseSequenceView<'a> {
    pub fn new(
        world_matrices: &'a [Mat4],
        morph_weights: &'a [f32],
        frame_count: usize,
        bone_count: usize,
        morph_count: usize,
        start_frame: f32,
        frame_step: f32,
    ) -> Result<Self, PoseReductionError> {
        if frame_count == 0 {
            return Err(PoseReductionError::EmptySequence);
        }
        if bone_count == 0 {
            return Err(PoseReductionError::EmptySkeleton);
        }
        if !start_frame.is_finite() || !frame_step.is_finite() || frame_step <= 0.0 {
            return Err(PoseReductionError::InvalidTimeBase);
        }
        let mut previous_frame = start_frame;
        for sample_index in 1..frame_count {
            let frame = start_frame + sample_index as f32 * frame_step;
            if !frame.is_finite() || frame <= previous_frame {
                return Err(PoseReductionError::InvalidTimeBase);
            }
            previous_frame = frame;
        }
        let expected_world = frame_count
            .checked_mul(bone_count)
            .ok_or(PoseReductionError::LengthOverflow)?;
        let expected_morph = frame_count
            .checked_mul(morph_count)
            .ok_or(PoseReductionError::LengthOverflow)?;
        if world_matrices.len() != expected_world {
            return Err(PoseReductionError::InvalidWorldMatrixCount {
                actual: world_matrices.len(),
                expected: expected_world,
            });
        }
        if morph_weights.len() != expected_morph {
            return Err(PoseReductionError::InvalidMorphWeightCount {
                actual: morph_weights.len(),
                expected: expected_morph,
            });
        }
        Ok(Self {
            world_matrices,
            morph_weights,
            frame_count,
            bone_count,
            morph_count,
            start_frame,
            frame_step,
        })
    }

    pub fn frame_count(&self) -> usize {
        self.frame_count
    }
    pub fn bone_count(&self) -> usize {
        self.bone_count
    }
    pub fn morph_count(&self) -> usize {
        self.morph_count
    }
    pub fn start_frame(&self) -> f32 {
        self.start_frame
    }
    pub fn frame_step(&self) -> f32 {
        self.frame_step
    }

    fn world_matrix(&self, frame: usize, bone: usize) -> Mat4 {
        self.world_matrices[frame * self.bone_count + bone]
    }

    fn morph_weight(&self, frame: usize, morph: usize) -> f32 {
        self.morph_weights[frame * self.morph_count + morph]
    }

    fn sample_frame(&self, sample_index: usize) -> f32 {
        self.start_frame + sample_index as f32 * self.frame_step
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkeletonSnapshot {
    parent_indices: Box<[i32]>,
    rest_local_translations: Box<[Vec3A]>,
    rest_local_rotations: Box<[Quat]>,
    evaluation_order: Box<[usize]>,
    morph_count: usize,
    model_identity: u64,
}

impl SkeletonSnapshot {
    pub fn new(
        parent_indices: Vec<i32>,
        rest_local_translations: Vec<Vec3A>,
        rest_local_rotations: Vec<Quat>,
        morph_count: usize,
        model_identity: u64,
    ) -> Result<Self, PoseReductionError> {
        if parent_indices.is_empty() {
            return Err(PoseReductionError::EmptySkeleton);
        }
        if rest_local_translations.len() != parent_indices.len()
            || rest_local_rotations.len() != parent_indices.len()
        {
            return Err(PoseReductionError::InvalidSkeletonLengths);
        }
        for (bone, &parent) in parent_indices.iter().enumerate() {
            if parent < -1 || parent >= parent_indices.len() as i32 || parent == bone as i32 {
                return Err(PoseReductionError::InvalidParent { bone, parent });
            }
            if !rest_local_translations[bone].is_finite()
                || !quat_is_finite(rest_local_rotations[bone])
            {
                return Err(PoseReductionError::NonFiniteSkeleton { bone });
            }
        }
        let evaluation_order = build_evaluation_order(&parent_indices)?;
        Ok(Self {
            parent_indices: parent_indices.into_boxed_slice(),
            rest_local_translations: rest_local_translations.into_boxed_slice(),
            rest_local_rotations: rest_local_rotations
                .into_iter()
                .map(normalize_quat)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            evaluation_order: evaluation_order.into_boxed_slice(),
            morph_count,
            model_identity,
        })
    }

    pub fn from_model(model: &ModelArena, model_identity: u64) -> Result<Self, PoseReductionError> {
        Self::from_model_with_morph_count(model, model_identity, model.morph_count() as usize)
    }

    pub fn from_model_with_morph_count(
        model: &ModelArena,
        model_identity: u64,
        morph_count: usize,
    ) -> Result<Self, PoseReductionError> {
        let mut parents = Vec::with_capacity(model.bone_count());
        let mut translations = Vec::with_capacity(model.bone_count());
        for bone in 0..model.bone_count() {
            let bone_index = BoneIndex(bone as u32);
            let parent = model.parent_index(bone_index);
            parents.push(parent.map_or(-1, |value| value.0 as i32));
            let parent_position = parent.map_or(Vec3A::ZERO, |value| model.rest_position(value));
            translations.push(model.rest_position(bone_index) - parent_position);
        }
        Self::new(
            parents,
            translations,
            vec![Quat::IDENTITY; model.bone_count()],
            morph_count,
            model_identity,
        )
    }

    pub fn bone_count(&self) -> usize {
        self.parent_indices.len()
    }
    pub fn morph_count(&self) -> usize {
        self.morph_count
    }
    pub fn model_identity(&self) -> u64 {
        self.model_identity
    }
    pub fn parent_indices(&self) -> &[i32] {
        &self.parent_indices
    }
    pub fn rest_local_translations(&self) -> &[Vec3A] {
        &self.rest_local_translations
    }
    pub fn rest_local_rotations(&self) -> &[Quat] {
        &self.rest_local_rotations
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReductionTarget {
    LinearSlerp,
    VmdBezier,
    DccCubic,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DccCubicSegment {
    pub translation_out_tangent: Vec3A,
    pub translation_in_tangent: Vec3A,
    pub rotation_start_euler_xyz: Vec3A,
    pub rotation_end_euler_xyz: Vec3A,
    pub rotation_out_tangent: Vec3A,
    pub rotation_in_tangent: Vec3A,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DccScalarSegment {
    pub out_tangent: f32,
    pub in_tangent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuantizedBezier {
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
}

impl QuantizedBezier {
    pub const LINEAR: Self = Self {
        x1: 20,
        y1: 20,
        x2: 107,
        y2: 107,
    };

    pub fn evaluate(self, time: f32) -> f32 {
        InterpolationScalar {
            x1: self.x1.min(127),
            y1: self.y1.min(127),
            x2: self.x2.min(127),
            y2: self.y2.min(127),
        }
        .evaluate(time)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmdBoneInterpolation {
    pub translation: [QuantizedBezier; 3],
    pub rotation: QuantizedBezier,
}

impl VmdBoneInterpolation {
    pub const LINEAR: Self = Self {
        translation: [QuantizedBezier::LINEAR; 3],
        rotation: QuantizedBezier::LINEAR,
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReductionTolerances {
    pub local_position: f32,
    pub local_rotation_radians: f32,
    pub world_position: f32,
    pub world_rotation_radians: f32,
    pub morph_weight: f32,
}

impl ReductionTolerances {
    fn validate(self) -> Result<Self, PoseReductionError> {
        let values = [
            self.local_position,
            self.local_rotation_radians,
            self.world_position,
            self.world_rotation_radians,
            self.morph_weight,
        ];
        if values
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            return Err(PoseReductionError::InvalidTolerance);
        }
        Ok(self)
    }
}

impl Default for ReductionTolerances {
    fn default() -> Self {
        Self {
            local_position: 1.0e-4,
            local_rotation_radians: 1.0e-4,
            world_position: 1.0e-4,
            world_rotation_radians: 1.0e-4,
            morph_weight: 1.0e-4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReducedBoneKey {
    pub sample_index: usize,
    pub translation: Vec3A,
    pub rotation: Quat,
    pub vmd_interpolation: VmdBoneInterpolation,
    pub dcc_segment: DccCubicSegment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReducedBoneTrack {
    keys: Box<[ReducedBoneKey]>,
}

impl ReducedBoneTrack {
    pub fn keys(&self) -> &[ReducedBoneKey] {
        &self.keys
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReducedMorphKey {
    pub sample_index: usize,
    pub weight: f32,
    pub dcc_segment: DccScalarSegment,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReducedMorphTrack {
    keys: Box<[ReducedMorphKey]>,
}

impl ReducedMorphTrack {
    pub fn keys(&self) -> &[ReducedMorphKey] {
        &self.keys
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PoseReductionReport {
    pub source_bone_key_count: usize,
    pub reduced_bone_key_count: usize,
    pub source_morph_key_count: usize,
    pub reduced_morph_key_count: usize,
    pub max_local_position_error: f32,
    pub max_local_rotation_error_radians: f32,
    pub max_world_position_error: f32,
    pub max_world_rotation_error_radians: f32,
    pub max_morph_weight_error: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReductionWorkStats {
    pub global_validation_passes: usize,
    pub candidate_rebuilds: usize,
    pub candidate_bone_track_rebuilds: usize,
    pub candidate_morph_track_rebuilds: usize,
    pub local_prefit_bone_segment_fits: usize,
    pub local_prefit_morph_segment_fits: usize,
    pub local_prefit_bone_key_additions: usize,
    pub local_prefit_morph_key_additions: usize,
    pub local_prefit_bone_samples: usize,
    pub local_prefit_morph_samples: usize,
    pub dcc_bone_segment_fits: usize,
    pub dcc_morph_segment_fits: usize,
    pub bone_samples: usize,
    pub morph_samples: usize,
    pub world_rebuilds: usize,
    /// Number of cached candidate world transforms recomputed during validation.
    pub world_bone_recomputes: usize,
    pub world_rotation_decompositions: usize,
    pub normal_key_additions: usize,
    pub ancestor_key_additions: usize,
    pub added_keys_per_pass: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidationMode {
    Incremental,
    FullScan,
}

#[derive(Debug, Clone)]
struct DirtyRanges {
    bone_local: Vec<Vec<std::ops::RangeInclusive<usize>>>,
    morph: Vec<Vec<std::ops::RangeInclusive<usize>>>,
}

impl DirtyRanges {
    fn full(frame_count: usize, bone_count: usize, morph_count: usize) -> Self {
        let range = || vec![0..=frame_count.saturating_sub(1)];
        Self {
            bone_local: (0..bone_count).map(|_| range()).collect(),
            morph: (0..morph_count).map(|_| range()).collect(),
        }
    }

    fn empty(bone_count: usize, morph_count: usize) -> Self {
        Self {
            bone_local: vec![Vec::new(); bone_count],
            morph: vec![Vec::new(); morph_count],
        }
    }

    fn mark_bone(&mut self, bone: usize, range: std::ops::RangeInclusive<usize>) {
        insert_dirty_range(&mut self.bone_local[bone], range);
    }

    fn mark_morph(&mut self, morph: usize, range: std::ops::RangeInclusive<usize>) {
        insert_dirty_range(&mut self.morph[morph], range);
    }
}

fn insert_dirty_range(
    target: &mut Vec<std::ops::RangeInclusive<usize>>,
    range: std::ops::RangeInclusive<usize>,
) {
    target.push(range);
    target.sort_by_key(|value| *value.start());
    let mut merged: Vec<std::ops::RangeInclusive<usize>> = Vec::with_capacity(target.len());
    for current in target.drain(..) {
        if let Some(previous) = merged.last_mut()
            && *current.start() <= previous.end().saturating_add(1)
        {
            let start = *previous.start();
            *previous = start..=(*previous.end()).max(*current.end());
        } else {
            merged.push(current);
        }
    }
    *target = merged;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReductionTimings {
    pub local_prefit: Duration,
    pub candidate_build: Duration,
    pub error_measure: Duration,
    pub dcc_fit: Duration,
}

#[cfg(not(target_family = "wasm"))]
type ReductionThreadPool = ThreadPool;

#[cfg(target_family = "wasm")]
struct ReductionThreadPool;

#[derive(Debug, Clone)]
pub struct ReducedPoseSequence {
    snapshot: SkeletonSnapshot,
    target: ReductionTarget,
    start_frame: f32,
    frame_step: f32,
    frame_count: usize,
    sample_frames: Box<[f32]>,
    bone_tracks: Box<[ReducedBoneTrack]>,
    morph_tracks: Box<[ReducedMorphTrack]>,
    report: PoseReductionReport,
    work_stats: ReductionWorkStats,
    timings: ReductionTimings,
}

impl PartialEq for ReducedPoseSequence {
    fn eq(&self, other: &Self) -> bool {
        self.snapshot == other.snapshot
            && self.target == other.target
            && self.start_frame == other.start_frame
            && self.frame_step == other.frame_step
            && self.frame_count == other.frame_count
            && self.sample_frames == other.sample_frames
            && self.bone_tracks == other.bone_tracks
            && self.morph_tracks == other.morph_tracks
            && self.report == other.report
            && self.work_stats == other.work_stats
    }
}

impl ReducedPoseSequence {
    pub fn snapshot(&self) -> &SkeletonSnapshot {
        &self.snapshot
    }
    pub fn target(&self) -> ReductionTarget {
        self.target
    }
    pub fn start_frame(&self) -> f32 {
        self.start_frame
    }
    pub fn frame_step(&self) -> f32 {
        self.frame_step
    }
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }
    pub fn sample_frames(&self) -> &[f32] {
        &self.sample_frames
    }
    pub fn bone_tracks(&self) -> &[ReducedBoneTrack] {
        &self.bone_tracks
    }
    pub fn morph_tracks(&self) -> &[ReducedMorphTrack] {
        &self.morph_tracks
    }
    pub fn report(&self) -> PoseReductionReport {
        self.report
    }
    pub fn work_stats(&self) -> &ReductionWorkStats {
        &self.work_stats
    }
    pub fn timings(&self) -> ReductionTimings {
        self.timings
    }

    pub fn sample(&self, frame: f32) -> Result<ReducedPoseSample, PoseReductionError> {
        let mut scratch = ReducedPoseScratch::default();
        self.sample_into(frame, &mut scratch)?;
        Ok(ReducedPoseSample {
            local_translations: scratch.local_translations,
            local_rotations: scratch.local_rotations,
            world_matrices: scratch.world_matrices,
            morph_weights: scratch.morph_weights,
        })
    }

    fn sample_into(
        &self,
        frame: f32,
        scratch: &mut ReducedPoseScratch,
    ) -> Result<(), PoseReductionError> {
        if !frame.is_finite() {
            return Err(PoseReductionError::InvalidSampleTime);
        }
        scratch.prepare(self.snapshot.bone_count(), self.snapshot.morph_count());
        for (bone, track) in self.bone_tracks.iter().enumerate() {
            let (translation, rotation) =
                sample_bone_track(track, &self.sample_frames, frame, self.target);
            scratch.local_translations[bone] = translation;
            scratch.local_rotations[bone] = rotation;
        }
        for (morph, track) in self.morph_tracks.iter().enumerate() {
            scratch.morph_weights[morph] =
                sample_morph_track(track, &self.sample_frames, frame, self.target);
        }
        build_world_matrices_into(
            &self.snapshot,
            &scratch.local_translations,
            &scratch.local_rotations,
            &mut scratch.world_matrices,
        );
        Ok(())
    }

    pub fn validate_model(
        &self,
        model_identity: u64,
        bone_count: usize,
        morph_count: usize,
    ) -> bool {
        self.snapshot.model_identity == model_identity
            && self.snapshot.bone_count() == bone_count
            && self.snapshot.morph_count == morph_count
    }
}

#[derive(Debug, Default)]
struct ReducedPoseScratch {
    local_translations: Vec<Vec3A>,
    local_rotations: Vec<Quat>,
    world_matrices: Vec<Mat4>,
    morph_weights: Vec<f32>,
}

impl ReducedPoseScratch {
    fn prepare(&mut self, bone_count: usize, morph_count: usize) {
        self.local_translations.resize(bone_count, Vec3A::ZERO);
        self.local_rotations.resize(bone_count, Quat::IDENTITY);
        self.world_matrices.resize(bone_count, Mat4::IDENTITY);
        self.morph_weights.resize(morph_count, 0.0);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReducedPoseSample {
    pub local_translations: Vec<Vec3A>,
    pub local_rotations: Vec<Quat>,
    pub world_matrices: Vec<Mat4>,
    pub morph_weights: Vec<f32>,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum PoseReductionError {
    #[error("dense pose sequence must contain at least one frame")]
    EmptySequence,
    #[error("skeleton must contain at least one bone")]
    EmptySkeleton,
    #[error("invalid or non-positive dense pose time base")]
    InvalidTimeBase,
    #[error("dense pose buffer length overflow")]
    LengthOverflow,
    #[error("world matrix count {actual} does not match expected {expected}")]
    InvalidWorldMatrixCount { actual: usize, expected: usize },
    #[error("morph weight count {actual} does not match expected {expected}")]
    InvalidMorphWeightCount { actual: usize, expected: usize },
    #[error("skeleton arrays have inconsistent lengths")]
    InvalidSkeletonLengths,
    #[error("bone {bone} has invalid parent index {parent}")]
    InvalidParent { bone: usize, parent: i32 },
    #[error("skeleton hierarchy contains a cycle at bone {bone}")]
    SkeletonCycle { bone: usize },
    #[error("bone {bone} has non-finite rest data")]
    NonFiniteSkeleton { bone: usize },
    #[error("dense pose skeleton counts do not match snapshot")]
    SnapshotMismatch,
    #[error("reduction tolerance must be finite and non-negative")]
    InvalidTolerance,
    #[error("frame {frame}, bone {bone} contains a non-finite matrix")]
    NonFiniteMatrix { frame: usize, bone: usize },
    #[error("frame {frame}, bone {bone} contains scale or shear")]
    ScaleOrShear { frame: usize, bone: usize },
    #[error("frame {frame}, bone {bone} has a singular parent transform")]
    SingularParent { frame: usize, bone: usize },
    #[error("frame {frame}, morph {morph} contains a non-finite weight")]
    NonFiniteMorph { frame: usize, morph: usize },
    #[error("sample time must be finite")]
    InvalidSampleTime,
    #[error("requested tolerance cannot be attained at source frame {frame}")]
    ToleranceUnattainable { frame: usize },
    #[error("failed to create pose reduction worker pool")]
    WorkerPool,
}

pub fn reduce_dense_pose_sequence(
    input: DensePoseSequenceView<'_>,
    snapshot: SkeletonSnapshot,
    tolerances: ReductionTolerances,
    target: ReductionTarget,
) -> Result<ReducedPoseSequence, PoseReductionError> {
    reduce_dense_pose_sequence_with_worker_count(input, snapshot, tolerances, target, 0)
}

pub fn reduce_dense_pose_sequence_with_worker_count(
    input: DensePoseSequenceView<'_>,
    snapshot: SkeletonSnapshot,
    tolerances: ReductionTolerances,
    target: ReductionTarget,
    worker_count: usize,
) -> Result<ReducedPoseSequence, PoseReductionError> {
    reduce_dense_pose_sequence_internal(
        input,
        snapshot,
        tolerances,
        target,
        worker_count,
        ValidationMode::Incremental,
    )
}

fn reduce_dense_pose_sequence_internal(
    input: DensePoseSequenceView<'_>,
    snapshot: SkeletonSnapshot,
    tolerances: ReductionTolerances,
    target: ReductionTarget,
    worker_count: usize,
    validation_mode: ValidationMode,
) -> Result<ReducedPoseSequence, PoseReductionError> {
    let tolerances = tolerances.validate()?;
    if input.bone_count != snapshot.bone_count() || input.morph_count != snapshot.morph_count() {
        return Err(PoseReductionError::SnapshotMismatch);
    }

    let (local_translations, local_rotations) = decompose_dense_pose(input, &snapshot)?;
    let (world_positions, world_rotations) = cache_dense_world_components(input)?;
    let mut work_stats = ReductionWorkStats {
        world_rotation_decompositions: input.frame_count * input.bone_count,
        ..Default::default()
    };
    let mut timings = ReductionTimings::default();
    let worker_count = resolve_reduction_worker_count(worker_count, input.frame_count);
    let worker_pool = build_reduction_worker_pool(worker_count)?;
    let local_euler_xyz = unwrap_euler_xyz(&local_rotations, input.frame_count, input.bone_count);
    for frame in 0..input.frame_count {
        for morph in 0..input.morph_count {
            if !input.morph_weight(frame, morph).is_finite() {
                return Err(PoseReductionError::NonFiniteMorph { frame, morph });
            }
        }
    }

    let mut bone_key_indices = vec![endpoint_indices(input.frame_count); input.bone_count];
    let mut morph_key_indices = vec![endpoint_indices(input.frame_count); input.morph_count];

    if target == ReductionTarget::DccCubic && input.frame_count >= DCC_LOCAL_PREFIT_MIN_FRAMES {
        let dcc_prefit_started = Instant::now();
        for (bone, keys) in bone_key_indices.iter_mut().enumerate() {
            let prefit = split_dcc_bone_track(
                keys,
                input,
                bone,
                &local_translations,
                &local_rotations,
                &local_euler_xyz,
                tolerances,
            );
            work_stats.local_prefit_bone_segment_fits += prefit.segment_fits;
            work_stats.local_prefit_bone_key_additions += prefit.key_additions;
            work_stats.local_prefit_bone_samples += prefit.samples;
        }
        for (morph, keys) in morph_key_indices.iter_mut().enumerate() {
            let prefit = split_dcc_morph_track(keys, input, morph, tolerances.morph_weight);
            work_stats.local_prefit_morph_segment_fits += prefit.segment_fits;
            work_stats.local_prefit_morph_key_additions += prefit.key_additions;
            work_stats.local_prefit_morph_samples += prefit.samples;
        }
        timings.local_prefit += dcc_prefit_started.elapsed();
    } else if target == ReductionTarget::LinearSlerp {
        for (bone, keys) in bone_key_indices.iter_mut().enumerate() {
            split_bone_track(
                keys,
                input,
                bone,
                &local_translations,
                &local_rotations,
                tolerances,
            );
        }
    }
    if target != ReductionTarget::DccCubic {
        for (morph, keys) in morph_key_indices.iter_mut().enumerate() {
            split_morph_track(keys, input, morph, tolerances.morph_weight);
        }
    }

    let mut candidate: Option<ReducedPoseSequence> = None;
    let mut dirty = DirtyRanges::full(input.frame_count, input.bone_count, input.morph_count);
    let mut validation_cache =
        ValidationCache::new(input.frame_count, input.bone_count, input.morph_count);
    loop {
        work_stats.global_validation_passes += 1;
        work_stats.candidate_rebuilds += 1;
        if candidate.is_some() && matches!(validation_mode, ValidationMode::FullScan) {
            dirty = DirtyRanges::full(input.frame_count, input.bone_count, input.morph_count);
        }
        let candidate_started = Instant::now();
        let local_pose = DenseLocalPose {
            translations: &local_translations,
            rotations: &local_rotations,
            euler_xyz: &local_euler_xyz,
        };
        if let Some(sequence) = candidate.as_mut() {
            rebuild_dirty_tracks(
                sequence,
                target,
                input,
                local_pose,
                &bone_key_indices,
                &morph_key_indices,
                &dirty,
                ReductionInstrumentation {
                    work_stats: &mut work_stats,
                    timings: &mut timings,
                },
            );
        } else {
            candidate = Some(build_sequence(
                &snapshot,
                target,
                input,
                local_pose,
                &bone_key_indices,
                &morph_key_indices,
                ReductionInstrumentation {
                    work_stats: &mut work_stats,
                    timings: &mut timings,
                },
            ));
        }
        timings.candidate_build += candidate_started.elapsed();
        let error_measure_started = Instant::now();
        let (report, worst_by_track) = measure_error_cached(
            candidate.as_ref().expect("candidate initialized"),
            input,
            DenseValidationPose {
                translations: &local_translations,
                rotations: &local_rotations,
                world_positions: &world_positions,
                world_rotations: &world_rotations,
            },
            tolerances,
            &mut work_stats,
            &dirty,
            &mut validation_cache,
            worker_pool.as_ref(),
            worker_count,
        )?;
        timings.error_measure += error_measure_started.elapsed();
        let mut failing = worst_by_track
            .into_iter()
            .flatten()
            .filter(|worst| worst.normalized_error > 1.0)
            .collect::<Vec<_>>();
        failing.sort_by(compare_worst_errors);
        if failing.is_empty() {
            work_stats.added_keys_per_pass.push(0);
            let mut result = candidate.expect("candidate initialized");
            result.report = PoseReductionReport {
                source_bone_key_count: input.frame_count * input.bone_count,
                reduced_bone_key_count: result
                    .bone_tracks
                    .iter()
                    .map(|track| track.keys.len())
                    .sum(),
                source_morph_key_count: input.frame_count * input.morph_count,
                reduced_morph_key_count: result
                    .morph_tracks
                    .iter()
                    .map(|track| track.keys.len())
                    .sum(),
                ..report
            };
            result.work_stats = work_stats;
            result.timings = timings;
            return Ok(result);
        }
        let mut inserted = false;
        let mut added_this_pass = 0;
        let mut next_dirty = DirtyRanges::empty(input.bone_count, input.morph_count);
        let first_failure_frame = failing[0].frame;
        for worst in failing {
            match worst.track {
                ErrorTrack::Bone(bone) => {
                    let mut cursor = Some(bone);
                    let mut is_origin = true;
                    while let Some(index) = cursor {
                        if let Some(range) = insert_key_with_affected_range(
                            &mut bone_key_indices[index],
                            worst.frame,
                        ) {
                            inserted = true;
                            added_this_pass += 1;
                            next_dirty.mark_bone(index, range);
                            if is_origin {
                                work_stats.normal_key_additions += 1;
                            } else {
                                work_stats.ancestor_key_additions += 1;
                            }
                        }
                        let parent = snapshot.parent_indices[index];
                        cursor = (parent >= 0).then_some(parent as usize);
                        is_origin = false;
                    }
                }
                ErrorTrack::Morph(morph) => {
                    if let Some(range) =
                        insert_key_with_affected_range(&mut morph_key_indices[morph], worst.frame)
                    {
                        inserted = true;
                        added_this_pass += 1;
                        next_dirty.mark_morph(morph, range);
                        work_stats.normal_key_additions += 1;
                    }
                }
            }
        }
        work_stats.added_keys_per_pass.push(added_this_pass);
        if !inserted {
            return Err(PoseReductionError::ToleranceUnattainable {
                frame: first_failure_frame,
            });
        }
        dirty = next_dirty;
    }
}

fn endpoint_indices(frame_count: usize) -> Vec<usize> {
    if frame_count == 1 {
        vec![0]
    } else {
        vec![0, frame_count - 1]
    }
}

fn decompose_dense_pose(
    input: DensePoseSequenceView<'_>,
    snapshot: &SkeletonSnapshot,
) -> Result<(Vec<Vec3A>, Vec<Quat>), PoseReductionError> {
    let mut translations = vec![Vec3A::ZERO; input.frame_count * input.bone_count];
    let mut rotations = vec![Quat::IDENTITY; input.frame_count * input.bone_count];
    for frame in 0..input.frame_count {
        for &bone in snapshot.evaluation_order.iter() {
            let world = input.world_matrix(frame, bone);
            validate_finite_matrix(world, frame, bone)?;
            let local = if snapshot.parent_indices[bone] < 0 {
                world
            } else {
                let parent = snapshot.parent_indices[bone] as usize;
                let parent_world = input.world_matrix(frame, parent);
                validate_finite_matrix(parent_world, frame, parent)?;
                let determinant = Mat3::from_mat4(parent_world).determinant();
                if !determinant.is_finite() || determinant.abs() <= f32::EPSILON {
                    return Err(PoseReductionError::SingularParent { frame, bone });
                }
                parent_world.inverse() * world
            };
            let (translation, rotation) = decompose_rigid(local, frame, bone)?;
            let index = frame * input.bone_count + bone;
            translations[index] = translation;
            rotations[index] = if frame > 0 {
                let previous = rotations[index - input.bone_count];
                if previous.dot(rotation) < 0.0 {
                    -rotation
                } else {
                    rotation
                }
            } else {
                rotation
            };
        }
    }
    Ok((translations, rotations))
}

fn cache_dense_world_components(
    input: DensePoseSequenceView<'_>,
) -> Result<(Vec<Vec3A>, Vec<Quat>), PoseReductionError> {
    let count = input.frame_count * input.bone_count;
    let mut positions = Vec::with_capacity(count);
    let mut rotations = Vec::with_capacity(count);
    for frame in 0..input.frame_count {
        for bone in 0..input.bone_count {
            let world = input.world_matrix(frame, bone);
            positions.push(Vec3A::from(world.w_axis.truncate()));
            rotations.push(decompose_rigid(world, frame, bone)?.1);
        }
    }
    Ok((positions, rotations))
}

fn decompose_rigid(
    matrix: Mat4,
    frame: usize,
    bone: usize,
) -> Result<(Vec3A, Quat), PoseReductionError> {
    validate_finite_matrix(matrix, frame, bone)?;
    let x = matrix.x_axis.truncate();
    let y = matrix.y_axis.truncate();
    let z = matrix.z_axis.truncate();
    let unit = |v: Vec3| (v.length_squared() - 1.0).abs() <= AFFINE_EPSILON;
    if !unit(x)
        || !unit(y)
        || !unit(z)
        || x.dot(y).abs() > AFFINE_EPSILON
        || x.dot(z).abs() > AFFINE_EPSILON
        || y.dot(z).abs() > AFFINE_EPSILON
        || Mat3::from_cols(x, y, z).determinant() < 0.0
        || matrix.x_axis.w.abs() > AFFINE_EPSILON
        || matrix.y_axis.w.abs() > AFFINE_EPSILON
        || matrix.z_axis.w.abs() > AFFINE_EPSILON
        || (matrix.w_axis.w - 1.0).abs() > AFFINE_EPSILON
    {
        return Err(PoseReductionError::ScaleOrShear { frame, bone });
    }
    let rotation = normalize_quat(Quat::from_mat3(&Mat3::from_cols(x, y, z)));
    Ok((Vec3A::from(matrix.w_axis.truncate()), rotation))
}

fn validate_finite_matrix(
    matrix: Mat4,
    frame: usize,
    bone: usize,
) -> Result<(), PoseReductionError> {
    if matrix
        .to_cols_array()
        .iter()
        .any(|value| !value.is_finite())
    {
        Err(PoseReductionError::NonFiniteMatrix { frame, bone })
    } else {
        Ok(())
    }
}

fn split_bone_track(
    keys: &mut Vec<usize>,
    input: DensePoseSequenceView<'_>,
    bone: usize,
    translations: &[Vec3A],
    rotations: &[Quat],
    tolerances: ReductionTolerances,
) {
    if input.frame_count <= 2 {
        return;
    }
    let bone_count = translations.len() / input.frame_count;
    let mut stack = vec![(0usize, input.frame_count - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let start_index = start * bone_count + bone;
        let end_index = end * bone_count + bone;
        let mut worst: Option<(f32, usize)> = None;
        for frame in start + 1..end {
            let amount = segment_amount(start, end, frame, |sample| input.sample_frame(sample));
            let index = frame * bone_count + bone;
            let position_error = translations[index]
                .distance(translations[start_index].lerp(translations[end_index], amount));
            let rotation_error = quat_angle(
                rotations[index],
                rotations[start_index].slerp(rotations[end_index], amount),
            );
            let normalized = normalized_error(position_error, tolerances.local_position).max(
                normalized_error(rotation_error, tolerances.local_rotation_radians),
            );
            if is_worse(worst, normalized, frame) {
                worst = Some((normalized, frame));
            }
        }
        if let Some((normalized, frame)) = worst.filter(|value| value.0 > 1.0) {
            let _ = normalized;
            insert_key(keys, frame);
            stack.push((frame, end));
            stack.push((start, frame));
        }
    }
}

#[derive(Default)]
struct LocalPrefitStats {
    segment_fits: usize,
    key_additions: usize,
    samples: usize,
}

fn split_dcc_bone_track(
    keys: &mut Vec<usize>,
    input: DensePoseSequenceView<'_>,
    bone: usize,
    translations: &[Vec3A],
    rotations: &[Quat],
    euler_xyz: &[Vec3A],
    tolerances: ReductionTolerances,
) -> LocalPrefitStats {
    if input.frame_count <= 2 {
        return LocalPrefitStats::default();
    }
    let bone_count = input.bone_count;
    let mut stats = LocalPrefitStats::default();
    let mut stack = vec![(0usize, input.frame_count - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let segment = fit_dcc_bone_segment(input, bone, start, end, translations, euler_xyz);
        stats.segment_fits += 1;
        let start_index = start * bone_count + bone;
        let end_index = end * bone_count + bone;
        let duration = input.sample_frame(end) - input.sample_frame(start);
        let mut worst: Option<(f32, usize)> = None;
        for frame in start + 1..end {
            stats.samples += 1;
            let amount = segment_amount(start, end, frame, |sample| input.sample_frame(sample));
            let translation = sample_dcc_vec3(
                translations[start_index],
                translations[end_index],
                segment.translation_out_tangent,
                segment.translation_in_tangent,
                duration,
                amount,
            );
            let euler = sample_dcc_vec3(
                segment.rotation_start_euler_xyz,
                segment.rotation_end_euler_xyz,
                segment.rotation_out_tangent,
                segment.rotation_in_tangent,
                duration,
                amount,
            );
            let rotation =
                normalize_quat(Quat::from_euler(EulerRot::XYZ, euler.x, euler.y, euler.z));
            let index = frame * bone_count + bone;
            let normalized = normalized_error(
                translations[index].distance(translation),
                tolerances.local_position,
            )
            .max(normalized_error(
                quat_angle(rotations[index], rotation),
                tolerances.local_rotation_radians,
            ));
            if is_worse(worst, normalized, frame) {
                worst = Some((normalized, frame));
            }
        }
        if let Some((_, frame)) = worst.filter(|value| value.0 > 1.0) {
            stats.key_additions += usize::from(insert_key(keys, frame));
            stack.push((frame, end));
            stack.push((start, frame));
        }
    }
    stats
}

fn split_dcc_morph_track(
    keys: &mut Vec<usize>,
    input: DensePoseSequenceView<'_>,
    morph: usize,
    tolerance: f32,
) -> LocalPrefitStats {
    if input.frame_count <= 2 {
        return LocalPrefitStats::default();
    }
    let mut stats = LocalPrefitStats::default();
    let mut stack = vec![(0usize, input.frame_count - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let segment = fit_dcc_scalar_segment(
            start,
            end,
            |sample| input.morph_weight(sample, morph),
            |sample| input.sample_frame(sample),
        );
        stats.segment_fits += 1;
        let duration = input.sample_frame(end) - input.sample_frame(start);
        let mut worst: Option<(f32, usize)> = None;
        for frame in start + 1..end {
            stats.samples += 1;
            let amount = segment_amount(start, end, frame, |sample| input.sample_frame(sample));
            let expected = sample_hermite(
                input.morph_weight(start, morph),
                input.morph_weight(end, morph),
                segment.out_tangent,
                segment.in_tangent,
                duration,
                amount,
            );
            let normalized = normalized_error(
                (input.morph_weight(frame, morph) - expected).abs(),
                tolerance,
            );
            if is_worse(worst, normalized, frame) {
                worst = Some((normalized, frame));
            }
        }
        if let Some((_, frame)) = worst.filter(|value| value.0 > 1.0) {
            stats.key_additions += usize::from(insert_key(keys, frame));
            stack.push((frame, end));
            stack.push((start, frame));
        }
    }
    stats
}

fn split_morph_track(
    keys: &mut Vec<usize>,
    input: DensePoseSequenceView<'_>,
    morph: usize,
    tolerance: f32,
) {
    if input.frame_count <= 2 {
        return;
    }
    let mut stack = vec![(0usize, input.frame_count - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let mut worst: Option<(f32, usize)> = None;
        for frame in start + 1..end {
            let amount = segment_amount(start, end, frame, |sample| input.sample_frame(sample));
            let expected = input.morph_weight(start, morph)
                + (input.morph_weight(end, morph) - input.morph_weight(start, morph)) * amount;
            let normalized = normalized_error(
                (input.morph_weight(frame, morph) - expected).abs(),
                tolerance,
            );
            if is_worse(worst, normalized, frame) {
                worst = Some((normalized, frame));
            }
        }
        if let Some((_, frame)) = worst.filter(|value| value.0 > 1.0) {
            insert_key(keys, frame);
            stack.push((frame, end));
            stack.push((start, frame));
        }
    }
}

fn insert_key(keys: &mut Vec<usize>, frame: usize) -> bool {
    if let Err(position) = keys.binary_search(&frame) {
        keys.insert(position, frame);
        true
    } else {
        false
    }
}

fn insert_key_with_affected_range(
    keys: &mut Vec<usize>,
    frame: usize,
) -> Option<std::ops::RangeInclusive<usize>> {
    let position = keys.binary_search(&frame).err()?;
    let start = keys[position.saturating_sub(1)];
    let end = keys[position.min(keys.len() - 1)];
    keys.insert(position, frame);
    Some(start..=end)
}

#[derive(Clone, Copy)]
struct DenseLocalPose<'a> {
    translations: &'a [Vec3A],
    rotations: &'a [Quat],
    euler_xyz: &'a [Vec3A],
}

struct ReductionInstrumentation<'a> {
    work_stats: &'a mut ReductionWorkStats,
    timings: &'a mut ReductionTimings,
}

fn build_sequence(
    snapshot: &SkeletonSnapshot,
    target: ReductionTarget,
    input: DensePoseSequenceView<'_>,
    local_pose: DenseLocalPose<'_>,
    bone_key_indices: &[Vec<usize>],
    morph_key_indices: &[Vec<usize>],
    instrumentation: ReductionInstrumentation<'_>,
) -> ReducedPoseSequence {
    let ReductionInstrumentation {
        work_stats,
        timings,
    } = instrumentation;
    if target == ReductionTarget::DccCubic {
        work_stats.dcc_bone_segment_fits += bone_key_indices
            .iter()
            .map(|indices| indices.len().saturating_sub(1))
            .sum::<usize>();
        work_stats.dcc_morph_segment_fits += morph_key_indices
            .iter()
            .map(|indices| indices.len().saturating_sub(1))
            .sum::<usize>();
    }
    work_stats.candidate_bone_track_rebuilds += bone_key_indices.len();
    work_stats.candidate_morph_track_rebuilds += morph_key_indices.len();
    let dcc_bone_started = (target == ReductionTarget::DccCubic).then(Instant::now);
    let bone_tracks = bone_key_indices
        .iter()
        .enumerate()
        .map(|(bone, indices)| build_bone_track(target, input, local_pose, bone, indices))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    if let Some(started) = dcc_bone_started {
        timings.dcc_fit += started.elapsed();
    }
    let dcc_morph_started = (target == ReductionTarget::DccCubic).then(Instant::now);
    let morph_tracks = morph_key_indices
        .iter()
        .enumerate()
        .map(|(morph, indices)| build_morph_track(target, input, morph, indices))
        .collect::<Vec<_>>()
        .into_boxed_slice();
    if let Some(started) = dcc_morph_started {
        timings.dcc_fit += started.elapsed();
    }
    ReducedPoseSequence {
        snapshot: snapshot.clone(),
        target,
        start_frame: input.start_frame,
        frame_step: input.frame_step,
        frame_count: input.frame_count,
        sample_frames: (0..input.frame_count)
            .map(|sample| input.sample_frame(sample))
            .collect::<Vec<_>>()
            .into_boxed_slice(),
        bone_tracks,
        morph_tracks,
        report: PoseReductionReport::default(),
        work_stats: ReductionWorkStats::default(),
        timings: ReductionTimings::default(),
    }
}

fn build_bone_track(
    target: ReductionTarget,
    input: DensePoseSequenceView<'_>,
    local_pose: DenseLocalPose<'_>,
    bone: usize,
    indices: &[usize],
) -> ReducedBoneTrack {
    ReducedBoneTrack {
        keys: indices
            .iter()
            .enumerate()
            .map(|(key_position, &frame)| {
                let index = frame * input.bone_count + bone;
                ReducedBoneKey {
                    sample_index: frame,
                    translation: local_pose.translations[index],
                    rotation: local_pose.rotations[index],
                    vmd_interpolation: if target == ReductionTarget::VmdBezier && key_position > 0 {
                        fit_vmd_bone_interpolation(
                            input,
                            bone,
                            indices[key_position - 1],
                            frame,
                            local_pose.translations,
                            local_pose.rotations,
                        )
                    } else {
                        VmdBoneInterpolation::LINEAR
                    },
                    dcc_segment: if target == ReductionTarget::DccCubic && key_position > 0 {
                        fit_dcc_bone_segment(
                            input,
                            bone,
                            indices[key_position - 1],
                            frame,
                            local_pose.translations,
                            local_pose.euler_xyz,
                        )
                    } else {
                        DccCubicSegment::default()
                    },
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    }
}

fn build_morph_track(
    target: ReductionTarget,
    input: DensePoseSequenceView<'_>,
    morph: usize,
    indices: &[usize],
) -> ReducedMorphTrack {
    ReducedMorphTrack {
        keys: indices
            .iter()
            .enumerate()
            .map(|(key_position, &frame)| ReducedMorphKey {
                sample_index: frame,
                weight: input.morph_weight(frame, morph),
                dcc_segment: if target == ReductionTarget::DccCubic && key_position > 0 {
                    fit_dcc_scalar_segment(
                        indices[key_position - 1],
                        frame,
                        |sample| input.morph_weight(sample, morph),
                        |sample| input.sample_frame(sample),
                    )
                } else {
                    DccScalarSegment::default()
                },
            })
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    }
}

#[allow(clippy::too_many_arguments)]
fn rebuild_dirty_tracks(
    sequence: &mut ReducedPoseSequence,
    target: ReductionTarget,
    input: DensePoseSequenceView<'_>,
    local_pose: DenseLocalPose<'_>,
    bone_key_indices: &[Vec<usize>],
    morph_key_indices: &[Vec<usize>],
    dirty: &DirtyRanges,
    instrumentation: ReductionInstrumentation<'_>,
) {
    let ReductionInstrumentation {
        work_stats,
        timings,
    } = instrumentation;
    let dcc_started = (target == ReductionTarget::DccCubic).then(Instant::now);
    for (bone, range) in dirty.bone_local.iter().enumerate() {
        if range.is_empty() {
            continue;
        }
        let indices = &bone_key_indices[bone];
        work_stats.candidate_bone_track_rebuilds += 1;
        if target == ReductionTarget::DccCubic {
            work_stats.dcc_bone_segment_fits += indices.len().saturating_sub(1);
        }
        sequence.bone_tracks[bone] = build_bone_track(target, input, local_pose, bone, indices);
    }
    for (morph, range) in dirty.morph.iter().enumerate() {
        if range.is_empty() {
            continue;
        }
        let indices = &morph_key_indices[morph];
        work_stats.candidate_morph_track_rebuilds += 1;
        if target == ReductionTarget::DccCubic {
            work_stats.dcc_morph_segment_fits += indices.len().saturating_sub(1);
        }
        sequence.morph_tracks[morph] = build_morph_track(target, input, morph, indices);
    }
    if let Some(started) = dcc_started {
        timings.dcc_fit += started.elapsed();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorTrack {
    Bone(usize),
    Morph(usize),
}

#[derive(Clone, Copy)]
struct WorstError {
    normalized_error: f32,
    frame: usize,
    track: ErrorTrack,
}

#[derive(Clone, Copy)]
struct DenseValidationPose<'a> {
    translations: &'a [Vec3A],
    rotations: &'a [Quat],
    world_positions: &'a [Vec3A],
    world_rotations: &'a [Quat],
}

#[derive(Clone, Copy, Default)]
struct BoneErrorCell {
    local_position: f32,
    local_rotation: f32,
    world_position: f32,
    world_rotation: f32,
}

struct ValidationCache {
    local_translations: Vec<Vec3A>,
    local_rotations: Vec<Quat>,
    world_matrices: Vec<Mat4>,
    world_rotations: Vec<Quat>,
    morph_weights: Vec<f32>,
    bone_errors: Vec<BoneErrorCell>,
    morph_errors: Vec<f32>,
}

#[cfg(not(target_family = "wasm"))]
struct ValidationCacheChunk {
    start_frame: usize,
    local_translations: Vec<Vec3A>,
    local_rotations: Vec<Quat>,
    world_matrices: Vec<Mat4>,
    world_rotations: Vec<Quat>,
    morph_weights: Vec<f32>,
    bone_errors: Vec<BoneErrorCell>,
    morph_errors: Vec<f32>,
}

impl ValidationCache {
    fn new(frame_count: usize, bone_count: usize, morph_count: usize) -> Self {
        Self {
            local_translations: vec![Vec3A::ZERO; frame_count * bone_count],
            local_rotations: vec![Quat::IDENTITY; frame_count * bone_count],
            world_matrices: vec![Mat4::IDENTITY; frame_count * bone_count],
            world_rotations: vec![Quat::IDENTITY; frame_count * bone_count],
            morph_weights: vec![0.0; frame_count * morph_count],
            bone_errors: vec![BoneErrorCell::default(); frame_count * bone_count],
            morph_errors: vec![0.0; frame_count * morph_count],
        }
    }
}

#[cfg(not(target_family = "wasm"))]
#[allow(clippy::too_many_arguments)]
fn refresh_full_validation_cache_parallel(
    sequence: &ReducedPoseSequence,
    input: DensePoseSequenceView<'_>,
    dense: DenseValidationPose<'_>,
    cache: &mut ValidationCache,
    work_stats: &mut ReductionWorkStats,
    worker_pool: &ReductionThreadPool,
    worker_count: usize,
) -> Result<(), PoseReductionError> {
    let chunk_size = input.frame_count.div_ceil(worker_count);
    let ranges = (0..worker_count)
        .map(|worker| {
            let start = worker * chunk_size;
            start..(start + chunk_size).min(input.frame_count)
        })
        .filter(|range| !range.is_empty())
        .collect::<Vec<_>>();
    let chunks = worker_pool.install(|| {
        ranges
            .par_iter()
            .map(|range| {
                let frame_count = range.end - range.start;
                let mut chunk = ValidationCacheChunk {
                    start_frame: range.start,
                    local_translations: Vec::with_capacity(frame_count * input.bone_count),
                    local_rotations: Vec::with_capacity(frame_count * input.bone_count),
                    world_matrices: Vec::with_capacity(frame_count * input.bone_count),
                    world_rotations: Vec::with_capacity(frame_count * input.bone_count),
                    morph_weights: Vec::with_capacity(frame_count * input.morph_count),
                    bone_errors: Vec::with_capacity(frame_count * input.bone_count),
                    morph_errors: Vec::with_capacity(frame_count * input.morph_count),
                };
                let mut scratch = ReducedPoseScratch::default();
                scratch.prepare(input.bone_count, input.morph_count);
                for frame in range.clone() {
                    sequence.sample_into(input.sample_frame(frame), &mut scratch)?;
                    chunk
                        .local_translations
                        .extend_from_slice(&scratch.local_translations);
                    chunk
                        .local_rotations
                        .extend_from_slice(&scratch.local_rotations);
                    chunk
                        .world_matrices
                        .extend_from_slice(&scratch.world_matrices);
                    for bone in 0..input.bone_count {
                        let index = frame * input.bone_count + bone;
                        let world_rotation =
                            decompose_rigid(scratch.world_matrices[bone], frame, bone)?.1;
                        chunk.world_rotations.push(world_rotation);
                        chunk.bone_errors.push(BoneErrorCell {
                            local_position: dense.translations[index]
                                .distance(scratch.local_translations[bone]),
                            local_rotation: quat_angle(
                                dense.rotations[index],
                                scratch.local_rotations[bone],
                            ),
                            world_position: dense.world_positions[index].distance(Vec3A::from(
                                scratch.world_matrices[bone].w_axis.truncate(),
                            )),
                            world_rotation: quat_angle(
                                dense.world_rotations[index],
                                world_rotation,
                            ),
                        });
                    }
                    chunk
                        .morph_weights
                        .extend_from_slice(&scratch.morph_weights);
                    for morph in 0..input.morph_count {
                        chunk.morph_errors.push(
                            (input.morph_weight(frame, morph) - scratch.morph_weights[morph]).abs(),
                        );
                    }
                }
                Ok(chunk)
            })
            .collect::<Result<Vec<_>, PoseReductionError>>()
    })?;

    for chunk in chunks {
        let bone_start = chunk.start_frame * input.bone_count;
        let bone_end = bone_start + chunk.local_translations.len();
        cache.local_translations[bone_start..bone_end].copy_from_slice(&chunk.local_translations);
        cache.local_rotations[bone_start..bone_end].copy_from_slice(&chunk.local_rotations);
        cache.world_matrices[bone_start..bone_end].copy_from_slice(&chunk.world_matrices);
        cache.world_rotations[bone_start..bone_end].copy_from_slice(&chunk.world_rotations);
        cache.bone_errors[bone_start..bone_end].copy_from_slice(&chunk.bone_errors);
        let morph_start = chunk.start_frame * input.morph_count;
        let morph_end = morph_start + chunk.morph_weights.len();
        cache.morph_weights[morph_start..morph_end].copy_from_slice(&chunk.morph_weights);
        cache.morph_errors[morph_start..morph_end].copy_from_slice(&chunk.morph_errors);
    }
    work_stats.bone_samples += input.frame_count * input.bone_count;
    work_stats.morph_samples += input.frame_count * input.morph_count;
    work_stats.world_rebuilds += input.frame_count;
    work_stats.world_bone_recomputes += input.frame_count * input.bone_count;
    work_stats.world_rotation_decompositions += input.frame_count * input.bone_count;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn measure_error_cached(
    sequence: &ReducedPoseSequence,
    input: DensePoseSequenceView<'_>,
    dense: DenseValidationPose<'_>,
    tolerances: ReductionTolerances,
    work_stats: &mut ReductionWorkStats,
    dirty: &DirtyRanges,
    cache: &mut ValidationCache,
    worker_pool: Option<&ReductionThreadPool>,
    worker_count: usize,
) -> Result<(PoseReductionReport, Vec<Option<WorstError>>), PoseReductionError> {
    let full_dirty = dirty_ranges_are_full(
        dirty,
        input.frame_count,
        input.bone_count,
        input.morph_count,
    );
    #[cfg(not(target_family = "wasm"))]
    let refreshed_in_parallel = if full_dirty && worker_count > 1 {
        refresh_full_validation_cache_parallel(
            sequence,
            input,
            dense,
            cache,
            work_stats,
            worker_pool.expect("multi-worker reduction has a pool"),
            worker_count,
        )?;
        true
    } else {
        false
    };
    #[cfg(target_family = "wasm")]
    let refreshed_in_parallel = {
        let _ = (full_dirty, worker_pool, worker_count);
        false
    };

    let mut world_dirty = dirty.bone_local.clone();
    for &bone in sequence.snapshot.evaluation_order.iter() {
        let parent = sequence.snapshot.parent_indices[bone];
        if parent >= 0 {
            for range in world_dirty[parent as usize].clone() {
                insert_dirty_range(&mut world_dirty[bone], range);
            }
        }
    }

    if !refreshed_in_parallel {
        for (bone, ranges) in dirty.bone_local.iter().enumerate() {
            for range in ranges {
                for frame in range.clone() {
                    let index = frame * input.bone_count + bone;
                    let (translation, rotation) = sample_bone_track(
                        &sequence.bone_tracks[bone],
                        &sequence.sample_frames,
                        input.sample_frame(frame),
                        sequence.target,
                    );
                    cache.local_translations[index] = translation;
                    cache.local_rotations[index] = rotation;
                    let cell = &mut cache.bone_errors[index];
                    cell.local_position = dense.translations[index].distance(translation);
                    cell.local_rotation = quat_angle(dense.rotations[index], rotation);
                    work_stats.bone_samples += 1;
                }
            }
        }
        for (morph, ranges) in dirty.morph.iter().enumerate() {
            for range in ranges {
                for frame in range.clone() {
                    let index = frame * input.morph_count + morph;
                    let sampled = sample_morph_track(
                        &sequence.morph_tracks[morph],
                        &sequence.sample_frames,
                        input.sample_frame(frame),
                        sequence.target,
                    );
                    cache.morph_weights[index] = sampled;
                    cache.morph_errors[index] = (input.morph_weight(frame, morph) - sampled).abs();
                    work_stats.morph_samples += 1;
                }
            }
        }

        let mut rebuilt_frames = vec![false; input.frame_count];
        for &bone in sequence.snapshot.evaluation_order.iter() {
            for range in &world_dirty[bone] {
                for frame in range.clone() {
                    rebuilt_frames[frame] = true;
                    let index = frame * input.bone_count + bone;
                    let local = Mat4::from_rotation_translation(
                        cache.local_rotations[index],
                        cache.local_translations[index].into(),
                    );
                    let parent = sequence.snapshot.parent_indices[bone];
                    cache.world_matrices[index] = if parent < 0 {
                        local
                    } else {
                        cache.world_matrices[frame * input.bone_count + parent as usize] * local
                    };
                    cache.world_rotations[index] =
                        decompose_rigid(cache.world_matrices[index], frame, bone)?.1;
                    let cell = &mut cache.bone_errors[index];
                    cell.world_position = dense.world_positions[index]
                        .distance(Vec3A::from(cache.world_matrices[index].w_axis.truncate()));
                    cell.world_rotation =
                        quat_angle(dense.world_rotations[index], cache.world_rotations[index]);
                    work_stats.world_bone_recomputes += 1;
                    work_stats.world_rotation_decompositions += 1;
                }
            }
        }
        work_stats.world_rebuilds += rebuilt_frames
            .into_iter()
            .filter(|rebuilt| *rebuilt)
            .count();
    }

    let mut report = PoseReductionReport::default();
    let mut worst = vec![None; input.bone_count + input.morph_count];
    for frame in 0..input.frame_count {
        let (bone_worst, morph_worst) = worst.split_at_mut(input.bone_count);
        for (bone, worst) in bone_worst.iter_mut().enumerate() {
            let index = frame * input.bone_count + bone;
            let cell = cache.bone_errors[index];
            report.max_local_position_error =
                report.max_local_position_error.max(cell.local_position);
            report.max_local_rotation_error_radians = report
                .max_local_rotation_error_radians
                .max(cell.local_rotation);
            report.max_world_position_error =
                report.max_world_position_error.max(cell.world_position);
            report.max_world_rotation_error_radians = report
                .max_world_rotation_error_radians
                .max(cell.world_rotation);
            for normalized in [
                normalized_error(cell.local_position, tolerances.local_position),
                normalized_error(cell.local_rotation, tolerances.local_rotation_radians),
                normalized_error(cell.world_position, tolerances.world_position),
                normalized_error(cell.world_rotation, tolerances.world_rotation_radians),
            ] {
                update_worst(worst, normalized, frame, ErrorTrack::Bone(bone));
            }
        }
        for (morph, morph_worst) in morph_worst.iter_mut().enumerate() {
            let error = cache.morph_errors[frame * input.morph_count + morph];
            report.max_morph_weight_error = report.max_morph_weight_error.max(error);
            update_worst(
                morph_worst,
                normalized_error(error, tolerances.morph_weight),
                frame,
                ErrorTrack::Morph(morph),
            );
        }
    }
    Ok((report, worst))
}

fn dirty_ranges_are_full(
    dirty: &DirtyRanges,
    frame_count: usize,
    bone_count: usize,
    morph_count: usize,
) -> bool {
    let is_full = |ranges: &[std::ops::RangeInclusive<usize>]| {
        ranges.len() == 1
            && *ranges[0].start() == 0
            && *ranges[0].end() == frame_count.saturating_sub(1)
    };
    dirty.bone_local.len() == bone_count
        && dirty.morph.len() == morph_count
        && dirty.bone_local.iter().all(|ranges| is_full(ranges))
        && dirty.morph.iter().all(|ranges| is_full(ranges))
}

fn update_worst(worst: &mut Option<WorstError>, normalized: f32, frame: usize, track: ErrorTrack) {
    update_worst_candidate(
        worst,
        WorstError {
            normalized_error: normalized,
            frame,
            track,
        },
    );
}

fn update_worst_candidate(worst: &mut Option<WorstError>, candidate: WorstError) {
    if worst.is_none_or(|current| compare_worst_errors(&candidate, &current) == Ordering::Less) {
        *worst = Some(candidate);
    }
}

fn compare_worst_errors(a: &WorstError, b: &WorstError) -> Ordering {
    b.normalized_error
        .total_cmp(&a.normalized_error)
        .then_with(|| a.frame.cmp(&b.frame))
        .then_with(|| error_track_sort_key(a.track).cmp(&error_track_sort_key(b.track)))
}

fn error_track_sort_key(track: ErrorTrack) -> (u8, usize) {
    match track {
        ErrorTrack::Bone(index) => (0, index),
        ErrorTrack::Morph(index) => (1, index),
    }
}

fn resolve_reduction_worker_count(requested: usize, frame_count: usize) -> usize {
    #[cfg(target_family = "wasm")]
    {
        let _ = (requested, frame_count);
        1
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let workers = if requested == 0 {
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
        } else {
            requested
        };
        workers.clamp(1, frame_count.max(1))
    }
}

#[cfg(not(target_family = "wasm"))]
fn build_reduction_worker_pool(
    worker_count: usize,
) -> Result<Option<ReductionThreadPool>, PoseReductionError> {
    (worker_count > 1)
        .then(|| {
            ThreadPoolBuilder::new()
                .num_threads(worker_count)
                .build()
                .map_err(|_| PoseReductionError::WorkerPool)
        })
        .transpose()
}

#[cfg(target_family = "wasm")]
fn build_reduction_worker_pool(
    _worker_count: usize,
) -> Result<Option<ReductionThreadPool>, PoseReductionError> {
    Ok(None)
}

fn fit_vmd_bone_interpolation(
    input: DensePoseSequenceView<'_>,
    bone: usize,
    start: usize,
    end: usize,
    translations: &[Vec3A],
    rotations: &[Quat],
) -> VmdBoneInterpolation {
    let bone_count = input.bone_count;
    let start_index = start * bone_count + bone;
    let end_index = end * bone_count + bone;
    let start_translation = translations[start_index];
    let end_translation = translations[end_index];
    let translation = std::array::from_fn(|axis| {
        let start_value = start_translation.to_array()[axis];
        let end_value = end_translation.to_array()[axis];
        fit_quantized_bezier(
            start,
            end,
            |sample| {
                let value = translations[sample * bone_count + bone].to_array()[axis];
                normalized_channel_value(start_value, end_value, value)
            },
            |sample| input.sample_frame(sample),
        )
    });
    let start_rotation = rotations[start_index];
    let end_rotation = rotations[end_index];
    let total_angle = quat_angle(start_rotation, end_rotation);
    let rotation = fit_quantized_bezier(
        start,
        end,
        |sample| {
            if total_angle <= f32::EPSILON {
                0.0
            } else {
                (quat_angle(start_rotation, rotations[sample * bone_count + bone]) / total_angle)
                    .clamp(0.0, 1.0)
            }
        },
        |sample| input.sample_frame(sample),
    );
    VmdBoneInterpolation {
        translation,
        rotation,
    }
}

fn fit_quantized_bezier(
    start: usize,
    end: usize,
    value_at: impl Fn(usize) -> f32,
    frame_at: impl Fn(usize) -> f32,
) -> QuantizedBezier {
    if end <= start + 1 {
        return QuantizedBezier::LINEAR;
    }
    let mut best = QuantizedBezier::LINEAR;
    let score = |curve: QuantizedBezier| -> f32 {
        (start + 1..end)
            .map(|sample| {
                let time = segment_amount(start, end, sample, &frame_at);
                (curve.evaluate(time) - value_at(sample)).abs()
            })
            .fold(0.0f32, f32::max)
    };
    let mut best_score = score(best);
    const COARSE: [u8; 9] = [0, 16, 32, 48, 64, 80, 96, 112, 127];
    for &x1 in &COARSE {
        for &x2 in &COARSE {
            if x1 > x2 {
                continue;
            }
            for &y1 in &COARSE {
                for &y2 in &COARSE {
                    let candidate = QuantizedBezier { x1, y1, x2, y2 };
                    let candidate_score = score(candidate);
                    if candidate_score.total_cmp(&best_score) == Ordering::Less {
                        best = candidate;
                        best_score = candidate_score;
                    }
                }
            }
        }
    }
    for step in [32i16, 16, 8, 4, 2, 1] {
        loop {
            let origin = [best.x1, best.y1, best.x2, best.y2];
            let mut next_best = best;
            let mut next_score = best_score;
            for d0 in [-step, 0, step] {
                for d1 in [-step, 0, step] {
                    for d2 in [-step, 0, step] {
                        for d3 in [-step, 0, step] {
                            let offsets = [d0, d1, d2, d3];
                            let mut values = [0u8; 4];
                            let mut valid = true;
                            for coordinate in 0..4 {
                                let value = origin[coordinate] as i16 + offsets[coordinate];
                                if !(0..=127).contains(&value) {
                                    valid = false;
                                    break;
                                }
                                values[coordinate] = value as u8;
                            }
                            if !valid || values[0] > values[2] {
                                continue;
                            }
                            let candidate = QuantizedBezier {
                                x1: values[0],
                                y1: values[1],
                                x2: values[2],
                                y2: values[3],
                            };
                            let candidate_score = score(candidate);
                            if candidate_score.total_cmp(&next_score) == Ordering::Less {
                                next_best = candidate;
                                next_score = candidate_score;
                            }
                        }
                    }
                }
            }
            if next_best == best {
                break;
            }
            best = next_best;
            best_score = next_score;
        }
    }
    best
}

fn normalized_channel_value(start: f32, end: f32, value: f32) -> f32 {
    let range = end - start;
    if range.abs() <= f32::EPSILON {
        0.0
    } else {
        ((value - start) / range).clamp(0.0, 1.0)
    }
}

fn unwrap_euler_xyz(rotations: &[Quat], frame_count: usize, bone_count: usize) -> Vec<Vec3A> {
    let mut result = vec![Vec3A::ZERO; rotations.len()];
    for bone in 0..bone_count {
        for frame in 0..frame_count {
            let index = frame * bone_count + bone;
            let (x, y, z) = rotations[index].to_euler(EulerRot::XYZ);
            let mut value = Vec3A::new(x, y, z);
            if frame > 0 {
                let previous = result[index - bone_count];
                value.x = unwrap_angle(previous.x, value.x);
                value.y = unwrap_angle(previous.y, value.y);
                value.z = unwrap_angle(previous.z, value.z);
            }
            result[index] = value;
        }
    }
    result
}

fn unwrap_angle(previous: f32, value: f32) -> f32 {
    let turns = ((previous - value) / std::f32::consts::TAU).round();
    value + turns * std::f32::consts::TAU
}

fn fit_dcc_bone_segment(
    input: DensePoseSequenceView<'_>,
    bone: usize,
    start: usize,
    end: usize,
    translations: &[Vec3A],
    euler_xyz: &[Vec3A],
) -> DccCubicSegment {
    let bone_count = input.bone_count;
    let translation = std::array::from_fn::<_, 3, _>(|axis| {
        fit_dcc_scalar_segment(
            start,
            end,
            |sample| translations[sample * bone_count + bone].to_array()[axis],
            |sample| input.sample_frame(sample),
        )
    });
    let rotation = std::array::from_fn::<_, 3, _>(|axis| {
        fit_dcc_scalar_segment(
            start,
            end,
            |sample| euler_xyz[sample * bone_count + bone].to_array()[axis],
            |sample| input.sample_frame(sample),
        )
    });
    DccCubicSegment {
        translation_out_tangent: Vec3A::new(
            translation[0].out_tangent,
            translation[1].out_tangent,
            translation[2].out_tangent,
        ),
        translation_in_tangent: Vec3A::new(
            translation[0].in_tangent,
            translation[1].in_tangent,
            translation[2].in_tangent,
        ),
        rotation_start_euler_xyz: euler_xyz[start * bone_count + bone],
        rotation_end_euler_xyz: euler_xyz[end * bone_count + bone],
        rotation_out_tangent: Vec3A::new(
            rotation[0].out_tangent,
            rotation[1].out_tangent,
            rotation[2].out_tangent,
        ),
        rotation_in_tangent: Vec3A::new(
            rotation[0].in_tangent,
            rotation[1].in_tangent,
            rotation[2].in_tangent,
        ),
    }
}

fn fit_dcc_scalar_segment(
    start: usize,
    end: usize,
    value_at: impl Fn(usize) -> f32,
    frame_at: impl Fn(usize) -> f32,
) -> DccScalarSegment {
    let duration = frame_at(end) - frame_at(start);
    let start_value = value_at(start);
    let end_value = value_at(end);
    let slope = (end_value - start_value) / duration;
    if end <= start + 1 {
        return DccScalarSegment {
            out_tangent: slope,
            in_tangent: slope,
        };
    }

    let mut aa = 0.0;
    let mut ab = 0.0;
    let mut bb = 0.0;
    let mut ar = 0.0;
    let mut br = 0.0;
    for sample in start + 1..end {
        let t = segment_amount(start, end, sample, &frame_at);
        let (h00, h10, h01, h11) = hermite_basis(t);
        let a = h10 * duration;
        let b = h11 * duration;
        let residual = value_at(sample) - h00 * start_value - h01 * end_value;
        aa += a * a;
        ab += a * b;
        bb += b * b;
        ar += a * residual;
        br += b * residual;
    }
    let determinant = aa * bb - ab * ab;
    let (mut out_tangent, mut in_tangent) = if determinant.abs() > f32::EPSILON {
        (
            (ar * bb - br * ab) / determinant,
            (br * aa - ar * ab) / determinant,
        )
    } else {
        (slope, slope)
    };
    clamp_monotonic_tangents(slope, &mut out_tangent, &mut in_tangent);
    DccScalarSegment {
        out_tangent,
        in_tangent,
    }
}

fn clamp_monotonic_tangents(slope: f32, out_tangent: &mut f32, in_tangent: &mut f32) {
    if slope.abs() <= f32::EPSILON {
        *out_tangent = 0.0;
        *in_tangent = 0.0;
        return;
    }
    if *out_tangent * slope < 0.0 {
        *out_tangent = 0.0;
    }
    if *in_tangent * slope < 0.0 {
        *in_tangent = 0.0;
    }
    let alpha = *out_tangent / slope;
    let beta = *in_tangent / slope;
    let length = alpha.hypot(beta);
    if length > 3.0 {
        let scale = 3.0 / length;
        *out_tangent = scale * alpha * slope;
        *in_tangent = scale * beta * slope;
    }
}

fn hermite_basis(t: f32) -> (f32, f32, f32, f32) {
    let t2 = t * t;
    let t3 = t2 * t;
    (
        2.0 * t3 - 3.0 * t2 + 1.0,
        t3 - 2.0 * t2 + t,
        -2.0 * t3 + 3.0 * t2,
        t3 - t2,
    )
}

fn sample_hermite(
    start: f32,
    end: f32,
    out_tangent: f32,
    in_tangent: f32,
    duration: f32,
    t: f32,
) -> f32 {
    let (h00, h10, h01, h11) = hermite_basis(t);
    h00 * start + h10 * duration * out_tangent + h01 * end + h11 * duration * in_tangent
}

fn sample_dcc_vec3(
    start: Vec3A,
    end: Vec3A,
    out_tangent: Vec3A,
    in_tangent: Vec3A,
    duration: f32,
    t: f32,
) -> Vec3A {
    Vec3A::new(
        sample_hermite(start.x, end.x, out_tangent.x, in_tangent.x, duration, t),
        sample_hermite(start.y, end.y, out_tangent.y, in_tangent.y, duration, t),
        sample_hermite(start.z, end.z, out_tangent.z, in_tangent.z, duration, t),
    )
}

fn sample_bone_track(
    track: &ReducedBoneTrack,
    sample_frames: &[f32],
    frame: f32,
    target: ReductionTarget,
) -> (Vec3A, Quat) {
    let upper = track
        .keys
        .partition_point(|key| sample_frames[key.sample_index] <= frame);
    if upper == 0 {
        return (track.keys[0].translation, track.keys[0].rotation);
    }
    if upper == track.keys.len() {
        let key = track.keys[track.keys.len() - 1];
        return (key.translation, key.rotation);
    }
    let left = track.keys[upper - 1];
    let right = track.keys[upper];
    let amount = ((frame - sample_frames[left.sample_index])
        / (sample_frames[right.sample_index] - sample_frames[left.sample_index]))
        .clamp(0.0, 1.0);
    if target == ReductionTarget::DccCubic {
        let duration = sample_frames[right.sample_index] - sample_frames[left.sample_index];
        let segment = right.dcc_segment;
        let translation = Vec3A::new(
            sample_hermite(
                left.translation.x,
                right.translation.x,
                segment.translation_out_tangent.x,
                segment.translation_in_tangent.x,
                duration,
                amount,
            ),
            sample_hermite(
                left.translation.y,
                right.translation.y,
                segment.translation_out_tangent.y,
                segment.translation_in_tangent.y,
                duration,
                amount,
            ),
            sample_hermite(
                left.translation.z,
                right.translation.z,
                segment.translation_out_tangent.z,
                segment.translation_in_tangent.z,
                duration,
                amount,
            ),
        );
        let euler = Vec3A::new(
            sample_hermite(
                segment.rotation_start_euler_xyz.x,
                segment.rotation_end_euler_xyz.x,
                segment.rotation_out_tangent.x,
                segment.rotation_in_tangent.x,
                duration,
                amount,
            ),
            sample_hermite(
                segment.rotation_start_euler_xyz.y,
                segment.rotation_end_euler_xyz.y,
                segment.rotation_out_tangent.y,
                segment.rotation_in_tangent.y,
                duration,
                amount,
            ),
            sample_hermite(
                segment.rotation_start_euler_xyz.z,
                segment.rotation_end_euler_xyz.z,
                segment.rotation_out_tangent.z,
                segment.rotation_in_tangent.z,
                duration,
                amount,
            ),
        );
        return (
            translation,
            normalize_quat(Quat::from_euler(EulerRot::XYZ, euler.x, euler.y, euler.z)),
        );
    }
    let translation_amount = if target == ReductionTarget::VmdBezier {
        Vec3A::new(
            right.vmd_interpolation.translation[0].evaluate(amount),
            right.vmd_interpolation.translation[1].evaluate(amount),
            right.vmd_interpolation.translation[2].evaluate(amount),
        )
    } else {
        Vec3A::splat(amount)
    };
    let rotation_amount = if target == ReductionTarget::VmdBezier {
        right.vmd_interpolation.rotation.evaluate(amount)
    } else {
        amount
    };
    (
        Vec3A::new(
            left.translation.x + (right.translation.x - left.translation.x) * translation_amount.x,
            left.translation.y + (right.translation.y - left.translation.y) * translation_amount.y,
            left.translation.z + (right.translation.z - left.translation.z) * translation_amount.z,
        ),
        normalize_quat(left.rotation.slerp(right.rotation, rotation_amount)),
    )
}

fn sample_morph_track(
    track: &ReducedMorphTrack,
    sample_frames: &[f32],
    frame: f32,
    target: ReductionTarget,
) -> f32 {
    let upper = track
        .keys
        .partition_point(|key| sample_frames[key.sample_index] <= frame);
    if upper == 0 {
        return track.keys[0].weight;
    }
    if upper == track.keys.len() {
        return track.keys[track.keys.len() - 1].weight;
    }
    let left = track.keys[upper - 1];
    let right = track.keys[upper];
    let amount = ((frame - sample_frames[left.sample_index])
        / (sample_frames[right.sample_index] - sample_frames[left.sample_index]))
        .clamp(0.0, 1.0);
    if target == ReductionTarget::DccCubic {
        let duration = sample_frames[right.sample_index] - sample_frames[left.sample_index];
        return sample_hermite(
            left.weight,
            right.weight,
            right.dcc_segment.out_tangent,
            right.dcc_segment.in_tangent,
            duration,
            amount,
        );
    }
    left.weight + (right.weight - left.weight) * amount
}

fn build_world_matrices_into(
    snapshot: &SkeletonSnapshot,
    translations: &[Vec3A],
    rotations: &[Quat],
    result: &mut Vec<Mat4>,
) {
    result.resize(snapshot.bone_count(), Mat4::IDENTITY);
    for &bone in snapshot.evaluation_order.iter() {
        let local = Mat4::from_rotation_translation(rotations[bone], translations[bone].into());
        result[bone] = if snapshot.parent_indices[bone] < 0 {
            local
        } else {
            result[snapshot.parent_indices[bone] as usize] * local
        };
    }
}

fn build_evaluation_order(parents: &[i32]) -> Result<Vec<usize>, PoseReductionError> {
    fn visit(
        bone: usize,
        parents: &[i32],
        state: &mut [u8],
        order: &mut Vec<usize>,
    ) -> Result<(), PoseReductionError> {
        match state[bone] {
            1 => return Err(PoseReductionError::SkeletonCycle { bone }),
            2 => return Ok(()),
            _ => {}
        }
        state[bone] = 1;
        if parents[bone] >= 0 {
            visit(parents[bone] as usize, parents, state, order)?;
        }
        state[bone] = 2;
        order.push(bone);
        Ok(())
    }
    let mut state = vec![0u8; parents.len()];
    let mut order = Vec::with_capacity(parents.len());
    for bone in 0..parents.len() {
        visit(bone, parents, &mut state, &mut order)?;
    }
    Ok(order)
}

fn normalize_quat(value: Quat) -> Quat {
    if value.length_squared() <= f32::EPSILON {
        Quat::IDENTITY
    } else {
        value.normalize()
    }
}

fn quat_is_finite(value: Quat) -> bool {
    value.to_array().iter().all(|value| value.is_finite()) && value.length_squared() > f32::EPSILON
}

fn quat_angle(a: Quat, b: Quat) -> f32 {
    2.0 * a.dot(b).abs().clamp(-1.0, 1.0).acos()
}

fn normalized_error(error: f32, tolerance: f32) -> f32 {
    if tolerance == 0.0 {
        if error == 0.0 { 0.0 } else { f32::INFINITY }
    } else {
        error / tolerance
    }
}

fn segment_amount(start: usize, end: usize, sample: usize, frame_at: impl Fn(usize) -> f32) -> f32 {
    let start_frame = frame_at(start);
    (frame_at(sample) - start_frame) / (frame_at(end) - start_frame)
}

fn is_worse(current: Option<(f32, usize)>, error: f32, frame: usize) -> bool {
    current.is_none_or(|(best, best_frame)| error > best || (error == best && frame < best_frame))
}

#[cfg(test)]
mod tests;
