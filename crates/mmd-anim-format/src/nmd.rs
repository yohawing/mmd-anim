use serde::{Deserialize, Serialize};

use crate::binary::ByteReader;
use crate::error::ImportError;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdParsedManifest {
    #[serde(default = "default_nmd_format", skip_deserializing)]
    pub format: &'static str,
    pub byte_length: usize,
    pub metadata: NmdParsedMetadata,
    pub global_track_count: usize,
    pub keyframe_bundles: NmdKeyframeBundleCounts,
    pub payload: NmdPayloadSummary,
    pub diagnostics: Vec<NmdDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdParsedMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_object_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_fps: Option<f32>,
    pub annotation_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdKeyframeBundleCounts {
    pub accessory: usize,
    pub bone: usize,
    pub camera: usize,
    pub light: usize,
    pub model: usize,
    pub morph: usize,
    pub self_shadow: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdPayloadSummary {
    pub accessory_keyframes: usize,
    pub bone_local_tracks: usize,
    pub bone_keyframes: usize,
    pub morph_local_tracks: usize,
    pub morph_keyframes: usize,
    pub camera_keyframes: usize,
    pub light_keyframes: usize,
    pub model_keyframes: usize,
    pub self_shadow_keyframes: usize,
    pub accessory_frames: Vec<NmdAccessoryKeyframe>,
    pub bone_frames: Vec<NmdBoneKeyframe>,
    pub morph_frames: Vec<NmdMorphKeyframe>,
    pub camera_frames: Vec<NmdCameraKeyframe>,
    pub light_frames: Vec<NmdLightKeyframe>,
    pub model_frames: Vec<NmdModelKeyframe>,
    pub self_shadow_frames: Vec<NmdSelfShadowKeyframe>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdAccessoryKeyframe {
    pub common: NmdKeyframeCommon,
    pub track_index: u64,
    pub translation: [f32; 3],
    pub orientation: [f32; 4],
    pub scale_factor: f32,
    pub opacity: f32,
    pub effect_parameters: Vec<NmdEffectParameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<NmdModelBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadow_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_blending_enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdKeyframeCommon {
    pub frame_index: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layer_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdBoneKeyframe {
    pub common: NmdKeyframeCommon,
    pub track_index: u64,
    pub local_translation: [f32; 3],
    pub local_orientation: [f32; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<NmdBoneKeyframeInterpolation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physics_simulation_enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdBoneKeyframeInterpolation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<NmdInterpolationUnit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdMorphKeyframe {
    pub common: NmdKeyframeCommon,
    pub track_index: u64,
    pub weight: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<NmdInterpolationUnit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdCameraKeyframe {
    pub common: NmdKeyframeCommon,
    pub look_at: [f32; 3],
    pub angle: [f32; 3],
    pub fov: f32,
    pub distance: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interpolation: Option<NmdCameraKeyframeInterpolation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perspective_view_enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdCameraKeyframeInterpolation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub angle: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fov: Option<NmdInterpolationUnit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<NmdInterpolationUnit>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdLightKeyframe {
    pub common: NmdKeyframeCommon,
    pub color: [f32; 3],
    pub direction: [f32; 3],
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdModelKeyframe {
    pub common: NmdKeyframeCommon,
    pub visible: bool,
    pub constraint_state_count: usize,
    pub effect_parameter_count: usize,
    pub binding_count: usize,
    pub constraint_states: Vec<NmdConstraintState>,
    pub effect_parameters: Vec<NmdEffectParameter>,
    pub bindings: Vec<NmdModelBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<NmdEdge>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_blending_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physics_simulation_enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdConstraintState {
    pub track_index: u64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdEffectParameter {
    pub track_index: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int_value: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float_value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_value: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdModelBinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_bone_track_index: Option<u64>,
    pub global_object_track_index: u64,
    pub global_bone_track_index: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdEdge {
    pub color: [f32; 4],
    pub scale_factor: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdSelfShadowKeyframe {
    pub common: NmdKeyframeCommon,
    pub enabled: bool,
    pub mode: i32,
    pub distance: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdInterpolationUnit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integer: Option<[u32; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub float: Option<[f32; 4]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmdDiagnostic {
    pub level: String,
    pub code: String,
    pub message: String,
}

pub fn parse_nmd_manifest(data: &[u8]) -> Result<NmdParsedManifest, ImportError> {
    if data.is_empty() {
        return Err(ImportError::UnexpectedEof(1));
    }

    let mut r = ProtoReader { data, pos: 0 };
    let mut metadata = NmdParsedMetadata {
        main_object_name: None,
        preferred_fps: None,
        annotation_count: 0,
    };
    let mut global_track_count = 0usize;
    let mut keyframe_bundles = NmdKeyframeBundleCounts::default();
    let mut payload = NmdPayloadSummary::default();

    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => {
                r.skip_len()?;
                metadata.annotation_count += 1;
            }
            (2, WIRE_LEN) => {
                let bytes = r.read_len()?;
                metadata.main_object_name = Some(String::from_utf8_lossy(bytes).into_owned());
            }
            (3, WIRE_FIXED32) => {
                let preferred_fps = r.read_f32()?;
                if !preferred_fps.is_finite() {
                    return Err(ImportError::UnsupportedFormat {
                        format: "NMD",
                        detail: "preferred FPS is not finite",
                    });
                }
                metadata.preferred_fps = Some(preferred_fps);
            }
            (4, WIRE_LEN) => {
                r.skip_len()?;
                global_track_count += 1;
            }
            (5, WIRE_LEN) => {
                let bytes = r.read_len()?;
                count_keyframe_bundle(bytes, &mut keyframe_bundles, &mut payload)?;
            }
            _ => r.skip_value(wire_type)?,
        }
    }

    Ok(NmdParsedManifest {
        format: "nmd",
        byte_length: data.len(),
        metadata,
        global_track_count,
        keyframe_bundles,
        payload,
        diagnostics: vec![NmdDiagnostic {
            level: "warning".to_owned(),
            code: "NMD_MOTION_DTO".to_owned(),
            message: "NMD protobuf was parsed as a motion DTO; exporter roundtrip is not implemented yet.".to_owned(),
        }],
    })
}

fn count_keyframe_bundle(
    data: &[u8],
    counts: &mut NmdKeyframeBundleCounts,
    payload: &mut NmdPayloadSummary,
) -> Result<(), ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut oneof_payload = None;
    let mut saw_len_field = false;
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        if wire_type == WIRE_LEN {
            saw_len_field = true;
            if matches!(field, 1 | 2 | 3 | 5 | 6 | 7 | 9) {
                oneof_payload = Some((field, r.peek_len()?));
            }
        }
        r.skip_value(wire_type)?;
    }

    match oneof_payload {
        Some((1, bytes)) => {
            counts.accessory += 1;
            let frames = read_len_messages(bytes, 2)?
                .into_iter()
                .map(parse_accessory_keyframe)
                .collect::<Result<Vec<_>, _>>()?;
            payload.accessory_keyframes += frames.len();
            payload.accessory_frames.extend(frames);
        }
        Some((2, bytes)) => {
            counts.bone += 1;
            count_bone_bundle_payload(bytes, payload)?;
        }
        Some((3, bytes)) => {
            counts.camera += 1;
            let frames = read_len_messages(bytes, 2)?
                .into_iter()
                .map(parse_camera_keyframe)
                .collect::<Result<Vec<_>, _>>()?;
            payload.camera_keyframes += frames.len();
            payload.camera_frames.extend(frames);
        }
        Some((5, bytes)) => {
            counts.light += 1;
            let frames = read_len_messages(bytes, 2)?
                .into_iter()
                .map(parse_light_keyframe)
                .collect::<Result<Vec<_>, _>>()?;
            payload.light_keyframes += frames.len();
            payload.light_frames.extend(frames);
        }
        Some((6, bytes)) => {
            counts.model += 1;
            let frames = read_len_messages(bytes, 2)?
                .into_iter()
                .map(parse_model_keyframe)
                .collect::<Result<Vec<_>, _>>()?;
            payload.model_keyframes += frames.len();
            payload.model_frames.extend(frames);
        }
        Some((7, bytes)) => {
            counts.morph += 1;
            count_morph_bundle_payload(bytes, payload)?;
        }
        Some((9, bytes)) => {
            counts.self_shadow += 1;
            let frames = read_len_messages(bytes, 2)?
                .into_iter()
                .map(parse_self_shadow_keyframe)
                .collect::<Result<Vec<_>, _>>()?;
            payload.self_shadow_keyframes += frames.len();
            payload.self_shadow_frames.extend(frames);
        }
        Some((_, _)) => counts.unknown += 1,
        None if saw_len_field => counts.unknown += 1,
        None => counts.unknown += 1,
    }
    Ok(())
}

fn count_bone_bundle_payload(
    data: &[u8],
    payload: &mut NmdPayloadSummary,
) -> Result<(), ImportError> {
    payload.bone_local_tracks += count_len_field(data, 2)?;
    let frames = read_len_messages(data, 3)?
        .into_iter()
        .map(parse_bone_keyframe)
        .collect::<Result<Vec<_>, _>>()?;
    payload.bone_keyframes += frames.len();
    payload.bone_frames.extend(frames);
    Ok(())
}

fn count_morph_bundle_payload(
    data: &[u8],
    payload: &mut NmdPayloadSummary,
) -> Result<(), ImportError> {
    payload.morph_local_tracks += count_len_field(data, 2)?;
    let frames = read_len_messages(data, 3)?
        .into_iter()
        .map(parse_morph_keyframe)
        .collect::<Result<Vec<_>, _>>()?;
    payload.morph_keyframes += frames.len();
    payload.morph_frames.extend(frames);
    Ok(())
}

fn count_len_field(data: &[u8], target_field: u32) -> Result<usize, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut count = 0usize;
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        if field == target_field && wire_type == WIRE_LEN {
            count += 1;
        }
        r.skip_value(wire_type)?;
    }
    Ok(count)
}

fn read_len_messages(data: &[u8], target_field: u32) -> Result<Vec<&[u8]>, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut messages = Vec::new();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        if field == target_field && wire_type == WIRE_LEN {
            messages.push(r.peek_len()?);
        }
        r.skip_value(wire_type)?;
    }
    Ok(messages)
}

fn parse_common(data: &[u8]) -> Result<NmdKeyframeCommon, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut common = NmdKeyframeCommon::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (2, WIRE_VARINT) => common.frame_index = r.read_varint()?,
            (3, WIRE_VARINT) => common.layer_index = Some(read_u32_varint(&mut r)?),
            (4, WIRE_VARINT) => common.selected = Some(r.read_bool()?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(common)
}

fn parse_accessory_keyframe(data: &[u8]) -> Result<NmdAccessoryKeyframe, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut frame = NmdAccessoryKeyframe::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => frame.common = parse_common(r.read_len()?)?,
            (2, WIRE_VARINT) => frame.track_index = r.read_varint()?,
            (3, WIRE_LEN) => frame.translation = parse_vec3(r.read_len()?)?,
            (4, WIRE_LEN) => frame.orientation = parse_vec4(r.read_len()?)?,
            (5, WIRE_FIXED32) => frame.scale_factor = r.read_f32()?,
            (6, WIRE_FIXED32) => frame.opacity = r.read_f32()?,
            (7, WIRE_LEN) => frame
                .effect_parameters
                .push(parse_effect_parameter(r.read_len()?)?),
            (8, WIRE_LEN) => frame.binding = Some(parse_model_binding(r.read_len()?)?),
            (9, WIRE_VARINT) => frame.visible = Some(r.read_bool()?),
            (10, WIRE_VARINT) => frame.shadow_enabled = Some(r.read_bool()?),
            (11, WIRE_VARINT) => frame.add_blending_enabled = Some(r.read_bool()?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(frame)
}

fn parse_bone_keyframe(data: &[u8]) -> Result<NmdBoneKeyframe, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut frame = NmdBoneKeyframe::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => frame.common = parse_common(r.read_len()?)?,
            (2, WIRE_VARINT) => frame.track_index = r.read_varint()?,
            (3, WIRE_LEN) => frame.local_translation = parse_vec3(r.read_len()?)?,
            (4, WIRE_LEN) => frame.local_orientation = parse_vec4(r.read_len()?)?,
            (5, WIRE_LEN) => frame.interpolation = Some(parse_bone_interpolation(r.read_len()?)?),
            (6, WIRE_VARINT) => frame.stage_index = Some(read_u32_varint(&mut r)?),
            (7, WIRE_VARINT) => frame.physics_simulation_enabled = Some(r.read_bool()?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(frame)
}

fn parse_morph_keyframe(data: &[u8]) -> Result<NmdMorphKeyframe, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut frame = NmdMorphKeyframe::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => frame.common = parse_common(r.read_len()?)?,
            (2, WIRE_VARINT) => frame.track_index = r.read_varint()?,
            (3, WIRE_FIXED32) => frame.weight = r.read_f32()?,
            (4, WIRE_LEN) => frame.interpolation = Some(parse_morph_interpolation(r.read_len()?)?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(frame)
}

fn parse_camera_keyframe(data: &[u8]) -> Result<NmdCameraKeyframe, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut frame = NmdCameraKeyframe::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => frame.common = parse_common(r.read_len()?)?,
            (2, WIRE_LEN) => frame.look_at = parse_vec3(r.read_len()?)?,
            (3, WIRE_LEN) => frame.angle = parse_vec3(r.read_len()?)?,
            (4, WIRE_FIXED32) => frame.fov = r.read_f32()?,
            (5, WIRE_FIXED32) => frame.distance = r.read_f32()?,
            (6, WIRE_LEN) => frame.interpolation = Some(parse_camera_interpolation(r.read_len()?)?),
            (7, WIRE_VARINT) => frame.stage_index = Some(read_u32_varint(&mut r)?),
            (8, WIRE_VARINT) => frame.perspective_view_enabled = Some(r.read_bool()?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(frame)
}

fn parse_light_keyframe(data: &[u8]) -> Result<NmdLightKeyframe, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut frame = NmdLightKeyframe::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => frame.common = parse_common(r.read_len()?)?,
            (2, WIRE_LEN) => frame.color = parse_vec3(r.read_len()?)?,
            (3, WIRE_LEN) => frame.direction = parse_vec3(r.read_len()?)?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(frame)
}

fn parse_model_keyframe(data: &[u8]) -> Result<NmdModelKeyframe, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut frame = NmdModelKeyframe::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => frame.common = parse_common(r.read_len()?)?,
            (2, WIRE_VARINT) => frame.visible = r.read_bool()?,
            (3, WIRE_LEN) => {
                frame.constraint_state_count += 1;
                frame
                    .constraint_states
                    .push(parse_constraint_state(r.read_len()?)?);
            }
            (4, WIRE_LEN) => {
                frame.effect_parameter_count += 1;
                frame
                    .effect_parameters
                    .push(parse_effect_parameter(r.read_len()?)?);
            }
            (5, WIRE_LEN) => {
                frame.binding_count += 1;
                frame.bindings.push(parse_model_binding(r.read_len()?)?);
            }
            (6, WIRE_LEN) => frame.edge = Some(parse_edge(r.read_len()?)?),
            (7, WIRE_VARINT) => frame.add_blending_enabled = Some(r.read_bool()?),
            (8, WIRE_VARINT) => frame.physics_simulation_enabled = Some(r.read_bool()?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(frame)
}

fn parse_self_shadow_keyframe(data: &[u8]) -> Result<NmdSelfShadowKeyframe, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut frame = NmdSelfShadowKeyframe::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => frame.common = parse_common(r.read_len()?)?,
            (2, WIRE_VARINT) => frame.enabled = r.read_bool()?,
            (3, WIRE_VARINT) => frame.mode = read_i32_varint(&mut r)?,
            (4, WIRE_FIXED32) => frame.distance = r.read_f32()?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(frame)
}

fn parse_bone_interpolation(data: &[u8]) -> Result<NmdBoneKeyframeInterpolation, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut interpolation = NmdBoneKeyframeInterpolation::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => interpolation.x = Some(parse_interpolation_unit(r.read_len()?)?),
            (2, WIRE_LEN) => interpolation.y = Some(parse_interpolation_unit(r.read_len()?)?),
            (3, WIRE_LEN) => interpolation.z = Some(parse_interpolation_unit(r.read_len()?)?),
            (4, WIRE_LEN) => {
                interpolation.orientation = Some(parse_interpolation_unit(r.read_len()?)?)
            }
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(interpolation)
}

fn parse_morph_interpolation(data: &[u8]) -> Result<NmdInterpolationUnit, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut interpolation = None;
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => interpolation = Some(parse_interpolation_unit(r.read_len()?)?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(interpolation.unwrap_or_default())
}

fn parse_camera_interpolation(data: &[u8]) -> Result<NmdCameraKeyframeInterpolation, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut interpolation = NmdCameraKeyframeInterpolation::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => interpolation.x = Some(parse_interpolation_unit(r.read_len()?)?),
            (2, WIRE_LEN) => interpolation.y = Some(parse_interpolation_unit(r.read_len()?)?),
            (3, WIRE_LEN) => interpolation.z = Some(parse_interpolation_unit(r.read_len()?)?),
            (4, WIRE_LEN) => interpolation.angle = Some(parse_interpolation_unit(r.read_len()?)?),
            (5, WIRE_LEN) => interpolation.fov = Some(parse_interpolation_unit(r.read_len()?)?),
            (6, WIRE_LEN) => {
                interpolation.distance = Some(parse_interpolation_unit(r.read_len()?)?)
            }
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(interpolation)
}

fn parse_interpolation_unit(data: &[u8]) -> Result<NmdInterpolationUnit, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut unit = NmdInterpolationUnit::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => unit.integer = Some(parse_integer_interpolation(r.read_len()?)?),
            (2, WIRE_LEN) => unit.float = Some(parse_float_interpolation(r.read_len()?)?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(unit)
}

fn parse_integer_interpolation(data: &[u8]) -> Result<[u32; 4], ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut values = [0; 4];
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_VARINT) => values[0] = read_u32_varint(&mut r)?,
            (2, WIRE_VARINT) => values[1] = read_u32_varint(&mut r)?,
            (3, WIRE_VARINT) => values[2] = read_u32_varint(&mut r)?,
            (4, WIRE_VARINT) => values[3] = read_u32_varint(&mut r)?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(values)
}

fn parse_float_interpolation(data: &[u8]) -> Result<[f32; 4], ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut values = [0.0; 4];
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_FIXED32) => values[0] = r.read_f32()?,
            (2, WIRE_FIXED32) => values[1] = r.read_f32()?,
            (3, WIRE_FIXED32) => values[2] = r.read_f32()?,
            (4, WIRE_FIXED32) => values[3] = r.read_f32()?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(values)
}

fn parse_constraint_state(data: &[u8]) -> Result<NmdConstraintState, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut state = NmdConstraintState::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_VARINT) => state.track_index = r.read_varint()?,
            (2, WIRE_VARINT) => state.enabled = r.read_bool()?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(state)
}

fn parse_effect_parameter(data: &[u8]) -> Result<NmdEffectParameter, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut parameter = NmdEffectParameter::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_VARINT) => parameter.track_index = r.read_varint()?,
            (2, WIRE_VARINT) => parameter.bool_value = Some(r.read_bool()?),
            (3, WIRE_VARINT) => parameter.int_value = Some(read_i32_varint(&mut r)?),
            (4, WIRE_FIXED32) => parameter.float_value = Some(r.read_f32()?),
            (5, WIRE_LEN) => parameter.vector_value = Some(parse_vec4(r.read_len()?)?),
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(parameter)
}

fn parse_model_binding(data: &[u8]) -> Result<NmdModelBinding, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut binding = NmdModelBinding::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_VARINT) => binding.local_bone_track_index = Some(r.read_varint()?),
            (2, WIRE_VARINT) => binding.global_object_track_index = r.read_varint()?,
            (3, WIRE_VARINT) => binding.global_bone_track_index = r.read_varint()?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(binding)
}

fn parse_edge(data: &[u8]) -> Result<NmdEdge, ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut edge = NmdEdge::default();
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_LEN) => edge.color = parse_vec4(r.read_len()?)?,
            (2, WIRE_FIXED32) => edge.scale_factor = r.read_f32()?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(edge)
}

fn parse_vec3(data: &[u8]) -> Result<[f32; 3], ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut values = [0.0; 3];
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_FIXED32) => values[0] = r.read_f32()?,
            (2, WIRE_FIXED32) => values[1] = r.read_f32()?,
            (3, WIRE_FIXED32) => values[2] = r.read_f32()?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(values)
}

fn parse_vec4(data: &[u8]) -> Result<[f32; 4], ImportError> {
    let mut r = ProtoReader { data, pos: 0 };
    let mut values = [0.0; 4];
    while !r.is_eof() {
        let Some((field, wire_type)) = r.read_key()? else {
            break;
        };
        match (field, wire_type) {
            (1, WIRE_FIXED32) => values[0] = r.read_f32()?,
            (2, WIRE_FIXED32) => values[1] = r.read_f32()?,
            (3, WIRE_FIXED32) => values[2] = r.read_f32()?,
            (4, WIRE_FIXED32) => values[3] = r.read_f32()?,
            _ => r.skip_value(wire_type)?,
        }
    }
    Ok(values)
}

fn read_u32_varint(r: &mut ProtoReader<'_>) -> Result<u32, ImportError> {
    let value = r.read_varint()?;
    if value > u64::from(u32::MAX) {
        return Err(ImportError::UnsupportedFormat {
            format: "NMD",
            detail: "uint32 value overflows u32",
        });
    }
    Ok(value as u32)
}

fn read_i32_varint(r: &mut ProtoReader<'_>) -> Result<i32, ImportError> {
    let value = r.read_varint()?;
    if value > u64::from(u32::MAX) {
        return Err(ImportError::UnsupportedFormat {
            format: "NMD",
            detail: "int32 varint value overflows u32",
        });
    }
    Ok(value as u32 as i32)
}

const WIRE_VARINT: u8 = 0;
const WIRE_FIXED64: u8 = 1;
const WIRE_LEN: u8 = 2;
const WIRE_FIXED32: u8 = 5;

type ProtoReader<'a> = ByteReader<'a>;

impl<'a> ByteReader<'a> {
    fn is_eof(&self) -> bool {
        self.remaining() == 0
    }

    fn read_key(&mut self) -> Result<Option<(u32, u8)>, ImportError> {
        if self.is_eof() {
            return Ok(None);
        }
        let key = self.read_varint()?;
        let field = key >> 3;
        if field == 0 {
            return Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "protobuf field number must be non-zero",
            });
        }
        if field > 0x1fff_ffff {
            return Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "protobuf field number exceeds the supported range",
            });
        }
        Ok(Some((field as u32, (key & 0x7) as u8)))
    }

    fn read_varint(&mut self) -> Result<u64, ImportError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = self.read_u8()?;
            if shift == 63 && byte > 1 {
                return Err(ImportError::UnsupportedFormat {
                    format: "NMD",
                    detail: "varint overflows u64",
                });
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(ImportError::UnsupportedFormat {
            format: "NMD",
            detail: "varint is too long",
        })
    }

    fn read_len(&mut self) -> Result<&'a [u8], ImportError> {
        let raw_len = self.read_varint()?;
        if raw_len > usize::MAX as u64 {
            return Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "length does not fit in usize",
            });
        }
        let len = raw_len as usize;
        self.read_bytes(len)
    }

    fn peek_len(&self) -> Result<&'a [u8], ImportError> {
        let mut copy = ProtoReader {
            data: self.data,
            pos: self.pos,
        };
        copy.read_len()
    }

    fn skip_len(&mut self) -> Result<(), ImportError> {
        self.read_len().map(|_| ())
    }

    fn read_f32(&mut self) -> Result<f32, ImportError> {
        let value = self.read_f32_le()?;
        if !value.is_finite() {
            return Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "float value is not finite",
            });
        }
        Ok(value)
    }

    fn read_bool(&mut self) -> Result<bool, ImportError> {
        Ok(self.read_varint()? != 0)
    }

    fn skip_value(&mut self, wire_type: u8) -> Result<(), ImportError> {
        match wire_type {
            WIRE_VARINT => {
                self.read_varint()?;
                Ok(())
            }
            WIRE_FIXED64 => {
                self.skip_bytes(8)?;
                Ok(())
            }
            WIRE_LEN => self.skip_len(),
            WIRE_FIXED32 => {
                self.skip_bytes(4)?;
                Ok(())
            }
            _ => Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "unsupported protobuf wire type",
            }),
        }
    }

    fn skip_bytes(&mut self, len: usize) -> Result<(), ImportError> {
        self.skip(len)
    }
}

fn default_nmd_format() -> &'static str {
    "nmd"
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(field: u32, wire_type: u8) -> u8 {
        ((field << 3) | u32::from(wire_type)) as u8
    }

    fn push_len(out: &mut Vec<u8>, field: u32, bytes: &[u8]) {
        out.push(key(field, WIRE_LEN));
        push_varint(out, bytes.len() as u64);
        out.extend_from_slice(bytes);
    }

    fn push_varint(out: &mut Vec<u8>, mut value: u64) {
        while value >= 0x80 {
            out.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn push_varint_field(out: &mut Vec<u8>, field: u32, value: u64) {
        out.push(key(field, WIRE_VARINT));
        push_varint(out, value);
    }

    fn push_f32_field(out: &mut Vec<u8>, field: u32, value: f32) {
        out.push(key(field, WIRE_FIXED32));
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn common(frame_index: u64) -> Vec<u8> {
        let mut out = Vec::new();
        push_varint_field(&mut out, 2, frame_index);
        out
    }

    fn vec3(x: f32, y: f32, z: f32) -> Vec<u8> {
        let mut out = Vec::new();
        push_f32_field(&mut out, 1, x);
        push_f32_field(&mut out, 2, y);
        push_f32_field(&mut out, 3, z);
        out
    }

    fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec<u8> {
        let mut out = vec3(x, y, z);
        push_f32_field(&mut out, 4, w);
        out
    }

    fn integer_interp(x0: u32, y0: u32, x1: u32, y1: u32) -> Vec<u8> {
        let mut out = Vec::new();
        push_varint_field(&mut out, 1, u64::from(x0));
        push_varint_field(&mut out, 2, u64::from(y0));
        push_varint_field(&mut out, 3, u64::from(x1));
        push_varint_field(&mut out, 4, u64::from(y1));
        out
    }

    fn float_interp(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<u8> {
        let mut out = Vec::new();
        push_f32_field(&mut out, 1, x0);
        push_f32_field(&mut out, 2, y0);
        push_f32_field(&mut out, 3, x1);
        push_f32_field(&mut out, 4, y1);
        out
    }

    fn interp_unit_integer(values: [u32; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        push_len(
            &mut out,
            1,
            &integer_interp(values[0], values[1], values[2], values[3]),
        );
        out
    }

    fn interp_unit_float(values: [f32; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        push_len(
            &mut out,
            2,
            &float_interp(values[0], values[1], values[2], values[3]),
        );
        out
    }

    fn push_bundle(out: &mut Vec<u8>, field: u32) {
        let bundle = vec![key(field, WIRE_LEN), 0];
        push_len(out, 5, &bundle);
    }

    fn push_bone_bundle(out: &mut Vec<u8>, local_track_count: usize, keyframe_count: usize) {
        let mut bone = Vec::new();
        for _ in 0..local_track_count {
            push_len(&mut bone, 2, &[]);
        }
        for _ in 0..keyframe_count {
            push_len(&mut bone, 3, &[]);
        }
        let mut unit = Vec::new();
        push_len(&mut unit, 2, &bone);
        push_len(out, 5, &unit);
    }

    fn push_morph_bundle(out: &mut Vec<u8>, local_track_count: usize, keyframe_count: usize) {
        let mut morph = Vec::new();
        for _ in 0..local_track_count {
            push_len(&mut morph, 2, &[]);
        }
        for _ in 0..keyframe_count {
            push_len(&mut morph, 3, &[]);
        }
        let mut unit = Vec::new();
        push_len(&mut unit, 7, &morph);
        push_len(out, 5, &unit);
    }

    fn push_len_keyframe_payload(out: &mut Vec<u8>, field: u32, keyframe_count: usize) {
        let mut payload = Vec::new();
        for _ in 0..keyframe_count {
            push_len(&mut payload, 2, &[]);
        }
        let mut unit = Vec::new();
        push_len(&mut unit, field, &payload);
        push_len(out, 5, &unit);
    }

    #[test]
    fn parses_nmd_top_level_motion_manifest() {
        let mut data = Vec::new();
        push_len(&mut data, 2, b"main-model");
        data.push(key(3, WIRE_FIXED32));
        data.extend_from_slice(&60.0f32.to_le_bytes());
        push_len(&mut data, 4, &[key(1, WIRE_VARINT), 1]);
        push_bone_bundle(&mut data, 2, 3);
        push_bundle(&mut data, 3);
        push_morph_bundle(&mut data, 1, 2);
        push_len_keyframe_payload(&mut data, 5, 4);
        push_len_keyframe_payload(&mut data, 6, 5);
        push_len_keyframe_payload(&mut data, 9, 6);

        let parsed = parse_nmd_manifest(&data).unwrap();

        assert_eq!(parsed.format, "nmd");
        assert_eq!(parsed.byte_length, data.len());
        assert_eq!(
            parsed.metadata.main_object_name.as_deref(),
            Some("main-model")
        );
        assert_eq!(parsed.metadata.preferred_fps, Some(60.0));
        assert_eq!(parsed.global_track_count, 1);
        assert_eq!(parsed.keyframe_bundles.bone, 1);
        assert_eq!(parsed.keyframe_bundles.camera, 1);
        assert_eq!(parsed.keyframe_bundles.light, 1);
        assert_eq!(parsed.keyframe_bundles.model, 1);
        assert_eq!(parsed.keyframe_bundles.morph, 1);
        assert_eq!(parsed.keyframe_bundles.self_shadow, 1);
        assert_eq!(parsed.payload.bone_local_tracks, 2);
        assert_eq!(parsed.payload.bone_keyframes, 3);
        assert_eq!(parsed.payload.light_keyframes, 4);
        assert_eq!(parsed.payload.model_keyframes, 5);
        assert_eq!(parsed.payload.morph_local_tracks, 1);
        assert_eq!(parsed.payload.morph_keyframes, 2);
        assert_eq!(parsed.payload.self_shadow_keyframes, 6);
        assert_eq!(parsed.diagnostics[0].code, "NMD_MOTION_DTO");
    }

    #[test]
    fn parses_nmd_keyframe_value_payloads() {
        let mut data = Vec::new();

        let mut accessory_frame = Vec::new();
        push_len(&mut accessory_frame, 1, &common(9));
        push_varint_field(&mut accessory_frame, 2, 2);
        push_len(&mut accessory_frame, 3, &vec3(7.0, 8.0, 9.0));
        push_len(&mut accessory_frame, 4, &vec4(0.0, 0.0, 0.0, 1.0));
        push_f32_field(&mut accessory_frame, 5, 1.25);
        push_f32_field(&mut accessory_frame, 6, 0.75);
        let mut accessory_effect = Vec::new();
        push_varint_field(&mut accessory_effect, 1, 8);
        push_f32_field(&mut accessory_effect, 4, 0.5);
        push_len(&mut accessory_frame, 7, &accessory_effect);
        let mut accessory_binding = Vec::new();
        push_varint_field(&mut accessory_binding, 1, 1);
        push_varint_field(&mut accessory_binding, 2, 2);
        push_varint_field(&mut accessory_binding, 3, 3);
        push_len(&mut accessory_frame, 8, &accessory_binding);
        push_varint_field(&mut accessory_frame, 9, 1);
        push_varint_field(&mut accessory_frame, 10, 0);
        push_varint_field(&mut accessory_frame, 11, 1);
        let mut accessory_bundle = Vec::new();
        push_len(&mut accessory_bundle, 2, &accessory_frame);
        let mut accessory_unit = Vec::new();
        push_len(&mut accessory_unit, 1, &accessory_bundle);
        push_len(&mut data, 5, &accessory_unit);

        let mut bone_frame = Vec::new();
        push_len(&mut bone_frame, 1, &common(10));
        push_varint_field(&mut bone_frame, 2, 3);
        push_len(&mut bone_frame, 3, &vec3(1.0, 2.0, 3.0));
        push_len(&mut bone_frame, 4, &vec4(0.0, 0.0, 0.0, 1.0));
        let mut bone_interpolation = Vec::new();
        push_len(
            &mut bone_interpolation,
            1,
            &interp_unit_integer([1, 2, 3, 4]),
        );
        push_len(
            &mut bone_interpolation,
            4,
            &interp_unit_float([0.1, 0.2, 0.3, 0.4]),
        );
        push_len(&mut bone_frame, 5, &bone_interpolation);
        push_varint_field(&mut bone_frame, 6, 2);
        push_varint_field(&mut bone_frame, 7, 1);
        let mut bone_bundle = Vec::new();
        push_len(&mut bone_bundle, 3, &bone_frame);
        let mut bone_unit = Vec::new();
        push_len(&mut bone_unit, 2, &bone_bundle);
        push_len(&mut data, 5, &bone_unit);

        let mut morph_frame = Vec::new();
        push_len(&mut morph_frame, 1, &common(11));
        push_varint_field(&mut morph_frame, 2, 4);
        push_f32_field(&mut morph_frame, 3, 0.5);
        let mut morph_interpolation = Vec::new();
        push_len(
            &mut morph_interpolation,
            1,
            &interp_unit_integer([5, 6, 7, 8]),
        );
        push_len(&mut morph_frame, 4, &morph_interpolation);
        let mut morph_bundle = Vec::new();
        push_len(&mut morph_bundle, 3, &morph_frame);
        let mut morph_unit = Vec::new();
        push_len(&mut morph_unit, 7, &morph_bundle);
        push_len(&mut data, 5, &morph_unit);

        let mut camera_frame = Vec::new();
        push_len(&mut camera_frame, 1, &common(12));
        push_len(&mut camera_frame, 2, &vec3(4.0, 5.0, 6.0));
        push_len(&mut camera_frame, 3, &vec3(0.1, 0.2, 0.3));
        push_f32_field(&mut camera_frame, 4, 45.0);
        push_f32_field(&mut camera_frame, 5, 20.0);
        let mut camera_interpolation = Vec::new();
        push_len(
            &mut camera_interpolation,
            6,
            &interp_unit_float([1.1, 1.2, 1.3, 1.4]),
        );
        push_len(&mut camera_frame, 6, &camera_interpolation);
        push_varint_field(&mut camera_frame, 8, 1);
        let mut camera_bundle = Vec::new();
        push_len(&mut camera_bundle, 2, &camera_frame);
        let mut camera_unit = Vec::new();
        push_len(&mut camera_unit, 3, &camera_bundle);
        push_len(&mut data, 5, &camera_unit);

        let mut light_frame = Vec::new();
        push_len(&mut light_frame, 1, &common(13));
        push_len(&mut light_frame, 2, &vec3(0.8, 0.7, 0.6));
        push_len(&mut light_frame, 3, &vec3(-1.0, -2.0, -3.0));
        let mut light_bundle = Vec::new();
        push_len(&mut light_bundle, 2, &light_frame);
        let mut light_unit = Vec::new();
        push_len(&mut light_unit, 5, &light_bundle);
        push_len(&mut data, 5, &light_unit);

        let mut model_frame = Vec::new();
        push_len(&mut model_frame, 1, &common(14));
        push_varint_field(&mut model_frame, 2, 1);
        let mut constraint = Vec::new();
        push_varint_field(&mut constraint, 1, 9);
        push_varint_field(&mut constraint, 2, 1);
        push_len(&mut model_frame, 3, &constraint);
        let mut effect = Vec::new();
        push_varint_field(&mut effect, 1, 10);
        push_len(&mut effect, 5, &vec4(0.1, 0.2, 0.3, 0.4));
        push_len(&mut model_frame, 4, &effect);
        let mut binding = Vec::new();
        push_varint_field(&mut binding, 1, 11);
        push_varint_field(&mut binding, 2, 12);
        push_varint_field(&mut binding, 3, 13);
        push_len(&mut model_frame, 5, &binding);
        let mut edge = Vec::new();
        push_len(&mut edge, 1, &vec4(1.0, 0.5, 0.25, 0.75));
        push_f32_field(&mut edge, 2, 1.5);
        push_len(&mut model_frame, 6, &edge);
        push_varint_field(&mut model_frame, 8, 0);
        let mut model_bundle = Vec::new();
        push_len(&mut model_bundle, 2, &model_frame);
        let mut model_unit = Vec::new();
        push_len(&mut model_unit, 6, &model_bundle);
        push_len(&mut data, 5, &model_unit);

        let mut shadow_frame = Vec::new();
        push_len(&mut shadow_frame, 1, &common(15));
        push_varint_field(&mut shadow_frame, 2, 1);
        push_varint_field(&mut shadow_frame, 3, 2);
        push_f32_field(&mut shadow_frame, 4, 30.0);
        let mut shadow_bundle = Vec::new();
        push_len(&mut shadow_bundle, 2, &shadow_frame);
        let mut shadow_unit = Vec::new();
        push_len(&mut shadow_unit, 9, &shadow_bundle);
        push_len(&mut data, 5, &shadow_unit);

        let parsed = parse_nmd_manifest(&data).unwrap();

        assert_eq!(parsed.payload.accessory_keyframes, 1);
        assert_eq!(parsed.payload.accessory_frames[0].common.frame_index, 9);
        assert_eq!(parsed.payload.accessory_frames[0].track_index, 2);
        assert_eq!(
            parsed.payload.accessory_frames[0].translation,
            [7.0, 8.0, 9.0]
        );
        assert_eq!(parsed.payload.accessory_frames[0].scale_factor, 1.25);
        assert_eq!(parsed.payload.accessory_frames[0].opacity, 0.75);
        assert_eq!(
            parsed.payload.accessory_frames[0].effect_parameters[0].float_value,
            Some(0.5)
        );
        assert_eq!(
            parsed.payload.accessory_frames[0]
                .binding
                .as_ref()
                .map(|binding| binding.global_bone_track_index),
            Some(3)
        );
        assert_eq!(parsed.payload.accessory_frames[0].visible, Some(true));
        assert_eq!(
            parsed.payload.accessory_frames[0].shadow_enabled,
            Some(false)
        );
        assert_eq!(
            parsed.payload.accessory_frames[0].add_blending_enabled,
            Some(true)
        );
        assert_eq!(parsed.payload.bone_frames[0].common.frame_index, 10);
        assert_eq!(parsed.payload.bone_frames[0].track_index, 3);
        assert_eq!(
            parsed.payload.bone_frames[0].local_translation,
            [1.0, 2.0, 3.0]
        );
        assert_eq!(
            parsed.payload.bone_frames[0].local_orientation,
            [0.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(
            parsed.payload.bone_frames[0]
                .interpolation
                .as_ref()
                .and_then(|interpolation| interpolation.x.as_ref())
                .and_then(|unit| unit.integer),
            Some([1, 2, 3, 4])
        );
        assert_eq!(
            parsed.payload.bone_frames[0]
                .interpolation
                .as_ref()
                .and_then(|interpolation| interpolation.orientation.as_ref())
                .and_then(|unit| unit.float),
            Some([0.1, 0.2, 0.3, 0.4])
        );
        assert_eq!(parsed.payload.bone_frames[0].stage_index, Some(2));
        assert_eq!(
            parsed.payload.bone_frames[0].physics_simulation_enabled,
            Some(true)
        );
        assert_eq!(parsed.payload.morph_frames[0].weight, 0.5);
        assert_eq!(
            parsed.payload.morph_frames[0]
                .interpolation
                .as_ref()
                .and_then(|unit| unit.integer),
            Some([5, 6, 7, 8])
        );
        assert_eq!(parsed.payload.camera_frames[0].look_at, [4.0, 5.0, 6.0]);
        assert_eq!(parsed.payload.camera_frames[0].fov, 45.0);
        assert_eq!(
            parsed.payload.camera_frames[0]
                .interpolation
                .as_ref()
                .and_then(|interpolation| interpolation.distance.as_ref())
                .and_then(|unit| unit.float),
            Some([1.1, 1.2, 1.3, 1.4])
        );
        assert_eq!(
            parsed.payload.camera_frames[0].perspective_view_enabled,
            Some(true)
        );
        assert_eq!(parsed.payload.light_frames[0].color, [0.8, 0.7, 0.6]);
        assert!(parsed.payload.model_frames[0].visible);
        assert_eq!(parsed.payload.model_frames[0].constraint_state_count, 1);
        assert_eq!(
            parsed.payload.model_frames[0].constraint_states[0].track_index,
            9
        );
        assert!(parsed.payload.model_frames[0].constraint_states[0].enabled);
        assert_eq!(
            parsed.payload.model_frames[0].effect_parameters[0].track_index,
            10
        );
        assert_eq!(
            parsed.payload.model_frames[0].effect_parameters[0].vector_value,
            Some([0.1, 0.2, 0.3, 0.4])
        );
        assert_eq!(
            parsed.payload.model_frames[0].bindings[0].local_bone_track_index,
            Some(11)
        );
        assert_eq!(
            parsed.payload.model_frames[0].bindings[0].global_object_track_index,
            12
        );
        assert_eq!(
            parsed.payload.model_frames[0]
                .edge
                .as_ref()
                .map(|edge| edge.scale_factor),
            Some(1.5)
        );
        assert_eq!(
            parsed.payload.model_frames[0].physics_simulation_enabled,
            Some(false)
        );
        assert_eq!(parsed.payload.self_shadow_frames[0].mode, 2);
        assert_eq!(parsed.payload.self_shadow_frames[0].distance, 30.0);
    }

    #[test]
    fn rejects_truncated_nmd_len_field() {
        let result = parse_nmd_manifest(&[key(2, WIRE_LEN), 5, b'a']);

        assert!(matches!(result, Err(ImportError::UnexpectedEof(4))));
    }

    #[test]
    fn rejects_empty_nmd_manifest() {
        let result = parse_nmd_manifest(&[]);

        assert!(matches!(result, Err(ImportError::UnexpectedEof(1))));
    }

    #[test]
    fn rejects_overflowing_varint_without_panicking() {
        let result = parse_nmd_manifest(&[0xff; 10]);

        assert!(matches!(
            result,
            Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "varint overflows u64"
            })
        ));
    }

    #[test]
    fn rejects_zero_protobuf_field_number() {
        let result = parse_nmd_manifest(&[0]);

        assert!(matches!(
            result,
            Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "protobuf field number must be non-zero"
            })
        ));
    }

    #[test]
    fn rejects_oversized_protobuf_field_number() {
        let mut data = Vec::new();
        push_varint(&mut data, (0x2000_0000u64 << 3) | u64::from(WIRE_VARINT));

        let result = parse_nmd_manifest(&data);

        assert!(matches!(
            result,
            Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "protobuf field number exceeds the supported range"
            })
        ));
    }

    #[test]
    fn keyframe_bundle_uses_last_oneof_field() {
        let mut bone = Vec::new();
        push_len(&mut bone, 3, &[]);
        let mut morph = Vec::new();
        push_len(&mut morph, 3, &[]);
        push_len(&mut morph, 3, &[]);

        let mut unit = Vec::new();
        push_len(&mut unit, 2, &bone);
        push_len(&mut unit, 7, &morph);
        let mut data = Vec::new();
        push_len(&mut data, 5, &unit);

        let parsed = parse_nmd_manifest(&data).unwrap();

        assert_eq!(parsed.keyframe_bundles.bone, 0);
        assert_eq!(parsed.keyframe_bundles.morph, 1);
        assert_eq!(parsed.payload.bone_keyframes, 0);
        assert_eq!(parsed.payload.morph_keyframes, 2);
    }

    #[test]
    fn unknown_len_field_does_not_override_known_keyframe_bundle_oneof() {
        let mut bone = Vec::new();
        push_len(&mut bone, 3, &[]);

        let mut unit = Vec::new();
        push_len(&mut unit, 2, &bone);
        push_len(&mut unit, 10, b"unknown");
        let mut data = Vec::new();
        push_len(&mut data, 5, &unit);

        let parsed = parse_nmd_manifest(&data).unwrap();

        assert_eq!(parsed.keyframe_bundles.bone, 1);
        assert_eq!(parsed.keyframe_bundles.unknown, 0);
        assert_eq!(parsed.payload.bone_keyframes, 1);
    }

    #[test]
    fn parses_negative_int32_varint_values() {
        let mut shadow_frame = Vec::new();
        push_len(&mut shadow_frame, 1, &common(1));
        push_varint_field(&mut shadow_frame, 3, u64::from(u32::MAX));

        let mut shadow_bundle = Vec::new();
        push_len(&mut shadow_bundle, 2, &shadow_frame);
        let mut shadow_unit = Vec::new();
        push_len(&mut shadow_unit, 9, &shadow_bundle);
        let mut data = Vec::new();
        push_len(&mut data, 5, &shadow_unit);

        let parsed = parse_nmd_manifest(&data).unwrap();

        assert_eq!(parsed.payload.self_shadow_frames[0].mode, -1);
    }

    #[test]
    fn rejects_int32_varint_values_that_exceed_u32() {
        let mut shadow_frame = Vec::new();
        push_len(&mut shadow_frame, 1, &common(1));
        push_varint_field(&mut shadow_frame, 3, u64::from(u32::MAX) + 1);

        let mut shadow_bundle = Vec::new();
        push_len(&mut shadow_bundle, 2, &shadow_frame);
        let mut shadow_unit = Vec::new();
        push_len(&mut shadow_unit, 9, &shadow_bundle);
        let mut data = Vec::new();
        push_len(&mut data, 5, &shadow_unit);

        let result = parse_nmd_manifest(&data);

        assert!(matches!(
            result,
            Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "int32 varint value overflows u32"
            })
        ));
    }

    #[test]
    fn rejects_nonfinite_keyframe_float_values() {
        let mut accessory_frame = Vec::new();
        push_len(&mut accessory_frame, 1, &common(1));
        push_f32_field(&mut accessory_frame, 5, f32::NAN);

        let mut accessory_bundle = Vec::new();
        push_len(&mut accessory_bundle, 2, &accessory_frame);
        let mut accessory_unit = Vec::new();
        push_len(&mut accessory_unit, 1, &accessory_bundle);
        let mut data = Vec::new();
        push_len(&mut data, 5, &accessory_unit);

        let result = parse_nmd_manifest(&data);

        assert!(matches!(
            result,
            Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "float value is not finite"
            })
        ));
    }

    #[test]
    fn rejects_nonfinite_preferred_fps() {
        let mut data = Vec::new();
        data.push(key(3, WIRE_FIXED32));
        data.extend_from_slice(&f32::INFINITY.to_le_bytes());

        let result = parse_nmd_manifest(&data);

        assert!(matches!(
            result,
            Err(ImportError::UnsupportedFormat {
                format: "NMD",
                detail: "float value is not finite"
            })
        ));
    }

    #[test]
    fn parses_multibyte_length_delimited_fields() {
        let mut data = Vec::new();
        let name = vec![b'a'; 130];
        push_len(&mut data, 2, &name);

        let parsed = parse_nmd_manifest(&data).unwrap();
        let expected = "a".repeat(130);

        assert_eq!(
            parsed.metadata.main_object_name.as_deref(),
            Some(expected.as_str())
        );
    }
}
