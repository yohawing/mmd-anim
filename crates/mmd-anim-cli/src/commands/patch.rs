use std::{path::Path, process::ExitCode};

use serde_json::json;

use crate::{parse_failure_error, read_file, write_file};

pub(crate) const PATCH_PMM_SCENE_FRAME_RANGE_USAGE: &str = "usage: mmd-anim patch-pmm-scene-frame-range <input.pmm> <output.pmm> [--current-frame <i32>] [--current-frame-text <i32>] [--begin-frame <i32>] [--end-frame <i32>] [--begin-frame-enabled <bool>] [--end-frame-enabled <bool>]";

fn parse_bool_option(value: &str, label: &str) -> Result<bool, String> {
    if value.eq_ignore_ascii_case("true")
        || value == "1"
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
    {
        Ok(true)
    } else if value.eq_ignore_ascii_case("false")
        || value == "0"
        || value.eq_ignore_ascii_case("no")
        || value.eq_ignore_ascii_case("off")
    {
        Ok(false)
    } else {
        Err(format!(
            "invalid {label}: {value} (expected true/false, 1/0, yes/no, or on/off)"
        ))
    }
}

pub(crate) fn parse_pmm_scene_frame_range_patch_options(
    option_args: &[String],
) -> Result<mmd_anim_format::pmm::PmmSceneFrameRangePatch, String> {
    let mut patch = mmd_anim_format::pmm::PmmSceneFrameRangePatch::default();
    let mut iter = option_args.iter();
    while let Some(arg) = iter.next() {
        let value = iter.next().ok_or_else(|| {
            format!("missing value for option {arg}; {PATCH_PMM_SCENE_FRAME_RANGE_USAGE}")
        })?;
        match arg.as_str() {
            "--current-frame" => {
                patch.current_frame_index = Some(value.parse().map_err(|_| {
                    format!("invalid --current-frame value: {value} (expected i32)")
                })?);
            }
            "--current-frame-text" => {
                patch.current_frame_index_in_text_field = Some(value.parse().map_err(|_| {
                    format!("invalid --current-frame-text value: {value} (expected i32)")
                })?);
            }
            "--begin-frame" => {
                patch.begin_frame_index =
                    Some(value.parse().map_err(|_| {
                        format!("invalid --begin-frame value: {value} (expected i32)")
                    })?);
            }
            "--end-frame" => {
                patch.end_frame_index =
                    Some(value.parse().map_err(|_| {
                        format!("invalid --end-frame value: {value} (expected i32)")
                    })?);
            }
            "--begin-frame-enabled" => {
                patch.begin_frame_index_enabled =
                    Some(parse_bool_option(value, "--begin-frame-enabled")?);
            }
            "--end-frame-enabled" => {
                patch.end_frame_index_enabled =
                    Some(parse_bool_option(value, "--end-frame-enabled")?);
            }
            other if other.starts_with("--") => {
                return Err(format!(
                    "unknown option {other}; {PATCH_PMM_SCENE_FRAME_RANGE_USAGE}"
                ));
            }
            other => {
                return Err(format!(
                    "unexpected positional argument {other:?}; {PATCH_PMM_SCENE_FRAME_RANGE_USAGE}"
                ));
            }
        }
    }

    if patch.current_frame_index.is_none()
        && patch.current_frame_index_in_text_field.is_none()
        && patch.begin_frame_index.is_none()
        && patch.end_frame_index.is_none()
        && patch.begin_frame_index_enabled.is_none()
        && patch.end_frame_index_enabled.is_none()
    {
        return Err(format!(
            "at least one patch option is required; {PATCH_PMM_SCENE_FRAME_RANGE_USAGE}"
        ));
    }

    Ok(patch)
}

pub(crate) fn count_pmm_scene_frame_range_patch_fields(
    patch: &mmd_anim_format::pmm::PmmSceneFrameRangePatch,
) -> usize {
    let mut count = 0usize;
    if patch.current_frame_index.is_some() {
        count += 1;
    }
    if patch.current_frame_index_in_text_field.is_some() {
        count += 1;
    }
    if patch.begin_frame_index.is_some() {
        count += 1;
    }
    if patch.end_frame_index.is_some() {
        count += 1;
    }
    if patch.begin_frame_index_enabled.is_some() {
        count += 1;
    }
    if patch.end_frame_index_enabled.is_some() {
        count += 1;
    }
    count
}

pub(crate) fn patch_document_model_path_json(
    input: &Path,
    output: &Path,
    document_model_index: u8,
    model_path: &str,
    bytes_in: usize,
    bytes_out: usize,
) -> serde_json::Value {
    json!({
        "status": "ok",
        "command": "patch",
        "mode": "document-model-path",
        "input": input.display().to_string(),
        "output": output.display().to_string(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "documentModelIndex": document_model_index,
        "modelPath": model_path,
    })
}

pub(crate) fn patch_scene_frame_range_json(
    input: &Path,
    output: &Path,
    patch: &mmd_anim_format::pmm::PmmSceneFrameRangePatch,
    bytes_in: usize,
    bytes_out: usize,
) -> serde_json::Value {
    let changed_fields = count_pmm_scene_frame_range_patch_fields(patch);
    let mut patch_fields = serde_json::Map::new();
    if let Some(value) = patch.current_frame_index {
        patch_fields.insert("currentFrame".to_owned(), json!(value));
    }
    if let Some(value) = patch.current_frame_index_in_text_field {
        patch_fields.insert("currentFrameText".to_owned(), json!(value));
    }
    if let Some(value) = patch.begin_frame_index {
        patch_fields.insert("beginFrame".to_owned(), json!(value));
    }
    if let Some(value) = patch.end_frame_index {
        patch_fields.insert("endFrame".to_owned(), json!(value));
    }
    if let Some(value) = patch.begin_frame_index_enabled {
        patch_fields.insert("beginFrameEnabled".to_owned(), json!(value));
    }
    if let Some(value) = patch.end_frame_index_enabled {
        patch_fields.insert("endFrameEnabled".to_owned(), json!(value));
    }
    json!({
        "status": "ok",
        "command": "patch",
        "mode": "scene-frame-range",
        "input": input.display().to_string(),
        "output": output.display().to_string(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "changedFields": changed_fields,
        "patch": patch_fields,
    })
}

pub(crate) fn patch_pmm_scene_frame_range(
    input: &Path,
    output: &Path,
    option_args: &[String],
    use_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let patch = parse_pmm_scene_frame_range_patch_options(option_args)?;
    let data = read_file(input)?;
    let bytes_in = data.len();
    let parsed = mmd_anim_format::parse_pmm_manifest(&data).map_err(|error| {
        parse_failure_error("patch", input, mmd_anim_format::MmdFormatKind::Pmm, error)
    })?;
    let exported =
        mmd_anim_format::pmm::export_pmm_manifest_with_scene_frame_range_patch(&parsed, &patch)
            .map_err(|e| format!("PMM scene frame range patch failed: {e}"))?;
    let bytes_out = exported.len();
    write_file(output, &exported)?;
    if use_json {
        let report = patch_scene_frame_range_json(input, output, &patch, bytes_in, bytes_out);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let changed_fields = count_pmm_scene_frame_range_patch_fields(&patch);
        println!(
            "PMM scene frame range patch: ok bytesIn={} bytesOut={} changedFields={} output={}",
            bytes_in,
            bytes_out,
            changed_fields,
            output.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn patch_pmm_document_model_path(
    input: &Path,
    document_model_index: &str,
    model_path: &str,
    output: &Path,
    use_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(input)?;
    let bytes_in = data.len();
    let parsed = mmd_anim_format::parse_pmm_manifest(&data).map_err(|error| {
        parse_failure_error("patch", input, mmd_anim_format::MmdFormatKind::Pmm, error)
    })?;
    let index: u8 = document_model_index.parse().map_err(|_| {
        format!(
            "invalid document-model-index {:?}: expected u8 (0-255)",
            document_model_index
        )
    })?;
    let exported = mmd_anim_format::pmm::export_pmm_manifest_with_document_model_path_overrides(
        &parsed,
        &[(index, model_path)],
    )
    .map_err(|e| format!("PMM document model path patch failed: {e}"))?;
    let bytes_out = exported.len();
    write_file(output, &exported)?;
    if use_json {
        let report =
            patch_document_model_path_json(input, output, index, model_path, bytes_in, bytes_out);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "PMM document model path patch: ok bytesIn={} bytesOut={} documentModelIndex={} output={}",
            bytes_in,
            bytes_out,
            index,
            output.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}
