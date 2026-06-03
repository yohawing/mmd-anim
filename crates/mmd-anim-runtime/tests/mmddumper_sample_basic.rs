use std::sync::Arc;

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, BoneInit, ModelArena, MorphAnimationBinding,
    MorphIndex, MorphKeyframe, MorphTrack, MovableBoneKeyframe, MovableBoneTrack,
    PropertyAnimationBinding, PropertyKeyframe, RuntimeInstance,
};
use mmd_anim_schema::MmdDumperOracleDump;

fn assert_matrix_near(actual: &[f32; 16], expected: &[f32; 16]) {
    for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1.0e-5,
            "matrix[{index}] actual={actual} expected={expected} delta={delta}"
        );
    }
}

#[test]
fn matches_mmddumper_sample_basic_oracle() {
    let oracle = r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"0.1.0","project":"synthetic/sample-basic.pmm"},"frame":0,"models":[{"index":0,"name":"fake-model","filename":"fake-model.pmd","visible":true,"bones":[{"index":0,"name":"センター","worldMatrix":[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]}],"morphs":[{"index":0,"name":"まばたき","weight":0}]}]}
{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"0.1.0","project":"synthetic/sample-basic.pmm"},"frame":30,"models":[{"index":0,"name":"fake-model","filename":"fake-model.pmd","visible":true,"bones":[{"index":0,"name":"センター","worldMatrix":[1,0,0,0,0,1,0,0,0,0,1,0,1,0,0,1]}],"morphs":[{"index":0,"name":"まばたき","weight":0.5}]}]}
{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"0.1.0","project":"synthetic/sample-basic.pmm"},"frame":60,"models":[{"index":0,"name":"fake-model","filename":"fake-model.pmd","visible":true,"bones":[{"index":0,"name":"センター","worldMatrix":[1,0,0,0,0,1,0,0,0,0,1,0,2,0,0,1]}],"morphs":[{"index":0,"name":"まばたき","weight":1}]}]}"#;
    let dump = MmdDumperOracleDump::from_jsonl_str(oracle, Some(&[0, 30, 60])).unwrap();
    let model = Arc::new(ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap());
    let clip = AnimationClip::new_full(
        vec![BoneAnimationBinding {
            bone: BoneIndex(0),
            track: MovableBoneTrack::from_keyframes(vec![
                MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                MovableBoneKeyframe::new(60, Vec3A::new(2.0, 0.0, 0.0), Quat::IDENTITY),
            ]),
        }],
        vec![MorphAnimationBinding {
            morph: MorphIndex(0),
            track: MorphTrack::from_keyframes(vec![
                MorphKeyframe::new(0, 0.0),
                MorphKeyframe::new(60, 1.0),
            ]),
        }],
        Some(PropertyAnimationBinding::from_keyframes(vec![
            PropertyKeyframe::new(0, vec![true]),
            PropertyKeyframe::new(30, vec![false]),
        ])),
    );
    let mut runtime = RuntimeInstance::new_with_counts(model, 1, 1);

    for frame in [0, 30, 60] {
        runtime.evaluate_clip_frame(&clip, frame as f32);
        let oracle_frame = dump.find_frame(frame).unwrap();
        let oracle_model = &oracle_frame.models[0];
        let center = oracle_model.find_bone("センター").unwrap();
        let blink = oracle_model.find_morph("まばたき").unwrap();

        assert_matrix_near(
            &runtime.world_matrices()[0].to_cols_array(),
            &center.world_matrix,
        );
        assert!((runtime.morph_weights()[0] - blink.weight).abs() < 1.0e-5);
        assert_eq!(runtime.ik_enabled()[0], u8::from(frame < 30));
    }
}
