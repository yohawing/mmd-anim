use std::{collections::BTreeMap, path::Path, process::ExitCode, sync::Arc};

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{AnimationClip, IkSolverRuntimeStats, RuntimeInstance};
use serde::Serialize;

use crate::{
    diagnostics_suffix, f32_checksum, import_failure_error, read_file, translation_checksum,
};

const MAX_IMPORT_BATCH_FRAMES: usize = 10_000;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairFrameEval {
    pub(crate) frame: f32,
    pub(crate) world_matrices: usize,
    pub(crate) first_translation: [f32; 3],
    pub(crate) translation_checksum: u32,
    pub(crate) nonzero_morphs: usize,
    pub(crate) morph_checksum: u32,
    pub(crate) ik_enabled: Vec<u8>,
    pub(crate) ik_enabled_count: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportRuntimeBatchReport {
    kind: &'static str,
    model: String,
    motion: String,
    frame_spec: ImportFrameSpecReport,
    summary: ImportRuntimeBatchSummary,
    pub(crate) per_frame: Vec<PairFrameEval>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportFrameSpecReport {
    mode: &'static str,
    frames: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImportRuntimeBatchSummary {
    pub(crate) bones: usize,
    pub(crate) ik: usize,
    pub(crate) morph_slots: usize,
    pub(crate) clip_bone_tracks: usize,
    pub(crate) clip_morph_tracks: usize,
    pub(crate) property_track: bool,
}

pub(crate) struct PairRuntimeContext {
    pub(crate) summary: ImportRuntimeBatchSummary,
    pub(crate) clip: AnimationClip,
    pub(crate) runtime: RuntimeInstance,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImportVerboseAppendAggregate {
    pub append_count: usize,
    pub rotation_affecting_count: usize,
    pub translation_affecting_count: usize,
    pub nonzero_position_outputs: usize,
    pub nonidentity_rotation_outputs: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImportVerboseAppendDetail {
    pub append_index: usize,
    pub target_bone_index: u32,
    pub source_bone_index: u32,
    pub ratio: f32,
    pub affect_rotation: bool,
    pub affect_translation: bool,
    pub local: bool,
    pub output_position: [f32; 3],
    pub output_rotation: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ImportVerboseAppendDiagnostics {
    pub aggregate: ImportVerboseAppendAggregate,
    pub details: Vec<ImportVerboseAppendDetail>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImportVerboseIkAggregate {
    pub solver_evaluations: u64,
    pub configured_iterations: u64,
    pub executed_iterations: u64,
    pub skipped_iterations: u64,
    pub skipped_ratio: f64,
    pub tolerance_precheck_breaks: u64,
    pub tolerance_post_iteration_breaks: u64,
    pub rollback_breaks: u64,
    pub max_iteration_exhaustions: u64,
    pub link_visits: u64,
    pub link_steps: u64,
}

pub(crate) fn import_pmx_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(path)?;
    let imported = mmd_anim_format::import_pmx_runtime(&data).map_err(|error| {
        import_failure_error("import", path, mmd_anim_format::MmdFormatKind::Pmx, error)
    })?;
    println!(
        "PMX runtime import: bones={} append={} fixedAxis={} ik={} boneNames={} morphNames={} ikNameMap={}",
        imported.model.bone_count(),
        imported.model.append_transforms().len(),
        imported.model.fixed_axis_count(),
        imported.model.ik_count(),
        imported.bone_name_to_index.len(),
        imported.morph_name_to_index.len(),
        imported.ik_solver_bone_name_to_index.len()
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_pmx_ik_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(path)?;
    let imported = mmd_anim_format::import_pmx_runtime(&data).map_err(|error| {
        import_failure_error("import", path, mmd_anim_format::MmdFormatKind::Pmx, error)
    })?;
    let solvers = imported.model.ik_solvers();
    let max_iterations = solvers
        .iter()
        .map(|solver| solver.iteration_count)
        .max()
        .unwrap_or(0);
    let mut distribution = BTreeMap::<u32, usize>::new();
    for solver in solvers {
        *distribution.entry(solver.iteration_count).or_default() += 1;
    }
    let distribution = distribution
        .iter()
        .map(|(iterations, count)| format!("{iterations}:{count}"))
        .collect::<Vec<_>>()
        .join(",");

    println!(
        "PMX IK summary: bones={} ik={} maxIterations={} distribution={}",
        imported.model.bone_count(),
        solvers.len(),
        max_iterations,
        distribution
    );
    for (solver_index, solver) in solvers.iter().enumerate() {
        if solver.iteration_count == max_iterations {
            let name = imported
                .bone_names
                .get(solver.ik_bone.as_usize())
                .map(String::as_str)
                .unwrap_or("<unknown>");
            println!(
                "max IK: solver={} bone={} name={} target={} iterations={} limitAngle={:.6} links={}",
                solver_index,
                solver.ik_bone.as_usize(),
                name,
                solver.target_bone.as_usize(),
                solver.iteration_count,
                solver.limit_angle,
                solver.links.len()
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_pmd_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(path)?;
    let imported = mmd_anim_format::import_pmd_runtime(&data).map_err(|error| {
        import_failure_error("import", path, mmd_anim_format::MmdFormatKind::Pmd, error)
    })?;
    println!(
        "PMD runtime import: bones={} ik={} morphSlots={} vertexMorphOffsets={} boneNames={} morphNames={} ikNameMap={}{}",
        imported.model.bone_count(),
        imported.model.ik_count(),
        imported.model.morph_count(),
        imported.model.vertex_morph_offsets().len(),
        imported.bone_name_to_index.len(),
        imported.morph_name_to_index.len(),
        imported.ik_solver_bone_name_to_index.len(),
        diagnostics_suffix(imported.diagnostics.len())
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_vmd_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(path)?;
    let imported = mmd_anim_format::import_vmd_motion(&data).map_err(|error| {
        import_failure_error("import", path, mmd_anim_format::MmdFormatKind::Vmd, error)
    })?;
    let property_ik_entries: usize = imported
        .property_ik_frames
        .iter()
        .map(|frame| frame.entries.len())
        .sum();
    println!(
        "VMD runtime import: boneKeys={} morphKeys={} propertyFrames={} propertyIkEntries={}",
        imported.bone_keyframes.len(),
        imported.morph_keyframes.len(),
        imported.property_ik_frames.len(),
        property_ik_entries
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_pair_summary(
    pmx_path: &Path,
    vmd_path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&read_file(pmx_path)?).map_err(|error| {
        import_failure_error(
            "import",
            pmx_path,
            mmd_anim_format::MmdFormatKind::Pmx,
            error,
        )
    })?;
    let vmd = mmd_anim_format::import_vmd_motion(&read_file(vmd_path)?).map_err(|error| {
        import_failure_error(
            "import",
            vmd_path,
            mmd_anim_format::MmdFormatKind::Vmd,
            error,
        )
    })?;

    let matched_bone_keys = vmd
        .bone_keyframes
        .iter()
        .filter(|keyframe| {
            pmx.bone_name_to_index
                .contains_key(&keyframe.bone_name_normalized)
        })
        .count();
    let matched_morph_keys = vmd
        .morph_keyframes
        .iter()
        .filter(|(name, _, _)| {
            let normalized = mmd_anim_format::normalize_vmd_name(name);
            pmx.morph_name_to_index.contains_key(&normalized)
        })
        .count();
    let property_ik_entries: usize = vmd
        .property_ik_frames
        .iter()
        .map(|frame| frame.entries.len())
        .sum();
    let matched_property_ik_entries = vmd
        .property_ik_frames
        .iter()
        .flat_map(|frame| frame.entries.iter())
        .filter(|entry| {
            pmx.ik_solver_bone_name_to_index
                .contains_key(&entry.name_normalized)
        })
        .count();

    let bone_match_pct = if vmd.bone_keyframes.is_empty() {
        100.0
    } else {
        matched_bone_keys as f64 / vmd.bone_keyframes.len() as f64 * 100.0
    };
    let morph_match_pct = if vmd.morph_keyframes.is_empty() {
        100.0
    } else {
        matched_morph_keys as f64 / vmd.morph_keyframes.len() as f64 * 100.0
    };
    println!("PMX/VMD runtime import:");
    println!(
        "  model:    bones={} append={} fixedAxis={} ik={}",
        pmx.model.bone_count(),
        pmx.model.append_transforms().len(),
        pmx.model.fixed_axis_count(),
        pmx.model.ik_count(),
    );
    println!(
        "  motion:   vmdBoneKeys={} matchedBoneKeys={} ({:.1}%) vmdMorphKeys={} matchedMorphKeys={} ({:.1}%)",
        vmd.bone_keyframes.len(),
        matched_bone_keys,
        bone_match_pct,
        vmd.morph_keyframes.len(),
        matched_morph_keys,
        morph_match_pct,
    );
    println!(
        "  property: propertyFrames={} propertyIkEntries={} matchedPropertyIkEntries={}",
        vmd.property_ik_frames.len(),
        property_ik_entries,
        matched_property_ik_entries,
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_pair_clip_summary(
    pmx_path: &Path,
    vmd_path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&read_file(pmx_path)?).map_err(|error| {
        import_failure_error(
            "import",
            pmx_path,
            mmd_anim_format::MmdFormatKind::Pmx,
            error,
        )
    })?;
    let vmd = mmd_anim_format::import_vmd_motion(&read_file(vmd_path)?).map_err(|error| {
        import_failure_error(
            "import",
            vmd_path,
            mmd_anim_format::MmdFormatKind::Vmd,
            error,
        )
    })?;

    let solver_count = pmx.model.ik_count();
    let clip = mmd_anim_format::build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        solver_count,
    );

    let frame_range = clip
        .frame_range()
        .map(|(first, last)| format!("{first}..{last}"))
        .unwrap_or_else(|| "none".to_owned());

    println!(
        "Pair clip built: bones={} append={} fixedAxis={} ik={} clipBoneTracks={} clipMorphTracks={} propertyTrack={} frameRange={}",
        pmx.model.bone_count(),
        pmx.model.append_transforms().len(),
        pmx.model.fixed_axis_count(),
        pmx.model.ik_count(),
        clip.bone_track_count(),
        clip.morph_track_count(),
        clip.has_property_track(),
        frame_range
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_pair_frame_summary(
    pmx_path: &Path,
    vmd_path: &Path,
    frame: f32,
    verbose: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut context = build_pair_runtime_context(pmx_path, vmd_path)?;
    if verbose {
        context.runtime.reset_ik_runtime_stats();
    }
    let eval = evaluate_pair_frame(&mut context.runtime, &context.clip, frame);

    println!(
        "PMX/VMD frame {:.3}: bones={} ik={} clipBoneTracks={} clipMorphTracks={} worldMatrices={} firstTranslation=({:.6},{:.6},{:.6}) translationChecksum={:08x} nonzeroMorphs={} morphChecksum={:08x}",
        eval.frame,
        context.summary.bones,
        context.summary.ik,
        context.summary.clip_bone_tracks,
        context.summary.clip_morph_tracks,
        eval.world_matrices,
        eval.first_translation[0],
        eval.first_translation[1],
        eval.first_translation[2],
        eval.translation_checksum,
        eval.nonzero_morphs,
        eval.morph_checksum,
    );

    if verbose {
        let ik_stats = aggregate_import_ik_runtime_stats(context.runtime.ik_runtime_stats());
        let append = collect_import_verbose_append_diagnostics(&context.runtime);
        for line in import_pair_frame_verbose_lines(&context.summary, &eval, ik_stats, append) {
            eprintln!("{line}");
        }
    }

    Ok(ExitCode::SUCCESS)
}

pub(crate) fn aggregate_import_ik_runtime_stats(
    stats: &[IkSolverRuntimeStats],
) -> ImportVerboseIkAggregate {
    let solver_evaluations = stats
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
    let skipped_ratio = if configured_iterations == 0 {
        0.0
    } else {
        skipped_iterations as f64 / configured_iterations as f64
    };

    ImportVerboseIkAggregate {
        solver_evaluations,
        configured_iterations,
        executed_iterations,
        skipped_iterations,
        skipped_ratio,
        tolerance_precheck_breaks: stats
            .iter()
            .map(|stats| stats.tolerance_precheck_breaks)
            .sum(),
        tolerance_post_iteration_breaks: stats
            .iter()
            .map(|stats| stats.tolerance_post_iteration_breaks)
            .sum(),
        rollback_breaks: stats.iter().map(|stats| stats.rollback_breaks).sum(),
        max_iteration_exhaustions: stats
            .iter()
            .map(|stats| stats.max_iteration_exhaustions)
            .sum(),
        link_visits: stats.iter().map(|stats| stats.link_visits).sum(),
        link_steps: stats.iter().map(|stats| stats.link_steps).sum(),
    }
}

pub(crate) fn collect_import_verbose_append_diagnostics(
    runtime: &RuntimeInstance,
) -> ImportVerboseAppendDiagnostics {
    let append_transforms = runtime.model().append_transforms();
    let mut rotation_affecting_count = 0usize;
    let mut translation_affecting_count = 0usize;
    let mut nonzero_position_outputs = 0usize;
    let mut nonidentity_rotation_outputs = 0usize;
    let mut details = Vec::with_capacity(append_transforms.len());

    for (append_index, append) in append_transforms.iter().enumerate() {
        if append.affect_rotation {
            rotation_affecting_count += 1;
        }
        if append.affect_translation {
            translation_affecting_count += 1;
        }

        let output_position = runtime
            .append_position_offset(append.target_bone)
            .to_array();
        let output_rotation = runtime.append_rotation(append.target_bone);
        if append_position_output_is_nonzero(output_position) {
            nonzero_position_outputs += 1;
        }
        if append_rotation_output_is_nonidentity(output_rotation) {
            nonidentity_rotation_outputs += 1;
        }

        details.push(ImportVerboseAppendDetail {
            append_index,
            target_bone_index: append.target_bone.0,
            source_bone_index: append.source_bone.0,
            ratio: append.ratio,
            affect_rotation: append.affect_rotation,
            affect_translation: append.affect_translation,
            local: append.local,
            output_position,
            output_rotation: [
                output_rotation.x,
                output_rotation.y,
                output_rotation.z,
                output_rotation.w,
            ],
        });
    }

    ImportVerboseAppendDiagnostics {
        aggregate: ImportVerboseAppendAggregate {
            append_count: append_transforms.len(),
            rotation_affecting_count,
            translation_affecting_count,
            nonzero_position_outputs,
            nonidentity_rotation_outputs,
        },
        details,
    }
}

fn append_position_output_is_nonzero(position: [f32; 3]) -> bool {
    position
        .iter()
        .any(|component| component.abs() > f32::EPSILON)
}

fn append_rotation_output_is_nonidentity(rotation: Quat) -> bool {
    rotation.normalize().dot(Quat::IDENTITY).abs() < 1.0 - f32::EPSILON
}

pub(crate) fn import_pair_frame_verbose_lines(
    summary: &ImportRuntimeBatchSummary,
    eval: &PairFrameEval,
    ik_stats: ImportVerboseIkAggregate,
    append: ImportVerboseAppendDiagnostics,
) -> Vec<String> {
    let ik_enabled_active = eval
        .ik_enabled
        .iter()
        .filter(|enabled| **enabled != 0)
        .count();
    let ik_enabled = eval
        .ik_enabled
        .iter()
        .map(|enabled| enabled.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let mut lines = vec![
        format!("import-verbose: frame={:.3}", eval.frame),
        format!(
            "import-verbose: summary bones={} ik={} morphSlots={} clipBoneTracks={} clipMorphTracks={} propertyTrack={}",
            summary.bones,
            summary.ik,
            summary.morph_slots,
            summary.clip_bone_tracks,
            summary.clip_morph_tracks,
            summary.property_track,
        ),
        format!(
            "import-verbose: ikEnabledCount={} ikEnabledActive={} ikEnabled=[{ik_enabled}]",
            eval.ik_enabled_count, ik_enabled_active,
        ),
        format!(
            "import-verbose: ikStats solverEvaluations={} configuredIterations={} executedIterations={} skippedIterations={} skippedRatio={:.3} tolerancePrecheckBreaks={} tolerancePostIterationBreaks={} rollbackBreaks={} maxIterationExhaustions={} linkVisits={} linkSteps={}",
            ik_stats.solver_evaluations,
            ik_stats.configured_iterations,
            ik_stats.executed_iterations,
            ik_stats.skipped_iterations,
            ik_stats.skipped_ratio,
            ik_stats.tolerance_precheck_breaks,
            ik_stats.tolerance_post_iteration_breaks,
            ik_stats.rollback_breaks,
            ik_stats.max_iteration_exhaustions,
            ik_stats.link_visits,
            ik_stats.link_steps,
        ),
        format!(
            "import-verbose: append count={} rotationAffecting={} translationAffecting={} nonzeroPositionOutputs={} nonidentityRotationOutputs={}",
            append.aggregate.append_count,
            append.aggregate.rotation_affecting_count,
            append.aggregate.translation_affecting_count,
            append.aggregate.nonzero_position_outputs,
            append.aggregate.nonidentity_rotation_outputs,
        ),
    ];

    for detail in &append.details {
        lines.push(format!(
            "import-verbose: append index={} targetBoneIndex={} sourceBoneIndex={} ratio={:.6} affectRotation={} affectTranslation={} local={} outputPosition=({:.6},{:.6},{:.6}) outputRotation=({:.6},{:.6},{:.6},{:.6})",
            detail.append_index,
            detail.target_bone_index,
            detail.source_bone_index,
            detail.ratio,
            detail.affect_rotation,
            detail.affect_translation,
            detail.local,
            detail.output_position[0],
            detail.output_position[1],
            detail.output_position[2],
            detail.output_rotation[0],
            detail.output_rotation[1],
            detail.output_rotation[2],
            detail.output_rotation[3],
        ));
    }

    lines.push(format!(
        "import-verbose: result worldMatrices={} firstTranslation=({:.6},{:.6},{:.6}) translationChecksum={:08x} nonzeroMorphs={} morphChecksum={:08x}",
        eval.world_matrices,
        eval.first_translation[0],
        eval.first_translation[1],
        eval.first_translation[2],
        eval.translation_checksum,
        eval.nonzero_morphs,
        eval.morph_checksum,
    ));

    lines
}

pub(crate) fn import_pair_frames_json(
    pmx_path: &Path,
    vmd_path: &Path,
    frame_spec: ImportFrameSpec,
    verbose: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let report = build_import_runtime_batch_report(pmx_path, vmd_path, frame_spec, verbose)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(ExitCode::SUCCESS)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ImportFrameSpec {
    List(Vec<f32>),
    Range(Vec<f32>),
}

impl ImportFrameSpec {
    fn mode(&self) -> &'static str {
        match self {
            Self::List(_) => "list",
            Self::Range(_) => "range",
        }
    }

    fn frames(&self) -> &[f32] {
        match self {
            Self::List(frames) | Self::Range(frames) => frames,
        }
    }
}

pub(crate) fn parse_import_frames_list(text: &str) -> Result<ImportFrameSpec, String> {
    let mut frames = Vec::new();
    for part in text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err("import --frames must not contain empty entries".to_owned());
        }
        let frame = part
            .parse::<f32>()
            .map_err(|_| format!("invalid import --frames value: {part:?}"))?;
        if !frame.is_finite() {
            return Err("import --frames values must be finite".to_owned());
        }
        frames.push(frame);
    }
    validate_batch_frames(&frames)?;
    Ok(ImportFrameSpec::List(frames))
}

pub(crate) fn parse_import_frame_range(text: &str) -> Result<ImportFrameSpec, String> {
    let parts: Vec<_> = text.split(':').collect();
    if parts.len() != 3 {
        return Err("import --frame-range must use START:END:STEP".to_owned());
    }
    let start = parse_frame_range_part(parts[0], "START")?;
    let end = parse_frame_range_part(parts[1], "END")?;
    let step = parse_frame_range_part(parts[2], "STEP")?;
    if step <= 0.0 {
        return Err("import --frame-range STEP must be positive".to_owned());
    }
    if end < start {
        return Err("import --frame-range END must be greater than or equal to START".to_owned());
    }

    let mut frames = Vec::new();
    let mut index = 0usize;
    loop {
        let frame = start + index as f32 * step;
        if frame > end {
            if frames.last().copied() != Some(end) {
                frames.push(end);
                if frames.len() > MAX_IMPORT_BATCH_FRAMES {
                    return Err(format!(
                        "import batch frame count exceeds limit {MAX_IMPORT_BATCH_FRAMES}"
                    ));
                }
            }
            break;
        }
        frames.push(frame);
        if frames.len() > MAX_IMPORT_BATCH_FRAMES {
            return Err(format!(
                "import batch frame count exceeds limit {MAX_IMPORT_BATCH_FRAMES}"
            ));
        }
        if frame == end {
            break;
        }
        index += 1;
    }
    validate_batch_frames(&frames)?;
    Ok(ImportFrameSpec::Range(frames))
}

fn parse_frame_range_part(text: &str, name: &str) -> Result<f32, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(format!("import --frame-range {name} must not be empty"));
    }
    let value = text
        .parse::<f32>()
        .map_err(|_| format!("invalid import --frame-range {name}: {text:?}"))?;
    if !value.is_finite() {
        return Err("import --frame-range values must be finite".to_owned());
    }
    Ok(value)
}

fn validate_batch_frames(frames: &[f32]) -> Result<(), String> {
    if frames.is_empty() {
        return Err("import batch frame list must not be empty".to_owned());
    }
    if frames.len() > MAX_IMPORT_BATCH_FRAMES {
        return Err(format!(
            "import batch frame count exceeds limit {MAX_IMPORT_BATCH_FRAMES}"
        ));
    }
    Ok(())
}

pub(crate) fn build_import_runtime_batch_report(
    pmx_path: &Path,
    vmd_path: &Path,
    frame_spec: ImportFrameSpec,
    verbose: bool,
) -> Result<ImportRuntimeBatchReport, Box<dyn std::error::Error>> {
    let mut context = build_pair_runtime_context(pmx_path, vmd_path)?;
    let mut per_frame = Vec::with_capacity(frame_spec.frames().len());
    for &frame in frame_spec.frames() {
        let (eval, ik_stats) =
            evaluate_pair_frame_with_ik_stats(&mut context.runtime, &context.clip, frame, verbose);
        if verbose {
            let append = collect_import_verbose_append_diagnostics(&context.runtime);
            for line in import_pair_frame_verbose_lines(&context.summary, &eval, ik_stats, append) {
                eprintln!("{line}");
            }
        }
        per_frame.push(eval);
    }
    Ok(ImportRuntimeBatchReport {
        kind: "import-runtime-batch",
        model: pmx_path.display().to_string(),
        motion: vmd_path.display().to_string(),
        frame_spec: ImportFrameSpecReport {
            mode: frame_spec.mode(),
            frames: frame_spec.frames().to_vec(),
        },
        summary: context.summary,
        per_frame,
    })
}

pub(crate) fn build_pair_runtime_context(
    pmx_path: &Path,
    vmd_path: &Path,
) -> Result<PairRuntimeContext, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&read_file(pmx_path)?).map_err(|error| {
        import_failure_error(
            "import",
            pmx_path,
            mmd_anim_format::MmdFormatKind::Pmx,
            error,
        )
    })?;
    let vmd = mmd_anim_format::import_vmd_motion(&read_file(vmd_path)?).map_err(|error| {
        import_failure_error(
            "import",
            vmd_path,
            mmd_anim_format::MmdFormatKind::Vmd,
            error,
        )
    })?;

    let bone_count = pmx.model.bone_count();
    let solver_count = pmx.model.ik_count();
    let morph_count = pmx
        .morph_name_to_index
        .values()
        .map(|index| index.as_usize() + 1)
        .max()
        .unwrap_or(0);

    let clip = mmd_anim_format::build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        solver_count,
    );
    let summary = ImportRuntimeBatchSummary {
        bones: bone_count,
        ik: solver_count,
        morph_slots: morph_count,
        clip_bone_tracks: clip.bone_track_count(),
        clip_morph_tracks: clip.morph_track_count(),
        property_track: clip.has_property_track(),
    };
    let model = Arc::new(pmx.model);
    let runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);

    Ok(PairRuntimeContext {
        summary,
        clip,
        runtime,
    })
}

pub(crate) fn evaluate_pair_frame_with_ik_stats(
    runtime: &mut RuntimeInstance,
    clip: &AnimationClip,
    frame: f32,
    reset_ik_stats: bool,
) -> (PairFrameEval, ImportVerboseIkAggregate) {
    if reset_ik_stats {
        runtime.reset_ik_runtime_stats();
    }
    let eval = evaluate_pair_frame(runtime, clip, frame);
    let ik_stats = aggregate_import_ik_runtime_stats(runtime.ik_runtime_stats());
    (eval, ik_stats)
}

pub(crate) fn evaluate_pair_frame(
    runtime: &mut RuntimeInstance,
    clip: &AnimationClip,
    frame: f32,
) -> PairFrameEval {
    runtime.evaluate_clip_frame(clip, frame);
    let world_matrices = runtime.world_matrices();
    let first_translation = if let Some(m) = world_matrices.first() {
        [m.w_axis.x, m.w_axis.y, m.w_axis.z]
    } else {
        Vec3A::ZERO.to_array()
    };
    let morph_weights = runtime.morph_weights();
    let ik_enabled = runtime.ik_enabled().to_vec();
    PairFrameEval {
        frame,
        world_matrices: world_matrices.len(),
        first_translation,
        translation_checksum: translation_checksum(world_matrices),
        nonzero_morphs: morph_weights
            .iter()
            .filter(|weight| weight.abs() > f32::EPSILON)
            .count(),
        morph_checksum: f32_checksum(morph_weights),
        ik_enabled_count: ik_enabled.len(),
        ik_enabled,
    }
}
