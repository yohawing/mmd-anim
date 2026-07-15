use glam::{Mat4, Quat, Vec3, Vec3A};

use super::*;

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
