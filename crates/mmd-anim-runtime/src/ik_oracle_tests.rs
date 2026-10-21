//! Original MMD 9.32 x64, neutral five-bone PMX/VMD, captured frame rounded to 10.
//! Source PMX hashes below identify independently generated fixtures.
//! No external assets or installed MMD are required by this regression test.
// Keep the decimal precision of independently captured reference values.
#![allow(clippy::excessive_precision, clippy::approx_constant)]
use super::*;
use crate::{BoneIndex, BoneInit, IkLinkInit, IkSolverInit, ModelArena, RuntimeInstance};
use std::sync::Arc;

#[test]
fn original_mmd_limited_chains_match_both_drivers() {
    // name, loops, angle, goal, proximal limits, distal limits, world rotations, tip
    let cases = [
        // PMX 77db0bff47c9e5d37e26a66337e6d984f9f72fe66960538c12d0d6f5979b0200
        (
            "limited-n1",
            1,
            0.5,
            [-0.11982713, 1.7151622, 0.0],
            [
                [[0.0, 0.0, -0.13872045], [0.0, 0.0, 0.34116379]],
                [[0.0, 0.0, -0.23500146], [0.0, 0.0, 2.1945855]],
            ],
            [
                [0.0, 0.0, 0.042474599, 0.99909755],
                [0.0, 0.0, 0.88462116, 0.46631039],
            ],
            [0.43128252, 0.90988863, 0.0],
        ),
        // PMX d6a7aecdcb6fce3e9450412295a3a5438ce1c62d1f3a1915e0a82189e871d50f
        (
            "limited-n24",
            24,
            0.5,
            [-0.11982713, 1.7151622, 0.0],
            [
                [[0.0, 0.0, -0.13872045], [0.0, 0.0, 0.34116379]],
                [[0.0, 0.0, -0.23500146], [0.0, 0.0, 2.1945855]],
            ],
            [
                [0.0, 0.0, 0.042474599, 0.99909755],
                [0.0, 0.0, 0.88462116, 0.46631039],
            ],
            [0.43128252, 0.90988863, 0.0],
        ),
        // PMX 67df0e42c3e93468d10f06d8b6fa66c8d723bf17b3efaff66ea8cd0dd25619cb
        (
            "limited-n25",
            25,
            0.5,
            [-0.11982713, 1.7151622, 0.0],
            [
                [[0.0, 0.0, -0.13872045], [0.0, 0.0, 0.34116379]],
                [[0.0, 0.0, -0.23500146], [0.0, 0.0, 2.1945855]],
            ],
            [
                [0.0, 0.0, 0.042474599, 0.99909755],
                [0.0, 0.0, 0.88462116, 0.46631039],
            ],
            [0.43128252, 0.90988863, 0.0],
        ),
        // PMX 380051c9e4cc627106526426e3f7b8a3ac0d6a01e9448502eb9614c7f6c6d64b
        (
            "limited-n26",
            26,
            0.5,
            [-0.11982713, 1.7151622, 0.0],
            [
                [[0.0, 0.0, -0.13872045], [0.0, 0.0, 0.34116379]],
                [[0.0, 0.0, -0.23500146], [0.0, 0.0, 2.1945855]],
            ],
            [
                [0.0, 0.0, 0.042474599, 0.99909755],
                [0.0, 0.0, 0.88462116, 0.46631039],
            ],
            [0.43128252, 0.90988863, 0.0],
        ),
        // PMX 16ed6010ecd211190d20d1c199ebf2c09d118a2af39ca84eb34bc53aae2f328a
        (
            "limited-n32",
            32,
            0.5,
            [-0.11982713, 1.7151622, 0.0],
            [
                [[0.0, 0.0, -0.13872045], [0.0, 0.0, 0.34116379]],
                [[0.0, 0.0, -0.23500146], [0.0, 0.0, 2.1945855]],
            ],
            [
                [0.0, 0.0, 0.042474599, 0.99909755],
                [0.0, 0.0, 0.88462116, 0.46631039],
            ],
            [0.43128252, 0.90988863, 0.0],
        ),
        // PMX 7923a30e6bfdf1d173e4dd7d72c713e468230bd7793055a567294c3e26fe6c71
        (
            "rollback-n1",
            1,
            16.0,
            [-1.8436406, 1.1624483, 0.0],
            [
                [[0.0, 0.0, -0.84857164], [0.0, 0.0, -0.83914857]],
                [[0.0, 0.0, -0.07899159], [0.0, 0.0, 0.88134842]],
            ],
            [
                [0.0, 0.0, -0.40737172, 0.91326245],
                [0.0, 0.0, 0.021098363, 0.99977741],
            ],
            [1.6672064, -0.70188725, 0.0],
        ),
        // PMX d5228ba21a4300fdc38ae9112bb490811e86bc60d20af127baa39efcdf9095e1
        (
            "rollback-n16",
            16,
            16.0,
            [-1.8436406, 1.1624483, 0.0],
            [
                [[0.0, 0.0, -0.84857164], [0.0, 0.0, -0.83914857]],
                [[0.0, 0.0, -0.07899159], [0.0, 0.0, 0.88134842]],
            ],
            [
                [0.0, 0.0, -0.40737172, 0.91326245],
                [0.0, 0.0, -0.44311469, 0.89646496],
            ],
            [1.2753955, -1.5385482, 0.0],
        ),
        // PMX d0fb111a6655267b5b042c38430f5e93e89be0f5ecc6ddd60e798dffb4b34019
        (
            "rollback-n2",
            2,
            16.0,
            [-1.8436406, 1.1624483, 0.0],
            [
                [[0.0, 0.0, -0.84857164], [0.0, 0.0, -0.83914857]],
                [[0.0, 0.0, -0.07899159], [0.0, 0.0, 0.88134842]],
            ],
            [
                [0.0, 0.0, -0.40737172, 0.91326245],
                [0.0, 0.0, -0.44311469, 0.89646496],
            ],
            [1.2753955, -1.5385482, 0.0],
        ),
        // PMX 582d6632b685c93af4d82a4ff05f9b204f419be21623e1cfb8ee558298d5b1a1
        (
            "rollback-n4",
            4,
            16.0,
            [-1.8436406, 1.1624483, 0.0],
            [
                [[0.0, 0.0, -0.84857164], [0.0, 0.0, -0.83914857]],
                [[0.0, 0.0, -0.07899159], [0.0, 0.0, 0.88134842]],
            ],
            [
                [0.0, 0.0, -0.40737172, 0.91326245],
                [0.0, 0.0, -0.44311469, 0.89646496],
            ],
            [1.2753955, -1.5385482, 0.0],
        ),
        // PMX 14eadfa63e88413b837dbe002f3c81b9af74596402b1f722b7e2b74c2dc81a75
        (
            "limits-20261022-multi-x",
            12,
            1.0,
            [0.5, 1.5, 0.7],
            [
                [[-0.4, -0.7, -0.8], [0.5, 0.8, 0.9]],
                [[-0.4, -0.7, -0.8], [0.5, 0.8, 0.9]],
            ],
            [
                [-0.0058293014, -0.15284907, 0.33866415, 0.92839098],
                [0.0073847481, -0.33153214, 0.70750855, 0.62407023],
            ],
            [0.50292397, 1.508781, 0.70410728],
        ),
        // PMX f2688e390c90369960ac535b7cd87df9ec8feea1c56fe2950723508ba609ac3d
        (
            "limits-20261022-multi-y",
            12,
            1.0,
            [0.5, 1.5, 0.7],
            [
                [[-2.0, -0.7, -0.8], [2.0, 0.8, 0.9]],
                [[-2.0, -0.7, -0.8], [2.0, 0.8, 0.9]],
            ],
            [
                [-0.020551308, -0.15127112, 0.35539074, 0.92216709],
                [-0.018166348, -0.37404438, 0.67477234, 0.63595841],
            ],
            [0.51117528, 1.5335212, 0.71562403],
        ),
        // PMX 3cdcf2a7aba70d69a718cbb50555354769edfacce259a1f9b9fb5965e20d8624
        (
            "limits-20261022-multi-z",
            12,
            1.0,
            [0.5, 1.5, 0.7],
            [
                [[-2.0, -2.0, -0.8], [2.0, 2.0, 0.9]],
                [[-2.0, -2.0, -0.8], [2.0, 2.0, 0.9]],
            ],
            [
                [-0.00035498183, -0.15540926, 0.33368626, 0.92978562],
                [-0.00052965913, -0.3329063, 0.70965708, 0.62093473],
            ],
            [0.50012344, 1.5022779, 0.70143211],
        ),
        // PMX 68e9cb3b406d4124a348d0f5f954f051b1e5e782c3226aecfff928b70734053d
        (
            "limits-20261022-sleeve",
            12,
            1.0,
            [0.5, 1.5, 0.7],
            [
                [[0.0, -0.7853982, 0.0], [0.0, 0.7853982, 2.0943952]],
                [[0.0, -0.7853982, 0.0], [0.0, 0.7853982, 2.0943952]],
            ],
            [
                [-0.059765754, -0.16741665, 0.33085239, 0.92678824],
                [-0.13080151, -0.48621036, 0.61234613, 0.60952668],
            ],
            [0.50228035, 1.5069485, 0.70329714],
        ),
    ];
    for (name, loops, angle, goal, limits, rotations, tip) in cases {
        let goal = Vec3A::from_array(goal);
        let limits = limits
            .map(|v| crate::IkAngleLimit::new(Vec3A::from_array(v[0]), Vec3A::from_array(v[1])));
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(1)), Vec3A::X),
                    BoneInit::new(Some(BoneIndex(2)), Vec3A::X),
                    BoneInit::new(Some(BoneIndex(0)), goal),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(4),
                    target_bone: BoneIndex(3),
                    iteration_count: loops,
                    limit_angle: angle,
                    links: vec![
                        IkLinkInit::new(BoneIndex(2)).with_angle_limit(limits[1]),
                        IkLinkInit::new(BoneIndex(1)).with_angle_limit(limits[0]),
                    ],
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.evaluate_current_pose();
        let mut primitive = IkChainSolver::new(IkChainDefinition {
            parent_slots: vec![None, Some(0), Some(1), Some(2), Some(0)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::ZERO, Vec3A::X, Vec3A::X, goal],
            fixed_axes: vec![None; 5],
            target_slot: 3,
            iteration_count: loops,
            limit_angle: angle,
            links: vec![
                IkChainLinkDefinition {
                    bone_slot: 2,
                    angle_limit: Some(limits[1]),
                },
                IkChainLinkDefinition {
                    bone_slot: 1,
                    angle_limit: Some(limits[0]),
                },
            ],
        });
        primitive.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &[Vec3A::ZERO; 5],
            local_rotations: &[Quat::IDENTITY; 5],
            goal_position: goal,
            tolerance: 1e-4,
            max_iterations_cap: None,
        });
        for matrices in [
            runtime.world_matrices(),
            primitive.world_matrices.as_slice(),
        ] {
            for (i, expected) in rotations.iter().enumerate() {
                let actual = rotation(matrices[i + 1]);
                let expected = Quat::from_array(*expected).normalize();
                let error = (actual - expected)
                    .length()
                    .min((actual + expected).length());
                assert!(
                    error < 0.001,
                    "{name} bone {} quaternion error={error}",
                    i + 1
                );
            }
            let error = (translation(matrices[3]) - Vec3A::from_array(tip))
                .abs()
                .max_element();
            assert!(error < 0.001, "{name} tip error={error}");
        }
        assert_eq!(runtime.ik_runtime_stats()[0].rollback_breaks, 0);
    }
}

// These sparse one-link captures leave the intermediate child matrices stale in
// original MMD. Only the explicitly solved link is an oracle for these cases;
// descendant propagation is deliberately not claimed as matching.
#[test]
fn original_mmd_wide_limits_and_collinear_goals() {
    let cases = [
        // PMX 9f430ef9cce9b06e3ee96311af6784597a563a16e0b94e458e1c35f85ab4c755
        (
            "limits-20261022-wide120",
            [-1.0, 1.7320507764816284, 0.0],
            [
                -0.500000179,
                0.866025329,
                -1.17124372e-08,
                0.0,
                -0.866025329,
                -0.500000179,
                -4.37114025e-08,
                0.0,
                -4.37114025e-08,
                -1.17124372e-08,
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
        ),
        // PMX 9f1e206dcaeeb584692b02074653075590af03a52369beb71d7c86ac3d19d150
        (
            "limits-20261022-wide895",
            [0.017453070729970932, 1.999923825263977, 0.0],
            [
                0.0349002071,
                0.999390781,
                0.0,
                0.0,
                -0.999390781,
                0.0349002071,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
            ],
        ),
        // PMX e0fbb7e536ca621acae1a4b8688916c2349bcc9d3fa04cbf99e7f6edb18e0fa9
        (
            "limits-20261022-opposite",
            [-2.0, 0.0, 0.0],
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        ),
        // PMX 9a786bc73bee78bd7bf4309944c4139cac9d48e74a84d6df37170d327215d264
        (
            "limits-20261022-aligned",
            [3.0, 0.0, 0.0],
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
        ),
    ];
    for (name, goal, expected) in cases {
        let goal = Vec3A::from_array(goal);
        let limits = crate::IkAngleLimit::new(Vec3A::splat(-3.2), Vec3A::splat(3.2));
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
                    BoneInit::new(Some(BoneIndex(1)), Vec3A::X),
                    BoneInit::new(Some(BoneIndex(2)), Vec3A::X),
                    BoneInit::new(Some(BoneIndex(0)), goal),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(4),
                    target_bone: BoneIndex(3),
                    iteration_count: 1,
                    limit_angle: 16.0,
                    links: vec![IkLinkInit::new(BoneIndex(1)).with_angle_limit(limits)],
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.evaluate_current_pose_with_ik_options(crate::IkSolveOptions {
            tolerance: 0.0,
            max_iterations_cap: None,
        });
        let mut primitive = IkChainSolver::new(IkChainDefinition {
            parent_slots: vec![None, Some(0), Some(1), Some(2), Some(0)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::ZERO, Vec3A::X, Vec3A::X, goal],
            fixed_axes: vec![None; 5],
            target_slot: 3,
            iteration_count: 1,
            limit_angle: 16.0,
            links: vec![IkChainLinkDefinition {
                bone_slot: 1,
                angle_limit: Some(limits),
            }],
        });
        let output = primitive.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &[Vec3A::ZERO; 5],
            local_rotations: &[Quat::IDENTITY; 5],
            goal_position: goal,
            tolerance: 0.0,
            max_iterations_cap: None,
        });
        for actual in [runtime.world_matrices()[1], primitive.world_matrices[1]] {
            for (a, b) in actual.to_cols_array().iter().zip(expected) {
                assert!((a - b).abs() < 0.00001, "{name}: {a} != {b}");
            }
        }
        if name.ends_with("aligned") {
            // Directional convergence stops with a one-unit positional error,
            // even when distance tolerance is zero.
            assert_eq!(output.link_steps, 0);
            assert!((output.final_distance - 1.0).abs() < 1e-6);
            let stats = runtime.ik_runtime_stats()[0];
            assert_eq!(stats.link_steps, 0);
            assert!((stats.final_distance_max - 1.0).abs() < 1e-6);
            assert_eq!(stats.max_iteration_exhaustions, 0);
        }
    }
}

#[test]
fn original_mmd_fixed_axis_with_limits_matches_both_drivers() {
    let cases = [
        // PMX dc3d1d5c38ab97ab027d086ef0675bf22f918c3fa4791c4a6baf60021070aabe
        (
            "parity-fixedlimits-reflect",
            4,
            16.0,
            [0.5, -1.5, 0.3],
            [
                [[0.0, 0.0, 0.2], [0.0, 0.0, 1.2]],
                [[0.0, 0.0, 0.2], [0.0, 0.0, 1.2]],
            ],
            [
                [0.0, 0.0, 0.099833415, 0.99500417],
                [0.0, 0.0, 0.19866932, 0.98006659],
            ],
            [1.9011277, 0.58808768, 0.0],
            [0.0, 0.0, 1.0],
            false,
        ),
        // PMX 03626226e7bc943e012df81a3f6781408a783a64c3b2523b667156713037ad55
        (
            "parity-fixedlimits-clamp",
            1,
            16.0,
            [0.5, -1.5, 0.3],
            [
                [[0.0, 0.0, 0.2], [0.0, 0.0, 1.2]],
                [[0.0, 0.0, 0.2], [0.0, 0.0, 1.2]],
            ],
            [
                [0.0, 0.0, 0.099833415, 0.99500417],
                [0.0, 0.0, 0.19866932, 0.98006659],
            ],
            [1.9011277, 0.58808768, 0.0],
            [0.0, 0.0, 1.0],
            false,
        ),
        // PMX 99127e4892d906a09a9cb599c1136b22b18821d23825ad92291e77b6d92c04f0
        (
            "parity-fixedlimits-wide",
            4,
            16.0,
            [-1.0, 1.0, 0.5],
            [
                [[0.0, 0.0, -2.8], [0.0, 0.0, 2.8]],
                [[0.0, 0.0, -2.8], [0.0, 0.0, 2.8]],
            ],
            [
                [0.0, 0.0, 0.70179683, 0.71237716],
                [0.0, 0.0, 0.99997564, -0.006979691],
            ],
            [-0.98494005, 0.98592913, 0.0],
            [0.0, 0.0, 1.0],
            false,
        ),
        // PMX cb25452cbbb6f6ccd5027e174fc16f957905be185cdea2b7f2f37af561da9164
        (
            "parity-fixedlimits-offset",
            4,
            16.0,
            [0.5, 1.5, 0.7],
            [
                [[0.0, 0.0, -0.3], [0.0, 0.0, 0.8]],
                [[0.0, 0.0, -0.3], [0.0, 0.0, 0.8]],
            ],
            [
                [0.0, 0.0, 0.38941834, 0.92106099],
                [0.0, 0.0, 0.71735608, 0.69670671],
            ],
            [0.66750717, 1.7169297, 0.0],
            [0.0, 0.0, 1.0],
            false,
        ),
        // PMX f621c9d8269b2aea47c3a1ab3b630a4dfb1e7f7dc2674044990d29c6cbf65502
        (
            "parity-fixedlimits-conflict",
            4,
            16.0,
            [0.0, 2.0, 0.0],
            [
                [[0.0, 0.0, -1.5707963], [0.0, 0.0, 1.5707963]],
                [[0.0, 0.0, -1.5707963], [0.0, 0.0, 1.5707963]],
            ],
            [
                [0.0, 0.0, 0.51392443, 0.85783546],
                [0.0, 0.0, 0.85783549, 0.51392434],
            ],
            [-2.3841858e-07, 1.7634503, 0.0],
            [0.0, 1.0, 0.0],
            false,
        ),
        // PMX 8b067fec3175a8bb8fc8af4549a0f436895e2f5995bfe4007a5d855558a5fded
        (
            "parity-fixedlimits-base",
            1,
            16.0,
            [0.0, 2.0, 0.0],
            [
                [
                    [-3.1415927, -3.1415927, -3.1415927],
                    [3.1415927, 3.1415927, 3.1415927],
                ],
                [
                    [-3.1415927, -3.1415927, -3.1415927],
                    [3.1415927, 3.1415927, 3.1415927],
                ],
            ],
            [
                [0.70710675, 0.0, 0.0, 0.70710681],
                [0.37174805, 0.60150096, 0.60150092, 0.37174808],
            ],
            [0.55278647, 0.89442724, -6.7527381e-08],
            [0.0, 1.0, 0.0],
            true,
        ),
        // PMX 6f66cd152f3410db5cbbe46d705d684d48a96a355904af9d960910d697589fcf
        (
            "parity-fixedlimits-multi",
            4,
            16.0,
            [1.4, 0.0, 1.4],
            [
                [[-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]],
                [[-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]],
            ],
            [
                [0.0, -0.23498335, 0.0, 0.97199939],
                [0.0, -0.5211491, 0.0, 0.85346564],
            ],
            [1.3463728, 0.0, 1.3463731],
            [0.0, 1.0, 0.0],
            false,
        ),
    ];
    for (name, loops, angle, goal, limits, rotations, tip, fixed, rotated_base) in cases {
        let fixed = Vec3A::from_array(fixed);
        let base = if rotated_base {
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)
        } else {
            Quat::IDENTITY
        };
        let goal = Vec3A::from_array(goal);
        let limits = limits
            .map(|v| crate::IkAngleLimit::new(Vec3A::from_array(v[0]), Vec3A::from_array(v[1])));
        let model = Arc::new(
            ModelArena::new_with_ik(
                vec![
                    BoneInit::new(None, Vec3A::ZERO),
                    {
                        let mut b = BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO);
                        b.fixed_axis = Some(fixed);
                        b
                    },
                    {
                        let mut b = BoneInit::new(Some(BoneIndex(1)), Vec3A::X);
                        b.fixed_axis = Some(fixed);
                        b
                    },
                    BoneInit::new(Some(BoneIndex(2)), Vec3A::X),
                    BoneInit::new(Some(BoneIndex(0)), goal),
                ],
                vec![IkSolverInit {
                    ik_bone: BoneIndex(4),
                    target_bone: BoneIndex(3),
                    iteration_count: loops,
                    limit_angle: angle,
                    links: vec![
                        IkLinkInit::new(BoneIndex(2)).with_angle_limit(limits[1]),
                        IkLinkInit::new(BoneIndex(1)).with_angle_limit(limits[0]),
                    ],
                }],
            )
            .unwrap(),
        );
        let mut runtime = RuntimeInstance::new(model);
        runtime.pose_mut().set_local_rotation(BoneIndex(1), base);
        runtime.evaluate_current_pose();
        let mut primitive = IkChainSolver::new(IkChainDefinition {
            parent_slots: vec![None, Some(0), Some(1), Some(2), Some(0)],
            rest_positions: vec![Vec3A::ZERO, Vec3A::ZERO, Vec3A::X, Vec3A::X, goal],
            fixed_axes: vec![None, Some(fixed), Some(fixed), None, None],
            target_slot: 3,
            iteration_count: loops,
            limit_angle: angle,
            links: vec![
                IkChainLinkDefinition {
                    bone_slot: 2,
                    angle_limit: Some(limits[1]),
                },
                IkChainLinkDefinition {
                    bone_slot: 1,
                    angle_limit: Some(limits[0]),
                },
            ],
        });
        primitive.solve(IkChainPoseInput {
            parent_world_matrix: None,
            local_position_offsets: &[Vec3A::ZERO; 5],
            local_rotations: &[
                Quat::IDENTITY,
                base,
                Quat::IDENTITY,
                Quat::IDENTITY,
                Quat::IDENTITY,
            ],
            goal_position: goal,
            tolerance: 1e-4,
            max_iterations_cap: None,
        });
        for matrices in [
            runtime.world_matrices(),
            primitive.world_matrices.as_slice(),
        ] {
            for (i, expected) in rotations.iter().enumerate() {
                let actual = rotation(matrices[i + 1]);
                let expected = Quat::from_array(*expected).normalize();
                let error = (actual - expected)
                    .length()
                    .min((actual + expected).length());
                assert!(
                    error < 0.00002,
                    "{name} bone {} quaternion error={error}",
                    i + 1
                );
            }
            let error = (translation(matrices[3]) - Vec3A::from_array(tip))
                .abs()
                .max_element();
            assert!(error < 0.00002, "{name} tip error={error}");
        }
        assert_eq!(runtime.ik_runtime_stats()[0].rollback_breaks, 0);
    }
}

#[test]
fn original_mmd_local_axes_do_not_change_ik_limit_frames() {
    let cases = [
        // control PMX 1bd8ff701f29335f6aa2110f0c96e6ad6771da3301a94e00970b9cee7f1cff5b
        // rotated PMX 384cd011c5192073e4e4d9d259e31f8479c9f2dd1b788bd6705f1f0b9ab51589
        (
            "knee",
            [0.0, -1.0, 0.0],
            [0.2, -1.0, 1.2],
            [[-2.9, 0.0, 0.0], [-0.01, 0.0, 0.0]],
            false,
            [
                [-0.10578972, 0.0, 0.0, 0.99438853],
                [-0.69332368, 0.0, 0.0, 0.72062632],
            ],
            [0.0, -1.0162218, 1.2096467],
        ),
        // control PMX febd073388bb6fd7c96fe52faaa9e69febda9df99a36f7caa343bee37f75406b
        // rotated PMX 198e035831ceb47ce5ba21e97c33599dde04326e81f82efef23252de958594f0
        (
            "multi",
            [1.0, 0.0, 0.0],
            [0.5, 1.5, 0.7],
            [[-0.7, -0.4, -0.5], [0.8, 0.6, 1.2]],
            false,
            [
                [6.988317e-08, -0.15530309, 0.33279217, 0.93012381],
                [-1.4384275e-07, -0.33163981, 0.71065665, 0.62046923],
            ],
            [0.50022477, 1.5009571, 0.70044661],
        ),
        // control PMX 714e2e08e867cc9e2d370551b18a99cb7df7e9f9a4ac1f90c8600c96c0303b3d
        // rotated PMX 80161f1f9e22df600b0026eee3209ce7e7712fb93cc58a10760622eed610f27a
        (
            "fixed",
            [1.0, 0.0, 0.0],
            [0.5, 1.5, 0.7],
            [[-0.7, -0.4, -0.5], [0.8, 0.6, 1.2]],
            true,
            [
                [0.0, 0.0, 0.31885659, 0.94780298],
                [0.0, 0.0, 0.7983335, 0.60221556],
            ],
            [0.52198809, 1.5659641, 0.0],
        ),
    ];
    for (name, segment, goal, limits, fixed, rotations, tip) in cases {
        let segment = Vec3A::from_array(segment);
        let goal = Vec3A::from_array(goal);
        let limits =
            crate::IkAngleLimit::new(Vec3A::from_array(limits[0]), Vec3A::from_array(limits[1]));
        for with_axes in [false, true] {
            let mut bones = vec![
                BoneInit::new(None, Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(0)), Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(1)), segment),
                BoneInit::new(Some(BoneIndex(2)), segment),
                BoneInit::new(Some(BoneIndex(0)), goal),
            ];
            if fixed {
                bones[1].fixed_axis = Some(Vec3A::Z);
                bones[2].fixed_axis = Some(Vec3A::Z);
            }
            let model = ModelArena::new_with_ik(
                bones,
                vec![IkSolverInit {
                    ik_bone: BoneIndex(4),
                    target_bone: BoneIndex(3),
                    iteration_count: 4,
                    limit_angle: 1.0,
                    links: vec![
                        IkLinkInit::new(BoneIndex(2)).with_angle_limit(limits),
                        IkLinkInit::new(BoneIndex(1)).with_angle_limit(limits),
                    ],
                }],
            )
            .unwrap();
            let axis = with_axes.then_some(crate::LocalAxis::new(-Vec3A::Z, Vec3A::X));
            let model = model.with_local_axes([None, axis, axis, None, None]);
            assert_eq!(model.local_axis(BoneIndex(1)), axis);
            let mut runtime = RuntimeInstance::new(Arc::new(model));
            runtime.evaluate_current_pose();
            for (i, expected) in rotations.iter().enumerate() {
                let actual = rotation(runtime.world_matrices()[i + 1]);
                let expected = Quat::from_array(*expected).normalize();
                let error = (actual - expected)
                    .length()
                    .min((actual + expected).length());
                assert!(
                    error < 0.0001,
                    "{name} axes={with_axes} bone={} error={error}",
                    i + 1
                );
            }
            let error = (translation(runtime.world_matrices()[3]) - Vec3A::from_array(tip))
                .abs()
                .max_element();
            assert!(error < 0.0001, "{name} axes={with_axes} tip error={error}");
        }
    }
}
