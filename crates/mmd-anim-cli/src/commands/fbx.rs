use std::{path::Path, process::ExitCode, sync::Arc};

use crate::{read_file, write_file};

pub(crate) fn convert_pmx_to_fbx(
    input: &Path,
    output: &Path,
    vmd: Option<&Path>,
    max_frame: Option<u32>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(input)?;
    let model = mmd_anim_format::parse_pmx_model(&data)?;
    let mut options = mmd_anim_format::fbx::FbxExportOptions::default();
    if !model.metadata.name.is_empty() {
        options.model_name.clone_from(&model.metadata.name);
    } else if let Some(stem) = input.file_stem().and_then(|value| value.to_str()) {
        options.model_name = stem.to_owned();
    }

    let mut baked_max_frame = None;
    let fbx = if let Some(vmd_path) = vmd {
        let motion_data = read_file(vmd_path)?;
        let motion = mmd_anim_format::parse_vmd_animation(&motion_data)?;
        let runtime_import = mmd_anim_format::import_pmx_runtime(&data)?;
        let runtime_motion = mmd_anim_format::import_vmd_motion(&motion_data)?;
        let clip = mmd_anim_format::build_pair_clip(
            &runtime_motion,
            &runtime_import.bone_name_to_index,
            &runtime_import.morph_name_to_index,
            &runtime_import.ik_solver_bone_name_to_index,
            runtime_import.model.ik_count(),
        );
        warn_about_ignored_vmd_tracks(&motion);
        let natural_last_frame = fbx_bone_evaluation_last_frame(&motion);
        let last_frame = capped_fbx_bone_evaluation_last_frame(&motion, max_frame);
        if let Some(cap) = max_frame
            && cap < natural_last_frame
        {
            eprintln!(
                "warning: convert-fbx runtime bake capped at frame {cap} (motion bone/IK property max frame {natural_last_frame})"
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
    write_file(output, &fbx)?;
    println!(
        "FBX export: ok input={} output={} vmd={} bakedMaxFrame={} bytesOut={} vertices={} faces={} materials={}",
        input.display(),
        output.display(),
        vmd.map(|path| path.display().to_string())
            .unwrap_or_else(|| "-".to_owned()),
        baked_max_frame
            .map(|frame| frame.to_string())
            .unwrap_or_else(|| "-".to_owned()),
        fbx.len(),
        model.metadata.counts.vertices,
        model.metadata.counts.faces,
        model.metadata.counts.materials
    );
    Ok(ExitCode::SUCCESS)
}

fn fbx_bone_evaluation_last_frame(motion: &mmd_anim_format::VmdParsedAnimation) -> u32 {
    let bone_last_frame = motion
        .bone_frames
        .iter()
        .map(|frame| frame.frame)
        .max()
        .unwrap_or(0);
    let property_ik_last_frame = motion
        .property_frames
        .iter()
        .filter(|frame| !frame.ik_states.is_empty())
        .map(|frame| frame.frame)
        .max()
        .unwrap_or(0);
    bone_last_frame.max(property_ik_last_frame)
}

fn capped_fbx_bone_evaluation_last_frame(
    motion: &mmd_anim_format::VmdParsedAnimation,
    max_frame: Option<u32>,
) -> u32 {
    let last_frame = fbx_bone_evaluation_last_frame(motion);
    max_frame
        .map(|frame| last_frame.min(frame))
        .unwrap_or(last_frame)
}

fn warn_about_ignored_vmd_tracks(motion: &mmd_anim_format::VmdParsedAnimation) {
    let ignored = [
        ("morph", motion.morph_frames.len()),
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
        "warning: convert-fbx exports FBX bone animation only; non-bone VMD tracks are not written as FBX tracks ({})",
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
    fn fbx_last_frame_uses_bone_and_ik_property_frames_only() {
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

        assert_eq!(fbx_bone_evaluation_last_frame(&motion), 48);
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

        assert_eq!(capped_fbx_bone_evaluation_last_frame(&motion, None), 120);
        assert_eq!(capped_fbx_bone_evaluation_last_frame(&motion, Some(30)), 30);
        assert_eq!(
            capped_fbx_bone_evaluation_last_frame(&motion, Some(180)),
            120
        );
    }
}
