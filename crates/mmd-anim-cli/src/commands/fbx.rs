use std::{path::Path, process::ExitCode};

use crate::{read_file, write_file};

pub(crate) fn convert_pmx_to_fbx(
    input: &Path,
    output: &Path,
    vmd: Option<&Path>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(input)?;
    let model = mmd_anim_format::parse_pmx_model(&data)?;
    let motion_data = vmd.map(read_file).transpose()?;
    let motion = motion_data
        .as_deref()
        .map(mmd_anim_format::parse_vmd_animation)
        .transpose()?;
    let mut options = mmd_anim_format::fbx::FbxExportOptions::default();
    if !model.metadata.name.is_empty() {
        options.model_name.clone_from(&model.metadata.name);
    } else if let Some(stem) = input.file_stem().and_then(|value| value.to_str()) {
        options.model_name = stem.to_owned();
    }

    let fbx = mmd_anim_format::fbx::export_fbx(&model, motion.as_ref(), &options)?;
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
