use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use crate::schema::{
    DEFAULT_FOCUSED_IK_BONE_NAMES, GoldenIkBatchManifest, GoldenIkFixture, MmdDumperOracleDump,
    MmdDumperOracleModel,
};
use glam::Vec3A;
use mmd_anim_format::VmdClipBuildOptions;
use mmd_anim_runtime::{BoneIndex, IkSolver, ModelArena, MorphIndex, RuntimeInstance};
use serde::Serialize;

pub(crate) const GOLDEN_IK_COMPARE_USAGE: &str =
    "usage: mmd-anim golden-ik-compare <golden-ik-oracle-root> [sample-frame-offset]";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GoldenImportDiagnostic {
    level: String,
    code: String,
    message: String,
}

pub(crate) struct RuntimeModelImport {
    pub(crate) model: ModelArena,
    pub(crate) bone_names: Vec<String>,
    pub(crate) bone_name_to_index: HashMap<Vec<u8>, BoneIndex>,
    pub(crate) morph_name_to_index: HashMap<Vec<u8>, MorphIndex>,
    pub(crate) ik_solver_bone_name_to_index: HashMap<Vec<u8>, usize>,
    pub(crate) diagnostics: Vec<GoldenImportDiagnostic>,
}

pub(crate) fn parse_golden_ik_compare_args(
    args: &mut impl Iterator<Item = String>,
) -> Result<(String, f32, bool), String> {
    let raw: Vec<String> = args.collect();
    let mut use_json = false;
    let mut positional = Vec::new();

    for token in &raw {
        if token == "--json" {
            use_json = true;
        } else if token.starts_with("--") {
            return Err(format!("unknown flag: {token}"));
        } else {
            positional.push(token.clone());
        }
    }

    let mut pos_iter = positional.into_iter();
    let root = match pos_iter.next() {
        Some(value) => value,
        None => {
            return Err(GOLDEN_IK_COMPARE_USAGE.to_owned());
        }
    };
    let offset = match pos_iter.next() {
        Some(value) => match value.parse::<f32>() {
            Ok(parsed) => parsed,
            Err(_) => {
                return Err(format!("invalid sample-frame-offset: {value}"));
            }
        },
        None => 0.0,
    };
    if let Some(extra) = pos_iter.next() {
        return Err(format!("unexpected extra argument: {extra}"));
    }

    Ok((root, offset, use_json))
}

pub(crate) fn golden_ik_summary(root: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&crate::read_text_file(&manifest_path)?)?;
    let mut parsed_cases = 0usize;
    let mut parsed_frames = 0usize;
    let mut parsed_bones = 0usize;
    let mut focused_frame_hits = 0usize;
    let mut missing = Vec::new();

    for case in &manifest.cases {
        let case_root = root.join(&case.name);
        let fixture_path = case_root.join("fixture.json");
        if !fixture_path.exists() {
            missing.push(fixture_path);
            continue;
        }

        let fixture = GoldenIkFixture::from_json_str(&crate::read_text_file(&fixture_path)?)?;
        let oracle_path = crate::resolve_maybe_absolute(&case_root, &fixture.output);
        if !oracle_path.exists() {
            missing.push(oracle_path);
            continue;
        }

        let frames = if fixture.frames.is_empty() {
            case.frames.as_slice()
        } else {
            fixture.frames.as_slice()
        };
        let dump = MmdDumperOracleDump::from_jsonl_str(
            &crate::read_text_file(&oracle_path)?,
            Some(frames),
        )?;
        parsed_cases += 1;
        parsed_frames += dump.frames.len();
        parsed_bones += dump
            .frames
            .first()
            .and_then(|frame| frame.models.first())
            .map(|model| model.bones.len())
            .unwrap_or(0);
        for frame in &dump.frames {
            let focused_count = frame
                .models
                .first()
                .map(|model| {
                    model
                        .focused_ik_bones(DEFAULT_FOCUSED_IK_BONE_NAMES)
                        .count()
                })
                .unwrap_or(0);
            if focused_count == 0 {
                return Err(format!(
                    "{} frame={} has no focused IK bones",
                    case.name, frame.frame
                )
                .into());
            }
            focused_frame_hits += 1;
        }
    }

    if !missing.is_empty() {
        for path in missing {
            eprintln!("missing: {}", path.display());
        }
        return Err("one or more golden IK oracle files are missing".into());
    }

    println!(
        "MMDDumper golden IK: cases={} selectedFrames={} firstFrameBoneTotal={} focusedFrameHits={}",
        parsed_cases, parsed_frames, parsed_bones, focused_frame_hits
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn golden_parser_summary(root: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&crate::read_text_file(&manifest_path)?)?;
    let mut parsed_cases = 0usize;
    let mut skipped_unsupported = 0usize;
    let mut missing_files = Vec::new();
    let mut matched_bones = 0usize;
    let mut missing_bones = 0usize;
    let mut matched_morphs = 0usize;
    let mut missing_morphs = 0usize;

    for case in &manifest.cases {
        let pmx_path = PathBuf::from(&case.pmx);
        if pmx_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("pmx"))
        {
            skipped_unsupported += 1;
            continue;
        }
        if !pmx_path.exists() {
            missing_files.push(pmx_path);
            continue;
        }

        let case_root = root.join(&case.name);
        let fixture_path = case_root.join("fixture.json");
        if !fixture_path.exists() {
            missing_files.push(fixture_path);
            continue;
        }
        let fixture = GoldenIkFixture::from_json_str(&crate::read_text_file(&fixture_path)?)?;
        let oracle_path = crate::resolve_maybe_absolute(&case_root, &fixture.output);
        if !oracle_path.exists() {
            missing_files.push(oracle_path);
            continue;
        }

        let parsed = mmd_anim_format::parse_pmx_model(&crate::read_file(&pmx_path)?)?;
        let bone_names = parsed
            .skeleton
            .bones
            .iter()
            .map(|bone| bone.name.as_str())
            .collect::<HashSet<_>>();
        let morph_names = parsed
            .morphs
            .iter()
            .map(|morph| morph.name.as_str())
            .collect::<HashSet<_>>();

        let frames = if fixture.frames.is_empty() {
            case.frames.as_slice()
        } else {
            fixture.frames.as_slice()
        };
        let dump = MmdDumperOracleDump::from_jsonl_str(
            &crate::read_text_file(&oracle_path)?,
            Some(frames),
        )?;
        parsed_cases += 1;

        let Some(model) = dump.frames.first().and_then(|frame| frame.models.first()) else {
            continue;
        };
        for bone in &model.bones {
            if bone_names.contains(bone.name.as_str()) {
                matched_bones += 1;
            } else {
                missing_bones += 1;
            }
        }
        for morph in &model.morphs {
            if morph_names.contains(morph.name.as_str()) {
                matched_morphs += 1;
            } else {
                missing_morphs += 1;
            }
        }
    }

    if !missing_files.is_empty() {
        for path in missing_files {
            eprintln!("missing: {}", path.display());
        }
        return Err("one or more Golden parser files are missing".into());
    }

    println!(
        "MMDDumper parser golden: cases={} skippedUnsupported={} matchedBones={} missingBones={} matchedMorphs={} missingMorphs={}",
        parsed_cases,
        skipped_unsupported,
        matched_bones,
        missing_bones,
        matched_morphs,
        missing_morphs
    );
    Ok(ExitCode::SUCCESS)
}

/// Root/control bone names to watch for translation deltas that indicate
/// root-motion or scene capture mismatches rather than IK solver errors.
const ROOT_MOTION_WATCH_BONES: &[&str] = &[
    "全ての親",
    "センター",
    "グルーブ",
    "左足ＩＫ",
    "右足ＩＫ",
    "左つま先ＩＫ",
    "右つま先ＩＫ",
];

/// Translation-component threshold for reporting a root/control bone mismatch.
/// Raised from 0.001 after real reports showed sub-0.1 deltas were noisy.
/// The important real mismatch to still catch is around 12.0 for the
/// rem-proseka-miku-vs-marine-mirage frame-300 center bone.
const ROOT_MOTION_DIAGNOSTIC_THRESHOLD: f32 = 0.1;

/// Ratio threshold for determining if a case+frame is root/control dominated.
/// If any diagnostic on the same case+frame has maxAbsError >= this ratio
/// times the frame's max matrix abs_error, the frame's focused bone comparisons
/// are excluded from solver-focused metrics.
const ROOT_CONTROL_DOMINATED_RATIO: f64 = 0.5;

/// Absolute maxAbsError threshold for `root_motion_mismatch` diagnostics to
/// dominate a frame regardless of the ratio rule. A root-motion error at or
/// above this value indicates the frame's focused-bone error is not a true IK
/// solver error but a capture mismatch propagated through the bone hierarchy.
const ROOT_MOTION_DOMINATED_ABS_THRESHOLD: f64 = 1.0;

/// Threshold for detecting oracle payload lag: the max component delta between
/// a diagnostic's oracleTranslation and the previous diagnostic's
/// runtimeTranslation. When the oracle sample payload lags one target frame,
/// the oracle translation at frame N matches the runtime translation at frame
/// N-1 even though both records carry the correct frame number.
const ORACLE_LAG_DELTA_THRESHOLD: f64 = 0.001;

/// Scan oracle model + runtime world matrices for large translation deltas
/// on root/control bones. Returns zero or more diagnostic entries, each with
/// bone name, frame, runtime/oracle translation, delta, max component error,
/// and a short classification.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GoldenRootMotionDiagnostic {
    bone: String,
    frame: i32,
    runtime_translation: [f32; 3],
    oracle_translation: [f32; 3],
    delta: [f32; 3],
    max_abs_error: f32,
    classification: &'static str,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GoldenRootMotionOracleLagMatch {
    case: String,
    bone: String,
    frame: i32,
    previous_frame: i32,
    #[serde(rename = "maxAbsError")]
    max_abs_error: f32,
    match_delta: f64,
}

#[derive(Clone, Serialize)]
pub(crate) struct GoldenRootMotionOracleLag {
    #[serde(rename = "matchCount")]
    match_count: usize,
    matches: Vec<GoldenRootMotionOracleLagMatch>,
}

pub(crate) fn compute_root_motion_diagnostics(
    oracle_model: &MmdDumperOracleModel,
    world_matrices: &[glam::Mat4],
    frame: i32,
) -> Vec<GoldenRootMotionDiagnostic> {
    let mut diagnostics = Vec::new();

    for &bone_name in ROOT_MOTION_WATCH_BONES {
        let Some(bone) = oracle_model.find_bone(bone_name) else {
            continue;
        };
        if bone.index < 0 {
            continue;
        }
        let index = bone.index as usize;
        if index >= world_matrices.len() {
            continue;
        }

        let rt_t = {
            let w = world_matrices[index].w_axis;
            glam::Vec3A::new(w.x, w.y, w.z)
        };
        let or_t = glam::Vec3A::new(
            bone.world_matrix[12],
            bone.world_matrix[13],
            bone.world_matrix[14],
        );
        let delta = rt_t - or_t;
        let max_abs = delta.x.abs().max(delta.y.abs().max(delta.z.abs()));

        if max_abs > ROOT_MOTION_DIAGNOSTIC_THRESHOLD {
            let classification = match bone_name {
                "全ての親" | "センター" | "グルーブ" => "root_motion_mismatch",
                _ => "control_bone_mismatch",
            };

            diagnostics.push(GoldenRootMotionDiagnostic {
                bone: bone_name.to_owned(),
                frame,
                runtime_translation: [rt_t.x, rt_t.y, rt_t.z],
                oracle_translation: [or_t.x, or_t.y, or_t.z],
                delta: [delta.x, delta.y, delta.z],
                max_abs_error: max_abs,
                classification,
            });
        }
    }

    diagnostics
}

/// Detect root-motion oracle sampling lag by checking whether a diagnostic's
/// oracleTranslation matches the previous frame's runtimeTranslation for the
/// same bone. This pattern reveals that the oracle sample payload can lag one
/// target frame even when the record frame number is correct.
///
/// Only diagnostics with classification `"root_motion_mismatch"` are inspected.
/// A match is counted when the max component delta between `curr.oracleTranslation`
/// and `prev.runtimeTranslation` is <= `ORACLE_LAG_DELTA_THRESHOLD`.
pub(crate) fn compute_root_motion_oracle_lag(
    case_name: &str,
    diagnostics: &[GoldenRootMotionDiagnostic],
) -> GoldenRootMotionOracleLag {
    use std::collections::BTreeMap;

    // Filter to root_motion_mismatch only
    let root_motion: Vec<&GoldenRootMotionDiagnostic> = diagnostics
        .iter()
        .filter(|d| d.classification == "root_motion_mismatch")
        .collect();

    // Group by bone name
    let mut by_bone: BTreeMap<&str, Vec<&GoldenRootMotionDiagnostic>> = BTreeMap::new();
    for d in &root_motion {
        by_bone.entry(d.bone.as_str()).or_default().push(d);
    }

    let mut matches: Vec<GoldenRootMotionOracleLagMatch> = Vec::new();

    for (_bone, entries) in by_bone.iter_mut() {
        // Sort by frame ascending
        entries.sort_by_key(|d| d.frame);

        for window in entries.windows(2) {
            let prev = window[0];
            let curr = window[1];

            let dx = (f64::from(curr.oracle_translation[0])
                - f64::from(prev.runtime_translation[0]))
            .abs();
            let dy = (f64::from(curr.oracle_translation[1])
                - f64::from(prev.runtime_translation[1]))
            .abs();
            let dz = (f64::from(curr.oracle_translation[2])
                - f64::from(prev.runtime_translation[2]))
            .abs();
            let max_delta = dx.max(dy).max(dz);

            if max_delta <= ORACLE_LAG_DELTA_THRESHOLD {
                matches.push(GoldenRootMotionOracleLagMatch {
                    case: case_name.to_owned(),
                    bone: curr.bone.clone(),
                    frame: curr.frame,
                    previous_frame: prev.frame,
                    max_abs_error: curr.max_abs_error,
                    match_delta: max_delta,
                });
            }
        }
    }

    GoldenRootMotionOracleLag {
        match_count: matches.len(),
        matches,
    }
}

/// Return true when any root/control diagnostic dominates the frame's
/// focused-bone matrix error, meaning the frame should be excluded from
/// solver-focused metrics.
///
/// A diagnostic dominates when:
/// - Its maxAbsError is at least `ROOT_CONTROL_DOMINATED_RATIO` times the
///   frame's max matrix abs error (ratio rule, any classification).
/// - Its classification is `"root_motion_mismatch"` and maxAbsError is at
///   least `ROOT_MOTION_DOMINATED_ABS_THRESHOLD` (absolute rule).
///
/// Returns `false` when `frame_max_error <= 0.0`.
fn is_frame_root_control_dominated(
    frame_max_error: f32,
    frame_diagnostics: &[GoldenRootMotionDiagnostic],
) -> bool {
    if frame_max_error <= 0.0 {
        return false;
    }
    frame_diagnostics.iter().any(|d| {
        let abs_err = f64::from(d.max_abs_error);
        // Ratio rule: a diagnostic error >= 50% of frame max error
        // dominates regardless of classification.
        abs_err >= ROOT_CONTROL_DOMINATED_RATIO * frame_max_error as f64
            // Absolute threshold: a root_motion_mismatch error at or
            // above ROOT_MOTION_DOMINATED_ABS_THRESHOLD dominates
            // even when the ratio check fails (capture mismatch
            // propagated through hierarchy).
            || (d.classification == "root_motion_mismatch"
                && abs_err >= ROOT_MOTION_DOMINATED_ABS_THRESHOLD)
    })
}

/// Compute per-solver IK residual (distance between ikBone and targetBone
/// world-matrix translations) for both the runtime solver result and the
/// oracle (MMD) reference.  This separates end-effector convergence quality
/// from per-bone world-matrix error: a solver can converge (small residual)
/// while individual link poses still differ from the oracle.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GoldenIkSolverResidual {
    solver_index: usize,
    ik_bone: String,
    ik_bone_index: u32,
    target_bone: String,
    target_bone_index: u32,
    enabled: bool,
    runtime_residual: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    oracle_residual: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    residual_delta: Option<f32>,
}

pub(crate) fn compute_ik_solver_residuals(
    ik_solvers: &[IkSolver],
    bone_names: &[String],
    ik_enabled: &[u8],
    world_matrices: &[glam::Mat4],
    oracle_model: &MmdDumperOracleModel,
    focus_bone_index: Option<usize>,
) -> Vec<GoldenIkSolverResidual> {
    let mut residuals = Vec::with_capacity(ik_solvers.len());

    for (solver_idx, solver) in ik_solvers.iter().enumerate() {
        let ik_idx = solver.ik_bone.as_usize();
        let tb_idx = solver.target_bone.as_usize();

        if let Some(focus) = focus_bone_index {
            let is_involved = ik_idx == focus
                || tb_idx == focus
                || solver
                    .links
                    .iter()
                    .any(|link| link.bone.as_usize() == focus);
            if !is_involved {
                continue;
            }
        }

        if ik_idx >= world_matrices.len() || tb_idx >= world_matrices.len() {
            continue;
        }

        let rt_ik = glam::Vec3A::new(
            world_matrices[ik_idx].w_axis.x,
            world_matrices[ik_idx].w_axis.y,
            world_matrices[ik_idx].w_axis.z,
        );
        let rt_tb = glam::Vec3A::new(
            world_matrices[tb_idx].w_axis.x,
            world_matrices[tb_idx].w_axis.y,
            world_matrices[tb_idx].w_axis.z,
        );
        let runtime_residual = (rt_ik - rt_tb).length();

        let oracle_residual = {
            let or_ik = oracle_model
                .bones
                .iter()
                .find(|b| b.index == solver.ik_bone.0 as i32);
            let or_tb = oracle_model
                .bones
                .iter()
                .find(|b| b.index == solver.target_bone.0 as i32);
            match (or_ik, or_tb) {
                (Some(ik), Some(tb)) => {
                    let oi = glam::Vec3A::new(
                        ik.world_matrix[12],
                        ik.world_matrix[13],
                        ik.world_matrix[14],
                    );
                    let ot = glam::Vec3A::new(
                        tb.world_matrix[12],
                        tb.world_matrix[13],
                        tb.world_matrix[14],
                    );
                    Some((oi - ot).length())
                }
                _ => None,
            }
        };

        let ik_name = bone_names.get(ik_idx).map(|s| s.as_str()).unwrap_or("?");
        let tb_name = bone_names.get(tb_idx).map(|s| s.as_str()).unwrap_or("?");

        residuals.push(GoldenIkSolverResidual {
            solver_index: solver_idx,
            ik_bone: ik_name.to_owned(),
            ik_bone_index: solver.ik_bone.0,
            target_bone: tb_name.to_owned(),
            target_bone_index: solver.target_bone.0,
            enabled: ik_enabled.get(solver_idx).copied().unwrap_or(1) != 0,
            runtime_residual,
            oracle_residual,
            residual_delta: oracle_residual.map(|or| runtime_residual - or),
        });
    }

    residuals
}

#[derive(Serialize)]
struct UnsupportedGoldenCaseSummaryEntry {
    name: String,
    model: String,
    extension: String,
    reason: String,
}

#[derive(Serialize)]
struct UnsupportedGoldenCasePerCaseEntry {
    name: String,
    status: &'static str,
    model: String,
    reason: String,
    #[serde(rename = "maxAbsError")]
    max_abs_error: f32,
    worst: &'static str,
    #[serde(rename = "rootMotionOracleLag")]
    root_motion_oracle_lag: GoldenRootMotionOracleLag,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenIkComparePerCaseEntry {
    name: String,
    #[serde(rename = "maxAbsError")]
    max_abs_error: f32,
    worst: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<GoldenRootMotionDiagnostic>,
    #[serde(rename = "importDiagnostics", skip_serializing_if = "Vec::is_empty")]
    import_diagnostics: Vec<GoldenImportDiagnostic>,
    #[serde(rename = "rootMotionOracleLag")]
    root_motion_oracle_lag: GoldenRootMotionOracleLag,
}

#[derive(Serialize)]
#[serde(untagged)]
enum GoldenIkCompareCaseEntry {
    Unsupported(UnsupportedGoldenCasePerCaseEntry),
    Compared(GoldenIkComparePerCaseEntry),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenIkCompareJsonReport {
    command: &'static str,
    root: String,
    sample_frame_offset: f32,
    summary: GoldenIkCompareJsonSummary,
    #[serde(rename = "perCase")]
    per_case: Vec<GoldenIkCompareCaseEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenIkCompareJsonSummary {
    cases: usize,
    compared_cases: usize,
    skipped_unsupported: usize,
    skipped_unsupported_cases: Vec<UnsupportedGoldenCaseSummaryEntry>,
    missing: usize,
    import_errors: usize,
    compared_frames: usize,
    compared_bones: usize,
    max_abs_error: f32,
    worst: String,
    worst_component: usize,
    worst_component_type: &'static str,
    worst_case_max_error: f32,
    diagnostics_total: usize,
    worst_diagnostic: Option<GoldenRootMotionDiagnostic>,
    worst_likely_root_control_dominated: bool,
    solver_focused: GoldenIkCompareSolverFocusedSummary,
    root_motion_oracle_lag: GoldenIkCompareRootMotionOracleLagSummary,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenIkCompareSolverFocusedSummary {
    compared_bones: usize,
    skipped_bones: usize,
    skipped_frames: usize,
    max_abs_error: f32,
    worst: String,
    worst_component: usize,
    worst_component_type: &'static str,
    worst_case_max_error: f32,
    worst_frame_solver_residuals: Vec<GoldenIkSolverResidual>,
    root_motion_dominated_abs_threshold: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoldenIkCompareRootMotionOracleLagSummary {
    total_match_count: usize,
    worst_match: Option<GoldenRootMotionOracleLagMatch>,
}

fn golden_component_type(component: usize) -> &'static str {
    match component {
        12..=14 => "translation",
        15 => "homogeneous",
        _ => "rotation",
    }
}

/// Build a JSON pair for an unsupported (non-.pmx) case.
///
/// Returns `(summary_entry, per_case_entry)` so callers can push them into
/// the summary `skippedUnsupportedCases` list and `perCase` list respectively.
fn make_unsupported_case_entry(
    pmx_path: &Path,
    case_name: &str,
) -> (
    UnsupportedGoldenCaseSummaryEntry,
    UnsupportedGoldenCasePerCaseEntry,
) {
    let ext = pmx_path.extension().and_then(|e| e.to_str()).unwrap_or("?");
    let model_name = pmx_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let reason = format!("unsupported model format: only .pmx and .pmd are supported (got .{ext})");

    let summary = UnsupportedGoldenCaseSummaryEntry {
        name: case_name.to_owned(),
        model: model_name.to_owned(),
        extension: ext.to_owned(),
        reason: reason.clone(),
    };
    let per_case = UnsupportedGoldenCasePerCaseEntry {
        name: case_name.to_owned(),
        status: "skipped",
        model: model_name.to_owned(),
        reason,
        max_abs_error: 0.0,
        worst: "",
        root_motion_oracle_lag: GoldenRootMotionOracleLag {
            match_count: 0,
            matches: Vec::new(),
        },
    };

    (summary, per_case)
}

pub(crate) fn is_supported_golden_model(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("pmx" | "pmd")
    )
}

pub(crate) fn import_golden_runtime_model(
    path: &Path,
    bytes: &[u8],
) -> Result<RuntimeModelImport, mmd_anim_format::error::ImportError> {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("pmd") => {
            let import = mmd_anim_format::import_pmd_runtime(bytes)?;
            Ok(RuntimeModelImport {
                model: import.model,
                bone_names: import.bone_names,
                bone_name_to_index: import.bone_name_to_index,
                morph_name_to_index: import.morph_name_to_index,
                ik_solver_bone_name_to_index: import.ik_solver_bone_name_to_index,
                diagnostics: import
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| GoldenImportDiagnostic {
                        level: diagnostic.level,
                        code: diagnostic.code,
                        message: diagnostic.message,
                    })
                    .collect(),
            })
        }
        _ => {
            let import = mmd_anim_format::import_pmx_runtime(bytes)?;
            Ok(RuntimeModelImport {
                model: import.model,
                bone_names: import.bone_names,
                bone_name_to_index: import.bone_name_to_index,
                morph_name_to_index: import.morph_name_to_index,
                ik_solver_bone_name_to_index: import.ik_solver_bone_name_to_index,
                diagnostics: Vec::new(),
            })
        }
    }
}

pub(crate) fn golden_ik_compare(
    root: &Path,
    sample_frame_offset: f32,
    use_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&fs::read_to_string(&manifest_path)?)?;

    let mut cases = 0usize;
    let mut compared_cases = 0usize;
    let mut skipped_unsupported = 0usize;
    let mut skipped_unsupported_cases: Vec<UnsupportedGoldenCaseSummaryEntry> = Vec::new();
    let mut per_case_entries: Vec<GoldenIkCompareCaseEntry> = Vec::new();
    let mut missing = 0usize;
    let mut import_errors = 0usize;
    let mut compared_frames = 0usize;
    let mut compared_bones = 0usize;
    let mut max_abs_error: f32 = 0.0;
    let mut worst = String::from("none");
    let mut worst_case_max_error: f32 = 0.0;
    let mut worst_component: usize = 0;
    let mut worst_case_name = String::new();
    let mut worst_frame: i32 = 0;

    let mut per_case_errors: Vec<(String, f32, String)> = Vec::new();
    let mut per_case_diagnostics: Vec<Vec<GoldenRootMotionDiagnostic>> = Vec::new();
    let mut all_lag_matches: Vec<GoldenRootMotionOracleLagMatch> = Vec::new();

    // Solver-focused tracking (excludes root/control-dominated frames)
    let mut solver_compared_bones: usize = 0;
    let mut solver_skipped_bones: usize = 0;
    let mut solver_skipped_frames: usize = 0;
    let mut solver_max_abs_error: f32 = 0.0;
    let mut solver_worst = String::from("none");
    let mut solver_worst_component: usize = 0;
    let mut solver_worst_case_max_error: f32 = 0.0;
    let mut solver_worst_residuals: Vec<GoldenIkSolverResidual> = Vec::new();

    for case in &manifest.cases {
        cases += 1;

        let case_root = root.join(&case.name);
        let pmx_path = case_root.join(&case.pmx);

        if !is_supported_golden_model(&pmx_path) {
            skipped_unsupported += 1;
            let (summary, per_case) = make_unsupported_case_entry(&pmx_path, &case.name);
            skipped_unsupported_cases.push(summary);
            per_case_entries.push(GoldenIkCompareCaseEntry::Unsupported(per_case));
            continue;
        }

        let vmd_path = case_root.join(&case.vmd);
        let fixture_path = case_root.join("fixture.json");

        if !pmx_path.exists() || !vmd_path.exists() || !fixture_path.exists() {
            missing += 1;
            if !pmx_path.exists() {
                eprintln!("missing: {}", pmx_path.display());
            }
            if !vmd_path.exists() {
                eprintln!("missing: {}", vmd_path.display());
            }
            if !fixture_path.exists() {
                eprintln!("missing: {}", fixture_path.display());
            }
            continue;
        }

        let fixture = GoldenIkFixture::from_json_str(&fs::read_to_string(&fixture_path)?)?;
        let oracle_path = crate::resolve_maybe_absolute(&case_root, &fixture.output);
        if !oracle_path.exists() {
            missing += 1;
            eprintln!("missing: {}", oracle_path.display());
            continue;
        }

        let frames = if fixture.frames.is_empty() {
            case.frames.as_slice()
        } else {
            fixture.frames.as_slice()
        };

        let dump =
            MmdDumperOracleDump::from_jsonl_str(&fs::read_to_string(&oracle_path)?, Some(frames))?;

        let model_bytes = fs::read(&pmx_path)?;
        let model_import = match import_golden_runtime_model(&pmx_path, &model_bytes) {
            Ok(import) => import,
            Err(error) => {
                import_errors += 1;
                eprintln!("import-error: {}: {}", pmx_path.display(), error);
                continue;
            }
        };
        let vmd_bytes = fs::read(&vmd_path)?;
        let vmd = match mmd_anim_format::import_vmd_motion(&vmd_bytes) {
            Ok(vmd) => vmd,
            Err(error) => {
                import_errors += 1;
                eprintln!("import-error: {}: {}", vmd_path.display(), error);
                continue;
            }
        };

        let solver_count = model_import.model.ik_count();
        let clip = mmd_anim_format::build_pair_clip_with_options(
            &vmd,
            &model_import.bone_name_to_index,
            &model_import.morph_name_to_index,
            &model_import.ik_solver_bone_name_to_index,
            solver_count,
            VmdClipBuildOptions {
                honor_property_ik: false,
            },
        );

        let model = Arc::new(model_import.model);
        let morph_count = model_import
            .morph_name_to_index
            .values()
            .map(|index| index.as_usize() + 1)
            .max()
            .unwrap_or(0);
        let mut runtime =
            RuntimeInstance::new_with_counts(Arc::clone(&model), morph_count, solver_count);

        let mut case_max_error: f32 = 0.0;
        let mut case_worst = String::new();
        let mut case_diagnostics: Vec<GoldenRootMotionDiagnostic> = Vec::new();

        for oracle_frame in &dump.frames {
            let sample_frame = oracle_frame.frame as f32 + sample_frame_offset;
            runtime.evaluate_clip_frame(&clip, sample_frame);

            let model0 = match oracle_frame.models.first() {
                Some(m) => m,
                None => continue,
            };

            let world_matrices = runtime.world_matrices();

            // Pass 1: compute frame max abs error for domination check
            let mut frame_max_error: f32 = 0.0;

            for oracle_bone in model0.focused_ik_bones(DEFAULT_FOCUSED_IK_BONE_NAMES) {
                if oracle_bone.index < 0 {
                    continue;
                }
                let index = oracle_bone.index as usize;
                if index >= world_matrices.len() {
                    continue;
                }

                let runtime_matrix = world_matrices[index].to_cols_array();
                let oracle_matrix = oracle_bone.world_matrix;

                for i in 0..16 {
                    let abs_error = (runtime_matrix[i] - oracle_matrix[i]).abs();
                    if abs_error > frame_max_error {
                        frame_max_error = abs_error;
                    }
                }
            }

            let frame_diagnostics =
                compute_root_motion_diagnostics(model0, world_matrices, oracle_frame.frame);

            let is_dominated = is_frame_root_control_dominated(frame_max_error, &frame_diagnostics);

            if is_dominated {
                solver_skipped_frames += 1;
            }

            // Pass 2: full comparison with existing + solver-focused tracking
            for oracle_bone in model0.focused_ik_bones(DEFAULT_FOCUSED_IK_BONE_NAMES) {
                if oracle_bone.index < 0 {
                    continue;
                }
                let index = oracle_bone.index as usize;
                if index >= world_matrices.len() {
                    continue;
                }

                let runtime_matrix = world_matrices[index].to_cols_array();
                let oracle_matrix = oracle_bone.world_matrix;

                for i in 0..16 {
                    let abs_error = (runtime_matrix[i] - oracle_matrix[i]).abs();
                    if abs_error > case_max_error {
                        case_max_error = abs_error;
                        case_worst =
                            format!("{}:{}:{}", case.name, oracle_frame.frame, oracle_bone.name);
                    }
                    if abs_error > max_abs_error {
                        max_abs_error = abs_error;
                        worst =
                            format!("{}:{}:{}", case.name, oracle_frame.frame, oracle_bone.name);
                        worst_case_max_error = abs_error;
                        worst_component = i;
                        worst_case_name = case.name.clone();
                        worst_frame = oracle_frame.frame;
                    }
                    if !is_dominated && abs_error > solver_max_abs_error {
                        solver_max_abs_error = abs_error;
                        solver_worst =
                            format!("{}:{}:{}", case.name, oracle_frame.frame, oracle_bone.name);
                        solver_worst_component = i;
                        solver_worst_case_max_error = abs_error;
                        solver_worst_residuals = compute_ik_solver_residuals(
                            model.ik_solvers(),
                            &model_import.bone_names,
                            runtime.pose().ik_enabled(),
                            world_matrices,
                            model0,
                            Some(index),
                        );
                    }
                }

                compared_bones += 1;
                if is_dominated {
                    solver_skipped_bones += 1;
                } else {
                    solver_compared_bones += 1;
                }
            }

            case_diagnostics.extend(frame_diagnostics);
            compared_frames += 1;
        }

        per_case_errors.push((case.name.clone(), case_max_error, case_worst.clone()));
        per_case_diagnostics.push(case_diagnostics.clone());

        let case_lag = compute_root_motion_oracle_lag(&case.name, &case_diagnostics);
        all_lag_matches.extend(case_lag.matches.iter().cloned());

        per_case_entries.push(GoldenIkCompareCaseEntry::Compared(
            GoldenIkComparePerCaseEntry {
                name: case.name.clone(),
                max_abs_error: case_max_error,
                worst: case_worst,
                status: "compared",
                diagnostics: case_diagnostics,
                import_diagnostics: model_import.diagnostics,
                root_motion_oracle_lag: case_lag,
            },
        ));
        compared_cases += 1;
    }

    // --- Summary classification: worst diagnostic tracking ---
    let diagnostics_total: usize = per_case_diagnostics.iter().map(|d| d.len()).sum();

    // Find the diagnostic entry at the same case+frame as the worst matrix error,
    // picking the one with the largest maxAbsError if multiple match.
    let worst_diagnostic = {
        let mut result: Option<GoldenRootMotionDiagnostic> = None;
        for ((name, _error, _worst_bone), case_diags) in
            per_case_errors.iter().zip(per_case_diagnostics.iter())
        {
            if *name != worst_case_name {
                continue;
            }
            for diag in case_diags {
                if diag.frame != worst_frame {
                    continue;
                }
                let larger = match &result {
                    None => true,
                    Some(best) => diag.max_abs_error > best.max_abs_error,
                };
                if larger {
                    result = Some(diag.clone());
                }
            }
            break;
        }
        result
    };

    let worst_likely_root_control_dominated = worst_diagnostic
        .as_ref()
        .map(|d| f64::from(d.max_abs_error))
        .map(|err| err >= max_abs_error as f64 * 0.5)
        .unwrap_or(false);
    let summary_lag_total = all_lag_matches.len();
    let summary_lag_worst = all_lag_matches
        .iter()
        .max_by(|a, b| {
            a.max_abs_error
                .partial_cmp(&b.max_abs_error)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();

    if use_json {
        let report = GoldenIkCompareJsonReport {
            command: "golden-ik-compare",
            root: root.to_string_lossy().into_owned(),
            sample_frame_offset,
            summary: GoldenIkCompareJsonSummary {
                cases,
                compared_cases,
                skipped_unsupported,
                skipped_unsupported_cases,
                missing,
                import_errors,
                compared_frames,
                compared_bones,
                max_abs_error,
                worst,
                worst_component,
                worst_component_type: golden_component_type(worst_component),
                worst_case_max_error,
                diagnostics_total,
                worst_diagnostic,
                worst_likely_root_control_dominated,
                solver_focused: GoldenIkCompareSolverFocusedSummary {
                    compared_bones: solver_compared_bones,
                    skipped_bones: solver_skipped_bones,
                    skipped_frames: solver_skipped_frames,
                    max_abs_error: solver_max_abs_error,
                    worst: solver_worst,
                    worst_component: solver_worst_component,
                    worst_component_type: golden_component_type(solver_worst_component),
                    worst_case_max_error: solver_worst_case_max_error,
                    worst_frame_solver_residuals: solver_worst_residuals,
                    root_motion_dominated_abs_threshold: ROOT_MOTION_DOMINATED_ABS_THRESHOLD,
                },
                root_motion_oracle_lag: GoldenIkCompareRootMotionOracleLagSummary {
                    total_match_count: summary_lag_total,
                    worst_match: summary_lag_worst,
                },
            },
            per_case: per_case_entries,
        };

        println!("{}", serde_json::to_string(&report)?);
    } else {
        println!(
            "Golden IK compare: cases={} comparedCases={} skippedUnsupported={} missing={} importErrors={} comparedFrames={} comparedBones={} maxAbsError={:.6} worst={} sampleFrameOffset={}",
            cases,
            compared_cases,
            skipped_unsupported,
            missing,
            import_errors,
            compared_frames,
            compared_bones,
            max_abs_error,
            worst,
            sample_frame_offset
        );

        if skipped_unsupported > 0 {
            println!("Skipped unsupported cases:");
            for case in &skipped_unsupported_cases {
                let name = &case.name;
                let reason = &case.reason;
                println!("  {name}: {reason}");
            }
        }

        let translation_error =
            if worst_component == 12 || worst_component == 13 || worst_component == 14 {
                "translation"
            } else if worst_component == 15 {
                "homogeneous"
            } else {
                "rotation"
            };
        println!(
            "  worst detail: component[{}]={} matrixElement={:.6}",
            worst_component, translation_error, worst_case_max_error
        );

        for (case_name, error, worst_bone) in &per_case_errors {
            println!(
                "  case {}: maxAbsError={:.6} worst={}",
                case_name, error, worst_bone
            );
        }
    }

    Ok(ExitCode::SUCCESS)
}

pub(crate) fn golden_ik_diagnose(
    root: &Path,
    case_name: &str,
    frame: i32,
    bone_name: &str,
    sample_frame_offset: f32,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&fs::read_to_string(&manifest_path)?)?;

    let case = manifest
        .cases
        .iter()
        .find(|c| c.name == case_name)
        .ok_or_else(|| format!("case not found: {case_name}"))?;

    let case_root = root.join(&case.name);
    let pmx_path = case_root.join(&case.pmx);
    let vmd_path = case_root.join(&case.vmd);
    let fixture_path = case_root.join("fixture.json");

    if !is_supported_golden_model(&pmx_path) {
        return Err("model is not a PMX/PMD file".into());
    }

    if !pmx_path.exists() || !vmd_path.exists() || !fixture_path.exists() {
        return Err("one or more required files are missing".into());
    }

    let fixture = GoldenIkFixture::from_json_str(&fs::read_to_string(&fixture_path)?)?;
    let oracle_path = crate::resolve_maybe_absolute(&case_root, &fixture.output);
    if !oracle_path.exists() {
        return Err("oracle file not found".into());
    }

    let dump = MmdDumperOracleDump::from_jsonl_str(&fs::read_to_string(&oracle_path)?, None)?;
    let oracle_frame = dump
        .find_frame(frame)
        .ok_or_else(|| format!("frame {frame} not found in oracle"))?;

    let model0 = oracle_frame
        .models
        .first()
        .ok_or("no models in oracle frame")?;

    let oracle_bone = model0
        .find_bone(bone_name)
        .ok_or_else(|| format!("bone '{bone_name}' not found in oracle"))?;

    let oracle_index = oracle_bone.index as usize;
    let model_bytes = fs::read(&pmx_path)?;
    let model_import = import_golden_runtime_model(&pmx_path, &model_bytes)
        .map_err(|e| format!("import error: {e}"))?;

    if oracle_index >= model_import.model.bone_count() {
        return Err(format!(
            "bone index {oracle_index} out of range (bone count: {})",
            model_import.model.bone_count()
        )
        .into());
    }

    let vmd_bytes = fs::read(&vmd_path)?;
    let vmd =
        mmd_anim_format::import_vmd_motion(&vmd_bytes).map_err(|e| format!("import error: {e}"))?;

    let solver_count = model_import.model.ik_count();
    let clip = mmd_anim_format::build_pair_clip_with_options(
        &vmd,
        &model_import.bone_name_to_index,
        &model_import.morph_name_to_index,
        &model_import.ik_solver_bone_name_to_index,
        solver_count,
        VmdClipBuildOptions {
            honor_property_ik: false,
        },
    );

    let morph_count = model_import
        .morph_name_to_index
        .values()
        .map(|index| index.as_usize() + 1)
        .max()
        .unwrap_or(0);
    let model = Arc::new(model_import.model);
    let mut pre_ik_runtime =
        RuntimeInstance::new_with_counts(Arc::clone(&model), morph_count, solver_count);
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);

    let sample_frame = frame as f32 + sample_frame_offset;
    pre_ik_runtime.evaluate_clip_frame_without_ik(&clip, sample_frame);
    runtime.evaluate_clip_frame(&clip, sample_frame);

    let world_matrices = runtime.world_matrices();
    let runtime_matrix = world_matrices[oracle_index];

    let rt_t = Vec3A::new(
        runtime_matrix.w_axis.x,
        runtime_matrix.w_axis.y,
        runtime_matrix.w_axis.z,
    );
    let or_t = Vec3A::new(
        oracle_bone.world_matrix[12],
        oracle_bone.world_matrix[13],
        oracle_bone.world_matrix[14],
    );
    let delta_t = rt_t - or_t;
    let abs_delta = delta_t.abs();

    let parent_info = runtime.model().parent_index(BoneIndex(oracle_index as u32));
    let parent_name = parent_info.and_then(|p| {
        let p_idx = p.as_usize();
        if p_idx < model_import.bone_names.len() {
            Some(model_import.bone_names[p_idx].as_str())
        } else {
            None
        }
    });

    let ik_solvers = runtime.model().ik_solvers();
    let mut ik_roles: Vec<String> = Vec::new();
    let bone_idx_u32 = oracle_index as u32;
    let ik_enabled = runtime.pose().ik_enabled();

    for (solver_idx, solver) in ik_solvers.iter().enumerate() {
        let sb_name = if solver.ik_bone.as_usize() < model_import.bone_names.len() {
            &model_import.bone_names[solver.ik_bone.as_usize()]
        } else {
            "?"
        };
        let tb_name = if solver.target_bone.as_usize() < model_import.bone_names.len() {
            &model_import.bone_names[solver.target_bone.as_usize()]
        } else {
            "?"
        };
        let enabled_ch = if solver_idx < ik_enabled.len() {
            if ik_enabled[solver_idx] != 0 {
                '1'
            } else {
                '0'
            }
        } else {
            '?'
        };

        if solver.ik_bone.0 == bone_idx_u32 {
            ik_roles.push(format!(
                "  solver[{}]: role=ikBone ikBone={}({}) targetBone={}({}) iterationCount={} limitAngle={:.8} enabled={}",
                solver_idx,
                sb_name,
                solver.ik_bone.0,
                tb_name,
                solver.target_bone.0,
                solver.iteration_count,
                solver.limit_angle,
                enabled_ch
            ));
        }

        if solver.target_bone.0 == bone_idx_u32 {
            ik_roles.push(format!(
                "  solver[{}]: role=targetBone ikBone={}({}) targetBone={}({}) iterationCount={} limitAngle={:.8} enabled={}",
                solver_idx,
                sb_name,
                solver.ik_bone.0,
                tb_name,
                solver.target_bone.0,
                solver.iteration_count,
                solver.limit_angle,
                enabled_ch
            ));
        }

        for (link_order, link) in solver.links.iter().enumerate() {
            if link.bone.0 == bone_idx_u32 {
                let link_name = if link.bone.as_usize() < model_import.bone_names.len() {
                    &model_import.bone_names[link.bone.as_usize()]
                } else {
                    "?"
                };
                let angle_limit_str = match &link.angle_limit {
                    Some(lim) => format!(
                        "min=({:.6},{:.6},{:.6}) max=({:.6},{:.6},{:.6})",
                        lim.min.x, lim.min.y, lim.min.z, lim.max.x, lim.max.y, lim.max.z
                    ),
                    None => "None".to_string(),
                };
                ik_roles.push(format!(
                    "  solver[{}]: role=link linkOrder={} linkBone={}({}) ikBone={}({}) targetBone={}({}) iterationCount={} limitAngle={:.8} angleLimit={} enabled={}",
                    solver_idx,
                    link_order,
                    link_name,
                    link.bone.0,
                    sb_name,
                    solver.ik_bone.0,
                    tb_name,
                    solver.target_bone.0,
                    solver.iteration_count,
                    solver.limit_angle,
                    angle_limit_str,
                    enabled_ch
                ));
            }
        }
    }

    println!(
        "IK Diagnostic: {case_name} frame={frame} bone=\"{bone_name}\" index={oracle_index} sampleFrameOffset={sample_frame_offset} sampleFrame={sample_frame:.3}"
    );
    println!(
        "  Post-IK runtime translation: ({:.6}, {:.6}, {:.6})",
        rt_t.x, rt_t.y, rt_t.z
    );
    let pre_ik_world = pre_ik_runtime.world_matrices()[oracle_index];
    let pre_ik_t = Vec3A::new(
        pre_ik_world.w_axis.x,
        pre_ik_world.w_axis.y,
        pre_ik_world.w_axis.z,
    );
    println!(
        "  Pre-IK runtime translation: ({:.6}, {:.6}, {:.6})",
        pre_ik_t.x, pre_ik_t.y, pre_ik_t.z
    );
    println!(
        "  Oracle translation: ({:.6}, {:.6}, {:.6})",
        or_t.x, or_t.y, or_t.z
    );
    println!(
        "  Translation delta: ({:.6}, {:.6}, {:.6})",
        delta_t.x, delta_t.y, delta_t.z
    );
    println!(
        "  Absolute delta (max component): {:.6}",
        abs_delta.x.max(abs_delta.y).max(abs_delta.z)
    );
    match parent_info {
        Some(p) => println!(
            "  Parent: index={} name=\"{}\"",
            p.as_usize(),
            parent_name.unwrap_or("?")
        ),
        None => println!("  Parent: None (root bone)"),
    }
    if ik_roles.is_empty() {
        println!("  IK involvement: None");
    } else {
        println!("  IK involvement:");
        for role in &ik_roles {
            println!("{role}");
        }
    }

    // IK residuals: for each solver involving this bone, compute
    // distance between ikBone and targetBone world translations.
    if !ik_roles.is_empty() {
        println!("  IK residuals:");
        for (solver_idx, solver) in ik_solvers.iter().enumerate() {
            let ik_idx = solver.ik_bone.0 as usize;
            let tb_idx = solver.target_bone.0 as usize;

            let is_involved = ik_idx == oracle_index
                || tb_idx == oracle_index
                || solver
                    .links
                    .iter()
                    .any(|link| link.bone.0 as usize == oracle_index);

            if !is_involved {
                continue;
            }

            if ik_idx >= world_matrices.len() || tb_idx >= world_matrices.len() {
                continue;
            }

            // Runtime world translations
            let rt_ik = Vec3A::new(
                world_matrices[ik_idx].w_axis.x,
                world_matrices[ik_idx].w_axis.y,
                world_matrices[ik_idx].w_axis.z,
            );
            let rt_tb = Vec3A::new(
                world_matrices[tb_idx].w_axis.x,
                world_matrices[tb_idx].w_axis.y,
                world_matrices[tb_idx].w_axis.z,
            );
            let runtime_distance = (rt_ik - rt_tb).length();

            // Oracle world translations (if both bones exist in the oracle model)
            let oracle_distance = {
                let or_ik = model0
                    .bones
                    .iter()
                    .find(|b| b.index == solver.ik_bone.0 as i32);
                let or_tb = model0
                    .bones
                    .iter()
                    .find(|b| b.index == solver.target_bone.0 as i32);
                match (or_ik, or_tb) {
                    (Some(ik), Some(tb)) => {
                        let oi = Vec3A::new(
                            ik.world_matrix[12],
                            ik.world_matrix[13],
                            ik.world_matrix[14],
                        );
                        let ot = Vec3A::new(
                            tb.world_matrix[12],
                            tb.world_matrix[13],
                            tb.world_matrix[14],
                        );
                        Some((oi - ot).length())
                    }
                    _ => None,
                }
            };

            let (oracle_display, delta_display) = match oracle_distance {
                Some(od) => (format!("{od:.6}"), format!("{:.6}", runtime_distance - od)),
                None => ("N/A".to_string(), "N/A".to_string()),
            };

            let ik_name = if solver.ik_bone.as_usize() < model_import.bone_names.len() {
                &model_import.bone_names[solver.ik_bone.as_usize()]
            } else {
                "?"
            };
            let tb_name = if solver.target_bone.as_usize() < model_import.bone_names.len() {
                &model_import.bone_names[solver.target_bone.as_usize()]
            } else {
                "?"
            };

            println!(
                "    solver[{}]: ikBone={}({}) targetBone={}({}) runtimeDistance={:.6} oracleDistance={} delta={}",
                solver_idx,
                ik_name,
                solver.ik_bone.0,
                tb_name,
                solver.target_bone.0,
                runtime_distance,
                oracle_display,
                delta_display,
            );
        }
    }

    let rest_pos = runtime
        .model()
        .rest_position(BoneIndex(oracle_index as u32));
    println!(
        "  Rest position: ({:.6}, {:.6}, {:.6})",
        rest_pos.x, rest_pos.y, rest_pos.z
    );

    // Pre-IK local state (before IK solver runs)
    let pre_ik_local_pos = pre_ik_runtime
        .pose()
        .local_position_offset(BoneIndex(oracle_index as u32));
    println!(
        "  Pre-IK local position offset: ({:.6}, {:.6}, {:.6})",
        pre_ik_local_pos.x, pre_ik_local_pos.y, pre_ik_local_pos.z
    );
    let pre_ik_local_rot = pre_ik_runtime
        .pose()
        .local_rotation(BoneIndex(oracle_index as u32));
    let pre_ik_axis_angle = pre_ik_local_rot.to_axis_angle();
    println!(
        "  Pre-IK local rotation: axis=({:.6}, {:.6}, {:.6}) angle={:.6}",
        pre_ik_axis_angle.0.x, pre_ik_axis_angle.0.y, pre_ik_axis_angle.0.z, pre_ik_axis_angle.1
    );

    // Post-IK local state (after IK solver modifies local rotations)
    let local_pos = runtime
        .pose()
        .local_position_offset(BoneIndex(oracle_index as u32));
    println!(
        "  Post-IK local position offset: ({:.6}, {:.6}, {:.6})",
        local_pos.x, local_pos.y, local_pos.z
    );
    let local_rot = runtime
        .pose()
        .local_rotation(BoneIndex(oracle_index as u32));
    let local_axis_angle = local_rot.to_axis_angle();
    println!(
        "  Post-IK local rotation: axis=({:.6}, {:.6}, {:.6}) angle={:.6}",
        local_axis_angle.0.x, local_axis_angle.0.y, local_axis_angle.0.z, local_axis_angle.1
    );

    // Oracle local transform (computed from world matrices)
    let oracle_bone_mat = glam::Mat4::from_cols_array(&oracle_bone.world_matrix);
    let oracle_local_mat = match parent_info {
        Some(parent) => model0
            .bones
            .iter()
            .find(|bone| bone.index == parent.as_usize() as i32)
            .map(|parent_bone| {
                let parent_mat = glam::Mat4::from_cols_array(&parent_bone.world_matrix);
                parent_mat.inverse() * oracle_bone_mat
            })
            .unwrap_or(oracle_bone_mat),
        None => oracle_bone_mat,
    };
    let (_, oracle_local_r, oracle_local_t) = oracle_local_mat.to_scale_rotation_translation();
    let oracle_axis_angle = oracle_local_r.to_axis_angle();
    println!(
        "  Oracle local translation: ({:.6}, {:.6}, {:.6})",
        oracle_local_t.x, oracle_local_t.y, oracle_local_t.z
    );
    println!(
        "  Oracle local rotation: axis=({:.6}, {:.6}, {:.6}) angle={:.6}",
        oracle_axis_angle.0.x, oracle_axis_angle.0.y, oracle_axis_angle.0.z, oracle_axis_angle.1
    );

    if oracle_index < model_import.bone_names.len() {
        let bone_bytes = model_import.bone_names[oracle_index].as_bytes();
        let vmd_kfs: Vec<_> = vmd
            .bone_keyframes
            .iter()
            .filter(|kf| kf.bone_name_normalized == *bone_bytes)
            .collect();
        if vmd_kfs.is_empty() {
            println!("  VMD bone keyframes: none");
        } else {
            println!("  VMD bone keyframes: {} frame(s)", vmd_kfs.len());
            let min_frame = vmd_kfs.iter().map(|kf| kf.frame as i32).min().unwrap_or(0);
            let max_frame = vmd_kfs.iter().map(|kf| kf.frame as i32).max().unwrap_or(0);
            println!("    VMD keyframe range: [{} .. {}]", min_frame, max_frame);

            let sample_frame_i32 = sample_frame.round() as i32;
            let exact_kfs: Vec<_> = vmd_kfs
                .iter()
                .filter(|kf| kf.frame as i32 == sample_frame_i32)
                .collect();
            println!(
                "    Exact-sample-frame raw VMD keyframes (frame={}): {}",
                sample_frame_i32,
                exact_kfs.len()
            );
            for (i, kf) in exact_kfs.iter().take(5).enumerate() {
                let axis_angle = kf.rotation.to_axis_angle();
                println!(
                    "      [#{}] frame={} translation=({:.6}, {:.6}, {:.6}) axis=({:.6}, {:.6}, {:.6}) angle={:.6}",
                    i,
                    kf.frame,
                    kf.position.x,
                    kf.position.y,
                    kf.position.z,
                    axis_angle.0.x,
                    axis_angle.0.y,
                    axis_angle.0.z,
                    axis_angle.1
                );
            }

            if let Some(prev_kf) = vmd_kfs
                .iter()
                .filter(|kf| (kf.frame as i32) < sample_frame_i32)
                .max_by_key(|kf| kf.frame)
            {
                println!(
                    "    Nearest prev keyframe: frame={} translation=({:.6}, {:.6}, {:.6})",
                    prev_kf.frame, prev_kf.position.x, prev_kf.position.y, prev_kf.position.z
                );
            } else {
                println!("    Nearest prev keyframe: none (before range)");
            }

            if let Some(next_kf) = vmd_kfs
                .iter()
                .filter(|kf| (kf.frame as i32) > sample_frame_i32)
                .min_by_key(|kf| kf.frame)
            {
                println!(
                    "    Nearest next keyframe: frame={} translation=({:.6}, {:.6}, {:.6})",
                    next_kf.frame, next_kf.position.x, next_kf.position.y, next_kf.position.z
                );
            } else {
                println!("    Nearest next keyframe: none (beyond range)");
            }
        }
    } else {
        println!("  VMD bone keyframes: N/A (bone name bytes out of range)");
    }

    if let Some(track) = clip.find_bone_track(BoneIndex(oracle_index as u32)) {
        if let Some((clip_pos, clip_rot)) = track.sample(sample_frame) {
            println!(
                "  Clip sample at sampleFrame {:.3} (before IK): translation=({:.6}, {:.6}, {:.6})",
                sample_frame, clip_pos.x, clip_pos.y, clip_pos.z
            );
            let clip_axis_angle = clip_rot.to_axis_angle();
            println!(
                "    rotation: axis=({:.6}, {:.6}, {:.6}) angle={:.6}",
                clip_axis_angle.0.x, clip_axis_angle.0.y, clip_axis_angle.0.z, clip_axis_angle.1
            );
        } else {
            println!("  Clip sample (before IK): no sample at sampleFrame {sample_frame:.3}");
        }
    } else {
        println!("  Clip sample (before IK): no bone track found");
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests;
