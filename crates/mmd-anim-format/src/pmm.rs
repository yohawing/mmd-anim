use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{
    AnimationClip, BoneAnimationBinding, BoneIndex, InterpolationScalar, InterpolationVector3,
    MorphAnimationBinding, MorphIndex, MorphKeyframe, MorphTrack, MovableBoneKeyframe,
    MovableBoneTrack,
};
use serde::Serialize;

use crate::binary::{
    ByteReader, write_f32_le as push_f32, write_fixed_bytes, write_i32_le as push_i32,
    write_u32_le as push_u32,
};
use crate::error::ImportError;
use crate::pmx::PmxParsedModel;
use crate::sjis::{decode_sjis, decode_sjis_trim_nul, encode_sjis, encode_sjis_prefix_fit};
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
    pub project_graph: Option<PmmProjectGraph>,
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
    #[serde(skip)]
    source_bytes: Vec<u8>,
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
pub struct PmmProjectSceneSettings {
    pub offset: usize,
    pub offset_end: usize,
    pub current_frame_index: i32,
    pub current_frame_index_in_text_field: i32,
    pub begin_frame_index_enabled: bool,
    pub end_frame_index_enabled: bool,
    pub begin_frame_index: i32,
    pub end_frame_index: i32,
    pub preferred_fps: f32,
    pub loop_enabled: bool,
    pub audio_enabled: bool,
    pub audio_path: String,
    pub audio_asset_reference_index: Option<usize>,
    pub background_video_enabled: bool,
    pub background_video_path: String,
    pub background_video_asset_reference_index: Option<usize>,
    pub background_video_offset: [i32; 2],
    pub background_video_scale_factor: f32,
    pub background_image_enabled: bool,
    pub background_image_path: String,
    pub background_image_asset_reference_index: Option<usize>,
    pub background_image_offset: [i32; 2],
    pub background_image_scale_factor: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectAssetBinding {
    pub scope: &'static str,
    pub asset_kind: &'static str,
    pub owner_index: Option<usize>,
    pub document_index: Option<u8>,
    pub owner_name: Option<String>,
    pub path: String,
    pub path_offset: Option<usize>,
    pub asset_reference_index: Option<usize>,
    pub asset_reference_offset: Option<usize>,
    pub asset_reference_end_offset: Option<usize>,
    pub asset_reference_confidence: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectExportBlocker {
    pub code: &'static str,
    pub severity: &'static str,
    pub message: String,
    pub scope: Option<&'static str>,
    pub kind: Option<&'static str>,
    pub count: Option<usize>,
    pub coverage_ratio: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectExportReadiness {
    pub lossless_parsed_byte_export_supported: bool,
    pub semantic_graph_export_supported: bool,
    pub source_byte_preservation_required: bool,
    pub blocker_count: usize,
    pub blockers: Vec<PmmProjectExportBlocker>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectGraph {
    pub source: &'static str,
    pub version: String,
    pub parsed_version: Option<u32>,
    pub timeline: PmmTimeline,
    pub display_state: PmmDisplayState,
    pub asset_summary: PmmAssetSummary,
    pub asset_references: Vec<PmmAssetReference>,
    pub models: Vec<PmmDocumentModelSummary>,
    pub global: PmmDocumentGlobalSummary,
    pub model_counts: PmmDocumentCounts,
    pub accessory_count: usize,
    pub accessory_keyframes: usize,
    pub track_references: Vec<PmmProjectTrackReference>,
    pub keyframe_references: Vec<PmmProjectKeyframeReference>,
    pub byte_coverage: PmmProjectByteCoverage,
    pub scene_settings: PmmProjectSceneSettings,
    pub asset_bindings: Vec<PmmProjectAssetBinding>,
    pub export_readiness: PmmProjectExportReadiness,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectTrackReference {
    pub scope: &'static str,
    pub track_kind: &'static str,
    pub owner_index: Option<usize>,
    pub document_index: Option<u8>,
    pub owner_name: Option<String>,
    pub initial_keyframes: usize,
    pub keyframes: usize,
    pub initial_keyframes_offset: Option<usize>,
    pub keyframe_count_offset: Option<usize>,
    pub keyframes_offset: usize,
    pub keyframes_end_offset: usize,
    pub state_offset: Option<usize>,
    pub state_end_offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectKeyframeReference {
    pub scope: &'static str,
    pub track_kind: &'static str,
    pub owner_index: Option<usize>,
    pub document_index: Option<u8>,
    pub owner_name: Option<String>,
    pub initial: bool,
    pub keyframe_index: Option<i32>,
    pub frame_index: i32,
    pub previous_keyframe_index: i32,
    pub next_keyframe_index: i32,
    pub offset: usize,
    pub byte_length: usize,
    pub payload_offset: usize,
    pub payload_byte_length: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectByteRange {
    pub scope: &'static str,
    pub kind: &'static str,
    pub owner_index: Option<usize>,
    pub document_index: Option<u8>,
    pub name: Option<String>,
    pub offset: usize,
    pub offset_end: usize,
    pub byte_length: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PmmProjectByteCoverage {
    pub offset: usize,
    pub offset_end: usize,
    pub byte_length: usize,
    pub covered_byte_length: usize,
    pub coverage_ratio: f32,
    pub gap_count: usize,
    pub gaps: Vec<PmmProjectByteRange>,
    pub range_count: usize,
    pub ranges: Vec<PmmProjectByteRange>,
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
        interpolation: [u8; 24],
        perspective_view: bool,
        fov: i32,
        selected: bool,
        distance_offset: usize,
        look_at_offset: usize,
        look_at_byte_length: usize,
        angle_offset: usize,
        angle_byte_length: usize,
        parent_model_index_offset: usize,
        parent_model_bone_index_offset: usize,
        interpolation_offset: usize,
        interpolation_byte_length: usize,
        perspective_view_offset: usize,
        fov_offset: usize,
        selected_offset: usize,
        offset: usize,
        byte_length: usize,
        payload_offset: usize,
        payload_byte_length: usize,
        payload_bytes: Vec<u8>,
    },
    Light {
        index: Option<i32>,
        frame_index: i32,
        previous_keyframe_index: i32,
        next_keyframe_index: i32,
        color: [f32; 3],
        direction: [f32; 3],
        selected: bool,
        color_offset: usize,
        color_byte_length: usize,
        direction_offset: usize,
        direction_byte_length: usize,
        selected_offset: usize,
        offset: usize,
        byte_length: usize,
        payload_offset: usize,
        payload_byte_length: usize,
        payload_bytes: Vec<u8>,
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
        noise_enabled_offset: usize,
        noise_offset: usize,
        acceleration_offset: usize,
        direction_offset: usize,
        direction_byte_length: usize,
        selected_offset: usize,
        offset: usize,
        byte_length: usize,
        payload_offset: usize,
        payload_byte_length: usize,
        payload_bytes: Vec<u8>,
    },
    SelfShadow {
        index: Option<i32>,
        frame_index: i32,
        previous_keyframe_index: i32,
        next_keyframe_index: i32,
        mode: u8,
        distance: f32,
        selected: bool,
        mode_offset: usize,
        distance_offset: usize,
        selected_offset: usize,
        offset: usize,
        byte_length: usize,
        payload_offset: usize,
        payload_byte_length: usize,
        payload_bytes: Vec<u8>,
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
    pub packed_opacity_visible_offset: usize,
    pub parent_model_index_offset: usize,
    pub parent_model_bone_index_offset: usize,
    pub translation_offset: usize,
    pub translation_byte_length: usize,
    pub orientation_offset: usize,
    pub orientation_byte_length: usize,
    pub scale_factor_offset: usize,
    pub shadow_enabled_offset: usize,
    pub selected_offset: usize,
    pub offset: usize,
    pub byte_length: usize,
    pub payload_offset: usize,
    pub payload_byte_length: usize,
    pub payload_bytes: Vec<u8>,
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
    pub offset: usize,
    pub byte_length: usize,
    pub payload_offset: usize,
    pub payload_byte_length: usize,
    pub payload_bytes: Vec<u8>,
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
    pub offset: usize,
    pub byte_length: usize,
    pub payload_offset: usize,
    pub payload_byte_length: usize,
    pub payload_bytes: Vec<u8>,
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
    pub visible_offset: usize,
    pub constraint_state_count: usize,
    pub constraint_states_offset: usize,
    pub constraint_states_byte_length: usize,
    pub outside_parent_index_count: usize,
    pub outside_parent_indices_offset: usize,
    pub outside_parent_indices_byte_length: usize,
    pub self_shadow_enabled_offset: usize,
    pub offset: usize,
    pub byte_length: usize,
    pub payload_offset: usize,
    pub payload_byte_length: usize,
    pub payload_bytes: Vec<u8>,
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
    pub current_frame_index_offset: usize,
    pub horizontal_scroll: i32,
    pub horizontal_scroll_thumb: i32,
    pub editing_mode: i32,
    pub camera_look_mode: u8,
    pub loop_enabled: bool,
    pub begin_frame_index_enabled: bool,
    pub begin_frame_index_enabled_offset: usize,
    pub end_frame_index_enabled: bool,
    pub end_frame_index_enabled_offset: usize,
    pub begin_frame_index: i32,
    pub begin_frame_index_offset: usize,
    pub end_frame_index: i32,
    pub end_frame_index_offset: usize,
    pub audio_enabled: bool,
    pub audio_path: String,
    pub audio_path_offset: usize,
    pub background_video_offset: [i32; 2],
    pub background_video_scale_factor: f32,
    pub background_video_path: String,
    pub background_video_path_offset: usize,
    pub background_video_enabled: bool,
    pub background_image_offset: [i32; 2],
    pub background_image_scale_factor: f32,
    pub background_image_path: String,
    pub background_image_path_offset: usize,
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
    pub current_frame_index_in_text_field_offset: usize,
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
    pub bone_names: Vec<String>,
    pub morph_count: usize,
    pub morph_names: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PmmDocumentClipBuildError {
    message: String,
}

impl PmmDocumentClipBuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for PmmDocumentClipBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PmmDocumentClipBuildError {}

/// Builds a runtime-neutral clip from one PMMv2 document model.
///
/// PMM document tracks are indexed by the model's stored bone/morph order, so
/// the resulting bindings preserve those indices. Callers must verify that the
/// paired runtime model has the same structure before evaluating the clip.
/// Model visibility, property IK, physics state, and outside-parent state are
/// intentionally outside this diagnostic slice.
pub fn build_pmm_document_model_clip(
    model: &PmmDocumentModelSummary,
) -> Result<AnimationClip, PmmDocumentClipBuildError> {
    if model.initial_bone_keyframe_summaries.len() != model.bone_count {
        return Err(PmmDocumentClipBuildError::new(format!(
            "PMM model {} initial bone keyframe count {} does not match bone count {}",
            model.name,
            model.initial_bone_keyframe_summaries.len(),
            model.bone_count
        )));
    }
    if model.initial_morph_keyframe_summaries.len() != model.morph_count {
        return Err(PmmDocumentClipBuildError::new(format!(
            "PMM model {} initial morph keyframe count {} does not match morph count {}",
            model.name,
            model.initial_morph_keyframe_summaries.len(),
            model.morph_count
        )));
    }

    for (bone_index, initial) in model.initial_bone_keyframe_summaries.iter().enumerate() {
        if let Some(index) = initial.index {
            return Err(PmmDocumentClipBuildError::new(format!(
                "PMM bone {bone_index} initial keyframe must not have explicit index {index}"
            )));
        }
    }
    for (morph_index, initial) in model.initial_morph_keyframe_summaries.iter().enumerate() {
        if let Some(index) = initial.index {
            return Err(PmmDocumentClipBuildError::new(format!(
                "PMM morph {morph_index} initial keyframe must not have explicit index {index}"
            )));
        }
    }

    let additional_bones = indexed_keyframes(
        &model.bone_keyframe_summaries,
        model.bone_count,
        "bone",
        |summary| summary.index,
    )?;
    let mut bone_tracks = Vec::with_capacity(model.bone_count);
    let mut consumed_bone_keyframes = HashSet::with_capacity(additional_bones.len());
    for (bone_index, initial) in model.initial_bone_keyframe_summaries.iter().enumerate() {
        let summaries =
            collect_keyframe_chain(initial, &additional_bones, "bone", bone_index, |summary| {
                (summary.previous_keyframe_index, summary.next_keyframe_index)
            })?;
        let mut keyframes = Vec::with_capacity(summaries.len());
        let mut frames = HashSet::with_capacity(summaries.len());
        for summary in summaries {
            if let Some(index) = summary.index
                && !consumed_bone_keyframes.insert(index)
            {
                return Err(PmmDocumentClipBuildError::new(format!(
                    "PMM additional bone keyframe index {index} is shared by multiple tracks"
                )));
            }
            let frame = pmm_frame_index(summary.frame_index, "bone", bone_index)?;
            if !frames.insert(frame) {
                return Err(PmmDocumentClipBuildError::new(format!(
                    "PMM bone {bone_index} has duplicate frame {frame}"
                )));
            }
            let (position_interpolation, rotation_interpolation) =
                decode_pmm_bone_interpolation(summary.interpolation);
            let position = Vec3A::from_array(summary.translation);
            let rotation = Quat::from_array(summary.orientation);
            if !position.is_finite() {
                return Err(PmmDocumentClipBuildError::new(format!(
                    "PMM bone {bone_index} frame {frame} has non-finite translation"
                )));
            }
            if !rotation.is_finite() || rotation.length_squared() <= f32::EPSILON {
                return Err(PmmDocumentClipBuildError::new(format!(
                    "PMM bone {bone_index} frame {frame} has invalid orientation"
                )));
            }
            keyframes.push(MovableBoneKeyframe {
                frame,
                position,
                rotation,
                position_interpolation,
                rotation_interpolation,
            });
        }
        bone_tracks.push(BoneAnimationBinding {
            bone: BoneIndex(bone_index as u32),
            track: MovableBoneTrack::from_keyframes(keyframes),
        });
    }
    // MMD can retain deleted/edited keys as records whose previous and next
    // links are both zero. Only the chains reachable from each initial key are
    // authoritative. Ignore only fully unlinked records; a remaining link on
    // an unreachable record indicates a disconnected or malformed chain.
    let disconnected_bone_keyframes = additional_bones
        .iter()
        .filter(|(index, summary)| {
            !consumed_bone_keyframes.contains(index)
                && (summary.previous_keyframe_index != 0 || summary.next_keyframe_index != 0)
        })
        .count();
    if disconnected_bone_keyframes != 0 {
        return Err(PmmDocumentClipBuildError::new(format!(
            "PMM model {} has {disconnected_bone_keyframes} disconnected additional bone keyframe(s)",
            model.name
        )));
    }

    let additional_morphs = indexed_keyframes(
        &model.morph_keyframe_summaries,
        model.morph_count,
        "morph",
        |summary| summary.index,
    )?;
    let mut morph_tracks = Vec::with_capacity(model.morph_count);
    let mut consumed_morph_keyframes = HashSet::with_capacity(additional_morphs.len());
    for (morph_index, initial) in model.initial_morph_keyframe_summaries.iter().enumerate() {
        let summaries = collect_keyframe_chain(
            initial,
            &additional_morphs,
            "morph",
            morph_index,
            |summary| (summary.previous_keyframe_index, summary.next_keyframe_index),
        )?;
        let mut keyframes = Vec::with_capacity(summaries.len());
        let mut frames = HashSet::with_capacity(summaries.len());
        for summary in summaries {
            if let Some(index) = summary.index
                && !consumed_morph_keyframes.insert(index)
            {
                return Err(PmmDocumentClipBuildError::new(format!(
                    "PMM additional morph keyframe index {index} is shared by multiple tracks"
                )));
            }
            let frame = pmm_frame_index(summary.frame_index, "morph", morph_index)?;
            if !frames.insert(frame) {
                return Err(PmmDocumentClipBuildError::new(format!(
                    "PMM morph {morph_index} has duplicate frame {frame}"
                )));
            }
            if !summary.weight.is_finite() {
                return Err(PmmDocumentClipBuildError::new(format!(
                    "PMM morph {morph_index} frame {frame} has non-finite weight"
                )));
            }
            keyframes.push(MorphKeyframe::new(frame, summary.weight));
        }
        morph_tracks.push(MorphAnimationBinding {
            morph: MorphIndex(morph_index as u32),
            track: MorphTrack::from_keyframes(keyframes),
        });
    }
    let disconnected_morph_keyframes = additional_morphs
        .iter()
        .filter(|(index, summary)| {
            !consumed_morph_keyframes.contains(index)
                && (summary.previous_keyframe_index != 0 || summary.next_keyframe_index != 0)
        })
        .count();
    if disconnected_morph_keyframes != 0 {
        return Err(PmmDocumentClipBuildError::new(format!(
            "PMM model {} has {disconnected_morph_keyframes} disconnected additional morph keyframe(s)",
            model.name
        )));
    }
    Ok(AnimationClip::new_full(bone_tracks, morph_tracks, None))
}

fn indexed_keyframes<'a, T>(
    summaries: &'a [T],
    track_count: usize,
    track_kind: &str,
    get_index: impl Fn(&T) -> Option<i32>,
) -> Result<HashMap<i32, &'a T>, PmmDocumentClipBuildError> {
    let mut indexed = HashMap::with_capacity(summaries.len());
    for summary in summaries {
        let index = get_index(summary).ok_or_else(|| {
            PmmDocumentClipBuildError::new(format!(
                "PMM additional {track_kind} keyframe is missing its index"
            ))
        })?;
        if index < 0 {
            return Err(PmmDocumentClipBuildError::new(format!(
                "PMM additional {track_kind} keyframe index {index} must be non-negative"
            )));
        }
        if (index as usize) < track_count {
            return Err(PmmDocumentClipBuildError::new(format!(
                "PMM additional {track_kind} keyframe index {index} overlaps initial {track_kind} track index namespace; expected index >= {track_count}"
            )));
        }
        if indexed.insert(index, summary).is_some() {
            return Err(PmmDocumentClipBuildError::new(format!(
                "PMM duplicate additional {track_kind} keyframe index {index}"
            )));
        }
    }
    Ok(indexed)
}

fn collect_keyframe_chain<'a, T>(
    initial: &'a T,
    indexed: &HashMap<i32, &'a T>,
    track_kind: &str,
    track_index: usize,
    links: impl Fn(&T) -> (i32, i32),
) -> Result<Vec<&'a T>, PmmDocumentClipBuildError> {
    let mut result = vec![initial];
    let mut visited = HashSet::new();
    let mut next_index = links(initial).1;
    let mut expected_previous_index = track_index as i32;
    while next_index != 0 {
        let summary = indexed.get(&next_index).copied().ok_or_else(|| {
            PmmDocumentClipBuildError::new(format!(
                "PMM {track_kind} {track_index} keyframe chain references missing index {next_index}"
            ))
        })?;
        if !visited.insert(next_index) {
            return Err(PmmDocumentClipBuildError::new(format!(
                "PMM {track_kind} {track_index} keyframe chain contains a cycle at index {next_index}"
            )));
        }
        let (previous_index, following_index) = links(summary);
        if previous_index != expected_previous_index {
            return Err(PmmDocumentClipBuildError::new(format!(
                "PMM {track_kind} {track_index} keyframe index {next_index} has previous index {previous_index}; expected {expected_previous_index}"
            )));
        }
        result.push(summary);
        expected_previous_index = next_index;
        next_index = following_index;
    }
    Ok(result)
}

fn pmm_frame_index(
    frame_index: i32,
    track_kind: &str,
    track_index: usize,
) -> Result<u32, PmmDocumentClipBuildError> {
    u32::try_from(frame_index).map_err(|_| {
        PmmDocumentClipBuildError::new(format!(
            "PMM {track_kind} {track_index} has negative frame index {frame_index}"
        ))
    })
}

fn decode_pmm_bone_interpolation(
    interpolation: [u8; 16],
) -> (InterpolationVector3, InterpolationScalar) {
    fn scalar(values: &[u8]) -> InterpolationScalar {
        InterpolationScalar {
            x1: values[0].min(127),
            y1: values[1].min(127),
            x2: values[2].min(127),
            y2: values[3].min(127),
        }
    }
    (
        InterpolationVector3 {
            x: scalar(&interpolation[0..4]),
            y: scalar(&interpolation[4..8]),
            z: scalar(&interpolation[8..12]),
        },
        scalar(&interpolation[12..16]),
    )
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

fn initial_and_additional_keyframes<'a, T>(
    initial: &'a [T],
    additional: &'a [T],
) -> impl Iterator<Item = (bool, &'a T)> {
    initial
        .iter()
        .map(|keyframe| (true, keyframe))
        .chain(additional.iter().map(|keyframe| (false, keyframe)))
}

fn make_global_track_ref(
    track: &PmmDocumentTrackSummary,
    kind: &'static str,
) -> PmmProjectTrackReference {
    let initial_kf_offset = track.initial_keyframe.as_ref().map(|kf| match kf {
        PmmDocumentKeyframeSummary::Camera { offset, .. } => *offset,
        PmmDocumentKeyframeSummary::Light { offset, .. } => *offset,
        PmmDocumentKeyframeSummary::Gravity { offset, .. } => *offset,
        PmmDocumentKeyframeSummary::SelfShadow { offset, .. } => *offset,
    });
    PmmProjectTrackReference {
        scope: "global",
        track_kind: kind,
        owner_index: None,
        document_index: None,
        owner_name: None,
        initial_keyframes: track.initial_keyframes,
        keyframes: track.keyframes,
        initial_keyframes_offset: initial_kf_offset,
        keyframe_count_offset: Some(track.keyframe_count_offset),
        keyframes_offset: track.keyframes_offset,
        keyframes_end_offset: track.keyframes_end_offset,
        state_offset: track.state_offset,
        state_end_offset: track.state_end_offset,
    }
}

fn append_global_keyframe_refs(
    refs: &mut Vec<PmmProjectKeyframeReference>,
    track: &PmmDocumentTrackSummary,
    kind: &'static str,
) {
    for (initial, keyframe) in initial_and_additional_keyframes(
        track.initial_keyframe.as_slice(),
        &track.keyframe_summaries,
    ) {
        match keyframe {
            PmmDocumentKeyframeSummary::Camera {
                index,
                frame_index,
                previous_keyframe_index,
                next_keyframe_index,
                offset,
                byte_length,
                payload_offset,
                payload_byte_length,
                ..
            }
            | PmmDocumentKeyframeSummary::Light {
                index,
                frame_index,
                previous_keyframe_index,
                next_keyframe_index,
                offset,
                byte_length,
                payload_offset,
                payload_byte_length,
                ..
            }
            | PmmDocumentKeyframeSummary::Gravity {
                index,
                frame_index,
                previous_keyframe_index,
                next_keyframe_index,
                offset,
                byte_length,
                payload_offset,
                payload_byte_length,
                ..
            }
            | PmmDocumentKeyframeSummary::SelfShadow {
                index,
                frame_index,
                previous_keyframe_index,
                next_keyframe_index,
                offset,
                byte_length,
                payload_offset,
                payload_byte_length,
                ..
            } => refs.push(PmmProjectKeyframeReference {
                scope: "global",
                track_kind: kind,
                owner_index: None,
                document_index: None,
                owner_name: None,
                initial,
                keyframe_index: *index,
                frame_index: *frame_index,
                previous_keyframe_index: *previous_keyframe_index,
                next_keyframe_index: *next_keyframe_index,
                offset: *offset,
                byte_length: *byte_length,
                payload_offset: *payload_offset,
                payload_byte_length: *payload_byte_length,
            }),
        }
    }
}

fn make_global_track_byte_range(
    track: &PmmDocumentTrackSummary,
    kind: &'static str,
) -> PmmProjectByteRange {
    let byte_length = track.offset_end - track.offset;
    PmmProjectByteRange {
        scope: "global",
        kind,
        owner_index: None,
        document_index: None,
        name: None,
        offset: track.offset,
        offset_end: track.offset_end,
        byte_length,
    }
}

fn build_project_track_references(
    doc: &PmmDocumentSummary,
    glob: &PmmDocumentGlobalSummary,
) -> Vec<PmmProjectTrackReference> {
    let mut track_references: Vec<PmmProjectTrackReference> = Vec::new();
    for model in &doc.models {
        let s = &model.sections;
        track_references.push(PmmProjectTrackReference {
            scope: "model",
            track_kind: "bone",
            owner_index: Some(model.slot_index),
            document_index: Some(model.document_model_index),
            owner_name: Some(model.name.clone()),
            initial_keyframes: model.initial_bone_keyframes,
            keyframes: model.bone_keyframes,
            initial_keyframes_offset: Some(s.initial_bone_keyframes_offset),
            keyframe_count_offset: Some(s.bone_keyframe_count_offset),
            keyframes_offset: s.bone_keyframes_offset,
            keyframes_end_offset: s.bone_keyframes_end_offset,
            state_offset: None,
            state_end_offset: None,
        });
        track_references.push(PmmProjectTrackReference {
            scope: "model",
            track_kind: "morph",
            owner_index: Some(model.slot_index),
            document_index: Some(model.document_model_index),
            owner_name: Some(model.name.clone()),
            initial_keyframes: model.initial_morph_keyframes,
            keyframes: model.morph_keyframes,
            initial_keyframes_offset: Some(s.initial_morph_keyframes_offset),
            keyframe_count_offset: Some(s.morph_keyframe_count_offset),
            keyframes_offset: s.morph_keyframes_offset,
            keyframes_end_offset: s.morph_keyframes_end_offset,
            state_offset: None,
            state_end_offset: None,
        });
        track_references.push(PmmProjectTrackReference {
            scope: "model",
            track_kind: "model",
            owner_index: Some(model.slot_index),
            document_index: Some(model.document_model_index),
            owner_name: Some(model.name.clone()),
            initial_keyframes: model.initial_model_keyframes,
            keyframes: model.model_keyframes,
            initial_keyframes_offset: Some(s.initial_model_keyframe_offset),
            keyframe_count_offset: Some(s.model_keyframe_count_offset),
            keyframes_offset: s.model_keyframes_offset,
            keyframes_end_offset: s.model_keyframes_end_offset,
            state_offset: None,
            state_end_offset: None,
        });
    }
    track_references.push(make_global_track_ref(&glob.camera, "camera"));
    track_references.push(make_global_track_ref(&glob.light, "light"));
    track_references.push(make_global_track_ref(&glob.gravity, "gravity"));
    track_references.push(make_global_track_ref(&glob.self_shadow, "selfShadow"));
    for acc in &glob.accessories.accessories {
        track_references.push(PmmProjectTrackReference {
            scope: "accessory",
            track_kind: "accessory",
            owner_index: Some(acc.slot_index),
            document_index: Some(acc.document_accessory_index),
            owner_name: Some(acc.name.clone()),
            initial_keyframes: 1,
            keyframes: acc.keyframes,
            initial_keyframes_offset: Some(acc.initial_keyframe.offset),
            keyframe_count_offset: Some(acc.keyframe_count_offset),
            keyframes_offset: acc.keyframes_offset,
            keyframes_end_offset: acc.keyframes_end_offset,
            state_offset: Some(acc.state_offset),
            state_end_offset: Some(acc.state_end_offset),
        });
    }
    track_references
}

fn build_project_keyframe_references(
    doc: &PmmDocumentSummary,
    glob: &PmmDocumentGlobalSummary,
) -> Vec<PmmProjectKeyframeReference> {
    // Derive the inventory from decoded summaries without copying payload bytes.
    let mut keyframe_references = Vec::new();
    for model in &doc.models {
        for (initial, keyframe) in initial_and_additional_keyframes(
            &model.initial_bone_keyframe_summaries,
            &model.bone_keyframe_summaries,
        ) {
            keyframe_references.push(PmmProjectKeyframeReference {
                scope: "model",
                track_kind: "bone",
                owner_index: Some(model.slot_index),
                document_index: Some(model.document_model_index),
                owner_name: Some(model.name.clone()),
                initial,
                keyframe_index: keyframe.index,
                frame_index: keyframe.frame_index,
                previous_keyframe_index: keyframe.previous_keyframe_index,
                next_keyframe_index: keyframe.next_keyframe_index,
                offset: keyframe.offset,
                byte_length: keyframe.byte_length,
                payload_offset: keyframe.payload_offset,
                payload_byte_length: keyframe.payload_byte_length,
            });
        }
        for (initial, keyframe) in initial_and_additional_keyframes(
            &model.initial_morph_keyframe_summaries,
            &model.morph_keyframe_summaries,
        ) {
            keyframe_references.push(PmmProjectKeyframeReference {
                scope: "model",
                track_kind: "morph",
                owner_index: Some(model.slot_index),
                document_index: Some(model.document_model_index),
                owner_name: Some(model.name.clone()),
                initial,
                keyframe_index: keyframe.index,
                frame_index: keyframe.frame_index,
                previous_keyframe_index: keyframe.previous_keyframe_index,
                next_keyframe_index: keyframe.next_keyframe_index,
                offset: keyframe.offset,
                byte_length: keyframe.byte_length,
                payload_offset: keyframe.payload_offset,
                payload_byte_length: keyframe.payload_byte_length,
            });
        }
        for (initial, keyframe) in initial_and_additional_keyframes(
            std::slice::from_ref(&model.initial_model_keyframe),
            &model.model_keyframe_summaries,
        ) {
            keyframe_references.push(PmmProjectKeyframeReference {
                scope: "model",
                track_kind: "model",
                owner_index: Some(model.slot_index),
                document_index: Some(model.document_model_index),
                owner_name: Some(model.name.clone()),
                initial,
                keyframe_index: keyframe.index,
                frame_index: keyframe.frame_index,
                previous_keyframe_index: keyframe.previous_keyframe_index,
                next_keyframe_index: keyframe.next_keyframe_index,
                offset: keyframe.offset,
                byte_length: keyframe.byte_length,
                payload_offset: keyframe.payload_offset,
                payload_byte_length: keyframe.payload_byte_length,
            });
        }
    }

    append_global_keyframe_refs(&mut keyframe_references, &glob.camera, "camera");
    append_global_keyframe_refs(&mut keyframe_references, &glob.light, "light");
    append_global_keyframe_refs(&mut keyframe_references, &glob.gravity, "gravity");
    append_global_keyframe_refs(&mut keyframe_references, &glob.self_shadow, "selfShadow");

    for accessory in &glob.accessories.accessories {
        for (initial, keyframe) in initial_and_additional_keyframes(
            std::slice::from_ref(&accessory.initial_keyframe),
            &accessory.keyframe_summaries,
        ) {
            keyframe_references.push(PmmProjectKeyframeReference {
                scope: "accessory",
                track_kind: "accessory",
                owner_index: Some(accessory.slot_index),
                document_index: Some(accessory.document_accessory_index),
                owner_name: Some(accessory.name.clone()),
                initial,
                keyframe_index: keyframe.index,
                frame_index: keyframe.frame_index,
                previous_keyframe_index: keyframe.previous_keyframe_index,
                next_keyframe_index: keyframe.next_keyframe_index,
                offset: keyframe.offset,
                byte_length: keyframe.byte_length,
                payload_offset: keyframe.payload_offset,
                payload_byte_length: keyframe.payload_byte_length,
            });
        }
    }
    keyframe_references
}

fn build_project_byte_coverage(
    doc: &PmmDocumentSummary,
    glob: &PmmDocumentGlobalSummary,
) -> PmmProjectByteCoverage {
    // Cover decoded top-level sections. Individual keyframe ranges are listed
    // separately in keyframeReferences.
    let mut bc_ranges: Vec<PmmProjectByteRange> = Vec::new();

    // each decoded document model
    for model in &doc.models {
        bc_ranges.push(PmmProjectByteRange {
            scope: "model",
            kind: "model",
            owner_index: Some(model.slot_index),
            document_index: Some(model.document_model_index),
            name: Some(model.name.clone()),
            offset: model.offset,
            offset_end: model.offset_end,
            byte_length: model.offset_end - model.offset,
        });
    }

    // global root (the document global summary span)
    bc_ranges.push(PmmProjectByteRange {
        scope: "global",
        kind: "root",
        owner_index: None,
        document_index: None,
        name: None,
        offset: glob.offset,
        offset_end: glob.offset_end,
        byte_length: glob.offset_end - glob.offset,
    });

    // global tracks (camera/light/gravity/selfShadow) - use their track.offset..offset_end
    bc_ranges.push(make_global_track_byte_range(&glob.camera, "camera"));
    bc_ranges.push(make_global_track_byte_range(&glob.light, "light"));
    bc_ranges.push(make_global_track_byte_range(&glob.gravity, "gravity"));
    bc_ranges.push(make_global_track_byte_range(
        &glob.self_shadow,
        "selfShadow",
    ));

    // accessory block
    bc_ranges.push(PmmProjectByteRange {
        scope: "global",
        kind: "accessoryBlock",
        owner_index: None,
        document_index: None,
        name: None,
        offset: glob.accessories.offset,
        offset_end: glob.accessories.offset_end,
        byte_length: glob.accessories.offset_end - glob.accessories.offset,
    });

    // each decoded accessory
    for acc in &glob.accessories.accessories {
        bc_ranges.push(PmmProjectByteRange {
            scope: "accessory",
            kind: "accessory",
            owner_index: Some(acc.slot_index),
            document_index: Some(acc.document_accessory_index),
            name: Some(acc.name.clone()),
            offset: acc.offset,
            offset_end: acc.offset_end,
            byte_length: acc.offset_end - acc.offset,
        });
    }

    // Merge overlapping decoded ranges (e.g. global root overlaps global tracks; models/tracks may abut or overlap)
    // before summing for covered_byte_length. Compute unknown gaps within the overall span [bc_offset, bc_offset_end].
    let mut merged: Vec<(usize, usize)> =
        bc_ranges.iter().map(|r| (r.offset, r.offset_end)).collect();
    merged.sort_by_key(|&(s, _)| s);
    let mut merged_intervals: Vec<(usize, usize)> = Vec::new();
    for (start, end) in merged {
        if let Some(last) = merged_intervals.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
            } else {
                merged_intervals.push((start, end));
            }
        } else {
            merged_intervals.push((start, end));
        }
    }
    let covered_byte_length: usize = merged_intervals.iter().map(|&(s, e)| e - s).sum();

    let bc_offset = bc_ranges.iter().map(|r| r.offset).min().unwrap_or(0);
    let bc_offset_end = bc_ranges.iter().map(|r| r.offset_end).max().unwrap_or(0);
    let bc_byte_length = bc_offset_end - bc_offset;

    let mut gaps: Vec<PmmProjectByteRange> = Vec::new();
    let mut cursor = bc_offset;
    for &(start, end) in &merged_intervals {
        if cursor < start {
            gaps.push(PmmProjectByteRange {
                scope: "unknown",
                kind: "gap",
                owner_index: None,
                document_index: None,
                name: None,
                offset: cursor,
                offset_end: start,
                byte_length: start - cursor,
            });
        }
        cursor = cursor.max(end);
    }
    // The span ends at the maximum decoded end, so no trailing gap remains.

    let coverage_ratio = if bc_byte_length == 0 {
        1.0
    } else {
        covered_byte_length as f32 / bc_byte_length as f32
    };

    PmmProjectByteCoverage {
        offset: bc_offset,
        offset_end: bc_offset_end,
        byte_length: bc_byte_length,
        covered_byte_length,
        coverage_ratio,
        gap_count: gaps.len(),
        gaps,
        range_count: bc_ranges.len(),
        ranges: bc_ranges,
    }
}

#[allow(clippy::too_many_arguments)]
fn make_asset_binding(
    scope: &'static str,
    asset_kind: &'static str,
    owner_index: Option<usize>,
    document_index: Option<u8>,
    owner_name: Option<String>,
    path: String,
    path_offset: Option<usize>,
    asset_reference_index: Option<usize>,
    asset_references: &[PmmAssetReference],
) -> PmmProjectAssetBinding {
    let (ar_offset, ar_end, ar_conf) = match asset_reference_index {
        Some(idx) => asset_references.get(idx).map_or((None, None, None), |ar| {
            (Some(ar.offset), Some(ar.offset_end), Some(ar.confidence))
        }),
        None => (None, None, None),
    };
    PmmProjectAssetBinding {
        scope,
        asset_kind,
        owner_index,
        document_index,
        owner_name,
        path,
        path_offset,
        asset_reference_index,
        asset_reference_offset: ar_offset,
        asset_reference_end_offset: ar_end,
        asset_reference_confidence: ar_conf,
    }
}

fn build_project_asset_bindings(
    doc: &PmmDocumentSummary,
    glob: &PmmDocumentGlobalSummary,
    asset_references: &[PmmAssetReference],
) -> Vec<PmmProjectAssetBinding> {
    let mut asset_bindings: Vec<PmmProjectAssetBinding> = Vec::new();

    for model in &doc.models {
        asset_bindings.push(make_asset_binding(
            "model",
            "model",
            Some(model.slot_index),
            Some(model.document_model_index),
            Some(model.name.clone()),
            model.path.clone(),
            Some(model.path_offset),
            model.asset_reference_index,
            asset_references,
        ));
    }
    for acc in &glob.accessories.accessories {
        asset_bindings.push(make_asset_binding(
            "accessory",
            "accessory",
            Some(acc.slot_index),
            Some(acc.document_accessory_index),
            Some(acc.name.clone()),
            acc.path.clone(),
            Some(acc.path_offset),
            acc.asset_reference_index,
            asset_references,
        ));
    }

    let settings = &glob.settings;
    for (kind, path, offset) in [
        ("audio", &settings.audio_path, settings.audio_path_offset),
        (
            "video",
            &settings.background_video_path,
            settings.background_video_path_offset,
        ),
        (
            "image",
            &settings.background_image_path,
            settings.background_image_path_offset,
        ),
    ] {
        if path.is_empty() {
            continue;
        }
        asset_bindings.push(make_asset_binding(
            "sceneSettings",
            kind,
            None,
            None,
            None,
            path.clone(),
            Some(offset),
            asset_reference_index_for_path(asset_references, kind, path),
            asset_references,
        ));
    }

    asset_bindings
}

fn build_project_scene_settings(
    glob: &PmmDocumentGlobalSummary,
    asset_references: &[PmmAssetReference],
) -> PmmProjectSceneSettings {
    let settings = &glob.settings;
    let audio_asset_reference_index = if settings.audio_path.is_empty() {
        None
    } else {
        asset_reference_index_for_path(asset_references, "audio", &settings.audio_path)
    };
    let background_video_asset_reference_index = if settings.background_video_path.is_empty() {
        None
    } else {
        asset_reference_index_for_path(asset_references, "video", &settings.background_video_path)
    };
    let background_image_asset_reference_index = if settings.background_image_path.is_empty() {
        None
    } else {
        asset_reference_index_for_path(asset_references, "image", &settings.background_image_path)
    };

    PmmProjectSceneSettings {
        offset: settings.offset,
        offset_end: settings.offset_end,
        current_frame_index: settings.current_frame_index,
        current_frame_index_in_text_field: settings.current_frame_index_in_text_field,
        begin_frame_index_enabled: settings.begin_frame_index_enabled,
        end_frame_index_enabled: settings.end_frame_index_enabled,
        begin_frame_index: settings.begin_frame_index,
        end_frame_index: settings.end_frame_index,
        preferred_fps: settings.preferred_fps,
        loop_enabled: settings.loop_enabled,
        audio_enabled: settings.audio_enabled,
        audio_path: settings.audio_path.clone(),
        audio_asset_reference_index,
        background_video_enabled: settings.background_video_enabled,
        background_video_path: settings.background_video_path.clone(),
        background_video_asset_reference_index,
        background_video_offset: settings.background_video_offset,
        background_video_scale_factor: settings.background_video_scale_factor,
        background_image_enabled: settings.background_image_enabled,
        background_image_path: settings.background_image_path.clone(),
        background_image_asset_reference_index,
        background_image_offset: settings.background_image_offset,
        background_image_scale_factor: settings.background_image_scale_factor,
    }
}

fn build_project_export_readiness(
    byte_coverage: &PmmProjectByteCoverage,
    asset_bindings: &[PmmProjectAssetBinding],
) -> PmmProjectExportReadiness {
    let mut blockers: Vec<PmmProjectExportBlocker> = Vec::new();
    blockers.push(PmmProjectExportBlocker {
        code: "PMM_SEMANTIC_EXPORTER_UNFINISHED",
        severity: "blocker",
        message: "full semantic graph export/editing is not yet supported".to_string(),
        scope: None,
        kind: None,
        count: None,
        coverage_ratio: None,
    });
    if byte_coverage.gap_count > 0 {
        blockers.push(PmmProjectExportBlocker {
            code: "PMM_DECODED_BYTE_GAPS_REMAIN",
            severity: "warning",
            message: format!(
                "{} decoded byte gaps remain (coverage ratio {:.4})",
                byte_coverage.gap_count, byte_coverage.coverage_ratio
            ),
            scope: Some("projectGraph"),
            kind: Some("byteCoverage"),
            count: Some(byte_coverage.gap_count),
            coverage_ratio: Some(byte_coverage.coverage_ratio),
        });
    }
    let unresolved_count = asset_bindings
        .iter()
        .filter(|binding| binding.asset_reference_index.is_none())
        .count();
    if unresolved_count > 0 {
        blockers.push(PmmProjectExportBlocker {
            code: "PMM_UNRESOLVED_ASSET_BINDINGS",
            severity: "warning",
            message: format!(
                "{} asset bindings have unresolved asset references",
                unresolved_count
            ),
            scope: Some("projectGraph"),
            kind: Some("assetBindings"),
            count: Some(unresolved_count),
            coverage_ratio: None,
        });
    }
    let blocker_count = blockers.len();
    PmmProjectExportReadiness {
        lossless_parsed_byte_export_supported: true,
        semantic_graph_export_supported: false,
        source_byte_preservation_required: true,
        blocker_count,
        blockers,
    }
}

const PMM_MANIFEST_PREFIX: &[u8] = b"Polygon Movie maker ";

fn parse_pmm_manifest_version(data: &[u8]) -> Result<(String, Option<u32>), ImportError> {
    if !data.starts_with(PMM_MANIFEST_PREFIX) {
        return Err(ImportError::InvalidMagic { format: "PMM" });
    }
    let version_bytes = &data[PMM_MANIFEST_PREFIX.len()..data.len().min(32)];
    let version_end = version_bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(version_bytes.len());
    let version = String::from_utf8_lossy(&version_bytes[..version_end])
        .trim()
        .to_owned();
    let parsed_version = version.parse::<u32>().ok();
    Ok((version, parsed_version))
}

pub fn parse_pmm_manifest(data: &[u8]) -> Result<PmmParsedManifest, ImportError> {
    let (version, parsed_version) = parse_pmm_manifest_version(data)?;
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
    let timeline =
        timeline_from_project_settings(&project_settings, document_global_summary.as_ref());
    let asset_summary = asset_summary(&asset_references);
    let diagnostics = pmm_diagnostics(
        &asset_references,
        &model_slot_scan.slots,
        document_summary.as_ref(),
        document_global_summary.as_ref(),
        &display_state,
        model_slot_scan.stop.as_ref(),
        data,
    );
    let project_graph = match (&document_summary, &document_global_summary) {
        (Some(doc), Some(glob)) => {
            let track_references = build_project_track_references(doc, glob);
            let keyframe_references = build_project_keyframe_references(doc, glob);
            let byte_coverage = build_project_byte_coverage(doc, glob);

            // Build assetBindings derived only from already-decoded summaries + asset_references.
            // Exporter-prep read-only slice: connects owners (model/accessory/sceneSettings) to their asset paths + resolved asset reference metadata.
            // No new parsing.
            let asset_bindings = build_project_asset_bindings(doc, glob, &asset_references);
            let scene_settings = build_project_scene_settings(glob, &asset_references);

            // Build exportReadiness immediately before PmmProjectGraph construction (exporter-prep diagnostics only).
            // lossless parsed byte supported; semantic graph export remains false (full exporter unfinished).
            let export_readiness = build_project_export_readiness(&byte_coverage, &asset_bindings);

            Some(PmmProjectGraph {
                source: "mmd-anim-format first PMMv2 document graph DTO slice",
                version: version.clone(),
                parsed_version,
                timeline: timeline.clone(),
                display_state: display_state.clone(),
                asset_summary: asset_summary.clone(),
                asset_references: asset_references.clone(),
                models: doc.models.clone(),
                global: glob.clone(),
                model_counts: doc.counts.clone(),
                accessory_count: glob.accessories.accessory_count,
                accessory_keyframes: glob.accessories.keyframes,
                track_references,
                keyframe_references,
                byte_coverage,
                scene_settings,
                asset_bindings,
                export_readiness,
            })
        }
        _ => None,
    };
    Ok(PmmParsedManifest {
        signature: "Polygon Movie maker",
        version,
        parsed_version,
        byte_length: data.len(),
        timeline,
        project_settings,
        display_state,
        header_text_entries,
        model_slots: model_slot_scan.slots,
        document_summary,
        document_global_summary,
        project_graph,
        asset_summary,
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
        source_bytes: data.to_vec(),
    })
}

pub fn export_pmm_manifest(manifest: &PmmParsedManifest) -> Vec<u8> {
    if !manifest.source_bytes.is_empty() {
        return manifest.source_bytes.clone();
    }
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

    let mut emitted_paths = Vec::<&str>::new();
    for slot in &manifest.model_slots {
        push_pmm_len_prefixed_sjis(&mut out, &slot.name, &slot.name_bytes);
        push_pmm_len_prefixed_sjis(&mut out, &slot.english_name, &slot.english_name_bytes);
        push_pmm_sjis_string(&mut out, &slot.model_path, None);
        out.push(0);
        emitted_paths.push(&slot.normalized_path);
    }

    for reference in &manifest.asset_references {
        if emitted_paths
            .iter()
            .any(|path| path.eq_ignore_ascii_case(&reference.normalized_path))
        {
            continue;
        }
        push_pmm_sjis_string(&mut out, &reference.path, None);
        out.push(0);
        emitted_paths.push(&reference.normalized_path);
    }

    out
}

/// Lossless source-byte patch export for PMMv2 document model paths only.
/// Clones the manifest's source_bytes and overwrites only the 256-byte fixed path
/// region for each specified document_model_index (from the decoded PmmDocumentModelSummary).
/// This is a narrow helper for MMD GUI load-smoke model path policy adjustments.
/// It does not implement full semantic PMM export (which remains unfinished).
pub fn export_pmm_manifest_with_document_model_path_overrides(
    manifest: &PmmParsedManifest,
    overrides: &[(u8, &str)],
) -> Result<Vec<u8>, String> {
    if manifest.source_bytes.is_empty() {
        return Err(
            "parsed source bytes are required for document model path override export".to_owned(),
        );
    }
    let document = manifest.document_summary.as_ref().ok_or_else(|| {
        "PMMv2 document summary is required for document model path override export".to_owned()
    })?;

    let mut patched = manifest.source_bytes.clone();

    for &(document_model_index, new_path) in overrides {
        let matching: Vec<_> = document
            .models
            .iter()
            .filter(|m| m.document_model_index == document_model_index)
            .collect();
        if matching.len() != 1 {
            return Err(format!(
                "exactly one PmmDocumentModelSummary with document_model_index {} is required (found {})",
                document_model_index,
                matching.len()
            ));
        }
        let model = matching[0];
        let start = model.path_offset;
        let end = start + 256;
        if end > patched.len() {
            return Err("document model path field exceeds source byte length".to_owned());
        }
        let field = &mut patched[start..end];
        write_pmm_fixed_sjis_to_slice(field, new_path)?;
    }

    Ok(patched)
}

/// Lossless source-byte patch export for decoded scene frame range fields in PMMv2 document settings.
/// Clones the manifest's source_bytes and overwrites only the exact byte positions recorded in
/// PmmDocumentSettingsSummary (current_frame_index_offset etc). Only fields with Some(...) are patched.
/// i32 values are written little-endian; bool as single byte 0/1. Byte length is preserved.
/// Requires source bytes and PMMv2 global/document settings summary. This is a narrow exporter-prep
/// step and does not implement full semantic PMM export (which remains unfinished).
#[derive(Debug, Clone, Default)]
pub struct PmmSceneFrameRangePatch {
    pub current_frame_index: Option<i32>,
    pub current_frame_index_in_text_field: Option<i32>,
    pub begin_frame_index_enabled: Option<bool>,
    pub end_frame_index_enabled: Option<bool>,
    pub begin_frame_index: Option<i32>,
    pub end_frame_index: Option<i32>,
}

pub fn export_pmm_manifest_with_scene_frame_range_patch(
    manifest: &PmmParsedManifest,
    patch: &PmmSceneFrameRangePatch,
) -> Result<Vec<u8>, String> {
    if manifest.source_bytes.is_empty() {
        return Err(
            "parsed source bytes are required for scene frame range patch export".to_owned(),
        );
    }
    let global = manifest.document_global_summary.as_ref().ok_or_else(|| {
        "PMMv2 global/document settings summary is required for scene frame range patch export"
            .to_owned()
    })?;
    let settings = &global.settings;

    let mut patched = manifest.source_bytes.clone();

    if let Some(v) = patch.current_frame_index {
        let off = settings.current_frame_index_offset;
        if off + 4 > patched.len() {
            return Err("current_frame_index field offset exceeds source byte length".to_owned());
        }
        patched[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    if let Some(v) = patch.current_frame_index_in_text_field {
        let off = settings.current_frame_index_in_text_field_offset;
        if off + 4 > patched.len() {
            return Err(
                "current_frame_index_in_text_field field offset exceeds source byte length"
                    .to_owned(),
            );
        }
        patched[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    if let Some(v) = patch.begin_frame_index_enabled {
        let off = settings.begin_frame_index_enabled_offset;
        if off >= patched.len() {
            return Err(
                "begin_frame_index_enabled field offset exceeds source byte length".to_owned(),
            );
        }
        patched[off] = if v { 1 } else { 0 };
    }
    if let Some(v) = patch.end_frame_index_enabled {
        let off = settings.end_frame_index_enabled_offset;
        if off >= patched.len() {
            return Err(
                "end_frame_index_enabled field offset exceeds source byte length".to_owned(),
            );
        }
        patched[off] = if v { 1 } else { 0 };
    }
    if let Some(v) = patch.begin_frame_index {
        let off = settings.begin_frame_index_offset;
        if off + 4 > patched.len() {
            return Err("begin_frame_index field offset exceeds source byte length".to_owned());
        }
        patched[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }
    if let Some(v) = patch.end_frame_index {
        let off = settings.end_frame_index_offset;
        if off + 4 > patched.len() {
            return Err("end_frame_index field offset exceeds source byte length".to_owned());
        }
        patched[off..off + 4].copy_from_slice(&v.to_le_bytes());
    }

    Ok(patched)
}

/// Lossless source-byte patch export for decoded scene media paths (audio / background video / background image)
/// in PMMv2 document settings.
/// Clones the manifest's source_bytes and overwrites only the exact 256-byte fixed path regions
/// recorded via the new *_path_offset fields in PmmDocumentSettingsSummary. Only fields with Some(...) are patched.
/// Uses write_pmm_fixed_sjis_to_slice to preserve 256-byte length and NUL room. Byte length is preserved.
/// Requires source bytes and PMMv2 global/document settings summary. This is a narrow exporter-prep
/// step and does not implement full semantic PMM export (which remains unfinished).
#[derive(Debug, Clone, Default)]
pub struct PmmSceneMediaPathPatch {
    pub audio_path: Option<String>,
    pub background_video_path: Option<String>,
    pub background_image_path: Option<String>,
}

pub fn export_pmm_manifest_with_scene_media_path_patch(
    manifest: &PmmParsedManifest,
    patch: &PmmSceneMediaPathPatch,
) -> Result<Vec<u8>, String> {
    if manifest.source_bytes.is_empty() {
        return Err(
            "parsed source bytes are required for scene media path patch export".to_owned(),
        );
    }
    let global = manifest.document_global_summary.as_ref().ok_or_else(|| {
        "PMMv2 global/document settings summary is required for scene media path patch export"
            .to_owned()
    })?;
    let settings = &global.settings;

    let mut patched = manifest.source_bytes.clone();

    if let Some(ref p) = patch.audio_path {
        let off = settings.audio_path_offset;
        if off + 256 > patched.len() {
            return Err("audio path field offset exceeds source byte length".to_owned());
        }
        let field = &mut patched[off..off + 256];
        write_pmm_fixed_sjis_to_slice(field, p)?;
    }
    if let Some(ref p) = patch.background_video_path {
        let off = settings.background_video_path_offset;
        if off + 256 > patched.len() {
            return Err("background video path field offset exceeds source byte length".to_owned());
        }
        let field = &mut patched[off..off + 256];
        write_pmm_fixed_sjis_to_slice(field, p)?;
    }
    if let Some(ref p) = patch.background_image_path {
        let off = settings.background_image_path_offset;
        if off + 256 > patched.len() {
            return Err("background image path field offset exceeds source byte length".to_owned());
        }
        let field = &mut patched[off..off + 256];
        write_pmm_fixed_sjis_to_slice(field, p)?;
    }

    Ok(patched)
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

/// Max byte length MMD keeps for a bone/morph name (20-byte NUL-terminated buffer).
const PMM_MODEL_NAME_MAX_BYTES: usize = 19;

/// Normalizes a runtime bone or morph name to the content PMM stores.
///
/// PMM uses a 20-byte Shift-JIS buffer with one byte reserved for the NUL
/// terminator. Prefix fitting is performed on encoded characters so a
/// two-byte Shift-JIS character is never split.
pub fn normalize_pmm_model_item_name(name: &str) -> String {
    decode_sjis(&encode_sjis_prefix_fit(name, PMM_MODEL_NAME_MAX_BYTES))
}

struct PmmExportModelStructure {
    fixed_tracks: u8,
    constraint_bone_indices: Vec<i32>,
    outside_parent_subject_bone_indices: Vec<i32>,
}

fn pmm_export_model_structure_from_pmx(model: &PmxParsedModel) -> PmmExportModelStructure {
    let constraint_bone_indices = model
        .skeleton
        .bones
        .iter()
        .enumerate()
        .filter(|(_, bone)| bone.flags.ik)
        .map(|(index, _)| index as i32)
        .collect();
    let outside_parent_subject_bone_indices = std::iter::once(-1)
        .chain(
            model
                .skeleton
                .bones
                .iter()
                .enumerate()
                .filter(|(_, bone)| pmm_is_outside_parent_subject_bone(bone))
                .map(|(index, _)| index as i32),
        )
        .collect();
    let fixed_tracks = if model.display_frames.is_empty() {
        0
    } else {
        (model.display_frames.len() + 1).min(255) as u8
    };
    PmmExportModelStructure {
        fixed_tracks,
        constraint_bone_indices,
        outside_parent_subject_bone_indices,
    }
}

fn pmm_is_outside_parent_subject_bone(bone: &crate::pmx::PmxParsedBone) -> bool {
    bone.flags.ik || (bone.flags.visible && bone.flags.translatable)
}

fn push_document_outside_parent_current_state(out: &mut Vec<u8>) {
    push_i32(out, 0);
    push_i32(out, 0);
    push_i32(out, -1);
    push_i32(out, 0);
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
    let model_structure = pmm_export_model_structure_from_pmx(model);
    let constraint_count = model_structure.constraint_bone_indices.len();
    let outside_parent_count = model_structure.outside_parent_subject_bone_indices.len();
    let constraint_states = vec![true; constraint_count];
    let outside_parent_keyframe_indices = vec![(-1, 0); outside_parent_count];

    let mut out = b"Polygon Movie maker 0002".to_vec();
    out.resize(30, 0);
    push_u32(&mut out, options.screen_width);
    push_u32(&mut out, options.screen_height);
    push_u32(&mut out, 480); // timeline panel width, not the animation end frame
    push_f32(&mut out, options.frame_rate);
    out.extend_from_slice(&[0, 1, 1, 1, 1, 1, 1]);
    out.push(0);
    out.push(1);

    out.push(0);
    push_pmm_len_prefixed_sjis(&mut out, &model.metadata.name, &[]);
    push_pmm_len_prefixed_sjis(&mut out, &model.metadata.english_name, &[]);
    push_pmm_fixed_sjis(&mut out, model_path, 256);
    out.push(model_structure.fixed_tracks);

    // MMD stores bone/morph names in a 20-byte NUL-terminated buffer, so it caps
    // each name at 19 content bytes (on a char boundary) when it loads the PMX. The
    // PMM must carry the same truncated names or MMD reports "モデル構造が異なります".
    push_i32(&mut out, bone_names.len() as i32);
    for name in bone_names {
        let fitted = encode_sjis_prefix_fit(name, PMM_MODEL_NAME_MAX_BYTES);
        push_pmm_len_prefixed_sjis(&mut out, name, &fitted);
    }
    push_i32(&mut out, morph_names.len() as i32);
    for name in morph_names {
        let fitted = encode_sjis_prefix_fit(name, PMM_MODEL_NAME_MAX_BYTES);
        push_pmm_len_prefixed_sjis(&mut out, name, &fitted);
    }

    push_i32(&mut out, constraint_count as i32);
    for index in &model_structure.constraint_bone_indices {
        push_i32(&mut out, *index);
    }
    push_i32(&mut out, outside_parent_count as i32);
    for index in &model_structure.outside_parent_subject_bone_indices {
        push_i32(&mut out, *index);
    }
    out.push(1); // draw order index (1-based; MMD treats 0 as invalid)
    out.push(1);
    push_i32(&mut out, -1);
    for _ in 0..4 {
        push_i32(&mut out, -1);
    }
    // PMM stores one expansion-state byte for each fixed display track.  MMD
    // indexes this block after accepting the model-structure confirmation, so
    // the count must agree with the fixed-track count even when every flag is
    // false (the default state).
    out.push(model_structure.fixed_tracks);
    out.resize(out.len() + usize::from(model_structure.fixed_tracks), 0);
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

    push_document_model_keyframe(
        &mut out,
        None,
        0,
        0,
        0,
        true,
        &constraint_states,
        &outside_parent_keyframe_indices,
        false,
    );
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

    // Constraint (IK) current states: all enabled.
    out.resize(out.len() + constraint_count, 1);
    for _ in 0..outside_parent_count {
        push_document_outside_parent_current_state(&mut out);
    }

    out.push(0);
    push_f32(&mut out, 1.0);
    out.push(0);
    out.push(1); // transform order index (1-based, single model)

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

fn push_pmm_fixed_sjis(out: &mut Vec<u8>, text: &str, length: usize) {
    let encoded = encode_sjis(text);
    write_fixed_bytes(out, &encoded, length);
}

/// Write Shift-JIS encoded bytes (with zero padding) into an existing fixed-size slice.
/// Leaves room for a trailing NUL if encoded length < slice length.
/// Intended for patching document model path fields in source bytes.
fn write_pmm_fixed_sjis_to_slice(dest: &mut [u8], text: &str) -> Result<(), String> {
    let encoded = encode_sjis(text);
    if encoded.len() >= dest.len() {
        return Err("encoded Shift-JIS path must leave room for NUL terminator".to_owned());
    }
    dest.fill(0);
    let n = encoded.len();
    dest[..n].copy_from_slice(&encoded[..n]);
    Ok(())
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
    push_document_base_keyframe(out, index, frame, previous, next);
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
    push_document_base_keyframe(out, index, frame, previous, next);
    push_f32(out, weight);
    out.push(0);
}

#[allow(clippy::too_many_arguments)]
fn push_document_model_keyframe(
    out: &mut Vec<u8>,
    index: Option<i32>,
    frame: i32,
    previous: i32,
    next: i32,
    visible: bool,
    constraint_states: &[bool],
    outside_parent_indices: &[(i32, i32)],
    self_shadow_enabled: bool,
) {
    push_document_base_keyframe(out, index, frame, previous, next);
    out.push(u8::from(visible));
    for &enabled in constraint_states {
        out.push(u8::from(enabled));
    }
    for &(parent_model_index, parent_model_bone_index) in outside_parent_indices {
        push_i32(out, parent_model_index);
        push_i32(out, parent_model_bone_index);
    }
    out.push(u8::from(self_shadow_enabled));
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
    push_i32(out, 0);
    push_i32(out, 0);
    out.push(0);
    out.push(0);
    out.push(0);
    out.push(0);
    push_i32(out, 0);
    push_i32(out, max_frame as i32);
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

fn push_pmm_len_prefixed_sjis(out: &mut Vec<u8>, text: &str, original_bytes: &[u8]) {
    let bytes = if !original_bytes.is_empty() {
        original_bytes.to_vec()
    } else {
        encode_sjis(text)
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
    out.extend_from_slice(&encode_sjis(text));
}

fn timeline_from_project_settings(
    settings: &PmmProjectSettings,
    global: Option<&PmmDocumentGlobalSummary>,
) -> PmmTimeline {
    let (frame_count, frame_rate) = global
        .map(|global| {
            (
                u32::try_from(global.settings.end_frame_index).ok(),
                Some(global.settings.preferred_fps),
            )
        })
        .unwrap_or((settings.timeline_frame_count, settings.frame_rate));
    let duration_seconds = frame_count
        .zip(frame_rate)
        .and_then(|(frame_count, frame_rate)| {
            if frame_rate > 0.0 {
                Some(frame_count as f32 / frame_rate)
            } else {
                None
            }
        });
    PmmTimeline {
        start_frame: frame_count.map(|_| 0),
        end_frame_exclusive: frame_count,
        frame_count,
        frame_rate,
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
            let text = decode_sjis(bytes).trim().to_owned();
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
    inner: ByteReader<'a>,
}

impl<'a> PmmDocumentCursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            inner: ByteReader::new(data),
        }
    }

    fn offset(&self) -> usize {
        self.inner.pos
    }

    fn data(&self) -> &'a [u8] {
        self.inner.data
    }

    fn read_u8(&mut self) -> Option<u8> {
        self.inner.read_u8().ok()
    }

    fn read_bool(&mut self) -> Option<bool> {
        Some(self.read_u8()? != 0)
    }

    fn read_i32(&mut self) -> Option<i32> {
        self.inner.read_i32_le().ok()
    }

    fn read_f32(&mut self) -> Option<f32> {
        self.inner.read_f32_le().ok()
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
        self.inner.read_bytes(16).ok()?.try_into().ok()
    }

    fn read_bytes24(&mut self) -> Option<[u8; 24]> {
        self.inner.read_bytes(24).ok()?.try_into().ok()
    }

    fn read_variable_string(&mut self) -> Option<String> {
        let length = self.read_u8()? as usize;
        let bytes = self.inner.read_bytes(length).ok()?;
        Some(decode_shift_jis(bytes))
    }

    fn read_fixed_string(&mut self, length: usize) -> Option<String> {
        let bytes = self.inner.read_bytes(length).ok()?;
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Some(decode_shift_jis(&bytes[..end]))
    }

    fn skip(&mut self, length: usize) -> Option<()> {
        self.inner.skip(length).ok()
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
    // This is a model slot, not an index into the serialized model list.
    // Empty/unoccupied selections do not prevent loading the scene tracks.
    let selected_model_index = cursor.read_u8()?;
    let model_count = cursor.read_u8()? as usize;

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
    cursor.read_u8()?; // selected model slot does not affect the stream layout
    let model_count = cursor.read_u8()? as usize;
    for slot_index in 0..model_count {
        read_document_model_summary(&mut cursor, slot_index, references)?;
    }

    let offset = cursor.offset();
    let camera = read_document_camera_summary(&mut cursor)?;
    let light = read_document_light_summary(&mut cursor)?;
    let accessories = read_document_accessory_block_summary(&mut cursor, references)?;
    let settings_before_gravity = read_document_settings_before_gravity(&mut cursor)?;
    let gravity = read_document_gravity_summary(&mut cursor)?;
    let self_shadow = read_document_self_shadow_summary(&mut cursor)?;
    let settings =
        finish_document_settings_summary(settings_before_gravity, &mut cursor, model_count)?;
    let offset_end = cursor.offset();

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
    let offset = cursor.offset();
    let document_model_index = cursor.read_u8()?;
    let name = cursor.read_variable_string()?;
    let english_name = cursor.read_variable_string()?;
    let path_offset = cursor.offset();
    let path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let asset_reference_index = asset_reference_index_for_path(references, "model", &path);
    cursor.read_u8()?;
    let bone_count = usize_from_i32(cursor.read_i32()?)?;
    let mut bone_names = Vec::new();
    for _ in 0..bone_count {
        bone_names.push(cursor.read_variable_string()?);
    }
    let morph_count = usize_from_i32(cursor.read_i32()?)?;
    let mut morph_names = Vec::new();
    for _ in 0..morph_count {
        morph_names.push(cursor.read_variable_string()?);
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

    let initial_bone_keyframes_offset = cursor.offset();
    let initial_bone_keyframes = bone_count;
    let mut initial_bone_keyframe_summaries = Vec::with_capacity(initial_bone_keyframes);
    for _ in 0..initial_bone_keyframes {
        initial_bone_keyframe_summaries.push(read_document_bone_keyframe(cursor, false)?);
    }
    let bone_keyframe_count_offset = cursor.offset();
    let bone_keyframes = usize_from_i32(cursor.read_i32()?)?;
    let bone_keyframes_offset = cursor.offset();
    let mut bone_keyframe_summaries = Vec::with_capacity(bone_keyframes);
    for _ in 0..bone_keyframes {
        bone_keyframe_summaries.push(read_document_bone_keyframe(cursor, true)?);
    }
    let bone_keyframes_end_offset = cursor.offset();

    let initial_morph_keyframes_offset = cursor.offset();
    let initial_morph_keyframes = morph_count;
    let mut initial_morph_keyframe_summaries = Vec::with_capacity(initial_morph_keyframes);
    for _ in 0..initial_morph_keyframes {
        initial_morph_keyframe_summaries.push(read_document_morph_keyframe(cursor, false)?);
    }
    let morph_keyframe_count_offset = cursor.offset();
    let morph_keyframes = usize_from_i32(cursor.read_i32()?)?;
    let morph_keyframes_offset = cursor.offset();
    let mut morph_keyframe_summaries = Vec::with_capacity(morph_keyframes);
    for _ in 0..morph_keyframes {
        morph_keyframe_summaries.push(read_document_morph_keyframe(cursor, true)?);
    }
    let morph_keyframes_end_offset = cursor.offset();

    let initial_model_keyframes = 1;
    let initial_model_keyframe_offset = cursor.offset();
    let initial_model_keyframe = read_document_model_keyframe(
        cursor,
        false,
        constraint_bone_count,
        outside_parent_subject_bone_count,
    )?;
    let model_keyframe_count_offset = cursor.offset();
    let model_keyframes = usize_from_i32(cursor.read_i32()?)?;
    let model_keyframes_offset = cursor.offset();
    let mut model_keyframe_summaries = Vec::with_capacity(model_keyframes);
    for _ in 0..model_keyframes {
        model_keyframe_summaries.push(read_document_model_keyframe(
            cursor,
            true,
            constraint_bone_count,
            outside_parent_subject_bone_count,
        )?);
    }
    let model_keyframes_end_offset = cursor.offset();

    let bone_states_offset = cursor.offset();
    let mut bone_state_summaries = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        bone_state_summaries.push(read_document_bone_state(cursor)?);
    }
    let morph_states_offset = cursor.offset();
    let mut morph_state_summaries = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        morph_state_summaries.push(read_document_morph_state(cursor)?);
    }
    let constraint_states_offset = cursor.offset();
    let mut constraint_state_summaries = Vec::with_capacity(constraint_bone_count);
    for _ in 0..constraint_bone_count {
        constraint_state_summaries.push(read_document_constraint_state(cursor)?);
    }
    let outside_parent_states_offset = cursor.offset();
    let mut outside_parent_state_summaries = Vec::with_capacity(outside_parent_subject_bone_count);
    for _ in 0..outside_parent_subject_bone_count {
        outside_parent_state_summaries.push(read_document_outside_parent_state(cursor)?);
    }

    let blend_enabled = cursor.read_bool()?;
    let edge_width = cursor.read_f32()?;
    let self_shadow_enabled = cursor.read_bool()?;
    let transform_order_index = cursor.read_u8()?;
    let offset_end = cursor.offset();

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
        bone_names,
        morph_count,
        morph_names,
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
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let interpolation = cursor.read_bytes16()?;
    let translation = cursor.read_f32x3()?;
    let orientation = cursor.read_f32x4()?;
    let physics_disabled = cursor.read_bool()?;
    let selected = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
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
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
    })
}

fn read_document_morph_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentMorphKeyframeSummary> {
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let weight = cursor.read_f32()?;
    let selected = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
    Some(PmmDocumentMorphKeyframeSummary {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        weight,
        selected,
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
    })
}

fn read_document_model_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
    constraint_bone_count: usize,
    outside_parent_subject_bone_count: usize,
) -> Option<PmmDocumentModelKeyframeSummary> {
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let visible_offset = cursor.offset();
    let visible = cursor.read_bool()?;
    let constraint_states_offset = cursor.offset();
    let mut constraint_states = Vec::with_capacity(constraint_bone_count);
    for _ in 0..constraint_bone_count {
        constraint_states.push(cursor.read_bool()?);
    }
    let constraint_states_byte_length = cursor.offset() - constraint_states_offset;
    let outside_parent_indices_offset = cursor.offset();
    let mut outside_parent_indices = Vec::with_capacity(outside_parent_subject_bone_count);
    for _ in 0..outside_parent_subject_bone_count {
        outside_parent_indices.push(PmmDocumentOutsideParentIndexSummary {
            parent_model_index: cursor.read_i32()?,
            parent_model_bone_index: cursor.read_i32()?,
        });
    }
    let outside_parent_indices_byte_length = cursor.offset() - outside_parent_indices_offset;
    let self_shadow_enabled_offset = cursor.offset();
    let self_shadow_enabled = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
    let constraint_state_count = constraint_bone_count;
    let outside_parent_index_count = outside_parent_subject_bone_count;
    Some(PmmDocumentModelKeyframeSummary {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        visible,
        constraint_states,
        outside_parent_indices,
        self_shadow_enabled,
        visible_offset,
        constraint_state_count,
        constraint_states_offset,
        constraint_states_byte_length,
        outside_parent_index_count,
        outside_parent_indices_offset,
        outside_parent_indices_byte_length,
        self_shadow_enabled_offset,
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
    })
}

fn read_document_track_summary(
    cursor: &mut PmmDocumentCursor<'_>,
    read_keyframe: impl Fn(&mut PmmDocumentCursor<'_>, bool) -> Option<PmmDocumentKeyframeSummary>,
) -> Option<PmmDocumentTrackSummary> {
    let offset = cursor.offset();
    let initial_keyframe = read_keyframe(cursor, false)?;
    let keyframe_count_offset = cursor.offset();
    let keyframes = usize_from_i32(cursor.read_i32()?)?;
    let keyframes_offset = cursor.offset();
    let mut keyframe_summaries = Vec::with_capacity(keyframes);
    for _ in 0..keyframes {
        keyframe_summaries.push(read_keyframe(cursor, true)?);
    }
    let keyframes_end_offset = cursor.offset();
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
        state_offset: None,
        state_end_offset: None,
    })
}

fn read_document_camera_summary(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentTrackSummary> {
    let mut summary = read_document_track_summary(cursor, read_document_camera_keyframe)?;
    summary.state_offset = Some(cursor.offset());
    cursor.skip(12 * 3)?;
    cursor.read_bool()?;
    summary.offset_end = cursor.offset();
    summary.state_end_offset = Some(summary.offset_end);
    Some(summary)
}

fn read_document_camera_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let distance_offset = cursor.offset();
    let distance = cursor.read_f32()?;
    let look_at_offset = cursor.offset();
    let look_at = cursor.read_f32x3()?;
    let look_at_byte_length = cursor.offset() - look_at_offset;
    let angle_offset = cursor.offset();
    let angle = cursor.read_f32x3()?;
    let angle_byte_length = cursor.offset() - angle_offset;
    let parent_model_index_offset = cursor.offset();
    let parent_model_index = cursor.read_i32()?;
    let parent_model_bone_index_offset = cursor.offset();
    let parent_model_bone_index = cursor.read_i32()?;
    let interpolation_offset = cursor.offset();
    let interpolation = cursor.read_bytes24()?;
    let interpolation_byte_length = cursor.offset() - interpolation_offset;
    let perspective_view_offset = cursor.offset();
    let perspective_view = !cursor.read_bool()?;
    let fov_offset = cursor.offset();
    let fov = cursor.read_i32()?;
    let selected_offset = cursor.offset();
    let selected = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
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
        interpolation,
        perspective_view,
        fov,
        selected,
        distance_offset,
        look_at_offset,
        look_at_byte_length,
        angle_offset,
        angle_byte_length,
        parent_model_index_offset,
        parent_model_bone_index_offset,
        interpolation_offset,
        interpolation_byte_length,
        perspective_view_offset,
        fov_offset,
        selected_offset,
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
    })
}

fn read_document_light_summary(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentTrackSummary> {
    let mut summary = read_document_track_summary(cursor, read_document_light_keyframe)?;
    summary.state_offset = Some(cursor.offset());
    cursor.skip(12)?;
    cursor.skip(12)?;
    summary.offset_end = cursor.offset();
    summary.state_end_offset = Some(summary.offset_end);
    Some(summary)
}

fn read_document_light_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let color_offset = cursor.offset();
    let color = cursor.read_f32x3()?;
    let color_byte_length = cursor.offset() - color_offset;
    let direction_offset = cursor.offset();
    let direction = cursor.read_f32x3()?;
    let direction_byte_length = cursor.offset() - direction_offset;
    let selected_offset = cursor.offset();
    let selected = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
    Some(PmmDocumentKeyframeSummary::Light {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        color,
        direction,
        selected,
        color_offset,
        color_byte_length,
        direction_offset,
        direction_byte_length,
        selected_offset,
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
    })
}

fn read_document_accessory_block_summary(
    cursor: &mut PmmDocumentCursor<'_>,
    references: &[PmmAssetReference],
) -> Option<PmmDocumentAccessoryBlockSummary> {
    let offset = cursor.offset();
    let selected_accessory_index = cursor.read_u8()?;
    let horizontal_scroll = cursor.read_i32()?;
    let accessory_count = cursor.read_u8()? as usize;
    let mut accessories = Vec::with_capacity(accessory_count);
    for slot_index in 0..accessory_count {
        accessories.push(read_document_accessory_summary(
            cursor, slot_index, references,
        )?);
    }
    let offset_end = cursor.offset();
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
    let offset = cursor.offset();
    let document_accessory_index = cursor.read_u8()?;
    let name = cursor.read_fixed_string(PMM_ACCESSORY_NAME_BYTE_LENGTH)?;
    let path_offset = cursor.offset();
    let path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let asset_reference_index = asset_reference_index_for_path(references, "accessory", &path);
    let draw_order_index = cursor.read_u8()?;
    let initial_keyframe = read_document_accessory_keyframe(cursor, false)?;
    let keyframe_count_offset = cursor.offset();
    let keyframes = usize_from_i32(cursor.read_i32()?)?;
    let keyframes_offset = cursor.offset();
    let mut keyframe_summaries = Vec::with_capacity(keyframes);
    for _ in 0..keyframes {
        keyframe_summaries.push(read_document_accessory_keyframe(cursor, true)?);
    }
    let keyframes_end_offset = cursor.offset();

    let state_offset = cursor.offset();
    let (opacity, visible) = unpack_document_accessory_opacity_and_visible(cursor.read_u8()?);
    let parent_model_index = cursor.read_i32()?;
    let parent_model_bone_index = cursor.read_i32()?;
    cursor.skip(12)?;
    let scale_factor = cursor.read_f32()?;
    cursor.skip(12)?;
    let shadow_enabled = cursor.read_bool()?;
    let add_blend_enabled = cursor.read_bool()?;
    let state_end_offset = cursor.offset();
    let offset_end = cursor.offset();

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
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let packed_opacity_visible_offset = cursor.offset();
    let (opacity, visible) = unpack_document_accessory_opacity_and_visible(cursor.read_u8()?);
    let parent_model_index_offset = cursor.offset();
    let parent_model_index = cursor.read_i32()?;
    let parent_model_bone_index_offset = cursor.offset();
    let parent_model_bone_index = cursor.read_i32()?;
    let translation_offset = cursor.offset();
    let translation = cursor.read_f32x3()?;
    let translation_byte_length = cursor.offset() - translation_offset;
    let orientation_offset = cursor.offset();
    let orientation = cursor.read_f32x3()?;
    let orientation_byte_length = cursor.offset() - orientation_offset;
    let scale_factor_offset = cursor.offset();
    let scale_factor = cursor.read_f32()?;
    let shadow_enabled_offset = cursor.offset();
    let shadow_enabled = cursor.read_bool()?;
    let selected_offset = cursor.offset();
    let selected = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
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
        packed_opacity_visible_offset,
        parent_model_index_offset,
        parent_model_bone_index_offset,
        translation_offset,
        translation_byte_length,
        orientation_offset,
        orientation_byte_length,
        scale_factor_offset,
        shadow_enabled_offset,
        selected_offset,
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
    })
}

struct PmmDocumentSettingsBeforeGravity {
    offset: usize,
    current_frame_index: i32,
    current_frame_index_offset: usize,
    horizontal_scroll: i32,
    horizontal_scroll_thumb: i32,
    editing_mode: i32,
    camera_look_mode: u8,
    loop_enabled: bool,
    begin_frame_index_enabled: bool,
    begin_frame_index_enabled_offset: usize,
    end_frame_index_enabled: bool,
    end_frame_index_enabled_offset: usize,
    begin_frame_index: i32,
    begin_frame_index_offset: usize,
    end_frame_index: i32,
    end_frame_index_offset: usize,
    audio_enabled: bool,
    audio_path: String,
    audio_path_offset: usize,
    background_video_offset: [i32; 2],
    background_video_scale_factor: f32,
    background_video_path: String,
    background_video_path_offset: usize,
    background_video_enabled: bool,
    background_image_offset: [i32; 2],
    background_image_scale_factor: f32,
    background_image_path: String,
    background_image_path_offset: usize,
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
    let offset = cursor.offset();
    let current_frame_index_offset = cursor.offset();
    let current_frame_index = cursor.read_i32()?;
    let horizontal_scroll = cursor.read_i32()?;
    let horizontal_scroll_thumb = cursor.read_i32()?;
    let editing_mode = cursor.read_i32()?;
    let camera_look_mode = cursor.read_u8()?;
    let loop_enabled = cursor.read_bool()?;
    let begin_frame_index_enabled_offset = cursor.offset();
    let begin_frame_index_enabled = cursor.read_bool()?;
    let end_frame_index_enabled_offset = cursor.offset();
    let end_frame_index_enabled = cursor.read_bool()?;
    let begin_frame_index_offset = cursor.offset();
    let begin_frame_index = cursor.read_i32()?;
    let end_frame_index_offset = cursor.offset();
    let end_frame_index = cursor.read_i32()?;
    let audio_enabled = cursor.read_bool()?;
    let audio_path_offset = cursor.offset();
    let audio_path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let background_video_offset = [cursor.read_i32()?, cursor.read_i32()?];
    let background_video_scale_factor = cursor.read_f32()?;
    let background_video_path_offset = cursor.offset();
    let background_video_path = cursor.read_fixed_string(PMM_PATH_BYTE_LENGTH)?;
    let background_video_enabled = cursor.read_i32()? != 0;
    let background_image_offset = [cursor.read_i32()?, cursor.read_i32()?];
    let background_image_scale_factor = cursor.read_f32()?;
    let background_image_path_offset = cursor.offset();
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
        current_frame_index_offset,
        horizontal_scroll,
        horizontal_scroll_thumb,
        editing_mode,
        camera_look_mode,
        loop_enabled,
        begin_frame_index_enabled,
        begin_frame_index_enabled_offset,
        end_frame_index_enabled,
        end_frame_index_enabled_offset,
        begin_frame_index,
        begin_frame_index_offset,
        end_frame_index,
        end_frame_index_offset,
        audio_enabled,
        audio_path,
        audio_path_offset,
        background_video_offset,
        background_video_scale_factor,
        background_video_path,
        background_video_path_offset,
        background_video_enabled,
        background_image_offset,
        background_image_scale_factor,
        background_image_path,
        background_image_path_offset,
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
    let state_offset = cursor.offset();
    cursor.read_f32()?;
    cursor.read_i32()?;
    cursor.skip(12)?;
    cursor.read_bool()?;
    let state_end_offset = cursor.offset();
    let mut summary = read_document_track_summary(cursor, read_document_gravity_keyframe)?;
    summary.offset = state_offset;
    summary.state_offset = Some(state_offset);
    summary.state_end_offset = Some(state_end_offset);
    Some(summary)
}

fn read_document_gravity_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let noise_enabled_offset = cursor.offset();
    let noise_enabled = cursor.read_bool()?;
    let noise_offset = cursor.offset();
    let noise = cursor.read_i32()?;
    let acceleration_offset = cursor.offset();
    let acceleration = cursor.read_f32()?;
    let direction_offset = cursor.offset();
    let direction = cursor.read_f32x3()?;
    let direction_byte_length = cursor.offset() - direction_offset;
    let selected_offset = cursor.offset();
    let selected = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
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
        noise_enabled_offset,
        noise_offset,
        acceleration_offset,
        direction_offset,
        direction_byte_length,
        selected_offset,
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
    })
}

fn read_document_self_shadow_summary(
    cursor: &mut PmmDocumentCursor<'_>,
) -> Option<PmmDocumentTrackSummary> {
    let state_offset = cursor.offset();
    cursor.read_bool()?;
    cursor.read_f32()?;
    let state_end_offset = cursor.offset();
    let mut summary = read_document_track_summary(cursor, read_document_self_shadow_keyframe)?;
    summary.offset = state_offset;
    summary.state_offset = Some(state_offset);
    summary.state_end_offset = Some(state_end_offset);
    Some(summary)
}

fn read_document_self_shadow_keyframe(
    cursor: &mut PmmDocumentCursor<'_>,
    include_index: bool,
) -> Option<PmmDocumentKeyframeSummary> {
    let offset = cursor.offset();
    let base = read_document_base_keyframe(cursor, include_index)?;
    let payload_offset = cursor.offset();
    let mode_offset = cursor.offset();
    let mode = cursor.read_u8()?;
    let distance_offset = cursor.offset();
    let distance = cursor.read_f32()?;
    let selected_offset = cursor.offset();
    let selected = cursor.read_bool()?;
    let byte_length = cursor.offset() - offset;
    let payload_byte_length = cursor.offset() - payload_offset;
    let payload_bytes = cursor.data().get(payload_offset..cursor.offset())?.to_vec();
    Some(PmmDocumentKeyframeSummary::SelfShadow {
        index: base.index,
        frame_index: base.frame_index,
        previous_keyframe_index: base.previous_keyframe_index,
        next_keyframe_index: base.next_keyframe_index,
        mode,
        distance,
        selected,
        mode_offset,
        distance_offset,
        selected_offset,
        offset,
        byte_length,
        payload_offset,
        payload_byte_length,
        payload_bytes,
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
    let unknown_matrix_offset = cursor.offset();
    cursor.skip(16 * 4)?;
    let unknown_matrix_end_offset = cursor.offset();
    let following_look_at_enabled = cursor.read_bool()?;
    cursor.read_bool()?;
    let physics_ground_enabled = cursor.read_bool()?;
    let current_frame_index_in_text_field_offset = cursor.offset();
    let current_frame_index_in_text_field = cursor.read_i32()?;

    let mut model_selection_footer_present = false;
    let mut model_selection_footer_offset = None;
    let mut model_selection_footer_end_offset = None;
    if cursor.offset() < cursor.data().len() {
        let footer_offset = cursor.offset();
        if cursor.read_u8()? != 0 {
            model_selection_footer_present = true;
            model_selection_footer_offset = Some(footer_offset);
            for _ in 0..model_count {
                cursor.read_u8()?;
                cursor.read_i32()?;
            }
            model_selection_footer_end_offset = Some(cursor.offset());
        }
    }
    let offset_end = cursor.offset();

    Some(PmmDocumentSettingsSummary {
        offset: before_gravity.offset,
        offset_end,
        current_frame_index: before_gravity.current_frame_index,
        current_frame_index_offset: before_gravity.current_frame_index_offset,
        horizontal_scroll: before_gravity.horizontal_scroll,
        horizontal_scroll_thumb: before_gravity.horizontal_scroll_thumb,
        editing_mode: before_gravity.editing_mode,
        camera_look_mode: before_gravity.camera_look_mode,
        loop_enabled: before_gravity.loop_enabled,
        begin_frame_index_enabled: before_gravity.begin_frame_index_enabled,
        begin_frame_index_enabled_offset: before_gravity.begin_frame_index_enabled_offset,
        end_frame_index_enabled: before_gravity.end_frame_index_enabled,
        end_frame_index_enabled_offset: before_gravity.end_frame_index_enabled_offset,
        begin_frame_index: before_gravity.begin_frame_index,
        begin_frame_index_offset: before_gravity.begin_frame_index_offset,
        end_frame_index: before_gravity.end_frame_index,
        end_frame_index_offset: before_gravity.end_frame_index_offset,
        audio_enabled: before_gravity.audio_enabled,
        audio_path: before_gravity.audio_path,
        audio_path_offset: before_gravity.audio_path_offset,
        background_video_offset: before_gravity.background_video_offset,
        background_video_scale_factor: before_gravity.background_video_scale_factor,
        background_video_path: before_gravity.background_video_path,
        background_video_path_offset: before_gravity.background_video_path_offset,
        background_video_enabled: before_gravity.background_video_enabled,
        background_image_offset: before_gravity.background_image_offset,
        background_image_scale_factor: before_gravity.background_image_scale_factor,
        background_image_path: before_gravity.background_image_path,
        background_image_path_offset: before_gravity.background_image_path_offset,
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
        current_frame_index_in_text_field_offset,
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
    decode_sjis_trim_nul(bytes)
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
    let decoded_path = decode_sjis(&data[path_offset..path_end]);
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
    Some((decode_sjis_trim_nul(bytes), bytes.to_vec(), end))
}

fn extract_asset_references(data: &[u8]) -> Vec<PmmAssetReference> {
    let mut refs = Vec::new();
    let mut chunk_start = 0usize;
    for index in 0..=data.len() {
        if index < data.len() && data[index] != 0 {
            continue;
        }
        if index > chunk_start {
            let decoded = decode_sjis(&data[chunk_start..index]);
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
    let mut by_file = HashMap::<(&str, &str), Vec<(usize, &PmmAssetReference)>>::new();
    for (index, reference) in references.iter().enumerate() {
        by_file
            .entry((&reference.file_name, &reference.extension))
            .or_default()
            .push((index, reference));
    }

    let mut diagnostics = Vec::new();
    for ((file_name, extension), entries) in by_file {
        if entries.len() < 2 {
            continue;
        }
        let first_path = &entries[0].1.normalized_path;
        if entries
            .iter()
            .all(|(_, reference)| reference.normalized_path.eq_ignore_ascii_case(first_path))
        {
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
mod tests;
