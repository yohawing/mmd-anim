//! Wasm wrapper for browser hosts.
//!
//! The core crate remains independent from `wasm-bindgen`. This wrapper owns
//! JavaScript-facing handle types and copies contiguous runtime outputs into
//! typed-array-compatible `Vec<f32>` values.

use std::collections::HashMap;
use std::sync::Arc;

use mmd_anim_runtime::ModelArena;
use mmd_anim_runtime::{
    AnimationClip, AppendTransformInit, BoneAnimationBinding, BoneIndex, BoneInit, BoneMorphOffset,
    GroupMorphOffset, IkAngleLimit, IkLinkInit, IkSolveOptions, IkSolverInit,
    MorphAnimationBinding, MorphIndex, MorphInit, MorphKeyframe, MorphOffsetSpan, MorphTrack,
    MovableBoneKeyframe, MovableBoneTrack, PropertyAnimationBinding, PropertyKeyframe,
    RuntimeInstance,
};
use wasm_bindgen::prelude::*;

pub const WASM_WRAPPER_VERSION: u32 = 2;

const APPEND_FLAG_ROTATION: u32 = 1;
const APPEND_FLAG_TRANSLATION: u32 = 1 << 1;
const APPEND_FLAG_LOCAL: u32 = 1 << 2;
const IK_LINK_FLAG_ANGLE_LIMIT: u32 = 1;

#[wasm_bindgen]
pub fn wasm_wrapper_version() -> u32 {
    WASM_WRAPPER_VERSION
}

#[wasm_bindgen(js_name = parsePmxModelJson)]
pub fn parse_pmx_model_json(data: &[u8]) -> Result<String, JsValue> {
    if data.is_empty() {
        return Err(JsValue::from_str("PMX data is empty"));
    }
    let parsed = mmd_anim_format::parse_pmx_model(data)
        .map_err(|error| js_parser_error("PMX", "parsePmxModelJson", None, error))?;
    serde_json::to_string(&parsed).map_err(|e| JsValue::from_str(&e.to_string()))
}

fn pmx_model_non_geometry_json_from_parsed(
    parsed: &mmd_anim_format::pmx::PmxParsedModel,
) -> Result<String, String> {
    // Serialize each non-geometry field individually into a JSON object.
    // `parsed.geometry` is intentionally omitted — no geometry JSON is constructed.
    let mut obj = serde_json::Map::with_capacity(9);
    let mut sv = |key: &str, val: serde_json::Result<serde_json::Value>| -> Result<(), String> {
        obj.insert(key.to_owned(), val.map_err(|e| e.to_string())?);
        Ok(())
    };
    sv("metadata", serde_json::to_value(&parsed.metadata))?;
    sv("materials", serde_json::to_value(&parsed.materials))?;
    sv("skeleton", serde_json::to_value(&parsed.skeleton))?;
    sv("morphs", serde_json::to_value(&parsed.morphs))?;
    sv(
        "displayFrames",
        serde_json::to_value(&parsed.display_frames),
    )?;
    sv("rigidBodies", serde_json::to_value(&parsed.rigid_bodies))?;
    sv("joints", serde_json::to_value(&parsed.joints))?;
    sv("softBodies", serde_json::to_value(&parsed.soft_bodies))?;
    sv("diagnostics", serde_json::to_value(&parsed.diagnostics))?;
    serde_json::to_string(&serde_json::Value::Object(obj)).map_err(|e| e.to_string())
}

fn parse_pmx_model_non_geometry_json_inner(data: &[u8]) -> Result<String, String> {
    if data.is_empty() {
        return Err("PMX data is empty".to_owned());
    }
    let parsed = mmd_anim_format::parse_pmx_model(data).map_err(|e| e.to_string())?;
    pmx_model_non_geometry_json_from_parsed(&parsed)
}

/// Parse PMX bytes and return a JSON string with all model data **except** the
/// geometry section (vertex positions, normals, UVs, indices, skinning data).
///
/// Each non-geometry field is serialized individually — no geometry JSON is
/// constructed. Use `parsePmxModelJson` when full-model JSON is required.
#[wasm_bindgen(js_name = parsePmxModelNonGeometryJson)]
pub fn parse_pmx_model_non_geometry_json(data: &[u8]) -> Result<String, JsValue> {
    parse_pmx_model_non_geometry_json_inner(data)
        .map_err(|error| js_parser_error("PMX", "parsePmxModelNonGeometryJson", None, error))
}

#[wasm_bindgen(js_name = parseVmdAnimationJson)]
pub fn parse_vmd_animation_json(data: &[u8]) -> Result<String, JsValue> {
    if data.is_empty() {
        return Err(JsValue::from_str("VMD data is empty"));
    }
    let parsed = mmd_anim_format::parse_vmd_animation(data)
        .map_err(|error| js_parser_error("VMD", "parseVmdAnimationJson", None, error))?;
    serde_json::to_string(&parsed).map_err(js_error)
}

/// Sample VMD camera bytes into a caller-owned `Float32Array`.
///
/// Writes `[distance, position.x, position.y, position.z, rotation.x,
/// rotation.y, rotation.z, fov, perspective]` to `out`.
/// `perspective` is encoded as `1.0` when enabled, otherwise `0.0`.
/// Returns `false` when `out.length < 9`.
#[wasm_bindgen(js_name = sampleVmdCamera)]
pub fn sample_vmd_camera(
    data: &[u8],
    frame: f32,
    out: &js_sys::Float32Array,
) -> Result<bool, JsValue> {
    if data.is_empty() {
        return Err(JsValue::from_str("VMD data is empty"));
    }
    if !frame.is_finite() {
        return Err(JsValue::from_str("frame must be finite"));
    }
    let parsed = mmd_anim_format::parse_vmd_animation(data)
        .map_err(|error| js_parser_error("VMD", "sampleVmdCamera", None, error))?;
    let camera = mmd_anim_format::sample_vmd_camera_frames(&parsed.camera_frames, frame)
        .ok_or_else(|| JsValue::from_str("VMD has no camera keyframes"))?;
    copy_camera_state_array(camera, out)
}

/// Sample VMD light bytes into a caller-owned `Float32Array`.
///
/// Writes `[color.r, color.g, color.b, direction.x, direction.y,
/// direction.z]` to `out`. Returns `false` when `out.length < 6`.
#[wasm_bindgen(js_name = sampleVmdLight)]
pub fn sample_vmd_light(
    data: &[u8],
    frame: f32,
    out: &js_sys::Float32Array,
) -> Result<bool, JsValue> {
    if data.is_empty() {
        return Err(JsValue::from_str("VMD data is empty"));
    }
    if !frame.is_finite() {
        return Err(JsValue::from_str("frame must be finite"));
    }
    let parsed = mmd_anim_format::parse_vmd_animation(data)
        .map_err(|error| js_parser_error("VMD", "sampleVmdLight", None, error))?;
    let light = mmd_anim_format::sample_vmd_light_frames(&parsed.light_frames, frame)
        .ok_or_else(|| JsValue::from_str("VMD has no light keyframes"))?;
    copy_light_state_array(light, out)
}

/// Sample VMD self-shadow bytes into a caller-owned `Float32Array`.
///
/// Writes `[mode, distance]` to `out`. `mode` is encoded as a float.
/// Returns `false` when `out.length < 2`.
#[wasm_bindgen(js_name = sampleVmdSelfShadow)]
pub fn sample_vmd_self_shadow(
    data: &[u8],
    frame: f32,
    out: &js_sys::Float32Array,
) -> Result<bool, JsValue> {
    if data.is_empty() {
        return Err(JsValue::from_str("VMD data is empty"));
    }
    if !frame.is_finite() {
        return Err(JsValue::from_str("frame must be finite"));
    }
    let parsed = mmd_anim_format::parse_vmd_animation(data)
        .map_err(|error| js_parser_error("VMD", "sampleVmdSelfShadow", None, error))?;
    let self_shadow =
        mmd_anim_format::sample_vmd_self_shadow_frames(&parsed.self_shadow_frames, frame)
            .ok_or_else(|| JsValue::from_str("VMD has no self-shadow keyframes"))?;
    copy_self_shadow_state_array(self_shadow, out)
}

#[wasm_bindgen(js_name = parseMmdFormatJson)]
pub fn parse_mmd_format_json(data: &[u8], file_name: Option<String>) -> Result<String, JsValue> {
    if data.is_empty() {
        return Err(JsValue::from_str("MMD data is empty"));
    }
    let file_name_ref = file_name.as_deref();
    let value = match mmd_anim_format::detect_mmd_format(data, file_name_ref) {
        mmd_anim_format::MmdFormatKind::Pmx => serde_json::to_value(
            mmd_anim_format::parse_pmx_model(data)
                .map_err(|error| js_parser_error("PMX", "parseMmdFormatJson", None, error))?,
        )
        .map_err(js_error)?,
        mmd_anim_format::MmdFormatKind::Pmd => serde_json::to_value(
            mmd_anim_format::parse_pmd_model(data)
                .map_err(|error| js_parser_error("PMD", "parseMmdFormatJson", None, error))?,
        )
        .map_err(js_error)?,
        mmd_anim_format::MmdFormatKind::Vmd => serde_json::to_value(
            mmd_anim_format::parse_vmd_animation(data)
                .map_err(|error| js_parser_error("VMD", "parseMmdFormatJson", None, error))?,
        )
        .map_err(js_error)?,
        mmd_anim_format::MmdFormatKind::Vpd => serde_json::to_value(
            mmd_anim_format::parse_vpd_pose(data)
                .map_err(|error| js_parser_error("VPD", "parseMmdFormatJson", None, error))?,
        )
        .map_err(js_error)?,
        mmd_anim_format::MmdFormatKind::Pmm => serde_json::to_value(
            mmd_anim_format::parse_pmm_manifest(data)
                .map_err(|error| js_parser_error("PMM", "parseMmdFormatJson", None, error))?,
        )
        .map_err(js_error)?,
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            serde_json::to_value(
                mmd_anim_format::parse_accessory_manifest(data, file_name_ref)
                    .map_err(|error| js_parser_error("X/VAC", "parseMmdFormatJson", None, error))?,
            )
            .map_err(js_error)?
        }
        mmd_anim_format::MmdFormatKind::Nmd => serde_json::to_value(
            mmd_anim_format::parse_nmd_manifest(data)
                .map_err(|error| js_parser_error("NMD", "parseMmdFormatJson", None, error))?,
        )
        .map_err(js_error)?,
        mmd_anim_format::MmdFormatKind::Unknown => {
            return Err(JsValue::from_str("unknown MMD format"));
        }
    };
    serde_json::to_string(&value).map_err(js_error)
}

#[wasm_bindgen(js_name = exportVmdAnimationBytes)]
pub fn export_vmd_animation_bytes(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    let parsed = mmd_anim_format::parse_vmd_animation(data)
        .map_err(|error| js_parser_error("VMD", "exportVmdAnimationBytes", None, error))?;
    Ok(mmd_anim_format::export_vmd_animation(&parsed))
}

#[wasm_bindgen(js_name = exportVmdAnimationJsonBytes)]
pub fn export_vmd_animation_json_bytes(json: &str) -> Result<Vec<u8>, JsValue> {
    let parsed: mmd_anim_format::VmdParsedAnimation =
        serde_json::from_str(json).map_err(js_error)?;
    Ok(mmd_anim_format::export_vmd_animation(&parsed))
}

#[wasm_bindgen(js_name = exportPmxModelBytes)]
pub fn export_pmx_model_bytes(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    let parsed = mmd_anim_format::parse_pmx_model(data)
        .map_err(|error| js_parser_error("PMX", "exportPmxModelBytes", None, error))?;
    Ok(mmd_anim_format::export_pmx_model(&parsed))
}

#[wasm_bindgen(js_name = exportPmxModelJsonBytes)]
pub fn export_pmx_model_json_bytes(json: &str) -> Result<Vec<u8>, JsValue> {
    let parsed: mmd_anim_format::PmxParsedModel = serde_json::from_str(json).map_err(js_error)?;
    mmd_anim_format::validate_pmx_export_model(&parsed).map_err(js_error)?;
    Ok(mmd_anim_format::export_pmx_model(&parsed))
}

#[wasm_bindgen(js_name = exportPmxFromParts)]
#[allow(clippy::too_many_arguments)]
pub fn export_pmx_from_parts(
    metadata_json: &str,
    positions_xyz: &[f32],
    normals_xyz: &[f32],
    uvs_xy: &[f32],
    indices: &[u32],
    skin_indices: &[u32],
    skin_weights: &[f32],
    edge_scale: &[f32],
) -> Result<Vec<u8>, JsValue> {
    let descriptor: mmd_anim_format::PmxPartsDescriptor =
        serde_json::from_str(metadata_json).map_err(js_error)?;
    let model = mmd_anim_format::build_pmx_model_from_parts(mmd_anim_format::PmxPartsInput {
        descriptor,
        positions_xyz,
        normals_xyz,
        uvs_xy,
        indices,
        skin_indices,
        skin_weights,
        edge_scale,
    })
    .map_err(js_error)?;
    Ok(mmd_anim_format::export_pmx_model(&model))
}

#[wasm_bindgen(js_name = exportPmdModelBytes)]
pub fn export_pmd_model_bytes(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    let parsed = mmd_anim_format::parse_pmd_model(data)
        .map_err(|error| js_parser_error("PMD", "exportPmdModelBytes", None, error))?;
    Ok(mmd_anim_format::export_pmd_model(&parsed))
}

#[wasm_bindgen(js_name = exportPmdModelJsonBytes)]
pub fn export_pmd_model_json_bytes(json: &str) -> Result<Vec<u8>, JsValue> {
    let parsed: mmd_anim_format::PmdParsedModel = serde_json::from_str(json).map_err(js_error)?;
    Ok(mmd_anim_format::export_pmd_model(&parsed))
}

#[wasm_bindgen(js_name = exportAccessoryManifestBytes)]
pub fn export_accessory_manifest_bytes(
    data: &[u8],
    file_name: Option<String>,
) -> Result<Vec<u8>, JsValue> {
    let parsed = mmd_anim_format::parse_accessory_manifest(data, file_name.as_deref())
        .map_err(|error| js_parser_error("X/VAC", "exportAccessoryManifestBytes", None, error))?;
    Ok(mmd_anim_format::export_accessory_manifest(&parsed))
}

#[wasm_bindgen(js_name = exportVpdPoseBytes)]
pub fn export_vpd_pose_bytes(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    let parsed = mmd_anim_format::parse_vpd_pose(data)
        .map_err(|error| js_parser_error("VPD", "exportVpdPoseBytes", None, error))?;
    Ok(mmd_anim_format::export_vpd_pose(&parsed))
}

#[wasm_bindgen(js_name = exportVpdPoseJsonBytes)]
pub fn export_vpd_pose_json_bytes(json: &str) -> Result<Vec<u8>, JsValue> {
    let parsed: mmd_anim_format::VpdParsedPose = serde_json::from_str(json).map_err(js_error)?;
    Ok(mmd_anim_format::export_vpd_pose(&parsed))
}

#[wasm_bindgen(js_name = exportMmdFormatBytes)]
pub fn export_mmd_format_bytes(data: &[u8], file_name: Option<String>) -> Result<Vec<u8>, JsValue> {
    let file_name_ref = file_name.as_deref();
    match mmd_anim_format::detect_mmd_format(data, file_name_ref) {
        mmd_anim_format::MmdFormatKind::Pmx => export_pmx_model_bytes(data),
        mmd_anim_format::MmdFormatKind::Pmd => export_pmd_model_bytes(data),
        mmd_anim_format::MmdFormatKind::Vmd => export_vmd_animation_bytes(data),
        mmd_anim_format::MmdFormatKind::Vpd => export_vpd_pose_bytes(data),
        mmd_anim_format::MmdFormatKind::X | mmd_anim_format::MmdFormatKind::Vac => {
            export_accessory_manifest_bytes(data, file_name)
        }
        kind => Err(JsValue::from_str(&format!(
            "export is not implemented for {kind:?}"
        ))),
    }
}

// --- PMX geometry typed-array DTO ---

/// Typed-array geometry DTO for one parsed PMX model.
///
/// All getter methods return **owned copies** (no wasm-memory lifetime coupling).
///
/// Strides: positions/normals/sdefC/R0/R1/Rw0/Rw1 — vertex_count×3;
///   uvs — vertex_count×2; additionalUvs — additional_uv_count×vertex_count×4;
///   indices — face_count×3 (u32); materialGroups — group_count×3
///   ([start, count, materialIndex], u32); skinIndices/skinWeights — vertex_count×4;
///   edgeScale/sdefEnabled/qdefEnabled — vertex_count×1.
#[wasm_bindgen]
pub struct WasmPmxGeometry {
    positions: Vec<f32>,
    normals: Vec<f32>,
    uvs: Vec<f32>,
    additional_uvs: Vec<f32>,
    additional_uv_count: usize,
    indices: Vec<u32>,
    material_groups: Vec<u32>,
    skin_indices: Vec<u32>,
    skin_weights: Vec<f32>,
    edge_scale: Vec<f32>,
    sdef_enabled: Vec<u8>,
    sdef_c: Vec<f32>,
    sdef_r0: Vec<f32>,
    sdef_r1: Vec<f32>,
    sdef_rw0: Vec<f32>,
    sdef_rw1: Vec<f32>,
    qdef_enabled: Vec<u8>,
    skinning_modes: Vec<String>,
}

impl WasmPmxGeometry {
    fn from_geometry(g: &mmd_anim_format::pmx::PmxParsedGeometry) -> Self {
        Self {
            positions: g.positions.clone(),
            normals: g.normals.clone(),
            uvs: g.uvs.clone(),
            additional_uvs: g.additional_uvs.iter().flatten().copied().collect(),
            additional_uv_count: g.additional_uvs.len(),
            indices: g.indices.clone(),
            material_groups: g
                .material_groups
                .iter()
                .flat_map(|group| {
                    [
                        group.start as u32,
                        group.count as u32,
                        group.material_index as u32,
                    ]
                })
                .collect(),
            skin_indices: g.skin_indices.clone(),
            skin_weights: g.skin_weights.clone(),
            edge_scale: g.edge_scale.clone(),
            sdef_enabled: g.sdef.enabled.iter().map(|&v| u8::from(v != 0.0)).collect(),
            sdef_c: g.sdef.c.clone(),
            sdef_r0: g.sdef.r0.clone(),
            sdef_r1: g.sdef.r1.clone(),
            sdef_rw0: g.sdef.rw0.clone(),
            sdef_rw1: g.sdef.rw1.clone(),
            qdef_enabled: g.qdef.enabled.iter().map(|&v| u8::from(v != 0.0)).collect(),
            skinning_modes: pmx_skinning_modes(g),
        }
    }

    fn parse_inner(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("PMX data is empty".to_owned());
        }
        let parsed = mmd_anim_format::parse_pmx_model(data).map_err(|e| e.to_string())?;
        Ok(Self::from_geometry(&parsed.geometry))
    }
}

#[wasm_bindgen]
impl WasmPmxGeometry {
    /// Parse PMX bytes and return the geometry DTO. All returned arrays are copies.
    #[wasm_bindgen(js_name = fromPmxBytes)]
    pub fn from_pmx_bytes(data: &[u8]) -> Result<WasmPmxGeometry, JsValue> {
        Self::parse_inner(data)
            .map_err(|error| js_parser_error("PMX", "parsePmxGeometry", None, error))
    }

    #[wasm_bindgen(js_name = vertexCount)]
    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    #[wasm_bindgen(js_name = faceCount)]
    pub fn face_count(&self) -> usize {
        self.indices.len() / 3
    }

    #[wasm_bindgen(js_name = additionalUvCount)]
    pub fn additional_uv_count(&self) -> usize {
        self.additional_uv_count
    }

    #[wasm_bindgen(js_name = materialGroupCount)]
    pub fn material_group_count(&self) -> usize {
        self.material_groups.len() / 3
    }

    /// Copy of positions (vertex_count×3, XYZ, f32).
    #[wasm_bindgen(js_name = positions)]
    pub fn positions(&self) -> Vec<f32> {
        self.positions.clone()
    }

    /// Copy of normals (vertex_count×3, XYZ, f32).
    #[wasm_bindgen(js_name = normals)]
    pub fn normals(&self) -> Vec<f32> {
        self.normals.clone()
    }

    /// Copy of UV coordinates (vertex_count×2, UV, f32).
    #[wasm_bindgen(js_name = uvs)]
    pub fn uvs(&self) -> Vec<f32> {
        self.uvs.clone()
    }

    /// Copy of additional UV coordinates (additional_uv_count×vertex_count×4, f32).
    #[wasm_bindgen(js_name = additionalUvs)]
    pub fn additional_uvs(&self) -> Vec<f32> {
        self.additional_uvs.clone()
    }

    /// Copy of triangle indices (face_count×3, u32). u32 because PMX allows >65535 vertices.
    #[wasm_bindgen(js_name = indices)]
    pub fn indices(&self) -> Vec<u32> {
        self.indices.clone()
    }

    /// Copy of material groups (group_count×3, [start, count, materialIndex], u32).
    #[wasm_bindgen(js_name = materialGroups)]
    pub fn material_groups(&self) -> Vec<u32> {
        self.material_groups.clone()
    }

    /// Copy of bone skin indices (vertex_count×4, u32). 4 bones per vertex, 0-padded.
    #[wasm_bindgen(js_name = skinIndices)]
    pub fn skin_indices(&self) -> Vec<u32> {
        self.skin_indices.clone()
    }

    /// Copy of bone skin weights (vertex_count×4, f32). 4 weights per vertex.
    #[wasm_bindgen(js_name = skinWeights)]
    pub fn skin_weights(&self) -> Vec<f32> {
        self.skin_weights.clone()
    }

    /// Copy of per-vertex edge scale (vertex_count×1, f32).
    #[wasm_bindgen(js_name = edgeScale)]
    pub fn edge_scale(&self) -> Vec<f32> {
        self.edge_scale.clone()
    }

    /// Copy of SDEF active flags (vertex_count×1, u8; 1=SDEF, 0=other).
    #[wasm_bindgen(js_name = sdefEnabled)]
    pub fn sdef_enabled(&self) -> Vec<u8> {
        self.sdef_enabled.clone()
    }

    /// Copy of SDEF C vectors (vertex_count×3, XYZ, f32).
    #[wasm_bindgen(js_name = sdefC)]
    pub fn sdef_c(&self) -> Vec<f32> {
        self.sdef_c.clone()
    }

    /// Copy of SDEF R0 vectors (vertex_count×3, XYZ, f32).
    #[wasm_bindgen(js_name = sdefR0)]
    pub fn sdef_r0(&self) -> Vec<f32> {
        self.sdef_r0.clone()
    }

    /// Copy of SDEF R1 vectors (vertex_count×3, XYZ, f32).
    #[wasm_bindgen(js_name = sdefR1)]
    pub fn sdef_r1(&self) -> Vec<f32> {
        self.sdef_r1.clone()
    }

    /// Copy of SDEF Rw0 vectors (vertex_count×3, XYZ, f32). Pre-computed from R0/R1/C/weight.
    #[wasm_bindgen(js_name = sdefRw0)]
    pub fn sdef_rw0(&self) -> Vec<f32> {
        self.sdef_rw0.clone()
    }

    /// Copy of SDEF Rw1 vectors (vertex_count×3, XYZ, f32). Pre-computed from R0/R1/C/weight.
    #[wasm_bindgen(js_name = sdefRw1)]
    pub fn sdef_rw1(&self) -> Vec<f32> {
        self.sdef_rw1.clone()
    }

    /// Copy of QDEF active flags (vertex_count×1, u8; 1=QDEF, 0=other).
    #[wasm_bindgen(js_name = qdefEnabled)]
    pub fn qdef_enabled(&self) -> Vec<u8> {
        self.qdef_enabled.clone()
    }

    /// Copy of derived per-vertex skinning mode names.
    ///
    /// Values match the C ABI `mmd_runtime_parse_pmx_skinning_modes_json`
    /// payload: `bdef1`, `bdef2`, `bdef4`, `sdef`, or `qdef`.
    #[wasm_bindgen(js_name = skinningModes)]
    pub fn skinning_modes(&self) -> Vec<String> {
        self.skinning_modes.clone()
    }
}

fn pmx_skinning_modes(g: &mmd_anim_format::pmx::PmxParsedGeometry) -> Vec<String> {
    let vertex_count = g.positions.len() / 3;
    (0..vertex_count)
        .map(|i| {
            if g.sdef.enabled.get(i).copied().unwrap_or(0.0) > 0.5 {
                "sdef"
            } else if g.qdef.enabled.get(i).copied().unwrap_or(0.0) > 0.5 {
                "qdef"
            } else {
                let w2 = g.skin_weights.get(i * 4 + 2).copied().unwrap_or(0.0);
                let w3 = g.skin_weights.get(i * 4 + 3).copied().unwrap_or(0.0);
                let w1 = g.skin_weights.get(i * 4 + 1).copied().unwrap_or(0.0);
                if w2 != 0.0 || w3 != 0.0 {
                    "bdef4"
                } else if w1 != 0.0 {
                    "bdef2"
                } else {
                    "bdef1"
                }
            }
            .to_owned()
        })
        .collect()
}

/// Parsed PMX handle for the split loader ABI.
///
/// Use this when both non-geometry JSON and geometry typed arrays are needed
/// for the same PMX bytes. The PMX parser runs once; getters return owned
/// copies and the handle can be freed immediately after those copies are made.
#[wasm_bindgen]
pub struct WasmPmxParsedModel {
    parsed: mmd_anim_format::pmx::PmxParsedModel,
}

impl WasmPmxParsedModel {
    fn parse_inner(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("PMX data is empty".to_owned());
        }
        let parsed = mmd_anim_format::parse_pmx_model(data).map_err(|e| e.to_string())?;
        Ok(Self { parsed })
    }
}

#[wasm_bindgen]
impl WasmPmxParsedModel {
    /// Parse PMX bytes once and expose split non-geometry JSON plus geometry DTO getters.
    #[wasm_bindgen(js_name = parse)]
    pub fn parse(data: &[u8]) -> Result<WasmPmxParsedModel, JsValue> {
        Self::parse_inner(data)
            .map_err(|error| js_parser_error("PMX", "parsePmxParsedModel", None, error))
    }

    /// Return JSON with all model data except geometry.
    #[wasm_bindgen(js_name = nonGeometryJson)]
    pub fn non_geometry_json(&self) -> Result<String, JsValue> {
        pmx_model_non_geometry_json_from_parsed(&self.parsed)
            .map_err(|error| js_parser_error("PMX", "nonGeometryJson", None, error))
    }

    /// Return copied geometry typed arrays for this parsed PMX model.
    #[wasm_bindgen(js_name = geometry)]
    pub fn geometry(&self) -> WasmPmxGeometry {
        WasmPmxGeometry::from_geometry(&self.parsed.geometry)
    }
}

fn js_error(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn js_parser_error(
    format: &'static str,
    section: &'static str,
    offset: Option<usize>,
    error: impl std::fmt::Display,
) -> JsValue {
    JsValue::from_str(&parser_error_message(format, section, offset, error))
}

fn parser_error_message(
    format: &'static str,
    section: &'static str,
    offset: Option<usize>,
    error: impl std::fmt::Display,
) -> String {
    let offset = offset
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_owned());
    format!("format={format} section={section} offset={offset}: {error}",)
}

#[wasm_bindgen]
pub struct WasmMmdModel {
    model: Arc<ModelArena>,
    bone_name_to_index: HashMap<Vec<u8>, BoneIndex>,
    morph_name_to_index: HashMap<Vec<u8>, MorphIndex>,
    ik_solver_bone_name_to_index: HashMap<Vec<u8>, usize>,
}

#[wasm_bindgen]
impl WasmMmdModel {
    #[wasm_bindgen(constructor)]
    pub fn new(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices: &[],
            transform_orders: &[],
            ik_solvers_u32: &[],
            ik_solver_limit_angles: &[],
            ik_links_u32: &[],
            ik_link_limits: &[],
            append_u32: &[],
            append_ratios: &[],
            morph_count: 0,
            bone_morph_u32: &[],
            bone_morph_f32: &[],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = withInverseBind)]
    pub fn with_inverse_bind(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
        inverse_bind_matrices: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders: &[],
            ik_solvers_u32: &[],
            ik_solver_limit_angles: &[],
            ik_links_u32: &[],
            ik_link_limits: &[],
            append_u32: &[],
            append_ratios: &[],
            morph_count: 0,
            bone_morph_u32: &[],
            bone_morph_f32: &[],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = withAppend)]
    pub fn with_append(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
        append_u32: &[u32],
        append_ratios: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices: &[],
            transform_orders: &[],
            ik_solvers_u32: &[],
            ik_solver_limit_angles: &[],
            ik_links_u32: &[],
            ik_link_limits: &[],
            append_u32,
            append_ratios,
            morph_count: 0,
            bone_morph_u32: &[],
            bone_morph_f32: &[],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = withAppendAndInverseBind)]
    pub fn with_append_and_inverse_bind(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
        inverse_bind_matrices: &[f32],
        append_u32: &[u32],
        append_ratios: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders: &[],
            ik_solvers_u32: &[],
            ik_solver_limit_angles: &[],
            ik_links_u32: &[],
            ik_link_limits: &[],
            append_u32,
            append_ratios,
            morph_count: 0,
            bone_morph_u32: &[],
            bone_morph_f32: &[],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = withIk)]
    pub fn with_ik(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
        ik_solvers_u32: &[u32],
        ik_solver_limit_angles: &[f32],
        ik_links_u32: &[u32],
        ik_link_limits: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices: &[],
            transform_orders: &[],
            ik_solvers_u32,
            ik_solver_limit_angles,
            ik_links_u32,
            ik_link_limits,
            append_u32: &[],
            append_ratios: &[],
            morph_count: 0,
            bone_morph_u32: &[],
            bone_morph_f32: &[],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = withFull)]
    #[allow(clippy::too_many_arguments)]
    pub fn with_full(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
        inverse_bind_matrices: &[f32],
        ik_solvers_u32: &[u32],
        ik_solver_limit_angles: &[f32],
        ik_links_u32: &[u32],
        ik_link_limits: &[f32],
        append_u32: &[u32],
        append_ratios: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders: &[],
            ik_solvers_u32,
            ik_solver_limit_angles,
            ik_links_u32,
            ik_link_limits,
            append_u32,
            append_ratios,
            morph_count: 0,
            bone_morph_u32: &[],
            bone_morph_f32: &[],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = withFullAndTransformOrder)]
    #[allow(clippy::too_many_arguments)]
    pub fn with_full_and_transform_order(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
        inverse_bind_matrices: &[f32],
        transform_orders: &[i32],
        ik_solvers_u32: &[u32],
        ik_solver_limit_angles: &[f32],
        ik_links_u32: &[u32],
        ik_link_limits: &[f32],
        append_u32: &[u32],
        append_ratios: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders,
            ik_solvers_u32,
            ik_solver_limit_angles,
            ik_links_u32,
            ik_link_limits,
            append_u32,
            append_ratios,
            morph_count: 0,
            bone_morph_u32: &[],
            bone_morph_f32: &[],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = withMorphs)]
    #[allow(clippy::too_many_arguments)]
    pub fn with_morphs(
        parent_indices: &[i32],
        rest_positions_xyz: &[f32],
        inverse_bind_matrices: &[f32],
        transform_orders: &[i32],
        ik_solvers_u32: &[u32],
        ik_solver_limit_angles: &[f32],
        ik_links_u32: &[u32],
        ik_link_limits: &[f32],
        append_u32: &[u32],
        append_ratios: &[f32],
        morph_count: u32,
        bone_morph_u32: &[u32],
        bone_morph_f32: &[f32],
        group_morph_u32: &[u32],
        group_morph_ratios: &[f32],
    ) -> Result<WasmMmdModel, JsValue> {
        let model = build_model(ModelInput {
            parent_indices,
            rest_positions_xyz,
            inverse_bind_matrices,
            transform_orders,
            ik_solvers_u32,
            ik_solver_limit_angles,
            ik_links_u32,
            ik_link_limits,
            append_u32,
            append_ratios,
            morph_count,
            bone_morph_u32,
            bone_morph_f32,
            group_morph_u32,
            group_morph_ratios,
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self {
            model: Arc::new(model),
            bone_name_to_index: HashMap::new(),
            morph_name_to_index: HashMap::new(),
            ik_solver_bone_name_to_index: HashMap::new(),
        })
    }

    #[wasm_bindgen(js_name = boneCount)]
    pub fn bone_count(&self) -> usize {
        self.model.bone_count()
    }

    #[wasm_bindgen(js_name = fromPmxBytes)]
    pub fn from_pmx_bytes(data: &[u8]) -> Result<WasmMmdModel, JsValue> {
        if data.is_empty() {
            return Err(JsValue::from_str("PMX data is empty"));
        }
        let import = mmd_anim_format::import_pmx_runtime(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(Self {
            model: Arc::new(import.model),
            bone_name_to_index: import.bone_name_to_index,
            morph_name_to_index: import.morph_name_to_index,
            ik_solver_bone_name_to_index: import.ik_solver_bone_name_to_index,
        })
    }

    #[wasm_bindgen(js_name = morphCount)]
    pub fn morph_count(&self) -> u32 {
        self.model.morph_count()
    }

    #[wasm_bindgen(js_name = ikCount)]
    pub fn ik_count(&self) -> usize {
        self.model.ik_count()
    }
}

#[wasm_bindgen]
pub struct WasmMmdRuntimeInstance {
    model: Arc<ModelArena>,
    runtime: RuntimeInstance,
    world_matrices_cache: Vec<f32>,
    skinning_matrices_cache: Vec<f32>,
    morph_weights_cache: Vec<f32>,
    ik_enabled_cache: Vec<u8>,
}

impl WasmMmdRuntimeInstance {
    fn refresh_caches(&mut self) {
        let matrices = self.runtime.world_matrices();
        self.world_matrices_cache.clear();
        self.world_matrices_cache.reserve(matrices.len() * 16);
        for m in matrices {
            self.world_matrices_cache
                .extend_from_slice(&m.to_cols_array());
        }

        let skinning = self.runtime.skinning_matrices();
        self.skinning_matrices_cache.clear();
        self.skinning_matrices_cache.reserve(skinning.len() * 16);
        for m in skinning {
            self.skinning_matrices_cache
                .extend_from_slice(&m.to_cols_array());
        }

        let weights = self.runtime.morph_weights();
        self.morph_weights_cache.clear();
        self.morph_weights_cache.extend_from_slice(weights);

        let enabled = self.runtime.ik_enabled();
        self.ik_enabled_cache.clear();
        self.ik_enabled_cache.extend_from_slice(enabled);
    }
}

#[wasm_bindgen]
pub struct WasmMmdRuntimeBatchEvaluation {
    frame_count: usize,
    bone_count: usize,
    morph_count: usize,
    world_matrices: Vec<f32>,
    morph_weights: Vec<f32>,
}

#[wasm_bindgen]
impl WasmMmdRuntimeBatchEvaluation {
    #[wasm_bindgen(js_name = frameCount)]
    pub fn frame_count(&self) -> usize {
        self.frame_count
    }

    #[wasm_bindgen(js_name = boneCount)]
    pub fn bone_count(&self) -> usize {
        self.bone_count
    }

    #[wasm_bindgen(js_name = morphCount)]
    pub fn morph_count(&self) -> usize {
        self.morph_count
    }

    #[wasm_bindgen(js_name = worldMatrixF32Len)]
    pub fn world_matrix_f32_len(&self) -> usize {
        self.world_matrices.len()
    }

    #[wasm_bindgen(js_name = morphWeightF32Len)]
    pub fn morph_weight_f32_len(&self) -> usize {
        self.morph_weights.len()
    }

    #[wasm_bindgen(js_name = worldMatrices)]
    pub fn world_matrices(&self) -> Vec<f32> {
        self.world_matrices.clone()
    }

    #[wasm_bindgen(js_name = morphWeights)]
    pub fn morph_weights(&self) -> Vec<f32> {
        self.morph_weights.clone()
    }

    #[wasm_bindgen(js_name = worldMatricesView)]
    pub fn world_matrices_view(&self) -> js_sys::Float32Array {
        unsafe { js_sys::Float32Array::view(&self.world_matrices) }
    }

    #[wasm_bindgen(js_name = morphWeightsView)]
    pub fn morph_weights_view(&self) -> js_sys::Float32Array {
        unsafe { js_sys::Float32Array::view(&self.morph_weights) }
    }

    #[wasm_bindgen(js_name = copyWorldMatrices)]
    pub fn copy_world_matrices(&self, out: &mut [f32]) -> bool {
        if out.len() < self.world_matrices.len() {
            return false;
        }
        out[..self.world_matrices.len()].copy_from_slice(&self.world_matrices);
        true
    }

    #[wasm_bindgen(js_name = copyMorphWeights)]
    pub fn copy_morph_weights(&self, out: &mut [f32]) -> bool {
        if out.len() < self.morph_weights.len() {
            return false;
        }
        out[..self.morph_weights.len()].copy_from_slice(&self.morph_weights);
        true
    }
}

#[wasm_bindgen]
impl WasmMmdRuntimeInstance {
    #[wasm_bindgen(constructor)]
    pub fn new(model: &WasmMmdModel, morph_count: usize) -> WasmMmdRuntimeInstance {
        let model_arena = Arc::clone(&model.model);
        let mut instance = Self {
            model: Arc::clone(&model_arena),
            runtime: RuntimeInstance::new_with_morph_count(model_arena, morph_count),
            world_matrices_cache: Vec::new(),
            skinning_matrices_cache: Vec::new(),
            morph_weights_cache: Vec::new(),
            ik_enabled_cache: Vec::new(),
        };
        instance.refresh_caches();
        instance
    }

    #[wasm_bindgen(js_name = withCounts)]
    pub fn with_counts(
        model: &WasmMmdModel,
        morph_count: usize,
        ik_count: usize,
    ) -> WasmMmdRuntimeInstance {
        let model_arena = Arc::clone(&model.model);
        let mut instance = Self {
            model: Arc::clone(&model_arena),
            runtime: RuntimeInstance::new_with_counts(model_arena, morph_count, ik_count),
            world_matrices_cache: Vec::new(),
            skinning_matrices_cache: Vec::new(),
            morph_weights_cache: Vec::new(),
            ik_enabled_cache: Vec::new(),
        };
        instance.refresh_caches();
        instance
    }

    #[wasm_bindgen(js_name = forModel)]
    pub fn for_model(model: &WasmMmdModel) -> WasmMmdRuntimeInstance {
        let model_arena = Arc::clone(&model.model);
        let mut instance = Self {
            model: Arc::clone(&model_arena),
            runtime: RuntimeInstance::new(model_arena),
            world_matrices_cache: Vec::new(),
            skinning_matrices_cache: Vec::new(),
            morph_weights_cache: Vec::new(),
            ik_enabled_cache: Vec::new(),
        };
        instance.refresh_caches();
        instance
    }

    #[wasm_bindgen(js_name = evaluateRestPose)]
    pub fn evaluate_rest_pose(&mut self) {
        self.runtime.evaluate_rest_pose();
        self.refresh_caches();
    }

    #[wasm_bindgen(js_name = evaluateClipFrame)]
    pub fn evaluate_clip_frame(&mut self, clip: &WasmMmdClip, frame: f32) {
        self.runtime.evaluate_clip_frame(&clip.clip, frame);
        self.refresh_caches();
    }

    #[wasm_bindgen(js_name = evaluateClipFrameWithIkOptions)]
    pub fn evaluate_clip_frame_with_ik_options(
        &mut self,
        clip: &WasmMmdClip,
        frame: f32,
        ik_tolerance: f32,
        ik_max_iterations_cap: u32,
    ) -> Result<(), JsValue> {
        if !ik_tolerance.is_finite() || ik_tolerance < 0.0 {
            return Err(JsValue::from_str(
                "ikTolerance must be non-negative and finite",
            ));
        }
        self.runtime.evaluate_clip_frame_with_ik_options(
            &clip.clip,
            frame,
            IkSolveOptions {
                tolerance: ik_tolerance,
                max_iterations_cap: if ik_max_iterations_cap == 0 {
                    None
                } else {
                    Some(ik_max_iterations_cap)
                },
            },
        );
        self.refresh_caches();
        Ok(())
    }

    #[wasm_bindgen(js_name = clipFrameBatchWorldMatrixF32Len)]
    pub fn clip_frame_batch_world_matrix_f32_len(&self, frame_count: usize) -> usize {
        self.runtime
            .world_matrices()
            .len()
            .checked_mul(16)
            .and_then(|frame_len| frame_len.checked_mul(frame_count))
            .unwrap_or(0)
    }

    #[wasm_bindgen(js_name = clipFrameBatchMorphWeightF32Len)]
    pub fn clip_frame_batch_morph_weight_f32_len(&self, frame_count: usize) -> usize {
        self.runtime
            .morph_weights()
            .len()
            .checked_mul(frame_count)
            .unwrap_or(0)
    }

    #[wasm_bindgen(js_name = evaluateClipFrameBatch)]
    pub fn evaluate_clip_frame_batch(
        &self,
        clip: &WasmMmdClip,
        start_frame: f32,
        frame_step: f32,
        frame_count: usize,
        worker_count: u32,
    ) -> Result<WasmMmdRuntimeBatchEvaluation, JsValue> {
        if !start_frame.is_finite() || !frame_step.is_finite() {
            return Err(JsValue::from_str("startFrame and frameStep must be finite"));
        }

        let bone_count = self.runtime.world_matrices().len();
        let morph_count = self.runtime.morph_weights().len();
        let world_len = bone_count
            .checked_mul(16)
            .and_then(|frame_len| frame_len.checked_mul(frame_count))
            .ok_or_else(|| JsValue::from_str("batch world matrix output length overflow"))?;
        let morph_len = morph_count
            .checked_mul(frame_count)
            .ok_or_else(|| JsValue::from_str("batch morph weight output length overflow"))?;

        let mut world_matrices = Vec::with_capacity(world_len);
        let mut morph_weights = Vec::with_capacity(morph_len);
        let morph_state_count = morph_count;
        let ik_state_count = self.runtime.ik_enabled().len();
        let mut runtime = RuntimeInstance::new_with_counts(
            Arc::clone(&self.model),
            morph_state_count,
            ik_state_count,
        );

        // worker_count is accepted for C ABI parity. Wasm threads require a
        // separate build/runtime contract, so this surface currently runs the
        // batch in one worker and keeps the output layout stable.
        let _ = worker_count;
        for frame_index in 0..frame_count {
            let frame = start_frame + frame_step * frame_index as f32;
            runtime.evaluate_clip_frame(&clip.clip, frame);
            extend_matrices(&mut world_matrices, runtime.world_matrices());
            morph_weights.extend_from_slice(runtime.morph_weights());
        }

        Ok(WasmMmdRuntimeBatchEvaluation {
            frame_count,
            bone_count,
            morph_count,
            world_matrices,
            morph_weights,
        })
    }

    #[wasm_bindgen(js_name = worldMatrixF32Len)]
    pub fn world_matrix_f32_len(&self) -> usize {
        self.runtime.world_matrices().len() * 16
    }

    #[wasm_bindgen(js_name = worldMatrices)]
    pub fn world_matrices(&self) -> Vec<f32> {
        copy_matrices(self.runtime.world_matrices())
    }

    #[wasm_bindgen(js_name = skinningMatrixF32Len)]
    pub fn skinning_matrix_f32_len(&self) -> usize {
        self.runtime.skinning_matrices().len() * 16
    }

    #[wasm_bindgen(js_name = skinningMatrices)]
    pub fn skinning_matrices(&self) -> Vec<f32> {
        copy_matrices(self.runtime.skinning_matrices())
    }

    #[wasm_bindgen(js_name = morphWeightLen)]
    pub fn morph_weight_len(&self) -> usize {
        self.runtime.morph_weights().len()
    }

    #[wasm_bindgen(js_name = morphWeights)]
    pub fn morph_weights(&self) -> Vec<f32> {
        self.runtime.morph_weights().to_vec()
    }

    #[wasm_bindgen(js_name = ikEnabledLen)]
    pub fn ik_enabled_len(&self) -> usize {
        self.runtime.ik_enabled().len()
    }

    #[wasm_bindgen(js_name = ikEnabled)]
    pub fn ik_enabled(&self) -> Vec<u8> {
        self.runtime.ik_enabled().to_vec()
    }

    /// Direct typed-array view over the internal world-matrices cache.
    ///
    /// **Caution**: The returned `Float32Array` is invalidated by the next
    /// evaluation call (`evaluateRestPose` / `evaluateClipFrame`) and may be
    /// invalidated by Wasm memory growth. Callers that need persistent buffers
    /// should use `worldMatrices()` (copy) or `copyWorldMatrices()` instead.
    #[wasm_bindgen(js_name = worldMatricesView)]
    pub fn world_matrices_view(&self) -> js_sys::Float32Array {
        unsafe { js_sys::Float32Array::view(&self.world_matrices_cache) }
    }

    /// Direct typed-array view over the internal skinning-matrices cache.
    /// Subject to the same invalidation contract as `worldMatricesView`.
    #[wasm_bindgen(js_name = skinningMatricesView)]
    pub fn skinning_matrices_view(&self) -> js_sys::Float32Array {
        unsafe { js_sys::Float32Array::view(&self.skinning_matrices_cache) }
    }

    /// Direct typed-array view over the internal morph-weights cache.
    /// Subject to the same invalidation contract as `worldMatricesView`.
    #[wasm_bindgen(js_name = morphWeightsView)]
    pub fn morph_weights_view(&self) -> js_sys::Float32Array {
        unsafe { js_sys::Float32Array::view(&self.morph_weights_cache) }
    }

    /// Direct typed-array view over the internal IK-enabled cache.
    /// Subject to the same invalidation contract as `worldMatricesView`.
    #[wasm_bindgen(js_name = ikEnabledView)]
    pub fn ik_enabled_view(&self) -> js_sys::Uint8Array {
        unsafe { js_sys::Uint8Array::view(&self.ik_enabled_cache) }
    }

    #[wasm_bindgen(js_name = copyWorldMatrices)]
    pub fn copy_world_matrices(&self, out: &mut [f32]) -> bool {
        try_copy_matrices(self.runtime.world_matrices(), out)
    }

    #[wasm_bindgen(js_name = copySkinningMatrices)]
    pub fn copy_skinning_matrices(&self, out: &mut [f32]) -> bool {
        try_copy_matrices(self.runtime.skinning_matrices(), out)
    }

    #[wasm_bindgen(js_name = copyMorphWeights)]
    pub fn copy_morph_weights(&self, out: &mut [f32]) -> bool {
        let weights = self.runtime.morph_weights();
        if out.len() < weights.len() {
            return false;
        }
        out[..weights.len()].copy_from_slice(weights);
        true
    }

    #[wasm_bindgen(js_name = copyIkEnabled)]
    pub fn copy_ik_enabled(&self, out: &mut [u8]) -> bool {
        let enabled = self.runtime.ik_enabled();
        if out.len() < enabled.len() {
            return false;
        }
        out[..enabled.len()].copy_from_slice(enabled);
        true
    }
}

#[wasm_bindgen]
pub struct WasmMmdClip {
    clip: AnimationClip,
}

#[wasm_bindgen]
pub struct WasmVmdCameraTrack {
    frames: Vec<mmd_anim_format::vmd::VmdParsedCameraFrame>,
}

#[wasm_bindgen]
impl WasmVmdCameraTrack {
    #[wasm_bindgen(js_name = fromVmdBytes)]
    pub fn from_vmd_bytes(data: &[u8]) -> Result<WasmVmdCameraTrack, JsValue> {
        if data.is_empty() {
            return Err(JsValue::from_str("VMD data is empty"));
        }
        let parsed = mmd_anim_format::parse_vmd_animation(data).map_err(|error| {
            js_parser_error("VMD", "WasmVmdCameraTrack.fromVmdBytes", None, error)
        })?;
        if parsed.camera_frames.is_empty() {
            return Err(JsValue::from_str("VMD has no camera keyframes"));
        }
        Ok(Self {
            frames: parsed.camera_frames,
        })
    }

    #[wasm_bindgen(js_name = frameCount)]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Sample the camera track into a caller-owned `Float32Array`.
    ///
    /// Writes `[distance, position.x, position.y, position.z, rotation.x,
    /// rotation.y, rotation.z, fov, perspective]` to `out`.
    /// `perspective` is encoded as `1.0` when enabled, otherwise `0.0`.
    /// Returns `false` when `out.length < 9`.
    #[wasm_bindgen(js_name = sample)]
    pub fn sample(&self, frame: f32, out: &js_sys::Float32Array) -> Result<bool, JsValue> {
        if !frame.is_finite() {
            return Err(JsValue::from_str("frame must be finite"));
        }
        let camera = mmd_anim_format::sample_vmd_camera_frames(&self.frames, frame)
            .ok_or_else(|| JsValue::from_str("VMD has no camera keyframes"))?;
        copy_camera_state_array(camera, out)
    }
}

#[wasm_bindgen]
pub struct WasmVmdLightTrack {
    frames: Vec<mmd_anim_format::vmd::VmdParsedLightFrame>,
}

#[wasm_bindgen]
impl WasmVmdLightTrack {
    #[wasm_bindgen(js_name = fromVmdBytes)]
    pub fn from_vmd_bytes(data: &[u8]) -> Result<WasmVmdLightTrack, JsValue> {
        if data.is_empty() {
            return Err(JsValue::from_str("VMD data is empty"));
        }
        let parsed = mmd_anim_format::parse_vmd_animation(data).map_err(|error| {
            js_parser_error("VMD", "WasmVmdLightTrack.fromVmdBytes", None, error)
        })?;
        if parsed.light_frames.is_empty() {
            return Err(JsValue::from_str("VMD has no light keyframes"));
        }
        Ok(Self {
            frames: parsed.light_frames,
        })
    }

    #[wasm_bindgen(js_name = frameCount)]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Sample the light track into a caller-owned `Float32Array`.
    ///
    /// Writes `[color.r, color.g, color.b, direction.x, direction.y,
    /// direction.z]` to `out`. Returns `false` when `out.length < 6`.
    #[wasm_bindgen(js_name = sample)]
    pub fn sample(&self, frame: f32, out: &js_sys::Float32Array) -> Result<bool, JsValue> {
        if !frame.is_finite() {
            return Err(JsValue::from_str("frame must be finite"));
        }
        let light = mmd_anim_format::sample_vmd_light_frames(&self.frames, frame)
            .ok_or_else(|| JsValue::from_str("VMD has no light keyframes"))?;
        copy_light_state_array(light, out)
    }
}

#[wasm_bindgen]
pub struct WasmVmdSelfShadowTrack {
    frames: Vec<mmd_anim_format::vmd::VmdParsedSelfShadowFrame>,
}

#[wasm_bindgen]
impl WasmVmdSelfShadowTrack {
    #[wasm_bindgen(js_name = fromVmdBytes)]
    pub fn from_vmd_bytes(data: &[u8]) -> Result<WasmVmdSelfShadowTrack, JsValue> {
        if data.is_empty() {
            return Err(JsValue::from_str("VMD data is empty"));
        }
        let parsed = mmd_anim_format::parse_vmd_animation(data).map_err(|error| {
            js_parser_error("VMD", "WasmVmdSelfShadowTrack.fromVmdBytes", None, error)
        })?;
        if parsed.self_shadow_frames.is_empty() {
            return Err(JsValue::from_str("VMD has no self-shadow keyframes"));
        }
        Ok(Self {
            frames: parsed.self_shadow_frames,
        })
    }

    #[wasm_bindgen(js_name = frameCount)]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Sample the self-shadow track into a caller-owned `Float32Array`.
    ///
    /// Writes `[mode, distance]` to `out`. `mode` is encoded as a float.
    /// Returns `false` when `out.length < 2`.
    #[wasm_bindgen(js_name = sample)]
    pub fn sample(&self, frame: f32, out: &js_sys::Float32Array) -> Result<bool, JsValue> {
        if !frame.is_finite() {
            return Err(JsValue::from_str("frame must be finite"));
        }
        let self_shadow = mmd_anim_format::sample_vmd_self_shadow_frames(&self.frames, frame)
            .ok_or_else(|| JsValue::from_str("VMD has no self-shadow keyframes"))?;
        copy_self_shadow_state_array(self_shadow, out)
    }
}

fn camera_state_array(camera: mmd_anim_format::VmdCameraState) -> [f32; 9] {
    [
        camera.distance,
        camera.position[0],
        camera.position[1],
        camera.position[2],
        camera.rotation[0],
        camera.rotation[1],
        camera.rotation[2],
        camera.fov,
        if camera.perspective { 1.0 } else { 0.0 },
    ]
}

fn copy_camera_state_array(
    camera: mmd_anim_format::VmdCameraState,
    out: &js_sys::Float32Array,
) -> Result<bool, JsValue> {
    if out.length() < 9 {
        return Ok(false);
    }
    let values = camera_state_array(camera);
    for (index, value) in values.into_iter().enumerate() {
        out.set_index(index as u32, value);
    }
    Ok(true)
}

fn light_state_array(light: mmd_anim_format::VmdLightState) -> [f32; 6] {
    [
        light.color[0],
        light.color[1],
        light.color[2],
        light.direction[0],
        light.direction[1],
        light.direction[2],
    ]
}

fn copy_light_state_array(
    light: mmd_anim_format::VmdLightState,
    out: &js_sys::Float32Array,
) -> Result<bool, JsValue> {
    if out.length() < 6 {
        return Ok(false);
    }
    let values = light_state_array(light);
    for (index, value) in values.into_iter().enumerate() {
        out.set_index(index as u32, value);
    }
    Ok(true)
}

fn self_shadow_state_array(self_shadow: mmd_anim_format::VmdSelfShadowState) -> [f32; 2] {
    [self_shadow.mode as f32, self_shadow.distance]
}

fn copy_self_shadow_state_array(
    self_shadow: mmd_anim_format::VmdSelfShadowState,
    out: &js_sys::Float32Array,
) -> Result<bool, JsValue> {
    if out.length() < 2 {
        return Ok(false);
    }
    let values = self_shadow_state_array(self_shadow);
    for (index, value) in values.into_iter().enumerate() {
        out.set_index(index as u32, value);
    }
    Ok(true)
}

#[wasm_bindgen]
impl WasmMmdClip {
    #[wasm_bindgen(constructor)]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bone_tracks_u32: &[u32],
        bone_keyframe_frames: &[u32],
        bone_keyframe_values: &[f32],
        morph_tracks_u32: &[u32],
        morph_keyframe_frames: &[u32],
        morph_keyframe_weights: &[f32],
        property_frames: &[u32],
        property_ik_enabled: &[u8],
        property_ik_count: usize,
    ) -> Result<WasmMmdClip, JsValue> {
        let clip = build_clip(WasmClipInput {
            bone_tracks_u32,
            bone_keyframe_frames,
            bone_keyframe_values,
            morph_tracks_u32,
            morph_keyframe_frames,
            morph_keyframe_weights,
            property_frames,
            property_ik_enabled,
            property_ik_count,
        })
        .map_err(|error| JsValue::from_str(&error))?;
        Ok(Self { clip })
    }

    #[wasm_bindgen(js_name = fromVmdBytesForModel)]
    pub fn from_vmd_bytes_for_model(
        model: &WasmMmdModel,
        data: &[u8],
    ) -> Result<WasmMmdClip, JsValue> {
        if data.is_empty() {
            return Err(JsValue::from_str("VMD data is empty"));
        }
        if model.bone_name_to_index.is_empty() && model.morph_name_to_index.is_empty() {
            return Err(JsValue::from_str(
                "model was not imported from PMX bytes (no name maps)",
            ));
        }
        let motion = mmd_anim_format::import_vmd_motion(data)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        let solver_count = model.model.ik_count();
        let clip = mmd_anim_format::build_pair_clip(
            &motion,
            &model.bone_name_to_index,
            &model.morph_name_to_index,
            &model.ik_solver_bone_name_to_index,
            solver_count,
        );
        Ok(Self { clip })
    }

    #[wasm_bindgen(js_name = hasFrames)]
    pub fn has_frames(&self) -> bool {
        self.clip.frame_range().is_some()
    }

    #[wasm_bindgen(js_name = firstFrame)]
    pub fn first_frame(&self) -> u32 {
        self.clip
            .frame_range()
            .map(|(first, _last)| first)
            .unwrap_or(0)
    }

    #[wasm_bindgen(js_name = lastFrame)]
    pub fn last_frame(&self) -> u32 {
        self.clip
            .frame_range()
            .map(|(_first, last)| last)
            .unwrap_or(0)
    }
}

struct ModelInput<'a> {
    parent_indices: &'a [i32],
    rest_positions_xyz: &'a [f32],
    inverse_bind_matrices: &'a [f32],
    transform_orders: &'a [i32],
    ik_solvers_u32: &'a [u32],
    ik_solver_limit_angles: &'a [f32],
    ik_links_u32: &'a [u32],
    ik_link_limits: &'a [f32],
    append_u32: &'a [u32],
    append_ratios: &'a [f32],
    morph_count: u32,
    bone_morph_u32: &'a [u32],
    bone_morph_f32: &'a [f32],
    group_morph_u32: &'a [u32],
    group_morph_ratios: &'a [f32],
}

fn build_model(input: ModelInput<'_>) -> Result<ModelArena, String> {
    if input.parent_indices.is_empty() {
        return Err("model must contain at least one bone".to_owned());
    }
    if input.rest_positions_xyz.len() != input.parent_indices.len() * 3 {
        return Err("rest_positions_xyz must contain bone_count * 3 values".to_owned());
    }
    if !input.inverse_bind_matrices.is_empty()
        && input.inverse_bind_matrices.len() != input.parent_indices.len() * 16
    {
        return Err("inverse_bind_matrices must contain bone_count * 16 values".to_owned());
    }
    if !input.transform_orders.is_empty()
        && input.transform_orders.len() != input.parent_indices.len()
    {
        return Err("transform_orders must contain bone_count values".to_owned());
    }
    if !input.ik_solvers_u32.len().is_multiple_of(5) {
        return Err(
            "ik_solvers_u32 must contain ik/target/linkOffset/linkCount/iteration quintets"
                .to_owned(),
        );
    }
    if input.ik_solver_limit_angles.len() != input.ik_solvers_u32.len() / 5 {
        return Err("ik_solver_limit_angles length must match IK solver count".to_owned());
    }
    if !input.ik_links_u32.len().is_multiple_of(2) {
        return Err("ik_links_u32 must contain bone/flags pairs".to_owned());
    }
    if !input.ik_links_u32.is_empty()
        && input.ik_link_limits.len() != input.ik_links_u32.len() / 2 * 6
    {
        return Err("ik_link_limits must contain ik_link_count * 6 values".to_owned());
    }
    if !input.append_u32.len().is_multiple_of(3) {
        return Err("append_u32 must contain target/source/flags triplets".to_owned());
    }
    if input.append_ratios.len() != input.append_u32.len() / 3 {
        return Err("append_ratios length must match append transform count".to_owned());
    }

    let mut bones = Vec::with_capacity(input.parent_indices.len());
    for (bone_index, parent_index) in input.parent_indices.iter().enumerate() {
        let parent = match *parent_index {
            -1 => None,
            parent if parent >= 0 => Some(BoneIndex(parent as u32)),
            _ => return Err("parent index must be -1 or non-negative".to_owned()),
        };
        let position_offset = bone_index * 3;
        let mut bone = BoneInit::new(
            parent,
            glam::Vec3A::new(
                input.rest_positions_xyz[position_offset],
                input.rest_positions_xyz[position_offset + 1],
                input.rest_positions_xyz[position_offset + 2],
            ),
        );
        if !input.inverse_bind_matrices.is_empty() {
            let inverse_bind_offset = bone_index * 16;
            let inverse_bind_matrix = input.inverse_bind_matrices
                [inverse_bind_offset..inverse_bind_offset + 16]
                .try_into()
                .map_err(|_| "inverse bind matrix slice conversion failed".to_owned())?;
            bone.inverse_bind_matrix = glam::Mat4::from_cols_array(inverse_bind_matrix);
        }
        if !input.transform_orders.is_empty() {
            bone.transform_order = input.transform_orders[bone_index];
        }
        bones.push(bone);
    }

    let ik_links = input
        .ik_links_u32
        .chunks_exact(2)
        .enumerate()
        .map(|(link_index, link)| {
            let mut init = IkLinkInit::new(BoneIndex(link[0]));
            if link[1] & IK_LINK_FLAG_ANGLE_LIMIT != 0 {
                let limit_offset = link_index * 6;
                init = init.with_angle_limit(IkAngleLimit::new(
                    glam::Vec3A::new(
                        input.ik_link_limits[limit_offset],
                        input.ik_link_limits[limit_offset + 1],
                        input.ik_link_limits[limit_offset + 2],
                    ),
                    glam::Vec3A::new(
                        input.ik_link_limits[limit_offset + 3],
                        input.ik_link_limits[limit_offset + 4],
                        input.ik_link_limits[limit_offset + 5],
                    ),
                ));
            }
            init
        })
        .collect::<Vec<_>>();

    let ik_solvers = input
        .ik_solvers_u32
        .chunks_exact(5)
        .zip(input.ik_solver_limit_angles.iter())
        .map(|(solver, limit_angle)| {
            let link_offset = solver[2] as usize;
            let link_count = solver[3] as usize;
            let links = checked_range(&ik_links, link_offset, link_count)?.to_vec();
            Ok(IkSolverInit {
                ik_bone: BoneIndex(solver[0]),
                target_bone: BoneIndex(solver[1]),
                links,
                iteration_count: solver[4],
                limit_angle: *limit_angle,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let append_transforms = input
        .append_u32
        .chunks_exact(3)
        .zip(input.append_ratios.iter())
        .map(|(append, ratio)| {
            let mut init =
                AppendTransformInit::new(BoneIndex(append[0]), BoneIndex(append[1]), *ratio);
            let flags = append[2];
            if flags & APPEND_FLAG_ROTATION != 0 {
                init = init.with_rotation();
            }
            if flags & APPEND_FLAG_TRANSLATION != 0 {
                init = init.with_translation();
            }
            if flags & APPEND_FLAG_LOCAL != 0 {
                init = init.with_local();
            }
            init
        })
        .collect::<Vec<_>>();

    let morph = build_morph_init_from_wasm(&input)?;
    ModelArena::new_with_morphs(bones, ik_solvers, append_transforms, morph)
        .map_err(|error| error.to_string())
}

fn build_morph_init_from_wasm(input: &ModelInput<'_>) -> Result<MorphInit, String> {
    if input.morph_count == 0 {
        if !input.bone_morph_u32.is_empty()
            || !input.bone_morph_f32.is_empty()
            || !input.group_morph_u32.is_empty()
            || !input.group_morph_ratios.is_empty()
        {
            return Err("morph_count must be non-zero when morph data is provided".to_owned());
        }
        return Ok(MorphInit::default());
    }
    let mc = input.morph_count as usize;

    if !input.bone_morph_u32.len().is_multiple_of(2) {
        return Err("bone_morph_u32 must contain morphIndex/targetBone pairs".to_owned());
    }
    if input.bone_morph_f32.len() != input.bone_morph_u32.len() / 2 * 7 {
        return Err("bone_morph_f32 must contain bone_morph_count * 7 values".to_owned());
    }
    if !input.group_morph_u32.len().is_multiple_of(2) {
        return Err("group_morph_u32 must contain morphIndex/childMorph pairs".to_owned());
    }
    if input.group_morph_ratios.len() != input.group_morph_u32.len() / 2 {
        return Err("group_morph_ratios length must match group morph count".to_owned());
    }

    let (bone_offsets, bone_spans) = if input.bone_morph_u32.is_empty() {
        (Vec::new(), vec![MorphOffsetSpan::default(); mc])
    } else {
        let mut entries: Vec<(u32, u32, usize)> = input
            .bone_morph_u32
            .chunks_exact(2)
            .enumerate()
            .map(|(i, pair)| (pair[0], pair[1], i))
            .collect();
        entries.sort_by_key(|a| a.0);
        if entries.last().unwrap().0 as usize >= mc {
            return Err("bone_morph_u32 contains morph_index >= morph_count".to_owned());
        }
        let mut offsets = Vec::with_capacity(entries.len());
        let mut spans = vec![MorphOffsetSpan::default(); mc];
        let mut i = 0;
        while i < entries.len() {
            let morph = entries[i].0 as usize;
            let start = offsets.len() as u32;
            let mut count = 0u32;
            while i < entries.len() && entries[i].0 as usize == morph {
                let (_, target_bone, entry_idx) = entries[i];
                let f32_offset = entry_idx * 7;
                offsets.push(BoneMorphOffset {
                    target_bone: BoneIndex(target_bone),
                    position_offset: glam::Vec3A::new(
                        input.bone_morph_f32[f32_offset],
                        input.bone_morph_f32[f32_offset + 1],
                        input.bone_morph_f32[f32_offset + 2],
                    ),
                    rotation_offset: glam::Quat::from_xyzw(
                        input.bone_morph_f32[f32_offset + 3],
                        input.bone_morph_f32[f32_offset + 4],
                        input.bone_morph_f32[f32_offset + 5],
                        input.bone_morph_f32[f32_offset + 6],
                    ),
                });
                count += 1;
                i += 1;
            }
            spans[morph] = MorphOffsetSpan { start, count };
        }
        (offsets, spans)
    };

    let (group_offsets, group_spans) = if input.group_morph_u32.is_empty() {
        (Vec::new(), vec![MorphOffsetSpan::default(); mc])
    } else {
        let mut entries: Vec<(u32, u32, usize)> = input
            .group_morph_u32
            .chunks_exact(2)
            .enumerate()
            .map(|(i, pair)| (pair[0], pair[1], i))
            .collect();
        entries.sort_by_key(|a| a.0);
        if entries.last().unwrap().0 as usize >= mc {
            return Err("group_morph_u32 contains morph_index >= morph_count".to_owned());
        }
        let mut offsets = Vec::with_capacity(entries.len());
        let mut spans = vec![MorphOffsetSpan::default(); mc];
        let mut i = 0;
        while i < entries.len() {
            let morph = entries[i].0 as usize;
            let start = offsets.len() as u32;
            let mut count = 0u32;
            while i < entries.len() && entries[i].0 as usize == morph {
                let (_, child_morph, orig_idx) = entries[i];
                offsets.push(GroupMorphOffset {
                    child_morph: MorphIndex(child_morph),
                    ratio: input.group_morph_ratios[orig_idx],
                });
                count += 1;
                i += 1;
            }
            spans[morph] = MorphOffsetSpan { start, count };
        }
        (offsets, spans)
    };

    Ok(MorphInit {
        morph_count: input.morph_count,
        bone_offsets,
        bone_spans,
        group_offsets,
        group_spans,
        ..MorphInit::default()
    })
}

fn copy_matrices(matrices: &[glam::Mat4]) -> Vec<f32> {
    let mut out = Vec::with_capacity(matrices.len() * 16);
    extend_matrices(&mut out, matrices);
    out
}

fn extend_matrices(out: &mut Vec<f32>, matrices: &[glam::Mat4]) {
    for matrix in matrices {
        out.extend_from_slice(&matrix.to_cols_array());
    }
}

fn try_copy_matrices(matrices: &[glam::Mat4], out: &mut [f32]) -> bool {
    let required = matrices.len() * 16;
    if out.len() < required {
        return false;
    }
    for (i, matrix) in matrices.iter().enumerate() {
        let offset = i * 16;
        out[offset..offset + 16].copy_from_slice(&matrix.to_cols_array());
    }
    true
}

struct WasmClipInput<'a> {
    bone_tracks_u32: &'a [u32],
    bone_keyframe_frames: &'a [u32],
    bone_keyframe_values: &'a [f32],
    morph_tracks_u32: &'a [u32],
    morph_keyframe_frames: &'a [u32],
    morph_keyframe_weights: &'a [f32],
    property_frames: &'a [u32],
    property_ik_enabled: &'a [u8],
    property_ik_count: usize,
}

fn build_clip(input: WasmClipInput<'_>) -> Result<AnimationClip, String> {
    if !input.bone_tracks_u32.len().is_multiple_of(3) {
        return Err("bone_tracks_u32 must contain triplets".to_owned());
    }
    if input.bone_keyframe_values.len() != input.bone_keyframe_frames.len() * 7 {
        return Err("bone keyframe values must contain frame_count * 7 values".to_owned());
    }
    if !input.morph_tracks_u32.len().is_multiple_of(3) {
        return Err("morph_tracks_u32 must contain triplets".to_owned());
    }
    if input.morph_keyframe_weights.len() != input.morph_keyframe_frames.len() {
        return Err("morph keyframe frames and weights must have the same length".to_owned());
    }
    if input.property_ik_count == 0 && !input.property_frames.is_empty() {
        return Err("property_ik_count must be non-zero when property frames exist".to_owned());
    }
    if input.property_ik_enabled.len() != input.property_frames.len() * input.property_ik_count {
        return Err(
            "property IK states must contain property_frame_count * property_ik_count values"
                .to_owned(),
        );
    }

    let mut bone_bindings = Vec::with_capacity(input.bone_tracks_u32.len() / 3);
    for track in input.bone_tracks_u32.chunks_exact(3) {
        let bone_index = track[0];
        let keyframe_offset = track[1] as usize;
        let keyframe_count = track[2] as usize;
        let frames = checked_range(input.bone_keyframe_frames, keyframe_offset, keyframe_count)?;
        let values_offset = keyframe_offset
            .checked_mul(7)
            .ok_or_else(|| "bone keyframe value offset overflow".to_owned())?;
        let values = checked_range(
            input.bone_keyframe_values,
            values_offset,
            keyframe_count * 7,
        )?;
        let mut keyframes = Vec::with_capacity(keyframe_count);
        for (keyframe_index, frame) in frames.iter().enumerate() {
            let offset = keyframe_index * 7;
            keyframes.push(MovableBoneKeyframe::new(
                *frame,
                glam::Vec3A::new(values[offset], values[offset + 1], values[offset + 2]),
                glam::Quat::from_xyzw(
                    values[offset + 3],
                    values[offset + 4],
                    values[offset + 5],
                    values[offset + 6],
                ),
            ));
        }
        bone_bindings.push(BoneAnimationBinding {
            bone: BoneIndex(bone_index),
            track: MovableBoneTrack::from_keyframes(keyframes),
        });
    }

    let mut morph_bindings = Vec::with_capacity(input.morph_tracks_u32.len() / 3);
    for track in input.morph_tracks_u32.chunks_exact(3) {
        let morph_index = track[0];
        let keyframe_offset = track[1] as usize;
        let keyframe_count = track[2] as usize;
        let frames = checked_range(input.morph_keyframe_frames, keyframe_offset, keyframe_count)?;
        let weights = checked_range(
            input.morph_keyframe_weights,
            keyframe_offset,
            keyframe_count,
        )?;
        let keyframes = frames
            .iter()
            .zip(weights.iter())
            .map(|(frame, weight)| MorphKeyframe::new(*frame, *weight))
            .collect::<Vec<_>>();
        morph_bindings.push(MorphAnimationBinding {
            morph: MorphIndex(morph_index),
            track: MorphTrack::from_keyframes(keyframes),
        });
    }

    let property_track = if input.property_frames.is_empty() {
        None
    } else {
        let keyframes = input
            .property_frames
            .iter()
            .enumerate()
            .map(|(frame_index, frame)| {
                let offset = frame_index * input.property_ik_count;
                let states = input.property_ik_enabled[offset..offset + input.property_ik_count]
                    .iter()
                    .map(|state| *state != 0)
                    .collect::<Vec<_>>();
                PropertyKeyframe::new(*frame, states)
            })
            .collect::<Vec<_>>();
        Some(PropertyAnimationBinding::from_keyframes(keyframes))
    };

    Ok(AnimationClip::new_full(
        bone_bindings,
        morph_bindings,
        property_track,
    ))
}

fn checked_range<T>(slice: &[T], offset: usize, count: usize) -> Result<&[T], String> {
    let end = offset
        .checked_add(count)
        .ok_or_else(|| "range overflow".to_owned())?;
    slice
        .get(offset..end)
        .ok_or_else(|| "track keyframe range is out of bounds".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_version_matches_current_breaking_surface() {
        assert_eq!(WASM_WRAPPER_VERSION, 2);
        assert_eq!(wasm_wrapper_version(), WASM_WRAPPER_VERSION);
    }

    #[test]
    fn evaluates_rest_pose_through_wasm_wrapper() {
        let model = WasmMmdModel::new(&[-1, 0], &[1.0, 0.0, 0.0, 0.0, 2.0, 0.0]).unwrap();
        assert_eq!(model.bone_count(), 2);

        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);
        runtime.evaluate_rest_pose();

        assert_eq!(runtime.world_matrix_f32_len(), 32);
        let matrices = runtime.world_matrices();
        assert_eq!(matrices[12], 1.0);
        assert_eq!(matrices[16 + 12], 1.0);
        assert_eq!(matrices[16 + 13], 2.0);

        assert_eq!(runtime.skinning_matrix_f32_len(), 32);
        assert_eq!(runtime.skinning_matrices(), matrices);
    }

    #[test]
    fn parser_error_message_includes_format_section_and_offset() {
        let message = parser_error_message(
            "VMD",
            "parseMmdFormatJson",
            Some(30),
            "unexpected end of data",
        );

        assert_eq!(
            message,
            "format=VMD section=parseMmdFormatJson offset=30: unexpected end of data"
        );
    }

    #[test]
    fn parser_error_message_marks_unknown_offset() {
        let message =
            parser_error_message("PMX", "parsePmxModelJson", None, "invalid PMX magic bytes");

        assert_eq!(
            message,
            "format=PMX section=parsePmxModelJson offset=unknown: invalid PMX magic bytes"
        );
    }

    #[test]
    fn parses_vmd_animation_json_through_wasm_wrapper() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
        let json = parse_vmd_animation_json(bytes).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(value["kind"], "vmd");
        assert_eq!(value["metadata"]["format"], "vmd");
        assert!(value["cameraFrames"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn samples_vmd_camera_array_layout() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
        let parsed = mmd_anim_format::parse_vmd_animation(bytes).unwrap();
        let camera = mmd_anim_format::sample_vmd_camera_frames(&parsed.camera_frames, 22.5)
            .expect("fixture has camera keyframes");
        let values = camera_state_array(camera);

        assert_eq!(values.len(), 9);
        assert_near(values[0], -40.25);
        assert_vec3_near([values[1], values[2], values[3]], [-0.25, 6.0, 1.625]);
        assert_vec3_near([values[4], values[5], values[6]], [-0.1, -0.1, 0.75]);
        assert_near(values[7], 47.5);
        assert_near(values[8], 1.0);
    }

    #[test]
    fn samples_vmd_camera_track_through_wasm_wrapper() {
        let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
        let track = WasmVmdCameraTrack::from_vmd_bytes(bytes).unwrap();
        assert_eq!(track.frame_count(), 2);
    }

    #[test]
    fn samples_vmd_light_array_layout_and_track_json() {
        let bytes = light_and_self_shadow_vmd_bytes();
        let parsed = mmd_anim_format::parse_vmd_animation(&bytes).unwrap();
        let light = mmd_anim_format::sample_vmd_light_frames(&parsed.light_frames, 20.0)
            .expect("fixture has light keyframes");
        let values = light_state_array(light);

        assert_eq!(values.len(), 6);
        assert_vec3_near([values[0], values[1], values[2]], [0.5, 0.25, 0.5]);
        assert_vec3_near([values[3], values[4], values[5]], [0.5, -0.5, 0.0]);

        let track = WasmVmdLightTrack::from_vmd_bytes(&bytes).unwrap();
        assert_eq!(track.frame_count(), 2);
    }

    #[test]
    fn samples_vmd_self_shadow_array_layout_and_track_json() {
        let bytes = light_and_self_shadow_vmd_bytes();
        let parsed = mmd_anim_format::parse_vmd_animation(&bytes).unwrap();
        let self_shadow =
            mmd_anim_format::sample_vmd_self_shadow_frames(&parsed.self_shadow_frames, 20.0)
                .expect("fixture has self-shadow keyframes");
        let values = self_shadow_state_array(self_shadow);

        assert_eq!(values.len(), 2);
        assert_near(values[0], 1.0);
        assert_near(values[1], 40.0);

        let track = WasmVmdSelfShadowTrack::from_vmd_bytes(&bytes).unwrap();
        assert_eq!(track.frame_count(), 2);
    }

    fn light_and_self_shadow_vmd_bytes() -> Vec<u8> {
        mmd_anim_format::export_vmd_animation(&mmd_anim_format::vmd::VmdParsedAnimation {
            kind: "vmd",
            metadata: mmd_anim_format::vmd::VmdParsedMetadata {
                format: "vmd",
                model_name: "light_shadow".to_owned(),
                model_name_bytes: Vec::new(),
                counts: mmd_anim_format::vmd::VmdParsedCounts {
                    bones: 0,
                    morphs: 0,
                    cameras: 0,
                    lights: 2,
                    self_shadows: 2,
                    properties: 0,
                },
                max_frame: 30,
            },
            bone_frames: Vec::new(),
            morph_frames: Vec::new(),
            camera_frames: Vec::new(),
            light_frames: vec![
                mmd_anim_format::vmd::VmdParsedLightFrame {
                    frame: 10,
                    color: [0.0, 0.0, 1.0],
                    direction: [1.0, 0.0, 0.0],
                },
                mmd_anim_format::vmd::VmdParsedLightFrame {
                    frame: 30,
                    color: [1.0, 0.5, 0.0],
                    direction: [0.0, -1.0, 0.0],
                },
            ],
            self_shadow_frames: vec![
                mmd_anim_format::vmd::VmdParsedSelfShadowFrame {
                    frame: 10,
                    mode: 1,
                    distance: 20.0,
                },
                mmd_anim_format::vmd::VmdParsedSelfShadowFrame {
                    frame: 30,
                    mode: 2,
                    distance: 60.0,
                },
            ],
            property_frames: Vec::new(),
        })
    }

    fn assert_near(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-4,
            "actual={actual} expected={expected}"
        );
    }

    fn assert_vec3_near(actual: [f32; 3], expected: [f32; 3]) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert_near(*actual, expected);
        }
    }

    #[test]
    fn exports_pmx_json_bytes_through_wasm_wrapper() {
        let json = serde_json::json!({
            "metadata": {
                "format": "pmx",
                "version": 2.0,
                "encoding": "utf-8",
                "name": "model",
                "englishName": "",
                "comment": "",
                "englishComment": "",
                "counts": {
                    "vertices": 0,
                    "faces": 0,
                    "materials": 0,
                    "bones": 0,
                    "morphs": 0,
                    "displayFrames": 0,
                    "rigidBodies": 0,
                    "joints": 0,
                    "softBodies": 0
                },
                "indexSizes": {
                    "vertex": 4,
                    "texture": 1,
                    "material": 1,
                    "bone": 1,
                    "morph": 1,
                    "rigidBody": 1
                },
                "additionalUvCount": 0
            },
            "geometry": {
                "positions": [],
                "normals": [],
                "uvs": [],
                "additionalUvs": [],
                "indices": [],
                "skinIndices": [],
                "skinWeights": [],
                "edgeScale": [],
                "materialGroups": [],
                "sdef": { "enabled": [], "c": [], "r0": [], "r1": [], "rw0": [], "rw1": [] },
                "qdef": { "enabled": [] }
            },
            "materials": [],
            "skeleton": { "bones": [] },
            "morphs": [],
            "displayFrames": [],
            "rigidBodies": [],
            "joints": [],
            "softBodies": [],
            "diagnostics": []
        })
        .to_string();

        let bytes = export_pmx_model_json_bytes(&json).unwrap();
        let parsed = mmd_anim_format::parse_pmx_model(&bytes).unwrap();

        assert_eq!(parsed.metadata.name, "model");
        assert_eq!(parsed.metadata.counts.vertices, 0);
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn exports_accessory_manifest_bytes_through_wasm_wrapper() {
        let data = br#"xof 0303txt 0032
TextureFilename { "tex/main.png"; }
"#;
        let bytes = export_accessory_manifest_bytes(data, Some("stage.x".to_owned())).unwrap();
        let parsed = mmd_anim_format::parse_accessory_manifest(&bytes, Some("stage.x")).unwrap();

        assert_eq!(parsed.format, "x");
        assert_eq!(parsed.texture_references, vec!["tex/main.png"]);
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn rejects_invalid_pmx_json_geometry_lengths() {
        let model: mmd_anim_format::PmxParsedModel = serde_json::from_value(serde_json::json!({
            "metadata": {
                "format": "pmx",
                "version": 2.0,
                "encoding": "utf-8",
                "name": "model",
                "englishName": "",
                "comment": "",
                "englishComment": "",
                "counts": {
                    "vertices": 1,
                    "faces": 0,
                    "materials": 0,
                    "bones": 0,
                    "morphs": 0,
                    "displayFrames": 0,
                    "rigidBodies": 0,
                    "joints": 0,
                    "softBodies": 0
                },
                "indexSizes": {
                    "vertex": 4,
                    "texture": 1,
                    "material": 1,
                    "bone": 1,
                    "morph": 1,
                    "rigidBody": 1
                },
                "additionalUvCount": 0
            },
            "geometry": {
                "positions": [0.0, 0.0, 0.0],
                "normals": [],
                "uvs": [0.0, 0.0],
                "additionalUvs": [],
                "indices": [],
                "skinIndices": [],
                "skinWeights": [],
                "edgeScale": [],
                "materialGroups": [],
                "sdef": { "enabled": [], "c": [], "r0": [], "r1": [], "rw0": [], "rw1": [] },
                "qdef": { "enabled": [] }
            },
            "materials": [],
            "skeleton": { "bones": [] },
            "morphs": [],
            "displayFrames": [],
            "rigidBodies": [],
            "joints": [],
            "softBodies": [],
            "diagnostics": []
        }))
        .unwrap();

        let error = mmd_anim_format::validate_pmx_export_model(&model).unwrap_err();
        assert!(error.contains("normals length mismatch"));
    }

    #[test]
    fn exports_pmx_from_parts_roundtrip() {
        let metadata = serde_json::json!({
            "name": "parts-model",
            "englishName": "parts-model-en",
            "comment": "built from typed arrays",
            "encoding": "utf-8",
            "indexSizes": {
                "vertex": 1,
                "texture": 1,
                "material": 1,
                "bone": 1,
                "morph": 1,
                "rigidBody": 1
            },
            "materialName": "default-mat"
        })
        .to_string();
        let bytes = export_pmx_from_parts(
            &metadata,
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            &[0, 1, 2],
            &[],
            &[],
            &[],
        )
        .unwrap();
        let parsed = mmd_anim_format::parse_pmx_model(&bytes).unwrap();

        assert_eq!(parsed.metadata.name, "parts-model");
        assert_eq!(parsed.metadata.english_name, "parts-model-en");
        assert_eq!(parsed.metadata.comment, "built from typed arrays");
        assert_eq!(parsed.metadata.counts.vertices, 3);
        assert_eq!(parsed.metadata.counts.faces, 1);
        assert_eq!(parsed.metadata.counts.materials, 1);
        assert_eq!(parsed.metadata.counts.bones, 1);
        assert_eq!(parsed.metadata.index_sizes.vertex, 1);
        assert_eq!(parsed.materials[0].name, "default-mat");
        assert_eq!(parsed.geometry.indices, vec![0, 1, 2]);
        assert_eq!(parsed.geometry.skin_weights[0], 1.0);
        assert_eq!(parsed.geometry.skin_weights[4], 1.0);
        assert_eq!(parsed.geometry.skin_weights[8], 1.0);
    }

    #[test]
    fn rejects_pmx_parts_stride_mismatch() {
        let descriptor: mmd_anim_format::PmxPartsDescriptor =
            serde_json::from_value(serde_json::json!({"name": "bad"})).unwrap();
        let error = mmd_anim_format::build_pmx_model_from_parts(mmd_anim_format::PmxPartsInput {
            descriptor,
            positions_xyz: &[0.0, 0.0, 0.0],
            normals_xyz: &[],
            uvs_xy: &[0.0, 0.0],
            indices: &[],
            skin_indices: &[],
            skin_weights: &[],
            edge_scale: &[],
        })
        .unwrap_err();

        assert!(error.contains("normals_xyz"));
    }

    #[test]
    fn rejects_pmx_parts_partial_skinning_input() {
        let descriptor: mmd_anim_format::PmxPartsDescriptor =
            serde_json::from_value(serde_json::json!({"name": "bad-skin"})).unwrap();
        let error = mmd_anim_format::build_pmx_model_from_parts(mmd_anim_format::PmxPartsInput {
            descriptor,
            positions_xyz: &[0.0, 0.0, 0.0],
            normals_xyz: &[0.0, 0.0, 1.0],
            uvs_xy: &[0.0, 0.0],
            indices: &[],
            skin_indices: &[0, 0, 0, 0],
            skin_weights: &[],
            edge_scale: &[],
        })
        .unwrap_err();

        assert!(error.contains("skin_indices and skin_weights"));
    }

    #[test]
    fn rejects_pmx_parts_out_of_range_skin_bone_index() {
        let descriptor: mmd_anim_format::PmxPartsDescriptor =
            serde_json::from_value(serde_json::json!({"name": "bad-skin-index"})).unwrap();
        let error = mmd_anim_format::build_pmx_model_from_parts(mmd_anim_format::PmxPartsInput {
            descriptor,
            positions_xyz: &[0.0, 0.0, 0.0],
            normals_xyz: &[0.0, 0.0, 1.0],
            uvs_xy: &[0.0, 0.0],
            indices: &[],
            skin_indices: &[1, 0, 0, 0],
            skin_weights: &[1.0, 0.0, 0.0, 0.0],
            edge_scale: &[],
        })
        .unwrap_err();

        assert!(error.contains("out-of-range bone index"));
    }

    #[test]
    fn rejects_mismatched_rest_position_buffer() {
        assert!(
            build_model(ModelInput {
                parent_indices: &[-1, 0],
                rest_positions_xyz: &[0.0, 0.0, 0.0],
                inverse_bind_matrices: &[],
                transform_orders: &[],
                ik_solvers_u32: &[],
                ik_solver_limit_angles: &[],
                ik_links_u32: &[],
                ik_link_limits: &[],
                append_u32: &[],
                append_ratios: &[],
                morph_count: 0,
                bone_morph_u32: &[],
                bone_morph_f32: &[],
                group_morph_u32: &[],
                group_morph_ratios: &[],
            })
            .is_err()
        );
    }

    #[test]
    fn applies_inverse_bind_through_wasm_wrapper() {
        let inverse_bind =
            glam::Mat4::from_translation(glam::Vec3::new(-2.0, 0.0, 0.0)).to_cols_array();
        let model =
            WasmMmdModel::with_inverse_bind(&[-1], &[2.0, 0.0, 0.0], &inverse_bind).unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);

        runtime.evaluate_rest_pose();

        let world_matrices = runtime.world_matrices();
        assert_eq!(world_matrices[12], 2.0);
        let skinning_matrices = runtime.skinning_matrices();
        assert_eq!(skinning_matrices[12], 0.0);
    }

    #[test]
    fn creates_ik_solver_through_wasm_wrapper() {
        let model = WasmMmdModel::with_ik(
            &[-1, 0, 1],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[0, 2, 0, 1, 2],
            &[0.5],
            &[1, IK_LINK_FLAG_ANGLE_LIMIT],
            &[-1.0, -0.5, -0.25, 1.0, 0.5, 0.25],
        )
        .unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 0);

        assert_eq!(runtime.ik_enabled(), vec![1]);
    }

    #[test]
    fn applies_transform_order_to_append_chain_through_wasm_wrapper() {
        let model = WasmMmdModel::with_full_and_transform_order(
            &[-1, -1, -1, 1],
            &[
                0.0, 0.0, 0.0, //
                0.0, 0.0, 0.0, //
                0.0, 0.0, 0.0, //
                1.0, 0.0, 0.0,
            ],
            &[],
            &[0, 2, 1, 3],
            &[],
            &[],
            &[],
            &[],
            &[2, 0, APPEND_FLAG_ROTATION, 1, 2, APPEND_FLAG_ROTATION],
            &[1.0, 1.0],
        )
        .unwrap();
        let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let clip = WasmMmdClip::new(
            &[0, 0, 1],
            &[0],
            &[
                0.0,
                0.0,
                0.0,
                rotation[0],
                rotation[1],
                rotation[2],
                rotation[3],
            ],
            &[],
            &[],
            &[],
            &[],
            &[],
            0,
        )
        .unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);

        runtime.evaluate_clip_frame(&clip, 0.0);

        let matrices = runtime.world_matrices();
        assert!(matrices[48 + 12].abs() < 1.0e-5);
        assert!((matrices[48 + 13] - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn evaluates_clip_frame_through_wasm_wrapper() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let clip = WasmMmdClip::new(
            &[0, 0, 2],
            &[0, 60],
            &[
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            &[0, 0, 2],
            &[0, 60],
            &[0.0, 1.0],
            &[0, 30],
            &[1, 0],
            1,
        )
        .unwrap();
        let mut runtime = WasmMmdRuntimeInstance::with_counts(&model, 1, 1);

        runtime.evaluate_clip_frame(&clip, 30.0);

        let matrices = runtime.world_matrices();
        assert_eq!(matrices[12], 1.0);
        assert_eq!(runtime.morph_weights(), vec![0.5]);
        assert_eq!(runtime.ik_enabled(), vec![0]);
        assert_eq!(runtime.morph_weights_cache, vec![0.5]);
        assert_eq!(runtime.ik_enabled_cache, vec![0]);
    }

    #[test]
    fn evaluates_clip_frame_batch_through_wasm_wrapper_without_mutating_source() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let clip = WasmMmdClip::new(
            &[0, 0, 2],
            &[0, 60],
            &[
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            &[0, 0, 2],
            &[0, 60],
            &[0.0, 1.0],
            &[0, 30],
            &[1, 0],
            1,
        )
        .unwrap();
        let mut runtime = WasmMmdRuntimeInstance::with_counts(&model, 1, 1);
        runtime.evaluate_clip_frame(&clip, 30.0);
        let source_world_before = runtime.world_matrices();
        let source_morph_before = runtime.morph_weights();

        assert_eq!(runtime.clip_frame_batch_world_matrix_f32_len(3), 48);
        assert_eq!(runtime.clip_frame_batch_morph_weight_f32_len(3), 3);
        let batch = runtime
            .evaluate_clip_frame_batch(&clip, 0.0, 30.0, 3, 0)
            .unwrap();

        assert_eq!(batch.frame_count(), 3);
        assert_eq!(batch.bone_count(), 1);
        assert_eq!(batch.morph_count(), 1);
        assert_eq!(batch.world_matrix_f32_len(), 48);
        assert_eq!(batch.morph_weight_f32_len(), 3);
        let batch_world = batch.world_matrices();
        assert_eq!(batch_world[12], 0.0);
        assert_eq!(batch_world[16 + 12], 1.0);
        assert_eq!(batch_world[32 + 12], 2.0);
        assert_eq!(batch.morph_weights(), vec![0.0, 0.5, 1.0]);

        let mut world_copy = vec![0.0; batch.world_matrix_f32_len()];
        assert!(batch.copy_world_matrices(&mut world_copy));
        assert_eq!(world_copy, batch_world);
        let mut short_world_copy = vec![0.0; batch.world_matrix_f32_len() - 1];
        assert!(!batch.copy_world_matrices(&mut short_world_copy));

        assert_eq!(runtime.world_matrices(), source_world_before);
        assert_eq!(runtime.morph_weights(), source_morph_before);
    }

    #[test]
    fn evaluates_append_rotation_through_wasm_wrapper() {
        let model = WasmMmdModel::with_append(
            &[-1, -1, 1],
            &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[1, 0, APPEND_FLAG_ROTATION],
            &[1.0],
        )
        .unwrap();
        let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let clip = WasmMmdClip::new(
            &[0, 0, 1],
            &[0],
            &[
                0.0,
                0.0,
                0.0,
                rotation[0],
                rotation[1],
                rotation[2],
                rotation[3],
            ],
            &[],
            &[],
            &[],
            &[],
            &[],
            0,
        )
        .unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);

        runtime.evaluate_clip_frame(&clip, 0.0);

        let matrices = runtime.world_matrices();
        assert!(matrices[32 + 12].abs() < 1.0e-5);
        assert!((matrices[32 + 13] - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn copy_world_matrices_matches_allocating_api() {
        let model = WasmMmdModel::new(&[-1, 0], &[1.0, 0.0, 0.0, 0.0, 2.0, 0.0]).unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);
        runtime.evaluate_rest_pose();

        let expected = runtime.world_matrices();
        let mut buf = vec![0.0_f32; expected.len()];
        assert!(runtime.copy_world_matrices(&mut buf));
        assert_eq!(buf, expected);
    }

    #[test]
    fn copy_world_matrices_rejects_short_buffer() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);
        runtime.evaluate_rest_pose();

        let mut buf = vec![0.0_f32; 15];
        assert!(!runtime.copy_world_matrices(&mut buf));
    }

    #[test]
    fn copy_skinning_matrices_matches_allocating_api() {
        let model = WasmMmdModel::new(&[-1, 0], &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]).unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);
        runtime.evaluate_rest_pose();

        let expected = runtime.skinning_matrices();
        let mut buf = vec![0.0_f32; expected.len()];
        assert!(runtime.copy_skinning_matrices(&mut buf));
        assert_eq!(buf, expected);
    }

    #[test]
    fn copy_morph_weights_matches_allocating_api() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let clip = WasmMmdClip::new(
            &[],
            &[],
            &[],
            &[0, 0, 2],
            &[0, 60],
            &[0.0, 1.0],
            &[],
            &[],
            0,
        )
        .unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 2);
        runtime.evaluate_clip_frame(&clip, 30.0);

        let expected = runtime.morph_weights();
        let mut buf = vec![0.0_f32; expected.len()];
        assert!(runtime.copy_morph_weights(&mut buf));
        assert_eq!(buf, expected);
    }

    #[test]
    fn copy_morph_weights_rejects_short_buffer() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 1);
        runtime.evaluate_rest_pose();

        let mut buf = vec![0.0_f32; 0];
        assert!(!runtime.copy_morph_weights(&mut buf));
    }

    #[test]
    fn copy_ik_enabled_matches_allocating_api() {
        let model = WasmMmdModel::with_ik(
            &[-1, 0, 1],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[0, 2, 0, 1, 2],
            &[0.5],
            &[1, IK_LINK_FLAG_ANGLE_LIMIT],
            &[-1.0, -0.5, -0.25, 1.0, 0.5, 0.25],
        )
        .unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 0);

        let expected = runtime.ik_enabled();
        let mut buf = vec![0u8; expected.len()];
        assert!(runtime.copy_ik_enabled(&mut buf));
        assert_eq!(buf, expected);
    }

    #[test]
    fn copy_ik_enabled_rejects_short_buffer() {
        let model = WasmMmdModel::with_ik(
            &[-1, 0, 1],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[0, 2, 0, 1, 2],
            &[0.5],
            &[1, IK_LINK_FLAG_ANGLE_LIMIT],
            &[-1.0, -0.5, -0.25, 1.0, 0.5, 0.25],
        )
        .unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 0);

        let mut buf = vec![0u8; 0];
        assert!(!runtime.copy_ik_enabled(&mut buf));
    }

    #[test]
    fn morph_weight_len_matches_weights_length() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 5);
        assert_eq!(runtime.morph_weight_len(), runtime.morph_weights().len());
    }

    #[test]
    fn morph_weight_len_reflects_constructor_count() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 3);
        assert_eq!(runtime.morph_weight_len(), 3);
    }

    #[test]
    fn ik_enabled_len_matches_enabled_length() {
        let model = WasmMmdModel::with_ik(
            &[-1, 0, 1],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[0, 2, 0, 1, 2],
            &[0.5],
            &[1, IK_LINK_FLAG_ANGLE_LIMIT],
            &[-1.0, -0.5, -0.25, 1.0, 0.5, 0.25],
        )
        .unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 0);
        assert_eq!(runtime.ik_enabled_len(), runtime.ik_enabled().len());
    }

    #[test]
    fn ik_enabled_len_reflects_solver_count() {
        let model = WasmMmdModel::with_ik(
            &[-1, 0, 1],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[0, 2, 0, 1, 2],
            &[0.5],
            &[1, IK_LINK_FLAG_ANGLE_LIMIT],
            &[-1.0, -0.5, -0.25, 1.0, 0.5, 0.25],
        )
        .unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 0);
        assert_eq!(runtime.ik_enabled_len(), 1);
    }

    #[test]
    fn creates_bone_morph_through_wasm_wrapper() {
        let model = WasmMmdModel::with_morphs(
            &[-1],
            &[0.0, 0.0, 0.0],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            1,
            &[0, 0],
            &[2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            &[],
            &[],
        )
        .unwrap();
        let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
        let clip = WasmMmdClip::new(
            &[0, 0, 1],
            &[0],
            &[
                0.0,
                0.0,
                0.0,
                rotation[0],
                rotation[1],
                rotation[2],
                rotation[3],
            ],
            &[0, 0, 2],
            &[0, 60],
            &[0.0, 1.0],
            &[],
            &[],
            0,
        )
        .unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 1);
        runtime.evaluate_clip_frame(&clip, 60.0);
        let matrices = runtime.world_matrices();
        assert!((matrices[12] - 2.0).abs() < 1.0e-5);
    }

    #[test]
    fn rejects_bone_morph_index_out_of_range() {
        let result = build_morph_init_from_wasm(&ModelInput {
            parent_indices: &[-1],
            rest_positions_xyz: &[0.0, 0.0, 0.0],
            inverse_bind_matrices: &[],
            transform_orders: &[],
            ik_solvers_u32: &[],
            ik_solver_limit_angles: &[],
            ik_links_u32: &[],
            ik_link_limits: &[],
            append_u32: &[],
            append_ratios: &[],
            morph_count: 1,
            bone_morph_u32: &[1, 0],
            bone_morph_f32: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        });
        assert!(result.is_err());
    }

    #[test]
    fn rejects_bone_morph_odd_u32_length() {
        let result = build_morph_init_from_wasm(&ModelInput {
            parent_indices: &[-1],
            rest_positions_xyz: &[0.0, 0.0, 0.0],
            inverse_bind_matrices: &[],
            transform_orders: &[],
            ik_solvers_u32: &[],
            ik_solver_limit_angles: &[],
            ik_links_u32: &[],
            ik_link_limits: &[],
            append_u32: &[],
            append_ratios: &[],
            morph_count: 1,
            bone_morph_u32: &[0, 0, 0],
            bone_morph_f32: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        });
        assert!(result.is_err());
    }

    #[test]
    fn cache_populated_after_construction() {
        let model = WasmMmdModel::new(&[-1, 0], &[1.0, 0.0, 0.0, 0.0, 2.0, 0.0]).unwrap();
        let runtime = WasmMmdRuntimeInstance::new(&model, 0);

        assert_eq!(runtime.world_matrices_cache.len(), 32);
        assert_eq!(runtime.skinning_matrices_cache.len(), 32);
        assert_eq!(runtime.morph_weights_cache.len(), 0);
        assert_eq!(runtime.ik_enabled_cache.len(), 0);
    }

    #[test]
    fn for_model_uses_model_morph_and_ik_counts() {
        let morph_model = WasmMmdModel::with_morphs(
            &[-1],
            &[0.0, 0.0, 0.0],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            &[],
            2,
            &[1, 0],
            &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            &[],
            &[],
        )
        .unwrap();
        let morph_runtime = WasmMmdRuntimeInstance::for_model(&morph_model);

        assert_eq!(morph_runtime.world_matrix_f32_len(), 16);
        assert_eq!(morph_runtime.morph_weight_len(), 2);
        assert_eq!(morph_runtime.ik_enabled_len(), 0);

        let ik_model = WasmMmdModel::with_ik(
            &[-1, 0],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[0, 1, 0, 1, 2],
            &[0.5],
            &[1, IK_LINK_FLAG_ANGLE_LIMIT],
            &[-1.0, -0.5, -0.25, 1.0, 0.5, 0.25],
        )
        .unwrap();
        let ik_runtime = WasmMmdRuntimeInstance::for_model(&ik_model);

        assert_eq!(ik_runtime.world_matrix_f32_len(), 32);
        assert_eq!(ik_runtime.morph_weight_len(), 0);
        assert_eq!(ik_runtime.ik_enabled_len(), 1);
    }

    #[test]
    fn cache_contents_match_allocating_apis() {
        let model = WasmMmdModel::new(&[-1, 0], &[1.0, 0.0, 0.0, 0.0, 2.0, 0.0]).unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);
        runtime.evaluate_rest_pose();

        let expected_world = runtime.world_matrices();
        let mut buf = vec![0.0_f32; expected_world.len()];
        runtime.copy_world_matrices(&mut buf);
        assert_eq!(runtime.world_matrices_cache, buf);

        let expected_skin = runtime.skinning_matrices();
        let mut skin_buf = vec![0.0_f32; expected_skin.len()];
        runtime.copy_skinning_matrices(&mut skin_buf);
        assert_eq!(runtime.skinning_matrices_cache, skin_buf);
    }

    #[test]
    fn cache_refreshed_after_clip_frame() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        let clip = WasmMmdClip::new(
            &[0, 0, 2],
            &[0, 60],
            &[
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            &[],
            &[],
            &[],
            &[],
            &[],
            0,
        )
        .unwrap();
        let mut runtime = WasmMmdRuntimeInstance::new(&model, 0);
        runtime.evaluate_rest_pose();

        let before = runtime.world_matrices_cache[12];
        runtime.evaluate_clip_frame(&clip, 30.0);
        let after = runtime.world_matrices_cache[12];

        assert_ne!(before, after);
        assert!((after - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn rejects_morph_count_zero_with_data() {
        let result = build_morph_init_from_wasm(&ModelInput {
            parent_indices: &[-1],
            rest_positions_xyz: &[0.0, 0.0, 0.0],
            inverse_bind_matrices: &[],
            transform_orders: &[],
            ik_solvers_u32: &[],
            ik_solver_limit_angles: &[],
            ik_links_u32: &[],
            ik_link_limits: &[],
            append_u32: &[],
            append_ratios: &[],
            morph_count: 0,
            bone_morph_u32: &[0, 0],
            bone_morph_f32: &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            group_morph_u32: &[],
            group_morph_ratios: &[],
        });
        assert!(result.is_err());
    }

    #[test]
    fn morph_and_ik_count_on_flat_model() {
        let model = WasmMmdModel::new(&[-1], &[0.0, 0.0, 0.0]).unwrap();
        assert_eq!(model.morph_count(), 0);
        assert_eq!(model.ik_count(), 0);
    }

    #[test]
    fn morph_and_ik_count_on_ik_model() {
        let model = WasmMmdModel::with_ik(
            &[-1, 0],
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            &[0, 1, 0, 1, 2],
            &[0.5],
            &[1, IK_LINK_FLAG_ANGLE_LIMIT],
            &[-1.0, -0.5, -0.25, 1.0, 0.5, 0.25],
        )
        .unwrap();
        assert_eq!(model.morph_count(), 0);
        assert_eq!(model.ik_count(), 1);
    }

    // --- WasmPmxGeometry tests ---
    // Tests call WasmPmxGeometry::parse_inner (returns String error) rather than
    // from_pmx_bytes (returns JsValue) to avoid JsValue::from_str panicking outside wasm.

    #[test]
    fn pmx_geometry_dto_basic_roundtrip() {
        let metadata = serde_json::json!({
            "name": "geo-test",
            "encoding": "utf-8",
            "indexSizes": { "vertex": 1, "texture": 1, "material": 1, "bone": 1, "morph": 1, "rigidBody": 1 }
        })
        .to_string();
        let pmx_bytes = export_pmx_from_parts(
            &metadata,
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            &[0, 1, 2],
            &[],
            &[],
            &[],
        )
        .unwrap();

        let geo = WasmPmxGeometry::parse_inner(&pmx_bytes).unwrap();

        assert_eq!(geo.vertex_count(), 3);
        assert_eq!(geo.face_count(), 1);
        assert_eq!(geo.additional_uv_count(), 0);
        assert_eq!(
            geo.positions(),
            vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        );
        assert_eq!(
            geo.normals(),
            vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0]
        );
        assert_eq!(geo.additional_uvs(), Vec::<f32>::new());
        assert_eq!(geo.indices(), vec![0, 1, 2]);
        assert_eq!(geo.material_groups().len(), geo.material_group_count() * 3);
        assert_eq!(geo.skin_indices().len(), 12); // vertex_count × 4
        assert_eq!(geo.skin_weights().len(), 12); // vertex_count × 4
        assert_eq!(geo.edge_scale().len(), 3);
        assert_eq!(geo.sdef_enabled(), vec![0u8, 0, 0]);
        assert_eq!(geo.sdef_c().len(), 9); // vertex_count × 3
        assert_eq!(geo.sdef_r0().len(), 9);
        assert_eq!(geo.sdef_r1().len(), 9);
        assert_eq!(geo.sdef_rw0().len(), 9);
        assert_eq!(geo.sdef_rw1().len(), 9);
        assert_eq!(geo.qdef_enabled(), vec![0u8, 0, 0]);
        assert_eq!(geo.skinning_modes(), vec!["bdef1", "bdef1", "bdef1"]);
    }

    #[test]
    fn pmx_geometry_dto_sdef_vertex() {
        // Minimal PMX with 1 SDEF vertex (weight type 3)
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"PMX ");
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.push(8); // data_count
        buf.push(1); // UTF-8
        buf.push(0); // extra_uv_count
        buf.push(1); // vertex_index_size
        buf.push(1); // texture_index_size
        buf.push(1); // material_index_size
        buf.push(1); // bone_index_size
        buf.push(1); // morph_index_size
        buf.push(1); // rigidbody_index_size
        // 4 empty model-info strings (UTF-8 i32-prefixed, len=0 each)
        for _ in 0..4 {
            buf.extend_from_slice(&0i32.to_le_bytes());
        }
        // 1 vertex
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // pos x
        buf.extend_from_slice(&2.0f32.to_le_bytes()); // pos y
        buf.extend_from_slice(&3.0f32.to_le_bytes()); // pos z
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // normal x
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // normal y
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // normal z
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // uv u
        buf.extend_from_slice(&0.0f32.to_le_bytes()); // uv v
        buf.push(3u8); // weight type = SDEF
        buf.push(0u8); // bone_index_0 (1-byte)
        buf.push(0u8); // bone_index_1 (1-byte)
        buf.extend_from_slice(&0.25f32.to_le_bytes()); // weight
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // c.x
        buf.extend_from_slice(&2.0f32.to_le_bytes()); // c.y
        buf.extend_from_slice(&3.0f32.to_le_bytes()); // c.z
        buf.extend_from_slice(&4.0f32.to_le_bytes()); // r0.x
        buf.extend_from_slice(&5.0f32.to_le_bytes()); // r0.y
        buf.extend_from_slice(&6.0f32.to_le_bytes()); // r0.z
        buf.extend_from_slice(&7.0f32.to_le_bytes()); // r1.x
        buf.extend_from_slice(&8.0f32.to_le_bytes()); // r1.y
        buf.extend_from_slice(&9.0f32.to_le_bytes()); // r1.z
        buf.extend_from_slice(&1.0f32.to_le_bytes()); // edge_scale
        // 8 empty sections (faces/textures/materials/bones/morphs/displayFrames/rigidBodies/joints)
        for _ in 0..8 {
            buf.extend_from_slice(&0i32.to_le_bytes());
        }

        let geo = WasmPmxGeometry::parse_inner(&buf).unwrap();

        assert_eq!(geo.vertex_count(), 1);
        assert_eq!(geo.positions(), vec![1.0, 2.0, 3.0]);
        assert_eq!(geo.sdef_enabled(), vec![1u8]);
        assert_eq!(geo.sdef_c(), vec![1.0, 2.0, 3.0]);
        assert_eq!(geo.sdef_r0(), vec![4.0, 5.0, 6.0]);
        assert_eq!(geo.sdef_r1(), vec![7.0, 8.0, 9.0]);
        assert_eq!(geo.sdef_rw0().len(), 3); // pre-computed from r0/r1/c/weight
        assert_eq!(geo.sdef_rw1().len(), 3);
        assert_eq!(geo.qdef_enabled(), vec![0u8]);
    }

    #[test]
    fn pmx_geometry_dto_rejects_empty_input() {
        assert!(WasmPmxGeometry::parse_inner(&[]).is_err());
    }

    // --- parsePmxModelNonGeometryJson tests ---

    fn minimal_pmx_bytes() -> Vec<u8> {
        export_pmx_from_parts(
            &serde_json::json!({
                "name": "test",
                "encoding": "utf-8",
                "indexSizes": { "vertex": 1, "texture": 1, "material": 1, "bone": 1, "morph": 1, "rigidBody": 1 }
            })
            .to_string(),
            &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            &[0, 1, 2],
            &[],
            &[],
            &[],
        )
        .unwrap()
    }

    #[test]
    fn pmx_non_geometry_json_excludes_geometry_key() {
        let pmx_bytes = minimal_pmx_bytes();
        let json_str = parse_pmx_model_non_geometry_json_inner(&pmx_bytes).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(
            value.get("geometry").is_none(),
            "geometry key must not appear in non-geometry JSON"
        );
    }

    #[test]
    fn pmx_non_geometry_json_contains_required_keys() {
        let pmx_bytes = minimal_pmx_bytes();
        let json_str = parse_pmx_model_non_geometry_json_inner(&pmx_bytes).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();

        assert!(value.get("metadata").is_some(), "metadata must be present");
        assert!(
            value.get("materials").is_some(),
            "materials must be present"
        );
        assert!(value.get("skeleton").is_some(), "skeleton must be present");
        assert!(value.get("morphs").is_some(), "morphs must be present");
        assert!(
            value.get("displayFrames").is_some(),
            "displayFrames must be present"
        );
        assert!(
            value.get("rigidBodies").is_some(),
            "rigidBodies must be present"
        );
        assert!(value.get("joints").is_some(), "joints must be present");
        assert!(
            value.get("softBodies").is_some(),
            "softBodies must be present"
        );
        assert!(
            value.get("diagnostics").is_some(),
            "diagnostics must be present"
        );
    }

    #[test]
    fn pmx_non_geometry_json_rejects_empty_input() {
        let result = parse_pmx_model_non_geometry_json_inner(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn pmx_parsed_model_handle_exposes_non_geometry_json_and_geometry() {
        let pmx_bytes = minimal_pmx_bytes();
        let parsed = WasmPmxParsedModel::parse_inner(&pmx_bytes).unwrap();
        let json_str = parsed.non_geometry_json().unwrap();
        let value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let geometry = parsed.geometry();

        assert!(value.get("geometry").is_none());
        assert!(value.get("metadata").is_some());
        assert_eq!(geometry.vertex_count(), 3);
        assert_eq!(geometry.indices(), vec![0, 1, 2]);
    }

    #[test]
    fn pmx_parsed_model_handle_rejects_empty_input() {
        assert!(WasmPmxParsedModel::parse_inner(&[]).is_err());
    }
}
