use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

use glam::{Mat4, Quat, Vec3, Vec3A};

use super::*;

static TEST_ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static COUNT_TEST_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
}

struct TestCountingAllocator;

fn record_test_allocation() {
    if COUNT_TEST_ALLOCATIONS.try_with(Cell::get).unwrap_or(false) {
        TEST_ALLOCATIONS.fetch_add(1, AtomicOrdering::Relaxed);
    }
}

unsafe impl GlobalAlloc for TestCountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_test_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_test_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_test_allocation();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static TEST_GLOBAL_ALLOCATOR: TestCountingAllocator = TestCountingAllocator;

fn count_test_allocations(f: impl FnOnce()) -> usize {
    COUNT_TEST_ALLOCATIONS.with(|enabled| {
        enabled.set(false);
        TEST_ALLOCATIONS.store(0, AtomicOrdering::Relaxed);
        enabled.set(true);
        f();
        enabled.set(false);
    });
    TEST_ALLOCATIONS.load(AtomicOrdering::Relaxed)
}

fn snapshot() -> SkeletonSnapshot {
    SkeletonSnapshot::new(
        vec![-1, 0],
        vec![Vec3A::ZERO, Vec3A::X],
        vec![Quat::IDENTITY; 2],
        1,
        42,
    )
    .unwrap()
}

fn dense_world(frame_count: usize) -> Vec<Mat4> {
    let mut result = Vec::new();
    for frame in 0..frame_count {
        let root = Mat4::from_rotation_translation(
            Quat::from_rotation_z(frame as f32 * 0.1),
            Vec3::new(frame as f32, 0.0, 0.0),
        );
        result.push(root);
        result.push(root * Mat4::from_translation(Vec3::X));
    }
    result
}

#[test]
fn reduces_exact_linear_translation_and_constant_child_to_endpoints() {
    let world = dense_world(5);
    let morphs = [0.0, 0.25, 0.5, 0.75, 1.0];
    let input = DensePoseSequenceView::new(&world, &morphs, 5, 2, 1, 0.0, 1.0).unwrap();
    let reduced = reduce_dense_pose_sequence(
        input,
        snapshot(),
        ReductionTolerances {
            local_rotation_radians: 0.001,
            world_rotation_radians: 0.001,
            ..Default::default()
        },
        ReductionTarget::LinearSlerp,
    )
    .unwrap();
    assert_eq!(reduced.bone_tracks()[0].keys().len(), 2);
    assert_eq!(reduced.bone_tracks()[1].keys().len(), 2);
    assert_eq!(reduced.morph_tracks()[0].keys().len(), 2);
    assert!(reduced.report().max_world_position_error <= 1.0e-4);
    assert_eq!(reduced.snapshot().model_identity(), 42);
}

#[test]
fn deterministic_peak_split_keeps_the_peak() {
    let world = [
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::Y),
        Mat4::IDENTITY,
    ];
    let input = DensePoseSequenceView::new(&world, &[], 3, 1, 0, 0.0, 1.0).unwrap();
    let snapshot =
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 1).unwrap();
    let reduced = reduce_dense_pose_sequence(
        input,
        snapshot,
        Default::default(),
        ReductionTarget::LinearSlerp,
    )
    .unwrap();
    assert_eq!(
        reduced.bone_tracks()[0]
            .keys()
            .iter()
            .map(|key| key.sample_index)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}

#[test]
fn sample_reconstructs_world_without_runtime_procedural_layers() {
    let world = dense_world(3);
    let input = DensePoseSequenceView::new(&world, &[0.0, 0.5, 1.0], 3, 2, 1, 10.0, 0.5).unwrap();
    let reduced = reduce_dense_pose_sequence(
        input,
        snapshot(),
        Default::default(),
        ReductionTarget::LinearSlerp,
    )
    .unwrap();
    let sample = reduced.sample(10.5).unwrap();
    for (actual, expected) in sample.world_matrices.iter().zip(&world[2..4]) {
        for (a, e) in actual.to_cols_array().iter().zip(expected.to_cols_array()) {
            assert!((a - e).abs() < 1.0e-4);
        }
    }
    assert!((sample.morph_weights[0] - 0.5).abs() < 1.0e-6);
}

#[test]
fn sample_into_reuses_scratch_without_allocating_and_matches_sample() {
    let world = dense_world(7);
    let morphs = [0.0, 0.2, 0.8, 0.3, 0.7, 0.9, 1.0];
    let input = DensePoseSequenceView::new(&world, &morphs, 7, 2, 1, 0.0, 1.0).unwrap();
    let reduced = reduce_dense_pose_sequence(
        input,
        snapshot(),
        ReductionTolerances {
            local_position: 0.01,
            local_rotation_radians: 0.01,
            world_position: 0.01,
            world_rotation_radians: 0.01,
            morph_weight: 0.01,
        },
        ReductionTarget::DccCubic,
    )
    .unwrap();
    let expected = reduced.sample(2.5).unwrap();
    let mut scratch = ReducedPoseScratch::default();
    reduced.sample_into(2.5, &mut scratch).unwrap();

    let allocations = count_test_allocations(|| {
        reduced.sample_into(black_box(2.5), &mut scratch).unwrap();
    });

    assert_eq!(allocations, 0);
    assert_eq!(scratch.local_translations, expected.local_translations);
    assert_eq!(scratch.local_rotations, expected.local_rotations);
    assert_eq!(scratch.world_matrices, expected.world_matrices);
    assert_eq!(scratch.morph_weights, expected.morph_weights);
}

#[test]
fn reduction_work_stats_are_deterministic_and_separate_from_quality_report() {
    let world = dense_world(7);
    let morphs = [0.0, 0.2, 0.8, 0.3, 0.7, 0.9, 1.0];
    let reduce = || {
        reduce_dense_pose_sequence(
            DensePoseSequenceView::new(&world, &morphs, 7, 2, 1, 0.0, 1.0).unwrap(),
            snapshot(),
            ReductionTolerances {
                local_position: 0.01,
                local_rotation_radians: 0.01,
                world_position: 0.01,
                world_rotation_radians: 0.01,
                morph_weight: 0.01,
            },
            ReductionTarget::DccCubic,
        )
        .unwrap()
    };

    let first = reduce();
    let second = reduce();
    let stats = first.work_stats();
    assert_eq!(stats, second.work_stats());
    assert_eq!(first.report(), second.report());
    assert_eq!(stats.candidate_rebuilds, stats.global_validation_passes);
    assert_eq!(
        stats.added_keys_per_pass.len(),
        stats.global_validation_passes
    );
    assert_eq!(stats.added_keys_per_pass.last(), Some(&0));
    assert_eq!(
        stats.normal_key_additions + stats.ancestor_key_additions,
        stats.added_keys_per_pass.iter().sum::<usize>()
    );
    assert_eq!(stats.local_prefit_bone_segment_fits, 0);
    assert_eq!(stats.local_prefit_morph_segment_fits, 0);
    assert_eq!(stats.bone_samples, stats.global_validation_passes * 7 * 2);
    assert_eq!(stats.morph_samples, stats.global_validation_passes * 7);
    assert_eq!(stats.world_rebuilds, stats.global_validation_passes * 7);
    assert_eq!(
        stats.world_rotation_decompositions,
        (stats.global_validation_passes + 1) * 7 * 2
    );
    assert!(stats.dcc_bone_segment_fits > 0);
    assert!(stats.dcc_morph_segment_fits > 0);
}

#[test]
fn dcc_local_prefit_records_separate_work_for_long_sequences() {
    let frame_count = 97;
    let world = dense_world(frame_count);
    let morphs = (0..frame_count)
        .map(|frame| ((frame as f32 * 0.37).sin() * 0.5 + 0.5).clamp(0.0, 1.0))
        .collect::<Vec<_>>();
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(&world, &morphs, frame_count, 2, 1, 0.0, 1.0).unwrap(),
        snapshot(),
        ReductionTolerances {
            local_position: 0.01,
            local_rotation_radians: 0.01,
            world_position: 0.01,
            world_rotation_radians: 0.01,
            morph_weight: 0.01,
        },
        ReductionTarget::DccCubic,
    )
    .unwrap();
    let stats = reduced.work_stats();

    assert!(stats.local_prefit_bone_segment_fits > 0);
    assert!(stats.local_prefit_morph_segment_fits > 0);
    assert!(stats.local_prefit_morph_key_additions > 0);
    assert!(stats.local_prefit_bone_samples > 0);
    assert!(stats.local_prefit_morph_samples > 0);
}

#[test]
fn frame_validation_is_deterministic_across_worker_counts() {
    let frame_count = 97;
    let world = dense_world(frame_count);
    let morphs = (0..frame_count)
        .map(|frame| ((frame as f32 * 0.73).sin() * 0.5 + 0.5).clamp(0.0, 1.0))
        .collect::<Vec<_>>();
    let reduce = |workers| {
        reduce_dense_pose_sequence_with_worker_count(
            DensePoseSequenceView::new(&world, &morphs, frame_count, 2, 1, 0.0, 1.0).unwrap(),
            snapshot(),
            ReductionTolerances {
                local_position: 0.01,
                local_rotation_radians: 0.01,
                world_position: 0.01,
                world_rotation_radians: 0.01,
                morph_weight: 0.01,
            },
            ReductionTarget::DccCubic,
            workers,
        )
        .unwrap()
    };

    let single = reduce(1);
    let two = reduce(2);
    let four = reduce(4);
    assert_eq!(single, two);
    assert_eq!(single, four);
    assert_eq!(single.work_stats(), two.work_stats());
    assert_eq!(single.work_stats(), four.work_stats());
}

#[test]
fn worst_error_order_is_normalized_then_frame_then_track() {
    let mut errors = [
        WorstError {
            normalized_error: 2.0,
            frame: 4,
            track: ErrorTrack::Morph(0),
        },
        WorstError {
            normalized_error: 3.0,
            frame: 8,
            track: ErrorTrack::Bone(2),
        },
        WorstError {
            normalized_error: 2.0,
            frame: 3,
            track: ErrorTrack::Bone(5),
        },
        WorstError {
            normalized_error: 2.0,
            frame: 4,
            track: ErrorTrack::Bone(1),
        },
    ];
    errors.sort_by(compare_worst_errors);

    assert_eq!(errors[0].normalized_error, 3.0);
    assert_eq!(errors[1].frame, 3);
    assert_eq!(errors[2].track, ErrorTrack::Bone(1));
    assert_eq!(errors[3].track, ErrorTrack::Morph(0));
}

#[test]
fn rejects_non_finite_scale_shear_and_invalid_time_base_atomically() {
    let mut non_finite = Mat4::IDENTITY;
    non_finite.x_axis.x = f32::NAN;
    let non_finite_world = [non_finite];
    let view = DensePoseSequenceView::new(&non_finite_world, &[], 1, 1, 0, 0.0, 1.0).unwrap();
    let one =
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 0).unwrap();
    assert!(matches!(
        reduce_dense_pose_sequence(
            view,
            one.clone(),
            Default::default(),
            ReductionTarget::LinearSlerp
        ),
        Err(PoseReductionError::NonFiniteMatrix { .. })
    ));

    let scaled = Mat4::from_scale(Vec3::new(2.0, 1.0, 1.0));
    let scaled_world = [scaled];
    let view = DensePoseSequenceView::new(&scaled_world, &[], 1, 1, 0, 0.0, 1.0).unwrap();
    assert!(matches!(
        reduce_dense_pose_sequence(view, one, Default::default(), ReductionTarget::LinearSlerp),
        Err(PoseReductionError::ScaleOrShear { .. })
    ));
    assert_eq!(
        DensePoseSequenceView::new(&[Mat4::IDENTITY], &[], 1, 1, 0, 0.0, 0.0).unwrap_err(),
        PoseReductionError::InvalidTimeBase
    );
}

#[test]
fn rejects_snapshot_mismatch_and_hierarchy_cycles() {
    let world = [Mat4::IDENTITY];
    let input = DensePoseSequenceView::new(&world, &[], 1, 1, 0, 0.0, 1.0).unwrap();
    assert!(matches!(
        reduce_dense_pose_sequence(
            input,
            snapshot(),
            Default::default(),
            ReductionTarget::LinearSlerp
        ),
        Err(PoseReductionError::SnapshotMismatch)
    ));
    assert!(matches!(
        SkeletonSnapshot::new(
            vec![1, 0],
            vec![Vec3A::ZERO; 2],
            vec![Quat::IDENTITY; 2],
            0,
            0
        ),
        Err(PoseReductionError::SkeletonCycle { .. })
    ));
}

#[test]
fn independent_dense_inputs_produce_identical_results() {
    let world_a = dense_world(7);
    let world_b = dense_world(7);
    let morphs = vec![0.0; 7];
    let reduce = |world: &[Mat4]| {
        reduce_dense_pose_sequence(
            DensePoseSequenceView::new(world, &morphs, 7, 2, 1, 0.0, 1.0).unwrap(),
            snapshot(),
            Default::default(),
            ReductionTarget::LinearSlerp,
        )
        .unwrap()
    };
    assert_eq!(reduce(&world_a), reduce(&world_b));
}

#[test]
fn preserves_physics_seed_sample_as_the_first_endpoint() {
    let world = [
        Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0)),
        Mat4::from_translation(Vec3::new(3.0, 3.5, 5.0)),
        Mat4::from_translation(Vec3::new(3.0, 3.0, 5.0)),
    ];
    let input = DensePoseSequenceView::new(&world, &[], 3, 1, 0, 12.0, 1.0).unwrap();
    let one =
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 7).unwrap();
    let reduced =
        reduce_dense_pose_sequence(input, one, Default::default(), ReductionTarget::LinearSlerp)
            .unwrap();
    let seed = reduced.sample(12.0).unwrap();
    assert_eq!(seed.world_matrices[0], world[0]);
    assert_eq!(reduced.bone_tracks()[0].keys()[0].sample_index, 0);
}

#[test]
fn impossible_zero_tolerance_returns_instead_of_looping() {
    let world = dense_world(5);
    let input = DensePoseSequenceView::new(&world, &[], 5, 2, 0, 0.0, 1.0).unwrap();
    let zero = ReductionTolerances {
        local_position: 0.0,
        local_rotation_radians: 0.0,
        world_position: 0.0,
        world_rotation_radians: 0.0,
        morph_weight: 0.0,
    };
    let zero_morph_snapshot = SkeletonSnapshot::new(
        vec![-1, 0],
        vec![Vec3A::ZERO, Vec3A::X],
        vec![Quat::IDENTITY; 2],
        0,
        42,
    )
    .unwrap();
    assert!(matches!(
        reduce_dense_pose_sequence(
            input,
            zero_morph_snapshot,
            zero,
            ReductionTarget::LinearSlerp
        ),
        Err(PoseReductionError::ToleranceUnattainable { .. })
    ));
}

#[test]
fn rejects_time_base_whose_adjacent_f32_samples_collapse() {
    let world = [Mat4::IDENTITY; 2];
    assert_eq!(
        DensePoseSequenceView::new(&world, &[], 2, 1, 0, 16_777_216.0, 1.0).unwrap_err(),
        PoseReductionError::InvalidTimeBase
    );
}

#[test]
fn large_start_frame_samples_by_stored_f32_timestamps() {
    let world = [
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::Y),
        Mat4::IDENTITY,
    ];
    let start = 1_000_000.0f32;
    let step = 0.1f32;
    let input = DensePoseSequenceView::new(&world, &[], 3, 1, 0, start, step).unwrap();
    let one =
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 9).unwrap();
    let reduced =
        reduce_dense_pose_sequence(input, one, Default::default(), ReductionTarget::LinearSlerp)
            .unwrap();
    let middle_frame = start + step;
    let sample = reduced.sample(middle_frame).unwrap();
    assert_eq!(sample.world_matrices[0], world[1]);
}

#[test]
fn custom_tolerance_changes_key_count_and_bounds_report() {
    let world = [
        Mat4::IDENTITY,
        Mat4::from_translation(Vec3::new(0.0, 0.5, 0.0)),
        Mat4::IDENTITY,
    ];
    let make = |position_tolerance| {
        let input = DensePoseSequenceView::new(&world, &[], 3, 1, 0, 0.0, 1.0).unwrap();
        let one = SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 10)
            .unwrap();
        reduce_dense_pose_sequence(
            input,
            one,
            ReductionTolerances {
                local_position: position_tolerance,
                world_position: position_tolerance,
                ..Default::default()
            },
            ReductionTarget::LinearSlerp,
        )
        .unwrap()
    };
    let loose = make(1.0);
    let strict = make(0.1);
    assert_eq!(loose.bone_tracks()[0].keys().len(), 2);
    assert_eq!(strict.bone_tracks()[0].keys().len(), 3);
    assert!(loose.report().max_local_position_error <= 1.0);
    assert!(loose.report().max_world_position_error <= 1.0);
    assert!(strict.report().max_local_position_error <= 0.1);
    assert!(strict.report().max_world_position_error <= 0.1);
    assert!(strict.report().max_local_rotation_error_radians <= 1.0e-4);
    assert!(strict.report().max_world_rotation_error_radians <= 1.0e-4);
    assert!(strict.report().max_morph_weight_error <= 1.0e-4);
}

#[test]
fn vmd_bezier_target_fits_quantized_curve_and_samples_with_it() {
    let source_curve = QuantizedBezier {
        x1: 30,
        y1: 10,
        x2: 100,
        y2: 120,
    };
    let frame_count = 17;
    let fitted = fit_quantized_bezier(
        0,
        frame_count - 1,
        |frame| source_curve.evaluate(frame as f32 / (frame_count - 1) as f32),
        |frame| frame as f32,
    );
    let fit_error = (1..frame_count - 1)
        .map(|frame| {
            let time = frame as f32 / (frame_count - 1) as f32;
            (fitted.evaluate(time) - source_curve.evaluate(time)).abs()
        })
        .fold(0.0f32, f32::max);
    assert!(fit_error <= 0.005, "{fitted:?} {fit_error}");
    let world = (0..frame_count)
        .map(|frame| {
            let time = frame as f32 / (frame_count - 1) as f32;
            Mat4::from_translation(Vec3::new(source_curve.evaluate(time), 0.0, 0.0))
        })
        .collect::<Vec<_>>();
    let input = DensePoseSequenceView::new(&world, &[], frame_count, 1, 0, 0.0, 1.0).unwrap();
    let one =
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 11).unwrap();
    let reduced = reduce_dense_pose_sequence(
        input,
        one,
        ReductionTolerances {
            local_position: 0.005,
            world_position: 0.005,
            ..Default::default()
        },
        ReductionTarget::VmdBezier,
    )
    .unwrap();
    let interpolation = reduced.bone_tracks()[0].keys()[1]
        .vmd_interpolation
        .translation[0];
    assert_eq!(
        reduced.bone_tracks()[0].keys().len(),
        2,
        "{interpolation:?}"
    );
    assert!(interpolation.x1 <= 127 && interpolation.y1 <= 127);
    assert!(interpolation.x2 <= 127 && interpolation.y2 <= 127);
    assert!(reduced.report().max_world_position_error <= 0.005);
}

#[test]
fn vmd_bezier_world_rotation_gate_handles_near_pi_motion() {
    let frame_count = 9;
    let world = (0..frame_count)
        .map(|frame| {
            let amount = frame as f32 / (frame_count - 1) as f32;
            Mat4::from_quat(Quat::from_rotation_y(
                (std::f32::consts::PI - 0.01) * amount,
            ))
        })
        .collect::<Vec<_>>();
    let input = DensePoseSequenceView::new(&world, &[], frame_count, 1, 0, 0.0, 1.0).unwrap();
    let one =
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 12).unwrap();
    let reduced = reduce_dense_pose_sequence(
        input,
        one,
        ReductionTolerances {
            local_rotation_radians: 0.002,
            world_rotation_radians: 0.002,
            ..Default::default()
        },
        ReductionTarget::VmdBezier,
    )
    .unwrap();
    assert!(reduced.report().max_world_rotation_error_radians <= 0.002);
}

#[test]
fn dcc_hermite_matches_independent_reference_equation() {
    let start = -2.0;
    let end = 3.0;
    let out_tangent = 1.25;
    let in_tangent = -0.75;
    let duration = 4.0;
    for step in 0..=16 {
        let t = step as f32 / 16.0;
        let t2 = t * t;
        let t3 = t2 * t;
        let reference = (2.0 * t3 - 3.0 * t2 + 1.0) * start
            + (t3 - 2.0 * t2 + t) * duration * out_tangent
            + (-2.0 * t3 + 3.0 * t2) * end
            + (t3 - t2) * duration * in_tangent;
        assert!(
            (sample_hermite(start, end, out_tangent, in_tangent, duration, t) - reference).abs()
                <= f32::EPSILON
        );
    }
}

#[test]
fn dcc_cubic_uses_fewer_keys_for_a_single_smooth_peak() {
    let frame_count = 9;
    let world = (0..frame_count)
        .map(|frame| {
            let t = frame as f32 / (frame_count - 1) as f32;
            Mat4::from_translation(Vec3::new(4.0 * t * (1.0 - t), 0.0, 0.0))
        })
        .collect::<Vec<_>>();
    let morphs = (0..frame_count)
        .map(|frame| {
            let t = frame as f32 / (frame_count - 1) as f32;
            4.0 * t * (1.0 - t)
        })
        .collect::<Vec<_>>();
    let reduce = |target| {
        reduce_dense_pose_sequence(
            DensePoseSequenceView::new(&world, &morphs, frame_count, 1, 1, 0.0, 1.0).unwrap(),
            SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 1, 13)
                .unwrap(),
            ReductionTolerances {
                local_position: 0.01,
                world_position: 0.01,
                morph_weight: 0.01,
                ..Default::default()
            },
            target,
        )
        .unwrap()
    };
    let linear = reduce(ReductionTarget::LinearSlerp);
    let cubic = reduce(ReductionTarget::DccCubic);
    assert_eq!(
        cubic.bone_tracks()[0].keys().len(),
        3,
        "{:?} {:?}",
        cubic.report(),
        cubic.bone_tracks()[0].keys()
    );
    assert_eq!(cubic.morph_tracks()[0].keys().len(), 3);
    assert!(
        cubic.report().reduced_bone_key_count < linear.report().reduced_bone_key_count
            && cubic.report().reduced_morph_key_count < linear.report().reduced_morph_key_count
    );
    assert!(cubic.report().max_world_position_error <= 0.01);
    assert!(cubic.report().max_morph_weight_error <= 0.01);
}

#[test]
fn dcc_constant_tangent_motion_is_finite_and_exact() {
    let constant = [2.0f32; 4];
    let constant_segment = fit_dcc_scalar_segment(
        0,
        constant.len() - 1,
        |sample| constant[sample],
        |sample| sample as f32,
    );
    assert_eq!(constant_segment, DccScalarSegment::default());
    for step in 0..=16 {
        assert_eq!(
            sample_hermite(
                constant[0],
                constant[constant.len() - 1],
                constant_segment.out_tangent,
                constant_segment.in_tangent,
                (constant.len() - 1) as f32,
                step as f32 / 16.0,
            ),
            2.0
        );
    }

    let frame_count = 6;
    let world = (0..frame_count)
        .map(|frame| {
            Mat4::from_rotation_translation(
                Quat::from_rotation_z(frame as f32 * 0.1),
                Vec3::new(frame as f32 * 0.25, 0.0, 0.0),
            )
        })
        .collect::<Vec<_>>();
    let morphs = (0..frame_count)
        .map(|frame| frame as f32 * 0.2)
        .collect::<Vec<_>>();
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(&world, &morphs, frame_count, 1, 1, 0.0, 1.0).unwrap(),
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 1, 16).unwrap(),
        ReductionTolerances {
            local_position: 1.0e-5,
            local_rotation_radians: 1.0e-4,
            world_position: 1.0e-5,
            world_rotation_radians: 1.0e-4,
            morph_weight: 1.0e-5,
        },
        ReductionTarget::DccCubic,
    )
    .unwrap();
    assert_eq!(reduced.bone_tracks()[0].keys().len(), 2);
    assert_eq!(reduced.morph_tracks()[0].keys().len(), 2);
    let bone_segment = reduced.bone_tracks()[0].keys()[1].dcc_segment;
    let morph_segment = reduced.morph_tracks()[0].keys()[1].dcc_segment;
    assert!((bone_segment.translation_out_tangent.x - 0.25).abs() <= 1.0e-5);
    assert!((bone_segment.translation_in_tangent.x - 0.25).abs() <= 1.0e-5);
    assert!((bone_segment.rotation_out_tangent.z - 0.1).abs() <= 1.0e-4);
    assert!((bone_segment.rotation_in_tangent.z - 0.1).abs() <= 1.0e-4);
    assert!((morph_segment.out_tangent - 0.2).abs() <= 1.0e-5);
    assert!((morph_segment.in_tangent - 0.2).abs() <= 1.0e-5);
}

#[test]
fn dcc_euler_xyz_unwraps_across_pi_without_flips() {
    let degrees = [170.0f32, 175.0, 179.0, 181.0, 185.0, 190.0];
    let world = degrees
        .iter()
        .map(|degrees| Mat4::from_quat(Quat::from_rotation_x(degrees.to_radians())))
        .collect::<Vec<_>>();
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(&world, &[], world.len(), 1, 0, 0.0, 1.0).unwrap(),
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 14).unwrap(),
        ReductionTolerances {
            local_rotation_radians: 0.002,
            world_rotation_radians: 0.002,
            ..Default::default()
        },
        ReductionTarget::DccCubic,
    )
    .unwrap();
    assert!(reduced.report().max_world_rotation_error_radians <= 0.002);
    for (frame, degrees) in degrees.iter().enumerate() {
        let sample = reduced.sample(frame as f32).unwrap();
        assert!(sample.local_rotations[0].is_finite());
        assert!(
            quat_angle(
                Quat::from_rotation_x(degrees.to_radians()),
                sample.local_rotations[0]
            ) <= 0.002
        );
    }
}

#[test]
fn dcc_broken_tangents_stay_finite_and_do_not_overshoot_monotonic_segments() {
    let values = [0.0f32, 0.85, 0.9, 1.0];
    let segment = fit_dcc_scalar_segment(
        0,
        values.len() - 1,
        |sample| values[sample],
        |sample| sample as f32,
    );
    assert!(segment.out_tangent.is_finite() && segment.in_tangent.is_finite());
    for step in 0..=128 {
        let value = sample_hermite(
            values[0],
            values[values.len() - 1],
            segment.out_tangent,
            segment.in_tangent,
            (values.len() - 1) as f32,
            step as f32 / 128.0,
        );
        assert!(
            (-1.0e-6..=1.0 + 1.0e-6).contains(&value),
            "{segment:?} {value}"
        );
    }

    let extrema = [0.0f32, 1.0, -1.0, 1.0, 0.0];
    let world = extrema
        .iter()
        .map(|value| Mat4::from_translation(Vec3::new(*value, 0.0, 0.0)))
        .collect::<Vec<_>>();
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(&world, &[], world.len(), 1, 0, 0.0, 1.0).unwrap(),
        SkeletonSnapshot::new(vec![-1], vec![Vec3A::ZERO], vec![Quat::IDENTITY], 0, 15).unwrap(),
        ReductionTolerances {
            local_position: 0.01,
            world_position: 0.01,
            ..Default::default()
        },
        ReductionTarget::DccCubic,
    )
    .unwrap();
    assert!(reduced.report().max_world_position_error <= 0.01);
    assert!(reduced.bone_tracks()[0].keys().iter().all(|key| {
        key.dcc_segment.translation_out_tangent.is_finite()
            && key.dcc_segment.translation_in_tangent.is_finite()
    }));
}
