use std::{path::Path, process::ExitCode};

use crate::{read_file, write_file};

pub(crate) fn convert_pmx_to_fbx(
    input: &Path,
    output: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(input)?;
    let model = mmd_anim_format::parse_pmx_model(&data)?;
    let mut options = mmd_anim_format::fbx::FbxExportOptions::default();
    if !model.metadata.name.is_empty() {
        options.model_name.clone_from(&model.metadata.name);
    } else if let Some(stem) = input.file_stem().and_then(|value| value.to_str()) {
        options.model_name = stem.to_owned();
    }

    let fbx = mmd_anim_format::fbx::export_pmx_fbx_binary(&model, &options)?;
    write_file(output, &fbx)?;
    println!(
        "FBX export: ok input={} output={} bytesOut={} vertices={} faces={} materials={}",
        input.display(),
        output.display(),
        fbx.len(),
        model.metadata.counts.vertices,
        model.metadata.counts.faces,
        model.metadata.counts.materials
    );
    Ok(ExitCode::SUCCESS)
}
