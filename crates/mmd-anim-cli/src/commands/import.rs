use std::{collections::BTreeMap, fs, path::Path, process::ExitCode, sync::Arc};

use glam::Vec3A;
use mmd_anim_runtime::RuntimeInstance;

use crate::{f32_checksum, translation_checksum};

pub(crate) fn import_pmx_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
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
    let data = fs::read(path)?;
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
    let data = fs::read(path)?;
    let imported = mmd_anim_format::import_pmd_runtime(&data)?;
    println!(
        "PMD runtime import: bones={} ik={} morphSlots={} vertexMorphOffsets={} boneNames={} morphNames={} ikNameMap={} diagnostics={}",
        imported.model.bone_count(),
        imported.model.ik_count(),
        imported.model.morph_count(),
        imported.model.vertex_morph_offsets().len(),
        imported.bone_name_to_index.len(),
        imported.morph_name_to_index.len(),
        imported.ik_solver_bone_name_to_index.len(),
        imported.diagnostics.len()
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_vmd_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
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
    let pmx = mmd_anim_format::import_pmx_runtime(&fs::read(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&fs::read(vmd_path)?)?;

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

    println!(
        "PMX/VMD runtime import: bones={} append={} fixedAxis={} ik={} vmdBoneKeys={} matchedBoneKeys={} vmdMorphKeys={} matchedMorphKeys={} propertyFrames={} propertyIkEntries={} matchedPropertyIkEntries={}",
        pmx.model.bone_count(),
        pmx.model.append_transforms().len(),
        pmx.model.fixed_axis_count(),
        pmx.model.ik_count(),
        vmd.bone_keyframes.len(),
        matched_bone_keys,
        vmd.morph_keyframes.len(),
        matched_morph_keys,
        vmd.property_ik_frames.len(),
        property_ik_entries,
        matched_property_ik_entries
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn import_pair_clip_summary(
    pmx_path: &Path,
    vmd_path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&fs::read(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&fs::read(vmd_path)?)?;

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
    let pmx = mmd_anim_format::import_pmx_runtime(&fs::read(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&fs::read(vmd_path)?)?;

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

    let model = Arc::new(pmx.model);
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);

    runtime.evaluate_clip_frame(&clip, frame);

    let world_matrices = runtime.world_matrices();

    let first_translation = if let Some(m) = world_matrices.first() {
        Vec3A::new(m.w_axis.x, m.w_axis.y, m.w_axis.z)
    } else {
        Vec3A::ZERO
    };

    let checksum = translation_checksum(world_matrices);
    let morph_weights = runtime.morph_weights();
    let nonzero_morphs = morph_weights
        .iter()
        .filter(|weight| weight.abs() > f32::EPSILON)
        .count();
    let morph_checksum = f32_checksum(morph_weights);

    println!(
        "PMX/VMD frame {:.3}: bones={} ik={} clipBoneTracks={} clipMorphTracks={} worldMatrices={} firstTranslation=({:.6},{:.6},{:.6}) translationChecksum={:08x} nonzeroMorphs={} morphChecksum={:08x}",
        frame,
        bone_count,
        solver_count,
        clip.bone_track_count(),
        clip.morph_track_count(),
        world_matrices.len(),
        first_translation.x,
        first_translation.y,
        first_translation.z,
        checksum,
        nonzero_morphs,
        morph_checksum,
    );

    Ok(ExitCode::SUCCESS)
}
