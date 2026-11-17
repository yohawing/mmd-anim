use std::{path::Path, process::ExitCode};

use mmd_anim_format::pmm::PmmSceneFrameRangePatch;
use serde_json::json;

use crate::{parse_failure_error, read_file, write_file};

#[derive(Default)]
pub(crate) struct PmmFrameRangeArgs {
    pub current_frame: Option<i32>,
    pub current_frame_text: Option<i32>,
    pub begin_frame: Option<i32>,
    pub end_frame: Option<i32>,
    pub begin_frame_enabled: Option<String>,
    pub end_frame_enabled: Option<String>,
}

impl PmmFrameRangeArgs {
    pub(crate) fn has_any(&self) -> bool {
        self.current_frame.is_some()
            || self.current_frame_text.is_some()
            || self.begin_frame.is_some()
            || self.end_frame.is_some()
            || self.begin_frame_enabled.is_some()
            || self.end_frame_enabled.is_some()
    }

    pub(crate) fn to_patch(&self) -> Result<PmmSceneFrameRangePatch, String> {
        Ok(PmmSceneFrameRangePatch {
            current_frame_index: self.current_frame,
            current_frame_index_in_text_field: self.current_frame_text,
            begin_frame_index: self.begin_frame,
            end_frame_index: self.end_frame,
            begin_frame_index_enabled: self
                .begin_frame_enabled
                .as_deref()
                .map(|value| parse_bool_option(value, "--begin-frame-enabled"))
                .transpose()?,
            end_frame_index_enabled: self
                .end_frame_enabled
                .as_deref()
                .map(|value| parse_bool_option(value, "--end-frame-enabled"))
                .transpose()?,
        })
    }
}

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

fn count_pmm_scene_frame_range_patch_fields(patch: &PmmSceneFrameRangePatch) -> usize {
    [
        patch.current_frame_index.is_some(),
        patch.current_frame_index_in_text_field.is_some(),
        patch.begin_frame_index.is_some(),
        patch.end_frame_index.is_some(),
        patch.begin_frame_index_enabled.is_some(),
        patch.end_frame_index_enabled.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
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
    patch: &PmmSceneFrameRangePatch,
    bytes_in: usize,
    bytes_out: usize,
) -> serde_json::Value {
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
        "changedFields": patch_fields.len(),
        "patch": patch_fields,
    })
}

pub(crate) fn patch_pmm_scene_frame_range(
    input: &Path,
    output: &Path,
    patch: &PmmSceneFrameRangePatch,
    use_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(input)?;
    let bytes_in = data.len();
    let parsed = mmd_anim_format::parse_pmm_manifest(&data).map_err(|error| {
        parse_failure_error("patch", input, mmd_anim_format::MmdFormatKind::Pmm, error)
    })?;
    let exported =
        mmd_anim_format::pmm::export_pmm_manifest_with_scene_frame_range_patch(&parsed, patch)
            .map_err(|e| format!("PMM scene frame range patch failed: {e}"))?;
    let bytes_out = exported.len();
    write_file(output, &exported)?;
    if use_json {
        let report = patch_scene_frame_range_json(input, output, patch, bytes_in, bytes_out);
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        let changed_fields = count_pmm_scene_frame_range_patch_fields(patch);
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
