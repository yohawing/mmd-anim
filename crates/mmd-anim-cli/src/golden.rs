use std::{collections::HashMap, fs, path::Path, process::ExitCode, sync::Arc};

use glam::Vec3A;
use mmd_anim_format::VmdClipBuildOptions;
use mmd_anim_runtime::{BoneIndex, IkSolver, ModelArena, MorphIndex, RuntimeInstance};
use mmd_anim_schema::{
    DEFAULT_FOCUSED_IK_BONE_NAMES, GoldenIkBatchManifest, GoldenIkFixture, MmdDumperOracleDump,
    MmdDumperOracleModel,
};
use serde_json::json;

pub(crate) const GOLDEN_IK_COMPARE_USAGE: &str =
    "usage: mmd-anim golden-ik-compare <golden-ik-oracle-root> [sample-frame-offset]";

pub(crate) struct RuntimeModelImport {
    pub(crate) model: ModelArena,
    pub(crate) bone_names: Vec<String>,
    pub(crate) bone_name_to_index: HashMap<Vec<u8>, BoneIndex>,
    pub(crate) morph_name_to_index: HashMap<Vec<u8>, MorphIndex>,
    pub(crate) ik_solver_bone_name_to_index: HashMap<Vec<u8>, usize>,
    pub(crate) diagnostics: Vec<serde_json::Value>,
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
pub(crate) fn compute_root_motion_diagnostics(
    oracle_model: &MmdDumperOracleModel,
    world_matrices: &[glam::Mat4],
    frame: i32,
) -> Vec<serde_json::Value> {
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

            diagnostics.push(json!({
                "bone": bone_name,
                "frame": frame,
                "runtimeTranslation": [rt_t.x, rt_t.y, rt_t.z],
                "oracleTranslation": [or_t.x, or_t.y, or_t.z],
                "delta": [delta.x, delta.y, delta.z],
                "maxAbsError": max_abs,
                "classification": classification,
            }));
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
    diagnostics: &[serde_json::Value],
) -> serde_json::Value {
    use std::collections::BTreeMap;

    // Filter to root_motion_mismatch only
    let root_motion: Vec<&serde_json::Value> = diagnostics
        .iter()
        .filter(|d| d["classification"].as_str() == Some("root_motion_mismatch"))
        .collect();

    // Group by bone name
    let mut by_bone: BTreeMap<&str, Vec<&serde_json::Value>> = BTreeMap::new();
    for d in &root_motion {
        if let Some(name) = d["bone"].as_str() {
            by_bone.entry(name).or_default().push(d);
        }
    }

    let mut matches: Vec<serde_json::Value> = Vec::new();

    for (_bone, entries) in by_bone.iter_mut() {
        // Sort by frame ascending
        entries.sort_by_key(|d| d["frame"].as_i64().unwrap_or(0));

        for window in entries.windows(2) {
            let prev = window[0];
            let curr = window[1];

            let curr_oracle = curr["oracleTranslation"].as_array();
            let prev_runtime = prev["runtimeTranslation"].as_array();

            let (co, pr) = match (curr_oracle, prev_runtime) {
                (Some(co), Some(pr)) => (co, pr),
                _ => continue,
            };

            if co.len() < 3 || pr.len() < 3 {
                continue;
            }

            let dx = (co[0].as_f64().unwrap_or(0.0) - pr[0].as_f64().unwrap_or(0.0)).abs();
            let dy = (co[1].as_f64().unwrap_or(0.0) - pr[1].as_f64().unwrap_or(0.0)).abs();
            let dz = (co[2].as_f64().unwrap_or(0.0) - pr[2].as_f64().unwrap_or(0.0)).abs();
            let max_delta = dx.max(dy).max(dz);

            if max_delta <= ORACLE_LAG_DELTA_THRESHOLD {
                matches.push(json!({
                    "case": case_name,
                    "bone": curr["bone"],
                    "frame": curr["frame"],
                    "previousFrame": prev["frame"],
                    "maxAbsError": curr["maxAbsError"],
                    "matchDelta": max_delta,
                }));
            }
        }
    }

    json!({
        "matchCount": matches.len(),
        "matches": matches,
    })
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
    frame_diagnostics: &[serde_json::Value],
) -> bool {
    if frame_max_error <= 0.0 {
        return false;
    }
    frame_diagnostics.iter().any(|d| {
        let abs_err = d["maxAbsError"].as_f64().unwrap_or(0.0);
        // Ratio rule: a diagnostic error >= 50% of frame max error
        // dominates regardless of classification.
        abs_err >= ROOT_CONTROL_DOMINATED_RATIO * frame_max_error as f64
            // Absolute threshold: a root_motion_mismatch error at or
            // above ROOT_MOTION_DOMINATED_ABS_THRESHOLD dominates
            // even when the ratio check fails (capture mismatch
            // propagated through hierarchy).
            || (d["classification"].as_str() == Some("root_motion_mismatch")
                && abs_err >= ROOT_MOTION_DOMINATED_ABS_THRESHOLD)
    })
}

/// Compute per-solver IK residual (distance between ikBone and targetBone
/// world-matrix translations) for both the runtime solver result and the
/// oracle (MMD) reference.  This separates end-effector convergence quality
/// from per-bone world-matrix error: a solver can converge (small residual)
/// while individual link poses still differ from the oracle.
pub(crate) fn compute_ik_solver_residuals(
    ik_solvers: &[IkSolver],
    bone_names: &[String],
    ik_enabled: &[u8],
    world_matrices: &[glam::Mat4],
    oracle_model: &MmdDumperOracleModel,
    focus_bone_index: Option<usize>,
) -> Vec<serde_json::Value> {
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

        let mut entry = json!({
            "solverIndex": solver_idx,
            "ikBone": ik_name,
            "ikBoneIndex": solver.ik_bone.0,
            "targetBone": tb_name,
            "targetBoneIndex": solver.target_bone.0,
            "enabled": ik_enabled.get(solver_idx).copied().unwrap_or(1) != 0,
            "runtimeResidual": runtime_residual,
        });
        if let Some(or) = oracle_residual {
            entry["oracleResidual"] = json!(or);
            entry["residualDelta"] = json!(runtime_residual - or);
        }
        residuals.push(entry);
    }

    residuals
}

/// Build a JSON pair for an unsupported (non-.pmx) case.
///
/// Returns `(summary_entry, per_case_entry)` so callers can push them into
/// the summary `skippedUnsupportedCases` list and `perCase` list respectively.
fn make_unsupported_case_entry(
    pmx_path: &Path,
    case_name: &str,
) -> (serde_json::Value, serde_json::Value) {
    let ext = pmx_path.extension().and_then(|e| e.to_str()).unwrap_or("?");
    let model_name = pmx_path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let reason = format!("unsupported model format: only .pmx and .pmd are supported (got .{ext})");

    let summary = json!({
        "name": case_name,
        "model": model_name,
        "extension": ext,
        "reason": reason,
    });
    let per_case = json!({
        "name": case_name,
        "status": "skipped",
        "model": model_name,
        "reason": reason,
        "maxAbsError": 0.0,
        "worst": "",
        "rootMotionOracleLag": {
            "matchCount": 0,
            "matches": [],
        },
    });

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
                    .map(|diagnostic| {
                        json!({
                            "level": diagnostic.level,
                            "code": diagnostic.code,
                            "message": diagnostic.message,
                        })
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
    let mut skipped_unsupported_cases: Vec<serde_json::Value> = Vec::new();
    let mut per_case_entries: Vec<serde_json::Value> = Vec::new();
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
    let mut per_case_diagnostics: Vec<Vec<serde_json::Value>> = Vec::new();
    let mut all_lag_matches: Vec<serde_json::Value> = Vec::new();

    // Solver-focused tracking (excludes root/control-dominated frames)
    let mut solver_compared_bones: usize = 0;
    let mut solver_skipped_bones: usize = 0;
    let mut solver_skipped_frames: usize = 0;
    let mut solver_max_abs_error: f32 = 0.0;
    let mut solver_worst = String::from("none");
    let mut solver_worst_component: usize = 0;
    let mut solver_worst_case_max_error: f32 = 0.0;
    let mut solver_worst_residuals: Vec<serde_json::Value> = Vec::new();

    for case in &manifest.cases {
        cases += 1;

        let case_root = root.join(&case.name);
        let pmx_path = case_root.join(&case.pmx);

        if !is_supported_golden_model(&pmx_path) {
            skipped_unsupported += 1;
            let (summary, per_case) = make_unsupported_case_entry(&pmx_path, &case.name);
            skipped_unsupported_cases.push(summary);
            per_case_entries.push(per_case);
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
        let oracle_path = super::resolve_maybe_absolute(&case_root, &fixture.output);
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
        let mut case_diagnostics: Vec<serde_json::Value> = Vec::new();

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
        if let Some(matches) = case_lag.get("matches").and_then(|m| m.as_array()) {
            for m in matches {
                all_lag_matches.push(m.clone());
            }
        }

        {
            let mut entry = json!({
                "name": case.name,
                "maxAbsError": case_max_error,
                "worst": case_worst,
                "status": "compared",
            });
            if !case_diagnostics.is_empty() {
                entry["diagnostics"] = json!(case_diagnostics);
            }
            if !model_import.diagnostics.is_empty() {
                entry["importDiagnostics"] = json!(model_import.diagnostics);
            }
            entry["rootMotionOracleLag"] = case_lag;
            per_case_entries.push(entry);
        }
        compared_cases += 1;
    }

    // --- Summary classification: worst diagnostic tracking ---
    let diagnostics_total: usize = per_case_diagnostics.iter().map(|d| d.len()).sum();

    // Find the diagnostic entry at the same case+frame as the worst matrix error,
    // picking the one with the largest maxAbsError if multiple match.
    let worst_diagnostic = {
        let mut result: Option<serde_json::Value> = None;
        for ((name, _error, _worst_bone), case_diags) in
            per_case_errors.iter().zip(per_case_diagnostics.iter())
        {
            if *name != worst_case_name {
                continue;
            }
            for diag in case_diags {
                if diag["frame"].as_i64() != Some(worst_frame as i64) {
                    continue;
                }
                let larger = match &result {
                    None => true,
                    Some(best) => {
                        let cur = diag["maxAbsError"].as_f64().unwrap_or(0.0);
                        let best_val = best["maxAbsError"].as_f64().unwrap_or(0.0);
                        cur > best_val
                    }
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
        .and_then(|d| d["maxAbsError"].as_f64())
        .map(|err| err >= max_abs_error as f64 * 0.5)
        .unwrap_or(false);

    let summary_lag_total = all_lag_matches.len();
    let summary_lag_worst = all_lag_matches
        .iter()
        .max_by(|a, b| {
            let a_err = a["maxAbsError"].as_f64().unwrap_or(0.0);
            let b_err = b["maxAbsError"].as_f64().unwrap_or(0.0);
            a_err
                .partial_cmp(&b_err)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned();

    if use_json {
        let worst_type = if worst_component == 12 || worst_component == 13 || worst_component == 14
        {
            "translation"
        } else if worst_component == 15 {
            "homogeneous"
        } else {
            "rotation"
        };

        let per_case: Vec<serde_json::Value> = per_case_entries;

        let report = json!({
            "command": "golden-ik-compare",
            "root": root.to_string_lossy(),
            "sampleFrameOffset": sample_frame_offset,
            "summary": {
                "cases": cases,
                "comparedCases": compared_cases,
                "skippedUnsupported": skipped_unsupported,
                "skippedUnsupportedCases": skipped_unsupported_cases,
                "missing": missing,
                "importErrors": import_errors,
                "comparedFrames": compared_frames,
                "comparedBones": compared_bones,
                "maxAbsError": max_abs_error,
                "worst": worst,
                "worstComponent": worst_component,
                "worstComponentType": worst_type,
                "worstCaseMaxError": worst_case_max_error,
                "diagnosticsTotal": diagnostics_total,
                "worstDiagnostic": worst_diagnostic,
                "worstLikelyRootControlDominated": worst_likely_root_control_dominated,
                "solverFocused": {
                    "comparedBones": solver_compared_bones,
                    "skippedBones": solver_skipped_bones,
                    "skippedFrames": solver_skipped_frames,
                    "maxAbsError": solver_max_abs_error,
                    "worst": solver_worst,
                    "worstComponent": solver_worst_component,
                    "worstComponentType": if solver_worst_component == 12
                        || solver_worst_component == 13
                        || solver_worst_component == 14
                    {
                        "translation"
                    } else if solver_worst_component == 15 {
                        "homogeneous"
                    } else {
                        "rotation"
                    },
                    "worstCaseMaxError": solver_worst_case_max_error,
                    "worstFrameSolverResiduals": solver_worst_residuals,
                    "rootMotionDominatedAbsThreshold": ROOT_MOTION_DOMINATED_ABS_THRESHOLD,
                },
                "rootMotionOracleLag": {
                    "totalMatchCount": summary_lag_total,
                    "worstMatch": summary_lag_worst,
                },
            },
            "perCase": per_case,
        });

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
                let name = case["name"].as_str().unwrap_or("?");
                let reason = case["reason"].as_str().unwrap_or("?");
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
    let oracle_path = super::resolve_maybe_absolute(&case_root, &fixture.output);
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
mod tests {
    use super::*;
    use glam::Mat4;
    use mmd_anim_runtime::{IkLink, IkSolver};
    use mmd_anim_schema::{MmdDumperOracleBone, MmdDumperOracleModel};

    fn make_identity_matrix(tx: f32, ty: f32, tz: f32) -> [f32; 16] {
        let mut m = [0f32; 16];
        m[0] = 1.0;
        m[5] = 1.0;
        m[10] = 1.0;
        m[15] = 1.0;
        m[12] = tx;
        m[13] = ty;
        m[14] = tz;
        m
    }

    #[test]
    fn golden_ik_compare_args_parses_root_only() {
        let mut args = vec!["/some/root".to_owned()].into_iter();
        let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
        assert_eq!(root, "/some/root");
        assert_eq!(offset, 0.0);
        assert!(!use_json);
    }

    #[test]
    fn golden_ik_compare_args_parses_root_and_offset() {
        let mut args = vec!["/some/root".to_owned(), "0.5".to_owned()].into_iter();
        let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
        assert_eq!(root, "/some/root");
        assert_eq!(offset, 0.5);
        assert!(!use_json);
    }

    #[test]
    fn golden_ik_compare_args_json_flag_after_root() {
        let mut args = vec!["/some/root".to_owned(), "--json".to_owned()].into_iter();
        let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
        assert_eq!(root, "/some/root");
        assert_eq!(offset, 0.0);
        assert!(use_json);
    }

    #[test]
    fn golden_ik_compare_args_json_flag_before_root() {
        let mut args = vec!["--json".to_owned(), "/some/root".to_owned()].into_iter();
        let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
        assert_eq!(root, "/some/root");
        assert_eq!(offset, 0.0);
        assert!(use_json);
    }

    #[test]
    fn golden_ik_compare_args_json_with_offset() {
        let mut args = vec![
            "/some/root".to_owned(),
            "1.5".to_owned(),
            "--json".to_owned(),
        ]
        .into_iter();
        let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
        assert_eq!(root, "/some/root");
        assert_eq!(offset, 1.5);
        assert!(use_json);
    }

    #[test]
    fn golden_ik_compare_args_all_json_first() {
        let mut args = vec![
            "--json".to_owned(),
            "/other/root".to_owned(),
            "-0.25".to_owned(),
        ]
        .into_iter();
        let (root, offset, use_json) = parse_golden_ik_compare_args(&mut args).unwrap();
        assert_eq!(root, "/other/root");
        assert_eq!(offset, -0.25);
        assert!(use_json);
    }

    #[test]
    fn golden_ik_compare_args_reject_extra_values() {
        let mut args = vec![
            "/some/root".to_owned(),
            "0.5".to_owned(),
            "extra".to_owned(),
        ]
        .into_iter();
        let error = parse_golden_ik_compare_args(&mut args).unwrap_err();
        assert!(error.contains("unexpected extra argument"));
    }

    #[test]
    fn golden_ik_compare_args_reject_unknown_flag() {
        let mut args = vec!["/some/root".to_owned(), "--bad".to_owned()].into_iter();
        let error = parse_golden_ik_compare_args(&mut args).unwrap_err();
        assert!(error.contains("unknown flag"));
    }

    #[test]
    fn golden_ik_compare_args_reject_invalid_offset() {
        let mut args = vec!["/some/root".to_owned(), "nope".to_owned()].into_iter();
        let error = parse_golden_ik_compare_args(&mut args).unwrap_err();
        assert!(error.contains("invalid sample-frame-offset"));
    }

    #[test]
    fn root_motion_diagnostics_center_large_delta() {
        let bone = MmdDumperOracleBone {
            index: 0,
            name: "センター".into(),
            world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
        };
        let model = MmdDumperOracleModel {
            index: 0,
            name: "test".into(),
            filename: "test.pmx".into(),
            visible: true,
            bones: vec![bone],
            morphs: vec![],
        };
        // Runtime translation = (1, 2, 3), a large delta from (0, 0, 0).
        let world = vec![Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 2.0, 3.0, 1.0,
        ])];

        let diags = compute_root_motion_diagnostics(&model, &world, 300);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0]["bone"], "センター");
        assert_eq!(diags[0]["frame"], 300);
        assert_eq!(diags[0]["classification"], "root_motion_mismatch");
        assert!((diags[0]["maxAbsError"].as_f64().unwrap() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn root_motion_diagnostics_below_threshold() {
        let bone = MmdDumperOracleBone {
            index: 0,
            name: "センター".into(),
            world_matrix: make_identity_matrix(0.5, 0.5, 0.5),
        };
        let model = MmdDumperOracleModel {
            index: 0,
            name: "test".into(),
            filename: "test.pmx".into(),
            visible: true,
            bones: vec![bone],
            morphs: vec![],
        };
        // Runtime translation = (0.5005, 0.5, 0.5), below the threshold.
        let world = vec![Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5005, 0.5, 0.5, 1.0,
        ])];

        let diags = compute_root_motion_diagnostics(&model, &world, 1);
        assert!(diags.is_empty());
    }

    #[test]
    fn root_motion_diagnostics_mid_delta_below_new_threshold() {
        let bone = MmdDumperOracleBone {
            index: 0,
            name: "センター".into(),
            world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
        };
        let model = MmdDumperOracleModel {
            index: 0,
            name: "test".into(),
            filename: "test.pmx".into(),
            visible: true,
            bones: vec![bone],
            morphs: vec![],
        };
        // Runtime translation = (0.05, 0, 0). This is below the current
        // reporting threshold and should not produce a diagnostic.
        let world = vec![Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.05, 0.0, 0.0, 1.0,
        ])];
        let diags = compute_root_motion_diagnostics(&model, &world, 1);
        assert!(diags.is_empty());
    }

    #[test]
    fn root_motion_diagnostics_control_bone_classification() {
        let bone = MmdDumperOracleBone {
            index: 0,
            name: "左足ＩＫ".into(),
            world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
        };
        let model = MmdDumperOracleModel {
            index: 0,
            name: "test".into(),
            filename: "test.pmx".into(),
            visible: true,
            bones: vec![bone],
            morphs: vec![],
        };
        let world = vec![Mat4::from_cols_array(&[
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 10.0, 0.0, 1.0,
        ])];

        let diags = compute_root_motion_diagnostics(&model, &world, 42);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0]["bone"], "左足ＩＫ");
        assert_eq!(diags[0]["classification"], "control_bone_mismatch");
    }

    #[test]
    fn root_motion_diagnostics_bone_not_found() {
        let model = MmdDumperOracleModel {
            index: 0,
            name: "empty".into(),
            filename: "empty.pmx".into(),
            visible: true,
            bones: vec![],
            morphs: vec![],
        };
        let diags = compute_root_motion_diagnostics(&model, &[], 0);
        assert!(diags.is_empty());
    }

    #[test]
    fn root_motion_diagnostics_index_out_of_range() {
        let bone = MmdDumperOracleBone {
            index: 5,
            name: "センター".into(),
            world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
        };
        let model = MmdDumperOracleModel {
            index: 0,
            name: "test".into(),
            filename: "test.pmx".into(),
            visible: true,
            bones: vec![bone],
            morphs: vec![],
        };
        let diags = compute_root_motion_diagnostics(&model, &[Mat4::IDENTITY], 0);
        assert!(diags.is_empty());
    }

    #[test]
    fn ik_solver_residuals_reports_enabled_and_delta() {
        let solvers = vec![IkSolver {
            ik_bone: BoneIndex(0),
            target_bone: BoneIndex(1),
            links: vec![IkLink {
                bone: BoneIndex(2),
                angle_limit: None,
            }]
            .into_boxed_slice(),
            iteration_count: 1,
            limit_angle: 0.0,
        }];
        let bone_names = vec!["ik".to_owned(), "target".to_owned(), "link".to_owned()];
        let world = vec![
            Mat4::from_translation(Vec3A::new(0.0, 0.0, 0.0).into()),
            Mat4::from_translation(Vec3A::new(3.0, 0.0, 0.0).into()),
            Mat4::IDENTITY,
        ];
        let oracle = MmdDumperOracleModel {
            index: 0,
            name: "test".into(),
            filename: "test.pmx".into(),
            visible: true,
            bones: vec![
                MmdDumperOracleBone {
                    index: 0,
                    name: "ik".into(),
                    world_matrix: make_identity_matrix(0.0, 0.0, 0.0),
                },
                MmdDumperOracleBone {
                    index: 1,
                    name: "target".into(),
                    world_matrix: make_identity_matrix(1.0, 0.0, 0.0),
                },
            ],
            morphs: vec![],
        };

        let residuals =
            compute_ik_solver_residuals(&solvers, &bone_names, &[0], &world, &oracle, Some(2));

        assert_eq!(residuals.len(), 1);
        assert_eq!(residuals[0]["solverIndex"], 0);
        assert_eq!(residuals[0]["ikBone"], "ik");
        assert_eq!(residuals[0]["targetBone"], "target");
        assert_eq!(residuals[0]["enabled"], false);
        assert!((residuals[0]["runtimeResidual"].as_f64().unwrap() - 3.0).abs() < 1e-6);
        assert!((residuals[0]["oracleResidual"].as_f64().unwrap() - 1.0).abs() < 1e-6);
        assert!((residuals[0]["residualDelta"].as_f64().unwrap() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn ik_solver_residuals_filters_unrelated_focus_bone() {
        let solvers = vec![IkSolver {
            ik_bone: BoneIndex(0),
            target_bone: BoneIndex(1),
            links: vec![IkLink {
                bone: BoneIndex(2),
                angle_limit: None,
            }]
            .into_boxed_slice(),
            iteration_count: 1,
            limit_angle: 0.0,
        }];
        let bone_names = vec!["ik".to_owned(), "target".to_owned(), "link".to_owned()];
        let world = vec![Mat4::IDENTITY, Mat4::IDENTITY, Mat4::IDENTITY];
        let oracle = MmdDumperOracleModel {
            index: 0,
            name: "test".into(),
            filename: "test.pmx".into(),
            visible: true,
            bones: vec![],
            morphs: vec![],
        };

        let residuals =
            compute_ik_solver_residuals(&solvers, &bone_names, &[1], &world, &oracle, Some(99));

        assert!(residuals.is_empty());
    }

    // is_frame_root_control_dominated tests

    #[test]
    fn is_dominated_zero_frame_error_returns_false() {
        let diags = vec![json!({"maxAbsError": 100.0, "classification": "root_motion_mismatch"})];
        assert!(!is_frame_root_control_dominated(0.0, &diags));
    }

    #[test]
    fn is_dominated_negative_frame_error_returns_false() {
        let diags = vec![json!({"maxAbsError": 100.0, "classification": "root_motion_mismatch"})];
        assert!(!is_frame_root_control_dominated(-1.0, &diags));
    }

    #[test]
    fn is_dominated_empty_diagnostics_returns_false() {
        assert!(!is_frame_root_control_dominated(10.0, &[]));
    }

    #[test]
    fn is_dominated_ratio_rule_dominates() {
        // frame_max_error = 2.0, maxAbsError = 1.0 (>= 0.5 * 2.0)
        // Ratio rule fires regardless of classification.
        let diags = vec![json!({"maxAbsError": 1.0, "classification": "control_bone_mismatch"})];
        assert!(is_frame_root_control_dominated(2.0, &diags));
    }

    #[test]
    fn is_dominated_ratio_below_threshold_does_not_dominate() {
        // frame_max_error = 10.0, maxAbsError = 1.0 (< 0.5 * 10.0)
        let diags = vec![json!({"maxAbsError": 1.0, "classification": "control_bone_mismatch"})];
        assert!(!is_frame_root_control_dominated(10.0, &diags));
    }

    #[test]
    fn is_dominated_root_motion_abs_threshold_when_ratio_fails() {
        // frame_max_error = 100.0, maxAbsError = 1.0
        // Ratio check: 1.0 < 0.5 * 100.0 fails.
        // Absolute check: root_motion_mismatch && 1.0 >= 1.0 passes.
        let diags = vec![json!({"maxAbsError": 1.0, "classification": "root_motion_mismatch"})];
        assert!(is_frame_root_control_dominated(100.0, &diags));
    }

    #[test]
    fn is_dominated_control_bone_abs_alone_does_not_dominate() {
        // frame_max_error = 100.0, maxAbsError = 1.0
        // Ratio check: 1.0 < 0.5 * 100.0 fails.
        // Absolute check: control_bone_mismatch is not root_motion_mismatch, so it fails.
        let diags = vec![json!({"maxAbsError": 1.0, "classification": "control_bone_mismatch"})];
        assert!(!is_frame_root_control_dominated(100.0, &diags));
    }

    // make_unsupported_case_entry tests

    #[test]
    fn unsupported_case_entry_x_extension() {
        let pmx_path = Path::new("some/case/accessory.x");
        let (summary, per_case) = make_unsupported_case_entry(pmx_path, "test-case");

        assert_eq!(summary["name"], "test-case");
        assert_eq!(summary["model"], "accessory.x");
        assert_eq!(summary["extension"], "x");
        assert_eq!(
            summary["reason"],
            "unsupported model format: only .pmx and .pmd are supported (got .x)"
        );

        assert_eq!(per_case["name"], "test-case");
        assert_eq!(per_case["status"], "skipped");
        assert_eq!(per_case["model"], "accessory.x");
        assert_eq!(
            per_case["reason"],
            "unsupported model format: only .pmx and .pmd are supported (got .x)"
        );
        assert_eq!(per_case["maxAbsError"], 0.0);
        assert_eq!(per_case["worst"], "");
    }

    #[test]
    fn unsupported_case_entry_no_extension() {
        let pmx_path = Path::new("some/case/model_no_ext");
        let (summary, per_case) = make_unsupported_case_entry(pmx_path, "test-case");

        assert_eq!(summary["name"], "test-case");
        assert_eq!(summary["model"], "model_no_ext");
        assert_eq!(summary["extension"], "?");
        assert_eq!(
            summary["reason"],
            "unsupported model format: only .pmx and .pmd are supported (got .?)"
        );

        assert_eq!(per_case["name"], "test-case");
        assert_eq!(per_case["status"], "skipped");
        assert_eq!(per_case["model"], "model_no_ext");
        assert_eq!(
            per_case["reason"],
            "unsupported model format: only .pmx and .pmd are supported (got .?)"
        );
        assert_eq!(per_case["maxAbsError"], 0.0);
        assert_eq!(per_case["worst"], "");
    }

    #[test]
    fn golden_model_supports_pmx_and_pmd_extensions() {
        assert!(is_supported_golden_model(Path::new("model.pmx")));
        assert!(is_supported_golden_model(Path::new("model.PMD")));
        assert!(!is_supported_golden_model(Path::new("stage.x")));
        assert!(!is_supported_golden_model(Path::new("model")));
    }

    // compute_root_motion_oracle_lag tests

    #[test]
    fn oracle_lag_empty_diagnostics() {
        let result = compute_root_motion_oracle_lag("test", &[]);
        assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
        assert!(result["matches"].as_array().unwrap().is_empty());
    }

    #[test]
    fn oracle_lag_no_root_motion_classification() {
        let diags = vec![json!({
            "bone": "センター",
            "frame": 300,
            "oracleTranslation": [10.0, 0.0, 0.0],
            "runtimeTranslation": [20.0, 0.0, 0.0],
            "maxAbsError": 10.0,
            "classification": "control_bone_mismatch",
        })];
        let result = compute_root_motion_oracle_lag("test", &diags);
        assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
    }

    #[test]
    fn oracle_lag_single_bone_exact_match() {
        // Frame 300: runtime=(1.0,0,0), oracle=(12.0,0,0)
        // Frame 600: runtime=(2.0,0,0), oracle=(1.0,0,0)
        // oracle@600 matches runtime@300 -> lag detected
        let diags = vec![
            json!({
                "bone": "センター",
                "frame": 300,
                "oracleTranslation": [12.0, 0.0, 0.0],
                "runtimeTranslation": [1.0, 0.0, 0.0],
                "maxAbsError": 11.0,
                "classification": "root_motion_mismatch",
            }),
            json!({
                "bone": "センター",
                "frame": 600,
                "oracleTranslation": [1.0, 0.0, 0.0],
                "runtimeTranslation": [2.0, 0.0, 0.0],
                "maxAbsError": 12.0,
                "classification": "root_motion_mismatch",
            }),
        ];
        let result = compute_root_motion_oracle_lag("test-case", &diags);
        assert_eq!(result["matchCount"].as_u64().unwrap(), 1);
        let matches = result["matches"].as_array().unwrap();
        assert_eq!(matches[0]["case"], "test-case");
        assert_eq!(matches[0]["bone"], "センター");
        assert_eq!(matches[0]["frame"], 600);
        assert_eq!(matches[0]["previousFrame"], 300);
        assert_eq!(matches[0]["maxAbsError"], 12.0);
        assert!((matches[0]["matchDelta"].as_f64().unwrap() - 0.0).abs() < 1e-9);
    }

    #[test]
    fn oracle_lag_below_threshold_no_match() {
        // oracle@600 nearly matches runtime@300 but delta > 0.001
        let diags = vec![
            json!({
                "bone": "センター",
                "frame": 300,
                "oracleTranslation": [12.0, 0.0, 0.0],
                "runtimeTranslation": [1.0, 0.0, 0.0],
                "maxAbsError": 11.0,
                "classification": "root_motion_mismatch",
            }),
            json!({
                "bone": "センター",
                "frame": 600,
                "oracleTranslation": [1.002, 0.0, 0.0],
                "runtimeTranslation": [2.0, 0.0, 0.0],
                "maxAbsError": 12.0,
                "classification": "root_motion_mismatch",
            }),
        ];
        let result = compute_root_motion_oracle_lag("test", &diags);
        assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
    }

    #[test]
    fn oracle_lag_exactly_at_threshold() {
        // delta == 0.001 exactly -> counted as match
        let diags = vec![
            json!({
                "bone": "センター",
                "frame": 300,
                "oracleTranslation": [12.0, 0.0, 0.0],
                "runtimeTranslation": [1.0, 0.0, 0.0],
                "maxAbsError": 11.0,
                "classification": "root_motion_mismatch",
            }),
            json!({
                "bone": "センター",
                "frame": 600,
                "oracleTranslation": [1.001, 0.0, 0.0],
                "runtimeTranslation": [2.0, 0.0, 0.0],
                "maxAbsError": 12.0,
                "classification": "root_motion_mismatch",
            }),
        ];
        let result = compute_root_motion_oracle_lag("test", &diags);
        assert_eq!(result["matchCount"].as_u64().unwrap(), 1);
    }

    #[test]
    fn oracle_lag_two_bones_independent() {
        let diags = vec![
            json!({
                "bone": "センター",
                "frame": 300,
                "oracleTranslation": [10.0, 0.0, 0.0],
                "runtimeTranslation": [0.0, 0.0, 0.0],
                "maxAbsError": 10.0,
                "classification": "root_motion_mismatch",
            }),
            json!({
                "bone": "センター",
                "frame": 600,
                "oracleTranslation": [0.0, 0.0, 0.0],
                "runtimeTranslation": [5.0, 0.0, 0.0],
                "maxAbsError": 10.0,
                "classification": "root_motion_mismatch",
            }),
            json!({
                "bone": "グルーブ",
                "frame": 300,
                "oracleTranslation": [20.0, 0.0, 0.0],
                "runtimeTranslation": [10.0, 0.0, 0.0],
                "maxAbsError": 10.0,
                "classification": "root_motion_mismatch",
            }),
            json!({
                "bone": "グルーブ",
                "frame": 600,
                "oracleTranslation": [10.0, 0.0, 0.0],
                "runtimeTranslation": [15.0, 0.0, 0.0],
                "maxAbsError": 10.0,
                "classification": "root_motion_mismatch",
            }),
        ];
        let result = compute_root_motion_oracle_lag("test", &diags);
        // Two bones, one lag match each = 2 total
        assert_eq!(result["matchCount"].as_u64().unwrap(), 2);
    }

    #[test]
    fn oracle_lag_no_lag_when_oracle_differs() {
        // oracle@600 does NOT match runtime@300
        let diags = vec![
            json!({
                "bone": "センター",
                "frame": 300,
                "oracleTranslation": [12.0, 0.0, 0.0],
                "runtimeTranslation": [1.0, 0.0, 0.0],
                "maxAbsError": 11.0,
                "classification": "root_motion_mismatch",
            }),
            json!({
                "bone": "センター",
                "frame": 600,
                "oracleTranslation": [99.0, 0.0, 0.0],
                "runtimeTranslation": [2.0, 0.0, 0.0],
                "maxAbsError": 97.0,
                "classification": "root_motion_mismatch",
            }),
        ];
        let result = compute_root_motion_oracle_lag("test", &diags);
        assert_eq!(result["matchCount"].as_u64().unwrap(), 0);
    }
}
