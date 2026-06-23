use std::{fs, path::Path, process::ExitCode};

use serde_json::json;

pub(crate) fn export_roundtrip_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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
        mmd_anim_format::MmdFormatKind::Pmm => {
            let parsed = mmd_anim_format::parse_pmm_manifest(&data)?;
            let exported = mmd_anim_format::export_pmm_manifest(&parsed);
            let _reparsed = mmd_anim_format::parse_pmm_manifest(&exported)?;
            ensure_pmm_lossless_roundtrip(&data, &exported)?;
            println!(
                "PMM export roundtrip: ok bytesIn={} bytesOut={} version={} modelReferences={} assetReferences={} diagnostics={}",
                data.len(),
                exported.len(),
                parsed.version,
                parsed.model_paths.len(),
                parsed.asset_summary.reference_count,
                parsed.diagnostics.len()
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

pub(crate) fn export_roundtrip_json(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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
        mmd_anim_format::MmdFormatKind::Pmm => {
            let parsed = mmd_anim_format::parse_pmm_manifest(&data)?;
            let exported = mmd_anim_format::export_pmm_manifest(&parsed);
            let _reparsed = mmd_anim_format::parse_pmm_manifest(&exported)?;
            ensure_pmm_lossless_roundtrip(&data, &exported)?;
            pmm_roundtrip_json(
                path,
                "parse-export-parse-lossless",
                data.len(),
                exported.len(),
                data == exported,
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

pub(crate) fn export_json_roundtrip_summary(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

pub(crate) fn export_json_roundtrip_json(path: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

pub(crate) fn export_format(input: &Path, output: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

/// Resolves a PMX model path for PMM export to a canonical path string that MMD can open.
pub(crate) fn resolve_pmx_path_for_pmm(model_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let canonical = model_path.canonicalize().map_err(|e| {
        format!(
            "failed to canonicalize PMX model path {}: {}",
            model_path.display(),
            e
        )
    })?;
    Ok(pmm_display_path(&canonical))
}

fn pmm_display_path(path: &Path) -> String {
    let text = path.display().to_string();
    #[cfg(windows)]
    {
        if let Some(stripped) = text.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{stripped}");
        }
        if let Some(stripped) = text.strip_prefix(r"\\?\") {
            return stripped.to_owned();
        }
    }
    text
}

pub(crate) fn export_pmm_scene(
    model_path: &Path,
    motion_path: &Path,
    output_path: &Path,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let model_data = fs::read(model_path)?;
    let motion_data = fs::read(motion_path)?;
    let model = mmd_anim_format::parse_pmx_model(&model_data)?;
    let motion = mmd_anim_format::parse_vmd_animation(&motion_data)?;
    let model_path_text = resolve_pmx_path_for_pmm(model_path)?;
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

pub(crate) fn export_json_format(input: &Path, output: &Path) -> Result<ExitCode, Box<dyn std::error::Error>> {
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

pub(crate) fn vmd_roundtrip_json(
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

pub(crate) fn vpd_roundtrip_json(
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

pub(crate) fn pmd_roundtrip_json(
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

pub(crate) fn pmx_roundtrip_json(
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

pub(crate) fn accessory_roundtrip_json(
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

pub(crate) fn pmm_roundtrip_json(
    path: &Path,
    mode: &str,
    bytes_in: usize,
    bytes_out: usize,
    byte_for_byte: bool,
    parsed: &mmd_anim_format::PmmParsedManifest,
) -> serde_json::Value {
    serde_json::json!({
        "status": "ok",
        "mode": mode,
        "format": "pmm",
        "path": path.display().to_string(),
        "bytesIn": bytes_in,
        "bytesOut": bytes_out,
        "version": parsed.version,
        "modelReferences": parsed.model_paths.len(),
        "assetReferences": parsed.asset_summary.reference_count,
        "diagnostics": parsed.diagnostics.len(),
        "byteForByte": byte_for_byte,
    })
}

pub(crate) fn ensure_pmx_roundtrip(
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

pub(crate) fn ensure_pmd_roundtrip(
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

pub(crate) fn ensure_pmm_lossless_roundtrip(
    original: &[u8],
    exported: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    if original != exported {
        return Err(
            "PMM parse/export/parse did not preserve source bytes (lossless path failed)".into(),
        );
    }
    Ok(())
}

pub(crate) fn ensure_accessory_roundtrip(
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

pub(crate) fn ensure_accessory_json_roundtrip(
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

pub(crate) fn ensure_vmd_roundtrip(
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

pub(crate) fn ensure_vpd_roundtrip(
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
