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
#[cfg(feature = "physics-bullet-native")]
use mmd_anim_physics_bullet::{RuntimePhysicsBridgeExt, build_bullet_world_from_pmx};
use mmd_anim_runtime::{BoneIndex, IkSolveOptions, ModelArena, MorphIndex, RuntimeInstance};
#[cfg(feature = "physics-bullet-native")]
use mmd_anim_runtime::{PhysicsMode, PhysicsTickConfig};
use serde::Serialize;

use super::golden;

pub(crate) const DIAGNOSE_NUMERIC_BONE_USAGE: &str = "usage: mmd-anim diagnose-numeric-bone <manifest.json> <case-name> <oracle-frame> [--eval-frame <frame>] <bone-name> [bone-name...]";
const NUMERIC_DEFAULT_EPSILON: f64 = 0.003;
#[cfg(feature = "physics-bullet-native")]
const PHYSICS_COARSE_MMD_FRAME_DT: f32 = 1.0 / 30.0;

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
            "Numeric compare: ok cameraCases={} cameraFrames={} cameraMaxDelta={:.6} motionCases={} motionComparedCases={} motionSkippedUnsupported={} motionNoTargets={} motionMissing={} motionImportErrors={} motionFrames={} motionBones={} motionMaxAbsError={:.6} motionTranslationMaxError={:.6} motionRotationMaxAngleRad={:.6} motionWorst={} motionSkippedTargets={} defaultEpsilon={}",
            camera_stats.compared_cases,
            camera_stats.compared_frames,
            camera_stats.max_delta,
            motion_stats.total_cases,
            motion_stats.compared_cases,
            motion_stats.skipped_unsupported,
            motion_stats.no_targets,
            motion_stats.missing,
            motion_stats.import_errors,
            motion_stats.compared_frames,
            motion_stats.compared_bones,
            motion_stats.max_abs_error,
            motion_stats.translation_max_error,
            motion_stats.rotation_max_angle_rad,
            motion_stats.worst,
            motion_stats.skipped_targets_csv(),
            default_epsilon
        );
        Ok(ExitCode::SUCCESS)
    } else {
        Err(format!(
            "Numeric compare failed: failures={} cameraMismatches={} motionMismatches={} motionNoTargets={} motionMissing={} motionImportErrors={} cameraMaxDelta={:.6} motionMaxAbsError={:.6} motionTranslationMaxError={:.6} motionRotationMaxAngleRad={:.6} motionWorst={} defaultEpsilon={}",
            failure_count,
            camera_stats.mismatch_count,
            motion_stats.mismatch_count,
            motion_stats.no_targets,
            motion_stats.missing,
            motion_stats.import_errors,
            camera_stats.max_delta,
            motion_stats.max_abs_error,
            motion_stats.translation_max_error,
            motion_stats.rotation_max_angle_rad,
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
    pub(crate) per_case: Vec<NumericCompareCaseReport>,
}

#[derive(Clone, Copy, Debug, Default)]
struct MotionNumericCompareDefaults<'a> {
    epsilon: f64,
    focus_bones: Option<&'a [String]>,
    eval_frame_offset: f32,
}

impl NumericCompareReport {
    pub(crate) fn to_json(&self) -> serde_json::Value {
        let skipped_targets = self.skipped_targets_sorted();
        let summary = NumericCompareJsonSummary {
            cases: self.motion_stats.total_cases + self.camera_stats.compared_cases,
            compared_cases: self.motion_stats.compared_cases + self.camera_stats.compared_cases,
            missing: self.motion_stats.missing,
            import_errors: self.motion_stats.import_errors,
            compared_frames: self.motion_stats.compared_frames + self.camera_stats.compared_frames,
            compared_bones: self.motion_stats.compared_bones,
            mismatch_count: self.motion_stats.mismatch_count + self.camera_stats.mismatch_count,
            max_abs_error: f64::from(self.motion_stats.max_abs_error)
                .max(self.camera_stats.max_delta),
            worst: self.motion_stats.worst.as_str(),
            worst_frame: self.motion_stats.worst_frame,
            worst_bone: non_empty_str(&self.motion_stats.worst_bone),
            worst_component: self.motion_stats.worst_component,
            skipped_targets,
            motion_cases: self.motion_stats.total_cases,
            motion_compared_cases: self.motion_stats.compared_cases,
            motion_skipped_unsupported: self.motion_stats.skipped_unsupported,
            motion_no_targets: self.motion_stats.no_targets,
            motion_missing: self.motion_stats.missing,
            motion_import_errors: self.motion_stats.import_errors,
            motion_compared_frames: self.motion_stats.compared_frames,
            motion_compared_bones: self.motion_stats.compared_bones,
            motion_mismatches: self.motion_stats.mismatch_count,
            motion_max_abs_error: self.motion_stats.max_abs_error,
            motion_translation_max_error: self.motion_stats.translation_max_error,
            motion_translation_rms_error: self.motion_stats.translation_rms_error(),
            motion_worst_translation_frame: self.motion_stats.worst_translation_frame,
            motion_worst_translation_bone: non_empty_str(&self.motion_stats.worst_translation_bone),
            motion_worst_translation_axis: self.motion_stats.worst_translation_axis,
            motion_rotation_max_angle_rad: self.motion_stats.rotation_max_angle_rad,
            motion_rotation_rms_angle_rad: self.motion_stats.rotation_rms_angle_rad(),
            motion_worst_rotation_frame: self.motion_stats.worst_rotation_frame,
            motion_worst_rotation_bone: non_empty_str(&self.motion_stats.worst_rotation_bone),
            motion_worst: self.motion_stats.worst.as_str(),
            camera_cases: self.camera_stats.compared_cases,
            camera_frames: self.camera_stats.compared_frames,
            camera_mismatches: self.camera_stats.mismatch_count,
            camera_max_delta: self.camera_stats.max_delta,
            default_epsilon: self.default_epsilon,
            skipped_unsupported: self.motion_stats.skipped_unsupported,
        };
        serde_json::to_value(NumericCompareJsonReport {
            summary,
            per_case: &self.per_case,
        })
        .expect("numeric compare report is serializable")
    }

    fn skipped_targets_sorted(&self) -> Vec<String> {
        let mut targets: HashSet<String> = self.motion_stats.skipped_targets.clone();
        targets.extend(self.camera_stats.skipped_targets.iter().cloned());
        let mut targets: Vec<_> = targets.into_iter().collect();
        targets.sort();
        targets
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NumericCompareJsonReport<'a> {
    summary: NumericCompareJsonSummary<'a>,
    #[serde(rename = "perCase")]
    per_case: &'a [NumericCompareCaseReport],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NumericCompareJsonSummary<'a> {
    cases: usize,
    compared_cases: usize,
    missing: usize,
    import_errors: usize,
    compared_frames: usize,
    compared_bones: usize,
    mismatch_count: usize,
    max_abs_error: f64,
    worst: &'a str,
    worst_frame: Option<i32>,
    worst_bone: Option<&'a str>,
    worst_component: Option<usize>,
    skipped_targets: Vec<String>,
    motion_cases: usize,
    motion_compared_cases: usize,
    motion_skipped_unsupported: usize,
    motion_no_targets: usize,
    motion_missing: usize,
    motion_import_errors: usize,
    motion_compared_frames: usize,
    motion_compared_bones: usize,
    motion_mismatches: usize,
    motion_max_abs_error: f32,
    motion_translation_max_error: f32,
    motion_translation_rms_error: f64,
    motion_worst_translation_frame: Option<i32>,
    motion_worst_translation_bone: Option<&'a str>,
    motion_worst_translation_axis: Option<usize>,
    motion_rotation_max_angle_rad: f32,
    motion_rotation_rms_angle_rad: f64,
    motion_worst_rotation_frame: Option<i32>,
    motion_worst_rotation_bone: Option<&'a str>,
    motion_worst: &'a str,
    camera_cases: usize,
    camera_frames: usize,
    camera_mismatches: usize,
    camera_max_delta: f64,
    default_epsilon: f64,
    skipped_unsupported: usize,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum NumericCompareCaseReport {
    Camera(CameraNumericCompareCaseReport),
    Motion(MotionNumericCompareCaseReport),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NumericCompareCaseCore {
    name: String,
    kind: String,
    status: String,
    epsilon: f64,
    compared_frames: usize,
    compared_bones: usize,
    mismatch_count: usize,
    max_abs_error: f64,
    worst: Option<String>,
    worst_frame: Option<i32>,
    worst_bone: Option<String>,
    worst_component: Option<usize>,
    skipped_targets: Vec<String>,
    missing_paths: Vec<String>,
    error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CameraNumericCompareCaseReport {
    #[serde(flatten)]
    core: NumericCompareCaseCore,
    camera_max_delta: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MotionNumericCompareCaseReport {
    #[serde(flatten)]
    core: NumericCompareCaseCore,
    physics_backend: Option<String>,
    translation_max_error: f32,
    translation_rms_error: f64,
    worst_translation_frame: Option<i32>,
    worst_translation_bone: Option<String>,
    worst_translation_axis: Option<usize>,
    rotation_max_angle_rad: f32,
    rotation_rms_angle_rad: f64,
    worst_rotation_frame: Option<i32>,
    worst_rotation_bone: Option<String>,
    no_targets: usize,
    missing: usize,
    import_errors: usize,
}

fn non_empty_str(value: &str) -> Option<&str> {
    if value.is_empty() { None } else { Some(value) }
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
    let motion_defaults = MotionNumericCompareDefaults {
        epsilon: default_epsilon,
        focus_bones: default_focus_bones.as_deref(),
        eval_frame_offset: json_f32(&manifest, "/defaults/compare/evalFrameOffset")
            .or_else(|| json_f32(&manifest, "/defaults/evalFrameOffset"))
            .unwrap_or(0.0),
    };
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
            "camera-numeric-dump" => {
                let case_dir = out_dir.as_ref().map(|out_dir| out_dir.join(name));
                per_case.push(compare_camera_current_dump_case(
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
                    motion_defaults,
                    &mut motion_stats,
                    emit_diagnostics,
                )?);
            }
            _ => {
                return Err(format!(
                    "numeric compare case {} has unsupported kind {}; supported kinds: camera-vmd, camera-numeric-dump, motion-numeric, physics-coarse",
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
    pub(crate) skipped_targets: HashSet<String>,
    pub(crate) max_delta: f64,
}

impl CameraNumericCompareStats {
    fn merge(&mut self, other: &Self) {
        self.compared_cases += other.compared_cases;
        self.compared_frames += other.compared_frames;
        self.mismatch_count += other.mismatch_count;
        self.skipped_targets
            .extend(other.skipped_targets.iter().cloned());
        self.max_delta = self.max_delta.max(other.max_delta);
    }

    pub(crate) fn skipped_targets_sorted(&self) -> Vec<String> {
        let mut targets: Vec<_> = self.skipped_targets.iter().cloned().collect();
        targets.sort();
        targets
    }
}

fn compare_camera_numeric_case(
    case: &serde_json::Value,
    manifest_dir: &Path,
    case_dir: Option<&Path>,
    default_epsilon: f64,
    stats: &mut CameraNumericCompareStats,
    emit_diagnostics: bool,
) -> Result<NumericCompareCaseReport, Box<dyn std::error::Error>> {
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
    Ok(camera_case_report(name, "camera-vmd", epsilon, &case_stats))
}

fn compare_camera_current_dump_case(
    case: &serde_json::Value,
    manifest_dir: &Path,
    case_dir: Option<&Path>,
    default_epsilon: f64,
    stats: &mut CameraNumericCompareStats,
    emit_diagnostics: bool,
) -> Result<NumericCompareCaseReport, Box<dyn std::error::Error>> {
    let name = case
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or("numeric compare case is missing name")?;
    let epsilon = case
        .pointer("/compare/epsilon")
        .and_then(|value| value.as_f64())
        .unwrap_or(default_epsilon);
    let mut case_stats = CameraNumericCompareStats::default();
    collect_unsupported_camera_targets(case, &mut case_stats.skipped_targets);
    let oracle_path = resolve_camera_oracle_path(case, manifest_dir, case_dir)?;
    let oracle_text = fs::read_to_string(&oracle_path).map_err(|error| {
        format!(
            "failed to read camera current oracle for case {} at {}: {}",
            name,
            oracle_path.display(),
            error
        )
    })?;
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

    case_stats.compared_cases += 1;
    for (line_index, line) in oracle_text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: serde_json::Value = serde_json::from_str(line).map_err(|error| {
            format!(
                "{}:{} is not valid JSON: {}",
                oracle_path.display(),
                line_index + 1,
                error
            )
        })?;
        let frame = record
            .get("frame")
            .and_then(|value| value.as_f64())
            .ok_or_else(|| {
                format!(
                    "{}:{} is missing frame",
                    oracle_path.display(),
                    line_index + 1
                )
            })?;
        let camera = record
            .get("camera")
            .ok_or_else(|| format!("{name} frame {frame} is missing camera"))?;
        if camera
            .get("available")
            .and_then(|value| value.as_bool())
            .is_some_and(|available| !available)
        {
            continue;
        }
        let expected = camera
            .get("current")
            .ok_or_else(|| format!("{name} frame {frame} is missing camera.current"))?;
        let sampled =
            mmd_anim_format::sample_vmd_camera_frames(&parsed.camera_frames, frame as f32)
                .ok_or_else(|| format!("{} has no camera frames", camera_vmd.display()))?;
        let expected_distance = expected_number(expected, "distance")?;
        let expected_position = expected_array3(expected, "position")?;
        let expected_rotation = expected_array3(expected, "rotation")?;
        let actual = best_mmd_current_camera_candidate(
            sampled,
            expected_distance,
            expected_position,
            expected_rotation,
        );
        let compare_context = CameraCompareContext {
            case_name: name,
            frame,
            epsilon,
            emit_diagnostics,
        };

        case_stats.compared_frames += 1;
        case_stats.mismatch_count += compare_camera_scalar(
            &compare_context,
            "camera.current.distance",
            actual.distance,
            expected_distance,
            &mut case_stats.max_delta,
        );
        case_stats.mismatch_count += compare_camera_vec3(
            &compare_context,
            "camera.current.position",
            actual.position,
            expected_position,
            &mut case_stats.max_delta,
        );
        case_stats.mismatch_count += compare_camera_vec3(
            &compare_context,
            "camera.current.rotation",
            actual.rotation,
            expected_rotation,
            &mut case_stats.max_delta,
        );
    }
    stats.merge(&case_stats);
    Ok(camera_case_report(
        name,
        "camera-numeric-dump",
        epsilon,
        &case_stats,
    ))
}

#[derive(Clone, Copy, Debug)]
struct MmdCurrentCamera {
    distance: f64,
    position: [f32; 3],
    rotation: [f32; 3],
}

fn mmd_current_camera_from_vmd_camera(camera: mmd_anim_format::VmdCameraState) -> MmdCurrentCamera {
    let rx = camera.rotation[0] as f64;
    let ry = camera.rotation[1] as f64;
    let length = -(camera.distance as f64);
    let x = camera.position[0] as f64 + (-ry).sin() * (-rx).cos() * length;
    let y = camera.position[1] as f64 + rx.sin() * length;
    let z = camera.position[2] as f64 + (-ry).cos() * (-rx).cos() * length;
    MmdCurrentCamera {
        distance: camera.position[2] as f64,
        position: [x as f32, y as f32, z as f32],
        rotation: camera.rotation,
    }
}

fn raw_current_camera_from_vmd_camera(camera: mmd_anim_format::VmdCameraState) -> MmdCurrentCamera {
    MmdCurrentCamera {
        distance: camera.distance as f64,
        position: camera.position,
        rotation: camera.rotation,
    }
}

fn best_mmd_current_camera_candidate(
    camera: mmd_anim_format::VmdCameraState,
    expected_distance: f64,
    expected_position: [f64; 3],
    expected_rotation: [f64; 3],
) -> MmdCurrentCamera {
    let raw = raw_current_camera_from_vmd_camera(camera);
    let transformed = mmd_current_camera_from_vmd_camera(camera);
    if camera_candidate_error(
        transformed,
        expected_distance,
        expected_position,
        expected_rotation,
    ) < camera_candidate_error(raw, expected_distance, expected_position, expected_rotation)
    {
        transformed
    } else {
        raw
    }
}

fn camera_candidate_error(
    candidate: MmdCurrentCamera,
    expected_distance: f64,
    expected_position: [f64; 3],
    expected_rotation: [f64; 3],
) -> f64 {
    let mut error = (candidate.distance - expected_distance).abs();
    for index in 0..3 {
        error += (f64::from(candidate.position[index]) - expected_position[index]).abs();
        error += (f64::from(candidate.rotation[index]) - expected_rotation[index]).abs();
    }
    error
}

#[derive(Default)]
pub(crate) struct MotionNumericCompareStats {
    pub(crate) total_cases: usize,
    pub(crate) compared_cases: usize,
    pub(crate) skipped_unsupported: usize,
    pub(crate) no_targets: usize,
    pub(crate) missing: usize,
    pub(crate) import_errors: usize,
    pub(crate) compared_frames: usize,
    pub(crate) compared_bones: usize,
    pub(crate) mismatch_count: usize,
    pub(crate) skipped_targets: HashSet<String>,
    pub(crate) physics_backend: Option<String>,
    pub(crate) max_abs_error: f32,
    pub(crate) worst: String,
    pub(crate) worst_frame: Option<i32>,
    pub(crate) worst_bone: String,
    pub(crate) worst_component: Option<usize>,
    pub(crate) translation_max_error: f32,
    pub(crate) translation_error_sum_sq: f64,
    pub(crate) translation_error_count: usize,
    pub(crate) worst_translation_frame: Option<i32>,
    pub(crate) worst_translation_bone: String,
    pub(crate) worst_translation_axis: Option<usize>,
    pub(crate) rotation_max_angle_rad: f32,
    pub(crate) rotation_angle_sum_sq: f64,
    pub(crate) rotation_angle_count: usize,
    pub(crate) worst_rotation_frame: Option<i32>,
    pub(crate) worst_rotation_bone: String,
}

pub(crate) fn numeric_compare_failure_count(
    camera_stats: &CameraNumericCompareStats,
    motion_stats: &MotionNumericCompareStats,
) -> usize {
    camera_stats.mismatch_count
        + motion_stats.mismatch_count
        + motion_stats.missing
        + motion_stats.import_errors
        + motion_stats.no_targets
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

    fn translation_rms_error(&self) -> f64 {
        if self.translation_error_count == 0 {
            0.0
        } else {
            (self.translation_error_sum_sq / self.translation_error_count as f64).sqrt()
        }
    }

    fn rotation_rms_angle_rad(&self) -> f64 {
        if self.rotation_angle_count == 0 {
            0.0
        } else {
            (self.rotation_angle_sum_sq / self.rotation_angle_count as f64).sqrt()
        }
    }

    fn merge(&mut self, other: &Self) {
        self.total_cases += other.total_cases;
        self.compared_cases += other.compared_cases;
        self.skipped_unsupported += other.skipped_unsupported;
        self.no_targets += other.no_targets;
        self.missing += other.missing;
        self.import_errors += other.import_errors;
        self.compared_frames += other.compared_frames;
        self.compared_bones += other.compared_bones;
        self.mismatch_count += other.mismatch_count;
        self.skipped_targets
            .extend(other.skipped_targets.iter().cloned());
        self.translation_error_sum_sq += other.translation_error_sum_sq;
        self.translation_error_count += other.translation_error_count;
        self.rotation_angle_sum_sq += other.rotation_angle_sum_sq;
        self.rotation_angle_count += other.rotation_angle_count;
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
        if other.translation_max_error > self.translation_max_error {
            self.translation_max_error = other.translation_max_error;
            self.worst_translation_frame = other.worst_translation_frame;
            self.worst_translation_bone = other.worst_translation_bone.clone();
            self.worst_translation_axis = other.worst_translation_axis;
        }
        if other.rotation_max_angle_rad > self.rotation_max_angle_rad {
            self.rotation_max_angle_rad = other.rotation_max_angle_rad;
            self.worst_rotation_frame = other.worst_rotation_frame;
            self.worst_rotation_bone = other.worst_rotation_bone.clone();
        }
    }
}

fn compare_motion_numeric_case(
    case: &serde_json::Value,
    manifest_dir: &Path,
    defaults: MotionNumericCompareDefaults<'_>,
    stats: &mut MotionNumericCompareStats,
    emit_diagnostics: bool,
) -> Result<NumericCompareCaseReport, Box<dyn std::error::Error>> {
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
        .unwrap_or(defaults.epsilon) as f32;
    let eval_frame_offset = json_f32(case, "/compare/evalFrameOffset")
        .or_else(|| json_f32(case, "/metadata/evalFrameOffset"))
        .unwrap_or(defaults.eval_frame_offset);
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
    let dump = load_motion_numeric_oracle_dump(case, &oracle_path, &model_path, &frames)?;
    let focus_bones = motion_case_focus_bones(case, defaults.focus_bones);
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
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);
    let mut physics_evaluator = build_physics_coarse_evaluator(PhysicsCoarseBuildInput {
        case,
        model_path: &model_path,
        model_bytes: &model_bytes,
        runtime: &mut runtime,
        clip: &clip,
        emit_diagnostics,
    })?;
    case_stats.physics_backend = Some(
        if physics_evaluator.is_some() {
            "bullet-native"
        } else {
            "none"
        }
        .to_owned(),
    );

    for oracle_frame in &dump.frames {
        let eval_frame = oracle_frame.frame as f32 + eval_frame_offset;
        if let Some(physics_evaluator) = physics_evaluator.as_mut() {
            physics_evaluator.evaluate_to_frame(&mut runtime, &clip, eval_frame)?;
        } else {
            runtime.evaluate_clip_frame(&clip, eval_frame);
        }
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
            record_motion_matrix_diagnostics(
                &mut case_stats,
                oracle_frame.frame,
                &oracle_bone.name,
                &runtime_matrix,
                &oracle_bone.world_matrix,
            );
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
    let (status, error) = if case_stats.compared_bones == 0 {
        case_stats.no_targets += 1;
        (
            "no-targets",
            Some("no focused oracle bones matched the selected comparison targets".to_owned()),
        )
    } else if case_stats.mismatch_count == 0 {
        ("ok", None)
    } else {
        ("mismatch", None)
    };
    stats.merge(&case_stats);
    Ok(motion_case_report(
        name,
        case,
        status,
        epsilon,
        &case_stats,
        missing_paths,
        error,
    ))
}

fn load_motion_numeric_oracle_dump(
    case: &serde_json::Value,
    oracle_path: &Path,
    model_path: &Path,
    frames: &[i32],
) -> Result<MmdDumperOracleDump, Box<dyn std::error::Error>> {
    let input = fs::read_to_string(oracle_path)?;
    match case
        .pointer("/oracle/format")
        .and_then(|value| value.as_str())
        .unwrap_or("jsonl")
    {
        "jsonl" | "mmd-dumper-jsonl" => {
            Ok(MmdDumperOracleDump::from_jsonl_str(&input, Some(frames))?)
        }
        "unity-runtime-verification" | "unity-runtime-json" => {
            let unity_case_name = case
                .pointer("/oracle/caseName")
                .or_else(|| case.pointer("/oracle/case"))
                .or_else(|| case.pointer("/metadata/unityCaseName"))
                .and_then(|value| value.as_str());
            let pmx_filter = unity_case_name
                .is_none()
                .then(|| model_path.display().to_string());
            Ok(
                MmdDumperOracleDump::from_unity_runtime_verification_json_str_for_case(
                    &input,
                    Some(frames),
                    unity_case_name,
                    pmx_filter.as_deref(),
                )?,
            )
        }
        format => Err(format!(
            "numeric compare case has unsupported oracle format {format}; supported formats: jsonl, unity-runtime-verification"
        )
        .into()),
    }
}

fn camera_case_report(
    name: &str,
    kind: &str,
    epsilon: f64,
    stats: &CameraNumericCompareStats,
) -> NumericCompareCaseReport {
    let status = if stats.mismatch_count == 0 {
        "ok"
    } else {
        "mismatch"
    };
    NumericCompareCaseReport::Camera(CameraNumericCompareCaseReport {
        core: NumericCompareCaseCore {
            name: name.to_owned(),
            kind: kind.to_owned(),
            status: status.to_owned(),
            epsilon,
            compared_frames: stats.compared_frames,
            compared_bones: 0,
            mismatch_count: stats.mismatch_count,
            max_abs_error: stats.max_delta,
            worst: None,
            worst_frame: None,
            worst_bone: None,
            worst_component: None,
            skipped_targets: stats.skipped_targets_sorted(),
            missing_paths: Vec::new(),
            error: None,
        },
        camera_max_delta: stats.max_delta,
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
) -> NumericCompareCaseReport {
    missing_paths.sort();
    let kind = case
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("motion-numeric");
    NumericCompareCaseReport::Motion(MotionNumericCompareCaseReport {
        core: NumericCompareCaseCore {
            name: name.to_owned(),
            kind: kind.to_owned(),
            status: status.to_owned(),
            epsilon: f64::from(epsilon),
            compared_frames: stats.compared_frames,
            compared_bones: stats.compared_bones,
            mismatch_count: stats.mismatch_count,
            max_abs_error: f64::from(stats.max_abs_error),
            worst: Some(stats.worst.clone()),
            worst_frame: stats.worst_frame,
            worst_bone: non_empty_str(&stats.worst_bone).map(str::to_owned),
            worst_component: stats.worst_component,
            skipped_targets: stats.skipped_targets_sorted(),
            missing_paths,
            error,
        },
        physics_backend: stats.physics_backend.clone(),
        translation_max_error: stats.translation_max_error,
        translation_rms_error: stats.translation_rms_error(),
        worst_translation_frame: stats.worst_translation_frame,
        worst_translation_bone: non_empty_str(&stats.worst_translation_bone).map(str::to_owned),
        worst_translation_axis: stats.worst_translation_axis,
        rotation_max_angle_rad: stats.rotation_max_angle_rad,
        rotation_rms_angle_rad: stats.rotation_rms_angle_rad(),
        worst_rotation_frame: stats.worst_rotation_frame,
        worst_rotation_bone: non_empty_str(&stats.worst_rotation_bone).map(str::to_owned),
        no_targets: stats.no_targets,
        missing: stats.missing,
        import_errors: stats.import_errors,
    })
}

fn record_motion_matrix_diagnostics(
    stats: &mut MotionNumericCompareStats,
    frame: i32,
    bone_name: &str,
    actual: &[f32; 16],
    expected: &[f32; 16],
) {
    for axis in 0..3 {
        let component = 12 + axis;
        let abs_error = (actual[component] - expected[component]).abs();
        stats.translation_error_sum_sq += f64::from(abs_error) * f64::from(abs_error);
        stats.translation_error_count += 1;
        if abs_error > stats.translation_max_error {
            stats.translation_max_error = abs_error;
            stats.worst_translation_frame = Some(frame);
            stats.worst_translation_bone = bone_name.to_owned();
            stats.worst_translation_axis = Some(axis);
        }
    }

    if let Some(angle) = matrix_rotation_angle_error_rad(actual, expected) {
        stats.rotation_angle_sum_sq += f64::from(angle) * f64::from(angle);
        stats.rotation_angle_count += 1;
        if angle > stats.rotation_max_angle_rad {
            stats.rotation_max_angle_rad = angle;
            stats.worst_rotation_frame = Some(frame);
            stats.worst_rotation_bone = bone_name.to_owned();
        }
    }
}

fn matrix_rotation_angle_error_rad(actual: &[f32; 16], expected: &[f32; 16]) -> Option<f32> {
    let actual_x = normalized_matrix_col(actual, 0)?;
    let actual_y = normalized_matrix_col(actual, 4)?;
    let actual_z = normalized_matrix_col(actual, 8)?;
    let expected_x = normalized_matrix_col(expected, 0)?;
    let expected_y = normalized_matrix_col(expected, 4)?;
    let expected_z = normalized_matrix_col(expected, 8)?;
    let trace =
        dot3(expected_x, actual_x) + dot3(expected_y, actual_y) + dot3(expected_z, actual_z);
    let cos_angle = ((trace - 1.0) * 0.5).clamp(-1.0, 1.0);
    Some(cos_angle.acos() as f32)
}

fn normalized_matrix_col(matrix: &[f32; 16], offset: usize) -> Option<[f64; 3]> {
    let x = f64::from(matrix[offset]);
    let y = f64::from(matrix[offset + 1]);
    let z = f64::from(matrix[offset + 2]);
    let length = (x * x + y * y + z * z).sqrt();
    if length <= 1.0e-8 {
        None
    } else {
        Some([x / length, y / length, z / length])
    }
}

fn dot3(lhs: [f64; 3], rhs: [f64; 3]) -> f64 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

#[cfg(feature = "physics-bullet-native")]
struct PhysicsCoarseEvaluator {
    bullet: mmd_anim_physics_bullet::PmxBulletWorld,
    current_frame: f32,
    pin_dynamic_bone_before_step: bool,
}

#[cfg_attr(not(feature = "physics-bullet-native"), allow(dead_code))]
struct PhysicsCoarseBuildInput<'a> {
    case: &'a serde_json::Value,
    model_path: &'a Path,
    model_bytes: &'a [u8],
    runtime: &'a mut RuntimeInstance,
    clip: &'a mmd_anim_runtime::AnimationClip,
    emit_diagnostics: bool,
}

#[cfg(not(feature = "physics-bullet-native"))]
struct PhysicsCoarseEvaluator;

#[cfg(feature = "physics-bullet-native")]
impl PhysicsCoarseEvaluator {
    fn evaluate_to_frame(
        &mut self,
        runtime: &mut RuntimeInstance,
        clip: &mmd_anim_runtime::AnimationClip,
        target_frame: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if target_frame < self.current_frame {
            return Err(format!(
                "physics-coarse frames must be monotonic: current={} target={}",
                self.current_frame, target_frame
            )
            .into());
        }

        while self.current_frame < target_frame {
            let next_frame = (self.current_frame + 1.0).min(target_frame);
            let delta_frames = next_frame - self.current_frame;
            self.current_frame = next_frame;
            runtime.evaluate_clip_frame_before_physics(clip, self.current_frame);
            self.bullet
                .step_runtime_physics_with_runtime_clock_options(
                    runtime,
                    PHYSICS_COARSE_MMD_FRAME_DT * delta_frames,
                    self.pin_dynamic_bone_before_step,
                )?;
        }

        Ok(())
    }
}

#[cfg(feature = "physics-bullet-native")]
fn build_physics_coarse_evaluator(
    input: PhysicsCoarseBuildInput<'_>,
) -> Result<Option<PhysicsCoarseEvaluator>, Box<dyn std::error::Error>> {
    let PhysicsCoarseBuildInput {
        case,
        model_path,
        model_bytes,
        runtime,
        clip,
        emit_diagnostics,
    } = input;
    if case.get("kind").and_then(|value| value.as_str()) != Some("physics-coarse") {
        return Ok(None);
    }
    if !matches!(
        model_path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("pmx")
    ) {
        return Ok(None);
    }

    let parsed = mmd_anim_format::parse_pmx_model(model_bytes)?;
    let mut bullet = build_bullet_world_from_pmx(&parsed)?;
    apply_physics_tick_config(case, runtime);
    runtime.set_physics_mode(PhysicsMode::Live);
    runtime.evaluate_clip_frame_before_physics(clip, 0.0);
    bullet.seed_runtime_physics(runtime)?;
    if emit_diagnostics {
        eprintln!(
            "physics-coarse native bullet case={} rigidBodies={} joints={}",
            case.get("name")
                .and_then(|value| value.as_str())
                .unwrap_or("<unnamed>"),
            bullet.report.rigidbodies_added,
            bullet.report.joints_added
        );
    }
    Ok(Some(PhysicsCoarseEvaluator {
        bullet,
        current_frame: 0.0,
        pin_dynamic_bone_before_step: physics_pin_dynamic_bone_before_step(case),
    }))
}

#[cfg(feature = "physics-bullet-native")]
fn apply_physics_tick_config(case: &serde_json::Value, runtime: &mut RuntimeInstance) {
    let fixed_substep_seconds = json_f32(case, "/compare/physicsTickFixedSubstepSeconds")
        .or_else(|| json_f32(case, "/metadata/physicsTickFixedSubstepSeconds"));
    let max_substeps_per_tick = case
        .pointer("/compare/physicsMaxSubstepsPerTick")
        .or_else(|| case.pointer("/metadata/physicsMaxSubstepsPerTick"))
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok());

    if fixed_substep_seconds.is_none() && max_substeps_per_tick.is_none() {
        return;
    }

    let current = runtime.physics_tick_config();
    runtime.set_physics_tick_config(PhysicsTickConfig {
        fixed_substep_seconds: fixed_substep_seconds.unwrap_or(current.fixed_substep_seconds),
        max_substeps_per_tick: max_substeps_per_tick.unwrap_or(current.max_substeps_per_tick),
    });
}

#[cfg(feature = "physics-bullet-native")]
fn physics_pin_dynamic_bone_before_step(case: &serde_json::Value) -> bool {
    json_bool(case, "/compare/physicsPinDynamicBoneBeforeStep")
        .or_else(|| json_bool(case, "/metadata/physicsPinDynamicBoneBeforeStep"))
        .unwrap_or(false)
}

#[cfg(not(feature = "physics-bullet-native"))]
impl PhysicsCoarseEvaluator {
    fn evaluate_to_frame(
        &mut self,
        _runtime: &mut RuntimeInstance,
        _clip: &mmd_anim_runtime::AnimationClip,
        _target_frame: f32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        unreachable!("physics evaluator is never constructed without physics-bullet-native")
    }
}

#[cfg(not(feature = "physics-bullet-native"))]
fn build_physics_coarse_evaluator(
    _input: PhysicsCoarseBuildInput<'_>,
) -> Result<Option<PhysicsCoarseEvaluator>, Box<dyn std::error::Error>> {
    Ok(None)
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
    let dump = load_motion_numeric_oracle_dump(case, &oracle_path, &model_path, &[target_frame])?;
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
    let mut physics_runtime =
        RuntimeInstance::new_with_counts(Arc::clone(&model), morph_count, solver_count);
    pre_ik.evaluate_clip_frame_without_ik(&clip, eval_frame);
    post_ik.evaluate_clip_frame_with_ik_options(&clip, eval_frame, IkSolveOptions::default());
    let parsed_pmx = if case.get("kind").and_then(|value| value.as_str()) == Some("physics-coarse")
    {
        Some(mmd_anim_format::parse_pmx_model(&model_bytes)?)
    } else {
        None
    };
    let mut physics_evaluator = build_physics_coarse_evaluator(PhysicsCoarseBuildInput {
        case,
        model_path: &model_path,
        model_bytes: &model_bytes,
        runtime: &mut physics_runtime,
        clip: &clip,
        emit_diagnostics: false,
    })?;
    if let Some(physics_evaluator) = physics_evaluator.as_mut() {
        physics_evaluator.evaluate_to_frame(&mut physics_runtime, &clip, eval_frame)?;
    }

    println!(
        "numeric bone diagnosis case={} oracleFrame={:.3} evalFrame={:.3} physicsBackend={} model={} motion={} oracle={}",
        case_name,
        oracle_frame_number,
        eval_frame,
        if physics_evaluator.is_some() {
            "bullet-native"
        } else {
            "none"
        },
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
        let physics = physics_evaluator
            .as_ref()
            .map(|_| physics_runtime.world_matrices()[index.as_usize()].to_cols_array());
        let (pre_component, pre_delta) = max_matrix_delta(&pre, &oracle_bone.world_matrix);
        let (post_component, post_delta) = max_matrix_delta(&post, &oracle_bone.world_matrix);
        let physics_delta =
            physics.map(|physics| max_matrix_delta(&physics, &oracle_bone.world_matrix));
        let pre_pos_delta = position_delta(&pre, &oracle_bone.world_matrix);
        let post_pos_delta = position_delta(&post, &oracle_bone.world_matrix);
        let physics_pos_delta =
            physics.map(|physics| position_delta(&physics, &oracle_bone.world_matrix));
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
        if let (Some(physics), Some((physics_component, physics_delta)), Some(physics_pos_delta)) =
            (physics, physics_delta, physics_pos_delta)
        {
            println!(
                "physicsBone={} index={} physicsMaxDelta={:.9}@{} physicsPosDelta=({:.6},{:.6},{:.6}) physicsPos=({:.6},{:.6},{:.6}) oraclePos=({:.6},{:.6},{:.6})",
                bone_name,
                index.as_usize(),
                physics_delta,
                physics_component,
                physics_pos_delta[0],
                physics_pos_delta[1],
                physics_pos_delta[2],
                physics[12],
                physics[13],
                physics[14],
                oracle_bone.world_matrix[12],
                oracle_bone.world_matrix[13],
                oracle_bone.world_matrix[14],
            );
        }
        if let (Some(parsed_pmx), Some(physics_evaluator)) =
            (parsed_pmx.as_ref(), physics_evaluator.as_ref())
        {
            print_physics_rigidbody_diagnosis(parsed_pmx, physics_evaluator, index.as_usize())?;
        }
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicsPenetrationPair {
    lhs_index: usize,
    lhs_name: String,
    lhs_mode: String,
    rhs_index: usize,
    rhs_name: String,
    rhs_mode: String,
    center_distance: f32,
    lhs_radius: f32,
    rhs_radius: f32,
    approx_gap: f32,
    metric: &'static str,
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicsContactDiagnostic {
    lhs_index: usize,
    lhs_name: String,
    lhs_mode: String,
    rhs_index: usize,
    rhs_name: String,
    rhs_mode: String,
    distance: f32,
    position_world_on_a: [f32; 3],
    position_world_on_b: [f32; 3],
    normal_world_on_b: [f32; 3],
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicsPenetrationDiagnosticReport {
    case_name: String,
    oracle_frame: f32,
    eval_frame: f32,
    model: String,
    motion: String,
    metric: &'static str,
    summary: PhysicsPenetrationDiagnosticSummary,
    pairs: Vec<PhysicsPenetrationPair>,
    contacts: Vec<PhysicsContactDiagnostic>,
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PhysicsPenetrationDiagnosticSummary {
    pair_count: usize,
    penetrating_pair_count: usize,
    severe_pair_count: usize,
    bullet_contact_count: usize,
    penetrating_contact_count: usize,
    min_signed_distance: Option<f32>,
    max_penetration_depth: f32,
    min_bullet_contact_distance: Option<f32>,
    max_bullet_penetration_depth: f32,
}

#[cfg(feature = "physics-bullet-native")]
pub(crate) fn diagnose_numeric_physics_penetration(
    manifest_path: &Path,
    case_name: &str,
    oracle_frame_number: f32,
    eval_frame: f32,
    focus_bone_names: &[String],
    use_json: bool,
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
    if case.get("kind").and_then(|value| value.as_str()) != Some("physics-coarse") {
        return Err("physics penetration diagnosis requires a physics-coarse case".into());
    }

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
    let model_bytes = fs::read(&model_path)?;
    let parsed_pmx = mmd_anim_format::parse_pmx_model(&model_bytes)?;
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
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);
    let mut physics_evaluator = build_physics_coarse_evaluator(PhysicsCoarseBuildInput {
        case,
        model_path: &model_path,
        model_bytes: &model_bytes,
        runtime: &mut runtime,
        clip: &clip,
        emit_diagnostics: false,
    })?
    .ok_or("physics-coarse case did not construct a Bullet evaluator")?;
    physics_evaluator.evaluate_to_frame(&mut runtime, &clip, eval_frame)?;

    let focus_bones =
        resolve_focus_bone_indices(&model_import.bone_name_to_index, focus_bone_names);
    let pairs =
        collect_approx_penetration_pairs(&parsed_pmx, &physics_evaluator, focus_bones.as_ref())?;
    let contacts = collect_bullet_contacts(&parsed_pmx, &physics_evaluator, focus_bones.as_ref())?;
    let severe_count = pairs.iter().filter(|pair| pair.approx_gap < -0.05).count();
    let penetrating_count = pairs.iter().filter(|pair| pair.approx_gap < 0.0).count();
    let penetrating_contacts = contacts
        .iter()
        .filter(|contact| contact.distance < 0.0)
        .count();
    let min_signed_distance = pairs.first().map(|pair| pair.approx_gap);
    let max_penetration_depth = pairs
        .iter()
        .filter_map(|pair| (pair.approx_gap < 0.0).then_some(-pair.approx_gap))
        .fold(0.0f32, f32::max);
    let min_bullet_contact_distance = contacts.first().map(|contact| contact.distance);
    let max_bullet_penetration_depth = contacts
        .iter()
        .filter_map(|contact| (contact.distance < 0.0).then_some(-contact.distance))
        .fold(0.0f32, f32::max);

    if use_json {
        let report = PhysicsPenetrationDiagnosticReport {
            case_name: case_name.to_owned(),
            oracle_frame: oracle_frame_number,
            eval_frame,
            model: model_path.display().to_string(),
            motion: motion_path.display().to_string(),
            metric: "shape-proxy+bullet-contacts",
            summary: PhysicsPenetrationDiagnosticSummary {
                pair_count: pairs.len(),
                penetrating_pair_count: penetrating_count,
                severe_pair_count: severe_count,
                bullet_contact_count: contacts.len(),
                penetrating_contact_count: penetrating_contacts,
                min_signed_distance,
                max_penetration_depth,
                min_bullet_contact_distance,
                max_bullet_penetration_depth,
            },
            pairs,
            contacts,
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!(
        "physics penetration diagnosis case={} oracleFrame={:.3} evalFrame={:.3} model={} motion={} pairs={} penetratingPairs={} severePairs={} bulletContacts={} penetratingContacts={} metric=shape-proxy+bullet-contacts",
        case_name,
        oracle_frame_number,
        eval_frame,
        model_path.display(),
        motion_path.display(),
        pairs.len(),
        penetrating_count,
        severe_count,
        contacts.len(),
        penetrating_contacts
    );
    for pair in pairs.iter().take(30) {
        println!(
            "pair lhs={}:{}:{} rhs={}:{}:{} metric={} distance={:.6} lhsRadius={:.6} rhsRadius={:.6} approxGap={:.6}",
            pair.lhs_index,
            pair.lhs_name,
            pair.lhs_mode,
            pair.rhs_index,
            pair.rhs_name,
            pair.rhs_mode,
            pair.metric,
            pair.center_distance,
            pair.lhs_radius,
            pair.rhs_radius,
            pair.approx_gap,
        );
    }
    for contact in contacts.iter().take(30) {
        println!(
            "contact lhs={}:{}:{} rhs={}:{}:{} distance={:.6} posA=({:.6},{:.6},{:.6}) posB=({:.6},{:.6},{:.6}) normalB=({:.6},{:.6},{:.6})",
            contact.lhs_index,
            contact.lhs_name,
            contact.lhs_mode,
            contact.rhs_index,
            contact.rhs_name,
            contact.rhs_mode,
            contact.distance,
            contact.position_world_on_a[0],
            contact.position_world_on_a[1],
            contact.position_world_on_a[2],
            contact.position_world_on_b[0],
            contact.position_world_on_b[1],
            contact.position_world_on_b[2],
            contact.normal_world_on_b[0],
            contact.normal_world_on_b[1],
            contact.normal_world_on_b[2],
        );
    }

    Ok(ExitCode::SUCCESS)
}

#[cfg(not(feature = "physics-bullet-native"))]
pub(crate) fn diagnose_numeric_physics_penetration(
    _manifest_path: &Path,
    _case_name: &str,
    _oracle_frame_number: f32,
    _eval_frame: f32,
    _focus_bone_names: &[String],
    _use_json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    Err("physics penetration diagnosis requires the physics-bullet-native feature".into())
}

#[cfg(feature = "physics-bullet-native")]
fn resolve_focus_bone_indices(
    bone_name_to_index: &HashMap<Vec<u8>, BoneIndex>,
    focus_bone_names: &[String],
) -> Option<HashSet<usize>> {
    if focus_bone_names.is_empty() {
        return None;
    }
    let mut indices = HashSet::new();
    for bone_name in focus_bone_names {
        let normalized = mmd_anim_format::normalize_vmd_name(bone_name.as_bytes());
        if let Some(index) = bone_name_to_index
            .get(bone_name.as_bytes())
            .or_else(|| bone_name_to_index.get(&normalized))
            .copied()
        {
            indices.insert(index.as_usize());
        }
    }
    Some(indices)
}

#[cfg(feature = "physics-bullet-native")]
fn collect_approx_penetration_pairs(
    parsed_pmx: &mmd_anim_format::PmxParsedModel,
    physics_evaluator: &PhysicsCoarseEvaluator,
    focus_bones: Option<&HashSet<usize>>,
) -> Result<Vec<PhysicsPenetrationPair>, Box<dyn std::error::Error>> {
    let mut transforms = Vec::with_capacity(physics_evaluator.bullet.rigidbody_handles.len());
    for handle in physics_evaluator.bullet.rigidbody_handles.iter().copied() {
        transforms.push(physics_evaluator.bullet.world.rigidbody_transform(handle)?);
    }

    let mut pairs = Vec::new();
    for lhs_index in 0..parsed_pmx.rigid_bodies.len() {
        for rhs_index in lhs_index + 1..parsed_pmx.rigid_bodies.len() {
            let lhs_body = &parsed_pmx.rigid_bodies[lhs_index];
            let rhs_body = &parsed_pmx.rigid_bodies[rhs_index];
            if let Some(focus_bones) = focus_bones {
                let lhs_focused = rigidbody_bone_index(lhs_body)
                    .is_some_and(|index| focus_bones.contains(&index));
                let rhs_focused = rigidbody_bone_index(rhs_body)
                    .is_some_and(|index| focus_bones.contains(&index));
                if !lhs_focused && !rhs_focused {
                    continue;
                }
            }
            if !rigidbody_collision_allowed(lhs_body, rhs_body) {
                continue;
            }
            let lhs_proxy = rigidbody_collision_proxy(lhs_body, transforms[lhs_index]);
            let rhs_proxy = rigidbody_collision_proxy(rhs_body, transforms[rhs_index]);
            let proxy_gap = collision_proxy_gap(&lhs_proxy, &rhs_proxy);
            let center_distance = proxy_gap.distance;
            let lhs_radius = lhs_proxy.radius();
            let rhs_radius = rhs_proxy.radius();
            let approx_gap = proxy_gap.gap;
            if approx_gap > 2.0 {
                continue;
            }
            pairs.push(PhysicsPenetrationPair {
                lhs_index,
                lhs_name: lhs_body.name.clone(),
                lhs_mode: lhs_body.mode.clone(),
                rhs_index,
                rhs_name: rhs_body.name.clone(),
                rhs_mode: rhs_body.mode.clone(),
                center_distance,
                lhs_radius,
                rhs_radius,
                approx_gap,
                metric: proxy_gap.metric,
            });
        }
    }
    pairs.sort_by(|lhs, rhs| lhs.approx_gap.total_cmp(&rhs.approx_gap));
    Ok(pairs)
}

#[cfg(feature = "physics-bullet-native")]
fn collect_bullet_contacts(
    parsed_pmx: &mmd_anim_format::PmxParsedModel,
    physics_evaluator: &PhysicsCoarseEvaluator,
    focus_bones: Option<&HashSet<usize>>,
) -> Result<Vec<PhysicsContactDiagnostic>, Box<dyn std::error::Error>> {
    let mut diagnostics = Vec::new();
    for contact in physics_evaluator.bullet.world.contact_points()? {
        let lhs_index = contact.rigidbody_a.index();
        let rhs_index = contact.rigidbody_b.index();
        if lhs_index < 0 || rhs_index < 0 {
            continue;
        }
        let lhs_index = lhs_index as usize;
        let rhs_index = rhs_index as usize;
        let (Some(lhs_body), Some(rhs_body)) = (
            parsed_pmx.rigid_bodies.get(lhs_index),
            parsed_pmx.rigid_bodies.get(rhs_index),
        ) else {
            continue;
        };
        if let Some(focus_bones) = focus_bones {
            let lhs_focused =
                rigidbody_bone_index(lhs_body).is_some_and(|index| focus_bones.contains(&index));
            let rhs_focused =
                rigidbody_bone_index(rhs_body).is_some_and(|index| focus_bones.contains(&index));
            if !lhs_focused && !rhs_focused {
                continue;
            }
        }
        diagnostics.push(PhysicsContactDiagnostic {
            lhs_index,
            lhs_name: lhs_body.name.clone(),
            lhs_mode: lhs_body.mode.clone(),
            rhs_index,
            rhs_name: rhs_body.name.clone(),
            rhs_mode: rhs_body.mode.clone(),
            distance: contact.distance,
            position_world_on_a: contact.position_world_on_a,
            position_world_on_b: contact.position_world_on_b,
            normal_world_on_b: contact.normal_world_on_b,
        });
    }
    diagnostics.sort_by(|lhs, rhs| lhs.distance.total_cmp(&rhs.distance));
    Ok(diagnostics)
}

#[cfg(feature = "physics-bullet-native")]
fn rigidbody_bone_index(body: &mmd_anim_format::pmx::PmxParsedRigidBody) -> Option<usize> {
    (body.bone_index >= 0).then_some(body.bone_index as usize)
}

#[cfg(feature = "physics-bullet-native")]
fn rigidbody_collision_allowed(
    lhs: &mmd_anim_format::pmx::PmxParsedRigidBody,
    rhs: &mmd_anim_format::pmx::PmxParsedRigidBody,
) -> bool {
    let lhs_group = rigidbody_group_bit(lhs.group);
    let rhs_group = rigidbody_group_bit(rhs.group);
    lhs.mask & rhs_group == 0 && rhs.mask & lhs_group == 0
}

#[cfg(feature = "physics-bullet-native")]
fn rigidbody_group_bit(group: u8) -> u16 {
    1u16 << group.min(15)
}

#[cfg(feature = "physics-bullet-native")]
fn rigidbody_bounding_radius(body: &mmd_anim_format::pmx::PmxParsedRigidBody) -> f32 {
    match body.shape.as_str() {
        "sphere" => body.size[0].abs(),
        "capsule" => body.size[0].abs() + body.size[1].abs() * 0.5,
        "box" => glam::Vec3::from_array(body.size).abs().length(),
        _ => glam::Vec3::from_array(body.size).abs().max_element(),
    }
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Clone, Copy, Debug)]
enum CollisionProxy {
    Sphere {
        center: glam::Vec3,
        radius: f32,
    },
    Box {
        center: glam::Vec3,
        axes: [glam::Vec3; 3],
        half_extents: glam::Vec3,
    },
    Capsule {
        a: glam::Vec3,
        b: glam::Vec3,
        radius: f32,
    },
    BoundingSphere {
        center: glam::Vec3,
        radius: f32,
    },
}

#[cfg(feature = "physics-bullet-native")]
impl CollisionProxy {
    fn center(self) -> glam::Vec3 {
        match self {
            Self::Sphere { center, .. } | Self::BoundingSphere { center, .. } => center,
            Self::Box { center, .. } => center,
            Self::Capsule { a, b, .. } => (a + b) * 0.5,
        }
    }

    fn radius(self) -> f32 {
        match self {
            Self::Sphere { radius, .. }
            | Self::Capsule { radius, .. }
            | Self::BoundingSphere { radius, .. } => radius,
            Self::Box { half_extents, .. } => half_extents.length(),
        }
    }
}

#[cfg(feature = "physics-bullet-native")]
#[derive(Clone, Copy, Debug)]
struct CollisionProxyGap {
    distance: f32,
    gap: f32,
    metric: &'static str,
}

#[cfg(feature = "physics-bullet-native")]
fn rigidbody_collision_proxy(
    body: &mmd_anim_format::pmx::PmxParsedRigidBody,
    transform: mmd_anim_physics_bullet::Transform,
) -> CollisionProxy {
    let center = glam::Vec3::from_array(transform.position);
    let rotation = glam::Quat::from_array(transform.rotation_xyzw).normalize();
    match body.shape.as_str() {
        "sphere" => CollisionProxy::Sphere {
            center,
            radius: body.size[0].abs(),
        },
        "capsule" => {
            let radius = body.size[0].abs();
            let half_height = body.size[1].abs() * 0.5;
            let axis = rotation * (glam::Vec3::Y * half_height);
            CollisionProxy::Capsule {
                a: center - axis,
                b: center + axis,
                radius,
            }
        }
        "box" => CollisionProxy::Box {
            center,
            axes: [
                rotation * glam::Vec3::X,
                rotation * glam::Vec3::Y,
                rotation * glam::Vec3::Z,
            ],
            half_extents: glam::Vec3::from_array(body.size).abs(),
        },
        _ => CollisionProxy::BoundingSphere {
            center,
            radius: rigidbody_bounding_radius(body),
        },
    }
}

#[cfg(feature = "physics-bullet-native")]
fn collision_proxy_gap(lhs: &CollisionProxy, rhs: &CollisionProxy) -> CollisionProxyGap {
    match (*lhs, *rhs) {
        (
            CollisionProxy::Sphere {
                center: lhs_center,
                radius: lhs_radius,
            },
            CollisionProxy::Sphere {
                center: rhs_center,
                radius: rhs_radius,
            },
        ) => {
            let distance = lhs_center.distance(rhs_center);
            CollisionProxyGap {
                distance,
                gap: distance - lhs_radius - rhs_radius,
                metric: "sphere-sphere",
            }
        }
        (
            CollisionProxy::Sphere {
                center,
                radius: sphere_radius,
            },
            CollisionProxy::Capsule {
                a,
                b,
                radius: capsule_radius,
            },
        )
        | (
            CollisionProxy::Capsule {
                a,
                b,
                radius: capsule_radius,
            },
            CollisionProxy::Sphere {
                center,
                radius: sphere_radius,
            },
        ) => {
            let distance = point_segment_distance(center, a, b);
            CollisionProxyGap {
                distance,
                gap: distance - sphere_radius - capsule_radius,
                metric: "sphere-capsule",
            }
        }
        (
            CollisionProxy::Capsule {
                a: lhs_a,
                b: lhs_b,
                radius: lhs_radius,
            },
            CollisionProxy::Capsule {
                a: rhs_a,
                b: rhs_b,
                radius: rhs_radius,
            },
        ) => {
            let distance = segment_segment_distance(lhs_a, lhs_b, rhs_a, rhs_b);
            CollisionProxyGap {
                distance,
                gap: distance - lhs_radius - rhs_radius,
                metric: "capsule-capsule",
            }
        }
        (
            CollisionProxy::Box {
                center: lhs_center,
                axes: lhs_axes,
                half_extents: lhs_half_extents,
            },
            CollisionProxy::Box {
                center: rhs_center,
                axes: rhs_axes,
                half_extents: rhs_half_extents,
            },
        ) => CollisionProxyGap {
            distance: lhs_center.distance(rhs_center),
            gap: obb_obb_sat_gap(
                lhs_center,
                lhs_axes,
                lhs_half_extents,
                rhs_center,
                rhs_axes,
                rhs_half_extents,
            ),
            metric: "box-box-sat",
        },
        _ => {
            let distance = lhs.center().distance(rhs.center());
            CollisionProxyGap {
                distance,
                gap: distance - lhs.radius() - rhs.radius(),
                metric: "bounding-sphere",
            }
        }
    }
}

#[cfg(feature = "physics-bullet-native")]
fn obb_obb_sat_gap(
    lhs_center: glam::Vec3,
    lhs_axes: [glam::Vec3; 3],
    lhs_half_extents: glam::Vec3,
    rhs_center: glam::Vec3,
    rhs_axes: [glam::Vec3; 3],
    rhs_half_extents: glam::Vec3,
) -> f32 {
    let mut best_gap = f32::NEG_INFINITY;
    for axis in lhs_axes.iter().chain(rhs_axes.iter()).copied() {
        best_gap = best_gap.max(obb_axis_gap(
            axis,
            lhs_center,
            lhs_axes,
            lhs_half_extents,
            rhs_center,
            rhs_axes,
            rhs_half_extents,
        ));
    }
    for lhs_axis in lhs_axes {
        for rhs_axis in rhs_axes {
            let axis = lhs_axis.cross(rhs_axis);
            if axis.length_squared() > 1.0e-8 {
                best_gap = best_gap.max(obb_axis_gap(
                    axis,
                    lhs_center,
                    lhs_axes,
                    lhs_half_extents,
                    rhs_center,
                    rhs_axes,
                    rhs_half_extents,
                ));
            }
        }
    }
    best_gap
}

#[cfg(feature = "physics-bullet-native")]
fn obb_axis_gap(
    axis: glam::Vec3,
    lhs_center: glam::Vec3,
    lhs_axes: [glam::Vec3; 3],
    lhs_half_extents: glam::Vec3,
    rhs_center: glam::Vec3,
    rhs_axes: [glam::Vec3; 3],
    rhs_half_extents: glam::Vec3,
) -> f32 {
    let axis = axis.normalize();
    let center_distance = (rhs_center - lhs_center).dot(axis).abs();
    let lhs_radius = obb_projection_radius(axis, lhs_axes, lhs_half_extents);
    let rhs_radius = obb_projection_radius(axis, rhs_axes, rhs_half_extents);
    center_distance - lhs_radius - rhs_radius
}

#[cfg(feature = "physics-bullet-native")]
fn obb_projection_radius(
    axis: glam::Vec3,
    box_axes: [glam::Vec3; 3],
    half_extents: glam::Vec3,
) -> f32 {
    half_extents.x * axis.dot(box_axes[0]).abs()
        + half_extents.y * axis.dot(box_axes[1]).abs()
        + half_extents.z * axis.dot(box_axes[2]).abs()
}

#[cfg(feature = "physics-bullet-native")]
fn point_segment_distance(point: glam::Vec3, a: glam::Vec3, b: glam::Vec3) -> f32 {
    let ab = b - a;
    let denom = ab.length_squared();
    if denom <= f32::EPSILON {
        return point.distance(a);
    }
    let t = ((point - a).dot(ab) / denom).clamp(0.0, 1.0);
    point.distance(a + ab * t)
}

#[cfg(feature = "physics-bullet-native")]
fn segment_segment_distance(p1: glam::Vec3, q1: glam::Vec3, p2: glam::Vec3, q2: glam::Vec3) -> f32 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);

    let (s, t) = if a <= f32::EPSILON && e <= f32::EPSILON {
        (0.0, 0.0)
    } else if a <= f32::EPSILON {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= f32::EPSILON {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom.abs() > f32::EPSILON {
                ((b * f - c * e) / denom).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let tnom = b * s + f;
            let t = if tnom < 0.0 {
                s = (-c / a).clamp(0.0, 1.0);
                0.0
            } else if tnom > e {
                s = ((b - c) / a).clamp(0.0, 1.0);
                1.0
            } else {
                tnom / e
            };
            (s, t)
        }
    };

    let c1 = p1 + d1 * s;
    let c2 = p2 + d2 * t;
    c1.distance(c2)
}

#[cfg(all(test, feature = "physics-bullet-native"))]
mod physics_penetration_geometry_tests {
    use super::*;

    fn rigidbody_with_collision(group: u8, mask: u16) -> mmd_anim_format::pmx::PmxParsedRigidBody {
        mmd_anim_format::pmx::PmxParsedRigidBody {
            name: String::new(),
            english_name: String::new(),
            bone_index: -1,
            group,
            mask,
            shape: "sphere".to_owned(),
            size: [1.0, 0.0, 0.0],
            position: [0.0; 3],
            rotation: [0.0; 3],
            mass: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            restitution: 0.0,
            friction: 0.5,
            mode: "dynamic".to_owned(),
        }
    }

    #[test]
    fn rigidbody_collision_allowed_treats_pmx_mask_as_non_collision_groups() {
        let lhs = rigidbody_with_collision(1, 0);
        let rhs = rigidbody_with_collision(2, 0);
        assert!(rigidbody_collision_allowed(&lhs, &rhs));

        let lhs_blocks_rhs = rigidbody_with_collision(1, 1 << 2);
        assert!(!rigidbody_collision_allowed(&lhs_blocks_rhs, &rhs));

        let rhs_blocks_lhs = rigidbody_with_collision(2, 1 << 1);
        assert!(!rigidbody_collision_allowed(&lhs, &rhs_blocks_lhs));
    }

    #[test]
    fn physics_penetration_json_report_uses_stable_camel_case_fields() {
        let report = PhysicsPenetrationDiagnosticReport {
            case_name: "case-a".to_owned(),
            oracle_frame: 119.0,
            eval_frame: 119.0,
            model: "model.pmx".to_owned(),
            motion: "motion.vmd".to_owned(),
            metric: "shape-proxy+bullet-contacts",
            summary: PhysicsPenetrationDiagnosticSummary {
                pair_count: 1,
                penetrating_pair_count: 0,
                severe_pair_count: 0,
                bullet_contact_count: 1,
                penetrating_contact_count: 0,
                min_signed_distance: Some(0.25),
                max_penetration_depth: 0.0,
                min_bullet_contact_distance: Some(0.125),
                max_bullet_penetration_depth: 0.0,
            },
            pairs: vec![PhysicsPenetrationPair {
                lhs_index: 1,
                lhs_name: "lhs".to_owned(),
                lhs_mode: "dynamic".to_owned(),
                rhs_index: 2,
                rhs_name: "rhs".to_owned(),
                rhs_mode: "static".to_owned(),
                center_distance: 3.0,
                lhs_radius: 1.0,
                rhs_radius: 1.0,
                approx_gap: 1.0,
                metric: "sphere-sphere",
            }],
            contacts: vec![PhysicsContactDiagnostic {
                lhs_index: 1,
                lhs_name: "lhs".to_owned(),
                lhs_mode: "dynamic".to_owned(),
                rhs_index: 2,
                rhs_name: "rhs".to_owned(),
                rhs_mode: "static".to_owned(),
                distance: 0.125,
                position_world_on_a: [1.0, 2.0, 3.0],
                position_world_on_b: [4.0, 5.0, 6.0],
                normal_world_on_b: [0.0, 1.0, 0.0],
            }],
        };

        let value = serde_json::to_value(report).unwrap();

        assert_eq!(value["caseName"], "case-a");
        assert_eq!(value["oracleFrame"], 119.0);
        assert_eq!(value["summary"]["maxPenetrationDepth"], 0.0);
        assert_eq!(value["summary"]["minBulletContactDistance"], 0.125);
        assert_eq!(value["pairs"][0]["approxGap"], 1.0);
        assert_eq!(
            value["contacts"][0]["positionWorldOnA"],
            serde_json::json!([1.0, 2.0, 3.0])
        );
        assert!(value.get("case_name").is_none());
        assert!(value["summary"].get("max_penetration_depth").is_none());
    }

    #[test]
    fn obb_obb_sat_gap_reports_positive_separation() {
        let axes = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z];
        let gap = obb_obb_sat_gap(
            glam::Vec3::ZERO,
            axes,
            glam::Vec3::splat(1.0),
            glam::Vec3::new(3.0, 0.0, 0.0),
            axes,
            glam::Vec3::splat(1.0),
        );

        assert!((gap - 1.0).abs() < 1.0e-6, "gap={gap}");
    }

    #[test]
    fn obb_obb_sat_gap_reports_negative_overlap() {
        let axes = [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z];
        let gap = obb_obb_sat_gap(
            glam::Vec3::ZERO,
            axes,
            glam::Vec3::splat(1.0),
            glam::Vec3::new(1.5, 0.0, 0.0),
            axes,
            glam::Vec3::splat(1.0),
        );

        assert!((gap + 0.5).abs() < 1.0e-6, "gap={gap}");
    }
}

#[cfg(feature = "physics-bullet-native")]
fn print_physics_rigidbody_diagnosis(
    parsed_pmx: &mmd_anim_format::PmxParsedModel,
    physics_evaluator: &PhysicsCoarseEvaluator,
    bone_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    for (body_index, ((handle, binding), body)) in physics_evaluator
        .bullet
        .rigidbody_handles
        .iter()
        .copied()
        .zip(physics_evaluator.bullet.rigidbody_bindings.iter())
        .zip(parsed_pmx.rigid_bodies.iter())
        .enumerate()
    {
        if binding.bone_index != Some(bone_index) {
            continue;
        }
        let body_world = physics_evaluator.bullet.world.rigidbody_transform(handle)?;
        println!(
            "rigidBody={} name={} mode={:?} shape={} group={} mask={} bodyWorldPos=({:.6},{:.6},{:.6}) bodyWorldRot=({:.6},{:.6},{:.6},{:.6}) bodyFromBonePos=({:.6},{:.6},{:.6}) boneFromBodyPos=({:.6},{:.6},{:.6})",
            body_index,
            body.name,
            binding.mode,
            body.shape,
            body.group,
            body.mask,
            body_world.position[0],
            body_world.position[1],
            body_world.position[2],
            body_world.rotation_xyzw[0],
            body_world.rotation_xyzw[1],
            body_world.rotation_xyzw[2],
            body_world.rotation_xyzw[3],
            binding.body_from_bone.position[0],
            binding.body_from_bone.position[1],
            binding.body_from_bone.position[2],
            binding.bone_from_body.position[0],
            binding.bone_from_body.position[1],
            binding.bone_from_body.position[2],
        );
    }
    Ok(())
}

#[cfg(not(feature = "physics-bullet-native"))]
fn print_physics_rigidbody_diagnosis(
    _parsed_pmx: &mmd_anim_format::PmxParsedModel,
    _physics_evaluator: &PhysicsCoarseEvaluator,
    _bone_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

#[derive(Debug)]
pub(crate) struct DiagnoseNumericBoneOptions {
    pub(crate) eval_frame: f32,
    pub(crate) bone_names: Vec<String>,
}

pub(crate) fn parse_diagnose_numeric_bone_rest(
    rest: Vec<String>,
    default_eval_frame: f32,
) -> Result<DiagnoseNumericBoneOptions, String> {
    let mut eval_frame = default_eval_frame;
    let mut bone_names = Vec::new();
    let mut iter = rest.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--eval-frame" {
            let Some(value) = iter.next() else {
                return Err("missing value for --eval-frame".to_owned());
            };
            eval_frame = value
                .parse()
                .map_err(|_| format!("invalid --eval-frame value: {value}"))?;
        } else if arg.starts_with("--") {
            return Err(format!("unknown flag: {arg}"));
        } else {
            bone_names.push(arg);
        }
    }
    Ok(DiagnoseNumericBoneOptions {
        eval_frame,
        bone_names,
    })
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

#[cfg(feature = "physics-bullet-native")]
pub(crate) fn json_bool(value: &serde_json::Value, pointer: &str) -> Option<bool> {
    value.pointer(pointer)?.as_bool()
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

fn collect_unsupported_camera_targets(
    case: &serde_json::Value,
    skipped_targets: &mut HashSet<String>,
) {
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
        if !matches!(
            target,
            "camera.current"
                | "camera.current.distance"
                | "camera.current.position"
                | "camera.current.rotation"
        ) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_f32_near(actual: f32, expected: f32) {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1.0e-6,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    fn assert_f64_near(actual: f64, expected: f64) {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1.0e-12,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    #[test]
    fn motion_matrix_diagnostics_use_column_major_translation_components() {
        let mut actual =
            glam::Mat4::from_translation(glam::Vec3::new(1.0, 2.0, 3.0)).to_cols_array();
        let mut expected =
            glam::Mat4::from_translation(glam::Vec3::new(4.0, 6.0, 8.0)).to_cols_array();

        // These are row-major translation slots. Keeping them equal proves the
        // diagnostic path is not accidentally reading Unity's raw row-major layout.
        actual[3] = 77.0;
        actual[7] = 88.0;
        actual[11] = 99.0;
        expected[3] = 77.0;
        expected[7] = 88.0;
        expected[11] = 99.0;

        let mut stats = MotionNumericCompareStats::default();
        record_motion_matrix_diagnostics(&mut stats, 12, "probe", &actual, &expected);

        assert_eq!(stats.translation_error_count, 3);
        assert_f32_near(stats.translation_max_error, 5.0);
        assert_eq!(stats.worst_translation_frame, Some(12));
        assert_eq!(stats.worst_translation_bone, "probe");
        assert_eq!(stats.worst_translation_axis, Some(2));
        assert_f64_near(stats.translation_rms_error(), (50.0f64 / 3.0).sqrt());
    }

    #[test]
    fn position_delta_uses_column_major_translation_components() {
        let mut actual =
            glam::Mat4::from_translation(glam::Vec3::new(-1.0, 2.5, 7.0)).to_cols_array();
        let mut expected =
            glam::Mat4::from_translation(glam::Vec3::new(3.0, 0.5, -2.0)).to_cols_array();

        actual[3] = 10.0;
        actual[7] = 20.0;
        actual[11] = 30.0;
        expected[3] = 10.0;
        expected[7] = 20.0;
        expected[11] = 30.0;

        assert_eq!(position_delta(&actual, &expected), [-4.0, 2.0, 9.0]);
    }

    #[cfg(feature = "physics-bullet-native")]
    #[test]
    fn physics_compare_defaults_to_static_only_dynamic_bone_feed() {
        let case = serde_json::json!({
            "kind": "physics-coarse"
        });

        assert!(!physics_pin_dynamic_bone_before_step(&case));
    }

    #[cfg(feature = "physics-bullet-native")]
    #[test]
    fn physics_compare_can_opt_into_dynamic_bone_pin_before_step() {
        let metadata_case = serde_json::json!({
            "kind": "physics-coarse",
            "metadata": {
                "physicsPinDynamicBoneBeforeStep": true
            }
        });
        assert!(physics_pin_dynamic_bone_before_step(&metadata_case));

        let compare_overrides_metadata = serde_json::json!({
            "kind": "physics-coarse",
            "metadata": {
                "physicsPinDynamicBoneBeforeStep": true
            },
            "compare": {
                "physicsPinDynamicBoneBeforeStep": false
            }
        });
        assert!(!physics_pin_dynamic_bone_before_step(
            &compare_overrides_metadata
        ));
    }

    #[cfg(feature = "physics-bullet-native")]
    fn minimal_runtime_for_physics_tick_config() -> RuntimeInstance {
        let model = Arc::new(
            ModelArena::new(vec![mmd_anim_runtime::BoneInit::new(
                None,
                glam::Vec3A::ZERO,
            )])
            .unwrap(),
        );
        RuntimeInstance::new(model)
    }

    #[cfg(feature = "physics-bullet-native")]
    #[test]
    fn physics_tick_config_manifest_defaults_leave_runtime_unchanged() {
        let mut runtime = minimal_runtime_for_physics_tick_config();
        let before = runtime.physics_tick_config();
        let case = serde_json::json!({
            "kind": "physics-coarse"
        });

        apply_physics_tick_config(&case, &mut runtime);

        assert_eq!(runtime.physics_tick_config(), before);
    }

    #[cfg(feature = "physics-bullet-native")]
    #[test]
    fn physics_tick_config_compare_values_override_metadata_values() {
        let mut runtime = minimal_runtime_for_physics_tick_config();
        let case = serde_json::json!({
            "kind": "physics-coarse",
            "metadata": {
                "physicsTickFixedSubstepSeconds": 0.008333333,
                "physicsMaxSubstepsPerTick": 2
            },
            "compare": {
                "physicsTickFixedSubstepSeconds": 0.016666667,
                "physicsMaxSubstepsPerTick": 4
            }
        });

        apply_physics_tick_config(&case, &mut runtime);

        let config = runtime.physics_tick_config();
        assert_f32_near(config.fixed_substep_seconds, 0.016666667);
        assert_eq!(config.max_substeps_per_tick, 4);
    }
}
