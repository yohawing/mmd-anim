use std::{path::Path, process::ExitCode};

use clap::ValueEnum;
use serde::Serialize;

use crate::{parse_failure_error, read_file};

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum VmdSampleKind {
    Camera,
    Light,
    #[value(name = "self-shadow")]
    SelfShadow,
}

impl VmdSampleKind {
    fn label(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Light => "light",
            Self::SelfShadow => "self-shadow",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum VmdSampleState {
    Camera(mmd_anim_format::VmdCameraState),
    Light(mmd_anim_format::VmdLightState),
    SelfShadow(mmd_anim_format::VmdSelfShadowState),
}

impl VmdSampleState {
    fn kind(&self) -> VmdSampleKind {
        match self {
            Self::Camera(_) => VmdSampleKind::Camera,
            Self::Light(_) => VmdSampleKind::Light,
            Self::SelfShadow(_) => VmdSampleKind::SelfShadow,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VmdSampleJsonReport<'a> {
    kind: &'static str,
    motion: String,
    frame: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    camera: Option<&'a mmd_anim_format::VmdCameraState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    light: Option<&'a mmd_anim_format::VmdLightState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    self_shadow: Option<&'a mmd_anim_format::VmdSelfShadowState>,
}

pub(crate) fn vmd_sample(
    motion: &Path,
    kind: VmdSampleKind,
    frame: f32,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = read_file(motion)?;
    let parsed = mmd_anim_format::parse_vmd_animation(&data).map_err(|error| {
        parse_failure_error(
            "vmd-sample",
            motion,
            mmd_anim_format::MmdFormatKind::Vmd,
            error,
        )
    })?;
    let state = sample_vmd_animation(&parsed, kind, frame)?;
    if json {
        print_json(motion, frame, &state)?;
    } else {
        print_text(frame, state);
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
pub(crate) fn sample_vmd_bytes(
    data: &[u8],
    kind: VmdSampleKind,
    frame: f32,
) -> Result<VmdSampleState, Box<dyn std::error::Error>> {
    let parsed = mmd_anim_format::parse_vmd_animation(data)?;
    sample_vmd_animation(&parsed, kind, frame)
}

fn sample_vmd_animation(
    parsed: &mmd_anim_format::VmdParsedAnimation,
    kind: VmdSampleKind,
    frame: f32,
) -> Result<VmdSampleState, Box<dyn std::error::Error>> {
    match kind {
        VmdSampleKind::Camera => {
            mmd_anim_format::sample_vmd_camera_frames(&parsed.camera_frames, frame)
                .map(VmdSampleState::Camera)
                .ok_or_else(|| "VMD has no camera keyframes".into())
        }
        VmdSampleKind::Light => {
            mmd_anim_format::sample_vmd_light_frames(&parsed.light_frames, frame)
                .map(VmdSampleState::Light)
                .ok_or_else(|| "VMD has no light keyframes".into())
        }
        VmdSampleKind::SelfShadow => {
            mmd_anim_format::sample_vmd_self_shadow_frames(&parsed.self_shadow_frames, frame)
                .map(VmdSampleState::SelfShadow)
                .ok_or_else(|| "VMD has no self-shadow keyframes".into())
        }
    }
}

fn print_json(
    motion: &Path,
    frame: f32,
    state: &VmdSampleState,
) -> Result<(), Box<dyn std::error::Error>> {
    let (camera, light, self_shadow) = match state {
        VmdSampleState::Camera(camera) => (Some(camera), None, None),
        VmdSampleState::Light(light) => (None, Some(light), None),
        VmdSampleState::SelfShadow(self_shadow) => (None, None, Some(self_shadow)),
    };
    let report = VmdSampleJsonReport {
        kind: state.kind().label(),
        motion: motion.display().to_string(),
        frame,
        camera,
        light,
        self_shadow,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn print_text(frame: f32, state: VmdSampleState) {
    match state {
        VmdSampleState::Camera(camera) => {
            println!(
                "VMD camera sample: frame={:.3} distance={:.6} position=({:.6},{:.6},{:.6}) rotation=({:.6},{:.6},{:.6}) fov={:.6} perspective={}",
                frame,
                camera.distance,
                camera.position[0],
                camera.position[1],
                camera.position[2],
                camera.rotation[0],
                camera.rotation[1],
                camera.rotation[2],
                camera.fov,
                camera.perspective
            );
        }
        VmdSampleState::Light(light) => {
            println!(
                "VMD light sample: frame={:.3} color=({:.6},{:.6},{:.6}) direction=({:.6},{:.6},{:.6})",
                frame,
                light.color[0],
                light.color[1],
                light.color[2],
                light.direction[0],
                light.direction[1],
                light.direction[2]
            );
        }
        VmdSampleState::SelfShadow(self_shadow) => {
            println!(
                "VMD self-shadow sample: frame={:.3} mode={} distance={:.6}",
                frame, self_shadow.mode, self_shadow.distance
            );
        }
    }
}
