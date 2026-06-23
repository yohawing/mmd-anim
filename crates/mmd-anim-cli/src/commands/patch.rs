use std::{fs, path::Path, process::ExitCode};

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

pub(crate) fn patch_pmm_scene_frame_range(
    input: &Path,
    output: &Path,
    option_args: &[String],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let patch = parse_pmm_scene_frame_range_patch_options(option_args)?;
    let data = fs::read(input)?;
    let bytes_in = data.len();
    let parsed = mmd_anim_format::parse_pmm_manifest(&data)?;
    let exported =
        mmd_anim_format::pmm::export_pmm_manifest_with_scene_frame_range_patch(&parsed, &patch)
            .map_err(|e| format!("PMM scene frame range patch failed: {e}"))?;
    let bytes_out = exported.len();
    let changed_fields = count_pmm_scene_frame_range_patch_fields(&patch);
    fs::write(output, &exported)?;
    println!(
        "PMM scene frame range patch: ok bytesIn={} bytesOut={} changedFields={} output={}",
        bytes_in,
        bytes_out,
        changed_fields,
        output.display()
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn patch_pmm_document_model_path(
    input: &Path,
    document_model_index: &str,
    model_path: &str,
    output: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(input)?;
    let bytes_in = data.len();
    let parsed = mmd_anim_format::parse_pmm_manifest(&data)?;
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
    fs::write(output, &exported)?;
    println!(
        "PMM document model path patch: ok bytesIn={} bytesOut={} documentModelIndex={} output={}",
        bytes_in,
        bytes_out,
        index,
        output.display()
    );
    Ok(ExitCode::SUCCESS)
}
