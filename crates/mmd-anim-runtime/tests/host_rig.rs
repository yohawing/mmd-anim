use std::sync::Arc;

use glam::{Mat4, Quat, Vec3A};
use mmd_anim_runtime::{
    AppendTransformInit, BoneIndex, BoneInit, BoneMorphOffset, GroupMorphOffset, HostPoseView,
    HostRigDefinition, HostRigError, IkLinkInit, IkSolveOptions, IkSolverInit, ModelArena,
    MorphInit, PhysicsMode, RuntimeInstance, build_morph_init_from_offsets,
};

fn fixture() -> Arc<ModelArena> {
    let mut after = BoneInit::new(None, Vec3A::ZERO);
    after.transform_after_physics = true;
    let bones = vec![
        BoneInit::new(None, Vec3A::ZERO),
        BoneInit::new(Some(BoneIndex(0)), Vec3A::Y),
        BoneInit::new(Some(BoneIndex(1)), Vec3A::Y).with_fixed_axis(Vec3A::X),
        BoneInit::new(None, Vec3A::ZERO),
        BoneInit::new(Some(BoneIndex(2)), Vec3A::X),
        BoneInit::new(None, Vec3A::new(3.0, 1.0, 0.0)),
        BoneInit::new(None, Vec3A::new(3.0, 0.0, 0.0)),
        BoneInit::new(Some(BoneIndex(6)), Vec3A::X),
        after,
    ];
    let ik = vec![
        IkSolverInit::new(
            BoneIndex(5),
            BoneIndex(7),
            vec![IkLinkInit::new(BoneIndex(6))],
        ),
        IkSolverInit::new(
            BoneIndex(5),
            BoneIndex(4),
            vec![IkLinkInit::new(BoneIndex(0))],
        ),
    ];
    let appends = vec![
        AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 0.5)
            .with_rotation()
            .with_translation(),
        AppendTransformInit::new(BoneIndex(2), BoneIndex(1), 0.7).with_rotation(),
        AppendTransformInit::new(BoneIndex(3), BoneIndex(2), 1.0)
            .with_rotation()
            .with_translation(),
        AppendTransformInit::new(BoneIndex(8), BoneIndex(3), 0.5)
            .with_rotation()
            .with_translation(),
    ];
    Arc::new(ModelArena::new_full(bones, ik, appends).unwrap())
}

struct Input {
    p: Vec<Vec3A>,
    q: Vec<Quat>,
    s: Vec<Vec3A>,
    morph: Vec<f32>,
    ik: Vec<u8>,
}
impl Input {
    fn new(model: &ModelArena) -> Self {
        Self {
            p: vec![Vec3A::ZERO; model.bone_count()],
            q: vec![Quat::IDENTITY; model.bone_count()],
            s: vec![Vec3A::ONE; model.bone_count()],
            morph: vec![0.0; model.morph_count() as usize],
            ik: vec![0; model.ik_count()],
        }
    }
    fn view(&self) -> HostPoseView<'_> {
        HostPoseView {
            local_position_offsets: &self.p,
            local_rotations: &self.q,
            local_scales: &self.s,
            morph_weights: &self.morph,
            ik_enabled: &self.ik,
        }
    }
}

fn near(a: Mat4, b: Mat4) {
    let error = a
        .to_cols_array()
        .into_iter()
        .zip(b.to_cols_array())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0, f32::max);
    assert!(error < 1e-5, "error={error}, actual={a:?}, expected={b:?}");
}

#[test]
fn preserves_fk_through_append_parent_and_feeds_helpers_in_both_phases() {
    let model = fixture();
    let rig = HostRigDefinition::new(model.clone(), &[BoneIndex(0), BoneIndex(2)], &[]).unwrap();
    let mut runtime = RuntimeInstance::new(model.clone());
    let mut input = Input::new(&model);
    input.p[0] = Vec3A::X;
    input.p[2] = Vec3A::new(0.2, 0.0, 0.0);
    input.q[0] = Quat::from_rotation_z(0.4);
    input.q[2] = Quat::from_rotation_z(0.2);
    let root = Mat4::from_rotation_translation(input.q[0], input.p[0].into());
    let primary = root
        * Mat4::from_translation(Vec3A::Y.into())
        * Mat4::from_rotation_translation(input.q[2], (Vec3A::Y + input.p[2]).into());
    runtime
        .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
        .unwrap();
    near(runtime.world_matrices()[2], primary);
    near(
        runtime.world_matrices()[4],
        primary * Mat4::from_translation(Vec3A::X.into()),
    );
    near(
        runtime.world_matrices()[3],
        Mat4::from_rotation_translation(input.q[2], input.p[2].into()),
    );
    near(
        runtime.world_matrices()[8],
        Mat4::from_rotation_translation(Quat::from_rotation_z(0.1), (input.p[2] * 0.5).into()),
    );
    near(
        runtime.skinning_matrices()[2],
        primary * model.inverse_bind_matrix(BoneIndex(2)),
    );
    // Characterize the old full-pose route: it adds Append and fixed-axis
    // constraints on top of the host's already-retargeted primary motion.
    let mut legacy = RuntimeInstance::new(model);
    legacy.apply_host_pose(&input.view()).unwrap();
    legacy.evaluate_current_pose();
    assert!(!legacy.world_matrices()[2].abs_diff_eq(primary, 0.01));
    // A normal call on the same instance must not retain rig ownership.
    runtime.apply_host_pose(&input.view()).unwrap();
    runtime.evaluate_current_pose();
    near(runtime.world_matrices()[2], legacy.world_matrices()[2]);
}

#[test]
fn repeated_frames_and_seek_do_not_accumulate_and_instances_are_independent() {
    let model = fixture();
    let rig = HostRigDefinition::new(model.clone(), &[BoneIndex(0), BoneIndex(2)], &[]).unwrap();
    let mut runtime = RuntimeInstance::new(model.clone());
    let mut input = Input::new(&model);
    input.q[0] = Quat::from_rotation_z(0.4);
    input.q[2] = Quat::from_rotation_z(-0.3);
    runtime
        .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
        .unwrap();
    let first = runtime.world_matrices().to_vec();
    for _ in 0..300 {
        runtime
            .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
            .unwrap();
        assert_eq!(runtime.world_matrices(), first);
    }
    for angle in [0.8, -0.2, 0.0, 1.1, -0.3] {
        input.q[2] = Quat::from_rotation_z(angle);
        runtime
            .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
            .unwrap();
        let mut independent = RuntimeInstance::new(model.clone());
        independent
            .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
            .unwrap();
        assert_eq!(runtime.world_matrices(), independent.world_matrices());
    }
    assert_eq!(runtime.world_matrices(), first);
}

#[test]
fn explicit_goal_solves_and_missing_goal_or_protected_link_is_rejected() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                BoneInit::new(None, Vec3A::Y),
            ],
            vec![IkSolverInit::new(
                BoneIndex(2),
                BoneIndex(1),
                vec![IkLinkInit::new(BoneIndex(0))],
            )],
        )
        .unwrap(),
    );
    let rig = HostRigDefinition::new(model.clone(), &[], &[BoneIndex(2)]).unwrap();
    let mut input = Input::new(&model);
    input.ik[0] = 1;
    let mut runtime = RuntimeInstance::new(model.clone());
    runtime
        .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
        .unwrap();
    assert!((runtime.world_matrices()[1].w_axis.truncate() - glam::Vec3::Y).length() < 1e-4);
    for protected in [BoneIndex(0), BoneIndex(1)] {
        assert!(matches!(
            HostRigDefinition::new(model.clone(), &[protected], &[BoneIndex(2)]),
            Err(HostRigError::IkConflict { chain: 0, .. })
        ));
    }
    let fk = HostRigDefinition::new(model.clone(), &[BoneIndex(0)], &[]).unwrap();
    let previous = runtime.world_matrices().to_vec();
    assert_eq!(
        runtime.evaluate_host_rig_pose(&fk, &input.view(), IkSolveOptions::default()),
        Err(HostRigError::MissingGoal(0))
    );
    assert_eq!(runtime.world_matrices(), previous);
    input.ik[0] = 0;
    runtime
        .evaluate_host_rig_pose(&fk, &input.view(), IkSolveOptions::default())
        .unwrap();
    near(runtime.world_matrices()[0], Mat4::IDENTITY);
    assert!((runtime.world_matrices()[1].w_axis.truncate() - glam::Vec3::X).length() < 1e-5);
}

#[test]
fn group_and_bone_morphs_are_applied_once_before_protection() {
    let morphs: MorphInit = build_morph_init_from_offsets(
        2,
        vec![(
            mmd_anim_runtime::MorphIndex(1),
            BoneMorphOffset {
                target_bone: BoneIndex(0),
                position_offset: Vec3A::X,
                rotation_offset: Quat::from_rotation_z(0.4),
            },
        )],
        vec![(
            mmd_anim_runtime::MorphIndex(0),
            GroupMorphOffset {
                child_morph: mmd_anim_runtime::MorphIndex(1),
                ratio: 0.5,
            },
        )],
    )
    .unwrap();
    let model = Arc::new(
        ModelArena::new_with_morphs(
            vec![BoneInit::new(None, Vec3A::ZERO)],
            vec![],
            vec![],
            morphs,
        )
        .unwrap(),
    );
    let rig = HostRigDefinition::new(model.clone(), &[BoneIndex(0)], &[]).unwrap();
    let mut input = Input::new(&model);
    input.morph[0] = 1.0;
    let mut runtime = RuntimeInstance::new(model);
    for _ in 0..3 {
        runtime
            .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
            .unwrap();
        near(
            runtime.world_matrices()[0],
            Mat4::from_rotation_translation(Quat::from_rotation_z(0.2), glam::Vec3::X * 0.5),
        );
        assert_eq!(runtime.morph_weights(), &[1.0, 0.5]);
    }
}

#[test]
fn invalid_input_is_atomic_and_next_valid_frame_recovers() {
    let model = fixture();
    let rig = HostRigDefinition::new(model.clone(), &[BoneIndex(2)], &[]).unwrap();
    let mut runtime = RuntimeInstance::new(model.clone());
    let mut input = Input::new(&model);
    runtime
        .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
        .unwrap();
    let previous = runtime.world_matrices().to_vec();
    input.p[0].x = f32::NAN;
    assert!(
        runtime
            .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
            .is_err()
    );
    assert_eq!(runtime.world_matrices(), previous);
    input.p[0] = Vec3A::ZERO;
    input.q[0] = Quat::from_xyzw(0.0, 0.0, 0.0, 0.0);
    assert!(
        runtime
            .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
            .is_err()
    );
    input.q[0] = Quat::IDENTITY;
    input.s[0] = Vec3A::new(1.0, 2.0, 1.0);
    assert_eq!(
        runtime.evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default()),
        Err(HostRigError::InvalidScale(0))
    );
    input.s[0] = Vec3A::ONE;
    input.p.pop();
    assert!(
        runtime
            .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
            .is_err()
    );
    input.p.push(Vec3A::ZERO);
    let other = HostRigDefinition::new(fixture(), &[], &[]).unwrap();
    assert_eq!(
        runtime.evaluate_host_rig_pose(&other, &input.view(), IkSolveOptions::default()),
        Err(HostRigError::ModelMismatch)
    );
    assert_eq!(runtime.world_matrices(), previous);
    runtime.set_physics_mode(PhysicsMode::Live);
    assert_eq!(
        runtime.evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default()),
        Err(HostRigError::PhysicsEnabled)
    );
    runtime.set_physics_mode(PhysicsMode::Off);
    runtime
        .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
        .unwrap();
    assert_eq!(runtime.world_matrices(), previous);
}

#[test]
fn empty_rig_accepts_extra_ik_slots_without_solvers_like_legacy_host_pose() {
    let model = Arc::new(ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap());
    let rig = HostRigDefinition::new(model.clone(), &[], &[]).unwrap();
    let mut runtime = RuntimeInstance::new_with_counts(model.clone(), 0, 2);
    let mut input = Input::new(&model);
    input.ik = vec![1, 1];
    input.p[0] = Vec3A::X;
    runtime
        .evaluate_host_rig_pose(&rig, &input.view(), IkSolveOptions::default())
        .unwrap();
    near(
        runtime.world_matrices()[0],
        Mat4::from_translation(glam::Vec3::X),
    );
    assert_eq!(runtime.ik_enabled(), &[1, 1]);
}
