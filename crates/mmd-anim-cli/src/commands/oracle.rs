use std::{fs, process::ExitCode};

use crate::schema::MmdDumperOracleDump;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OracleSummaryMetrics {
    pub(crate) frames: usize,
    pub(crate) models: usize,
    pub(crate) first_model_bones: usize,
    pub(crate) first_model_morphs: usize,
}

pub(crate) fn oracle_summary_metrics(dump: &MmdDumperOracleDump) -> OracleSummaryMetrics {
    OracleSummaryMetrics {
        frames: dump.frames.len(),
        models: dump
            .frames
            .iter()
            .map(|frame| frame.models.len())
            .max()
            .unwrap_or(0),
        first_model_bones: dump
            .frames
            .first()
            .and_then(|frame| frame.models.first())
            .map(|model| model.bones.len())
            .unwrap_or(0),
        first_model_morphs: dump
            .frames
            .first()
            .and_then(|frame| frame.models.first())
            .map(|model| model.morphs.len())
            .unwrap_or(0),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OracleSummarySourceJson {
    mmd_version: String,
    dumper_version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OracleSummaryJsonReport {
    status: &'static str,
    command: &'static str,
    mode: &'static str,
    input: String,
    frames: usize,
    models: usize,
    first_model_bones: usize,
    first_model_morphs: usize,
    source: OracleSummarySourceJson,
}

pub(crate) fn oracle_summary_json_report(
    path: &str,
    dump: &MmdDumperOracleDump,
) -> OracleSummaryJsonReport {
    let metrics = oracle_summary_metrics(dump);
    OracleSummaryJsonReport {
        status: "ok",
        command: "verify",
        mode: "oracle",
        input: path.to_owned(),
        frames: metrics.frames,
        models: metrics.models,
        first_model_bones: metrics.first_model_bones,
        first_model_morphs: metrics.first_model_morphs,
        source: OracleSummarySourceJson {
            mmd_version: dump.source.mmd_version.clone(),
            dumper_version: dump.source.dumper_version.clone(),
        },
    }
}

pub(crate) fn oracle_summary(
    path: &str,
    use_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let dump = MmdDumperOracleDump::from_jsonl_str(&content, None)?;
    let metrics = oracle_summary_metrics(&dump);

    if use_json {
        println!(
            "{}",
            serde_json::to_string(&oracle_summary_json_report(path, &dump))?
        );
    } else {
        println!(
            "MMDDumper oracle: frames={} models={} firstModelBones={} firstModelMorphs={} mmd={} dumper={}",
            metrics.frames,
            metrics.models,
            metrics.first_model_bones,
            metrics.first_model_morphs,
            dump.source.mmd_version,
            dump.source.dumper_version
        );
    }
    Ok(ExitCode::SUCCESS)
}
