use std::sync::Arc;
use std::time::Duration;

use fbxcel::{low::v7400::AttributeValue, tree::any::AnyTree};
use glam::Vec3;
use mmd_anim_runtime::{
    DensePoseSequenceView, ReductionTolerances, SkeletonSnapshot, reduce_dense_pose_sequence,
};

use super::*;

#[test]
fn reduced_export_equality_ignores_nondeterministic_timings() {
    let first = FbxReducedPoseExport {
        bytes: vec![1, 2, 3],
        report: PoseReductionReport::default(),
        work_stats: ReductionWorkStats::default(),
        timings: ReductionTimings {
            local_prefit: Duration::from_millis(3),
            candidate_build: Duration::from_millis(1),
            error_measure: Duration::from_millis(2),
            dcc_fit: Duration::from_millis(1),
            ancestor_prune: Duration::from_millis(4),
        },
    };
    let mut second = first.clone();
    second.timings = ReductionTimings {
        local_prefit: Duration::from_secs(3),
        candidate_build: Duration::from_secs(1),
        error_measure: Duration::from_secs(2),
        dcc_fit: Duration::from_secs(1),
        ancestor_prune: Duration::from_secs(4),
    };
    assert_eq!(first, second);
}

fn fixture_model() -> PmxParsedModel {
    crate::parse_pmx_model(include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx"))
        .expect("PMX fixture should parse")
}

fn runtime_bake_fixture() -> (Arc<ModelArena>, AnimationClip, u32) {
    let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
    let vmd_data = include_bytes!("../../fixtures/vmd/ik_multi_bone_nondefault.vmd");
    let runtime_import =
        crate::import_pmx_runtime(pmx_data).expect("PMX runtime fixture should import");
    let runtime_motion =
        crate::import_vmd_motion(vmd_data).expect("VMD runtime fixture should import");
    let parsed_motion = crate::parse_vmd_animation(vmd_data).expect("VMD fixture should parse");
    let clip = crate::build_mmd_registered_pair_clip(
        &runtime_import.model,
        &runtime_motion,
        &runtime_import.bone_name_to_index,
        &runtime_import.morph_name_to_index,
        &runtime_import.ik_solver_bone_name_to_index,
        runtime_import.model.ik_count(),
    )
    .expect("MMD-registered VMD clip should build");
    let last_frame = parsed_motion
        .bone_frames
        .iter()
        .map(|frame| frame.frame)
        .max()
        .unwrap_or(0);
    (Arc::new(runtime_import.model), clip, last_frame)
}

fn runtime_baked_fixture_fbx() -> (PmxParsedModel, Vec<u8>) {
    let model = fixture_model();
    let (runtime_model, clip, last_frame) = runtime_bake_fixture();
    let fbx = export_fbx_with_runtime_bake(
        &model,
        runtime_model,
        &clip,
        last_frame,
        &FbxExportOptions::default(),
    )
    .expect("runtime-baked FBX should export");
    (model, fbx)
}

fn reduced_fbx_fixture() -> (PmxParsedModel, ReducedPoseSequence, usize) {
    let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
    let model = crate::parse_pmx_model(pmx_data).unwrap();
    let runtime = crate::import_pmx_runtime(pmx_data).unwrap().model;
    let snapshot = SkeletonSnapshot::from_model(&runtime, 123).unwrap();
    let root = snapshot
        .parent_indices()
        .iter()
        .position(|parent| *parent < 0)
        .unwrap();
    let frame_count = 9;
    let mut world = Vec::with_capacity(frame_count * snapshot.bone_count());
    for frame in 0..frame_count {
        let t = frame as f32 / (frame_count - 1) as f32;
        let peak = 4.0 * t * (1.0 - t);
        let mut frame_world = vec![Mat4::IDENTITY; snapshot.bone_count()];
        let mut resolved = vec![false; snapshot.bone_count()];
        fn resolve(
            bone: usize,
            root: usize,
            peak: f32,
            snapshot: &SkeletonSnapshot,
            world: &mut [Mat4],
            resolved: &mut [bool],
        ) {
            if resolved[bone] {
                return;
            }
            let parent = snapshot.parent_indices()[bone];
            if parent >= 0 {
                resolve(parent as usize, root, peak, snapshot, world, resolved);
            }
            let mut translation = snapshot.rest_local_translations()[bone];
            if bone == root {
                translation.x += peak;
            }
            let local = Mat4::from_rotation_translation(
                snapshot.rest_local_rotations()[bone],
                Vec3::from(translation),
            );
            world[bone] = if parent < 0 {
                local
            } else {
                world[parent as usize] * local
            };
            resolved[bone] = true;
        }
        for bone in 0..snapshot.bone_count() {
            resolve(bone, root, peak, &snapshot, &mut frame_world, &mut resolved);
        }
        world.extend(frame_world);
    }
    let morphs = vec![0.0; frame_count * snapshot.morph_count()];
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(
            &world,
            &morphs,
            frame_count,
            snapshot.bone_count(),
            snapshot.morph_count(),
            0.0,
            1.0,
        )
        .unwrap(),
        snapshot,
        ReductionTolerances {
            local_position: 0.01,
            world_position: 0.01,
            ..Default::default()
        },
        ReductionTarget::DccCubic,
    )
    .unwrap();
    (model, reduced, root)
}

fn reduced_fbx_rotation_fixture() -> (ReducedPoseSequence, usize) {
    let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
    let runtime = crate::import_pmx_runtime(pmx_data).unwrap().model;
    let snapshot = SkeletonSnapshot::from_model(&runtime, 123).unwrap();
    let root = snapshot
        .parent_indices()
        .iter()
        .position(|parent| *parent < 0)
        .unwrap();
    let frame_count = 5;
    let mut world = Vec::with_capacity(frame_count * snapshot.bone_count());
    for frame in 0..frame_count {
        let mut frame_world = vec![Mat4::IDENTITY; snapshot.bone_count()];
        let mut resolved = vec![false; snapshot.bone_count()];
        fn resolve(
            bone: usize,
            root: usize,
            frame: usize,
            snapshot: &SkeletonSnapshot,
            world: &mut [Mat4],
            resolved: &mut [bool],
        ) {
            if resolved[bone] {
                return;
            }
            let parent = snapshot.parent_indices()[bone];
            if parent >= 0 {
                resolve(parent as usize, root, frame, snapshot, world, resolved);
            }
            let mut rotation = snapshot.rest_local_rotations()[bone];
            if bone == root {
                rotation = glam::Quat::from_rotation_x(frame as f32 * 0.25);
            }
            let local = Mat4::from_rotation_translation(
                rotation,
                Vec3::from(snapshot.rest_local_translations()[bone]),
            );
            world[bone] = if parent < 0 {
                local
            } else {
                world[parent as usize] * local
            };
            resolved[bone] = true;
        }
        for bone in 0..snapshot.bone_count() {
            resolve(
                bone,
                root,
                frame,
                &snapshot,
                &mut frame_world,
                &mut resolved,
            );
        }
        world.extend(frame_world);
    }
    let morphs = vec![0.0; frame_count * snapshot.morph_count()];
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(
            &world,
            &morphs,
            frame_count,
            snapshot.bone_count(),
            snapshot.morph_count(),
            0.0,
            1.0,
        )
        .unwrap(),
        snapshot,
        ReductionTolerances {
            local_position: 0.01,
            world_position: 0.01,
            local_rotation_radians: 1.0e-6,
            world_rotation_radians: 1.0e-6,
            ..Default::default()
        },
        ReductionTarget::DccCubic,
    )
    .unwrap();
    (reduced, root)
}

#[test]
fn reduced_pose_writes_sparse_per_key_user_tangents_and_replays_samples() {
    let (model, reduced, root) = reduced_fbx_fixture();
    let fbx = export_pmx_fbx_binary_with_reduced_pose(
        &model,
        &reduced,
        123,
        &FbxExportOptions {
            flip_z: false,
            ..Default::default()
        },
    )
    .unwrap();
    let tree = load_tree(&fbx);
    let root_node = tree.root();
    let objects = root_node.first_child_by_name("Objects").unwrap();
    let curve = objects
        .children_by_name("AnimationCurve")
        .find(|node| {
            node.attributes().first().and_then(AttributeValue::get_i64)
                == Some(animation_curve_id(root, 3))
        })
        .unwrap();
    let times = child_arr_i64(curve, "KeyTime");
    let values = child_arr_f32(curve, "KeyValueFloat");
    let flags = child_arr_i32(curve, "KeyAttrFlags");
    let data = child_arr_f32(curve, "KeyAttrDataFloat");
    let refs = child_arr_i32(curve, "KeyAttrRefCount");
    assert_eq!(times.len(), 3);
    assert_eq!(times.len(), values.len());
    assert_eq!(flags.len(), values.len());
    assert_eq!(data.len(), values.len() * 4);
    assert_eq!(refs, vec![1; values.len()]);
    assert!(flags[..flags.len() - 1].iter().all(|flag| *flag == 0x408));
    assert_eq!(flags[flags.len() - 1], 0x404);

    for frame in 0..9 {
        let time = frame as f32 / 30.0;
        let upper =
            times.partition_point(|key| (*key as f64 / FBX_TIME_ONE_SECOND as f64) <= time as f64);
        let actual = if upper == 0 {
            values[0]
        } else if upper == values.len() {
            values[values.len() - 1]
        } else {
            let left = upper - 1;
            let right = upper;
            let left_time = times[left] as f32 / FBX_TIME_ONE_SECOND as f32;
            let right_time = times[right] as f32 / FBX_TIME_ONE_SECOND as f32;
            let amount = (time - left_time) / (right_time - left_time);
            let duration = right_time - left_time;
            let t2 = amount * amount;
            let t3 = t2 * amount;
            (2.0 * t3 - 3.0 * t2 + 1.0) * values[left]
                + (t3 - 2.0 * t2 + amount) * duration * data[left * 4]
                + (-2.0 * t3 + 3.0 * t2) * values[right]
                + (t3 - t2) * duration * data[left * 4 + 1]
        };
        let t = frame as f32 / 8.0;
        let expected = reduced.snapshot().rest_local_translations()[root].x + 4.0 * t * (1.0 - t);
        assert!(
            (actual - expected).abs() <= 0.01,
            "{frame}: {actual} {expected}"
        );
    }
}

#[test]
fn reduced_pose_default_flip_z_uses_raw_baseline_gate() {
    let (model, reduced, _) = reduced_fbx_fixture();
    let bytes = export_pmx_fbx_binary_with_reduced_pose(
        &model,
        &reduced,
        123,
        &FbxExportOptions::default(),
    )
    .unwrap();
    assert!(!bytes.is_empty());
}

#[test]
fn dcc_channel_plan_samples_actual_mid_segment_and_reports_omissions() {
    let (_model, reduced, root) = reduced_fbx_fixture();
    let plan = build_dcc_channel_plans(
        &reduced,
        &FbxExportOptions {
            flip_z: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(plan[root].translation_channels[0].is_some());
    assert!(plan[root].translation_channels[1].is_none());
    assert!(plan[root].rotation_channels.iter().all(Option::is_none));
    let raw = build_raw_full_dcc_plans(
        &reduced,
        &plan,
        &FbxExportOptions {
            flip_z: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(dcc_plans_validate(&plan, &raw, &reduced,));
    let channel = plan[root].translation_channels[0].as_ref().unwrap();
    let frame = 4.0;
    let actual = sample_dcc_channel(
        &reduced,
        &plan[root].translation_channels[0],
        frame,
        plan[root].translation_defaults[0],
    );
    let expected = reduced.sample(frame).unwrap().local_translations[root].x;
    assert!((actual - expected).abs() <= 1.0e-4);
    assert_eq!(
        channel.values.len(),
        reduced.bone_tracks()[root].keys().len()
    );
    assert_eq!(
        canonical_dcc_scalar(-0.0, DCC_TRANSLATION_EPSILON).to_bits(),
        0
    );
    let flipped = build_dcc_channel_plans(&reduced, &FbxExportOptions::default()).unwrap();
    assert!(flipped[root].translation_channels[0].is_some());
}

#[test]
fn dcc_rotation_values_preserve_segment_euler_branch() {
    let (reduced, root) = reduced_fbx_rotation_fixture();
    let track = &reduced.bone_tracks()[root];
    assert!(track.keys().len() >= 2);
    let no_flip = dcc_rotation_values(
        track,
        &FbxExportOptions {
            flip_z: false,
            ..Default::default()
        },
        [1.0; 3],
    );
    let first = track.keys()[1]
        .dcc_segment
        .rotation_start_euler_xyz
        .to_array();
    for axis in 0..3 {
        assert!((no_flip[axis][0] - first[axis].to_degrees()).abs() <= 1.0e-5);
    }
    let flipped = dcc_rotation_values(track, &FbxExportOptions::default(), [-1.0, -1.0, 1.0]);
    for axis in 0..3 {
        assert!(
            (flipped[axis][0] - first[axis].to_degrees() * [-1.0, -1.0, 1.0][axis]).abs() <= 1.0e-5
        );
    }
    for (index, key) in track.keys().iter().enumerate().skip(1) {
        let end = key.dcc_segment.rotation_end_euler_xyz.to_array();
        for axis in 0..3 {
            assert!((no_flip[axis][index] - end[axis].to_degrees()).abs() <= 1.0e-5);
        }
    }
}

#[test]
fn dcc_channel_constant_nondefault_is_one_key() {
    let channel = dcc_channel(
        &[0, 4, 8],
        &[0, FBX_FRAME_DURATION * 4, FBX_FRAME_DURATION * 8],
        &[2.0, 2.0, 2.0],
        &[
            SegmentTangents {
                out_tangent: 0.0,
                next_in_tangent: 0.0,
            },
            SegmentTangents {
                out_tangent: 0.0,
                next_in_tangent: 0.0,
            },
            SegmentTangents {
                out_tangent: 0.0,
                next_in_tangent: 0.0,
            },
        ],
        0.0,
        DCC_TRANSLATION_EPSILON,
    )
    .unwrap();
    assert_eq!(channel.sample_indices, vec![0]);
    assert_eq!(channel.values, vec![2.0]);
    assert_eq!(channel.tangents.len(), 1);
}

#[test]
fn raw_full_channel_preserves_sub_epsilon_values_for_fallback() {
    let (_model, reduced, root) = reduced_fbx_fixture();
    let values = [-0.0_f32, 1.0e-7, -1.0e-7];
    let channel = full_dcc_channel(
        &reduced,
        root,
        0,
        false,
        &values,
        &FbxExportOptions {
            flip_z: false,
            ..Default::default()
        },
        0.0,
    )
    .unwrap();
    assert_eq!(channel.values[0].to_bits(), 0);
    assert_eq!(&channel.values[1..], &values[1..]);
    assert_eq!(canonical_dcc_scalar(-0.0, 0.0).to_bits(), 0);
}

#[test]
fn fbx_euler_xyz_round_trips_non_commuting_rotation() {
    let source = glam::Quat::from_xyzw(0.21, -0.37, 0.18, 0.88).normalize();
    let euler = quat_to_euler_xyz([
        source.x as f64,
        source.y as f64,
        source.z as f64,
        source.w as f64,
    ]);
    let reconstructed = euler_xyz_to_quat(euler.map(|value| value as f32));
    assert!(tight_quat_angle_radians(source, reconstructed) <= 1.0e-6);
}

#[test]
fn fbx_euler_xyz_round_trips_exact_gimbal_boundaries() {
    for beta in [std::f64::consts::FRAC_PI_2, -std::f64::consts::FRAC_PI_2] {
        let source = glam::Quat::from_euler(glam::EulerRot::XYZ, 0.73, beta as f32, -0.41);
        let euler = quat_to_euler_xyz([
            source.x as f64,
            source.y as f64,
            source.z as f64,
            source.w as f64,
        ]);
        let reconstructed = euler_xyz_to_quat(euler.map(|value| value as f32));
        let error = tight_quat_angle_radians(source, reconstructed);
        assert!(error <= 2.0e-5, "beta={beta} euler={euler:?} error={error}");
    }
}

#[test]
fn fbx_euler_xyz_round_trips_near_gimbal_rotation() {
    for beta in [
        std::f32::consts::FRAC_PI_2 - 1.0e-4,
        -std::f32::consts::FRAC_PI_2 + 1.0e-4,
    ] {
        let source = glam::Quat::from_euler(glam::EulerRot::XYZ, 0.73, beta, -0.41);
        let euler = quat_to_euler_xyz([
            source.x as f64,
            source.y as f64,
            source.z as f64,
            source.w as f64,
        ]);
        let reconstructed = euler_xyz_to_quat(euler.map(|value| value as f32));
        let error = tight_quat_angle_radians(source, reconstructed);
        assert!(error <= 2.0e-4, "beta={beta} euler={euler:?} error={error}");
    }
}

#[test]
fn unity_dto_uses_actual_planned_channel_count() {
    let (_model, reduced, root) = reduced_fbx_fixture();
    let dto = reduced_pose_to_unity_animation_clip(
        &reduced,
        &UnityReducedPoseBindings {
            model_identity: 123,
            bone_paths: (0..reduced.snapshot().bone_count())
                .map(|bone| format!("bone{bone}"))
                .collect(),
            morph_bindings: vec![None; reduced.snapshot().morph_count()],
        },
        false,
    )
    .unwrap();
    let root_curves = dto
        .curves
        .iter()
        .filter(|curve| curve.path == format!("bone{root}"))
        .count();
    assert!(root_curves < 6);
    let plan = build_dcc_channel_plans(&reduced, &FbxExportOptions::default()).unwrap();
    let expected_curve_count = plan
        .iter()
        .map(|plan| {
            plan.translation_channels
                .iter()
                .chain(plan.rotation_channels.iter())
                .filter(|channel| channel.is_some())
                .count()
        })
        .sum::<usize>();
    assert_eq!(dto.curves.len(), expected_curve_count);
    assert_eq!(
        dto.reduced_key_count,
        reduced.report().reduced_bone_key_count
    );
}

#[test]
fn unity_direct_clip_dto_keeps_sparse_times_and_tangents_separate_from_fbx_import() {
    let (_model, reduced, root) = reduced_fbx_fixture();
    let mut bone_paths = (0..reduced.snapshot().bone_count())
        .map(|bone| format!("root/bone{bone}"))
        .collect::<Vec<_>>();
    bone_paths[root] = "root/moving".to_owned();
    let dto = reduced_pose_to_unity_animation_clip(
        &reduced,
        &UnityReducedPoseBindings {
            model_identity: 123,
            bone_paths,
            morph_bindings: vec![None; reduced.snapshot().morph_count()],
        },
        false,
    )
    .unwrap();
    let curve = dto
        .curves
        .iter()
        .find(|curve| curve.path == "root/moving" && curve.property == "localPosition.x")
        .unwrap();
    assert_eq!(curve.keys.len(), 3);
    assert_eq!(curve.keys[0].time_seconds, 0.0);
    assert!((curve.keys[2].time_seconds - 8.0 / 30.0).abs() <= f32::EPSILON);
    assert!(curve.keys.iter().all(|key| {
        key.in_tangent.is_finite() && key.out_tangent.is_finite() && key.value.is_finite()
    }));
    assert!(dto.reduced_key_count < dto.source_key_count);

    let sixty_fps = reduced_pose_to_unity_animation_clip_with_fps(
        &reduced,
        &UnityReducedPoseBindings {
            model_identity: 123,
            bone_paths: (0..reduced.snapshot().bone_count())
                .map(|bone| format!("root/bone{bone}"))
                .collect(),
            morph_bindings: vec![None; reduced.snapshot().morph_count()],
        },
        60.0,
        false,
    )
    .unwrap();
    let sixty_curve = &sixty_fps.curves[root * 6];
    assert_eq!(sixty_fps.frame_rate, 60.0);
    assert_eq!(sixty_curve.keys[2].time_seconds, 8.0 / 60.0);
    assert_eq!(
        sixty_curve.keys[0].out_tangent,
        curve.keys[0].out_tangent * 2.0
    );
    assert!(matches!(
        reduced_pose_to_unity_animation_clip_with_fps(
            &reduced,
            &UnityReducedPoseBindings {
                model_identity: 123,
                bone_paths: vec![String::new(); reduced.snapshot().bone_count()],
                morph_bindings: vec![None; reduced.snapshot().morph_count()],
            },
            f32::NAN,
            false,
        ),
        Err(FbxExportError::InvalidFramesPerSecond)
    ));
    for frames_per_second in [f32::MAX, f32::from_bits(1)] {
        assert!(matches!(
            reduced_pose_to_unity_animation_clip_with_fps(
                &reduced,
                &UnityReducedPoseBindings {
                    model_identity: 123,
                    bone_paths: vec![String::new(); reduced.snapshot().bone_count()],
                    morph_bindings: vec![None; reduced.snapshot().morph_count()],
                },
                frames_per_second,
                false,
            ),
            Err(FbxExportError::NonFiniteUnityKey { .. })
        ));
    }
}

#[test]
fn pose_source_seam_preserves_non_physics_runtime_bake_bytes() {
    struct RecordingPoseSource<'a> {
        runtime: RuntimeInstance,
        clip: &'a AnimationClip,
        frames: Vec<u32>,
        matrix_buffer_address: Option<usize>,
    }

    impl FbxPoseSource for RecordingPoseSource<'_> {
        fn world_matrices(&mut self, frame: u32) -> Result<&[Mat4], String> {
            self.frames.push(frame);
            self.runtime.evaluate_clip_frame(self.clip, frame as f32);
            let matrices = self.runtime.world_matrices();
            let address = matrices.as_ptr() as usize;
            assert_eq!(*self.matrix_buffer_address.get_or_insert(address), address);
            Ok(matrices)
        }
    }

    let pmx_data = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
    let vmd_data = include_bytes!("../../fixtures/vmd/ik_multi_bone_nondefault.vmd");
    let model = crate::parse_pmx_model(pmx_data).unwrap();
    let runtime_import = crate::import_pmx_runtime(pmx_data).unwrap();
    let runtime_motion = crate::import_vmd_motion(vmd_data).unwrap();
    let clip = crate::build_mmd_registered_pair_clip(
        &runtime_import.model,
        &runtime_motion,
        &runtime_import.bone_name_to_index,
        &runtime_import.morph_name_to_index,
        &runtime_import.ik_solver_bone_name_to_index,
        runtime_import.model.ik_count(),
    )
    .expect("MMD-registered VMD clip should build");
    let runtime_model = Arc::new(runtime_import.model);
    let expected = export_fbx_with_runtime_bake(
        &model,
        Arc::clone(&runtime_model),
        &clip,
        2,
        &FbxExportOptions::default(),
    )
    .unwrap();
    let mut pose_source = RecordingPoseSource {
        runtime: RuntimeInstance::new(Arc::clone(&runtime_model)),
        clip: &clip,
        frames: Vec::new(),
        matrix_buffer_address: None,
    };
    let actual = export_pmx_fbx_binary_with_pose_source(
        &model,
        runtime_model,
        &clip,
        2,
        &FbxExportOptions::default(),
        &mut pose_source,
    )
    .unwrap();

    assert_eq!(pose_source.frames, vec![0, 1, 2]);
    assert_eq!(actual, expected);
}

#[test]
fn pose_source_validates_parent_matrix_outside_exported_bone_subset() {
    use mmd_anim_runtime::BoneInit;

    struct ShortPoseSource {
        matrices: Vec<Mat4>,
    }

    impl FbxPoseSource for ShortPoseSource {
        fn world_matrices(&mut self, _frame: u32) -> Result<&[Mat4], String> {
            Ok(&self.matrices)
        }
    }

    let mut model = fixture_model();
    model.skeleton.bones.truncate(1);
    let runtime_model = Arc::new(
        ModelArena::new(vec![
            BoneInit::new(Some(BoneIndex(1)), glam::Vec3A::ZERO),
            BoneInit::new(None, glam::Vec3A::ZERO),
        ])
        .unwrap(),
    );
    let clip = AnimationClip::new(Vec::new());
    let mut source = ShortPoseSource {
        matrices: vec![Mat4::IDENTITY],
    };
    let options = FbxExportOptions {
        bones_only: true,
        ..FbxExportOptions::default()
    };

    let error = export_pmx_fbx_binary_with_pose_source(
        &model,
        runtime_model,
        &clip,
        0,
        &options,
        &mut source,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        FbxExportError::PoseBoneCount {
            frame: 0,
            expected: 2,
            actual: 1,
        }
    ));
}

fn load_tree(bytes: &[u8]) -> fbxcel::tree::v7400::Tree {
    match AnyTree::from_seekable_reader(Cursor::new(bytes)).expect("FBX should parse") {
        AnyTree::V7400(version, tree, footer) => {
            assert_eq!(version, FbxVersion::V7_4);
            assert_eq!(
                footer.expect("FBX footer should parse").fbx_version,
                FbxVersion::V7_4
            );
            tree
        }
        _ => panic!("FBX should be parsed as a v7400 tree"),
    }
}

fn child_i32(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> i32 {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_i32)
        .unwrap_or_else(|| panic!("missing i32 child node {name}"))
}

fn child_string<'a>(node: fbxcel::tree::v7400::NodeHandle<'a>, name: &str) -> &'a str {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_string)
        .unwrap_or_else(|| panic!("missing string child node {name}"))
}

fn child_arr_f32(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<f32> {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_arr_f32)
        .map(|values| values.to_vec())
        .unwrap_or_else(|| panic!("missing f32 array child node {name}"))
}

fn child_arr_i32(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<i32> {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_arr_i32)
        .map(|values| values.to_vec())
        .unwrap_or_else(|| panic!("missing i32 array child node {name}"))
}

fn child_arr_i64(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<i64> {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_arr_i64)
        .map(|values| values.to_vec())
        .unwrap_or_else(|| panic!("missing i64 array child node {name}"))
}

fn child_arr_f64(node: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> Vec<f64> {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_arr_f64)
        .map(|values| values.to_vec())
        .unwrap_or_else(|| panic!("missing f64 array child node {name}"))
}

fn object_type_count(definitions: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> i32 {
    definitions
        .children_by_name("ObjectType")
        .find(|node| {
            node.attributes()
                .first()
                .and_then(AttributeValue::get_string)
                == Some(name)
        })
        .map(|node| child_i32(node, "Count"))
        .unwrap_or_else(|| panic!("missing ObjectType {name}"))
}

fn optional_object_type_count(definitions: fbxcel::tree::v7400::NodeHandle<'_>, name: &str) -> i32 {
    definitions
        .children_by_name("ObjectType")
        .find(|node| {
            node.attributes()
                .first()
                .and_then(AttributeValue::get_string)
                == Some(name)
        })
        .map(|node| child_i32(node, "Count"))
        .unwrap_or(0)
}

fn descendants_by_name<'a>(
    tree: &'a fbxcel::tree::v7400::Tree,
    name: &str,
) -> Vec<fbxcel::tree::v7400::NodeHandle<'a>> {
    let mut traversal = tree.root().node_id().traverse_depth_first();
    let mut nodes = Vec::new();
    while let Some(node_id) = traversal.next_open_forward(tree) {
        let node = node_id.to_handle(tree);
        if node.name() == name {
            nodes.push(node);
        }
    }
    nodes
}

#[test]
fn runtime_bake_export_writes_parseable_fbx_structure() {
    let (model, fbx) = runtime_baked_fixture_fbx();
    let tree = load_tree(&fbx);
    let root = tree.root();
    let definitions = root
        .first_child_by_name("Definitions")
        .expect("Definitions node should exist");
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    let connections = root
        .first_child_by_name("Connections")
        .expect("Connections node should exist");

    assert_eq!(objects.children_by_name("Geometry").count(), 1);
    assert_eq!(objects.children_by_name("Pose").count(), 1);
    assert_eq!(
        objects.children_by_name("Model").count(),
        model.skeleton.bones.len() + 1
    );
    assert_eq!(
        objects.children_by_name("NodeAttribute").count(),
        model.skeleton.bones.len()
    );
    assert_eq!(
        objects.children_by_name("Deformer").count(),
        model.skeleton.bones.len() + 1
    );

    let animation_curves = objects.children_by_name("AnimationCurve").count();
    let animation_curve_nodes = objects.children_by_name("AnimationCurveNode").count();
    let morph_animation_curve_nodes = objects
        .children_by_name("AnimationCurveNode")
        .filter(|node| {
            node.attributes()
                .get(1)
                .and_then(AttributeValue::get_string)
                .is_some_and(|name| name.contains("DeformPercent"))
        })
        .count();
    let bone_animation_curve_nodes =
        animation_curve_nodes.saturating_sub(morph_animation_curve_nodes);
    assert!(animation_curves > 0);
    assert!(animation_curve_nodes > 0);
    assert!(bone_animation_curve_nodes.is_multiple_of(2));
    assert_eq!(
        animation_curves,
        bone_animation_curve_nodes / 2 * 6 + morph_animation_curve_nodes
    );
    assert_eq!(objects.children_by_name("AnimationStack").count(), 1);
    assert_eq!(objects.children_by_name("AnimationLayer").count(), 1);

    assert_eq!(
        child_i32(definitions, "Count") as usize,
        objects.children().count() + 1
    );
    assert_eq!(
        object_type_count(definitions, "Model") as usize,
        model.skeleton.bones.len() + 1
    );
    assert_eq!(
        object_type_count(definitions, "AnimationCurve") as usize,
        animation_curves
    );
    assert_eq!(
        object_type_count(definitions, "AnimationCurveNode") as usize,
        animation_curve_nodes
    );

    let clusters: Vec<_> = objects
        .children_by_name("Deformer")
        .filter(|node| {
            node.attributes()
                .get(2)
                .and_then(AttributeValue::get_string)
                == Some("Cluster")
        })
        .collect();
    assert_eq!(clusters.len(), model.skeleton.bones.len());
    for cluster in clusters {
        let cluster_id = cluster
            .attributes()
            .first()
            .and_then(AttributeValue::get_i64)
            .expect("Cluster should have an object id");
        let bone_index = usize::try_from(cluster_id - CLUSTER_ID_BASE)
            .expect("Cluster id should map to a bone index");
        let bone = &model.skeleton.bones[bone_index];
        assert_eq!(
            child_arr_f64(cluster, "Transform"),
            bone_world_transform_inverse(bone, &FbxExportOptions::default()),
            "Cluster Transform should be the inverse bone bind matrix"
        );
        assert_eq!(
            child_arr_f64(cluster, "TransformLink"),
            bone_world_transform(bone, &FbxExportOptions::default()),
            "Cluster TransformLink should be the bone bind matrix"
        );
    }

    let pose = objects
        .first_child_by_name("Pose")
        .expect("Bind pose should exist");
    assert_eq!(child_string(pose, "Type"), "BindPose");
    assert_eq!(
        child_i32(pose, "NbPoseNodes") as usize,
        model.skeleton.bones.len() + 1
    );
    assert_eq!(
        pose.children_by_name("PoseNode").count(),
        model.skeleton.bones.len() + 1
    );

    let connection_kinds: Vec<_> = connections
        .children_by_name("C")
        .filter_map(|node| {
            node.attributes()
                .first()
                .and_then(AttributeValue::get_string)
        })
        .collect();
    assert!(connection_kinds.contains(&"OO"));
    assert!(connection_kinds.contains(&"OP"));
}

#[test]
fn runtime_bake_export_writes_monotonic_animation_keys() {
    let (_model, fbx) = runtime_baked_fixture_fbx();
    let tree = load_tree(&fbx);
    let curves = descendants_by_name(&tree, "AnimationCurve");
    assert!(!curves.is_empty());

    for curve in curves {
        let key_times = curve
            .first_child_by_name("KeyTime")
            .and_then(|node| node.attributes().first())
            .and_then(AttributeValue::get_arr_i64)
            .expect("AnimationCurve should have KeyTime values");
        let key_values = curve
            .first_child_by_name("KeyValueFloat")
            .and_then(|node| node.attributes().first())
            .and_then(AttributeValue::get_arr_f32)
            .expect("AnimationCurve should have KeyValueFloat values");
        assert_eq!(key_times.len(), key_values.len());
        assert!(key_times.len() >= 2);
        assert!(
            key_times.windows(2).all(|window| window[0] < window[1]),
            "FBX KeyTime values should be strictly monotonic"
        );
    }
}

#[test]
fn bones_only_export_omits_mesh_skin_material_pose_and_morphs() {
    let model = fixture_model();
    let options = FbxExportOptions {
        bones_only: true,
        ..FbxExportOptions::default()
    };
    let fbx = export_fbx(&model, None, &options).expect("bones-only FBX should export");
    let tree = load_tree(&fbx);
    let root = tree.root();
    let definitions = root
        .first_child_by_name("Definitions")
        .expect("Definitions node should exist");
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    let connections = root
        .first_child_by_name("Connections")
        .expect("Connections node should exist");

    assert_eq!(objects.children_by_name("Geometry").count(), 0);
    assert_eq!(objects.children_by_name("Material").count(), 0);
    assert_eq!(objects.children_by_name("Texture").count(), 0);
    assert_eq!(objects.children_by_name("Video").count(), 0);
    assert_eq!(objects.children_by_name("Deformer").count(), 0);
    assert_eq!(objects.children_by_name("Pose").count(), 0);
    assert_eq!(
        objects.children_by_name("Model").count(),
        model.skeleton.bones.len() + 1
    );
    assert_eq!(
        objects.children_by_name("NodeAttribute").count(),
        model.skeleton.bones.len()
    );
    assert_eq!(optional_object_type_count(definitions, "Geometry"), 0);
    assert_eq!(optional_object_type_count(definitions, "Material"), 0);
    assert_eq!(optional_object_type_count(definitions, "Texture"), 0);
    assert_eq!(optional_object_type_count(definitions, "Video"), 0);
    assert_eq!(optional_object_type_count(definitions, "Deformer"), 0);
    assert_eq!(optional_object_type_count(definitions, "Pose"), 0);
    assert_eq!(
        child_i32(definitions, "Count") as usize,
        objects.children().count() + 1
    );

    let root_model = objects
        .children_by_name("Model")
        .find(|node| node.attributes().first().and_then(AttributeValue::get_i64) == Some(MODEL_ID))
        .expect("root model should exist");
    assert_eq!(
        root_model
            .attributes()
            .get(2)
            .and_then(AttributeValue::get_string),
        Some("Null")
    );
    assert!(has_oo_connection(connections, MODEL_ID, ROOT_NODE_ID));
    assert!(!has_oo_connection(connections, GEOMETRY_ID, MODEL_ID));
    assert!(!has_oo_connection(connections, SKIN_ID, GEOMETRY_ID));
    assert!(!has_oo_connection(connections, cluster_id(0), SKIN_ID));
    assert!(has_oo_connection(
        connections,
        bone_attr_id(0),
        bone_model_id(0)
    ));
}

#[test]
fn bones_only_runtime_bake_keeps_bone_animation_without_morph_curves() {
    let model = fixture_model();
    let (runtime_model, clip, last_frame) = runtime_bake_fixture();
    let options = FbxExportOptions {
        bones_only: true,
        ..FbxExportOptions::default()
    };
    let fbx = export_fbx_with_runtime_bake(&model, runtime_model, &clip, last_frame, &options)
        .expect("bones-only runtime-baked FBX should export");
    let tree = load_tree(&fbx);
    let root = tree.root();
    let definitions = root
        .first_child_by_name("Definitions")
        .expect("Definitions node should exist");
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    let connections = root
        .first_child_by_name("Connections")
        .expect("Connections node should exist");

    assert_eq!(objects.children_by_name("AnimationStack").count(), 1);
    assert_eq!(objects.children_by_name("AnimationLayer").count(), 1);
    let animation_curves = objects.children_by_name("AnimationCurve").count();
    let animation_curve_nodes = objects.children_by_name("AnimationCurveNode").count();
    assert!(animation_curves > 0);
    assert!(animation_curve_nodes > 0);
    assert_eq!(animation_curves, animation_curve_nodes / 2 * 6);
    assert_eq!(
        optional_object_type_count(definitions, "AnimationCurve") as usize,
        animation_curves
    );
    assert_eq!(
        objects
            .children_by_name("AnimationCurveNode")
            .filter(|node| {
                node.attributes()
                    .get(1)
                    .and_then(AttributeValue::get_string)
                    .is_some_and(|name| name.contains("DeformPercent"))
            })
            .count(),
        0
    );
    assert!(connections.children_by_name("C").any(|node| {
        let attrs = node.attributes();
        attrs.first().and_then(AttributeValue::get_string) == Some("OP")
            && attrs.get(3).and_then(AttributeValue::get_string) == Some("Lcl Rotation")
    }));
}

#[test]
fn fbx_export_writes_diffuse_texture_references() {
    let mut model = fixture_model();
    model.materials[0].texture_path = "textures/diffuse.png".to_owned();
    let fbx = export_fbx(&model, None, &FbxExportOptions::default()).expect("FBX should export");
    let tree = load_tree(&fbx);
    let root = tree.root();
    let definitions = root
        .first_child_by_name("Definitions")
        .expect("Definitions node should exist");
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    let connections = root
        .first_child_by_name("Connections")
        .expect("Connections node should exist");

    assert_eq!(object_type_count(definitions, "Texture"), 1);
    assert_eq!(object_type_count(definitions, "Video"), 1);

    let texture = objects
        .children_by_name("Texture")
        .next()
        .expect("Texture object should exist");
    let video = objects
        .children_by_name("Video")
        .next()
        .expect("Video object should exist");
    assert_eq!(child_string(texture, "FileName"), "textures/diffuse.png");
    assert_eq!(
        child_string(texture, "RelativeFilename"),
        "textures/diffuse.png"
    );
    assert_eq!(child_string(video, "FileName"), "textures/diffuse.png");
    assert_eq!(
        child_string(video, "RelativeFilename"),
        "textures/diffuse.png"
    );

    let has_texture_to_material = connections.children_by_name("C").any(|node| {
        let attrs = node.attributes();
        attrs.first().and_then(AttributeValue::get_string) == Some("OP")
            && attrs.get(1).and_then(AttributeValue::get_i64) == Some(TEXTURE_ID_BASE)
            && attrs.get(2).and_then(AttributeValue::get_i64) == Some(MATERIAL_ID_BASE)
            && attrs.get(3).and_then(AttributeValue::get_string) == Some("DiffuseColor")
    });
    let has_video_to_texture = connections.children_by_name("C").any(|node| {
        let attrs = node.attributes();
        attrs.first().and_then(AttributeValue::get_string) == Some("OO")
            && attrs.get(1).and_then(AttributeValue::get_i64) == Some(VIDEO_ID_BASE)
            && attrs.get(2).and_then(AttributeValue::get_i64) == Some(TEXTURE_ID_BASE)
    });
    assert!(has_texture_to_material);
    assert!(has_video_to_texture);
}

fn has_oo_connection(
    connections: fbxcel::tree::v7400::NodeHandle<'_>,
    child_id: i64,
    parent_id: i64,
) -> bool {
    connections.children_by_name("C").any(|node| {
        let attrs = node.attributes();
        attrs.first().and_then(AttributeValue::get_string) == Some("OO")
            && attrs.get(1).and_then(AttributeValue::get_i64) == Some(child_id)
            && attrs.get(2).and_then(AttributeValue::get_i64) == Some(parent_id)
    })
}

fn deformer_type(node: fbxcel::tree::v7400::NodeHandle<'_>) -> Option<&str> {
    node.attributes()
        .get(2)
        .and_then(AttributeValue::get_string)
}

fn vertex_morph_test_model() -> PmxParsedModel {
    use crate::pmx::PmxParsedVertexMorphOffset;

    let mut model = fixture_model();
    model.geometry.positions = vec![
        0.0, 0.0, 0.0, //
        1.0, 0.0, 0.0, //
        0.0, 1.0, 0.0,
    ];
    model.geometry.normals = vec![0.0; 9];
    model.geometry.uvs = vec![0.0; 6];
    model.geometry.indices = vec![0, 1, 2];
    model.geometry.skin_indices = vec![0; 12];
    model.geometry.skin_weights = vec![1.0; 12];
    model.metadata.counts.vertices = 3;
    model.metadata.counts.faces = 1;
    model.morphs = vec![PmxParsedMorph {
        name: "Smile".to_owned(),
        english_name: "smile".to_owned(),
        panel: "mouth".to_owned(),
        kind: "vertex".to_owned(),
        vertex_offsets: vec![PmxParsedVertexMorphOffset {
            vertex_index: 1,
            position: [0.0, 0.5, 0.25],
        }],
        group_offsets: Vec::new(),
        bone_offsets: Vec::new(),
        uv_offsets: Vec::new(),
        additional_uv_offsets: Vec::new(),
        material_offsets: Vec::new(),
        flip_offsets: Vec::new(),
        impulse_offsets: Vec::new(),
    }];
    model.metadata.counts.morphs = model.morphs.len();
    model
}

#[test]
fn fbx_export_writes_vertex_morph_blendshape() {
    let mut model = vertex_morph_test_model();
    model.morphs.push(PmxParsedMorph {
        name: "BoneMorph".to_owned(),
        english_name: "bone".to_owned(),
        panel: "other".to_owned(),
        kind: "bone".to_owned(),
        vertex_offsets: Vec::new(),
        group_offsets: Vec::new(),
        bone_offsets: Vec::new(),
        uv_offsets: Vec::new(),
        additional_uv_offsets: Vec::new(),
        material_offsets: Vec::new(),
        flip_offsets: Vec::new(),
        impulse_offsets: Vec::new(),
    });
    model.metadata.counts.morphs = model.morphs.len();

    let fbx = export_fbx(&model, None, &FbxExportOptions::default()).expect("FBX should export");
    let tree = load_tree(&fbx);
    let root = tree.root();
    let definitions = root
        .first_child_by_name("Definitions")
        .expect("Definitions node should exist");
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    let connections = root
        .first_child_by_name("Connections")
        .expect("Connections node should exist");

    assert_eq!(objects.children_by_name("Geometry").count(), 2);
    assert_eq!(
        objects
            .children_by_name("Deformer")
            .filter(|node| deformer_type(*node) == Some("BlendShape"))
            .count(),
        1
    );
    assert_eq!(
        objects
            .children_by_name("Deformer")
            .filter(|node| deformer_type(*node) == Some("BlendShapeChannel"))
            .count(),
        1
    );
    assert_eq!(object_type_count(definitions, "Geometry"), 2);
    assert_eq!(
        object_type_count(definitions, "Deformer") as usize,
        model.skeleton.bones.len() + 1 + 2
    );
    assert_eq!(
        child_i32(definitions, "Count") as usize,
        objects.children().count() + 1
    );

    let shape = objects
        .children_by_name("Geometry")
        .find(|node| {
            node.attributes()
                .get(2)
                .and_then(AttributeValue::get_string)
                == Some("Shape")
        })
        .expect("Shape geometry should exist");
    let shape_vertices = child_arr_f64(shape, "Vertices");
    assert_eq!(child_arr_i32(shape, "Indexes"), vec![1]);
    assert_eq!(shape_vertices, vec![0.0, 0.5, -0.25]);

    assert!(has_oo_connection(
        connections,
        shape_geometry_id(0),
        blend_shape_channel_id(0)
    ));
    assert!(has_oo_connection(
        connections,
        blend_shape_channel_id(0),
        blend_shape_id(0)
    ));
    assert!(has_oo_connection(
        connections,
        blend_shape_id(0),
        GEOMETRY_ID
    ));
}

fn has_op_connection(
    connections: fbxcel::tree::v7400::NodeHandle<'_>,
    child_id: i64,
    parent_id: i64,
    property: &str,
) -> bool {
    connections.children_by_name("C").any(|node| {
        let attrs = node.attributes();
        attrs.first().and_then(AttributeValue::get_string) == Some("OP")
            && attrs.get(1).and_then(AttributeValue::get_i64) == Some(child_id)
            && attrs.get(2).and_then(AttributeValue::get_i64) == Some(parent_id)
            && attrs.get(3).and_then(AttributeValue::get_string) == Some(property)
    })
}

#[test]
fn vmd_tracks_keep_last_key_after_name_resolution() {
    let mut model = vertex_morph_test_model();
    model.skeleton.bones[0].english_name = "root-alias".to_owned();
    let mut motion = crate::parse_vmd_animation(include_bytes!(
        "../../fixtures/vmd/ik_multi_bone_nondefault.vmd"
    ))
    .unwrap();
    let bone_template = motion.bone_frames[0].clone();
    motion.bone_frames.clear();
    motion.morph_frames.clear();
    for (index, (frame, weight)) in [(30, 0.25), (0, 0.5), (30, 0.75), (0, 1.0)]
        .into_iter()
        .enumerate()
    {
        let mut bone = bone_template.clone();
        bone.bone_name = if index < 2 {
            model.skeleton.bones[0].name.clone()
        } else {
            "root-alias".to_owned()
        };
        bone.frame = frame;
        bone.translation = [weight * 4.0, 0.0, 0.0];
        motion.bone_frames.push(bone);
        motion.morph_frames.push(VmdParsedMorphFrame {
            morph_name: if index < 2 { "Smile" } else { "smile" }.to_owned(),
            morph_name_bytes: Vec::new(),
            frame,
            weight,
        });
    }

    let bones = collect_bone_tracks(&model, &motion);
    assert_eq!(bones.len(), 1);
    assert_eq!(bones[0].bone_index, 0);
    let keys: Vec<_> = bones[0]
        .keyframes
        .iter()
        .map(|key| (key.frame, key.translation[0]))
        .collect();
    assert_eq!(keys, [(0, 4.0), (30, 3.0)]);

    let morphs = collect_vmd_morph_tracks(&model, &motion);
    assert_eq!(morphs.len(), 1);
    assert_eq!(morphs[0].export_index, 0);
    assert_eq!(morphs[0].frame_times, [0, 30 * FBX_FRAME_DURATION]);
    assert_eq!(morphs[0].weight_values, [100.0, 75.0]);
}

#[test]
fn vmd_export_writes_vertex_morph_weight_animation() {
    use crate::vmd::{VmdParsedAnimation, VmdParsedCounts, VmdParsedMetadata, VmdParsedMorphFrame};

    let model = vertex_morph_test_model();
    let vmd = VmdParsedAnimation {
        kind: "vmd",
        metadata: VmdParsedMetadata {
            format: "vmd",
            model_name: "fixture".to_owned(),
            model_name_bytes: Vec::new(),
            counts: VmdParsedCounts {
                bones: 0,
                morphs: 2,
                cameras: 0,
                lights: 0,
                self_shadows: 0,
                properties: 0,
            },
            max_frame: 30,
        },
        bone_frames: Vec::new(),
        morph_frames: vec![
            VmdParsedMorphFrame {
                morph_name: "smile".to_owned(),
                morph_name_bytes: Vec::new(),
                frame: 0,
                weight: 0.0,
            },
            VmdParsedMorphFrame {
                morph_name: "smile".to_owned(),
                morph_name_bytes: Vec::new(),
                frame: 30,
                weight: 0.75,
            },
        ],
        camera_frames: Vec::new(),
        light_frames: Vec::new(),
        self_shadow_frames: Vec::new(),
        property_frames: Vec::new(),
    };

    let fbx = export_fbx(&model, Some(&vmd), &FbxExportOptions::default())
        .expect("VMD morph FBX should export");
    let tree = load_tree(&fbx);
    let root = tree.root();
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    let connections = root
        .first_child_by_name("Connections")
        .expect("Connections node should exist");

    let morph_curve_nodes: Vec<_> = objects
        .children_by_name("AnimationCurveNode")
        .filter(|node| {
            node.attributes()
                .get(1)
                .and_then(AttributeValue::get_string)
                .is_some_and(|name| name.contains("DeformPercent"))
        })
        .collect();
    assert_eq!(morph_curve_nodes.len(), 1);

    let morph_curves: Vec<_> = objects
        .children_by_name("AnimationCurve")
        .filter(|node| {
            node.attributes().first().and_then(AttributeValue::get_i64)
                == Some(animation_curve_morph_id(0))
        })
        .collect();
    assert_eq!(morph_curves.len(), 1);

    let morph_curve = morph_curves[0];
    assert_eq!(child_arr_f32(morph_curve, "KeyValueFloat"), vec![0.0, 75.0]);

    let morph_node_id = animation_curvenode_morph_id(0);
    let morph_curve_id = animation_curve_morph_id(0);
    assert!(has_oo_connection(connections, morph_node_id, ANIM_LAYER_ID));
    assert!(has_op_connection(
        connections,
        morph_node_id,
        blend_shape_channel_id(0),
        "DeformPercent"
    ));
    assert!(has_op_connection(
        connections,
        morph_curve_id,
        morph_node_id,
        "d|DeformPercent"
    ));
}

#[test]
fn runtime_bake_export_writes_vertex_morph_weight_animation() {
    use mmd_anim_runtime::{
        AnimationClip, BoneInit, MorphAnimationBinding, MorphIndex, MorphInit, MorphKeyframe,
        MorphOffsetSpan, MorphTrack,
    };

    let model = vertex_morph_test_model();
    let runtime_model = mmd_anim_runtime::ModelArena::new_with_morphs(
        vec![BoneInit::new(None, glam::Vec3A::ZERO)],
        Vec::new(),
        Vec::new(),
        MorphInit {
            morph_count: 1,
            vertex_spans: vec![MorphOffsetSpan::default()],
            bone_spans: vec![MorphOffsetSpan::default()],
            group_spans: vec![MorphOffsetSpan::default()],
            ..MorphInit::default()
        },
    )
    .expect("runtime model with one morph slot should build");

    let clip = AnimationClip::new_full(
        Vec::new(),
        vec![MorphAnimationBinding {
            morph: MorphIndex(0),
            track: MorphTrack::from_keyframes(vec![
                MorphKeyframe::new(0, 0.0),
                MorphKeyframe::new(2, 0.5),
            ]),
        }],
        None,
    );
    let fbx = export_fbx_with_runtime_bake(
        &model,
        Arc::new(runtime_model),
        &clip,
        2,
        &FbxExportOptions::default(),
    )
    .expect("runtime-baked morph FBX should export");
    let tree = load_tree(&fbx);
    let root = tree.root();
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    let connections = root
        .first_child_by_name("Connections")
        .expect("Connections node should exist");

    let morph_curve = objects
        .children_by_name("AnimationCurve")
        .find(|node| {
            node.attributes().first().and_then(AttributeValue::get_i64)
                == Some(animation_curve_morph_id(0))
        })
        .expect("runtime-baked morph AnimationCurve should exist");
    assert_eq!(
        child_arr_f32(morph_curve, "KeyValueFloat"),
        vec![0.0, 25.0, 50.0]
    );

    let morph_node_id = animation_curvenode_morph_id(0);
    let morph_curve_id = animation_curve_morph_id(0);
    assert!(has_oo_connection(connections, morph_node_id, ANIM_LAYER_ID));
    assert!(has_op_connection(
        connections,
        morph_node_id,
        blend_shape_channel_id(0),
        "DeformPercent"
    ));
    assert!(has_op_connection(
        connections,
        morph_curve_id,
        morph_node_id,
        "d|DeformPercent"
    ));
}

fn test_bone(name: &str, english_name: &str) -> PmxParsedBone {
    use crate::pmx::PmxParsedBoneFlags;

    PmxParsedBone {
        name: name.to_owned(),
        english_name: english_name.to_owned(),
        parent_index: -1,
        layer: 0,
        position: [0.0, 0.0, 0.0],
        tail_index: -1,
        tail_position: Some([0.0, 1.0, 0.0]),
        flags: PmxParsedBoneFlags {
            indexed_tail: false,
            rotatable: true,
            translatable: true,
            visible: true,
            enabled: true,
            ik: false,
            append_local: false,
            append_rotate: false,
            append_translate: false,
            fixed_axis: false,
            local_axis: false,
            transform_after_physics: false,
            external_parent_transform: false,
        },
        append_transform: None,
        fixed_axis: None,
        local_axis: None,
        external_parent_key: None,
        ik: None,
    }
}

fn fbx_object_label(name_attr: &str) -> &str {
    name_attr.split('\0').next().unwrap_or(name_attr)
}

fn limb_joint_object_labels(tree: &fbxcel::tree::v7400::Tree, kind: &str) -> Vec<String> {
    let objects = tree
        .root()
        .first_child_by_name("Objects")
        .expect("Objects node should exist");
    objects
        .children_by_name(kind)
        .filter_map(|node| {
            let id = node
                .attributes()
                .first()
                .and_then(AttributeValue::get_i64)?;
            if id < BONE_MODEL_ID_BASE {
                return None;
            }
            let label = node
                .attributes()
                .get(1)
                .and_then(AttributeValue::get_string)
                .map(fbx_object_label)?;
            Some(label.to_owned())
        })
        .collect()
}

#[test]
fn legacy_bone_name_policy_hex_encodes_japanese_names() {
    let bones = vec![test_bone("センター", "")];
    let names = build_bone_names(&bones, FbxBoneNamePolicy::LegacyHex);
    assert_eq!(names[0], japanese_to_ascii("センター"));
    assert_ne!(names[0], "center");
}

#[test]
fn readable_bone_name_policy_maps_standard_mmd_names() {
    let bones = vec![
        test_bone("センター", ""),
        test_bone("左足", ""),
        test_bone("全ての親", ""),
    ];
    let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
    assert_eq!(names, vec!["center", "left_leg", "master"]);
}

#[test]
fn readable_bone_name_policy_uses_standard_dictionary_before_partial_ascii() {
    let bones = vec![test_bone("左足ＩＫ", "")];
    let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
    assert_eq!(names[0], "left_leg_ik");
}

#[test]
fn readable_bone_name_policy_uses_ascii_pmx_name() {
    let bones = vec![test_bone("left arm.01", "")];
    let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
    assert_eq!(names[0], "left_arm_01");
}

#[test]
fn readable_bone_name_policy_prefers_pmx_english_name() {
    let bones = vec![test_bone("センター", "RootCenter")];
    let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
    assert_eq!(names[0], "RootCenter");
}

#[test]
fn readable_bone_name_policy_sanitizes_english_names() {
    let bones = vec![test_bone("センター", "Left Arm-01")];
    let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
    assert_eq!(names[0], "Left_Arm_01");
}

#[test]
fn readable_bone_name_policy_deduplicates_collisions() {
    let bones = vec![
        test_bone("custom1", "left_arm"),
        test_bone("custom2", "left_arm"),
        test_bone("custom3", "left_arm"),
        test_bone("custom4", "left_arm_5"),
        test_bone("custom5", "left_arm_5_2"),
        test_bone("custom6", "left_arm"),
    ];
    let names = build_bone_names(&bones, FbxBoneNamePolicy::Readable);
    assert_eq!(
        names,
        vec![
            "left_arm",
            "left_arm_1",
            "left_arm_2",
            "left_arm_5",
            "left_arm_5_2",
            "left_arm_5_3",
        ]
    );
}

#[test]
fn readable_bone_name_policy_writes_readable_fbx_joint_names() {
    let mut model = fixture_model();
    model.skeleton.bones[0].name = "センター".to_owned();
    model.skeleton.bones[0].english_name.clear();

    let legacy_fbx = export_fbx(
        &model,
        None,
        &FbxExportOptions {
            bones_only: true,
            ..FbxExportOptions::default()
        },
    )
    .expect("legacy FBX should export");
    let legacy_tree = load_tree(&legacy_fbx);
    let legacy_models = limb_joint_object_labels(&legacy_tree, "Model");
    let legacy_attrs = limb_joint_object_labels(&legacy_tree, "NodeAttribute");
    assert!(legacy_models.iter().any(|name| name.contains("E382")));
    assert!(legacy_attrs.iter().any(|name| name.contains("E382")));

    let readable_fbx = export_fbx(
        &model,
        None,
        &FbxExportOptions {
            bones_only: true,
            bone_name_policy: FbxBoneNamePolicy::Readable,
            ..FbxExportOptions::default()
        },
    )
    .expect("readable FBX should export");
    let readable_tree = load_tree(&readable_fbx);
    let readable_models = limb_joint_object_labels(&readable_tree, "Model");
    let readable_attrs = limb_joint_object_labels(&readable_tree, "NodeAttribute");
    assert!(readable_models.contains(&"center".to_owned()));
    assert!(readable_attrs.contains(&"center".to_owned()));
    assert!(!readable_models.iter().any(|name| name.contains("E382")));
    assert!(!readable_attrs.iter().any(|name| name.contains("E382")));
}

#[derive(Debug, PartialEq, Eq)]
struct FbxStructuralAnimationCounts {
    definitions_object_count: i32,
    objects_child_count: usize,
    geometry_count: usize,
    pose_count: usize,
    model_count: usize,
    node_attribute_count: usize,
    deformer_count: usize,
    animation_curve_count: usize,
    animation_curve_node_count: usize,
    morph_animation_curve_node_count: usize,
    animation_stack_count: usize,
    animation_layer_count: usize,
    definitions_model_type_count: i32,
    definitions_animation_curve_type_count: i32,
    definitions_animation_curve_node_type_count: i32,
}

fn structural_animation_counts(tree: &fbxcel::tree::v7400::Tree) -> FbxStructuralAnimationCounts {
    let root = tree.root();
    let definitions = root
        .first_child_by_name("Definitions")
        .expect("Definitions node should exist");
    let objects = root
        .first_child_by_name("Objects")
        .expect("Objects node should exist");

    let animation_curve_node_count = objects.children_by_name("AnimationCurveNode").count();
    let morph_animation_curve_node_count = objects
        .children_by_name("AnimationCurveNode")
        .filter(|node| {
            node.attributes()
                .get(1)
                .and_then(AttributeValue::get_string)
                .is_some_and(|name| name.contains("DeformPercent"))
        })
        .count();

    FbxStructuralAnimationCounts {
        definitions_object_count: child_i32(definitions, "Count"),
        objects_child_count: objects.children().count(),
        geometry_count: objects.children_by_name("Geometry").count(),
        pose_count: objects.children_by_name("Pose").count(),
        model_count: objects.children_by_name("Model").count(),
        node_attribute_count: objects.children_by_name("NodeAttribute").count(),
        deformer_count: objects.children_by_name("Deformer").count(),
        animation_curve_count: objects.children_by_name("AnimationCurve").count(),
        animation_curve_node_count,
        morph_animation_curve_node_count,
        animation_stack_count: objects.children_by_name("AnimationStack").count(),
        animation_layer_count: objects.children_by_name("AnimationLayer").count(),
        definitions_model_type_count: object_type_count(definitions, "Model"),
        definitions_animation_curve_type_count: object_type_count(definitions, "AnimationCurve"),
        definitions_animation_curve_node_type_count: object_type_count(
            definitions,
            "AnimationCurveNode",
        ),
    }
}

fn fbx_connection_signatures(
    tree: &fbxcel::tree::v7400::Tree,
) -> Vec<(String, i64, i64, Option<String>)> {
    let connections = tree
        .root()
        .first_child_by_name("Connections")
        .expect("Connections node should exist");
    let mut signatures = connections
        .children_by_name("C")
        .map(|node| {
            let attrs = node.attributes();
            let kind = attrs
                .first()
                .and_then(AttributeValue::get_string)
                .expect("connection kind should exist")
                .to_owned();
            let child_id = attrs
                .get(1)
                .and_then(AttributeValue::get_i64)
                .expect("connection child id should exist");
            let parent_id = attrs
                .get(2)
                .and_then(AttributeValue::get_i64)
                .expect("connection parent id should exist");
            let property = attrs
                .get(3)
                .and_then(AttributeValue::get_string)
                .map(str::to_owned);
            (kind, child_id, parent_id, property)
        })
        .collect::<Vec<_>>();
    signatures.sort();
    signatures
}

fn id_based_bone_animation_op_connections(
    tree: &fbxcel::tree::v7400::Tree,
) -> Vec<(i64, i64, String)> {
    let connections = tree
        .root()
        .first_child_by_name("Connections")
        .expect("Connections node should exist");
    let mut links = connections
        .children_by_name("C")
        .filter_map(|node| {
            let attrs = node.attributes();
            if attrs.first().and_then(AttributeValue::get_string) != Some("OP") {
                return None;
            }
            let curve_node_id = attrs.get(1).and_then(AttributeValue::get_i64)?;
            let bone_model_id = attrs.get(2).and_then(AttributeValue::get_i64)?;
            let property = attrs.get(3).and_then(AttributeValue::get_string)?;
            if bone_model_id < BONE_MODEL_ID_BASE {
                return None;
            }
            if property != "Lcl Rotation" && property != "Lcl Translation" {
                return None;
            }
            Some((curve_node_id, bone_model_id, property.to_owned()))
        })
        .collect::<Vec<_>>();
    links.sort();
    links
}

#[test]
fn readable_bone_name_policy_preserves_runtime_baked_structure_and_animation_ids() {
    let mut model = fixture_model();
    model.skeleton.bones[0].name = "センター".to_owned();
    model.skeleton.bones[0].english_name.clear();

    let (runtime_model, clip, last_frame) = runtime_bake_fixture();

    let legacy_fbx = export_fbx_with_runtime_bake(
        &model,
        Arc::clone(&runtime_model),
        &clip,
        last_frame,
        &FbxExportOptions::default(),
    )
    .expect("legacy runtime-baked FBX should export");
    let readable_fbx = export_fbx_with_runtime_bake(
        &model,
        runtime_model,
        &clip,
        last_frame,
        &FbxExportOptions {
            bone_name_policy: FbxBoneNamePolicy::Readable,
            ..FbxExportOptions::default()
        },
    )
    .expect("readable runtime-baked FBX should export");

    let legacy_tree = load_tree(&legacy_fbx);
    let readable_tree = load_tree(&readable_fbx);

    assert_eq!(
        structural_animation_counts(&legacy_tree),
        structural_animation_counts(&readable_tree),
        "readable policy should not change FBX structural or animation object counts"
    );
    assert_eq!(
        fbx_connection_signatures(&legacy_tree),
        fbx_connection_signatures(&readable_tree),
        "readable policy should not change FBX connection topology"
    );
    assert_eq!(
        id_based_bone_animation_op_connections(&legacy_tree),
        id_based_bone_animation_op_connections(&readable_tree),
        "readable policy should not change model-id animation connections"
    );
    assert!(!id_based_bone_animation_op_connections(&legacy_tree).is_empty());

    let legacy_models = limb_joint_object_labels(&legacy_tree, "Model");
    let readable_models = limb_joint_object_labels(&readable_tree, "Model");
    let legacy_attrs = limb_joint_object_labels(&legacy_tree, "NodeAttribute");
    let readable_attrs = limb_joint_object_labels(&readable_tree, "NodeAttribute");
    assert!(legacy_models.iter().any(|name| name.contains("E382")));
    assert!(readable_models.contains(&"center".to_owned()));
    assert!(legacy_attrs.iter().any(|name| name.contains("E382")));
    assert!(readable_attrs.contains(&"center".to_owned()));
    assert_ne!(legacy_models, readable_models);
    assert_ne!(legacy_attrs, readable_attrs);
}
