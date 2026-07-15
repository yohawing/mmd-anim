use std::{collections::HashMap, fs, path::Path, process::ExitCode, sync::Arc};

use glam::{Quat, Vec3A};
use mmd_anim_format::vmd::{VmdParsedBoneFrame, VmdParsedCounts, VmdParsedMorphFrame};
use mmd_anim_runtime::{
    BoneIndex, BoneInit, DensePoseSequenceView, ModelArena, MorphIndex, ReductionTarget,
    ReductionTolerances, RuntimeInstance, SkeletonSnapshot, reduce_dense_pose_sequence,
};
use serde_json::json;

use crate::{parse_failure_error, read_file, write_file};

const MODEL_IDENTITY: u64 = 0x564d_4452;

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub(crate) enum VmdReductionCurveMode {
    Linear,
    VmdBezier,
}

pub(crate) struct ReduceVmdOptions {
    pub position_tolerance: f32,
    pub rotation_tolerance: f32,
    pub morph_tolerance: f32,
    pub curve_mode: VmdReductionCurveMode,
    pub max_sampled_frames: usize,
    pub use_json: bool,
}

pub(crate) fn reduce_vmd(
    input: &Path,
    output: &Path,
    options: ReduceVmdOptions,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    ensure_distinct_paths(input, output)?;
    if options.max_sampled_frames == 0 {
        return Err("reduce-vmd --max-sampled-frames must be greater than zero".into());
    }
    let data = read_file(input)?;
    let mut animation = mmd_anim_format::parse_vmd_animation(&data).map_err(|error| {
        parse_failure_error(
            "reduce-vmd",
            input,
            mmd_anim_format::MmdFormatKind::Vmd,
            error,
        )
    })?;
    let mut runtime_import = mmd_anim_format::import_vmd_motion(&data).map_err(|error| {
        parse_failure_error(
            "reduce-vmd",
            input,
            mmd_anim_format::MmdFormatKind::Vmd,
            error,
        )
    })?;
    // Property/IK state is preserved verbatim in the output. It must not be
    // applied to this model-independent, zero-IK sampling runtime.
    runtime_import.property_keyframes.clear();

    let (bone_names, bone_indices) = unique_bone_names(&animation);
    let (morph_names, morph_indices) = unique_morph_names(&animation);
    if bone_indices.is_empty() && morph_indices.is_empty() {
        return Err("reduce-vmd requires at least one bone or morph keyframe".into());
    }
    let runtime_bone_count = bone_names.len().max(1);
    let clip = mmd_anim_format::build_clip_from_import(
        runtime_import,
        &|name| bone_indices.get(name).copied().map(BoneIndex),
        &|name| morph_indices.get(name).copied().map(MorphIndex),
    );
    let model = Arc::new(ModelArena::new(vec![
        BoneInit::new(None, Vec3A::ZERO);
        runtime_bone_count
    ])?);
    let mut runtime = RuntimeInstance::new_with_morph_count(model.clone(), morph_names.len());
    let pose_max_frame = animation
        .bone_frames
        .iter()
        .map(|frame| frame.frame)
        .chain(animation.morph_frames.iter().map(|frame| frame.frame))
        .max()
        .unwrap_or(0);
    let frame_count = usize::try_from(pose_max_frame)?
        .checked_add(1)
        .ok_or("reduce-vmd frame count overflow")?;
    if frame_count > options.max_sampled_frames {
        return Err(format!(
            "reduce-vmd requires {frame_count} sampled frames, exceeding --max-sampled-frames {}",
            options.max_sampled_frames
        )
        .into());
    }
    let world_capacity = frame_count
        .checked_mul(runtime_bone_count)
        .ok_or("reduce-vmd world sample capacity overflow")?;
    let morph_capacity = frame_count
        .checked_mul(morph_names.len())
        .ok_or("reduce-vmd morph sample capacity overflow")?;
    let mut world = Vec::with_capacity(world_capacity);
    let mut morphs = Vec::with_capacity(morph_capacity);
    for frame in 0..=pose_max_frame {
        runtime.evaluate_clip_frame_without_ik(&clip, frame as f32);
        world.extend_from_slice(runtime.pose().world_matrices());
        morphs.extend_from_slice(runtime.pose().morph_weights());
    }

    let tolerances = ReductionTolerances {
        local_position: options.position_tolerance,
        local_rotation_radians: options.rotation_tolerance,
        world_position: options.position_tolerance,
        world_rotation_radians: options.rotation_tolerance,
        morph_weight: options.morph_tolerance,
    };
    let snapshot = SkeletonSnapshot::new(
        vec![-1; runtime_bone_count],
        vec![Vec3A::ZERO; runtime_bone_count],
        vec![Quat::IDENTITY; runtime_bone_count],
        morph_names.len(),
        MODEL_IDENTITY,
    )?;
    let target = match options.curve_mode {
        VmdReductionCurveMode::Linear => ReductionTarget::LinearSlerp,
        VmdReductionCurveMode::VmdBezier => {
            eprintln!(
                "warning: reduce-vmd --curve-mode vmd-bezier performs an expensive quantized control-point search"
            );
            ReductionTarget::VmdBezier
        }
    };
    let reduced = reduce_dense_pose_sequence(
        DensePoseSequenceView::new(
            &world,
            &morphs,
            frame_count,
            runtime_bone_count,
            morph_names.len(),
            0.0,
            1.0,
        )?,
        snapshot,
        tolerances,
        target,
    )?;
    let report = reduced.report();
    let source_bone_keys = animation.bone_frames.len();
    let source_morph_keys = animation.morph_frames.len();
    animation.bone_frames = prefer_smaller_bone_tracks(
        &animation.bone_frames,
        reduced_bone_frames(&reduced, &bone_names),
    );
    animation.morph_frames = prefer_smaller_morph_tracks(
        &animation.morph_frames,
        reduced_morph_frames(&reduced, &morph_names),
    );
    animation.metadata.counts = VmdParsedCounts {
        bones: animation.bone_frames.len(),
        morphs: animation.morph_frames.len(),
        cameras: animation.camera_frames.len(),
        lights: animation.light_frames.len(),
        self_shadows: animation.self_shadow_frames.len(),
        properties: animation.property_frames.len(),
    };
    let reduced_bone_keys = animation.bone_frames.len();
    let reduced_morph_keys = animation.morph_frames.len();
    let bytes = mmd_anim_format::export_vmd_animation(&animation);
    mmd_anim_format::parse_vmd_animation(&bytes).map_err(|error| {
        parse_failure_error(
            "reduce-vmd output validation",
            output,
            mmd_anim_format::MmdFormatKind::Vmd,
            error,
        )
    })?;
    write_file(output, &bytes)?;

    let value = json!({
        "status": "ok",
        "command": "reduce-vmd",
        "curveMode": match options.curve_mode {
            VmdReductionCurveMode::Linear => "linear",
            VmdReductionCurveMode::VmdBezier => "vmd-bezier",
        },
        "input": input.display().to_string(),
        "output": output.display().to_string(),
        "bytesIn": data.len(),
        "bytesOut": bytes.len(),
        "maxFrame": animation.metadata.max_frame,
        "sourceBoneKeys": source_bone_keys,
        "reducedBoneKeys": reduced_bone_keys,
        "sourceMorphKeys": source_morph_keys,
        "reducedMorphKeys": reduced_morph_keys,
        "maxLocalPositionError": report.max_local_position_error,
        "maxLocalRotationErrorRadians": report.max_local_rotation_error_radians,
        "maxMorphWeightError": report.max_morph_weight_error,
        "preservedCameraKeys": animation.camera_frames.len(),
        "preservedLightKeys": animation.light_frames.len(),
        "preservedSelfShadowKeys": animation.self_shadow_frames.len(),
        "preservedPropertyKeys": animation.property_frames.len(),
    });
    if options.use_json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!(
            "VMD reduced: boneKeys={source_bone_keys}->{reduced_bone_keys} morphKeys={source_morph_keys}->{reduced_morph_keys} bytes={}->{} maxFrame={}",
            data.len(),
            bytes.len(),
            animation.metadata.max_frame
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn ensure_distinct_paths(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let input = fs::canonicalize(input)?;
    let output = if output.exists() {
        fs::canonicalize(output)?
    } else {
        let parent = output.parent().filter(|path| !path.as_os_str().is_empty());
        let parent = fs::canonicalize(parent.unwrap_or(Path::new(".")))?;
        let file_name = output
            .file_name()
            .ok_or("reduce-vmd output must have a file name")?;
        parent.join(file_name)
    };
    #[cfg(windows)]
    let same = input
        .to_string_lossy()
        .eq_ignore_ascii_case(&output.to_string_lossy());
    #[cfg(not(windows))]
    let same = input == output;
    if same {
        Err("reduce-vmd input and output must be different files".into())
    } else {
        Ok(())
    }
}

fn reduced_bone_frames(
    reduced: &mmd_anim_runtime::ReducedPoseSequence,
    names: &[mmd_anim_format::VmdExportName],
) -> Vec<VmdParsedBoneFrame> {
    let mut frames = Vec::new();
    for (bone, track) in reduced.bone_tracks().iter().enumerate().take(names.len()) {
        for key in track.keys() {
            frames.push(VmdParsedBoneFrame {
                bone_name: names[bone].text.clone(),
                bone_name_bytes: names[bone].bytes.clone(),
                frame: key.sample_index as u32,
                translation: key.translation.to_array(),
                rotation: key.rotation.to_array(),
                interpolation: vmd_interpolation_block(key.vmd_interpolation).to_vec(),
            });
        }
    }
    frames.sort_by(|a, b| (a.frame, &a.bone_name_bytes).cmp(&(b.frame, &b.bone_name_bytes)));
    frames
}

fn reduced_morph_frames(
    reduced: &mmd_anim_runtime::ReducedPoseSequence,
    names: &[mmd_anim_format::VmdExportName],
) -> Vec<VmdParsedMorphFrame> {
    let mut frames = Vec::new();
    for (morph, track) in reduced.morph_tracks().iter().enumerate() {
        for key in track.keys() {
            frames.push(VmdParsedMorphFrame {
                morph_name: names[morph].text.clone(),
                morph_name_bytes: names[morph].bytes.clone(),
                frame: key.sample_index as u32,
                weight: key.weight,
            });
        }
    }
    frames.sort_by(|a, b| (a.frame, &a.morph_name_bytes).cmp(&(b.frame, &b.morph_name_bytes)));
    frames
}

fn prefer_smaller_bone_tracks(
    source: &[VmdParsedBoneFrame],
    candidate: Vec<VmdParsedBoneFrame>,
) -> Vec<VmdParsedBoneFrame> {
    let mut source_tracks: HashMap<Vec<u8>, Vec<VmdParsedBoneFrame>> = HashMap::new();
    let mut candidate_tracks: HashMap<Vec<u8>, Vec<VmdParsedBoneFrame>> = HashMap::new();
    for frame in source {
        source_tracks
            .entry(mmd_anim_format::normalize_vmd_name(&frame.bone_name_bytes))
            .or_default()
            .push(frame.clone());
    }
    for frame in candidate {
        candidate_tracks
            .entry(mmd_anim_format::normalize_vmd_name(&frame.bone_name_bytes))
            .or_default()
            .push(frame);
    }
    let mut frames = Vec::new();
    for (name, source_track) in source_tracks {
        let candidate_track = candidate_tracks.remove(&name).unwrap_or_default();
        if candidate_track.len() < source_track.len() {
            frames.extend(candidate_track);
        } else {
            frames.extend(source_track);
        }
    }
    frames.sort_by(|a, b| (a.frame, &a.bone_name_bytes).cmp(&(b.frame, &b.bone_name_bytes)));
    frames
}

fn prefer_smaller_morph_tracks(
    source: &[VmdParsedMorphFrame],
    candidate: Vec<VmdParsedMorphFrame>,
) -> Vec<VmdParsedMorphFrame> {
    let mut source_tracks: HashMap<Vec<u8>, Vec<VmdParsedMorphFrame>> = HashMap::new();
    let mut candidate_tracks: HashMap<Vec<u8>, Vec<VmdParsedMorphFrame>> = HashMap::new();
    for frame in source {
        source_tracks
            .entry(mmd_anim_format::normalize_vmd_name(&frame.morph_name_bytes))
            .or_default()
            .push(frame.clone());
    }
    for frame in candidate {
        candidate_tracks
            .entry(mmd_anim_format::normalize_vmd_name(&frame.morph_name_bytes))
            .or_default()
            .push(frame);
    }
    let mut frames = Vec::new();
    for (name, source_track) in source_tracks {
        let candidate_track = candidate_tracks.remove(&name).unwrap_or_default();
        if candidate_track.len() < source_track.len() {
            frames.extend(candidate_track);
        } else {
            frames.extend(source_track);
        }
    }
    frames.sort_by(|a, b| (a.frame, &a.morph_name_bytes).cmp(&(b.frame, &b.morph_name_bytes)));
    frames
}

fn vmd_interpolation_block(curves: mmd_anim_runtime::VmdBoneInterpolation) -> [u8; 64] {
    let curves = [
        curves.translation[0],
        curves.translation[1],
        curves.translation[2],
        curves.rotation,
    ];
    let mut first = [0u8; 16];
    for (channel, curve) in curves.into_iter().enumerate() {
        first[channel] = curve.x1.min(127);
        first[4 + channel] = curve.y1.min(127);
        first[8 + channel] = curve.x2.min(127);
        first[12 + channel] = curve.y2.min(127);
    }
    let mut block = [0u8; 64];
    for chunk in block.chunks_exact_mut(16) {
        chunk.copy_from_slice(&first);
    }
    block
}

fn unique_bone_names(
    animation: &mmd_anim_format::VmdParsedAnimation,
) -> (Vec<mmd_anim_format::VmdExportName>, HashMap<Vec<u8>, u32>) {
    let mut names = Vec::new();
    let mut indices = HashMap::new();
    for frame in &animation.bone_frames {
        let normalized = mmd_anim_format::normalize_vmd_name(&frame.bone_name_bytes);
        if let std::collections::hash_map::Entry::Vacant(entry) = indices.entry(normalized) {
            let index = names.len() as u32;
            entry.insert(index);
            names.push(mmd_anim_format::VmdExportName::new(
                frame.bone_name.clone(),
                frame.bone_name_bytes.clone(),
            ));
        }
    }
    if names.is_empty() {
        names.push(mmd_anim_format::VmdExportName::new("", Vec::new()));
    }
    (names, indices)
}

fn unique_morph_names(
    animation: &mmd_anim_format::VmdParsedAnimation,
) -> (Vec<mmd_anim_format::VmdExportName>, HashMap<Vec<u8>, u32>) {
    let mut names = Vec::new();
    let mut indices = HashMap::new();
    for frame in &animation.morph_frames {
        let normalized = mmd_anim_format::normalize_vmd_name(&frame.morph_name_bytes);
        if let std::collections::hash_map::Entry::Vacant(entry) = indices.entry(normalized) {
            let index = names.len() as u32;
            entry.insert(index);
            names.push(mmd_anim_format::VmdExportName::new(
                frame.morph_name.clone(),
                frame.morph_name_bytes.clone(),
            ));
        }
    }
    (names, indices)
}

#[cfg(test)]
mod tests;
