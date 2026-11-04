use glam::{Quat, Vec3A};
use mmd_anim_runtime::*;
use std::sync::Arc;

#[test]
fn authored_rotation_and_limits_match_full_runtime() {
    let goal = Vec3A::new(0.3, 0.8, 0.4);
    for limit in [
        None,
        Some(IkAngleLimit::new(
            Vec3A::new(-0.7, 0.0, 0.0),
            Vec3A::new(0.7, 0.0, 0.0),
        )),
        Some(IkAngleLimit::new(Vec3A::splat(-0.7), Vec3A::splat(0.7))),
    ] {
        for base in [
            Quat::IDENTITY,
            Quat::from_euler(glam::EulerRot::XYZ, 0.3, -0.4, 0.2),
        ] {
            let mut link = IkLinkInit::new(BoneIndex(0));
            link.angle_limit = limit;
            let model = Arc::new(
                ModelArena::new_with_ik(
                    vec![
                        BoneInit::new(None, Vec3A::ZERO),
                        BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
                        BoneInit::new(None, goal),
                    ],
                    vec![IkSolverInit {
                        ik_bone: BoneIndex(2),
                        target_bone: BoneIndex(1),
                        links: vec![link],
                        iteration_count: 4,
                        limit_angle: 0.5,
                    }],
                )
                .unwrap(),
            );
            let mut full = RuntimeInstance::new(model);
            full.pose_mut().set_local_rotation(BoneIndex(0), base);
            let options = IkSolveOptions {
                tolerance: 1e-4,
                max_iterations_cap: Some(4),
            };
            full.evaluate_current_pose_with_ik_options(options);
            let mut primitive = IkChainSolver::new(IkChainDefinition {
                parent_slots: vec![None, Some(0)],
                rest_positions: vec![Vec3A::ZERO, Vec3A::X],
                fixed_axes: vec![None; 2],
                target_slot: 1,
                links: vec![IkChainLinkDefinition {
                    bone_slot: 0,
                    angle_limit: limit,
                }],
                iteration_count: 4,
                limit_angle: 0.5,
            });
            let output = primitive.solve(IkChainPoseInput {
                parent_world_matrix: None,
                local_position_offsets: &[Vec3A::ZERO; 2],
                local_rotations: &[base, Quat::IDENTITY],
                goal_position: goal,
                tolerance: options.tolerance,
                max_iterations_cap: options.max_iterations_cap,
            });
            let expected = full.pose().local_rotation(BoneIndex(0));
            let actual = output.solved_link_rotations[0];
            assert!(
                actual.abs_diff_eq(expected, 1e-5) || actual.abs_diff_eq(-expected, 1e-5),
                "limit={limit:?}, base={base:?}, actual={actual:?}, expected={expected:?}"
            );
        }
    }
}

fn append_during_ik_model() -> Arc<ModelArena> {
    Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(1)), Vec3A::X),
                BoneInit::new(None, Vec3A::Y),
            ],
            vec![IkSolverInit {
                ik_bone: BoneIndex(3),
                target_bone: BoneIndex(2),
                links: vec![IkLinkInit::new(BoneIndex(0))],
                iteration_count: 2,
                limit_angle: 0.0,
            }],
            vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    )
}

#[test]
fn append_during_link_updates_requires_model_evaluation() {
    let mut full = RuntimeInstance::new(append_during_ik_model());
    full.evaluate_current_pose();
    let mut primitive = IkChainSolver::new(IkChainDefinition {
        parent_slots: vec![None, Some(0), Some(1)],
        rest_positions: vec![Vec3A::ZERO, Vec3A::ZERO, Vec3A::X],
        fixed_axes: vec![None; 3],
        target_slot: 2,
        links: vec![IkChainLinkDefinition {
            bone_slot: 0,
            angle_limit: None,
        }],
        iteration_count: 2,
        limit_angle: 0.0,
    });
    // The initial Append rotation is identity, so both paths start with
    // identical FK and effector. Refreshing Append after each link is essential.
    let out = primitive.solve_with_initial_effector(
        IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &[Vec3A::ZERO; 3],
            local_rotations: &[Quat::IDENTITY; 3],
            goal_position: Vec3A::Y,
            tolerance: 1e-4,
            max_iterations_cap: Some(2),
        },
        Some(Vec3A::X),
    );
    assert!(
        out.solved_link_rotations[0]
            .dot(full.pose().local_rotation(BoneIndex(0)))
            .abs()
            < 0.99
    );
}

fn layered_shared_model(after_physics: bool) -> Arc<ModelArena> {
    let mut bones = vec![
        BoneInit::new(None, Vec3A::ZERO),
        BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
        BoneInit::new(Some(BoneIndex(1)), Vec3A::X),
        BoneInit::new(Some(BoneIndex(2)), Vec3A::X),
        BoneInit::new(None, Vec3A::new(0.5, 1.2, 0.3)),
        BoneInit::new(None, Vec3A::new(1.0, 0.2, 0.8)),
        BoneInit::new(None, Vec3A::ZERO),
    ];
    bones[3].transform_order = 4; // later-layer effector retains world history
    bones[4].transform_order = 1;
    bones[5].transform_order = 2;
    bones[5].transform_after_physics = after_physics;
    bones[6].transform_after_physics = after_physics;
    Arc::new(
        ModelArena::new_full(
            bones,
            vec![
                IkSolverInit {
                    ik_bone: BoneIndex(4),
                    target_bone: BoneIndex(2),
                    links: vec![IkLinkInit::new(BoneIndex(1))],
                    iteration_count: 2,
                    limit_angle: 0.5,
                },
                IkSolverInit {
                    ik_bone: BoneIndex(5),
                    target_bone: BoneIndex(3),
                    links: vec![IkLinkInit::new(BoneIndex(2)), IkLinkInit::new(BoneIndex(1))],
                    iteration_count: 2,
                    limit_angle: 0.5,
                },
            ],
            vec![AppendTransformInit::new(BoneIndex(6), BoneIndex(1), 0.5).with_rotation()],
        )
        .unwrap(),
    )
}

fn compare_pose(a: &RuntimeInstance, b: &RuntimeInstance) {
    for (index, (a, b)) in a
        .world_matrices()
        .iter()
        .zip(b.world_matrices())
        .enumerate()
    {
        assert!(a.abs_diff_eq(*b, 1e-5), "bone {index}: {a:?} != {b:?}");
    }
}

#[test]
fn model_bound_route_matches_full_runtime_for_composed_rigs_and_seek() {
    // Small fixed corpus: live Append inside CCD, shared links, later-layer
    // effectors, IK->Append and both evaluation phases. No external assets.
    for model in [
        append_during_ik_model(),
        layered_shared_model(false),
        layered_shared_model(true),
    ] {
        let goals: Vec<_> = model.ik_solvers().iter().map(|s| s.ik_bone).collect();
        let rig = HostRigDefinition::new(model.clone(), &[], &goals).unwrap();
        let mut full = RuntimeInstance::new(model.clone());
        let mut host = RuntimeInstance::new(model.clone());
        let positions = vec![Vec3A::ZERO; model.bone_count()];
        let mut rotations = vec![Quat::IDENTITY; model.bone_count()];
        let scales = vec![Vec3A::ONE; model.bone_count()];
        let enabled = vec![1; model.ik_count()];
        let options = IkSolveOptions {
            tolerance: 1e-4,
            max_iterations_cap: Some(2),
        };
        for angle in [0.3, 0.3, -0.2, 0.8, 0.3] {
            rotations[0] = Quat::from_rotation_y(angle);
            let input = HostPoseView {
                local_position_offsets: &positions,
                local_rotations: &rotations,
                local_scales: &scales,
                morph_weights: &[],
                ik_enabled: &enabled,
            };
            full.apply_host_pose(&input).unwrap();
            full.evaluate_current_pose_with_ik_options(options);
            host.evaluate_host_rig_pose(&rig, &input, options).unwrap();
            compare_pose(&host, &full);
            // Independent sampling/seek: clear BOTH accumulator and target-world
            // history, then replay fresh input. Pose reset alone is insufficient.
            host.reset_ik_history();
            host.evaluate_host_rig_pose(&rig, &input, options).unwrap();
            let mut fresh = RuntimeInstance::new(model.clone());
            fresh.apply_host_pose(&input).unwrap();
            fresh.evaluate_current_pose_with_ik_options(options);
            compare_pose(&host, &fresh);
            // Restore matching history for the next sequential evaluation.
            full.reset_ik_history();
            full.apply_host_pose(&input).unwrap();
            full.evaluate_current_pose_with_ik_options(options);
        }
    }
}

#[test]
fn shared_link_primitives_match_when_later_chain_gets_its_initial_effector() {
    let bones = vec![
        BoneInit::new(None, Vec3A::ZERO),
        BoneInit::new(Some(BoneIndex(0)), Vec3A::X),
        BoneInit::new(Some(BoneIndex(0)), Vec3A::Y),
        BoneInit::new(None, Vec3A::new(0.2, 0.8, 0.3)),
        BoneInit::new(None, Vec3A::new(0.6, 0.1, 0.7)),
    ];
    let goals = [Vec3A::new(0.2, 0.8, 0.3), Vec3A::new(0.6, 0.1, 0.7)];
    let model = Arc::new(
        ModelArena::new_with_ik(
            bones,
            (0..2)
                .map(|i| IkSolverInit {
                    ik_bone: BoneIndex(3 + i),
                    target_bone: BoneIndex(1 + i),
                    links: vec![IkLinkInit::new(BoneIndex(0))],
                    iteration_count: 1,
                    limit_angle: 0.0,
                })
                .collect(),
        )
        .unwrap(),
    );
    let base = Quat::from_euler(glam::EulerRot::XYZ, 0.2, -0.3, 0.1);
    let mut full = RuntimeInstance::new(model);
    full.pose_mut().set_local_rotation(BoneIndex(0), base);
    full.evaluate_current_pose();
    let mut solved = base;
    for (i, rest) in [Vec3A::X, Vec3A::Y].into_iter().enumerate() {
        let mut primitive = IkChainSolver::new(IkChainDefinition {
            parent_slots: vec![None, Some(0)],
            rest_positions: vec![Vec3A::ZERO, rest],
            fixed_axes: vec![None; 2],
            target_slot: 1,
            links: vec![IkChainLinkDefinition {
                bone_slot: 0,
                angle_limit: None,
            }],
            iteration_count: 1,
            limit_angle: 0.0,
        });
        solved = primitive
            .solve_with_initial_effector(
                IkChainPoseInput {
                    parent_world_matrix: None,
                    local_position_offsets: &[Vec3A::ZERO; 2],
                    local_rotations: &[solved, Quat::IDENTITY],
                    goal_position: goals[i],
                    tolerance: 1e-4,
                    max_iterations_cap: Some(1),
                },
                Some(base.mul_vec3a(rest)),
            )
            .solved_link_rotations[0];
    }
    assert!(solved.dot(full.pose().local_rotation(BoneIndex(0))).abs() > 1.0 - 1e-6);
}
