use mmd_anim_runtime::{
    QuantizedBezier, ReducedBoneKey, ReducedPoseSequence, ReductionTarget, VmdBoneInterpolation,
};
use thiserror::Error;

use super::{
    VmdParsedAnimation, VmdParsedBoneFrame, VmdParsedCounts, VmdParsedIkState, VmdParsedMetadata,
    VmdParsedMorphFrame, VmdParsedPropertyFrame,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmdExportName {
    pub text: String,
    pub bytes: Vec<u8>,
}

impl VmdExportName {
    pub fn new(text: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            text: text.into(),
            bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmdExportMorphKind {
    Vertex,
    Uv,
    Other,
    Bone,
    Group,
    Material,
}

#[derive(Debug, Clone)]
pub struct VmdPoseExportBindings {
    pub model_identity: u64,
    pub model_name: VmdExportName,
    pub bone_names: Vec<VmdExportName>,
    pub morph_names: Vec<VmdExportName>,
    pub ik_names: Vec<VmdExportName>,
    pub ik_solver_count: usize,
    pub append_affected_bones: Vec<bool>,
    pub morph_kinds: Vec<VmdExportMorphKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmdPoseExportReport {
    pub physics_must_be_disabled_by_host: bool,
    pub ik_disabled_in_vmd: bool,
    pub skipped_constant_bone_tracks: usize,
    pub skipped_zero_morph_tracks: usize,
}

#[derive(Debug, Clone)]
pub struct VmdPoseExport {
    pub animation: VmdParsedAnimation,
    pub report: VmdPoseExportReport,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum VmdPoseExportError {
    #[error("reduced pose target must be VmdBezier")]
    WrongTarget,
    #[error("export binding model identity or counts do not match reduced pose")]
    BindingMismatch,
    #[error("sample {sample} has non-integer VMD frame {frame}")]
    NonIntegerFrame { sample: usize, frame: String },
    #[error("sample {sample} frame is outside the VMD u32 range")]
    FrameOutOfRange { sample: usize },
    #[error("{kind} name {index} exceeds the VMD byte limit {limit}")]
    NameTooLong {
        kind: &'static str,
        index: usize,
        limit: usize,
    },
    #[error("bone {bone} has baked motion but also participates in append transform")]
    AppendTransformWouldDoubleApply { bone: usize },
    #[error("morph {morph} of kind {kind:?} would double-apply baked deformation")]
    MorphWouldDoubleApply {
        morph: usize,
        kind: VmdExportMorphKind,
    },
    #[error("morph {morph} of kind {kind:?} cannot be emitted by the VMD pose adapter")]
    UnsupportedMorphKind {
        morph: usize,
        kind: VmdExportMorphKind,
    },
}

pub fn export_reduced_pose_to_vmd(
    sequence: &ReducedPoseSequence,
    bindings: &VmdPoseExportBindings,
) -> Result<VmdPoseExport, VmdPoseExportError> {
    if sequence.target() != ReductionTarget::VmdBezier {
        return Err(VmdPoseExportError::WrongTarget);
    }
    let snapshot = sequence.snapshot();
    if !sequence.validate_model(
        bindings.model_identity,
        bindings.bone_names.len(),
        bindings.morph_names.len(),
    ) || bindings.append_affected_bones.len() != snapshot.bone_count()
        || bindings.morph_kinds.len() != snapshot.morph_count()
        || bindings.ik_names.len() != bindings.ik_solver_count
    {
        return Err(VmdPoseExportError::BindingMismatch);
    }
    validate_name(&bindings.model_name, "model", 0, 20)?;
    for (index, name) in bindings.bone_names.iter().enumerate() {
        validate_name(name, "bone", index, 15)?;
    }
    for (index, name) in bindings.morph_names.iter().enumerate() {
        validate_name(name, "morph", index, 15)?;
    }
    for (index, name) in bindings.ik_names.iter().enumerate() {
        validate_name(name, "ik", index, 20)?;
    }

    let sample_frames = vmd_sample_frames(sequence)?;
    let mut bone_frames = Vec::new();
    let mut skipped_constant_bone_tracks = 0;
    for (bone, track) in sequence.bone_tracks().iter().enumerate() {
        let rest_translation = snapshot.rest_local_translations()[bone];
        let rest_rotation = snapshot.rest_local_rotations()[bone];
        let active = track.keys().iter().any(|key| {
            key.translation.distance(rest_translation) > 1.0e-6
                || rotation_error(key.rotation, rest_rotation) > 1.0e-6
        });
        if !active {
            skipped_constant_bone_tracks += 1;
            continue;
        }
        if bindings.append_affected_bones[bone] {
            return Err(VmdPoseExportError::AppendTransformWouldDoubleApply { bone });
        }
        for key in track.keys() {
            bone_frames.push(vmd_bone_frame(
                &bindings.bone_names[bone],
                sample_frames[key.sample_index],
                key,
                rest_translation,
                rest_rotation,
            ));
        }
    }

    let mut morph_frames = Vec::new();
    let mut skipped_zero_morph_tracks = 0;
    for (morph, track) in sequence.morph_tracks().iter().enumerate() {
        let active = track.keys().iter().any(|key| key.weight.abs() > 1.0e-6);
        if !active {
            skipped_zero_morph_tracks += 1;
            continue;
        }
        let kind = bindings.morph_kinds[morph];
        if matches!(
            kind,
            VmdExportMorphKind::Bone | VmdExportMorphKind::Group | VmdExportMorphKind::Material
        ) {
            return Err(VmdPoseExportError::MorphWouldDoubleApply { morph, kind });
        }
        if kind != VmdExportMorphKind::Vertex {
            return Err(VmdPoseExportError::UnsupportedMorphKind { morph, kind });
        }
        for key in track.keys() {
            morph_frames.push(VmdParsedMorphFrame {
                morph_name: bindings.morph_names[morph].text.clone(),
                morph_name_bytes: bindings.morph_names[morph].bytes.clone(),
                frame: sample_frames[key.sample_index],
                weight: key.weight,
            });
        }
    }
    bone_frames.sort_by(|a, b| (a.frame, &a.bone_name_bytes).cmp(&(b.frame, &b.bone_name_bytes)));
    morph_frames
        .sort_by(|a, b| (a.frame, &a.morph_name_bytes).cmp(&(b.frame, &b.morph_name_bytes)));

    let property_frames = vec![VmdParsedPropertyFrame {
        frame: sample_frames[0],
        visible: true,
        ik_states: bindings
            .ik_names
            .iter()
            .map(|name| VmdParsedIkState {
                bone_name: name.text.clone(),
                bone_name_bytes: name.bytes.clone(),
                enabled: false,
            })
            .collect(),
    }];
    let max_frame = *sample_frames.last().expect("validated non-empty sequence");
    let animation = VmdParsedAnimation {
        kind: "vmd",
        metadata: VmdParsedMetadata {
            format: "vmd",
            model_name: bindings.model_name.text.clone(),
            model_name_bytes: bindings.model_name.bytes.clone(),
            counts: VmdParsedCounts {
                bones: bone_frames.len(),
                morphs: morph_frames.len(),
                cameras: 0,
                lights: 0,
                self_shadows: 0,
                properties: property_frames.len(),
            },
            max_frame,
        },
        bone_frames,
        morph_frames,
        camera_frames: Vec::new(),
        light_frames: Vec::new(),
        self_shadow_frames: Vec::new(),
        property_frames,
    };
    Ok(VmdPoseExport {
        animation,
        report: VmdPoseExportReport {
            physics_must_be_disabled_by_host: true,
            ik_disabled_in_vmd: true,
            skipped_constant_bone_tracks,
            skipped_zero_morph_tracks,
        },
    })
}

fn validate_name(
    name: &VmdExportName,
    kind: &'static str,
    index: usize,
    limit: usize,
) -> Result<(), VmdPoseExportError> {
    if name.bytes.len() > limit {
        Err(VmdPoseExportError::NameTooLong { kind, index, limit })
    } else {
        Ok(())
    }
}

fn vmd_sample_frames(sequence: &ReducedPoseSequence) -> Result<Vec<u32>, VmdPoseExportError> {
    (0..sequence.frame_count())
        .map(|sample| {
            let frame = sequence.start_frame() + sample as f32 * sequence.frame_step();
            if frame < 0.0 || frame as f64 > u32::MAX as f64 {
                return Err(VmdPoseExportError::FrameOutOfRange { sample });
            }
            let rounded = frame.round();
            if frame != rounded {
                return Err(VmdPoseExportError::NonIntegerFrame {
                    sample,
                    frame: frame.to_string(),
                });
            }
            Ok(rounded as u32)
        })
        .collect()
}

fn vmd_bone_frame(
    name: &VmdExportName,
    frame: u32,
    key: &ReducedBoneKey,
    rest_translation: glam::Vec3A,
    rest_rotation: glam::Quat,
) -> VmdParsedBoneFrame {
    let translation = key.translation - rest_translation;
    let rotation = (rest_rotation.inverse() * key.rotation).normalize();
    VmdParsedBoneFrame {
        bone_name: name.text.clone(),
        bone_name_bytes: name.bytes.clone(),
        frame,
        translation: translation.to_array(),
        rotation: rotation.to_array(),
        interpolation: vmd_interpolation_block(key.vmd_interpolation).to_vec(),
    }
}

fn vmd_interpolation_block(curves: VmdBoneInterpolation) -> [u8; 64] {
    let channels = [
        curves.translation[0],
        curves.translation[1],
        curves.translation[2],
        curves.rotation,
    ];
    let mut first = [0u8; 16];
    for (channel, curve) in channels.into_iter().enumerate() {
        write_curve(&mut first, channel, curve);
    }
    let mut block = [0u8; 64];
    for chunk in block.chunks_exact_mut(16) {
        chunk.copy_from_slice(&first);
    }
    block
}

fn write_curve(block: &mut [u8; 16], channel: usize, curve: QuantizedBezier) {
    block[channel] = curve.x1.min(127);
    block[4 + channel] = curve.y1.min(127);
    block[8 + channel] = curve.x2.min(127);
    block[12 + channel] = curve.y2.min(127);
}

fn rotation_error(a: glam::Quat, b: glam::Quat) -> f32 {
    2.0 * a.dot(b).abs().clamp(-1.0, 1.0).acos()
}

#[cfg(test)]
mod tests;
