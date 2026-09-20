use glam::{Quat, Vec3A};
use mmd_anim_runtime::*;
use std::sync::Arc;

fn fixture() -> RuntimeInstance {
    let morphs = build_morph_init_from_offsets(
        2,
        vec![(
            MorphIndex(1),
            BoneMorphOffset {
                target_bone: BoneIndex(0),
                position_offset: Vec3A::Y,
                rotation_offset: Quat::IDENTITY,
            },
        )],
        vec![(
            MorphIndex(0),
            GroupMorphOffset {
                child_morph: MorphIndex(1),
                ratio: 0.5,
            },
        )],
    )
    .unwrap();
    RuntimeInstance::new(Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
            ],
            vec![],
            vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 2.0).with_translation()],
            morphs,
        )
        .unwrap(),
    ))
}

fn clip() -> AnimationClip {
    AnimationClip::new_with_morphs(
        vec![],
        vec![MorphAnimationBinding {
            morph: MorphIndex(0),
            track: MorphTrack::from_keyframes(vec![MorphKeyframe::new(0, 0.8)]),
        }],
    )
}

#[test]
fn override_precedes_group_bone_and_append_and_release_restores_clip() {
    let mut runtime = fixture();
    let clip = clip();
    for _ in 0..3 {
        runtime
            .evaluate_with_morph_overrides(
                Some(&clip),
                0.0,
                &[0, 0, 1],
                &[1.0, 0.5, 0.25],
                IkSolveOptions::default(),
                true,
            )
            .unwrap();
        assert_eq!(runtime.morph_weights(), &[0.5, 0.5]);
        assert_eq!(runtime.world_matrices()[0].w_axis.y, 0.5);
        assert_eq!(runtime.world_matrices()[1].w_axis.y, 1.0);
    }
    runtime
        .evaluate_with_morph_overrides(
            Some(&clip),
            0.0,
            &[0],
            &[0.0],
            IkSolveOptions::default(),
            true,
        )
        .unwrap();
    assert_eq!(runtime.world_matrices()[1].w_axis.y, 0.0);
    runtime.evaluate_clip_frame(&clip, 0.0);
    assert!((runtime.world_matrices()[1].w_axis.y - 0.8).abs() < 1e-6);
    for _ in 0..3 {
        runtime
            .evaluate_with_morph_overrides(
                None,
                0.0,
                &[1],
                &[-0.5],
                IkSolveOptions::default(),
                true,
            )
            .unwrap();
        assert_eq!(runtime.world_matrices()[1].w_axis.y, -1.0);
    }
    runtime.evaluate_rest_pose();
    assert_eq!(runtime.world_matrices()[1].w_axis.y, 0.0);
}

#[test]
fn invalid_overrides_preserve_previous_pose_and_output() {
    let mut runtime = fixture();
    runtime.evaluate_clip_frame(&clip(), 0.0);
    let before = runtime.world_matrices().to_vec();
    let weights = runtime.morph_weights().to_vec();
    for (indices, values) in [
        (&[0][..], &[][..]),
        (&[0, 2][..], &[0.0, 1.0][..]),
        (&[0][..], &[f32::NAN][..]),
        (&[0][..], &[f32::INFINITY][..]),
    ] {
        assert!(
            runtime
                .evaluate_with_morph_overrides(
                    None,
                    0.0,
                    indices,
                    values,
                    IkSolveOptions::default(),
                    true
                )
                .is_err()
        );
        assert_eq!(runtime.world_matrices(), before);
        assert_eq!(runtime.morph_weights(), weights);
    }
}

#[test]
fn bone_morph_moves_ik_goal_before_solving() {
    let morphs = build_morph_init_from_offsets(
        1,
        vec![(
            MorphIndex(0),
            BoneMorphOffset {
                target_bone: BoneIndex(2),
                position_offset: Vec3A::new(-1.0, 1.0, 0.0),
                rotation_offset: Quat::IDENTITY,
            },
        )],
        vec![],
    )
    .unwrap();
    let mut runtime = RuntimeInstance::new(Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                BoneInit::new(None, Vec3A::X),
            ],
            vec![IkSolverInit::new(
                BoneIndex(2),
                BoneIndex(1),
                vec![IkLinkInit::new(BoneIndex(0))],
            )],
            vec![],
            morphs,
        )
        .unwrap(),
    ));
    runtime
        .evaluate_with_morph_overrides(None, 0.0, &[0], &[1.0], IkSolveOptions::default(), true)
        .unwrap();
    assert!((runtime.world_matrices()[1].w_axis.truncate() - glam::Vec3::Y).length() < 0.02);
    runtime
        .evaluate_with_morph_overrides(None, 0.0, &[0], &[1.0], IkSolveOptions::default(), false)
        .unwrap();
    assert_eq!(runtime.world_matrices()[1].w_axis.truncate(), glam::Vec3::X);
}
