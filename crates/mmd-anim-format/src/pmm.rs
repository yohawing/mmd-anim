use std::collections::{BTreeMap, HashMap};

use encoding_rs::SHIFT_JIS;
use serde::Serialize;

use crate::error::ImportError;
use crate::pmx::PmxParsedModel;
use crate::vmd::{VmdParsedAnimation, VmdParsedBoneFrame, VmdParsedMorphFrame};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmParsedManifest {
    pub signature: &'static str,
    pub version: String,
    pub parsed_version: Option<u32>,
    pub byte_length: usize,
    pub project_settings: PmmProjectSettings,
    pub timeline: PmmTimeline,
    pub display_state: PmmDisplayState,
    pub header_text_entries: Vec<PmmHeaderTextEntry>,
    pub model_slots: Vec<PmmModelSlot>,
    pub document_summary: Option<PmmDocumentSummary>,
    pub document_global_summary: Option<PmmDocumentGlobalSummary>,
    pub asset_summary: PmmAssetSummary,
    pub asset_references: Vec<PmmAssetReference>,
    pub model_assets: Vec<PmmSceneAsset>,
    pub accessory_assets: Vec<PmmSceneAsset>,
    pub motion_assets: Vec<PmmSceneAsset>,
    pub audio_assets: Vec<PmmSceneAsset>,
    pub image_assets: Vec<PmmSceneAsset>,
    pub video_assets: Vec<PmmSceneAsset>,
    pub model_paths: Vec<String>,
    pub accessory_paths: Vec<String>,
    pub motion_paths: Vec<String>,
    pub audio_paths: Vec<String>,
    pub image_paths: Vec<String>,
    pub video_paths: Vec<String>,
    pub diagnostics: Vec<PmmParserDiagnostic>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectSettings {
    pub screen_width: Option<u32>,
    pub screen_height: Option<u32>,
    pub timeline_frame_count: Option<u32>,
    pub frame_rate: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmTimeline {
    pub start_frame: Option<u32>,
    pub end_frame_exclusive: Option<u32>,
    pub frame_count: Option<u32>,
    pub frame_rate: Option<f32>,
    pub duration_seconds: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDisplayState {
    pub layout: &'static str,
    pub model_slot_flags: Vec<u8>,
    pub model_slot_flag_entries: Vec<PmmModelSlotFlagEntry>,
    pub document_expand_flags: Option<PmmDocumentExpandFlags>,
    pub selected_model_index: Option<u8>,
    pub document_model_count: Option<u8>,
    pub declared_model_slot_count: Option<u8>,
    pub model_slot_count: usize,
    pub non_zero_model_slot_count: usize,
    pub accessory_slot_count: Option<u8>,
    pub active_model_slot_indices: Vec<usize>,
    pub empty_model_slot_indices: Vec<usize>,
    pub model_slot_flag_counts: BTreeMap<u8, usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentExpandFlags {
    pub editing_cla: bool,
    pub camera_panel: bool,
    pub light_panel: bool,
    pub accessory_panel: bool,
    pub bone_panel: bool,
    pub morph_panel: bool,
    pub self_shadow_panel: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmModelSlotFlagEntry {
    pub slot_index: usize,
    pub flag: u8,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmHeaderTextEntry {
    pub index: usize,
    pub offset: usize,
    pub offset_end: usize,
    pub text: String,
    pub text_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmModelSlot {
    pub slot_index: usize,
    pub display_slot_index: Option<usize>,
    pub offset: usize,
    pub offset_end: usize,
    pub model_path_offset: usize,
    pub trailing_zero_padding_bytes: usize,
    pub next_non_zero_offset: Option<usize>,
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub english_name: String,
    pub english_name_bytes: Vec<u8>,
    pub model_path: String,
    pub normalized_path: String,
    pub asset_reference_index: Option<usize>,
    pub confidence: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmAssetSummary {
    pub reference_count: usize,
    pub high_confidence_count: usize,
    pub medium_confidence_count: usize,
    pub low_confidence_count: usize,
    pub kind_counts: BTreeMap<String, usize>,
    pub extension_counts: BTreeMap<String, usize>,
    pub confidence_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentSummary {
    pub source: &'static str,
    pub selected_model_index: u8,
    pub model_count: usize,
    pub counts: PmmDocumentCounts,
    pub models: Vec<PmmDocumentModelSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentCounts {
    pub models: usize,
    pub bones: usize,
    pub morphs: usize,
    pub initial_bone_keyframes: usize,
    pub bone_keyframes: usize,
    pub initial_morph_keyframes: usize,
    pub morph_keyframes: usize,
    pub initial_model_keyframes: usize,
    pub model_keyframes: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentGlobalSummary {
    pub source: &'static str,
    pub offset: usize,
    pub offset_end: usize,
    pub camera: PmmDocumentTrackSummary,
    pub light: PmmDocumentTrackSummary,
    pub accessories: PmmDocumentAccessoryBlockSummary,
    pub settings: PmmDocumentSettingsSummary,
    pub gravity: PmmDocumentTrackSummary,
    pub self_shadow: PmmDocumentTrackSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentTrackSummary {
    pub offset: usize,
    pub offset_end: usize,
    pub initial_keyframes: usize,
    pub keyframes: usize,
    pub initial_keyframe: Option<PmmDocumentKeyframeSummary>,
    pub keyframe_summaries: Vec<PmmDocumentKeyframeSummary>,
    pub keyframe_count_offset: usize,
    pub keyframes_offset: usize,
    pub keyframes_end_offset: usize,
    pub state_offset: Option<usize>,
    pub state_end_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum PmmDocumentKeyframeSummary {
    Camera {
        index: Option<i32>,
        frame_index: i32,
        previous_keyframe_index: i32,
        next_keyframe_index: i32,
        distance: f32,
        look_at: [f32; 3],
        angle: [f32; 3],
        parent_model_index: i32,
        parent_model_bone_index: i32,
        perspective_view: bool,
        fov: i32,
        selected: bool,
    },
    Light {
        index: Option<i32>,
        frame_index: i32,
        previous_keyframe_index: i32,
        next_keyframe_index: i32,
        color: [f32; 3],
        direction: [f32; 3],
        selected: bool,
    },
    Gravity {
        index: Option<i32>,
        frame_index: i32,
        previous_keyframe_index: i32,
        next_keyframe_index: i32,
        noise_enabled: bool,
        noise: i32,
        acceleration: f32,
        direction: [f32; 3],
        selected: bool,
    },
    SelfShadow {
        index: Option<i32>,
        frame_index: i32,
        previous_keyframe_index: i32,
        next_keyframe_index: i32,
        mode: u8,
        distance: f32,
        selected: bool,
    },
}

#[derive(Debug, Clone, Copy)]
struct PmmDocumentBaseKeyframeSummary {
    index: Option<i32>,
    frame_index: i32,
    previous_keyframe_index: i32,
    next_keyframe_index: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentAccessoryKeyframeSummary {
    pub index: Option<i32>,
    pub frame_index: i32,
    pub previous_keyframe_index: i32,
    pub next_keyframe_index: i32,
    pub visible: bool,
    pub opacity: f32,
    pub parent_model_index: i32,
    pub parent_model_bone_index: i32,
    pub translation: [f32; 3],
    pub orientation: [f32; 3],
    pub scale_factor: f32,
    pub shadow_enabled: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentBoneKeyframeSummary {
    pub index: Option<i32>,
    pub frame_index: i32,
    pub previous_keyframe_index: i32,
    pub next_keyframe_index: i32,
    pub interpolation: [u8; 16],
    pub translation: [f32; 3],
    pub orientation: [f32; 4],
    pub physics_disabled: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentMorphKeyframeSummary {
    pub index: Option<i32>,
    pub frame_index: i32,
    pub previous_keyframe_index: i32,
    pub next_keyframe_index: i32,
    pub weight: f32,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentModelKeyframeSummary {
    pub index: Option<i32>,
    pub frame_index: i32,
    pub previous_keyframe_index: i32,
    pub next_keyframe_index: i32,
    pub visible: bool,
    pub constraint_states: Vec<bool>,
    pub outside_parent_indices: Vec<PmmDocumentOutsideParentIndexSummary>,
    pub self_shadow_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentOutsideParentIndexSummary {
    pub parent_model_index: i32,
    pub parent_model_bone_index: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentBoneStateSummary {
    pub translation: [f32; 3],
    pub orientation: [f32; 4],
    pub dirty: bool,
    pub physics_disabled: bool,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentMorphStateSummary {
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentConstraintStateSummary {
    pub enabled: bool,
}

/// Raw 16-byte outside-parent state block. Semantics not fully confirmed;
/// field names reflect observed byte layout only.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentOutsideParentStateSummary {
    pub parent_model_index: i32,
    pub parent_model_bone_index: i32,
    pub subject_bone_index: i32,
    pub target_model_index: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentAccessoryBlockSummary {
    pub offset: usize,
    pub offset_end: usize,
    pub selected_accessory_index: u8,
    pub horizontal_scroll: i32,
    pub accessory_count: usize,
    pub keyframes: usize,
    pub accessories: Vec<PmmDocumentAccessorySummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentAccessorySummary {
    pub slot_index: usize,
    pub document_accessory_index: u8,
    pub offset: usize,
    pub offset_end: usize,
    pub path_offset: usize,
    pub name: String,
    pub path: String,
    pub asset_reference_index: Option<usize>,
    pub draw_order_index: u8,
    pub keyframes: usize,
    pub initial_keyframe: PmmDocumentAccessoryKeyframeSummary,
    pub keyframe_summaries: Vec<PmmDocumentAccessoryKeyframeSummary>,
    pub keyframe_count_offset: usize,
    pub keyframes_offset: usize,
    pub keyframes_end_offset: usize,
    pub state_offset: usize,
    pub state_end_offset: usize,
    pub visible: bool,
    pub opacity: f32,
    pub parent_model_index: i32,
    pub parent_model_bone_index: i32,
    pub scale_factor: f32,
    pub shadow_enabled: bool,
    pub add_blend_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentSettingsSummary {
    pub offset: usize,
    pub offset_end: usize,
    pub current_frame_index: i32,
    pub horizontal_scroll: i32,
    pub horizontal_scroll_thumb: i32,
    pub editing_mode: i32,
    pub camera_look_mode: u8,
    pub loop_enabled: bool,
    pub begin_frame_index_enabled: bool,
    pub end_frame_index_enabled: bool,
    pub begin_frame_index: i32,
    pub end_frame_index: i32,
    pub audio_enabled: bool,
    pub audio_path: String,
    pub background_video_offset: [i32; 2],
    pub background_video_scale_factor: f32,
    pub background_video_path: String,
    pub background_video_enabled: bool,
    pub background_image_offset: [i32; 2],
    pub background_image_scale_factor: f32,
    pub background_image_path: String,
    pub background_image_enabled: bool,
    pub information_shown: bool,
    pub grid_and_axis_shown: bool,
    pub ground_shadow_shown: bool,
    pub preferred_fps: f32,
    pub screen_capture_mode: i32,
    pub accessory_index_after_models: i32,
    pub ground_shadow_brightness: f32,
    pub translucent_ground_shadow_enabled: bool,
    pub physics_simulation_mode: u8,
    pub edge_color: [f32; 3],
    pub black_background_enabled: bool,
    pub camera_look_at_model_index: i32,
    pub camera_look_at_model_bone_index: i32,
    pub unknown_matrix_offset: usize,
    pub unknown_matrix_end_offset: usize,
    pub following_look_at_enabled: bool,
    pub physics_ground_enabled: bool,
    pub current_frame_index_in_text_field: i32,
    pub model_selection_footer_present: bool,
    pub model_selection_footer_offset: Option<usize>,
    pub model_selection_footer_end_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentModelSummary {
    pub slot_index: usize,
    pub document_model_index: u8,
    pub offset: usize,
    pub offset_end: usize,
    pub path_offset: usize,
    pub name: String,
    pub english_name: String,
    pub path: String,
    pub asset_reference_index: Option<usize>,
    pub bone_count: usize,
    pub morph_count: usize,
    pub constraint_bone_count: usize,
    pub outside_parent_subject_bone_count: usize,
    pub draw_order_index: u8,
    pub transform_order_index: u8,
    pub selected_bone_index: i32,
    pub selected_morph_indices: [i32; 4],
    pub vertical_scroll: i32,
    pub sections: PmmDocumentModelSections,
    pub initial_bone_keyframes: usize,
    pub initial_bone_keyframe_summaries: Vec<PmmDocumentBoneKeyframeSummary>,
    pub bone_keyframes: usize,
    pub bone_keyframe_summaries: Vec<PmmDocumentBoneKeyframeSummary>,
    pub initial_morph_keyframes: usize,
    pub initial_morph_keyframe_summaries: Vec<PmmDocumentMorphKeyframeSummary>,
    pub morph_keyframes: usize,
    pub morph_keyframe_summaries: Vec<PmmDocumentMorphKeyframeSummary>,
    pub initial_model_keyframes: usize,
    pub model_keyframes: usize,
    pub initial_model_keyframe: PmmDocumentModelKeyframeSummary,
    pub model_keyframe_summaries: Vec<PmmDocumentModelKeyframeSummary>,
    pub last_frame_index: i32,
    pub visible: bool,
    pub blend_enabled: bool,
    pub edge_width: f32,
    pub self_shadow_enabled: bool,
    pub bone_state_summaries: Vec<PmmDocumentBoneStateSummary>,
    pub morph_state_summaries: Vec<PmmDocumentMorphStateSummary>,
    pub constraint_state_summaries: Vec<PmmDocumentConstraintStateSummary>,
    pub outside_parent_state_summaries: Vec<PmmDocumentOutsideParentStateSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmDocumentModelSections {
    pub initial_bone_keyframes_offset: usize,
    pub bone_keyframe_count_offset: usize,
    pub bone_keyframes_offset: usize,
    pub bone_keyframes_end_offset: usize,
    pub initial_morph_keyframes_offset: usize,
    pub morph_keyframe_count_offset: usize,
    pub morph_keyframes_offset: usize,
    pub morph_keyframes_end_offset: usize,
    pub initial_model_keyframe_offset: usize,
    pub model_keyframe_count_offset: usize,
    pub model_keyframes_offset: usize,
    pub model_keyframes_end_offset: usize,
    pub bone_states_offset: usize,
    pub morph_states_offset: usize,
    pub constraint_states_offset: usize,
    pub outside_parent_states_offset: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmAssetReference {
    pub path: String,
    pub normalized_path: String,
    pub file_name: String,
    pub extension: String,
    pub kind: &'static str,
    pub offset: usize,
    pub offset_end: usize,
    pub confidence: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmSceneAsset {
    pub reference_index: usize,
    pub kind_index: usize,
    pub path: String,
    pub normalized_path: String,
    pub file_name: String,
    pub extension: String,
    pub offset: usize,
    pub offset_end: usize,
    pub confidence: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmParserDiagnostic {
    pub level: &'static str,
    pub code: &'static str,
    pub message: String,
}

pub fn parse_pmm_manifest(data: &[u8]) -> Result<PmmParsedManifest, ImportError> {
    const PREFIX: &[u8] = b"Polygon Movie maker ";
    if !data.starts_with(PREFIX) {
        return Err(ImportError::InvalidMagic { format: "PMM" });
    }
    let version_bytes = &data[PREFIX.len()..data.len().min(32)];
    let version_end = version_bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(version_bytes.len());
    let version = String::from_utf8_lossy(&version_bytes[..version_end])
        .trim()
        .to_owned();
    let parsed_version = version.parse::<u32>().ok();
    let project_settings = parse_project_settings(data);
    let display_state = parse_display_state(data, parsed_version);
    let asset_references = extract_asset_references(data);
    let model_slot_scan = parse_model_slots_from_header(data, &asset_references, &display_state);
    let document_summary = parse_document_summary(data, parsed_version, &asset_references);
    let document_global_summary =
        parse_document_global_summary(data, parsed_version, &asset_references);
    let header_text_entries =
        parse_header_text_entries(data, parsed_version, asset_references.first());
    let model_assets = scene_assets_by_kind(&asset_references, "model");
    let accessory_assets = scene_assets_by_kind(&asset_references, "accessory");
    let motion_assets = scene_assets_by_kind(&asset_references, "motion");
    let audio_assets = scene_assets_by_kind(&asset_references, "audio");
    let image_assets = scene_assets_by_kind(&asset_references, "image");
    let video_assets = scene_assets_by_kind(&asset_references, "video");
    let diagnostics = pmm_diagnostics(
        &asset_references,
        &model_slot_scan.slots,
        document_summary.as_ref(),
        document_global_summary.as_ref(),
        &display_state,
        model_slot_scan.stop.as_ref(),
        data,
    );
    Ok(PmmParsedManifest {
        signature: "Polygon Movie maker",
        version,
        parsed_version,
        byte_length: data.len(),
        timeline: timeline_from_project_settings(&project_settings),
        project_settings,
        display_state,
        header_text_entries,
        model_slots: model_slot_scan.slots,
        document_summary,
        document_global_summary,
        asset_summary: asset_summary(&asset_references),
        model_assets,
        accessory_assets,
        motion_assets,
        audio_assets,
        image_assets,
        video_assets,
        model_paths: paths_by_kind(&asset_references, "model"),
        accessory_paths: paths_by_kind(&asset_references, "accessory"),
        motion_paths: paths_by_kind(&asset_references, "motion"),
        audio_paths: paths_by_kind(&asset_references, "audio"),
        image_paths: paths_by_kind(&asset_references, "image"),
        video_paths: paths_by_kind(&asset_references, "video"),
        diagnostics,
        asset_references,
    })
}

pub fn export_pmm_manifest(manifest: &PmmParsedManifest) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Polygon Movie maker 0002");
    out.resize(30, 0);

    push_u32(
        &mut out,
        manifest.project_settings.screen_width.unwrap_or(640),
    );
    push_u32(
        &mut out,
        manifest.project_settings.screen_height.unwrap_or(480),
    );
    push_u32(
        &mut out,
        manifest
            .project_settings
            .timeline_frame_count
            .or(manifest.timeline.frame_count)
            .unwrap_or(0),
    );
    push_f32(
        &mut out,
        manifest.project_settings.frame_rate.unwrap_or(30.0),
    );

    let mut flags = [0u8; 8];
    for (index, flag) in manifest
        .display_state
        .model_slot_flags
        .iter()
        .take(8)
        .enumerate()
    {
        flags[index] = *flag;
    }
    if flags.iter().all(|flag| *flag == 0) && !manifest.model_slots.is_empty() {
        for slot in &manifest.model_slots {
            if slot.slot_index < flags.len() {
                flags[slot.slot_index] = 1;
            }
        }
    }
    out.extend_from_slice(&flags);

    let document_model_count = manifest
        .display_state
        .document_model_count
        .unwrap_or_else(|| manifest.model_slots.len().min(u8::MAX as usize) as u8);
    out.push(document_model_count);
    out.push(manifest.display_state.accessory_slot_count.unwrap_or(0));

    let mut emitted_paths = Vec::<String>::new();
    for slot in &manifest.model_slots {
        push_pmm_len_prefixed_sjis(&mut out, &slot.name, &slot.name_bytes);
        push_pmm_len_prefixed_sjis(&mut out, &slot.english_name, &slot.english_name_bytes);
        push_pmm_sjis_string(&mut out, &slot.model_path, None);
        out.push(0);
        emitted_paths.push(slot.normalized_path.to_ascii_lowercase());
    }

    for reference in &manifest.asset_references {
        let normalized = reference.normalized_path.to_ascii_lowercase();
        if emitted_paths.iter().any(|path| path == &normalized) {
            continue;
        }
        push_pmm_sjis_string(&mut out, &reference.path, None);
        out.push(0);
        emitted_paths.push(normalized);
    }

    out
}

#[derive(Debug, Clone)]
pub struct PmmSceneExportOptions {
    pub screen_width: u32,
    pub screen_height: u32,
    pub frame_rate: f32,
    pub camera_fov: f32,
}

impl Default for PmmSceneExportOptions {
    fn default() -> Self {
        Self {
            screen_width: 1024,
            screen_height: 1024,
            frame_rate: 30.0,
            camera_fov: 30.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PmmSceneExportReport {
    pub bytes: Vec<u8>,
    pub bone_keyframes: usize,
    pub morph_keyframes: usize,
    pub frame_zero_bone_keyframes: usize,
    pub frame_zero_morph_keyframes: usize,
    pub skipped_bone_keyframes: usize,
    pub skipped_morph_keyframes: usize,
    pub max_frame: u32,
}

#[derive(Debug, Clone)]
struct PmmExportBoneKeyframe {
    bone_index: usize,
    frame: u32,
    translation: [f32; 3],
    rotation: [f32; 4],
    interpolation: [u8; 16],
}

#[derive(Debug, Clone)]
struct PmmExportMorphKeyframe {
    morph_index: usize,
    frame: u32,
    weight: f32,
}

pub fn export_pmm_scene_from_pmx_vmd(
    model: &PmxParsedModel,
    motion: &VmdParsedAnimation,
    model_path: &str,
    options: &PmmSceneExportOptions,
) -> PmmSceneExportReport {
    let bone_names: Vec<&str> = model
        .skeleton
        .bones
        .iter()
        .map(|bone| bone.name.as_str())
        .collect();
    let morph_names: Vec<&str> = model
        .morphs
        .iter()
        .map(|morph| morph.name.as_str())
        .collect();
    let bone_indices: HashMap<&str, usize> = bone_names
        .iter()
        .enumerate()
        .map(|(index, name)| (*name, index))
        .collect();
    let morph_indices: HashMap<&str, usize> = morph_names
        .iter()
        .enumerate()
        .map(|(index, name)| (*name, index))
        .collect();

    let mut deduped_bones = BTreeMap::<(usize, u32), &VmdParsedBoneFrame>::new();
    let mut skipped_bone_keyframes = 0usize;
    for frame in &motion.bone_frames {
        if let Some(&bone_index) = bone_indices.get(frame.bone_name.as_str()) {
            deduped_bones.insert((bone_index, frame.frame), frame);
        } else {
            skipped_bone_keyframes += 1;
        }
    }

    let mut deduped_morphs = BTreeMap::<(usize, u32), &VmdParsedMorphFrame>::new();
    let mut skipped_morph_keyframes = 0usize;
    for frame in &motion.morph_frames {
        if let Some(&morph_index) = morph_indices.get(frame.morph_name.as_str()) {
            deduped_morphs.insert((morph_index, frame.frame), frame);
        } else {
            skipped_morph_keyframes += 1;
        }
    }

    let mut initial_bones = vec![None::<PmmExportBoneKeyframe>; bone_names.len()];
    let mut additional_bones = Vec::<PmmExportBoneKeyframe>::new();
    let mut max_frame = 0u32;
    for ((bone_index, frame_index), frame) in deduped_bones {
        max_frame = max_frame.max(frame_index);
        let keyframe = pmm_export_bone_keyframe(bone_index, frame);
        if frame_index == 0 {
            initial_bones[bone_index] = Some(keyframe);
        } else {
            additional_bones.push(keyframe);
        }
    }
    additional_bones.sort_by_key(|frame| (frame.bone_index, frame.frame));

    let mut initial_morphs = vec![None::<PmmExportMorphKeyframe>; morph_names.len()];
    let mut additional_morphs = Vec::<PmmExportMorphKeyframe>::new();
    for ((morph_index, frame_index), frame) in deduped_morphs {
        max_frame = max_frame.max(frame_index);
        let keyframe = PmmExportMorphKeyframe {
            morph_index,
            frame: frame.frame,
            weight: frame.weight,
        };
        if frame_index == 0 {
            initial_morphs[morph_index] = Some(keyframe);
        } else {
            additional_morphs.push(keyframe);
        }
    }
    additional_morphs.sort_by_key(|frame| (frame.morph_index, frame.frame));

    let frame_zero_bone_keyframes = initial_bones.iter().filter(|frame| frame.is_some()).count();
    let frame_zero_morph_keyframes = initial_morphs
        .iter()
        .filter(|frame| frame.is_some())
        .count();
    let bytes = write_pmm_scene(
        model,
        model_path,
        options,
        &bone_names,
        &morph_names,
        &initial_bones,
        &additional_bones,
        &initial_morphs,
        &additional_morphs,
        max_frame,
    );

    PmmSceneExportReport {
        bytes,
        bone_keyframes: additional_bones.len(),
        morph_keyframes: additional_morphs.len(),
        frame_zero_bone_keyframes,
        frame_zero_morph_keyframes,
        skipped_bone_keyframes,
        skipped_morph_keyframes,
        max_frame,
    }
}

fn pmm_export_bone_keyframe(
    bone_index: usize,
    frame: &VmdParsedBoneFrame,
) -> PmmExportBoneKeyframe {
    PmmExportBoneKeyframe {
        bone_index,
        frame: frame.frame,
        translation: frame.translation,
        rotation: frame.rotation,
        interpolation: pmm_bone_interpolation_from_vmd(&frame.interpolation),
    }
}

fn pmm_bone_interpolation_from_vmd(interpolation: &[u8]) -> [u8; 16] {
    if interpolation.len() < 16 {
        return [20; 16];
    }
    [
        interpolation[0],
        interpolation[4],
        interpolation[8],
        interpolation[12],
        interpolation[1],
        interpolation[5],
        interpolation[9],
        interpolation[13],
        interpolation[2],
        interpolation[6],
        interpolation[10],
        interpolation[14],
        interpolation[3],
        interpolation[7],
        interpolation[11],
        interpolation[15],
    ]
}

#[allow(clippy::too_many_arguments)]
fn write_pmm_scene(
    model: &PmxParsedModel,
    model_path: &str,
    options: &PmmSceneExportOptions,
    bone_names: &[&str],
    morph_names: &[&str],
    initial_bones: &[Option<PmmExportBoneKeyframe>],
    additional_bones: &[PmmExportBoneKeyframe],
    initial_morphs: &[Option<PmmExportMorphKeyframe>],
    additional_morphs: &[PmmExportMorphKeyframe],
    max_frame: u32,
) -> Vec<u8> {
    let mut out = b"Polygon Movie maker 0002".to_vec();
    out.resize(30, 0);
    push_u32(&mut out, options.screen_width);
    push_u32(&mut out, options.screen_height);
    push_u32(&mut out, max_frame);
    push_f32(&mut out, options.frame_rate);
    out.extend_from_slice(&[0, 1, 1, 1, 1, 1, 1]);
    out.push(0);
    out.push(1);

    out.push(0);
    push_pmm_len_prefixed_sjis(&mut out, &model.metadata.name, &[]);
    push_pmm_len_prefixed_sjis(&mut out, &model.metadata.english_name, &[]);
    push_pmm_fixed_sjis(&mut out, model_path, 256);
    out.push(0);

    push_i32(&mut out, bone_names.len() as i32);
    for name in bone_names {
        push_pmm_len_prefixed_sjis(&mut out, name, &[]);
    }
    push_i32(&mut out, morph_names.len() as i32);
    for name in morph_names {
        push_pmm_len_prefixed_sjis(&mut out, name, &[]);
    }

    push_i32(&mut out, 0);
    push_i32(&mut out, 0);
    out.push(0);
    out.push(1);
    push_i32(&mut out, -1);
    for _ in 0..4 {
        push_i32(&mut out, -1);
    }
    out.push(0);
    push_i32(&mut out, 0);
    push_i32(&mut out, max_frame as i32);

    let bone_next =
        next_keyframe_indices(bone_names.len(), bone_names.len(), additional_bones, |f| {
            f.bone_index
        });
    for bone_index in 0..bone_names.len() {
        let fallback = PmmExportBoneKeyframe {
            bone_index,
            frame: 0,
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            interpolation: [20; 16],
        };
        let frame = initial_bones[bone_index].as_ref().unwrap_or(&fallback);
        push_document_bone_keyframe(
            &mut out,
            None,
            0,
            0,
            bone_next[bone_index],
            frame.interpolation,
            frame.translation,
            frame.rotation,
        );
    }
    push_i32(&mut out, additional_bones.len() as i32);
    for (offset, frame) in additional_bones.iter().enumerate() {
        let index = bone_names.len() + offset;
        let previous = previous_keyframe_index(
            additional_bones,
            offset,
            frame.bone_index,
            bone_names.len(),
            |f| f.bone_index,
        );
        let next = next_keyframe_index(
            additional_bones,
            offset,
            frame.bone_index,
            bone_names.len(),
            |f| f.bone_index,
        );
        push_document_bone_keyframe(
            &mut out,
            Some(index as i32),
            frame.frame as i32,
            previous,
            next,
            frame.interpolation,
            frame.translation,
            frame.rotation,
        );
    }

    let morph_next = next_keyframe_indices(
        morph_names.len(),
        morph_names.len(),
        additional_morphs,
        |f| f.morph_index,
    );
    for morph_index in 0..morph_names.len() {
        let weight = initial_morphs[morph_index]
            .as_ref()
            .map(|frame| frame.weight)
            .unwrap_or(0.0);
        push_document_morph_keyframe(&mut out, None, 0, 0, morph_next[morph_index], weight);
    }
    push_i32(&mut out, additional_morphs.len() as i32);
    for (offset, frame) in additional_morphs.iter().enumerate() {
        let index = morph_names.len() + offset;
        let previous = previous_keyframe_index(
            additional_morphs,
            offset,
            frame.morph_index,
            morph_names.len(),
            |f| f.morph_index,
        );
        let next = next_keyframe_index(
            additional_morphs,
            offset,
            frame.morph_index,
            morph_names.len(),
            |f| f.morph_index,
        );
        push_document_morph_keyframe(
            &mut out,
            Some(index as i32),
            frame.frame as i32,
            previous,
            next,
            frame.weight,
        );
    }

    push_document_model_keyframe(&mut out, None, 0, 0, 0, true);
    push_i32(&mut out, 0);

    for _ in bone_names {
        for value in [0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
            push_f32(&mut out, value);
        }
        out.push(0);
        out.push(0);
        out.push(0);
    }
    for _ in morph_names {
        push_f32(&mut out, 0.0);
    }

    out.push(0);
    push_f32(&mut out, 1.0);
    out.push(0);
    out.push(0);

    write_pmm_global_tail(&mut out, max_frame, options.camera_fov);
    push_pmm_sjis_string(&mut out, model_path, None);
    out.push(0);
    out
}

fn next_keyframe_indices<T>(
    count: usize,
    initial_count: usize,
    frames: &[T],
    object_index: impl Fn(&T) -> usize,
) -> Vec<i32> {
    let mut next = vec![0; count];
    for (offset, frame) in frames.iter().enumerate() {
        let target = object_index(frame);
        if target < count && next[target] == 0 {
            next[target] = (initial_count + offset) as i32;
        }
    }
    next
}

fn previous_keyframe_index<T>(
    frames: &[T],
    offset: usize,
    object_index: usize,
    initial_count: usize,
    frame_object_index: impl Fn(&T) -> usize,
) -> i32 {
    if offset > 0 && frame_object_index(&frames[offset - 1]) == object_index {
        (initial_count + offset - 1) as i32
    } else {
        object_index as i32
    }
}

fn next_keyframe_index<T>(
    frames: &[T],
    offset: usize,
    object_index: usize,
    initial_count: usize,
    frame_object_index: impl Fn(&T) -> usize,
) -> i32 {
    if offset + 1 < frames.len() && frame_object_index(&frames[offset + 1]) == object_index {
        (initial_count + offset + 1) as i32
    } else {
        0
    }
}

fn push_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_pmm_fixed_sjis(out: &mut Vec<u8>, text: &str, length: usize) {
    let (encoded, _, _) = SHIFT_JIS.encode(text);
    let encoded = encoded.into_owned();
    let mut bytes = vec![0u8; length];
    let copy_len = encoded.len().min(length);
    bytes[..copy_len].copy_from_slice(&encoded[..copy_len]);
    out.extend_from_slice(&bytes);
}

fn push_empty_pmm_path(out: &mut Vec<u8>) {
    out.extend_from_slice(&[0u8; 256]);
}

#[allow(clippy::too_many_arguments)]
fn push_document_bone_keyframe(
    out: &mut Vec<u8>,
    index: Option<i32>,
    frame: i32,
    previous: i32,
    next: i32,
    interpolation: [u8; 16],
    translation: [f32; 3],
    rotation: [f32; 4],
) {
    if let Some(index) = index {
        push_i32(out, index);
    }
    push_i32(out, frame);
    push_i32(out, previous);
    push_i32(out, next);
    out.extend_from_slice(&interpolation);
    for value in translation {
        push_f32(out, value);
    }
    for value in rotation {
        push_f32(out, value);
    }
    out.push(0);
    out.push(0);
}

fn push_document_morph_keyframe(
    out: &mut Vec<u8>,
    index: Option<i32>,
    frame: i32,
    previous: i32,
    next: i32,
    weight: f32,
) {
    if let Some(index) = index {
        push_i32(out, index);
    }
    push_i32(out, frame);
    push_i32(out, previous);
    push_i32(out, next);
    push_f32(out, weight);
    out.push(0);
}

fn push_document_model_keyframe(
    out: &mut Vec<u8>,
    index: Option<i32>,
    frame: i32,
    previous: i32,
    next: i32,
    visible: bool,
) {
    if let Some(index) = index {
        push_i32(out, index);
    }
    push_i32(out, frame);
    push_i32(out, previous);
    push_i32(out, next);
    out.push(u8::from(visible));
    out.push(0);
}

fn push_document_base_keyframe(
    out: &mut Vec<u8>,
    index: Option<i32>,
    frame: i32,
    previous: i32,
    next: i32,
) {
    if let Some(index) = index {
        push_i32(out, index);
    }
    push_i32(out, frame);
    push_i32(out, previous);
    push_i32(out, next);
}

fn push_document_camera_keyframe(out: &mut Vec<u8>, index: Option<i32>, selected: bool, fov: i32) {
    push_document_base_keyframe(out, index, 0, 0, 0);
    push_f32(out, 45.0);
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(&[0u8; 12]);
    push_i32(out, -1);
    push_i32(out, -1);
    out.extend_from_slice(&[20u8; 24]);
    out.push(0);
    push_i32(out, fov);
    out.push(u8::from(selected));
}

fn push_document_light_keyframe(out: &mut Vec<u8>, index: Option<i32>, selected: bool) {
    push_document_base_keyframe(out, index, 0, 0, 0);
    for value in [0.602f32, 0.602, 0.602, -0.5, -1.0, 0.5] {
        push_f32(out, value);
    }
    out.push(u8::from(selected));
}

fn push_document_gravity_keyframe(out: &mut Vec<u8>, index: Option<i32>, selected: bool) {
    push_document_base_keyframe(out, index, 0, 0, 0);
    out.push(0);
    push_i32(out, 10);
    push_f32(out, 9.8);
    for value in [0.0f32, -1.0, 0.0] {
        push_f32(out, value);
    }
    out.push(u8::from(selected));
}

fn push_document_self_shadow_keyframe(out: &mut Vec<u8>, index: Option<i32>, selected: bool) {
    push_document_base_keyframe(out, index, 0, 0, 0);
    out.push(1);
    push_f32(out, 0.01125);
    out.push(u8::from(selected));
}

fn write_pmm_global_tail(out: &mut Vec<u8>, max_frame: u32, camera_fov: f32) {
    let camera_fov = camera_fov.round().clamp(1.0, i32::MAX as f32) as i32;
    push_document_camera_keyframe(out, None, true, camera_fov);
    push_i32(out, 0);
    out.extend_from_slice(&[0u8; 12 * 3]);
    out.push(0);

    push_document_light_keyframe(out, None, true);
    push_i32(out, 0);
    for value in [0.602f32, 0.602, 0.602, -0.5, -1.0, 0.5] {
        push_f32(out, value);
    }

    out.push(0);
    push_i32(out, 0);
    out.push(0);

    push_i32(out, 0);
    push_i32(out, 0);
    push_i32(out, max_frame as i32);
    push_i32(out, 0);
    out.push(0);
    out.push(0);
    out.push(0);
    out.push(0);
    push_i32(out, 500);
    push_i32(out, 0);
    out.push(0);
    push_empty_pmm_path(out);
    push_i32(out, 0);
    push_i32(out, 0);
    push_f32(out, 0.0);
    push_empty_pmm_path(out);
    push_i32(out, 0);
    push_i32(out, 0);
    push_i32(out, 0);
    push_f32(out, 1.0);
    push_empty_pmm_path(out);
    out.push(0);
    out.push(0);
    out.push(1);
    out.push(1);
    push_f32(out, 60.0);
    push_i32(out, 0);
    push_i32(out, 1);
    push_f32(out, 1.0);
    out.push(1);
    out.push(2);

    push_f32(out, 9.8);
    push_i32(out, 10);
    for value in [0.0f32, -1.0, 0.0] {
        push_f32(out, value);
    }
    out.push(0);
    push_document_gravity_keyframe(out, None, true);
    push_i32(out, 0);

    out.push(1);
    push_f32(out, 0.01125);
    push_document_self_shadow_keyframe(out, None, true);
    push_i32(out, 0);

    push_i32(out, 0);
    push_i32(out, 0);
    push_i32(out, 0);
    out.push(0);
    push_i32(out, -1);
    push_i32(out, -1);
    for value in [
        1.0f32, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ] {
        push_f32(out, value);
    }
    out.push(0);
    out.push(0);
    out.push(1);
    push_i32(out, 0);
    out.push(1);
    out.push(0);
    push_i32(out, 0);
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_pmm_len_prefixed_sjis(out: &mut Vec<u8>, text: &str, original_bytes: &[u8]) {
    let bytes = if !original_bytes.is_empty() {
        original_bytes.to_vec()
    } else {
        let (encoded, _, _) = SHIFT_JIS.encode(text);
        encoded.into_owned()
    };
    let length = bytes.len().min(u8::MAX as usize);
    out.push(length as u8);
    out.extend_from_slice(&bytes[..length]);
}

fn push_pmm_sjis_string(out: &mut Vec<u8>, text: &str, original_bytes: Option<&[u8]>) {
    if let Some(bytes) = original_bytes
        && !bytes.is_empty()
    {
        out.extend_from_slice(bytes);
        return;
    }
    let (encoded, _, _) = SHIFT_JIS.encode(text);
    out.extend_from_slice(&encoded);
}

fn timeline_from_project_settings(settings: &PmmProjectSettings) -> PmmTimeline {
    let duration_seconds = settings
        .timeline_frame_count
        .zip(settings.frame_rate)
        .and_then(|(frame_count, frame_rate)| {
            if frame_rate > 0.0 {
                Some(frame_count as f32 / frame_rate)
            } else {
                None
            }
        });
    PmmTimeline {
        start_frame: settings.timeline_frame_count.map(|_| 0),
        end_frame_exclusive: settings.timeline_frame_count,
        frame_count: settings.timeline_frame_count,
        frame_rate: settings.frame_rate,
        duration_seconds,
    }
}

fn parse_project_settings(data: &[u8]) -> PmmProjectSettings {
    let screen_width = read_u32_at(data, 30).filter(|value| (1..=16_384).contains(value));
    let screen_height = read_u32_at(data, 34).filter(|value| (1..=16_384).contains(value));
    let timeline_frame_count = read_u32_at(data, 38).filter(|value| *value <= 10_000_000);
    let frame_rate =
        read_f32_at(data, 42).filter(|value| value.is_finite() && *value > 0.0 && *value <= 1000.0);

    PmmProjectSettings {
        screen_width,
        screen_height,
        timeline_frame_count,
        frame_rate,
    }
}

fn active_slot_indices(flags: &[u8]) -> Vec<usize> {
    flags
        .iter()
        .enumerate()
        .filter(|&(_, f)| *f != 0)
        .map(|(i, _)| i)
        .collect()
}

fn empty_slot_indices(flags: &[u8]) -> Vec<usize> {
    flags
        .iter()
        .enumerate()
        .filter(|&(_, f)| *f == 0)
        .map(|(i, _)| i)
        .collect()
}

fn slot_flag_counts(flags: &[u8]) -> BTreeMap<u8, usize> {
    let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
    for &flag in flags {
        *counts.entry(flag).or_insert(0) += 1;
    }
    counts
}

fn slot_flag_entries(flags: &[u8]) -> Vec<PmmModelSlotFlagEntry> {
    flags
        .iter()
        .enumerate()
        .map(|(slot_index, flag)| PmmModelSlotFlagEntry {
            slot_index,
            flag: *flag,
            active: *flag != 0,
        })
        .collect()
}

fn document_expand_flags(flags: &[u8]) -> Option<PmmDocumentExpandFlags> {
    Some(PmmDocumentExpandFlags {
        editing_cla: *flags.first()? != 0,
        camera_panel: *flags.get(1)? != 0,
        light_panel: *flags.get(2)? != 0,
        accessory_panel: *flags.get(3)? != 0,
        bone_panel: *flags.get(4)? != 0,
        morph_panel: *flags.get(5)? != 0,
        self_shadow_panel: *flags.get(6)? != 0,
    })
}

fn parse_display_state(data: &[u8], parsed_version: Option<u32>) -> PmmDisplayState {
    if parsed_version == Some(1) {
        if let (Some(model_count), Some(accessory_count)) =
            (data.get(52).copied(), data.get(53).copied())
            && model_count <= 64
            && accessory_count <= 64
        {
            return PmmDisplayState {
                layout: "pmm-v1-counts",
                model_slot_flags: Vec::new(),
                model_slot_flag_entries: Vec::new(),
                document_expand_flags: None,
                selected_model_index: None,
                document_model_count: None,
                declared_model_slot_count: Some(model_count),
                model_slot_count: model_count as usize,
                non_zero_model_slot_count: model_count as usize,
                accessory_slot_count: Some(accessory_count),
                active_model_slot_indices: Vec::new(),
                empty_model_slot_indices: Vec::new(),
                model_slot_flag_counts: BTreeMap::new(),
            };
        }
        return unknown_display_state();
    }

    let model_slot_flags = data
        .get(46..54)
        .filter(|bytes| bytes.iter().all(|byte| *byte <= 2))
        .map(|bytes| bytes.to_vec());
    let accessory_slot_count = None;
    let document_expand_flags = model_slot_flags
        .as_ref()
        .and_then(|flags| document_expand_flags(flags));
    let selected_model_index = model_slot_flags
        .as_ref()
        .and_then(|_| data.get(53).copied());
    let document_model_count = model_slot_flags
        .as_ref()
        .and_then(|_| data.get(54).copied());

    let model_slot_flags = model_slot_flags.unwrap_or_default();
    let model_slot_count = model_slot_flags.len();
    let non_zero_model_slot_count = model_slot_flags.iter().filter(|flag| **flag != 0).count();
    let active_model_slot_indices = active_slot_indices(&model_slot_flags);
    let empty_model_slot_indices = empty_slot_indices(&model_slot_flags);
    let model_slot_flag_counts = slot_flag_counts(&model_slot_flags);
    let model_slot_flag_entries = slot_flag_entries(&model_slot_flags);
    PmmDisplayState {
        layout: if model_slot_flags.is_empty() {
            "unknown"
        } else {
            "pmm-v2-flags"
        },
        declared_model_slot_count: if model_slot_flags.is_empty() {
            None
        } else {
            Some(model_slot_count as u8)
        },
        model_slot_count,
        non_zero_model_slot_count,
        model_slot_flags,
        model_slot_flag_entries,
        document_expand_flags,
        selected_model_index,
        document_model_count,
        accessory_slot_count,
        active_model_slot_indices,
        empty_model_slot_indices,
        model_slot_flag_counts,
    }
}

fn unknown_display_state() -> PmmDisplayState {
    PmmDisplayState {
        layout: "unknown",
        model_slot_flags: Vec::new(),
        model_slot_flag_entries: Vec::new(),
        document_expand_flags: None,
        selected_model_index: None,
        document_model_count: None,
        declared_model_slot_count: None,
        model_slot_count: 0,
        non_zero_model_slot_count: 0,
        accessory_slot_count: None,
        active_model_slot_indices: Vec::new(),
        empty_model_slot_indices: Vec::new(),
        model_slot_flag_counts: BTreeMap::new(),
    }
}

fn parse_header_text_entries(
    data: &[u8],
    parsed_version: Option<u32>,
    first_reference: Option<&PmmAssetReference>,
) -> Vec<PmmHeaderTextEntry> {
    if parsed_version != Some(1) {
        return Vec::new();
    }
    let start = 54usize;
    let Some(first_reference) = first_reference else {
        return Vec::new();
    };
    let end = first_reference.offset.min(data.len());
    if end <= start {
        return Vec::new();
    }

    let mut entries = Vec::new();
    let mut chunk_start = start;
    for index in start..end {
        if data[index] != 0 {
            continue;
        }
        if index > chunk_start {
            let bytes = &data[chunk_start..index];
            let (decoded, _, _) = SHIFT_JIS.decode(bytes);
            let text = decoded.trim().to_owned();
            if !text.is_empty() {
                entries.push(PmmHeaderTextEntry {
                    index: entries.len(),
                    offset: chunk_start,
                    offset_end: index,
                    text,
                    text_bytes: bytes.to_vec(),
                });
            }
        }
        chunk_start = index + 1;
    }
    entries
}

fn read_u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
}

fn read_f32_at(data: &[u8], offset: usize) -> Option<f32> {
    data.get(offset..offset + 4)
        .and_then(|bytes| bytes.try_into().ok())
        .map(f32::from_le_bytes)
}

struct PmmDocumentCursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> PmmDocumentCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn read_u8(&mut self) -> Option<u8> {
        let value = *self.data.get(self.offset)?;
        self.offset += 1;
        Some(value)
    }

    fn read_bool(&mut self) -> Option<bool> {
        Some(self.read_u8()? != 0)
    }

    fn read_i32(&mut self) -> Option<i32> {
        let bytes: [u8; 4] = self
            .data
            .get(self.offset..self.offset + 4)?
            .try_into()
            .ok()?;
        self.offset += 4;
        Some(i32::from_le_bytes(bytes))
    }

    fn read_f32(&mut self) -> Option<f32> {
        let bytes: [u8; 4] = self
            .data
            .get(self.offset..self.offset + 4)?
            .try_into()
            .ok()?;
        self.offset += 4;
        Some(f32::from_le_bytes(bytes))
    }

    fn read_f32x3(&mut self) -> Option<[f32; 3]> {
        Some([self.read_f32()?, self.read_f32()?, self.read_f32()?])
    }

    fn read_f32x4(&mut self) -> Option<[f32; 4]> {
        Some([
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
            self.read_f32()?,
        ])
    }

    fn read_bytes16(&mut self) -> Option<[u8; 16]> {
        let bytes: [u8; 16] = self
            .data
            .get(self.offset..self.offset + 16)?
            .try_into()
            .ok()?;
        self.offset += 16;
        Some(bytes)
    }

    fn read_variable_string(&mut self) -> Option<String> {
        let length = self.read_u8()? as usize;
        let bytes = self.data.get(self.offset..self.offset + length)?;
        self.offset += length;
        Some(decode_shift_jis(bytes))
    }

    fn read_fixed_string(&mut self, length: usize) -> Option<String> {
        let bytes = self.data.get(self.offset..self.offset + length)?;
        self.offset += length;
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Some(decode_shift_jis(&bytes[..end]))
    }

    fn skip(&mut self, length: usize) -> Option<()> {
        self.data.get(self.offset..self.offset + length)?;
        self.offset += length;
        Some(())
    }
}

fn parse_document_summary(
    data: &[u8],
    parsed_version: Option<u32>,
    references: &[PmmAssetReference],
) -> Option<PmmDocumentSummary> {
    if parsed_version != Some(2) {
        return None;
    }
    let mut cursor = PmmDocumentCursor::new(data);
    cursor.skip(30)?;
    cursor.read_i32()?;
    cursor.read_i32()?;
    cursor.read_i32()?;
    cursor.read_f32()?;
    cursor.skip(7)?;
    let selected_model_index = cursor.read_u8()?;
    let model_count = cursor.read_u8()? as usize;
    if model_count == 0 || selected_model_index as usize >= model_count {
        return None;
    }

    let mut models = Vec::with_capacity(model_count);
    for slot_index in 0..model_count {
        models.push(read_document_model_summary(
            &mut cursor,
            slot_index,
            references,
        )?);
    }
    let counts = summarize_document_counts(&models);

    Some(PmmDocumentSummary {
        source: "nanoem/ext/document.c PMMv2 layout",
        selected_model_index,
        model_count,
        counts,
        models,
    })
}

fn parse_document_global_summary(
    data: &[u8],
    parsed_version: Option<u32>,
    references: &[PmmAssetReference],
) -> Option<PmmDocumentGlobalSummary> {
    if parsed_version != Some(2) {
        return None;
    }
    let mut cursor = PmmDocumentCursor::new(data);
    cursor.skip(30)?;
    cursor.read_i32()?;
    cursor.read_i32()?;
    cursor.read_i32()?;
    cursor.read_f32()?;
    cursor.skip(7)?;
    let selected_model_index = cursor.read_u8()?;
    let model_count = cursor.read_u8()? as usize;
    if model_count == 0 || selected_model_index as usize >= model_count {
        return None;
    }
    for slot_index in 0..model_count {
        read_document_model_summary(&mut cursor, slot_index, references)?;
    }

    let offset = cursor.offset;
    let camera = read_document_camera_summary(&mut cursor)?;
    let light = read_document_light_summary(&mut cursor)?;
    let accessories = read_document_accessory_block_summary(&mut cursor, references)?;
    let settings_before_gravity = read_document_settings_before_gravity(&mut cursor)?;
    let gravity = read_document_gravity_summary(&mut cursor)?;
    let self_shadow = read_document_self_shadow_summary(&mut cursor)?;
    let settings =
        finish_document_settings_summary(settings_before_gravity, &mut cursor, model_count)?;
    let offset_end = cursor.offset;

    Some(PmmDocumentGlobalSummary {
        source: "nanoem/ext/document.c PMMv2 global layout",
        offset,
        offset_end,
        camera,
        light,
        accessories,
        settings,
        gravity,
        self_shadow,
    })
}

fn read_document_model_summary(
    cursor: &mut PmmDocumentCursor<'_>,
    slot_index: usize,
    references: &[PmmAssetReference],
) -> Option<PmmDocumentModelSummary> {
    const PMM_PATH_BYTE_LENGTH: usize = 256;
    let offset = cursor.offset;
    let document_model_index = cursor.read_u8()?;
    let name = cursor.read_variable_string()?;
    let english_name = cursor.read_variable_string()?;
    let path_offset = cursor.offset;
    let path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let asset_reference_index = asset_reference_index_for_path(references, "model", &path);
    cursor.read_u8()?;
    let bone_count = usize_from_i32(cursor.read_i32()?)?;
    for _ in 0..bone_count {
        cursor.read_variable_string()?;
    }
    let morph_count = usize_from_i32(cursor.read_i32()?)?;
    for _ in 0..morph_count {
        cursor.read_variable_string()?;
    }
    let constraint_bone_count = usize_from_i32(cursor.read_i32()?)?;
    cursor.skip(constraint_bone_count.checked_mul(4)?)?;
    let outside_parent_subject_bone_count = usize_from_i32(cursor.read_i32()?)?;
    cursor.skip(outside_parent_subject_bone_count.checked_mul(4)?)?;
    let draw_order_index = cursor.read_u8()?;
    let visible = cursor.read_bool()?;
    let selected_bone_index = cursor.read_i32()?;
    let selected_morph_indices = [
        cursor.read_i32()?,
        cursor.read_i32()?,
        cursor.read_i32()?,
        cursor.read_i32()?,
    ];
    let expansion_state_count = cursor.read_u8()? as usize;
    cursor.skip(expansion_state_count)?;
    let vertical_scroll = cursor.read_i32()?;
    let last_frame_index = cursor.read_i32()?;

    let initial_bone_keyframes_offset = cursor.offset;
    let initial_bone_keyframes = bone_count;
    let mut initial_bone_keyframe_summaries = Vec::with_capacity(initial_bone_keyframes);
    for _ in 0..initial_bone_keyframes {
        initial_bone_keyframe_summaries.push(read_document_bone_keyframe(cursor, false)?);
    }
    let bone_keyframe_count_offset = cursor.offset;
    let bone_keyframes = usize_from_i32(cursor.read_i32()?)?;
    let bone_keyframes_offset = cursor.offset;
    let mut bone_keyframe_summaries = Vec::with_capacity(bone_keyframes);
    for _ in 0..bone_keyframes {
        bone_keyframe_summaries.push(read_document_bone_keyframe(cursor, true)?);
    }
    let bone_keyframes_end_offset = cursor.offset;

    let initial_morph_keyframes_offset = cursor.offset;
    let initial_morph_keyframes = morph_count;
    let mut initial_morph_keyframe_summaries = Vec::with_capacity(initial_morph_keyframes);
    for _ in 0..initial_morph_keyframes {
        initial_morph_keyframe_summaries.push(read_document_morph_keyframe(cursor, false)?);
    }
    let morph_keyframe_count_offset = cursor.offset;
    let morph_keyframes = usize_from_i32(cursor.read_i32()?)?;
    let morph_keyframes_offset = cursor.offset;
    let mut morph_keyframe_summaries = Vec::with_capacity(morph_keyframes);
    for _ in 0..morph_keyframes {
        morph_keyframe_summaries.push(read_document_morph_keyframe(cursor, true)?);
    }
    let morph_keyframes_end_offset = cursor.offset;

    let initial_model_keyframes = 1;
    let initial_model_keyframe_offset = cursor.offset;
    let initial_model_keyframe = read_document_model_keyframe(
        cursor,
        false,
        constraint_bone_count,
        outside_parent_subject_bone_count,
    )?;
    let model_keyframe_count_offset = cursor.offset;
    let model_keyframes = usize_from_i32(cursor.read_i32()?)?;
    let model_keyframes_offset = cursor.offset;
    let mut model_keyframe_summaries = Vec::with_capacity(model_keyframes);
    for _ in 0..model_keyframes {
        model_keyframe_summaries.push(read_document_model_keyframe(
            cursor,
            true,
            constraint_bone_count,
            outside_parent_subject_bone_count,
        )?);
    }
    let model_keyframes_end_offset = cursor.offset;

    let bone_states_offset = cursor.offset;
    let mut bone_state_summaries = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        bone_state_summaries.push(read_document_bone_state(cursor)?);
    }
    let morph_states_offset = cursor.offset;
    let mut morph_state_summaries = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        morph_state_summaries.push(read_document_morph_state(cursor)?);
    }
    let constraint_states_offset = cursor.offset;
    let mut constraint_state_summaries = Vec::with_capacity(constraint_bone_count);
    for _ in 0..constraint_bone_count {
        constraint_state_summaries.push(read_document_constraint_state(cursor)?);
    }
    let outside_parent_states_offset = cursor.offset;
    let mut outside_parent_state_summaries = Vec::with_capacity(outside_parent_subject_bone_count);
    for _ in 0..outside_parent_subject_bone_count {
        outside_parent_state_summaries.push(read_document_outside_parent_state(cursor)?);
    }

    let blend_enabled = cursor.read_bool()?;
    let edge_width = cursor.read_f32()?;
    let self_shadow_enabled = cursor.read_bool()?;
    let transform_order_index = cursor.read_u8()?;
    let offset_end = cursor.offset;

    Some(PmmDocumentModelSummary {
        slot_index,
        document_model_index,
        offset,
        offset_end,
        path_offset,
        name,
        english_name,
        path,
        asset_reference_index,
        bone_count,
        morph_count,
        constraint_bone_count,
        outside_parent_subject_bone_count,
        draw_order_index,
        transform_order_index,
        selected_bone_index,
        selected_morph_indices,
        vertical_scroll,
        sections: PmmDocumentModelSections {
            initial_bone_keyframes_offset,
            bone_keyframe_count_offset,
            bone_keyframes_offset,
            bone_keyframes_end_offset,
            initial_morph_keyframes_offset,
            morph_keyframe_count_offset,
            morph_keyframes_offset,
            morph_keyframes_end_offset,
            initial_model_keyframe_offset,
            model_keyframe_count_offset,
            model_keyframes_offset,
            model_keyframes_end_offset,
            bone_states_offset,
            morph_states_offset,
            constraint_states_offset,
            outside_parent_states_offset,
        },
        initial_bone_keyframes,
        initial_bone_keyframe_summaries,
        bone_keyframes,
        bone_keyframe_summaries,
        initial_morph_keyframes,
        initial_morph_keyframe_summaries,
        morph_keyframes,
        morph_keyframe_summaries,
        initial_model_keyframes,
        model_keyframes,
        initial_model_keyframe,
        model_keyframe_summaries,
        last_frame_index,
        visible,
        blend_enabled,
        edge_width,
        self_shadow_enabled,
        bone_state_summaries,
        morph_state_summaries,
        constraint_state_summaries,
        outside_parent_state_summaries,
    })
}

fn read_document_bone_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentBoneKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let interpolation = cursor.read_bytes16()?;
    let translation = cursor.read_f32x3()?;
    let orientation = cursor.read_f32x4()?;
    let physics_disabled = cursor.read_bool()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentBoneKeyframeSummary {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        interpolation,
        translation,
        orientation,
        physics_disabled,
        selected,
    })
}

fn read_document_morph_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentMorphKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let weight = cursor.read_f32()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentMorphKeyframeSummary {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        weight,
        selected,
    })
}

fn read_document_model_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
    constraint_bone_count: usize,
    outside_parent_subject_bone_count: usize,
) -> Option<PmmDocumentModelKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let visible = cursor.read_bool()?;
    let mut constraint_states = Vec::with_capacity(constraint_bone_count);
    for _ in 0..constraint_bone_count {
        constraint_states.push(cursor.read_bool()?);
    }
    let mut outside_parent_indices = Vec::with_capacity(outside_parent_subject_bone_count);
    for _ in 0..outside_parent_subject_bone_count {
        outside_parent_indices.push(PmmDocumentOutsideParentIndexSummary {
            parent_model_index: cursor.read_i32()?,
            parent_model_bone_index: cursor.read_i32()?,
        });
    }
    let self_shadow_enabled = cursor.read_bool()?;
    Some(PmmDocumentModelKeyframeSummary {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        visible,
        constraint_states,
        outside_parent_indices,
        self_shadow_enabled,
    })
}

fn read_document_camera_summary(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentTrackSummary> {
    let offset = cursor.offset;
    let initial_keyframe = read_document_camera_keyframe(cursor, false)?;
    let keyframe_count_offset = cursor.offset;
    let keyframes = usize_from_i32(cursor.read_i32()?)?;
    let keyframes_offset = cursor.offset;
    let mut keyframe_summaries = Vec::with_capacity(keyframes);
    for _ in 0..keyframes {
        keyframe_summaries.push(read_document_camera_keyframe(cursor, true)?);
    }
    let keyframes_end_offset = cursor.offset;
    let state_offset = cursor.offset;
    cursor.skip(12 * 3)?;
    cursor.read_bool()?;
    let state_end_offset = cursor.offset;
    Some(PmmDocumentTrackSummary {
        offset,
        offset_end: state_end_offset,
        initial_keyframes: 1,
        keyframes,
        initial_keyframe: Some(initial_keyframe),
        keyframe_summaries,
        keyframe_count_offset,
        keyframes_offset,
        keyframes_end_offset,
        state_offset: Some(state_offset),
        state_end_offset: Some(state_end_offset),
    })
}

fn read_document_camera_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let distance = cursor.read_f32()?;
    let look_at = cursor.read_f32x3()?;
    let angle = cursor.read_f32x3()?;
    let parent_model_index = cursor.read_i32()?;
    let parent_model_bone_index = cursor.read_i32()?;
    cursor.skip(6 * 4)?;
    let perspective_view = !cursor.read_bool()?;
    let fov = cursor.read_i32()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentKeyframeSummary::Camera {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        distance,
        look_at,
        angle,
        parent_model_index,
        parent_model_bone_index,
        perspective_view,
        fov,
        selected,
    })
}

fn read_document_light_summary(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentTrackSummary> {
    let offset = cursor.offset;
    let initial_keyframe = read_document_light_keyframe(cursor, false)?;
    let keyframe_count_offset = cursor.offset;
    let keyframes = usize_from_i32(cursor.read_i32()?)?;
    let keyframes_offset = cursor.offset;
    let mut keyframe_summaries = Vec::with_capacity(keyframes);
    for _ in 0..keyframes {
        keyframe_summaries.push(read_document_light_keyframe(cursor, true)?);
    }
    let keyframes_end_offset = cursor.offset;
    let state_offset = cursor.offset;
    cursor.skip(12)?;
    cursor.skip(12)?;
    let state_end_offset = cursor.offset;
    Some(PmmDocumentTrackSummary {
        offset,
        offset_end: state_end_offset,
        initial_keyframes: 1,
        keyframes,
        initial_keyframe: Some(initial_keyframe),
        keyframe_summaries,
        keyframe_count_offset,
        keyframes_offset,
        keyframes_end_offset,
        state_offset: Some(state_offset),
        state_end_offset: Some(state_end_offset),
    })
}

fn read_document_light_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let color = cursor.read_f32x3()?;
    let direction = cursor.read_f32x3()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentKeyframeSummary::Light {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        color,
        direction,
        selected,
    })
}

fn read_document_accessory_block_summary(
    cursor: &mut PmmDocumentCursor<'_>,
    references: &[PmmAssetReference],
) -> Option<PmmDocumentAccessoryBlockSummary> {
    let offset = cursor.offset;
    let selected_accessory_index = cursor.read_u8()?;
    let horizontal_scroll = cursor.read_i32()?;
    let accessory_count = cursor.read_u8()? as usize;
    let mut accessories = Vec::with_capacity(accessory_count);
    for slot_index in 0..accessory_count {
        accessories.push(read_document_accessory_summary(
            cursor, slot_index, references,
        )?);
    }
    let offset_end = cursor.offset;
    let keyframes = accessories
        .iter()
        .map(|accessory| accessory.keyframes)
        .sum();
    Some(PmmDocumentAccessoryBlockSummary {
        offset,
        offset_end,
        selected_accessory_index,
        horizontal_scroll,
        accessory_count,
        keyframes,
        accessories,
    })
}

fn read_document_accessory_summary(
    cursor: &mut PmmDocumentCursor<'_>,
    slot_index: usize,
    references: &[PmmAssetReference],
) -> Option<PmmDocumentAccessorySummary> {
    const PMM_ACCESSORY_NAME_BYTE_LENGTH: usize = 100;
    const PMM_PATH_BYTE_LENGTH: usize = 256;
    let offset = cursor.offset;
    let document_accessory_index = cursor.read_u8()?;
    let name = cursor.read_fixed_string(PMM_ACCESSORY_NAME_BYTE_LENGTH)?;
    let path_offset = cursor.offset;
    let path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let asset_reference_index = asset_reference_index_for_path(references, "accessory", &path);
    let draw_order_index = cursor.read_u8()?;
    let initial_keyframe = read_document_accessory_keyframe(cursor, false)?;
    let keyframe_count_offset = cursor.offset;
    let keyframes = usize_from_i32(cursor.read_i32()?)?;
    let keyframes_offset = cursor.offset;
    let mut keyframe_summaries = Vec::with_capacity(keyframes);
    for _ in 0..keyframes {
        keyframe_summaries.push(read_document_accessory_keyframe(cursor, true)?);
    }
    let keyframes_end_offset = cursor.offset;

    let state_offset = cursor.offset;
    let (opacity, visible) = unpack_document_accessory_opacity_and_visible(cursor.read_u8()?);
    let parent_model_index = cursor.read_i32()?;
    let parent_model_bone_index = cursor.read_i32()?;
    cursor.skip(12)?;
    let scale_factor = cursor.read_f32()?;
    cursor.skip(12)?;
    let shadow_enabled = cursor.read_bool()?;
    let add_blend_enabled = cursor.read_bool()?;
    let state_end_offset = cursor.offset;
    let offset_end = cursor.offset;

    Some(PmmDocumentAccessorySummary {
        slot_index,
        document_accessory_index,
        offset,
        offset_end,
        path_offset,
        name,
        path,
        asset_reference_index,
        draw_order_index,
        keyframes,
        initial_keyframe,
        keyframe_summaries,
        keyframe_count_offset,
        keyframes_offset,
        keyframes_end_offset,
        state_offset,
        state_end_offset,
        visible,
        opacity,
        parent_model_index,
        parent_model_bone_index,
        scale_factor,
        shadow_enabled,
        add_blend_enabled,
    })
}

fn read_document_accessory_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentAccessoryKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let (opacity, visible) = unpack_document_accessory_opacity_and_visible(cursor.read_u8()?);
    let parent_model_index = cursor.read_i32()?;
    let parent_model_bone_index = cursor.read_i32()?;
    let translation = cursor.read_f32x3()?;
    let orientation = cursor.read_f32x3()?;
    let scale_factor = cursor.read_f32()?;
    let shadow_enabled = cursor.read_bool()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentAccessoryKeyframeSummary {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        visible,
        opacity,
        parent_model_index,
        parent_model_bone_index,
        translation,
        orientation,
        scale_factor,
        shadow_enabled,
        selected,
    })
}

struct PmmDocumentSettingsBeforeGravity {
    offset: usize,
    current_frame_index: i32,
    horizontal_scroll: i32,
    horizontal_scroll_thumb: i32,
    editing_mode: i32,
    camera_look_mode: u8,
    loop_enabled: bool,
    begin_frame_index_enabled: bool,
    end_frame_index_enabled: bool,
    begin_frame_index: i32,
    end_frame_index: i32,
    audio_enabled: bool,
    audio_path: String,
    background_video_offset: [i32; 2],
    background_video_scale_factor: f32,
    background_video_path: String,
    background_video_enabled: bool,
    background_image_offset: [i32; 2],
    background_image_scale_factor: f32,
    background_image_path: String,
    background_image_enabled: bool,
    information_shown: bool,
    grid_and_axis_shown: bool,
    ground_shadow_shown: bool,
    preferred_fps: f32,
    screen_capture_mode: i32,
    accessory_index_after_models: i32,
    ground_shadow_brightness: f32,
    translucent_ground_shadow_enabled: bool,
    physics_simulation_mode: u8,
}

fn read_document_settings_before_gravity(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentSettingsBeforeGravity> {
    const PMM_PATH_BYTE_LENGTH: usize = 256;
    let offset = cursor.offset;
    let current_frame_index = cursor.read_i32()?;
    let horizontal_scroll = cursor.read_i32()?;
    let horizontal_scroll_thumb = cursor.read_i32()?;
    let editing_mode = cursor.read_i32()?;
    let camera_look_mode = cursor.read_u8()?;
    let loop_enabled = cursor.read_bool()?;
    let begin_frame_index_enabled = cursor.read_bool()?;
    let end_frame_index_enabled = cursor.read_bool()?;
    let begin_frame_index = cursor.read_i32()?;
    let end_frame_index = cursor.read_i32()?;
    let audio_enabled = cursor.read_bool()?;
    let audio_path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let background_video_offset = [cursor.read_i32()?, cursor.read_i32()?];
    let background_video_scale_factor = cursor.read_f32()?;
    let background_video_path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let background_video_enabled = cursor.read_i32()? != 0;
    let background_image_offset = [cursor.read_i32()?, cursor.read_i32()?];
    let background_image_scale_factor = cursor.read_f32()?;
    let background_image_path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let background_image_enabled = cursor.read_bool()?;
    let information_shown = cursor.read_bool()?;
    let grid_and_axis_shown = cursor.read_bool()?;
    let ground_shadow_shown = cursor.read_bool()?;
    let preferred_fps = cursor.read_f32()?;
    let screen_capture_mode = cursor.read_i32()?;
    let accessory_index_after_models = cursor.read_i32()?;
    let ground_shadow_brightness = cursor.read_f32()?;
    let translucent_ground_shadow_enabled = cursor.read_bool()?;
    let physics_simulation_mode = cursor.read_u8()?;

    Some(PmmDocumentSettingsBeforeGravity {
        offset,
        current_frame_index,
        horizontal_scroll,
        horizontal_scroll_thumb,
        editing_mode,
        camera_look_mode,
        loop_enabled,
        begin_frame_index_enabled,
        end_frame_index_enabled,
        begin_frame_index,
        end_frame_index,
        audio_enabled,
        audio_path,
        background_video_offset,
        background_video_scale_factor,
        background_video_path,
        background_video_enabled,
        background_image_offset,
        background_image_scale_factor,
        background_image_path,
        background_image_enabled,
        information_shown,
        grid_and_axis_shown,
        ground_shadow_shown,
        preferred_fps,
        screen_capture_mode,
        accessory_index_after_models,
        ground_shadow_brightness,
        translucent_ground_shadow_enabled,
        physics_simulation_mode,
    })
}

fn read_document_gravity_summary(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentTrackSummary> {
    let offset = cursor.offset;
    let state_offset = cursor.offset;
    cursor.read_f32()?;
    cursor.read_i32()?;
    cursor.skip(12)?;
    cursor.read_bool()?;
    let state_end_offset = cursor.offset;
    let initial_keyframe = read_document_gravity_keyframe(cursor, false)?;
    let keyframe_count_offset = cursor.offset;
    let keyframes = usize_from_i32(cursor.read_i32()?)?;
    let keyframes_offset = cursor.offset;
    let mut keyframe_summaries = Vec::with_capacity(keyframes);
    for _ in 0..keyframes {
        keyframe_summaries.push(read_document_gravity_keyframe(cursor, true)?);
    }
    let keyframes_end_offset = cursor.offset;
    Some(PmmDocumentTrackSummary {
        offset,
        offset_end: keyframes_end_offset,
        initial_keyframes: 1,
        keyframes,
        initial_keyframe: Some(initial_keyframe),
        keyframe_summaries,
        keyframe_count_offset,
        keyframes_offset,
        keyframes_end_offset,
        state_offset: Some(state_offset),
        state_end_offset: Some(state_end_offset),
    })
}

fn read_document_gravity_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let noise_enabled = cursor.read_bool()?;
    let noise = cursor.read_i32()?;
    let acceleration = cursor.read_f32()?;
    let direction = cursor.read_f32x3()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentKeyframeSummary::Gravity {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        noise_enabled,
        noise,
        acceleration,
        direction,
        selected,
    })
}

fn read_document_self_shadow_summary(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentTrackSummary> {
    let offset = cursor.offset;
    let state_offset = cursor.offset;
    cursor.read_bool()?;
    cursor.read_f32()?;
    let state_end_offset = cursor.offset;
    let initial_keyframe = read_document_self_shadow_keyframe(cursor, false)?;
    let keyframe_count_offset = cursor.offset;
    let keyframes = usize_from_i32(cursor.read_i32()?)?;
    let keyframes_offset = cursor.offset;
    let mut keyframe_summaries = Vec::with_capacity(keyframes);
    for _ in 0..keyframes {
        keyframe_summaries.push(read_document_self_shadow_keyframe(cursor, true)?);
    }
    let keyframes_end_offset = cursor.offset;
    Some(PmmDocumentTrackSummary {
        offset,
        offset_end: keyframes_end_offset,
        initial_keyframes: 1,
        keyframes,
        initial_keyframe: Some(initial_keyframe),
        keyframe_summaries,
        keyframe_count_offset,
        keyframes_offset,
        keyframes_end_offset,
        state_offset: Some(state_offset),
        state_end_offset: Some(state_end_offset),
    })
}

fn read_document_self_shadow_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let base = read_document_base_keyframe(cursor, include_index)?;
    let mode = cursor.read_u8()?;
    let distance = cursor.read_f32()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentKeyframeSummary::SelfShadow {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        mode,
        distance,
        selected,
    })
}

fn finish_document_settings_summary(
    before_gravity: PmmDocumentSettingsBeforeGravity,
    cursor: &mut PmmDocumentCursor<'_>,
    model_count: usize,
) -> Option<PmmDocumentSettingsSummary> {
    let edge_color = [
        cursor.read_i32()? as f32 / 255.0,
        cursor.read_i32()? as f32 / 255.0,
        cursor.read_i32()? as f32 / 255.0,
    ];
    let black_background_enabled = cursor.read_bool()?;
    let camera_look_at_model_index = cursor.read_i32()?;
    let camera_look_at_model_bone_index = cursor.read_i32()?;
    let unknown_matrix_offset = cursor.offset;
    cursor.skip(16 * 4)?;
    let unknown_matrix_end_offset = cursor.offset;
    let following_look_at_enabled = cursor.read_bool()?;
    cursor.read_bool()?;
    let physics_ground_enabled = cursor.read_bool()?;
    let current_frame_index_in_text_field = cursor.read_i32()?;

    let mut model_selection_footer_present = false;
    let mut model_selection_footer_offset = None;
    let mut model_selection_footer_end_offset = None;
    if cursor.offset < cursor.data.len() {
        let footer_offset = cursor.offset;
        if cursor.read_u8()? != 0 {
            model_selection_footer_present = true;
            model_selection_footer_offset = Some(footer_offset);
            for _ in 0..model_count {
                cursor.read_u8()?;
                cursor.read_i32()?;
            }
            model_selection_footer_end_offset = Some(cursor.offset);
        }
    }
    let offset_end = cursor.offset;

    Some(PmmDocumentSettingsSummary {
        offset: before_gravity.offset,
        offset_end,
        current_frame_index: before_gravity.current_frame_index,
        horizontal_scroll: before_gravity.horizontal_scroll,
        horizontal_scroll_thumb: before_gravity.horizontal_scroll_thumb,
        editing_mode: before_gravity.editing_mode,
        camera_look_mode: before_gravity.camera_look_mode,
        loop_enabled: before_gravity.loop_enabled,
        begin_frame_index_enabled: before_gravity.begin_frame_index_enabled,
        end_frame_index_enabled: before_gravity.end_frame_index_enabled,
        begin_frame_index: before_gravity.begin_frame_index,
        end_frame_index: before_gravity.end_frame_index,
        audio_enabled: before_gravity.audio_enabled,
        audio_path: before_gravity.audio_path,
        background_video_offset: before_gravity.background_video_offset,
        background_video_scale_factor: before_gravity.background_video_scale_factor,
        background_video_path: before_gravity.background_video_path,
        background_video_enabled: before_gravity.background_video_enabled,
        background_image_offset: before_gravity.background_image_offset,
        background_image_scale_factor: before_gravity.background_image_scale_factor,
        background_image_path: before_gravity.background_image_path,
        background_image_enabled: before_gravity.background_image_enabled,
        information_shown: before_gravity.information_shown,
        grid_and_axis_shown: before_gravity.grid_and_axis_shown,
        ground_shadow_shown: before_gravity.ground_shadow_shown,
        preferred_fps: before_gravity.preferred_fps,
        screen_capture_mode: before_gravity.screen_capture_mode,
        accessory_index_after_models: before_gravity.accessory_index_after_models,
        ground_shadow_brightness: before_gravity.ground_shadow_brightness,
        translucent_ground_shadow_enabled: before_gravity.translucent_ground_shadow_enabled,
        physics_simulation_mode: before_gravity.physics_simulation_mode,
        edge_color,
        black_background_enabled,
        camera_look_at_model_index,
        camera_look_at_model_bone_index,
        unknown_matrix_offset,
        unknown_matrix_end_offset,
        following_look_at_enabled,
        physics_ground_enabled,
        current_frame_index_in_text_field,
        model_selection_footer_present,
        model_selection_footer_offset,
        model_selection_footer_end_offset,
    })
}

fn read_document_bone_state(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentBoneStateSummary> {
    let translation = cursor.read_f32x3()?;
    let orientation = cursor.read_f32x4()?;
    let dirty = cursor.read_bool()?;
    let physics_disabled = cursor.read_bool()?;
    let selected = cursor.read_bool()?;
    Some(PmmDocumentBoneStateSummary {
        translation,
        orientation,
        dirty,
        physics_disabled,
        selected,
    })
}

fn read_document_morph_state(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentMorphStateSummary> {
    let weight = cursor.read_f32()?;
    Some(PmmDocumentMorphStateSummary { weight })
}

fn read_document_constraint_state(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentConstraintStateSummary> {
    let enabled = cursor.read_bool()?;
    Some(PmmDocumentConstraintStateSummary { enabled })
}

fn read_document_outside_parent_state(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentOutsideParentStateSummary> {
    let parent_model_index = cursor.read_i32()?;
    let parent_model_bone_index = cursor.read_i32()?;
    let subject_bone_index = cursor.read_i32()?;
    let target_model_index = cursor.read_i32()?;
    Some(PmmDocumentOutsideParentStateSummary {
        parent_model_index,
        parent_model_bone_index,
        subject_bone_index,
        target_model_index,
    })
}

fn read_document_base_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentBaseKeyframeSummary> {
    let index = if include_index {
        Some(cursor.read_i32()?)
    } else {
        None
    };
    let frame_index = cursor.read_i32()?;
    let previous_keyframe_index = cursor.read_i32()?;
    let next_keyframe_index = cursor.read_i32()?;
    Some(PmmDocumentBaseKeyframeSummary {
        index,
        frame_index,
        previous_keyframe_index,
        next_keyframe_index,
    })
}

fn unpack_document_accessory_opacity_and_visible(value: u8) -> (f32, bool) {
    let visible = value & 1 != 0;
    let opacity = 1.0 - ((value >> 1) as f32 / 100.0);
    (opacity, visible)
}

fn summarize_document_counts(models: &[PmmDocumentModelSummary]) -> PmmDocumentCounts {
    PmmDocumentCounts {
        models: models.len(),
        bones: models.iter().map(|model| model.bone_count).sum(),
        morphs: models.iter().map(|model| model.morph_count).sum(),
        initial_bone_keyframes: models
            .iter()
            .map(|model| model.initial_bone_keyframes)
            .sum(),
        bone_keyframes: models.iter().map(|model| model.bone_keyframes).sum(),
        initial_morph_keyframes: models
            .iter()
            .map(|model| model.initial_morph_keyframes)
            .sum(),
        morph_keyframes: models.iter().map(|model| model.morph_keyframes).sum(),
        initial_model_keyframes: models
            .iter()
            .map(|model| model.initial_model_keyframes)
            .sum(),
        model_keyframes: models.iter().map(|model| model.model_keyframes).sum(),
    }
}

fn usize_from_i32(value: i32) -> Option<usize> {
    usize::try_from(value).ok()
}

fn decode_shift_jis(bytes: &[u8]) -> String {
    let (decoded, _, _) = SHIFT_JIS.decode(bytes);
    decoded.trim_end_matches('\0').to_owned()
}

#[derive(Debug, Clone)]
struct PmmModelSlotScan {
    slots: Vec<PmmModelSlot>,
    stop: Option<PmmModelSlotScanStop>,
}

#[derive(Debug, Clone)]
struct PmmModelSlotScanStop {
    offset: usize,
    reason: &'static str,
}

fn parse_model_slots_from_header(
    data: &[u8],
    references: &[PmmAssetReference],
    display_state: &PmmDisplayState,
) -> PmmModelSlotScan {
    const INITIAL_MODEL_SLOT_OFFSET: usize = 56;
    if display_state.layout != "pmm-v2-flags" {
        return PmmModelSlotScan {
            slots: Vec::new(),
            stop: None,
        };
    }
    let declared_count = display_state.non_zero_model_slot_count.max(1);
    let mut slots = Vec::new();
    let mut offset = INITIAL_MODEL_SLOT_OFFSET;
    let mut stop = None;

    for slot_index in 0..declared_count {
        let require_reference_at_offset = slot_index > 0;
        match parse_model_slot_at(
            data,
            references,
            offset,
            slot_index,
            display_state
                .active_model_slot_indices
                .get(slot_index)
                .copied(),
            require_reference_at_offset,
        ) {
            Ok((slot, next_offset)) => {
                slots.push(slot);
                offset = next_offset;
            }
            Err(reason) => {
                if !slots.is_empty() && slot_index < declared_count {
                    stop = Some(PmmModelSlotScanStop { offset, reason });
                }
                break;
            }
        }
    }

    PmmModelSlotScan { slots, stop }
}

fn parse_model_slot_at(
    data: &[u8],
    references: &[PmmAssetReference],
    offset: usize,
    slot_index: usize,
    display_slot_index: Option<usize>,
    require_reference_at_offset: bool,
) -> Result<(PmmModelSlot, usize), &'static str> {
    let (name, name_bytes, after_name) =
        read_pmm_len_prefixed_sjis(data, offset).ok_or("missing_name")?;
    let (english_name, english_name_bytes, path_offset) =
        read_pmm_len_prefixed_sjis(data, after_name).ok_or("missing_english_name")?;
    let path_end = data
        .get(path_offset..)
        .ok_or("missing_model_path")?
        .iter()
        .position(|byte| *byte == 0)
        .map(|relative| path_offset + relative)
        .ok_or("missing_path_terminator")?;
    if path_end == path_offset {
        return Err("empty_path");
    }
    let (decoded_path, _, _) = SHIFT_JIS.decode(&data[path_offset..path_end]);
    let model_path = find_asset_candidates(&decoded_path)
        .into_iter()
        .find(|candidate| {
            candidate
                .rsplit_once('.')
                .map(|(_, ext)| matches!(ext.to_ascii_lowercase().as_str(), "pmx" | "pmd"))
                .unwrap_or(false)
        })
        .ok_or("missing_model_path")?;
    let normalized_path = normalize_path(&model_path);
    let asset_reference_index = references.iter().position(|reference| {
        reference.kind == "model"
            && reference
                .normalized_path
                .eq_ignore_ascii_case(&normalized_path)
            && (!require_reference_at_offset || reference.offset == offset)
    });
    if require_reference_at_offset && asset_reference_index.is_none() {
        return Err("asset_reference_not_matched");
    }
    let confidence = asset_reference_index
        .and_then(|index| references.get(index).map(|reference| reference.confidence))
        .unwrap_or_else(|| asset_reference_confidence(&model_path, &normalized_path));
    let padding_start = path_end + 1;
    let trailing_zero_padding_bytes = data
        .get(padding_start..)
        .map(|bytes| bytes.iter().take_while(|byte| **byte == 0).count())
        .unwrap_or(0);
    let next_non_zero_offset = data
        .get(padding_start..)
        .and_then(|bytes| bytes.iter().position(|byte| *byte != 0))
        .map(|relative| padding_start + relative);

    Ok((
        PmmModelSlot {
            slot_index,
            display_slot_index,
            offset,
            offset_end: path_end,
            model_path_offset: path_offset,
            trailing_zero_padding_bytes,
            next_non_zero_offset,
            name,
            name_bytes,
            english_name,
            english_name_bytes,
            model_path,
            normalized_path,
            asset_reference_index,
            confidence,
        },
        path_end + 1,
    ))
}

fn read_pmm_len_prefixed_sjis(data: &[u8], offset: usize) -> Option<(String, Vec<u8>, usize)> {
    let length = *data.get(offset)? as usize;
    if length == 0 || length > 128 {
        return None;
    }
    let start = offset + 1;
    let end = start.checked_add(length)?;
    let bytes = data.get(start..end)?;
    if bytes.contains(&0) {
        return None;
    }
    let (decoded, _, _) = SHIFT_JIS.decode(bytes);
    Some((
        decoded.trim_end_matches('\0').to_owned(),
        bytes.to_vec(),
        end,
    ))
}

fn extract_asset_references(data: &[u8]) -> Vec<PmmAssetReference> {
    let mut refs = Vec::new();
    let mut chunk_start = 0usize;
    for index in 0..=data.len() {
        if index < data.len() && data[index] != 0 {
            continue;
        }
        if index > chunk_start {
            let (decoded, _, _) = SHIFT_JIS.decode(&data[chunk_start..index]);
            for candidate in find_asset_candidates(&decoded) {
                let normalized = normalize_path(&candidate);
                if refs.iter().any(|existing: &PmmAssetReference| {
                    existing.normalized_path.eq_ignore_ascii_case(&normalized)
                }) {
                    continue;
                }
                let file_name = normalized
                    .rsplit('/')
                    .next()
                    .unwrap_or(&normalized)
                    .to_owned();
                let extension = file_name
                    .rsplit_once('.')
                    .map(|(_, ext)| ext.to_ascii_lowercase())
                    .unwrap_or_default();
                refs.push(PmmAssetReference {
                    confidence: asset_reference_confidence(&candidate, &normalized),
                    path: candidate,
                    normalized_path: normalized,
                    file_name,
                    kind: classify_extension(&extension),
                    extension,
                    offset: chunk_start,
                    offset_end: index,
                });
            }
        }
        chunk_start = index + 1;
    }
    refs.sort_by_key(|reference| reference.offset);
    refs
}

fn asset_reference_confidence(path: &str, normalized_path: &str) -> &'static str {
    if path.contains('\u{FFFD}')
        || normalized_path.starts_with('/')
        || normalized_path.starts_with(":/")
    {
        return "low";
    }
    if normalized_path.contains('/') || last_windows_drive_path_start(path).is_some() {
        return "high";
    }
    "medium"
}

fn find_asset_candidates(text: &str) -> Vec<String> {
    let mut candidates = Vec::new();
    for segment in text.split(|c: char| c.is_control() || c == '"' || c == '\'') {
        let lower = segment.to_ascii_lowercase();
        let mut search_from = 0usize;
        while let Some((relative_end, extension)) = next_asset_extension(&lower[search_from..]) {
            let end = search_from + relative_end + extension.len();
            let start = asset_candidate_start(segment, end);
            let candidate = segment[start..end].trim();
            if has_asset_file_stem(candidate)
                && !candidates
                    .iter()
                    .any(|existing: &String| existing.eq_ignore_ascii_case(candidate))
            {
                candidates.push(candidate.to_owned());
            }
            search_from = end;
        }
    }
    candidates
}

fn next_asset_extension(text: &str) -> Option<(usize, &'static str)> {
    [
        ".pmd", ".pmx", ".vmd", ".vac", ".x", ".wav", ".mp3", ".ogg", ".bmp", ".tga", ".png",
        ".jpg", ".jpeg", ".dds", ".avi", ".avs", ".mp4", ".wmv",
    ]
    .iter()
    .filter_map(|extension| {
        find_extension_with_boundary(text, extension).map(|index| (index, *extension))
    })
    .min_by_key(|(index, _)| *index)
}

fn find_extension_with_boundary(text: &str, extension: &str) -> Option<usize> {
    let mut search_from = 0usize;
    while let Some(relative_index) = text[search_from..].find(extension) {
        let index = search_from + relative_index;
        let after = index + extension.len();
        if text[after..]
            .chars()
            .next()
            .is_none_or(|ch| ch.is_whitespace())
        {
            return Some(index);
        }
        search_from = after;
    }
    None
}

fn asset_candidate_start(segment: &str, end: usize) -> usize {
    let prefix = &segment[..end];
    let lower = prefix.to_ascii_lowercase();
    if let Some(index) = last_userfile_path_start(&lower) {
        return index;
    }
    if let Some(index) = last_windows_drive_path_start(prefix) {
        return index;
    }
    prefix
        .rfind(char::is_whitespace)
        .map(|index| index + 1)
        .unwrap_or(0)
}

fn last_userfile_path_start(value: &str) -> Option<usize> {
    ["userfile\\", "userfile/"]
        .iter()
        .flat_map(|needle| value.match_indices(needle).map(|(index, _)| index))
        .filter(|index| {
            *index == 0
                || value.as_bytes()[index - 1] == b'\\'
                || value.as_bytes()[index - 1] == b'/'
        })
        .last()
}

fn last_windows_drive_path_start(value: &str) -> Option<usize> {
    let bytes = value.as_bytes();
    if bytes.len() < 3 {
        return None;
    }
    let mut last = None;
    for index in 0..=bytes.len() - 3 {
        if bytes[index].is_ascii_alphabetic()
            && bytes[index + 1] == b':'
            && (bytes[index + 2] == b'\\' || bytes[index + 2] == b'/')
        {
            last = Some(index);
        }
    }
    last
}

fn has_asset_file_stem(value: &str) -> bool {
    let Some((stem, _)) = value
        .rsplit(['\\', '/'])
        .next()
        .and_then(|file_name| file_name.rsplit_once('.'))
    else {
        return false;
    };
    if stem.is_empty() {
        return false;
    }
    if value.contains(['\\', '/']) || last_windows_drive_path_start(value).is_some() {
        return true;
    }
    !stem
        .chars()
        .any(|c| c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*' | '='))
}

fn normalize_path(value: &str) -> String {
    let normalized = value.replace('\\', "/").replace("//", "/");
    let lower = normalized.to_ascii_lowercase();
    match last_userfile_path_start(&lower) {
        Some(index) => normalized[index..].to_owned(),
        None => normalized,
    }
}

fn classify_extension(extension: &str) -> &'static str {
    match extension {
        "pmd" | "pmx" => "model",
        "x" | "vac" => "accessory",
        "vmd" => "motion",
        "wav" | "mp3" | "ogg" => "audio",
        "bmp" | "tga" | "png" | "jpg" | "jpeg" | "dds" => "image",
        "avi" | "avs" | "mp4" | "wmv" => "video",
        _ => "unknown",
    }
}

fn paths_by_kind(references: &[PmmAssetReference], kind: &str) -> Vec<String> {
    references
        .iter()
        .filter(|reference| reference.kind == kind)
        .map(|reference| reference.normalized_path.clone())
        .collect()
}

fn asset_reference_index_for_path(
    references: &[PmmAssetReference],
    kind: &str,
    path: &str,
) -> Option<usize> {
    let normalized = normalize_path(path);
    references.iter().position(|reference| {
        reference.kind == kind && reference.normalized_path.eq_ignore_ascii_case(&normalized)
    })
}

fn scene_assets_by_kind(references: &[PmmAssetReference], kind: &str) -> Vec<PmmSceneAsset> {
    references
        .iter()
        .enumerate()
        .filter(|(_, reference)| reference.kind == kind)
        .enumerate()
        .map(|(kind_index, (reference_index, reference))| PmmSceneAsset {
            reference_index,
            kind_index,
            path: reference.path.clone(),
            normalized_path: reference.normalized_path.clone(),
            file_name: reference.file_name.clone(),
            extension: reference.extension.clone(),
            offset: reference.offset,
            offset_end: reference.offset_end,
            confidence: reference.confidence,
        })
        .collect()
}

fn asset_summary(references: &[PmmAssetReference]) -> PmmAssetSummary {
    let mut kind_counts = BTreeMap::new();
    let mut extension_counts = BTreeMap::new();
    let mut confidence_counts = BTreeMap::new();
    for reference in references {
        *kind_counts.entry(reference.kind.to_owned()).or_insert(0) += 1;
        *extension_counts
            .entry(reference.extension.to_owned())
            .or_insert(0) += 1;
        *confidence_counts
            .entry(reference.confidence.to_owned())
            .or_insert(0) += 1;
    }

    PmmAssetSummary {
        reference_count: references.len(),
        high_confidence_count: *confidence_counts.get("high").unwrap_or(&0),
        medium_confidence_count: *confidence_counts.get("medium").unwrap_or(&0),
        low_confidence_count: *confidence_counts.get("low").unwrap_or(&0),
        kind_counts,
        extension_counts,
        confidence_counts,
    }
}

fn pmm_diagnostics(
    references: &[PmmAssetReference],
    model_slots: &[PmmModelSlot],
    document_summary: Option<&PmmDocumentSummary>,
    document_global_summary: Option<&PmmDocumentGlobalSummary>,
    display_state: &PmmDisplayState,
    model_slot_scan_stop: Option<&PmmModelSlotScanStop>,
    data: &[u8],
) -> Vec<PmmParserDiagnostic> {
    let mut diagnostics = vec![
        PmmParserDiagnostic {
            level: "warning",
            code: "PMM_PROJECT_GRAPH_PARTIAL",
            message: "PMM parser currently exposes header-derived project settings, timeline, display state, and manifest-derived asset references; binary project graph tracks are not fully decoded yet.".to_owned(),
        },
        PmmParserDiagnostic {
            level: "info",
            code: "PMM_ASSET_REFERENCES_SCAN",
            message: format!(
                "PMM asset references are extracted from decoded null-terminated text chunks and grouped by extension; references={}.",
                references.len()
            ),
        },
    ];
    if !model_slots.is_empty() {
        diagnostics.push(PmmParserDiagnostic {
            level: "info",
            code: "PMM_MODEL_SLOT_INITIAL_SLICE",
            message: format!(
                "PMM parser decoded model slot records from the initial fixed-header slice; modelSlots={}.",
                model_slots.len()
            ),
        });
    }
    if let Some(stop) = model_slot_scan_stop {
        diagnostics.push(PmmParserDiagnostic {
            level: "info",
            code: "PMM_MODEL_SLOT_SCAN_STOPPED",
            message: format!(
                "PMM model slot header scan stopped at offset {}: {}; decodedSlots={} declaredModelSlots={}.",
                stop.offset,
                stop.reason,
                model_slots.len(),
                display_state.non_zero_model_slot_count
            ),
        });
    }
    if let Some(summary) = document_summary {
        let total_bone_keyframes =
            summary.counts.initial_bone_keyframes + summary.counts.bone_keyframes;
        let total_morph_keyframes =
            summary.counts.initial_morph_keyframes + summary.counts.morph_keyframes;
        diagnostics.push(PmmParserDiagnostic {
            level: "info",
            code: "PMM_DOCUMENT_SUMMARY_PARTIAL",
            message: format!(
                "PMM parser decoded a PMMv2 document inventory slice with {} model(s), {} total bone keyframe(s) ({} additional), and {} total morph keyframe(s) ({} additional); full keyframe payloads and camera/light/self-shadow/property tracks are not exposed yet.",
                summary.model_count,
                total_bone_keyframes,
                summary.counts.bone_keyframes,
                total_morph_keyframes,
                summary.counts.morph_keyframes
            ),
        });
    }
    if let Some(summary) = document_global_summary {
        diagnostics.push(PmmParserDiagnostic {
            level: "info",
            code: "PMM_DOCUMENT_GLOBAL_SUMMARY_PARTIAL",
            message: format!(
                "PMM parser decoded a PMMv2 global document inventory slice with {} camera keyframe(s), {} light keyframe(s), {} accessory object(s), {} gravity keyframe(s), and {} self-shadow keyframe(s); full payloads are not exposed yet.",
                summary.camera.initial_keyframes + summary.camera.keyframes,
                summary.light.initial_keyframes + summary.light.keyframes,
                summary.accessories.accessory_count,
                summary.gravity.initial_keyframes + summary.gravity.keyframes,
                summary.self_shadow.initial_keyframes + summary.self_shadow.keyframes
            ),
        });
    }
    if display_state.layout == "pmm-v2-flags"
        && display_state.non_zero_model_slot_count > model_slots.len()
    {
        diagnostics.push(PmmParserDiagnostic {
            level: "warning",
            code: "PMM_MODEL_SLOT_COUNT_PARTIAL_DECODE",
            message: format!(
                "PMM displayState reports {} non-zero model slot flags, but only {} initial model slot(s) are decoded.",
                display_state.non_zero_model_slot_count,
                model_slots.len()
            ),
        });
    }
    diagnostics.extend(asset_count_mismatch_diagnostics(references, display_state));
    if display_state.layout == "unknown"
        && let Some(bytes) = data.get(46..54)
        && bytes.iter().any(|byte| *byte > 2)
    {
        diagnostics.push(PmmParserDiagnostic {
            level: "warning",
            code: "PMM_DISPLAY_STATE_UNPLAUSIBLE",
            message: format!(
                "PMM displayState.modelSlotFlags header bytes were outside the observed 0..=2 range and were omitted: {:?}.",
                bytes
            ),
        });
    }
    diagnostics.extend(duplicate_asset_reference_diagnostics(references));
    if references.iter().any(|reference| reference.kind == "model")
        && !references
            .iter()
            .any(|reference| reference.kind == "motion")
    {
        diagnostics.push(PmmParserDiagnostic {
            level: "warning",
            code: "PMM_MOTION_REFERENCES_NOT_FOUND_IN_SCAN",
            message: "PMM manifest-derived asset scan found model references but no motion references; motion paths may live in unparsed binary project graph sections.".to_owned(),
        });
    }
    diagnostics
}

fn asset_count_mismatch_diagnostics(
    references: &[PmmAssetReference],
    display_state: &PmmDisplayState,
) -> Vec<PmmParserDiagnostic> {
    let mut diagnostics = Vec::new();
    let scanned_models = references
        .iter()
        .filter(|reference| reference.kind == "model")
        .count();
    if display_state.model_slot_count > 0 && scanned_models != display_state.model_slot_count {
        diagnostics.push(PmmParserDiagnostic {
            level: "info",
            code: "PMM_ASSET_COUNT_MISMATCH",
            message: format!(
                "PMM displayState declares {} model slot(s), but manifest-derived asset scan found {} model reference(s).",
                display_state.model_slot_count,
                scanned_models
            ),
        });
    }

    if let Some(accessory_count) = display_state.accessory_slot_count {
        let scanned_accessories = references
            .iter()
            .filter(|reference| reference.kind == "accessory")
            .count();
        if scanned_accessories != accessory_count as usize {
            diagnostics.push(PmmParserDiagnostic {
                level: "info",
                code: "PMM_ASSET_COUNT_MISMATCH",
                message: format!(
                    "PMM displayState declares {} accessory slot(s), but manifest-derived asset scan found {} accessory reference(s).",
                    accessory_count,
                    scanned_accessories
                ),
            });
        }
    }
    diagnostics
}

fn duplicate_asset_reference_diagnostics(
    references: &[PmmAssetReference],
) -> Vec<PmmParserDiagnostic> {
    let mut by_file = HashMap::<(String, String), Vec<(usize, &PmmAssetReference)>>::new();
    for (index, reference) in references.iter().enumerate() {
        by_file
            .entry((reference.file_name.clone(), reference.extension.clone()))
            .or_default()
            .push((index, reference));
    }

    let mut diagnostics = Vec::new();
    for ((file_name, extension), entries) in by_file {
        if entries.len() < 2 {
            continue;
        }
        let mut unique_paths = entries
            .iter()
            .map(|(_, reference)| reference.normalized_path.to_ascii_lowercase())
            .collect::<Vec<_>>();
        unique_paths.sort();
        unique_paths.dedup();
        if unique_paths.len() < 2 {
            continue;
        }
        let indices = entries
            .iter()
            .map(|(index, _)| index.to_string())
            .collect::<Vec<_>>()
            .join(",");
        diagnostics.push(PmmParserDiagnostic {
            level: "warning",
            code: "PMM_ASSET_REFERENCE_DUPLICATE",
            message: format!(
                "PMM manifest-derived asset scan found duplicate file names with different paths: fileName={file_name:?} extension={extension:?} referenceIndices={indices}."
            ),
        });
    }
    diagnostics.sort_by(|a, b| a.message.cmp(&b.message));
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pmm_with_project_settings() -> Vec<u8> {
        let mut data = b"Polygon Movie maker 0002".to_vec();
        data.resize(30, 0);
        data.extend_from_slice(&1920u32.to_le_bytes());
        data.extend_from_slice(&1080u32.to_le_bytes());
        data.extend_from_slice(&424u32.to_le_bytes());
        data.extend_from_slice(&30.0f32.to_le_bytes());
        data.extend_from_slice(&[1, 1, 2, 0, 0, 0, 0, 0]);
        data.push(3);
        data.push(0);
        data.extend_from_slice(
            b"UserFile\\Model\\miku.pmx\0UserFile\\Motion\\walk.vmd\0UserFile\\Audio\\song.wav\0",
        );
        data
    }

    fn pmm_with_initial_model_slot() -> Vec<u8> {
        let mut data = b"Polygon Movie maker 0002".to_vec();
        data.resize(30, 0);
        data.extend_from_slice(&640u32.to_le_bytes());
        data.extend_from_slice(&360u32.to_le_bytes());
        data.extend_from_slice(&250u32.to_le_bytes());
        data.extend_from_slice(&30.0f32.to_le_bytes());
        data.extend_from_slice(&[0, 1, 1, 1, 1, 1, 1, 0]);
        data.push(1);
        data.push(0);
        data.push(9);
        data.extend_from_slice(b"TestModel");
        data.push(4);
        data.extend_from_slice(b"Base");
        data.extend_from_slice(b"F:\\Develop\\MMDDev\\data\\unittest\\test_1bone_cube.pmx");
        data.push(0);
        data.extend_from_slice(b"UserFile\\Motion\\walk.vmd\0");
        data
    }

    #[test]
    fn exports_pmm_project_settings_and_asset_references() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let exported = export_pmm_manifest(&parsed);
        let reparsed = parse_pmm_manifest(&exported).unwrap();

        assert_eq!(reparsed.version, "0002");
        assert_eq!(reparsed.project_settings.screen_width, Some(1920));
        assert_eq!(reparsed.project_settings.screen_height, Some(1080));
        assert_eq!(reparsed.project_settings.timeline_frame_count, Some(424));
        assert_eq!(reparsed.project_settings.frame_rate, Some(30.0));
        assert_eq!(reparsed.model_paths, vec!["UserFile/Model/miku.pmx"]);
        assert_eq!(reparsed.motion_paths, vec!["UserFile/Motion/walk.vmd"]);
        assert_eq!(reparsed.audio_paths, vec!["UserFile/Audio/song.wav"]);
    }

    #[test]
    fn exports_pmm_manifest_as_v2_layout_for_v1_input() {
        let mut data = pmm_with_project_settings();
        data[20..24].copy_from_slice(b"0001");
        let parsed = parse_pmm_manifest(&data).unwrap();
        assert_eq!(parsed.version, "0001");

        let exported = export_pmm_manifest(&parsed);
        let reparsed = parse_pmm_manifest(&exported).unwrap();

        assert_eq!(reparsed.version, "0002");
        assert_eq!(reparsed.project_settings.timeline_frame_count, Some(424));
        assert_eq!(reparsed.model_paths, vec!["UserFile/Model/miku.pmx"]);
    }

    #[test]
    fn exports_pmm_initial_model_slot() {
        let parsed = parse_pmm_manifest(&pmm_with_initial_model_slot()).unwrap();
        let exported = export_pmm_manifest(&parsed);
        let reparsed = parse_pmm_manifest(&exported).unwrap();

        assert_eq!(reparsed.project_settings.screen_width, Some(640));
        assert_eq!(reparsed.project_settings.screen_height, Some(360));
        assert_eq!(reparsed.project_settings.timeline_frame_count, Some(250));
        assert_eq!(reparsed.model_slots.len(), 1);
        let slot = &reparsed.model_slots[0];
        assert_eq!(slot.name, "TestModel");
        assert_eq!(slot.english_name, "Base");
        assert_eq!(
            slot.normalized_path,
            "F:/Develop/MMDDev/data/unittest/test_1bone_cube.pmx"
        );
        assert_eq!(reparsed.motion_paths, vec!["UserFile/Motion/walk.vmd"]);
    }

    #[test]
    fn exports_pmm_scene_header_timeline_fps_and_camera_fov() {
        let model =
            crate::pmx::parse_pmx_model(include_bytes!("../fixtures/pmx/ik_multi_axis_limit.pmx"))
                .unwrap();
        let motion = VmdParsedAnimation {
            kind: "vmd",
            metadata: crate::vmd::VmdParsedMetadata {
                format: "vmd",
                model_name: "ik_multi_axis_limit_fixture".to_owned(),
                model_name_bytes: Vec::new(),
                counts: crate::vmd::VmdParsedCounts {
                    bones: 1,
                    morphs: 0,
                    cameras: 0,
                    lights: 0,
                    self_shadows: 0,
                    properties: 0,
                },
                max_frame: 600,
            },
            bone_frames: vec![VmdParsedBoneFrame {
                bone_name: "link_root".to_owned(),
                bone_name_bytes: Vec::new(),
                frame: 600,
                translation: [0.0, 1.0, 0.0],
                rotation: [0.0, 0.0, 0.0, 1.0],
                interpolation: vec![20; 64],
            }],
            morph_frames: Vec::new(),
            camera_frames: Vec::new(),
            light_frames: Vec::new(),
            self_shadow_frames: Vec::new(),
            property_frames: Vec::new(),
        };

        let report = export_pmm_scene_from_pmx_vmd(
            &model,
            &motion,
            "UserFile/Model/ik_multi_axis_limit.pmx",
            &PmmSceneExportOptions {
                screen_width: 800,
                screen_height: 600,
                frame_rate: 60.0,
                camera_fov: 42.0,
            },
        );
        let reparsed = parse_pmm_manifest(&report.bytes).unwrap();

        assert_eq!(report.max_frame, 600);
        assert_eq!(reparsed.project_settings.screen_width, Some(800));
        assert_eq!(reparsed.project_settings.screen_height, Some(600));
        assert_eq!(reparsed.project_settings.timeline_frame_count, Some(600));
        assert_eq!(reparsed.project_settings.frame_rate, Some(60.0));
        let global = reparsed.document_global_summary.as_ref().unwrap();
        match global.camera.initial_keyframe.as_ref().unwrap() {
            PmmDocumentKeyframeSummary::Camera { fov, .. } => assert_eq!(*fov, 42),
            other => panic!("unexpected camera keyframe summary: {other:?}"),
        }
    }

    fn append_pmm_model_slot(data: &mut Vec<u8>, name: &[u8], english_name: &[u8], path: &[u8]) {
        data.push(name.len() as u8);
        data.extend_from_slice(name);
        data.push(english_name.len() as u8);
        data.extend_from_slice(english_name);
        data.extend_from_slice(path);
        data.push(0);
    }

    fn push_i32(data: &mut Vec<u8>, value: i32) {
        data.extend_from_slice(&value.to_le_bytes());
    }

    fn push_f32(data: &mut Vec<u8>, value: f32) {
        data.extend_from_slice(&value.to_le_bytes());
    }

    fn push_pmm_variable_string(data: &mut Vec<u8>, value: &[u8]) {
        data.push(value.len() as u8);
        data.extend_from_slice(value);
    }

    fn push_pmm_fixed_path(data: &mut Vec<u8>, value: &[u8]) {
        let mut path = [0u8; 256];
        path[..value.len()].copy_from_slice(value);
        data.extend_from_slice(&path);
    }

    fn push_pmm_fixed_bytes<const N: usize>(data: &mut Vec<u8>, value: &[u8]) {
        let mut bytes = [0u8; N];
        bytes[..value.len()].copy_from_slice(value);
        data.extend_from_slice(&bytes);
    }

    fn push_document_bone_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        if include_index {
            push_i32(data, 0);
        }
        push_i32(data, frame);
        push_i32(data, 0);
        push_i32(data, 0);
        data.extend_from_slice(&[20u8; 16]);
        data.extend_from_slice(&[0.0f32.to_le_bytes(); 3].concat());
        push_f32(data, 0.0);
        push_f32(data, 0.0);
        push_f32(data, 0.0);
        push_f32(data, 1.0);
        data.push(0);
        data.push(0);
    }

    fn push_document_morph_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        if include_index {
            push_i32(data, 0);
        }
        push_i32(data, frame);
        push_i32(data, 0);
        push_i32(data, 0);
        push_f32(data, 0.5);
        data.push(0);
    }

    fn push_document_model_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        if include_index {
            push_i32(data, frame);
        }
        push_i32(data, frame);
        push_i32(data, 0);
        push_i32(data, 0);
        data.push(1);
        data.push(1);
        push_i32(data, 2);
        push_i32(data, 3);
        data.push(1);
    }

    fn push_document_base_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        if include_index {
            push_i32(data, frame);
        }
        push_i32(data, frame);
        push_i32(data, 0);
        push_i32(data, 0);
    }

    fn push_document_camera_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        push_document_base_keyframe(data, include_index, frame);
        push_f32(data, 45.0);
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(&[0u8; 12]);
        push_i32(data, -1);
        push_i32(data, -1);
        data.extend_from_slice(&[20u8; 24]);
        data.push(0);
        push_i32(data, 30);
        data.push(0);
    }

    fn push_document_light_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        push_document_base_keyframe(data, include_index, frame);
        for value in [0.6f32, 0.6, 0.6, -0.5, -1.0, 0.5] {
            push_f32(data, value);
        }
        data.push(0);
    }

    fn push_document_gravity_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        push_document_base_keyframe(data, include_index, frame);
        data.push(0);
        push_i32(data, 10);
        push_f32(data, 9.8);
        for value in [0.0f32, -1.0, 0.0] {
            push_f32(data, value);
        }
        data.push(0);
    }

    fn push_document_self_shadow_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        push_document_base_keyframe(data, include_index, frame);
        data.push(1);
        push_f32(data, 8875.0);
        data.push(0);
    }

    fn push_document_accessory_keyframe(data: &mut Vec<u8>, include_index: bool, frame: i32) {
        push_document_base_keyframe(data, include_index, frame);
        data.push(1);
        push_i32(data, -1);
        push_i32(data, -1);
        for value in [1.0f32, 2.0, 3.0, 0.1, 0.2, 0.3] {
            push_f32(data, value);
        }
        push_f32(data, 10.0);
        data.push(1);
        data.push(0);
    }

    fn push_empty_pmm_path(data: &mut Vec<u8>) {
        data.extend_from_slice(&[0u8; 256]);
    }

    fn pmm_with_document_summary() -> Vec<u8> {
        let mut data = b"Polygon Movie maker 0002".to_vec();
        data.resize(30, 0);
        push_i32(&mut data, 640);
        push_i32(&mut data, 360);
        push_i32(&mut data, 120);
        push_f32(&mut data, 30.0);
        data.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0]);
        data.push(0);
        data.push(1);
        data.push(0);
        push_pmm_variable_string(&mut data, b"Miku");
        push_pmm_variable_string(&mut data, b"MikuEn");
        push_pmm_fixed_path(&mut data, b"UserFile\\Model\\miku.pmx");
        data.push(0);
        push_i32(&mut data, 1);
        push_pmm_variable_string(&mut data, b"center");
        push_i32(&mut data, 1);
        push_pmm_variable_string(&mut data, b"smile");
        push_i32(&mut data, 1);
        push_i32(&mut data, 7);
        push_i32(&mut data, 1);
        push_i32(&mut data, 8);
        data.push(0);
        data.push(1);
        push_i32(&mut data, 0);
        for _ in 0..4 {
            push_i32(&mut data, -1);
        }
        data.push(0);
        push_i32(&mut data, 0);
        push_i32(&mut data, 30);
        push_document_bone_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_bone_keyframe(&mut data, true, 30);
        push_document_morph_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_morph_keyframe(&mut data, true, 15);
        push_document_model_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_model_keyframe(&mut data, true, 30);
        // bone state: translation [1.0, 2.0, 3.0], orientation [0.0, 0.0, 0.0, 1.0], dirty=true, physicsDisabled=false, selected=true
        push_f32(&mut data, 1.0);
        push_f32(&mut data, 2.0);
        push_f32(&mut data, 3.0);
        push_f32(&mut data, 0.0);
        push_f32(&mut data, 0.0);
        push_f32(&mut data, 0.0);
        push_f32(&mut data, 1.0);
        data.push(1); // dirty
        data.push(0); // physicsDisabled
        data.push(1); // selected
        // morph state: weight 0.75
        push_f32(&mut data, 0.75);
        // constraint state: enabled=true
        data.push(1);
        // outside parent state: parentModelIndex=0, parentModelBoneIndex=5, subjectBoneIndex=7, targetModelIndex=1
        push_i32(&mut data, 0);
        push_i32(&mut data, 5);
        push_i32(&mut data, 7);
        push_i32(&mut data, 1);
        data.push(1);
        push_f32(&mut data, 1.0);
        data.push(0);
        data.push(0);
        data.extend_from_slice(b"UserFile\\Model\\miku.pmx\0");
        data
    }

    fn pmm_with_document_global_summary() -> Vec<u8> {
        let mut data = pmm_with_document_summary();
        let trailing_reference = b"UserFile\\Model\\miku.pmx\0";
        data.truncate(data.len() - trailing_reference.len());

        push_document_camera_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_camera_keyframe(&mut data, true, 42);
        data.extend_from_slice(&[0u8; 12 * 3]);
        data.push(0);

        push_document_light_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_light_keyframe(&mut data, true, 43);
        for value in [0.6f32, 0.6, 0.6, -0.5, -1.0, 0.5] {
            push_f32(&mut data, value);
        }

        data.push(0);
        push_i32(&mut data, 0);
        data.push(1);
        data.push(0);
        push_pmm_fixed_bytes::<100>(&mut data, b"Stage");
        push_pmm_fixed_path(&mut data, b"UserFile\\Accessory\\stage.x");
        data.push(0);
        push_document_accessory_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_accessory_keyframe(&mut data, true, 46);
        data.push(1);
        push_i32(&mut data, -1);
        push_i32(&mut data, -1);
        for value in [1.0f32, 2.0, 3.0] {
            push_f32(&mut data, value);
        }
        push_f32(&mut data, 10.0);
        for value in [0.1f32, 0.2, 0.3] {
            push_f32(&mut data, value);
        }
        data.push(1);
        data.push(0);

        push_i32(&mut data, 12);
        push_i32(&mut data, 34);
        push_i32(&mut data, 56);
        push_i32(&mut data, 2);
        data.push(0);
        data.push(1);
        data.push(0);
        data.push(1);
        push_i32(&mut data, 3);
        push_i32(&mut data, 120);
        data.push(1);
        push_empty_pmm_path(&mut data);
        push_i32(&mut data, 0);
        push_i32(&mut data, 0);
        push_f32(&mut data, 1.0);
        push_empty_pmm_path(&mut data);
        push_i32(&mut data, 0);
        push_i32(&mut data, 0);
        push_i32(&mut data, 0);
        push_f32(&mut data, 1.0);
        push_empty_pmm_path(&mut data);
        data.push(0);
        data.push(1);
        data.push(1);
        data.push(1);
        push_f32(&mut data, 60.0);
        push_i32(&mut data, 2);
        push_i32(&mut data, -1);
        push_f32(&mut data, 1.0);
        data.push(1);
        data.push(2);

        push_f32(&mut data, 9.8);
        push_i32(&mut data, 10);
        for value in [0.0f32, -1.0, 0.0] {
            push_f32(&mut data, value);
        }
        data.push(0);
        push_document_gravity_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_gravity_keyframe(&mut data, true, 44);

        data.push(1);
        push_f32(&mut data, 8875.0);
        push_document_self_shadow_keyframe(&mut data, false, 0);
        push_i32(&mut data, 1);
        push_document_self_shadow_keyframe(&mut data, true, 45);

        push_i32(&mut data, 0);
        push_i32(&mut data, 0);
        push_i32(&mut data, 0);
        data.push(0);
        push_i32(&mut data, -1);
        push_i32(&mut data, -1);
        data.extend_from_slice(&[0u8; 16 * 4]);
        data.push(0);
        data.push(0);
        data.push(1);
        push_i32(&mut data, 12);
        data.push(1);
        data.push(0);
        push_i32(&mut data, 0);

        data
    }

    fn pmm_with_two_initial_model_slots() -> Vec<u8> {
        let mut data = b"Polygon Movie maker 0002".to_vec();
        data.resize(30, 0);
        data.extend_from_slice(&640u32.to_le_bytes());
        data.extend_from_slice(&360u32.to_le_bytes());
        data.extend_from_slice(&250u32.to_le_bytes());
        data.extend_from_slice(&30.0f32.to_le_bytes());
        data.extend_from_slice(&[1, 1, 0, 0, 0, 0, 0, 0]);
        data.push(1);
        data.push(0);
        append_pmm_model_slot(
            &mut data,
            b"TestModel",
            b"Base",
            b"F:\\Develop\\MMDDev\\data\\unittest\\test_1bone_cube.pmx",
        );
        append_pmm_model_slot(
            &mut data,
            b"Sour",
            b"Sour",
            b"G:\\MikuMikuDance\\MMD Models\\Sour\\sour.pmx",
        );
        data.extend_from_slice(b"UserFile\\Motion\\walk.vmd\0");
        data
    }

    fn pmm_v1_with_display_counts() -> Vec<u8> {
        let mut data = b"Polygon Movie maker 0001".to_vec();
        data.resize(30, 0);
        data.extend_from_slice(&512u32.to_le_bytes());
        data.extend_from_slice(&288u32.to_le_bytes());
        data.extend_from_slice(&435u32.to_le_bytes());
        data.extend_from_slice(&35.0f32.to_le_bytes());
        data.extend_from_slice(&[1, 1, 1, 1, 1, 1]);
        data.push(8);
        data.push(9);
        data.extend_from_slice(b"Dummy\0 maker Dummy\0");
        data.extend_from_slice(b"UserFile\\Model\\miku.pmd\0UserFile\\Accessory\\stage01.x\0");
        data
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
    fn pmm_manifest_json_top_level_schema_is_stable() {
        let data =
            b"Polygon Movie maker 0002\0UserFile\\Model\\miku.pmx\0UserFile\\Motion\\walk.vmd\0";
        let parsed = parse_pmm_manifest(data).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed).unwrap());

        assert_eq!(
            keys,
            vec![
                "accessoryAssets",
                "accessoryPaths",
                "assetReferences",
                "assetSummary",
                "audioAssets",
                "audioPaths",
                "byteLength",
                "diagnostics",
                "displayState",
                "documentGlobalSummary",
                "documentSummary",
                "headerTextEntries",
                "imageAssets",
                "imagePaths",
                "modelAssets",
                "modelPaths",
                "modelSlots",
                "motionAssets",
                "motionPaths",
                "parsedVersion",
                "projectSettings",
                "signature",
                "timeline",
                "version",
                "videoAssets",
                "videoPaths",
            ]
        );
    }

    #[test]
    fn pmm_header_text_entry_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_v1_with_display_counts()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.header_text_entries[0]).unwrap());

        assert_eq!(
            keys,
            vec!["index", "offset", "offsetEnd", "text", "textBytes"]
        );
    }

    #[test]
    fn pmm_model_slot_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_initial_model_slot()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.model_slots[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "assetReferenceIndex",
                "confidence",
                "displaySlotIndex",
                "englishName",
                "englishNameBytes",
                "modelPath",
                "modelPathOffset",
                "name",
                "nameBytes",
                "nextNonZeroOffset",
                "normalizedPath",
                "offset",
                "offsetEnd",
                "slotIndex",
                "trailingZeroPaddingBytes"
            ]
        );
    }

    #[test]
    fn pmm_project_settings_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.project_settings).unwrap());

        assert_eq!(
            keys,
            vec![
                "frameRate",
                "screenHeight",
                "screenWidth",
                "timelineFrameCount",
            ]
        );
    }

    #[test]
    fn pmm_display_state_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.display_state).unwrap());

        assert_eq!(
            keys,
            vec![
                "accessorySlotCount",
                "activeModelSlotIndices",
                "declaredModelSlotCount",
                "documentExpandFlags",
                "documentModelCount",
                "emptyModelSlotIndices",
                "layout",
                "modelSlotCount",
                "modelSlotFlagCounts",
                "modelSlotFlagEntries",
                "modelSlotFlags",
                "nonZeroModelSlotCount",
                "selectedModelIndex"
            ]
        );
    }

    #[test]
    fn pmm_model_slot_flag_entry_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(
            &serde_json::to_value(&parsed.display_state.model_slot_flag_entries[0]).unwrap(),
        );

        assert_eq!(keys, vec!["active", "flag", "slotIndex"]);
    }

    #[test]
    fn pmm_document_expand_flags_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(
            &serde_json::to_value(parsed.display_state.document_expand_flags.unwrap()).unwrap(),
        );

        assert_eq!(
            keys,
            vec![
                "accessoryPanel",
                "bonePanel",
                "cameraPanel",
                "editingCla",
                "lightPanel",
                "morphPanel",
                "selfShadowPanel"
            ]
        );
    }

    #[test]
    fn pmm_timeline_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.timeline).unwrap());

        assert_eq!(
            keys,
            vec![
                "durationSeconds",
                "endFrameExclusive",
                "frameCount",
                "frameRate",
                "startFrame"
            ]
        );
    }

    #[test]
    fn pmm_scene_asset_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.model_assets[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "confidence",
                "extension",
                "fileName",
                "kindIndex",
                "normalizedPath",
                "offset",
                "offsetEnd",
                "path",
                "referenceIndex"
            ]
        );
    }

    #[test]
    fn pmm_asset_reference_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.asset_references[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "confidence",
                "extension",
                "fileName",
                "kind",
                "normalizedPath",
                "offset",
                "offsetEnd",
                "path"
            ]
        );
    }

    #[test]
    fn pmm_asset_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.asset_summary).unwrap());

        assert_eq!(
            keys,
            vec![
                "confidenceCounts",
                "extensionCounts",
                "highConfidenceCount",
                "kindCounts",
                "lowConfidenceCount",
                "mediumConfidenceCount",
                "referenceCount"
            ]
        );
    }

    #[test]
    fn pmm_document_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let keys = json_keys(&serde_json::to_value(parsed.document_summary.unwrap()).unwrap());

        assert_eq!(
            keys,
            vec![
                "counts",
                "modelCount",
                "models",
                "selectedModelIndex",
                "source"
            ]
        );
    }

    #[test]
    fn pmm_document_counts_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let keys = json_keys(&serde_json::to_value(document.counts).unwrap());

        assert_eq!(
            keys,
            vec![
                "boneKeyframes",
                "bones",
                "initialBoneKeyframes",
                "initialModelKeyframes",
                "initialMorphKeyframes",
                "modelKeyframes",
                "models",
                "morphKeyframes",
                "morphs"
            ]
        );
    }

    #[test]
    fn pmm_document_global_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let keys =
            json_keys(&serde_json::to_value(parsed.document_global_summary.unwrap()).unwrap());

        assert_eq!(
            keys,
            vec![
                "accessories",
                "camera",
                "gravity",
                "light",
                "offset",
                "offsetEnd",
                "selfShadow",
                "settings",
                "source"
            ]
        );
    }

    #[test]
    fn pmm_initial_camera_keyframe_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys =
            json_keys(&serde_json::to_value(global.camera.initial_keyframe.unwrap()).unwrap());

        assert_eq!(
            keys,
            vec![
                "angle",
                "distance",
                "fov",
                "frameIndex",
                "index",
                "kind",
                "lookAt",
                "nextKeyframeIndex",
                "parentModelBoneIndex",
                "parentModelIndex",
                "perspectiveView",
                "previousKeyframeIndex",
                "selected"
            ]
        );
    }

    #[test]
    fn pmm_initial_light_keyframe_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys =
            json_keys(&serde_json::to_value(global.light.initial_keyframe.unwrap()).unwrap());

        assert_eq!(
            keys,
            vec![
                "color",
                "direction",
                "frameIndex",
                "index",
                "kind",
                "nextKeyframeIndex",
                "previousKeyframeIndex",
                "selected"
            ]
        );
    }

    #[test]
    fn pmm_initial_gravity_keyframe_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys =
            json_keys(&serde_json::to_value(global.gravity.initial_keyframe.unwrap()).unwrap());

        assert_eq!(
            keys,
            vec![
                "acceleration",
                "direction",
                "frameIndex",
                "index",
                "kind",
                "nextKeyframeIndex",
                "noise",
                "noiseEnabled",
                "previousKeyframeIndex",
                "selected"
            ]
        );
    }

    #[test]
    fn pmm_initial_self_shadow_keyframe_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys =
            json_keys(&serde_json::to_value(global.self_shadow.initial_keyframe.unwrap()).unwrap());

        assert_eq!(
            keys,
            vec![
                "distance",
                "frameIndex",
                "index",
                "kind",
                "mode",
                "nextKeyframeIndex",
                "previousKeyframeIndex",
                "selected"
            ]
        );
    }

    #[test]
    fn pmm_document_track_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys = json_keys(&serde_json::to_value(global.camera).unwrap());

        assert_eq!(
            keys,
            vec![
                "initialKeyframe",
                "initialKeyframes",
                "keyframeCountOffset",
                "keyframeSummaries",
                "keyframes",
                "keyframesEndOffset",
                "keyframesOffset",
                "offset",
                "offsetEnd",
                "stateEndOffset",
                "stateOffset"
            ]
        );
    }

    #[test]
    fn pmm_document_settings_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys = json_keys(&serde_json::to_value(global.settings).unwrap());

        assert_eq!(
            keys,
            vec![
                "accessoryIndexAfterModels",
                "audioEnabled",
                "audioPath",
                "backgroundImageEnabled",
                "backgroundImageOffset",
                "backgroundImagePath",
                "backgroundImageScaleFactor",
                "backgroundVideoEnabled",
                "backgroundVideoOffset",
                "backgroundVideoPath",
                "backgroundVideoScaleFactor",
                "beginFrameIndex",
                "beginFrameIndexEnabled",
                "blackBackgroundEnabled",
                "cameraLookAtModelBoneIndex",
                "cameraLookAtModelIndex",
                "cameraLookMode",
                "currentFrameIndex",
                "currentFrameIndexInTextField",
                "edgeColor",
                "editingMode",
                "endFrameIndex",
                "endFrameIndexEnabled",
                "followingLookAtEnabled",
                "gridAndAxisShown",
                "groundShadowBrightness",
                "groundShadowShown",
                "horizontalScroll",
                "horizontalScrollThumb",
                "informationShown",
                "loopEnabled",
                "modelSelectionFooterEndOffset",
                "modelSelectionFooterOffset",
                "modelSelectionFooterPresent",
                "offset",
                "offsetEnd",
                "physicsGroundEnabled",
                "physicsSimulationMode",
                "preferredFps",
                "screenCaptureMode",
                "translucentGroundShadowEnabled",
                "unknownMatrixEndOffset",
                "unknownMatrixOffset"
            ]
        );
    }

    #[test]
    fn pmm_document_accessory_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys = json_keys(&serde_json::to_value(&global.accessories.accessories[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "addBlendEnabled",
                "assetReferenceIndex",
                "documentAccessoryIndex",
                "drawOrderIndex",
                "initialKeyframe",
                "keyframeCountOffset",
                "keyframeSummaries",
                "keyframes",
                "keyframesEndOffset",
                "keyframesOffset",
                "name",
                "offset",
                "offsetEnd",
                "opacity",
                "parentModelBoneIndex",
                "parentModelIndex",
                "path",
                "pathOffset",
                "scaleFactor",
                "shadowEnabled",
                "slotIndex",
                "stateEndOffset",
                "stateOffset",
                "visible"
            ]
        );
    }

    #[test]
    fn pmm_document_accessory_keyframe_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.unwrap();
        let keys = json_keys(
            &serde_json::to_value(&global.accessories.accessories[0].initial_keyframe).unwrap(),
        );

        assert_eq!(
            keys,
            vec![
                "frameIndex",
                "index",
                "nextKeyframeIndex",
                "opacity",
                "orientation",
                "parentModelBoneIndex",
                "parentModelIndex",
                "previousKeyframeIndex",
                "scaleFactor",
                "selected",
                "shadowEnabled",
                "translation",
                "visible"
            ]
        );
    }

    #[test]
    fn parses_pmm_v2_document_global_summary() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.as_ref().unwrap();

        assert_eq!(global.source, "nanoem/ext/document.c PMMv2 global layout");
        assert_eq!(global.camera.initial_keyframes, 1);
        assert_eq!(global.camera.keyframes, 1);
        match global.camera.initial_keyframe.as_ref().unwrap() {
            PmmDocumentKeyframeSummary::Camera {
                index,
                frame_index,
                distance,
                look_at,
                angle,
                parent_model_index,
                parent_model_bone_index,
                perspective_view,
                fov,
                selected,
                ..
            } => {
                assert_eq!(*index, None);
                assert_eq!(*frame_index, 0);
                assert_eq!(*distance, 45.0);
                assert_eq!(*look_at, [0.0, 0.0, 0.0]);
                assert_eq!(*angle, [0.0, 0.0, 0.0]);
                assert_eq!(*parent_model_index, -1);
                assert_eq!(*parent_model_bone_index, -1);
                assert!(*perspective_view);
                assert_eq!(*fov, 30);
                assert!(!*selected);
            }
            other => panic!("unexpected camera keyframe summary: {other:?}"),
        }
        match &global.camera.keyframe_summaries[0] {
            PmmDocumentKeyframeSummary::Camera {
                index, frame_index, ..
            } => {
                assert_eq!(*index, Some(42));
                assert_eq!(*frame_index, 42);
            }
            other => panic!("unexpected additional camera keyframe summary: {other:?}"),
        }
        assert_eq!(global.light.initial_keyframes, 1);
        assert_eq!(global.light.keyframes, 1);
        match global.light.initial_keyframe.as_ref().unwrap() {
            PmmDocumentKeyframeSummary::Light {
                color,
                direction,
                selected,
                ..
            } => {
                assert_eq!(*color, [0.6, 0.6, 0.6]);
                assert_eq!(*direction, [-0.5, -1.0, 0.5]);
                assert!(!*selected);
            }
            other => panic!("unexpected light keyframe summary: {other:?}"),
        }
        match &global.light.keyframe_summaries[0] {
            PmmDocumentKeyframeSummary::Light {
                index, frame_index, ..
            } => {
                assert_eq!(*index, Some(43));
                assert_eq!(*frame_index, 43);
            }
            other => panic!("unexpected additional light keyframe summary: {other:?}"),
        }
        assert_eq!(global.accessories.accessory_count, 1);
        assert_eq!(global.accessories.keyframes, 1);
        let accessory = &global.accessories.accessories[0];
        assert_eq!(accessory.slot_index, 0);
        assert_eq!(accessory.document_accessory_index, 0);
        assert_eq!(accessory.name, "Stage");
        assert_eq!(accessory.path, "UserFile\\Accessory\\stage.x");
        assert_eq!(accessory.keyframes, 1);
        assert!(accessory.visible);
        assert_eq!(accessory.opacity, 1.0);
        assert_eq!(accessory.scale_factor, 10.0);
        assert!(accessory.shadow_enabled);
        assert!(!accessory.add_blend_enabled);
        assert_eq!(accessory.initial_keyframe.index, None);
        assert_eq!(accessory.initial_keyframe.frame_index, 0);
        assert_eq!(accessory.initial_keyframe.translation, [1.0, 2.0, 3.0]);
        assert_eq!(accessory.initial_keyframe.orientation, [0.1, 0.2, 0.3]);
        assert_eq!(accessory.keyframe_summaries[0].index, Some(46));
        assert_eq!(accessory.keyframe_summaries[0].frame_index, 46);
        assert_eq!(global.gravity.initial_keyframes, 1);
        assert_eq!(global.gravity.keyframes, 1);
        match global.gravity.initial_keyframe.as_ref().unwrap() {
            PmmDocumentKeyframeSummary::Gravity {
                noise_enabled,
                noise,
                acceleration,
                direction,
                selected,
                ..
            } => {
                assert!(!*noise_enabled);
                assert_eq!(*noise, 10);
                assert_eq!(*acceleration, 9.8);
                assert_eq!(*direction, [0.0, -1.0, 0.0]);
                assert!(!*selected);
            }
            other => panic!("unexpected gravity keyframe summary: {other:?}"),
        }
        match &global.gravity.keyframe_summaries[0] {
            PmmDocumentKeyframeSummary::Gravity {
                index, frame_index, ..
            } => {
                assert_eq!(*index, Some(44));
                assert_eq!(*frame_index, 44);
            }
            other => panic!("unexpected additional gravity keyframe summary: {other:?}"),
        }
        assert_eq!(global.self_shadow.initial_keyframes, 1);
        assert_eq!(global.self_shadow.keyframes, 1);
        match global.self_shadow.initial_keyframe.as_ref().unwrap() {
            PmmDocumentKeyframeSummary::SelfShadow {
                mode,
                distance,
                selected,
                ..
            } => {
                assert_eq!(*mode, 1);
                assert_eq!(*distance, 8875.0);
                assert!(!*selected);
            }
            other => panic!("unexpected self-shadow keyframe summary: {other:?}"),
        }
        match &global.self_shadow.keyframe_summaries[0] {
            PmmDocumentKeyframeSummary::SelfShadow {
                index, frame_index, ..
            } => {
                assert_eq!(*index, Some(45));
                assert_eq!(*frame_index, 45);
            }
            other => panic!("unexpected additional self-shadow keyframe summary: {other:?}"),
        }
        assert_eq!(global.settings.current_frame_index, 12);
        assert_eq!(global.settings.horizontal_scroll, 34);
        assert_eq!(global.settings.horizontal_scroll_thumb, 56);
        assert_eq!(global.settings.begin_frame_index, 3);
        assert_eq!(global.settings.end_frame_index, 120);
        assert!(global.settings.audio_enabled);
        assert!(global.settings.grid_and_axis_shown);
        assert!(global.settings.physics_ground_enabled);
        assert!(global.settings.model_selection_footer_present);
        assert!(global.offset_end > global.offset);
        assert!(global.camera.offset < global.light.offset);
        assert!(global.light.offset < global.accessories.offset);
        assert!(global.gravity.offset < global.self_shadow.offset);
    }

    #[test]
    fn pmm_document_model_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let keys = json_keys(&serde_json::to_value(&document.models[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "assetReferenceIndex",
                "blendEnabled",
                "boneCount",
                "boneKeyframeSummaries",
                "boneKeyframes",
                "boneStateSummaries",
                "constraintBoneCount",
                "constraintStateSummaries",
                "documentModelIndex",
                "drawOrderIndex",
                "edgeWidth",
                "englishName",
                "initialBoneKeyframeSummaries",
                "initialBoneKeyframes",
                "initialModelKeyframe",
                "initialModelKeyframes",
                "initialMorphKeyframeSummaries",
                "initialMorphKeyframes",
                "lastFrameIndex",
                "modelKeyframeSummaries",
                "modelKeyframes",
                "morphCount",
                "morphKeyframeSummaries",
                "morphKeyframes",
                "morphStateSummaries",
                "name",
                "offset",
                "offsetEnd",
                "outsideParentStateSummaries",
                "outsideParentSubjectBoneCount",
                "path",
                "pathOffset",
                "sections",
                "selectedBoneIndex",
                "selectedMorphIndices",
                "selfShadowEnabled",
                "slotIndex",
                "transformOrderIndex",
                "verticalScroll",
                "visible"
            ]
        );
    }

    #[test]
    fn pmm_document_model_keyframe_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let keys =
            json_keys(&serde_json::to_value(&document.models[0].initial_model_keyframe).unwrap());

        assert_eq!(
            keys,
            vec![
                "constraintStates",
                "frameIndex",
                "index",
                "nextKeyframeIndex",
                "outsideParentIndices",
                "previousKeyframeIndex",
                "selfShadowEnabled",
                "visible"
            ]
        );
    }

    #[test]
    fn pmm_outside_parent_index_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let kf = &document.models[0].initial_model_keyframe;
        let keys = json_keys(&serde_json::to_value(&kf.outside_parent_indices[0]).unwrap());

        assert_eq!(keys, vec!["parentModelBoneIndex", "parentModelIndex"]);
    }

    #[test]
    fn pmm_document_model_sections_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let keys = json_keys(&serde_json::to_value(&document.models[0].sections).unwrap());

        assert_eq!(
            keys,
            vec![
                "boneKeyframeCountOffset",
                "boneKeyframesEndOffset",
                "boneKeyframesOffset",
                "boneStatesOffset",
                "constraintStatesOffset",
                "initialBoneKeyframesOffset",
                "initialModelKeyframeOffset",
                "initialMorphKeyframesOffset",
                "modelKeyframeCountOffset",
                "modelKeyframesEndOffset",
                "modelKeyframesOffset",
                "morphKeyframeCountOffset",
                "morphKeyframesEndOffset",
                "morphKeyframesOffset",
                "morphStatesOffset",
                "outsideParentStatesOffset"
            ]
        );
    }

    #[test]
    fn pmm_diagnostic_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed.diagnostics[0]).unwrap());

        assert_eq!(keys, vec!["code", "level", "message"]);
    }

    #[test]
    fn parses_pmm_project_settings_header_slice() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();

        assert_eq!(parsed.project_settings.screen_width, Some(1920));
        assert_eq!(parsed.project_settings.screen_height, Some(1080));
        assert_eq!(parsed.project_settings.timeline_frame_count, Some(424));
        assert_eq!(parsed.project_settings.frame_rate, Some(30.0));
        assert_eq!(parsed.timeline.start_frame, Some(0));
        assert_eq!(parsed.timeline.end_frame_exclusive, Some(424));
        assert_eq!(parsed.timeline.frame_count, Some(424));
        assert_eq!(parsed.timeline.frame_rate, Some(30.0));
        assert_eq!(parsed.timeline.duration_seconds, Some(424.0 / 30.0));
        assert_eq!(
            parsed.display_state.model_slot_flags,
            vec![1, 1, 2, 0, 0, 0, 0, 0]
        );
        assert_eq!(parsed.display_state.layout, "pmm-v2-flags");
        assert_eq!(parsed.display_state.declared_model_slot_count, Some(8));
        assert_eq!(parsed.display_state.model_slot_count, 8);
        assert_eq!(parsed.display_state.non_zero_model_slot_count, 3);
        assert_eq!(parsed.display_state.accessory_slot_count, None);
        assert_eq!(parsed.model_paths, vec!["UserFile/Model/miku.pmx"]);
        assert_eq!(parsed.motion_paths, vec!["UserFile/Motion/walk.vmd"]);
        assert_eq!(parsed.audio_paths, vec!["UserFile/Audio/song.wav"]);
        assert_eq!(parsed.model_assets.len(), 1);
        assert_eq!(parsed.model_assets[0].reference_index, 0);
        assert_eq!(parsed.model_assets[0].kind_index, 0);
        assert_eq!(parsed.model_assets[0].file_name, "miku.pmx");
        assert_eq!(parsed.model_assets[0].confidence, "high");
        assert!(parsed.model_assets[0].offset_end > parsed.model_assets[0].offset);
        assert_eq!(parsed.motion_assets[0].reference_index, 1);
        assert_eq!(parsed.motion_assets[0].kind_index, 0);
        assert_eq!(parsed.audio_assets[0].reference_index, 2);
        assert_eq!(parsed.audio_assets[0].extension, "wav");
        assert_eq!(parsed.diagnostics.len(), 4);
        assert_eq!(parsed.diagnostics[0].code, "PMM_PROJECT_GRAPH_PARTIAL");
        assert_eq!(parsed.diagnostics[1].code, "PMM_ASSET_REFERENCES_SCAN");
        assert!(parsed.diagnostics[1].message.contains("references=3"));
        assert_eq!(
            parsed.diagnostics[2].code,
            "PMM_MODEL_SLOT_COUNT_PARTIAL_DECODE"
        );
        assert_eq!(parsed.diagnostics[3].code, "PMM_ASSET_COUNT_MISMATCH");
    }

    #[test]
    fn pmm_asset_summary_groups_manifest_derived_references() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let summary = &parsed.asset_summary;

        assert_eq!(summary.reference_count, 3);
        assert_eq!(summary.high_confidence_count, 3);
        assert_eq!(summary.medium_confidence_count, 0);
        assert_eq!(summary.low_confidence_count, 0);
        assert_eq!(summary.kind_counts.get("model"), Some(&1));
        assert_eq!(summary.kind_counts.get("motion"), Some(&1));
        assert_eq!(summary.kind_counts.get("audio"), Some(&1));
        assert_eq!(summary.extension_counts.get("pmx"), Some(&1));
        assert_eq!(summary.extension_counts.get("vmd"), Some(&1));
        assert_eq!(summary.extension_counts.get("wav"), Some(&1));
        assert_eq!(summary.confidence_counts.get("high"), Some(&3));
    }

    #[test]
    fn parses_pmm_v2_document_model_keyframe_summary() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.as_ref().unwrap();

        assert_eq!(document.source, "nanoem/ext/document.c PMMv2 layout");
        assert_eq!(document.selected_model_index, 0);
        assert_eq!(document.model_count, 1);
        assert_eq!(document.counts.models, 1);
        assert_eq!(document.counts.bones, 1);
        assert_eq!(document.counts.morphs, 1);
        assert_eq!(document.counts.initial_bone_keyframes, 1);
        assert_eq!(document.counts.bone_keyframes, 1);
        assert_eq!(document.counts.initial_morph_keyframes, 1);
        assert_eq!(document.counts.morph_keyframes, 1);
        assert_eq!(document.counts.initial_model_keyframes, 1);
        assert_eq!(document.counts.model_keyframes, 1);

        let model = &document.models[0];
        assert_eq!(model.slot_index, 0);
        assert_eq!(model.document_model_index, 0);
        assert_eq!(model.name, "Miku");
        assert_eq!(model.english_name, "MikuEn");
        assert_eq!(model.path, "UserFile\\Model\\miku.pmx");
        let asset_reference_index = model.asset_reference_index.unwrap();
        assert_eq!(
            parsed.asset_references[asset_reference_index].normalized_path,
            "UserFile/Model/miku.pmx"
        );
        assert_eq!(model.bone_count, 1);
        assert_eq!(model.morph_count, 1);
        assert_eq!(model.constraint_bone_count, 1);
        assert_eq!(model.outside_parent_subject_bone_count, 1);
        assert_eq!(model.bone_keyframes, 1);
        assert_eq!(model.morph_keyframes, 1);
        assert_eq!(model.model_keyframes, 1);
        assert_eq!(model.initial_model_keyframe.index, None);
        assert_eq!(model.initial_model_keyframe.frame_index, 0);
        assert_eq!(model.initial_model_keyframe.previous_keyframe_index, 0);
        assert_eq!(model.initial_model_keyframe.next_keyframe_index, 0);
        assert!(model.initial_model_keyframe.visible);
        assert_eq!(model.initial_model_keyframe.constraint_states, vec![true]);
        assert_eq!(
            model.initial_model_keyframe.outside_parent_indices[0].parent_model_index,
            2
        );
        assert_eq!(
            model.initial_model_keyframe.outside_parent_indices[0].parent_model_bone_index,
            3
        );
        assert!(model.initial_model_keyframe.self_shadow_enabled);
        assert_eq!(model.model_keyframe_summaries.len(), 1);
        assert_eq!(model.model_keyframe_summaries[0].index, Some(30));
        assert_eq!(model.model_keyframe_summaries[0].frame_index, 30);
        assert!(model.model_keyframe_summaries[0].visible);
        assert_eq!(
            model.model_keyframe_summaries[0].constraint_states,
            vec![true]
        );
        assert_eq!(
            model.model_keyframe_summaries[0].outside_parent_indices[0].parent_model_index,
            2
        );
        assert_eq!(
            model.model_keyframe_summaries[0].outside_parent_indices[0].parent_model_bone_index,
            3
        );
        assert!(model.model_keyframe_summaries[0].self_shadow_enabled);
        assert_eq!(model.draw_order_index, 0);
        assert_eq!(model.transform_order_index, 0);
        assert_eq!(model.selected_bone_index, 0);
        assert_eq!(model.selected_morph_indices, [-1, -1, -1, -1]);
        assert_eq!(model.vertical_scroll, 0);
        assert_eq!(model.last_frame_index, 30);
        assert!(model.visible);
        assert!(model.blend_enabled);
        assert_eq!(model.edge_width, 1.0);
        assert!(!model.self_shadow_enabled);
        assert!(model.offset_end > model.offset);
        assert!(model.path_offset > model.offset);
        assert!(model.sections.initial_bone_keyframes_offset > model.path_offset);
        assert!(model.sections.bone_keyframes_offset > model.sections.bone_keyframe_count_offset);
        assert!(model.sections.bone_keyframes_end_offset > model.sections.bone_keyframes_offset);
        assert!(model.sections.morph_keyframes_offset > model.sections.morph_keyframe_count_offset);
        assert!(model.sections.model_keyframes_offset > model.sections.model_keyframe_count_offset);
        assert!(
            model.sections.outside_parent_states_offset >= model.sections.constraint_states_offset
        );
        let document_diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "PMM_DOCUMENT_SUMMARY_PARTIAL")
            .unwrap();
        assert!(
            document_diagnostic
                .message
                .contains("2 total bone keyframe(s) (1 additional)")
        );
        assert!(
            document_diagnostic
                .message
                .contains("2 total morph keyframe(s) (1 additional)")
        );
    }

    #[test]
    fn pmm_document_bone_keyframe_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let model = &document.models[0];
        let keys =
            json_keys(&serde_json::to_value(&model.initial_bone_keyframe_summaries[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "frameIndex",
                "index",
                "interpolation",
                "nextKeyframeIndex",
                "orientation",
                "physicsDisabled",
                "previousKeyframeIndex",
                "selected",
                "translation"
            ]
        );
    }

    #[test]
    fn pmm_document_morph_keyframe_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let model = &document.models[0];
        let keys =
            json_keys(&serde_json::to_value(&model.initial_morph_keyframe_summaries[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "frameIndex",
                "index",
                "nextKeyframeIndex",
                "previousKeyframeIndex",
                "selected",
                "weight"
            ]
        );
    }

    #[test]
    fn parses_pmm_v2_document_bone_morph_keyframe_summaries() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.as_ref().unwrap();
        let model = &document.models[0];

        assert_eq!(model.initial_bone_keyframe_summaries.len(), 1);
        let ibk = &model.initial_bone_keyframe_summaries[0];
        assert_eq!(ibk.index, None);
        assert_eq!(ibk.frame_index, 0);
        assert_eq!(ibk.previous_keyframe_index, 0);
        assert_eq!(ibk.next_keyframe_index, 0);
        assert_eq!(ibk.interpolation, [20u8; 16]);
        assert_eq!(ibk.translation, [0.0, 0.0, 0.0]);
        assert_eq!(ibk.orientation, [0.0, 0.0, 0.0, 1.0]);
        assert!(!ibk.physics_disabled);
        assert!(!ibk.selected);

        assert_eq!(model.bone_keyframe_summaries.len(), 1);
        let bk = &model.bone_keyframe_summaries[0];
        assert_eq!(bk.index, Some(0));
        assert_eq!(bk.frame_index, 30);
        assert_eq!(bk.previous_keyframe_index, 0);
        assert_eq!(bk.next_keyframe_index, 0);
        assert_eq!(bk.interpolation, [20u8; 16]);
        assert_eq!(bk.translation, [0.0, 0.0, 0.0]);
        assert_eq!(bk.orientation, [0.0, 0.0, 0.0, 1.0]);
        assert!(!bk.physics_disabled);
        assert!(!bk.selected);

        assert_eq!(model.initial_morph_keyframe_summaries.len(), 1);
        let imk = &model.initial_morph_keyframe_summaries[0];
        assert_eq!(imk.index, None);
        assert_eq!(imk.frame_index, 0);
        assert_eq!(imk.previous_keyframe_index, 0);
        assert_eq!(imk.next_keyframe_index, 0);
        assert!((imk.weight - 0.5).abs() < 1e-6);
        assert!(!imk.selected);

        assert_eq!(model.morph_keyframe_summaries.len(), 1);
        let mk = &model.morph_keyframe_summaries[0];
        assert_eq!(mk.index, Some(0));
        assert_eq!(mk.frame_index, 15);
        assert_eq!(mk.previous_keyframe_index, 0);
        assert_eq!(mk.next_keyframe_index, 0);
        assert!((mk.weight - 0.5).abs() < 1e-6);
        assert!(!mk.selected);
    }

    #[test]
    fn pmm_v2_document_summary_rejects_selected_model_index_out_of_range() {
        let mut data = pmm_with_document_summary();
        data[53] = 1;

        let parsed = parse_pmm_manifest(&data).unwrap();

        assert!(parsed.document_summary.is_none());
    }

    #[test]
    fn parses_initial_pmm_model_slot_slice() {
        let parsed = parse_pmm_manifest(&pmm_with_initial_model_slot()).unwrap();

        assert_eq!(parsed.model_slots.len(), 1);
        let slot = &parsed.model_slots[0];
        assert_eq!(slot.slot_index, 0);
        assert_eq!(slot.display_slot_index, Some(1));
        assert_eq!(slot.offset, 56);
        assert_eq!(slot.model_path_offset, 71);
        assert_eq!(slot.trailing_zero_padding_bytes, 0);
        assert_eq!(slot.next_non_zero_offset, Some(slot.offset_end + 1));
        assert_eq!(slot.name, "TestModel");
        assert_eq!(slot.name_bytes, b"TestModel");
        assert_eq!(slot.english_name, "Base");
        assert_eq!(slot.english_name_bytes, b"Base");
        assert_eq!(
            slot.model_path,
            "F:\\Develop\\MMDDev\\data\\unittest\\test_1bone_cube.pmx"
        );
        assert_eq!(
            slot.normalized_path,
            "F:/Develop/MMDDev/data/unittest/test_1bone_cube.pmx"
        );
        assert_eq!(slot.asset_reference_index, Some(0));
        assert_eq!(slot.confidence, "high");
        assert_eq!(parsed.asset_references[0].offset, 56);
        assert_eq!(parsed.motion_paths, vec!["UserFile/Motion/walk.vmd"]);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_INITIAL_SLICE")
        );
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_COUNT_PARTIAL_DECODE")
        );
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_SCAN_STOPPED")
        );
    }

    #[test]
    fn parses_sequential_pmm_model_slot_header_slice() {
        let parsed = parse_pmm_manifest(&pmm_with_two_initial_model_slots()).unwrap();

        assert_eq!(parsed.model_slots.len(), 2);
        assert_eq!(parsed.model_slots[0].slot_index, 0);
        assert_eq!(parsed.model_slots[1].slot_index, 1);
        assert_eq!(parsed.model_slots[0].display_slot_index, Some(0));
        assert_eq!(parsed.model_slots[1].display_slot_index, Some(1));
        assert_eq!(parsed.model_slots[0].trailing_zero_padding_bytes, 0);
        assert_eq!(
            parsed.model_slots[0].next_non_zero_offset,
            Some(parsed.model_slots[1].offset)
        );
        assert_eq!(
            parsed.model_slots[1].offset,
            parsed.model_slots[0].offset_end + 1
        );
        assert_eq!(parsed.model_slots[1].name, "Sour");
        assert_eq!(
            parsed.model_slots[1].model_path,
            "G:\\MikuMikuDance\\MMD Models\\Sour\\sour.pmx"
        );
        assert_eq!(
            parsed.model_slots[1].normalized_path,
            "G:/MikuMikuDance/MMD Models/Sour/sour.pmx"
        );
        assert_eq!(parsed.model_slots[1].asset_reference_index, Some(1));
        assert_eq!(
            parsed.asset_references[1].offset,
            parsed.model_slots[1].offset
        );
        assert!(
            !parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_SCAN_STOPPED")
        );
    }

    #[test]
    fn pmm_model_slot_scan_stops_when_next_asset_reference_is_not_matched() {
        let data = pmm_with_two_initial_model_slots();
        let display_state = parse_display_state(&data, Some(2));
        let references = extract_asset_references(&data);
        let scan = parse_model_slots_from_header(&data, &references[..1], &display_state);

        assert_eq!(scan.slots.len(), 1);
        let stop = scan.stop.unwrap();
        assert_eq!(stop.offset, scan.slots[0].offset_end + 1);
        assert_eq!(stop.reason, "asset_reference_not_matched");
    }

    #[test]
    fn pmm_model_slot_scan_stops_at_implausible_next_name_length() {
        let mut data = pmm_with_initial_model_slot();
        let slot_end = parse_pmm_manifest(&data).unwrap().model_slots[0].offset_end;
        data.truncate(slot_end + 1);
        data.push(255);
        data.extend_from_slice(b"UserFile\\Motion\\walk.vmd\0");
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert_eq!(parsed.model_slots.len(), 1);
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_SCAN_STOPPED")
            .unwrap();
        assert!(diagnostic.message.contains("missing_name"));
    }

    #[test]
    fn pmm_model_slot_scan_stops_at_null_byte_in_next_name() {
        let mut data = pmm_with_initial_model_slot();
        let slot_end = parse_pmm_manifest(&data).unwrap().model_slots[0].offset_end;
        data.truncate(slot_end + 1);
        data.push(4);
        data.extend_from_slice(b"A\0BC");
        data.extend_from_slice(b"UserFile\\Motion\\walk.vmd\0");
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert_eq!(parsed.model_slots.len(), 1);
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_SCAN_STOPPED")
            .unwrap();
        assert!(diagnostic.message.contains("missing_name"));
    }

    #[test]
    fn pmm_model_slot_scan_records_zero_padding_before_binary_graph() {
        let mut data = pmm_with_initial_model_slot();
        let slot_end = parse_pmm_manifest(&data).unwrap().model_slots[0].offset_end;
        data.truncate(slot_end + 1);
        data.extend_from_slice(&[0; 12]);
        data.extend_from_slice(&[15, 34, 3, 0, 0, 8, 145, 128]);
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert_eq!(parsed.model_slots.len(), 1);
        assert_eq!(parsed.model_slots[0].trailing_zero_padding_bytes, 12);
        assert_eq!(
            parsed.model_slots[0].next_non_zero_offset,
            Some(slot_end + 13)
        );
        let diagnostic = parsed
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_SCAN_STOPPED")
            .unwrap();
        assert!(diagnostic.message.contains("missing_name"));
    }

    #[test]
    fn parses_pmm_numeric_version() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();

        assert_eq!(parsed.version, "0002");
        assert_eq!(parsed.parsed_version, Some(2));
    }

    #[test]
    fn parses_pmm_v1_display_count_layout() {
        let parsed = parse_pmm_manifest(&pmm_v1_with_display_counts()).unwrap();

        assert_eq!(parsed.version, "0001");
        assert_eq!(parsed.parsed_version, Some(1));
        assert_eq!(parsed.project_settings.screen_width, Some(512));
        assert_eq!(parsed.project_settings.screen_height, Some(288));
        assert_eq!(parsed.project_settings.timeline_frame_count, Some(435));
        assert_eq!(parsed.project_settings.frame_rate, Some(35.0));
        assert_eq!(parsed.display_state.layout, "pmm-v1-counts");
        assert!(parsed.display_state.model_slot_flags.is_empty());
        assert_eq!(parsed.display_state.declared_model_slot_count, Some(8));
        assert_eq!(parsed.display_state.model_slot_count, 8);
        assert_eq!(parsed.display_state.non_zero_model_slot_count, 8);
        assert_eq!(parsed.display_state.accessory_slot_count, Some(9));
        assert_eq!(parsed.header_text_entries.len(), 2);
        assert_eq!(parsed.header_text_entries[0].index, 0);
        assert_eq!(parsed.header_text_entries[0].offset, 54);
        assert_eq!(parsed.header_text_entries[0].text, "Dummy");
        assert_eq!(parsed.header_text_entries[0].text_bytes, b"Dummy");
        assert_eq!(parsed.header_text_entries[1].text, "maker Dummy");
        assert!(
            !parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_MODEL_SLOT_COUNT_PARTIAL_DECODE")
        );
        assert!(parsed.model_slots.is_empty());
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_ASSET_COUNT_MISMATCH")
        );
        assert!(
            !parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_DISPLAY_STATE_UNPLAUSIBLE")
        );
    }

    #[test]
    fn pmm_v1_invalid_display_counts_do_not_fall_through_to_v2_flags() {
        let mut data = pmm_v1_with_display_counts();
        data[52] = 255;
        data[53] = 255;
        data[54] = 1;
        data[55] = 1;
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert_eq!(parsed.display_state.layout, "unknown");
        assert!(parsed.display_state.model_slot_flags.is_empty());
        assert_eq!(parsed.display_state.declared_model_slot_count, None);
        assert_eq!(parsed.display_state.model_slot_count, 0);
        assert_eq!(parsed.display_state.accessory_slot_count, None);
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_DISPLAY_STATE_UNPLAUSIBLE")
        );
    }

    #[test]
    fn pmm_v1_header_text_entries_require_asset_boundary() {
        let mut data = b"Polygon Movie maker 0001".to_vec();
        data.resize(54, 0);
        data.extend_from_slice(b"Header\0Without asset boundary\0");
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert!(parsed.asset_references.is_empty());
        assert!(parsed.header_text_entries.is_empty());
    }

    #[test]
    fn pmm_v1_header_text_entries_allow_empty_region_before_asset() {
        let mut data = b"Polygon Movie maker 0001".to_vec();
        data.resize(54, 0);
        data.extend_from_slice(b"\0\0\0UserFile\\Model\\miku.pmd\0");
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert_eq!(parsed.model_paths, vec!["UserFile/Model/miku.pmd"]);
        assert!(parsed.header_text_entries.is_empty());
    }

    #[test]
    fn pmm_v1_header_text_entries_ignore_unterminated_chunk_before_asset() {
        let mut data = b"Polygon Movie maker 0001".to_vec();
        data.resize(54, 0);
        data.extend_from_slice(b"Unterminated header UserFile\\Model\\miku.pmd\0");
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert_eq!(parsed.model_paths, vec!["UserFile/Model/miku.pmd"]);
        assert!(parsed.header_text_entries.is_empty());
    }

    #[test]
    fn parses_pmm_bare_asset_file_names() {
        let data = b"Polygon Movie maker 0002\0miku.pmx\0walk.vmd\0song.wav\0stage.avi\0.X\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(parsed.model_paths, vec!["miku.pmx"]);
        assert_eq!(parsed.motion_paths, vec!["walk.vmd"]);
        assert_eq!(parsed.audio_paths, vec!["song.wav"]);
        assert_eq!(parsed.video_paths, vec!["stage.avi"]);
        assert!(parsed.accessory_paths.is_empty());
        assert_eq!(parsed.model_assets[0].confidence, "medium");
        assert_eq!(parsed.model_assets[0].file_name, "miku.pmx");
        assert_eq!(parsed.motion_assets[0].kind_index, 0);
        assert_eq!(parsed.audio_assets[0].reference_index, 2);
        assert_eq!(parsed.video_assets[0].reference_index, 3);
        assert_eq!(parsed.video_assets[0].kind_index, 0);
    }

    #[test]
    fn parses_pmm_background_video_asset_paths() {
        let data =
            b"Polygon Movie maker 0002\0UserFile\\Background\\stage.avs\0C:\\Video\\movie.avi\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(
            parsed.video_paths,
            vec!["UserFile/Background/stage.avs", "C:/Video/movie.avi"]
        );
        assert_eq!(parsed.video_assets.len(), 2);
        assert_eq!(parsed.video_assets[0].extension, "avs");
        assert_eq!(parsed.video_assets[0].confidence, "high");
        assert_eq!(parsed.video_assets[1].extension, "avi");
        assert_eq!(parsed.video_assets[1].file_name, "movie.avi");
    }

    #[test]
    fn parses_pmm_common_audio_image_and_video_asset_extensions() {
        let data = b"Polygon Movie maker 0002\0UserFile\\Audio\\song.mp3\0UserFile\\Audio\\loop.ogg\0UserFile\\Texture\\toon.png\0UserFile\\Texture\\photo.jpg\0UserFile\\Texture\\poster.jpeg\0UserFile\\Texture\\normal.dds\0UserFile\\Movie\\clip.mp4\0UserFile\\Movie\\stage.wmv\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(
            parsed.audio_paths,
            vec!["UserFile/Audio/song.mp3", "UserFile/Audio/loop.ogg"]
        );
        assert_eq!(
            parsed.image_paths,
            vec![
                "UserFile/Texture/toon.png",
                "UserFile/Texture/photo.jpg",
                "UserFile/Texture/poster.jpeg",
                "UserFile/Texture/normal.dds"
            ]
        );
        assert_eq!(
            parsed.video_paths,
            vec!["UserFile/Movie/clip.mp4", "UserFile/Movie/stage.wmv"]
        );
        assert_eq!(parsed.audio_assets[0].extension, "mp3");
        assert_eq!(parsed.audio_assets[1].extension, "ogg");
        assert_eq!(parsed.image_assets[0].extension, "png");
        assert_eq!(parsed.image_assets[1].extension, "jpg");
        assert_eq!(parsed.image_assets[2].extension, "jpeg");
        assert_eq!(parsed.image_assets[3].extension, "dds");
        assert_eq!(parsed.video_assets[0].extension, "mp4");
        assert_eq!(parsed.video_assets[1].extension, "wmv");
    }

    #[test]
    fn pmm_asset_scanner_rejects_partial_common_media_extensions() {
        let data = b"Polygon Movie maker 0002\0song.mp3extra\0toon.png2\0poster.jpeg_backup\0movie.mp4.tmp\0UserFile\\Audio\\song.mp3\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(parsed.audio_paths, vec!["UserFile/Audio/song.mp3"]);
        assert!(parsed.image_paths.is_empty());
        assert!(parsed.video_paths.is_empty());
    }

    #[test]
    fn parses_pmm_asset_paths_with_spaces() {
        let data = b"Polygon Movie maker 0002\0Label C:\\MikuMikuDance\\MMD Models\\Append Miku.pmx\0UserFile\\Motion\\walk cycle.vmd\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(
            parsed.model_paths,
            vec!["C:/MikuMikuDance/MMD Models/Append Miku.pmx"]
        );
        assert_eq!(parsed.motion_paths, vec!["UserFile/Motion/walk cycle.vmd"]);
        assert_eq!(
            parsed.model_assets[0].path,
            "C:\\MikuMikuDance\\MMD Models\\Append Miku.pmx"
        );
        assert_eq!(parsed.model_assets[0].file_name, "Append Miku.pmx");
        assert_eq!(parsed.motion_assets[0].file_name, "walk cycle.vmd");
    }

    #[test]
    fn pmm_asset_reference_marks_replacement_char_as_low_confidence() {
        let data = "Polygon Movie maker 0002\0C:\\Models\\bad�name.pmx\0"
            .as_bytes()
            .to_vec();
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert_eq!(parsed.model_assets[0].confidence, "low");
    }

    #[test]
    fn pmm_asset_reference_marks_root_relative_fragment_as_low_confidence() {
        let data = b"Polygon Movie maker 0002\0\\Model\\miku.pmx\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(parsed.model_assets[0].normalized_path, "/Model/miku.pmx");
        assert_eq!(parsed.model_assets[0].confidence, "low");
    }

    #[test]
    fn pmm_asset_reference_marks_missing_drive_letter_fragment_as_low_confidence() {
        let data =
            b"Polygon Movie maker 0002\0:\\Develop\\MMDDev\\data\\unittest\\test_1bone_cube.pmx\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(
            parsed.model_assets[0].normalized_path,
            ":/Develop/MMDDev/data/unittest/test_1bone_cube.pmx"
        );
        assert_eq!(parsed.model_assets[0].confidence, "low");
    }

    #[test]
    fn pmm_asset_scan_keeps_complete_and_fragmented_duplicate_paths_separate() {
        let data = b"Polygon Movie maker 0002\0F:\\Develop\\MMDDev\\data\\unittest\\test_1bone_cube.pmx\0:\\Develop\\MMDDev\\data\\unittest\\test_1bone_cube.pmx\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(parsed.model_assets.len(), 2);
        assert_eq!(
            parsed.model_assets[0].normalized_path,
            "F:/Develop/MMDDev/data/unittest/test_1bone_cube.pmx"
        );
        assert_eq!(parsed.model_assets[0].confidence, "high");
        assert_eq!(
            parsed.model_assets[1].normalized_path,
            ":/Develop/MMDDev/data/unittest/test_1bone_cube.pmx"
        );
        assert_eq!(parsed.model_assets[1].confidence, "low");
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_ASSET_REFERENCE_DUPLICATE")
        );
    }

    #[test]
    fn pmm_asset_reference_keeps_offset_range() {
        let data = b"Polygon Movie maker 0002\0Label UserFile\\Model\\miku.pmx\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        let reference = &parsed.asset_references[0];
        assert_eq!(reference.offset, 25);
        assert_eq!(reference.offset_end, data.len() - 1);
        assert_eq!(&data[reference.offset_end..reference.offset_end + 1], b"\0");
    }

    #[test]
    fn pmm_asset_reference_offset_end_allows_truncated_final_chunk() {
        let data = b"Polygon Movie maker 0002\0UserFile\\Model\\miku.pmx";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(parsed.asset_references[0].offset_end, data.len());
    }

    #[test]
    fn pmm_diagnostics_flags_duplicate_asset_by_filename() {
        let data = b"Polygon Movie maker 0002\0C:\\Models\\miku.pmx\0UserFile\\Model\\miku.pmx\0UserFile\\Motion\\walk.vmd\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_ASSET_REFERENCE_DUPLICATE")
        );
    }

    #[test]
    fn pmm_diagnostics_flags_missing_motion_when_models_present() {
        let data = b"Polygon Movie maker 0002\0UserFile\\Model\\miku.pmx\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "PMM_MOTION_REFERENCES_NOT_FOUND_IN_SCAN")
        );
    }

    #[test]
    fn pmm_diagnostics_flags_declared_asset_count_mismatch() {
        let parsed = parse_pmm_manifest(&pmm_v1_with_display_counts()).unwrap();
        let mismatches = parsed
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "PMM_ASSET_COUNT_MISMATCH")
            .collect::<Vec<_>>();

        assert_eq!(mismatches.len(), 2);
        assert_eq!(mismatches[0].level, "info");
        assert_eq!(mismatches[1].level, "info");
        assert!(mismatches[0].message.contains("8 model slot"));
        assert!(mismatches[0].message.contains("1 model reference"));
        assert!(mismatches[1].message.contains("9 accessory slot"));
        assert!(mismatches[1].message.contains("1 accessory reference"));
    }

    #[test]
    fn pmm_display_state_derives_active_and_empty_slot_indices_from_flags() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let ds = &parsed.display_state;

        assert_eq!(ds.layout, "pmm-v2-flags");
        assert_eq!(ds.model_slot_flags, vec![1, 1, 2, 0, 0, 0, 0, 0]);
        assert_eq!(ds.selected_model_index, Some(0));
        assert_eq!(ds.document_model_count, Some(3));
        let expand_flags = ds.document_expand_flags.as_ref().unwrap();
        assert!(expand_flags.editing_cla);
        assert!(expand_flags.camera_panel);
        assert!(expand_flags.light_panel);
        assert!(!expand_flags.accessory_panel);
        assert!(!expand_flags.self_shadow_panel);
        assert_eq!(ds.model_slot_flag_entries.len(), 8);
        assert_eq!(ds.model_slot_flag_entries[0].slot_index, 0);
        assert_eq!(ds.model_slot_flag_entries[0].flag, 1);
        assert!(ds.model_slot_flag_entries[0].active);
        assert_eq!(ds.model_slot_flag_entries[3].slot_index, 3);
        assert_eq!(ds.model_slot_flag_entries[3].flag, 0);
        assert!(!ds.model_slot_flag_entries[3].active);
        assert_eq!(ds.active_model_slot_indices, vec![0, 1, 2]);
        assert_eq!(ds.empty_model_slot_indices, vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn pmm_display_state_flag_counts_are_grouped_and_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
        let counts = &parsed.display_state.model_slot_flag_counts;

        assert_eq!(counts.get(&0), Some(&5));
        assert_eq!(counts.get(&1), Some(&2));
        assert_eq!(counts.get(&2), Some(&1));
        assert_eq!(counts.len(), 3);

        let json = serde_json::to_value(counts).unwrap();
        let keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(|k| k.as_str())
            .collect();
        assert_eq!(keys, vec!["0", "1", "2"]);
    }

    #[test]
    fn pmm_v1_display_state_derived_arrays_are_empty_when_no_raw_flags() {
        let parsed = parse_pmm_manifest(&pmm_v1_with_display_counts()).unwrap();
        let ds = &parsed.display_state;

        assert_eq!(ds.layout, "pmm-v1-counts");
        assert!(ds.model_slot_flags.is_empty());
        assert!(ds.model_slot_flag_entries.is_empty());
        assert!(ds.document_expand_flags.is_none());
        assert_eq!(ds.selected_model_index, None);
        assert_eq!(ds.document_model_count, None);
        assert!(ds.active_model_slot_indices.is_empty());
        assert!(ds.empty_model_slot_indices.is_empty());
        assert!(ds.model_slot_flag_counts.is_empty());
    }

    #[test]
    fn pmm_unknown_display_state_derived_arrays_are_empty() {
        let mut data = pmm_with_project_settings();
        data[46] = 7;
        let parsed = parse_pmm_manifest(&data).unwrap();
        let ds = &parsed.display_state;

        assert_eq!(ds.layout, "unknown");
        assert!(ds.active_model_slot_indices.is_empty());
        assert!(ds.empty_model_slot_indices.is_empty());
        assert!(ds.model_slot_flag_counts.is_empty());
        assert!(ds.model_slot_flag_entries.is_empty());
    }

    #[test]
    fn pmm_display_state_all_active_slots() {
        let flags: &[u8] = &[1, 1, 1, 1, 1, 1, 1, 1];
        let active = active_slot_indices(flags);
        let empty = empty_slot_indices(flags);
        let counts = slot_flag_counts(flags);

        assert_eq!(active, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(empty.is_empty());
        assert_eq!(counts.get(&1), Some(&8));
        assert_eq!(counts.len(), 1);
    }

    #[test]
    fn pmm_display_state_all_empty_slots() {
        let flags: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0];
        let active = active_slot_indices(flags);
        let empty = empty_slot_indices(flags);
        let counts = slot_flag_counts(flags);

        assert!(active.is_empty());
        assert_eq!(empty, vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(counts.get(&0), Some(&8));
        assert_eq!(counts.len(), 1);
    }

    #[test]
    fn pmm_document_bone_state_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let model = &document.models[0];
        let keys = json_keys(&serde_json::to_value(&model.bone_state_summaries[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "dirty",
                "orientation",
                "physicsDisabled",
                "selected",
                "translation"
            ]
        );
    }

    #[test]
    fn pmm_document_morph_state_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let model = &document.models[0];
        let keys = json_keys(&serde_json::to_value(&model.morph_state_summaries[0]).unwrap());

        assert_eq!(keys, vec!["weight"]);
    }

    #[test]
    fn pmm_document_constraint_state_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let model = &document.models[0];
        let keys = json_keys(&serde_json::to_value(&model.constraint_state_summaries[0]).unwrap());

        assert_eq!(keys, vec!["enabled"]);
    }

    #[test]
    fn pmm_document_outside_parent_state_summary_json_schema_is_stable() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.unwrap();
        let model = &document.models[0];
        let keys =
            json_keys(&serde_json::to_value(&model.outside_parent_state_summaries[0]).unwrap());

        assert_eq!(
            keys,
            vec![
                "parentModelBoneIndex",
                "parentModelIndex",
                "subjectBoneIndex",
                "targetModelIndex"
            ]
        );
    }

    #[test]
    fn parses_pmm_v2_document_model_state_block_summaries() {
        let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
        let document = parsed.document_summary.as_ref().unwrap();
        let model = &document.models[0];

        assert_eq!(model.bone_state_summaries.len(), 1);
        let bs = &model.bone_state_summaries[0];
        assert_eq!(bs.translation, [1.0, 2.0, 3.0]);
        assert_eq!(bs.orientation, [0.0, 0.0, 0.0, 1.0]);
        assert!(bs.dirty);
        assert!(!bs.physics_disabled);
        assert!(bs.selected);

        assert_eq!(model.morph_state_summaries.len(), 1);
        let ms = &model.morph_state_summaries[0];
        assert!((ms.weight - 0.75).abs() < 1e-6);

        assert_eq!(model.constraint_state_summaries.len(), 1);
        let cs = &model.constraint_state_summaries[0];
        assert!(cs.enabled);

        assert_eq!(model.outside_parent_state_summaries.len(), 1);
        let ops = &model.outside_parent_state_summaries[0];
        assert_eq!(ops.parent_model_index, 0);
        assert_eq!(ops.parent_model_bone_index, 5);
        assert_eq!(ops.subject_bone_index, 7);
        assert_eq!(ops.target_model_index, 1);
    }

    #[test]
    fn pmm_asset_scanner_requires_extension_and_userfile_boundaries() {
        let data = b"Polygon Movie maker 0002\0C:\\notuserfile\\Model\\fake.pmx\0dummy.x_v2\0model.x_v2.pmd\0";
        let parsed = parse_pmm_manifest(data).unwrap();

        assert_eq!(
            parsed.model_paths,
            vec!["C:/notuserfile/Model/fake.pmx", "model.x_v2.pmd"]
        );
        assert!(parsed.accessory_paths.is_empty());
    }

    #[test]
    fn ignores_implausible_pmm_model_slot_flags() {
        let mut data = pmm_with_project_settings();
        data[46] = 7;
        let parsed = parse_pmm_manifest(&data).unwrap();

        assert!(parsed.display_state.model_slot_flags.is_empty());
        assert_eq!(parsed.display_state.layout, "unknown");
        assert_eq!(parsed.display_state.declared_model_slot_count, None);
        assert_eq!(parsed.display_state.model_slot_count, 0);
        assert_eq!(parsed.display_state.non_zero_model_slot_count, 0);
        assert_eq!(parsed.display_state.accessory_slot_count, None);
        assert_eq!(parsed.diagnostics.len(), 3);
        assert_eq!(parsed.diagnostics[2].code, "PMM_DISPLAY_STATE_UNPLAUSIBLE");
    }

    #[test]
    fn parses_pmm_v2_accessory_keyframe_scalar_summaries() {
        let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
        let global = parsed.document_global_summary.as_ref().unwrap();
        let accessory = &global.accessories.accessories[0];

        assert_eq!(accessory.keyframe_summaries.len(), 1);

        let initial = &accessory.initial_keyframe;
        assert_eq!(initial.index, None);
        assert_eq!(initial.frame_index, 0);
        assert!(initial.visible);
        assert_eq!(initial.opacity, 1.0);
        assert_eq!(initial.parent_model_index, -1);
        assert_eq!(initial.parent_model_bone_index, -1);
        assert_eq!(initial.translation, [1.0, 2.0, 3.0]);
        assert_eq!(initial.orientation, [0.1, 0.2, 0.3]);
        assert_eq!(initial.scale_factor, 10.0);
        assert!(initial.shadow_enabled);
        assert!(!initial.selected);

        let kf = &accessory.keyframe_summaries[0];
        assert_eq!(kf.index, Some(46));
        assert_eq!(kf.frame_index, 46);
        assert!(kf.visible);
        assert_eq!(kf.opacity, 1.0);
        assert_eq!(kf.translation, [1.0, 2.0, 3.0]);
        assert_eq!(kf.orientation, [0.1, 0.2, 0.3]);
        assert_eq!(kf.scale_factor, 10.0);
        assert!(kf.shadow_enabled);
        assert!(!kf.selected);
    }
}
