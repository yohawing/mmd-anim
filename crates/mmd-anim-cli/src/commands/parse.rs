use std::{fs, path::Path, process::ExitCode};

pub(crate) fn parse_pmx_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

pub(crate) fn parse_format_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

pub(crate) fn parse_format_json(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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
