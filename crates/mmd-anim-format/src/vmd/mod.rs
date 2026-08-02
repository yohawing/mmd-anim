use glam::{Quat, Vec3A};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, InterpolationScalar, InterpolationVector3,
    ModelArena, MorphAnimationBinding, MorphIndex, MorphKeyframe, MorphTrack, MovableBoneKeyframe,
    MovableBoneTrack, PropertyAnimationBinding, PropertyKeyframe,
};

use crate::binary::{
    ByteReader, write_f32_le as write_f32, write_fixed_bytes, write_u32_le as write_u32,
};
use crate::error::ImportError;
use crate::normalize::normalize_vmd_name;
use crate::sjis::{decode_sjis_fixed_trimmed, encode_sjis};
use thiserror::Error;

#[cfg(test)]
use mmd_anim_runtime::BoneInit;

mod reduced;
pub use reduced::{
    VmdExportMorphKind, VmdExportName, VmdPoseExport, VmdPoseExportBindings, VmdPoseExportError,
    VmdPoseExportReport, export_reduced_pose_to_vmd,
};

type Reader<'a> = ByteReader<'a>;

const VMD_MAGIC: [u8; 30] = *b"Vocaloid Motion Data 0002\0\0\0\0\0";
const VMD_MAGIC_PREFIX: &[u8] = b"Vocaloid Motion Data 0002\0";

impl<'a> ByteReader<'a> {
    fn read_optional_u32_le(&mut self) -> Result<Option<u32>, ImportError> {
        if self.remaining() == 0 {
            Ok(None)
        } else {
            self.read_u32_le().map(Some)
        }
    }

    fn read_record_count(&mut self, record_size: usize) -> Result<usize, ImportError> {
        let count = self.read_u32_le()? as usize;
        self.require_record_bytes(count, record_size)?;
        Ok(count)
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

/// Owns all VMD channels produced by one parser pass.
///
/// The parsed animation is the typed scene/raw-track view used by camera,
/// light, self-shadow, and property consumers. The import result retains the
/// runtime-facing bone/morph/property representation used by model-aware clip
/// construction. Both views are owned by the context so callers can create a
/// clip and copy scene channels without reparsing the input bytes.
#[derive(Debug, Clone)]
pub struct VmdSharedContext {
    raw: VmdImportResult,
    parsed: VmdParsedAnimation,
    summary: VmdSharedContextSummary,
}

/// Counts for one VMD source channel in a shared-context summary.
///
/// Bone and morph track counts are the number of distinct raw target names.
/// Camera, light, self-shadow, and property channels have one track when they
/// contain at least one key. `key_count` always counts source keyframe records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmdSharedContextTrackSummary {
    pub track_count: usize,
    pub key_count: usize,
}

/// Fixed-width summary retained by a parsed shared VMD context.
///
/// `target_model_name_bytes` is the original 20-byte VMD header field. It is
/// Shift-JIS/CP932 data with the same first-NUL trimming convention as the
/// existing parsed model-name value. Counts remain `usize` inside the format
/// layer; the native ABI exposes them as checked `uint32_t` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmdSharedContextSummary {
    pub target_model_name_bytes: [u8; 20],
    pub max_frame: u32,
    pub bones: VmdSharedContextTrackSummary,
    pub morphs: VmdSharedContextTrackSummary,
    pub cameras: VmdSharedContextTrackSummary,
    pub lights: VmdSharedContextTrackSummary,
    pub self_shadows: VmdSharedContextTrackSummary,
    pub properties: VmdSharedContextTrackSummary,
    pub property_ik_entry_count: usize,
}

/// Errors returned by the non-materializing VMD summary scanner.
///
/// The summary-only validation surface is intentionally separate from
/// `ImportError` so adding summary diagnostics does not make the existing
/// public import error enum's exhaustive matches non-exhaustive.
#[derive(Debug, Error, PartialEq)]
pub enum VmdSummaryError {
    #[error("VMD summary parse failed: {0}")]
    Parse(ImportError),
    #[error("invalid VMD summary data: {0}")]
    Invalid(&'static str),
}

impl VmdSharedContextSummary {
    fn from_parsed(target_model_name_bytes: [u8; 20], parsed: &VmdParsedAnimation) -> Self {
        Self {
            target_model_name_bytes,
            max_frame: parsed.metadata.max_frame,
            bones: VmdSharedContextTrackSummary {
                track_count: distinct_name_count(
                    parsed
                        .bone_frames
                        .iter()
                        .map(|frame| frame.bone_name_bytes.as_slice()),
                ),
                key_count: parsed.bone_frames.len(),
            },
            morphs: VmdSharedContextTrackSummary {
                track_count: distinct_name_count(
                    parsed
                        .morph_frames
                        .iter()
                        .map(|frame| frame.morph_name_bytes.as_slice()),
                ),
                key_count: parsed.morph_frames.len(),
            },
            cameras: singleton_track_summary(parsed.camera_frames.len()),
            lights: singleton_track_summary(parsed.light_frames.len()),
            self_shadows: singleton_track_summary(parsed.self_shadow_frames.len()),
            properties: singleton_track_summary(parsed.property_frames.len()),
            property_ik_entry_count: parsed
                .property_frames
                .iter()
                .map(|frame| frame.ik_states.len())
                .sum(),
        }
    }
}

fn distinct_name_count<'a>(names: impl Iterator<Item = &'a [u8]>) -> usize {
    names
        .map(|name| name.to_vec())
        .collect::<HashSet<_>>()
        .len()
}

fn singleton_track_summary(key_count: usize) -> VmdSharedContextTrackSummary {
    VmdSharedContextTrackSummary {
        track_count: usize::from(key_count != 0),
        key_count,
    }
}

impl VmdSharedContext {
    pub fn import_result(&self) -> &VmdImportResult {
        &self.raw
    }

    pub fn parsed_animation(&self) -> &VmdParsedAnimation {
        &self.parsed
    }

    pub fn summary(&self) -> &VmdSharedContextSummary {
        &self.summary
    }
}

/// Parses VMD bytes once and retains every supported channel for shared
/// model-clip and scene-track consumers.
pub fn parse_vmd_shared_context(data: &[u8]) -> Result<VmdSharedContext, ImportError> {
    let scan = parse_vmd_scan(data, VmdScanOptions::BOTH)?;
    let VmdScanResult {
        raw,
        parsed,
        summary: _,
        target_model_name_bytes,
    } = scan;
    let raw = raw.expect("combined VMD scan must produce raw output");
    let parsed = parsed.expect("combined VMD scan must produce parsed output");
    Ok(VmdSharedContext {
        summary: VmdSharedContextSummary::from_parsed(target_model_name_bytes, &parsed),
        raw,
        parsed,
    })
}

/// Reads only the fixed-width VMD summary without materializing raw or parsed
/// keyframe tracks.
///
/// Bone and morph distinct-track counts borrow their fixed-width names from
/// `data` for the duration of the scan. All other channels retain only their
/// key and IK-entry counts. Optional camera/light/self-shadow tails preserve
/// the compatibility parser's absent-tail behavior when a complete count is
/// present but its records are unavailable; a partial four-byte count remains
/// a structural parse error.
pub fn parse_vmd_summary(data: &[u8]) -> Result<VmdSharedContextSummary, VmdSummaryError> {
    let scan = parse_vmd_scan(data, VmdScanOptions::SUMMARY).map_err(|error| match error {
        ImportError::UnsupportedFormat {
            format: "VMD",
            detail,
        } => VmdSummaryError::Invalid(detail),
        error => VmdSummaryError::Parse(error),
    })?;
    Ok(scan
        .summary
        .expect("summary-only VMD scan must produce summary output"))
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
    Ok(parse_vmd_scan(data, VmdScanOptions::RAW)?
        .raw
        .expect("raw-only VMD scan must produce raw output"))
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdLightState {
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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VmdSelfShadowState {
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

#[derive(Clone, Copy)]
struct VmdScanOptions {
    build_raw: bool,
    build_parsed: bool,
    build_summary: bool,
}

impl VmdScanOptions {
    const RAW: Self = Self {
        build_raw: true,
        build_parsed: false,
        build_summary: false,
    };
    const PARSED: Self = Self {
        build_raw: false,
        build_parsed: true,
        build_summary: false,
    };
    const BOTH: Self = Self {
        build_raw: true,
        build_parsed: true,
        build_summary: false,
    };
    const SUMMARY: Self = Self {
        build_raw: false,
        build_parsed: false,
        build_summary: true,
    };
}

struct VmdScanResult {
    raw: Option<VmdImportResult>,
    parsed: Option<VmdParsedAnimation>,
    summary: Option<VmdSharedContextSummary>,
    target_model_name_bytes: [u8; 20],
}

struct VmdSummaryAccumulator {
    bone_names: HashSet<Vec<u8>>,
    morph_names: HashSet<Vec<u8>>,
    bone_key_count: usize,
    morph_key_count: usize,
    camera_key_count: usize,
    light_key_count: usize,
    self_shadow_key_count: usize,
    property_key_count: usize,
    property_ik_entry_count: usize,
}

impl VmdSummaryAccumulator {
    fn new() -> Self {
        Self {
            bone_names: HashSet::new(),
            morph_names: HashSet::new(),
            bone_key_count: 0,
            morph_key_count: 0,
            camera_key_count: 0,
            light_key_count: 0,
            self_shadow_key_count: 0,
            property_key_count: 0,
            property_ik_entry_count: 0,
        }
    }

    fn record_bone(&mut self, name: &[u8]) -> Result<(), ImportError> {
        validate_summary_name(name, "empty bone name")?;
        self.bone_names.insert(name.to_vec());
        self.bone_key_count = self
            .bone_key_count
            .checked_add(1)
            .ok_or(ImportError::SectionOverflow)?;
        Ok(())
    }

    fn record_morph(&mut self, name: &[u8]) -> Result<(), ImportError> {
        validate_summary_name(name, "empty morph name")?;
        self.morph_names.insert(name.to_vec());
        self.morph_key_count = self
            .morph_key_count
            .checked_add(1)
            .ok_or(ImportError::SectionOverflow)?;
        Ok(())
    }

    fn record_camera(&mut self) -> Result<(), ImportError> {
        self.camera_key_count = self
            .camera_key_count
            .checked_add(1)
            .ok_or(ImportError::SectionOverflow)?;
        Ok(())
    }

    fn record_light(&mut self) -> Result<(), ImportError> {
        self.light_key_count = self
            .light_key_count
            .checked_add(1)
            .ok_or(ImportError::SectionOverflow)?;
        Ok(())
    }

    fn record_self_shadow(&mut self) -> Result<(), ImportError> {
        self.self_shadow_key_count = self
            .self_shadow_key_count
            .checked_add(1)
            .ok_or(ImportError::SectionOverflow)?;
        Ok(())
    }

    fn record_property(&mut self, ik_count: usize) -> Result<(), ImportError> {
        self.property_key_count = self
            .property_key_count
            .checked_add(1)
            .ok_or(ImportError::SectionOverflow)?;
        self.property_ik_entry_count = self
            .property_ik_entry_count
            .checked_add(ik_count)
            .ok_or(ImportError::SectionOverflow)?;
        Ok(())
    }

    fn finish(self, target_model_name_bytes: [u8; 20], max_frame: u32) -> VmdSharedContextSummary {
        VmdSharedContextSummary {
            target_model_name_bytes,
            max_frame,
            bones: VmdSharedContextTrackSummary {
                track_count: self.bone_names.len(),
                key_count: self.bone_key_count,
            },
            morphs: VmdSharedContextTrackSummary {
                track_count: self.morph_names.len(),
                key_count: self.morph_key_count,
            },
            cameras: singleton_track_summary(self.camera_key_count),
            lights: singleton_track_summary(self.light_key_count),
            self_shadows: singleton_track_summary(self.self_shadow_key_count),
            properties: singleton_track_summary(self.property_key_count),
            property_ik_entry_count: self.property_ik_entry_count,
        }
    }
}

fn summary_invalid_error(detail: &'static str) -> ImportError {
    ImportError::UnsupportedFormat {
        format: "VMD",
        detail,
    }
}

fn validate_summary_name(name: &[u8], detail: &'static str) -> Result<(), ImportError> {
    if vmd_fixed_name_is_blank(name) {
        Err(summary_invalid_error(detail))
    } else {
        Ok(())
    }
}

fn read_vmd_f32(
    r: &mut Reader<'_>,
    validate: bool,
    field: &'static str,
) -> Result<f32, ImportError> {
    let value = r.read_f32_le()?;
    if validate && !value.is_finite() {
        return Err(summary_invalid_error(field));
    }
    Ok(value)
}

fn read_vmd_vec3(
    r: &mut Reader<'_>,
    validate: bool,
    field: &'static str,
) -> Result<Vec3A, ImportError> {
    Ok(Vec3A::new(
        read_vmd_f32(r, validate, field)?,
        read_vmd_f32(r, validate, field)?,
        read_vmd_f32(r, validate, field)?,
    ))
}

fn read_vmd_quat(
    r: &mut Reader<'_>,
    validate: bool,
    field: &'static str,
) -> Result<Quat, ImportError> {
    Ok(Quat::from_xyzw(
        read_vmd_f32(r, validate, field)?,
        read_vmd_f32(r, validate, field)?,
        read_vmd_f32(r, validate, field)?,
        read_vmd_f32(r, validate, field)?,
    ))
}

fn vmd_fixed_name_is_blank(bytes: &[u8]) -> bool {
    decode_sjis_fixed(bytes).trim().is_empty()
}

fn parse_vmd_scan(data: &[u8], options: VmdScanOptions) -> Result<VmdScanResult, ImportError> {
    let (header, pos) = read_header(data)?;
    let mut r = Reader { data, pos };
    let model_name = options
        .build_parsed
        .then(|| decode_sjis_fixed(&header.model_name_bytes));
    let model_name_bytes = options
        .build_parsed
        .then(|| trim_fixed_bytes(&header.model_name_bytes).to_vec());
    let mut max_frame = 0u32;
    let mut summary = options.build_summary.then(VmdSummaryAccumulator::new);

    let bone_count = r.read_record_count(111)?;
    let mut raw = options.build_raw.then(|| VmdImportResult {
        bone_keyframes: Vec::with_capacity(bone_count),
        morph_keyframes: Vec::new(),
        property_keyframes: Vec::new(),
        property_ik_frames: Vec::new(),
    });
    let mut bone_frames = options.build_parsed.then(|| Vec::with_capacity(bone_count));
    for _ in 0..bone_count {
        let bone_name_field = r.read_slice(15)?;
        if let Some(summary) = summary.as_mut() {
            summary.record_bone(trim_fixed_bytes(bone_name_field))?;
        }
        let raw_bone_name = options
            .build_raw
            .then(|| trim_fixed_bytes(bone_name_field).to_vec());
        let raw_bone_name_normalized = raw_bone_name
            .as_ref()
            .map(|name_bytes| normalize_vmd_name(name_bytes));
        let parsed_bone_name = options
            .build_parsed
            .then(|| decode_sjis_fixed(bone_name_field));
        let parsed_bone_name_bytes = options
            .build_parsed
            .then(|| trim_fixed_bytes(bone_name_field).to_vec());
        let frame = r.read_u32_le()?;
        if options.build_parsed || options.build_summary {
            max_frame = max_frame.max(frame);
        }
        let position = read_vmd_vec3(&mut r, options.build_summary, "bone translation")?;
        let rotation = read_vmd_quat(&mut r, options.build_summary, "bone rotation")?;
        let interpolation: [u8; 64] = r.read_slice(64)?.try_into().unwrap();
        if let Some(raw) = raw.as_mut() {
            raw.bone_keyframes.push(VmdBoneKeyframeRaw {
                bone_mode: VmdBoneImportMode::ByName(
                    raw_bone_name.expect("raw VMD scan must build bone names"),
                ),
                frame,
                position,
                rotation,
                interpolation,
                bone_name_normalized: raw_bone_name_normalized
                    .expect("raw VMD scan must normalize bone names"),
            });
        }
        if let Some(bone_frames) = bone_frames.as_mut() {
            bone_frames.push(VmdParsedBoneFrame {
                bone_name: parsed_bone_name.expect("parsed VMD scan must decode bone names"),
                bone_name_bytes: parsed_bone_name_bytes
                    .expect("parsed VMD scan must retain bone name bytes"),
                frame,
                translation: [position.x, position.y, position.z],
                rotation: [rotation.x, rotation.y, rotation.z, rotation.w],
                interpolation: interpolation.to_vec(),
            });
        }
    }

    let Some(morph_count) = r.read_optional_u32_le()? else {
        return Ok(vmd_scan_result(
            model_name,
            model_name_bytes,
            header.model_name_bytes,
            max_frame,
            raw,
            bone_frames.map(|bone_frames| VmdParsedSections {
                bone_frames,
                morph_frames: Vec::new(),
                camera_frames: Vec::new(),
                light_frames: Vec::new(),
                self_shadow_frames: Vec::new(),
                property_frames: Vec::new(),
            }),
            summary.map(|summary| summary.finish(header.model_name_bytes, max_frame)),
        ));
    };
    let morph_count = morph_count as usize;
    r.require_record_bytes(morph_count, 23)?;
    if let Some(raw) = raw.as_mut() {
        raw.morph_keyframes = Vec::with_capacity(morph_count);
    }
    let mut morph_frames = options
        .build_parsed
        .then(|| Vec::with_capacity(morph_count));
    for _ in 0..morph_count {
        let morph_name_field = r.read_slice(15)?;
        if let Some(summary) = summary.as_mut() {
            summary.record_morph(trim_fixed_bytes(morph_name_field))?;
        }
        let raw_morph_name = options
            .build_raw
            .then(|| trim_fixed_bytes(morph_name_field).to_vec());
        let parsed_morph_name = options
            .build_parsed
            .then(|| decode_sjis_fixed(morph_name_field));
        let parsed_morph_name_bytes = options
            .build_parsed
            .then(|| trim_fixed_bytes(morph_name_field).to_vec());
        let frame = r.read_u32_le()?;
        if options.build_parsed || options.build_summary {
            max_frame = max_frame.max(frame);
        }
        let weight = read_vmd_f32(&mut r, options.build_summary, "morph weight")?;
        if let Some(raw) = raw.as_mut() {
            raw.morph_keyframes.push((
                raw_morph_name.expect("raw VMD scan must build morph names"),
                frame,
                weight,
            ));
        }
        if let Some(morph_frames) = morph_frames.as_mut() {
            morph_frames.push(VmdParsedMorphFrame {
                morph_name: parsed_morph_name.expect("parsed VMD scan must decode morph names"),
                morph_name_bytes: parsed_morph_name_bytes
                    .expect("parsed VMD scan must retain morph name bytes"),
                frame,
                weight,
            });
        }
    }

    let camera_frames = if options.build_parsed || options.build_summary {
        read_camera_frames(&mut r, &mut max_frame, options.build_parsed, &mut summary)?
    } else {
        skip_optional_records(&mut r, 61)?;
        None
    };
    let light_frames = if options.build_parsed || options.build_summary {
        read_light_frames(&mut r, &mut max_frame, options.build_parsed, &mut summary)?
    } else {
        skip_optional_records(&mut r, 28)?;
        None
    };
    let self_shadow_frames = if options.build_parsed || options.build_summary {
        read_self_shadow_frames(&mut r, &mut max_frame, options.build_parsed, &mut summary)?
    } else {
        skip_optional_records(&mut r, 9)?;
        None
    };
    let (property_frames, raw_property_keyframes, raw_property_ik_frames) =
        read_property_frames(&mut r, &mut max_frame, options, &mut summary)?;

    if let Some(raw) = raw.as_mut() {
        raw.property_keyframes =
            raw_property_keyframes.expect("raw VMD scan must build property keyframes");
        raw.property_ik_frames =
            raw_property_ik_frames.expect("raw VMD scan must build property IK frames");
    }
    let parsed = bone_frames.map(|bone_frames| VmdParsedSections {
        bone_frames,
        morph_frames: morph_frames.expect("parsed VMD scan must build morph frames"),
        camera_frames: camera_frames.expect("parsed VMD scan must build camera frames"),
        light_frames: light_frames.expect("parsed VMD scan must build light frames"),
        self_shadow_frames: self_shadow_frames
            .expect("parsed VMD scan must build self-shadow frames"),
        property_frames: property_frames.expect("parsed VMD scan must build property frames"),
    });

    Ok(vmd_scan_result(
        model_name,
        model_name_bytes,
        header.model_name_bytes,
        max_frame,
        raw,
        parsed,
        summary.map(|summary| summary.finish(header.model_name_bytes, max_frame)),
    ))
}

pub fn parse_vmd_animation(data: &[u8]) -> Result<VmdParsedAnimation, ImportError> {
    Ok(parse_vmd_scan(data, VmdScanOptions::PARSED)?
        .parsed
        .expect("parsed-only VMD scan must produce parsed output"))
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

fn vmd_scan_result(
    model_name: Option<String>,
    model_name_bytes: Option<Vec<u8>>,
    target_model_name_bytes: [u8; 20],
    max_frame: u32,
    raw: Option<VmdImportResult>,
    sections: Option<VmdParsedSections>,
    summary: Option<VmdSharedContextSummary>,
) -> VmdScanResult {
    VmdScanResult {
        raw,
        target_model_name_bytes,
        summary,
        parsed: sections.map(|sections| {
            vmd_parsed_animation(
                model_name.expect("parsed VMD scan must decode model name"),
                model_name_bytes.expect("parsed VMD scan must retain model name bytes"),
                max_frame,
                sections,
            )
        }),
    }
}

fn skip_optional_records(r: &mut Reader<'_>, record_size: usize) -> Result<(), ImportError> {
    if let Some(count) = r.read_optional_record_count(record_size)? {
        r.skip(count * record_size)?;
    }
    Ok(())
}

fn read_camera_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
    build_parsed: bool,
    summary: &mut Option<VmdSummaryAccumulator>,
) -> Result<Option<Vec<VmdParsedCameraFrame>>, ImportError> {
    let Some(count) = r.read_optional_record_count(61)? else {
        return Ok(build_parsed.then(Vec::new));
    };
    let mut frames = build_parsed.then(|| Vec::with_capacity(count));
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        *max_frame = (*max_frame).max(frame);
        let distance = read_vmd_f32(r, summary.is_some(), "camera distance")?;
        let position = read_vmd_vec3(r, summary.is_some(), "camera position")?;
        let rotation = read_vmd_vec3(r, summary.is_some(), "camera rotation")?;
        let interpolation: [u8; 24] = r.read_slice(24)?.try_into().unwrap();
        let fov = r.read_u32_le()?;
        let perspective = r.read_u8()? == 0;
        if let Some(summary) = summary.as_mut() {
            summary.record_camera()?;
        }
        if let Some(frames) = frames.as_mut() {
            frames.push(VmdParsedCameraFrame {
                frame,
                distance,
                position: [position.x, position.y, position.z],
                rotation: [rotation.x, rotation.y, rotation.z],
                interpolation,
                fov,
                perspective,
            });
        }
    }
    Ok(frames)
}

fn read_light_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
    build_parsed: bool,
    summary: &mut Option<VmdSummaryAccumulator>,
) -> Result<Option<Vec<VmdParsedLightFrame>>, ImportError> {
    let Some(count) = r.read_optional_record_count(28)? else {
        return Ok(build_parsed.then(Vec::new));
    };
    let mut frames = build_parsed.then(|| Vec::with_capacity(count));
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        *max_frame = (*max_frame).max(frame);
        let color = read_vmd_vec3(r, summary.is_some(), "light color")?;
        let direction = read_vmd_vec3(r, summary.is_some(), "light direction")?;
        if let Some(summary) = summary.as_mut() {
            summary.record_light()?;
        }
        if let Some(frames) = frames.as_mut() {
            frames.push(VmdParsedLightFrame {
                frame,
                color: [color.x, color.y, color.z],
                direction: [direction.x, direction.y, direction.z],
            });
        }
    }
    Ok(frames)
}

fn read_self_shadow_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
    build_parsed: bool,
    summary: &mut Option<VmdSummaryAccumulator>,
) -> Result<Option<Vec<VmdParsedSelfShadowFrame>>, ImportError> {
    let Some(count) = r.read_optional_record_count(9)? else {
        return Ok(build_parsed.then(Vec::new));
    };
    let mut frames = build_parsed.then(|| Vec::with_capacity(count));
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        *max_frame = (*max_frame).max(frame);
        let mode = r.read_u8()?;
        let distance = read_vmd_f32(r, summary.is_some(), "self-shadow distance")?;
        if let Some(summary) = summary.as_mut() {
            summary.record_self_shadow()?;
        }
        if let Some(frames) = frames.as_mut() {
            frames.push(VmdParsedSelfShadowFrame {
                frame,
                mode,
                distance,
            });
        }
    }
    Ok(frames)
}

fn read_property_frames(
    r: &mut Reader<'_>,
    max_frame: &mut u32,
    options: VmdScanOptions,
    summary: &mut Option<VmdSummaryAccumulator>,
) -> Result<
    (
        Option<Vec<VmdParsedPropertyFrame>>,
        Option<Vec<PropertyKeyframe>>,
        Option<Vec<VmdPropertyIkFrame>>,
    ),
    ImportError,
> {
    let Some(count) = r.read_optional_u32_le()? else {
        return Ok((
            options.build_parsed.then(Vec::new),
            options.build_raw.then(Vec::new),
            options.build_raw.then(Vec::new),
        ));
    };
    let count = count as usize;
    r.require_record_bytes(count, 9)?;
    let mut parsed_frames = options.build_parsed.then(|| Vec::with_capacity(count));
    let mut raw_keyframes = options.build_raw.then(|| Vec::with_capacity(count));
    let mut raw_ik_frames = options.build_raw.then(|| Vec::with_capacity(count));
    for _ in 0..count {
        let frame = r.read_u32_le()?;
        if options.build_parsed || options.build_summary {
            *max_frame = (*max_frame).max(frame);
        }
        let show = r.read_u8()?;
        let ik_count = r.read_u32_le()? as usize;
        r.require_record_bytes(ik_count, 21)?;
        if let Some(summary) = summary.as_mut() {
            summary.record_property(ik_count)?;
        }
        let mut raw_ik_enabled = options.build_raw.then(|| Vec::with_capacity(ik_count));
        let mut raw_ik_entries = options.build_raw.then(|| Vec::with_capacity(ik_count));
        let mut ik_states = options.build_parsed.then(|| Vec::with_capacity(ik_count));
        for _ in 0..ik_count {
            let name_field = r.read_slice(20)?;
            if options.build_summary {
                validate_summary_name(trim_fixed_bytes(name_field), "empty property IK name")?;
            }
            let raw_name_bytes = options.build_raw.then(|| name_field.to_vec());
            let raw_name_normalized = raw_name_bytes
                .as_ref()
                .map(|name_bytes| normalize_vmd_name(trim_fixed_bytes(name_bytes)));
            let parsed_name = options.build_parsed.then(|| decode_sjis_fixed(name_field));
            let parsed_name_bytes = options
                .build_parsed
                .then(|| trim_fixed_bytes(name_field).to_vec());
            let enabled = r.read_u8()?;
            if let Some(raw_ik_enabled) = raw_ik_enabled.as_mut() {
                raw_ik_enabled.push(enabled);
            }
            if let Some(raw_ik_entries) = raw_ik_entries.as_mut() {
                raw_ik_entries.push(VmdIkEntry {
                    name_bytes: raw_name_bytes
                        .expect("raw VMD scan must retain property name bytes"),
                    enabled,
                    name_normalized: raw_name_normalized
                        .expect("raw VMD scan must normalize property names"),
                });
            }
            if let Some(ik_states) = ik_states.as_mut() {
                ik_states.push(VmdParsedIkState {
                    bone_name: parsed_name.expect("parsed VMD scan must decode property names"),
                    bone_name_bytes: parsed_name_bytes
                        .expect("parsed VMD scan must retain property name bytes"),
                    enabled: enabled != 0,
                });
            }
        }
        if let Some(parsed_frames) = parsed_frames.as_mut() {
            parsed_frames.push(VmdParsedPropertyFrame {
                frame,
                visible: show != 0,
                ik_states: ik_states.expect("parsed VMD scan must build IK states"),
            });
        }
        if let Some(raw_keyframes) = raw_keyframes.as_mut() {
            raw_keyframes.push(PropertyKeyframe::new(
                frame,
                raw_ik_enabled
                    .expect("raw VMD scan must build IK enabled values")
                    .into_iter()
                    .map(|value| value != 0)
                    .collect(),
            ));
        }
        if let Some(raw_ik_frames) = raw_ik_frames.as_mut() {
            raw_ik_frames.push(VmdPropertyIkFrame {
                frame,
                show,
                entries: raw_ik_entries.expect("raw VMD scan must build property IK entries"),
            });
        }
    }
    Ok((parsed_frames, raw_keyframes, raw_ik_frames))
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

pub fn sample_vmd_light_frames(
    frames: &[VmdParsedLightFrame],
    frame: f32,
) -> Option<VmdLightState> {
    if frames.is_empty() {
        return None;
    }

    let mut sorted: Vec<&VmdParsedLightFrame> = frames.iter().collect();
    sorted.sort_by_key(|keyframe| keyframe.frame);

    let mut index = 0usize;
    while index + 1 < sorted.len() && sorted[index + 1].frame as f32 <= frame {
        index += 1;
    }

    let previous = sorted[index];
    let next = sorted.get(index + 1).copied().unwrap_or(previous);
    let t = interpolation_ratio(previous.frame, next.frame, frame);

    Some(VmdLightState {
        color: [
            lerp(previous.color[0], next.color[0], t),
            lerp(previous.color[1], next.color[1], t),
            lerp(previous.color[2], next.color[2], t),
        ],
        direction: [
            lerp(previous.direction[0], next.direction[0], t),
            lerp(previous.direction[1], next.direction[1], t),
            lerp(previous.direction[2], next.direction[2], t),
        ],
    })
}

pub fn sample_vmd_self_shadow_frames(
    frames: &[VmdParsedSelfShadowFrame],
    frame: f32,
) -> Option<VmdSelfShadowState> {
    if frames.is_empty() {
        return None;
    }

    let mut sorted: Vec<&VmdParsedSelfShadowFrame> = frames.iter().collect();
    sorted.sort_by_key(|keyframe| keyframe.frame);

    let mut index = 0usize;
    while index + 1 < sorted.len() && sorted[index + 1].frame as f32 <= frame {
        index += 1;
    }

    let previous = sorted[index];
    let next = sorted.get(index + 1).copied().unwrap_or(previous);
    let t = interpolation_ratio(previous.frame, next.frame, frame);

    Some(VmdSelfShadowState {
        mode: if t < 1.0 { previous.mode } else { next.mode },
        distance: lerp(previous.distance, next.distance, t),
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
    decode_sjis_fixed_trimmed(bytes)
}

fn trim_fixed_bytes(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn write_fixed_name_bytes(out: &mut Vec<u8>, value: &str, raw_bytes: &[u8], len: usize) {
    if raw_bytes.is_empty() {
        let encoded = encode_sjis(value);
        write_fixed_bytes(out, &encoded, len);
    } else {
        write_fixed_bytes(out, raw_bytes, len);
    }
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

/// Decodes the interpolation block as MMD's PMX/VMD registration path uses it.
/// The raw parser intentionally retains nanoem's first-16-byte strided layout;
/// this alternate layout is used only by the model-aware paired clip builder.
fn decode_mmd_registered_bone_interpolation(
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
            interpolation[16],
            interpolation[20],
            interpolation[24],
            interpolation[28],
        ]),
        z: decode_interpolation_scalar([
            interpolation[32],
            interpolation[36],
            interpolation[40],
            interpolation[44],
        ]),
    };
    let rotation = decode_interpolation_scalar([
        interpolation[48],
        interpolation[52],
        interpolation[56],
        interpolation[60],
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

/// Errors returned while constructing an MMD-registered VMD clip.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum VmdClipBuildError {
    /// A model-aware bone map contains a bone index that cannot be looked up
    /// in the supplied [`ModelArena`].
    #[error(
        "VMD bone map contains bone index {bone_index:?} outside model bone count {bone_count}"
    )]
    BoneIndexOutOfRange {
        bone_index: BoneIndex,
        bone_count: usize,
    },
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

/// Builds a VMD clip using the paired PMX model's MMD registration semantics.
///
/// The default VMD builder intentionally preserves the raw imported rotations.
/// This opt-in paired path applies MMD's fixed-axis registration rule to
/// mapped bone keys and decodes their interpolation using MMD's registered
/// block layout; positions, morphs, and property IK remain identical to
/// [`build_pair_clip_with_options`].
pub fn build_mmd_registered_pair_clip(
    model: &ModelArena,
    result: &VmdImportResult,
    bone_name_to_index: &std::collections::HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: &std::collections::HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: &std::collections::HashMap<Vec<u8>, usize>,
    solver_count: usize,
) -> Result<AnimationClip, VmdClipBuildError> {
    build_mmd_registered_pair_clip_with_options(
        model,
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
    build_pair_clip_with_registration_policy(
        result,
        bone_name_to_index,
        morph_name_to_index,
        ik_solver_bone_name_to_index,
        solver_count,
        options,
        VmdRegistrationPolicy::Raw,
    )
}

/// Builds a VMD clip using the paired PMX model's MMD registration semantics and
/// explicit clip construction options.
pub fn build_mmd_registered_pair_clip_with_options(
    model: &ModelArena,
    result: &VmdImportResult,
    bone_name_to_index: &std::collections::HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: &std::collections::HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: &std::collections::HashMap<Vec<u8>, usize>,
    solver_count: usize,
    options: VmdClipBuildOptions,
) -> Result<AnimationClip, VmdClipBuildError> {
    validate_bone_map(model, bone_name_to_index)?;
    Ok(build_pair_clip_with_registration_policy(
        result,
        bone_name_to_index,
        morph_name_to_index,
        ik_solver_bone_name_to_index,
        solver_count,
        options,
        VmdRegistrationPolicy::MmdRegistered(model),
    ))
}

#[derive(Clone, Copy)]
enum VmdRegistrationPolicy<'a> {
    Raw,
    MmdRegistered(&'a ModelArena),
}

fn validate_bone_map(
    model: &ModelArena,
    bone_name_to_index: &std::collections::HashMap<Vec<u8>, BoneIndex>,
) -> Result<(), VmdClipBuildError> {
    let bone_count = model.bone_count();
    if let Some(&bone_index) = bone_name_to_index
        .values()
        .find(|bone_index| bone_index.as_usize() >= bone_count)
    {
        return Err(VmdClipBuildError::BoneIndexOutOfRange {
            bone_index,
            bone_count,
        });
    }
    Ok(())
}

fn build_pair_clip_with_registration_policy(
    result: &VmdImportResult,
    bone_name_to_index: &std::collections::HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: &std::collections::HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: &std::collections::HashMap<Vec<u8>, usize>,
    solver_count: usize,
    options: VmdClipBuildOptions,
    registration_policy: VmdRegistrationPolicy<'_>,
) -> AnimationClip {
    let mut bone_tracks_map: std::collections::BTreeMap<u32, Vec<MovableBoneKeyframe>> =
        std::collections::BTreeMap::new();

    for kf in &result.bone_keyframes {
        let bone_index = match bone_name_to_index.get(&kf.bone_name_normalized) {
            Some(idx) => *idx,
            None => continue,
        };

        bone_tracks_map
            .entry(bone_index.0)
            .or_default()
            .push(build_mapped_bone_keyframe(
                kf,
                bone_index,
                registration_policy,
            ));
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

fn build_mapped_bone_keyframe(
    kf: &VmdBoneKeyframeRaw,
    bone_index: BoneIndex,
    registration_policy: VmdRegistrationPolicy<'_>,
) -> MovableBoneKeyframe {
    let (position_interpolation, rotation_interpolation) = match registration_policy {
        VmdRegistrationPolicy::Raw => decode_bone_interpolation(&kf.interpolation),
        VmdRegistrationPolicy::MmdRegistered(_) => {
            decode_mmd_registered_bone_interpolation(&kf.interpolation)
        }
    };
    let rotation = match registration_policy {
        VmdRegistrationPolicy::Raw => kf.rotation,
        VmdRegistrationPolicy::MmdRegistered(model) => {
            model.fixed_axis(bone_index).map_or(kf.rotation, |axis| {
                project_rotation_to_fixed_axis(kf.rotation, axis)
            })
        }
    };
    MovableBoneKeyframe {
        frame: kf.frame,
        position: kf.position,
        rotation,
        position_interpolation,
        rotation_interpolation,
    }
}

fn project_rotation_to_fixed_axis(rotation: Quat, axis: Vec3A) -> Quat {
    let axis = axis.normalize();
    let vector = Vec3A::new(rotation.x, rotation.y, rotation.z);
    let sign = if vector.dot(axis) < 0.0 { -1.0 } else { 1.0 };
    let projected = axis * vector.length() * sign;
    Quat::from_xyzw(projected.x, projected.y, projected.z, rotation.w)
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
        for (index, value) in interpolation.iter_mut().enumerate() {
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
    fn decodes_mmd_registered_bone_interpolation_as_block_curves() {
        let mut interpolation = [0u8; 64];
        for (index, value) in interpolation.iter_mut().enumerate() {
            *value = index as u8;
        }

        let (position, rotation) = decode_mmd_registered_bone_interpolation(&interpolation);

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
                x1: 16,
                y1: 20,
                x2: 24,
                y2: 28
            }
        );
        assert_eq!(
            position.z,
            InterpolationScalar {
                x1: 32,
                y1: 36,
                x2: 40,
                y2: 44
            }
        );
        assert_eq!(
            rotation,
            InterpolationScalar {
                x1: 48,
                y1: 52,
                x2: 56,
                y2: 60
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
    fn shared_context_preserves_raw_property_bytes_and_values() {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&0u32.to_le_bytes()); // bones
        buf.extend_from_slice(&0u32.to_le_bytes()); // morphs
        buf.extend_from_slice(&0u32.to_le_bytes()); // cameras
        buf.extend_from_slice(&0u32.to_le_bytes()); // lights
        buf.extend_from_slice(&0u32.to_le_bytes()); // self shadows
        buf.extend_from_slice(&1u32.to_le_bytes()); // property frames
        buf.extend_from_slice(&42u32.to_le_bytes());
        buf.push(0x7f); // preserve a nonstandard show byte
        buf.extend_from_slice(&1u32.to_le_bytes()); // IK entries
        let mut name_bytes = [0u8; 20];
        name_bytes[..2].copy_from_slice(b"IK");
        name_bytes[2] = 0;
        name_bytes[3] = 0xa5; // preserve bytes after the terminating NUL
        name_bytes[19] = 0x5a;
        buf.extend_from_slice(&name_bytes);
        buf.push(0x7f); // preserve a nonstandard enabled byte

        let shared = parse_vmd_shared_context(&buf).unwrap();
        let raw = shared.import_result();
        assert_eq!(raw.property_keyframes[0].frame, 42);
        assert_eq!(raw.property_keyframes[0].ik_enabled.as_ref(), &[1]);
        assert_eq!(raw.property_ik_frames[0].show, 0x7f);
        assert_eq!(
            raw.property_ik_frames[0].entries[0].name_bytes,
            name_bytes.to_vec()
        );
        assert_eq!(raw.property_ik_frames[0].entries[0].enabled, 0x7f);
        assert_eq!(
            raw.property_ik_frames[0].entries[0].name_normalized,
            normalize_vmd_name(b"IK")
        );

        let parsed = shared.parsed_animation();
        assert!(parsed.property_frames[0].visible);
        assert_eq!(parsed.property_frames[0].ik_states[0].bone_name, "IK");
        assert!(parsed.property_frames[0].ik_states[0].enabled);
    }

    #[test]
    fn shared_context_summary_reports_target_and_channel_counts() {
        let animation = VmdParsedAnimation {
            kind: "vmd",
            metadata: VmdParsedMetadata {
                format: "vmd",
                model_name: "target".to_owned(),
                model_name_bytes: Vec::new(),
                counts: VmdParsedCounts {
                    bones: 3,
                    morphs: 2,
                    cameras: 1,
                    lights: 1,
                    self_shadows: 1,
                    properties: 1,
                },
                max_frame: 0,
            },
            bone_frames: vec![
                VmdParsedBoneFrame {
                    bone_name: "bone".to_owned(),
                    bone_name_bytes: Vec::new(),
                    frame: 5,
                    translation: [0.0; 3],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    interpolation: vec![0; 64],
                },
                VmdParsedBoneFrame {
                    bone_name: "bone".to_owned(),
                    bone_name_bytes: Vec::new(),
                    frame: 10,
                    translation: [0.0; 3],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    interpolation: vec![0; 64],
                },
                VmdParsedBoneFrame {
                    bone_name: "other".to_owned(),
                    bone_name_bytes: Vec::new(),
                    frame: 15,
                    translation: [0.0; 3],
                    rotation: [0.0, 0.0, 0.0, 1.0],
                    interpolation: vec![0; 64],
                },
            ],
            morph_frames: vec![
                VmdParsedMorphFrame {
                    morph_name: "smile".to_owned(),
                    morph_name_bytes: Vec::new(),
                    frame: 20,
                    weight: 0.5,
                },
                VmdParsedMorphFrame {
                    morph_name: "smile".to_owned(),
                    morph_name_bytes: Vec::new(),
                    frame: 25,
                    weight: 1.0,
                },
            ],
            camera_frames: vec![VmdParsedCameraFrame {
                frame: 30,
                distance: -40.0,
                position: [0.0; 3],
                rotation: [0.0; 3],
                interpolation: [0; 24],
                fov: 45,
                perspective: true,
            }],
            light_frames: vec![VmdParsedLightFrame {
                frame: 35,
                color: [1.0; 3],
                direction: [0.0; 3],
            }],
            self_shadow_frames: vec![VmdParsedSelfShadowFrame {
                frame: 40,
                mode: 1,
                distance: 10.0,
            }],
            property_frames: vec![VmdParsedPropertyFrame {
                frame: 42,
                visible: true,
                ik_states: vec![
                    VmdParsedIkState {
                        bone_name: "ik_a".to_owned(),
                        bone_name_bytes: Vec::new(),
                        enabled: true,
                    },
                    VmdParsedIkState {
                        bone_name: "ik_b".to_owned(),
                        bone_name_bytes: Vec::new(),
                        enabled: false,
                    },
                ],
            }],
        };

        let bytes = export_vmd_animation(&animation);
        let shared = parse_vmd_shared_context(&bytes).unwrap();
        let summary_only = parse_vmd_summary(&bytes).unwrap();
        assert_eq!(summary_only, *shared.summary());
        let summary = shared.summary();
        let mut expected_name = [0u8; 20];
        expected_name[..6].copy_from_slice(b"target");

        assert_eq!(summary.target_model_name_bytes, expected_name);
        assert_eq!(summary.max_frame, 42);
        assert_eq!(
            summary.bones,
            VmdSharedContextTrackSummary {
                track_count: 2,
                key_count: 3,
            }
        );
        assert_eq!(
            summary.morphs,
            VmdSharedContextTrackSummary {
                track_count: 1,
                key_count: 2,
            }
        );
        assert_eq!(
            summary.cameras,
            VmdSharedContextTrackSummary {
                track_count: 1,
                key_count: 1,
            }
        );
        assert_eq!(summary.lights.key_count, 1);
        assert_eq!(summary.self_shadows.key_count, 1);
        assert_eq!(summary.properties.key_count, 1);
        assert_eq!(summary.property_ik_entry_count, 2);
    }

    fn summary_validation_vmd_bytes() -> Vec<u8> {
        let mut buf = build_vmd_header_bytes();
        buf.extend_from_slice(&1u32.to_le_bytes()); // one bone
        let mut bone_name = [0u8; 15];
        bone_name[..4].copy_from_slice(b"bone");
        buf.extend_from_slice(&bone_name);
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 12]); // translation
        buf.extend_from_slice(&[0u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // rotation
        buf.extend_from_slice(&[0u8; 64]); // interpolation

        buf.extend_from_slice(&1u32.to_le_bytes()); // one morph
        let mut morph_name = [0u8; 15];
        morph_name[..5].copy_from_slice(b"morph");
        buf.extend_from_slice(&morph_name);
        buf.extend_from_slice(&2u32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&1u32.to_le_bytes()); // one camera
        buf.extend_from_slice(&3u32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // distance
        buf.extend_from_slice(&[0u8; 12]); // position
        buf.extend_from_slice(&[0u8; 12]); // rotation
        buf.extend_from_slice(&[0u8; 24]); // interpolation
        buf.extend_from_slice(&45u32.to_le_bytes());
        buf.push(0);

        buf.extend_from_slice(&1u32.to_le_bytes()); // one light
        buf.extend_from_slice(&4u32.to_le_bytes());
        buf.extend_from_slice(&[0u8; 12]); // color
        buf.extend_from_slice(&[0u8; 12]); // direction

        buf.extend_from_slice(&1u32.to_le_bytes()); // one self-shadow
        buf.extend_from_slice(&5u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&1u32.to_le_bytes()); // one property
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&1u32.to_le_bytes()); // one IK entry
        let mut ik_name = [0u8; 20];
        ik_name[..2].copy_from_slice(b"ik");
        buf.extend_from_slice(&ik_name);
        buf.push(1);
        buf
    }

    #[test]
    fn summary_scanner_rejects_empty_names_and_non_finite_values() {
        let mut empty_bone = summary_validation_vmd_bytes();
        empty_bone[54..69].fill(0);
        assert_eq!(
            parse_vmd_summary(&empty_bone),
            Err(VmdSummaryError::Invalid("empty bone name"))
        );

        let mut empty_morph = summary_validation_vmd_bytes();
        empty_morph[169..184].fill(0);
        assert_eq!(
            parse_vmd_summary(&empty_morph),
            Err(VmdSummaryError::Invalid("empty morph name"))
        );

        let non_finite_offsets = [
            (73, "bone translation"),
            (85, "bone rotation"),
            (188, "morph weight"),
            (200, "camera distance"),
            (204, "camera position"),
            (216, "camera rotation"),
            (265, "light color"),
            (277, "light direction"),
            (298, "self-shadow distance"),
        ];
        for (offset, field) in non_finite_offsets {
            let mut malformed = summary_validation_vmd_bytes();
            malformed[offset..offset + 4].copy_from_slice(&f32::NAN.to_le_bytes());
            assert_eq!(
                parse_vmd_summary(&malformed),
                Err(VmdSummaryError::Invalid(field)),
                "summary scanner must reject non-finite {field}"
            );
        }

        let mut empty_property_ik = summary_validation_vmd_bytes();
        empty_property_ik[315..335].fill(0);
        assert_eq!(
            parse_vmd_summary(&empty_property_ik),
            Err(VmdSummaryError::Invalid("empty property IK name"))
        );

        let mut whitespace_property_ik = summary_validation_vmd_bytes();
        whitespace_property_ik[315..335].fill(0x20);
        assert_eq!(
            parse_vmd_summary(&whitespace_property_ik),
            Err(VmdSummaryError::Invalid("empty property IK name"))
        );
    }

    #[test]
    fn summary_scanner_preserves_optional_tail_absence_semantics() {
        let bytes = summary_validation_vmd_bytes();
        let context = parse_vmd_shared_context(&bytes).unwrap();

        // The camera count starts after the 50-byte header, bone record, and
        // morph record. A complete count with insufficient records is the
        // same absent-tail condition used by the existing parser.
        let mut truncated_camera_tail = bytes.clone();
        truncated_camera_tail[192..196].copy_from_slice(&10u32.to_le_bytes());
        let summary = parse_vmd_summary(&truncated_camera_tail).unwrap();
        let parsed_context = parse_vmd_shared_context(&truncated_camera_tail).unwrap();
        assert_eq!(summary, *parsed_context.summary());
        assert_eq!(summary.bones, context.summary().bones);
        assert_eq!(summary.morphs, context.summary().morphs);
        let empty_track_summary = VmdSharedContextTrackSummary {
            track_count: 0,
            key_count: 0,
        };
        assert_eq!(summary.cameras, empty_track_summary);
        assert_eq!(summary.lights, empty_track_summary);
        assert_eq!(summary.properties, empty_track_summary);

        let mut partial_camera_count = build_vmd_header_bytes();
        partial_camera_count.extend_from_slice(&0u32.to_le_bytes()); // bones
        partial_camera_count.extend_from_slice(&0u32.to_le_bytes()); // morphs
        partial_camera_count.extend_from_slice(&[1, 2, 3]);
        assert_eq!(
            parse_vmd_summary(&partial_camera_count),
            Err(VmdSummaryError::Parse(ImportError::UnexpectedEof(1)))
        );
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

    fn fixed_axis_test_keyframe(rotation: Quat) -> VmdBoneKeyframeRaw {
        VmdBoneKeyframeRaw {
            bone_mode: VmdBoneImportMode::ByIndex(7),
            frame: 12,
            position: Vec3A::new(1.0, 2.0, 3.0),
            rotation,
            interpolation: [20u8; 64],
            bone_name_normalized: b"FixedAxis".to_vec(),
        }
    }

    fn assert_quat_near(actual: Quat, expected: Quat) {
        for (actual, expected) in [actual.x, actual.y, actual.z, actual.w]
            .into_iter()
            .zip([expected.x, expected.y, expected.z, expected.w])
        {
            assert!((actual - expected).abs() < 1.0e-6);
        }
    }

    fn test_model(fixed_axis: Option<Vec3A>) -> ModelArena {
        let mut bones: Vec<_> = (0..8).map(|_| BoneInit::new(None, Vec3A::ZERO)).collect();
        if let Some(axis) = fixed_axis {
            bones[7] = BoneInit::new(None, Vec3A::ZERO).with_fixed_axis(axis);
        }
        ModelArena::new(bones).unwrap()
    }

    #[test]
    fn model_fixed_axis_projects_off_axis_positive_dot_and_preserves_keyframe_data() {
        let source = fixed_axis_test_keyframe(Quat::from_xyzw(0.3, 0.4, 0.0, 0.8));
        let model = test_model(Some(Vec3A::new(0.0, 3.0, 0.0)));
        let mapped = build_mapped_bone_keyframe(
            &source,
            BoneIndex(7),
            VmdRegistrationPolicy::MmdRegistered(&model),
        );

        assert_quat_near(mapped.rotation, Quat::from_xyzw(0.0, 0.5, 0.0, 0.8));
        assert_eq!(mapped.frame, source.frame);
        assert_eq!(mapped.position, source.position);
        let (position_interpolation, rotation_interpolation) =
            decode_mmd_registered_bone_interpolation(&source.interpolation);
        assert_eq!(mapped.position_interpolation, position_interpolation);
        assert_eq!(mapped.rotation_interpolation, rotation_interpolation);
    }

    #[test]
    fn model_fixed_axis_projects_off_axis_negative_dot_with_negative_sign() {
        let source = fixed_axis_test_keyframe(Quat::from_xyzw(0.3, -0.4, 0.0, 0.8));
        let model = test_model(Some(Vec3A::new(0.0, 3.0, 0.0)));
        let mapped = build_mapped_bone_keyframe(
            &source,
            BoneIndex(7),
            VmdRegistrationPolicy::MmdRegistered(&model),
        );

        assert_quat_near(mapped.rotation, Quat::from_xyzw(0.0, -0.5, 0.0, 0.8));
    }

    #[test]
    fn model_fixed_axis_leaves_already_axis_aligned_rotation_unchanged() {
        let source = fixed_axis_test_keyframe(Quat::from_xyzw(0.0, -0.5, 0.0, 0.8));
        let model = test_model(Some(Vec3A::new(0.0, 3.0, 0.0)));
        let mapped = build_mapped_bone_keyframe(
            &source,
            BoneIndex(7),
            VmdRegistrationPolicy::MmdRegistered(&model),
        );

        assert_quat_near(mapped.rotation, source.rotation);
    }

    #[test]
    fn model_fixed_axis_leaves_non_fixed_bone_rotation_unchanged() {
        let source = fixed_axis_test_keyframe(Quat::from_xyzw(0.3, 0.4, 0.1, 0.8));
        let model = test_model(None);
        let mapped = build_mapped_bone_keyframe(
            &source,
            BoneIndex(7),
            VmdRegistrationPolicy::MmdRegistered(&model),
        );

        assert_eq!(mapped.rotation, source.rotation);
    }

    #[test]
    fn model_aware_bone_keyframe_uses_registered_interpolation_for_all_mapped_bones() {
        let mut source = fixed_axis_test_keyframe(Quat::IDENTITY);
        for (index, value) in source.interpolation.iter_mut().enumerate() {
            *value = index as u8;
        }
        let model = test_model(None);
        let raw = build_mapped_bone_keyframe(&source, BoneIndex(7), VmdRegistrationPolicy::Raw);
        let model_aware = build_mapped_bone_keyframe(
            &source,
            BoneIndex(7),
            VmdRegistrationPolicy::MmdRegistered(&model),
        );
        let (raw_position, raw_rotation) = decode_bone_interpolation(&source.interpolation);
        let (registered_position, registered_rotation) =
            decode_mmd_registered_bone_interpolation(&source.interpolation);

        assert_eq!(raw.position_interpolation, raw_position);
        assert_eq!(raw.rotation_interpolation, raw_rotation);
        assert_eq!(model_aware.position_interpolation, registered_position);
        assert_eq!(model_aware.rotation_interpolation, registered_rotation);
        assert_ne!(
            raw.position_interpolation,
            model_aware.position_interpolation
        );
        assert_ne!(
            raw.rotation_interpolation,
            model_aware.rotation_interpolation
        );
    }

    #[test]
    fn public_mmd_registered_builder_projects_fixed_axis_rotation() {
        let source = fixed_axis_test_keyframe(Quat::from_xyzw(0.3, 0.4, 0.0, 0.8));
        let model = test_model(Some(Vec3A::Y));
        let result = VmdImportResult {
            bone_keyframes: vec![source],
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        };
        let mut bone_name_to_index = std::collections::HashMap::new();
        bone_name_to_index.insert(b"FixedAxis".to_vec(), BoneIndex(7));
        let morph_name_to_index = std::collections::HashMap::new();
        let ik_solver_bone_name_to_index = std::collections::HashMap::new();

        let clip = build_mmd_registered_pair_clip_with_options(
            &model,
            &result,
            &bone_name_to_index,
            &morph_name_to_index,
            &ik_solver_bone_name_to_index,
            0,
            VmdClipBuildOptions::default(),
        )
        .unwrap();
        let sample = clip.sample_at(12.0);
        let rotation = sample.bone_samples()[0].rotation;
        assert_quat_near(rotation, Quat::from_xyzw(0.0, 0.5, 0.0, 0.8).normalize());
    }

    #[test]
    fn public_mmd_registered_builder_uses_registered_interpolation_layout() {
        let mut first = fixed_axis_test_keyframe(Quat::IDENTITY);
        first.frame = 0;
        first.position = Vec3A::ZERO;
        let mut second = fixed_axis_test_keyframe(Quat::IDENTITY);
        second.frame = 10;
        second.position = Vec3A::Y;
        for (index, value) in second.interpolation.iter_mut().enumerate() {
            *value = index as u8;
        }
        let result = VmdImportResult {
            bone_keyframes: vec![first, second],
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        };
        let mut bone_name_to_index = std::collections::HashMap::new();
        bone_name_to_index.insert(b"FixedAxis".to_vec(), BoneIndex(7));
        let morph_name_to_index = std::collections::HashMap::new();
        let ik_solver_bone_name_to_index = std::collections::HashMap::new();
        let model = test_model(None);

        let registered = build_mmd_registered_pair_clip(
            &model,
            &result,
            &bone_name_to_index,
            &morph_name_to_index,
            &ik_solver_bone_name_to_index,
            0,
        )
        .unwrap();
        let raw = build_pair_clip(
            &result,
            &bone_name_to_index,
            &morph_name_to_index,
            &ik_solver_bone_name_to_index,
            0,
        );
        let registered_position = registered.sample_at(5.0).bone_samples()[0].position;
        let raw_position = raw.sample_at(5.0).bone_samples()[0].position;
        assert_ne!(registered_position, raw_position);
    }

    #[test]
    fn public_mmd_registered_builder_rejects_out_of_range_bone_map() {
        let model = test_model(None);
        let result = VmdImportResult {
            bone_keyframes: Vec::new(),
            morph_keyframes: Vec::new(),
            property_keyframes: Vec::new(),
            property_ik_frames: Vec::new(),
        };
        let mut bone_name_to_index = std::collections::HashMap::new();
        bone_name_to_index.insert(b"Invalid".to_vec(), BoneIndex(8));
        let morph_name_to_index = std::collections::HashMap::new();
        let ik_solver_bone_name_to_index = std::collections::HashMap::new();

        let error = build_mmd_registered_pair_clip(
            &model,
            &result,
            &bone_name_to_index,
            &morph_name_to_index,
            &ik_solver_bone_name_to_index,
            0,
        )
        .unwrap_err();
        assert_eq!(
            error,
            VmdClipBuildError::BoneIndexOutOfRange {
                bone_index: BoneIndex(8),
                bone_count: 8,
            }
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
    fn samples_vmd_light_frames_linearly() {
        let frames = vec![
            VmdParsedLightFrame {
                frame: 30,
                color: [1.0, 0.5, 0.0],
                direction: [0.0, -1.0, 0.0],
            },
            VmdParsedLightFrame {
                frame: 10,
                color: [0.0, 0.0, 1.0],
                direction: [1.0, 0.0, 0.0],
            },
        ];

        let light = sample_vmd_light_frames(&frames, 20.0).unwrap();

        assert_vec3_near(light.color, [0.5, 0.25, 0.5]);
        assert_vec3_near(light.direction, [0.5, -0.5, 0.0]);
    }

    #[test]
    fn samples_vmd_self_shadow_frames_linearly_with_stepped_mode() {
        let frames = vec![
            VmdParsedSelfShadowFrame {
                frame: 10,
                mode: 1,
                distance: 20.0,
            },
            VmdParsedSelfShadowFrame {
                frame: 30,
                mode: 2,
                distance: 60.0,
            },
        ];

        let middle = sample_vmd_self_shadow_frames(&frames, 20.0).unwrap();
        assert_eq!(middle.mode, 1);
        assert_near(middle.distance, 40.0);

        let last = sample_vmd_self_shadow_frames(&frames, 30.0).unwrap();
        assert_eq!(last.mode, 2);
        assert_near(last.distance, 60.0);
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
