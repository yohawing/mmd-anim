use glam::{Mat4, Quat, Vec3, Vec3A};
use mmd_anim_runtime::{
    BoneIndex, BoneInit, DensePoseSequenceView, ModelArena, MorphIndex, ReducedPoseSequence,
    ReductionTarget, ReductionTolerances, RuntimeInstance, SkeletonSnapshot,
    reduce_dense_pose_sequence,
};
use std::sync::Arc;

use super::*;
use crate::vmd::{
    build_clip_from_import, export_vmd_animation, import_vmd_motion, parse_vmd_animation,
};

fn reduced(start_frame: f32, frame_step: f32, morphs: &[f32]) -> ReducedPoseSequence {
    let world = [
        Mat4::from_translation(Vec3::new(1.0, 0.0, 0.0)),
        Mat4::from_translation(Vec3::new(2.0, 0.5, 0.0)),
        Mat4::from_translation(Vec3::new(3.0, 0.0, 0.0)),
    ];
    let input = DensePoseSequenceView::new(
        &world,
        morphs,
        3,
        1,
        usize::from(!morphs.is_empty()),
        start_frame,
        frame_step,
    )
    .unwrap();
    let snapshot = SkeletonSnapshot::new(
        vec![-1],
        vec![Vec3A::ZERO],
        vec![Quat::IDENTITY],
        usize::from(!morphs.is_empty()),
        99,
    )
    .unwrap();
    reduce_dense_pose_sequence(
        input,
        snapshot,
        ReductionTolerances {
            local_position: 0.01,
            world_position: 0.01,
            ..Default::default()
        },
        ReductionTarget::VmdBezier,
    )
    .unwrap()
}

fn bindings(morph_kind: VmdExportMorphKind) -> VmdPoseExportBindings {
    VmdPoseExportBindings {
        model_identity: 99,
        model_name: VmdExportName::new("model", b"model".to_vec()),
        bone_names: vec![VmdExportName::new("bone", b"bone".to_vec())],
        morph_names: vec![VmdExportName::new("morph", b"morph".to_vec())],
        ik_names: vec![VmdExportName::new("ik", b"ik".to_vec())],
        ik_solver_count: 1,
        append_affected_bones: vec![false],
        morph_kinds: vec![morph_kind],
    }
}

#[test]
fn static_track_rotation_comparison_is_exact_and_hemisphere_invariant() {
    let rotation =
        Quat::from_xyzw(0.182_574_18, -0.365_148_37, 0.547_722_6, 0.730_296_73).normalize();
    assert_eq!(rotation_error(rotation, rotation), 0.0);
    assert_eq!(rotation_error(rotation, -rotation), 0.0);
}

#[test]
fn exports_quantized_64_byte_curves_ik_off_and_roundtrips() {
    let sequence = reduced(0.0, 1.0, &[0.0, 0.5, 1.0]);
    let exported =
        export_reduced_pose_to_vmd(&sequence, &bindings(VmdExportMorphKind::Vertex)).unwrap();
    assert!(exported.report.physics_must_be_disabled_by_host);
    assert!(exported.report.ik_disabled_in_vmd);
    assert!(!exported.animation.bone_frames.is_empty());
    for frame in &exported.animation.bone_frames {
        assert_eq!(frame.interpolation.len(), 64);
        assert!(frame.interpolation.iter().all(|value| *value <= 127));
    }
    assert_eq!(exported.animation.property_frames.len(), 1);
    assert!(
        exported.animation.property_frames[0]
            .ik_states
            .iter()
            .all(|state| !state.enabled)
    );
    let bytes = export_vmd_animation(&exported.animation);
    let reparsed = parse_vmd_animation(&bytes).unwrap();
    assert_eq!(
        reparsed.bone_frames.len(),
        exported.animation.bone_frames.len()
    );
    assert_eq!(
        reparsed.morph_frames.len(),
        exported.animation.morph_frames.len()
    );
    assert!(!reparsed.property_frames[0].ik_states[0].enabled);
    let mut unique = reparsed
        .bone_frames
        .iter()
        .map(|frame| frame.frame)
        .collect::<Vec<_>>();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), reparsed.bone_frames.len());
}

#[test]
fn rejects_fractional_frames_without_rounding() {
    let sequence = reduced(0.5, 1.0, &[]);
    let mut binding = bindings(VmdExportMorphKind::Vertex);
    binding.morph_names.clear();
    binding.morph_kinds.clear();
    binding.ik_names.clear();
    binding.ik_solver_count = 0;
    assert!(matches!(
        export_reduced_pose_to_vmd(&sequence, &binding),
        Err(VmdPoseExportError::NonIntegerFrame { .. })
    ));
}

#[test]
fn rejects_append_and_baked_deformation_morph_double_application() {
    let no_morph = reduced(0.0, 1.0, &[]);
    let mut append_binding = bindings(VmdExportMorphKind::Vertex);
    append_binding.morph_names.clear();
    append_binding.morph_kinds.clear();
    append_binding.append_affected_bones[0] = true;
    assert_eq!(
        export_reduced_pose_to_vmd(&no_morph, &append_binding).unwrap_err(),
        VmdPoseExportError::AppendTransformWouldDoubleApply { bone: 0 }
    );

    for kind in [
        VmdExportMorphKind::Bone,
        VmdExportMorphKind::Group,
        VmdExportMorphKind::Material,
    ] {
        let sequence = reduced(0.0, 1.0, &[0.0, 0.5, 1.0]);
        assert_eq!(
            export_reduced_pose_to_vmd(&sequence, &bindings(kind)).unwrap_err(),
            VmdPoseExportError::MorphWouldDoubleApply { morph: 0, kind }
        );
    }

    for kind in [VmdExportMorphKind::Uv, VmdExportMorphKind::Other] {
        let sequence = reduced(0.0, 1.0, &[0.0, 0.5, 1.0]);
        assert_eq!(
            export_reduced_pose_to_vmd(&sequence, &bindings(kind)).unwrap_err(),
            VmdPoseExportError::UnsupportedMorphKind { morph: 0, kind }
        );
    }
}

#[test]
fn rejects_incomplete_ik_bindings() {
    let sequence = reduced(0.0, 1.0, &[]);
    let mut binding = bindings(VmdExportMorphKind::Vertex);
    binding.morph_names.clear();
    binding.morph_kinds.clear();
    binding.ik_names.clear();

    assert_eq!(
        export_reduced_pose_to_vmd(&sequence, &binding).unwrap_err(),
        VmdPoseExportError::BindingMismatch
    );
}

#[test]
fn reparsed_vmd_runtime_pose_matches_reducer_sampler() {
    let sequence = reduced(0.0, 1.0, &[]);
    let mut binding = bindings(VmdExportMorphKind::Vertex);
    binding.morph_names.clear();
    binding.morph_kinds.clear();
    binding.ik_names.clear();
    binding.ik_solver_count = 0;
    let exported = export_reduced_pose_to_vmd(&sequence, &binding).unwrap();
    let bytes = export_vmd_animation(&exported.animation);
    let imported = import_vmd_motion(&bytes).unwrap();
    let clip = build_clip_from_import(
        imported,
        &|name| (name == b"bone").then_some(BoneIndex(0)),
        &|_name| None::<MorphIndex>,
    );
    let model = Arc::new(ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap());
    let mut runtime = RuntimeInstance::new(model);
    for frame in 0..3 {
        runtime.evaluate_clip_frame(&clip, frame as f32);
        let expected = sequence.sample(frame as f32).unwrap();
        for (actual, expected) in runtime.world_matrices()[0]
            .to_cols_array()
            .iter()
            .zip(expected.world_matrices[0].to_cols_array())
        {
            assert!((actual - expected).abs() <= 0.01);
        }
    }
}
