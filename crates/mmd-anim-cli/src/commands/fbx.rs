use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use serde_json::json;

use crate::{import_failure_error, parse_failure_error, read_file, write_file};

pub(crate) struct ConvertFbxJsonReport<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub vmd: Option<&'a Path>,
    pub bones_only: bool,
    pub readable_bone_names: bool,
    pub bone_name_map: Option<&'a Path>,
    pub baked_max_frame: Option<u32>,
    pub bytes_out: usize,
    pub counts: &'a mmd_anim_format::pmx::PmxParsedCounts,
    pub exported_blend_shapes: usize,
    pub copied_diffuse_textures: usize,
}

pub(crate) fn convert_fbx_json(report: ConvertFbxJsonReport<'_>) -> serde_json::Value {
    let mode = match (report.vmd.is_some(), report.bones_only) {
        (true, true) => "pmx-vmd-bake-bones-only",
        (false, true) => "pmx-bones-only",
        (true, false) => "pmx-vmd-bake",
        (false, false) => "pmx-to-fbx",
    };
    json!({
        "status": "ok",
        "command": "convert-fbx",
        "mode": mode,
        "bonesOnly": report.bones_only,
        "readableBoneNames": report.readable_bone_names,
        "boneNameMap": report.bone_name_map.map(|path| path.display().to_string()),
        "input": report.input.display().to_string(),
        "output": report.output.display().to_string(),
        "vmd": report.vmd.map(|path| path.display().to_string()),
        "bakedMaxFrame": report.baked_max_frame,
        "bytesOut": report.bytes_out,
        "counts": {
            "vertices": report.counts.vertices,
            "faces": report.counts.faces,
            "materials": report.counts.materials,
            "bones": report.counts.bones,
        },
        "exportedCounts": {
            "geometry": if report.bones_only { 0 } else { 1 },
            "materials": if report.bones_only { 0 } else { report.counts.materials },
            "joints": report.counts.bones,
            "skinClusters": if report.bones_only { 0 } else { report.counts.bones },
            "bindPoses": if report.bones_only { 0 } else { 1 },
            "blendShapes": report.exported_blend_shapes,
        },
        "copiedDiffuseTextures": report.copied_diffuse_textures,
    })
}

pub(crate) struct ConvertFbxOptions {
    pub max_frame: Option<u32>,
    pub copy_diffuse_textures: bool,
    pub bones_only: bool,
    pub readable_bone_names: bool,
    pub use_json: bool,
}

pub(crate) fn convert_pmx_to_fbx(
    input: &Path,
    output: &Path,
    vmd: Option<&Path>,
    convert_options: ConvertFbxOptions,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(input)?;
    let model = mmd_anim_format::parse_pmx_model(&data).map_err(|error| {
        parse_failure_error(
            "convert-fbx",
            input,
            mmd_anim_format::MmdFormatKind::Pmx,
            error,
        )
    })?;
    let mut options = mmd_anim_format::fbx::FbxExportOptions::default();
    if !model.metadata.name.is_empty() {
        options.model_name.clone_from(&model.metadata.name);
    } else if let Some(stem) = input.file_stem().and_then(|value| value.to_str()) {
        options.model_name = stem.to_owned();
    }
    options.bones_only = convert_options.bones_only;
    options.bone_name_policy = if convert_options.readable_bone_names {
        mmd_anim_format::fbx::FbxBoneNamePolicy::Readable
    } else {
        mmd_anim_format::fbx::FbxBoneNamePolicy::LegacyHex
    };
    let copied_diffuse_textures = if convert_options.bones_only {
        if convert_options.copy_diffuse_textures {
            eprintln!("warning: convert-fbx --bones-only ignores --copy-diffuse-textures");
        }
        0
    } else {
        configure_diffuse_texture_paths(
            input,
            output,
            &model,
            &mut options,
            convert_options.copy_diffuse_textures,
        )?
    };

    let mut baked_max_frame = None;
    let fbx = if let Some(vmd_path) = vmd {
        let motion_data = read_file(vmd_path)?;
        let motion = mmd_anim_format::parse_vmd_animation(&motion_data).map_err(|error| {
            parse_failure_error(
                "convert-fbx",
                vmd_path,
                mmd_anim_format::MmdFormatKind::Vmd,
                error,
            )
        })?;
        let runtime_import = mmd_anim_format::import_pmx_runtime(&data).map_err(|error| {
            import_failure_error(
                "convert-fbx",
                input,
                mmd_anim_format::MmdFormatKind::Pmx,
                error,
            )
        })?;
        let runtime_motion = mmd_anim_format::import_vmd_motion(&motion_data).map_err(|error| {
            import_failure_error(
                "convert-fbx",
                vmd_path,
                mmd_anim_format::MmdFormatKind::Vmd,
                error,
            )
        })?;
        let clip = mmd_anim_format::build_pair_clip(
            &runtime_motion,
            &runtime_import.bone_name_to_index,
            &runtime_import.morph_name_to_index,
            &runtime_import.ik_solver_bone_name_to_index,
            runtime_import.model.ik_count(),
        );
        warn_about_ignored_vmd_tracks(&motion);
        let natural_last_frame =
            fbx_bone_evaluation_last_frame(&motion, convert_options.bones_only);
        let last_frame = capped_fbx_bone_evaluation_last_frame(
            &motion,
            convert_options.max_frame,
            convert_options.bones_only,
        );
        if let Some(cap) = convert_options.max_frame
            && cap < natural_last_frame
        {
            let max_frame_scope = if convert_options.bones_only {
                "motion bone/IK property"
            } else {
                "motion bone/morph/IK property"
            };
            eprintln!(
                "warning: convert-fbx runtime bake capped at frame {cap} ({max_frame_scope} max frame {natural_last_frame})"
            );
        }
        baked_max_frame = Some(last_frame);
        mmd_anim_format::fbx::export_fbx_with_runtime_bake(
            &model,
            Arc::new(runtime_import.model),
            &clip,
            last_frame,
            &options,
        )?
    } else {
        mmd_anim_format::fbx::export_fbx(&model, None, &options)?
    };
    let exported_blend_shapes = exported_fbx_blend_shape_count(&model, convert_options.bones_only);
    write_file(output, &fbx)?;
    let bone_name_map = if convert_options.readable_bone_names {
        Some(write_bone_name_map_sidecar(output, &model, &options)?)
    } else {
        None
    };
    if convert_options.use_json {
        let report = convert_fbx_json(ConvertFbxJsonReport {
            input,
            output,
            vmd,
            bones_only: convert_options.bones_only,
            readable_bone_names: convert_options.readable_bone_names,
            bone_name_map: bone_name_map.as_deref(),
            baked_max_frame,
            bytes_out: fbx.len(),
            counts: &model.metadata.counts,
            exported_blend_shapes,
            copied_diffuse_textures,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "FBX export: ok input={} output={} vmd={} bonesOnly={} readableBoneNames={} bakedMaxFrame={} bytesOut={} vertices={} faces={} materials={} bones={} exportedGeometry={} exportedMaterials={} exportedSkinClusters={} exportedBlendShapes={}",
            input.display(),
            output.display(),
            vmd.map(|path| path.display().to_string())
                .unwrap_or_else(|| "-".to_owned()),
            convert_options.bones_only,
            convert_options.readable_bone_names,
            baked_max_frame
                .map(|frame| frame.to_string())
                .unwrap_or_else(|| "-".to_owned()),
            fbx.len(),
            model.metadata.counts.vertices,
            model.metadata.counts.faces,
            model.metadata.counts.materials,
            model.metadata.counts.bones,
            if convert_options.bones_only { 0 } else { 1 },
            if convert_options.bones_only {
                0
            } else {
                model.metadata.counts.materials
            },
            if convert_options.bones_only {
                0
            } else {
                model.metadata.counts.bones
            },
            exported_blend_shapes
        );
        if let Some(path) = &bone_name_map {
            println!("  boneNameMap={}", path.display());
        }
        if copied_diffuse_textures > 0 {
            println!("  copiedDiffuseTextures={copied_diffuse_textures}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn configure_diffuse_texture_paths(
    input: &Path,
    output: &Path,
    model: &mmd_anim_format::PmxParsedModel,
    options: &mut mmd_anim_format::fbx::FbxExportOptions,
    copy_diffuse_textures: bool,
) -> Result<usize, Box<dyn std::error::Error>> {
    if !copy_diffuse_textures {
        return Ok(0);
    }
    let input_dir = input.parent().unwrap_or_else(|| Path::new("."));
    let output_dir = output.parent().unwrap_or_else(|| Path::new("."));
    let output_stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("fbx");
    let texture_dir_name = format!("{output_stem}-textures");
    let texture_dir = output_dir.join(&texture_dir_name);
    let mut seen_destinations = HashMap::<PathBuf, PathBuf>::new();
    let mut copied = 0usize;
    options.diffuse_texture_paths = vec![String::new(); model.materials.len()];

    for (material_index, material) in model.materials.iter().enumerate() {
        let texture_path = material.texture_path.trim();
        if texture_path.is_empty() {
            continue;
        }
        let source = resolve_texture_source(input_dir, texture_path);
        if !source.is_file() {
            eprintln!(
                "warning: convert-fbx diffuse texture not found for material {material_index}: {}",
                source.display()
            );
            continue;
        }
        let Some(file_name) = Path::new(texture_path)
            .file_name()
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty())
        else {
            eprintln!(
                "warning: convert-fbx diffuse texture path has no file name for material {material_index}: {texture_path}"
            );
            continue;
        };
        let mut relative_destination = PathBuf::from(&texture_dir_name).join(file_name);
        let mut destination = output_dir.join(&relative_destination);
        if let Some(existing_source) = seen_destinations.get(&destination)
            && existing_source != &source
        {
            let prefixed = format!("m{material_index:03}_{file_name}");
            relative_destination = PathBuf::from(&texture_dir_name).join(prefixed);
            destination = output_dir.join(&relative_destination);
        }
        fs::create_dir_all(&texture_dir)?;
        fs::copy(&source, &destination)?;
        seen_destinations.insert(destination, source);
        options.diffuse_texture_paths[material_index] =
            relative_destination.to_string_lossy().replace('\\', "/");
        copied += 1;
    }
    Ok(copied)
}

fn resolve_texture_source(input_dir: &Path, texture_path: &str) -> PathBuf {
    let path = Path::new(texture_path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        input_dir.join(path)
    }
}

fn exported_fbx_blend_shape_count(
    model: &mmd_anim_format::PmxParsedModel,
    bones_only: bool,
) -> usize {
    if bones_only {
        0
    } else {
        model
            .morphs
            .iter()
            .filter(|morph| morph.kind == "vertex" && !morph.vertex_offsets.is_empty())
            .count()
    }
}

fn write_bone_name_map_sidecar(
    output: &Path,
    model: &mmd_anim_format::PmxParsedModel,
    options: &mmd_anim_format::fbx::FbxExportOptions,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let path = output.with_extension("bone-map.json");
    let entries =
        mmd_anim_format::fbx::build_bone_name_map(&model.skeleton.bones, options.bone_name_policy)
            .into_iter()
            .map(|entry| {
                json!({
                    "index": entry.index,
                    "pmxName": entry.pmx_name,
                    "pmxEnglishName": entry.pmx_english_name,
                    "fbxName": entry.fbx_name,
                    "source": fbx_bone_name_source_label(entry.source),
                    "collisionSuffix": entry.collision_suffix,
                })
            })
            .collect::<Vec<_>>();
    let report = json!({
        "schemaVersion": 1,
        "kind": "fbxBoneNameMap",
        "policy": "readable",
        "bones": entries,
    });
    write_file(&path, serde_json::to_string_pretty(&report)?.as_bytes())?;
    Ok(path)
}

fn fbx_bone_name_source_label(source: mmd_anim_format::fbx::FbxBoneNameSource) -> &'static str {
    match source {
        mmd_anim_format::fbx::FbxBoneNameSource::LegacyHex => "legacy_hex",
        mmd_anim_format::fbx::FbxBoneNameSource::PmxEnglish => "pmx_english",
        mmd_anim_format::fbx::FbxBoneNameSource::AsciiName => "ascii_name",
        mmd_anim_format::fbx::FbxBoneNameSource::StandardDictionary => "standard_dict",
        mmd_anim_format::fbx::FbxBoneNameSource::HexFallback => "hex_fallback",
    }
}

fn fbx_bone_evaluation_last_frame(
    motion: &mmd_anim_format::VmdParsedAnimation,
    bones_only: bool,
) -> u32 {
    let bone_last_frame = motion
        .bone_frames
        .iter()
        .map(|frame| frame.frame)
        .max()
        .unwrap_or(0);
    let morph_last_frame = if bones_only {
        0
    } else {
        motion
            .morph_frames
            .iter()
            .map(|frame| frame.frame)
            .max()
            .unwrap_or(0)
    };
    let property_ik_last_frame = motion
        .property_frames
        .iter()
        .filter(|frame| !frame.ik_states.is_empty())
        .map(|frame| frame.frame)
        .max()
        .unwrap_or(0);
    bone_last_frame
        .max(morph_last_frame)
        .max(property_ik_last_frame)
}

fn capped_fbx_bone_evaluation_last_frame(
    motion: &mmd_anim_format::VmdParsedAnimation,
    max_frame: Option<u32>,
    bones_only: bool,
) -> u32 {
    let last_frame = fbx_bone_evaluation_last_frame(motion, bones_only);
    max_frame
        .map(|frame| last_frame.min(frame))
        .unwrap_or(last_frame)
}

fn warn_about_ignored_vmd_tracks(motion: &mmd_anim_format::VmdParsedAnimation) {
    let ignored = [
        ("camera", motion.camera_frames.len()),
        ("light", motion.light_frames.len()),
        ("self-shadow", motion.self_shadow_frames.len()),
        ("property", motion.property_frames.len()),
    ]
    .into_iter()
    .filter(|(_, count)| *count > 0)
    .map(|(label, count)| format!("{label}={count}"))
    .collect::<Vec<_>>();

    if ignored.is_empty() {
        return;
    }

    eprintln!(
        "warning: convert-fbx does not write these VMD tracks as FBX animation ({})",
        ignored.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_motion() -> mmd_anim_format::VmdParsedAnimation {
        mmd_anim_format::VmdParsedAnimation {
            kind: "vmd",
            metadata: mmd_anim_format::vmd::VmdParsedMetadata {
                format: "vmd",
                model_name: "fixture".to_owned(),
                model_name_bytes: Vec::new(),
                counts: mmd_anim_format::vmd::VmdParsedCounts {
                    bones: 0,
                    morphs: 0,
                    cameras: 0,
                    lights: 0,
                    self_shadows: 0,
                    properties: 0,
                },
                max_frame: 0,
            },
            bone_frames: Vec::new(),
            morph_frames: Vec::new(),
            camera_frames: Vec::new(),
            light_frames: Vec::new(),
            self_shadow_frames: Vec::new(),
            property_frames: Vec::new(),
        }
    }

    #[test]
    fn fbx_last_frame_uses_bone_morph_and_ik_property_frames() {
        let mut motion = empty_motion();
        motion
            .bone_frames
            .push(mmd_anim_format::vmd::VmdParsedBoneFrame {
                bone_name: "bone".to_owned(),
                bone_name_bytes: Vec::new(),
                frame: 12,
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                interpolation: vec![0; 64],
            });
        motion
            .morph_frames
            .push(mmd_anim_format::vmd::VmdParsedMorphFrame {
                morph_name: "morph".to_owned(),
                morph_name_bytes: Vec::new(),
                frame: 240,
                weight: 1.0,
            });
        motion
            .property_frames
            .push(mmd_anim_format::vmd::VmdParsedPropertyFrame {
                frame: 300,
                visible: true,
                ik_states: Vec::new(),
            });
        motion
            .property_frames
            .push(mmd_anim_format::vmd::VmdParsedPropertyFrame {
                frame: 48,
                visible: true,
                ik_states: vec![mmd_anim_format::vmd::VmdParsedIkState {
                    bone_name: "IK".to_owned(),
                    bone_name_bytes: Vec::new(),
                    enabled: false,
                }],
            });

        assert_eq!(fbx_bone_evaluation_last_frame(&motion, false), 240);
        assert_eq!(fbx_bone_evaluation_last_frame(&motion, true), 48);
    }

    #[test]
    fn fbx_last_frame_can_be_capped_for_smoke_checks() {
        let mut motion = empty_motion();
        motion
            .bone_frames
            .push(mmd_anim_format::vmd::VmdParsedBoneFrame {
                bone_name: "bone".to_owned(),
                bone_name_bytes: Vec::new(),
                frame: 120,
                translation: [0.0; 3],
                rotation: [0.0, 0.0, 0.0, 1.0],
                interpolation: vec![0; 64],
            });

        assert_eq!(
            capped_fbx_bone_evaluation_last_frame(&motion, None, false),
            120
        );
        assert_eq!(
            capped_fbx_bone_evaluation_last_frame(&motion, Some(30), false),
            30
        );
        assert_eq!(
            capped_fbx_bone_evaluation_last_frame(&motion, Some(180), false),
            120
        );
    }

    #[test]
    fn convert_fbx_json_reports_bones_only_export_counts() {
        let counts = mmd_anim_format::pmx::PmxParsedCounts {
            vertices: 12,
            faces: 4,
            materials: 2,
            bones: 7,
            morphs: 3,
            display_frames: 0,
            rigid_bodies: 0,
            joints: 0,
            soft_bodies: 0,
        };
        let report = convert_fbx_json(ConvertFbxJsonReport {
            input: Path::new("model.pmx"),
            output: Path::new("model.fbx"),
            vmd: Some(Path::new("motion.vmd")),
            bones_only: true,
            readable_bone_names: false,
            bone_name_map: None,
            baked_max_frame: Some(60),
            bytes_out: 1234,
            counts: &counts,
            exported_blend_shapes: 0,
            copied_diffuse_textures: 0,
        });

        assert_eq!(report["mode"], "pmx-vmd-bake-bones-only");
        assert_eq!(report["bonesOnly"], true);
        assert_eq!(report["readableBoneNames"], false);
        assert_eq!(report["counts"]["bones"], 7);
        assert_eq!(report["exportedCounts"]["geometry"], 0);
        assert_eq!(report["exportedCounts"]["materials"], 0);
        assert_eq!(report["exportedCounts"]["joints"], 7);
        assert_eq!(report["exportedCounts"]["skinClusters"], 0);
        assert_eq!(report["exportedCounts"]["bindPoses"], 0);
        assert_eq!(report["exportedCounts"]["blendShapes"], 0);
    }

    #[test]
    fn convert_fbx_json_reports_readable_bone_names_flag() {
        let counts = mmd_anim_format::pmx::PmxParsedCounts {
            vertices: 0,
            faces: 0,
            materials: 0,
            bones: 1,
            morphs: 0,
            display_frames: 0,
            rigid_bodies: 0,
            joints: 0,
            soft_bodies: 0,
        };
        let report = convert_fbx_json(ConvertFbxJsonReport {
            input: Path::new("model.pmx"),
            output: Path::new("model.fbx"),
            vmd: None,
            bones_only: false,
            readable_bone_names: true,
            bone_name_map: Some(Path::new("model.bone-map.json")),
            baked_max_frame: None,
            bytes_out: 42,
            counts: &counts,
            exported_blend_shapes: 0,
            copied_diffuse_textures: 0,
        });

        assert_eq!(report["readableBoneNames"], true);
        assert_eq!(report["boneNameMap"], "model.bone-map.json");
    }
}
