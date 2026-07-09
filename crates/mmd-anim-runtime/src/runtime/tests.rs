use std::sync::Arc;

use glam::{Quat, Vec3A};

use crate::{
    AnimationClip, AppendTransformInit, BoneAnimationBinding, BoneIndex, BoneInit, IkAngleLimit,
    IkChainDefinition, IkChainLinkDefinition, IkChainPoseInput, IkChainSolver, IkLinkInit,
    IkSolveOptions, IkSolverInit, ModelArena, MovableBoneKeyframe, MovableBoneTrack, PhysicsMode,
    PhysicsTickConfig, RuntimeInstance,
};

fn translation(matrix: glam::Mat4) -> Vec3A {
    Vec3A::from_vec4(matrix.w_axis)
}

fn assert_vec3a_near(actual: Vec3A, expected: Vec3A) {
    let delta = (actual - expected).abs();
    assert!(
        delta.x < 1.0e-5 && delta.y < 1.0e-5 && delta.z < 1.0e-5,
        "actual={actual:?} expected={expected:?} delta={delta:?}"
    );
}

fn assert_quat_near(actual: Quat, expected: Quat) {
    let alignment = actual.dot(expected).abs();
    assert!(
        (1.0 - alignment) < 1.0e-5,
        "actual={actual:?} expected={expected:?} alignment={alignment}"
    );
}

#[test]
fn evaluates_rest_pose_world_matrices() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_rest_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[0]),
        Vec3A::new(1.0, 0.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(1.0, 2.0, 0.0),
    );
}

#[test]
fn evaluates_current_pose_with_parent_rotation() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.pose_mut().set_local_rotation(
        BoneIndex(0),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    );
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(-1.0, 0.0, 0.0),
    );
}

#[test]
fn apply_physics_world_matrices_updates_local_pose_and_descendants() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
            BoneInit::new(Some(BoneIndex(1)), Vec3A::new(0.0, 1.0, 0.0)),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.evaluate_rest_pose();

    let updated = runtime.apply_physics_world_matrices(&[
        None,
        Some(glam::Mat4::from_translation(
            Vec3A::new(5.0, 6.0, 0.0).into(),
        )),
        None,
    ]);

    assert_eq!(updated, 1);
    assert_vec3a_near(
        runtime.pose().local_position_offset(BoneIndex(1)),
        Vec3A::new(4.0, 4.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(5.0, 6.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(5.0, 7.0, 0.0),
    );
}

#[test]
fn apply_physics_world_matrices_uses_physics_parent_for_child_local_pose() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::ZERO),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
            BoneInit::new(Some(BoneIndex(1)), Vec3A::new(0.0, 1.0, 0.0)),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.evaluate_rest_pose();

    let updated = runtime.apply_physics_world_matrices(&[
        None,
        Some(glam::Mat4::from_translation(
            Vec3A::new(10.0, 10.0, 0.0).into(),
        )),
        Some(glam::Mat4::from_translation(
            Vec3A::new(10.0, 11.0, 0.0).into(),
        )),
    ]);

    assert_eq!(updated, 2);
    assert_vec3a_near(
        runtime.pose().local_position_offset(BoneIndex(2)),
        Vec3A::ZERO,
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(10.0, 10.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(10.0, 11.0, 0.0),
    );
}

#[test]
fn apply_physics_world_matrices_propagates_sparse_physics_ancestors() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::ZERO),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
            BoneInit::new(Some(BoneIndex(1)), Vec3A::new(0.0, 1.0, 0.0)),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.evaluate_rest_pose();

    let updated = runtime.apply_physics_world_matrices(&[
        Some(glam::Mat4::from_translation(
            Vec3A::new(10.0, 10.0, 0.0).into(),
        )),
        None,
        Some(glam::Mat4::from_translation(
            Vec3A::new(10.0, 12.0, 0.0).into(),
        )),
    ]);

    assert_eq!(updated, 2);
    assert_vec3a_near(
        runtime.pose().local_position_offset(BoneIndex(2)),
        Vec3A::ZERO,
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[0]),
        Vec3A::new(10.0, 10.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(10.0, 11.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(10.0, 12.0, 0.0),
    );
}

#[test]
fn fixed_axis_bone_rotation_keeps_only_axis_twist() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::ZERO).with_fixed_axis(Vec3A::Y),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.pose_mut().set_local_rotation(
        BoneIndex(0),
        (Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
            * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
        .normalize(),
    );
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(0.0, 0.0, -1.0),
    );
}

#[test]
fn evaluates_current_pose_with_local_position_offset() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime
        .pose_mut()
        .set_local_position_offset(BoneIndex(1), Vec3A::new(0.0, 0.0, 3.0));
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(1.0, 2.0, 3.0),
    );
}

#[test]
fn evaluates_clip_frame_into_world_matrices() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
        ])
        .unwrap(),
    );
    let clip = AnimationClip::new(vec![BoneAnimationBinding {
        bone: BoneIndex(1),
        track: MovableBoneTrack::from_keyframes(vec![
            MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
            MovableBoneKeyframe::new(10, Vec3A::new(0.0, 0.0, 4.0), Quat::IDENTITY),
        ]),
    }]);
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_clip_frame(&clip, 5.0);

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(1.0, 2.0, 2.0),
    );
}

#[test]
fn append_output_accessors_reflect_pose_state() {
    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
            ],
            Vec::new(),
            vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 0.5).with_translation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime
        .pose_mut()
        .set_local_position_offset(BoneIndex(0), Vec3A::new(2.0, 0.0, 0.0));
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        runtime.append_position_offset(BoneIndex(1)),
        Vec3A::new(1.0, 0.0, 0.0),
    );
    assert!(
        runtime
            .append_rotation(BoneIndex(1))
            .dot(Quat::IDENTITY)
            .abs()
            > 1.0 - f32::EPSILON
    );
}

#[test]
fn applies_append_rotation_before_world_matrix_output() {
    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
            ],
            Vec::new(),
            vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.pose_mut().set_local_rotation(
        BoneIndex(0),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    );
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn applies_append_translation_before_world_matrix_output() {
    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            Vec::new(),
            vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 0.5).with_translation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime
        .pose_mut()
        .set_local_position_offset(BoneIndex(0), Vec3A::new(2.0, 0.0, 0.0));
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(1.0, 1.0, 0.0),
    );
}

#[test]
fn initializes_ik_enabled_from_model_solvers() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
            ],
            vec![IkSolverInit::new(
                BoneIndex(1),
                BoneIndex(0),
                vec![IkLinkInit::new(BoneIndex(0))],
            )],
        )
        .unwrap(),
    );

    let runtime = RuntimeInstance::new(model);

    assert_eq!(runtime.ik_enabled(), &[1]);
}

#[test]
fn solves_one_link_ik_toward_controller_bone() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn skips_disabled_ik_solver() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.pose_mut().set_ik_enabled(0, false);
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(1.0, 0.0, 0.0),
    );
}

#[test]
fn solves_two_link_ik_chain_toward_controller_bone() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(3),
                target_bone: BoneIndex(2),
                links: vec![IkLinkInit::new(BoneIndex(1)), IkLinkInit::new(BoneIndex(0))],
                iteration_count: 4,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(1.0, 1.0, 0.0),
    );
}

#[test]
fn primitive_ik_solver_matches_runtime_ik_driver_for_two_link_chain() {
    let bones = vec![
        BoneInit::new(None, Vec3A::ZERO),
        BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
        BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
        BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)),
    ];
    let solver = IkSolverInit {
        ik_bone: BoneIndex(3),
        target_bone: BoneIndex(2),
        links: vec![IkLinkInit::new(BoneIndex(1)), IkLinkInit::new(BoneIndex(0))],
        iteration_count: 4,
        limit_angle: 0.0,
    };
    let model = Arc::new(ModelArena::new_with_ik(bones, vec![solver]).unwrap());
    let mut runtime = RuntimeInstance::new(Arc::clone(&model));
    runtime.evaluate_current_pose_without_ik();

    let mut primitive = IkChainSolver::new(IkChainDefinition {
        parent_slots: vec![None, Some(0), Some(1), None],
        rest_positions: vec![
            Vec3A::ZERO,
            Vec3A::new(1.0, 0.0, 0.0),
            Vec3A::new(1.0, 0.0, 0.0),
            Vec3A::new(1.0, 1.0, 0.0),
        ],
        fixed_axes: vec![None, None, None, None],
        target_slot: 2,
        links: vec![
            IkChainLinkDefinition {
                bone_slot: 1,
                angle_limit: None,
            },
            IkChainLinkDefinition {
                bone_slot: 0,
                angle_limit: None,
            },
        ],
        iteration_count: 4,
        limit_angle: 0.0,
    });
    let primitive_output = primitive.solve(IkChainPoseInput {
        parent_world_matrix: None,
        local_position_offsets: &[Vec3A::ZERO; 4],
        local_rotations: &[Quat::IDENTITY; 4],
        goal_position: Vec3A::new(1.0, 1.0, 0.0),
        tolerance: 0.0,
        max_iterations_cap: None,
    });

    runtime.solve_ik_solver(0, IkSolveOptions::default(), false);

    assert_quat_near(
        runtime.pose().local_rotation(BoneIndex(1)),
        primitive_output.solved_link_rotations[0],
    );
    assert_quat_near(
        runtime.pose().local_rotation(BoneIndex(0)),
        primitive_output.solved_link_rotations[1],
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(1.0, 1.0, 0.0),
    );
    assert_eq!(
        runtime.ik_runtime_stats()[0].executed_iterations,
        u64::from(primitive_output.executed_iterations)
    );
    assert_eq!(
        runtime.ik_runtime_stats()[0].link_steps,
        u64::from(primitive_output.link_steps)
    );
}

#[test]
fn evaluates_all_solvers_attached_to_same_ik_bone() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![
                IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
                IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_eq!(runtime.ik_runtime_stats()[0].solver_evaluations, 1);
    assert_eq!(runtime.ik_runtime_stats()[1].solver_evaluations, 1);
}

#[test]
fn evaluates_ik_solvers_through_bone_lookup_for_distinct_ik_bones() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                BoneInit::new(None, Vec3A::new(10.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(3)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(10.0, 1.0, 0.0)),
            ],
            vec![
                IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
                IkSolverInit {
                    ik_bone: BoneIndex(5),
                    target_bone: BoneIndex(4),
                    links: vec![IkLinkInit::new(BoneIndex(3))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[4]),
        Vec3A::new(10.0, 1.0, 0.0),
    );
    assert_eq!(runtime.ik_runtime_stats()[0].solver_evaluations, 1);
    assert_eq!(runtime.ik_runtime_stats()[1].solver_evaluations, 1);
}

#[test]
fn ik_target_descendant_recomputes_after_scoped_ik_update() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                BoneInit::new(Some(BoneIndex(1)), Vec3A::X),
                BoneInit::new(None, Vec3A::Y),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(3),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(translation(runtime.world_matrices()[1]), Vec3A::Y);
    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(0.0, 2.0, 0.0),
    );
}

#[test]
fn ik_updates_only_affected_eval_suffix_for_late_chain() {
    let unrelated_count = 96usize;
    let chain_root = BoneIndex(unrelated_count as u32);
    let chain_mid = BoneIndex(unrelated_count as u32 + 1);
    let chain_tip = BoneIndex(unrelated_count as u32 + 2);
    let controller = BoneIndex(unrelated_count as u32 + 3);

    let mut bones = Vec::new();
    for i in 0..unrelated_count {
        bones.push(BoneInit::new(None, Vec3A::new(i as f32 * 10.0, -10.0, 0.0)));
    }
    bones.push(BoneInit::new(None, Vec3A::ZERO));
    bones.push(BoneInit::new(Some(chain_root), Vec3A::new(1.0, 0.0, 0.0)));
    bones.push(BoneInit::new(Some(chain_mid), Vec3A::new(1.0, 0.0, 0.0)));
    bones.push(BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)));

    let model = Arc::new(
        ModelArena::new_with_ik(
            bones,
            vec![IkSolverInit {
                ik_bone: controller,
                target_bone: chain_tip,
                links: vec![IkLinkInit::new(chain_mid), IkLinkInit::new(chain_root)],
                iteration_count: 4,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.reset_world_matrix_bone_update_count();
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[chain_tip.as_usize()]),
        Vec3A::new(1.0, 1.0, 0.0),
    );
    assert!(
        runtime.world_matrix_bone_update_count() < 360,
        "IK should not recompute unrelated prefix bones repeatedly; updated {} bones",
        runtime.world_matrix_bone_update_count()
    );
}

#[test]
fn ik_updates_only_affected_bones_for_root_near_chain() {
    let unrelated_count = 96usize;
    let chain_root = BoneIndex(0);
    let chain_mid = BoneIndex(1);
    let chain_tip = BoneIndex(2);
    let controller = BoneIndex(3);

    let mut bones = vec![
        BoneInit::new(None, Vec3A::ZERO),
        BoneInit::new(Some(chain_root), Vec3A::new(1.0, 0.0, 0.0)),
        BoneInit::new(Some(chain_mid), Vec3A::new(1.0, 0.0, 0.0)),
        BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)),
    ];
    for i in 0..unrelated_count {
        bones.push(BoneInit::new(None, Vec3A::new(i as f32 * 10.0, -10.0, 0.0)));
    }

    let model = Arc::new(
        ModelArena::new_with_ik(
            bones,
            vec![IkSolverInit {
                ik_bone: controller,
                target_bone: chain_tip,
                links: vec![IkLinkInit::new(chain_mid), IkLinkInit::new(chain_root)],
                iteration_count: 4,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.reset_world_matrix_bone_update_count();
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[chain_tip.as_usize()]),
        Vec3A::new(1.0, 1.0, 0.0),
    );
    assert!(
        runtime.world_matrix_bone_update_count() < 360,
        "IK should not recompute unrelated tail bones repeatedly; updated {} bones",
        runtime.world_matrix_bone_update_count()
    );
}

#[test]
fn phase_scoped_world_matrix_update_visits_only_matching_phase_bones() {
    let before_count = 96usize;
    let mut bones = Vec::new();
    for i in 0..before_count {
        bones.push(BoneInit::new(None, Vec3A::new(i as f32, 0.0, 0.0)));
    }
    let mut after_a = BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0));
    after_a.transform_after_physics = true;
    let mut after_b = BoneInit::new(
        Some(BoneIndex(before_count as u32)),
        Vec3A::new(1.0, 0.0, 0.0),
    );
    after_b.transform_after_physics = true;
    let mut after_c = BoneInit::new(None, Vec3A::new(0.0, 0.0, 1.0));
    after_c.transform_after_physics = true;
    bones.extend([after_a, after_b, after_c]);
    let after_count = 3usize;
    let total_count = before_count + after_count;

    let model = Arc::new(ModelArena::new(bones).unwrap());
    let mut runtime = RuntimeInstance::new(model);

    runtime.reset_world_matrix_bone_update_count();
    runtime.update_world_matrices_using_current_append_from_eval_order_position_for_phase(
        0,
        Some(false),
    );
    assert_eq!(
        runtime.world_matrix_bone_update_count(),
        before_count,
        "before-physics phase update should touch only before-physics bones"
    );

    runtime.reset_world_matrix_bone_update_count();
    runtime.update_world_matrices_using_current_append_from_eval_order_position_for_phase(
        0,
        Some(true),
    );
    assert_eq!(
        runtime.world_matrix_bone_update_count(),
        after_count,
        "after-physics phase update should touch only after-physics bones"
    );

    runtime.reset_world_matrix_bone_update_count();
    runtime.update_world_matrices_using_current_append_from_eval_order_position_for_phase(0, None);
    assert_eq!(
        runtime.world_matrix_bone_update_count(),
        total_count,
        "unscoped phase update should touch all eval-order bones"
    );
}

#[test]
fn categorized_world_matrix_bone_update_counts_for_simple_no_ik_pose() {
    let bone_count = 3usize;
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::ZERO),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
            BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
        ])
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.reset_world_matrix_bone_update_count();
    runtime.evaluate_current_pose();

    assert_eq!(
        runtime.world_matrix_bone_update_leading_bookend_count(),
        bone_count,
        "leading bookend should update each eval-order bone once"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_phase_loop_count(),
        bone_count,
        "before-physics phase loop should update each before-physics bone once"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_trailing_bookend_count(),
        0,
        "all-before-physics no-IK pose should skip trailing bookend"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_ik_link_change_count(),
        0,
        "no-IK model should not perform IK link-change world updates"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_other_count(),
        0,
        "ordered pose evaluation should not leave unclassified world updates"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_count(),
        runtime.world_matrix_bone_update_leading_bookend_count()
            + runtime.world_matrix_bone_update_phase_loop_count()
            + runtime.world_matrix_bone_update_trailing_bookend_count()
            + runtime.world_matrix_bone_update_ik_link_change_count()
            + runtime.world_matrix_bone_update_other_count(),
        "total world update count should equal categorized sum"
    );
}

#[test]
fn all_pre_physics_transitive_append_recomputes_trailing_suffix() {
    let mut target = BoneInit::new(None, Vec3A::ZERO);
    target.transform_order = 0;
    let mut child = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
    child.transform_order = 1;
    let mut driver = BoneInit::new(None, Vec3A::ZERO);
    driver.transform_order = 2;
    let mut source = BoneInit::new(None, Vec3A::ZERO);
    source.transform_order = 3;

    let model = Arc::new(
        ModelArena::new_full(
            vec![target, child, driver, source],
            Vec::new(),
            vec![
                AppendTransformInit::new(BoneIndex(3), BoneIndex(2), 1.0).with_rotation(),
                AppendTransformInit::new(BoneIndex(0), BoneIndex(3), 1.0).with_rotation(),
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.pose_mut().set_local_rotation(
        BoneIndex(2),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    );

    runtime.reset_world_matrix_bone_update_count();
    runtime.evaluate_current_pose();

    assert_vec3a_near(translation(runtime.world_matrices()[1]), Vec3A::Y);
    assert_eq!(
        runtime.world_matrix_bone_update_trailing_bookend_count(),
        4,
        "out-of-order transitive append dependencies need one trailing suffix refresh"
    );
}

#[test]
fn categorized_world_matrix_bone_update_counts_split_ik_link_changes() {
    let bone_count = 3usize;
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.reset_world_matrix_bone_update_count();
    runtime.evaluate_current_pose();

    assert_eq!(
        runtime.world_matrix_bone_update_leading_bookend_count(),
        bone_count
    );
    assert_eq!(
        runtime.world_matrix_bone_update_phase_loop_count(),
        bone_count
    );
    assert_eq!(
        runtime.world_matrix_bone_update_trailing_bookend_count(),
        0,
        "all-before-physics IK pose should rely on link-change scoped updates"
    );
    assert!(
        runtime.world_matrix_bone_update_ik_link_change_count() > 0,
        "IK solve should account scoped link-change world updates separately"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_other_count(),
        0,
        "ordered pose evaluation should not leave unclassified world updates"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_count(),
        runtime.world_matrix_bone_update_leading_bookend_count()
            + runtime.world_matrix_bone_update_phase_loop_count()
            + runtime.world_matrix_bone_update_trailing_bookend_count()
            + runtime.world_matrix_bone_update_ik_link_change_count()
            + runtime.world_matrix_bone_update_other_count(),
        "total world update count should equal the category sum"
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorldMatrixBoneUpdateCategorySnapshot {
    leading_bookend: usize,
    phase_loop: usize,
    trailing_bookend: usize,
    ik_link_change: usize,
    other: usize,
}

impl WorldMatrixBoneUpdateCategorySnapshot {
    fn from_runtime(runtime: &RuntimeInstance) -> Self {
        Self {
            leading_bookend: runtime.world_matrix_bone_update_leading_bookend_count(),
            phase_loop: runtime.world_matrix_bone_update_phase_loop_count(),
            trailing_bookend: runtime.world_matrix_bone_update_trailing_bookend_count(),
            ik_link_change: runtime.world_matrix_bone_update_ik_link_change_count(),
            other: runtime.world_matrix_bone_update_other_count(),
        }
    }

    fn total(&self) -> usize {
        self.leading_bookend
            + self.phase_loop
            + self.trailing_bookend
            + self.ik_link_change
            + self.other
    }

    fn bookend(&self) -> usize {
        self.leading_bookend + self.trailing_bookend
    }

    fn assert_matches_total(&self, total: usize) {
        assert_eq!(
            total,
            self.total(),
            "total world update count should equal categorized sum: snapshot={self:?}"
        );
        assert_eq!(
            self.other, 0,
            "ordered pose evaluation should not leave unclassified updates"
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PerfWorldUpdateOptimizationBranch {
    TrailingSuffixShrinking,
    IkLinkChangeScope,
    ApplyPoseTrackSampling,
}

fn perf_world_update_optimization_branch(
    snapshot: &WorldMatrixBoneUpdateCategorySnapshot,
) -> PerfWorldUpdateOptimizationBranch {
    let bookend = snapshot.bookend();
    let ik = snapshot.ik_link_change;
    if ik > bookend {
        PerfWorldUpdateOptimizationBranch::IkLinkChangeScope
    } else if bookend > ik {
        PerfWorldUpdateOptimizationBranch::TrailingSuffixShrinking
    } else {
        PerfWorldUpdateOptimizationBranch::ApplyPoseTrackSampling
    }
}

fn build_late_chain_multi_link_ik_model(
    unrelated_prefix_count: usize,
    chain_bone_count: usize,
    iteration_count: u32,
) -> Arc<ModelArena> {
    assert!(chain_bone_count >= 2);

    let mut bones = Vec::new();
    for i in 0..unrelated_prefix_count {
        bones.push(BoneInit::new(None, Vec3A::new(i as f32 * 10.0, -10.0, 0.0)));
    }

    let chain_root = BoneIndex(unrelated_prefix_count as u32);
    bones.push(BoneInit::new(None, Vec3A::ZERO));

    let mut chain_indices = vec![chain_root];
    let mut parent = chain_root;
    for offset in 1..chain_bone_count {
        let bone = BoneIndex(unrelated_prefix_count as u32 + offset as u32);
        bones.push(BoneInit::new(Some(parent), Vec3A::new(1.0, 0.0, 0.0)));
        chain_indices.push(bone);
        parent = bone;
    }
    let chain_tip = *chain_indices.last().expect("chain tip");
    let controller = BoneIndex(unrelated_prefix_count as u32 + chain_bone_count as u32);
    bones.push(BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)));

    let links = chain_indices
        .iter()
        .rev()
        .skip(1)
        .map(|&bone| IkLinkInit::new(bone))
        .collect();

    Arc::new(
        ModelArena::new_with_ik(
            bones,
            vec![IkSolverInit {
                ik_bone: controller,
                target_bone: chain_tip,
                links,
                iteration_count,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    )
}

fn build_root_near_multi_link_ik_model(
    unrelated_tail_count: usize,
    chain_bone_count: usize,
    iteration_count: u32,
) -> Arc<ModelArena> {
    assert!(chain_bone_count >= 2);

    let chain_root = BoneIndex(0);
    let mut bones = vec![BoneInit::new(None, Vec3A::ZERO)];
    let mut chain_indices = vec![chain_root];
    let mut parent = chain_root;
    for offset in 1..chain_bone_count {
        let bone = BoneIndex(offset as u32);
        bones.push(BoneInit::new(Some(parent), Vec3A::new(1.0, 0.0, 0.0)));
        chain_indices.push(bone);
        parent = bone;
    }
    let chain_tip = *chain_indices.last().expect("chain tip");
    let controller = BoneIndex(chain_bone_count as u32);
    bones.push(BoneInit::new(None, Vec3A::new(1.0, 1.0, 0.0)));

    for i in 0..unrelated_tail_count {
        bones.push(BoneInit::new(
            None,
            Vec3A::new((i + chain_bone_count + 1) as f32 * 10.0, -10.0, 0.0),
        ));
    }

    let links = chain_indices
        .iter()
        .rev()
        .skip(1)
        .map(|&bone| IkLinkInit::new(bone))
        .collect();

    Arc::new(
        ModelArena::new_with_ik(
            bones,
            vec![IkSolverInit {
                ik_bone: controller,
                target_bone: chain_tip,
                links,
                iteration_count,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    )
}

fn evaluate_pose_category_snapshot(
    runtime: &mut RuntimeInstance,
) -> WorldMatrixBoneUpdateCategorySnapshot {
    runtime.reset_world_matrix_bone_update_count();
    runtime.evaluate_current_pose();
    let snapshot = WorldMatrixBoneUpdateCategorySnapshot::from_runtime(runtime);
    snapshot.assert_matches_total(runtime.world_matrix_bone_update_count());
    snapshot
}

#[test]
fn categorized_world_matrix_bone_update_counts_decide_trailing_suffix_for_suffix_scoped_ik() {
    // Mirrors the 4b real-asset profile: many prefix bones with extremity IK solvers
    // that only refresh a short eval-order suffix per link step.
    let bone_count = 449usize;
    let unrelated_prefix_count = bone_count - 4;
    let snapshot = evaluate_pose_category_snapshot(&mut RuntimeInstance::new(
        build_late_chain_multi_link_ik_model(unrelated_prefix_count, 3, 20),
    ));

    assert_eq!(snapshot.leading_bookend, bone_count);
    assert_eq!(snapshot.trailing_bookend, 0);
    assert_eq!(snapshot.phase_loop, bone_count);
    assert_eq!(snapshot.bookend(), bone_count);
    assert_eq!(snapshot.ik_link_change, 28);
    assert!(
        snapshot.bookend() > snapshot.ik_link_change,
        "suffix-scoped IK should leave leading bookend dominant: snapshot={snapshot:?}"
    );
    assert_eq!(
        perf_world_update_optimization_branch(&snapshot),
        PerfWorldUpdateOptimizationBranch::TrailingSuffixShrinking,
        "real-asset proxy should branch to trailing suffix shrinking: snapshot={snapshot:?}"
    );
}

#[test]
fn categorized_world_matrix_bone_update_counts_decide_ik_scope_for_broad_link_change_updates() {
    // Synthetic control case: a long root-near chain multiplies per-link-step refreshes
    // across the whole chain scope on every iteration.
    let chain_bone_count = 12usize;
    let unrelated_tail_count = 8usize;
    let bone_count = chain_bone_count + 1 + unrelated_tail_count;
    let snapshot = evaluate_pose_category_snapshot(&mut RuntimeInstance::new(
        build_root_near_multi_link_ik_model(unrelated_tail_count, chain_bone_count, 16),
    ));

    assert_eq!(snapshot.leading_bookend, bone_count);
    assert_eq!(snapshot.trailing_bookend, 0);
    assert_eq!(snapshot.phase_loop, bone_count);
    assert_eq!(snapshot.bookend(), bone_count);
    assert_eq!(snapshot.ik_link_change, 468);
    assert!(
        snapshot.ik_link_change > snapshot.bookend(),
        "long-chain IK should dominate leading bookend updates: snapshot={snapshot:?}"
    );
    assert_eq!(
        perf_world_update_optimization_branch(&snapshot),
        PerfWorldUpdateOptimizationBranch::IkLinkChangeScope,
        "long-chain IK should branch to IK link-change scope work: snapshot={snapshot:?}"
    );
}

#[test]
fn clamps_ik_rotation_by_solver_limit_angle() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: std::f32::consts::FRAC_PI_4,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    let expected = Vec3A::new(
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
    );
    assert_vec3a_near(translation(runtime.world_matrices()[1]), expected);
}

#[test]
fn applies_constant_limit_angle_per_iteration() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(1)), IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: std::f32::consts::FRAC_PI_4,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    // With constant limit_angle = PI/4 (not scaled by link_index), only the root
    // (link 1, bone 0) rotates at most PI/4. The effector bone is skipped.
    // The child bone ends up at (cos(PI/4)*1, sin(PI/4)*1, 0)
    let expected = Vec3A::new(
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
    );
    assert_vec3a_near(translation(runtime.world_matrices()[1]), expected);
}

#[test]
fn clip_frame_produces_deterministic_world_translations() {
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0)),
        ])
        .unwrap(),
    );
    let clip = AnimationClip::new(vec![BoneAnimationBinding {
        bone: BoneIndex(1),
        track: MovableBoneTrack::from_keyframes(vec![
            MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
            MovableBoneKeyframe::new(10, Vec3A::new(0.0, 0.0, 4.0), Quat::IDENTITY),
        ]),
    }]);
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_clip_frame(&clip, 5.0);

    let matrices = runtime.world_matrices();
    assert_eq!(matrices.len(), 2);
    assert_vec3a_near(translation(matrices[0]), Vec3A::new(1.0, 0.0, 0.0));
    assert_vec3a_near(translation(matrices[1]), Vec3A::new(1.0, 2.0, 2.0));
}

#[test]
fn split_physics_tick_matches_full_clip_evaluation_without_backend() {
    let mut after_child = BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 2.0, 0.0));
    after_child.transform_after_physics = true;
    let model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(None, Vec3A::new(1.0, 0.0, 0.0)),
            after_child,
        ])
        .unwrap(),
    );
    let clip = AnimationClip::new(vec![BoneAnimationBinding {
        bone: BoneIndex(0),
        track: MovableBoneTrack::from_keyframes(vec![
            MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
            MovableBoneKeyframe::new(10, Vec3A::new(0.0, 0.0, 4.0), Quat::IDENTITY),
        ]),
    }]);
    let mut full = RuntimeInstance::new(Arc::clone(&model));
    let mut split = RuntimeInstance::new(model);

    full.evaluate_clip_frame(&clip, 5.0);
    split.evaluate_clip_frame_before_physics(&clip, 5.0);
    let stats = split.step_physics(0.0);

    assert_eq!(stats.substeps, 0);
    assert_vec3a_near(
        translation(split.world_matrices()[0]),
        translation(full.world_matrices()[0]),
    );
    assert_vec3a_near(
        translation(split.world_matrices()[1]),
        translation(full.world_matrices()[1]),
    );
}

#[test]
fn physics_tick_accumulates_fixed_substeps_and_clamps_large_dt() {
    let model = Arc::new(ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap());
    let mut runtime = RuntimeInstance::new(model);
    runtime.set_physics_tick_config(PhysicsTickConfig {
        fixed_substep_seconds: 1.0 / 120.0,
        max_substeps_per_tick: 4,
    });

    let first = runtime.step_physics(1.0 / 240.0);
    assert_eq!(first.substeps, 0);
    assert!((first.accumulator_seconds - 1.0 / 240.0).abs() < 1.0e-6);

    let second = runtime.step_physics(1.0 / 240.0);
    assert_eq!(second.substeps, 1);
    assert!(second.accumulator_seconds.abs() < 1.0e-6);

    let clamped = runtime.step_physics(1.0);
    assert_eq!(clamped.clamped_dt_seconds, 4.0 / 120.0);
    assert_eq!(clamped.substeps, 4);
    assert!(runtime.physics_accumulator_seconds() <= 1.0 / 120.0);
}

#[test]
fn physics_mode_defaults_off_and_resets_tick_when_disabled() {
    let model = Arc::new(ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap());
    let mut runtime = RuntimeInstance::new(model);

    assert_eq!(runtime.physics_mode(), PhysicsMode::Off);
    assert!(!runtime.physics_mode().steps_backend());
    assert!(PhysicsMode::Trace.steps_backend());
    assert!(PhysicsMode::Live.steps_backend());

    runtime.set_physics_mode(PhysicsMode::Trace);
    let stats = runtime.advance_physics_tick_clock(1.0 / 240.0);
    assert_eq!(stats.substeps, 0);
    assert!(runtime.physics_accumulator_seconds() > 0.0);

    runtime.set_physics_mode(PhysicsMode::Live);
    assert!(runtime.physics_accumulator_seconds() > 0.0);

    runtime.set_physics_mode(PhysicsMode::Off);

    assert_eq!(runtime.physics_mode(), PhysicsMode::Off);
    assert_eq!(runtime.physics_accumulator_seconds(), 0.0);
}

#[test]
fn split_physics_tick_preserves_after_physics_ik_options() {
    let mut ik_bone = BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0));
    ik_bone.transform_after_physics = true;
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                ik_bone,
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 10,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let clip = AnimationClip::new(vec![]);
    let mut runtime = RuntimeInstance::new(model);
    let options = IkSolveOptions {
        tolerance: 0.0,
        max_iterations_cap: Some(1),
    };

    runtime.evaluate_clip_frame_before_physics_with_ik_options(&clip, 0.0, options);
    runtime.reset_ik_runtime_stats();
    runtime.step_physics_with_ik_options(0.0, options);

    assert_eq!(runtime.ik_runtime_stats()[0].solver_evaluations, 1);
    assert_eq!(runtime.ik_runtime_stats()[0].configured_iterations, 1);
}

#[test]
fn evaluate_clip_frame_without_ik_leaves_ik_unsolved() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let clip = AnimationClip::new(vec![]);

    let mut without_ik = RuntimeInstance::new(Arc::clone(&model));
    let mut with_ik = RuntimeInstance::new(model);

    without_ik.evaluate_clip_frame_without_ik(&clip, 0.0);
    with_ik.evaluate_clip_frame(&clip, 0.0);

    // Without IK: effector bone stays at rest position (1, 0, 0)
    assert_vec3a_near(
        translation(without_ik.world_matrices()[1]),
        Vec3A::new(1.0, 0.0, 0.0),
    );
    // With IK: effector bone rotates toward target at (0, 1, 0)
    assert_vec3a_near(
        translation(with_ik.world_matrices()[1]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn ik_options_cap_configured_iterations() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 100,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.reset_ik_runtime_stats();
    runtime.evaluate_current_pose_with_ik_options(super::IkSolveOptions {
        tolerance: 0.0,
        max_iterations_cap: Some(5),
    });

    assert_eq!(runtime.ik_runtime_stats()[0].configured_iterations, 5);
}

// ---- morph expansion tests ----

fn assert_near(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(
        delta < 1.0e-5,
        "actual={actual:?} expected={expected:?} delta={delta:?}"
    );
}

#[test]
fn bone_morph_position_offset_drives_world_position() {
    let model = Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
            ],
            Vec::new(),
            Vec::new(),
            crate::MorphInit {
                morph_count: 1,
                bone_offsets: vec![crate::BoneMorphOffset {
                    target_bone: BoneIndex(1),
                    position_offset: Vec3A::new(0.0, 0.0, 2.0),
                    rotation_offset: Quat::IDENTITY,
                }],
                bone_spans: vec![crate::MorphOffsetSpan { start: 0, count: 1 }],
                group_offsets: vec![],
                group_spans: vec![crate::MorphOffsetSpan::default()],
                ..crate::MorphInit::default()
            },
        )
        .unwrap(),
    );
    let clip = AnimationClip::new_with_morphs(
        Vec::new(),
        vec![crate::MorphAnimationBinding {
            morph: crate::MorphIndex(0),
            track: crate::MorphTrack::from_keyframes(vec![
                crate::MorphKeyframe::new(0, 0.0),
                crate::MorphKeyframe::new(10, 1.0),
            ]),
        }],
    );
    let mut runtime = RuntimeInstance::new_with_morph_count(model, 1);

    runtime.evaluate_clip_frame(&clip, 5.0);

    // weight = 0.5: bone offset = (0,0,2) * 0.5 = (0,0,1)
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(0.0, 1.0, 1.0),
    );
    assert_near(runtime.morph_weights()[0], 0.5);
}

#[test]
fn bone_morph_rotation_offset_affects_child_position() {
    let model = Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(1)), Vec3A::new(1.0, 0.0, 0.0)),
            ],
            Vec::new(),
            Vec::new(),
            crate::MorphInit {
                morph_count: 1,
                bone_offsets: vec![crate::BoneMorphOffset {
                    target_bone: BoneIndex(1),
                    position_offset: Vec3A::ZERO,
                    rotation_offset: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                }],
                bone_spans: vec![crate::MorphOffsetSpan { start: 0, count: 1 }],
                group_offsets: vec![],
                group_spans: vec![crate::MorphOffsetSpan::default()],
                ..crate::MorphInit::default()
            },
        )
        .unwrap(),
    );
    let clip = AnimationClip::new_with_morphs(
        Vec::new(),
        vec![crate::MorphAnimationBinding {
            morph: crate::MorphIndex(0),
            track: crate::MorphTrack::from_keyframes(vec![
                crate::MorphKeyframe::new(0, 0.0),
                crate::MorphKeyframe::new(10, 1.0),
            ]),
        }],
    );
    let mut runtime = RuntimeInstance::new_with_morph_count(model, 1);

    runtime.evaluate_clip_frame(&clip, 10.0);

    // weight = 1.0: bone 1 (rest 1,0,0) rotated Z-90 by morph (position unchanged)
    // bone 2 at (1,0,0) relative to bone 1: world = (1,0,0) + (0,1,0)
    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(1.0, 1.0, 0.0),
    );
}

#[test]
fn group_morph_contributes_to_bone_morph_weight() {
    // PMX order: child (bone morph) has smaller index than parent (group morph)
    // Morph 0 = bone morph, Morph 1 = group morph with MorphIndex(0) as child.
    let model = Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
            ],
            Vec::new(),
            Vec::new(),
            crate::MorphInit {
                morph_count: 2,
                bone_offsets: vec![crate::BoneMorphOffset {
                    target_bone: BoneIndex(1),
                    position_offset: Vec3A::new(0.0, 0.0, 2.0),
                    rotation_offset: Quat::IDENTITY,
                }],
                bone_spans: vec![
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                    crate::MorphOffsetSpan::default(),
                ],
                group_offsets: vec![crate::GroupMorphOffset {
                    child_morph: crate::MorphIndex(0),
                    ratio: 0.5,
                }],
                group_spans: vec![
                    crate::MorphOffsetSpan::default(),
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                ],
                ..crate::MorphInit::default()
            },
        )
        .unwrap(),
    );
    // VMD track only on group morph (index 1), weight = 1.0
    let clip = AnimationClip::new_with_morphs(
        Vec::new(),
        vec![crate::MorphAnimationBinding {
            morph: crate::MorphIndex(1),
            track: crate::MorphTrack::from_keyframes(vec![
                crate::MorphKeyframe::new(0, 0.0),
                crate::MorphKeyframe::new(10, 1.0),
            ]),
        }],
    );
    let mut runtime = RuntimeInstance::new_with_morph_count(model, 2);

    runtime.evaluate_clip_frame(&clip, 10.0);

    // Group expansion: morph_weights[0] += 1.0 * 0.5 = 0.5
    // Bone morph applies: (0,0,2) * 0.5 = (0,0,1)
    assert_near(runtime.morph_weights()[0], 0.5);
    assert_near(runtime.morph_weights()[1], 1.0);
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(0.0, 1.0, 1.0),
    );
}

#[test]
fn group_morph_can_reference_later_child_morph() {
    let model = Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
            ],
            Vec::new(),
            Vec::new(),
            crate::MorphInit {
                morph_count: 2,
                bone_offsets: vec![crate::BoneMorphOffset {
                    target_bone: BoneIndex(1),
                    position_offset: Vec3A::new(0.0, 0.0, 2.0),
                    rotation_offset: Quat::IDENTITY,
                }],
                bone_spans: vec![
                    crate::MorphOffsetSpan::default(),
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                ],
                group_offsets: vec![crate::GroupMorphOffset {
                    child_morph: crate::MorphIndex(1),
                    ratio: 0.5,
                }],
                group_spans: vec![
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                    crate::MorphOffsetSpan::default(),
                ],
                ..crate::MorphInit::default()
            },
        )
        .unwrap(),
    );
    let clip = AnimationClip::new_with_morphs(
        Vec::new(),
        vec![crate::MorphAnimationBinding {
            morph: crate::MorphIndex(0),
            track: crate::MorphTrack::from_keyframes(vec![crate::MorphKeyframe::new(0, 1.0)]),
        }],
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_clip_frame(&clip, 0.0);

    assert_near(runtime.morph_weights()[0], 1.0);
    assert_near(runtime.morph_weights()[1], 0.5);
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(0.0, 1.0, 1.0),
    );
}

#[test]
fn chained_group_morphs_descend_to_bone_morph_weight() {
    let model = Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
            ],
            Vec::new(),
            Vec::new(),
            crate::MorphInit {
                morph_count: 3,
                bone_offsets: vec![crate::BoneMorphOffset {
                    target_bone: BoneIndex(1),
                    position_offset: Vec3A::new(0.0, 0.0, 2.0),
                    rotation_offset: Quat::IDENTITY,
                }],
                bone_spans: vec![
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                    crate::MorphOffsetSpan::default(),
                    crate::MorphOffsetSpan::default(),
                ],
                group_offsets: vec![
                    crate::GroupMorphOffset {
                        child_morph: crate::MorphIndex(0),
                        ratio: 0.25,
                    },
                    crate::GroupMorphOffset {
                        child_morph: crate::MorphIndex(1),
                        ratio: 0.5,
                    },
                ],
                group_spans: vec![
                    crate::MorphOffsetSpan::default(),
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                    crate::MorphOffsetSpan { start: 1, count: 1 },
                ],
                ..crate::MorphInit::default()
            },
        )
        .unwrap(),
    );
    let clip = AnimationClip::new_with_morphs(
        Vec::new(),
        vec![crate::MorphAnimationBinding {
            morph: crate::MorphIndex(2),
            track: crate::MorphTrack::from_keyframes(vec![crate::MorphKeyframe::new(0, 1.0)]),
        }],
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_clip_frame(&clip, 0.0);

    assert_near(runtime.morph_weights()[2], 1.0);
    assert_near(runtime.morph_weights()[1], 0.5);
    assert_near(runtime.morph_weights()[0], 0.125);
    assert_vec3a_near(
        translation(runtime.world_matrices()[1]),
        Vec3A::new(0.0, 1.0, 0.25),
    );
}

#[test]
fn expand_morphs_noop_when_no_morph_defs() {
    let model = Arc::new(ModelArena::new(vec![BoneInit::new(None, Vec3A::ZERO)]).unwrap());
    let mut runtime = RuntimeInstance::new_with_morph_count(model, 1);
    runtime
        .pose_mut()
        .set_morph_weight(crate::MorphIndex(0), 1.0);
    runtime.expand_morphs();
    // No crash = pass
    assert_near(runtime.morph_weights()[0], 1.0);
}

#[test]
fn clamps_link_local_rotation_to_angle_limit() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![
                    IkLinkInit::new(BoneIndex(0)).with_angle_limit(IkAngleLimit::new(
                        Vec3A::new(0.0, 0.0, 0.0),
                        Vec3A::new(0.0, 0.0, std::f32::consts::FRAC_PI_4),
                    )),
                ],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    let expected = Vec3A::new(
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
        0.0,
    );
    assert_vec3a_near(translation(runtime.world_matrices()[1]), expected);
}

#[test]
fn multi_axis_limited_link_solves_before_clamping() {
    let local_effector = Vec3A::X;
    let local_target = Vec3A::new(0.25, 0.55, 0.80).normalize();
    let limits = IkAngleLimit::new(Vec3A::new(0.0, -1.0, -1.0), Vec3A::new(0.0, 1.0, 1.0));
    let base_rotations = vec![Quat::IDENTITY];
    let mut ik_rotations = vec![Quat::IDENTITY];
    let mut chain_states = vec![super::ChainLinkState {
        previous_euler: [0.0; 3],
        plane_mode_angle: 0.0,
    }];

    super::solve_limited_axes_link_step(super::LimitedAxesLinkStepInput {
        local_effector: &local_effector,
        local_target: &local_target,
        link_index: 0,
        base_rotations: &base_rotations,
        ik_rotations: &mut ik_rotations,
        chain_states: &mut chain_states,
        limits,
        limit_angle: 0.0,
    });

    let current_direction = ik_rotations[0].mul_vec3a(local_effector).normalize();
    let legacy_direction =
        legacy_clamp_only_limited_direction(local_effector, local_target, limits);
    let current_error = (current_direction - local_target).length();
    let legacy_error = (legacy_direction - local_target).length();

    assert!(
        current_error < legacy_error - 0.015,
        "current_error={current_error:.6} legacy_error={legacy_error:.6} current={current_direction:?} legacy={legacy_direction:?} target={local_target:?}"
    );
    assert!(
        chain_states[0].previous_euler[1].abs() > 0.1
            && chain_states[0].previous_euler[2].abs() > 0.1,
        "multi-axis limited IK should use both Y and Z axes; euler={:?}",
        chain_states[0].previous_euler
    );
}

#[test]
fn multi_axis_limited_link_applies_limits_to_total_rotation() {
    let local_effector = Vec3A::new(0.25, 0.45, 0.85).normalize();
    let local_target = Vec3A::new(0.55, 0.15, 0.80).normalize();
    let limits = IkAngleLimit::new(Vec3A::new(-1.0, -1.0, 0.0), Vec3A::new(1.0, 1.0, 0.0));
    let base_rotations = vec![Quat::from_rotation_z(0.45)];
    let mut ik_rotations = vec![Quat::IDENTITY];
    let mut chain_states = vec![super::ChainLinkState {
        previous_euler: [0.0; 3],
        plane_mode_angle: 0.0,
    }];

    super::solve_limited_axes_link_step(super::LimitedAxesLinkStepInput {
        local_effector: &local_effector,
        local_target: &local_target,
        link_index: 0,
        base_rotations: &base_rotations,
        ik_rotations: &mut ik_rotations,
        chain_states: &mut chain_states,
        limits,
        limit_angle: 0.0,
    });

    let base_direction = base_rotations[0].mul_vec3a(local_effector).normalize();
    let effective = (ik_rotations[0] * base_rotations[0]).normalize();
    let stale_direction = limited_direction_without_fixed_axis_working_update(
        local_effector,
        local_target,
        base_rotations[0],
        limits,
    );
    let solved_direction = effective.mul_vec3a(local_effector).normalize();
    assert_near(chain_states[0].previous_euler[2], 0.0);
    assert!(
        (solved_direction - stale_direction).length() > 0.05,
        "fixed axis clamp should affect later axis solve; solved={solved_direction:?} stale={stale_direction:?}"
    );
    assert!(
        (solved_direction - local_target).length() < (base_direction - local_target).length(),
        "non-identity base should still solve toward target; base={base_direction:?} solved={solved_direction:?} target={local_target:?}"
    );
}

fn limited_direction_without_fixed_axis_working_update(
    local_effector: Vec3A,
    local_target: Vec3A,
    base: Quat,
    limits: IkAngleLimit,
) -> Vec3A {
    let mut total_euler =
        super::decompose_euler_xyz(&super::quat_to_rotation_mat3(base), &[0.0; 3]);
    let mut working_effector = local_effector;
    let target = local_target.normalize();

    for axis_index in [2usize, 1, 0] {
        let (lower, upper) = super::limit_axis_bounds(limits, axis_index);
        if lower == 0.0 && upper == 0.0 {
            total_euler[axis_index] = total_euler[axis_index].clamp(lower, upper);
            continue;
        }

        let axis = super::axis_vec(axis_index);
        let signed_angle = super::signed_projected_angle(working_effector, target, axis);
        if signed_angle.abs() <= 1.0e-6 {
            continue;
        }
        let next = (total_euler[axis_index] + signed_angle).clamp(lower, upper);
        let applied = next - total_euler[axis_index];
        total_euler[axis_index] = next;
        if applied.abs() > 0.0 {
            working_effector =
                Quat::from_axis_angle(axis.into(), applied).mul_vec3a(working_effector);
        }
    }

    super::euler_xyz_to_quat(&total_euler)
        .normalize()
        .mul_vec3a(local_effector)
        .normalize()
}

fn legacy_clamp_only_limited_direction(
    local_effector: Vec3A,
    local_target: Vec3A,
    limits: IkAngleLimit,
) -> Vec3A {
    let local_eff_n = local_effector.normalize();
    let local_tgt_n = local_target.normalize();
    let dot = local_eff_n.dot(local_tgt_n).clamp(-1.0, 1.0);
    let angle = dot.acos();
    let axis = local_eff_n.cross(local_tgt_n);
    let axis_vec = if axis.length() < 1e-5 {
        if dot > -1.0 + 1e-5 {
            return local_eff_n;
        }
        let basis = if local_eff_n.x.abs() < 0.9 {
            Vec3A::new(1.0, 0.0, 0.0)
        } else {
            Vec3A::new(0.0, 1.0, 0.0)
        };
        local_eff_n.cross(basis).normalize()
    } else {
        axis.normalize()
    };
    let rotation = Quat::from_axis_angle(axis_vec.into(), angle).normalize();
    let euler = super::decompose_euler_xyz(&super::quat_to_rotation_mat3(rotation), &[0.0; 3]);
    let clamped = [
        euler[0].clamp(limits.min.x, limits.max.x),
        euler[1].clamp(limits.min.y, limits.max.y),
        euler[2].clamp(limits.min.z, limits.max.z),
    ];
    super::euler_xyz_to_quat(&clamped)
        .normalize()
        .mul_vec3a(local_effector)
        .normalize()
}

#[test]
fn plane_link_step_matches_saba_total_axis_rotation() {
    let base = Quat::from_rotation_x(0.3);
    let base_rotations = vec![base];
    let mut ik_rotations = vec![Quat::IDENTITY];
    let mut chain_states = vec![super::ChainLinkState {
        previous_euler: [0.0; 3],
        plane_mode_angle: 0.0,
    }];
    let local_effector = Vec3A::X;
    let local_target = Vec3A::Y;

    super::solve_plane_link_step(super::PlaneLinkStepInput {
        local_effector: &local_effector,
        local_target: &local_target,
        link_index: 0,
        base_rotations: &base_rotations,
        ik_rotations: &mut ik_rotations,
        chain_states: &mut chain_states,
        axis_index: 2,
        limits: IkAngleLimit::new(
            Vec3A::new(-std::f32::consts::PI, 0.0, -std::f32::consts::PI),
            Vec3A::new(std::f32::consts::PI, 0.0, std::f32::consts::PI),
        ),
        iteration: 0,
        limit_angle: 0.0,
    });

    let effective = (ik_rotations[0] * base_rotations[0]).normalize();
    assert_near(
        chain_states[0].plane_mode_angle,
        std::f32::consts::FRAC_PI_2,
    );
    assert_vec3a_near(
        effective.mul_vec3a(Vec3A::X),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).mul_vec3a(Vec3A::X),
    );
    assert_vec3a_near(effective.mul_vec3a(Vec3A::Z), Vec3A::Z);
}

#[test]
fn append_rotation_propagates_post_ik_link_rotation() {
    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(3)), Vec3A::new(1.0, 0.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
            vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[4]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn append_source_with_own_append_includes_ik_link_rotation() {
    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
            vec![
                AppendTransformInit::new(BoneIndex(0), BoneIndex(3), 1.0).with_rotation(),
                AppendTransformInit::new(BoneIndex(4), BoneIndex(0), 1.0).with_rotation(),
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.pose_mut().set_local_rotation(
        BoneIndex(3),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
    );

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[5]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn without_ik_evaluation_clears_previous_ik_link_rotation_for_append_sources() {
    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
            vec![
                AppendTransformInit::new(BoneIndex(0), BoneIndex(3), 1.0).with_rotation(),
                AppendTransformInit::new(BoneIndex(4), BoneIndex(0), 1.0).with_rotation(),
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.pose_mut().set_local_rotation(
        BoneIndex(3),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
    );

    runtime.evaluate_current_pose();
    runtime.evaluate_current_pose_without_ik();

    assert_vec3a_near(
        translation(runtime.world_matrices()[5]),
        Vec3A::new(
            std::f32::consts::FRAC_1_SQRT_2,
            std::f32::consts::FRAC_1_SQRT_2,
            0.0,
        ),
    );
}

#[test]
fn shared_ik_link_preserves_accumulated_rotation_for_later_append_source() {
    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
            ],
            vec![
                IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
                IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(2),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
            ],
            vec![
                AppendTransformInit::new(BoneIndex(0), BoneIndex(3), 1.0).with_rotation(),
                AppendTransformInit::new(BoneIndex(4), BoneIndex(0), 1.0).with_rotation(),
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.pose_mut().set_local_rotation(
        BoneIndex(3),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
    );

    runtime.evaluate_current_pose();

    assert_eq!(runtime.ik_runtime_stats()[1].tolerance_precheck_breaks, 1);
    assert_vec3a_near(
        translation(runtime.world_matrices()[5]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn earlier_append_target_updates_after_later_ik_link_rotation() {
    let mut append_target = BoneInit::new(None, Vec3A::ZERO);
    append_target.transform_order = 0;
    let mut append_child = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);
    append_child.transform_order = 1;
    let mut link = BoneInit::new(None, Vec3A::ZERO);
    link.transform_order = 10;
    let mut effector = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
    effector.transform_order = 11;
    let mut controller = BoneInit::new(None, Vec3A::Y);
    controller.transform_order = 12;

    let model = Arc::new(
        ModelArena::new_full(
            vec![link, effector, controller, append_target, append_child],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
            vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[4]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn earlier_append_target_preserves_later_ik_link_source_append_rotation() {
    let mut append_target = BoneInit::new(None, Vec3A::ZERO);
    append_target.transform_order = 0;
    let mut append_child = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);
    append_child.transform_order = 1;
    let mut append_driver = BoneInit::new(None, Vec3A::ZERO);
    append_driver.transform_order = 9;
    let mut link = BoneInit::new(None, Vec3A::ZERO);
    link.transform_order = 10;
    let mut effector = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
    effector.transform_order = 11;
    let mut controller = BoneInit::new(None, Vec3A::Y);
    controller.transform_order = 12;

    let model = Arc::new(
        ModelArena::new_full(
            vec![
                link,
                effector,
                controller,
                append_target,
                append_child,
                append_driver,
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
            vec![
                AppendTransformInit::new(BoneIndex(0), BoneIndex(5), 1.0).with_rotation(),
                AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation(),
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.pose_mut().set_local_rotation(
        BoneIndex(5),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
    );

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[4]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn transitive_append_target_recomputes_after_opposite_phase_ik_source_rotation() {
    let mut append_a = BoneInit::new(None, Vec3A::ZERO);
    append_a.transform_order = 0;
    let mut append_b = BoneInit::new(None, Vec3A::ZERO);
    append_b.transform_order = 1;
    let mut append_b_child = BoneInit::new(Some(BoneIndex(4)), Vec3A::X);
    append_b_child.transform_order = 2;

    let mut link = BoneInit::new(None, Vec3A::ZERO);
    link.transform_order = 10;
    link.transform_after_physics = true;
    let mut effector = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
    effector.transform_order = 11;
    effector.transform_after_physics = true;
    let mut controller = BoneInit::new(None, Vec3A::Y);
    controller.transform_order = 12;
    controller.transform_after_physics = true;

    let model = Arc::new(
        ModelArena::new_full(
            vec![
                link,
                effector,
                controller,
                append_a,
                append_b,
                append_b_child,
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
            vec![
                AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation(),
                AppendTransformInit::new(BoneIndex(4), BoneIndex(3), 1.0).with_rotation(),
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[5]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn mixed_phase_ik_updates_opposite_phase_controller_dependency() {
    let mut link_a = BoneInit::new(None, Vec3A::ZERO);
    link_a.transform_order = 0;
    let mut effector_a = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
    effector_a.transform_order = 1;
    let mut controller_a = BoneInit::new(None, Vec3A::Y);
    controller_a.transform_order = 2;
    let mut after_append = BoneInit::new(None, Vec3A::ZERO);
    after_append.transform_order = 3;
    after_append.transform_after_physics = true;
    let mut link_b = BoneInit::new(None, Vec3A::ZERO);
    link_b.transform_order = 4;
    let mut effector_b = BoneInit::new(Some(BoneIndex(4)), Vec3A::X);
    effector_b.transform_order = 5;
    let mut controller_b = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);
    controller_b.transform_order = 6;

    let model = Arc::new(
        ModelArena::new_full(
            vec![
                link_a,
                effector_a,
                controller_a,
                after_append,
                link_b,
                effector_b,
                controller_b,
            ],
            vec![
                IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
                IkSolverInit {
                    ik_bone: BoneIndex(6),
                    target_bone: BoneIndex(5),
                    links: vec![IkLinkInit::new(BoneIndex(4))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
            ],
            vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[5]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[6]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
    assert_vec3a_near(
        translation(runtime.world_matrices()[3]),
        Vec3A::new(0.0, 0.0, 0.0),
    );
}

#[test]
fn after_physics_plain_child_recomputes_after_pre_physics_parent_ik() {
    let mut after_child = BoneInit::new(Some(BoneIndex(0)), Vec3A::X);
    after_child.transform_order = 3;
    after_child.transform_after_physics = true;

    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                BoneInit::new(None, Vec3A::Y),
                after_child,
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[3]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn pre_physics_child_recomputes_after_after_physics_append_parent() {
    let mut after_parent = BoneInit::new(None, Vec3A::ZERO);
    after_parent.transform_order = 1;
    after_parent.transform_after_physics = true;
    let mut pre_child = BoneInit::new(Some(BoneIndex(1)), Vec3A::X);
    pre_child.transform_order = 2;

    let model = Arc::new(
        ModelArena::new_full(
            vec![BoneInit::new(None, Vec3A::ZERO), after_parent, pre_child],
            Vec::new(),
            vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.pose_mut().set_local_rotation(
        BoneIndex(0),
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    );

    runtime.reset_world_matrix_bone_update_count();
    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[2]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
    assert_eq!(
        runtime.world_matrix_bone_update_trailing_bookend_count(),
        2,
        "cross-phase append should refresh a trailing suffix, not the full eval-order bookend"
    );
    assert_eq!(
        runtime.world_matrix_bone_update_leading_bookend_count(),
        3,
        "leading bookend should still refresh the full eval-order prefix"
    );
}

#[test]
fn append_target_recomputes_after_opposite_phase_ik_source_rotation() {
    let mut after_controller = BoneInit::new(None, Vec3A::Y);
    after_controller.transform_order = 2;
    after_controller.transform_after_physics = true;
    let mut append_target = BoneInit::new(None, Vec3A::ZERO);
    append_target.transform_order = 3;
    let append_child = BoneInit::new(Some(BoneIndex(3)), Vec3A::X);

    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                after_controller,
                append_target,
                append_child,
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(2),
                target_bone: BoneIndex(1),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 1,
                limit_angle: 0.0,
            }],
            vec![AppendTransformInit::new(BoneIndex(3), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    assert_vec3a_near(
        translation(runtime.world_matrices()[4]),
        Vec3A::new(0.0, 1.0, 0.0),
    );
}

#[test]
fn scratch_ik_capacities_stable_after_repeated_evaluate() {
    let model = Arc::new(
        ModelArena::new_with_ik(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(3)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(Some(BoneIndex(4)), Vec3A::new(1.0, 0.0, 0.0)),
                BoneInit::new(None, Vec3A::new(0.0, 1.0, 0.0)),
            ],
            vec![
                IkSolverInit {
                    ik_bone: BoneIndex(2),
                    target_bone: BoneIndex(1),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
                IkSolverInit {
                    ik_bone: BoneIndex(6),
                    target_bone: BoneIndex(5),
                    links: vec![IkLinkInit::new(BoneIndex(3)), IkLinkInit::new(BoneIndex(4))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                },
            ],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);

    runtime.evaluate_current_pose();

    let cap_links = runtime.ik_scratch.links.capacity();
    let cap_base = runtime.ik_scratch.base_rotations.capacity();
    let cap_base_ik = runtime.ik_scratch.base_ik_rotations.capacity();
    let cap_ik = runtime.ik_scratch.ik_rotations.capacity();
    let cap_best = runtime.ik_scratch.best_ik_rotations.capacity();
    let cap_chain = runtime.ik_scratch.chain_states.capacity();

    for _ in 0..10 {
        runtime.evaluate_current_pose();
    }

    assert_eq!(runtime.ik_scratch.links.capacity(), cap_links);
    assert_eq!(runtime.ik_scratch.base_rotations.capacity(), cap_base);
    assert_eq!(runtime.ik_scratch.base_ik_rotations.capacity(), cap_base_ik);
    assert_eq!(runtime.ik_scratch.ik_rotations.capacity(), cap_ik);
    assert_eq!(runtime.ik_scratch.best_ik_rotations.capacity(), cap_best);
    assert_eq!(runtime.ik_scratch.chain_states.capacity(), cap_chain);
}

#[test]
fn scratch_morph_capacity_stable_after_repeated_clip_frame() {
    let model = Arc::new(
        ModelArena::new_with_morphs(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::new(0.0, 1.0, 0.0)),
            ],
            Vec::new(),
            Vec::new(),
            crate::MorphInit {
                morph_count: 2,
                bone_offsets: vec![crate::BoneMorphOffset {
                    target_bone: BoneIndex(1),
                    position_offset: Vec3A::new(0.0, 0.0, 2.0),
                    rotation_offset: Quat::IDENTITY,
                }],
                bone_spans: vec![
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                    crate::MorphOffsetSpan::default(),
                ],
                group_offsets: vec![crate::GroupMorphOffset {
                    child_morph: crate::MorphIndex(0),
                    ratio: 0.5,
                }],
                group_spans: vec![
                    crate::MorphOffsetSpan::default(),
                    crate::MorphOffsetSpan { start: 0, count: 1 },
                ],
                ..crate::MorphInit::default()
            },
        )
        .unwrap(),
    );
    let clip = AnimationClip::new_with_morphs(
        Vec::new(),
        vec![crate::MorphAnimationBinding {
            morph: crate::MorphIndex(1),
            track: crate::MorphTrack::from_keyframes(vec![
                crate::MorphKeyframe::new(0, 0.0),
                crate::MorphKeyframe::new(10, 1.0),
            ]),
        }],
    );
    let mut runtime = RuntimeInstance::new_with_morph_count(model, 2);

    runtime.evaluate_clip_frame(&clip, 5.0);

    let cap_expanded = runtime.morph_scratch.expanded_weights.capacity();

    for _ in 0..10 {
        runtime.evaluate_clip_frame(&clip, 5.0);
    }

    assert_eq!(
        runtime.morph_scratch.expanded_weights.capacity(),
        cap_expanded
    );
}
