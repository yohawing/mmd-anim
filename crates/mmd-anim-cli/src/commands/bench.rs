use std::{path::PathBuf, process::ExitCode, sync::Arc, time::Instant};

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, BoneInit, IkSolveOptions, ModelArena,
    MovableBoneKeyframe, MovableBoneTrack, RuntimeInstance,
};

use crate::{copy_world_matrices_to_f32, f32_checksum, read_file, translation_checksum};

pub(crate) const BENCH_PAIR_USAGE: &str = "usage: mmd-anim bench-pair <model.pmx> <motion.vmd> [start-frame] [frame-count] [step] [--no-ik] [--ik-tolerance <value>] [--ik-max-iterations-cap <count>]";

#[derive(Debug)]
pub(crate) struct BenchPairConfig {
    pmx_path: PathBuf,
    vmd_path: PathBuf,
    start_frame: f32,
    frame_count: usize,
    step: f32,
    solve_ik: bool,
    ik_options: IkSolveOptions,
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
        .map(|(index, solver)| {
            let name = pmx
                .bone_names
                .get(solver.ik_bone.as_usize())
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_owned());
            (
                index,
                solver.ik_bone.as_usize(),
                name,
                solver.iteration_count,
                solver.links.len(),
            )
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
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);
    runtime.reset_ik_runtime_stats();

    let eval_start = Instant::now();
    let mut checksum = 0u32;
    let mut morph_checksum = 0u32;
    for i in 0..cfg.frame_count {
        let frame = cfg.start_frame + cfg.step * i as f32;
        if cfg.solve_ik {
            runtime.evaluate_clip_frame_with_ik_options(&clip, frame, cfg.ik_options);
        } else {
            runtime.evaluate_clip_frame_without_ik(&clip, frame);
        }
        checksum = checksum.rotate_left(1) ^ translation_checksum(runtime.world_matrices());
        morph_checksum = morph_checksum.rotate_left(1) ^ f32_checksum(runtime.morph_weights());
    }
    let eval_elapsed = eval_start.elapsed();
    let total_elapsed = total_start.elapsed();

    let frame_range = clip
        .frame_range()
        .map(|(first, last)| format!("{first}..{last}"))
        .unwrap_or_else(|| "none".to_owned());
    let eval_ms = eval_elapsed.as_secs_f64() * 1000.0;
    let ms_per_frame = eval_ms / cfg.frame_count as f64;
    let fps = cfg.frame_count as f64 / eval_elapsed.as_secs_f64();

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
        bone_count, ik_display, cfg.ik_options.tolerance, ik_cap_display, append_count, fixed_axis_count,
    );
    println!(
        "  motion:  vmdBoneKeys={} vmdMorphKeys={} clipBoneTracks={} clipMorphTracks={} propertyTrack={} clipFrameRange={}",
        vmd.bone_keyframes.len(), vmd.morph_keyframes.len(), clip.bone_track_count(), clip.morph_track_count(), clip.has_property_track(), frame_range,
    );
    println!(
        "  timing:  readMs={:.3} pmxImportMs={:.3} vmdImportMs={:.3} clipBuildMs={:.3} evalMs={:.3} totalMs={:.3}",
        read_elapsed.as_secs_f64() * 1000.0, pmx_elapsed.as_secs_f64() * 1000.0, vmd_elapsed.as_secs_f64() * 1000.0, clip_elapsed.as_secs_f64() * 1000.0, eval_ms, total_elapsed.as_secs_f64() * 1000.0,
    );
    println!(
        "  result:  frames={} startFrame={:.3} step={:.3} msPerFrame={:.6} fps={:.1} checksum={:08x} morphChecksum={:08x}",
        cfg.frame_count, cfg.start_frame, cfg.step, ms_per_frame, fps, checksum, morph_checksum,
    );
    if cfg.solve_ik {
        let stats = runtime.ik_runtime_stats();
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
            let (solver_index, bone_index, name, max_iterations, links) =
                &ik_solver_summaries[index];
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
                solver_index,
                bone_index,
                name,
                max_iterations,
                links,
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
    let mut positional = Vec::new();

    let mut raw_iter = raw.into_iter();
    while let Some(token) = raw_iter.next() {
        match token.as_str() {
            "--no-ik" => solve_ik = false,
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
    })
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
