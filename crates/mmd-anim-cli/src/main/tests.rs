use super::*;
use std::{
    env, fs,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, BoneInit, ModelArena, MovableBoneKeyframe,
    MovableBoneTrack, RuntimeInstance,
};

use crate::commands::{bench, compare, export, patch};

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!("mmd-anim-cli-{name}-{nanos}"))
}

#[test]
fn test_synthetic_model_bone_count() {
    let bones = (0..8)
        .map(|i| {
            let parent = if i == 0 {
                None
            } else {
                Some(BoneIndex(i as u32 - 1))
            };
            BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0))
        })
        .collect();
    let model = ModelArena::new(bones).unwrap();
    assert_eq!(model.bone_count(), 8);
}

#[test]
fn test_synthetic_clip_track_count() {
    let tracks: Vec<_> = (0..4)
        .map(|i| {
            let track = MovableBoneTrack::from_keyframes(vec![
                MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                MovableBoneKeyframe::new(
                    30,
                    Vec3A::new(1.0, 0.0, 0.0),
                    Quat::from_axis_angle(Vec3A::Y.into(), 0.5),
                ),
            ]);
            BoneAnimationBinding {
                bone: BoneIndex(i as u32),
                track,
            }
        })
        .collect();
    let clip = AnimationClip::new(tracks);
    assert_eq!(clip.bone_track_count(), 4);
}

#[test]
fn test_bench_checksum_deterministic() {
    let bones = (0..4)
        .map(|i| {
            let parent = if i == 0 {
                None
            } else {
                Some(BoneIndex(i as u32 - 1))
            };
            BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0))
        })
        .collect();
    let model = Arc::new(ModelArena::new(bones).unwrap());
    let tracks: Vec<_> = (0..4)
        .map(|i| {
            let track = MovableBoneTrack::from_keyframes(vec![
                MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                MovableBoneKeyframe::new(
                    30,
                    Vec3A::new(1.0, 0.0, 0.0),
                    Quat::from_axis_angle(Vec3A::Y.into(), 0.5),
                ),
            ]);
            BoneAnimationBinding {
                bone: BoneIndex(i as u32),
                track,
            }
        })
        .collect();
    let clip = AnimationClip::new(tracks);

    let mut r1 = RuntimeInstance::new(Arc::clone(&model));
    let mut r2 = RuntimeInstance::new(model);
    r1.evaluate_clip_frame(&clip, 15.0);
    r2.evaluate_clip_frame(&clip, 15.0);
    assert_eq!(
        translation_checksum(r1.world_matrices()),
        translation_checksum(r2.world_matrices()),
    );
}

#[test]
fn bench_synthetic_args_use_defaults() {
    let mut args = Vec::<String>::new().into_iter();
    let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
    assert_eq!(cfg.models, 1);
    assert_eq!(cfg.bones, 32);
    assert_eq!(cfg.frames, 1000);
    assert!(!cfg.use_json);
}

#[test]
fn bench_synthetic_args_json_flag() {
    let mut args = vec!["--json".to_owned()].into_iter();
    let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
    assert_eq!(cfg.models, 1);
    assert_eq!(cfg.bones, 32);
    assert_eq!(cfg.frames, 1000);
    assert!(cfg.use_json);
}

#[test]
fn bench_synthetic_args_json_with_positional() {
    let mut args = vec![
        "4".to_owned(),
        "--json".to_owned(),
        "16".to_owned(),
        "50".to_owned(),
    ]
    .into_iter();
    let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
    assert_eq!(cfg.models, 4);
    assert_eq!(cfg.bones, 16);
    assert_eq!(cfg.frames, 50);
    assert!(cfg.use_json);
}

#[test]
fn bench_synthetic_args_json_after_positional() {
    let mut args = vec![
        "2".to_owned(),
        "8".to_owned(),
        "200".to_owned(),
        "--json".to_owned(),
    ]
    .into_iter();
    let cfg = bench::parse_bench_synthetic_args(&mut args).unwrap();
    assert_eq!(cfg.models, 2);
    assert_eq!(cfg.bones, 8);
    assert_eq!(cfg.frames, 200);
    assert!(cfg.use_json);
}

#[test]
fn bench_synthetic_args_reject_unknown_flag() {
    let mut args = vec!["--unknown".to_owned()].into_iter();
    let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
    assert!(error.to_string().contains("unknown flag"));
}

#[test]
fn bench_synthetic_args_reject_invalid_models() {
    let mut args = vec!["nope".to_owned()].into_iter();
    let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
    assert!(error.to_string().contains("invalid models"));
}

#[test]
fn bench_synthetic_args_reject_zero_models() {
    let mut args = vec!["0".to_owned()].into_iter();
    let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
    assert!(error.to_string().contains("models must be positive"));
}

#[test]
fn bench_synthetic_args_reject_extra_values() {
    let mut args = vec![
        "1".to_owned(),
        "8".to_owned(),
        "100".to_owned(),
        "extra".to_owned(),
    ]
    .into_iter();
    let error = bench::parse_bench_synthetic_args(&mut args).unwrap_err();
    assert!(error.to_string().contains("unexpected extra argument"));
}

#[test]
fn compare_numeric_mixed_manifest_dispatches_by_case_kind() {
    let temp = unique_test_dir("compare-numeric-mixed");
    fs::create_dir_all(&temp).unwrap();
    fs::write(
        temp.join("camera.vmd"),
        include_bytes!("../../../mmd-anim-format/fixtures/vmd/simple_camera.vmd"),
    )
    .unwrap();
    fs::write(
        temp.join("camera-oracle.json"),
        r#"{
                "frames": [
                    {
                        "frame": 0,
                        "camera": {
                            "distance": -30.5,
                            "position": [1.0, 2.0, 3.0],
                            "rotation": [0.1, -0.2, 0.3],
                            "fov": 35,
                            "perspective": true
                        }
                    }
                ]
            }"#,
    )
    .unwrap();
    fs::write(
        temp.join("manifest.json"),
        r#"{
                "cases": [
                    {
                        "name": "camera",
                        "kind": "camera-vmd",
                        "assets": { "cameraMotion": "camera.vmd" },
                        "oracle": { "path": "camera-oracle.json" },
                        "compare": { "epsilon": 0.003 }
                    },
                    {
                        "name": "motion",
                        "kind": "motion-numeric",
                        "assets": {
                            "model": "missing.pmx",
                            "motion": "missing.vmd"
                        },
                        "oracle": { "path": "missing.json" },
                        "frames": [0],
                        "compare": { "targets": ["bones"], "epsilon": 0.003 }
                    }
                ]
            }"#,
    )
    .unwrap();

    let error = compare::compare_numeric_manifest(&temp.join("manifest.json")).unwrap_err();
    let error = error.to_string();
    assert!(error.contains("cameraMismatches=0"));
    assert!(error.contains("motionMissing=1"));
    assert!(!error.contains("unsupported kind"));

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn numeric_compare_json_report_is_report_only_for_missing_motion_case() {
    let temp = unique_test_dir("compare-numeric-json-report-only");
    fs::create_dir_all(&temp).unwrap();
    let manifest_path = temp.join("manifest.json");
    fs::write(
        &manifest_path,
        r#"{
                "cases": [
                    {
                        "name": "missing-motion",
                        "kind": "motion-numeric",
                        "assets": {
                            "model": "missing.pmx",
                            "motion": "missing.vmd"
                        },
                        "oracle": { "path": "missing.jsonl" },
                        "frames": [0],
                        "compare": {
                            "targets": ["bones", "morphs", "rigidBodies"],
                            "epsilon": 0.003
                        }
                    }
                ]
            }"#,
    )
    .unwrap();

    let report = compare::build_numeric_compare_report(&manifest_path, false).unwrap();
    let value = report.to_json();

    assert_eq!(value["summary"]["cases"], 1);
    assert_eq!(value["summary"]["comparedCases"], 0);
    assert_eq!(value["summary"]["missing"], 1);
    assert_eq!(value["summary"]["importErrors"], 0);
    assert_eq!(value["summary"]["mismatchCount"], 0);
    assert_eq!(
        value["summary"]["skippedTargets"],
        serde_json::json!(["morphs", "rigidBodies"])
    );
    assert_eq!(value["perCase"][0]["name"], "missing-motion");
    assert_eq!(value["perCase"][0]["status"], "missing");
    assert_eq!(
        value["perCase"][0]["missingPaths"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let code = dispatch_verify(
        &manifest_path,
        Some(VerifyMode::Numeric),
        None,
        false,
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(code, ExitCode::SUCCESS);

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn numeric_compare_report_summary_reuses_merged_stats() {
    let camera = compare::CameraNumericCompareStats {
        compared_cases: 1,
        compared_frames: 2,
        mismatch_count: 3,
        skipped_targets: Default::default(),
        max_delta: 0.25,
    };
    let mut motion = compare::MotionNumericCompareStats {
        total_cases: 2,
        compared_cases: 1,
        compared_frames: 5,
        compared_bones: 7,
        mismatch_count: 11,
        max_abs_error: 1.25,
        worst: "case-a:30:左足".to_owned(),
        worst_frame: Some(30),
        worst_bone: "左足".to_owned(),
        worst_component: Some(12),
        ..compare::MotionNumericCompareStats::default()
    };
    motion.skipped_targets.insert("morphs".to_owned());
    motion.skipped_targets.insert("rigidBodies".to_owned());
    let report = compare::NumericCompareReport {
        default_epsilon: 0.003,
        camera_stats: camera,
        motion_stats: motion,
        per_case: Vec::new(),
    };
    let value = report.to_json();

    assert_eq!(value["summary"]["cases"], 3);
    assert_eq!(value["summary"]["comparedCases"], 2);
    assert_eq!(value["summary"]["comparedFrames"], 7);
    assert_eq!(value["summary"]["comparedBones"], 7);
    assert_eq!(value["summary"]["mismatchCount"], 14);
    assert_eq!(value["summary"]["maxAbsError"], 1.25);
    assert_eq!(value["summary"]["cameraMaxDelta"], 0.25);
    assert_eq!(
        value["summary"]["skippedTargets"],
        serde_json::json!(["morphs", "rigidBodies"])
    );
}

#[test]
fn compare_numeric_camera_current_dump_reads_jsonl_current_state() {
    let temp = unique_test_dir("compare-numeric-camera-current");
    fs::create_dir_all(&temp).unwrap();
    fs::write(
        temp.join("camera.vmd"),
        include_bytes!("../../../mmd-anim-format/fixtures/vmd/simple_camera.vmd"),
    )
    .unwrap();
    fs::write(
            temp.join("oracle.actual.jsonl"),
            r#"{"frame":0,"camera":{"available":true,"current":{"distance":3.0,"position":[7.029143,5.044919,32.742695],"rotation":[0.1,-0.2,0.3]}}}"#,
        )
        .unwrap();
    fs::write(
        temp.join("manifest.json"),
        r#"{
                "cases": [
                    {
                        "name": "camera-current",
                        "kind": "camera-numeric-dump",
                        "assets": { "cameraMotion": "camera.vmd" },
                        "oracle": { "path": "oracle.actual.jsonl", "format": "jsonl" },
                        "compare": {
                            "targets": ["camera.current", "d3d.projection.derived"],
                            "epsilon": 0.003
                        }
                    }
                ]
            }"#,
    )
    .unwrap();

    let report = compare::build_numeric_compare_report(&temp.join("manifest.json"), false)
        .unwrap()
        .to_json();

    assert_eq!(report["summary"]["cameraCases"], 1);
    assert_eq!(report["summary"]["cameraFrames"], 1);
    assert_eq!(report["summary"]["cameraMismatches"], 0);
    assert!(
        report["summary"]["cameraMaxDelta"].as_f64().unwrap() < 1.0e-6,
        "cameraMaxDelta={}",
        report["summary"]["cameraMaxDelta"]
    );
    assert_eq!(
        report["summary"]["skippedTargets"],
        serde_json::json!(["d3d.projection.derived"])
    );
    assert_eq!(report["perCase"][0]["kind"], "camera-numeric-dump");
    assert_eq!(
        report["perCase"][0]["skippedTargets"],
        serde_json::json!(["d3d.projection.derived"])
    );

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn verify_numeric_json_rejects_diagnose_and_camera_json_stays_rejected() {
    let target = Path::new("manifest.json");
    let numeric = dispatch_verify(
        target,
        Some(VerifyMode::Numeric),
        Some(vec!["case".to_owned(), "0".to_owned(), "bone".to_owned()]),
        false,
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(numeric, ExitCode::from(2));

    let camera = dispatch_verify(
        target,
        Some(VerifyMode::Camera),
        None,
        false,
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(camera, ExitCode::from(2));
}

#[test]
fn numeric_compare_failure_count_includes_motion_mismatches() {
    let camera = compare::CameraNumericCompareStats::default();
    let motion = compare::MotionNumericCompareStats {
        mismatch_count: 1,
        ..compare::MotionNumericCompareStats::default()
    };

    assert_eq!(compare::numeric_compare_failure_count(&camera, &motion), 1);
}

#[test]
fn motion_case_focus_bones_prefers_case_metadata_focus() {
    let case = serde_json::json!({
        "metadata": {
            "focus": {
                "bones": ["右袖", "左袖"]
            }
        }
    });
    let defaults = vec!["左ひざ".to_owned()];

    assert_eq!(
        compare::motion_case_focus_bones(&case, Some(&defaults)),
        vec!["右袖".to_owned(), "左袖".to_owned()]
    );
}

#[test]
fn motion_case_focus_bones_uses_default_focus() {
    let case = serde_json::json!({});
    let defaults = vec!["右腕".to_owned(), "左腕".to_owned()];

    assert_eq!(
        compare::motion_case_focus_bones(&case, Some(&defaults)),
        defaults
    );
}

#[test]
fn json_f32_reads_nested_number() {
    let value = serde_json::json!({
        "compare": {
            "evalFrameOffset": 1.25
        }
    });

    assert_eq!(
        compare::json_f32(&value, "/compare/evalFrameOffset"),
        Some(1.25)
    );
}

#[test]
fn vmd_roundtrip_json_reports_machine_readable_counts() {
    let parsed = mmd_anim_format::VmdParsedAnimation {
        kind: "vmd",
        metadata: mmd_anim_format::vmd::VmdParsedMetadata {
            format: "vmd",
            model_name: "miku".to_owned(),
            model_name_bytes: Vec::new(),
            counts: mmd_anim_format::vmd::VmdParsedCounts {
                bones: 1,
                morphs: 2,
                cameras: 3,
                lights: 4,
                self_shadows: 5,
                properties: 6,
            },
            max_frame: 120,
        },
        bone_frames: Vec::new(),
        morph_frames: Vec::new(),
        camera_frames: Vec::new(),
        light_frames: Vec::new(),
        self_shadow_frames: Vec::new(),
        property_frames: Vec::new(),
    };
    let value = export::vmd_roundtrip_json(
        Path::new("motion.vmd"),
        "parse-json-export-parse",
        10,
        20,
        Some(30),
        &parsed,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["format"], "vmd");
    assert_eq!(value["mode"], "parse-json-export-parse");
    assert_eq!(value["bytesIn"], 10);
    assert_eq!(value["bytesOut"], 20);
    assert_eq!(value["jsonBytes"], 30);
    assert_eq!(value["counts"]["boneFrames"], 1);
    assert_eq!(value["counts"]["propertyFrames"], 6);
    assert_eq!(value["maxFrame"], 120);
}

#[test]
fn vpd_roundtrip_json_reports_machine_readable_counts() {
    let parsed = mmd_anim_format::VpdParsedPose {
        format: "vpd",
        model_file: "model.pmx".to_owned(),
        bone_count: 2,
        bones: Vec::new(),
        diagnostics: Vec::new(),
    };
    let value = export::vpd_roundtrip_json(
        Path::new("pose.vpd"),
        "parse-export-parse",
        11,
        22,
        None,
        &parsed,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["format"], "vpd");
    assert_eq!(value["mode"], "parse-export-parse");
    assert_eq!(value["bytesIn"], 11);
    assert_eq!(value["bytesOut"], 22);
    assert!(value["jsonBytes"].is_null());
    assert_eq!(value["counts"]["bones"], 2);
}

#[test]
fn accessory_roundtrip_json_reports_text_mesh_material_export_scope() {
    let parsed = mmd_anim_format::AccessoryParsedManifest {
        format: "x".to_owned(),
        byte_length: 100,
        text: true,
        header: "xof 0303txt 0032".to_owned(),
        mesh_count: 1,
        material_count: 1,
        mesh_summaries: vec![mmd_anim_format::xfile::AccessoryMeshSummary {
            vertex_count: 3,
            face_count: 1,
            positions: vec![[0.0, 0.0, 0.0]],
            face_indices: vec![vec![0, 1, 2]],
            normals: Vec::new(),
            normal_face_indices: Vec::new(),
            texture_coordinates: vec![[0.0, 0.0]],
            vertex_colors: vec![mmd_anim_format::xfile::AccessoryVertexColor {
                vertex_index: 2,
                color: [1.0, 0.5, 0.25, 1.0],
            }],
            material_indices: vec![0],
            material_start_index: 0,
            material_count: 1,
        }],
        materials: vec![mmd_anim_format::xfile::AccessoryMaterial {
            name: Some("mat".to_owned()),
            face_color: Some([1.0, 1.0, 1.0, 1.0]),
            power: Some(5.0),
            specular_color: Some([0.0, 0.0, 0.0]),
            emissive_color: Some([0.0, 0.0, 0.0]),
            texture_references: vec!["tex.png".to_owned()],
        }],
        vac_settings: None,
        texture_references: vec!["tex.png".to_owned()],
        diagnostics: Vec::new(),
    };
    let value = export::accessory_roundtrip_json(
        Path::new("stage.x"),
        "parse-json-export-parse",
        100,
        50,
        Some(200),
        &parsed,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["format"], "x");
    assert_eq!(value["counts"]["meshes"], 1);
    assert_eq!(value["counts"]["materials"], 1);
    assert_eq!(value["counts"]["meshVertices"], 3);
    assert_eq!(value["counts"]["meshFaces"], 1);
    assert_eq!(value["counts"]["meshNormals"], 0);
    assert_eq!(value["counts"]["meshTextureCoordinates"], 1);
    assert_eq!(value["counts"]["meshVertexColors"], 1);
    assert_eq!(value["counts"]["meshMaterialIndices"], 1);
    assert_eq!(
        value["metadata"]["exportScope"],
        "text-mesh-material-attributes"
    );
    assert_eq!(value["metadata"]["meshMaterialReemitted"], true);
    assert_eq!(
        value["metadata"]["preservedFields"],
        serde_json::json!([
            "format",
            "header",
            "textureReferences",
            "meshSummaries",
            "materials"
        ])
    );
}

#[test]
fn ensure_accessory_roundtrip_rejects_text_flag_changes() {
    let expected = mmd_anim_format::AccessoryParsedManifest {
        format: "x".to_owned(),
        byte_length: 16,
        text: false,
        header: "xof 0303bin 0032".to_owned(),
        mesh_count: 0,
        material_count: 0,
        mesh_summaries: Vec::new(),
        materials: Vec::new(),
        vac_settings: None,
        texture_references: Vec::new(),
        diagnostics: Vec::new(),
    };
    let mut actual = expected.clone();
    actual.text = true;

    let error = export::ensure_accessory_roundtrip(&expected, &actual).unwrap_err();
    assert!(error.contains("text flag changed"));
}

#[test]
fn ensure_accessory_roundtrip_accepts_multi_mesh_material_ownership() {
    let expected = mmd_anim_format::AccessoryParsedManifest {
        format: "x".to_owned(),
        byte_length: 100,
        text: true,
        header: "xof 0303txt 0032".to_owned(),
        mesh_count: 2,
        material_count: 2,
        mesh_summaries: vec![
            mmd_anim_format::xfile::AccessoryMeshSummary {
                vertex_count: 3,
                face_count: 1,
                positions: vec![[0.0, 0.0, 0.0]],
                face_indices: vec![vec![0, 1, 2]],
                normals: Vec::new(),
                normal_face_indices: Vec::new(),
                texture_coordinates: Vec::new(),
                vertex_colors: Vec::new(),
                material_indices: vec![0],
                material_start_index: 0,
                material_count: 1,
            },
            mmd_anim_format::xfile::AccessoryMeshSummary {
                vertex_count: 3,
                face_count: 1,
                positions: vec![[0.0, 0.0, 1.0]],
                face_indices: vec![vec![0, 2, 1]],
                normals: Vec::new(),
                normal_face_indices: Vec::new(),
                texture_coordinates: Vec::new(),
                vertex_colors: Vec::new(),
                material_indices: vec![0],
                material_start_index: 1,
                material_count: 1,
            },
        ],
        materials: vec![
            mmd_anim_format::xfile::AccessoryMaterial {
                name: Some("mat0".to_owned()),
                face_color: Some([1.0, 1.0, 1.0, 1.0]),
                power: Some(5.0),
                specular_color: Some([0.0, 0.0, 0.0]),
                emissive_color: Some([0.0, 0.0, 0.0]),
                texture_references: Vec::new(),
            },
            mmd_anim_format::xfile::AccessoryMaterial {
                name: Some("mat1".to_owned()),
                face_color: Some([0.5, 0.5, 0.5, 1.0]),
                power: Some(2.0),
                specular_color: Some([0.0, 0.0, 0.0]),
                emissive_color: Some([0.0, 0.0, 0.0]),
                texture_references: Vec::new(),
            },
        ],
        vac_settings: None,
        texture_references: Vec::new(),
        diagnostics: Vec::new(),
    };
    let actual = expected.clone();

    export::ensure_accessory_roundtrip(&expected, &actual).unwrap();
}

#[test]
fn ensure_accessory_json_roundtrip_rejects_dto_changes() {
    let expected = mmd_anim_format::AccessoryParsedManifest {
        format: "x".to_owned(),
        byte_length: 16,
        text: true,
        header: "xof 0303txt 0032".to_owned(),
        mesh_count: 0,
        material_count: 0,
        mesh_summaries: Vec::new(),
        materials: Vec::new(),
        vac_settings: None,
        texture_references: vec!["tex.png".to_owned()],
        diagnostics: Vec::new(),
    };
    let mut actual = expected.clone();
    actual.texture_references.clear();

    let error = export::ensure_accessory_json_roundtrip(&expected, &actual).unwrap_err();
    assert_eq!(error, "Accessory JSON data differs after re-encoding");
}

#[test]
fn pmd_roundtrip_json_reports_machine_readable_counts() {
    let parsed = mmd_anim_format::PmdParsedModel {
        metadata: mmd_anim_format::pmd::PmdParsedMetadata {
            format: "pmd".to_owned(),
            version: 1.0,
            encoding: "shift-jis".to_owned(),
            name: "model".to_owned(),
            name_bytes: Vec::new(),
            english_name: String::new(),
            english_name_bytes: Vec::new(),
            comment: String::new(),
            comment_bytes: Vec::new(),
            english_comment: String::new(),
            english_comment_bytes: Vec::new(),
            counts: mmd_anim_format::pmd::PmdParsedCounts {
                vertices: 1,
                faces: 2,
                materials: 3,
                bones: 4,
                ik: 5,
                morphs: 6,
                display_frames: 7,
                rigid_bodies: 8,
                joints: 9,
            },
        },
        geometry: mmd_anim_format::pmd::PmdParsedGeometry {
            vertices: Vec::new(),
            indices: Vec::new(),
        },
        materials: Vec::new(),
        toon_textures: Vec::new(),
        toon_texture_bytes: Vec::new(),
        skeleton: mmd_anim_format::pmd::PmdParsedSkeleton {
            bones: Vec::new(),
            ik: Vec::new(),
        },
        morphs: Vec::new(),
        display_frames: Vec::new(),
        rigid_bodies: Vec::new(),
        joints: Vec::new(),
        diagnostics: Vec::new(),
    };
    let value = export::pmd_roundtrip_json(
        Path::new("model.pmd"),
        "parse-json-export-parse",
        10,
        20,
        Some(30),
        &parsed,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["format"], "pmd");
    assert_eq!(value["mode"], "parse-json-export-parse");
    assert_eq!(value["bytesIn"], 10);
    assert_eq!(value["bytesOut"], 20);
    assert_eq!(value["jsonBytes"], 30);
    assert_eq!(value["counts"]["vertices"], 1);
    assert_eq!(value["counts"]["ik"], 5);
    assert_eq!(value["counts"]["joints"], 9);
}

#[test]
fn pmx_roundtrip_json_reports_machine_readable_counts() {
    let parsed = mmd_anim_format::PmxParsedModel {
        metadata: mmd_anim_format::pmx::PmxParsedMetadata {
            format: "pmx".to_owned(),
            version: 2.0,
            encoding: "utf-8".to_owned(),
            name: "model".to_owned(),
            english_name: String::new(),
            comment: String::new(),
            english_comment: String::new(),
            counts: mmd_anim_format::pmx::PmxParsedCounts {
                vertices: 1,
                faces: 2,
                materials: 3,
                bones: 4,
                morphs: 5,
                display_frames: 6,
                rigid_bodies: 7,
                joints: 8,
                soft_bodies: 9,
            },
            index_sizes: mmd_anim_format::pmx::PmxParsedIndexSizes {
                vertex: 4,
                texture: 1,
                material: 1,
                bone: 2,
                morph: 1,
                rigid_body: 1,
            },
            additional_uv_count: 0,
        },
        geometry: mmd_anim_format::pmx::PmxParsedGeometry {
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            additional_uvs: Vec::new(),
            indices: Vec::new(),
            skin_indices: Vec::new(),
            skin_weights: Vec::new(),
            edge_scale: Vec::new(),
            material_groups: Vec::new(),
            sdef: mmd_anim_format::pmx::PmxParsedSdef::default(),
            qdef: mmd_anim_format::pmx::PmxParsedQdef::default(),
        },
        materials: Vec::new(),
        skeleton: mmd_anim_format::pmx::PmxParsedSkeleton { bones: Vec::new() },
        morphs: Vec::new(),
        display_frames: Vec::new(),
        rigid_bodies: Vec::new(),
        joints: Vec::new(),
        soft_bodies: Vec::new(),
        diagnostics: Vec::new(),
    };
    let value = export::pmx_roundtrip_json(
        Path::new("model.pmx"),
        "parse-json-export-parse",
        10,
        20,
        Some(30),
        &parsed,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["format"], "pmx");
    assert_eq!(value["mode"], "parse-json-export-parse");
    assert_eq!(value["bytesIn"], 10);
    assert_eq!(value["bytesOut"], 20);
    assert_eq!(value["jsonBytes"], 30);
    assert_eq!(value["metadata"]["version"], 2.0);
    assert_eq!(value["metadata"]["encoding"], "utf-8");
    assert_eq!(value["metadata"]["additionalUvCount"], 0);
    assert_eq!(value["metadata"]["indexSizes"]["vertex"], 4);
    assert_eq!(value["metadata"]["indexSizes"]["bone"], 2);
    assert_eq!(value["counts"]["vertices"], 1);
    assert_eq!(value["counts"]["softBodies"], 9);
}

#[test]
fn resolve_pmx_path_for_pmm_makes_relative_existing_path_absolute() {
    let relative = Path::new("Cargo.toml");
    assert!(
        relative.exists(),
        "Cargo.toml must exist for this repository-local test"
    );

    let resolved = export::resolve_pmx_path_for_pmm(relative)
        .expect("canonicalize must succeed for an existing repository file");

    let resolved_path = Path::new(&resolved);
    assert!(
        resolved_path.is_absolute(),
        "expected canonical PMX path for PMM to be absolute, got: {}",
        resolved
    );
    assert!(
        !resolved.starts_with(r"\\?\"),
        "expected PMM path to avoid Windows verbatim prefix for MMD GUI loading, got: {}",
        resolved
    );
}

#[test]
fn export_pmm_scene_embeds_clean_absolute_model_path() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmx_path = format_crate.join("fixtures/pmx/ik_multi_axis_limit.pmx");
    let vmd_path = format_crate.join("fixtures/vmd/ik_multi_bone_nondefault.vmd");

    let model_path_text =
        export::resolve_pmx_path_for_pmm(&pmx_path).expect("PMX fixture path must resolve");
    let model_bytes = fs::read(&pmx_path).expect("PMX fixture must exist");
    let motion_bytes = fs::read(&vmd_path).expect("VMD fixture must exist");
    let model = mmd_anim_format::parse_pmx_model(&model_bytes).expect("PMX fixture parses");
    let motion = mmd_anim_format::parse_vmd_animation(&motion_bytes).expect("VMD fixture parses");

    let report = mmd_anim_format::export_pmm_scene_from_pmx_vmd(
        &model,
        &motion,
        &model_path_text,
        &mmd_anim_format::PmmSceneExportOptions::default(),
    );
    let reparsed =
        mmd_anim_format::parse_pmm_manifest(&report.bytes).expect("exported PMM reparses");
    let document = reparsed
        .document_summary
        .as_ref()
        .expect("exported PMM includes a document summary");
    let embedded_path = &document.models[0].path;

    assert_eq!(embedded_path, &model_path_text);
    assert!(
        Path::new(embedded_path).is_absolute(),
        "expected exported PMM model path to be absolute, got: {}",
        embedded_path
    );
    assert!(
        !embedded_path.starts_with(r"\\?\"),
        "expected exported PMM model path to avoid Windows verbatim prefix, got: {}",
        embedded_path
    );
}

#[test]
fn pmm_roundtrip_json_reports_machine_readable_counts() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let data =
        fs::read(&pmm_path).expect("existing PMM fixture must be readable for helper shape test");
    let parsed = mmd_anim_format::parse_pmm_manifest(&data)
        .expect("existing PMM fixture must parse for helper shape test");
    let value = export::pmm_roundtrip_json(
        Path::new("scene.pmm"),
        "parse-export-parse-lossless",
        data.len(),
        data.len(),
        true,
        &parsed,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["format"], "pmm");
    assert_eq!(value["mode"], "parse-export-parse-lossless");
    assert_eq!(value["bytesIn"], data.len());
    assert_eq!(value["bytesOut"], data.len());
    assert_eq!(value["version"], parsed.version);
    assert!(value["modelReferences"].is_number());
    assert!(value["assetReferences"].is_number());
    assert!(value["diagnostics"].is_number());
    assert_eq!(value["byteForByte"], true);
}

#[test]
fn ensure_pmm_lossless_roundtrip_rejects_non_identical_bytes() {
    let original: &[u8] = b"Polygon Movie maker 0002\0dummy";
    let exported: &[u8] = b"different-bytes";
    let error = export::ensure_pmm_lossless_roundtrip(original, exported).unwrap_err();
    let msg = error.to_string();
    assert!(
        msg.contains("byte") || msg.contains("lossless") || msg.contains("preserve"),
        "expected rejection message about non-identical bytes, got: {}",
        msg
    );
}

#[test]
fn pmm_parse_export_parse_lossless_roundtrip_via_helpers() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let data = fs::read(&pmm_path).expect("existing PMM fixture must be readable");
    let parsed = mmd_anim_format::parse_pmm_manifest(&data).expect("fixture parses");
    let exported = mmd_anim_format::export_pmm_manifest(&parsed);
    let reparsed = mmd_anim_format::parse_pmm_manifest(&exported).expect("exported reparses");

    export::ensure_pmm_lossless_roundtrip(&data, &exported)
        .expect("PMM parse-export-parse must be byte-for-byte lossless for parsed source");
    assert_eq!(reparsed.version, parsed.version);
    assert_eq!(
        exported, data,
        "exported bytes must equal original input bytes"
    );
}

#[test]
fn export_roundtrip_summary_calls_pmm_lossless_branch_successfully() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let result = export::export_roundtrip_summary(&pmm_path);
    assert!(
        result.is_ok(),
        "export_roundtrip_summary on repo-local PMM fixture must succeed (lossless branch)"
    );
    let code = result.unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn export_roundtrip_json_calls_pmm_lossless_branch_successfully() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let result = export::export_roundtrip_json(&pmm_path);
    assert!(
        result.is_ok(),
        "export_roundtrip_json on repo-local PMM fixture must succeed (lossless branch)"
    );
    let code = result.unwrap();
    assert_eq!(code, ExitCode::SUCCESS);
}

#[test]
fn export_json_roundtrip_summary_rejects_pmm_as_unsupported() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let result = export::export_json_roundtrip_summary(&pmm_path);
    let err =
        result.expect_err("export_json_roundtrip_summary on PMM fixture must remain unsupported");
    let msg = err.to_string();
    assert!(
        msg.contains("not implemented") || msg.contains("PMM"),
        "expected 'not implemented' error mentioning PMM for json roundtrip, got: {}",
        msg
    );
}

#[test]
fn patch_pmm_document_model_path_replaces_path_and_preserves_length() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let data = fs::read(&pmm_path).expect("existing PMM fixture must be readable");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let target_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let out_dir = target_root.join("pmm-document-model-patch-test");
    let _ = fs::create_dir_all(&out_dir);
    let out_path = out_dir.join(format!("patched-doc0-{}.pmm", nanos));

    let replacement = "UserFile\\Model\\override_for_cli_patch_test.pmx";
    let result = patch::patch_pmm_document_model_path(&pmm_path, "0", replacement, &out_path);
    assert!(
        result.is_ok(),
        "patch_pmm_document_model_path on repo-local fixture must succeed: {:?}",
        result.err()
    );
    let code = result.unwrap();
    assert_eq!(code, ExitCode::SUCCESS);

    let out_data = fs::read(&out_path).expect("patched output must exist");
    assert_eq!(
        out_data.len(),
        data.len(),
        "byte length must be unchanged by document model path patch"
    );

    let reparsed =
        mmd_anim_format::parse_pmm_manifest(&out_data).expect("patched output must reparse");
    let doc = reparsed
        .document_summary
        .as_ref()
        .expect("fixture PMM must have document_summary");
    let model0 = doc
        .models
        .iter()
        .find(|m| m.document_model_index == 0)
        .expect("document model 0 must exist in fixture");
    assert_eq!(
        model0.path, replacement,
        "document model 0 path must equal replacement after patch"
    );

    let _ = fs::remove_file(&out_path);
}

#[test]
fn patch_pmm_scene_frame_range_updates_fields_and_preserves_length() {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    let pmm_path = format_crate.join("fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let data = fs::read(&pmm_path).expect("existing PMM fixture must be readable");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let target_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target");
    let out_dir = target_root.join("pmm-scene-frame-range-patch-test");
    let _ = fs::create_dir_all(&out_dir);
    let out_path = out_dir.join(format!("patched-scene-frame-range-{}.pmm", nanos));

    let options = vec![
        "--current-frame".to_string(),
        "99".to_string(),
        "--current-frame-text".to_string(),
        "77".to_string(),
        "--begin-frame-enabled".to_string(),
        "true".to_string(),
        "--end-frame-enabled".to_string(),
        "false".to_string(),
        "--begin-frame".to_string(),
        "10".to_string(),
        "--end-frame".to_string(),
        "240".to_string(),
    ];
    let result = patch::patch_pmm_scene_frame_range(&pmm_path, &out_path, &options);
    assert!(
        result.is_ok(),
        "patch_pmm_scene_frame_range on repo-local fixture must succeed: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), ExitCode::SUCCESS);

    let out_data = fs::read(&out_path).expect("patched output must exist");
    assert_eq!(
        out_data.len(),
        data.len(),
        "byte length must be unchanged by scene frame range patch"
    );

    let reparsed =
        mmd_anim_format::parse_pmm_manifest(&out_data).expect("patched output must reparse");
    let settings = &reparsed
        .document_global_summary
        .as_ref()
        .expect("fixture PMM must have document_global_summary")
        .settings;
    assert_eq!(settings.current_frame_index, 99);
    assert_eq!(settings.current_frame_index_in_text_field, 77);
    assert!(settings.begin_frame_index_enabled);
    assert!(!settings.end_frame_index_enabled);
    assert_eq!(settings.begin_frame_index, 10);
    assert_eq!(settings.end_frame_index, 240);

    let _ = fs::remove_file(&out_path);
}

#[test]
fn parse_pmm_scene_frame_range_patch_options_requires_at_least_one_option() {
    let err = patch::parse_pmm_scene_frame_range_patch_options(&[]).unwrap_err();
    assert!(
        err.contains("at least one patch option is required"),
        "unexpected error: {err}"
    );
}

#[test]
fn parse_pmm_scene_frame_range_patch_options_rejects_unknown_and_invalid_values() {
    let unknown = patch::parse_pmm_scene_frame_range_patch_options(&[
        "--unknown".to_string(),
        "1".to_string(),
    ])
    .unwrap_err();
    assert!(
        unknown.contains("unknown option"),
        "unexpected unknown-option error: {unknown}"
    );

    let missing_value =
        patch::parse_pmm_scene_frame_range_patch_options(&["--begin-frame".to_string()])
            .unwrap_err();
    assert!(
        missing_value.contains("missing value"),
        "unexpected missing-value error: {missing_value}"
    );

    let invalid_bool = patch::parse_pmm_scene_frame_range_patch_options(&[
        "--begin-frame-enabled".to_string(),
        "maybe".to_string(),
    ])
    .unwrap_err();
    assert!(
        invalid_bool.contains("invalid --begin-frame-enabled"),
        "unexpected invalid-bool error: {invalid_bool}"
    );
}
