use std::{path::Path, process::ExitCode};

use crate::{
    diagnostics_suffix, parse_failure_error, read_file, unsupported_format_error, write_file,
};

fn parse_pmx_summary(path: &Path, data: &[u8]) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let parsed = mmd_anim_format::parse_pmx_model(data).map_err(|error| {
        parse_failure_error("inspect", path, mmd_anim_format::MmdFormatKind::Pmx, error)
    })?;
    println!(
        "PMX parser: vertices={} faces={} materials={} bones={} morphs={} displayFrames={} rigidBodies={} joints={} softBodies={}{}",
        parsed.metadata.counts.vertices,
        parsed.metadata.counts.faces,
        parsed.metadata.counts.materials,
        parsed.metadata.counts.bones,
        parsed.metadata.counts.morphs,
        parsed.metadata.counts.display_frames,
        parsed.metadata.counts.rigid_bodies,
        parsed.metadata.counts.joints,
        parsed.metadata.counts.soft_bodies,
        diagnostics_suffix(parsed.diagnostics.len())
    );
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn parse_format_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(path)?;
    let file_name = path.file_name().and_then(|v| v.to_str());
    let kind = mmd_anim_format::detect_mmd_format(&data, file_name);
    let parse_error = |error| parse_failure_error("inspect", path, kind, error);
    match kind {
        mmd_anim_format::MmdFormatKind::Pmx => parse_pmx_summary(path, &data),
        mmd_anim_format::MmdFormatKind::Pmd => {
            let parsed = mmd_anim_format::parse_pmd_model(&data).map_err(parse_error)?;
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
            let parsed = mmd_anim_format::parse_vmd_animation(&data).map_err(parse_error)?;
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
            let parsed = mmd_anim_format::parse_vpd_pose(&data).map_err(parse_error)?;
            println!(
                "VPD parser: bones={}{}",
                parsed.bone_count,
                diagnostics_suffix(parsed.diagnostics.len())
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Pmm => {
            let parsed = mmd_anim_format::parse_pmm_manifest(&data).map_err(parse_error)?;
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
            let first_model_slot = parsed.model_slots.first();
            let first_model_slot_padding =
                display_or_unknown(first_model_slot.map(|slot| slot.trailing_zero_padding_bytes));
            let first_model_slot_next_non_zero =
                display_or_unknown(first_model_slot.and_then(|slot| slot.next_non_zero_offset));
            let document_counts = parsed
                .document_summary
                .as_ref()
                .map(|document| &document.counts);
            let document_models = display_or_unknown(document_counts.map(|counts| counts.models));
            let document_bone_keyframes =
                display_or_unknown(document_counts.map(|counts| counts.bone_keyframes));
            let document_morph_keyframes =
                display_or_unknown(document_counts.map(|counts| counts.morph_keyframes));
            let global = parsed.document_global_summary.as_ref();
            let global_camera_keyframes =
                display_or_unknown(global.map(|g| g.camera.initial_keyframes + g.camera.keyframes));
            let global_light_keyframes =
                display_or_unknown(global.map(|g| g.light.initial_keyframes + g.light.keyframes));
            let global_accessory_count =
                display_or_unknown(global.map(|g| g.accessories.accessory_count));
            let global_accessory_keyframes =
                display_or_unknown(global.map(|g| g.accessories.keyframes));
            let global_gravity_keyframes = display_or_unknown(
                global.map(|g| g.gravity.initial_keyframes + g.gravity.keyframes),
            );
            let global_self_shadow_keyframes = display_or_unknown(
                global.map(|g| g.self_shadow.initial_keyframes + g.self_shadow.keyframes),
            );
            let global_audio_path =
                display_or_unknown(global.map(|g| !g.settings.audio_path.is_empty()));
            let global_bg_video_path =
                display_or_unknown(global.map(|g| !g.settings.background_video_path.is_empty()));
            let global_bg_image_path =
                display_or_unknown(global.map(|g| !g.settings.background_image_path.is_empty()));
            println!(
                "PMM parser: references={} version={} parsedVersion={} models={} accessories={} motions={} audio={} images={} videos={} modelAssets={} modelSlots={} firstModelSlotPadding={} firstModelSlotNextNonZeroOffset={} documentModels={} documentBoneKeyframes={} documentMorphKeyframes={} headerTextEntries={} audioAssets={} assetConfidence=high:{} medium:{} low:{} assetKindCounts={} assetExtensionCounts={} screen={}x{} timelineFrames={} timelineRange={}..{} frameRate={} durationSeconds={} displayLayout={} selectedModelIndex={} documentModelCount={} declaredModelSlotCount={} modelSlotCount={} nonZeroModelSlotCount={} modelSlotFlags={} activeModelSlotIndices={} emptyModelSlotIndices={} modelSlotFlagCounts={} accessorySlotCount={} globalCameraKeyframes={} globalLightKeyframes={} globalAccessoryCount={} globalAccessoryKeyframes={} globalGravityKeyframes={} globalSelfShadowKeyframes={} globalAudioPath={} globalBgVideoPath={} globalBgImagePath={}{}",
                parsed.asset_summary.reference_count,
                parsed.version,
                display_or_unknown(parsed.parsed_version),
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
                display_or_unknown(parsed.project_settings.screen_width),
                display_or_unknown(parsed.project_settings.screen_height),
                display_or_unknown(parsed.project_settings.timeline_frame_count),
                display_or_unknown(parsed.timeline.start_frame),
                display_or_unknown(parsed.timeline.end_frame_exclusive),
                display_or_unknown(parsed.project_settings.frame_rate),
                display_or_unknown(parsed.timeline.duration_seconds),
                parsed.display_state.layout,
                display_or_unknown(parsed.display_state.selected_model_index),
                display_or_unknown(parsed.display_state.document_model_count),
                display_or_unknown(parsed.display_state.declared_model_slot_count),
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
                display_or_unknown(parsed.display_state.accessory_slot_count),
                global_camera_keyframes,
                global_light_keyframes,
                global_accessory_count,
                global_accessory_keyframes,
                global_gravity_keyframes,
                global_self_shadow_keyframes,
                global_audio_path,
                global_bg_video_path,
                global_bg_image_path,
                diagnostics_suffix(parsed.diagnostics.len())
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            let parsed =
                mmd_anim_format::parse_accessory_manifest(&data, file_name).map_err(parse_error)?;
            println!(
                "{} parser: byteLength={} meshes={} materials={} textures={}{}",
                parsed.format.to_uppercase(),
                parsed.byte_length,
                parsed.mesh_count,
                parsed.material_count,
                parsed.texture_references.len(),
                diagnostics_suffix(parsed.diagnostics.len())
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Nmd => {
            let parsed = mmd_anim_format::parse_nmd_manifest(&data).map_err(parse_error)?;
            println!(
                "NMD parser: byteLength={} annotations={} globalTracks={} bundles={{accessory:{}, bone:{}, camera:{}, light:{}, model:{}, morph:{}, selfShadow:{}, unknown:{}}}{}",
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
                diagnostics_suffix(parsed.diagnostics.len())
            );
            Ok(ExitCode::SUCCESS)
        }
        mmd_anim_format::MmdFormatKind::Unknown => {
            Err(unsupported_format_error("inspect", path, kind))
        }
    }
}

fn display_or_unknown(value: Option<impl std::fmt::Display>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_owned())
}

pub(crate) fn parse_format_json(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let text = parse_format_json_text(path)?;
    println!("{text}");
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn parse_format_json_to_file(
    path: &Path,
    output: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let text = parse_format_json_text(path)?;
    write_file(output, text.as_bytes())?;
    Ok(ExitCode::SUCCESS)
}

pub(crate) fn parse_format_json_text(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let value = parse_format_json_value(path)?;
    Ok(serde_json::to_string_pretty(&value)?)
}

fn parse_format_json_value(path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let data = read_file(path)?;
    let file_name = path.file_name().and_then(|v| v.to_str());
    let kind = mmd_anim_format::detect_mmd_format(&data, file_name);
    let parse_error = |error| parse_failure_error("inspect", path, kind, error);
    let value = match kind {
        mmd_anim_format::MmdFormatKind::Pmx => {
            serde_json::to_value(mmd_anim_format::parse_pmx_model(&data).map_err(parse_error)?)?
        }
        mmd_anim_format::MmdFormatKind::Pmd => {
            serde_json::to_value(mmd_anim_format::parse_pmd_model(&data).map_err(parse_error)?)?
        }
        mmd_anim_format::MmdFormatKind::Vmd => {
            serde_json::to_value(mmd_anim_format::parse_vmd_animation(&data).map_err(parse_error)?)?
        }
        mmd_anim_format::MmdFormatKind::Vpd => {
            serde_json::to_value(mmd_anim_format::parse_vpd_pose(&data).map_err(parse_error)?)?
        }
        mmd_anim_format::MmdFormatKind::Pmm => {
            serde_json::to_value(mmd_anim_format::parse_pmm_manifest(&data).map_err(parse_error)?)?
        }
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            serde_json::to_value(
                mmd_anim_format::parse_accessory_manifest(&data, file_name).map_err(parse_error)?,
            )?
        }
        mmd_anim_format::MmdFormatKind::Nmd => {
            serde_json::to_value(mmd_anim_format::parse_nmd_manifest(&data).map_err(parse_error)?)?
        }
        mmd_anim_format::MmdFormatKind::Unknown => {
            return Err(unsupported_format_error("inspect", path, kind));
        }
    };
    Ok(value)
}
