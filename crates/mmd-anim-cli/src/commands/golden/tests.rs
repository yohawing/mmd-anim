use super::*;
use crate::schema::{MmdDumperOracleBone, MmdDumperOracleModel};
use glam::Mat4;
use mmd_anim_runtime::{IkLink, IkSolver};

fn make_identity_matrix(tx: f32, ty: f32, tz: f32) -> [f32; 16] {
    let mut m = [0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m[12] = tx;
    m[13] = ty;
    m[14] = tz;
    m
}

fn diagnostic(
    bone: &str,
    frame: i32,
    oracle_translation: [f32; 3],
    runtime_translation: [f32; 3],
    max_abs_error: f32,
    classification: &'static str,
) -> GoldenRootMotionDiagnostic {
    GoldenRootMotionDiagnostic {
        bone: bone.to_owned(),
        frame,
        runtime_translation,
        oracle_translation,
        delta: [
            runtime_translation[0] - oracle_translation[0],
            runtime_translation[1] - oracle_translation[1],
            runtime_translation[2] - oracle_translation[2],
        ],
        max_abs_error,
        classification,
    }
}

fn diagnostic_error(
    max_abs_error: f32,
    classification: &'static str,
) -> GoldenRootMotionDiagnostic {
    diagnostic(
        "センター",
        0,
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        max_abs_error,
        classification,
    )
}

#[test]
fn golden_ik_compare_args_parses_root_only() {
    let mut args = vec!["/some/root".to_owned()].into_iter();
    let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
    assert_eq!(root, "/some/root");
    assert_eq!(offset, 0.0);
    assert!(!use_json);
}

#[test]
fn golden_ik_compare_args_parses_root_and_offset() {
    let mut args = vec!["/some/root".to_owned(), "0.5".to_owned()].into_iter();
    let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
    assert_eq!(root, "/some/root");
    assert_eq!(offset, 0.5);
    assert!(!use_json);
}

#[test]
fn golden_ik_compare_args_json_flag_after_root() {
    let mut args = vec!["/some/root".to_owned(), "--json".to_owned()].into_iter();
    let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
    assert_eq!(root, "/some/root");
    assert_eq!(offset, 0.0);
    assert!(use_json);
}

#[test]
fn golden_ik_compare_args_json_flag_before_root() {
    let mut args = vec!["--json".to_owned(), "/some/root".to_owned()].into_iter();
    let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
    assert_eq!(root, "/some/root");
    assert_eq!(offset, 0.0);
    assert!(use_json);
}

#[test]
fn golden_ik_compare_args_json_with_offset() {
    let mut args = vec![
        "/some/root".to_owned(),
        "1.5".to_owned(),
        "--json".to_owned(),
    ]
    .into_iter();
    let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
    assert_eq!(root, "/some/root");
    assert_eq!(offset, 1.5);
    assert!(use_json);
}

#[test]
fn golden_ik_compare_args_all_json_first() {
    let mut args = vec![
        "--json".to_owned(),
        "/other/root".to_owned(),
        "-0.25".to_owned(),
    ]
    .into_iter();
    let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
    assert_eq!(root, "/other/root");
    assert_eq!(offset, -0.25);
    assert!(use_json);
}

#[test]
fn golden_ik_compare_args_reject_extra_values() {
    let mut args = vec![
        "/some/root".to_owned(),
        "0.5".to_owned(),
        "extra".to_owned(),
    ]
    .into_iter();
    let error = parse_golden_ik_compare_args(&mut args).unwrap_err();
    assert!(error.contains("unexpected extra argument"));
}

#[test]
fn golden_ik_compare_args_reject_unknown_flag() {
    let mut args = vec!["/some/root".to_owned(), "--bad".to_owned()].into_iter();
    let error = parse_golden_ik_compare_args(&mut args).unwrap_err();
    assert!(error.contains("unknown flag"));
}

#[test]
fn golden_ik_compare_args_reject_invalid_offset() {
    let mut args = vec!["/some/root".to_owned(), "nope".to_owned()].into_iter();
    let error = parse_golden_ik_compare_args(&mut args).unwrap_err();
    assert!(error.contains("invalid sample-frame-offset"));
}

#[test]
fn root_motion_diagnostics_center_large_delta() {
    let bone = MmdDumperOracleBone {
        index: 0,
        name: "センター".into(),
        world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
    };
    let model = MmdDumperOracleModel {
        index: 0,
        name: "test".into(),
        filename: "test.pmx".into(),
        visible: true,
        bones: vec![bone],
        morphs: vec![],
    };
    // Runtime translation = (1, 2, 3), a large delta from (0, 0, 0).
    let world = vec![Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 2.0, 3.0, 1.0,
    ])];

    let diags = compute_root_motion_diagnostics(&model, &world, 300);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].bone, "センター");
    assert_eq!(diags[0].frame, 300);
    assert_eq!(diags[0].classification, "root_motion_mismatch");
    assert!((diags[0].max_abs_error - 3.0).abs() < 1e-6);
}

#[test]
fn root_motion_diagnostics_below_threshold() {
    let bone = MmdDumperOracleBone {
        index: 0,
        name: "センター".into(),
        world_matrix: make_identity_matrix(0.5, 0.5, 0.5),
    };
    let model = MmdDumperOracleModel {
        index: 0,
        name: "test".into(),
        filename: "test.pmx".into(),
        visible: true,
        bones: vec![bone],
        morphs: vec![],
    };
    // Runtime translation = (0.5005, 0.5, 0.5), below the threshold.
    let world = vec![Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5005, 0.5, 0.5, 1.0,
    ])];

    let diags = compute_root_motion_diagnostics(&model, &world, 1);
    assert!(diags.is_empty());
}

#[test]
fn root_motion_diagnostics_mid_delta_below_new_threshold() {
    let bone = MmdDumperOracleBone {
        index: 0,
        name: "センター".into(),
        world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
    };
    let model = MmdDumperOracleModel {
        index: 0,
        name: "test".into(),
        filename: "test.pmx".into(),
        visible: true,
        bones: vec![bone],
        morphs: vec![],
    };
    // Runtime translation = (0.05, 0, 0). This is below the current
    // reporting threshold and should not produce a diagnostic.
    let world = vec![Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.05, 0.0, 0.0, 1.0,
    ])];
    let diags = compute_root_motion_diagnostics(&model, &world, 1);
    assert!(diags.is_empty());
}

#[test]
fn root_motion_diagnostics_control_bone_classification() {
    let bone = MmdDumperOracleBone {
        index: 0,
        name: "左足ＩＫ".into(),
        world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
    };
    let model = MmdDumperOracleModel {
        index: 0,
        name: "test".into(),
        filename: "test.pmx".into(),
        visible: true,
        bones: vec![bone],
        morphs: vec![],
    };
    let world = vec![Mat4::from_cols_array(&[
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 10.0, 0.0, 1.0,
    ])];

    let diags = compute_root_motion_diagnostics(&model, &world, 42);
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].bone, "左足ＩＫ");
    assert_eq!(diags[0].classification, "control_bone_mismatch");
}

#[test]
fn root_motion_diagnostics_bone_not_found() {
    let model = MmdDumperOracleModel {
        index: 0,
        name: "empty".into(),
        filename: "empty.pmx".into(),
        visible: true,
        bones: vec![],
        morphs: vec![],
    };
    let diags = compute_root_motion_diagnostics(&model, &[], 0);
    assert!(diags.is_empty());
}

#[test]
fn root_motion_diagnostics_index_out_of_range() {
    let bone = MmdDumperOracleBone {
        index: 5,
        name: "センター".into(),
        world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
    };
    let model = MmdDumperOracleModel {
        index: 0,
        name: "test".into(),
        filename: "test.pmx".into(),
        visible: true,
        bones: vec![bone],
        morphs: vec![],
    };
    let diags = compute_root_motion_diagnostics(&model, &[Mat4::IDENTITY], 0);
    assert!(diags.is_empty());
}

#[test]
fn ik_solver_residuals_reports_enabled_and_delta() {
    let solvers = vec![IkSolver {
        ik_bone: BoneIndex(0),
        target_bone: BoneIndex(1),
        links: vec![IkLink {
            bone: BoneIndex(2),
            angle_limit: None,
        }]
        .into_boxed_slice(),
        iteration_count: 1,
        limit_angle: 0.0,
    }];
    let bone_names = vec!["ik".to_owned(), "target".to_owned(), "link".to_owned()];
    let world = vec![
        Mat4::from_translation(Vec3A::new(0.0, 0.0, 0.0).into()),
        Mat4::from_translation(Vec3A::new(3.0, 0.0, 0.0).into()),
        Mat4::IDENTITY,
    ];
    let oracle = MmdDumperOracleModel {
        index: 0,
        name: "test".into(),
        filename: "test.pmx".into(),
        visible: true,
        bones: vec![
            MmdDumperOracleBone {
                index: 0,
                name: "ik".into(),
                world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
            },
            MmdDumperOracleBone {
                index: 1,
                name: "target".into(),
                world_matrix: make_identity_matrix(1.0, 0.0, 0.0),
            },
        ],
        morphs: vec![],
    };

    let residuals =
        compute_ik_solver_residuals(&solvers, &bone_names, &[0], &world, &oracle, Some(2));

    assert_eq!(residuals.len(), 1);
    assert_eq!(residuals[0].solver_index, 0);
    assert_eq!(residuals[0].ik_bone, "ik");
    assert_eq!(residuals[0].target_bone, "target");
    assert!(!residuals[0].enabled);
    assert!((residuals[0].runtime_residual - 3.0).abs() < 1e-6);
    assert!((residuals[0].oracle_residual.unwrap() - 1.0).abs() < 1e-6);
    assert!((residuals[0].residual_delta.unwrap() - 2.0).abs() < 1e-6);
}

#[test]
fn ik_solver_residuals_filters_unrelated_focus_bone() {
    let solvers = vec![IkSolver {
        ik_bone: BoneIndex(0),
        target_bone: BoneIndex(1),
        links: vec![IkLink {
            bone: BoneIndex(2),
            angle_limit: None,
        }]
        .into_boxed_slice(),
        iteration_count: 1,
        limit_angle: 0.0,
    }];
    let bone_names = vec!["ik".to_owned(), "target".to_owned(), "link".to_owned()];
    let world = vec![Mat4::IDENTITY, Mat4::IDENTITY, Mat4::IDENTITY];
    let oracle = MmdDumperOracleModel {
        index: 0,
        name: "test".into(),
        filename: "test.pmx".into(),
        visible: true,
        bones: vec![],
        morphs: vec![],
    };

    let residuals =
        compute_ik_solver_residuals(&solvers, &bone_names, &[1], &world, &oracle, Some(99));

    assert!(residuals.is_empty());
}

// is_frame_root_control_dominated tests

#[test]
fn is_dominated_zero_frame_error_returns_false() {
    let diags = vec![diagnostic_error(100.0, "root_motion_mismatch")];
    assert!(!is_frame_root_control_dominated(0.0, &diags));
}

#[test]
fn is_dominated_negative_frame_error_returns_false() {
    let diags = vec![diagnostic_error(100.0, "root_motion_mismatch")];
    assert!(!is_frame_root_control_dominated(-1.0, &diags));
}

#[test]
fn is_dominated_empty_diagnostics_returns_false() {
    assert!(!is_frame_root_control_dominated(10.0, &[]));
}

#[test]
fn is_dominated_ratio_rule_dominates() {
    // frame_max_error = 2.0, maxAbsError = 1.0 (>= 0.5 * 2.0)
    // Ratio rule fires regardless of classification.
    let diags = vec![diagnostic_error(1.0, "control_bone_mismatch")];
    assert!(is_frame_root_control_dominated(2.0, &diags));
}

#[test]
fn is_dominated_ratio_below_threshold_does_not_dominate() {
    // frame_max_error = 10.0, maxAbsError = 1.0 (< 0.5 * 10.0)
    let diags = vec![diagnostic_error(1.0, "control_bone_mismatch")];
    assert!(!is_frame_root_control_dominated(10.0, &diags));
}

#[test]
fn is_dominated_root_motion_abs_threshold_when_ratio_fails() {
    // frame_max_error = 100.0, maxAbsError = 1.0
    // Ratio check: 1.0 < 0.5 * 100.0 fails.
    // Absolute check: root_motion_mismatch && 1.0 >= 1.0 passes.
    let diags = vec![diagnostic_error(1.0, "root_motion_mismatch")];
    assert!(is_frame_root_control_dominated(100.0, &diags));
}

#[test]
fn is_dominated_control_bone_abs_alone_does_not_dominate() {
    // frame_max_error = 100.0, maxAbsError = 1.0
    // Ratio check: 1.0 < 0.5 * 100.0 fails.
    // Absolute check: control_bone_mismatch is not root_motion_mismatch, so it fails.
    let diags = vec![diagnostic_error(1.0, "control_bone_mismatch")];
    assert!(!is_frame_root_control_dominated(100.0, &diags));
}

// make_unsupported_case_entry tests

#[test]
fn unsupported_case_entry_x_extension() {
    let pmx_path = Path::new("some/case/accessory.x");
    let (summary, per_case) = make_unsupported_case_entry(pmx_path, "test-case");
    let summary = serde_json::to_value(summary).unwrap();
    let per_case = serde_json::to_value(per_case).unwrap();

    assert_eq!(summary["name"], "test-case");
    assert_eq!(summary["model"], "accessory.x");
    assert_eq!(summary["extension"], "x");
    assert_eq!(
        summary["reason"],
        "unsupported model format: only .pmx and .pmd are supported (got .x)"
    );

    assert_eq!(per_case["name"], "test-case");
    assert_eq!(per_case["status"], "skipped");
    assert_eq!(per_case["model"], "accessory.x");
    assert_eq!(
        per_case["reason"],
        "unsupported model format: only .pmx and .pmd are supported (got .x)"
    );
    assert_eq!(per_case["maxAbsError"], 0.0);
    assert_eq!(per_case["worst"], "");
}

#[test]
fn unsupported_case_entry_no_extension() {
    let pmx_path = Path::new("some/case/model_no_ext");
    let (summary, per_case) = make_unsupported_case_entry(pmx_path, "test-case");
    let summary = serde_json::to_value(summary).unwrap();
    let per_case = serde_json::to_value(per_case).unwrap();

    assert_eq!(summary["name"], "test-case");
    assert_eq!(summary["model"], "model_no_ext");
    assert_eq!(summary["extension"], "?");
    assert_eq!(
        summary["reason"],
        "unsupported model format: only .pmx and .pmd are supported (got .?)"
    );

    assert_eq!(per_case["name"], "test-case");
    assert_eq!(per_case["status"], "skipped");
    assert_eq!(per_case["model"], "model_no_ext");
    assert_eq!(
        per_case["reason"],
        "unsupported model format: only .pmx and .pmd are supported (got .?)"
    );
    assert_eq!(per_case["maxAbsError"], 0.0);
    assert_eq!(per_case["worst"], "");
}

#[test]
fn golden_model_supports_pmx_and_pmd_extensions() {
    assert!(is_supported_golden_model(Path::new("model.pmx")));
    assert!(is_supported_golden_model(Path::new("model.PMD")));
    assert!(!is_supported_golden_model(Path::new("stage.x")));
    assert!(!is_supported_golden_model(Path::new("model")));
}

// compute_root_motion_oracle_lag tests

#[test]
fn oracle_lag_empty_diagnostics() {
    let result = serde_json::to_value(compute_root_motion_oracle_lag("test", &[])).unwrap();
    assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
    assert!(result["matches"].as_array().unwrap().is_empty());
}

#[test]
fn oracle_lag_no_root_motion_classification() {
    let diags = vec![diagnostic(
        "センター",
        300,
        [10.0, 0.0, 0.0],
        [20.0, 0.0, 0.0],
        10.0,
        "control_bone_mismatch",
    )];
    let result = serde_json::to_value(compute_root_motion_oracle_lag("test", &diags)).unwrap();
    assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
}

#[test]
fn oracle_lag_single_bone_exact_match() {
    // Frame 300: runtime=(1.0,0,0), oracle=(12.0,0,0)
    // Frame 600: runtime=(2.0,0,0), oracle=(1.0,0,0)
    // oracle@600 matches runtime@300 -> lag detected
    let diags = vec![
        diagnostic(
            "センター",
            300,
            [12.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            11.0,
            "root_motion_mismatch",
        ),
        diagnostic(
            "センター",
            600,
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            12.0,
            "root_motion_mismatch",
        ),
    ];
    let result = serde_json::to_value(compute_root_motion_oracle_lag("test-case", &diags)).unwrap();
    assert_eq!(result["matchCount"].as_u64().unwrap(), 1);
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches[0]["case"], "test-case");
    assert_eq!(matches[0]["bone"], "センター");
    assert_eq!(matches[0]["frame"], 600);
    assert_eq!(matches[0]["previousFrame"], 300);
    assert_eq!(matches[0]["maxAbsError"], 12.0);
    assert!((matches[0]["matchDelta"].as_f64().unwrap() - 0.0).abs() < 1e-9);
}

#[test]
fn oracle_lag_below_threshold_no_match() {
    // oracle@600 nearly matches runtime@300 but delta > 0.001
    let diags = vec![
        diagnostic(
            "センター",
            300,
            [12.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            11.0,
            "root_motion_mismatch",
        ),
        diagnostic(
            "センター",
            600,
            [1.002, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            12.0,
            "root_motion_mismatch",
        ),
    ];
    let result = serde_json::to_value(compute_root_motion_oracle_lag("test", &diags)).unwrap();
    assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
}

#[test]
fn oracle_lag_exactly_at_threshold() {
    // f32 fixture value stays within the 0.001 oracle-lag threshold.
    let diags = vec![
        diagnostic(
            "センター",
            300,
            [12.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            11.0,
            "root_motion_mismatch",
        ),
        diagnostic(
            "センター",
            600,
            [1.0009999, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            12.0,
            "root_motion_mismatch",
        ),
    ];
    let result = serde_json::to_value(compute_root_motion_oracle_lag("test", &diags)).unwrap();
    assert_eq!(result["matchCount"].as_u64().unwrap(), 1);
}

#[test]
fn oracle_lag_two_bones_independent() {
    let diags = vec![
        diagnostic(
            "センター",
            300,
            [10.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            10.0,
            "root_motion_mismatch",
        ),
        diagnostic(
            "センター",
            600,
            [0.0, 0.0, 0.0],
            [5.0, 0.0, 0.0],
            10.0,
            "root_motion_mismatch",
        ),
        diagnostic(
            "グルーブ",
            300,
            [20.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            10.0,
            "root_motion_mismatch",
        ),
        diagnostic(
            "グルーブ",
            600,
            [10.0, 0.0, 0.0],
            [15.0, 0.0, 0.0],
            10.0,
            "root_motion_mismatch",
        ),
    ];
    let result = serde_json::to_value(compute_root_motion_oracle_lag("test", &diags)).unwrap();
    // Two bones, one lag match each = 2 total
    assert_eq!(result["matchCount"].as_u64().unwrap(), 2);
}

#[test]
fn oracle_lag_no_lag_when_oracle_differs() {
    // oracle@600 does NOT match runtime@300
    let diags = vec![
        diagnostic(
            "センター",
            300,
            [12.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            11.0,
            "root_motion_mismatch",
        ),
        diagnostic(
            "センター",
            600,
            [99.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            97.0,
            "root_motion_mismatch",
        ),
    ];
    let result = serde_json::to_value(compute_root_motion_oracle_lag("test", &diags)).unwrap();
    assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
}
