use std::{path::Path, process::ExitCode, sync::Arc};

use crate::{read_file, write_file};

pub(crate) fn convert_pmx_to_fbx(
    input: &Path,
    output: &Path,
    vmd: Option<&Path>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(input)?;
    let model = mmd_anim_format::parse_pmx_model(&data)?;
    let mut options = mmd_anim_format::fbx::FbxExportOptions::default();
    if !model.metadata.name.is_empty() {
        options.model_name.clone_from(&model.metadata.name);
    } else if let Some(stem) = input.file_stem().and_then(|value| value.to_str()) {
        options.model_name = stem.to_owned();
    }

    let fbx = if let Some(vmd_path) = vmd {
        let motion_data = read_file(vmd_path)?;
        let motion = mmd_anim_format::parse_vmd_animation(&motion_data)?;
        let runtime_import = mmd_anim_format::import_pmx_runtime(&data)?;
        let runtime_motion = mmd_anim_format::import_vmd_motion(&motion_data)?;
        let clip = mmd_anim_format::build_pair_clip(
            &runtime_motion,
            &runtime_import.bone_name_to_index,
            &runtime_import.morph_name_to_index,
            &runtime_import.ik_solver_bone_name_to_index,
            runtime_import.model.ik_count(),
        );
        let bone_max = motion
            .bone_frames
            .iter()
            .map(|f| f.frame)
            .max()
            .unwrap_or(0);
        let morph_max = motion
            .morph_frames
            .iter()
            .map(|f| f.frame)
            .max()
            .unwrap_or(0);
        let prop_max = motion
            .property_frames
            .iter()
            .map(|f| f.frame)
            .max()
            .unwrap_or(0);
        let last_frame = bone_max.max(morph_max).max(prop_max);
        mmd_anim_format::fbx::export_fbx_with_runtime_bake(
            &model,
            Arc::new(runtime_import.model),
            &clip,
            last_frame,
            &options,
        )?
    } else {
        mmd_anim_format::fbx::export_fbx(&model, None, &options)?
    };
    write_file(output, &fbx)?;
    println!(
        "FBX export: ok input={} output={} vmd={} bytesOut={} vertices={} faces={} materials={}",
        input.display(),
        output.display(),
        vmd.map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned()),
        fbx.len(),
        model.metadata.counts.vertices,
        model.metadata.counts.faces,
        model.metadata.counts.materials
    );
    Ok(ExitCode::SUCCESS)
}
