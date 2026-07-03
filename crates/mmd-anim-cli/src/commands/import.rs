use std::{collections::BTreeMap, path::Path, process::ExitCode, sync::Arc};

use glam::Vec3A;
use mmd_anim_runtime::{AnimationClip, RuntimeInstance};
use serde::Serialize;

use crate::{diagnostics_suffix, f32_checksum, read_file, translation_checksum};

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
struct ImportRuntimeBatchSummary {
    bones: usize,
    ik: usize,
    morph_slots: usize,
    clip_bone_tracks: usize,
    clip_morph_tracks: usize,
    property_track: bool,
}

struct PairRuntimeContext {
    summary: ImportRuntimeBatchSummary,
    clip: AnimationClip,
    runtime: RuntimeInstance,
}

pub(crate) fn import_pmx_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(path)?;
    let imported = mmd_anim_format::import_pmx_runtime(&data)?;
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
    let imported = mmd_anim_format::import_pmx_runtime(&data)?;
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
    let imported = mmd_anim_format::import_pmd_runtime(&data)?;
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
    let imported = mmd_anim_format::import_vmd_motion(&data)?;
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
    let pmx = mmd_anim_format::import_pmx_runtime(&read_file(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&read_file(vmd_path)?)?;

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
    let pmx = mmd_anim_format::import_pmx_runtime(&read_file(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&read_file(vmd_path)?)?;

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
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut context = build_pair_runtime_context(pmx_path, vmd_path)?;
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

    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_pair_frames_json(
    pmx_path: &Path,
    vmd_path: &Path,
    frame_spec: ImportFrameSpec,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let report = build_import_runtime_batch_report(pmx_path, vmd_path, frame_spec)?;
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
) -> Result<ImportRuntimeBatchReport, Box<dyn std::error::Error>> {
    let mut context = build_pair_runtime_context(pmx_path, vmd_path)?;
    let per_frame = frame_spec
        .frames()
        .iter()
        .map(|frame| evaluate_pair_frame(&mut context.runtime, &context.clip, *frame))
        .collect();
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

fn build_pair_runtime_context(
    pmx_path: &Path,
    vmd_path: &Path,
) -> Result<PairRuntimeContext, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&read_file(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&read_file(vmd_path)?)?;

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
