use std::sync::Arc;

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{
    BoneIndex, IkAngleLimit, LocalAxis, MorphIndex, RuntimeAppendTransformDescriptorV1,
    RuntimeBoneDescriptorV1, RuntimeBoneMorphOffsetDescriptorV1,
    RuntimeGroupMorphOffsetDescriptorV1, RuntimeIkLinkDescriptorV1, RuntimeIkSolverDescriptorV1,
    RuntimeInstance, RuntimeModelDescriptorErrorKind, RuntimeModelDescriptorV1,
    RuntimeMorphDescriptorV1, compile_runtime_model_descriptor_v1,
};

fn valid_descriptor() -> RuntimeModelDescriptorV1 {
    let mut root = RuntimeBoneDescriptorV1::new(None, Vec3A::new(1.0, 2.0, 3.0));
    root.transform_order = 3;
    root.fixed_axis = Some(Vec3A::Y);
    root.local_axis = Some(LocalAxis::new(Vec3A::X, Vec3A::Z));
    let mut child = RuntimeBoneDescriptorV1::new(Some(BoneIndex(0)), Vec3A::new(1.0, 4.0, 3.0));
    child.transform_after_physics = true;
    child.transform_order = 5;

    let mut solver = RuntimeIkSolverDescriptorV1::new(
        BoneIndex(1),
        BoneIndex(0),
        vec![RuntimeIkLinkDescriptorV1 {
            bone: BoneIndex(0),
            angle_limit: Some(IkAngleLimit::new(Vec3A::splat(-1.0), Vec3A::splat(1.0))),
        }],
    );
    solver.iteration_count = 4;
    solver.limit_angle = 0.5;

    let mut append = RuntimeAppendTransformDescriptorV1::new(BoneIndex(1), BoneIndex(0), 0.25);
    append.affect_rotation = true;
    append.affect_translation = true;
    append.local = true;

    RuntimeModelDescriptorV1 {
        bones: vec![root, child],
        ik_solvers: vec![solver],
        append_transforms: vec![append],
        morphs: RuntimeMorphDescriptorV1 {
            morph_count: 2,
            bone_offsets: vec![RuntimeBoneMorphOffsetDescriptorV1 {
                morph_index: MorphIndex(1),
                target_bone: BoneIndex(1),
                position_offset: Vec3A::new(0.1, 0.2, 0.3),
                // Host descriptors may use a scaled but non-degenerate
                // quaternion; the compiler stores its normalized form.
                rotation_offset: Quat::from_xyzw(0.0, 0.0, 0.0, 2.0),
            }],
            group_offsets: vec![RuntimeGroupMorphOffsetDescriptorV1 {
                morph_index: MorphIndex(0),
                child_morph: MorphIndex(1),
                ratio: 0.75,
            }],
        },
        ..RuntimeModelDescriptorV1::default()
    }
}

#[test]
fn compiles_absolute_rest_positions_and_metadata() {
    let model = compile_runtime_model_descriptor_v1(&valid_descriptor()).unwrap();
    assert_eq!(model.bone_count(), 2);
    assert_eq!(model.rest_position(BoneIndex(0)), Vec3A::new(1.0, 2.0, 3.0));
    assert_eq!(model.rest_position(BoneIndex(1)), Vec3A::new(0.0, 2.0, 0.0));
    assert_eq!(
        model.inverse_bind_matrix(BoneIndex(0)),
        glam::Mat4::from_translation(Vec3A::new(-1.0, -2.0, -3.0).into())
    );
    assert_eq!(
        model.inverse_bind_matrix(BoneIndex(1)),
        glam::Mat4::from_translation(Vec3A::new(-1.0, -4.0, -3.0).into())
    );
    assert_eq!(model.transform_order(BoneIndex(0)), 3);
    assert!(model.transform_after_physics(BoneIndex(1)));
    assert_eq!(model.fixed_axis(BoneIndex(0)), Some(Vec3A::Y));
    assert_eq!(model.local_axis_count(), 1);
    assert_eq!(model.ik_count(), 1);
    assert_eq!(model.ik_solvers()[0].links[0].bone, BoneIndex(0));
    assert_eq!(model.append_transforms()[0].target_bone, BoneIndex(1));
    assert_eq!(model.append_transforms()[0].source_bone, BoneIndex(0));
    assert!(model.append_transforms()[0].local);
    assert_eq!(model.morph_count(), 2);
    assert_eq!(model.bone_morph_spans()[1].count, 1);
    assert_eq!(model.group_morph_spans()[0].count, 1);
    assert_eq!(model.group_morph_offsets()[0].child_morph, MorphIndex(1));
    assert_eq!(
        model.bone_morph_offsets()[0].rotation_offset,
        Quat::IDENTITY
    );
}

#[test]
fn rest_evaluation_produces_world_and_identity_skinning_matrices() {
    let model = Arc::new(compile_runtime_model_descriptor_v1(&valid_descriptor()).unwrap());
    let mut runtime = RuntimeInstance::new(model);
    runtime.evaluate_rest_pose();
    let root_world = Vec3A::from_array(runtime.world_matrices()[0].w_axis.truncate().to_array());
    let child_world = Vec3A::from_array(runtime.world_matrices()[1].w_axis.truncate().to_array());
    assert!((root_world - Vec3A::new(1.0, 2.0, 3.0)).length() < 1.0e-5);
    assert!((child_world - Vec3A::new(1.0, 4.0, 3.0)).length() < 1.0e-5);
    let root_skin = runtime.skinning_matrices()[0];
    let child_skin = runtime.skinning_matrices()[1];
    assert!(
        root_skin.abs_diff_eq(glam::Mat4::IDENTITY, 1.0e-5),
        "{root_skin:?}"
    );
    assert!(
        child_skin.abs_diff_eq(glam::Mat4::IDENTITY, 1.0e-5),
        "{child_skin:?}"
    );
}

fn assert_path_error(
    descriptor: RuntimeModelDescriptorV1,
    path: &str,
    kind: RuntimeModelDescriptorErrorKind,
) {
    let error = compile_runtime_model_descriptor_v1(&descriptor).unwrap_err();
    assert_eq!(error.path, path);
    assert_eq!(error.kind, kind);
    assert!(error.to_string().contains(path));
}

#[test]
fn rejects_version_and_empty_bones() {
    let descriptor = RuntimeModelDescriptorV1 {
        descriptor_version: 99,
        ..RuntimeModelDescriptorV1::default()
    };
    assert_path_error(
        descriptor,
        "descriptor_version",
        RuntimeModelDescriptorErrorKind::UnsupportedVersion {
            expected: 1,
            actual: 99,
        },
    );
    assert_path_error(
        RuntimeModelDescriptorV1::default(),
        "bones",
        RuntimeModelDescriptorErrorKind::EmptyBones,
    );
}

#[test]
fn rejects_parent_and_float_validation_categories() {
    let mut descriptor = RuntimeModelDescriptorV1::new(vec![RuntimeBoneDescriptorV1::new(
        Some(BoneIndex(9)),
        Vec3A::ZERO,
    )]);
    assert_path_error(
        descriptor.clone(),
        "bones[0].parent",
        RuntimeModelDescriptorErrorKind::IndexOutOfRange {
            value: 9,
            length: 1,
        },
    );
    descriptor.bones[0].parent = Some(BoneIndex(0));
    assert_path_error(
        descriptor.clone(),
        "bones[0].parent",
        RuntimeModelDescriptorErrorKind::SelfParent,
    );
    descriptor.bones[0].parent = None;
    descriptor
        .bones
        .push(RuntimeBoneDescriptorV1::new(Some(BoneIndex(2)), Vec3A::ONE));
    descriptor.bones.push(RuntimeBoneDescriptorV1::new(
        Some(BoneIndex(1)),
        Vec3A::splat(2.0),
    ));
    assert_path_error(
        descriptor.clone(),
        "bones[1].parent",
        RuntimeModelDescriptorErrorKind::ParentCycle,
    );
    descriptor.bones[1].parent = None;
    descriptor.bones[0].rest_position.x = f32::NAN;
    assert_path_error(
        descriptor,
        "bones[0].rest_position",
        RuntimeModelDescriptorErrorKind::NonFinite,
    );
}

#[test]
fn rejects_non_finite_derived_translation_and_unstable_normalization() {
    let descriptor = RuntimeModelDescriptorV1::new(vec![
        RuntimeBoneDescriptorV1::new(None, Vec3A::splat(f32::MAX)),
        RuntimeBoneDescriptorV1::new(Some(BoneIndex(0)), Vec3A::splat(-f32::MAX)),
    ]);
    assert_path_error(
        descriptor,
        "bones[1].rest_position",
        RuntimeModelDescriptorErrorKind::NonFinite,
    );

    let mut descriptor =
        RuntimeModelDescriptorV1::new(vec![RuntimeBoneDescriptorV1::new(None, Vec3A::ZERO)]);
    descriptor.bones[0].fixed_axis = Some(Vec3A::splat(f32::MAX));
    assert_path_error(
        descriptor,
        "bones[0].fixed_axis",
        RuntimeModelDescriptorErrorKind::DegenerateAxis,
    );

    let mut descriptor =
        RuntimeModelDescriptorV1::new(vec![RuntimeBoneDescriptorV1::new(None, Vec3A::ZERO)]);
    descriptor.morphs = RuntimeMorphDescriptorV1 {
        morph_count: 1,
        bone_offsets: vec![RuntimeBoneMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(0),
            target_bone: BoneIndex(0),
            position_offset: Vec3A::ZERO,
            rotation_offset: Quat::from_xyzw(f32::MAX, 0.0, 0.0, 1.0),
        }],
        ..RuntimeMorphDescriptorV1::default()
    };
    assert_path_error(
        descriptor,
        "morphs.bone_offsets[0].rotation_offset",
        RuntimeModelDescriptorErrorKind::DegenerateQuaternion,
    );
}

#[test]
fn rejects_axis_ik_append_and_morph_categories() {
    let mut descriptor =
        RuntimeModelDescriptorV1::new(vec![RuntimeBoneDescriptorV1::new(None, Vec3A::ZERO)]);
    descriptor.bones[0].fixed_axis = Some(Vec3A::ZERO);
    assert_path_error(
        descriptor.clone(),
        "bones[0].fixed_axis",
        RuntimeModelDescriptorErrorKind::DegenerateAxis,
    );
    descriptor.bones[0].fixed_axis = None;
    descriptor.bones[0].local_axis = Some(LocalAxis::new(Vec3A::X, Vec3A::X));
    assert_path_error(
        descriptor.clone(),
        "bones[0].local_axis",
        RuntimeModelDescriptorErrorKind::DegenerateAxis,
    );
    descriptor.bones[0].local_axis = None;
    descriptor.ik_solvers.push(RuntimeIkSolverDescriptorV1 {
        ik_bone: BoneIndex(0),
        target_bone: BoneIndex(0),
        links: vec![RuntimeIkLinkDescriptorV1 {
            bone: BoneIndex(0),
            angle_limit: None,
        }],
        iteration_count: 0,
        limit_angle: 0.0,
    });
    assert_path_error(
        descriptor.clone(),
        "ik_solvers[0].iteration_count",
        RuntimeModelDescriptorErrorKind::InvalidIterationCount,
    );
    descriptor.ik_solvers[0].iteration_count = 1;
    descriptor.ik_solvers[0].links[0].bone = BoneIndex(3);
    assert_path_error(
        descriptor.clone(),
        "ik_solvers[0].links[0].bone",
        RuntimeModelDescriptorErrorKind::IndexOutOfRange {
            value: 3,
            length: 1,
        },
    );
    descriptor.ik_solvers[0].links[0].bone = BoneIndex(0);
    descriptor
        .append_transforms
        .push(RuntimeAppendTransformDescriptorV1 {
            target_bone: BoneIndex(0),
            source_bone: BoneIndex(2),
            ratio: f32::INFINITY,
            affect_rotation: true,
            affect_translation: false,
            local: false,
        });
    assert_path_error(
        descriptor.clone(),
        "append_transforms[0].source_bone",
        RuntimeModelDescriptorErrorKind::IndexOutOfRange {
            value: 2,
            length: 1,
        },
    );
    descriptor.append_transforms[0].source_bone = BoneIndex(0);
    assert_path_error(
        descriptor.clone(),
        "append_transforms[0].ratio",
        RuntimeModelDescriptorErrorKind::InvalidAppendRatio,
    );
    descriptor.append_transforms[0].ratio = 1.0;
    descriptor.append_transforms[0].affect_rotation = false;
    assert_path_error(
        descriptor.clone(),
        "append_transforms[0]",
        RuntimeModelDescriptorErrorKind::InvalidAppendFlags,
    );
    descriptor.append_transforms[0].affect_rotation = true;
    descriptor
        .append_transforms
        .push(descriptor.append_transforms[0].clone());
    assert_path_error(
        descriptor.clone(),
        "append_transforms[1].target_bone",
        RuntimeModelDescriptorErrorKind::DuplicateAppendTarget,
    );
    descriptor.append_transforms.clear();
    descriptor.morphs.morph_count = 1;
    descriptor
        .morphs
        .bone_offsets
        .push(RuntimeBoneMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(0),
            target_bone: BoneIndex(0),
            position_offset: Vec3A::ZERO,
            rotation_offset: Quat::from_xyzw(0.0, 0.0, 0.0, 0.0),
        });
    assert_path_error(
        descriptor.clone(),
        "morphs.bone_offsets[0].rotation_offset",
        RuntimeModelDescriptorErrorKind::DegenerateQuaternion,
    );
    descriptor.morphs.bone_offsets[0].rotation_offset = Quat::IDENTITY;
    descriptor
        .morphs
        .group_offsets
        .push(RuntimeGroupMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(0),
            child_morph: MorphIndex(0),
            ratio: 1.0,
        });
    assert_path_error(
        descriptor,
        "morphs.group_offsets[0].child_morph",
        RuntimeModelDescriptorErrorKind::GroupMorphCycle,
    );
}

#[test]
fn deep_parent_and_group_chains_are_stack_safe() {
    const DEPTH: usize = 20_000;
    let bones = (0..DEPTH)
        .map(|index| {
            RuntimeBoneDescriptorV1::new(
                if index > 0 {
                    Some(BoneIndex((index - 1) as u32))
                } else {
                    None
                },
                Vec3A::new(index as f32, 0.0, 0.0),
            )
        })
        .collect();
    let group_offsets = (0..(DEPTH - 1))
        .map(|index| RuntimeGroupMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(index as u32),
            child_morph: MorphIndex((index + 1) as u32),
            ratio: 1.0,
        })
        .collect();
    let descriptor = RuntimeModelDescriptorV1 {
        bones,
        morphs: RuntimeMorphDescriptorV1 {
            morph_count: DEPTH as u32,
            group_offsets,
            ..RuntimeMorphDescriptorV1::default()
        },
        ..RuntimeModelDescriptorV1::default()
    };
    let model = compile_runtime_model_descriptor_v1(&descriptor).unwrap();
    assert_eq!(model.bone_count(), DEPTH);
    assert_eq!(model.morph_count(), DEPTH as u32);
}

#[test]
fn group_cycle_reports_original_source_index_after_bucketing() {
    let descriptor = RuntimeModelDescriptorV1 {
        bones: vec![RuntimeBoneDescriptorV1::new(None, Vec3A::ZERO)],
        morphs: RuntimeMorphDescriptorV1 {
            morph_count: 2,
            group_offsets: vec![
                RuntimeGroupMorphOffsetDescriptorV1 {
                    morph_index: MorphIndex(1),
                    child_morph: MorphIndex(0),
                    ratio: 1.0,
                },
                RuntimeGroupMorphOffsetDescriptorV1 {
                    morph_index: MorphIndex(0),
                    child_morph: MorphIndex(1),
                    ratio: 1.0,
                },
            ],
            ..RuntimeMorphDescriptorV1::default()
        },
        ..RuntimeModelDescriptorV1::default()
    };
    let error = compile_runtime_model_descriptor_v1(&descriptor).unwrap_err();
    assert_eq!(error.path, "morphs.group_offsets[0].child_morph");
    assert_eq!(error.kind, RuntimeModelDescriptorErrorKind::GroupMorphCycle);
}

#[test]
fn late_parent_and_group_cycles_are_indexed_without_stack_overflow() {
    const DEPTH: usize = 20_000;
    let mut bones: Vec<_> = (0..DEPTH)
        .map(|index| {
            RuntimeBoneDescriptorV1::new(
                if index > 0 {
                    Some(BoneIndex((index - 1) as u32))
                } else {
                    None
                },
                Vec3A::new(index as f32, 0.0, 0.0),
            )
        })
        .collect();
    bones[DEPTH - 2].parent = Some(BoneIndex((DEPTH - 1) as u32));
    bones[DEPTH - 1].parent = Some(BoneIndex((DEPTH - 2) as u32));
    let mut descriptor = RuntimeModelDescriptorV1 {
        bones,
        ..RuntimeModelDescriptorV1::default()
    };
    let error = compile_runtime_model_descriptor_v1(&descriptor).unwrap_err();
    assert_eq!(error.path, format!("bones[{}].parent", DEPTH - 2));
    assert_eq!(error.kind, RuntimeModelDescriptorErrorKind::ParentCycle);

    descriptor.bones[DEPTH - 2].parent = Some(BoneIndex((DEPTH - 3) as u32));
    descriptor.bones[DEPTH - 1].parent = Some(BoneIndex((DEPTH - 2) as u32));
    descriptor.morphs = RuntimeMorphDescriptorV1 {
        morph_count: DEPTH as u32,
        group_offsets: (0..(DEPTH - 1))
            .map(|index| RuntimeGroupMorphOffsetDescriptorV1 {
                morph_index: MorphIndex(index as u32),
                child_morph: MorphIndex((index + 1) as u32),
                ratio: 1.0,
            })
            .chain(std::iter::once(RuntimeGroupMorphOffsetDescriptorV1 {
                morph_index: MorphIndex((DEPTH - 1) as u32),
                child_morph: MorphIndex((DEPTH - 2) as u32),
                ratio: 1.0,
            }))
            .collect(),
        ..RuntimeMorphDescriptorV1::default()
    };
    let error = compile_runtime_model_descriptor_v1(&descriptor).unwrap_err();
    assert_eq!(
        error.path,
        format!("morphs.group_offsets[{}].child_morph", DEPTH - 1)
    );
    assert_eq!(error.kind, RuntimeModelDescriptorErrorKind::GroupMorphCycle);
}

#[test]
fn rejects_remaining_descriptor_validation_categories() {
    let base = || {
        RuntimeModelDescriptorV1::new(vec![
            RuntimeBoneDescriptorV1::new(None, Vec3A::ZERO),
            RuntimeBoneDescriptorV1::new(Some(BoneIndex(0)), Vec3A::ONE),
        ])
    };

    let mut descriptor = base();
    descriptor.morphs.bone_offsets = vec![RuntimeBoneMorphOffsetDescriptorV1 {
        morph_index: MorphIndex(0),
        target_bone: BoneIndex(0),
        position_offset: Vec3A::ZERO,
        rotation_offset: Quat::IDENTITY,
    }];
    assert_path_error(
        descriptor,
        "morphs.morph_count",
        RuntimeModelDescriptorErrorKind::EmptyMorphSet,
    );

    for (field, solver) in [
        (
            "ik_solvers[0].ik_bone",
            RuntimeIkSolverDescriptorV1::new(BoneIndex(2), BoneIndex(0), vec![]),
        ),
        (
            "ik_solvers[0].target_bone",
            RuntimeIkSolverDescriptorV1::new(BoneIndex(0), BoneIndex(2), vec![]),
        ),
    ] {
        let mut descriptor = base();
        descriptor.ik_solvers.push(solver);
        assert_path_error(
            descriptor,
            field,
            RuntimeModelDescriptorErrorKind::IndexOutOfRange {
                value: 2,
                length: 2,
            },
        );
    }

    let mut descriptor = base();
    let mut solver = RuntimeIkSolverDescriptorV1::new(BoneIndex(0), BoneIndex(1), vec![]);
    solver.limit_angle = f32::NAN;
    descriptor.ik_solvers.push(solver);
    assert_path_error(
        descriptor,
        "ik_solvers[0].limit_angle",
        RuntimeModelDescriptorErrorKind::InvalidLimitAngle,
    );

    for (limit, field, kind) in [
        (
            IkAngleLimit::new(Vec3A::ONE, Vec3A::ZERO),
            "ik_solvers[0].links[0].angle_limit",
            RuntimeModelDescriptorErrorKind::InvalidRange,
        ),
        (
            IkAngleLimit::new(Vec3A::new(f32::NAN, 0.0, 0.0), Vec3A::ONE),
            "ik_solvers[0].links[0].angle_limit.min",
            RuntimeModelDescriptorErrorKind::NonFinite,
        ),
    ] {
        let mut descriptor = base();
        let mut solver = RuntimeIkSolverDescriptorV1::new(
            BoneIndex(0),
            BoneIndex(1),
            vec![RuntimeIkLinkDescriptorV1::new(BoneIndex(0))],
        );
        solver.links[0].angle_limit = Some(limit);
        descriptor.ik_solvers.push(solver);
        assert_path_error(descriptor, field, kind);
    }

    let mut descriptor = base();
    descriptor.morphs = RuntimeMorphDescriptorV1 {
        morph_count: 1,
        bone_offsets: vec![RuntimeBoneMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(1),
            target_bone: BoneIndex(0),
            position_offset: Vec3A::ZERO,
            rotation_offset: Quat::IDENTITY,
        }],
        ..RuntimeMorphDescriptorV1::default()
    };
    assert_path_error(
        descriptor,
        "morphs.bone_offsets[0].morph_index",
        RuntimeModelDescriptorErrorKind::IndexOutOfRange {
            value: 1,
            length: 1,
        },
    );

    for (position_offset, rotation_offset, field) in [
        (
            Vec3A::new(f32::NAN, 0.0, 0.0),
            Quat::IDENTITY,
            "position_offset",
        ),
        (
            Vec3A::ZERO,
            Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0),
            "rotation_offset",
        ),
    ] {
        let mut descriptor = base();
        descriptor.morphs = RuntimeMorphDescriptorV1 {
            morph_count: 1,
            bone_offsets: vec![RuntimeBoneMorphOffsetDescriptorV1 {
                morph_index: MorphIndex(0),
                target_bone: BoneIndex(0),
                position_offset,
                rotation_offset,
            }],
            ..RuntimeMorphDescriptorV1::default()
        };
        assert_path_error(
            descriptor,
            &format!("morphs.bone_offsets[0].{field}"),
            RuntimeModelDescriptorErrorKind::NonFinite,
        );
    }

    let mut descriptor = base();
    descriptor.morphs = RuntimeMorphDescriptorV1 {
        morph_count: 1,
        bone_offsets: vec![RuntimeBoneMorphOffsetDescriptorV1 {
            morph_index: MorphIndex(0),
            target_bone: BoneIndex(2),
            position_offset: Vec3A::ZERO,
            rotation_offset: Quat::IDENTITY,
        }],
        ..RuntimeMorphDescriptorV1::default()
    };
    assert_path_error(
        descriptor,
        "morphs.bone_offsets[0].target_bone",
        RuntimeModelDescriptorErrorKind::IndexOutOfRange {
            value: 2,
            length: 2,
        },
    );

    for (child_morph, ratio, field, kind) in [
        (
            MorphIndex(1),
            1.0,
            "morphs.group_offsets[0].child_morph",
            RuntimeModelDescriptorErrorKind::IndexOutOfRange {
                value: 1,
                length: 1,
            },
        ),
        (
            MorphIndex(0),
            f32::NAN,
            "morphs.group_offsets[0].ratio",
            RuntimeModelDescriptorErrorKind::NonFinite,
        ),
    ] {
        let mut descriptor = base();
        descriptor.morphs = RuntimeMorphDescriptorV1 {
            morph_count: 1,
            group_offsets: vec![RuntimeGroupMorphOffsetDescriptorV1 {
                morph_index: MorphIndex(0),
                child_morph,
                ratio,
            }],
            ..RuntimeMorphDescriptorV1::default()
        };
        assert_path_error(descriptor, field, kind);
    }
}

#[test]
fn iterative_graph_validation_accepts_shared_parent_and_diamond_morph_dag() {
    let descriptor = RuntimeModelDescriptorV1 {
        bones: vec![
            RuntimeBoneDescriptorV1::new(None, Vec3A::ZERO),
            RuntimeBoneDescriptorV1::new(Some(BoneIndex(0)), Vec3A::X),
            RuntimeBoneDescriptorV1::new(Some(BoneIndex(0)), Vec3A::Y),
        ],
        morphs: RuntimeMorphDescriptorV1 {
            morph_count: 5,
            group_offsets: vec![
                RuntimeGroupMorphOffsetDescriptorV1 {
                    morph_index: MorphIndex(0),
                    child_morph: MorphIndex(1),
                    ratio: 1.0,
                },
                RuntimeGroupMorphOffsetDescriptorV1 {
                    morph_index: MorphIndex(0),
                    child_morph: MorphIndex(2),
                    ratio: 1.0,
                },
                RuntimeGroupMorphOffsetDescriptorV1 {
                    morph_index: MorphIndex(1),
                    child_morph: MorphIndex(3),
                    ratio: 1.0,
                },
                RuntimeGroupMorphOffsetDescriptorV1 {
                    morph_index: MorphIndex(2),
                    child_morph: MorphIndex(3),
                    ratio: 1.0,
                },
                RuntimeGroupMorphOffsetDescriptorV1 {
                    morph_index: MorphIndex(3),
                    child_morph: MorphIndex(4),
                    ratio: 1.0,
                },
            ],
            ..RuntimeMorphDescriptorV1::default()
        },
        ..RuntimeModelDescriptorV1::default()
    };

    let model = compile_runtime_model_descriptor_v1(&descriptor).unwrap();
    let mut eval_order = model
        .eval_order()
        .iter()
        .map(|bone| bone.0)
        .collect::<Vec<_>>();
    eval_order.sort_unstable();
    assert_eq!(eval_order, vec![0, 1, 2]);
    assert_eq!(model.group_morph_offsets().len(), 5);
}
