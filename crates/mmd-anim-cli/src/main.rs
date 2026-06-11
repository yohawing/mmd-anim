use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
    time::Instant,
};

use glam::{Quat, Vec3A};
use mmd_anim_format::vmd::VmdBoneKeyframeRaw;
use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, BoneInit, IkSolveOptions, ModelArena,
    MorphIndex, MovableBoneKeyframe, MovableBoneTrack, RuntimeInstance,
};
use mmd_anim_schema::{
    DEFAULT_FOCUSED_IK_BONE_NAMES, GoldenIkBatchManifest, GoldenIkFixture, MmdDumperOracleBone,
    MmdDumperOracleDump, MmdDumperOracleModel,
};
use serde_json::json;
mod golden;

const IMPORT_PAIR_USAGE: &str = "usage: mmd-anim import-pair-summary <model.pmx> <motion.vmd>";
const IMPORT_PAIR_CLIP_USAGE: &str =
    "usage: mmd-anim import-pair-clip-summary <model.pmx> <motion.vmd>";
const IMPORT_PAIR_FRAME_USAGE: &str =
    "usage: mmd-anim import-pair-frame-summary <model.pmx> <motion.vmd> <frame>";
const BENCH_PAIR_USAGE: &str = "usage: mmd-anim bench-pair <model.pmx> <motion.vmd> [start-frame] [frame-count] [step] [--no-ik] [--ik-tolerance <value>] [--ik-max-iterations-cap <count>]";
const IMPORT_PMD_SUMMARY_USAGE: &str = "usage: mmd-anim import-pmd-summary <model.pmd>";
const PARSE_PMX_SUMMARY_USAGE: &str = "usage: mmd-anim parse-pmx-summary <model.pmx>";
const PARSE_FORMAT_USAGE: &str = "usage: mmd-anim parse-format-summary <asset>";
const PARSE_FORMAT_JSON_USAGE: &str = "usage: mmd-anim parse-format-json <asset>";
const EXPORT_ROUNDTRIP_USAGE: &str = "usage: mmd-anim export-roundtrip-summary <asset>";
const EXPORT_ROUNDTRIP_JSON_USAGE: &str = "usage: mmd-anim export-roundtrip-json <asset>";
const EXPORT_JSON_ROUNDTRIP_USAGE: &str = "usage: mmd-anim export-json-roundtrip-summary <asset>";
const EXPORT_JSON_ROUNDTRIP_JSON_USAGE: &str = "usage: mmd-anim export-json-roundtrip-json <asset>";
const GOLDEN_PARSER_SUMMARY_USAGE: &str = "usage: mmd-anim golden-parser-summary <golden-run-root>";
const GOLDEN_IK_DIAGNOSE_USAGE: &str = "usage: mmd-anim golden-ik-diagnose <golden-ik-oracle-root> <case-name> <frame> <bone-name> [sample-frame-offset]";
const COMPARE_NUMERIC_USAGE: &str = "usage: mmd-anim compare-numeric <manifest.json>";
const DIAGNOSE_NUMERIC_BONE_USAGE: &str = "usage: mmd-anim diagnose-numeric-bone <manifest.json> <case-name> <oracle-frame> [--eval-frame <frame>] <bone-name> [bone-name...]";
const EXPORT_FORMAT_USAGE: &str = "usage: mmd-anim export-format <input-asset> <output-asset>";
const EXPORT_JSON_FORMAT_USAGE: &str =
    "usage: mmd-anim export-json-format <input-json> <output-asset>";
const EXPORT_PMM_SCENE_USAGE: &str =
    "usage: mmd-anim export-pmm-scene <model.pmx> <motion.vmd> <output.pmm>";

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("oracle-summary") => {
            let path = required_arg(&mut args, "usage: mmd-anim oracle-summary <oracle.jsonl>");
            if let Err(error) = oracle_summary(&path) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("golden-ik-summary") => {
            let root = required_arg(
                &mut args,
                "usage: mmd-anim golden-ik-summary <golden-ik-oracle-root>",
            );
            if let Err(error) = golden_ik_summary(Path::new(&root)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("golden-parser-summary") => {
            let root = required_arg(&mut args, GOLDEN_PARSER_SUMMARY_USAGE);
            if let Err(error) = golden_parser_summary(Path::new(&root)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("compare-numeric") => {
            let manifest = required_arg(&mut args, COMPARE_NUMERIC_USAGE);
            if let Err(error) = compare_numeric_manifest(Path::new(&manifest)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("compare-camera-vmd-numeric") => {
            let manifest = required_arg(&mut args, COMPARE_NUMERIC_USAGE);
            if let Err(error) = compare_numeric_manifest(Path::new(&manifest)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("diagnose-numeric-bone") => {
            let manifest = required_arg(&mut args, DIAGNOSE_NUMERIC_BONE_USAGE);
            let case_name = required_arg(&mut args, DIAGNOSE_NUMERIC_BONE_USAGE);
            let frame = required_arg(&mut args, DIAGNOSE_NUMERIC_BONE_USAGE);
            let frame: f32 = frame.parse().unwrap_or_else(|_| {
                eprintln!("{DIAGNOSE_NUMERIC_BONE_USAGE}");
                std::process::exit(1);
            });
            let rest: Vec<String> = args.collect();
            let diagnose_options = parse_diagnose_numeric_bone_rest(rest, frame);
            let eval_frame = diagnose_options.eval_frame;
            let bone_names = diagnose_options.bone_names;
            if bone_names.is_empty() {
                eprintln!("{DIAGNOSE_NUMERIC_BONE_USAGE}");
                std::process::exit(1);
            }
            if let Err(error) = diagnose_numeric_bones(
                Path::new(&manifest),
                &case_name,
                frame,
                eval_frame,
                &bone_names,
            ) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("import-pmx-summary") => {
            let path = required_arg(&mut args, "usage: mmd-anim import-pmx-summary <model.pmx>");
            if let Err(error) = import_pmx_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("import-pmx-ik-summary") => {
            let path = required_arg(
                &mut args,
                "usage: mmd-anim import-pmx-ik-summary <model.pmx>",
            );
            if let Err(error) = import_pmx_ik_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("import-pmd-summary") => {
            let path = required_arg(&mut args, IMPORT_PMD_SUMMARY_USAGE);
            if let Err(error) = import_pmd_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("parse-pmx-summary") => {
            let path = required_arg(&mut args, PARSE_PMX_SUMMARY_USAGE);
            if let Err(error) = parse_pmx_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("parse-format-summary") => {
            let path = required_arg(&mut args, PARSE_FORMAT_USAGE);
            if let Err(error) = parse_format_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("parse-format-json") => {
            let path = required_arg(&mut args, PARSE_FORMAT_JSON_USAGE);
            if let Err(error) = parse_format_json(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("export-roundtrip-summary") => {
            let path = required_arg(&mut args, EXPORT_ROUNDTRIP_USAGE);
            if let Err(error) = export_roundtrip_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("export-roundtrip-json") => {
            let path = required_arg(&mut args, EXPORT_ROUNDTRIP_JSON_USAGE);
            if let Err(error) = export_roundtrip_json(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("export-json-roundtrip-summary") => {
            let path = required_arg(&mut args, EXPORT_JSON_ROUNDTRIP_USAGE);
            if let Err(error) = export_json_roundtrip_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("export-json-roundtrip-json") => {
            let path = required_arg(&mut args, EXPORT_JSON_ROUNDTRIP_JSON_USAGE);
            if let Err(error) = export_json_roundtrip_json(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("import-vmd-summary") => {
            let path = required_arg(&mut args, "usage: mmd-anim import-vmd-summary <motion.vmd>");
            if let Err(error) = import_vmd_summary(Path::new(&path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("import-pair-summary") => {
            let pmx_path = required_arg(&mut args, IMPORT_PAIR_USAGE);
            let vmd_path = required_arg(&mut args, IMPORT_PAIR_USAGE);
            if let Err(error) = import_pair_summary(Path::new(&pmx_path), Path::new(&vmd_path)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("import-pair-clip-summary") => {
            let pmx_path = required_arg(&mut args, IMPORT_PAIR_CLIP_USAGE);
            let vmd_path = required_arg(&mut args, IMPORT_PAIR_CLIP_USAGE);
            if let Err(error) = import_pair_clip_summary(Path::new(&pmx_path), Path::new(&vmd_path))
            {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("import-pair-frame-summary") => {
            let pmx_path = required_arg(&mut args, IMPORT_PAIR_FRAME_USAGE);
            let vmd_path = required_arg(&mut args, IMPORT_PAIR_FRAME_USAGE);
            let frame = required_f32_arg(&mut args, IMPORT_PAIR_FRAME_USAGE, "frame number");
            if let Err(error) =
                import_pair_frame_summary(Path::new(&pmx_path), Path::new(&vmd_path), frame)
            {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("bench-pair") => {
            let result = parse_bench_pair_args(&mut args).and_then(bench_pair);
            if let Err(error) = result {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("golden-ik-compare") => {
            let (root, offset, use_json) = match golden::parse_golden_ik_compare_args(&mut args) {
                Ok(parsed) => parsed,
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(2);
                }
            };
            if let Err(error) = golden::golden_ik_compare(Path::new(&root), offset, use_json) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("golden-ik-diagnose") => {
            let root = required_arg(&mut args, GOLDEN_IK_DIAGNOSE_USAGE);
            let case_name = required_arg(&mut args, GOLDEN_IK_DIAGNOSE_USAGE);
            let frame = required_i32_arg(&mut args, GOLDEN_IK_DIAGNOSE_USAGE, "frame");
            let bone_name = required_arg(&mut args, GOLDEN_IK_DIAGNOSE_USAGE);
            let offset = optional_f32_arg(&mut args, "sample-frame-offset");
            if let Err(error) =
                golden::golden_ik_diagnose(Path::new(&root), &case_name, frame, &bone_name, offset)
            {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("bench-synthetic") => {
            let result = parse_bench_synthetic_args(&mut args).and_then(bench_synthetic);
            if let Err(error) = result {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("export-format") => {
            let input = required_arg(&mut args, EXPORT_FORMAT_USAGE);
            let output = required_arg(&mut args, EXPORT_FORMAT_USAGE);
            if let Err(error) = export_format(Path::new(&input), Path::new(&output)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("export-pmm-scene") => {
            let model = required_arg(&mut args, EXPORT_PMM_SCENE_USAGE);
            let motion = required_arg(&mut args, EXPORT_PMM_SCENE_USAGE);
            let output = required_arg(&mut args, EXPORT_PMM_SCENE_USAGE);
            if let Err(error) =
                export_pmm_scene(Path::new(&model), Path::new(&motion), Path::new(&output))
            {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("export-json-format") => {
            let input = required_arg(&mut args, EXPORT_JSON_FORMAT_USAGE);
            let output = required_arg(&mut args, EXPORT_JSON_FORMAT_USAGE);
            if let Err(error) = export_json_format(Path::new(&input), Path::new(&output)) {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        Some("--version") | Some("-V") => println!("mmd-anim {}", env!("CARGO_PKG_VERSION")),
        None => println!("mmd-anim {}", env!("CARGO_PKG_VERSION")),
        Some(command) => {
            eprintln!("unknown command: {command}");
            std::process::exit(2);
        }
    }
}

fn required_arg(args: &mut impl Iterator<Item = String>, usage: &str) -> String {
    match args.next() {
        Some(value) => value,
        None => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    }
}

fn required_f32_arg(args: &mut impl Iterator<Item = String>, usage: &str, label: &str) -> f32 {
    let value = required_arg(args, usage);
    match value.parse::<f32>() {
        Ok(parsed) => parsed,
        Err(_) => {
            eprintln!("invalid {label}: {value}");
            std::process::exit(2);
        }
    }
}

fn required_i32_arg(args: &mut impl Iterator<Item = String>, usage: &str, label: &str) -> i32 {
    let value = required_arg(args, usage);
    match value.parse::<i32>() {
        Ok(parsed) => parsed,
        Err(_) => {
            eprintln!("invalid {label}: {value}");
            std::process::exit(2);
        }
    }
}

fn optional_f32_arg(args: &mut impl Iterator<Item = String>, label: &str) -> f32 {
    match args.next() {
        Some(value) => match value.parse::<f32>() {
            Ok(parsed) => parsed,
            Err(_) => {
                eprintln!("invalid {label}: {value}");
                std::process::exit(2);
            }
        },
        None => 0.0,
    }
}

fn import_pmx_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let imported = mmd_anim_format::import_pmx_runtime(&data)?;
    println!(
        "PMX runtime import: bones={} append={} fixedAxis={} ik={} boneNames={} morphNames={} ikNameMap={}",
        imported.model.bone_count(),
        imported.model.append_transforms().len(),
        imported.model.fixed_axis_count(),
        imported.model.ik_count(),
        imported.bone_name_to_index.len(),
        imported.morph_name_to_index.len(),
        imported.ik_solver_bone_name_to_index.len()
    );
    Ok(ExitCode::SUCCESS)
}

fn import_pmx_ik_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let imported = mmd_anim_format::import_pmx_runtime(&data)?;
    let solvers = imported.model.ik_solvers();
    let max_iterations = solvers
        .iter()
        .map(|solver| solver.iteration_count)
        .max()
        .unwrap_or(0);
    let mut distribution = BTreeMap::<u32, usize>::new();
    for solver in solvers {
        *distribution.entry(solver.iteration_count).or_default() += 1;
    }
    let distribution = distribution
        .iter()
        .map(|(iterations, count)| format!("{iterations}:{count}"))
        .collect::<Vec<_>>()
        .join(",");

    println!(
        "PMX IK summary: bones={} ik={} maxIterations={} distribution={}",
        imported.model.bone_count(),
        solvers.len(),
        max_iterations,
        distribution
    );
    for (solver_index, solver) in solvers.iter().enumerate() {
        if solver.iteration_count == max_iterations {
            let name = imported
                .bone_names
                .get(solver.ik_bone.as_usize())
                .map(String::as_str)
                .unwrap_or("<unknown>");
            println!(
                "max IK: solver={} bone={} name={} target={} iterations={} limitAngle={:.6} links={}",
                solver_index,
                solver.ik_bone.as_usize(),
                name,
                solver.target_bone.as_usize(),
                solver.iteration_count,
                solver.limit_angle,
                solver.links.len()
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn import_pmd_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let imported = mmd_anim_format::import_pmd_runtime(&data)?;
    println!(
        "PMD runtime import: bones={} ik={} morphSlots={} vertexMorphOffsets={} boneNames={} morphNames={} ikNameMap={} diagnostics={}",
        imported.model.bone_count(),
        imported.model.ik_count(),
        imported.model.morph_count(),
        imported.model.vertex_morph_offsets().len(),
        imported.bone_name_to_index.len(),
        imported.morph_name_to_index.len(),
        imported.ik_solver_bone_name_to_index.len(),
        imported.diagnostics.len()
    );
    Ok(ExitCode::SUCCESS)
}

fn parse_pmx_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let parsed = mmd_anim_format::parse_pmx_model(&data)?;
    println!(
        "PMX parser: vertices={} faces={} materials={} bones={} morphs={} displayFrames={} rigidBodies={} joints={} softBodies={} diagnostics={}",
        parsed.metadata.counts.vertices,
        parsed.metadata.counts.faces,
        parsed.metadata.counts.materials,
        parsed.metadata.counts.bones,
        parsed.metadata.counts.morphs,
        parsed.metadata.counts.display_frames,
        parsed.metadata.counts.rigid_bodies,
        parsed.metadata.counts.joints,
        parsed.metadata.counts.soft_bodies,
        parsed.diagnostics.len()
    );
    Ok(ExitCode::SUCCESS)
}

fn parse_format_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    match mmd_anim_format::detect_mmd_format(&data, path.file_name().and_then(|v| v.to_str())) {
        mmd_anim_format::MmdFormatKind::Pmx => parse_pmx_summary(path),
        mmd_anim_format::MmdFormatKind::Pmd => {
            let parsed = mmd_anim_format::parse_pmd_model(&data)?;
            println!(
                "PMD parser: vertices={} faces={} materials={} bones={} ik={} morphs={} displayFrames={} rigidBodies={} joints={}",
                parsed.metadata.counts.vertices,
                parsed.metadata.counts.faces,
                parsed.metadata.counts.materials,
                parsed.metadata.counts.bones,
                parsed.metadata.counts.ik,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.display_frames,
                parsed.metadata.counts.rigid_bodies,
                parsed.metadata.counts.joints
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Vmd => {
            let parsed = mmd_anim_format::parse_vmd_animation(&data)?;
            println!(
                "VMD parser: boneFrames={} morphFrames={} cameraFrames={} lightFrames={} selfShadowFrames={} propertyFrames={} maxFrame={}",
                parsed.metadata.counts.bones,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.cameras,
                parsed.metadata.counts.lights,
                parsed.metadata.counts.self_shadows,
                parsed.metadata.counts.properties,
                parsed.metadata.max_frame
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            let parsed = mmd_anim_format::parse_vpd_pose(&data)?;
            println!(
                "VPD parser: bones={} diagnostics={}",
                parsed.bone_count,
                parsed.diagnostics.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Pmm => {
            let parsed = mmd_anim_format::parse_pmm_manifest(&data)?;
            let model_slot_flag_counts = parsed
                .display_state
                .model_slot_flag_counts
                .iter()
                .map(|(flag, count)| format!("{flag}:{count}"))
                .collect::<Vec<_>>()
                .join(",");
            let asset_kind_counts = parsed
                .asset_summary
                .kind_counts
                .iter()
                .map(|(kind, count)| format!("{kind}:{count}"))
                .collect::<Vec<_>>()
                .join(",");
            let asset_extension_counts = parsed
                .asset_summary
                .extension_counts
                .iter()
                .map(|(extension, count)| format!("{extension}:{count}"))
                .collect::<Vec<_>>()
                .join(",");
            let first_model_slot_padding = parsed
                .model_slots
                .first()
                .map(|slot| slot.trailing_zero_padding_bytes.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let first_model_slot_next_non_zero = parsed
                .model_slots
                .first()
                .and_then(|slot| slot.next_non_zero_offset)
                .map(|offset| offset.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let document_models = parsed
                .document_summary
                .as_ref()
                .map(|document| document.counts.models.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let document_bone_keyframes = parsed
                .document_summary
                .as_ref()
                .map(|document| document.counts.bone_keyframes.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let document_morph_keyframes = parsed
                .document_summary
                .as_ref()
                .map(|document| document.counts.morph_keyframes.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_camera_keyframes = parsed
                .document_global_summary
                .as_ref()
                .map(|g| (g.camera.initial_keyframes + g.camera.keyframes).to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_light_keyframes = parsed
                .document_global_summary
                .as_ref()
                .map(|g| (g.light.initial_keyframes + g.light.keyframes).to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_accessory_count = parsed
                .document_global_summary
                .as_ref()
                .map(|g| g.accessories.accessory_count.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_accessory_keyframes = parsed
                .document_global_summary
                .as_ref()
                .map(|g| g.accessories.keyframes.to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_gravity_keyframes = parsed
                .document_global_summary
                .as_ref()
                .map(|g| (g.gravity.initial_keyframes + g.gravity.keyframes).to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_self_shadow_keyframes = parsed
                .document_global_summary
                .as_ref()
                .map(|g| (g.self_shadow.initial_keyframes + g.self_shadow.keyframes).to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_audio_path = parsed
                .document_global_summary
                .as_ref()
                .map(|g| (!g.settings.audio_path.is_empty()).to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_bg_video_path = parsed
                .document_global_summary
                .as_ref()
                .map(|g| (!g.settings.background_video_path.is_empty()).to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            let global_bg_image_path = parsed
                .document_global_summary
                .as_ref()
                .map(|g| (!g.settings.background_image_path.is_empty()).to_string())
                .unwrap_or_else(|| "unknown".to_owned());
            println!(
                "PMM parser: references={} version={} parsedVersion={} models={} accessories={} motions={} audio={} images={} videos={} modelAssets={} modelSlots={} firstModelSlotPadding={} firstModelSlotNextNonZeroOffset={} documentModels={} documentBoneKeyframes={} documentMorphKeyframes={} headerTextEntries={} audioAssets={} assetConfidence=high:{} medium:{} low:{} assetKindCounts={} assetExtensionCounts={} screen={}x{} timelineFrames={} timelineRange={}..{} frameRate={} durationSeconds={} displayLayout={} selectedModelIndex={} documentModelCount={} declaredModelSlotCount={} modelSlotCount={} nonZeroModelSlotCount={} modelSlotFlags={} activeModelSlotIndices={} emptyModelSlotIndices={} modelSlotFlagCounts={} accessorySlotCount={} globalCameraKeyframes={} globalLightKeyframes={} globalAccessoryCount={} globalAccessoryKeyframes={} globalGravityKeyframes={} globalSelfShadowKeyframes={} globalAudioPath={} globalBgVideoPath={} globalBgImagePath={} diagnostics={}",
                parsed.asset_summary.reference_count,
                parsed.version,
                parsed
                    .parsed_version
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed.model_paths.len(),
                parsed.accessory_paths.len(),
                parsed.motion_paths.len(),
                parsed.audio_paths.len(),
                parsed.image_paths.len(),
                parsed.video_paths.len(),
                parsed.model_assets.len(),
                parsed.model_slots.len(),
                first_model_slot_padding,
                first_model_slot_next_non_zero,
                document_models,
                document_bone_keyframes,
                document_morph_keyframes,
                parsed.header_text_entries.len(),
                parsed.audio_assets.len(),
                parsed.asset_summary.high_confidence_count,
                parsed.asset_summary.medium_confidence_count,
                parsed.asset_summary.low_confidence_count,
                asset_kind_counts,
                asset_extension_counts,
                parsed
                    .project_settings
                    .screen_width
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .project_settings
                    .screen_height
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .project_settings
                    .timeline_frame_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .timeline
                    .start_frame
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .timeline
                    .end_frame_exclusive
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .project_settings
                    .frame_rate
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .timeline
                    .duration_seconds
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed.display_state.layout,
                parsed
                    .display_state
                    .selected_model_index
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .display_state
                    .document_model_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed
                    .display_state
                    .declared_model_slot_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                parsed.display_state.model_slot_count,
                parsed.display_state.non_zero_model_slot_count,
                parsed
                    .display_state
                    .model_slot_flags
                    .iter()
                    .map(|flag| flag.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                parsed
                    .display_state
                    .active_model_slot_indices
                    .iter()
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                parsed
                    .display_state
                    .empty_model_slot_indices
                    .iter()
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
                model_slot_flag_counts,
                parsed
                    .display_state
                    .accessory_slot_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
                global_camera_keyframes,
                global_light_keyframes,
                global_accessory_count,
                global_accessory_keyframes,
                global_gravity_keyframes,
                global_self_shadow_keyframes,
                global_audio_path,
                global_bg_video_path,
                global_bg_image_path,
                parsed.diagnostics.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            let parsed = mmd_anim_format::parse_accessory_manifest(
                &data,
                path.file_name().and_then(|v| v.to_str()),
            )?;
            println!(
                "{} parser: byteLength={} meshes={} materials={} textures={} diagnostics={}",
                parsed.format.to_uppercase(),
                parsed.byte_length,
                parsed.mesh_count,
                parsed.material_count,
                parsed.texture_references.len(),
                parsed.diagnostics.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Nmd => {
            let parsed = mmd_anim_format::parse_nmd_manifest(&data)?;
            println!(
                "NMD parser: byteLength={} annotations={} globalTracks={} bundles={{accessory:{}, bone:{}, camera:{}, light:{}, model:{}, morph:{}, selfShadow:{}, unknown:{}}} diagnostics={}",
                parsed.byte_length,
                parsed.metadata.annotation_count,
                parsed.global_track_count,
                parsed.keyframe_bundles.accessory,
                parsed.keyframe_bundles.bone,
                parsed.keyframe_bundles.camera,
                parsed.keyframe_bundles.light,
                parsed.keyframe_bundles.model,
                parsed.keyframe_bundles.morph,
                parsed.keyframe_bundles.self_shadow,
                parsed.keyframe_bundles.unknown,
                parsed.diagnostics.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Unknown => Err("unknown MMD format".into()),
    }
}

fn parse_format_json(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let value = match mmd_anim_format::detect_mmd_format(
        &data,
        path.file_name().and_then(|v| v.to_str()),
    ) {
        mmd_anim_format::MmdFormatKind::Pmx => {
            serde_json::to_value(mmd_anim_format::parse_pmx_model(&data)?)?
        }
        mmd_anim_format::MmdFormatKind::Pmd => {
            serde_json::to_value(mmd_anim_format::parse_pmd_model(&data)?)?
        }
        mmd_anim_format::MmdFormatKind::Vmd => {
            serde_json::to_value(mmd_anim_format::parse_vmd_animation(&data)?)?
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            serde_json::to_value(mmd_anim_format::parse_vpd_pose(&data)?)?
        }
        mmd_anim_format::MmdFormatKind::Pmm => {
            serde_json::to_value(mmd_anim_format::parse_pmm_manifest(&data)?)?
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            serde_json::to_value(mmd_anim_format::parse_accessory_manifest(
                &data,
                path.file_name().and_then(|v| v.to_str()),
            )?)?
        }
        mmd_anim_format::MmdFormatKind::Nmd => {
            serde_json::to_value(mmd_anim_format::parse_nmd_manifest(&data)?)?
        }
        mmd_anim_format::MmdFormatKind::Unknown => return Err("unknown MMD format".into()),
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(ExitCode::SUCCESS)
}

fn export_roundtrip_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    match mmd_anim_format::detect_mmd_format(&data, path.file_name().and_then(|v| v.to_str())) {
        mmd_anim_format::MmdFormatKind::Vmd => {
            let parsed = mmd_anim_format::parse_vmd_animation(&data)?;
            let exported = mmd_anim_format::export_vmd_animation(&parsed);
            let reparsed = mmd_anim_format::parse_vmd_animation(&exported)?;
            ensure_vmd_roundtrip(&parsed, &reparsed)?;
            println!(
                "VMD export roundtrip: ok bytesIn={} bytesOut={} boneFrames={} morphFrames={} cameraFrames={} lightFrames={} selfShadowFrames={} propertyFrames={} maxFrame={}",
                data.len(),
                exported.len(),
                parsed.metadata.counts.bones,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.cameras,
                parsed.metadata.counts.lights,
                parsed.metadata.counts.self_shadows,
                parsed.metadata.counts.properties,
                parsed.metadata.max_frame
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            let parsed = mmd_anim_format::parse_vpd_pose(&data)?;
            let exported = mmd_anim_format::export_vpd_pose(&parsed);
            let reparsed = mmd_anim_format::parse_vpd_pose(&exported)?;
            ensure_vpd_roundtrip(&parsed, &reparsed)?;
            println!(
                "VPD export roundtrip: ok bytesIn={} bytesOut={} bones={}",
                data.len(),
                exported.len(),
                parsed.bone_count
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Pmx => {
            let parsed = mmd_anim_format::parse_pmx_model(&data)?;
            let exported = mmd_anim_format::export_pmx_model(&parsed);
            let reparsed = mmd_anim_format::parse_pmx_model(&exported)?;
            ensure_pmx_roundtrip(&parsed, &reparsed)?;
            println!(
                "PMX export roundtrip: ok bytesIn={} bytesOut={} vertices={} faces={} materials={} bones={} morphs={} displayFrames={} rigidBodies={} joints={} softBodies={}",
                data.len(),
                exported.len(),
                parsed.metadata.counts.vertices,
                parsed.metadata.counts.faces,
                parsed.metadata.counts.materials,
                parsed.metadata.counts.bones,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.display_frames,
                parsed.metadata.counts.rigid_bodies,
                parsed.metadata.counts.joints,
                parsed.metadata.counts.soft_bodies
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Pmd => {
            let parsed = mmd_anim_format::parse_pmd_model(&data)?;
            let exported = mmd_anim_format::export_pmd_model(&parsed);
            let reparsed = mmd_anim_format::parse_pmd_model(&exported)?;
            ensure_pmd_roundtrip(&parsed, &reparsed)?;
            println!(
                "PMD export roundtrip: ok bytesIn={} bytesOut={} vertices={} faces={} materials={} bones={} ik={} morphs={} displayFrames={} rigidBodies={} joints={}",
                data.len(),
                exported.len(),
                parsed.metadata.counts.vertices,
                parsed.metadata.counts.faces,
                parsed.metadata.counts.materials,
                parsed.metadata.counts.bones,
                parsed.metadata.counts.ik,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.display_frames,
                parsed.metadata.counts.rigid_bodies,
                parsed.metadata.counts.joints
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            let file_name = path.file_name().and_then(|v| v.to_str());
            let parsed = mmd_anim_format::parse_accessory_manifest(&data, file_name)?;
            let exported = mmd_anim_format::export_accessory_manifest(&parsed);
            let reparsed = mmd_anim_format::parse_accessory_manifest(&exported, file_name)?;
            ensure_accessory_roundtrip(&parsed, &reparsed)?;
            println!(
                "{} export roundtrip: ok bytesIn={} bytesOut={} textures={} diagnostics={}",
                parsed.format.to_ascii_uppercase(),
                data.len(),
                exported.len(),
                parsed.texture_references.len(),
                parsed.diagnostics.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        kind => Err(format!("export roundtrip is not implemented for {kind:?}").into()),
    }
}

fn export_roundtrip_json(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let result = match mmd_anim_format::detect_mmd_format(
        &data,
        path.file_name().and_then(|v| v.to_str()),
    ) {
        mmd_anim_format::MmdFormatKind::Vmd => {
            let parsed = mmd_anim_format::parse_vmd_animation(&data)?;
            let exported = mmd_anim_format::export_vmd_animation(&parsed);
            let reparsed = mmd_anim_format::parse_vmd_animation(&exported)?;
            ensure_vmd_roundtrip(&parsed, &reparsed)?;
            vmd_roundtrip_json(
                path,
                "parse-export-parse",
                data.len(),
                exported.len(),
                None,
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            let parsed = mmd_anim_format::parse_vpd_pose(&data)?;
            let exported = mmd_anim_format::export_vpd_pose(&parsed);
            let reparsed = mmd_anim_format::parse_vpd_pose(&exported)?;
            ensure_vpd_roundtrip(&parsed, &reparsed)?;
            vpd_roundtrip_json(
                path,
                "parse-export-parse",
                data.len(),
                exported.len(),
                None,
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::Pmx => {
            let parsed = mmd_anim_format::parse_pmx_model(&data)?;
            let exported = mmd_anim_format::export_pmx_model(&parsed);
            let reparsed = mmd_anim_format::parse_pmx_model(&exported)?;
            ensure_pmx_roundtrip(&parsed, &reparsed)?;
            pmx_roundtrip_json(
                path,
                "parse-export-parse",
                data.len(),
                exported.len(),
                None,
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::Pmd => {
            let parsed = mmd_anim_format::parse_pmd_model(&data)?;
            let exported = mmd_anim_format::export_pmd_model(&parsed);
            let reparsed = mmd_anim_format::parse_pmd_model(&exported)?;
            ensure_pmd_roundtrip(&parsed, &reparsed)?;
            pmd_roundtrip_json(
                path,
                "parse-export-parse",
                data.len(),
                exported.len(),
                None,
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            let file_name = path.file_name().and_then(|v| v.to_str());
            let parsed = mmd_anim_format::parse_accessory_manifest(&data, file_name)?;
            let exported = mmd_anim_format::export_accessory_manifest(&parsed);
            let reparsed = mmd_anim_format::parse_accessory_manifest(&exported, file_name)?;
            ensure_accessory_roundtrip(&parsed, &reparsed)?;
            accessory_roundtrip_json(
                path,
                "parse-export-parse",
                data.len(),
                exported.len(),
                None,
                &parsed,
            )
        }
        kind => return Err(format!("export roundtrip is not implemented for {kind:?}").into()),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(ExitCode::SUCCESS)
}

fn export_json_roundtrip_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    match mmd_anim_format::detect_mmd_format(&data, path.file_name().and_then(|v| v.to_str())) {
        mmd_anim_format::MmdFormatKind::Vmd => {
            let parsed = mmd_anim_format::parse_vmd_animation(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::VmdParsedAnimation = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_vmd_animation(&from_json);
            let reparsed = mmd_anim_format::parse_vmd_animation(&exported)?;
            ensure_vmd_roundtrip(&parsed, &reparsed)?;
            println!(
                "VMD export JSON roundtrip: ok jsonBytes={} bytesIn={} bytesOut={} boneFrames={} morphFrames={} cameraFrames={} lightFrames={} selfShadowFrames={} propertyFrames={} maxFrame={}",
                json.len(),
                data.len(),
                exported.len(),
                parsed.metadata.counts.bones,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.cameras,
                parsed.metadata.counts.lights,
                parsed.metadata.counts.self_shadows,
                parsed.metadata.counts.properties,
                parsed.metadata.max_frame
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            let parsed = mmd_anim_format::parse_vpd_pose(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::VpdParsedPose = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_vpd_pose(&from_json);
            let reparsed = mmd_anim_format::parse_vpd_pose(&exported)?;
            ensure_vpd_roundtrip(&parsed, &reparsed)?;
            println!(
                "VPD export JSON roundtrip: ok jsonBytes={} bytesIn={} bytesOut={} bones={}",
                json.len(),
                data.len(),
                exported.len(),
                parsed.bone_count
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Pmx => {
            let parsed = mmd_anim_format::parse_pmx_model(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::PmxParsedModel = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_pmx_model(&from_json);
            let reparsed = mmd_anim_format::parse_pmx_model(&exported)?;
            ensure_pmx_roundtrip(&parsed, &reparsed)?;
            println!(
                "PMX export JSON roundtrip: ok jsonBytes={} bytesIn={} bytesOut={} vertices={} faces={} materials={} bones={} morphs={} displayFrames={} rigidBodies={} joints={} softBodies={}",
                json.len(),
                data.len(),
                exported.len(),
                parsed.metadata.counts.vertices,
                parsed.metadata.counts.faces,
                parsed.metadata.counts.materials,
                parsed.metadata.counts.bones,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.display_frames,
                parsed.metadata.counts.rigid_bodies,
                parsed.metadata.counts.joints,
                parsed.metadata.counts.soft_bodies
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Pmd => {
            let parsed = mmd_anim_format::parse_pmd_model(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::PmdParsedModel = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_pmd_model(&from_json);
            let reparsed = mmd_anim_format::parse_pmd_model(&exported)?;
            ensure_pmd_roundtrip(&parsed, &reparsed)?;
            println!(
                "PMD export JSON roundtrip: ok jsonBytes={} bytesIn={} bytesOut={} vertices={} faces={} materials={} bones={} ik={} morphs={} displayFrames={} rigidBodies={} joints={}",
                json.len(),
                data.len(),
                exported.len(),
                parsed.metadata.counts.vertices,
                parsed.metadata.counts.faces,
                parsed.metadata.counts.materials,
                parsed.metadata.counts.bones,
                parsed.metadata.counts.ik,
                parsed.metadata.counts.morphs,
                parsed.metadata.counts.display_frames,
                parsed.metadata.counts.rigid_bodies,
                parsed.metadata.counts.joints
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            let file_name = path.file_name().and_then(|v| v.to_str());
            let parsed = mmd_anim_format::parse_accessory_manifest(&data, file_name)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::AccessoryParsedManifest = serde_json::from_str(&json)?;
            ensure_accessory_json_roundtrip(&parsed, &from_json)?;
            let exported = mmd_anim_format::export_accessory_manifest(&from_json);
            let reparsed = mmd_anim_format::parse_accessory_manifest(&exported, file_name)?;
            ensure_accessory_roundtrip(&parsed, &reparsed)?;
            println!(
                "{} export JSON roundtrip: ok jsonBytes={} bytesIn={} bytesOut={} textures={} diagnostics={}",
                parsed.format.to_ascii_uppercase(),
                json.len(),
                data.len(),
                exported.len(),
                parsed.texture_references.len(),
                parsed.diagnostics.len()
            );
            Ok(ExitCode::SUCCESS)
        }
        kind => Err(format!("export JSON roundtrip is not implemented for {kind:?}").into()),
    }
}

fn export_json_roundtrip_json(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let result = match mmd_anim_format::detect_mmd_format(
        &data,
        path.file_name().and_then(|v| v.to_str()),
    ) {
        mmd_anim_format::MmdFormatKind::Vmd => {
            let parsed = mmd_anim_format::parse_vmd_animation(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::VmdParsedAnimation = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_vmd_animation(&from_json);
            let reparsed = mmd_anim_format::parse_vmd_animation(&exported)?;
            ensure_vmd_roundtrip(&parsed, &reparsed)?;
            vmd_roundtrip_json(
                path,
                "parse-json-export-parse",
                data.len(),
                exported.len(),
                Some(json.len()),
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            let parsed = mmd_anim_format::parse_vpd_pose(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::VpdParsedPose = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_vpd_pose(&from_json);
            let reparsed = mmd_anim_format::parse_vpd_pose(&exported)?;
            ensure_vpd_roundtrip(&parsed, &reparsed)?;
            vpd_roundtrip_json(
                path,
                "parse-json-export-parse",
                data.len(),
                exported.len(),
                Some(json.len()),
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::Pmx => {
            let parsed = mmd_anim_format::parse_pmx_model(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::PmxParsedModel = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_pmx_model(&from_json);
            let reparsed = mmd_anim_format::parse_pmx_model(&exported)?;
            ensure_pmx_roundtrip(&parsed, &reparsed)?;
            pmx_roundtrip_json(
                path,
                "parse-json-export-parse",
                data.len(),
                exported.len(),
                Some(json.len()),
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::Pmd => {
            let parsed = mmd_anim_format::parse_pmd_model(&data)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::PmdParsedModel = serde_json::from_str(&json)?;
            let exported = mmd_anim_format::export_pmd_model(&from_json);
            let reparsed = mmd_anim_format::parse_pmd_model(&exported)?;
            ensure_pmd_roundtrip(&parsed, &reparsed)?;
            pmd_roundtrip_json(
                path,
                "parse-json-export-parse",
                data.len(),
                exported.len(),
                Some(json.len()),
                &parsed,
            )
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            let file_name = path.file_name().and_then(|v| v.to_str());
            let parsed = mmd_anim_format::parse_accessory_manifest(&data, file_name)?;
            let json = serde_json::to_string(&parsed)?;
            let from_json: mmd_anim_format::AccessoryParsedManifest = serde_json::from_str(&json)?;
            ensure_accessory_json_roundtrip(&parsed, &from_json)?;
            let exported = mmd_anim_format::export_accessory_manifest(&from_json);
            let reparsed = mmd_anim_format::parse_accessory_manifest(&exported, file_name)?;
            ensure_accessory_roundtrip(&parsed, &reparsed)?;
            accessory_roundtrip_json(
                path,
                "parse-json-export-parse",
                data.len(),
                exported.len(),
                Some(json.len()),
                &parsed,
            )
        }
        kind => {
            return Err(format!("export JSON roundtrip is not implemented for {kind:?}").into());
        }
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(ExitCode::SUCCESS)
}

fn export_format(input: &Path, output: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(input)?;
    let kind =
        mmd_anim_format::detect_mmd_format(&data, input.file_name().and_then(|v| v.to_str()));
    let (exported, kind_label): (Vec<u8>, &str) = match kind {
        mmd_anim_format::MmdFormatKind::Vmd => {
            let parsed = mmd_anim_format::parse_vmd_animation(&data)?;
            (mmd_anim_format::export_vmd_animation(&parsed), "VMD")
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            let parsed = mmd_anim_format::parse_vpd_pose(&data)?;
            (mmd_anim_format::export_vpd_pose(&parsed), "VPD")
        }
        mmd_anim_format::MmdFormatKind::Pmx => {
            let parsed = mmd_anim_format::parse_pmx_model(&data)?;
            (mmd_anim_format::export_pmx_model(&parsed), "PMX")
        }
        mmd_anim_format::MmdFormatKind::Pmd => {
            let parsed = mmd_anim_format::parse_pmd_model(&data)?;
            (mmd_anim_format::export_pmd_model(&parsed), "PMD")
        }
        mmd_anim_format::MmdFormatKind::Pmm => {
            let parsed = mmd_anim_format::parse_pmm_manifest(&data)?;
            (mmd_anim_format::export_pmm_manifest(&parsed), "PMM")
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            let file_name = input.file_name().and_then(|v| v.to_str());
            let parsed = mmd_anim_format::parse_accessory_manifest(&data, file_name)?;
            let label: &'static str = if parsed.format == "vac" { "VAC" } else { "X" };
            (mmd_anim_format::export_accessory_manifest(&parsed), label)
        }
        kind => return Err(format!("export is not supported for {kind:?}").into()),
    };
    fs::write(output, &exported)?;
    println!(
        "{kind_label} export: ok bytesIn={} bytesOut={} output={}",
        data.len(),
        exported.len(),
        output.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn export_pmm_scene(
    model_path: &Path,
    motion_path: &Path,
    output_path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let model_data = fs::read(model_path)?;
    let motion_data = fs::read(motion_path)?;
    let model = mmd_anim_format::parse_pmx_model(&model_data)?;
    let motion = mmd_anim_format::parse_vmd_animation(&motion_data)?;
    let model_path_text = model_path.to_string_lossy();
    let report = mmd_anim_format::export_pmm_scene_from_pmx_vmd(
        &model,
        &motion,
        &model_path_text,
        &mmd_anim_format::PmmSceneExportOptions::default(),
    );
    fs::write(output_path, &report.bytes)?;

    let reparsed = mmd_anim_format::parse_pmm_manifest(&report.bytes)?;
    let document = reparsed
        .document_summary
        .as_ref()
        .ok_or("generated PMM does not contain a document model block")?;
    println!(
        "PMM scene export: ok bytesOut={} bones={} morphs={} boneKeyframes={} morphKeyframes={} frame0Bones={} frame0Morphs={} skippedBones={} skippedMorphs={} maxFrame={} output={}",
        report.bytes.len(),
        document.counts.bones,
        document.counts.morphs,
        report.bone_keyframes,
        report.morph_keyframes,
        report.frame_zero_bone_keyframes,
        report.frame_zero_morph_keyframes,
        report.skipped_bone_keyframes,
        report.skipped_morph_keyframes,
        report.max_frame,
        output_path.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn export_json_format(input: &Path, output: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let json = fs::read_to_string(input)?;
    let ext = output
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (exported, kind_label): (Vec<u8>, &str) = match ext.as_str() {
        "vmd" => {
            let dto: mmd_anim_format::VmdParsedAnimation = serde_json::from_str(&json)?;
            (mmd_anim_format::export_vmd_animation(&dto), "VMD")
        }
        "vpd" => {
            let dto: mmd_anim_format::VpdParsedPose = serde_json::from_str(&json)?;
            (mmd_anim_format::export_vpd_pose(&dto), "VPD")
        }
        "pmx" => {
            let dto: mmd_anim_format::PmxParsedModel = serde_json::from_str(&json)?;
            (mmd_anim_format::export_pmx_model(&dto), "PMX")
        }
        "pmd" => {
            let dto: mmd_anim_format::PmdParsedModel = serde_json::from_str(&json)?;
            (mmd_anim_format::export_pmd_model(&dto), "PMD")
        }
        "x" | "vac" => {
            let dto: mmd_anim_format::AccessoryParsedManifest = serde_json::from_str(&json)?;
            let label: &'static str = if ext == "vac" { "VAC" } else { "X" };
            (mmd_anim_format::export_accessory_manifest(&dto), label)
        }
        _ => {
            return Err(format!(
                "cannot determine export format from output extension {:?}; \
                 supported: vmd, vpd, pmx, pmd, x, vac",
                ext
            )
            .into());
        }
    };
    fs::write(output, &exported)?;
    println!(
        "{kind_label} export from JSON: ok jsonBytes={} bytesOut={} output={}",
        json.len(),
        exported.len(),
        output.display()
    );
    Ok(ExitCode::SUCCESS)
}

fn vmd_roundtrip_json(
    path: &Path,
    mode: &str,
    bytes_in: usize,
    bytes_out: usize,
    json_bytes: Option<usize>,
    parsed: &mmd_anim_format::VmdParsedAnimation,
) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "mode": mode,
        "format": "vmd",
        "path": path.display().to_string(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "jsonBytes": json_bytes,
        "counts": {
            "boneFrames": parsed.metadata.counts.bones,
            "morphFrames": parsed.metadata.counts.morphs,
            "cameraFrames": parsed.metadata.counts.cameras,
            "lightFrames": parsed.metadata.counts.lights,
            "selfShadowFrames": parsed.metadata.counts.self_shadows,
            "propertyFrames": parsed.metadata.counts.properties,
        },
        "maxFrame": parsed.metadata.max_frame,
    })
}

fn vpd_roundtrip_json(
    path: &Path,
    mode: &str,
    bytes_in: usize,
    bytes_out: usize,
    json_bytes: Option<usize>,
    parsed: &mmd_anim_format::VpdParsedPose,
) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "mode": mode,
        "format": "vpd",
        "path": path.display().to_string(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "jsonBytes": json_bytes,
        "counts": {
            "bones": parsed.bone_count,
        },
    })
}

fn pmd_roundtrip_json(
    path: &Path,
    mode: &str,
    bytes_in: usize,
    bytes_out: usize,
    json_bytes: Option<usize>,
    parsed: &mmd_anim_format::PmdParsedModel,
) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "mode": mode,
        "format": "pmd",
        "path": path.display().to_string(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "jsonBytes": json_bytes,
        "counts": {
            "vertices": parsed.metadata.counts.vertices,
            "faces": parsed.metadata.counts.faces,
            "materials": parsed.metadata.counts.materials,
            "bones": parsed.metadata.counts.bones,
            "ik": parsed.metadata.counts.ik,
            "morphs": parsed.metadata.counts.morphs,
            "displayFrames": parsed.metadata.counts.display_frames,
            "rigidBodies": parsed.metadata.counts.rigid_bodies,
            "joints": parsed.metadata.counts.joints,
        },
    })
}

fn pmx_roundtrip_json(
    path: &Path,
    mode: &str,
    bytes_in: usize,
    bytes_out: usize,
    json_bytes: Option<usize>,
    parsed: &mmd_anim_format::PmxParsedModel,
) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "mode": mode,
        "format": "pmx",
        "path": path.display().to_string(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "jsonBytes": json_bytes,
        "metadata": {
            "version": parsed.metadata.version,
            "encoding": parsed.metadata.encoding,
            "additionalUvCount": parsed.metadata.additional_uv_count,
            "indexSizes": {
                "vertex": parsed.metadata.index_sizes.vertex,
                "texture": parsed.metadata.index_sizes.texture,
                "material": parsed.metadata.index_sizes.material,
                "bone": parsed.metadata.index_sizes.bone,
                "morph": parsed.metadata.index_sizes.morph,
                "rigidBody": parsed.metadata.index_sizes.rigid_body,
            },
        },
        "counts": {
            "vertices": parsed.metadata.counts.vertices,
            "faces": parsed.metadata.counts.faces,
            "materials": parsed.metadata.counts.materials,
            "bones": parsed.metadata.counts.bones,
            "morphs": parsed.metadata.counts.morphs,
            "displayFrames": parsed.metadata.counts.display_frames,
            "rigidBodies": parsed.metadata.counts.rigid_bodies,
            "joints": parsed.metadata.counts.joints,
            "softBodies": parsed.metadata.counts.soft_bodies,
        },
    })
}

fn accessory_roundtrip_json(
    path: &Path,
    mode: &str,
    bytes_in: usize,
    bytes_out: usize,
    json_bytes: Option<usize>,
    parsed: &mmd_anim_format::AccessoryParsedManifest,
) -> serde_json::Value {
    let mesh_material_reemitted = !parsed.mesh_summaries.is_empty();
    let preserved_fields = if mesh_material_reemitted {
        serde_json::json!([
            "format",
            "header",
            "textureReferences",
            "meshSummaries",
            "materials"
        ])
    } else if parsed.format == "vac" {
        serde_json::json!(["format", "header", "textureReferences", "vacSettings"])
    } else {
        serde_json::json!(["format", "header", "textureReferences"])
    };
    json!({
        "status": "ok",
        "mode": mode,
        "format": parsed.format,
        "path": path.to_string_lossy(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "jsonBytes": json_bytes,
        "counts": {
            "meshes": parsed.mesh_count,
            "materials": parsed.material_count,
            "meshVertices": parsed.mesh_summaries.iter().map(|mesh| mesh.vertex_count).sum::<usize>(),
            "meshFaces": parsed.mesh_summaries.iter().map(|mesh| mesh.face_count).sum::<usize>(),
            "meshNormals": parsed.mesh_summaries.iter().map(|mesh| mesh.normals.len()).sum::<usize>(),
            "meshTextureCoordinates": parsed.mesh_summaries.iter().map(|mesh| mesh.texture_coordinates.len()).sum::<usize>(),
            "meshVertexColors": parsed.mesh_summaries.iter().map(|mesh| mesh.vertex_colors.len()).sum::<usize>(),
            "meshMaterialIndices": parsed.mesh_summaries.iter().map(|mesh| mesh.material_indices.len()).sum::<usize>(),
            "textureReferences": parsed.texture_references.len(),
            "diagnostics": parsed.diagnostics.len()
        },
        "metadata": {
            "text": parsed.text,
            "header": parsed.header,
            "exportScope": if mesh_material_reemitted { "text-mesh-material-attributes" } else { "manifest" },
            "meshMaterialReemitted": mesh_material_reemitted,
            "preservedFields": preserved_fields
        }
    })
}

fn ensure_pmx_roundtrip(
    left: &mmd_anim_format::PmxParsedModel,
    right: &mmd_anim_format::PmxParsedModel,
) -> Result<(), Box<dyn std::error::Error>> {
    let left = serde_json::to_value(left)?;
    let right = serde_json::to_value(right)?;
    if left != right {
        return Err("PMX parse/export/parse DTO changed".into());
    }
    Ok(())
}

fn ensure_pmd_roundtrip(
    left: &mmd_anim_format::PmdParsedModel,
    right: &mmd_anim_format::PmdParsedModel,
) -> Result<(), Box<dyn std::error::Error>> {
    let left = serde_json::to_value(left)?;
    let right = serde_json::to_value(right)?;
    if left != right {
        return Err("PMD parse/export/parse DTO changed".into());
    }
    Ok(())
}

fn ensure_accessory_roundtrip(
    expected: &mmd_anim_format::AccessoryParsedManifest,
    actual: &mmd_anim_format::AccessoryParsedManifest,
) -> Result<(), String> {
    if expected.format != actual.format {
        return Err(format!(
            "Accessory roundtrip format changed: expected {}, got {}",
            expected.format, actual.format
        ));
    }
    if expected.header != actual.header {
        return Err(format!(
            "Accessory roundtrip header changed: expected {:?}, got {:?}",
            expected.header, actual.header
        ));
    }
    if expected.text != actual.text {
        return Err(format!(
            "Accessory roundtrip text flag changed: expected {}, got {}",
            expected.text, actual.text
        ));
    }
    if expected.texture_references != actual.texture_references {
        return Err(format!(
            "Accessory roundtrip textureReferences changed: expected {:?}, got {:?}",
            expected.texture_references, actual.texture_references
        ));
    }
    if !expected.mesh_summaries.is_empty() && expected.mesh_summaries != actual.mesh_summaries {
        return Err(format!(
            "Accessory roundtrip meshSummaries changed: {}",
            describe_accessory_mesh_diff(expected, actual)
        ));
    }
    if expected.materials != actual.materials {
        return Err(format!(
            "Accessory roundtrip materials changed: expected {} entries, got {} entries",
            expected.materials.len(),
            actual.materials.len()
        ));
    }
    if expected.vac_settings != actual.vac_settings {
        return Err("Accessory roundtrip vacSettings changed".to_owned());
    }
    Ok(())
}

fn ensure_accessory_json_roundtrip(
    expected: &mmd_anim_format::AccessoryParsedManifest,
    actual: &mmd_anim_format::AccessoryParsedManifest,
) -> Result<(), String> {
    let expected = serde_json::to_value(expected).map_err(|error| {
        format!("Accessory JSON roundtrip expected serialization failed: {error}")
    })?;
    let actual = serde_json::to_value(actual).map_err(|error| {
        format!("Accessory JSON roundtrip actual serialization failed: {error}")
    })?;
    if expected != actual {
        return Err("Accessory JSON DTO changed before export".to_owned());
    }
    Ok(())
}

fn describe_accessory_mesh_diff(
    expected: &mmd_anim_format::AccessoryParsedManifest,
    actual: &mmd_anim_format::AccessoryParsedManifest,
) -> String {
    if expected.mesh_summaries.len() != actual.mesh_summaries.len() {
        return format!(
            "expected {} entries, got {} entries",
            expected.mesh_summaries.len(),
            actual.mesh_summaries.len()
        );
    }
    for (index, (expected_mesh, actual_mesh)) in expected
        .mesh_summaries
        .iter()
        .zip(actual.mesh_summaries.iter())
        .enumerate()
    {
        if expected_mesh.vertex_count != actual_mesh.vertex_count {
            return format!(
                "mesh {index} vertexCount expected {}, got {}",
                expected_mesh.vertex_count, actual_mesh.vertex_count
            );
        }
        if expected_mesh.face_count != actual_mesh.face_count {
            return format!(
                "mesh {index} faceCount expected {}, got {}",
                expected_mesh.face_count, actual_mesh.face_count
            );
        }
        if expected_mesh.positions != actual_mesh.positions {
            return format!(
                "mesh {index} positions expected {} entries, got {} entries",
                expected_mesh.positions.len(),
                actual_mesh.positions.len()
            );
        }
        if expected_mesh.face_indices != actual_mesh.face_indices {
            return format!(
                "mesh {index} faceIndices expected {} entries, got {} entries",
                expected_mesh.face_indices.len(),
                actual_mesh.face_indices.len()
            );
        }
        if expected_mesh.normals != actual_mesh.normals {
            return format!(
                "mesh {index} normals expected {} entries, got {} entries",
                expected_mesh.normals.len(),
                actual_mesh.normals.len()
            );
        }
        if expected_mesh.normal_face_indices != actual_mesh.normal_face_indices {
            return format!(
                "mesh {index} normalFaceIndices expected {} entries, got {} entries",
                expected_mesh.normal_face_indices.len(),
                actual_mesh.normal_face_indices.len()
            );
        }
        if expected_mesh.texture_coordinates != actual_mesh.texture_coordinates {
            return format!(
                "mesh {index} textureCoordinates expected {} entries, got {} entries",
                expected_mesh.texture_coordinates.len(),
                actual_mesh.texture_coordinates.len()
            );
        }
        if expected_mesh.vertex_colors != actual_mesh.vertex_colors {
            return format!(
                "mesh {index} vertexColors expected {} entries, got {} entries",
                expected_mesh.vertex_colors.len(),
                actual_mesh.vertex_colors.len()
            );
        }
        if expected_mesh.material_indices != actual_mesh.material_indices {
            return format!(
                "mesh {index} materialIndices expected {:?}, got {:?}",
                expected_mesh.material_indices, actual_mesh.material_indices
            );
        }
    }
    "unknown mesh summary delta".to_owned()
}

fn ensure_vmd_roundtrip(
    left: &mmd_anim_format::VmdParsedAnimation,
    right: &mmd_anim_format::VmdParsedAnimation,
) -> Result<(), Box<dyn std::error::Error>> {
    if left.metadata.model_name != right.metadata.model_name {
        return Err(format!(
            "VMD metadata.model_name changed: expected={:?} got={:?}",
            left.metadata.model_name, right.metadata.model_name
        )
        .into());
    }
    if left.metadata.max_frame != right.metadata.max_frame {
        return Err(format!(
            "VMD metadata.max_frame changed: expected={} got={}",
            left.metadata.max_frame, right.metadata.max_frame
        )
        .into());
    }
    macro_rules! check_count {
        ($field:ident, $label:expr) => {
            if left.metadata.counts.$field != right.metadata.counts.$field {
                return Err(format!(
                    "VMD metadata.counts.{} changed: expected={} got={}",
                    $label, left.metadata.counts.$field, right.metadata.counts.$field
                )
                .into());
            }
        };
    }
    check_count!(bones, "bones");
    check_count!(morphs, "morphs");
    check_count!(cameras, "cameras");
    check_count!(lights, "lights");
    check_count!(self_shadows, "selfShadows");
    check_count!(properties, "properties");

    for (i, (l, r)) in left.bone_frames.iter().zip(&right.bone_frames).enumerate() {
        if l.bone_name != r.bone_name {
            return Err(format!(
                "VMD bone_frames[{i}] bone_name: expected={:?} got={:?}",
                l.bone_name, r.bone_name
            )
            .into());
        }
        if l.frame != r.frame {
            return Err(format!(
                "VMD bone_frames[{i}] bone={:?} frame: expected={} got={}",
                l.bone_name, l.frame, r.frame
            )
            .into());
        }
        if l.translation != r.translation {
            return Err(format!(
                "VMD bone_frames[{i}] bone={:?} frame={} translation: expected={:?} got={:?}",
                l.bone_name, l.frame, l.translation, r.translation
            )
            .into());
        }
        if l.rotation != r.rotation {
            return Err(format!(
                "VMD bone_frames[{i}] bone={:?} frame={} rotation: expected={:?} got={:?}",
                l.bone_name, l.frame, l.rotation, r.rotation
            )
            .into());
        }
        if l.interpolation != r.interpolation {
            return Err(format!(
                "VMD bone_frames[{i}] bone={:?} frame={} interpolation changed",
                l.bone_name, l.frame
            )
            .into());
        }
    }

    for (i, (l, r)) in left
        .morph_frames
        .iter()
        .zip(&right.morph_frames)
        .enumerate()
    {
        if l.morph_name != r.morph_name {
            return Err(format!(
                "VMD morph_frames[{i}] morph_name: expected={:?} got={:?}",
                l.morph_name, r.morph_name
            )
            .into());
        }
        if l.frame != r.frame {
            return Err(format!(
                "VMD morph_frames[{i}] morph={:?} frame: expected={} got={}",
                l.morph_name, l.frame, r.frame
            )
            .into());
        }
        if l.weight != r.weight {
            return Err(format!(
                "VMD morph_frames[{i}] morph={:?} frame={} weight: expected={} got={}",
                l.morph_name, l.frame, l.weight, r.weight
            )
            .into());
        }
    }

    for (i, (l, r)) in left
        .camera_frames
        .iter()
        .zip(&right.camera_frames)
        .enumerate()
    {
        if l.frame != r.frame {
            return Err(format!(
                "VMD camera_frames[{i}] frame: expected={} got={}",
                l.frame, r.frame
            )
            .into());
        }
        if l.distance != r.distance {
            return Err(format!(
                "VMD camera_frames[{i}] frame={} distance: expected={} got={}",
                l.frame, l.distance, r.distance
            )
            .into());
        }
        if l.position != r.position {
            return Err(format!(
                "VMD camera_frames[{i}] frame={} position: expected={:?} got={:?}",
                l.frame, l.position, r.position
            )
            .into());
        }
        if l.rotation != r.rotation {
            return Err(format!(
                "VMD camera_frames[{i}] frame={} rotation: expected={:?} got={:?}",
                l.frame, l.rotation, r.rotation
            )
            .into());
        }
        if l.interpolation != r.interpolation {
            return Err(format!(
                "VMD camera_frames[{i}] frame={} interpolation changed",
                l.frame
            )
            .into());
        }
        if l.fov != r.fov {
            return Err(format!(
                "VMD camera_frames[{i}] frame={} fov: expected={} got={}",
                l.frame, l.fov, r.fov
            )
            .into());
        }
        if l.perspective != r.perspective {
            return Err(format!(
                "VMD camera_frames[{i}] frame={} perspective: expected={} got={}",
                l.frame, l.perspective, r.perspective
            )
            .into());
        }
    }

    for (i, (l, r)) in left
        .light_frames
        .iter()
        .zip(&right.light_frames)
        .enumerate()
    {
        if l.frame != r.frame {
            return Err(format!(
                "VMD light_frames[{i}] frame: expected={} got={}",
                l.frame, r.frame
            )
            .into());
        }
        if l.color != r.color {
            return Err(format!(
                "VMD light_frames[{i}] frame={} color: expected={:?} got={:?}",
                l.frame, l.color, r.color
            )
            .into());
        }
        if l.direction != r.direction {
            return Err(format!(
                "VMD light_frames[{i}] frame={} direction: expected={:?} got={:?}",
                l.frame, l.direction, r.direction
            )
            .into());
        }
    }

    for (i, (l, r)) in left
        .self_shadow_frames
        .iter()
        .zip(&right.self_shadow_frames)
        .enumerate()
    {
        if l.frame != r.frame {
            return Err(format!(
                "VMD self_shadow_frames[{i}] frame: expected={} got={}",
                l.frame, r.frame
            )
            .into());
        }
        if l.mode != r.mode {
            return Err(format!(
                "VMD self_shadow_frames[{i}] frame={} mode: expected={} got={}",
                l.frame, l.mode, r.mode
            )
            .into());
        }
        if l.distance != r.distance {
            return Err(format!(
                "VMD self_shadow_frames[{i}] frame={} distance: expected={} got={}",
                l.frame, l.distance, r.distance
            )
            .into());
        }
    }

    for (i, (l, r)) in left
        .property_frames
        .iter()
        .zip(&right.property_frames)
        .enumerate()
    {
        if l.frame != r.frame {
            return Err(format!(
                "VMD property_frames[{i}] frame: expected={} got={}",
                l.frame, r.frame
            )
            .into());
        }
        if l.visible != r.visible {
            return Err(format!(
                "VMD property_frames[{i}] frame={} visible: expected={} got={}",
                l.frame, l.visible, r.visible
            )
            .into());
        }
        if l.ik_states.len() != r.ik_states.len() {
            return Err(format!(
                "VMD property_frames[{i}] frame={} ik_states count: expected={} got={}",
                l.frame,
                l.ik_states.len(),
                r.ik_states.len()
            )
            .into());
        }
        for (j, (lk, rk)) in l.ik_states.iter().zip(&r.ik_states).enumerate() {
            if lk.bone_name != rk.bone_name {
                return Err(format!(
                    "VMD property_frames[{i}] frame={} ik_states[{j}] bone_name: expected={:?} got={:?}",
                    l.frame, lk.bone_name, rk.bone_name
                )
                .into());
            }
            if lk.enabled != rk.enabled {
                return Err(format!(
                    "VMD property_frames[{i}] frame={} ik_states[{j}] bone={:?} enabled: expected={} got={}",
                    l.frame, lk.bone_name, lk.enabled, rk.enabled
                )
                .into());
            }
        }
    }
    Ok(())
}

fn ensure_vpd_roundtrip(
    left: &mmd_anim_format::VpdParsedPose,
    right: &mmd_anim_format::VpdParsedPose,
) -> Result<(), Box<dyn std::error::Error>> {
    if left.model_file != right.model_file {
        return Err(format!(
            "VPD model_file changed: expected={:?} got={:?}",
            left.model_file, right.model_file
        )
        .into());
    }
    if left.bone_count != right.bone_count {
        return Err(format!(
            "VPD bone_count changed: expected={} got={}",
            left.bone_count, right.bone_count
        )
        .into());
    }
    for (i, (l, r)) in left.bones.iter().zip(&right.bones).enumerate() {
        if l.name != r.name {
            return Err(format!(
                "VPD bones[{i}] name: expected={:?} got={:?}",
                l.name, r.name
            )
            .into());
        }
        if l.translation != r.translation {
            return Err(format!(
                "VPD bones[{i}] bone={:?} translation: expected={:?} got={:?}",
                l.name, l.translation, r.translation
            )
            .into());
        }
        if l.rotation != r.rotation {
            return Err(format!(
                "VPD bones[{i}] bone={:?} rotation: expected={:?} got={:?}",
                l.name, l.rotation, r.rotation
            )
            .into());
        }
    }
    Ok(())
}

fn compare_numeric_manifest(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    const EPSILON: f64 = 0.003;

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
        .unwrap_or(EPSILON);
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
                compare_camera_numeric_case(
                    case,
                    manifest_dir,
                    case_dir.as_deref(),
                    default_epsilon,
                    &mut camera_stats,
                )?;
            }
            "motion-numeric" | "physics-coarse" => {
                compare_motion_numeric_case(
                    case,
                    manifest_dir,
                    default_epsilon,
                    default_focus_bones.as_deref(),
                    default_motion_eval_frame_offset,
                    &mut motion_stats,
                )?;
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

    let failure_count = numeric_compare_failure_count(&camera_stats, &motion_stats);
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

#[derive(Default)]
struct CameraNumericCompareStats {
    compared_cases: usize,
    compared_frames: usize,
    mismatch_count: usize,
    max_delta: f64,
}

fn compare_camera_numeric_case(
    case: &serde_json::Value,
    manifest_dir: &Path,
    case_dir: Option<&Path>,
    default_epsilon: f64,
    stats: &mut CameraNumericCompareStats,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let name = case
        .get("name")
        .and_then(|value| value.as_str())
        .ok_or("numeric compare case is missing name")?;
    let epsilon = case
        .pointer("/compare/epsilon")
        .and_then(|value| value.as_f64())
        .unwrap_or(default_epsilon);
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

    stats.compared_cases += 1;
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

        stats.compared_frames += 1;
        stats.mismatch_count += compare_camera_scalar(
            name,
            frame,
            "distance",
            actual.distance as f64,
            expected_number(expected, "distance")?,
            epsilon,
            &mut stats.max_delta,
        );
        stats.mismatch_count += compare_camera_vec3(
            name,
            frame,
            "position",
            actual.position,
            expected_array3(expected, "position")?,
            epsilon,
            &mut stats.max_delta,
        );
        stats.mismatch_count += compare_camera_vec3(
            name,
            frame,
            "rotation",
            actual.rotation,
            expected_array3(expected, "rotation")?,
            epsilon,
            &mut stats.max_delta,
        );
        stats.mismatch_count += compare_camera_scalar(
            name,
            frame,
            "fov",
            actual.fov as f64,
            expected_number(expected, "fov")?,
            epsilon,
            &mut stats.max_delta,
        );
        let expected_perspective = expected
            .get("perspective")
            .and_then(|value| value.as_bool())
            .ok_or_else(|| format!("{name} frame {frame} camera.perspective is missing"))?;
        if actual.perspective != expected_perspective {
            stats.mismatch_count += 1;
            eprintln!(
                "camera mismatch case={} frame={} field=perspective actual={} expected={}",
                name, frame, actual.perspective, expected_perspective
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[derive(Default)]
struct MotionNumericCompareStats {
    total_cases: usize,
    compared_cases: usize,
    skipped_unsupported: usize,
    missing: usize,
    import_errors: usize,
    compared_frames: usize,
    compared_bones: usize,
    mismatch_count: usize,
    skipped_targets: HashSet<String>,
    max_abs_error: f32,
    worst: String,
}

fn numeric_compare_failure_count(
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
        let mut targets: Vec<_> = self.skipped_targets.iter().cloned().collect();
        targets.sort();
        targets.join(",")
    }
}

fn compare_motion_numeric_case(
    case: &serde_json::Value,
    manifest_dir: &Path,
    default_epsilon: f64,
    default_focus_bones: Option<&[String]>,
    default_eval_frame_offset: f32,
    stats: &mut MotionNumericCompareStats,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    stats.total_cases += 1;
    if stats.worst.is_empty() {
        stats.worst = String::from("none");
    }
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
    collect_unsupported_targets(case, &mut stats.skipped_targets);

    let model_path = match case
        .pointer("/assets/model")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
    {
        Some(path) => path,
        None => {
            stats.missing += 1;
            eprintln!("missing: {name} assets.model");
            return Ok(ExitCode::SUCCESS);
        }
    };
    let motion_path = match case
        .pointer("/assets/motion")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
    {
        Some(path) => path,
        None => {
            stats.missing += 1;
            eprintln!("missing: {name} assets.motion");
            return Ok(ExitCode::SUCCESS);
        }
    };
    let oracle_path = match case
        .pointer("/oracle/path")
        .and_then(|value| value.as_str())
        .map(|value| resolve_manifest_path(manifest_dir, value))
    {
        Some(path) => path,
        None => {
            stats.missing += 1;
            eprintln!("missing: {name} oracle.path");
            return Ok(ExitCode::SUCCESS);
        }
    };

    if !golden::is_supported_golden_model(&model_path) {
        stats.skipped_unsupported += 1;
        eprintln!("skipped unsupported model: {}", model_path.display());
        return Ok(ExitCode::SUCCESS);
    }
    if !model_path.exists() || !motion_path.exists() || !oracle_path.exists() {
        stats.missing += 1;
        if !model_path.exists() {
            eprintln!("missing: {}", model_path.display());
        }
        if !motion_path.exists() {
            eprintln!("missing: {}", motion_path.display());
        }
        if !oracle_path.exists() {
            eprintln!("missing: {}", oracle_path.display());
        }
        return Ok(ExitCode::SUCCESS);
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
            stats.import_errors += 1;
            eprintln!("import-error: {}: {}", model_path.display(), error);
            return Ok(ExitCode::SUCCESS);
        }
    };
    let vmd_bytes = fs::read(&motion_path)?;
    let vmd = match mmd_anim_format::import_vmd_motion(&vmd_bytes) {
        Ok(vmd) => vmd,
        Err(error) => {
            stats.import_errors += 1;
            eprintln!("import-error: {}: {}", motion_path.display(), error);
            return Ok(ExitCode::SUCCESS);
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
                if abs_error > stats.max_abs_error {
                    stats.max_abs_error = abs_error;
                    stats.worst = format!("{}:{}:{}", name, oracle_frame.frame, oracle_bone.name);
                }
                if abs_error > epsilon {
                    stats.mismatch_count += 1;
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
            stats.compared_bones += 1;
        }
        stats.compared_frames += 1;
    }
    stats.compared_cases += 1;
    Ok(ExitCode::SUCCESS)
}

fn diagnose_numeric_bones(
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

struct DiagnoseNumericBoneOptions {
    eval_frame: f32,
    bone_names: Vec<String>,
}

fn parse_diagnose_numeric_bone_rest(
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

fn motion_case_focus_bones(
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

fn json_string_array(value: &serde_json::Value, pointer: &str) -> Option<Vec<String>> {
    let values = value.pointer(pointer)?.as_array()?;
    let strings: Vec<String> = values
        .iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect();
    (!strings.is_empty()).then_some(strings)
}

fn json_f32(value: &serde_json::Value, pointer: &str) -> Option<f32> {
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

fn resolve_manifest_path(manifest_dir: &Path, value: &str) -> PathBuf {
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

fn compare_camera_vec3(
    case_name: &str,
    frame: f64,
    field: &str,
    actual: [f32; 3],
    expected: [f64; 3],
    epsilon: f64,
    max_delta: &mut f64,
) -> usize {
    let mut mismatches = 0usize;
    for component in 0..3 {
        mismatches += compare_camera_scalar(
            case_name,
            frame,
            &format!("{field}[{component}]"),
            actual[component] as f64,
            expected[component],
            epsilon,
            max_delta,
        );
    }
    mismatches
}

fn compare_camera_scalar(
    case_name: &str,
    frame: f64,
    field: &str,
    actual: f64,
    expected: f64,
    epsilon: f64,
    max_delta: &mut f64,
) -> usize {
    let delta = (actual - expected).abs();
    *max_delta = (*max_delta).max(delta);
    if delta <= epsilon {
        0
    } else {
        eprintln!(
            "camera mismatch case={} frame={} field={} actual={:.9} expected={:.9} delta={:.9}",
            case_name, frame, field, actual, expected, delta
        );
        1
    }
}

fn import_vmd_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let imported = mmd_anim_format::import_vmd_motion(&data)?;
    let property_ik_entries: usize = imported
        .property_ik_frames
        .iter()
        .map(|frame| frame.entries.len())
        .sum();
    println!(
        "VMD runtime import: boneKeys={} morphKeys={} propertyFrames={} propertyIkEntries={}",
        imported.bone_keyframes.len(),
        imported.morph_keyframes.len(),
        imported.property_ik_frames.len(),
        property_ik_entries
    );
    Ok(ExitCode::SUCCESS)
}

fn import_pair_summary(
    pmx_path: &Path,
    vmd_path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&fs::read(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&fs::read(vmd_path)?)?;

    let matched_bone_keys = vmd
        .bone_keyframes
        .iter()
        .filter(|keyframe| {
            pmx.bone_name_to_index
                .contains_key(&keyframe.bone_name_normalized)
        })
        .count();
    let matched_morph_keys = vmd
        .morph_keyframes
        .iter()
        .filter(|(name, _, _)| {
            let normalized = mmd_anim_format::normalize_vmd_name(name);
            pmx.morph_name_to_index.contains_key(&normalized)
        })
        .count();
    let property_ik_entries: usize = vmd
        .property_ik_frames
        .iter()
        .map(|frame| frame.entries.len())
        .sum();
    let matched_property_ik_entries = vmd
        .property_ik_frames
        .iter()
        .flat_map(|frame| frame.entries.iter())
        .filter(|entry| {
            pmx.ik_solver_bone_name_to_index
                .contains_key(&entry.name_normalized)
        })
        .count();

    println!(
        "PMX/VMD runtime import: bones={} append={} fixedAxis={} ik={} vmdBoneKeys={} matchedBoneKeys={} vmdMorphKeys={} matchedMorphKeys={} propertyFrames={} propertyIkEntries={} matchedPropertyIkEntries={}",
        pmx.model.bone_count(),
        pmx.model.append_transforms().len(),
        pmx.model.fixed_axis_count(),
        pmx.model.ik_count(),
        vmd.bone_keyframes.len(),
        matched_bone_keys,
        vmd.morph_keyframes.len(),
        matched_morph_keys,
        vmd.property_ik_frames.len(),
        property_ik_entries,
        matched_property_ik_entries
    );
    Ok(ExitCode::SUCCESS)
}

fn import_pair_clip_summary(
    pmx_path: &Path,
    vmd_path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&fs::read(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&fs::read(vmd_path)?)?;

    let solver_count = pmx.model.ik_count();
    let clip = mmd_anim_format::build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        solver_count,
    );

    let frame_range = clip
        .frame_range()
        .map(|(first, last)| format!("{first}..{last}"))
        .unwrap_or_else(|| "none".to_owned());

    println!(
        "Pair clip built: bones={} append={} fixedAxis={} ik={} clipBoneTracks={} clipMorphTracks={} propertyTrack={} frameRange={}",
        pmx.model.bone_count(),
        pmx.model.append_transforms().len(),
        pmx.model.fixed_axis_count(),
        pmx.model.ik_count(),
        clip.bone_track_count(),
        clip.morph_track_count(),
        clip.has_property_track(),
        frame_range
    );
    Ok(ExitCode::SUCCESS)
}

fn oracle_summary(path: &str) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

fn golden_ik_summary(root: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&fs::read_to_string(&manifest_path)?)?;
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

        let fixture = GoldenIkFixture::from_json_str(&fs::read_to_string(&fixture_path)?)?;
        let oracle_path = resolve_maybe_absolute(&case_root, &fixture.output);
        if !oracle_path.exists() {
            missing.push(oracle_path);
            continue;
        }

        let frames = if fixture.frames.is_empty() {
            case.frames.as_slice()
        } else {
            fixture.frames.as_slice()
        };
        let dump =
            MmdDumperOracleDump::from_jsonl_str(&fs::read_to_string(&oracle_path)?, Some(frames))?;
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

fn golden_parser_summary(root: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let manifest_path = root.join("oracle-batch.json");
    let manifest = GoldenIkBatchManifest::from_json_str(&fs::read_to_string(&manifest_path)?)?;
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
        let fixture = GoldenIkFixture::from_json_str(&fs::read_to_string(&fixture_path)?)?;
        let oracle_path = resolve_maybe_absolute(&case_root, &fixture.output);
        if !oracle_path.exists() {
            missing_files.push(oracle_path);
            continue;
        }

        let parsed = mmd_anim_format::parse_pmx_model(&fs::read(&pmx_path)?)?;
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
        let dump =
            MmdDumperOracleDump::from_jsonl_str(&fs::read_to_string(&oracle_path)?, Some(frames))?;
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

pub(crate) fn resolve_maybe_absolute(root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn import_pair_frame_summary(
    pmx_path: &Path,
    vmd_path: &Path,
    frame: f32,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pmx = mmd_anim_format::import_pmx_runtime(&fs::read(pmx_path)?)?;
    let vmd = mmd_anim_format::import_vmd_motion(&fs::read(vmd_path)?)?;

    let bone_count = pmx.model.bone_count();
    let solver_count = pmx.model.ik_count();
    let morph_count = pmx
        .morph_name_to_index
        .values()
        .map(|index| index.as_usize() + 1)
        .max()
        .unwrap_or(0);

    let clip = mmd_anim_format::build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        solver_count,
    );

    let model = Arc::new(pmx.model);
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);

    runtime.evaluate_clip_frame(&clip, frame);

    let world_matrices = runtime.world_matrices();

    let first_translation = if let Some(m) = world_matrices.first() {
        Vec3A::new(m.w_axis.x, m.w_axis.y, m.w_axis.z)
    } else {
        Vec3A::ZERO
    };

    let checksum = translation_checksum(world_matrices);
    let morph_weights = runtime.morph_weights();
    let nonzero_morphs = morph_weights
        .iter()
        .filter(|weight| weight.abs() > f32::EPSILON)
        .count();
    let morph_checksum = f32_checksum(morph_weights);

    println!(
        "PMX/VMD frame {:.3}: bones={} ik={} clipBoneTracks={} clipMorphTracks={} worldMatrices={} firstTranslation=({:.6},{:.6},{:.6}) translationChecksum={:08x} nonzeroMorphs={} morphChecksum={:08x}",
        frame,
        bone_count,
        solver_count,
        clip.bone_track_count(),
        clip.morph_track_count(),
        world_matrices.len(),
        first_translation.x,
        first_translation.y,
        first_translation.z,
        checksum,
        nonzero_morphs,
        morph_checksum,
    );

    Ok(ExitCode::SUCCESS)
}

#[derive(Debug)]
struct BenchPairConfig {
    pmx_path: PathBuf,
    vmd_path: PathBuf,
    start_frame: f32,
    frame_count: usize,
    step: f32,
    solve_ik: bool,
    ik_options: IkSolveOptions,
}

fn bench_pair(cfg: BenchPairConfig) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let total_start = Instant::now();

    let read_start = Instant::now();
    let pmx_bytes = fs::read(&cfg.pmx_path)?;
    let vmd_bytes = fs::read(&cfg.vmd_path)?;
    let read_elapsed = read_start.elapsed();

    let pmx_start = Instant::now();
    let pmx = mmd_anim_format::import_pmx_runtime(&pmx_bytes)?;
    let pmx_elapsed = pmx_start.elapsed();

    let vmd_start = Instant::now();
    let vmd = mmd_anim_format::import_vmd_motion(&vmd_bytes)?;
    let vmd_elapsed = vmd_start.elapsed();

    let bone_count = pmx.model.bone_count();
    let append_count = pmx.model.append_transforms().len();
    let fixed_axis_count = pmx.model.fixed_axis_count();
    let solver_count = pmx.model.ik_count();
    let ik_solver_summaries = pmx
        .model
        .ik_solvers()
        .iter()
        .enumerate()
        .map(|(index, solver)| {
            let name = pmx
                .bone_names
                .get(solver.ik_bone.as_usize())
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_owned());
            (
                index,
                solver.ik_bone.as_usize(),
                name,
                solver.iteration_count,
                solver.links.len(),
            )
        })
        .collect::<Vec<_>>();
    let morph_count = pmx
        .morph_name_to_index
        .values()
        .map(|index| index.as_usize() + 1)
        .max()
        .unwrap_or(0);

    let clip_start = Instant::now();
    let clip = mmd_anim_format::build_pair_clip(
        &vmd,
        &pmx.bone_name_to_index,
        &pmx.morph_name_to_index,
        &pmx.ik_solver_bone_name_to_index,
        solver_count,
    );
    let clip_elapsed = clip_start.elapsed();

    let model = Arc::new(pmx.model);
    let mut runtime = RuntimeInstance::new_with_counts(model, morph_count, solver_count);
    runtime.reset_ik_runtime_stats();

    let eval_start = Instant::now();
    let mut checksum = 0u32;
    let mut morph_checksum = 0u32;
    for i in 0..cfg.frame_count {
        let frame = cfg.start_frame + cfg.step * i as f32;
        if cfg.solve_ik {
            runtime.evaluate_clip_frame_with_ik_options(&clip, frame, cfg.ik_options);
        } else {
            runtime.evaluate_clip_frame_without_ik(&clip, frame);
        }
        checksum = checksum.rotate_left(1) ^ translation_checksum(runtime.world_matrices());
        morph_checksum = morph_checksum.rotate_left(1) ^ f32_checksum(runtime.morph_weights());
    }
    let eval_elapsed = eval_start.elapsed();
    let total_elapsed = total_start.elapsed();

    let frame_range = clip
        .frame_range()
        .map(|(first, last)| format!("{first}..{last}"))
        .unwrap_or_else(|| "none".to_owned());
    let eval_ms = eval_elapsed.as_secs_f64() * 1000.0;
    let ms_per_frame = eval_ms / cfg.frame_count as f64;
    let fps = cfg.frame_count as f64 / eval_elapsed.as_secs_f64();

    println!(
        "bench-pair: bones={} ik={} ikTolerance={:.8} ikMaxIterationsCap={} append={} fixedAxis={} vmdBoneKeys={} vmdMorphKeys={} clipBoneTracks={} clipMorphTracks={} propertyTrack={} clipFrameRange={} frames={} startFrame={:.3} step={:.3} readMs={:.3} pmxImportMs={:.3} vmdImportMs={:.3} clipBuildMs={:.3} evalMs={:.3} msPerFrame={:.6} fps={:.1} totalMs={:.3} checksum={:08x} morphChecksum={:08x}",
        bone_count,
        if cfg.solve_ik {
            solver_count.to_string()
        } else {
            "disabled".to_owned()
        },
        cfg.ik_options.tolerance,
        cfg.ik_options
            .max_iterations_cap
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_owned()),
        append_count,
        fixed_axis_count,
        vmd.bone_keyframes.len(),
        vmd.morph_keyframes.len(),
        clip.bone_track_count(),
        clip.morph_track_count(),
        clip.has_property_track(),
        frame_range,
        cfg.frame_count,
        cfg.start_frame,
        cfg.step,
        read_elapsed.as_secs_f64() * 1000.0,
        pmx_elapsed.as_secs_f64() * 1000.0,
        vmd_elapsed.as_secs_f64() * 1000.0,
        clip_elapsed.as_secs_f64() * 1000.0,
        eval_ms,
        ms_per_frame,
        fps,
        total_elapsed.as_secs_f64() * 1000.0,
        checksum,
        morph_checksum,
    );
    if cfg.solve_ik {
        let stats = runtime.ik_runtime_stats();
        let total_evaluations = stats
            .iter()
            .map(|stats| stats.solver_evaluations)
            .sum::<u64>();
        let configured_iterations = stats
            .iter()
            .map(|stats| stats.configured_iterations)
            .sum::<u64>();
        let executed_iterations = stats
            .iter()
            .map(|stats| stats.executed_iterations)
            .sum::<u64>();
        let skipped_iterations = configured_iterations.saturating_sub(executed_iterations);
        let tolerance_precheck_breaks = stats
            .iter()
            .map(|stats| stats.tolerance_precheck_breaks)
            .sum::<u64>();
        let tolerance_post_iteration_breaks = stats
            .iter()
            .map(|stats| stats.tolerance_post_iteration_breaks)
            .sum::<u64>();
        let rollback_breaks = stats.iter().map(|stats| stats.rollback_breaks).sum::<u64>();
        let max_iteration_exhaustions = stats
            .iter()
            .map(|stats| stats.max_iteration_exhaustions)
            .sum::<u64>();
        let link_steps = stats.iter().map(|stats| stats.link_steps).sum::<u64>();
        let skip_ratio = if configured_iterations == 0 {
            0.0
        } else {
            skipped_iterations as f64 / configured_iterations as f64
        };
        println!(
            "bench-pair-ik-stats: solverEvaluations={} configuredIterations={} executedIterations={} skippedIterations={} skippedRatio={:.3} tolerancePrecheckBreaks={} tolerancePostIterationBreaks={} rollbackBreaks={} maxIterationExhaustions={} linkSteps={}",
            total_evaluations,
            configured_iterations,
            executed_iterations,
            skipped_iterations,
            skip_ratio,
            tolerance_precheck_breaks,
            tolerance_post_iteration_breaks,
            rollback_breaks,
            max_iteration_exhaustions,
            link_steps,
        );

        let mut ranked = stats
            .iter()
            .enumerate()
            .map(|(index, stats)| (index, *stats))
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(_, stats)| {
            std::cmp::Reverse((stats.executed_iterations, stats.configured_iterations))
        });
        for (index, stats) in ranked.into_iter().take(8) {
            let (solver_index, bone_index, name, max_iterations, links) =
                &ik_solver_summaries[index];
            let skipped = stats
                .configured_iterations
                .saturating_sub(stats.executed_iterations);
            let avg_final_distance = if stats.solver_evaluations == 0 {
                0.0
            } else {
                stats.final_distance_sum / stats.solver_evaluations as f64
            };
            let avg_exhausted_final_distance = if stats.max_iteration_exhaustions == 0 {
                0.0
            } else {
                stats.exhausted_final_distance_sum / stats.max_iteration_exhaustions as f64
            };
            println!(
                "bench-pair-ik-solver: solver={} bone={} name={} maxIterations={} links={} evaluations={} configuredIterations={} executedIterations={} skippedIterations={} precheckBreaks={} postBreaks={} rollbackBreaks={} exhausted={} avgFinalDistance={:.8} maxFinalDistance={:.8} avgExhaustedFinalDistance={:.8} maxExhaustedFinalDistance={:.8}",
                solver_index,
                bone_index,
                name,
                max_iterations,
                links,
                stats.solver_evaluations,
                stats.configured_iterations,
                stats.executed_iterations,
                skipped,
                stats.tolerance_precheck_breaks,
                stats.tolerance_post_iteration_breaks,
                stats.rollback_breaks,
                stats.max_iteration_exhaustions,
                avg_final_distance,
                stats.final_distance_max,
                avg_exhausted_final_distance,
                stats.exhausted_final_distance_max,
            );
        }
    }

    Ok(ExitCode::SUCCESS)
}

#[derive(Debug)]
struct BenchSyntheticConfig {
    models: usize,
    bones: usize,
    frames: u32,
    use_json: bool,
}

fn bench_synthetic(cfg: BenchSyntheticConfig) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let model_count = cfg.models;
    let bone_count = cfg.bones;
    let frame_count = cfg.frames;
    let use_json = cfg.use_json;
    if model_count == 0 || bone_count == 0 || frame_count == 0 {
        return Err("models, bones, and frames must be positive".into());
    }

    // Build chain of bones: bone 0 = root, each child parented to previous
    let mut bones = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        let parent = if i == 0 {
            None
        } else {
            Some(BoneIndex(i as u32 - 1))
        };
        bones.push(BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0)));
    }
    let model = Arc::new(ModelArena::new(bones)?);

    // Build clip with two keyframes per bone (linear interpolation)
    let mut bone_tracks = Vec::with_capacity(bone_count);
    for i in 0..bone_count {
        let angle = 0.1 + (i as f32) * 0.02;
        let track = MovableBoneTrack::from_keyframes(vec![
            MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
            MovableBoneKeyframe::new(
                30,
                Vec3A::new(1.0, 0.0, 0.0),
                Quat::from_axis_angle(Vec3A::Y.into(), angle),
            ),
        ]);
        bone_tracks.push(BoneAnimationBinding {
            bone: BoneIndex(i as u32),
            track,
        });
    }
    let clip = AnimationClip::new(bone_tracks);

    // Create model_count independent RuntimeInstances
    let mut runtimes: Vec<RuntimeInstance> = (0..model_count)
        .map(|_| RuntimeInstance::new(Arc::clone(&model)))
        .collect();
    let mut matrix_scratch = vec![0.0f32; bone_count * 16];

    // Warm-up: one call to ensure any lazy init is done
    for runtime in &mut runtimes {
        runtime.evaluate_clip_frame(&clip, 0.0);
        copy_world_matrices_to_f32(runtime.world_matrices(), &mut matrix_scratch);
    }

    // Timed loop
    let mut rolling_checksum: u32 = 0;
    let start = Instant::now();
    for frame in 0..frame_count {
        let frame_f = frame as f32;
        for runtime in &mut runtimes {
            runtime.evaluate_clip_frame(&clip, frame_f);
            copy_world_matrices_to_f32(runtime.world_matrices(), &mut matrix_scratch);
            rolling_checksum = rolling_checksum.wrapping_add(f32_checksum(&matrix_scratch));
        }
    }
    let elapsed = start.elapsed();

    // Accumulate checksum from final state (prevents dead-code elimination)
    let mut final_checksum: u32 = 0;
    for runtime in &runtimes {
        final_checksum =
            final_checksum.wrapping_add(translation_checksum(runtime.world_matrices()));
    }
    final_checksum ^= rolling_checksum;

    let total_frames = frame_count as u64 * model_count as u64;
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let fps = total_frames as f64 / elapsed.as_secs_f64();

    if use_json {
        println!(
            r#"{{"models":{},"bones":{},"frames":{},"elapsedMs":{:.3},"totalFrames":{},"fps":{:.1},"checksum":"{:08x}"}}"#,
            model_count, bone_count, frame_count, elapsed_ms, total_frames, fps, final_checksum
        );
    } else {
        println!(
            "bench-synthetic: models={} bones={} frames={} elapsedMs={:.3} totalFrames={} fps={:.1} checksum={:08x}",
            model_count, bone_count, frame_count, elapsed_ms, total_frames, fps, final_checksum
        );
    }

    Ok(ExitCode::SUCCESS)
}

fn parse_bench_synthetic_args(
    args: &mut impl Iterator<Item = String>,
) -> Result<BenchSyntheticConfig, Box<dyn std::error::Error>> {
    let raw: Vec<String> = args.collect();
    let mut use_json = false;
    let mut positional = Vec::new();

    for token in &raw {
        if token == "--json" {
            use_json = true;
        } else if token.starts_with("--") {
            return Err(format!("unknown flag: {token}").into());
        } else {
            positional.push(token.clone());
        }
    }

    let mut pos_iter = positional.into_iter();
    let models = optional_positive_usize_arg(&mut pos_iter, 1, "models")?;
    let bones = optional_positive_usize_arg(&mut pos_iter, 32, "bones")?;
    let frames = optional_positive_u32_arg(&mut pos_iter, 1000, "frames")?;
    if let Some(extra) = pos_iter.next() {
        return Err(format!("unexpected extra argument: {extra}").into());
    }

    Ok(BenchSyntheticConfig {
        models,
        bones,
        frames,
        use_json,
    })
}

fn parse_bench_pair_args(
    args: &mut impl Iterator<Item = String>,
) -> Result<BenchPairConfig, Box<dyn std::error::Error>> {
    let raw: Vec<String> = args.collect();
    let mut solve_ik = true;
    let mut ik_tolerance = IkSolveOptions::default().tolerance;
    let mut ik_max_iterations_cap = None;
    let mut positional = Vec::new();

    let mut raw_iter = raw.into_iter();
    while let Some(token) = raw_iter.next() {
        match token.as_str() {
            "--no-ik" => solve_ik = false,
            "--ik-tolerance" => {
                let value = raw_iter.next().ok_or("missing value for --ik-tolerance")?;
                ik_tolerance = parse_finite_f32(&value, "ik-tolerance")?;
                if ik_tolerance < 0.0 {
                    return Err("ik-tolerance must be non-negative".into());
                }
            }
            "--ik-max-iterations-cap" => {
                let value = raw_iter
                    .next()
                    .ok_or("missing value for --ik-max-iterations-cap")?;
                ik_max_iterations_cap = Some(parse_positive_u32(&value, "ik-max-iterations-cap")?);
            }
            _ if token.starts_with("--") => {
                return Err(format!("unknown flag: {token}").into());
            }
            _ => positional.push(token),
        }
    }

    let mut pos_iter = positional.into_iter();
    let pmx_path = PathBuf::from(pos_iter.next().ok_or(BENCH_PAIR_USAGE)?);
    let vmd_path = PathBuf::from(pos_iter.next().ok_or(BENCH_PAIR_USAGE)?);
    let start_frame = optional_f32_parse_arg(&mut pos_iter, 0.0, "start-frame")?;
    let frame_count = optional_positive_usize_arg(&mut pos_iter, 1000, "frame-count")?;
    let step = optional_f32_parse_arg(&mut pos_iter, 1.0, "step")?;
    if step <= 0.0 {
        return Err("step must be positive".into());
    }
    if let Some(extra) = pos_iter.next() {
        return Err(format!("unexpected extra argument: {extra}").into());
    }

    Ok(BenchPairConfig {
        pmx_path,
        vmd_path,
        start_frame,
        frame_count,
        step,
        solve_ik,
        ik_options: IkSolveOptions {
            tolerance: ik_tolerance,
            max_iterations_cap: ik_max_iterations_cap,
        },
    })
}

fn optional_positive_usize_arg(
    args: &mut impl Iterator<Item = String>,
    default: usize,
    label: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let Some(value) = args.next() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be positive").into());
    }
    Ok(parsed)
}

fn optional_positive_u32_arg(
    args: &mut impl Iterator<Item = String>,
    default: u32,
    label: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let Some(value) = args.next() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be positive").into());
    }
    Ok(parsed)
}

fn parse_positive_u32(value: &str, label: &str) -> Result<u32, Box<dyn std::error::Error>> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if parsed == 0 {
        return Err(format!("{label} must be positive").into());
    }
    Ok(parsed)
}

fn optional_f32_parse_arg(
    args: &mut impl Iterator<Item = String>,
    default: f32,
    label: &str,
) -> Result<f32, Box<dyn std::error::Error>> {
    let Some(value) = args.next() else {
        return Ok(default);
    };
    let parsed = value
        .parse::<f32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if !parsed.is_finite() {
        return Err(format!("{label} must be finite").into());
    }
    Ok(parsed)
}

fn parse_finite_f32(value: &str, label: &str) -> Result<f32, Box<dyn std::error::Error>> {
    let parsed = value
        .parse::<f32>()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    if !parsed.is_finite() {
        return Err(format!("{label} must be finite").into());
    }
    Ok(parsed)
}

fn copy_world_matrices_to_f32(matrices: &[glam::Mat4], out: &mut [f32]) {
    debug_assert!(out.len() >= matrices.len() * 16);
    for (index, matrix) in matrices.iter().enumerate() {
        let offset = index * 16;
        out[offset..offset + 16].copy_from_slice(&matrix.to_cols_array());
    }
}

fn f32_checksum(values: &[f32]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for value in values {
        hash ^= value.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

fn translation_checksum(matrices: &[glam::Mat4]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for m in matrices {
        hash ^= m.w_axis.x.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= m.w_axis.y.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= m.w_axis.z.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("mmd-anim-cli-{name}-{nanos}"))
    }

    #[test]
    fn test_synthetic_model_bone_count() {
        let bones = (0..8)
            .map(|i| {
                let parent = if i == 0 {
                    None
                } else {
                    Some(BoneIndex(i as u32 - 1))
                };
                BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0))
            })
            .collect();
        let model = ModelArena::new(bones).unwrap();
        assert_eq!(model.bone_count(), 8);
    }

    #[test]
    fn test_synthetic_clip_track_count() {
        let tracks: Vec<_> = (0..4)
            .map(|i| {
                let track = MovableBoneTrack::from_keyframes(vec![
                    MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                    MovableBoneKeyframe::new(
                        30,
                        Vec3A::new(1.0, 0.0, 0.0),
                        Quat::from_axis_angle(Vec3A::Y.into(), 0.5),
                    ),
                ]);
                BoneAnimationBinding {
                    bone: BoneIndex(i as u32),
                    track,
                }
            })
            .collect();
        let clip = AnimationClip::new(tracks);
        assert_eq!(clip.bone_track_count(), 4);
    }

    #[test]
    fn test_bench_checksum_deterministic() {
        let bones = (0..4)
            .map(|i| {
                let parent = if i == 0 {
                    None
                } else {
                    Some(BoneIndex(i as u32 - 1))
                };
                BoneInit::new(parent, Vec3A::new(0.0, i as f32 * 5.0, 0.0))
            })
            .collect();
        let model = Arc::new(ModelArena::new(bones).unwrap());
        let tracks: Vec<_> = (0..4)
            .map(|i| {
                let track = MovableBoneTrack::from_keyframes(vec![
                    MovableBoneKeyframe::new(0, Vec3A::ZERO, Quat::IDENTITY),
                    MovableBoneKeyframe::new(
                        30,
                        Vec3A::new(1.0, 0.0, 0.0),
                        Quat::from_axis_angle(Vec3A::Y.into(), 0.5),
                    ),
                ]);
                BoneAnimationBinding {
                    bone: BoneIndex(i as u32),
                    track,
                }
            })
            .collect();
        let clip = AnimationClip::new(tracks);

        let mut r1 = RuntimeInstance::new(Arc::clone(&model));
        let mut r2 = RuntimeInstance::new(model);
        r1.evaluate_clip_frame(&clip, 15.0);
        r2.evaluate_clip_frame(&clip, 15.0);
        assert_eq!(
            translation_checksum(r1.world_matrices()),
            translation_checksum(r2.world_matrices()),
        );
    }

    #[test]
    fn bench_synthetic_args_use_defaults() {
        let mut args = Vec::<String>::new().into_iter();
        let cfg = parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 1);
        assert_eq!(cfg.bones, 32);
        assert_eq!(cfg.frames, 1000);
        assert!(!cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_json_flag() {
        let mut args = vec!["--json".to_owned()].into_iter();
        let cfg = parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 1);
        assert_eq!(cfg.bones, 32);
        assert_eq!(cfg.frames, 1000);
        assert!(cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_json_with_positional() {
        let mut args = vec![
            "4".to_owned(),
            "--json".to_owned(),
            "16".to_owned(),
            "50".to_owned(),
        ]
        .into_iter();
        let cfg = parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 4);
        assert_eq!(cfg.bones, 16);
        assert_eq!(cfg.frames, 50);
        assert!(cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_json_after_positional() {
        let mut args = vec![
            "2".to_owned(),
            "8".to_owned(),
            "200".to_owned(),
            "--json".to_owned(),
        ]
        .into_iter();
        let cfg = parse_bench_synthetic_args(&mut args).unwrap();
        assert_eq!(cfg.models, 2);
        assert_eq!(cfg.bones, 8);
        assert_eq!(cfg.frames, 200);
        assert!(cfg.use_json);
    }

    #[test]
    fn bench_synthetic_args_reject_unknown_flag() {
        let mut args = vec!["--unknown".to_owned()].into_iter();
        let error = parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("unknown flag"));
    }

    #[test]
    fn bench_synthetic_args_reject_invalid_models() {
        let mut args = vec!["nope".to_owned()].into_iter();
        let error = parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("invalid models"));
    }

    #[test]
    fn bench_synthetic_args_reject_zero_models() {
        let mut args = vec!["0".to_owned()].into_iter();
        let error = parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("models must be positive"));
    }

    #[test]
    fn bench_synthetic_args_reject_extra_values() {
        let mut args = vec![
            "1".to_owned(),
            "8".to_owned(),
            "100".to_owned(),
            "extra".to_owned(),
        ]
        .into_iter();
        let error = parse_bench_synthetic_args(&mut args).unwrap_err();
        assert!(error.to_string().contains("unexpected extra argument"));
    }

    #[test]
    fn compare_numeric_mixed_manifest_dispatches_by_case_kind() {
        let temp = unique_test_dir("compare-numeric-mixed");
        fs::create_dir_all(&temp).unwrap();
        fs::write(
            temp.join("camera.vmd"),
            include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd"),
        )
        .unwrap();
        fs::write(
            temp.join("camera-oracle.json"),
            r#"{
                "frames": [
                    {
                        "frame": 0,
                        "camera": {
                            "distance": -30.5,
                            "position": [1.0, 2.0, 3.0],
                            "rotation": [0.1, -0.2, 0.3],
                            "fov": 35,
                            "perspective": true
                        }
                    }
                ]
            }"#,
        )
        .unwrap();
        fs::write(
            temp.join("manifest.json"),
            r#"{
                "cases": [
                    {
                        "name": "camera",
                        "kind": "camera-vmd",
                        "assets": { "cameraMotion": "camera.vmd" },
                        "oracle": { "path": "camera-oracle.json" },
                        "compare": { "epsilon": 0.003 }
                    },
                    {
                        "name": "motion",
                        "kind": "motion-numeric",
                        "assets": {
                            "model": "missing.pmx",
                            "motion": "missing.vmd"
                        },
                        "oracle": { "path": "missing.json" },
                        "frames": [0],
                        "compare": { "targets": ["bones"], "epsilon": 0.003 }
                    }
                ]
            }"#,
        )
        .unwrap();

        let error = compare_numeric_manifest(&temp.join("manifest.json")).unwrap_err();
        let error = error.to_string();
        assert!(error.contains("cameraMismatches=0"));
        assert!(error.contains("motionMissing=1"));
        assert!(!error.contains("unsupported kind"));

        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn numeric_compare_failure_count_includes_motion_mismatches() {
        let camera = CameraNumericCompareStats::default();
        let motion = MotionNumericCompareStats {
            mismatch_count: 1,
            ..MotionNumericCompareStats::default()
        };

        assert_eq!(numeric_compare_failure_count(&camera, &motion), 1);
    }

    #[test]
    fn motion_case_focus_bones_prefers_case_metadata_focus() {
        let case = serde_json::json!({
            "metadata": {
                "focus": {
                    "bones": ["右袖", "左袖"]
                }
            }
        });
        let defaults = vec!["左ひざ".to_owned()];

        assert_eq!(
            motion_case_focus_bones(&case, Some(&defaults)),
            vec!["右袖".to_owned(), "左袖".to_owned()]
        );
    }

    #[test]
    fn motion_case_focus_bones_uses_default_focus() {
        let case = serde_json::json!({});
        let defaults = vec!["右腕".to_owned(), "左腕".to_owned()];

        assert_eq!(motion_case_focus_bones(&case, Some(&defaults)), defaults);
    }

    #[test]
    fn json_f32_reads_nested_number() {
        let value = serde_json::json!({
            "compare": {
                "evalFrameOffset": 1.25
            }
        });

        assert_eq!(json_f32(&value, "/compare/evalFrameOffset"), Some(1.25));
    }

    #[test]
    fn vmd_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::VmdParsedAnimation {
            kind: "vmd",
            metadata: mmd_anim_format::vmd::VmdParsedMetadata {
                format: "vmd",
                model_name: "miku".to_owned(),
                model_name_bytes: Vec::new(),
                counts: mmd_anim_format::vmd::VmdParsedCounts {
                    bones: 1,
                    morphs: 2,
                    cameras: 3,
                    lights: 4,
                    self_shadows: 5,
                    properties: 6,
                },
                max_frame: 120,
            },
            bone_frames: Vec::new(),
            morph_frames: Vec::new(),
            camera_frames: Vec::new(),
            light_frames: Vec::new(),
            self_shadow_frames: Vec::new(),
            property_frames: Vec::new(),
        };
        let value = vmd_roundtrip_json(
            Path::new("motion.vmd"),
            "parse-json-export-parse",
            10,
            20,
            Some(30),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "vmd");
        assert_eq!(value["mode"], "parse-json-export-parse");
        assert_eq!(value["bytesIn"], 10);
        assert_eq!(value["bytesOut"], 20);
        assert_eq!(value["jsonBytes"], 30);
        assert_eq!(value["counts"]["boneFrames"], 1);
        assert_eq!(value["counts"]["propertyFrames"], 6);
        assert_eq!(value["maxFrame"], 120);
    }

    #[test]
    fn vpd_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::VpdParsedPose {
            format: "vpd",
            model_file: "model.pmx".to_owned(),
            bone_count: 2,
            bones: Vec::new(),
            diagnostics: Vec::new(),
        };
        let value = vpd_roundtrip_json(
            Path::new("pose.vpd"),
            "parse-export-parse",
            11,
            22,
            None,
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "vpd");
        assert_eq!(value["mode"], "parse-export-parse");
        assert_eq!(value["bytesIn"], 11);
        assert_eq!(value["bytesOut"], 22);
        assert!(value["jsonBytes"].is_null());
        assert_eq!(value["counts"]["bones"], 2);
    }

    #[test]
    fn accessory_roundtrip_json_reports_text_mesh_material_export_scope() {
        let parsed = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 100,
            text: true,
            header: "xof 0303txt 0032".to_owned(),
            mesh_count: 1,
            material_count: 1,
            mesh_summaries: vec![mmd_anim_format::xfile::AccessoryMeshSummary {
                vertex_count: 3,
                face_count: 1,
                positions: vec![[0.0, 0.0, 0.0]],
                face_indices: vec![vec![0, 1, 2]],
                normals: Vec::new(),
                normal_face_indices: Vec::new(),
                texture_coordinates: vec![[0.0, 0.0]],
                vertex_colors: vec![mmd_anim_format::xfile::AccessoryVertexColor {
                    vertex_index: 2,
                    color: [1.0, 0.5, 0.25, 1.0],
                }],
                material_indices: vec![0],
                material_start_index: 0,
                material_count: 1,
            }],
            materials: vec![mmd_anim_format::xfile::AccessoryMaterial {
                name: Some("mat".to_owned()),
                face_color: Some([1.0, 1.0, 1.0, 1.0]),
                power: Some(5.0),
                specular_color: Some([0.0, 0.0, 0.0]),
                emissive_color: Some([0.0, 0.0, 0.0]),
                texture_references: vec!["tex.png".to_owned()],
            }],
            vac_settings: None,
            texture_references: vec!["tex.png".to_owned()],
            diagnostics: Vec::new(),
        };
        let value = accessory_roundtrip_json(
            Path::new("stage.x"),
            "parse-json-export-parse",
            100,
            50,
            Some(200),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "x");
        assert_eq!(value["counts"]["meshes"], 1);
        assert_eq!(value["counts"]["materials"], 1);
        assert_eq!(value["counts"]["meshVertices"], 3);
        assert_eq!(value["counts"]["meshFaces"], 1);
        assert_eq!(value["counts"]["meshNormals"], 0);
        assert_eq!(value["counts"]["meshTextureCoordinates"], 1);
        assert_eq!(value["counts"]["meshVertexColors"], 1);
        assert_eq!(value["counts"]["meshMaterialIndices"], 1);
        assert_eq!(
            value["metadata"]["exportScope"],
            "text-mesh-material-attributes"
        );
        assert_eq!(value["metadata"]["meshMaterialReemitted"], true);
        assert_eq!(
            value["metadata"]["preservedFields"],
            serde_json::json!([
                "format",
                "header",
                "textureReferences",
                "meshSummaries",
                "materials"
            ])
        );
    }

    #[test]
    fn ensure_accessory_roundtrip_rejects_text_flag_changes() {
        let expected = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 16,
            text: false,
            header: "xof 0303bin 0032".to_owned(),
            mesh_count: 0,
            material_count: 0,
            mesh_summaries: Vec::new(),
            materials: Vec::new(),
            vac_settings: None,
            texture_references: Vec::new(),
            diagnostics: Vec::new(),
        };
        let mut actual = expected.clone();
        actual.text = true;

        let error = ensure_accessory_roundtrip(&expected, &actual).unwrap_err();
        assert!(error.contains("text flag changed"));
    }

    #[test]
    fn ensure_accessory_roundtrip_accepts_multi_mesh_material_ownership() {
        let expected = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 100,
            text: true,
            header: "xof 0303txt 0032".to_owned(),
            mesh_count: 2,
            material_count: 2,
            mesh_summaries: vec![
                mmd_anim_format::xfile::AccessoryMeshSummary {
                    vertex_count: 3,
                    face_count: 1,
                    positions: vec![[0.0, 0.0, 0.0]],
                    face_indices: vec![vec![0, 1, 2]],
                    normals: Vec::new(),
                    normal_face_indices: Vec::new(),
                    texture_coordinates: Vec::new(),
                    vertex_colors: Vec::new(),
                    material_indices: vec![0],
                    material_start_index: 0,
                    material_count: 1,
                },
                mmd_anim_format::xfile::AccessoryMeshSummary {
                    vertex_count: 3,
                    face_count: 1,
                    positions: vec![[0.0, 0.0, 1.0]],
                    face_indices: vec![vec![0, 2, 1]],
                    normals: Vec::new(),
                    normal_face_indices: Vec::new(),
                    texture_coordinates: Vec::new(),
                    vertex_colors: Vec::new(),
                    material_indices: vec![0],
                    material_start_index: 1,
                    material_count: 1,
                },
            ],
            materials: vec![
                mmd_anim_format::xfile::AccessoryMaterial {
                    name: Some("mat0".to_owned()),
                    face_color: Some([1.0, 1.0, 1.0, 1.0]),
                    power: Some(5.0),
                    specular_color: Some([0.0, 0.0, 0.0]),
                    emissive_color: Some([0.0, 0.0, 0.0]),
                    texture_references: Vec::new(),
                },
                mmd_anim_format::xfile::AccessoryMaterial {
                    name: Some("mat1".to_owned()),
                    face_color: Some([0.5, 0.5, 0.5, 1.0]),
                    power: Some(2.0),
                    specular_color: Some([0.0, 0.0, 0.0]),
                    emissive_color: Some([0.0, 0.0, 0.0]),
                    texture_references: Vec::new(),
                },
            ],
            vac_settings: None,
            texture_references: Vec::new(),
            diagnostics: Vec::new(),
        };
        let actual = expected.clone();

        ensure_accessory_roundtrip(&expected, &actual).unwrap();
    }

    #[test]
    fn ensure_accessory_json_roundtrip_rejects_dto_changes() {
        let expected = mmd_anim_format::AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 16,
            text: true,
            header: "xof 0303txt 0032".to_owned(),
            mesh_count: 0,
            material_count: 0,
            mesh_summaries: Vec::new(),
            materials: Vec::new(),
            vac_settings: None,
            texture_references: vec!["tex.png".to_owned()],
            diagnostics: Vec::new(),
        };
        let mut actual = expected.clone();
        actual.texture_references.clear();

        let error = ensure_accessory_json_roundtrip(&expected, &actual).unwrap_err();
        assert_eq!(error, "Accessory JSON DTO changed before export");
    }

    #[test]
    fn pmd_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::PmdParsedModel {
            metadata: mmd_anim_format::pmd::PmdParsedMetadata {
                format: "pmd".to_owned(),
                version: 1.0,
                encoding: "shift-jis".to_owned(),
                name: "model".to_owned(),
                name_bytes: Vec::new(),
                english_name: String::new(),
                english_name_bytes: Vec::new(),
                comment: String::new(),
                comment_bytes: Vec::new(),
                english_comment: String::new(),
                english_comment_bytes: Vec::new(),
                counts: mmd_anim_format::pmd::PmdParsedCounts {
                    vertices: 1,
                    faces: 2,
                    materials: 3,
                    bones: 4,
                    ik: 5,
                    morphs: 6,
                    display_frames: 7,
                    rigid_bodies: 8,
                    joints: 9,
                },
            },
            geometry: mmd_anim_format::pmd::PmdParsedGeometry {
                vertices: Vec::new(),
                indices: Vec::new(),
            },
            materials: Vec::new(),
            toon_textures: Vec::new(),
            toon_texture_bytes: Vec::new(),
            skeleton: mmd_anim_format::pmd::PmdParsedSkeleton {
                bones: Vec::new(),
                ik: Vec::new(),
            },
            morphs: Vec::new(),
            display_frames: Vec::new(),
            rigid_bodies: Vec::new(),
            joints: Vec::new(),
            diagnostics: Vec::new(),
        };
        let value = pmd_roundtrip_json(
            Path::new("model.pmd"),
            "parse-json-export-parse",
            10,
            20,
            Some(30),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "pmd");
        assert_eq!(value["mode"], "parse-json-export-parse");
        assert_eq!(value["bytesIn"], 10);
        assert_eq!(value["bytesOut"], 20);
        assert_eq!(value["jsonBytes"], 30);
        assert_eq!(value["counts"]["vertices"], 1);
        assert_eq!(value["counts"]["ik"], 5);
        assert_eq!(value["counts"]["joints"], 9);
    }

    #[test]
    fn pmx_roundtrip_json_reports_machine_readable_counts() {
        let parsed = mmd_anim_format::PmxParsedModel {
            metadata: mmd_anim_format::pmx::PmxParsedMetadata {
                format: "pmx".to_owned(),
                version: 2.0,
                encoding: "utf-8".to_owned(),
                name: "model".to_owned(),
                english_name: String::new(),
                comment: String::new(),
                english_comment: String::new(),
                counts: mmd_anim_format::pmx::PmxParsedCounts {
                    vertices: 1,
                    faces: 2,
                    materials: 3,
                    bones: 4,
                    morphs: 5,
                    display_frames: 6,
                    rigid_bodies: 7,
                    joints: 8,
                    soft_bodies: 9,
                },
                index_sizes: mmd_anim_format::pmx::PmxParsedIndexSizes {
                    vertex: 4,
                    texture: 1,
                    material: 1,
                    bone: 2,
                    morph: 1,
                    rigid_body: 1,
                },
                additional_uv_count: 0,
            },
            geometry: mmd_anim_format::pmx::PmxParsedGeometry {
                positions: Vec::new(),
                normals: Vec::new(),
                uvs: Vec::new(),
                additional_uvs: Vec::new(),
                indices: Vec::new(),
                skin_indices: Vec::new(),
                skin_weights: Vec::new(),
                edge_scale: Vec::new(),
                material_groups: Vec::new(),
                sdef: mmd_anim_format::pmx::PmxParsedSdef::default(),
                qdef: mmd_anim_format::pmx::PmxParsedQdef::default(),
            },
            materials: Vec::new(),
            skeleton: mmd_anim_format::pmx::PmxParsedSkeleton { bones: Vec::new() },
            morphs: Vec::new(),
            display_frames: Vec::new(),
            rigid_bodies: Vec::new(),
            joints: Vec::new(),
            soft_bodies: Vec::new(),
            diagnostics: Vec::new(),
        };
        let value = pmx_roundtrip_json(
            Path::new("model.pmx"),
            "parse-json-export-parse",
            10,
            20,
            Some(30),
            &parsed,
        );

        assert_eq!(value["status"], "ok");
        assert_eq!(value["format"], "pmx");
        assert_eq!(value["mode"], "parse-json-export-parse");
        assert_eq!(value["bytesIn"], 10);
        assert_eq!(value["bytesOut"], 20);
        assert_eq!(value["jsonBytes"], 30);
        assert_eq!(value["metadata"]["version"], 2.0);
        assert_eq!(value["metadata"]["encoding"], "utf-8");
        assert_eq!(value["metadata"]["additionalUvCount"], 0);
        assert_eq!(value["metadata"]["indexSizes"]["vertex"], 4);
        assert_eq!(value["metadata"]["indexSizes"]["bone"], 2);
        assert_eq!(value["counts"]["vertices"], 1);
        assert_eq!(value["counts"]["softBodies"], 9);
    }
}
