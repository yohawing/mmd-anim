use std::{fs, process::ExitCode};

use mmd_anim_schema::MmdDumperOracleDump;

pub(crate) fn oracle_summary(path: &str) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let dump = MmdDumperOracleDump::from_jsonl_str(&content, None)?;
    let model_count = dump
        .frames
        .iter()
        .map(|frame| frame.models.len())
        .max()
        .unwrap_or(0);
    let bone_count = dump
        .frames
        .first()
        .and_then(|frame| frame.models.first())
        .map(|model| model.bones.len())
        .unwrap_or(0);
    let morph_count = dump
        .frames
        .first()
        .and_then(|frame| frame.models.first())
        .map(|model| model.morphs.len())
        .unwrap_or(0);

    println!(
        "MMDDumper oracle: frames={} models={} firstModelBones={} firstModelMorphs={} mmd={} dumper={}",
        dump.frames.len(),
        model_count,
        bone_count,
        morph_count,
        dump.source.mmd_version,
        dump.source.dumper_version
    );
    Ok(ExitCode::SUCCESS)
}
