use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Instant};

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, BoneInit, IkSolveOptions, IkSolverRuntimeStats,
    ModelArena, MovableBoneKeyframe, MovableBoneTrack, RuntimeInstance,
};
use serde_json::json;

use crate::{copy_world_matrices_to_f32, f32_checksum, read_file, translation_checksum};

pub(crate) const BENCH_PAIR_USAGE: &str = "usage: mmd-anim bench-pair <model.pmx> <motion.vmd> [start-frame] [frame-count] [step] [--instances <count>] [--no-ik] [--ik-tolerance <value>] [--ik-max-iterations-cap <count>] [--json]";

#[derive(Debug)]
pub(crate) struct BenchPairConfig {
    pub(crate) pmx_path: PathBuf,
    pub(crate) vmd_path: PathBuf,
    pub(crate) start_frame: f32,
    pub(crate) frame_count: usize,
    pub(crate) step: f32,
    pub(crate) solve_ik: bool,
    pub(crate) ik_options: IkSolveOptions,
    pub(crate) instances: usize,
    pub(crate) use_json: bool,
}

#[derive(Debug)]
pub(crate) struct BenchPairIkSolverSummary {
    pub solver_index: usize,
    pub bone_index: usize,
    pub name: String,
    pub max_iterations: u32,
    pub links: usize,
}

#[derive(Debug)]
pub(crate) struct BenchPairReportInput<'a> {
    pub pmx_path: &'a PathBuf,
    pub vmd_path: &'a PathBuf,
    pub bone_count: usize,
    pub append_count: usize,
    pub fixed_axis_count: usize,
    pub solver_count: usize,
    pub morph_count: usize,
    pub vmd_bone_keys: usize,
    pub vmd_morph_keys: usize,
    pub clip_bone_tracks: usize,
    pub clip_morph_tracks: usize,
    pub property_track: bool,
    pub clip_frame_range: Option<(u32, u32)>,
    pub start_frame: f32,
    pub frame_count: usize,
    pub step: f32,
    pub instances: usize,
    pub total_evaluations: u64,
    pub solve_ik: bool,
    pub ik_options: IkSolveOptions,
    pub read_ms: f64,
    pub pmx_import_ms: f64,
    pub vmd_import_ms: f64,
    pub clip_build_ms: f64,
    pub eval_ms: f64,
    pub apply_pose_ms: f64,
    pub morph_expand_ms: f64,
    pub pose_eval_ms: f64,
    pub world_copy_ms: f64,
    pub skinning_copy_ms: f64,
    pub morph_copy_ms: f64,
    pub hot_loop_ms: f64,
    pub total_ms: f64,
    pub ms_per_frame: f64,
    pub fps: f64,
    pub ms_per_evaluation: f64,
    pub evaluations_per_second: f64,
    pub checksum: u32,
    pub morph_checksum: u32,
    pub ik_solver_summaries: &'a [BenchPairIkSolverSummary],
    pub ik_stats: Option<&'a [IkSolverRuntimeStats]>,
}

pub(crate) fn bench_pair_report_json(input: BenchPairReportInput<'_>) -> serde_json::Value {
    let clip_frame_range = input
        .clip_frame_range
        .map(|(first, last)| format!("{first}..{last}"));

    let mut root = json!({
        "status": "ok",
        "command": "bench",
        "mode": "pair",
        "model": input.pmx_path.display().to_string(),
        "motion": input.vmd_path.display().to_string(),
        "counts": {
            "bones": input.bone_count,
            "append": input.append_count,
            "fixedAxis": input.fixed_axis_count,
            "ikSolvers": input.solver_count,
            "morphs": input.morph_count,
            "vmdBoneKeys": input.vmd_bone_keys,
            "vmdMorphKeys": input.vmd_morph_keys,
            "clipBoneTracks": input.clip_bone_tracks,
            "clipMorphTracks": input.clip_morph_tracks,
            "propertyTrack": input.property_track,
        },
        "config": {
            "startFrame": input.start_frame,
            "frameCount": input.frame_count,
            "step": input.step,
            "instances": input.instances,
            "totalEvaluations": input.total_evaluations,
            "solveIk": input.solve_ik,
            "ikTolerance": input.ik_options.tolerance,
            "ikMaxIterationsCap": input.ik_options.max_iterations_cap,
        },
        "timing": {
            "readMs": input.read_ms,
            "pmxImportMs": input.pmx_import_ms,
            "vmdImportMs": input.vmd_import_ms,
            "clipBuildMs": input.clip_build_ms,
            "evalMs": input.eval_ms,
            "applyPoseMs": input.apply_pose_ms,
            "morphExpandMs": input.morph_expand_ms,
            "poseEvalMs": input.pose_eval_ms,
            "worldCopyMs": input.world_copy_ms,
            "skinningCopyMs": input.skinning_copy_ms,
            "morphCopyMs": input.morph_copy_ms,
            "hotLoopMs": input.hot_loop_ms,
            "totalMs": input.total_ms,
            "msPerFrame": input.ms_per_frame,
            "fps": input.fps,
            "msPerEvaluation": input.ms_per_evaluation,
            "evaluationsPerSecond": input.evaluations_per_second,
        },
        "result": {
            "checksum": format!("{:08x}", input.checksum),
            "morphChecksum": format!("{:08x}", input.morph_checksum),
            "clipFrameRange": clip_frame_range,
        },
    });

    if let Some(stats) = input.ik_stats {
        let total_evaluations = stats
            .iter()
            .map(|stats| stats.solver_evaluations)
            .sum::<u64>();
        let configured_iterations = stats
            .iter()
            .map(|stats| stats.configured_iterations)
            .sum::<u64>();
        let executed_iterations = stats
            .iter()
            .map(|stats| stats.executed_iterations)
            .sum::<u64>();
        let skipped_iterations = configured_iterations.saturating_sub(executed_iterations);
        let tolerance_precheck_breaks = stats
            .iter()
            .map(|stats| stats.tolerance_precheck_breaks)
            .sum::<u64>();
        let tolerance_post_iteration_breaks = stats
            .iter()
            .map(|stats| stats.tolerance_post_iteration_breaks)
            .sum::<u64>();
        let rollback_breaks = stats.iter().map(|stats| stats.rollback_breaks).sum::<u64>();
        let max_iteration_exhaustions = stats
            .iter()
            .map(|stats| stats.max_iteration_exhaustions)
            .sum::<u64>();
        let link_steps = stats.iter().map(|stats| stats.link_steps).sum::<u64>();
        let skip_ratio = if configured_iterations == 0 {
            0.0
        } else {
            skipped_iterations as f64 / configured_iterations as f64
        };

        let mut ranked = stats
            .iter()
            .enumerate()
            .map(|(index, stats)| (index, *stats))
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(_, stats)| {
            std::cmp::Reverse((stats.executed_iterations, stats.configured_iterations))
        });

        let top_solvers = ranked
            .into_iter()
            .take(8)
            .map(|(index, stats)| {
                let summary = &input.ik_solver_summaries[index];
                let skipped = stats
                    .configured_iterations
                    .saturating_sub(stats.executed_iterations);
                let avg_final_distance = if stats.solver_evaluations == 0 {
                    0.0
                } else {
                    stats.final_distance_sum / stats.solver_evaluations as f64
                };
                let avg_exhausted_final_distance = if stats.max_iteration_exhaustions == 0 {
                    0.0
                } else {
                    stats.exhausted_final_distance_sum / stats.max_iteration_exhaustions as f64
                };
                json!({
                    "solver": summary.solver_index,
                    "bone": summary.bone_index,
                    "name": summary.name,
                    "maxIterations": summary.max_iterations,
                    "links": summary.links,
                    "evaluations": stats.solver_evaluations,
                    "configuredIterations": stats.configured_iterations,
                    "executedIterations": stats.executed_iterations,
                    "skippedIterations": skipped,
                    "precheckBreaks": stats.tolerance_precheck_breaks,
                    "postBreaks": stats.tolerance_post_iteration_breaks,
                    "rollbackBreaks": stats.rollback_breaks,
                    "exhausted": stats.max_iteration_exhaustions,
                    "avgFinalDistance": avg_final_distance,
                    "maxFinalDistance": stats.final_distance_max,
                    "avgExhaustedFinalDistance": avg_exhausted_final_distance,
                    "maxExhaustedFinalDistance": stats.exhausted_final_distance_max,
                })
            })
            .collect::<Vec<_>>();

        root["ik"] = json!({
            "aggregate": {
                "solverEvaluations": total_evaluations,
                "configuredIterations": configured_iterations,
                "executedIterations": executed_iterations,
                "skippedIterations": skipped_iterations,
                "skippedRatio": skip_ratio,
                "tolerancePrecheckBreaks": tolerance_precheck_breaks,
                "tolerancePostIterationBreaks": tolerance_post_iteration_breaks,
                "rollbackBreaks": rollback_breaks,
                "maxIterationExhaustions": max_iteration_exhaustions,
                "linkSteps": link_steps,
            },
            "topSolvers": top_solvers,
        });
    }

    root
}

pub(crate) fn aggregate_ik_runtime_stats<'a>(
    runtime_stats: impl IntoIterator<Item = &'a [IkSolverRuntimeStats]>,
) -> Vec<IkSolverRuntimeStats> {
    let mut aggregate = Vec::new();
    for stats_set in runtime_stats {
        if aggregate.len() < stats_set.len() {
            aggregate.resize(stats_set.len(), IkSolverRuntimeStats::default());
        }
        for (target, stats) in aggregate.iter_mut().zip(stats_set.iter()) {
            target.solver_evaluations += stats.solver_evaluations;
            target.configured_iterations += stats.configured_iterations;
            target.executed_iterations += stats.executed_iterations;
            target.tolerance_precheck_breaks += stats.tolerance_precheck_breaks;
            target.tolerance_post_iteration_breaks += stats.tolerance_post_iteration_breaks;
            target.rollback_breaks += stats.rollback_breaks;
            target.max_iteration_exhaustions += stats.max_iteration_exhaustions;
            target.link_visits += stats.link_visits;
            target.link_steps += stats.link_steps;
            target.final_distance_sum += stats.final_distance_sum;
            target.final_distance_max = target.final_distance_max.max(stats.final_distance_max);
            target.exhausted_final_distance_sum += stats.exhausted_final_distance_sum;
            target.exhausted_final_distance_max = target
                .exhausted_final_distance_max
                .max(stats.exhausted_final_distance_max);
        }
    }
    aggregate
}

pub(crate) fn bench_pair(cfg: BenchPairConfig) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let total_start = Instant::now();

    let read_start = Instant::now();
    let pmx_bytes = read_file(&cfg.pmx_path)?;
    let vmd_bytes = read_file(&cfg.vmd_path)?;
    let read_elapsed = read_start.elapsed();

    let pmx_start = Instant::now();
    let pmx = mmd_anim_format::import_pmx_runtime(&pmx_bytes)?;
    let pmx_elapsed = pmx_start.elapsed();

    let vmd_start = Instant::now();
    let vmd = mmd_anim_format::import_vmd_motion(&vmd_bytes)?;
    let vmd_elapsed = vmd_start.elapsed();

    let bone_count = pmx.model.bone_count();
    let append_count = pmx.model.append_transforms().len();
    let fixed_axis_count = pmx.model.fixed_axis_count();
    let solver_count = pmx.model.ik_count();
    let ik_solver_summaries = pmx
        .model
        .ik_solvers()
        .iter()
        .enumerate()
        .map(|(index, solver)| BenchPairIkSolverSummary {
            solver_index: index,
            bone_index: solver.ik_bone.as_usize(),
            name: pmx
                .bone_names
                .get(solver.ik_bone.as_usize())
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_owned()),
            max_iterations: solver.iteration_count,
            links: solver.links.len(),
        })
        .collect::<Vec<_>>();
    let morph_count = pmx
        .morph_name_to_index
        .values()
        .map(|index| index.as_usize() + 1)
        .max()
        .unwrap_or(0);

    let clip_start = Instant::now();
    let clip = mmd_anim_format::build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        solver_count,
    );
    let clip_elapsed = clip_start.elapsed();

    let model = Arc::new(pmx.model);
    let mut runtimes: Vec<RuntimeInstance> = (0..cfg.instances)
        .map(|_| RuntimeInstance::new_with_counts(Arc::clone(&model), morph_count, solver_count))
        .collect();
    if cfg.solve_ik {
        for runtime in &mut runtimes {
            runtime.reset_ik_runtime_stats();
        }
    }

    let world_scratch_len = bone_count * 16;
    let mut world_scratch = vec![0.0f32; world_scratch_len];
    let mut skinning_scratch = vec![0.0f32; world_scratch_len];
    let mut morph_scratch = vec![0.0f32; morph_count];

    let hot_loop_start = Instant::now();
    let mut apply_pose_elapsed = std::time::Duration::ZERO;
    let mut morph_expand_elapsed = std::time::Duration::ZERO;
    let mut pose_eval_elapsed = std::time::Duration::ZERO;
    let mut world_copy_elapsed = std::time::Duration::ZERO;
    let mut skinning_copy_elapsed = std::time::Duration::ZERO;
    let mut morph_copy_elapsed = std::time::Duration::ZERO;
    let mut checksum = 0u32;
    let mut morph_checksum = 0u32;
    for i in 0..cfg.frame_count {
        let frame = cfg.start_frame + cfg.step * i as f32;
        for runtime in &mut runtimes {
            let apply_pose_start = Instant::now();
            clip.apply_to_pose(frame, runtime.pose_mut());
            apply_pose_elapsed += apply_pose_start.elapsed();

            let morph_expand_start = Instant::now();
            runtime.expand_morphs();
            morph_expand_elapsed += morph_expand_start.elapsed();

            let pose_eval_start = Instant::now();
            if cfg.solve_ik {
                runtime.evaluate_current_pose_with_ik_options(cfg.ik_options);
            } else {
                runtime.evaluate_current_pose_without_ik();
            }
            pose_eval_elapsed += pose_eval_start.elapsed();

            let world_start = Instant::now();
            copy_world_matrices_to_f32(runtime.world_matrices(), &mut world_scratch);
            world_copy_elapsed += world_start.elapsed();

            let skinning_start = Instant::now();
            copy_world_matrices_to_f32(runtime.skinning_matrices(), &mut skinning_scratch);
            skinning_copy_elapsed += skinning_start.elapsed();

            let morph_start = Instant::now();
            if !morph_scratch.is_empty() {
                morph_scratch.copy_from_slice(runtime.morph_weights());
            }
            morph_copy_elapsed += morph_start.elapsed();

            checksum = checksum.rotate_left(1) ^ translation_checksum(runtime.world_matrices());
            morph_checksum = morph_checksum.rotate_left(1) ^ f32_checksum(runtime.morph_weights());
            std::hint::black_box(world_scratch.first().copied());
            std::hint::black_box(skinning_scratch.first().copied());
            std::hint::black_box(morph_scratch.first().copied());
        }
    }
    let hot_loop_elapsed = hot_loop_start.elapsed();
    let total_elapsed = total_start.elapsed();

    let clip_frame_range = clip.frame_range();
    let frame_range = clip_frame_range
        .map(|(first, last)| format!("{first}..{last}"))
        .unwrap_or_else(|| "none".to_owned());
    let total_evaluations = cfg.instances as u64 * cfg.frame_count as u64;
    let apply_pose_ms = duration_to_ms(apply_pose_elapsed);
    let morph_expand_ms = duration_to_ms(morph_expand_elapsed);
    let pose_eval_ms = duration_to_ms(pose_eval_elapsed);
    let eval_ms = apply_pose_ms + morph_expand_ms + pose_eval_ms;
    let world_copy_ms = duration_to_ms(world_copy_elapsed);
    let skinning_copy_ms = duration_to_ms(skinning_copy_elapsed);
    let morph_copy_ms = duration_to_ms(morph_copy_elapsed);
    let hot_loop_ms = duration_to_ms(hot_loop_elapsed);
    let hot_loop_secs = hot_loop_elapsed.as_secs_f64();
    let ms_per_frame = if cfg.frame_count == 0 {
        0.0
    } else {
        hot_loop_ms / cfg.frame_count as f64
    };
    let fps = if hot_loop_secs == 0.0 {
        0.0
    } else {
        cfg.frame_count as f64 / hot_loop_secs
    };
    let ms_per_evaluation = if total_evaluations == 0 {
        0.0
    } else {
        hot_loop_ms / total_evaluations as f64
    };
    let evaluations_per_second = if hot_loop_secs == 0.0 {
        0.0
    } else {
        total_evaluations as f64 / hot_loop_secs
    };
    let read_ms = read_elapsed.as_secs_f64() * 1000.0;
    let pmx_import_ms = pmx_elapsed.as_secs_f64() * 1000.0;
    let vmd_import_ms = vmd_elapsed.as_secs_f64() * 1000.0;
    let clip_build_ms = clip_elapsed.as_secs_f64() * 1000.0;
    let total_ms = total_elapsed.as_secs_f64() * 1000.0;
    let aggregated_ik_stats = cfg.solve_ik.then(|| {
        aggregate_ik_runtime_stats(runtimes.iter().map(RuntimeInstance::ik_runtime_stats))
    });

    if cfg.use_json {
        let report = bench_pair_report_json(BenchPairReportInput {
            pmx_path: &cfg.pmx_path,
            vmd_path: &cfg.vmd_path,
            bone_count,
            append_count,
            fixed_axis_count,
            solver_count,
            morph_count,
            vmd_bone_keys: vmd.bone_keyframes.len(),
            vmd_morph_keys: vmd.morph_keyframes.len(),
            clip_bone_tracks: clip.bone_track_count(),
            clip_morph_tracks: clip.morph_track_count(),
            property_track: clip.has_property_track(),
            clip_frame_range,
            start_frame: cfg.start_frame,
            frame_count: cfg.frame_count,
            step: cfg.step,
            instances: cfg.instances,
            total_evaluations,
            solve_ik: cfg.solve_ik,
            ik_options: cfg.ik_options,
            read_ms,
            pmx_import_ms,
            vmd_import_ms,
            clip_build_ms,
            eval_ms,
            apply_pose_ms,
            morph_expand_ms,
            pose_eval_ms,
            world_copy_ms,
            skinning_copy_ms,
            morph_copy_ms,
            hot_loop_ms,
            total_ms,
            ms_per_frame,
            fps,
            ms_per_evaluation,
            evaluations_per_second,
            checksum,
            morph_checksum,
            ik_solver_summaries: &ik_solver_summaries,
            ik_stats: aggregated_ik_stats.as_deref(),
        });
        println!("{}", serde_json::to_string(&report)?);
        return Ok(ExitCode::SUCCESS);
    }

    let ik_display = if cfg.solve_ik {
        solver_count.to_string()
    } else {
        "disabled".to_owned()
    };
    let ik_cap_display = cfg
        .ik_options
        .max_iterations_cap
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_owned());
    println!("bench-pair:");
    println!(
        "  model:   bones={} ik={} ikTolerance={:.8} ikMaxIterationsCap={} append={} fixedAxis={}",
        bone_count,
        ik_display,
        cfg.ik_options.tolerance,
        ik_cap_display,
        append_count,
        fixed_axis_count,
    );
    println!(
        "  motion:  vmdBoneKeys={} vmdMorphKeys={} clipBoneTracks={} clipMorphTracks={} propertyTrack={} clipFrameRange={}",
        vmd.bone_keyframes.len(),
        vmd.morph_keyframes.len(),
        clip.bone_track_count(),
        clip.morph_track_count(),
        clip.has_property_track(),
        frame_range,
    );
    println!(
        "  timing:  readMs={:.3} pmxImportMs={:.3} vmdImportMs={:.3} clipBuildMs={:.3} evalMs={:.3} applyPoseMs={:.3} morphExpandMs={:.3} poseEvalMs={:.3} worldCopyMs={:.3} skinningCopyMs={:.3} morphCopyMs={:.3} hotLoopMs={:.3} totalMs={:.3}",
        read_ms,
        pmx_import_ms,
        vmd_import_ms,
        clip_build_ms,
        eval_ms,
        apply_pose_ms,
        morph_expand_ms,
        pose_eval_ms,
        world_copy_ms,
        skinning_copy_ms,
        morph_copy_ms,
        hot_loop_ms,
        total_ms,
    );
    println!(
        "  result:  instances={} frames={} totalEvaluations={} startFrame={:.3} step={:.3} msPerFrame={:.6} fps={:.1} msPerEvaluation={:.6} evaluationsPerSecond={:.1} checksum={:08x} morphChecksum={:08x}",
        cfg.instances,
        cfg.frame_count,
        total_evaluations,
        cfg.start_frame,
        cfg.step,
        ms_per_frame,
        fps,
        ms_per_evaluation,
        evaluations_per_second,
        checksum,
        morph_checksum,
    );
    if let Some(stats) = aggregated_ik_stats.as_deref() {
        let total_evaluations = stats
            .iter()
            .map(|stats| stats.solver_evaluations)
            .sum::<u64>();
        let configured_iterations = stats
            .iter()
            .map(|stats| stats.configured_iterations)
            .sum::<u64>();
        let executed_iterations = stats
            .iter()
            .map(|stats| stats.executed_iterations)
            .sum::<u64>();
        let skipped_iterations = configured_iterations.saturating_sub(executed_iterations);
        let tolerance_precheck_breaks = stats
            .iter()
            .map(|stats| stats.tolerance_precheck_breaks)
            .sum::<u64>();
        let tolerance_post_iteration_breaks = stats
            .iter()
            .map(|stats| stats.tolerance_post_iteration_breaks)
            .sum::<u64>();
        let rollback_breaks = stats.iter().map(|stats| stats.rollback_breaks).sum::<u64>();
        let max_iteration_exhaustions = stats
            .iter()
            .map(|stats| stats.max_iteration_exhaustions)
            .sum::<u64>();
        let link_steps = stats.iter().map(|stats| stats.link_steps).sum::<u64>();
        let skip_ratio = if configured_iterations == 0 {
            0.0
        } else {
            skipped_iterations as f64 / configured_iterations as f64
        };
        println!(
            "bench-pair-ik-stats: solverEvaluations={} configuredIterations={} executedIterations={} skippedIterations={} skippedRatio={:.3} tolerancePrecheckBreaks={} tolerancePostIterationBreaks={} rollbackBreaks={} maxIterationExhaustions={} linkSteps={}",
            total_evaluations,
            configured_iterations,
            executed_iterations,
            skipped_iterations,
            skip_ratio,
            tolerance_precheck_breaks,
            tolerance_post_iteration_breaks,
            rollback_breaks,
            max_iteration_exhaustions,
            link_steps,
        );

        let mut ranked = stats
            .iter()
            .enumerate()
            .map(|(index, stats)| (index, *stats))
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(_, stats)| {
            std::cmp::Reverse((stats.executed_iterations, stats.configured_iterations))
        });
        for (index, stats) in ranked.into_iter().take(8) {
            let summary = &ik_solver_summaries[index];
            let skipped = stats
                .configured_iterations
                .saturating_sub(stats.executed_iterations);
            let avg_final_distance = if stats.solver_evaluations == 0 {
                0.0
            } else {
                stats.final_distance_sum / stats.solver_evaluations as f64
            };
            let avg_exhausted_final_distance = if stats.max_iteration_exhaustions == 0 {
                0.0
            } else {
                stats.exhausted_final_distance_sum / stats.max_iteration_exhaustions as f64
            };
            println!(
                "bench-pair-ik-solver: solver={} bone={} name={} maxIterations={} links={} evaluations={} configuredIterations={} executedIterations={} skippedIterations={} precheckBreaks={} postBreaks={} rollbackBreaks={} exhausted={} avgFinalDistance={:.8} maxFinalDistance={:.8} avgExhaustedFinalDistance={:.8} maxExhaustedFinalDistance={:.8}",
                summary.solver_index,
                summary.bone_index,
                summary.name,
                summary.max_iterations,
                summary.links,
                stats.solver_evaluations,
                stats.configured_iterations,
                stats.executed_iterations,
                skipped,
                stats.tolerance_precheck_breaks,
                stats.tolerance_post_iteration_breaks,
                stats.rollback_breaks,
                stats.max_iteration_exhaustions,
                avg_final_distance,
                stats.final_distance_max,
                avg_exhausted_final_distance,
                stats.exhausted_final_distance_max,
            );
        }
    }

    Ok(ExitCode::SUCCESS)
}

#[derive(Debug)]
pub(crate) struct BenchSyntheticConfig {
    pub(crate) models: usize,
    pub(crate) bones: usize,
    pub(crate) frames: u32,
    pub(crate) use_json: bool,
}

pub(crate) fn bench_synthetic(
    cfg: BenchSyntheticConfig,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let model_count = cfg.models;
    let bone_count = cfg.bones;
    let frame_count = cfg.frames;
    let use_json = cfg.use_json;
    if model_count == 0 || bone_count == 0 || frame_count == 0 {
        return Err("models, bones, and frames must be positive".into());
    }

    // Build chain of bones: bone 0 = root, each child parented to previous
    let mut bones = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        let parent = if i == 0 {
            None
        } else {
            Some(BoneIndex(i as u32 - 1))
        };
        bones.push(BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0)));
    }
    let model = Arc::new(ModelArena::new(bones)?);

    // Build clip with two keyframes per bone (linear interpolation)
    let mut bone_tracks = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        let angle = 0.1 + (i as f32) * 0.02;
        let track = MovableBoneTrack::from_keyframes(vec![
            MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
            MovableBoneKeyframe::new(
                30,
                Vec3A::new(1.0, 0.0, 0.0),
                Quat::from_axis_angle(Vec3A::Y.into(), angle),
            ),
        ]);
        bone_tracks.push(BoneAnimationBinding {
            bone: BoneIndex(i as u32),
            track,
        });
    }
    let clip = AnimationClip::new(bone_tracks);

    // Create model_count independent RuntimeInstances
    let mut runtimes: Vec<RuntimeInstance> = (0..model_count)
        .map(|_| RuntimeInstance::new(Arc::clone(&model)))
        .collect();
    let mut matrix_scratch = vec![0.0f32; bone_count * 16];

    // Warm-up: one call to ensure any lazy init is done
    for runtime in &mut runtimes {
        runtime.evaluate_clip_frame(&clip, 0.0);
        copy_world_matrices_to_f32(runtime.world_matrices(), &mut matrix_scratch);
    }

    // Timed loop
    let mut rolling_checksum: u32 = 0;
    let start = Instant::now();
    for frame in 0..frame_count {
        let frame_f = frame as f32;
        for runtime in &mut runtimes {
            runtime.evaluate_clip_frame(&clip, frame_f);
            copy_world_matrices_to_f32(runtime.world_matrices(), &mut matrix_scratch);
            rolling_checksum = rolling_checksum.wrapping_add(f32_checksum(&matrix_scratch));
        }
    }
    let elapsed = start.elapsed();

    // Accumulate checksum from final state (prevents dead-code elimination)
    let mut final_checksum: u32 = 0;
    for runtime in &runtimes {
        final_checksum =
            final_checksum.wrapping_add(translation_checksum(runtime.world_matrices()));
    }
    final_checksum ^= rolling_checksum;

    let total_frames = frame_count as u64 * model_count as u64;
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let fps = total_frames as f64 / elapsed.as_secs_f64();

    if use_json {
        println!(
            r#"{{"models":{},"bones":{},"frames":{},"elapsedMs":{:.3},"totalFrames":{},"fps":{:.1},"checksum":"{:08x}"}}"#,
            model_count, bone_count, frame_count, elapsed_ms, total_frames, fps, final_checksum
        );
    } else {
        println!(
            "bench-synthetic: models={} bones={} frames={} elapsedMs={:.3} totalFrames={} fps={:.1} checksum={:08x}",
            model_count, bone_count, frame_count, elapsed_ms, total_frames, fps, final_checksum
        );
    }

    Ok(ExitCode::SUCCESS)
}

pub(crate) fn parse_bench_synthetic_args(
    args: &mut impl Iterator<Item = String>,
) -> Result<BenchSyntheticConfig, Box<dyn std::error::Error>> {
    let raw: Vec<String> = args.collect();
    let mut use_json = false;
    let mut positional = Vec::new();

    for token in &raw {
        if token == "--json" {
            use_json = true;
        } else if token.starts_with("--") {
            return Err(format!("unknown flag: {token}").into());
        } else {
            positional.push(token.clone());
        }
    }

    let mut pos_iter = positional.into_iter();
    let models = optional_positive_usize_arg(&mut pos_iter, 1, "models")?;
    let bones = optional_positive_usize_arg(&mut pos_iter, 32, "bones")?;
    let frames = optional_positive_u32_arg(&mut pos_iter, 1000, "frames")?;
    if let Some(extra) = pos_iter.next() {
        return Err(format!("unexpected extra argument: {extra}").into());
    }

    Ok(BenchSyntheticConfig {
        models,
        bones,
        frames,
        use_json,
    })
}

pub(crate) fn parse_bench_pair_args(
    args: &mut impl Iterator<Item = String>,
) -> Result<BenchPairConfig, Box<dyn std::error::Error>> {
    let raw: Vec<String> = args.collect();
    let mut solve_ik = true;
    let mut ik_tolerance = IkSolveOptions::default().tolerance;
    let mut ik_max_iterations_cap = None;
    let mut instances = 1usize;
    let mut use_json = false;
    let mut positional = Vec::new();

    let mut raw_iter = raw.into_iter();
    while let Some(token) = raw_iter.next() {
        match token.as_str() {
            "--json" => use_json = true,
            "--no-ik" => solve_ik = false,
            "--instances" => {
                let value = raw_iter.next().ok_or("missing value for --instances")?;
                instances = parse_positive_usize(&value, "instances")?;
            }
            "--ik-tolerance" => {
                let value = raw_iter.next().ok_or("missing value for --ik-tolerance")?;
                ik_tolerance = parse_finite_f32(&value, "ik-tolerance")?;
                if ik_tolerance < 0.0 {
                    return Err("ik-tolerance must be non-negative".into());
                }
            }
            "--ik-max-iterations-cap" => {
                let value = raw_iter
                    .next()
                    .ok_or("missing value for --ik-max-iterations-cap")?;
                ik_max_iterations_cap = Some(parse_positive_u32(&value, "ik-max-iterations-cap")?);
            }
            _ if token.starts_with("--") => {
                return Err(format!("unknown flag: {token}").into());
            }
            _ => positional.push(token),
        }
    }

    let mut pos_iter = positional.into_iter();
    let pmx_path = PathBuf::from(pos_iter.next().ok_or(BENCH_PAIR_USAGE)?);
    let vmd_path = PathBuf::from(pos_iter.next().ok_or(BENCH_PAIR_USAGE)?);
    let start_frame = optional_f32_parse_arg(&mut pos_iter, 0.0, "start-frame")?;
    let frame_count = optional_positive_usize_arg(&mut pos_iter, 1000, "frame-count")?;
    let step = optional_f32_parse_arg(&mut pos_iter, 1.0, "step")?;
    if step <= 0.0 {
        return Err("step must be positive".into());
    }
    if let Some(extra) = pos_iter.next() {
        return Err(format!("unexpected extra argument: {extra}").into());
    }

    Ok(BenchPairConfig {
        pmx_path,
        vmd_path,
        start_frame,
        frame_count,
        step,
        solve_ik,
        ik_options: IkSolveOptions {
            tolerance: ik_tolerance,
            max_iterations_cap: ik_max_iterations_cap,
        },
        instances,
        use_json,
    })
}

fn duration_to_ms(duration: std::time::Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn optional_positive_usize_arg(
    args: &mut impl Iterator<Item = String>,
    default: usize,
    label: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let Some(value) = args.next() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be positive").into());
    }
    Ok(parsed)
}

fn optional_positive_u32_arg(
    args: &mut impl Iterator<Item = String>,
    default: u32,
    label: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let Some(value) = args.next() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be positive").into());
    }
    Ok(parsed)
}

fn parse_positive_usize(value: &str, label: &str) -> Result<usize, Box<dyn std::error::Error>> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be positive").into());
    }
    Ok(parsed)
}

fn parse_positive_u32(value: &str, label: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be positive").into());
    }
    Ok(parsed)
}

fn optional_f32_parse_arg(
    args: &mut impl Iterator<Item = String>,
    default: f32,
    label: &str,
) -> Result<f32, Box<dyn std::error::Error>> {
    let Some(value) = args.next() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<f32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if !parsed.is_finite() {
        return Err(format!("{label} must be finite").into());
    }
    Ok(parsed)
}

fn parse_finite_f32(value: &str, label: &str) -> Result<f32, Box<dyn std::error::Error>> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if !parsed.is_finite() {
        return Err(format!("{label} must be finite").into());
    }
    Ok(parsed)
}
