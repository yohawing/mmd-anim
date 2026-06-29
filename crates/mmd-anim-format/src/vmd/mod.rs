use glam::{Quat, Vec3A};
use serde::{Deserialize, Serialize};

use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, InterpolationScalar, InterpolationVector3,
    MorphAnimationBinding, MorphIndex, MorphKeyframe, MorphTrack, MovableBoneKeyframe,
    MovableBoneTrack, PropertyAnimationBinding, PropertyKeyframe,
};

use crate::error::ImportError;
use crate::normalize::normalize_vmd_name;

const VMD_MAGIC: [u8; 30] = *b"Vocaloid Motion Data 0002\0\0\0\0\0";
const VMD_MAGIC_PREFIX: &[u8] = b"Vocaloid Motion Data 0002\0";

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn require(&self, n: usize) -> Result<(), ImportError> {
        if self.remaining() >= n {
            Ok(())
        } else {
            Err(ImportError::UnexpectedEof(
                n.saturating_sub(self.remaining()),
            ))
        }
    }

    fn read_slice(&mut self, n: usize) -> Result<&'a [u8], ImportError> {
        self.require(n)?;
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, ImportError> {
        Ok(self.read_slice(1)?[0])
    }

    fn read_u32_le(&mut self) -> Result<u32, ImportError> {
        let b = self.read_slice(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_optional_u32_le(&mut self) -> Result<Option<u32>, ImportError> {
        if self.remaining() == 0 {
            Ok(None)
        } else {
            self.read_u32_le().map(Some)
        }
    }

    fn require_record_bytes(&self, count: usize, record_size: usize) -> Result<(), ImportError> {
        let bytes = count
            .checked_mul(record_size)
            .ok_or(ImportError::SectionOverflow)?;
        self.require(bytes)
    }

    fn read_record_count(&mut self, record_size: usize) -> Result<usize, ImportError> {
        let count = self.read_u32_le()? as usize;
        self.require_record_bytes(count, record_size)?;
        Ok(count)
    }

    fn skip_optional_ignored_records(&mut self, record_size: usize) -> Result<bool, ImportError> {
        let Some(count) = self.read_optional_u32_le()? else {
            return Ok(false);
        };
        let Some(bytes) = (count as usize).checked_mul(record_size) else {
            self.pos = self.data.len();
            return Ok(false);
        };
        if bytes > self.remaining() {
            self.pos = self.data.len();
            return Ok(false);
        }
        self.read_slice(bytes)?;
        Ok(true)
    }

    fn read_optional_record_count(
        &mut self,
        record_size: usize,
    ) -> Result<Option<usize>, ImportError> {
        let Some(count) = self.read_optional_u32_le()? else {
            return Ok(None);
        };
        let count = count as usize;
        let Some(bytes) = count.checked_mul(record_size) else {
            self.pos = self.data.len();
            return Ok(None);
        };
        if bytes > self.remaining() {
            self.pos = self.data.len();
            return Ok(None);
        }
        Ok(Some(count))
    }

    fn read_f32_le(&mut self) -> Result<f32, ImportError> {
        let b = self.read_slice(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_vec3(&mut self) -> Result<Vec3A, ImportError> {
        Ok(Vec3A::new(
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
        ))
    }

    fn read_quat(&mut self) -> Result<Quat, ImportError> {
        Ok(Quat::from_xyzw(
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
        ))
    }

    fn read_shifts_jis_name(&mut self) -> Result<Vec<u8>, ImportError> {
        let raw = self.read_slice(15)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        Ok(raw[..end].to_vec())
    }
}

#[derive(Debug, Clone)]
pub struct VmdHeader {
    pub model_name_bytes: [u8; 20],
}

#[derive(Debug, Clone)]
pub enum VmdBoneImportMode {
    ByName(Vec<u8>),
    ByIndex(u32),
}

#[derive(Debug, Clone)]
pub struct VmdBoneKeyframeRaw {
    pub bone_mode: VmdBoneImportMode,
    pub frame: u32,
    pub position: Vec3A,
    pub rotation: Quat,
    pub interpolation: [u8; 64],
    pub bone_name_normalized: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct VmdIkEntry {
    pub name_bytes: Vec<u8>,
    pub enabled: u8,
    pub name_normalized: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct VmdPropertyIkFrame {
    pub frame: u32,
    pub show: u8,
    pub entries: Vec<VmdIkEntry>,
}

#[derive(Debug, Clone)]
pub struct VmdImportResult {
    pub bone_keyframes: Vec<VmdBoneKeyframeRaw>,
    pub morph_keyframes: Vec<(Vec<u8>, u32, f32)>,
    pub property_keyframes: Vec<PropertyKeyframe>,
    pub property_ik_frames: Vec<VmdPropertyIkFrame>,
}

pub fn read_header(data: &[u8]) -> Result<(VmdHeader, usize), ImportError> {
    let mut r = Reader::new(data);

    let magic = r.read_slice(30)?;
    if magic != VMD_MAGIC && !magic.starts_with(VMD_MAGIC_PREFIX) {
        return Err(ImportError::InvalidVmdMagic);
    }

    let model_name_bytes: [u8; 20] = r
        .read_slice(20)?
        .try_into()
        .map_err(|_| ImportError::InvalidVmdModelName)?;

    Ok((VmdHeader { model_name_bytes }, r.pos))
}

pub fn import_vmd_motion(data: &[u8]) -> Result<VmdImportResult, ImportError> {
    let (_header, pos) = read_header(data)?;
    let mut r = Reader { data, pos };

    let bone_count = r.read_record_count(111)?;
    let mut bone_keyframes = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        let bone_name = r.read_shifts_jis_name()?;
        let frame = r.read_u32_le()?;
        let position = r.read_vec3()?;
        let rotation = r.read_quat()?;
        let interpolation: [u8; 64] = r.read_slice(64)?.try_into().unwrap();

        let bone_name_normalized = normalize_vmd_name(&bone_name);

        bone_keyframes.push(VmdBoneKeyframeRaw {
            bone_mode: VmdBoneImportMode::ByName(bone_name),
            frame,
            position,
            rotation,
            interpolation,
            bone_name_normalized,
        });
    }

    let Some(morph_count) = r.read_optional_u32_le()? else {
        return Ok(VmdImportResult {
            bone_keyframes,
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        });
    };
    let morph_count = morph_count as usize;
    r.require_record_bytes(morph_count, 23)?;
    let mut morph_keyframes = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        let morph_name = r.read_shifts_jis_name()?;
        let frame = r.read_u32_le()?;
        let weight = r.read_f32_le()?;
        morph_keyframes.push((morph_name, frame, weight));
    }

    if !r.skip_optional_ignored_records(61)? {
        return Ok(VmdImportResult {
            bone_keyframes,
            morph_keyframes,
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        });
    }

    if !r.skip_optional_ignored_records(28)? {
        return Ok(VmdImportResult {
            bone_keyframes,
            morph_keyframes,
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        });
    }

    if !r.skip_optional_ignored_records(9)? {
        return Ok(VmdImportResult {
            bone_keyframes,
            morph_keyframes,
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        });
    }

    let Some(show_ik_count) = r.read_optional_u32_le()? else {
        return Ok(VmdImportResult {
            bone_keyframes,
            morph_keyframes,
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        });
    };
    let show_ik_count = show_ik_count as usize;
    r.require_record_bytes(show_ik_count, 9)?;
    let mut property_keyframes = Vec::with_capacity(show_ik_count);
    let mut property_ik_frames = Vec::with_capacity(show_ik_count);
    for _ in 0..show_ik_count {
        let frame = r.read_u32_le()?;
        let show = r.read_u8()?;
        let ik_count = r.read_u32_le()? as usize;
        r.require_record_bytes(ik_count, 21)?;
        let mut ik_enabled = Vec::with_capacity(ik_count);
        let mut ik_entries = Vec::with_capacity(ik_count);
        for _ in 0..ik_count {
            let ik_name_bytes = r.read_slice(20)?.to_vec();
            let enabled = r.read_u8()?;
            let end = ik_name_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(ik_name_bytes.len());
            let name_normalized = normalize_vmd_name(&ik_name_bytes[..end]);
            ik_enabled.push(enabled);
            ik_entries.push(VmdIkEntry {
                name_bytes: ik_name_bytes,
                enabled,
                name_normalized,
            });
        }
        property_keyframes.push(PropertyKeyframe::new(
            frame,
            ik_enabled.iter().map(|&b| b != 0).collect(),
        ));
        property_ik_frames.push(VmdPropertyIkFrame {
            frame,
            show,
            entries: ik_entries,
        });
    }

    Ok(VmdImportResult {
        bone_keyframes,
        morph_keyframes,
        property_keyframes,
        property_ik_frames,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedAnimation {
    #[serde(default = "default_vmd_kind", skip_deserializing)]
    pub kind: &'static str,
    pub metadata: VmdParsedMetadata,
    pub bone_frames: Vec<VmdParsedBoneFrame>,
    pub morph_frames: Vec<VmdParsedMorphFrame>,
    pub camera_frames: Vec<VmdParsedCameraFrame>,
    pub light_frames: Vec<VmdParsedLightFrame>,
    pub self_shadow_frames: Vec<VmdParsedSelfShadowFrame>,
    pub property_frames: Vec<VmdParsedPropertyFrame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedMetadata {
    #[serde(default = "default_vmd_format", skip_deserializing)]
    pub format: &'static str,
    pub model_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_name_bytes: Vec<u8>,
    pub counts: VmdParsedCounts,
    pub max_frame: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmdParsedCounts {
    pub bones: usize,
    pub morphs: usize,
    pub cameras: usize,
    pub lights: usize,
    #[serde(rename = "selfShadows")]
    pub self_shadows: usize,
    pub properties: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedBoneFrame {
    pub bone_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bone_name_bytes: Vec<u8>,
    pub frame: u32,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub interpolation: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedMorphFrame {
    pub morph_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub morph_name_bytes: Vec<u8>,
    pub frame: u32,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedCameraFrame {
    pub frame: u32,
    pub distance: f32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub interpolation: [u8; 24],
    pub fov: u32,
    pub perspective: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdCameraState {
    pub distance: f32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub fov: f32,
    pub perspective: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedLightFrame {
    pub frame: u32,
    pub color: [f32; 3],
    pub direction: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedSelfShadowFrame {
    pub frame: u32,
    pub mode: u8,
    pub distance: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedPropertyFrame {
    pub frame: u32,
    pub visible: bool,
    pub ik_states: Vec<VmdParsedIkState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdParsedIkState {
    pub bone_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bone_name_bytes: Vec<u8>,
    pub enabled: bool,
}

pub fn parse_vmd_animation(data: &[u8]) -> Result<VmdParsedAnimation, ImportError> {
    let (_header, pos) = read_header(data)?;
    let mut r = Reader { data, pos };
    let model_name = decode_sjis_fixed(&_header.model_name_bytes);
    let model_name_bytes = trim_fixed_bytes(&_header.model_name_bytes).to_vec();
    let mut max_frame = 0u32;

    let bone_count = r.read_record_count(111)?;
    let mut bone_frames = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        let bone_name_bytes = r.read_slice(15)?;
        let bone_name = decode_sjis_fixed(bone_name_bytes);
        let frame = r.read_u32_le()?;
        max_frame = max_frame.max(frame);
        let position = r.read_vec3()?;
        let rotation = r.read_quat()?;
        let interpolation: [u8; 64] = r.read_slice(64)?.try_into().unwrap();
        bone_frames.push(VmdParsedBoneFrame {
            bone_name,
            bone_name_bytes: trim_fixed_bytes(bone_name_bytes).to_vec(),
            frame,
            translation: [position.x, position.y, position.z],
            rotation: [rotation.x, rotation.y, rotation.z, rotation.w],
            interpolation: interpolation.to_vec(),
        });
    }

    let Some(morph_count) = r.read_optional_u32_le()? else {
        return Ok(vmd_parsed_animation(
            model_name,
            model_name_bytes,
            max_frame,
            VmdParsedSections {
                bone_frames,
                morph_frames: Vec::new(),
                camera_frames: Vec::new(),
                light_frames: Vec::new(),
                self_shadow_frames: Vec::new(),
                property_frames: Vec::new(),
            },
        ));
    };
    let morph_count = morph_count as usize;
    r.require_record_bytes(morph_count, 23)?;
    let mut morph_frames = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        let morph_name_bytes = r.read_slice(15)?;
        let morph_name = decode_sjis_fixed(morph_name_bytes);
        let frame = r.read_u32_le()?;
        max_frame = max_frame.max(frame);
        morph_frames.push(VmdParsedMorphFrame {
            morph_name,
            morph_name_bytes: trim_fixed_bytes(morph_name_bytes).to_vec(),
            frame,
            weight: r.read_f32_le()?,
        });
    }

    let camera_frames = read_parsed_camera_frames(&mut r, &mut max_frame)?;
    let light_frames = read_parsed_light_frames(&mut r, &mut max_frame)?;
    let self_shadow_frames = read_parsed_self_shadow_frames(&mut r, &mut max_frame)?;
    let property_frames = read_parsed_property_frames(&mut r, &mut max_frame)?;

    Ok(vmd_parsed_animation(
        model_name,
        model_name_bytes,
        max_frame,
        VmdParsedSections {
            bone_frames,
            morph_frames,
            camera_frames,
            light_frames,
            self_shadow_frames,
            property_frames,
        },
    ))
}

fn default_vmd_kind() -> &'static str {
    "vmd"
}

fn default_vmd_format() -> &'static str {
    "vmd"
}

pub fn export_vmd_animation(animation: &VmdParsedAnimation) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&VMD_MAGIC);
    write_fixed_name_bytes(
        &mut out,
        &animation.metadata.model_name,
        &animation.metadata.model_name_bytes,
        20,
    );

    write_u32(&mut out, animation.bone_frames.len() as u32);
    for frame in &animation.bone_frames {
        write_fixed_name_bytes(&mut out, &frame.bone_name, &frame.bone_name_bytes, 15);
        write_u32(&mut out, frame.frame);
        write_f32(&mut out, frame.translation[0]);
        write_f32(&mut out, frame.translation[1]);
        write_f32(&mut out, frame.translation[2]);
        write_f32(&mut out, frame.rotation[0]);
        write_f32(&mut out, frame.rotation[1]);
        write_f32(&mut out, frame.rotation[2]);
        write_f32(&mut out, frame.rotation[3]);
        write_fixed_bytes(&mut out, &frame.interpolation, 64);
    }

    write_u32(&mut out, animation.morph_frames.len() as u32);
    for frame in &animation.morph_frames {
        write_fixed_name_bytes(&mut out, &frame.morph_name, &frame.morph_name_bytes, 15);
        write_u32(&mut out, frame.frame);
        write_f32(&mut out, frame.weight);
    }

    write_u32(&mut out, animation.camera_frames.len() as u32);
    for frame in &animation.camera_frames {
        write_u32(&mut out, frame.frame);
        write_f32(&mut out, frame.distance);
        write_f32(&mut out, frame.position[0]);
        write_f32(&mut out, frame.position[1]);
        write_f32(&mut out, frame.position[2]);
        write_f32(&mut out, frame.rotation[0]);
        write_f32(&mut out, frame.rotation[1]);
        write_f32(&mut out, frame.rotation[2]);
        out.extend_from_slice(&frame.interpolation);
        write_u32(&mut out, frame.fov);
        out.push(if frame.perspective { 0 } else { 1 });
    }

    write_u32(&mut out, animation.light_frames.len() as u32);
    for frame in &animation.light_frames {
        write_u32(&mut out, frame.frame);
        write_f32(&mut out, frame.color[0]);
        write_f32(&mut out, frame.color[1]);
        write_f32(&mut out, frame.color[2]);
        write_f32(&mut out, frame.direction[0]);
        write_f32(&mut out, frame.direction[1]);
        write_f32(&mut out, frame.direction[2]);
    }

    write_u32(&mut out, animation.self_shadow_frames.len() as u32);
    for frame in &animation.self_shadow_frames {
        write_u32(&mut out, frame.frame);
        out.push(frame.mode);
        write_f32(&mut out, frame.distance);
    }

    write_u32(&mut out, animation.property_frames.len() as u32);
    for frame in &animation.property_frames {
        write_u32(&mut out, frame.frame);
        out.push(u8::from(frame.visible));
        write_u32(&mut out, frame.ik_states.len() as u32);
        for state in &frame.ik_states {
            write_fixed_name_bytes(&mut out, &state.bone_name, &state.bone_name_bytes, 20);
            out.push(u8::from(state.enabled));
        }
    }

    out
}

struct VmdParsedSections {
    bone_frames: Vec<VmdParsedBoneFrame>,
    morph_frames: Vec<VmdParsedMorphFrame>,
    camera_frames: Vec<VmdParsedCameraFrame>,
    light_frames: Vec<VmdParsedLightFrame>,
    self_shadow_frames: Vec<VmdParsedSelfShadowFrame>,
    property_frames: Vec<VmdParsedPropertyFrame>,
}

fn vmd_parsed_animation(
    model_name: String,
    model_name_bytes: Vec<u8>,
    max_frame: u32,
    sections: VmdParsedSections,
) -> VmdParsedAnimation {
    VmdParsedAnimation {
        kind: "vmd",
        metadata: VmdParsedMetadata {
            format: "vmd",
            model_name,
            model_name_bytes,
            counts: VmdParsedCounts {
                bones: sections.bone_frames.len(),
                morphs: sections.morph_frames.len(),
                cameras: sections.camera_frames.len(),
                lights: sections.light_frames.len(),
                self_shadows: sections.self_shadow_frames.len(),
                properties: sections.property_frames.len(),
            },
            max_frame,
        },
        bone_frames: sections.bone_frames,
        morph_frames: sections.morph_frames,
        camera_frames: sections.camera_frames,
        light_frames: sections.light_frames,
        self_shadow_frames: sections.self_shadow_frames,
        property_frames: sections.property_frames,
    }
}

fn read_parsed_camera_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
) -> Result<Vec<VmdParsedCameraFrame>, ImportError> {
    let Some(count) = r.read_optional_record_count(61)? else {
        return Ok(Vec::new());
    };
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        *max_frame = (*max_frame).max(frame);
        frames.push(VmdParsedCameraFrame {
            frame,
            distance: r.read_f32_le()?,
            position: {
                let p = r.read_vec3()?;
                [p.x, p.y, p.z]
            },
            rotation: {
                let p = r.read_vec3()?;
                [p.x, p.y, p.z]
            },
            interpolation: r.read_slice(24)?.try_into().unwrap(),
            fov: r.read_u32_le()?,
            perspective: r.read_u8()? == 0,
        });
    }
    Ok(frames)
}

fn read_parsed_light_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
) -> Result<Vec<VmdParsedLightFrame>, ImportError> {
    let Some(count) = r.read_optional_record_count(28)? else {
        return Ok(Vec::new());
    };
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        *max_frame = (*max_frame).max(frame);
        let color = r.read_vec3()?;
        let direction = r.read_vec3()?;
        frames.push(VmdParsedLightFrame {
            frame,
            color: [color.x, color.y, color.z],
            direction: [direction.x, direction.y, direction.z],
        });
    }
    Ok(frames)
}

fn read_parsed_self_shadow_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
) -> Result<Vec<VmdParsedSelfShadowFrame>, ImportError> {
    let Some(count) = r.read_optional_record_count(9)? else {
        return Ok(Vec::new());
    };
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        *max_frame = (*max_frame).max(frame);
        frames.push(VmdParsedSelfShadowFrame {
            frame,
            mode: r.read_u8()?,
            distance: r.read_f32_le()?,
        });
    }
    Ok(frames)
}

fn read_parsed_property_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
) -> Result<Vec<VmdParsedPropertyFrame>, ImportError> {
    let Some(count) = r.read_optional_u32_le()? else {
        return Ok(Vec::new());
    };
    let count = count as usize;
    r.require_record_bytes(count, 9)?;
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        *max_frame = (*max_frame).max(frame);
        let visible = r.read_u8()? != 0;
        let ik_count = r.read_u32_le()? as usize;
        r.require_record_bytes(ik_count, 21)?;
        let mut ik_states = Vec::with_capacity(ik_count);
        for _ in 0..ik_count {
            let bone_name_bytes = r.read_slice(20)?;
            ik_states.push(VmdParsedIkState {
                bone_name: decode_sjis_fixed(bone_name_bytes),
                bone_name_bytes: trim_fixed_bytes(bone_name_bytes).to_vec(),
                enabled: r.read_u8()? != 0,
            });
        }
        frames.push(VmdParsedPropertyFrame {
            frame,
            visible,
            ik_states,
        });
    }
    Ok(frames)
}

pub fn sample_vmd_camera_frames(
    frames: &[VmdParsedCameraFrame],
    frame: f32,
) -> Option<VmdCameraState> {
    if frames.is_empty() {
        return None;
    }

    let mut sorted: Vec<&VmdParsedCameraFrame> = frames.iter().collect();
    sorted.sort_by_key(|keyframe| keyframe.frame);

    let mut index = 0usize;
    while index + 1 < sorted.len() && sorted[index + 1].frame as f32 <= frame {
        index += 1;
    }

    let previous = sorted[index];
    let next = sorted.get(index + 1).copied().unwrap_or(previous);
    let t = interpolation_ratio(previous.frame, next.frame, frame);
    let interpolation = decode_camera_interpolation(&next.interpolation);

    let distance_t = interpolation.distance.evaluate(t);
    let position_x_t = interpolation.position.x.evaluate(t);
    let position_y_t = interpolation.position.y.evaluate(t);
    let position_z_t = interpolation.position.z.evaluate(t);
    let rotation_t = interpolation.rotation.evaluate(t);
    let fov_t = interpolation.fov.evaluate(t);

    Some(VmdCameraState {
        distance: lerp(previous.distance, next.distance, distance_t),
        position: [
            lerp(previous.position[0], next.position[0], position_x_t),
            lerp(previous.position[1], next.position[1], position_y_t),
            lerp(previous.position[2], next.position[2], position_z_t),
        ],
        rotation: [
            lerp(previous.rotation[0], next.rotation[0], rotation_t),
            lerp(previous.rotation[1], next.rotation[1], rotation_t),
            lerp(previous.rotation[2], next.rotation[2], rotation_t),
        ],
        fov: lerp(previous.fov as f32, next.fov as f32, fov_t),
        perspective: if t < 1.0 {
            previous.perspective
        } else {
            next.perspective
        },
    })
}

struct CameraInterpolation {
    position: InterpolationVector3,
    rotation: InterpolationScalar,
    distance: InterpolationScalar,
    fov: InterpolationScalar,
}

fn decode_camera_interpolation(interpolation: &[u8; 24]) -> CameraInterpolation {
    CameraInterpolation {
        position: InterpolationVector3 {
            x: decode_camera_interpolation_scalar(interpolation, 0),
            y: decode_camera_interpolation_scalar(interpolation, 1),
            z: decode_camera_interpolation_scalar(interpolation, 2),
        },
        rotation: decode_camera_interpolation_scalar(interpolation, 3),
        distance: decode_camera_interpolation_scalar(interpolation, 4),
        fov: decode_camera_interpolation_scalar(interpolation, 5),
    }
}

fn decode_camera_interpolation_scalar(
    interpolation: &[u8; 24],
    channel: usize,
) -> InterpolationScalar {
    let offset = channel * 4;
    decode_interpolation_scalar([
        interpolation[offset],
        interpolation[offset + 1],
        interpolation[offset + 2],
        interpolation[offset + 3],
    ])
}

fn interpolation_ratio(previous_frame: u32, next_frame: u32, frame: f32) -> f32 {
    if next_frame <= previous_frame {
        return 0.0;
    }
    let span = next_frame - previous_frame;
    if span <= 1 {
        return if frame >= next_frame as f32 { 1.0 } else { 0.0 };
    }
    ((frame - previous_frame as f32) / span as f32).clamp(0.0, 1.0)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn decode_sjis_fixed(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let (decoded, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes[..end]);
    decoded.trim().to_owned()
}

fn trim_fixed_bytes(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_fixed_name_bytes(out: &mut Vec<u8>, value: &str, raw_bytes: &[u8], len: usize) {
    if raw_bytes.is_empty() {
        let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(value);
        write_fixed_bytes(out, encoded.as_ref(), len);
    } else {
        write_fixed_bytes(out, raw_bytes, len);
    }
}

fn write_fixed_bytes(out: &mut Vec<u8>, value: &[u8], len: usize) {
    let copied = value.len().min(len);
    out.extend_from_slice(&value[..copied]);
    out.resize(out.len() + len - copied, 0);
}

fn decode_interpolation_scalar(data: [u8; 4]) -> InterpolationScalar {
    InterpolationScalar {
        x1: data[0].min(127),
        y1: data[1].min(127),
        x2: data[2].min(127),
        y2: data[3].min(127),
    }
}

fn decode_bone_interpolation(
    interpolation: &[u8; 64],
) -> (InterpolationVector3, InterpolationScalar) {
    let position = InterpolationVector3 {
        x: decode_interpolation_scalar([
            interpolation[0],
            interpolation[4],
            interpolation[8],
            interpolation[12],
        ]),
        y: decode_interpolation_scalar([
            interpolation[1],
            interpolation[5],
            interpolation[9],
            interpolation[13],
        ]),
        z: decode_interpolation_scalar([
            interpolation[2],
            interpolation[6],
            interpolation[10],
            interpolation[14],
        ]),
    };
    let rotation = decode_interpolation_scalar([
        interpolation[3],
        interpolation[7],
        interpolation[11],
        interpolation[15],
    ]);
    (position, rotation)
}

pub fn build_clip_from_import(
    result: VmdImportResult,
    bone_name_to_index: &dyn Fn(&[u8]) -> Option<BoneIndex>,
    morph_name_to_index: &dyn Fn(&[u8]) -> Option<MorphIndex>,
) -> AnimationClip {
    let mut bone_tracks_map: std::collections::BTreeMap<u32, Vec<MovableBoneKeyframe>> =
        std::collections::BTreeMap::new();

    for kf in result.bone_keyframes {
        let bone_index = match &kf.bone_mode {
            VmdBoneImportMode::ByName(_name) => {
                match bone_name_to_index(&kf.bone_name_normalized) {
                    Some(idx) => idx,
                    None => continue,
                }
            }
            VmdBoneImportMode::ByIndex(idx) => BoneIndex(*idx),
        };

        let (pos_interp, rot_interp) = decode_bone_interpolation(&kf.interpolation);

        bone_tracks_map
            .entry(bone_index.0)
            .or_default()
            .push(MovableBoneKeyframe {
                frame: kf.frame,
                position: kf.position,
                rotation: kf.rotation,
                position_interpolation: pos_interp,
                rotation_interpolation: rot_interp,
            });
    }

    let bone_tracks: Vec<BoneAnimationBinding> = bone_tracks_map
        .into_iter()
        .map(|(bone_idx, kfs)| BoneAnimationBinding {
            bone: BoneIndex(bone_idx),
            track: MovableBoneTrack::from_keyframes(kfs),
        })
        .collect();

    let mut morph_tracks_map: std::collections::BTreeMap<u32, Vec<MorphKeyframe>> =
        std::collections::BTreeMap::new();

    for (morph_name, frame, weight) in result.morph_keyframes {
        let morph_name_normalized = normalize_vmd_name(&morph_name);
        let morph_index = match morph_name_to_index(&morph_name_normalized) {
            Some(idx) => idx,
            None => continue,
        };
        morph_tracks_map
            .entry(morph_index.0)
            .or_default()
            .push(MorphKeyframe::new(frame, weight));
    }

    let morph_tracks: Vec<MorphAnimationBinding> = morph_tracks_map
        .into_iter()
        .map(|(morph_idx, kfs)| MorphAnimationBinding {
            morph: MorphIndex(morph_idx),
            track: MorphTrack::from_keyframes(kfs),
        })
        .collect();

    let property_track = if result.property_keyframes.is_empty() {
        None
    } else {
        Some(PropertyAnimationBinding::from_keyframes(
            result.property_keyframes,
        ))
    };

    AnimationClip::new_full(bone_tracks, morph_tracks, property_track)
}

pub fn build_property_binding_with_ik_resolver(
    ik_frames: &[VmdPropertyIkFrame],
    ik_name_to_solver_index: &dyn Fn(&[u8]) -> Option<usize>,
    solver_count: usize,
) -> Option<PropertyAnimationBinding> {
    if solver_count == 0 || ik_frames.is_empty() {
        return None;
    }

    let keyframes: Vec<PropertyKeyframe> = ik_frames
        .iter()
        .map(|frame| {
            let mut ik_enabled = vec![1u8; solver_count];
            for entry in &frame.entries {
                match ik_name_to_solver_index(&entry.name_normalized) {
                    Some(idx) if idx < solver_count => {
                        ik_enabled[idx] = entry.enabled;
                    }
                    _ => {}
                }
            }
            PropertyKeyframe {
                frame: frame.frame,
                ik_enabled: ik_enabled.into_boxed_slice(),
            }
        })
        .collect();

    Some(PropertyAnimationBinding::from_keyframes(keyframes))
}

/// Options for controlling VMD clip construction behavior.
#[derive(Debug, Clone, Copy)]
pub struct VmdClipBuildOptions {
    /// When `true` (the default), the property IK enable/disable data from the
    /// VMD is baked into the clip. When `false`, the clip omits the property
    /// track entirely, so all IK solvers remain at their runtime default
    /// (enabled) state — useful when comparing against toolchain outputs that
    /// do not preserve property IK.
    pub honor_property_ik: bool,
}

impl Default for VmdClipBuildOptions {
    fn default() -> Self {
        Self {
            honor_property_ik: true,
        }
    }
}

pub fn build_pair_clip(
    result: &VmdImportResult,
    bone_name_to_index: &std::collections::HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: &std::collections::HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: &std::collections::HashMap<Vec<u8>, usize>,
    solver_count: usize,
) -> AnimationClip {
    build_pair_clip_with_options(
        result,
        bone_name_to_index,
        morph_name_to_index,
        ik_solver_bone_name_to_index,
        solver_count,
        VmdClipBuildOptions::default(),
    )
}

pub fn build_pair_clip_with_options(
    result: &VmdImportResult,
    bone_name_to_index: &std::collections::HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: &std::collections::HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: &std::collections::HashMap<Vec<u8>, usize>,
    solver_count: usize,
    options: VmdClipBuildOptions,
) -> AnimationClip {
    let mut bone_tracks_map: std::collections::BTreeMap<u32, Vec<MovableBoneKeyframe>> =
        std::collections::BTreeMap::new();

    for kf in &result.bone_keyframes {
        let bone_index = match bone_name_to_index.get(&kf.bone_name_normalized) {
            Some(idx) => *idx,
            None => continue,
        };

        let (pos_interp, rot_interp) = decode_bone_interpolation(&kf.interpolation);

        bone_tracks_map
            .entry(bone_index.0)
            .or_default()
            .push(MovableBoneKeyframe {
                frame: kf.frame,
                position: kf.position,
                rotation: kf.rotation,
                position_interpolation: pos_interp,
                rotation_interpolation: rot_interp,
            });
    }

    let bone_tracks: Vec<BoneAnimationBinding> = bone_tracks_map
        .into_iter()
        .map(|(bone_idx, kfs)| BoneAnimationBinding {
            bone: BoneIndex(bone_idx),
            track: MovableBoneTrack::from_keyframes(kfs),
        })
        .collect();

    let mut morph_tracks_map: std::collections::BTreeMap<u32, Vec<MorphKeyframe>> =
        std::collections::BTreeMap::new();

    for (morph_name, frame, weight) in &result.morph_keyframes {
        let morph_name_normalized = normalize_vmd_name(morph_name);
        let morph_index = match morph_name_to_index.get(&morph_name_normalized) {
            Some(idx) => *idx,
            None => continue,
        };
        morph_tracks_map
            .entry(morph_index.0)
            .or_default()
            .push(MorphKeyframe::new(*frame, *weight));
    }

    let morph_tracks: Vec<MorphAnimationBinding> = morph_tracks_map
        .into_iter()
        .map(|(morph_idx, kfs)| MorphAnimationBinding {
            morph: MorphIndex(morph_idx),
            track: MorphTrack::from_keyframes(kfs),
        })
        .collect();

    let property_track = if options.honor_property_ik {
        build_property_binding_with_ik_resolver(
            &result.property_ik_frames,
            &|name| ik_solver_bone_name_to_index.get(name).copied(),
            solver_count,
        )
    } else {
        None
    };

    AnimationClip::new_full(bone_tracks, morph_tracks, property_track)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use serde_json;

    fn build_vmd_header_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&VMD_MAGIC);
        buf.extend_from_slice(&[0u8; 20]);
        buf
    }

    #[test]
    fn rejects_impossible_vmd_bone_count_before_allocation() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&u32::MAX.to_le_bytes());

        assert!(matches!(
            import_vmd_motion(&buf),
            Err(ImportError::UnexpectedEof(_))
        ));
        assert!(matches!(
            parse_vmd_animation(&buf),
            Err(ImportError::UnexpectedEof(_))
        ));
    }

    #[test]
    fn rejects_impossible_vmd_property_ik_count_before_allocation() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes()); // bones
        buf.extend_from_slice(&0u32.to_le_bytes()); // morphs
        buf.extend_from_slice(&0u32.to_le_bytes()); // cameras
        buf.extend_from_slice(&0u32.to_le_bytes()); // lights
        buf.extend_from_slice(&0u32.to_le_bytes()); // self shadows
        buf.extend_from_slice(&1u32.to_le_bytes()); // property frames
        buf.extend_from_slice(&0u32.to_le_bytes()); // frame
        buf.push(1); // visible
        buf.extend_from_slice(&u32::MAX.to_le_bytes()); // IK entries

        assert!(matches!(
            import_vmd_motion(&buf),
            Err(ImportError::UnexpectedEof(_))
        ));
        assert!(matches!(
            parse_vmd_animation(&buf),
            Err(ImportError::UnexpectedEof(_))
        ));
    }

    #[test]
    fn parses_vmd_header() {
        let header_bytes = build_vmd_header_bytes();
        let (header, pos) = read_header(&header_bytes).unwrap();
        assert_eq!(header.model_name_bytes, [0u8; 20]);
        assert!(pos > 0);
    }

    #[test]
    fn rejects_bad_vmd_magic() {
        let mut buf = build_vmd_header_bytes();
        buf[0] = 0xFF;
        assert_eq!(read_header(&buf).unwrap_err(), ImportError::InvalidVmdMagic);
    }

    #[test]
    fn accepts_vmd_magic_with_nonzero_padding_bytes() {
        let mut buf = build_vmd_header_bytes();
        buf[26..30].copy_from_slice(b"JKLM");

        let (_header, pos) = read_header(&buf).unwrap();
        assert_eq!(pos, 50);
    }

    #[test]
    fn parses_minimal_vmd_motion() {
        let mut buf = build_vmd_header_bytes();

        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        let result = import_vmd_motion(&buf).unwrap();
        assert!(result.bone_keyframes.is_empty());
        assert!(result.morph_keyframes.is_empty());
        assert!(result.property_keyframes.is_empty());
    }

    #[test]
    fn accepts_vmd_without_optional_tail_sections() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes());

        let result = import_vmd_motion(&buf).unwrap();
        assert!(result.bone_keyframes.is_empty());
        assert!(result.morph_keyframes.is_empty());
        assert!(result.property_keyframes.is_empty());
        assert!(result.property_ik_frames.is_empty());
    }

    #[test]
    fn accepts_vmd_ending_after_shadow_section() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        let result = import_vmd_motion(&buf).unwrap();
        assert!(result.bone_keyframes.is_empty());
        assert!(result.morph_keyframes.is_empty());
        assert!(result.property_keyframes.is_empty());
        assert!(result.property_ik_frames.is_empty());
    }

    #[test]
    fn rejects_partial_optional_count() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&[1, 2, 3]);

        assert_eq!(
            import_vmd_motion(&buf).unwrap_err(),
            ImportError::UnexpectedEof(1)
        );
    }

    #[test]
    fn ignores_truncated_unused_tail_sections() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 8]);

        let result = import_vmd_motion(&buf).unwrap();
        assert!(result.bone_keyframes.is_empty());
        assert!(result.morph_keyframes.is_empty());
        assert!(result.property_keyframes.is_empty());
        assert!(result.property_ik_frames.is_empty());
    }

    #[test]
    fn parsed_animation_ignores_implausible_optional_tail() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes()); // bones
        buf.extend_from_slice(&0u32.to_le_bytes()); // morphs
        buf.extend_from_slice(&0u32.to_le_bytes()); // camera count

        // Some real-world VMD files contain a 61-byte camera-shaped tail after
        // a zero camera count. Treat it as an optional malformed tail instead
        // of failing the parser/exporter gate.
        buf.extend_from_slice(&300u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 57]);

        let parsed = parse_vmd_animation(&buf).unwrap();
        assert!(parsed.camera_frames.is_empty());
        assert!(parsed.light_frames.is_empty());
        assert!(parsed.self_shadow_frames.is_empty());
        assert!(parsed.property_frames.is_empty());
    }

    #[test]
    fn parses_single_bone_keyframe() {
        let mut buf = build_vmd_header_bytes();

        buf.extend_from_slice(&1u32.to_le_bytes());
        let mut bone_name = [0u8; 15];
        bone_name[..4].copy_from_slice(b"Bone");
        buf.extend_from_slice(&bone_name);
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&3.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&[20u8; 64]);

        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        let result = import_vmd_motion(&buf).unwrap();
        assert_eq!(result.bone_keyframes.len(), 1);
        assert_eq!(result.bone_keyframes[0].frame, 10);
        assert!((result.bone_keyframes[0].position.x - 1.0).abs() < 0.001);
        assert!((result.bone_keyframes[0].position.y - 2.0).abs() < 0.001);
        assert!((result.bone_keyframes[0].position.z - 3.0).abs() < 0.001);
        assert_eq!(&result.bone_keyframes[0].bone_name_normalized[..], b"Bone");
    }

    #[test]
    fn exports_parsed_vmd_animation_for_roundtrip() {
        let mut buf = build_vmd_header_bytes();

        buf.extend_from_slice(&1u32.to_le_bytes());
        let mut bone_name = [0u8; 15];
        bone_name[..4].copy_from_slice(b"Bone");
        buf.extend_from_slice(&bone_name);
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&3.0f32.to_le_bytes());
        buf.extend_from_slice(&0.1f32.to_le_bytes());
        buf.extend_from_slice(&0.2f32.to_le_bytes());
        buf.extend_from_slice(&0.3f32.to_le_bytes());
        buf.extend_from_slice(&0.4f32.to_le_bytes());
        buf.extend_from_slice(&[20u8; 64]);

        buf.extend_from_slice(&1u32.to_le_bytes());
        let mut morph_name = [0u8; 15];
        morph_name[..5].copy_from_slice(b"Smile");
        buf.extend_from_slice(&morph_name);
        buf.extend_from_slice(&11u32.to_le_bytes());
        buf.extend_from_slice(&0.75f32.to_le_bytes());

        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&30.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&3.0f32.to_le_bytes());
        buf.extend_from_slice(&0.1f32.to_le_bytes());
        buf.extend_from_slice(&0.2f32.to_le_bytes());
        buf.extend_from_slice(&0.3f32.to_le_bytes());
        buf.extend_from_slice(&[30u8; 24]);
        buf.extend_from_slice(&45u32.to_le_bytes());
        buf.push(0);

        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&13u32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.5f32.to_le_bytes());
        buf.extend_from_slice(&0.25f32.to_le_bytes());
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&(-0.5f32).to_le_bytes());
        buf.extend_from_slice(&(-0.25f32).to_le_bytes());

        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&14u32.to_le_bytes());
        buf.push(2);
        buf.extend_from_slice(&0.6f32.to_le_bytes());

        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&15u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&1u32.to_le_bytes());
        let mut ik_name = [0u8; 20];
        ik_name[..2].copy_from_slice(b"IK");
        buf.extend_from_slice(&ik_name);
        buf.push(1);

        let parsed = parse_vmd_animation(&buf).unwrap();
        let exported = export_vmd_animation(&parsed);
        let reparsed = parse_vmd_animation(&exported).unwrap();

        assert_vmd_roundtrip_eq(&parsed, &reparsed);
    }

    fn assert_vmd_roundtrip_eq(left: &VmdParsedAnimation, right: &VmdParsedAnimation) {
        assert_eq!(left.metadata.model_name, right.metadata.model_name);
        assert_eq!(left.metadata.max_frame, right.metadata.max_frame);
        assert_eq!(left.bone_frames.len(), right.bone_frames.len());
        assert_eq!(left.morph_frames.len(), right.morph_frames.len());
        assert_eq!(left.camera_frames.len(), right.camera_frames.len());
        assert_eq!(left.light_frames.len(), right.light_frames.len());
        assert_eq!(
            left.self_shadow_frames.len(),
            right.self_shadow_frames.len()
        );
        assert_eq!(left.property_frames.len(), right.property_frames.len());
        assert_eq!(
            left.bone_frames[0].bone_name,
            right.bone_frames[0].bone_name
        );
        assert_eq!(left.bone_frames[0].frame, right.bone_frames[0].frame);
        assert_eq!(
            left.bone_frames[0].translation,
            right.bone_frames[0].translation
        );
        assert_eq!(left.bone_frames[0].rotation, right.bone_frames[0].rotation);
        assert_eq!(
            left.bone_frames[0].interpolation,
            right.bone_frames[0].interpolation
        );
        assert_eq!(
            left.morph_frames[0].morph_name,
            right.morph_frames[0].morph_name
        );
        assert_eq!(left.morph_frames[0].frame, right.morph_frames[0].frame);
        assert_eq!(left.morph_frames[0].weight, right.morph_frames[0].weight);
        assert_eq!(left.camera_frames[0].frame, right.camera_frames[0].frame);
        assert_eq!(
            left.camera_frames[0].position,
            right.camera_frames[0].position
        );
        assert_eq!(
            left.camera_frames[0].rotation,
            right.camera_frames[0].rotation
        );
        assert_eq!(
            left.camera_frames[0].interpolation,
            right.camera_frames[0].interpolation
        );
        assert_eq!(
            left.camera_frames[0].perspective,
            right.camera_frames[0].perspective
        );
        assert_eq!(left.light_frames[0].color, right.light_frames[0].color);
        assert_eq!(
            left.light_frames[0].direction,
            right.light_frames[0].direction
        );
        assert_eq!(
            left.self_shadow_frames[0].mode,
            right.self_shadow_frames[0].mode
        );
        assert_eq!(
            left.self_shadow_frames[0].distance,
            right.self_shadow_frames[0].distance
        );
        assert_eq!(
            left.property_frames[0].visible,
            right.property_frames[0].visible
        );
        assert_eq!(
            left.property_frames[0].ik_states[0].bone_name,
            right.property_frames[0].ik_states[0].bone_name
        );
        assert_eq!(
            left.property_frames[0].ik_states[0].enabled,
            right.property_frames[0].ik_states[0].enabled
        );
    }

    #[test]
    fn decodes_raw_vmd_bone_interpolation_as_strided_curves() {
        let mut interpolation = [0u8; 64];
        for (index, value) in interpolation.iter_mut().enumerate().take(16) {
            *value = index as u8;
        }

        let (position, rotation) = decode_bone_interpolation(&interpolation);

        assert_eq!(
            position.x,
            InterpolationScalar {
                x1: 0,
                y1: 4,
                x2: 8,
                y2: 12
            }
        );
        assert_eq!(
            position.y,
            InterpolationScalar {
                x1: 1,
                y1: 5,
                x2: 9,
                y2: 13
            }
        );
        assert_eq!(
            position.z,
            InterpolationScalar {
                x1: 2,
                y1: 6,
                x2: 10,
                y2: 14
            }
        );
        assert_eq!(
            rotation,
            InterpolationScalar {
                x1: 3,
                y1: 7,
                x2: 11,
                y2: 15
            }
        );
    }

    #[test]
    fn decodes_raw_vmd_camera_interpolation_as_contiguous_curves() {
        let mut interpolation = [0u8; 24];
        for (index, value) in interpolation.iter_mut().enumerate() {
            *value = index as u8;
        }

        let decoded = decode_camera_interpolation(&interpolation);

        assert_eq!(
            decoded.position.x,
            InterpolationScalar {
                x1: 0,
                y1: 1,
                x2: 2,
                y2: 3
            }
        );
        assert_eq!(
            decoded.position.y,
            InterpolationScalar {
                x1: 4,
                y1: 5,
                x2: 6,
                y2: 7
            }
        );
        assert_eq!(
            decoded.position.z,
            InterpolationScalar {
                x1: 8,
                y1: 9,
                x2: 10,
                y2: 11
            }
        );
        assert_eq!(
            decoded.rotation,
            InterpolationScalar {
                x1: 12,
                y1: 13,
                x2: 14,
                y2: 15
            }
        );
        assert_eq!(
            decoded.distance,
            InterpolationScalar {
                x1: 16,
                y1: 17,
                x2: 18,
                y2: 19
            }
        );
        assert_eq!(
            decoded.fov,
            InterpolationScalar {
                x1: 20,
                y1: 21,
                x2: 22,
                y2: 23
            }
        );
    }

    #[test]
    fn skips_camera_light_shadow_and_reads_ik_property_names() {
        let mut buf = build_vmd_header_bytes();

        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 61]);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 28]);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&0.5f32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&24u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&1u32.to_le_bytes());
        let mut ik_name = [0u8; 20];
        ik_name[..6].copy_from_slice(b"LeftIK");
        buf.extend_from_slice(&ik_name);
        buf.push(0);

        let result = import_vmd_motion(&buf).unwrap();
        assert_eq!(result.property_keyframes.len(), 1);
        assert_eq!(result.property_keyframes[0].frame, 24);
        assert_eq!(&*result.property_keyframes[0].ik_enabled, &[0]);

        assert_eq!(result.property_ik_frames.len(), 1);
        assert_eq!(result.property_ik_frames[0].frame, 24);
        assert_eq!(result.property_ik_frames[0].show, 1);
        assert_eq!(result.property_ik_frames[0].entries.len(), 1);
        let name_end = result.property_ik_frames[0].entries[0]
            .name_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(20);
        assert_eq!(
            &result.property_ik_frames[0].entries[0].name_bytes[..name_end],
            b"LeftIK"
        );
        assert_eq!(result.property_ik_frames[0].entries[0].enabled, 0);
        assert_eq!(
            &result.property_ik_frames[0].entries[0].name_normalized[..],
            b"LeftIK"
        );
    }

    #[test]
    fn builds_clip_from_bone_track() {
        let kfs = vec![
            VmdBoneKeyframeRaw {
                bone_mode: VmdBoneImportMode::ByName(b"BoneA".to_vec()),
                frame: 0,
                position: Vec3A::ZERO,
                rotation: Quat::IDENTITY,
                interpolation: [
                    20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20,
                    107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107,
                    107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107,
                    20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107,
                ],
                bone_name_normalized: b"BoneA".to_vec(),
            },
            VmdBoneKeyframeRaw {
                bone_mode: VmdBoneImportMode::ByName(b"BoneA".to_vec()),
                frame: 30,
                position: Vec3A::new(10.0, 0.0, 0.0),
                rotation: Quat::IDENTITY,
                interpolation: [
                    20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20,
                    107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107,
                    107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107,
                    20, 20, 107, 107, 20, 20, 107, 107, 20, 20, 107, 107,
                ],
                bone_name_normalized: b"BoneA".to_vec(),
            },
        ];

        fn lookup(name: &[u8]) -> Option<BoneIndex> {
            if name == b"BoneA" {
                Some(BoneIndex(0))
            } else {
                None
            }
        }
        fn morph_lookup(_name: &[u8]) -> Option<MorphIndex> {
            None
        }

        let result = VmdImportResult {
            bone_keyframes: kfs,
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        };

        let _clip = build_clip_from_import(result, &lookup, &morph_lookup);
    }

    #[test]
    fn build_clip_from_import_resolves_morph_by_normalized_name() {
        let sjis_morph = vec![0x83, 0x65, 0x83, 0x58, 0x83, 0x67];
        let result = VmdImportResult {
            bone_keyframes: Vec::new(),
            morph_keyframes: vec![(sjis_morph, 0, 1.0)],
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        };

        fn morph_lookup(name: &[u8]) -> Option<MorphIndex> {
            if name == b"\xE3\x83\x86\xE3\x82\xB9\xE3\x83\x88" {
                Some(MorphIndex(0))
            } else {
                None
            }
        }
        fn bone_lookup(_name: &[u8]) -> Option<BoneIndex> {
            None
        }

        let clip = build_clip_from_import(result, &bone_lookup, &morph_lookup);
        assert_eq!(clip.morph_track_count(), 1);
    }

    fn ik_name_bytes(name: &str) -> [u8; 20] {
        let mut buf = [0u8; 20];
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(20);
        buf[..len].copy_from_slice(&name_bytes[..len]);
        buf
    }

    #[test]
    fn parses_property_ik_entry_names() {
        let mut buf = build_vmd_header_bytes();

        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&10u32.to_le_bytes());
        buf.push(0);
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&ik_name_bytes("LeftLegIK"));
        buf.push(1);
        buf.extend_from_slice(&ik_name_bytes("RightLegIK"));
        buf.push(0);

        let result = import_vmd_motion(&buf).unwrap();
        assert_eq!(result.property_ik_frames.len(), 1);
        assert_eq!(result.property_ik_frames[0].frame, 10);
        assert_eq!(result.property_ik_frames[0].entries.len(), 2);

        let name1: Vec<u8> = result.property_ik_frames[0].entries[0]
            .name_bytes
            .iter()
            .copied()
            .take_while(|&b| b != 0)
            .collect();
        assert_eq!(&name1[..], b"LeftLegIK");
        assert_eq!(result.property_ik_frames[0].entries[0].enabled, 1);
        assert_eq!(
            &result.property_ik_frames[0].entries[0].name_normalized[..],
            b"LeftLegIK"
        );

        let name2: Vec<u8> = result.property_ik_frames[0].entries[1]
            .name_bytes
            .iter()
            .copied()
            .take_while(|&b| b != 0)
            .collect();
        assert_eq!(&name2[..], b"RightLegIK");
        assert_eq!(result.property_ik_frames[0].entries[1].enabled, 0);
    }

    #[test]
    fn reorders_ik_enabled_to_solver_order() {
        let frames = vec![VmdPropertyIkFrame {
            frame: 0,
            show: 0,
            entries: vec![
                VmdIkEntry {
                    name_bytes: {
                        let mut buf = [0u8; 20];
                        buf[..9].copy_from_slice(b"RightLegI");
                        buf.to_vec()
                    },
                    enabled: 1,
                    name_normalized: b"RightLegI".to_vec(),
                },
                VmdIkEntry {
                    name_bytes: {
                        let mut buf = [0u8; 20];
                        buf[..8].copy_from_slice(b"LeftLegI");
                        buf.to_vec()
                    },
                    enabled: 0,
                    name_normalized: b"LeftLegI".to_vec(),
                },
            ],
        }];

        fn ik_resolver(name: &[u8]) -> Option<usize> {
            match name {
                b"LeftLegI" => Some(0),
                b"RightLegI" => Some(1),
                _ => None,
            }
        }

        let binding = build_property_binding_with_ik_resolver(&frames, &ik_resolver, 2).unwrap();
        let sample = binding.sample(0.0).unwrap();
        assert_eq!(sample, &[0, 1]);
    }

    #[test]
    fn unknown_and_unmentioned_ik_names_keep_default_enabled_state() {
        let frames = vec![VmdPropertyIkFrame {
            frame: 5,
            show: 0,
            entries: vec![
                VmdIkEntry {
                    name_bytes: {
                        let mut buf = [0u8; 20];
                        buf[..7].copy_from_slice(b"KnownIK");
                        buf.to_vec()
                    },
                    enabled: 1,
                    name_normalized: b"KnownIK".to_vec(),
                },
                VmdIkEntry {
                    name_bytes: {
                        let mut buf = [0u8; 20];
                        buf[..9].copy_from_slice(b"UnknownIK");
                        buf.to_vec()
                    },
                    enabled: 1,
                    name_normalized: b"UnknownIK".to_vec(),
                },
            ],
        }];

        fn ik_resolver(name: &[u8]) -> Option<usize> {
            if name == b"KnownIK" { Some(0) } else { None }
        }

        let binding = build_property_binding_with_ik_resolver(&frames, &ik_resolver, 2).unwrap();
        let sample = binding.sample(5.0).unwrap();
        assert_eq!(sample, &[1, 1]);
    }

    #[test]
    fn empty_ik_frames_returns_none() {
        fn ik_resolver(_name: &[u8]) -> Option<usize> {
            None
        }
        let binding = build_property_binding_with_ik_resolver(&[], &ik_resolver, 2);
        assert!(binding.is_none());
    }

    #[test]
    fn zero_solver_count_returns_none() {
        let frames = vec![VmdPropertyIkFrame {
            frame: 0,
            show: 0,
            entries: vec![VmdIkEntry {
                name_bytes: ik_name_bytes("IK").to_vec(),
                enabled: 1,
                name_normalized: b"IK".to_vec(),
            }],
        }];
        fn ik_resolver(_name: &[u8]) -> Option<usize> {
            Some(0)
        }
        let binding = build_property_binding_with_ik_resolver(&frames, &ik_resolver, 0);
        assert!(binding.is_none());
    }

    #[test]
    fn build_property_binding_resolves_ik_by_normalized_name() {
        let frames = vec![VmdPropertyIkFrame {
            frame: 0,
            show: 0,
            entries: vec![VmdIkEntry {
                name_bytes: vec![0x8D, 0xB6, 0x91, 0xAB],
                enabled: 1,
                name_normalized: vec![0xE5, 0xB7, 0xA6, 0xE8, 0xB6, 0xB3],
            }],
        }];

        fn ik_resolver(name: &[u8]) -> Option<usize> {
            if name == b"\xE5\xB7\xA6\xE8\xB6\xB3" {
                Some(0)
            } else {
                None
            }
        }

        let binding = build_property_binding_with_ik_resolver(&frames, &ik_resolver, 1).unwrap();
        let sample = binding.sample(0.0).unwrap();
        assert_eq!(sample, &[1]);
    }

    #[test]
    fn build_clip_from_import_resolves_bone_by_normalized_name() {
        let sjis_name: Vec<u8> = vec![0x8D, 0xB6, 0x91, 0xAB];
        let utf8_name: Vec<u8> = vec![0xE5, 0xB7, 0xA6, 0xE8, 0xB6, 0xB3];

        let kf = VmdBoneKeyframeRaw {
            bone_mode: VmdBoneImportMode::ByName(sjis_name),
            frame: 0,
            position: Vec3A::ZERO,
            rotation: Quat::IDENTITY,
            interpolation: [20u8; 64],
            bone_name_normalized: utf8_name.clone(),
        };

        fn lookup(name: &[u8]) -> Option<BoneIndex> {
            if name == b"\xE5\xB7\xA6\xE8\xB6\xB3" {
                Some(BoneIndex(0))
            } else {
                None
            }
        }
        fn morph_lookup(_name: &[u8]) -> Option<MorphIndex> {
            None
        }

        let result = VmdImportResult {
            bone_keyframes: vec![kf],
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        };

        let clip = build_clip_from_import(result, &lookup, &morph_lookup);
        assert_eq!(
            clip.bone_track_count(),
            1,
            "bone should be resolved via normalized UTF-8 name, not raw Shift-JIS bytes"
        );
    }

    #[test]
    fn japanese_vmd_name_matches_pmx_utf8_name_via_normalization() {
        let sjis_name: &[u8] = &[0x8D, 0xB6, 0x91, 0xAB];
        let utf8_name: &[u8] = &[0xE5, 0xB7, 0xA6, 0xE8, 0xB6, 0xB3];

        let mut pmx_buf = Vec::new();
        pmx_buf.extend_from_slice(b"PMX ");
        pmx_buf.extend_from_slice(&2.0f32.to_le_bytes());
        pmx_buf.push(8);
        pmx_buf.push(1);
        pmx_buf.push(0);
        pmx_buf.push(4);
        pmx_buf.push(1);
        pmx_buf.push(1);
        pmx_buf.push(2);
        pmx_buf.push(1);
        pmx_buf.push(1);
        for _ in 0..4 {
            pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        }
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&1i32.to_le_bytes());
        pmx_buf.extend_from_slice(&(utf8_name.len() as i32).to_le_bytes());
        pmx_buf.extend_from_slice(utf8_name);
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&(-1i16).to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0u16.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());

        let pmx = crate::pmx::import_pmx_runtime(&pmx_buf).unwrap();

        let mut vmd_buf = build_vmd_header_bytes();
        vmd_buf.extend_from_slice(&1u32.to_le_bytes());
        let mut bone_name = [0u8; 15];
        bone_name[..sjis_name.len()].copy_from_slice(sjis_name);
        vmd_buf.extend_from_slice(&bone_name);
        vmd_buf.extend_from_slice(&0u32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&1.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&[20u8; 64]);
        vmd_buf.extend_from_slice(&0u32.to_le_bytes());
        vmd_buf.extend_from_slice(&0u32.to_le_bytes());
        vmd_buf.extend_from_slice(&0u32.to_le_bytes());
        vmd_buf.extend_from_slice(&0u32.to_le_bytes());
        vmd_buf.extend_from_slice(&0u32.to_le_bytes());

        let vmd = import_vmd_motion(&vmd_buf).unwrap();

        let normalized = normalize_vmd_name(sjis_name);
        assert_eq!(&normalized[..], utf8_name);

        assert!(
            pmx.bone_name_to_index.contains_key(&normalized),
            "PMX should have normalized bone name key"
        );

        let clip = build_pair_clip(
            &vmd,
            &pmx.bone_name_to_index,
            &pmx.morph_name_to_index,
            &pmx.ik_solver_bone_name_to_index,
            pmx.model.ik_count(),
        );

        assert_eq!(clip.bone_track_count(), 1);
        assert_eq!(clip.morph_track_count(), 0);
        assert!(!clip.has_property_track());
    }

    #[test]
    fn japanese_vmd_name_matches_pmx_utf16le_name_via_decoded_key() {
        let sjis_name: &[u8] = &[0x8D, 0xB6, 0x91, 0xAB];
        let utf8_name: &[u8] = &[0xE5, 0xB7, 0xA6, 0xE8, 0xB6, 0xB3];
        let utf16le_name: Vec<u8> = "\u{5DE6}\u{8DB3}"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();

        let mut pmx_buf = Vec::new();
        pmx_buf.extend_from_slice(b"PMX ");
        pmx_buf.extend_from_slice(&2.0f32.to_le_bytes());
        pmx_buf.push(8);
        pmx_buf.push(0);
        pmx_buf.push(0);
        pmx_buf.push(4);
        pmx_buf.push(1);
        pmx_buf.push(1);
        pmx_buf.push(2);
        pmx_buf.push(1);
        pmx_buf.push(1);
        for _ in 0..4 {
            pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        }
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&1i32.to_le_bytes());
        pmx_buf.extend_from_slice(&(utf16le_name.len() as i32).to_le_bytes());
        pmx_buf.extend_from_slice(&utf16le_name);
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&(-1i16).to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());
        pmx_buf.extend_from_slice(&0u16.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0.0f32.to_le_bytes());
        pmx_buf.extend_from_slice(&0i32.to_le_bytes());

        let pmx = crate::pmx::import_pmx_runtime(&pmx_buf).unwrap();

        let mut vmd_buf = build_vmd_header_bytes();
        vmd_buf.extend_from_slice(&1u32.to_le_bytes());
        let mut bone_name = [0u8; 15];
        bone_name[..sjis_name.len()].copy_from_slice(sjis_name);
        vmd_buf.extend_from_slice(&bone_name);
        vmd_buf.extend_from_slice(&0u32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&0.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&1.0f32.to_le_bytes());
        vmd_buf.extend_from_slice(&[20u8; 64]);

        let vmd = import_vmd_motion(&vmd_buf).unwrap();
        let normalized = normalize_vmd_name(sjis_name);
        assert_eq!(&normalized[..], utf8_name);
        assert!(
            pmx.bone_name_to_index.contains_key(&utf16le_name),
            "PMX should retain the raw UTF-16LE name key"
        );
        assert!(
            pmx.bone_name_to_index.contains_key(&normalized),
            "PMX should also index UTF-16LE names by decoded UTF-8 bytes"
        );

        let clip = build_pair_clip(
            &vmd,
            &pmx.bone_name_to_index,
            &pmx.morph_name_to_index,
            &pmx.ik_solver_bone_name_to_index,
            pmx.model.ik_count(),
        );

        assert_eq!(clip.bone_track_count(), 1);
    }

    #[test]
    fn build_pair_clip_default_includes_property_ik() {
        // VMD with one property IK frame that disables "LeftLegIK" at frame 0.
        let result = VmdImportResult {
            bone_keyframes: Vec::new(),
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: vec![VmdPropertyIkFrame {
                frame: 0,
                show: 0,
                entries: vec![VmdIkEntry {
                    name_bytes: ik_name_bytes("LeftLegIK").to_vec(),
                    enabled: 0,
                    name_normalized: b"LeftLegIK".to_vec(),
                }],
            }],
        };

        let bone_name_to_index: std::collections::HashMap<Vec<u8>, BoneIndex> =
            std::collections::HashMap::new();
        let morph_name_to_index: std::collections::HashMap<Vec<u8>, MorphIndex> =
            std::collections::HashMap::new();
        let mut ik_solver_bone_name_to_index: std::collections::HashMap<Vec<u8>, usize> =
            std::collections::HashMap::new();
        ik_solver_bone_name_to_index.insert(b"LeftLegIK".to_vec(), 0);

        let clip = build_pair_clip(
            &result,
            &bone_name_to_index,
            &morph_name_to_index,
            &ik_solver_bone_name_to_index,
            2, // 2 solvers total
        );

        assert!(
            clip.has_property_track(),
            "default build_pair_clip should include property IK track"
        );
    }

    #[test]
    fn build_pair_clip_with_options_omits_property_ik() {
        // Same VMD data, but with honor_property_ik: false => no property track.
        let result = VmdImportResult {
            bone_keyframes: Vec::new(),
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: vec![VmdPropertyIkFrame {
                frame: 5,
                show: 0,
                entries: vec![VmdIkEntry {
                    name_bytes: ik_name_bytes("RightLegIK").to_vec(),
                    enabled: 0,
                    name_normalized: b"RightLegIK".to_vec(),
                }],
            }],
        };

        let bone_name_to_index: std::collections::HashMap<Vec<u8>, BoneIndex> =
            std::collections::HashMap::new();
        let morph_name_to_index: std::collections::HashMap<Vec<u8>, MorphIndex> =
            std::collections::HashMap::new();
        let mut ik_solver_bone_name_to_index: std::collections::HashMap<Vec<u8>, usize> =
            std::collections::HashMap::new();
        ik_solver_bone_name_to_index.insert(b"RightLegIK".to_vec(), 1);

        let clip = build_pair_clip_with_options(
            &result,
            &bone_name_to_index,
            &morph_name_to_index,
            &ik_solver_bone_name_to_index,
            2,
            VmdClipBuildOptions {
                honor_property_ik: false,
            },
        );

        assert!(
            !clip.has_property_track(),
            "build_pair_clip_with_options(honor_property_ik: false) should omit property IK track"
        );
    }

    // ---------------------------------------------------------------------------
    // Synthetic roundtrip fixtures (CI gate for P0 Exporter Roundtrip Gate)
    // ---------------------------------------------------------------------------

    /// Build a minimal VMD binary from plain components so tests carry no external files.
    type SyntheticBoneFrame<'a> = (&'a str, u32, [f32; 3], [f32; 4], [u8; 64]);
    type SyntheticMorphFrame<'a> = (&'a str, u32, f32);
    type SyntheticPropertyFrame<'a> = (&'a [(&'a str, bool)], u32, bool);

    fn assert_near(actual: f32, expected: f32) {
        let delta = (actual - expected).abs();
        assert!(
            delta < 1.0e-4,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    fn assert_vec3_near(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert_near(actual, expected);
        }
    }

    fn simple_camera_vmd_fixture() -> &'static [u8] {
        include_bytes!("../../fixtures/vmd/simple_camera.vmd")
    }

    fn make_vmd_bytes(
        model_name_ascii: &str,
        bone_frames: &[SyntheticBoneFrame<'_>],
        morph_frames: &[SyntheticMorphFrame<'_>],
        property_frames: &[SyntheticPropertyFrame<'_>],
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&VMD_MAGIC);

        let mut name_buf = [0u8; 20];
        let nb = model_name_ascii.as_bytes();
        name_buf[..nb.len().min(20)].copy_from_slice(&nb[..nb.len().min(20)]);
        v.extend_from_slice(&name_buf);

        v.extend_from_slice(&(bone_frames.len() as u32).to_le_bytes());
        for (name, frame, trans, rot, interp) in bone_frames {
            let mut nb = [0u8; 15];
            let b = name.as_bytes();
            nb[..b.len().min(15)].copy_from_slice(&b[..b.len().min(15)]);
            v.extend_from_slice(&nb);
            v.extend_from_slice(&frame.to_le_bytes());
            for &x in trans {
                v.extend_from_slice(&x.to_le_bytes());
            }
            for &x in rot {
                v.extend_from_slice(&x.to_le_bytes());
            }
            v.extend_from_slice(interp);
        }

        v.extend_from_slice(&(morph_frames.len() as u32).to_le_bytes());
        for (name, frame, weight) in morph_frames {
            let mut nb = [0u8; 15];
            let b = name.as_bytes();
            nb[..b.len().min(15)].copy_from_slice(&b[..b.len().min(15)]);
            v.extend_from_slice(&nb);
            v.extend_from_slice(&frame.to_le_bytes());
            v.extend_from_slice(&weight.to_le_bytes());
        }

        // camera / light / self-shadow: 0 frames each
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());
        v.extend_from_slice(&0u32.to_le_bytes());

        v.extend_from_slice(&(property_frames.len() as u32).to_le_bytes());
        for (ik_states, frame, visible) in property_frames {
            v.extend_from_slice(&frame.to_le_bytes());
            v.push(u8::from(*visible));
            v.extend_from_slice(&(ik_states.len() as u32).to_le_bytes());
            for (ik_name, enabled) in *ik_states {
                let mut nb = [0u8; 20];
                let b = ik_name.as_bytes();
                nb[..b.len().min(20)].copy_from_slice(&b[..b.len().min(20)]);
                v.extend_from_slice(&nb);
                v.push(u8::from(*enabled));
            }
        }
        v
    }

    fn json_keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys = value
            .as_object()
            .unwrap()
            .keys()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }

    #[test]
    fn vmd_animation_json_top_level_schema_is_stable() {
        let vmd = make_vmd_bytes("miku", &[], &[], &[]);
        let parsed = parse_vmd_animation(&vmd).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed).unwrap());

        assert_eq!(
            keys,
            vec![
                "boneFrames",
                "cameraFrames",
                "kind",
                "lightFrames",
                "metadata",
                "morphFrames",
                "propertyFrames",
                "selfShadowFrames",
            ]
        );
    }

    #[test]
    fn parses_simple_camera_vmd_fixture() {
        let parsed = parse_vmd_animation(simple_camera_vmd_fixture()).unwrap();

        assert_eq!(parsed.metadata.model_name, "camera_fixture");
        assert_eq!(parsed.metadata.counts.bones, 0);
        assert_eq!(parsed.metadata.counts.morphs, 0);
        assert_eq!(parsed.metadata.counts.cameras, 2);
        assert_eq!(parsed.metadata.counts.lights, 0);
        assert_eq!(parsed.metadata.counts.self_shadows, 0);
        assert_eq!(parsed.metadata.counts.properties, 0);
        assert_eq!(parsed.metadata.max_frame, 45);
        assert_eq!(parsed.camera_frames.len(), 2);

        let first = &parsed.camera_frames[0];
        assert_eq!(first.frame, 0);
        assert_eq!(first.distance, -30.5);
        assert_eq!(first.position, [1.0, 2.0, 3.0]);
        assert_eq!(first.rotation, [0.1, -0.2, 0.3]);
        assert_eq!(
            first.interpolation,
            [
                20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,
                41, 42, 43
            ]
        );
        assert_eq!(first.fov, 35);
        assert!(first.perspective);

        let second = &parsed.camera_frames[1];
        assert_eq!(second.frame, 45);
        assert_eq!(second.distance, -50.0);
        assert_eq!(second.position, [-1.5, 10.0, 0.25]);
        assert_eq!(second.rotation, [-0.3, 0.0, 1.2]);
        assert_eq!(
            second.interpolation,
            [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 127, 127, 127, 127, 127, 127, 127, 127, 127,
                127, 127, 127
            ]
        );
        assert_eq!(second.fov, 60);
        assert!(!second.perspective);
    }

    #[test]
    fn roundtrip_simple_camera_vmd_fixture_parse_export_parse() {
        let parsed = parse_vmd_animation(simple_camera_vmd_fixture()).unwrap();
        let exported = export_vmd_animation(&parsed);
        let reparsed = parse_vmd_animation(&exported).unwrap();

        assert_eq!(reparsed.metadata.model_name, parsed.metadata.model_name);
        assert_eq!(reparsed.metadata.counts.cameras, 2);
        assert_eq!(reparsed.metadata.max_frame, 45);
        assert_eq!(reparsed.camera_frames.len(), parsed.camera_frames.len());
        for (left, right) in parsed.camera_frames.iter().zip(&reparsed.camera_frames) {
            assert_eq!(left.frame, right.frame);
            assert_eq!(left.distance, right.distance);
            assert_eq!(left.position, right.position);
            assert_eq!(left.rotation, right.rotation);
            assert_eq!(left.interpolation, right.interpolation);
            assert_eq!(left.fov, right.fov);
            assert_eq!(left.perspective, right.perspective);
        }
    }

    #[test]
    fn samples_simple_camera_vmd_fixture_with_channel_interpolation() {
        let parsed = parse_vmd_animation(simple_camera_vmd_fixture()).unwrap();
        let camera = sample_vmd_camera_frames(&parsed.camera_frames, 22.5).unwrap();

        assert_near(camera.distance, -40.25);
        assert_vec3_near(camera.position, [-0.25, 6.0, 1.625]);
        assert_vec3_near(camera.rotation, [-0.1, -0.1, 0.75]);
        assert_near(camera.fov, 47.5);
        assert!(camera.perspective);

        let last = sample_vmd_camera_frames(&parsed.camera_frames, 45.0).unwrap();
        assert!(!last.perspective);
    }

    #[test]
    fn roundtrip_bone_frame_parse_export_parse() {
        let interp = [20u8; 64];
        let vmd = make_vmd_bytes(
            "miku",
            &[("arm", 30, [1.0, 0.5, -0.5], [0.0, 0.0, 0.0, 1.0], interp)],
            &[],
            &[],
        );
        let parsed = parse_vmd_animation(&vmd).unwrap();
        assert_eq!(parsed.bone_frames.len(), 1);
        assert_eq!(parsed.bone_frames[0].bone_name, "arm");
        assert_eq!(parsed.bone_frames[0].frame, 30);
        assert_eq!(parsed.bone_frames[0].translation, [1.0, 0.5, -0.5]);
        assert_eq!(parsed.bone_frames[0].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(parsed.bone_frames[0].interpolation, vec![20u8; 64]);

        let exported = export_vmd_animation(&parsed);
        let reparsed = parse_vmd_animation(&exported).unwrap();
        assert_eq!(reparsed.metadata.model_name, parsed.metadata.model_name);
        assert_eq!(reparsed.metadata.max_frame, 30);
        assert_eq!(reparsed.bone_frames.len(), 1);
        assert_eq!(reparsed.bone_frames[0].bone_name, "arm");
        assert_eq!(reparsed.bone_frames[0].frame, 30);
        assert_eq!(reparsed.bone_frames[0].translation, [1.0, 0.5, -0.5]);
        assert_eq!(reparsed.bone_frames[0].rotation, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(reparsed.bone_frames[0].interpolation, vec![20u8; 64]);
    }

    #[test]
    fn roundtrip_morph_frame_parse_export_parse() {
        let vmd = make_vmd_bytes("miku", &[], &[("blink", 15, 0.75)], &[]);
        let parsed = parse_vmd_animation(&vmd).unwrap();
        let exported = export_vmd_animation(&parsed);
        let reparsed = parse_vmd_animation(&exported).unwrap();
        assert_eq!(reparsed.morph_frames.len(), 1);
        assert_eq!(reparsed.morph_frames[0].morph_name, "blink");
        assert_eq!(reparsed.morph_frames[0].frame, 15);
        assert!((reparsed.morph_frames[0].weight - 0.75f32).abs() < 1e-6);
    }

    #[test]
    fn roundtrip_property_ik_frames_parse_export_parse() {
        let vmd = make_vmd_bytes(
            "camera",
            &[],
            &[],
            &[(&[("leftIK", true), ("rightIK", false)], 20, true)],
        );
        let parsed = parse_vmd_animation(&vmd).unwrap();
        let exported = export_vmd_animation(&parsed);
        let reparsed = parse_vmd_animation(&exported).unwrap();
        assert_eq!(reparsed.property_frames.len(), 1);
        assert_eq!(reparsed.property_frames[0].frame, 20);
        assert!(reparsed.property_frames[0].visible);
        assert_eq!(reparsed.property_frames[0].ik_states.len(), 2);
        assert_eq!(reparsed.property_frames[0].ik_states[0].bone_name, "leftIK");
        assert!(reparsed.property_frames[0].ik_states[0].enabled);
        assert_eq!(
            reparsed.property_frames[0].ik_states[1].bone_name,
            "rightIK"
        );
        assert!(!reparsed.property_frames[0].ik_states[1].enabled);
    }

    #[test]
    fn roundtrip_json_dto_bone_and_morph() {
        let interp = [20u8; 64];
        let vmd = make_vmd_bytes(
            "miku",
            &[("spine", 60, [0.0, 1.0, 0.0], [0.1, 0.2, 0.3, 0.9], interp)],
            &[("mouth", 30, 0.8)],
            &[],
        );
        let parsed = parse_vmd_animation(&vmd).unwrap();
        let json = serde_json::to_string(&parsed).unwrap();
        let from_json: VmdParsedAnimation = serde_json::from_str(&json).unwrap();
        let exported = export_vmd_animation(&from_json);
        let reparsed = parse_vmd_animation(&exported).unwrap();

        assert_eq!(
            reparsed.bone_frames[0].bone_name,
            parsed.bone_frames[0].bone_name
        );
        assert_eq!(reparsed.bone_frames[0].frame, parsed.bone_frames[0].frame);
        assert_eq!(
            reparsed.bone_frames[0].translation,
            parsed.bone_frames[0].translation
        );
        assert_eq!(
            reparsed.bone_frames[0].rotation,
            parsed.bone_frames[0].rotation
        );
        assert_eq!(
            reparsed.morph_frames[0].morph_name,
            parsed.morph_frames[0].morph_name
        );
        assert_eq!(reparsed.morph_frames[0].frame, parsed.morph_frames[0].frame);
        assert!((reparsed.morph_frames[0].weight - parsed.morph_frames[0].weight).abs() < 1e-6);
    }

    fn expected_sjis_name_bytes(value: &str, len: usize) -> Vec<u8> {
        let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(value);
        encoded.as_ref()[..encoded.len().min(len)].to_vec()
    }

    #[test]
    fn export_json_dto_encodes_shift_jis_when_raw_name_bytes_are_missing() {
        let animation = vmd_parsed_animation(
            "初音ミク".to_owned(),
            Vec::new(),
            30,
            VmdParsedSections {
                bone_frames: vec![VmdParsedBoneFrame {
                    bone_name: "左足".to_owned(),
                    bone_name_bytes: Vec::new(),
                    frame: 10,
                    translation: [1.0, 2.0, 3.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    interpolation: vec![20; 64],
                }],
                morph_frames: vec![VmdParsedMorphFrame {
                    morph_name: "笑い".to_owned(),
                    morph_name_bytes: Vec::new(),
                    frame: 20,
                    weight: 0.75,
                }],
                camera_frames: Vec::new(),
                light_frames: Vec::new(),
                self_shadow_frames: Vec::new(),
                property_frames: vec![VmdParsedPropertyFrame {
                    frame: 30,
                    visible: true,
                    ik_states: vec![VmdParsedIkState {
                        bone_name: "右足IK".to_owned(),
                        bone_name_bytes: Vec::new(),
                        enabled: false,
                    }],
                }],
            },
        );
        let json = serde_json::to_string(&animation).unwrap();
        assert!(!json.contains("modelNameBytes"));
        assert!(!json.contains("boneNameBytes"));
        assert!(!json.contains("morphNameBytes"));

        let from_json: VmdParsedAnimation = serde_json::from_str(&json).unwrap();
        let exported = export_vmd_animation(&from_json);
        let reparsed = parse_vmd_animation(&exported).unwrap();

        assert_eq!(reparsed.metadata.model_name, "初音ミク");
        assert!(!reparsed.metadata.model_name_bytes.is_empty());
        assert_eq!(
            reparsed.metadata.model_name_bytes,
            expected_sjis_name_bytes("初音ミク", 20)
        );
        assert_eq!(reparsed.bone_frames[0].bone_name, "左足");
        assert!(!reparsed.bone_frames[0].bone_name_bytes.is_empty());
        assert_eq!(
            reparsed.bone_frames[0].bone_name_bytes,
            expected_sjis_name_bytes("左足", 15)
        );
        assert_eq!(reparsed.morph_frames[0].morph_name, "笑い");
        assert!(!reparsed.morph_frames[0].morph_name_bytes.is_empty());
        assert_eq!(
            reparsed.morph_frames[0].morph_name_bytes,
            expected_sjis_name_bytes("笑い", 15)
        );
        assert_eq!(reparsed.property_frames[0].ik_states[0].bone_name, "右足IK");
        assert!(
            !reparsed.property_frames[0].ik_states[0]
                .bone_name_bytes
                .is_empty()
        );
        assert_eq!(
            reparsed.property_frames[0].ik_states[0].bone_name_bytes,
            expected_sjis_name_bytes("右足IK", 20)
        );
        assert!(!reparsed.property_frames[0].ik_states[0].enabled);
    }

    #[test]
    fn export_json_dto_uses_encoding_rs_replacement_for_non_shift_jis_names() {
        let animation = vmd_parsed_animation(
            "miku".to_owned(),
            Vec::new(),
            1,
            VmdParsedSections {
                bone_frames: vec![VmdParsedBoneFrame {
                    bone_name: "左足🧪".to_owned(),
                    bone_name_bytes: Vec::new(),
                    frame: 1,
                    translation: [0.0, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    interpolation: vec![20; 64],
                }],
                morph_frames: Vec::new(),
                camera_frames: Vec::new(),
                light_frames: Vec::new(),
                self_shadow_frames: Vec::new(),
                property_frames: Vec::new(),
            },
        );
        let json = serde_json::to_string(&animation).unwrap();
        let from_json: VmdParsedAnimation = serde_json::from_str(&json).unwrap();
        let exported = export_vmd_animation(&from_json);
        let reparsed = parse_vmd_animation(&exported).unwrap();

        assert_eq!(
            reparsed.bone_frames[0].bone_name_bytes,
            expected_sjis_name_bytes("左足🧪", 15)
        );
        assert_eq!(
            reparsed.bone_frames[0].bone_name,
            encoding_rs::SHIFT_JIS
                .decode(&expected_sjis_name_bytes("左足🧪", 15))
                .0
                .trim()
        );
    }
}
