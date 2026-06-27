use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use crate::schema::{
    DEFAULT_FOCUSED_IK_BONE_NAMES, MmdDumperOracleBone, MmdDumperOracleDump, MmdDumperOracleModel,
};
use mmd_anim_format::vmd::VmdBoneKeyframeRaw;
use mmd_anim_runtime::{BoneIndex, IkSolveOptions, ModelArena, MorphIndex, RuntimeInstance};

use super::golden;

pub(crate) const DIAGNOSE_NUMERIC_BONE_USAGE: &str = "usage: mmd-anim diagnose-numeric-bone <manifest.json> <case-name> <oracle-frame> [--eval-frame <frame>] <bone-name> [bone-name...]";
const NUMERIC_DEFAULT_EPSILON: f64 = 0.003;

pub(crate) fn compare_numeric_manifest(
    path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let report = build_numeric_compare_report(path, true)?;
    let camera_stats = &report.camera_stats;
    let motion_stats = &report.motion_stats;
    let default_epsilon = report.default_epsilon;

    let failure_count = numeric_compare_failure_count(camera_stats, motion_stats);
    if failure_count == 0 {
        println!(
            "Numeric compare: ok cameraCases={} cameraFrames={} cameraMaxDelta={:.6} motionCases={} motionComparedCases={} motionSkippedUnsupported={} motionMissing={} motionImportErrors={} motionFrames={} motionBones={} motionMaxAbsError={:.6} motionWorst={} motionSkippedTargets={} defaultEpsilon={}",
            camera_stats.compared_cases,
            camera_stats.compared_frames,
            camera_stats.max_delta,
            motion_stats.total_cases,
            motion_stats.compared_cases,
            motion_stats.skipped_unsupported,
            motion_stats.missing,
            motion_stats.import_errors,
            motion_stats.compared_frames,
            motion_stats.compared_bones,
            motion_stats.max_abs_error,
            motion_stats.worst,
            motion_stats.skipped_targets_csv(),
            default_epsilon
        );
        Ok(ExitCode::SUCCESS)
    } else {
        Err(format!(
            "Numeric compare failed: failures={} cameraMismatches={} motionMismatches={} motionMissing={} motionImportErrors={} cameraMaxDelta={:.6} motionMaxAbsError={:.6} motionWorst={} defaultEpsilon={}",
            failure_count,
            camera_stats.mismatch_count,
            motion_stats.mismatch_count,
            motion_stats.missing,
            motion_stats.import_errors,
            camera_stats.max_delta,
            motion_stats.max_abs_error,
            motion_stats.worst,
            default_epsilon
        )
        .into())
    }
}

pub(crate) fn compare_numeric_manifest_json(
    path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let report = build_numeric_compare_report(path, false)?;
    println!("{}", serde_json::to_string(&report.to_json())?);
    Ok(ExitCode::SUCCESS)
}

pub(crate) struct NumericCompareReport {
    pub(crate) default_epsilon: f64,
    pub(crate) camera_stats: CameraNumericCompareStats,
    pub(crate) motion_stats: MotionNumericCompareStats,
    pub(crate) per_case: Vec<serde_json::Value>,
}

impl NumericCompareReport {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        let summary = serde_json::json!({
            "cases": self.motion_stats.total_cases + self.camera_stats.compared_cases,
            "comparedCases": self.motion_stats.compared_cases + self.camera_stats.compared_cases,
            "missing": self.motion_stats.missing,
            "importErrors": self.motion_stats.import_errors,
            "comparedFrames": self.motion_stats.compared_frames + self.camera_stats.compared_frames,
            "comparedBones": self.motion_stats.compared_bones,
            "mismatchCount": self.motion_stats.mismatch_count + self.camera_stats.mismatch_count,
            "maxAbsError": self.motion_stats.max_abs_error,
            "worst": self.motion_stats.worst,
            "worstFrame": self.motion_stats.worst_frame,
            "worstBone": empty_string_as_null(&self.motion_stats.worst_bone),
            "worstComponent": self.motion_stats.worst_component,
            "skippedTargets": self.motion_stats.skipped_targets_sorted(),
            "motionCases": self.motion_stats.total_cases,
            "motionComparedCases": self.motion_stats.compared_cases,
            "motionSkippedUnsupported": self.motion_stats.skipped_unsupported,
            "motionMissing": self.motion_stats.missing,
            "motionImportErrors": self.motion_stats.import_errors,
            "motionComparedFrames": self.motion_stats.compared_frames,
            "motionComparedBones": self.motion_stats.compared_bones,
            "motionMismatches": self.motion_stats.mismatch_count,
            "motionMaxAbsError": self.motion_stats.max_abs_error,
            "motionWorst": self.motion_stats.worst,
            "cameraCases": self.camera_stats.compared_cases,
            "cameraFrames": self.camera_stats.compared_frames,
            "cameraMismatches": self.camera_stats.mismatch_count,
            "cameraMaxDelta": self.camera_stats.max_delta,
            "defaultEpsilon": self.default_epsilon,
            "skippedUnsupported": self.motion_stats.skipped_unsupported,
        });
        serde_json::json!({
            "summary": summary,
            "perCase": self.per_case,
        })
    }
}

pub(crate) fn build_numeric_compare_report(
    path: &Path,
    emit_diagnostics: bool,
) -> Result<NumericCompareReport, Box<dyn std::error::Error>> {
    let manifest_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let manifest_bytes = fs::read(path)
        .map_err(|error| format!("failed to read manifest {}: {}", path.display(), error))?;
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes)?;
    let out_dir = manifest
        .pointer("/defaults/outDir")
        .and_then(|value| value.as_str())
        .map(|path| resolve_manifest_path(manifest_dir, path));
    let default_epsilon = manifest
        .pointer("/defaults/compare/epsilon")
        .or_else(|| manifest.pointer("/defaults/epsilon"))
        .and_then(|value| value.as_f64())
        .unwrap_or(NUMERIC_DEFAULT_EPSILON);
    let default_focus_bones = json_string_array(&manifest, "/defaults/focus/bones");
    let default_motion_eval_frame_offset = json_f32(&manifest, "/defaults/compare/evalFrameOffset")
        .or_else(|| json_f32(&manifest, "/defaults/evalFrameOffset"))
        .unwrap_or(0.0);
    let cases = manifest
        .get("cases")
        .and_then(|value| value.as_array())
        .ok_or("numeric compare manifest is missing cases")?;
    let mut camera_stats = CameraNumericCompareStats::default();
    let mut motion_stats = MotionNumericCompareStats::default();
    let mut per_case = Vec::new();

    for case in cases {
        let name = case
            .get("name")
            .and_then(|value| value.as_str())
            .ok_or("numeric compare case is missing name")?;
        let kind = case
            .get("kind")
            .and_then(|value| value.as_str())
            .ok_or_else(|| format!("{name} is missing kind"))?;
        match kind {
            "camera-vmd" => {
                let case_dir = out_dir.as_ref().map(|out_dir| out_dir.join(name));
                per_case.push(compare_camera_numeric_case(
                    case,
                    manifest_dir,
                    case_dir.as_deref(),
                    default_epsilon,
                    &mut camera_stats,
                    emit_diagnostics,
                )?);
            }
            "motion-numeric" | "physics-coarse" => {
                per_case.push(compare_motion_numeric_case(
                    case,
                    manifest_dir,
                    default_epsilon,
                    default_focus_bones.as_deref(),
                    default_motion_eval_frame_offset,
                    &mut motion_stats,
                    emit_diagnostics,
                )?);
            }
            _ => {
                return Err(format!(
                    "numeric compare case {} has unsupported kind {}; supported kinds: camera-vmd, motion-numeric, physics-coarse",
                    name, kind
                )
                .into());
            }
        }
    }

    Ok(NumericCompareReport {
        default_epsilon,
        camera_stats,
        motion_stats,
        per_case,
    })
}

#[derive(Default)]
pub(crate) struct CameraNumericCompareStats {
    pub(crate) compared_cases: usize,
    pub(crate) compared_frames: usize,
    pub(crate) mismatch_count: usize,
    pub(crate) max_delta: f64,
}

impl CameraNumericCompareStats {
    fn merge(&mut self, other: &Self) {
        self.compared_cases += other.compared_cases;
        self.compared_frames += other.compared_frames;
        self.mismatch_count += other.mismatch_count;
        self.max_delta = self.max_delta.max(other.max_delta);
    }
}

fn compare_camera_numeric_case(
    case: &serde_json::Value,
    manifest_dir: &Path,
    case_dir: Option<&Path>,
    default_epsilon: f64,
    stats: &mut CameraNumericCompareStats,
    emit_diagnostics: bool,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let name = case
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or("numeric compare case is missing name")?;
    let epsilon = case
        .pointer("/compare/epsilon")
        .and_then(|value| value.as_f64())
        .unwrap_or(default_epsilon);
    let mut case_stats = CameraNumericCompareStats::default();
    let oracle_path = resolve_camera_oracle_path(case, manifest_dir, case_dir)?;
    let oracle_bytes = fs::read(&oracle_path).map_err(|error| {
        format!(
            "failed to read camera oracle for case {} at {}: {}",
            name,
            oracle_path.display(),
            error
        )
    })?;
    let oracle: serde_json::Value = serde_json::from_slice(&oracle_bytes)?;
    let camera_vmd = resolve_camera_vmd_path(case, manifest_dir, case_dir)?;
    let camera_vmd_bytes = fs::read(&camera_vmd).map_err(|error| {
        format!(
            "failed to read camera VMD for case {} at {}: {}",
            name,
            camera_vmd.display(),
            error
        )
    })?;
    let parsed = mmd_anim_format::parse_vmd_animation(&camera_vmd_bytes)?;
    let frames = oracle
        .get("frames")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("{} is missing frames", oracle_path.display()))?;

    case_stats.compared_cases += 1;
    for frame_record in frames {
        let frame = frame_record
            .get("frame")
            .and_then(|value| value.as_f64())
            .ok_or_else(|| format!("{name} has a frame record without frame"))?;
        let expected = frame_record
            .get("camera")
            .ok_or_else(|| format!("{name} frame {frame} is missing camera"))?;
        let actual = mmd_anim_format::sample_vmd_camera_frames(&parsed.camera_frames, frame as f32)
            .ok_or_else(|| format!("{} has no camera frames", camera_vmd.display()))?;
        let compare_context = CameraCompareContext {
            case_name: name,
            frame,
            epsilon,
            emit_diagnostics,
        };

        case_stats.compared_frames += 1;
        case_stats.mismatch_count += compare_camera_scalar(
            &compare_context,
            "distance",
            actual.distance as f64,
            expected_number(expected, "distance")?,
            &mut case_stats.max_delta,
        );
        case_stats.mismatch_count += compare_camera_vec3(
            &compare_context,
            "position",
            actual.position,
            expected_array3(expected, "position")?,
            &mut case_stats.max_delta,
        );
        case_stats.mismatch_count += compare_camera_vec3(
            &compare_context,
            "rotation",
            actual.rotation,
            expected_array3(expected, "rotation")?,
            &mut case_stats.max_delta,
        );
        case_stats.mismatch_count += compare_camera_scalar(
            &compare_context,
            "fov",
            actual.fov as f64,
            expected_number(expected, "fov")?,
            &mut case_stats.max_delta,
        );
        let expected_perspective = expected
            .get("perspective")
            .and_then(|value| value.as_bool())
            .ok_or_else(|| format!("{name} frame {frame} camera.perspective is missing"))?;
        if actual.perspective != expected_perspective {
            case_stats.mismatch_count += 1;
            if emit_diagnostics {
                eprintln!(
                    "camera mismatch case={} frame={} field=perspective actual={} expected={}",
                    name, frame, actual.perspective, expected_perspective
                );
            }
        }
    }
    stats.merge(&case_stats);
    Ok(camera_case_report(name, epsilon, &case_stats))
}

#[derive(Default)]
pub(crate) struct MotionNumericCompareStats {
    pub(crate) total_cases: usize,
    pub(crate) compared_cases: usize,
    pub(crate) skipped_unsupported: usize,
    pub(crate) missing: usize,
    pub(crate) import_errors: usize,
    pub(crate) compared_frames: usize,
    pub(crate) compared_bones: usize,
    pub(crate) mismatch_count: usize,
    pub(crate) skipped_targets: HashSet<String>,
    pub(crate) max_abs_error: f32,
    pub(crate) worst: String,
    pub(crate) worst_frame: Option<i32>,
    pub(crate) worst_bone: String,
    pub(crate) worst_component: Option<usize>,
}

pub(crate) fn numeric_compare_failure_count(
    camera_stats: &CameraNumericCompareStats,
    motion_stats: &MotionNumericCompareStats,
) -> usize {
    camera_stats.mismatch_count
        + motion_stats.mismatch_count
        + motion_stats.missing
        + motion_stats.import_errors
}

impl MotionNumericCompareStats {
    fn skipped_targets_csv(&self) -> String {
        self.skipped_targets_sorted().join(",")
    }

    pub(crate) fn skipped_targets_sorted(&self) -> Vec<String> {
        let mut targets: Vec<_> = self.skipped_targets.iter().cloned().collect();
        targets.sort();
        targets
    }

    fn merge(&mut self, other: &Self) {
        self.total_cases += other.total_cases;
        self.compared_cases += other.compared_cases;
        self.skipped_unsupported += other.skipped_unsupported;
        self.missing += other.missing;
        self.import_errors += other.import_errors;
        self.compared_frames += other.compared_frames;
        self.compared_bones += other.compared_bones;
        self.mismatch_count += other.mismatch_count;
        self.skipped_targets
            .extend(other.skipped_targets.iter().cloned());
        if self.worst.is_empty() {
            self.worst = String::from("none");
        }
        if other.max_abs_error > self.max_abs_error {
            self.max_abs_error = other.max_abs_error;
            self.worst = other.worst.clone();
            self.worst_frame = other.worst_frame;
            self.worst_bone = other.worst_bone.clone();
            self.worst_component = other.worst_component;
        }
    }
}

fn compare_motion_numeric_case(
    case: &serde_json::Value,
    manifest_dir: &Path,
    default_epsilon: f64,
    default_focus_bones: Option<&[String]>,
    default_eval_frame_offset: f32,
    stats: &mut MotionNumericCompareStats,
    emit_diagnostics: bool,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut case_stats = MotionNumericCompareStats {
        total_cases: 1,
        worst: String::from("none"),
        ..MotionNumericCompareStats::default()
    };
    let name = case
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or("numeric compare case is missing name")?;
    let epsilon = case
        .pointer("/compare/epsilon")
        .and_then(|value| value.as_f64())
        .unwrap_or(default_epsilon) as f32;
    let eval_frame_offset = json_f32(case, "/compare/evalFrameOffset")
        .or_else(|| json_f32(case, "/metadata/evalFrameOffset"))
        .unwrap_or(default_eval_frame_offset);
    collect_unsupported_targets(case, &mut case_stats.skipped_targets);

    let mut missing_paths = Vec::<String>::new();
    let model_path = match case
        .pointer("/assets/model")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
    {
        Some(path) => path,
        None => {
            case_stats.missing += 1;
            missing_paths.push("assets.model".to_owned());
            if emit_diagnostics {
                eprintln!("missing: {name} assets.model");
            }
            stats.merge(&case_stats);
            return Ok(motion_case_report(
                name,
                case,
                "missing",
                epsilon,
                &case_stats,
                missing_paths,
                None,
            ));
        }
    };
    let motion_path = match case
        .pointer("/assets/motion")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
    {
        Some(path) => path,
        None => {
            case_stats.missing += 1;
            missing_paths.push("assets.motion".to_owned());
            if emit_diagnostics {
                eprintln!("missing: {name} assets.motion");
            }
            stats.merge(&case_stats);
            return Ok(motion_case_report(
                name,
                case,
                "missing",
                epsilon,
                &case_stats,
                missing_paths,
                None,
            ));
        }
    };
    let oracle_path = match case
        .pointer("/oracle/path")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
    {
        Some(path) => path,
        None => {
            case_stats.missing += 1;
            missing_paths.push("oracle.path".to_owned());
            if emit_diagnostics {
                eprintln!("missing: {name} oracle.path");
            }
            stats.merge(&case_stats);
            return Ok(motion_case_report(
                name,
                case,
                "missing",
                epsilon,
                &case_stats,
                missing_paths,
                None,
            ));
        }
    };

    if !golden::is_supported_golden_model(&model_path) {
        case_stats.skipped_unsupported += 1;
        let error = format!("unsupported model: {}", model_path.display());
        if emit_diagnostics {
            eprintln!("skipped unsupported model: {}", model_path.display());
        }
        stats.merge(&case_stats);
        return Ok(motion_case_report(
            name,
            case,
            "skipped-unsupported",
            epsilon,
            &case_stats,
            missing_paths,
            Some(error),
        ));
    }
    if !model_path.exists() || !motion_path.exists() || !oracle_path.exists() {
        case_stats.missing += 1;
        if !model_path.exists() {
            missing_paths.push(model_path.display().to_string());
            if emit_diagnostics {
                eprintln!("missing: {}", model_path.display());
            }
        }
        if !motion_path.exists() {
            missing_paths.push(motion_path.display().to_string());
            if emit_diagnostics {
                eprintln!("missing: {}", motion_path.display());
            }
        }
        if !oracle_path.exists() {
            missing_paths.push(oracle_path.display().to_string());
            if emit_diagnostics {
                eprintln!("missing: {}", oracle_path.display());
            }
        }
        stats.merge(&case_stats);
        return Ok(motion_case_report(
            name,
            case,
            "missing",
            epsilon,
            &case_stats,
            missing_paths,
            None,
        ));
    }

    let frames = numeric_case_frames(case)?;
    let dump =
        MmdDumperOracleDump::from_jsonl_str(&fs::read_to_string(&oracle_path)?, Some(&frames))?;
    let focus_bones = motion_case_focus_bones(case, default_focus_bones);
    let focus_bone_names: Vec<&str> = focus_bones.iter().map(String::as_str).collect();

    let model_bytes = fs::read(&model_path)?;
    let model_import = match golden::import_golden_runtime_model(&model_path, &model_bytes) {
        Ok(import) => import,
        Err(error) => {
            case_stats.import_errors += 1;
            let error = format!("{}: {}", model_path.display(), error);
            if emit_diagnostics {
                eprintln!("import-error: {}", error);
            }
            stats.merge(&case_stats);
            return Ok(motion_case_report(
                name,
                case,
                "import-error",
                epsilon,
                &case_stats,
                missing_paths,
                Some(error),
            ));
        }
    };
    let vmd_bytes = fs::read(&motion_path)?;
    let vmd = match mmd_anim_format::import_vmd_motion(&vmd_bytes) {
        Ok(vmd) => vmd,
        Err(error) => {
            case_stats.import_errors += 1;
            let error = format!("{}: {}", motion_path.display(), error);
            if emit_diagnostics {
                eprintln!("import-error: {}", error);
            }
            stats.merge(&case_stats);
            return Ok(motion_case_report(
                name,
                case,
                "import-error",
                epsilon,
                &case_stats,
                missing_paths,
                Some(error),
            ));
        }
    };

    let solver_count = model_import.model.ik_count();
    let clip = mmd_anim_format::build_pair_clip_with_options(
        &vmd,
        &model_import.bone_name_to_index,
        &model_import.morph_name_to_index,
        &model_import.ik_solver_bone_name_to_index,
        solver_count,
        mmd_anim_format::VmdClipBuildOptions {
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

    for oracle_frame in &dump.frames {
        let eval_frame = oracle_frame.frame as f32 + eval_frame_offset;
        runtime.evaluate_clip_frame(&clip, eval_frame);
        let Some(model0) = oracle_frame.models.first() else {
            continue;
        };
        let world_matrices = runtime.world_matrices();
        for oracle_bone in model0.focused_ik_bones(&focus_bone_names) {
            if oracle_bone.index < 0 {
                continue;
            }
            let index = oracle_bone.index as usize;
            if index >= world_matrices.len() {
                continue;
            }
            let runtime_matrix = world_matrices[index].to_cols_array();
            for (component, actual) in runtime_matrix.iter().enumerate() {
                let abs_error = (*actual - oracle_bone.world_matrix[component]).abs();
                if abs_error > case_stats.max_abs_error {
                    case_stats.max_abs_error = abs_error;
                    case_stats.worst =
                        format!("{}:{}:{}", name, oracle_frame.frame, oracle_bone.name);
                    case_stats.worst_frame = Some(oracle_frame.frame);
                    case_stats.worst_bone = oracle_bone.name.clone();
                    case_stats.worst_component = Some(component);
                }
                if abs_error > epsilon {
                    case_stats.mismatch_count += 1;
                    if emit_diagnostics {
                        eprintln!(
                            "motion mismatch case={} frame={} evalFrame={:.3} bone={} component={} actual={:.9} expected={:.9} delta={:.9} epsilon={:.9}",
                            name,
                            oracle_frame.frame,
                            eval_frame,
                            oracle_bone.name,
                            component,
                            actual,
                            oracle_bone.world_matrix[component],
                            abs_error,
                            epsilon
                        );
                    }
                }
            }
            case_stats.compared_bones += 1;
        }
        case_stats.compared_frames += 1;
    }
    case_stats.compared_cases += 1;
    let status = if case_stats.mismatch_count == 0 {
        "ok"
    } else {
        "mismatch"
    };
    stats.merge(&case_stats);
    Ok(motion_case_report(
        name,
        case,
        status,
        epsilon,
        &case_stats,
        missing_paths,
        None,
    ))
}

fn camera_case_report(
    name: &str,
    epsilon: f64,
    stats: &CameraNumericCompareStats,
) -> serde_json::Value {
    let status = if stats.mismatch_count == 0 {
        "ok"
    } else {
        "mismatch"
    };
    serde_json::json!({
        "name": name,
        "kind": "camera-vmd",
        "status": status,
        "epsilon": epsilon,
        "comparedFrames": stats.compared_frames,
        "comparedBones": 0,
        "mismatchCount": stats.mismatch_count,
        "maxAbsError": stats.max_delta,
        "cameraMaxDelta": stats.max_delta,
        "worst": serde_json::Value::Null,
        "worstFrame": serde_json::Value::Null,
        "worstBone": serde_json::Value::Null,
        "worstComponent": serde_json::Value::Null,
        "skippedTargets": Vec::<String>::new(),
        "missingPaths": Vec::<String>::new(),
        "error": serde_json::Value::Null,
    })
}

fn motion_case_report(
    name: &str,
    case: &serde_json::Value,
    status: &str,
    epsilon: f32,
    stats: &MotionNumericCompareStats,
    mut missing_paths: Vec<String>,
    error: Option<String>,
) -> serde_json::Value {
    missing_paths.sort();
    let kind = case
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("motion-numeric");
    serde_json::json!({
        "name": name,
        "kind": kind,
        "status": status,
        "epsilon": epsilon,
        "comparedFrames": stats.compared_frames,
        "comparedBones": stats.compared_bones,
        "mismatchCount": stats.mismatch_count,
        "maxAbsError": stats.max_abs_error,
        "worst": stats.worst,
        "worstFrame": stats.worst_frame,
        "worstBone": empty_string_as_null(&stats.worst_bone),
        "worstComponent": stats.worst_component,
        "skippedTargets": stats.skipped_targets_sorted(),
        "missingPaths": missing_paths,
        "missing": stats.missing,
        "importErrors": stats.import_errors,
        "error": error,
    })
}

fn empty_string_as_null(value: &str) -> serde_json::Value {
    if value.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(value.to_owned())
    }
}

pub(crate) fn diagnose_numeric_bones(
    manifest_path: &Path,
    case_name: &str,
    oracle_frame_number: f32,
    eval_frame: f32,
    bone_names: &[String],
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let manifest: serde_json::Value = serde_json::from_str(&fs::read_to_string(manifest_path)?)?;
    let cases = manifest
        .get("cases")
        .and_then(|value| value.as_array())
        .ok_or("numeric manifest is missing cases")?;
    let case = cases
        .iter()
        .find(|case| case.get("name").and_then(|value| value.as_str()) == Some(case_name))
        .ok_or_else(|| format!("numeric manifest has no case named {case_name}"))?;

    let model_path = case
        .pointer("/assets/model")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
        .ok_or("case is missing assets.model")?;
    let motion_path = case
        .pointer("/assets/motion")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
        .ok_or("case is missing assets.motion")?;
    let oracle_path = case
        .pointer("/oracle/path")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
        .ok_or("case is missing oracle.path")?;

    let target_frame = oracle_frame_number.round() as i32;
    let dump = MmdDumperOracleDump::from_jsonl_str(
        &fs::read_to_string(&oracle_path)?,
        Some(&[target_frame]),
    )?;
    let oracle_frame = dump
        .find_frame(target_frame)
        .ok_or_else(|| format!("oracle has no frame {target_frame}"))?;
    let oracle_model = oracle_frame
        .models
        .first()
        .ok_or_else(|| format!("oracle frame {target_frame} has no model"))?;

    let model_bytes = fs::read(&model_path)?;
    let model_import = golden::import_golden_runtime_model(&model_path, &model_bytes)?;
    let vmd = mmd_anim_format::import_vmd_motion(&fs::read(&motion_path)?)?;
    let solver_count = model_import.model.ik_count();
    let clip = mmd_anim_format::build_pair_clip_with_options(
        &vmd,
        &model_import.bone_name_to_index,
        &model_import.morph_name_to_index,
        &model_import.ik_solver_bone_name_to_index,
        solver_count,
        mmd_anim_format::VmdClipBuildOptions {
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
    let mut pre_ik =
        RuntimeInstance::new_with_counts(Arc::clone(&model), morph_count, solver_count);
    let mut post_ik =
        RuntimeInstance::new_with_counts(Arc::clone(&model), morph_count, solver_count);
    pre_ik.evaluate_clip_frame_without_ik(&clip, eval_frame);
    post_ik.evaluate_clip_frame_with_ik_options(&clip, eval_frame, IkSolveOptions::default());

    println!(
        "numeric bone diagnosis case={} oracleFrame={:.3} evalFrame={:.3} model={} motion={} oracle={}",
        case_name,
        oracle_frame_number,
        eval_frame,
        model_path.display(),
        motion_path.display(),
        oracle_path.display()
    );
    for bone_name in bone_names {
        let normalized = mmd_anim_format::normalize_vmd_name(bone_name.as_bytes());
        let Some(index) = model_import
            .bone_name_to_index
            .get(bone_name.as_bytes())
            .or_else(|| model_import.bone_name_to_index.get(&normalized))
            .copied()
        else {
            println!("bone={} missing runtimeIndex", bone_name);
            continue;
        };
        let Some(oracle_bone) = oracle_model.find_bone(bone_name) else {
            println!(
                "bone={} runtimeIndex={} missing oracleBone",
                bone_name,
                index.as_usize()
            );
            continue;
        };
        let pre = pre_ik.world_matrices()[index.as_usize()].to_cols_array();
        let post = post_ik.world_matrices()[index.as_usize()].to_cols_array();
        let (pre_component, pre_delta) = max_matrix_delta(&pre, &oracle_bone.world_matrix);
        let (post_component, post_delta) = max_matrix_delta(&post, &oracle_bone.world_matrix);
        let pre_pos_delta = position_delta(&pre, &oracle_bone.world_matrix);
        let post_pos_delta = position_delta(&post, &oracle_bone.world_matrix);
        let pre_local_pos = pre_ik.pose().local_position_offset(index);
        let post_local_pos = post_ik.pose().local_position_offset(index);
        let pre_local_rot = pre_ik.pose().local_rotation(index);
        let post_local_rot = post_ik.pose().local_rotation(index);
        let pre_local_axis = pre_local_rot.to_axis_angle();
        let post_local_axis = post_local_rot.to_axis_angle();
        let oracle_local = oracle_local_matrix(oracle_model, &model, oracle_bone);
        let (_, oracle_local_rot, oracle_local_pos) =
            glam::Mat4::from_cols_array(&oracle_local).to_scale_rotation_translation();
        let oracle_local_axis = oracle_local_rot.to_axis_angle();
        let vmd_keyframes: Vec<_> = vmd
            .bone_keyframes
            .iter()
            .filter(|kf| {
                model_import
                    .bone_name_to_index
                    .get(&kf.bone_name_normalized)
                    == Some(&index)
            })
            .collect();
        let vmd_lookup_frame = eval_frame;
        let exact_vmd_keyframes: Vec<_> = vmd_keyframes
            .iter()
            .copied()
            .filter(|kf| kf.frame as f32 == vmd_lookup_frame)
            .collect();
        let exact_vmd_rotation = exact_vmd_keyframes
            .first()
            .map(|kf| kf.rotation.to_axis_angle());
        let prev_vmd_keyframe = vmd_keyframes
            .iter()
            .copied()
            .filter(|kf| kf.frame as f32 <= vmd_lookup_frame)
            .max_by_key(|kf| kf.frame)
            .map(format_vmd_keyframe)
            .unwrap_or_else(|| "none".to_owned());
        let next_vmd_keyframe = vmd_keyframes
            .iter()
            .copied()
            .filter(|kf| kf.frame as f32 > vmd_lookup_frame)
            .min_by_key(|kf| kf.frame)
            .map(format_vmd_keyframe)
            .unwrap_or_else(|| "none".to_owned());
        let bone_morphs =
            describe_active_bone_morphs(&model_import.morph_name_to_index, &post_ik, &model, index);
        println!(
            "bone={} index={} oracleIndex={} preMaxDelta={:.9}@{} postMaxDelta={:.9}@{} prePosDelta=({:.6},{:.6},{:.6}) postPosDelta=({:.6},{:.6},{:.6}) prePos=({:.6},{:.6},{:.6}) postPos=({:.6},{:.6},{:.6}) oraclePos=({:.6},{:.6},{:.6}) preLocalOffset=({:.6},{:.6},{:.6}) postLocalOffset=({:.6},{:.6},{:.6}) oracleLocalPos=({:.6},{:.6},{:.6}) preLocalRotAxis=({:.6},{:.6},{:.6}) preLocalRotAngle={:.6} postLocalRotAxis=({:.6},{:.6},{:.6}) postLocalRotAngle={:.6} oracleLocalRotAxis=({:.6},{:.6},{:.6}) oracleLocalRotAngle={:.6} vmdKeys={} exactVmdKeys={} exactVmdRot={} prevVmd={} nextVmd={} activeBoneMorphs={}",
            bone_name,
            index.as_usize(),
            oracle_bone.index,
            pre_delta,
            pre_component,
            post_delta,
            post_component,
            pre_pos_delta[0],
            pre_pos_delta[1],
            pre_pos_delta[2],
            post_pos_delta[0],
            post_pos_delta[1],
            post_pos_delta[2],
            pre[12],
            pre[13],
            pre[14],
            post[12],
            post[13],
            post[14],
            oracle_bone.world_matrix[12],
            oracle_bone.world_matrix[13],
            oracle_bone.world_matrix[14],
            pre_local_pos.x,
            pre_local_pos.y,
            pre_local_pos.z,
            post_local_pos.x,
            post_local_pos.y,
            post_local_pos.z,
            oracle_local_pos.x,
            oracle_local_pos.y,
            oracle_local_pos.z,
            pre_local_axis.0.x,
            pre_local_axis.0.y,
            pre_local_axis.0.z,
            pre_local_axis.1,
            post_local_axis.0.x,
            post_local_axis.0.y,
            post_local_axis.0.z,
            post_local_axis.1,
            oracle_local_axis.0.x,
            oracle_local_axis.0.y,
            oracle_local_axis.0.z,
            oracle_local_axis.1,
            vmd_keyframes.len(),
            exact_vmd_keyframes.len(),
            exact_vmd_rotation
                .map(|axis| format!(
                    "axis=({:.6},{:.6},{:.6}) angle={:.6}",
                    axis.0.x, axis.0.y, axis.0.z, axis.1
                ))
                .unwrap_or_else(|| "none".to_owned()),
            prev_vmd_keyframe,
            next_vmd_keyframe,
            bone_morphs,
        );
    }

    Ok(ExitCode::SUCCESS)
}

pub(crate) struct DiagnoseNumericBoneOptions {
    pub(crate) eval_frame: f32,
    pub(crate) bone_names: Vec<String>,
}

pub(crate) fn parse_diagnose_numeric_bone_rest(
    rest: Vec<String>,
    default_eval_frame: f32,
) -> DiagnoseNumericBoneOptions {
    let mut eval_frame = default_eval_frame;
    let mut bone_names = Vec::new();
    let mut iter = rest.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--eval-frame" {
            let Some(value) = iter.next() else {
                eprintln!("{DIAGNOSE_NUMERIC_BONE_USAGE}");
                std::process::exit(1);
            };
            eval_frame = value.parse().unwrap_or_else(|_| {
                eprintln!("{DIAGNOSE_NUMERIC_BONE_USAGE}");
                std::process::exit(1);
            });
        } else if arg.starts_with("--") {
            eprintln!("unknown flag: {arg}");
            eprintln!("{DIAGNOSE_NUMERIC_BONE_USAGE}");
            std::process::exit(1);
        } else {
            bone_names.push(arg);
        }
    }
    DiagnoseNumericBoneOptions {
        eval_frame,
        bone_names,
    }
}

fn describe_active_bone_morphs(
    morph_name_to_index: &HashMap<Vec<u8>, MorphIndex>,
    runtime: &RuntimeInstance,
    model: &ModelArena,
    target_bone: BoneIndex,
) -> String {
    let mut entries = Vec::new();
    for morph_index in 0..model.morph_count() as usize {
        let span = model.bone_morph_spans()[morph_index];
        if span.count == 0 {
            continue;
        }
        let weight = runtime.pose().morph_weight(MorphIndex(morph_index as u32));
        for offset_index in span.start..span.start + span.count {
            let offset = model.bone_morph_offsets()[offset_index as usize];
            if offset.target_bone != target_bone {
                continue;
            }
            let axis = offset.rotation_offset.to_axis_angle();
            entries.push(format!(
                "morph={} name={} weight={:.6} pos=({:.6},{:.6},{:.6}) rotAxis=({:.6},{:.6},{:.6}) rotAngle={:.6}",
                morph_index,
                morph_names_for_index(morph_name_to_index, MorphIndex(morph_index as u32)).join("|"),
                weight,
                offset.position_offset.x,
                offset.position_offset.y,
                offset.position_offset.z,
                axis.0.x,
                axis.0.y,
                axis.0.z,
                axis.1
            ));
        }
    }
    if entries.is_empty() {
        "none".to_owned()
    } else {
        entries.join(";")
    }
}

fn morph_names_for_index(
    morph_name_to_index: &HashMap<Vec<u8>, MorphIndex>,
    target_index: MorphIndex,
) -> Vec<String> {
    let mut names: Vec<String> = morph_name_to_index
        .iter()
        .filter_map(|(name, index)| {
            if *index != target_index {
                return None;
            }
            String::from_utf8(name.clone()).ok()
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

fn format_vmd_keyframe(kf: &VmdBoneKeyframeRaw) -> String {
    let axis = kf.rotation.to_axis_angle();
    format!(
        "frame={} pos=({:.6},{:.6},{:.6}) rotAxis=({:.6},{:.6},{:.6}) rotAngle={:.6}",
        kf.frame, kf.position.x, kf.position.y, kf.position.z, axis.0.x, axis.0.y, axis.0.z, axis.1
    )
}

fn oracle_local_matrix(
    oracle_model: &MmdDumperOracleModel,
    model: &ModelArena,
    oracle_bone: &MmdDumperOracleBone,
) -> [f32; 16] {
    let bone_matrix = glam::Mat4::from_cols_array(&oracle_bone.world_matrix);
    let Some(parent) = model.parent_index(BoneIndex(oracle_bone.index as u32)) else {
        return oracle_bone.world_matrix;
    };
    let Some(parent_bone) = oracle_model
        .bones
        .iter()
        .find(|bone| bone.index == parent.as_usize() as i32)
    else {
        return oracle_bone.world_matrix;
    };
    let parent_matrix = glam::Mat4::from_cols_array(&parent_bone.world_matrix);
    (parent_matrix.inverse() * bone_matrix).to_cols_array()
}

fn max_matrix_delta(actual: &[f32; 16], expected: &[f32; 16]) -> (usize, f32) {
    let mut max_component = 0;
    let mut max_delta = 0.0f32;
    for component in 0..16 {
        let delta = (actual[component] - expected[component]).abs();
        if delta > max_delta {
            max_component = component;
            max_delta = delta;
        }
    }
    (max_component, max_delta)
}

fn position_delta(actual: &[f32; 16], expected: &[f32; 16]) -> [f32; 3] {
    [
        actual[12] - expected[12],
        actual[13] - expected[13],
        actual[14] - expected[14],
    ]
}

pub(crate) fn motion_case_focus_bones(
    case: &serde_json::Value,
    default_focus_bones: Option<&[String]>,
) -> Vec<String> {
    json_string_array(case, "/metadata/focus/bones")
        .or_else(|| json_string_array(case, "/focus/bones"))
        .or_else(|| default_focus_bones.map(|bones| bones.to_vec()))
        .unwrap_or_else(|| {
            DEFAULT_FOCUSED_IK_BONE_NAMES
                .iter()
                .map(|name| (*name).to_owned())
                .collect()
        })
}

pub(crate) fn json_string_array(value: &serde_json::Value, pointer: &str) -> Option<Vec<String>> {
    let values = value.pointer(pointer)?.as_array()?;
    let strings: Vec<String> = values
        .iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect();
    (!strings.is_empty()).then_some(strings)
}

pub(crate) fn json_f32(value: &serde_json::Value, pointer: &str) -> Option<f32> {
    value.pointer(pointer)?.as_f64().map(|value| value as f32)
}

fn collect_unsupported_targets(case: &serde_json::Value, skipped_targets: &mut HashSet<String>) {
    let Some(targets) = case
        .pointer("/compare/targets")
        .and_then(|value| value.as_array())
    else {
        return;
    };
    for target in targets {
        let Some(target) = target.as_str() else {
            continue;
        };
        if !matches!(target, "bones") {
            skipped_targets.insert(target.to_owned());
        }
    }
}

fn numeric_case_frames(case: &serde_json::Value) -> Result<Vec<i32>, Box<dyn std::error::Error>> {
    let frames = case
        .get("frames")
        .and_then(|value| value.as_array())
        .ok_or("numeric compare case is missing frames")?;
    frames
        .iter()
        .map(|frame| {
            frame
                .as_i64()
                .and_then(|frame| i32::try_from(frame).ok())
                .ok_or_else(|| "numeric compare frame must be an i32".into())
        })
        .collect()
}

pub(crate) fn resolve_manifest_path(manifest_dir: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        manifest_dir.join(path)
    }
}

fn resolve_camera_vmd_path(
    case: &serde_json::Value,
    manifest_dir: &Path,
    case_dir: Option<&Path>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let camera_vmd = case
        .pointer("/assets/cameraMotion")
        .or_else(|| case.pointer("/assets/cameraVmd"))
        .or_else(|| case.get("cameraVmd"))
        .or_else(|| case.get("cameraMotion"))
        .and_then(|value| value.as_str())
        .ok_or("camera manifest case is missing assets.cameraMotion/cameraVmd")?;
    let camera_vmd = resolve_manifest_path(manifest_dir, camera_vmd);
    if camera_vmd.exists() {
        return Ok(camera_vmd);
    }

    let fixture_path = case
        .get("fixture")
        .and_then(|value| value.as_str())
        .map(|path| resolve_manifest_path(manifest_dir, path))
        .or_else(|| case_dir.map(|case_dir| case_dir.join("fixture.json")));
    let Some(fixture_path) = fixture_path else {
        return Err(format!(
            "{} does not exist and no fixture path is available",
            camera_vmd.display()
        )
        .into());
    };
    let fixture: serde_json::Value = serde_json::from_slice(&fs::read(&fixture_path)?)?;
    let staged = fixture
        .get("stagedCameraVmd")
        .or_else(|| fixture.get("stagedCameraMotion"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            format!(
                "{} does not exist and {} is missing stagedCameraVmd/stagedCameraMotion",
                camera_vmd.display(),
                fixture_path.display()
            )
        })?;
    let fixture_dir = fixture_path.parent().unwrap_or(manifest_dir);
    Ok(resolve_manifest_path(fixture_dir, staged))
}

fn resolve_camera_oracle_path(
    case: &serde_json::Value,
    manifest_dir: &Path,
    case_dir: Option<&Path>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(output) = case
        .pointer("/oracle/path")
        .or_else(|| case.get("output"))
        .and_then(|value| value.as_str())
    {
        return Ok(resolve_manifest_path(manifest_dir, output));
    }
    if let Some(case_dir) = case_dir {
        return Ok(case_dir.join("oracle.actual.json"));
    }
    Err(
        "camera manifest case is missing oracle.path/output and no defaults.outDir is available"
            .into(),
    )
}

fn expected_number(
    camera: &serde_json::Value,
    field: &str,
) -> Result<f64, Box<dyn std::error::Error>> {
    camera
        .get(field)
        .and_then(|value| value.as_f64())
        .ok_or_else(|| format!("camera.{field} is missing").into())
}

fn expected_array3(
    camera: &serde_json::Value,
    field: &str,
) -> Result<[f64; 3], Box<dyn std::error::Error>> {
    let values = camera
        .get(field)
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("camera.{field} is missing"))?;
    if values.len() != 3 {
        return Err(format!("camera.{field} must have exactly 3 values").into());
    }
    Ok([
        values[0]
            .as_f64()
            .ok_or_else(|| format!("camera.{field}[0] is not a number"))?,
        values[1]
            .as_f64()
            .ok_or_else(|| format!("camera.{field}[1] is not a number"))?,
        values[2]
            .as_f64()
            .ok_or_else(|| format!("camera.{field}[2] is not a number"))?,
    ])
}

struct CameraCompareContext<'a> {
    case_name: &'a str,
    frame: f64,
    epsilon: f64,
    emit_diagnostics: bool,
}

fn compare_camera_vec3(
    context: &CameraCompareContext<'_>,
    field: &str,
    actual: [f32; 3],
    expected: [f64; 3],
    max_delta: &mut f64,
) -> usize {
    let mut mismatches = 0usize;
    for component in 0..3 {
        let component_field = format!("{field}[{component}]");
        mismatches += compare_camera_scalar(
            context,
            &component_field,
            actual[component] as f64,
            expected[component],
            max_delta,
        );
    }
    mismatches
}

fn compare_camera_scalar(
    context: &CameraCompareContext<'_>,
    field: &str,
    actual: f64,
    expected: f64,
    max_delta: &mut f64,
) -> usize {
    let delta = (actual - expected).abs();
    *max_delta = (*max_delta).max(delta);
    if delta <= context.epsilon {
        0
    } else {
        if context.emit_diagnostics {
            eprintln!(
                "camera mismatch case={} frame={} field={} actual={:.9} expected={:.9} delta={:.9}",
                context.case_name, context.frame, field, actual, expected, delta
            );
        }
        1
    }
}
