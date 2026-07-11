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

use crate::commands::{bench, compare, export, golden, import, oracle, parse, patch, vmd_sample};
use crate::schema::MmdDumperOracleDump;

#[test]
fn completion_subcommand_is_registered_in_clap_metadata() {
    use clap::CommandFactory;

    let subcommands: Vec<_> = Cli::command()
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .collect();
    assert!(
        subcommands.contains(&"completion".to_owned()),
        "expected completion subcommand, got: {subcommands:?}"
    );
}

#[test]
fn completion_generates_non_empty_scripts_for_supported_shells() {
    use clap::CommandFactory;
    use clap_complete::{Shell, generate};

    let expectations = [
        ("bash", Shell::Bash, "mmd-anim"),
        ("zsh", Shell::Zsh, "_mmd-anim"),
        ("fish", Shell::Fish, "mmd-anim"),
        ("powershell", Shell::PowerShell, "mmd-anim"),
    ];

    for (shell_name, shell, marker) in expectations {
        let mut cmd = Cli::command();
        let mut buffer = Vec::new();
        generate(shell, &mut cmd, "mmd-anim", &mut buffer);
        let script = String::from_utf8(buffer).expect("completion script must be valid UTF-8");
        assert!(
            !script.is_empty(),
            "completion for {shell_name} must be non-empty"
        );
        assert!(
            script.contains(marker),
            "completion for {shell_name} should mention {marker}"
        );
    }
}

#[test]
fn completion_dispatch_writes_non_empty_stdout_for_each_shell() {
    use clap::Parser;

    for shell_name in ["bash", "zsh", "fish", "powershell"] {
        let cli = Cli::try_parse_from(["mmd-anim", "completion", shell_name]).unwrap();
        let Commands::Completion { shell } = cli.command.expect("completion command must parse")
        else {
            panic!("expected completion subcommand for shell={shell_name}");
        };
        let mut capture = Vec::new();
        {
            use clap::CommandFactory;
            use clap_complete::{Shell, generate};

            let shell_kind = match shell {
                CompletionShell::Bash => Shell::Bash,
                CompletionShell::Zsh => Shell::Zsh,
                CompletionShell::Fish => Shell::Fish,
                CompletionShell::PowerShell => Shell::PowerShell,
            };
            let mut cmd = Cli::command();
            generate(shell_kind, &mut cmd, "mmd-anim", &mut capture);
        }
        assert!(
            !capture.is_empty(),
            "dispatch path must emit completion bytes for shell={shell_name}"
        );
    }
}

#[test]
fn completion_rejects_unsupported_shell_names() {
    use clap::Parser;

    let message = match Cli::try_parse_from(["mmd-anim", "completion", "nushell"]) {
        Ok(_) => panic!("unsupported shell name must be rejected by clap"),
        Err(error) => error.to_string(),
    };
    assert!(
        message.contains("invalid value") || message.contains("possible values"),
        "unexpected clap error for unsupported shell: {message}"
    );
    assert!(
        message.contains("bash")
            && message.contains("zsh")
            && message.contains("fish")
            && message.contains("powershell"),
        "unsupported shell error should list supported shells: {message}"
    );
}

#[test]
fn extended_version_text_includes_package_rustc_target_and_git_fields() {
    let text = extended_version_text();
    assert!(
        text.starts_with(&format!("mmd-anim {}", env!("CARGO_PKG_VERSION"))),
        "unexpected version prefix: {text}"
    );
    assert!(text.contains(&format!("rustc: {}", env!("MMD_ANIM_CLI_RUSTC_VERSION"))));
    assert!(text.contains(&format!("target: {}", env!("MMD_ANIM_CLI_BUILD_TARGET"))));
    assert!(text.contains(&format!("git: {}", env!("MMD_ANIM_CLI_GIT_COMMIT"))));
}

#[test]
fn extended_version_text_prefixes_clap_version_body() {
    assert_eq!(
        extended_version_text(),
        format!("mmd-anim {}", extended_version())
    );
}

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    env::temp_dir().join(format!("mmd-anim-cli-{name}-{nanos}"))
}

fn assert_f32_near(actual: f32, expected: f32) {
    let delta = (actual - expected).abs();
    assert!(
        delta < 1.0e-4,
        "actual={actual:?} expected={expected:?} delta={delta:?}"
    );
}

fn assert_array3_near(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_f32_near(actual, expected);
    }
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
fn bench_pair_args_json_flag() {
    let mut args = vec![
        "model.pmx".to_owned(),
        "motion.vmd".to_owned(),
        "--json".to_owned(),
    ]
    .into_iter();
    let cfg = bench::parse_bench_pair_args(&mut args).unwrap();
    assert_eq!(cfg.pmx_path, PathBuf::from("model.pmx"));
    assert_eq!(cfg.vmd_path, PathBuf::from("motion.vmd"));
    assert_eq!(cfg.start_frame, 0.0);
    assert_eq!(cfg.frame_count, 1000);
    assert_eq!(cfg.step, 1.0);
    assert_eq!(cfg.instances, 1);
    assert!(cfg.solve_ik);
    assert!(cfg.use_json);
}

#[test]
fn bench_pair_args_json_with_positional_and_flags() {
    let mut args = vec![
        "model.pmx".to_owned(),
        "motion.vmd".to_owned(),
        "10".to_owned(),
        "--json".to_owned(),
        "120".to_owned(),
        "2".to_owned(),
        "--no-ik".to_owned(),
        "--ik-tolerance".to_owned(),
        "0.001".to_owned(),
    ]
    .into_iter();
    let cfg = bench::parse_bench_pair_args(&mut args).unwrap();
    assert_eq!(cfg.start_frame, 10.0);
    assert_eq!(cfg.frame_count, 120);
    assert_eq!(cfg.step, 2.0);
    assert_eq!(cfg.instances, 1);
    assert!(!cfg.solve_ik);
    assert!((cfg.ik_options.tolerance - 0.001).abs() < f32::EPSILON);
    assert!(cfg.use_json);
}

#[test]
fn bench_pair_args_instances_flag() {
    let mut args = vec![
        "model.pmx".to_owned(),
        "motion.vmd".to_owned(),
        "--instances".to_owned(),
        "30".to_owned(),
        "--json".to_owned(),
    ]
    .into_iter();
    let cfg = bench::parse_bench_pair_args(&mut args).unwrap();
    assert_eq!(cfg.instances, 30);
    assert!(cfg.use_json);
}

#[test]
fn bench_pair_args_reject_zero_instances() {
    let mut args = vec![
        "model.pmx".to_owned(),
        "motion.vmd".to_owned(),
        "--instances".to_owned(),
        "0".to_owned(),
    ]
    .into_iter();
    let error = bench::parse_bench_pair_args(&mut args).unwrap_err();
    assert!(error.to_string().contains("instances must be positive"));
}

#[test]
fn bench_pair_args_reject_unknown_flag() {
    let mut args = vec![
        "model.pmx".to_owned(),
        "motion.vmd".to_owned(),
        "--unknown".to_owned(),
    ]
    .into_iter();
    let error = bench::parse_bench_pair_args(&mut args).unwrap_err();
    assert!(error.to_string().contains("unknown flag"));
}

#[test]
fn bench_pair_report_json_includes_core_fields_without_ik() {
    use bench::BenchPairReportInput;
    use mmd_anim_runtime::IkSolveOptions;

    let pmx_path = PathBuf::from("model.pmx");
    let vmd_path = PathBuf::from("motion.vmd");
    let report = bench::bench_pair_report_json(BenchPairReportInput {
        pmx_path: &pmx_path,
        vmd_path: &vmd_path,
        bone_count: 128,
        append_count: 4,
        fixed_axis_count: 2,
        solver_count: 3,
        morph_count: 16,
        vmd_bone_keys: 2400,
        vmd_morph_keys: 120,
        clip_bone_tracks: 64,
        clip_morph_tracks: 8,
        property_track: true,
        clip_frame_range: Some((0, 240)),
        start_frame: 0.0,
        frame_count: 240,
        step: 1.0,
        instances: 10,
        total_evaluations: 2400,
        solve_ik: false,
        ik_options: IkSolveOptions::default(),
        read_ms: 1.25,
        pmx_import_ms: 2.5,
        vmd_import_ms: 0.75,
        clip_build_ms: 3.0,
        eval_ms: 12.0,
        apply_pose_ms: 4.0,
        morph_expand_ms: 2.0,
        pose_eval_ms: 6.0,
        world_copy_ms: 2.0,
        skinning_copy_ms: 1.5,
        morph_copy_ms: 0.5,
        hot_loop_ms: 16.0,
        total_ms: 19.5,
        ms_per_frame: 0.066666,
        fps: 15000.0,
        ms_per_evaluation: 0.006666,
        evaluations_per_second: 150000.0,
        checksum: 0x1234_abcd,
        morph_checksum: 0xdead_beef,
        ik_solver_summaries: &[],
        ik_stats: None,
    });

    assert_eq!(report["status"], "ok");
    assert_eq!(report["command"], "bench");
    assert_eq!(report["mode"], "pair");
    assert_eq!(report["model"], "model.pmx");
    assert_eq!(report["motion"], "motion.vmd");
    assert_eq!(report["counts"]["bones"], 128);
    assert_eq!(report["counts"]["propertyTrack"], true);
    assert_eq!(report["config"]["instances"], 10);
    assert_eq!(report["config"]["totalEvaluations"], 2400);
    assert_eq!(report["config"]["solveIk"], false);
    assert_eq!(report["timing"]["evalMs"], 12.0);
    assert_eq!(report["timing"]["applyPoseMs"], 4.0);
    assert_eq!(report["timing"]["morphExpandMs"], 2.0);
    assert_eq!(report["timing"]["poseEvalMs"], 6.0);
    assert_eq!(
        report["timing"]["evalMs"].as_f64().unwrap(),
        report["timing"]["applyPoseMs"].as_f64().unwrap()
            + report["timing"]["morphExpandMs"].as_f64().unwrap()
            + report["timing"]["poseEvalMs"].as_f64().unwrap()
    );
    assert_eq!(report["timing"]["worldCopyMs"], 2.0);
    assert_eq!(report["timing"]["skinningCopyMs"], 1.5);
    assert_eq!(report["timing"]["morphCopyMs"], 0.5);
    assert_eq!(report["timing"]["hotLoopMs"], 16.0);
    assert_eq!(report["timing"]["msPerEvaluation"], 0.006666);
    assert_eq!(report["timing"]["evaluationsPerSecond"], 150000.0);
    assert_eq!(report["result"]["checksum"], "1234abcd");
    assert_eq!(report["result"]["clipFrameRange"], "0..240");
    assert!(report.get("ik").is_none());
}

#[test]
fn bench_pair_report_json_includes_ik_aggregate_and_top_solvers() {
    use bench::{BenchPairIkSolverSummary, BenchPairReportInput};
    use mmd_anim_runtime::{IkSolveOptions, IkSolverRuntimeStats};

    let pmx_path = PathBuf::from("model.pmx");
    let vmd_path = PathBuf::from("motion.vmd");
    let ik_solver_summaries = [
        BenchPairIkSolverSummary {
            solver_index: 0,
            bone_index: 10,
            name: "左足ＩＫ".to_owned(),
            max_iterations: 20,
            links: 3,
        },
        BenchPairIkSolverSummary {
            solver_index: 1,
            bone_index: 11,
            name: "右足ＩＫ".to_owned(),
            max_iterations: 20,
            links: 3,
        },
    ];
    let ik_stats = [
        IkSolverRuntimeStats {
            solver_evaluations: 240,
            configured_iterations: 4800,
            executed_iterations: 1200,
            tolerance_precheck_breaks: 12,
            tolerance_post_iteration_breaks: 34,
            rollback_breaks: 1,
            max_iteration_exhaustions: 2,
            link_visits: 0,
            link_steps: 3600,
            final_distance_sum: 1.5,
            final_distance_max: 0.01,
            exhausted_final_distance_sum: 0.2,
            exhausted_final_distance_max: 0.02,
        },
        IkSolverRuntimeStats {
            solver_evaluations: 120,
            configured_iterations: 2400,
            executed_iterations: 600,
            tolerance_precheck_breaks: 6,
            tolerance_post_iteration_breaks: 17,
            rollback_breaks: 0,
            max_iteration_exhaustions: 1,
            link_visits: 0,
            link_steps: 1800,
            final_distance_sum: 0.75,
            final_distance_max: 0.005,
            exhausted_final_distance_sum: 0.1,
            exhausted_final_distance_max: 0.01,
        },
    ];

    let report = bench::bench_pair_report_json(BenchPairReportInput {
        pmx_path: &pmx_path,
        vmd_path: &vmd_path,
        bone_count: 128,
        append_count: 4,
        fixed_axis_count: 2,
        solver_count: 2,
        morph_count: 16,
        vmd_bone_keys: 2400,
        vmd_morph_keys: 120,
        clip_bone_tracks: 64,
        clip_morph_tracks: 8,
        property_track: false,
        clip_frame_range: None,
        start_frame: 0.0,
        frame_count: 240,
        step: 1.0,
        instances: 1,
        total_evaluations: 240,
        solve_ik: true,
        ik_options: IkSolveOptions::default(),
        read_ms: 1.0,
        pmx_import_ms: 2.0,
        vmd_import_ms: 0.5,
        clip_build_ms: 3.0,
        eval_ms: 10.0,
        apply_pose_ms: 3.0,
        morph_expand_ms: 1.5,
        pose_eval_ms: 5.5,
        world_copy_ms: 1.0,
        skinning_copy_ms: 0.75,
        morph_copy_ms: 0.25,
        hot_loop_ms: 12.0,
        total_ms: 16.5,
        ms_per_frame: 0.05,
        fps: 20000.0,
        ms_per_evaluation: 0.05,
        evaluations_per_second: 20000.0,
        checksum: 0x00ff_00ff,
        morph_checksum: 0x0f0f_0f0f,
        ik_solver_summaries: &ik_solver_summaries,
        ik_stats: Some(&ik_stats),
    });

    assert_eq!(report["ik"]["aggregate"]["solverEvaluations"], 360);
    assert_eq!(report["ik"]["aggregate"]["executedIterations"], 1800);
    assert_eq!(report["ik"]["topSolvers"].as_array().unwrap().len(), 2);
    assert_eq!(report["ik"]["topSolvers"][0]["name"], "左足ＩＫ");
    assert_eq!(report["ik"]["topSolvers"][0]["executedIterations"], 1200);
}

#[test]
fn bench_pair_fixture_with_instances_completes_successfully() {
    let (pmx, vmd) = runtime_batch_fixture_paths();
    let cfg = bench::BenchPairConfig {
        pmx_path: pmx,
        vmd_path: vmd,
        start_frame: 0.0,
        frame_count: 3,
        step: 1.0,
        solve_ik: true,
        ik_options: mmd_anim_runtime::IkSolveOptions::default(),
        instances: 3,
        use_json: true,
    };

    let exit = bench::bench_pair(cfg).expect("fixture bench pair must complete");
    assert_eq!(exit, ExitCode::SUCCESS);
}

#[test]
fn bench_pair_aggregate_ik_runtime_stats_sums_all_instances() {
    use mmd_anim_runtime::IkSolverRuntimeStats;

    let first_runtime = [
        IkSolverRuntimeStats {
            solver_evaluations: 2,
            configured_iterations: 40,
            executed_iterations: 10,
            tolerance_precheck_breaks: 1,
            tolerance_post_iteration_breaks: 2,
            rollback_breaks: 1,
            max_iteration_exhaustions: 1,
            link_visits: 5,
            link_steps: 20,
            final_distance_sum: 0.5,
            final_distance_max: 0.02,
            exhausted_final_distance_sum: 0.25,
            exhausted_final_distance_max: 0.03,
        },
        IkSolverRuntimeStats {
            solver_evaluations: 3,
            configured_iterations: 60,
            executed_iterations: 30,
            final_distance_max: 0.04,
            ..IkSolverRuntimeStats::default()
        },
    ];
    let second_runtime = [
        IkSolverRuntimeStats {
            solver_evaluations: 7,
            configured_iterations: 140,
            executed_iterations: 70,
            tolerance_precheck_breaks: 3,
            tolerance_post_iteration_breaks: 4,
            rollback_breaks: 2,
            max_iteration_exhaustions: 2,
            link_visits: 15,
            link_steps: 80,
            final_distance_sum: 1.5,
            final_distance_max: 0.05,
            exhausted_final_distance_sum: 0.75,
            exhausted_final_distance_max: 0.01,
        },
        IkSolverRuntimeStats {
            solver_evaluations: 11,
            configured_iterations: 220,
            executed_iterations: 110,
            final_distance_max: 0.01,
            ..IkSolverRuntimeStats::default()
        },
    ];

    let aggregate = bench::aggregate_ik_runtime_stats([&first_runtime[..], &second_runtime[..]]);

    assert_eq!(aggregate.len(), 2);
    assert_eq!(aggregate[0].solver_evaluations, 9);
    assert_eq!(aggregate[0].configured_iterations, 180);
    assert_eq!(aggregate[0].executed_iterations, 80);
    assert_eq!(aggregate[0].tolerance_precheck_breaks, 4);
    assert_eq!(aggregate[0].tolerance_post_iteration_breaks, 6);
    assert_eq!(aggregate[0].rollback_breaks, 3);
    assert_eq!(aggregate[0].max_iteration_exhaustions, 3);
    assert_eq!(aggregate[0].link_visits, 20);
    assert_eq!(aggregate[0].link_steps, 100);
    assert_eq!(aggregate[0].final_distance_sum, 2.0);
    assert_eq!(aggregate[0].final_distance_max, 0.05);
    assert_eq!(aggregate[0].exhausted_final_distance_sum, 1.0);
    assert_eq!(aggregate[0].exhausted_final_distance_max, 0.03);
    assert_eq!(aggregate[1].solver_evaluations, 14);
    assert_eq!(aggregate[1].configured_iterations, 280);
    assert_eq!(aggregate[1].executed_iterations, 140);
    assert_eq!(aggregate[1].final_distance_max, 0.04);
}

const LOCAL_BENCH_PMX_ENV: &str = "MMD_ANIM_LOCAL_BENCH_PMX";
const LOCAL_BENCH_VMD_ENV: &str = "MMD_ANIM_LOCAL_BENCH_VMD";
const LOCAL_BENCH_INSTANCES_ENV: &str = "MMD_ANIM_LOCAL_BENCH_INSTANCES";

fn local_bench_pair_paths_from_env() -> Option<(PathBuf, PathBuf)> {
    let pmx = env::var(LOCAL_BENCH_PMX_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)?;
    let vmd = env::var(LOCAL_BENCH_VMD_ENV)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)?;
    Some((pmx, vmd))
}

fn local_bench_pair_instances_from_env() -> usize {
    env::var(LOCAL_BENCH_INSTANCES_ENV)
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|instances| *instances > 0)
        .unwrap_or(30)
}

#[test]
#[ignore = "local large-asset bench smoke; requires MMD_ANIM_LOCAL_BENCH_PMX and MMD_ANIM_LOCAL_BENCH_VMD"]
fn bench_pair_local_large_asset_smoke() {
    use mmd_anim_runtime::IkSolveOptions;

    let Some((pmx_path, vmd_path)) = local_bench_pair_paths_from_env() else {
        eprintln!(
            "skip bench_pair_local_large_asset_smoke: set {LOCAL_BENCH_PMX_ENV} and {LOCAL_BENCH_VMD_ENV}"
        );
        return;
    };

    assert!(
        pmx_path.is_file(),
        "{LOCAL_BENCH_PMX_ENV} must point to an existing PMX file: {}",
        pmx_path.display()
    );
    assert!(
        vmd_path.is_file(),
        "{LOCAL_BENCH_VMD_ENV} must point to an existing VMD file: {}",
        vmd_path.display()
    );

    let instances = local_bench_pair_instances_from_env();
    let cfg = bench::BenchPairConfig {
        pmx_path,
        vmd_path,
        start_frame: 0.0,
        frame_count: 30,
        step: 1.0,
        solve_ik: true,
        ik_options: IkSolveOptions::default(),
        instances,
        use_json: true,
    };

    let exit = bench::bench_pair(cfg).expect("local bench pair smoke must complete");
    assert_eq!(exit, ExitCode::SUCCESS);
}

fn runtime_batch_fixture_paths() -> (PathBuf, PathBuf) {
    let format_crate = Path::new(env!("CARGO_MANIFEST_DIR")).join("../mmd-anim-format");
    (
        format_crate.join("fixtures/pmx/ik_multi_axis_limit.pmx"),
        format_crate.join("fixtures/vmd/ik_multi_bone_nondefault.vmd"),
    )
}

#[test]
fn import_frames_list_parser_preserves_order() {
    let spec = import::parse_import_frames_list("0,30,120").unwrap();
    let import::ImportFrameSpec::List(frames) = spec else {
        panic!("expected list frame spec");
    };
    assert_eq!(frames, vec![0.0, 30.0, 120.0]);
}

#[test]
fn import_frame_range_parser_is_inclusive() {
    let spec = import::parse_import_frame_range("0:10:5").unwrap();
    let import::ImportFrameSpec::Range(frames) = spec else {
        panic!("expected range frame spec");
    };
    assert_eq!(frames, vec![0.0, 5.0, 10.0]);
}

#[test]
fn import_frame_range_parser_emits_decimal_end() {
    let spec = import::parse_import_frame_range("0:1:0.1").unwrap();
    let import::ImportFrameSpec::Range(frames) = spec else {
        panic!("expected range frame spec");
    };
    assert_eq!(frames.last().copied(), Some(1.0));
}

#[test]
fn import_frame_parsers_reject_invalid_values() {
    assert!(import::parse_import_frames_list("").is_err());
    assert!(import::parse_import_frames_list("0,,30").is_err());
    assert!(import::parse_import_frame_range("0:30").is_err());
    assert!(import::parse_import_frame_range("0:30:0").is_err());
    assert!(import::parse_import_frame_range("30:0:1").is_err());
}

#[test]
fn import_runtime_batch_report_preserves_requested_frame_order() {
    let (pmx, vmd) = runtime_batch_fixture_paths();
    let report = import::build_import_runtime_batch_report(
        &pmx,
        &vmd,
        import::ImportFrameSpec::List(vec![30.0, 0.0, 120.0]),
        false,
    )
    .unwrap();

    assert_eq!(report.per_frame.len(), 3);
    assert_eq!(report.per_frame[0].frame, 30.0);
    assert_eq!(report.per_frame[1].frame, 0.0);
    assert_eq!(report.per_frame[2].frame, 120.0);
    assert!(
        report
            .per_frame
            .iter()
            .all(|frame| frame.world_matrices > 0)
    );
}

#[test]
fn aggregate_import_ik_runtime_stats_sums_solver_metrics() {
    use import::ImportVerboseIkAggregate;
    use mmd_anim_runtime::IkSolverRuntimeStats;

    let stats = [
        IkSolverRuntimeStats {
            solver_evaluations: 2,
            configured_iterations: 40,
            executed_iterations: 10,
            tolerance_precheck_breaks: 1,
            tolerance_post_iteration_breaks: 2,
            rollback_breaks: 1,
            max_iteration_exhaustions: 1,
            link_visits: 5,
            link_steps: 20,
            ..IkSolverRuntimeStats::default()
        },
        IkSolverRuntimeStats {
            solver_evaluations: 1,
            configured_iterations: 20,
            executed_iterations: 5,
            tolerance_precheck_breaks: 2,
            tolerance_post_iteration_breaks: 1,
            rollback_breaks: 0,
            max_iteration_exhaustions: 0,
            link_visits: 3,
            link_steps: 10,
            ..IkSolverRuntimeStats::default()
        },
    ];

    let aggregate = import::aggregate_import_ik_runtime_stats(&stats);
    assert_eq!(
        aggregate,
        ImportVerboseIkAggregate {
            solver_evaluations: 3,
            configured_iterations: 60,
            executed_iterations: 15,
            skipped_iterations: 45,
            skipped_ratio: 0.75,
            tolerance_precheck_breaks: 3,
            tolerance_post_iteration_breaks: 3,
            rollback_breaks: 1,
            max_iteration_exhaustions: 1,
            link_visits: 8,
            link_steps: 30,
        }
    );
}

#[test]
fn import_pair_frame_verbose_lines_include_summary_and_result_fields() {
    let summary = import::ImportRuntimeBatchSummary {
        bones: 128,
        ik: 4,
        morph_slots: 16,
        clip_bone_tracks: 64,
        clip_morph_tracks: 8,
        property_track: true,
    };
    let eval = import::PairFrameEval {
        frame: 30.0,
        world_matrices: 128,
        first_translation: [1.0, 2.0, 3.0],
        translation_checksum: 0x1234_5678,
        nonzero_morphs: 2,
        morph_checksum: 0x9abc_def0,
        ik_enabled: vec![1, 0, 1],
        ik_enabled_count: 3,
    };
    let ik_stats = import::ImportVerboseIkAggregate {
        solver_evaluations: 4,
        configured_iterations: 80,
        executed_iterations: 20,
        skipped_iterations: 60,
        skipped_ratio: 0.75,
        tolerance_precheck_breaks: 1,
        tolerance_post_iteration_breaks: 2,
        rollback_breaks: 0,
        max_iteration_exhaustions: 1,
        link_visits: 12,
        link_steps: 40,
    };

    let append = import::ImportVerboseAppendDiagnostics {
        aggregate: import::ImportVerboseAppendAggregate {
            append_count: 0,
            rotation_affecting_count: 0,
            translation_affecting_count: 0,
            nonzero_position_outputs: 0,
            nonidentity_rotation_outputs: 0,
        },
        details: Vec::new(),
    };
    let lines = import::import_pair_frame_verbose_lines(&summary, &eval, ik_stats, append);
    assert_eq!(lines.len(), 6);
    assert!(lines[0].contains("frame=30.000"));
    assert!(lines[1].contains("bones=128"));
    assert!(lines[1].contains("propertyTrack=true"));
    assert!(lines[2].contains("ikEnabledCount=3"));
    assert!(lines[2].contains("ikEnabledActive=2"));
    assert!(lines[2].contains("ikEnabled=[1,0,1]"));
    assert!(lines[3].contains("solverEvaluations=4"));
    assert!(lines[3].contains("linkVisits=12"));
    assert!(lines[4].contains("append count=0"));
    assert!(lines[4].contains("nonidentityRotationOutputs=0"));
    assert!(lines[5].contains("translationChecksum=12345678"));
    assert!(lines[5].contains("firstTranslation=(1.000000,2.000000,3.000000)"));
}

#[test]
fn import_pair_frame_verbose_lines_include_append_descriptor_and_output_fields() {
    let summary = import::ImportRuntimeBatchSummary {
        bones: 3,
        ik: 0,
        morph_slots: 0,
        clip_bone_tracks: 2,
        clip_morph_tracks: 0,
        property_track: false,
    };
    let eval = import::PairFrameEval {
        frame: 0.0,
        world_matrices: 3,
        first_translation: [0.0, 0.0, 0.0],
        translation_checksum: 0,
        nonzero_morphs: 0,
        morph_checksum: 0,
        ik_enabled: Vec::new(),
        ik_enabled_count: 0,
    };
    let ik_stats = import::ImportVerboseIkAggregate {
        solver_evaluations: 0,
        configured_iterations: 0,
        executed_iterations: 0,
        skipped_iterations: 0,
        skipped_ratio: 0.0,
        tolerance_precheck_breaks: 0,
        tolerance_post_iteration_breaks: 0,
        rollback_breaks: 0,
        max_iteration_exhaustions: 0,
        link_visits: 0,
        link_steps: 0,
    };
    let append = import::ImportVerboseAppendDiagnostics {
        aggregate: import::ImportVerboseAppendAggregate {
            append_count: 1,
            rotation_affecting_count: 1,
            translation_affecting_count: 0,
            nonzero_position_outputs: 0,
            nonidentity_rotation_outputs: 1,
        },
        details: vec![import::ImportVerboseAppendDetail {
            append_index: 0,
            target_bone_index: 1,
            source_bone_index: 0,
            ratio: 1.0,
            affect_rotation: true,
            affect_translation: false,
            local: true,
            output_position: [0.0, 0.0, 0.0],
            output_rotation: [
                0.0,
                0.0,
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
            ],
        }],
    };

    let lines = import::import_pair_frame_verbose_lines(&summary, &eval, ik_stats, append);
    assert_eq!(lines.len(), 7);
    assert!(lines[4].contains("append count=1 rotationAffecting=1 translationAffecting=0"));
    assert!(lines[4].contains("nonidentityRotationOutputs=1"));
    assert!(lines[5].contains("append index=0 targetBoneIndex=1 sourceBoneIndex=0 ratio=1.000000"));
    assert!(lines[5].contains("affectRotation=true affectTranslation=false local=true"));
    assert!(lines[5].contains("outputPosition=(0.000000,0.000000,0.000000)"));
    assert!(lines[5].contains("outputRotation=(0.000000,0.000000,0.707107,0.707107)"));
    assert!(lines[6].contains("result worldMatrices=3"));
}

#[test]
fn collect_import_verbose_append_diagnostics_reports_evaluated_outputs() {
    use mmd_anim_runtime::{AppendTransformInit, BoneIndex, BoneInit, ModelArena, RuntimeInstance};
    use std::sync::Arc;

    let model = Arc::new(
        ModelArena::new_full(
            vec![
                BoneInit::new(None, glam::Vec3A::ZERO),
                BoneInit::new(None, glam::Vec3A::ZERO),
                BoneInit::new(Some(BoneIndex(1)), glam::Vec3A::X),
            ],
            Vec::new(),
            vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 1.0).with_rotation()],
        )
        .unwrap(),
    );
    let mut runtime = RuntimeInstance::new(model);
    runtime.pose_mut().set_local_rotation(
        BoneIndex(0),
        glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
    );
    runtime.evaluate_current_pose();

    let append = import::collect_import_verbose_append_diagnostics(&runtime);
    assert_eq!(append.aggregate.append_count, 1);
    assert_eq!(append.aggregate.rotation_affecting_count, 1);
    assert_eq!(append.aggregate.translation_affecting_count, 0);
    assert_eq!(append.aggregate.nonzero_position_outputs, 0);
    assert_eq!(append.aggregate.nonidentity_rotation_outputs, 1);
    assert_eq!(append.details.len(), 1);
    assert_eq!(append.details[0].append_index, 0);
    assert_eq!(append.details[0].target_bone_index, 1);
    assert_eq!(append.details[0].source_bone_index, 0);
    assert_eq!(append.details[0].ratio, 1.0);
    assert!(append.details[0].affect_rotation);
    assert!(!append.details[0].affect_translation);
    assert!(!append.details[0].local);
    assert_eq!(append.details[0].output_position, [0.0, 0.0, 0.0]);
    assert!(append.details[0].output_rotation[2].abs() > 0.5);
    assert!(append.details[0].output_rotation[3].abs() > 0.5);
}

#[test]
fn dispatch_import_verbose_rejects_unsupported_paths() {
    let model = Path::new("model.pmx");
    let motion = Path::new("motion.vmd");

    let verbose_options = || ImportDispatchOptions {
        use_json: false,
        show_clip: false,
        frame: None,
        frames: None,
        frame_range: None,
        verbose: true,
    };

    let model_only = dispatch_import(model, None, verbose_options()).unwrap();
    assert_eq!(model_only, ExitCode::from(2));

    let pair_without_frame = dispatch_import(model, Some(motion), verbose_options()).unwrap();
    assert_eq!(pair_without_frame, ExitCode::from(2));

    let clip = dispatch_import(
        model,
        Some(motion),
        ImportDispatchOptions {
            show_clip: true,
            ..verbose_options()
        },
    )
    .unwrap();
    assert_eq!(clip, ExitCode::from(2));

    let json_without_batch = dispatch_import(
        model,
        Some(motion),
        ImportDispatchOptions {
            use_json: true,
            ..verbose_options()
        },
    )
    .unwrap();
    assert_eq!(json_without_batch, ExitCode::from(2));

    let json_frame = dispatch_import(
        model,
        Some(motion),
        ImportDispatchOptions {
            use_json: true,
            frame: Some(0.0),
            ..verbose_options()
        },
    )
    .unwrap();
    assert_eq!(json_frame, ExitCode::from(2));
}

#[test]
fn dispatch_import_verbose_accepts_json_batch_paths() {
    let (pmx, vmd) = runtime_batch_fixture_paths();

    let frames_list = dispatch_import(
        &pmx,
        Some(&vmd),
        ImportDispatchOptions {
            use_json: true,
            frames: Some("0,30".to_owned()),
            verbose: true,
            show_clip: false,
            frame: None,
            frame_range: None,
        },
    )
    .unwrap();
    assert_eq!(frames_list, ExitCode::SUCCESS);

    let frame_range = dispatch_import(
        &pmx,
        Some(&vmd),
        ImportDispatchOptions {
            use_json: true,
            frame_range: Some("0:30:30".to_owned()),
            verbose: true,
            show_clip: false,
            frame: None,
            frames: None,
        },
    )
    .unwrap();
    assert_eq!(frame_range, ExitCode::SUCCESS);
}

#[test]
fn import_batch_verbose_resets_ik_stats_per_frame() {
    let (pmx, vmd) = runtime_batch_fixture_paths();
    let mut context = import::build_pair_runtime_context(&pmx, &vmd).unwrap();

    let (_, stats_frame0_alone) =
        import::evaluate_pair_frame_with_ik_stats(&mut context.runtime, &context.clip, 0.0, true);
    let (_, stats_frame120_alone) =
        import::evaluate_pair_frame_with_ik_stats(&mut context.runtime, &context.clip, 120.0, true);

    let (_, stats_frame0_batch) =
        import::evaluate_pair_frame_with_ik_stats(&mut context.runtime, &context.clip, 0.0, true);
    let (_, stats_frame120_batch) =
        import::evaluate_pair_frame_with_ik_stats(&mut context.runtime, &context.clip, 120.0, true);

    assert_eq!(stats_frame0_alone, stats_frame0_batch);
    assert_eq!(stats_frame120_alone, stats_frame120_batch);

    import::evaluate_pair_frame_with_ik_stats(&mut context.runtime, &context.clip, 0.0, true);
    let (_, stats_without_reset) = import::evaluate_pair_frame_with_ik_stats(
        &mut context.runtime,
        &context.clip,
        120.0,
        false,
    );
    assert_ne!(
        stats_without_reset.solver_evaluations,
        stats_frame120_batch.solver_evaluations
    );
}

#[test]
fn import_runtime_batch_report_json_remains_parseable_with_verbose() {
    let (pmx, vmd) = runtime_batch_fixture_paths();
    let report = import::build_import_runtime_batch_report(
        &pmx,
        &vmd,
        import::ImportFrameSpec::List(vec![0.0, 30.0]),
        true,
    )
    .unwrap();
    let json = serde_json::to_string_pretty(&report).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["kind"], "import-runtime-batch");
    assert_eq!(parsed["perFrame"].as_array().unwrap().len(), 2);
    assert!(!json.contains("import-verbose"));
}

#[test]
fn import_runtime_batch_single_frame_matches_range_frame() {
    let (pmx, vmd) = runtime_batch_fixture_paths();
    let list_report = import::build_import_runtime_batch_report(
        &pmx,
        &vmd,
        import::ImportFrameSpec::List(vec![120.0]),
        false,
    )
    .unwrap();
    let range_report = import::build_import_runtime_batch_report(
        &pmx,
        &vmd,
        import::parse_import_frame_range("120:120:1").unwrap(),
        false,
    )
    .unwrap();

    assert_eq!(list_report.per_frame[0], range_report.per_frame[0]);
    assert_eq!(
        list_report.per_frame[0].ik_enabled_count,
        list_report.per_frame[0].ik_enabled.len()
    );
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
fn numeric_compare_reports_no_targets_when_focus_bones_do_not_match() {
    let temp = unique_test_dir("compare-numeric-no-targets");
    fs::create_dir_all(&temp).unwrap();
    let (pmx, vmd) = runtime_batch_fixture_paths();
    let oracle = r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"test"},"frame":0.0,"models":[{"index":0,"name":"m","filename":"m.pmx","visible":true,"bones":[{"index":0,"name":"not-a-focus-bone","worldMatrix":[1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0]}],"morphs":[]}]}"#;
    fs::write(temp.join("oracle.actual.jsonl"), oracle).unwrap();
    fs::write(
        temp.join("manifest.json"),
        format!(
            r#"{{
                "cases": [
                    {{
                        "name": "no-targets",
                        "kind": "physics-coarse",
                        "assets": {{
                            "model": "{}",
                            "motion": "{}"
                        }},
                        "oracle": {{ "path": "oracle.actual.jsonl" }},
                        "frames": [0],
                        "metadata": {{ "focus": {{ "bones": ["missing-focus"] }} }},
                        "compare": {{ "targets": ["bones"], "epsilon": 0.003 }}
                    }}
                ]
            }}"#,
            pmx.display().to_string().replace('\\', "\\\\"),
            vmd.display().to_string().replace('\\', "\\\\")
        ),
    )
    .unwrap();

    let report = compare::build_numeric_compare_report(&temp.join("manifest.json"), false)
        .unwrap()
        .to_json();

    assert_eq!(report["summary"]["motionNoTargets"], 1);
    assert_eq!(report["summary"]["mismatchCount"], 0);
    assert_eq!(report["perCase"][0]["status"], "no-targets");
    assert_eq!(
        report["perCase"][0]["physicsBackend"],
        if cfg!(feature = "physics-bullet-native") {
            "bullet-native"
        } else {
            "none"
        }
    );
    assert_eq!(report["perCase"][0]["noTargets"], 1);
    assert_eq!(report["perCase"][0]["comparedBones"], 0);

    let error = compare::compare_numeric_manifest(&temp.join("manifest.json"))
        .unwrap_err()
        .to_string();
    assert!(error.contains("motionNoTargets=1"));

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn numeric_compare_accepts_unity_runtime_verification_oracle_format() {
    let temp = unique_test_dir("compare-numeric-unity-oracle");
    fs::create_dir_all(&temp).unwrap();
    let (pmx, vmd) = runtime_batch_fixture_paths();
    let oracle = format!(
        r#"{{
        "schemaVersion": 1,
        "unityVersion": "6000.4.8f1",
        "caseResults": [
            {{
                "name": "unity-case",
                "pmxPath": "{}",
                "sampledFrames": [
                    {{
                        "frame": 0,
                        "bones": [
                            {{
                                "index": 0,
                                "name": "not-a-focus-bone",
                                "worldMatrix": [
                                    1.0, 0.0, 0.0, 7.0,
                                    0.0, 1.0, 0.0, 8.0,
                                    0.0, 0.0, 1.0, 9.0,
                                    0.0, 0.0, 0.0, 1.0
                                ]
                            }}
                        ]
                    }}
                ]
            }}
        ]
    }}"#,
        pmx.display().to_string().replace('\\', "\\\\")
    );
    fs::write(temp.join("unity-oracle.json"), oracle).unwrap();
    fs::write(
        temp.join("manifest.json"),
        format!(
            r#"{{
                "cases": [
                    {{
                        "name": "unity-no-targets",
                        "kind": "physics-coarse",
                        "assets": {{
                            "model": "{}",
                            "motion": "{}"
                        }},
                        "oracle": {{
                            "path": "unity-oracle.json",
                            "format": "unity-runtime-verification"
                        }},
                        "frames": [0],
                        "metadata": {{ "focus": {{ "bones": ["missing-focus"] }} }},
                        "compare": {{ "targets": ["bones"], "epsilon": 0.003 }}
                    }}
                ]
            }}"#,
            pmx.display().to_string().replace('\\', "\\\\"),
            vmd.display().to_string().replace('\\', "\\\\")
        ),
    )
    .unwrap();

    let report = compare::build_numeric_compare_report(&temp.join("manifest.json"), false)
        .unwrap()
        .to_json();

    assert_eq!(report["perCase"][0]["status"], "no-targets");
    assert_eq!(report["perCase"][0]["comparedBones"], 0);

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
        translation_max_error: 0.75,
        translation_error_sum_sq: 0.25,
        translation_error_count: 1,
        worst_translation_frame: Some(31),
        worst_translation_bone: "左足".to_owned(),
        worst_translation_axis: Some(1),
        rotation_max_angle_rad: 0.5,
        rotation_angle_sum_sq: 0.04,
        rotation_angle_count: 1,
        worst_rotation_frame: Some(32),
        worst_rotation_bone: "左足".to_owned(),
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
    assert_eq!(value["summary"]["motionNoTargets"], 0);
    assert_eq!(value["summary"]["maxAbsError"], 1.25);
    assert_eq!(value["summary"]["motionTranslationMaxError"], 0.75);
    assert_eq!(value["summary"]["motionTranslationRmsError"], 0.5);
    assert_eq!(value["summary"]["motionWorstTranslationFrame"], 31);
    assert_eq!(value["summary"]["motionWorstTranslationBone"], "左足");
    assert_eq!(value["summary"]["motionWorstTranslationAxis"], 1);
    assert_eq!(value["summary"]["motionRotationMaxAngleRad"], 0.5);
    assert_eq!(value["summary"]["motionRotationRmsAngleRad"], 0.2);
    assert_eq!(value["summary"]["motionWorstRotationFrame"], 32);
    assert_eq!(value["summary"]["motionWorstRotationBone"], "左足");
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
fn verify_numeric_json_rejects_diagnose() {
    let target = Path::new("manifest.json");
    let numeric = dispatch_verify(
        target,
        Some(VerifyMode::Numeric),
        Some(vec!["case".to_owned(), "0".to_owned(), "bone".to_owned()]),
        false,
        false,
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(numeric, ExitCode::from(2));
}

#[test]
fn verify_numeric_json_rejects_physics_penetration_without_diagnose() {
    let target = Path::new("manifest.json");
    let numeric = dispatch_verify(
        target,
        Some(VerifyMode::Numeric),
        None,
        true,
        false,
        true,
        None,
        None,
    )
    .unwrap();
    assert_eq!(numeric, ExitCode::from(2));
}

#[test]
fn verify_camera_json_uses_numeric_compare_report() {
    let temp = unique_test_dir("verify-camera-json");
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
    let manifest_path = temp.join("manifest.json");
    fs::write(
        &manifest_path,
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

    let code = dispatch_verify(
        &manifest_path,
        Some(VerifyMode::Camera),
        None,
        false,
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
fn oracle_summary_json_report_includes_core_fields() {
    let jsonl = r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"1.0.0"},"frame":0.0,"models":[{"index":0,"name":"test_model","filename":"test.pmx","visible":true,"bones":[{"index":0,"name":"センター","worldMatrix":[1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0]}],"morphs":[{"index":0,"name":"まばたき","weight":0.5}]}]}"#;
    let dump = MmdDumperOracleDump::from_jsonl_str(jsonl, None).unwrap();
    let report = oracle::oracle_summary_json_report("oracle.jsonl", &dump);
    let value = serde_json::to_value(report).unwrap();

    assert_eq!(value["status"], "ok");
    assert_eq!(value["command"], "verify");
    assert_eq!(value["mode"], "oracle");
    assert_eq!(value["input"], "oracle.jsonl");
    assert_eq!(value["frames"], 1);
    assert_eq!(value["models"], 1);
    assert_eq!(value["firstModelBones"], 1);
    assert_eq!(value["firstModelMorphs"], 1);
    assert_eq!(value["source"]["mmdVersion"], "9.32-x64");
    assert_eq!(value["source"]["dumperVersion"], "1.0.0");
}

#[test]
fn verify_oracle_json_dispatch_accepts_modeless_summary() {
    let temp = unique_test_dir("verify-oracle-json");
    fs::create_dir_all(&temp).unwrap();
    let oracle_path = temp.join("oracle.jsonl");
    fs::write(
        &oracle_path,
        r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"1.0.0"},"frame":0.0,"models":[{"index":0,"name":"test_model","filename":"test.pmx","visible":true,"bones":[{"index":0,"name":"センター","worldMatrix":[1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0]}],"morphs":[]}]}"#,
    )
    .unwrap();

    let code = dispatch_verify(&oracle_path, None, None, false, false, true, None, None).unwrap();
    assert_eq!(code, ExitCode::SUCCESS);

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn golden_parser_summary_json_report_includes_core_fields() {
    let root = Path::new("golden-root");
    let metrics = golden::GoldenParserSummaryMetrics {
        cases: 2,
        skipped_unsupported: 1,
        matched_bones: 10,
        missing_bones: 1,
        matched_morphs: 4,
        missing_morphs: 0,
    };
    let report = golden::golden_parser_summary_json_report(root, metrics);
    let value = serde_json::to_value(report).unwrap();

    assert_eq!(value["status"], "ok");
    assert_eq!(value["command"], "verify");
    assert_eq!(value["mode"], "parser");
    assert_eq!(value["root"], "golden-root");
    assert_eq!(value["cases"], 2);
    assert_eq!(value["skippedUnsupported"], 1);
    assert_eq!(value["matchedBones"], 10);
    assert_eq!(value["missingBones"], 1);
    assert_eq!(value["matchedMorphs"], 4);
    assert_eq!(value["missingMorphs"], 0);
}

#[test]
fn diagnose_numeric_bone_rest_parses_bone_names_and_eval_frame() {
    let options = compare::parse_diagnose_numeric_bone_rest(
        vec![
            "センター".to_owned(),
            "--eval-frame".to_owned(),
            "12.5".to_owned(),
            "左足ＩＫ".to_owned(),
        ],
        0.0,
    )
    .unwrap();
    assert_f32_near(options.eval_frame, 12.5);
    assert_eq!(options.bone_names, vec!["センター", "左足ＩＫ"]);
}

#[test]
fn diagnose_numeric_bone_rest_rejects_bad_flags_as_errors() {
    let missing_value = compare::parse_diagnose_numeric_bone_rest(
        vec!["bone".to_owned(), "--eval-frame".to_owned()],
        0.0,
    )
    .unwrap_err();
    assert!(missing_value.contains("--eval-frame"), "{missing_value}");

    let invalid_value = compare::parse_diagnose_numeric_bone_rest(
        vec!["--eval-frame".to_owned(), "abc".to_owned()],
        0.0,
    )
    .unwrap_err();
    assert!(invalid_value.contains("abc"), "{invalid_value}");

    let unknown_flag =
        compare::parse_diagnose_numeric_bone_rest(vec!["--bogus".to_owned()], 0.0).unwrap_err();
    assert!(unknown_flag.contains("--bogus"), "{unknown_flag}");
}

#[test]
fn dispatch_numeric_diagnose_reports_usage_error_exit_code() {
    let exit = dispatch_numeric_diagnose(
        Path::new("manifest.json"),
        vec!["case".to_owned(), "0".to_owned(), "--bogus".to_owned()],
        None,
        false,
        false,
    )
    .unwrap();
    assert_eq!(exit, ExitCode::from(2));
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

fn full_vmd_dto_for_cli_export() -> mmd_anim_format::VmdParsedAnimation {
    mmd_anim_format::VmdParsedAnimation {
        kind: "vmd",
        metadata: mmd_anim_format::vmd::VmdParsedMetadata {
            format: "vmd",
            model_name: "cli-dto".to_owned(),
            model_name_bytes: Vec::new(),
            counts: mmd_anim_format::vmd::VmdParsedCounts {
                bones: 1,
                morphs: 1,
                cameras: 1,
                lights: 1,
                self_shadows: 1,
                properties: 1,
            },
            max_frame: 60,
        },
        bone_frames: vec![mmd_anim_format::vmd::VmdParsedBoneFrame {
            bone_name: "center".to_owned(),
            bone_name_bytes: Vec::new(),
            frame: 10,
            translation: [1.0, 2.0, 3.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            interpolation: vec![20; 64],
        }],
        morph_frames: vec![mmd_anim_format::vmd::VmdParsedMorphFrame {
            morph_name: "smile".to_owned(),
            morph_name_bytes: Vec::new(),
            frame: 20,
            weight: 0.75,
        }],
        camera_frames: vec![mmd_anim_format::vmd::VmdParsedCameraFrame {
            frame: 30,
            distance: -35.0,
            position: [0.0, 10.0, 5.0],
            rotation: [0.1, 0.2, 0.3],
            interpolation: [20; 24],
            fov: 42,
            perspective: true,
        }],
        light_frames: vec![mmd_anim_format::vmd::VmdParsedLightFrame {
            frame: 40,
            color: [1.0, 0.8, 0.6],
            direction: [0.0, -1.0, 0.0],
        }],
        self_shadow_frames: vec![mmd_anim_format::vmd::VmdParsedSelfShadowFrame {
            frame: 50,
            mode: 1,
            distance: 35.0,
        }],
        property_frames: vec![mmd_anim_format::vmd::VmdParsedPropertyFrame {
            frame: 60,
            visible: true,
            ik_states: vec![mmd_anim_format::vmd::VmdParsedIkState {
                bone_name: "legIK".to_owned(),
                bone_name_bytes: Vec::new(),
                enabled: false,
            }],
        }],
    }
}

#[test]
fn export_json_format_writes_full_vmd_dto_workflow() {
    let temp = unique_test_dir("vmd-dto-export");
    fs::create_dir_all(&temp).unwrap();
    let input = temp.join("motion-dto.json");
    let output = temp.join("motion.vmd");
    fs::write(
        &input,
        serde_json::to_string_pretty(&full_vmd_dto_for_cli_export()).unwrap(),
    )
    .unwrap();

    export::export_json_format(&input, &output, false).unwrap();
    export::export_roundtrip_summary(&output).unwrap();

    let reparsed = mmd_anim_format::parse_vmd_animation(&fs::read(&output).unwrap()).unwrap();
    assert_eq!(reparsed.metadata.model_name, "cli-dto");
    assert_eq!(reparsed.metadata.counts.bones, 1);
    assert_eq!(reparsed.metadata.counts.morphs, 1);
    assert_eq!(reparsed.metadata.counts.cameras, 1);
    assert_eq!(reparsed.metadata.counts.lights, 1);
    assert_eq!(reparsed.metadata.counts.self_shadows, 1);
    assert_eq!(reparsed.metadata.counts.properties, 1);
    assert_eq!(reparsed.bone_frames[0].bone_name, "center");
    assert_eq!(reparsed.morph_frames[0].morph_name, "smile");
    assert_eq!(reparsed.camera_frames[0].fov, 42);
    assert_eq!(reparsed.light_frames[0].color, [1.0, 0.8, 0.6]);
    assert_eq!(reparsed.self_shadow_frames[0].mode, 1);
    assert_eq!(reparsed.property_frames[0].ik_states[0].bone_name, "legIK");
    assert!(!reparsed.property_frames[0].ik_states[0].enabled);

    fs::remove_dir_all(temp).unwrap();
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

fn minimal_pmx_parts_manifest() -> serde_json::Value {
    serde_json::json!({
        "name": "cli-parts",
        "englishName": "cli-parts-en",
        "comment": "built from CLI parts manifest",
        "encoding": "utf-8",
        "indexSizes": {
            "vertex": 1,
            "texture": 1,
            "material": 1,
            "bone": 1,
            "morph": 1,
            "rigidBody": 1
        },
        "materialName": "default-mat",
        "positionsXyz": [
            0.0, 0.0, 0.0,
            1.0, 0.0, 0.0,
            0.0, 1.0, 0.0
        ],
        "normalsXyz": [
            0.0, 0.0, 1.0,
            0.0, 0.0, 1.0,
            0.0, 0.0, 1.0
        ],
        "uvsXy": [
            0.0, 0.0,
            1.0, 0.0,
            0.0, 1.0
        ],
        "indices": [0, 1, 2]
    })
}

#[test]
fn export_pmx_from_parts_manifest_writes_parseable_pmx() {
    let temp = unique_test_dir("pmx-parts-export");
    fs::create_dir_all(&temp).unwrap();
    let input = temp.join("parts.json");
    let output = temp.join("model.pmx");
    fs::write(
        &input,
        serde_json::to_string_pretty(&minimal_pmx_parts_manifest()).unwrap(),
    )
    .unwrap();

    export::export_pmx_from_parts_manifest(&input, &output, false).unwrap();

    let data = fs::read(&output).unwrap();
    let parsed = mmd_anim_format::parse_pmx_model(&data).unwrap();
    assert_eq!(parsed.metadata.name, "cli-parts");
    assert_eq!(parsed.metadata.english_name, "cli-parts-en");
    assert_eq!(parsed.metadata.counts.vertices, 3);
    assert_eq!(parsed.metadata.counts.faces, 1);
    assert_eq!(parsed.metadata.counts.materials, 1);
    assert_eq!(parsed.metadata.counts.bones, 1);

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn export_pmx_from_parts_manifest_rejects_normals_stride_mismatch() {
    let temp = unique_test_dir("pmx-parts-bad-normals");
    fs::create_dir_all(&temp).unwrap();
    let input = temp.join("parts.json");
    let output = temp.join("model.pmx");
    let mut manifest = minimal_pmx_parts_manifest();
    manifest["normalsXyz"] = serde_json::json!([0.0, 0.0, 1.0]);
    fs::write(&input, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

    let error = export::export_pmx_from_parts_manifest(&input, &output, false).unwrap_err();
    assert!(error.to_string().contains("normals_xyz"));
    assert!(!output.exists());

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn export_pmx_from_parts_manifest_rejects_partial_skinning() {
    let temp = unique_test_dir("pmx-parts-partial-skin");
    fs::create_dir_all(&temp).unwrap();
    let input = temp.join("parts.json");
    let output = temp.join("model.pmx");
    let mut manifest = minimal_pmx_parts_manifest();
    manifest["skinIndices"] = serde_json::json!([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    fs::write(&input, serde_json::to_string_pretty(&manifest).unwrap()).unwrap();

    let error = export::export_pmx_from_parts_manifest(&input, &output, false).unwrap_err();
    assert!(error.to_string().contains("skin_indices and skin_weights"));
    assert!(!output.exists());

    fs::remove_dir_all(temp).unwrap();
}

fn light_self_shadow_vmd_bytes() -> Vec<u8> {
    let parsed = mmd_anim_format::VmdParsedAnimation {
        kind: "vmd",
        metadata: mmd_anim_format::vmd::VmdParsedMetadata {
            format: "vmd",
            model_name: "sample".to_owned(),
            model_name_bytes: Vec::new(),
            counts: mmd_anim_format::vmd::VmdParsedCounts {
                bones: 0,
                morphs: 0,
                cameras: 0,
                lights: 2,
                self_shadows: 2,
                properties: 0,
            },
            max_frame: 30,
        },
        bone_frames: Vec::new(),
        morph_frames: Vec::new(),
        camera_frames: Vec::new(),
        light_frames: vec![
            mmd_anim_format::vmd::VmdParsedLightFrame {
                frame: 30,
                color: [1.0, 0.5, 0.0],
                direction: [0.0, -1.0, 0.0],
            },
            mmd_anim_format::vmd::VmdParsedLightFrame {
                frame: 10,
                color: [0.0, 0.0, 1.0],
                direction: [1.0, 0.0, 0.0],
            },
        ],
        self_shadow_frames: vec![
            mmd_anim_format::vmd::VmdParsedSelfShadowFrame {
                frame: 10,
                mode: 1,
                distance: 20.0,
            },
            mmd_anim_format::vmd::VmdParsedSelfShadowFrame {
                frame: 30,
                mode: 2,
                distance: 60.0,
            },
        ],
        property_frames: Vec::new(),
    };
    mmd_anim_format::export_vmd_animation(&parsed)
}

#[test]
fn vmd_sample_camera_matches_fixture_sampler() {
    let data = include_bytes!("../../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
    let state =
        vmd_sample::sample_vmd_bytes(data, vmd_sample::VmdSampleKind::Camera, 22.5).unwrap();

    let vmd_sample::VmdSampleState::Camera(camera) = state else {
        panic!("expected camera sample");
    };
    assert_f32_near(camera.distance, -40.25);
    assert_array3_near(camera.position, [-0.25, 6.0, 1.625]);
    assert_array3_near(camera.rotation, [-0.1, -0.1, 0.75]);
    assert_f32_near(camera.fov, 47.5);
    assert!(camera.perspective);
}

#[test]
fn vmd_sample_light_matches_exported_fixture_sampler() {
    let data = light_self_shadow_vmd_bytes();
    let state =
        vmd_sample::sample_vmd_bytes(&data, vmd_sample::VmdSampleKind::Light, 20.0).unwrap();

    let vmd_sample::VmdSampleState::Light(light) = state else {
        panic!("expected light sample");
    };
    assert_array3_near(light.color, [0.5, 0.25, 0.5]);
    assert_array3_near(light.direction, [0.5, -0.5, 0.0]);
}

#[test]
fn vmd_sample_self_shadow_matches_exported_fixture_sampler() {
    let data = light_self_shadow_vmd_bytes();
    let state =
        vmd_sample::sample_vmd_bytes(&data, vmd_sample::VmdSampleKind::SelfShadow, 20.0).unwrap();

    let vmd_sample::VmdSampleState::SelfShadow(self_shadow) = state else {
        panic!("expected self-shadow sample");
    };
    assert_eq!(self_shadow.mode, 1);
    assert_f32_near(self_shadow.distance, 40.0);
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
        msg.contains("roundtrip:") && msg.contains("PMM"),
        "expected roundtrip error mentioning PMM for json roundtrip, got: {}",
        msg
    );
}

#[test]
fn parse_format_summary_reports_unsupported_format_context() {
    let dir = unique_test_dir("unsupported-format");
    fs::create_dir_all(&dir).expect("temp dir must be creatable");
    let path = dir.join("mystery.bin");
    fs::write(&path, b"not an mmd asset").expect("temp file must be writable");

    let error = parse::parse_format_summary(&path).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("inspect:"), "{message}");
    assert!(message.contains("detected=unknown"), "{message}");
    assert!(message.contains("mystery.bin"), "{message}");
}

#[test]
fn import_pmx_summary_reports_import_failure_context() {
    let dir = unique_test_dir("import-failure");
    fs::create_dir_all(&dir).expect("temp dir must be creatable");
    let path = dir.join("broken.pmx");
    fs::write(&path, b"PMX broken header").expect("temp file must be writable");

    let error = import::import_pmx_summary(&path).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("import:"), "{message}");
    assert!(message.contains("failed to import PMX file"), "{message}");
    assert!(message.contains("broken.pmx"), "{message}");
}

#[test]
fn patch_document_model_path_json_reports_machine_readable_fields() {
    let value = patch::patch_document_model_path_json(
        Path::new("scene.pmm"),
        Path::new("out.pmm"),
        0,
        "model.pmx",
        1000,
        1000,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["command"], "patch");
    assert_eq!(value["mode"], "document-model-path");
    assert_eq!(value["bytesIn"], 1000);
    assert_eq!(value["bytesOut"], 1000);
    assert_eq!(value["documentModelIndex"], 0);
    assert_eq!(value["modelPath"], "model.pmx");
}

#[test]
fn patch_scene_frame_range_json_reports_changed_fields() {
    let patch = mmd_anim_format::pmm::PmmSceneFrameRangePatch {
        current_frame_index: Some(99),
        begin_frame_index_enabled: Some(true),
        ..Default::default()
    };
    let value = patch::patch_scene_frame_range_json(
        Path::new("scene.pmm"),
        Path::new("out.pmm"),
        &patch,
        2048,
        2048,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["command"], "patch");
    assert_eq!(value["mode"], "scene-frame-range");
    assert_eq!(value["changedFields"], 2);
    assert_eq!(value["patch"]["currentFrame"], 99);
    assert_eq!(value["patch"]["beginFrameEnabled"], true);
}

#[test]
fn export_format_json_reports_machine_readable_counts() {
    let value = export::export_format_json(
        Path::new("motion.vmd"),
        Path::new("out.vmd"),
        "VMD",
        512,
        480,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["command"], "export");
    assert_eq!(value["mode"], "parse-export");
    assert_eq!(value["format"], "vmd");
    assert_eq!(value["bytesIn"], 512);
    assert_eq!(value["bytesOut"], 480);
}

#[test]
fn build_pmx_from_parts_json_reports_machine_readable_counts() {
    let counts = mmd_anim_format::pmx::PmxParsedCounts {
        vertices: 3,
        faces: 1,
        materials: 1,
        bones: 1,
        morphs: 0,
        display_frames: 0,
        rigid_bodies: 0,
        joints: 0,
        soft_bodies: 0,
    };
    let value = export::build_pmx_from_parts_json(
        Path::new("parts.json"),
        Path::new("model.pmx"),
        256,
        1024,
        &counts,
    );

    assert_eq!(value["status"], "ok");
    assert_eq!(value["command"], "build-pmx");
    assert_eq!(value["mode"], "parts-manifest");
    assert_eq!(value["jsonBytes"], 256);
    assert_eq!(value["bytesOut"], 1024);
    assert_eq!(value["counts"]["vertices"], 3);
    assert_eq!(value["counts"]["bones"], 1);
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
    let result =
        patch::patch_pmm_document_model_path(&pmm_path, "0", replacement, &out_path, false);
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
    let result = patch::patch_pmm_scene_frame_range(&pmm_path, &out_path, &options, false);
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
