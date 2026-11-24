use super::*;

#[test]
fn normalize_pmm_model_item_name_fits_sjis_without_splitting_character() {
    assert_eq!(
        normalize_pmm_model_item_name("123456789あいうえおか"),
        "123456789あいうえお"
    );
}

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
    data.extend_from_slice(b"C:\\MMDDev\\data\\unittest\\test_1bone_cube.pmx");
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
fn exports_pmm_manifest_preserves_v1_input_bytes() {
    let mut data = pmm_with_project_settings();
    data[20..24].copy_from_slice(b"0001");
    let parsed = parse_pmm_manifest(&data).unwrap();
    assert_eq!(parsed.version, "0001");

    let exported = export_pmm_manifest(&parsed);
    assert_eq!(
        exported, data,
        "PMMv1 input must be preserved byte-for-byte"
    );
    let reparsed = parse_pmm_manifest(&exported).unwrap();

    assert_eq!(reparsed.version, "0001");
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
        "C:/MMDDev/data/unittest/test_1bone_cube.pmx"
    );
    assert_eq!(reparsed.motion_paths, vec!["UserFile/Motion/walk.vmd"]);
}

#[test]
fn pmm_export_roundtrip_bytes_v2_document_global() {
    let input = pmm_with_document_global_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let exported = export_pmm_manifest(&parsed);
    assert_eq!(
        exported, input,
        "parse->export must be byte-for-byte for PMMv2 document/global synthetic"
    );
    assert!(parsed.document_summary.is_some());
    assert!(parsed.document_global_summary.is_some());
}

#[test]
fn pmm_export_roundtrip_bytes_v2_document_summary() {
    let input = pmm_with_document_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let exported = export_pmm_manifest(&parsed);
    assert_eq!(
        exported, input,
        "parse->export must be byte-for-byte for PMMv2 document synthetic"
    );
}

#[test]
fn pmm_document_model_path_override_basic() {
    // Uses only repo-local synthetic fixture (pmm_with_document_summary) -- no MMDDumper.
    let input = pmm_with_document_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let document = parsed
        .document_summary
        .as_ref()
        .expect("document_summary required");
    let model0 = &document.models[0];
    assert_eq!(model0.document_model_index, 0);
    let original_path = model0.path.clone();
    let path_offset = model0.path_offset;

    let new_path = "UserFile\\Model\\override_for_gui_smoke.pmx";
    let patched =
        export_pmm_manifest_with_document_model_path_overrides(&parsed, &[(0u8, new_path)])
            .expect("override patch must succeed for valid short path");

    assert_eq!(
        patched.len(),
        input.len(),
        "patched bytes must keep identical length"
    );

    // Only the target 256-byte document model path field may differ.
    let mut changed_positions = Vec::new();
    for (i, (a, b)) in input.iter().zip(patched.iter()).enumerate() {
        if a != b {
            changed_positions.push(i);
        }
    }
    assert!(
        !changed_positions.is_empty(),
        "expected at least the path bytes to change"
    );
    let field_start = path_offset;
    let field_end = path_offset + 256;
    for &pos in &changed_positions {
        assert!(
            pos >= field_start && pos < field_end,
            "changed byte at {} must be inside the document model path field [{}, {})",
            pos,
            field_start,
            field_end
        );
    }

    let reparsed = parse_pmm_manifest(&patched).unwrap();
    let reparsed_doc = reparsed
        .document_summary
        .as_ref()
        .expect("reparsed must have document_summary");
    assert_eq!(
        reparsed_doc.models[0].path, new_path,
        "reparsed document model path must reflect the override"
    );
    assert_ne!(
        reparsed_doc.models[0].path, original_path,
        "reparsed path must differ from original"
    );
}

#[test]
fn pmm_document_model_path_override_too_long_returns_err() {
    let input = pmm_with_document_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    // 256 ASCII chars -> SJIS 256 bytes >=256 -> err (must leave room for NUL)
    let too_long = "X".repeat(256);
    let result =
        export_pmm_manifest_with_document_model_path_overrides(&parsed, &[(0u8, &too_long)]);
    assert!(result.is_err(), "too-long path must error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("NUL") || msg.contains("too long") || msg.contains("leave room"),
        "error message should indicate length/NUL room issue: {}",
        msg
    );
}

#[test]
fn pmm_document_model_path_override_missing_index_returns_err() {
    let input = pmm_with_document_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let result =
        export_pmm_manifest_with_document_model_path_overrides(&parsed, &[(99u8, "some/path.pmx")]);
    assert!(result.is_err(), "missing document_model_index must error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("exactly one") && msg.contains("99"),
        "error should report missing index: {}",
        msg
    );
}

#[test]
fn pmm_document_model_path_override_requires_source_bytes() {
    let input = pmm_with_document_summary();
    let mut parsed = parse_pmm_manifest(&input).unwrap();
    parsed.source_bytes.clear();

    let result = export_pmm_manifest_with_document_model_path_overrides(
        &parsed,
        &[(0u8, "UserFile\\Model\\override_for_gui_smoke.pmx")],
    );
    assert!(result.is_err(), "missing source bytes must error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("source bytes"),
        "error should explain source bytes are required: {}",
        msg
    );
}

#[test]
fn pmm_scene_frame_range_patch_updates_only_expected_bytes() {
    let input = pmm_with_document_global_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let settings = parsed
        .document_global_summary
        .as_ref()
        .expect("fixture must have global summary")
        .settings
        .clone();

    let patch = PmmSceneFrameRangePatch {
        current_frame_index: Some(99),
        current_frame_index_in_text_field: Some(77),
        begin_frame_index_enabled: Some(true),
        end_frame_index_enabled: Some(false),
        begin_frame_index: Some(10),
        end_frame_index: Some(240),
    };
    let patched = export_pmm_manifest_with_scene_frame_range_patch(&parsed, &patch)
        .expect("scene frame range patch must succeed");

    assert_eq!(
        patched.len(),
        input.len(),
        "patch must preserve byte length"
    );

    let reparsed = parse_pmm_manifest(&patched).unwrap();
    let updated = &reparsed
        .document_global_summary
        .as_ref()
        .expect("patched PMM must keep global summary")
        .settings;
    assert_eq!(updated.current_frame_index, 99);
    assert_eq!(updated.current_frame_index_in_text_field, 77);
    assert!(updated.begin_frame_index_enabled);
    assert!(!updated.end_frame_index_enabled);
    assert_eq!(updated.begin_frame_index, 10);
    assert_eq!(updated.end_frame_index, 240);

    let mut allowed_changed = Vec::new();
    allowed_changed
        .extend(settings.current_frame_index_offset..settings.current_frame_index_offset + 4);
    allowed_changed.push(settings.begin_frame_index_enabled_offset);
    allowed_changed.push(settings.end_frame_index_enabled_offset);
    allowed_changed
        .extend(settings.begin_frame_index_offset..settings.begin_frame_index_offset + 4);
    allowed_changed.extend(settings.end_frame_index_offset..settings.end_frame_index_offset + 4);
    allowed_changed.extend(
        settings.current_frame_index_in_text_field_offset
            ..settings.current_frame_index_in_text_field_offset + 4,
    );

    let changed: Vec<usize> = input
        .iter()
        .zip(patched.iter())
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect();

    assert!(
        changed.iter().all(|index| allowed_changed.contains(index)),
        "scene frame range patch changed bytes outside decoded frame range fields: {:?}",
        changed
            .iter()
            .filter(|index| !allowed_changed.contains(index))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        &patched[settings.current_frame_index_offset..settings.current_frame_index_offset + 4],
        &99i32.to_le_bytes()
    );
    assert_eq!(patched[settings.begin_frame_index_enabled_offset], 1);
    assert_eq!(patched[settings.end_frame_index_enabled_offset], 0);
    assert_eq!(
        &patched[settings.begin_frame_index_offset..settings.begin_frame_index_offset + 4],
        &10i32.to_le_bytes()
    );
    assert_eq!(
        &patched[settings.end_frame_index_offset..settings.end_frame_index_offset + 4],
        &240i32.to_le_bytes()
    );
    assert_eq!(
        &patched[settings.current_frame_index_in_text_field_offset
            ..settings.current_frame_index_in_text_field_offset + 4],
        &77i32.to_le_bytes()
    );
}

#[test]
fn pmm_scene_frame_range_patch_noop_preserves_source_bytes() {
    let input = pmm_with_document_global_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let patched = export_pmm_manifest_with_scene_frame_range_patch(
        &parsed,
        &PmmSceneFrameRangePatch::default(),
    )
    .expect("no-op scene frame range patch must succeed");

    assert_eq!(patched, input, "no-op patch must be byte-identical");
}

#[test]
fn pmm_scene_frame_range_patch_requires_source_bytes() {
    let input = pmm_with_document_global_summary();
    let mut parsed = parse_pmm_manifest(&input).unwrap();
    parsed.source_bytes.clear();

    let result = export_pmm_manifest_with_scene_frame_range_patch(
        &parsed,
        &PmmSceneFrameRangePatch {
            begin_frame_index: Some(1),
            ..PmmSceneFrameRangePatch::default()
        },
    );
    assert!(result.is_err(), "missing source bytes must error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("source bytes"),
        "error should explain source bytes are required: {}",
        msg
    );
}

#[test]
fn pmm_scene_media_path_patch_updates_only_expected_bytes() {
    let input = pmm_with_document_global_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let settings = parsed
        .document_global_summary
        .as_ref()
        .expect("fixture must have global summary")
        .settings
        .clone();

    let audio_path = "UserFile\\Audio\\song.wav";
    let background_video_path = "UserFile\\Movie\\bg.avi";
    let background_image_path = "UserFile\\Image\\bg.png";
    let patch = PmmSceneMediaPathPatch {
        audio_path: Some(audio_path.to_owned()),
        background_video_path: Some(background_video_path.to_owned()),
        background_image_path: Some(background_image_path.to_owned()),
    };
    let patched = export_pmm_manifest_with_scene_media_path_patch(&parsed, &patch)
        .expect("scene media path patch must succeed");

    assert_eq!(
        patched.len(),
        input.len(),
        "patch must preserve byte length"
    );

    let reparsed = parse_pmm_manifest(&patched).unwrap();
    let updated = &reparsed
        .document_global_summary
        .as_ref()
        .expect("patched PMM must keep global summary")
        .settings;
    assert_eq!(updated.audio_path, audio_path);
    assert_eq!(updated.background_video_path, background_video_path);
    assert_eq!(updated.background_image_path, background_image_path);

    let bindings = &reparsed
        .project_graph
        .as_ref()
        .expect("patched PMM must keep project graph")
        .asset_bindings;
    let scene_binding_offset = |kind: &str| {
        bindings
            .iter()
            .find(|binding| binding.scope == "sceneSettings" && binding.asset_kind == kind)
            .and_then(|binding| binding.path_offset)
    };
    assert_eq!(
        scene_binding_offset("audio"),
        Some(settings.audio_path_offset)
    );
    assert_eq!(
        scene_binding_offset("video"),
        Some(settings.background_video_path_offset)
    );
    assert_eq!(
        scene_binding_offset("image"),
        Some(settings.background_image_path_offset)
    );

    let allowed_ranges = [
        settings.audio_path_offset..settings.audio_path_offset + 256,
        settings.background_video_path_offset..settings.background_video_path_offset + 256,
        settings.background_image_path_offset..settings.background_image_path_offset + 256,
    ];
    let changed: Vec<usize> = input
        .iter()
        .zip(patched.iter())
        .enumerate()
        .filter_map(|(index, (before, after))| (before != after).then_some(index))
        .collect();

    assert!(
        changed
            .iter()
            .all(|index| allowed_ranges.iter().any(|range| range.contains(index))),
        "scene media path patch changed bytes outside decoded media path fields: {:?}",
        changed
            .iter()
            .filter(|index| !allowed_ranges.iter().any(|range| range.contains(index)))
            .collect::<Vec<_>>()
    );

    let assert_path_field = |offset: usize, path: &str| {
        let field = &patched[offset..offset + 256];
        assert_eq!(&field[..path.len()], path.as_bytes());
        assert_eq!(field[path.len()], 0);
    };
    assert_path_field(settings.audio_path_offset, audio_path);
    assert_path_field(settings.background_video_path_offset, background_video_path);
    assert_path_field(settings.background_image_path_offset, background_image_path);
}

#[test]
fn pmm_scene_media_path_patch_noop_preserves_source_bytes() {
    let input = pmm_with_document_global_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();
    let patched = export_pmm_manifest_with_scene_media_path_patch(
        &parsed,
        &PmmSceneMediaPathPatch::default(),
    )
    .expect("no-op scene media path patch must succeed");

    assert_eq!(patched, input, "no-op patch must be byte-identical");
}

#[test]
fn pmm_scene_media_path_patch_requires_source_bytes() {
    let input = pmm_with_document_global_summary();
    let mut parsed = parse_pmm_manifest(&input).unwrap();
    parsed.source_bytes.clear();

    let result = export_pmm_manifest_with_scene_media_path_patch(
        &parsed,
        &PmmSceneMediaPathPatch {
            audio_path: Some("UserFile\\Audio\\song.wav".to_owned()),
            ..PmmSceneMediaPathPatch::default()
        },
    );
    assert!(result.is_err(), "missing source bytes must error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("source bytes"),
        "error should explain source bytes are required: {}",
        msg
    );
}

#[test]
fn pmm_scene_media_path_patch_too_long_returns_err() {
    let input = pmm_with_document_global_summary();
    let parsed = parse_pmm_manifest(&input).unwrap();

    let result = export_pmm_manifest_with_scene_media_path_patch(
        &parsed,
        &PmmSceneMediaPathPatch {
            audio_path: Some("X".repeat(256)),
            ..PmmSceneMediaPathPatch::default()
        },
    );
    assert!(result.is_err(), "too-long media path must error");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("NUL") || msg.contains("leave room"),
        "error should explain fixed string length/NUL room: {}",
        msg
    );
}

#[test]
fn pmm_export_json_top_level_does_not_include_source_bytes() {
    // Explicit proof per spec: serde_json output must not expose the internal preservation field.
    let data = pmm_with_document_global_summary();
    let parsed = parse_pmm_manifest(&data).unwrap();
    let value = serde_json::to_value(&parsed).unwrap();
    let keys = json_keys(&value);
    assert!(
        !keys.contains(&"sourceBytes".to_string()) && !keys.contains(&"source_bytes".to_string()),
        "top-level JSON keys must not include source bytes field: {:?}",
        keys
    );
}

fn assert_fixed_track_expansion_state(parsed: &PmmParsedManifest, expected_fixed_tracks: u8) {
    let model = &parsed
        .document_summary
        .as_ref()
        .expect("exported PMM must contain document summary")
        .models[0];
    let initial_bones_offset = model.sections.initial_bone_keyframes_offset;
    let expansion_count_offset = initial_bones_offset
        .checked_sub(9 + usize::from(expected_fixed_tracks))
        .expect("initial bone section must follow expansion state");
    assert_eq!(
        parsed.source_bytes[expansion_count_offset], expected_fixed_tracks,
        "fixed-track expansion count must match the model header"
    );
    let expansion_flags_start = expansion_count_offset + 1;
    let expansion_flags_end = expansion_flags_start + usize::from(expected_fixed_tracks);
    assert!(
        parsed.source_bytes[expansion_flags_start..expansion_flags_end]
            .iter()
            .all(|flag| *flag == 0),
        "exported fixed-track expansion flags must start disabled"
    );
}

#[test]
fn exports_pmm_scene_header_timeline_fps_and_camera_fov() {
    let model =
        crate::pmx::parse_pmx_model(include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx"))
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
    assert_eq!(reparsed.project_settings.timeline_frame_count, Some(480));
    assert_eq!(reparsed.project_settings.frame_rate, Some(60.0));
    assert_eq!(reparsed.timeline.frame_count, Some(600));
    assert_eq!(reparsed.timeline.frame_rate, Some(60.0));
    assert_eq!(reparsed.timeline.duration_seconds, Some(10.0));
    let global = reparsed.document_global_summary.as_ref().unwrap();
    match global.camera.initial_keyframe.as_ref().unwrap() {
        PmmDocumentKeyframeSummary::Camera { fov, .. } => assert_eq!(*fov, 42),
        other => panic!("unexpected camera keyframe summary: {other:?}"),
    }
    assert_eq!(global.settings.horizontal_scroll_thumb, 0);
    assert_eq!(global.settings.begin_frame_index, 0);
    assert_eq!(global.settings.end_frame_index, 600);
    assert_fixed_track_expansion_state(&reparsed, 2);
}

#[test]
fn exports_pmm_scene_without_fixed_tracks_has_no_expansion_flags() {
    let model = crate::pmx::parse_pmx_model(include_bytes!(
        "../../fixtures/pmx/model_descriptor_parity.pmx"
    ))
    .unwrap();
    let motion =
        crate::vmd::parse_vmd_animation(include_bytes!("../../fixtures/vmd/simple_camera.vmd"))
            .unwrap();
    let structure = pmm_export_model_structure_from_pmx(&model);
    assert_eq!(structure.fixed_tracks, 0);

    let report = export_pmm_scene_from_pmx_vmd(
        &model,
        &motion,
        "UserFile/Model/model_descriptor_parity.pmx",
        &PmmSceneExportOptions::default(),
    );
    let reparsed = parse_pmm_manifest(&report.bytes).unwrap();

    assert_fixed_track_expansion_state(&reparsed, 0);
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

fn assert_model_keyframe_payload_layout(kf: &PmmDocumentModelKeyframeSummary) {
    assert_eq!(kf.visible_offset, kf.payload_offset);
    assert_eq!(kf.constraint_state_count, kf.constraint_states.len());
    assert_eq!(kf.constraint_state_count, 1);
    assert_eq!(kf.constraint_states_offset, kf.payload_offset + 1);
    assert_eq!(kf.constraint_states_byte_length, kf.constraint_state_count);
    assert_eq!(
        kf.outside_parent_index_count,
        kf.outside_parent_indices.len()
    );
    assert_eq!(kf.outside_parent_index_count, 1);
    assert_eq!(kf.outside_parent_indices_offset, kf.payload_offset + 2);
    assert_eq!(
        kf.outside_parent_indices_byte_length,
        kf.outside_parent_index_count * 8
    );
    assert_eq!(
        kf.self_shadow_enabled_offset,
        kf.payload_offset + kf.payload_byte_length - 1
    );
    assert_eq!(
        kf.payload_byte_length,
        1 + kf.constraint_states_byte_length + kf.outside_parent_indices_byte_length + 1
    );
    assert_eq!(kf.payload_bytes[0], kf.visible as u8);
    assert_eq!(kf.payload_bytes[1], kf.constraint_states[0] as u8);
    assert_eq!(
        i32::from_le_bytes(kf.payload_bytes[2..6].try_into().unwrap()),
        kf.outside_parent_indices[0].parent_model_index
    );
    assert_eq!(
        i32::from_le_bytes(kf.payload_bytes[6..10].try_into().unwrap()),
        kf.outside_parent_indices[0].parent_model_bone_index
    );
    assert_eq!(
        kf.payload_bytes[kf.payload_bytes.len() - 1],
        kf.self_shadow_enabled as u8
    );
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
        b"C:\\MMDDev\\data\\unittest\\test_1bone_cube.pmx",
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
    let data = b"Polygon Movie maker 0002\0UserFile\\Model\\miku.pmx\0UserFile\\Motion\\walk.vmd\0";
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
            "projectGraph",
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
fn pmm_project_graph_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    let keys = json_keys(&serde_json::to_value(&graph).unwrap());

    assert_eq!(
        keys,
        vec![
            "accessoryCount",
            "accessoryKeyframes",
            "assetBindings",
            "assetReferences",
            "assetSummary",
            "byteCoverage",
            "displayState",
            "exportReadiness",
            "global",
            "keyframeReferences",
            "modelCounts",
            "models",
            "parsedVersion",
            "sceneSettings",
            "source",
            "timeline",
            "trackReferences",
            "version",
        ]
    );
}

#[test]
fn pmm_project_export_readiness_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    let readiness = &graph.export_readiness;
    let keys = json_keys(&serde_json::to_value(readiness).unwrap());

    assert_eq!(
        keys,
        vec![
            "blockerCount",
            "blockers",
            "losslessParsedByteExportSupported",
            "semanticGraphExportSupported",
            "sourceBytePreservationRequired",
        ]
    );
}

#[test]
fn pmm_project_export_blocker_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    // the always-present semantic exporter unfinished blocker (first one)
    let blocker = graph
        .export_readiness
        .blockers
        .iter()
        .find(|b| b.code == "PMM_SEMANTIC_EXPORTER_UNFINISHED")
        .expect("PMM_SEMANTIC_EXPORTER_UNFINISHED blocker must be present for schema test");
    let keys = json_keys(&serde_json::to_value(blocker).unwrap());

    assert_eq!(
        keys,
        vec![
            "code",
            "count",
            "coverageRatio",
            "kind",
            "message",
            "scope",
            "severity",
        ]
    );
}

#[test]
fn pmm_project_asset_binding_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    // use first (model) binding for schema
    let binding = &graph.asset_bindings[0];
    let keys = json_keys(&serde_json::to_value(binding).unwrap());

    assert_eq!(
        keys,
        vec![
            "assetKind",
            "assetReferenceConfidence",
            "assetReferenceEndOffset",
            "assetReferenceIndex",
            "assetReferenceOffset",
            "documentIndex",
            "ownerIndex",
            "ownerName",
            "path",
            "pathOffset",
            "scope",
        ]
    );
}

#[test]
fn pmm_project_scene_settings_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    let keys = json_keys(&serde_json::to_value(&graph.scene_settings).unwrap());

    assert_eq!(
        keys,
        vec![
            "audioAssetReferenceIndex",
            "audioEnabled",
            "audioPath",
            "backgroundImageAssetReferenceIndex",
            "backgroundImageEnabled",
            "backgroundImageOffset",
            "backgroundImagePath",
            "backgroundImageScaleFactor",
            "backgroundVideoAssetReferenceIndex",
            "backgroundVideoEnabled",
            "backgroundVideoOffset",
            "backgroundVideoPath",
            "backgroundVideoScaleFactor",
            "beginFrameIndex",
            "beginFrameIndexEnabled",
            "currentFrameIndex",
            "currentFrameIndexInTextField",
            "endFrameIndex",
            "endFrameIndexEnabled",
            "loopEnabled",
            "offset",
            "offsetEnd",
            "preferredFps",
        ]
    );
}

#[test]
fn pmm_project_track_reference_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    let reference = &graph.track_references[0];
    let keys = json_keys(&serde_json::to_value(reference).unwrap());

    assert_eq!(
        keys,
        vec![
            "documentIndex",
            "initialKeyframes",
            "initialKeyframesOffset",
            "keyframeCountOffset",
            "keyframes",
            "keyframesEndOffset",
            "keyframesOffset",
            "ownerIndex",
            "ownerName",
            "scope",
            "stateEndOffset",
            "stateOffset",
            "trackKind",
        ]
    );
}

#[test]
fn pmm_project_keyframe_reference_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    let reference = &graph.keyframe_references[0];
    let keys = json_keys(&serde_json::to_value(reference).unwrap());

    assert_eq!(
        keys,
        vec![
            "byteLength",
            "documentIndex",
            "frameIndex",
            "initial",
            "keyframeIndex",
            "nextKeyframeIndex",
            "offset",
            "ownerIndex",
            "ownerName",
            "payloadByteLength",
            "payloadOffset",
            "previousKeyframeIndex",
            "scope",
            "trackKind",
        ]
    );
}

#[test]
fn pmm_project_byte_coverage_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    let coverage = &graph.byte_coverage;
    let keys = json_keys(&serde_json::to_value(coverage).unwrap());

    assert_eq!(
        keys,
        vec![
            "byteLength",
            "coverageRatio",
            "coveredByteLength",
            "gapCount",
            "gaps",
            "offset",
            "offsetEnd",
            "rangeCount",
            "ranges",
        ]
    );
}

#[test]
fn pmm_project_byte_range_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed.project_graph.unwrap();
    let range0 = &graph.byte_coverage.ranges[0];
    let keys = json_keys(&serde_json::to_value(range0).unwrap());

    assert_eq!(
        keys,
        vec![
            "byteLength",
            "documentIndex",
            "kind",
            "name",
            "offset",
            "offsetEnd",
            "ownerIndex",
            "scope",
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
    let keys =
        json_keys(&serde_json::to_value(&parsed.display_state.model_slot_flag_entries[0]).unwrap());

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
    let keys = json_keys(&serde_json::to_value(parsed.document_global_summary.unwrap()).unwrap());

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
    let keys = json_keys(&serde_json::to_value(global.camera.initial_keyframe.unwrap()).unwrap());

    assert_eq!(
        keys,
        vec![
            "angle",
            "angleByteLength",
            "angleOffset",
            "byteLength",
            "distance",
            "distanceOffset",
            "fov",
            "fovOffset",
            "frameIndex",
            "index",
            "interpolation",
            "interpolationByteLength",
            "interpolationOffset",
            "kind",
            "lookAt",
            "lookAtByteLength",
            "lookAtOffset",
            "nextKeyframeIndex",
            "offset",
            "parentModelBoneIndex",
            "parentModelBoneIndexOffset",
            "parentModelIndex",
            "parentModelIndexOffset",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
            "perspectiveView",
            "perspectiveViewOffset",
            "previousKeyframeIndex",
            "selected",
            "selectedOffset"
        ]
    );
}

#[test]
fn pmm_initial_light_keyframe_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let global = parsed.document_global_summary.unwrap();
    let keys = json_keys(&serde_json::to_value(global.light.initial_keyframe.unwrap()).unwrap());

    assert_eq!(
        keys,
        vec![
            "byteLength",
            "color",
            "colorByteLength",
            "colorOffset",
            "direction",
            "directionByteLength",
            "directionOffset",
            "frameIndex",
            "index",
            "kind",
            "nextKeyframeIndex",
            "offset",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
            "previousKeyframeIndex",
            "selected",
            "selectedOffset"
        ]
    );
}

#[test]
fn pmm_initial_gravity_keyframe_json_schema_is_stable() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let global = parsed.document_global_summary.unwrap();
    let keys = json_keys(&serde_json::to_value(global.gravity.initial_keyframe.unwrap()).unwrap());

    assert_eq!(
        keys,
        vec![
            "acceleration",
            "accelerationOffset",
            "byteLength",
            "direction",
            "directionByteLength",
            "directionOffset",
            "frameIndex",
            "index",
            "kind",
            "nextKeyframeIndex",
            "noise",
            "noiseEnabled",
            "noiseEnabledOffset",
            "noiseOffset",
            "offset",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
            "previousKeyframeIndex",
            "selected",
            "selectedOffset"
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
            "byteLength",
            "distance",
            "distanceOffset",
            "frameIndex",
            "index",
            "kind",
            "mode",
            "modeOffset",
            "nextKeyframeIndex",
            "offset",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
            "previousKeyframeIndex",
            "selected",
            "selectedOffset"
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
            "audioPathOffset",
            "backgroundImageEnabled",
            "backgroundImageOffset",
            "backgroundImagePath",
            "backgroundImagePathOffset",
            "backgroundImageScaleFactor",
            "backgroundVideoEnabled",
            "backgroundVideoOffset",
            "backgroundVideoPath",
            "backgroundVideoPathOffset",
            "backgroundVideoScaleFactor",
            "beginFrameIndex",
            "beginFrameIndexEnabled",
            "beginFrameIndexEnabledOffset",
            "beginFrameIndexOffset",
            "blackBackgroundEnabled",
            "cameraLookAtModelBoneIndex",
            "cameraLookAtModelIndex",
            "cameraLookMode",
            "currentFrameIndex",
            "currentFrameIndexInTextField",
            "currentFrameIndexInTextFieldOffset",
            "currentFrameIndexOffset",
            "edgeColor",
            "editingMode",
            "endFrameIndex",
            "endFrameIndexEnabled",
            "endFrameIndexEnabledOffset",
            "endFrameIndexOffset",
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
            "byteLength",
            "frameIndex",
            "index",
            "nextKeyframeIndex",
            "offset",
            "opacity",
            "orientation",
            "orientationByteLength",
            "orientationOffset",
            "packedOpacityVisibleOffset",
            "parentModelBoneIndex",
            "parentModelBoneIndexOffset",
            "parentModelIndex",
            "parentModelIndexOffset",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
            "previousKeyframeIndex",
            "scaleFactor",
            "scaleFactorOffset",
            "selected",
            "selectedOffset",
            "shadowEnabled",
            "shadowEnabledOffset",
            "translation",
            "translationByteLength",
            "translationOffset",
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
            interpolation,
            perspective_view,
            fov,
            selected,
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
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
            ..
        } => {
            assert_eq!(*index, None);
            assert_eq!(*frame_index, 0);
            assert_eq!(*offset, global.camera.offset);
            assert_eq!(*byte_length, 78);
            assert_eq!(*payload_offset, *offset + 12);
            assert_eq!(*payload_byte_length, *byte_length - 12);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            assert_eq!(&payload_bytes[..4], &(*distance).to_le_bytes()[..]);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));
            assert_eq!(*distance, 45.0);
            assert_eq!(*look_at, [0.0, 0.0, 0.0]);
            assert_eq!(*angle, [0.0, 0.0, 0.0]);
            assert_eq!(*parent_model_index, -1);
            assert_eq!(*parent_model_bone_index, -1);
            // 24-byte raw interpolation block exposed (after dist/look/ang/parents in payload)
            assert_eq!(interpolation.len(), 24);
            assert_eq!(*interpolation, [20u8; 24]);
            let interp_offset_in_payload = 4 + 12 + 12 + 4 + 4; // distance + lookAt + angle + parentModelIndex + parentModelBoneIndex
            assert_eq!(
                &payload_bytes[interp_offset_in_payload..interp_offset_in_payload + 24],
                &interpolation[..]
            );
            assert_eq!(
                &payload_bytes[interp_offset_in_payload..interp_offset_in_payload + 24],
                &[20u8; 24]
            );
            assert!(*perspective_view);
            assert_eq!(*fov, 30);
            assert!(!*selected);

            // layout metadata for camera keyframe payload (exporter-prep slice).
            // offsets are absolute; prove they address correct bytes inside payload_bytes for initial keyframe.
            assert_eq!(*distance_offset, *payload_offset);
            assert_eq!(*look_at_offset, *payload_offset + 4);
            assert_eq!(*look_at_byte_length, 12);
            assert_eq!(*angle_offset, *payload_offset + 16);
            assert_eq!(*angle_byte_length, 12);
            assert_eq!(*parent_model_index_offset, *payload_offset + 28);
            assert_eq!(*parent_model_bone_index_offset, *payload_offset + 32);
            assert_eq!(*interpolation_offset, *payload_offset + 36);
            assert_eq!(*interpolation_byte_length, 24);
            assert_eq!(*perspective_view_offset, *payload_offset + 60);
            assert_eq!(*fov_offset, *payload_offset + 61);
            assert_eq!(*selected_offset, *payload_offset + 65);
            let p0 = &payload_bytes;
            assert_eq!(
                &p0[*distance_offset - *payload_offset..*distance_offset - *payload_offset + 4],
                &45.0f32.to_le_bytes()
            );
            let la0 = *look_at_offset - *payload_offset;
            assert_eq!(&p0[la0..la0 + 12], &[0u8; 12]);
            let ang0 = *angle_offset - *payload_offset;
            assert_eq!(&p0[ang0..ang0 + 12], &[0u8; 12]);
            assert_eq!(
                &p0[*parent_model_index_offset - *payload_offset
                    ..*parent_model_index_offset - *payload_offset + 4],
                &(-1i32).to_le_bytes()
            );
            assert_eq!(
                &p0[*parent_model_bone_index_offset - *payload_offset
                    ..*parent_model_bone_index_offset - *payload_offset + 4],
                &(-1i32).to_le_bytes()
            );
            let ip0 = *interpolation_offset - *payload_offset;
            assert_eq!(&p0[ip0..ip0 + 24], &[20u8; 24]);
            assert_eq!(p0[*perspective_view_offset - *payload_offset], 0u8);
            assert_eq!(
                &p0[*fov_offset - *payload_offset..*fov_offset - *payload_offset + 4],
                &30i32.to_le_bytes()
            );
            assert_eq!(p0[*selected_offset - *payload_offset], 0u8);
        }
        other => panic!("unexpected camera keyframe summary: {other:?}"),
    }
    match &global.camera.keyframe_summaries[0] {
        PmmDocumentKeyframeSummary::Camera {
            index,
            frame_index,
            distance,
            parent_model_index,
            parent_model_bone_index,
            interpolation,
            selected,
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
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
            ..
        } => {
            assert_eq!(*index, Some(42));
            assert_eq!(*frame_index, 42);
            assert_eq!(*offset, global.camera.keyframes_offset);
            assert_eq!(*byte_length, 82);
            assert_eq!(*payload_offset, *offset + 16);
            assert_eq!(*payload_byte_length, *byte_length - 16);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            assert_eq!(&payload_bytes[..4], &(*distance).to_le_bytes()[..]);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));
            // additional camera keyframe also exposes the 24 raw interp bytes at same payload offset
            assert_eq!(interpolation.len(), 24);
            assert_eq!(*interpolation, [20u8; 24]);
            let interp_offset_in_payload = 4 + 12 + 12 + 4 + 4; // distance + lookAt + angle + parentModelIndex + parentModelBoneIndex
            assert_eq!(
                &payload_bytes[interp_offset_in_payload..interp_offset_in_payload + 24],
                &interpolation[..]
            );
            assert_eq!(*parent_model_index, -1);
            assert_eq!(*parent_model_bone_index, -1);

            // same layout assertions for indexed (non-initial) camera keyframe; relative positions inside payload identical.
            assert_eq!(*distance_offset, *payload_offset);
            assert_eq!(*look_at_offset, *payload_offset + 4);
            assert_eq!(*look_at_byte_length, 12);
            assert_eq!(*angle_offset, *payload_offset + 16);
            assert_eq!(*angle_byte_length, 12);
            assert_eq!(*parent_model_index_offset, *payload_offset + 28);
            assert_eq!(*parent_model_bone_index_offset, *payload_offset + 32);
            assert_eq!(*interpolation_offset, *payload_offset + 36);
            assert_eq!(*interpolation_byte_length, 24);
            assert_eq!(*perspective_view_offset, *payload_offset + 60);
            assert_eq!(*fov_offset, *payload_offset + 61);
            assert_eq!(*selected_offset, *payload_offset + 65);
            let p1 = &payload_bytes;
            assert_eq!(
                &p1[*distance_offset - *payload_offset..*distance_offset - *payload_offset + 4],
                &(*distance).to_le_bytes()[..]
            );
            let la1 = *look_at_offset - *payload_offset;
            assert_eq!(&p1[la1..la1 + 12], &[0u8; 12]);
            let ang1 = *angle_offset - *payload_offset;
            assert_eq!(&p1[ang1..ang1 + 12], &[0u8; 12]);
            assert_eq!(
                &p1[*parent_model_index_offset - *payload_offset
                    ..*parent_model_index_offset - *payload_offset + 4],
                &(-1i32).to_le_bytes()
            );
            assert_eq!(
                &p1[*parent_model_bone_index_offset - *payload_offset
                    ..*parent_model_bone_index_offset - *payload_offset + 4],
                &(-1i32).to_le_bytes()
            );
            let ip1 = *interpolation_offset - *payload_offset;
            assert_eq!(&p1[ip1..ip1 + 24], &[20u8; 24]);
            assert_eq!(p1[*perspective_view_offset - *payload_offset], 0u8);
            assert_eq!(
                &p1[*fov_offset - *payload_offset..*fov_offset - *payload_offset + 4],
                &30i32.to_le_bytes()
            );
            assert_eq!(p1[*selected_offset - *payload_offset], 0u8);
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
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
            color_offset,
            color_byte_length,
            direction_offset,
            direction_byte_length,
            selected_offset,
            ..
        } => {
            assert_eq!(*offset, global.light.offset);
            assert_eq!(*byte_length, 37);
            assert_eq!(*payload_offset, *offset + 12);
            assert_eq!(*payload_byte_length, *byte_length - 12);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            let mut color_bytes = Vec::with_capacity(12);
            for c in color {
                color_bytes.extend_from_slice(&c.to_le_bytes());
            }
            assert_eq!(&payload_bytes[..12], &color_bytes[..]);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));
            assert_eq!(*color, [0.6, 0.6, 0.6]);
            assert_eq!(*direction, [-0.5, -1.0, 0.5]);
            assert!(!*selected);

            // layout metadata for light keyframe payload (exporter-prep slice).
            assert_eq!(*color_offset, *payload_offset);
            assert_eq!(*color_byte_length, 12);
            assert_eq!(*direction_offset, *payload_offset + 12);
            assert_eq!(*direction_byte_length, 12);
            assert_eq!(*selected_offset, *payload_offset + 24);
            let p0 = &payload_bytes;
            let mut color_bytes = Vec::with_capacity(12);
            for c in color {
                color_bytes.extend_from_slice(&c.to_le_bytes());
            }
            assert_eq!(
                &p0[*color_offset - *payload_offset..*color_offset - *payload_offset + 12],
                &color_bytes[..]
            );
            let d0 = *direction_offset - *payload_offset;
            assert_eq!(
                &p0[d0..d0 + 12],
                &[-0.5f32, -1.0, 0.5]
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<_>>()[..]
            );
            assert_eq!(p0[*selected_offset - *payload_offset], 0u8);
        }
        other => panic!("unexpected light keyframe summary: {other:?}"),
    }
    match &global.light.keyframe_summaries[0] {
        PmmDocumentKeyframeSummary::Light {
            index,
            frame_index,
            color,
            selected,
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
            color_offset,
            color_byte_length,
            direction_offset,
            direction_byte_length,
            selected_offset,
            ..
        } => {
            assert_eq!(*index, Some(43));
            assert_eq!(*frame_index, 43);
            assert_eq!(*offset, global.light.keyframes_offset);
            assert_eq!(*byte_length, 41);
            assert_eq!(*payload_offset, *offset + 16);
            assert_eq!(*payload_byte_length, *byte_length - 16);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            let mut color_bytes = Vec::with_capacity(12);
            for c in color {
                color_bytes.extend_from_slice(&c.to_le_bytes());
            }
            assert_eq!(&payload_bytes[..12], &color_bytes[..]);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));

            // same layout for indexed light keyframe
            assert_eq!(*color_offset, *payload_offset);
            assert_eq!(*color_byte_length, 12);
            assert_eq!(*direction_offset, *payload_offset + 12);
            assert_eq!(*direction_byte_length, 12);
            assert_eq!(*selected_offset, *payload_offset + 24);
            let p1 = &payload_bytes;
            let mut color_bytes2 = Vec::with_capacity(12);
            for c in color {
                color_bytes2.extend_from_slice(&c.to_le_bytes());
            }
            assert_eq!(
                &p1[*color_offset - *payload_offset..*color_offset - *payload_offset + 12],
                &color_bytes2[..]
            );
            let d1 = *direction_offset - *payload_offset;
            assert_eq!(
                &p1[d1..d1 + 12],
                &[-0.5f32, -1.0, 0.5]
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<_>>()[..]
            );
            assert_eq!(p1[*selected_offset - *payload_offset], 0u8);
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
    assert_eq!(
        accessory.initial_keyframe.offset,
        accessory.offset + 1 + 100 + 256 + 1
    );
    assert_eq!(accessory.initial_keyframe.byte_length, 51);
    assert_eq!(
        accessory.initial_keyframe.payload_offset,
        accessory.initial_keyframe.offset + 12
    );
    assert_eq!(accessory.initial_keyframe.payload_byte_length, 39);
    assert_eq!(accessory.initial_keyframe.payload_bytes.len(), 39);
    assert_eq!(
        accessory.initial_keyframe.payload_bytes.len(),
        accessory.initial_keyframe.payload_byte_length
    );
    assert_eq!(accessory.initial_keyframe.payload_bytes[0], 1);
    assert_eq!(
        &accessory.initial_keyframe.payload_bytes
            [accessory.initial_keyframe.payload_bytes.len() - 2..],
        &[1, 0]
    );
    assert_eq!(accessory.initial_keyframe.translation, [1.0, 2.0, 3.0]);
    assert_eq!(accessory.initial_keyframe.orientation, [0.1, 0.2, 0.3]);
    // explicit payload layout offsets/ranges on initial accessory keyframe (from global summary path)
    assert_eq!(
        accessory.initial_keyframe.packed_opacity_visible_offset,
        accessory.initial_keyframe.payload_offset
    );
    assert_eq!(
        accessory.initial_keyframe.selected_offset,
        accessory.initial_keyframe.payload_offset + 38
    );
    assert_eq!(accessory.initial_keyframe.translation_byte_length, 12);
    assert_eq!(accessory.initial_keyframe.orientation_byte_length, 12);
    assert_eq!(accessory.keyframe_summaries[0].index, Some(46));
    assert_eq!(accessory.keyframe_summaries[0].frame_index, 46);
    assert_eq!(
        accessory.keyframe_summaries[0].offset,
        accessory.keyframes_offset
    );
    assert_eq!(accessory.keyframe_summaries[0].byte_length, 55);
    assert_eq!(
        accessory.keyframe_summaries[0].payload_offset,
        accessory.keyframe_summaries[0].offset + 16
    );
    assert_eq!(accessory.keyframe_summaries[0].payload_byte_length, 39);
    assert_eq!(accessory.keyframe_summaries[0].payload_bytes.len(), 39);
    assert_eq!(
        accessory.keyframe_summaries[0].payload_bytes.len(),
        accessory.keyframe_summaries[0].payload_byte_length
    );
    assert_eq!(accessory.keyframe_summaries[0].payload_bytes[0], 1);
    assert_eq!(
        &accessory.keyframe_summaries[0].payload_bytes
            [accessory.keyframe_summaries[0].payload_bytes.len() - 2..],
        &[1, 0]
    );
    // layout on indexed accessory keyframe (global summary test path)
    assert_eq!(
        accessory.keyframe_summaries[0].packed_opacity_visible_offset,
        accessory.keyframe_summaries[0].payload_offset
    );
    assert_eq!(
        accessory.keyframe_summaries[0].scale_factor_offset,
        accessory.keyframe_summaries[0].payload_offset + 33
    );
    assert_eq!(
        accessory.keyframe_summaries[0].shadow_enabled_offset,
        accessory.keyframe_summaries[0].payload_offset + 37
    );
    assert_eq!(global.gravity.initial_keyframes, 1);
    assert_eq!(global.gravity.keyframes, 1);
    match global.gravity.initial_keyframe.as_ref().unwrap() {
        PmmDocumentKeyframeSummary::Gravity {
            noise_enabled,
            noise,
            acceleration,
            direction,
            selected,
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
            noise_enabled_offset,
            noise_offset,
            acceleration_offset,
            direction_offset,
            direction_byte_length,
            selected_offset,
            ..
        } => {
            assert_eq!(*offset, global.gravity.state_end_offset.unwrap());
            assert_eq!(*byte_length, 34);
            assert_eq!(*payload_offset, *offset + 12);
            assert_eq!(*payload_byte_length, *byte_length - 12);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            assert_eq!(payload_bytes[0], *noise_enabled as u8);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));
            assert!(!*noise_enabled);
            assert_eq!(*noise, 10);
            assert_eq!(*acceleration, 9.8);
            assert_eq!(*direction, [0.0, -1.0, 0.0]);
            assert!(!*selected);

            // layout metadata for gravity keyframe payload (exporter-prep slice).
            assert_eq!(*noise_enabled_offset, *payload_offset);
            assert_eq!(*noise_offset, *payload_offset + 1);
            assert_eq!(*acceleration_offset, *payload_offset + 5);
            assert_eq!(*direction_offset, *payload_offset + 9);
            assert_eq!(*direction_byte_length, 12);
            assert_eq!(*selected_offset, *payload_offset + 21);
            let p0 = &payload_bytes;
            assert_eq!(
                p0[*noise_enabled_offset - *payload_offset],
                *noise_enabled as u8
            );
            assert_eq!(
                &p0[*noise_offset - *payload_offset..*noise_offset - *payload_offset + 4],
                &10i32.to_le_bytes()
            );
            assert_eq!(
                &p0[*acceleration_offset - *payload_offset
                    ..*acceleration_offset - *payload_offset + 4],
                &9.8f32.to_le_bytes()
            );
            let dir0 = *direction_offset - *payload_offset;
            assert_eq!(
                &p0[dir0..dir0 + 12],
                &[0.0f32, -1.0, 0.0]
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<_>>()[..]
            );
            assert_eq!(p0[*selected_offset - *payload_offset], 0u8);
        }
        other => panic!("unexpected gravity keyframe summary: {other:?}"),
    }
    match &global.gravity.keyframe_summaries[0] {
        PmmDocumentKeyframeSummary::Gravity {
            index,
            frame_index,
            noise_enabled,
            selected,
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
            noise_enabled_offset,
            noise_offset,
            acceleration_offset,
            direction_offset,
            direction_byte_length,
            selected_offset,
            ..
        } => {
            assert_eq!(*index, Some(44));
            assert_eq!(*frame_index, 44);
            assert_eq!(*offset, global.gravity.keyframes_offset);
            assert_eq!(*byte_length, 38);
            assert_eq!(*payload_offset, *offset + 16);
            assert_eq!(*payload_byte_length, *byte_length - 16);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            assert_eq!(payload_bytes[0], *noise_enabled as u8);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));

            // same layout for indexed gravity keyframe
            assert_eq!(*noise_enabled_offset, *payload_offset);
            assert_eq!(*noise_offset, *payload_offset + 1);
            assert_eq!(*acceleration_offset, *payload_offset + 5);
            assert_eq!(*direction_offset, *payload_offset + 9);
            assert_eq!(*direction_byte_length, 12);
            assert_eq!(*selected_offset, *payload_offset + 21);
            let p1 = &payload_bytes;
            assert_eq!(
                p1[*noise_enabled_offset - *payload_offset],
                *noise_enabled as u8
            );
            assert_eq!(
                &p1[*noise_offset - *payload_offset..*noise_offset - *payload_offset + 4],
                &10i32.to_le_bytes()
            );
            assert_eq!(
                &p1[*acceleration_offset - *payload_offset
                    ..*acceleration_offset - *payload_offset + 4],
                &9.8f32.to_le_bytes()
            );
            let dir1 = *direction_offset - *payload_offset;
            assert_eq!(
                &p1[dir1..dir1 + 12],
                &[0.0f32, -1.0, 0.0]
                    .iter()
                    .flat_map(|f| f.to_le_bytes())
                    .collect::<Vec<_>>()[..]
            );
            assert_eq!(p1[*selected_offset - *payload_offset], 0u8);
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
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
            mode_offset,
            distance_offset,
            selected_offset,
            ..
        } => {
            assert_eq!(*offset, global.self_shadow.state_end_offset.unwrap());
            assert_eq!(*byte_length, 18);
            assert_eq!(*payload_offset, *offset + 12);
            assert_eq!(*payload_byte_length, *byte_length - 12);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            assert_eq!(payload_bytes[0], *mode);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));
            assert_eq!(*mode, 1);
            assert_eq!(*distance, 8875.0);
            assert!(!*selected);

            // layout metadata for self-shadow keyframe payload (exporter-prep slice).
            assert_eq!(*mode_offset, *payload_offset);
            assert_eq!(*distance_offset, *payload_offset + 1);
            assert_eq!(*selected_offset, *payload_offset + 5);
            let p0 = &payload_bytes;
            assert_eq!(p0[*mode_offset - *payload_offset], *mode);
            assert_eq!(
                &p0[*distance_offset - *payload_offset..*distance_offset - *payload_offset + 4],
                &8875.0f32.to_le_bytes()
            );
            assert_eq!(p0[*selected_offset - *payload_offset], 0u8);
        }
        other => panic!("unexpected self-shadow keyframe summary: {other:?}"),
    }
    match &global.self_shadow.keyframe_summaries[0] {
        PmmDocumentKeyframeSummary::SelfShadow {
            index,
            frame_index,
            mode,
            selected,
            offset,
            byte_length,
            payload_offset,
            payload_byte_length,
            payload_bytes,
            mode_offset,
            distance_offset,
            selected_offset,
            ..
        } => {
            assert_eq!(*index, Some(45));
            assert_eq!(*frame_index, 45);
            assert_eq!(*offset, global.self_shadow.keyframes_offset);
            assert_eq!(*byte_length, 22);
            assert_eq!(*payload_offset, *offset + 16);
            assert_eq!(*payload_byte_length, *byte_length - 16);
            assert_eq!(payload_bytes.len(), *payload_byte_length);
            assert_eq!(payload_bytes[0], *mode);
            assert_eq!(payload_bytes.last().copied(), Some(*selected as u8));

            // same layout for indexed self-shadow keyframe
            assert_eq!(*mode_offset, *payload_offset);
            assert_eq!(*distance_offset, *payload_offset + 1);
            assert_eq!(*selected_offset, *payload_offset + 5);
            let p1 = &payload_bytes;
            assert_eq!(p1[*mode_offset - *payload_offset], *mode);
            assert_eq!(
                &p1[*distance_offset - *payload_offset..*distance_offset - *payload_offset + 4],
                &8875.0f32.to_le_bytes()
            );
            assert_eq!(p1[*selected_offset - *payload_offset], 0u8);
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
fn parses_pmm_project_graph_only_when_full_document_and_global_summary_present() {
    // PMM data without full document/global summary has project_graph == None
    let parsed_minimal = parse_pmm_manifest(
        b"Polygon Movie maker 0002\0UserFile\\Model\\miku.pmx\0UserFile\\Motion\\walk.vmd\0",
    )
    .unwrap();
    assert!(parsed_minimal.project_graph.is_none());

    let parsed_settings = parse_pmm_manifest(&pmm_with_project_settings()).unwrap();
    assert!(parsed_settings.project_graph.is_none());

    let parsed_doc_only = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    assert!(parsed_doc_only.project_graph.is_none());
    assert!(parsed_doc_only.document_summary.is_some());
    assert!(parsed_doc_only.document_global_summary.is_none());

    // pmm_with_document_global_summary() has project_graph == Some(...)
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed
        .project_graph
        .as_ref()
        .expect("project_graph must be Some for pmm_with_document_global_summary");
    assert_eq!(
        graph.source,
        "mmd-anim-format first PMMv2 document graph DTO slice"
    );
    assert_eq!(graph.version, "0002");
    assert_eq!(graph.parsed_version, Some(2));

    // the graph model/global counts match document_summary and document_global_summary
    let doc = parsed.document_summary.as_ref().unwrap();
    let glob = parsed.document_global_summary.as_ref().unwrap();
    assert_eq!(graph.model_counts.models, doc.counts.models);
    assert_eq!(graph.model_counts.bones, doc.counts.bones);
    assert_eq!(graph.model_counts.morphs, doc.counts.morphs);
    assert_eq!(graph.model_counts.bone_keyframes, doc.counts.bone_keyframes);
    assert_eq!(
        graph.model_counts.morph_keyframes,
        doc.counts.morph_keyframes
    );
    assert_eq!(graph.accessory_count, glob.accessories.accessory_count);
    assert_eq!(graph.accessory_keyframes, glob.accessories.keyframes);
    assert_eq!(graph.models.len(), doc.models.len());
    assert_eq!(graph.global.camera.keyframes, glob.camera.keyframes);
}

#[test]
fn pmm_project_scene_settings_synthetic_pmmv2_full_fixture() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed
        .project_graph
        .as_ref()
        .expect("project_graph must be Some for synthetic PMMv2 full fixture");
    let ss = &graph.scene_settings;
    let glob = parsed.document_global_summary.as_ref().unwrap();
    let settings = &glob.settings;

    // offset/offsetEnd match global.settings
    assert_eq!(ss.offset, settings.offset);
    assert_eq!(ss.offset_end, settings.offset_end);

    // current/begin/end/preferredFps/loop fields match global.settings
    assert_eq!(ss.current_frame_index, settings.current_frame_index);
    assert_eq!(
        ss.current_frame_index_in_text_field,
        settings.current_frame_index_in_text_field
    );
    assert_eq!(
        ss.begin_frame_index_enabled,
        settings.begin_frame_index_enabled
    );
    assert_eq!(ss.end_frame_index_enabled, settings.end_frame_index_enabled);
    assert_eq!(ss.begin_frame_index, settings.begin_frame_index);
    assert_eq!(ss.end_frame_index, settings.end_frame_index);
    assert_eq!(ss.preferred_fps, settings.preferred_fps);
    assert_eq!(ss.loop_enabled, settings.loop_enabled);

    // audio/background paths and enabled flags match global.settings
    assert_eq!(ss.audio_enabled, settings.audio_enabled);
    assert_eq!(ss.audio_path, settings.audio_path);
    assert_eq!(
        ss.background_video_enabled,
        settings.background_video_enabled
    );
    assert_eq!(ss.background_video_path, settings.background_video_path);
    assert_eq!(
        ss.background_image_enabled,
        settings.background_image_enabled
    );
    assert_eq!(ss.background_image_path, settings.background_image_path);

    // empty synthetic paths (using push_empty_pmm_path in fixture) resolve to None
    assert!(
        ss.audio_path.is_empty(),
        "synthetic fixture uses empty audio path"
    );
    assert!(
        ss.background_video_path.is_empty(),
        "synthetic fixture uses empty background video path"
    );
    assert!(
        ss.background_image_path.is_empty(),
        "synthetic fixture uses empty background image path"
    );
    assert!(
        ss.audio_asset_reference_index.is_none(),
        "empty audio path must resolve to None asset reference index"
    );
    assert!(
        ss.background_video_asset_reference_index.is_none(),
        "empty background video path must resolve to None asset reference index"
    );
    assert!(
        ss.background_image_asset_reference_index.is_none(),
        "empty background image path must resolve to None asset reference index"
    );
}

#[test]
fn pmm_project_asset_bindings_synthetic_pmmv2_full_fixture() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed
        .project_graph
        .as_ref()
        .expect("project_graph must be Some for synthetic PMMv2 full fixture");
    let doc = parsed.document_summary.as_ref().unwrap();
    let glob = parsed.document_global_summary.as_ref().unwrap();

    // exactly one model binding + one accessory binding (sceneSettings omitted for empty paths)
    assert_eq!(
        graph.asset_bindings.len(),
        2,
        "synthetic PMMv2 full fixture must have exactly 2 asset bindings (1 model + 1 accessory)"
    );
    let model_bindings: Vec<_> = graph
        .asset_bindings
        .iter()
        .filter(|b| b.scope == "model")
        .collect();
    let acc_bindings: Vec<_> = graph
        .asset_bindings
        .iter()
        .filter(|b| b.scope == "accessory")
        .collect();
    let scene_bindings: Vec<_> = graph
        .asset_bindings
        .iter()
        .filter(|b| b.scope == "sceneSettings")
        .collect();
    assert_eq!(model_bindings.len(), 1);
    assert_eq!(acc_bindings.len(), 1);
    assert_eq!(
        scene_bindings.len(),
        0,
        "no sceneSettings binding exists because the synthetic fixture paths are empty"
    );

    // model binding matches decoded owner path, path_offset, owner indexes, asset_reference_index
    let model0 = &doc.models[0];
    let mb = model_bindings[0];
    assert_eq!(mb.scope, "model");
    assert_eq!(mb.asset_kind, "model");
    assert_eq!(mb.owner_index, Some(model0.slot_index));
    assert_eq!(mb.document_index, Some(model0.document_model_index));
    assert_eq!(mb.owner_name.as_deref(), Some(model0.name.as_str()));
    assert_eq!(mb.path, model0.path);
    assert_eq!(mb.path_offset, Some(model0.path_offset));
    assert_eq!(mb.asset_reference_index, model0.asset_reference_index);
    if let Some(idx) = mb.asset_reference_index {
        let ar = &graph.asset_references[idx];
        assert_eq!(mb.asset_reference_offset, Some(ar.offset));
        assert_eq!(mb.asset_reference_end_offset, Some(ar.offset_end));
        assert_eq!(mb.asset_reference_confidence, Some(ar.confidence));
    }

    // accessory binding matches decoded owner path, path_offset, owner indexes, asset_reference_index
    let acc0 = &glob.accessories.accessories[0];
    let ab = acc_bindings[0];
    assert_eq!(ab.scope, "accessory");
    assert_eq!(ab.asset_kind, "accessory");
    assert_eq!(ab.owner_index, Some(acc0.slot_index));
    assert_eq!(ab.document_index, Some(acc0.document_accessory_index));
    assert_eq!(ab.owner_name.as_deref(), Some(acc0.name.as_str()));
    assert_eq!(ab.path, acc0.path);
    assert_eq!(ab.path_offset, Some(acc0.path_offset));
    assert_eq!(ab.asset_reference_index, acc0.asset_reference_index);
    if let Some(idx) = ab.asset_reference_index {
        let ar = &graph.asset_references[idx];
        assert_eq!(ab.asset_reference_offset, Some(ar.offset));
        assert_eq!(ab.asset_reference_end_offset, Some(ar.offset_end));
        assert_eq!(ab.asset_reference_confidence, Some(ar.confidence));
    }
}

#[test]
fn pmm_project_export_readiness_synthetic_pmmv2_full_fixture() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed
        .project_graph
        .as_ref()
        .expect("project_graph must be Some for synthetic PMMv2 full fixture");
    let cov = &graph.byte_coverage;
    let r = &graph.export_readiness;

    assert!(
        r.lossless_parsed_byte_export_supported,
        "lossless parsed-byte export supported"
    );
    assert!(
        !r.semantic_graph_export_supported,
        "semantic graph export must be false (full exporter unfinished)"
    );
    assert!(
        r.source_byte_preservation_required,
        "source byte preservation required"
    );
    assert_eq!(
        r.blocker_count,
        r.blockers.len(),
        "blocker_count must equal blockers.len()"
    );

    // unfinished semantic exporter blocker is always present with exact shape
    let semantic_blocker = r
        .blockers
        .iter()
        .find(|b| b.code == "PMM_SEMANTIC_EXPORTER_UNFINISHED")
        .expect("PMM_SEMANTIC_EXPORTER_UNFINISHED blocker must be present");
    assert_eq!(semantic_blocker.severity, "blocker");
    assert!(semantic_blocker.scope.is_none());
    assert!(semantic_blocker.kind.is_none());
    assert!(
        semantic_blocker
            .message
            .contains("semantic graph export/editing is not yet supported"),
        "message must state full semantic graph export/editing is not yet supported"
    );

    // gap blocker presence matches byte_coverage.gap_count > 0 (synthetic full fixture has 0)
    let has_gap_blocker = r
        .blockers
        .iter()
        .any(|b| b.code == "PMM_DECODED_BYTE_GAPS_REMAIN");
    assert_eq!(
        has_gap_blocker,
        cov.gap_count > 0,
        "PMM_DECODED_BYTE_GAPS_REMAIN presence must match gap_count > 0"
    );
}

#[test]
fn pmm_project_track_reference_synthetic_pmmv2_full_fixture() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed
        .project_graph
        .as_ref()
        .expect("project_graph must be Some for synthetic PMMv2 full fixture");

    // exactly 8 track references: 3 model (bone/morph/model) + 4 global + 1 accessory
    assert_eq!(
        graph.track_references.len(),
        8,
        "synthetic PMMv2 full fixture must have exactly 8 track references"
    );
    let model_tracks: Vec<_> = graph
        .track_references
        .iter()
        .filter(|r| r.scope == "model")
        .collect();
    let global_tracks: Vec<_> = graph
        .track_references
        .iter()
        .filter(|r| r.scope == "global")
        .collect();
    let accessory_tracks: Vec<_> = graph
        .track_references
        .iter()
        .filter(|r| r.scope == "accessory")
        .collect();
    assert_eq!(
        model_tracks.len(),
        3,
        "per document model: bone + morph + model track groups"
    );
    assert_eq!(
        global_tracks.len(),
        4,
        "global: camera + light + gravity + selfShadow"
    );
    assert_eq!(
        accessory_tracks.len(),
        1,
        "each decoded accessory contributes one accessory track group"
    );

    let doc = parsed.document_summary.as_ref().unwrap();
    let glob = parsed.document_global_summary.as_ref().unwrap();

    // model bone track counts/offsets must match PmmDocumentModelSummary + sections
    let bone_ref = graph
        .track_references
        .iter()
        .find(|r| r.scope == "model" && r.track_kind == "bone")
        .expect("model bone track ref");
    let model0 = &doc.models[0];
    assert_eq!(bone_ref.owner_index, Some(model0.slot_index));
    assert_eq!(bone_ref.document_index, Some(model0.document_model_index));
    assert_eq!(bone_ref.owner_name.as_deref(), Some(model0.name.as_str()));
    assert_eq!(bone_ref.initial_keyframes, model0.initial_bone_keyframes);
    assert_eq!(bone_ref.keyframes, model0.bone_keyframes);
    assert_eq!(
        bone_ref.initial_keyframes_offset,
        Some(model0.sections.initial_bone_keyframes_offset)
    );
    assert_eq!(
        bone_ref.keyframe_count_offset,
        Some(model0.sections.bone_keyframe_count_offset)
    );
    assert_eq!(
        bone_ref.keyframes_offset,
        model0.sections.bone_keyframes_offset
    );
    assert_eq!(
        bone_ref.keyframes_end_offset,
        model0.sections.bone_keyframes_end_offset
    );

    // global camera track counts/offsets must match PmmDocumentTrackSummary
    let cam_ref = graph
        .track_references
        .iter()
        .find(|r| r.scope == "global" && r.track_kind == "camera")
        .expect("global camera track ref");
    assert_eq!(cam_ref.initial_keyframes, glob.camera.initial_keyframes);
    assert_eq!(cam_ref.keyframes, glob.camera.keyframes);
    assert_eq!(cam_ref.keyframes_offset, glob.camera.keyframes_offset);
    assert_eq!(
        cam_ref.keyframes_end_offset,
        glob.camera.keyframes_end_offset
    );
    // for camera, initial kf starts at the track offset
    assert_eq!(cam_ref.initial_keyframes_offset, Some(glob.camera.offset));
    assert_eq!(
        cam_ref.keyframe_count_offset,
        Some(glob.camera.keyframe_count_offset)
    );

    // accessory track counts/offsets must match PmmDocumentAccessorySummary
    let acc_ref = graph
        .track_references
        .iter()
        .find(|r| r.scope == "accessory")
        .expect("accessory track ref");
    let acc0 = &glob.accessories.accessories[0];
    assert_eq!(acc_ref.owner_index, Some(acc0.slot_index));
    assert_eq!(acc_ref.document_index, Some(acc0.document_accessory_index));
    assert_eq!(acc_ref.owner_name.as_deref(), Some(acc0.name.as_str()));
    assert_eq!(acc_ref.initial_keyframes, 1);
    assert_eq!(acc_ref.keyframes, acc0.keyframes);
    assert_eq!(
        acc_ref.initial_keyframes_offset,
        Some(acc0.initial_keyframe.offset)
    );
    assert_eq!(
        acc_ref.keyframe_count_offset,
        Some(acc0.keyframe_count_offset)
    );
    assert_eq!(acc_ref.keyframes_offset, acc0.keyframes_offset);
    assert_eq!(acc_ref.keyframes_end_offset, acc0.keyframes_end_offset);
}

#[test]
fn pmm_project_keyframe_reference_synthetic_pmmv2_full_fixture() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed
        .project_graph
        .as_ref()
        .expect("project_graph must be Some for synthetic PMMv2 full fixture");

    // Inspect synthetic fixture: 1 model (6 kfs) + 4 globals (8 kfs) + 1 accessory (2 kfs) = 16
    // (model: 1 init bone +1 add bone +1 init morph +1 add morph +1 init model +1 add model =6;
    //  global: camera/light/gravity/selfShadow each 1+1=8; accessory:1 init +1 add=2)
    // Do not hardcode 15; use actual derived count from current summaries (16 here).
    assert_eq!(
        graph.keyframe_references.len(),
        16,
        "synthetic PMMv2 full fixture must have exactly 16 keyframe references (derived from decoded summaries: 6 model + 8 global + 2 accessory)"
    );

    let model_kfs: Vec<_> = graph
        .keyframe_references
        .iter()
        .filter(|r| r.scope == "model")
        .collect();
    let global_kfs: Vec<_> = graph
        .keyframe_references
        .iter()
        .filter(|r| r.scope == "global")
        .collect();
    let accessory_kfs: Vec<_> = graph
        .keyframe_references
        .iter()
        .filter(|r| r.scope == "accessory")
        .collect();
    assert_eq!(
        model_kfs.len(),
        6,
        "per document model: bone/morph/model each initial+additional"
    );
    assert_eq!(
        global_kfs.len(),
        8,
        "global: camera/light/gravity/selfShadow each initial+additional"
    );
    assert_eq!(
        accessory_kfs.len(),
        2,
        "each decoded accessory contributes initial + additional accessory keyframes"
    );

    let doc = parsed.document_summary.as_ref().unwrap();
    let glob = parsed.document_global_summary.as_ref().unwrap();

    // initial model bone keyframe reference must match source summary (no payloadBytes)
    let init_model_bone_kf = graph
        .keyframe_references
        .iter()
        .find(|r| r.scope == "model" && r.track_kind == "bone" && r.initial)
        .expect("initial model bone keyframe ref");
    let model0 = &doc.models[0];
    let src_init_bone = &model0.initial_bone_keyframe_summaries[0];
    assert_eq!(init_model_bone_kf.owner_index, Some(model0.slot_index));
    assert_eq!(
        init_model_bone_kf.document_index,
        Some(model0.document_model_index)
    );
    assert_eq!(
        init_model_bone_kf.owner_name.as_deref(),
        Some(model0.name.as_str())
    );
    assert!(init_model_bone_kf.initial);
    assert_eq!(init_model_bone_kf.keyframe_index, src_init_bone.index);
    assert_eq!(init_model_bone_kf.frame_index, src_init_bone.frame_index);
    assert_eq!(
        init_model_bone_kf.previous_keyframe_index,
        src_init_bone.previous_keyframe_index
    );
    assert_eq!(
        init_model_bone_kf.next_keyframe_index,
        src_init_bone.next_keyframe_index
    );
    assert_eq!(init_model_bone_kf.offset, src_init_bone.offset);
    assert_eq!(init_model_bone_kf.byte_length, src_init_bone.byte_length);
    assert_eq!(
        init_model_bone_kf.payload_offset,
        src_init_bone.payload_offset
    );
    assert_eq!(
        init_model_bone_kf.payload_byte_length,
        src_init_bone.payload_byte_length
    );

    // additional global camera keyframe reference must match source summary
    let add_global_cam_kf = graph
        .keyframe_references
        .iter()
        .find(|r| r.scope == "global" && r.track_kind == "camera" && !r.initial)
        .expect("additional global camera keyframe ref");
    let src_add_cam = match &glob.camera.keyframe_summaries[0] {
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
        } => (
            *index,
            *frame_index,
            *previous_keyframe_index,
            *next_keyframe_index,
            *offset,
            *byte_length,
            *payload_offset,
            *payload_byte_length,
        ),
        other => panic!("unexpected add cam summary: {other:?}"),
    };
    assert!(!add_global_cam_kf.initial);
    assert_eq!(add_global_cam_kf.keyframe_index, src_add_cam.0);
    assert_eq!(add_global_cam_kf.frame_index, src_add_cam.1);
    assert_eq!(add_global_cam_kf.previous_keyframe_index, src_add_cam.2);
    assert_eq!(add_global_cam_kf.next_keyframe_index, src_add_cam.3);
    assert_eq!(add_global_cam_kf.offset, src_add_cam.4);
    assert_eq!(add_global_cam_kf.byte_length, src_add_cam.5);
    assert_eq!(add_global_cam_kf.payload_offset, src_add_cam.6);
    assert_eq!(add_global_cam_kf.payload_byte_length, src_add_cam.7);

    // initial accessory keyframe reference must match source summary
    let init_acc_kf = graph
        .keyframe_references
        .iter()
        .find(|r| r.scope == "accessory" && r.initial)
        .expect("initial accessory keyframe ref");
    let acc0 = &glob.accessories.accessories[0];
    let src_init_acc = &acc0.initial_keyframe;
    assert_eq!(init_acc_kf.owner_index, Some(acc0.slot_index));
    assert_eq!(
        init_acc_kf.document_index,
        Some(acc0.document_accessory_index)
    );
    assert_eq!(init_acc_kf.owner_name.as_deref(), Some(acc0.name.as_str()));
    assert!(init_acc_kf.initial);
    assert_eq!(init_acc_kf.keyframe_index, src_init_acc.index);
    assert_eq!(init_acc_kf.frame_index, src_init_acc.frame_index);
    assert_eq!(
        init_acc_kf.previous_keyframe_index,
        src_init_acc.previous_keyframe_index
    );
    assert_eq!(
        init_acc_kf.next_keyframe_index,
        src_init_acc.next_keyframe_index
    );
    assert_eq!(init_acc_kf.offset, src_init_acc.offset);
    assert_eq!(init_acc_kf.byte_length, src_init_acc.byte_length);
    assert_eq!(init_acc_kf.payload_offset, src_init_acc.payload_offset);
    assert_eq!(
        init_acc_kf.payload_byte_length,
        src_init_acc.payload_byte_length
    );
}

#[test]
fn pmm_project_byte_coverage_synthetic_pmmv2_full_fixture() {
    let parsed = parse_pmm_manifest(&pmm_with_document_global_summary()).unwrap();
    let graph = parsed
        .project_graph
        .as_ref()
        .expect("project_graph must be Some for synthetic PMMv2 full fixture");

    let cov = &graph.byte_coverage;
    let doc = parsed.document_summary.as_ref().unwrap();
    let glob = parsed.document_global_summary.as_ref().unwrap();

    // 1 model + 1 global root + 4 global tracks + 1 accessory block + 1 accessory = 8
    assert_eq!(
        cov.range_count, 8,
        "synthetic PMMv2 full fixture must have exactly 8 byte coverage ranges (1 model + 1 global root + 4 global tracks + 1 accessory block + 1 accessory)"
    );
    assert_eq!(cov.ranges.len(), 8);

    // offset equals the first decoded model offset
    let first_model = &doc.models[0];
    assert_eq!(
        cov.offset, first_model.offset,
        "coverage offset must equal first model offset"
    );

    // offsetEnd equals global.offset_end
    assert_eq!(
        cov.offset_end, glob.offset_end,
        "coverage offsetEnd must equal global offset_end"
    );

    // byteLength == offsetEnd - offset
    assert_eq!(cov.byte_length, cov.offset_end - cov.offset);

    // a camera track range matches global.camera
    let cam_range = cov
        .ranges
        .iter()
        .find(|r| r.scope == "global" && r.kind == "camera")
        .expect("camera track byte range in coverage");
    assert_eq!(cam_range.offset, glob.camera.offset);
    assert_eq!(cam_range.offset_end, glob.camera.offset_end);
    assert_eq!(
        cam_range.byte_length,
        glob.camera.offset_end - glob.camera.offset
    );
    assert_eq!(cam_range.scope, "global");
    assert_eq!(cam_range.kind, "camera");
    assert!(cam_range.owner_index.is_none());
    assert!(cam_range.document_index.is_none());

    // an accessory range matches the decoded accessory summary
    let acc_range = cov
        .ranges
        .iter()
        .find(|r| r.scope == "accessory")
        .expect("accessory byte range in coverage");
    let acc0 = &glob.accessories.accessories[0];
    assert_eq!(acc_range.owner_index, Some(acc0.slot_index));
    assert_eq!(
        acc_range.document_index,
        Some(acc0.document_accessory_index)
    );
    assert_eq!(acc_range.name.as_deref(), Some(acc0.name.as_str()));
    assert_eq!(acc_range.offset, acc0.offset);
    assert_eq!(acc_range.offset_end, acc0.offset_end);
    assert_eq!(acc_range.byte_length, acc0.offset_end - acc0.offset);
    assert_eq!(acc_range.scope, "accessory");
    assert_eq!(acc_range.kind, "accessory");

    // New merged coverage metrics + unknown gap ranges (exporter-prep diagnostic, read-only, from bc_ranges only)
    assert!(
        cov.covered_byte_length <= cov.byte_length,
        "covered_byte_length must not exceed byte_length"
    );
    assert!(
        (0.0..=1.0).contains(&cov.coverage_ratio),
        "coverage_ratio must be in [0.0, 1.0], got {}",
        cov.coverage_ratio
    );
    assert_eq!(
        cov.gap_count,
        cov.gaps.len(),
        "gap_count must equal gaps.len()"
    );
    let cov_span_start = cov.offset;
    let cov_span_end = cov.offset_end;
    for (i, g) in cov.gaps.iter().enumerate() {
        assert_eq!(g.scope, "unknown", "gap[{}] scope must be unknown", i);
        assert_eq!(g.kind, "gap", "gap[{}] kind must be gap", i);
        assert!(
            g.byte_length > 0,
            "gap[{}] must have nonzero byte_length",
            i
        );
        assert!(
            g.offset >= cov_span_start && g.offset_end <= cov_span_end,
            "gap[{}] must be inside coverage span [{}, {})",
            i,
            cov_span_start,
            cov_span_end
        );
        assert_eq!(g.offset_end - g.offset, g.byte_length);
        assert!(g.owner_index.is_none());
        assert!(g.document_index.is_none());
        assert!(g.name.is_none());
    }

    // Decoded ranges (ranges) still exist and populated as before (count + spot checks)
    assert_eq!(cov.range_count, 8);
    assert_eq!(cov.ranges.len(), 8);
    let model_range = cov
        .ranges
        .iter()
        .find(|r| r.scope == "model")
        .expect("model byte range still present in ranges after adding coverage metrics");
    assert_eq!(model_range.scope, "model");
    assert_eq!(model_range.kind, "model");
    assert!(model_range.owner_index.is_some());
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
            "boneNames",
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
            "morphNames",
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
            "byteLength",
            "constraintStateCount",
            "constraintStates",
            "constraintStatesByteLength",
            "constraintStatesOffset",
            "frameIndex",
            "index",
            "nextKeyframeIndex",
            "offset",
            "outsideParentIndexCount",
            "outsideParentIndices",
            "outsideParentIndicesByteLength",
            "outsideParentIndicesOffset",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
            "previousKeyframeIndex",
            "selfShadowEnabled",
            "selfShadowEnabledOffset",
            "visible",
            "visibleOffset"
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
    assert_eq!(
        model.path_offset,
        model.offset + 1 + 1 + "Miku".len() + 1 + "MikuEn".len()
    );
    let asset_reference_index = model.asset_reference_index.unwrap();
    assert_eq!(
        parsed.asset_references[asset_reference_index].normalized_path,
        "UserFile/Model/miku.pmx"
    );
    assert_eq!(model.bone_count, 1);
    assert_eq!(model.bone_names, vec!["center".to_owned()]);
    assert_eq!(model.morph_count, 1);
    assert_eq!(model.morph_names, vec!["smile".to_owned()]);
    assert_eq!(model.constraint_bone_count, 1);
    assert_eq!(model.outside_parent_subject_bone_count, 1);
    assert_eq!(model.bone_keyframes, 1);
    assert_eq!(model.morph_keyframes, 1);
    assert_eq!(model.model_keyframes, 1);
    assert_eq!(model.initial_model_keyframe.index, None);
    assert_eq!(model.initial_model_keyframe.frame_index, 0);
    assert_eq!(model.initial_model_keyframe.previous_keyframe_index, 0);
    assert_eq!(model.initial_model_keyframe.next_keyframe_index, 0);
    assert_eq!(
        model.initial_model_keyframe.offset,
        model.sections.initial_model_keyframe_offset
    );
    assert_eq!(model.initial_model_keyframe.byte_length, 23);
    assert_eq!(
        model.initial_model_keyframe.payload_offset,
        model.initial_model_keyframe.offset + 12
    );
    assert_eq!(model.initial_model_keyframe.payload_byte_length, 11);
    assert_eq!(model.initial_model_keyframe.payload_bytes.len(), 11);
    assert_eq!(
        model.initial_model_keyframe.payload_bytes.len(),
        model.initial_model_keyframe.payload_byte_length
    );
    assert_model_keyframe_payload_layout(&model.initial_model_keyframe);
    assert_eq!(
        model.initial_model_keyframe.payload_bytes[0],
        model.initial_model_keyframe.visible as u8
    );
    assert_eq!(
        model.initial_model_keyframe.payload_bytes[10],
        model.initial_model_keyframe.self_shadow_enabled as u8
    );
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
    assert_eq!(
        model.model_keyframe_summaries[0].offset,
        model.sections.model_keyframes_offset
    );
    assert_eq!(model.model_keyframe_summaries[0].byte_length, 27);
    assert_eq!(
        model.model_keyframe_summaries[0].payload_offset,
        model.model_keyframe_summaries[0].offset + 16
    );
    assert_eq!(model.model_keyframe_summaries[0].payload_byte_length, 11);
    assert_eq!(model.model_keyframe_summaries[0].payload_bytes.len(), 11);
    assert_eq!(
        model.model_keyframe_summaries[0].payload_bytes.len(),
        model.model_keyframe_summaries[0].payload_byte_length
    );
    assert_model_keyframe_payload_layout(&model.model_keyframe_summaries[0]);
    assert_eq!(
        model.model_keyframe_summaries[0].payload_bytes[0],
        model.model_keyframe_summaries[0].visible as u8
    );
    assert_eq!(
        model.model_keyframe_summaries[0].payload_bytes[10],
        model.model_keyframe_summaries[0].self_shadow_enabled as u8
    );
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
    assert!(model.sections.outside_parent_states_offset >= model.sections.constraint_states_offset);
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
    let keys = json_keys(&serde_json::to_value(&model.initial_bone_keyframe_summaries[0]).unwrap());

    assert_eq!(
        keys,
        vec![
            "byteLength",
            "frameIndex",
            "index",
            "interpolation",
            "nextKeyframeIndex",
            "offset",
            "orientation",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
            "physicsDisabled",
            "previousKeyframeIndex",
            "selected",
            "translation"
        ]
    );
}

#[test]
fn builds_runtime_clip_from_pmm_document_keyframe_chains() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.initial_bone_keyframe_summaries[0].next_keyframe_index = 1;
    model.bone_keyframe_summaries[0].index = Some(1);
    model.initial_morph_keyframe_summaries[0].next_keyframe_index = 1;
    model.morph_keyframe_summaries[0].index = Some(1);

    let clip = build_pmm_document_model_clip(&model).unwrap();

    assert_eq!(clip.bone_tracks().len(), 1);
    let bone_track = clip.find_bone_track(BoneIndex(0)).unwrap();
    assert_eq!(bone_track.keyframe_count(), 2);
    assert_eq!(bone_track.frame_range(), Some((0, 30)));
    assert_eq!(bone_track.sample(0.0).unwrap().0, Vec3A::ZERO);
    assert_eq!(bone_track.sample(30.0).unwrap().1, Quat::IDENTITY);

    assert_eq!(clip.morph_tracks().len(), 1);
    let morph_track = clip.find_morph_track(MorphIndex(0)).unwrap();
    assert_eq!(morph_track.keyframe_count(), 2);
    assert!((morph_track.sample(0.0).unwrap() - 0.5).abs() < 1.0e-6);
    assert!((morph_track.sample(30.0).unwrap() - 0.5).abs() < 1.0e-6);
}

#[test]
fn pmm_document_clip_builder_ignores_unlinked_deleted_keyframes() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.bone_keyframe_summaries[0].index = Some(1);
    model.bone_keyframe_summaries[0].previous_keyframe_index = 0;
    model.bone_keyframe_summaries[0].next_keyframe_index = 0;
    model.morph_keyframe_summaries[0].index = Some(1);
    model.morph_keyframe_summaries[0].previous_keyframe_index = 0;
    model.morph_keyframe_summaries[0].next_keyframe_index = 0;

    let clip = build_pmm_document_model_clip(&model).unwrap();

    assert_eq!(
        clip.find_bone_track(BoneIndex(0)).unwrap().keyframe_count(),
        1
    );
    assert_eq!(
        clip.find_morph_track(MorphIndex(0))
            .unwrap()
            .keyframe_count(),
        1
    );
}

#[test]
fn pmm_document_clip_builder_rejects_disconnected_bone_chain() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.bone_keyframe_summaries[0].index = Some(1);
    model.bone_keyframe_summaries[0].previous_keyframe_index = 7;
    model.bone_keyframe_summaries[0].next_keyframe_index = 0;
    model.morph_keyframe_summaries.clear();

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("1 disconnected additional bone keyframe")
    );
}

#[test]
fn pmm_document_clip_builder_rejects_disconnected_morph_chain() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.bone_keyframe_summaries.clear();
    model.morph_keyframe_summaries[0].index = Some(1);
    model.morph_keyframe_summaries[0].previous_keyframe_index = 7;
    model.morph_keyframe_summaries[0].next_keyframe_index = 0;

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("1 disconnected additional morph keyframe")
    );
}

#[test]
fn pmm_document_clip_builder_rejects_missing_linked_keyframe() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.initial_bone_keyframe_summaries[0].next_keyframe_index = 999;
    model.bone_keyframe_summaries[0].index = Some(1);

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(error.to_string().contains("missing index 999"));
}

#[test]
fn pmm_document_clip_builder_rejects_negative_bone_keyframe_index() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.bone_keyframe_summaries[0].index = Some(-1);

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("additional bone keyframe index -1 must be non-negative")
    );
}

#[test]
fn pmm_document_clip_builder_rejects_negative_morph_keyframe_index() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.bone_keyframe_summaries.clear();
    model.morph_keyframe_summaries[0].index = Some(-1);

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("additional morph keyframe index -1 must be non-negative")
    );
}

#[test]
fn pmm_document_clip_builder_rejects_wrong_bone_keyframe_backlink() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.initial_bone_keyframe_summaries[0].next_keyframe_index = 1;
    model.bone_keyframe_summaries[0].index = Some(1);
    model.bone_keyframe_summaries[0].previous_keyframe_index = 99;

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("PMM bone 0 keyframe index 1 has previous index 99; expected 0")
    );
}

#[test]
fn pmm_document_clip_builder_rejects_wrong_morph_keyframe_backlink() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.bone_keyframe_summaries.clear();
    model.initial_morph_keyframe_summaries[0].next_keyframe_index = 1;
    model.morph_keyframe_summaries[0].index = Some(1);
    model.morph_keyframe_summaries[0].previous_keyframe_index = 99;

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("PMM morph 0 keyframe index 1 has previous index 99; expected 0")
    );
}

#[test]
fn pmm_document_clip_builder_rejects_bone_index_in_initial_namespace() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let model = parsed.document_summary.unwrap().models.remove(0);

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(error.to_string().contains(
        "PMM additional bone keyframe index 0 overlaps initial bone track index namespace"
    ));
}

#[test]
fn pmm_document_clip_builder_rejects_morph_index_in_initial_namespace() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.bone_keyframe_summaries.clear();

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(error.to_string().contains(
        "PMM additional morph keyframe index 0 overlaps initial morph track index namespace"
    ));
}

#[test]
fn builds_nonzero_track_two_hop_pmm_chains() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);

    model.bone_count = 2;
    model.initial_bone_keyframe_summaries[0].next_keyframe_index = 0;
    let mut bone_track_one_initial = model.initial_bone_keyframe_summaries[0].clone();
    bone_track_one_initial.next_keyframe_index = 2;
    model
        .initial_bone_keyframe_summaries
        .push(bone_track_one_initial);
    let mut bone_first = model.bone_keyframe_summaries[0].clone();
    bone_first.index = Some(2);
    bone_first.frame_index = 15;
    bone_first.previous_keyframe_index = 1;
    bone_first.next_keyframe_index = 3;
    let mut bone_second = bone_first.clone();
    bone_second.index = Some(3);
    bone_second.frame_index = 30;
    bone_second.previous_keyframe_index = 2;
    bone_second.next_keyframe_index = 0;
    model.bone_keyframe_summaries = vec![bone_first, bone_second];

    model.morph_count = 2;
    model.initial_morph_keyframe_summaries[0].next_keyframe_index = 0;
    let mut morph_track_one_initial = model.initial_morph_keyframe_summaries[0].clone();
    morph_track_one_initial.next_keyframe_index = 2;
    model
        .initial_morph_keyframe_summaries
        .push(morph_track_one_initial);
    let mut morph_first = model.morph_keyframe_summaries[0].clone();
    morph_first.index = Some(2);
    morph_first.frame_index = 15;
    morph_first.previous_keyframe_index = 1;
    morph_first.next_keyframe_index = 3;
    let mut morph_second = morph_first.clone();
    morph_second.index = Some(3);
    morph_second.frame_index = 30;
    morph_second.previous_keyframe_index = 2;
    morph_second.next_keyframe_index = 0;
    model.morph_keyframe_summaries = vec![morph_first, morph_second];

    let clip = build_pmm_document_model_clip(&model).unwrap();

    let bone_track = clip.find_bone_track(BoneIndex(1)).unwrap();
    assert_eq!(bone_track.keyframe_count(), 3);
    assert_eq!(bone_track.frame_range(), Some((0, 30)));
    let morph_track = clip.find_morph_track(MorphIndex(1)).unwrap();
    assert_eq!(morph_track.keyframe_count(), 3);
    assert_eq!(morph_track.frame_range(), Some((0, 30)));
}

#[test]
fn pmm_document_clip_builder_rejects_indexed_initial_bone_keyframe() {
    let parsed = parse_pmm_manifest(&pmm_with_document_summary()).unwrap();
    let mut model = parsed.document_summary.unwrap().models.remove(0);
    model.initial_bone_keyframe_summaries[0].index = Some(0);

    let error = build_pmm_document_model_clip(&model).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("PMM bone 0 initial keyframe must not have explicit index 0")
    );
}

#[test]
fn decodes_pmm_bone_interpolation_as_contiguous_curves() {
    let interpolation = [1, 2, 3, 4, 11, 12, 13, 14, 21, 22, 23, 24, 31, 32, 33, 34];

    let (position, rotation) = decode_pmm_bone_interpolation(interpolation);

    assert_eq!(
        position.x,
        InterpolationScalar {
            x1: 1,
            y1: 2,
            x2: 3,
            y2: 4
        }
    );
    assert_eq!(
        position.y,
        InterpolationScalar {
            x1: 11,
            y1: 12,
            x2: 13,
            y2: 14
        }
    );
    assert_eq!(
        position.z,
        InterpolationScalar {
            x1: 21,
            y1: 22,
            x2: 23,
            y2: 24
        }
    );
    assert_eq!(
        rotation,
        InterpolationScalar {
            x1: 31,
            y1: 32,
            x2: 33,
            y2: 34
        }
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
            "byteLength",
            "frameIndex",
            "index",
            "nextKeyframeIndex",
            "offset",
            "payloadByteLength",
            "payloadBytes",
            "payloadOffset",
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
    assert_eq!(ibk.byte_length, 58);
    assert_eq!(ibk.offset, model.sections.initial_bone_keyframes_offset);
    assert_eq!(ibk.payload_offset, ibk.offset + 12);
    assert_eq!(ibk.payload_byte_length, 46);
    assert_eq!(ibk.payload_bytes.len(), 46);
    assert_eq!(ibk.payload_bytes.len(), ibk.payload_byte_length);
    assert_eq!(&ibk.payload_bytes[..16], &ibk.interpolation[..]);

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
    assert_eq!(bk.byte_length, 62);
    assert_eq!(bk.offset, model.sections.bone_keyframes_offset);
    assert_eq!(bk.payload_offset, bk.offset + 16);
    assert_eq!(bk.payload_byte_length, 46);
    assert_eq!(bk.payload_bytes.len(), 46);
    assert_eq!(bk.payload_bytes.len(), bk.payload_byte_length);
    assert_eq!(&bk.payload_bytes[..16], &bk.interpolation[..]);

    assert_eq!(model.initial_morph_keyframe_summaries.len(), 1);
    let imk = &model.initial_morph_keyframe_summaries[0];
    assert_eq!(imk.index, None);
    assert_eq!(imk.frame_index, 0);
    assert_eq!(imk.previous_keyframe_index, 0);
    assert_eq!(imk.next_keyframe_index, 0);
    assert!((imk.weight - 0.5).abs() < 1e-6);
    assert!(!imk.selected);
    assert_eq!(imk.byte_length, 17);
    assert_eq!(imk.offset, model.sections.initial_morph_keyframes_offset);
    assert_eq!(imk.payload_offset, imk.offset + 12);
    assert_eq!(imk.payload_byte_length, 5);
    assert_eq!(imk.payload_bytes.len(), 5);
    assert_eq!(imk.payload_bytes.len(), imk.payload_byte_length);
    let imk_weight_bytes = f32::from_le_bytes([
        imk.payload_bytes[0],
        imk.payload_bytes[1],
        imk.payload_bytes[2],
        imk.payload_bytes[3],
    ]);
    assert!((imk_weight_bytes - imk.weight).abs() < 1e-6);
    assert_eq!(imk.payload_bytes[4], imk.selected as u8);

    assert_eq!(model.morph_keyframe_summaries.len(), 1);
    let mk = &model.morph_keyframe_summaries[0];
    assert_eq!(mk.index, Some(0));
    assert_eq!(mk.frame_index, 15);
    assert_eq!(mk.previous_keyframe_index, 0);
    assert_eq!(mk.next_keyframe_index, 0);
    assert!((mk.weight - 0.5).abs() < 1e-6);
    assert!(!mk.selected);
    assert_eq!(mk.byte_length, 21);
    assert_eq!(mk.offset, model.sections.morph_keyframes_offset);
    assert_eq!(mk.payload_offset, mk.offset + 16);
    assert_eq!(mk.payload_byte_length, 5);
    assert_eq!(mk.payload_bytes.len(), 5);
    assert_eq!(mk.payload_bytes.len(), mk.payload_byte_length);
    let mk_weight_bytes = f32::from_le_bytes([
        mk.payload_bytes[0],
        mk.payload_bytes[1],
        mk.payload_bytes[2],
        mk.payload_bytes[3],
    ]);
    assert!((mk_weight_bytes - mk.weight).abs() < 1e-6);
    assert_eq!(mk.payload_bytes[4], mk.selected as u8);
}

#[test]
fn pmm_v2_document_accepts_sparse_selected_model_slot() {
    let mut data = pmm_with_document_global_summary();
    data[53] = 5; // selected slot
    data[55] = 5; // the single serialized model occupies slot 5
    let parsed = parse_pmm_manifest(&data).unwrap();
    let document = parsed.document_summary.as_ref().unwrap();
    assert_eq!(document.selected_model_index, 5);
    assert_eq!(document.model_count, 1);
    assert_eq!(document.models[0].document_model_index, 5);
    let global = parsed.document_global_summary.as_ref().unwrap();
    assert_eq!(global.camera.keyframes, 1);
    assert_eq!(global.accessories.accessory_count, 1);
    let graph = parsed.project_graph.as_ref().unwrap();
    assert_eq!(graph.models[0].document_model_index, 5);
    assert_eq!(export_pmm_manifest(&parsed), data);
}

#[test]
fn pmm_v2_document_preserves_unoccupied_selected_slot() {
    let mut data = pmm_with_document_global_summary();
    data[53] = 5;
    let parsed = parse_pmm_manifest(&data).unwrap();
    let document = parsed.document_summary.as_ref().unwrap();
    assert_eq!(document.selected_model_index, 5);
    assert_eq!(document.models[0].document_model_index, 0);
    assert!(parsed.project_graph.is_some());
}

#[test]
fn pmm_v2_document_loads_global_tracks_without_models() {
    let mut data = pmm_with_document_global_summary();
    let original = parse_pmm_manifest(&data).unwrap();
    let global = original.document_global_summary.unwrap();
    // Remove the one model's footer entry as well as its main record.
    let footer = global.settings.model_selection_footer_offset.unwrap();
    data.drain(footer + 1..global.settings.model_selection_footer_end_offset.unwrap());
    data.drain(55..global.offset);
    data[54] = 0;

    let parsed = parse_pmm_manifest(&data).unwrap();
    let document = parsed.document_summary.as_ref().unwrap();
    assert_eq!(document.model_count, 0);
    assert!(document.models.is_empty());
    assert_eq!(document.counts.models, 0);
    let global = parsed.document_global_summary.as_ref().unwrap();
    assert_eq!(global.offset, 55);
    assert_eq!(global.offset_end, data.len());
    assert_eq!(global.camera.keyframes, 1);
    assert_eq!(global.light.keyframes, 1);
    assert_eq!(global.accessories.accessory_count, 1);
    assert_eq!(global.gravity.keyframes, 1);
    assert_eq!(global.self_shadow.keyframes, 1);
    assert_eq!(global.settings.current_frame_index, 12);
    assert_eq!(global.settings.end_frame_index, 120);
    let graph = parsed.project_graph.as_ref().unwrap();
    assert!(graph.models.is_empty());
    assert_eq!(graph.accessory_count, 1);
    assert_eq!(export_pmm_manifest(&parsed), data);

    // A valid empty model list must not turn a truncated global track into
    // a complete document graph.
    data.truncate(global.camera.offset + 1);
    let truncated = parse_pmm_manifest(&data).unwrap();
    assert!(truncated.document_summary.is_some());
    assert!(truncated.document_global_summary.is_none());
    assert!(truncated.project_graph.is_none());
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
        "C:\\MMDDev\\data\\unittest\\test_1bone_cube.pmx"
    );
    assert_eq!(
        slot.normalized_path,
        "C:/MMDDev/data/unittest/test_1bone_cube.pmx"
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
fn rejects_invalid_pmm_magic() {
    let err = parse_pmm_manifest(b"not a PMM project").unwrap_err();

    assert_eq!(err, ImportError::InvalidMagic { format: "PMM" });
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
    let data = b"Polygon Movie maker 0002\0UserFile\\Background\\stage.avs\0C:\\Video\\movie.avi\0";
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
    let data = b"Polygon Movie maker 0002\0C:\\MMDDev\\data\\unittest\\test_1bone_cube.pmx\0:\\MMDDev\\data\\unittest\\test_1bone_cube.pmx\0";
    let parsed = parse_pmm_manifest(data).unwrap();

    assert_eq!(parsed.model_assets.len(), 2);
    assert_eq!(
        parsed.model_assets[0].normalized_path,
        "C:/MMDDev/data/unittest/test_1bone_cube.pmx"
    );
    assert_eq!(parsed.model_assets[0].confidence, "high");
    assert_eq!(
        parsed.model_assets[1].normalized_path,
        ":/MMDDev/data/unittest/test_1bone_cube.pmx"
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

    let mut references = parsed.asset_references;
    references[1].normalized_path = "c:/models/miku.pmx".to_owned();
    assert!(duplicate_asset_reference_diagnostics(&references).is_empty());

    references[0].normalized_path = "C:/Ä/miku.pmx".to_owned();
    references[1].normalized_path = "C:/ä/miku.pmx".to_owned();
    assert_eq!(duplicate_asset_reference_diagnostics(&references).len(), 1);
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
    let keys = json_keys(&serde_json::to_value(&model.outside_parent_state_summaries[0]).unwrap());

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
    let data =
        b"Polygon Movie maker 0002\0C:\\notuserfile\\Model\\fake.pmx\0dummy.x_v2\0model.x_v2.pmd\0";
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
    assert_eq!(initial.payload_bytes.len(), 39);
    assert_eq!(initial.payload_bytes.len(), initial.payload_byte_length);
    assert_eq!(initial.payload_bytes[0], 1);
    assert_eq!(
        &initial.payload_bytes[initial.payload_bytes.len() - 2..],
        &[1, 0]
    );

    // layout metadata for accessory keyframe payload (exporter-prep slice).
    // offsets are absolute; prove they address correct bytes inside payload_bytes for initial keyframe.
    assert_eq!(
        initial.packed_opacity_visible_offset,
        initial.payload_offset
    );
    assert_eq!(
        initial.parent_model_index_offset,
        initial.payload_offset + 1
    );
    assert_eq!(
        initial.parent_model_bone_index_offset,
        initial.payload_offset + 5
    );
    assert_eq!(initial.translation_offset, initial.payload_offset + 9);
    assert_eq!(initial.translation_byte_length, 12);
    assert_eq!(initial.orientation_offset, initial.payload_offset + 21);
    assert_eq!(initial.orientation_byte_length, 12);
    assert_eq!(initial.scale_factor_offset, initial.payload_offset + 33);
    assert_eq!(initial.shadow_enabled_offset, initial.payload_offset + 37);
    assert_eq!(initial.selected_offset, initial.payload_offset + 38);
    let p0 = &initial.payload_bytes;
    assert_eq!(
        p0[initial.packed_opacity_visible_offset - initial.payload_offset],
        1u8
    );
    let pmi0 = initial.parent_model_index_offset - initial.payload_offset;
    assert_eq!(&p0[pmi0..pmi0 + 4], &(-1i32).to_le_bytes());
    let pbi0 = initial.parent_model_bone_index_offset - initial.payload_offset;
    assert_eq!(&p0[pbi0..pbi0 + 4], &(-1i32).to_le_bytes());
    let t0 = initial.translation_offset - initial.payload_offset;
    assert_eq!(&p0[t0..t0 + 4], &1.0f32.to_le_bytes());
    let o0 = initial.orientation_offset - initial.payload_offset;
    assert_eq!(&p0[o0 + 8..o0 + 12], &0.3f32.to_le_bytes());
    let sf0 = initial.scale_factor_offset - initial.payload_offset;
    assert_eq!(&p0[sf0..sf0 + 4], &10.0f32.to_le_bytes());
    assert_eq!(
        p0[initial.shadow_enabled_offset - initial.payload_offset],
        1
    );
    assert_eq!(p0[initial.selected_offset - initial.payload_offset], 0);

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
    assert_eq!(kf.payload_bytes.len(), 39);
    assert_eq!(kf.payload_bytes.len(), kf.payload_byte_length);
    assert_eq!(kf.payload_bytes[0], 1);
    assert_eq!(&kf.payload_bytes[kf.payload_bytes.len() - 2..], &[1, 0]);

    // same layout assertions for indexed (non-initial) accessory keyframe; relative positions inside payload identical.
    assert_eq!(kf.packed_opacity_visible_offset, kf.payload_offset);
    assert_eq!(kf.parent_model_index_offset, kf.payload_offset + 1);
    assert_eq!(kf.parent_model_bone_index_offset, kf.payload_offset + 5);
    assert_eq!(kf.translation_offset, kf.payload_offset + 9);
    assert_eq!(kf.translation_byte_length, 12);
    assert_eq!(kf.orientation_offset, kf.payload_offset + 21);
    assert_eq!(kf.orientation_byte_length, 12);
    assert_eq!(kf.scale_factor_offset, kf.payload_offset + 33);
    assert_eq!(kf.shadow_enabled_offset, kf.payload_offset + 37);
    assert_eq!(kf.selected_offset, kf.payload_offset + 38);
    let p1 = &kf.payload_bytes;
    assert_eq!(
        p1[kf.packed_opacity_visible_offset - kf.payload_offset],
        1u8
    );
    let pmi1 = kf.parent_model_index_offset - kf.payload_offset;
    assert_eq!(&p1[pmi1..pmi1 + 4], &(-1i32).to_le_bytes());
    let pbi1 = kf.parent_model_bone_index_offset - kf.payload_offset;
    assert_eq!(&p1[pbi1..pbi1 + 4], &(-1i32).to_le_bytes());
    let t1 = kf.translation_offset - kf.payload_offset;
    assert_eq!(&p1[t1..t1 + 4], &1.0f32.to_le_bytes());
    let o1 = kf.orientation_offset - kf.payload_offset;
    assert_eq!(&p1[o1 + 8..o1 + 12], &0.3f32.to_le_bytes());
    let sf1 = kf.scale_factor_offset - kf.payload_offset;
    assert_eq!(&p1[sf1..sf1 + 4], &10.0f32.to_le_bytes());
    assert_eq!(p1[kf.shadow_enabled_offset - kf.payload_offset], 1);
    assert_eq!(p1[kf.selected_offset - kf.payload_offset], 0);
}

#[test]
fn parses_ik_multi_bone_pmm_fixture_document_summary_and_keyframes() {
    let data = include_bytes!("../../fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let parsed = parse_pmm_manifest(data).unwrap();

    let document = parsed
        .document_summary
        .as_ref()
        .expect("document summary must exist for this fixture");
    assert_eq!(document.model_count, 1);
    assert_eq!(document.counts.models, 1);
    assert_eq!(document.counts.bones, 3);
    assert_eq!(document.counts.morphs, 0);
    assert_eq!(document.counts.initial_bone_keyframes, 3);
    assert_eq!(document.counts.bone_keyframes, 2);
    assert_eq!(document.counts.initial_morph_keyframes, 0);
    assert_eq!(document.counts.morph_keyframes, 0);
    assert_eq!(document.counts.initial_model_keyframes, 1);
    assert_eq!(document.counts.model_keyframes, 0);

    assert_eq!(document.models.len(), 1);
    let model = &document.models[0];
    assert_eq!(model.bone_count, 3);
    assert_eq!(model.morph_count, 0);
    assert_eq!(model.initial_bone_keyframes, 3);
    assert_eq!(model.bone_keyframes, 2);
    assert_eq!(model.initial_morph_keyframes, 0);
    assert_eq!(model.morph_keyframes, 0);
    assert_eq!(model.initial_model_keyframes, 1);
    assert_eq!(model.model_keyframes, 0);

    // Parser fixture only: direct MMD GUI load can fail on this relative model path.
    // GUI smoke PMMs should be generated by the CLI, which embeds an absolute path.
    let model_path = &model.path;
    assert_eq!(
        model_path, "..\\pmx\\ik_multi_axis_limit.pmx",
        "model path must be the corrected resolvable relative path"
    );

    // non-default additional bone keyframes are present inside the additional keyframe block.
    assert_eq!(model.bone_keyframe_summaries.len(), 2);

    let link15 = model
        .bone_keyframe_summaries
        .iter()
        .find(|kf| kf.frame_index == 15)
        .expect("link_root frame 15 must be present as additional keyframe");
    assert_eq!(link15.previous_keyframe_index, 0);
    assert_eq!(link15.next_keyframe_index, 0);
    assert_eq!(link15.translation, [0.05, 0.0, 0.0]);
    assert_eq!(link15.orientation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        link15.interpolation,
        [
            10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160
        ]
    );
    assert!(!link15.physics_disabled);

    let eff30 = model
        .bone_keyframe_summaries
        .iter()
        .find(|kf| kf.frame_index == 30)
        .expect("effector frame 30 must be present as additional keyframe");
    assert_eq!(eff30.previous_keyframe_index, 1);
    assert_eq!(eff30.next_keyframe_index, 0);
    assert_eq!(eff30.translation, [0.0, 0.1, 0.0]);
    assert_eq!(eff30.orientation, [0.0, 0.0, 0.0, 1.0]);
    assert!(!eff30.physics_disabled);

    // all asserted physics_disabled flags are false
    for kf in &model.initial_bone_keyframe_summaries {
        assert!(!kf.physics_disabled);
    }
    for kf in &model.bone_keyframe_summaries {
        assert!(!kf.physics_disabled);
    }

    // relevant section offsets are monotonic (and model offsets)
    let s = &model.sections;
    assert!(s.initial_bone_keyframes_offset < s.bone_keyframe_count_offset);
    assert!(s.bone_keyframe_count_offset < s.bone_keyframes_offset);
    assert!(s.bone_keyframes_offset < s.bone_keyframes_end_offset);
    assert!(s.initial_morph_keyframes_offset <= s.morph_keyframe_count_offset);
    assert!(s.morph_keyframes_offset <= s.morph_keyframes_end_offset);
    assert!(model.offset_end > model.offset);
    assert!(s.bone_keyframes_end_offset > s.bone_keyframes_offset);
}

#[test]
fn pmm_ik_multi_bone_fixture_has_expected_document_summary() {
    let data = include_bytes!("../../fixtures/pmm/ik_multi_bone_from_pmx_vmd.pmm");
    let parsed = parse_pmm_manifest(data).unwrap();

    assert_eq!(
        parsed.byte_length, 2125,
        "byte_length must match the checked-in PMM fixture"
    );

    let document = parsed
        .document_summary
        .as_ref()
        .expect("document summary must exist for this fixture");
    assert_eq!(document.selected_model_index, 0);

    let c = &document.counts;
    assert_eq!(c.models, 1);
    assert_eq!(c.bones, 3);
    assert_eq!(c.morphs, 0);
    assert_eq!(c.bone_keyframes, 2);
    assert_eq!(c.morph_keyframes, 0);

    assert_eq!(document.models.len(), 1);
    let model = &document.models[0];
    assert_eq!(model.document_model_index, 0);
    assert_eq!(model.name, "ik_multi_axis_limit_fixture");
    assert_eq!(model.english_name, "ik_multi_axis_limit_fixture");
    assert_eq!(model.path, "..\\pmx\\ik_multi_axis_limit.pmx");
    assert_eq!(model.path_offset, 112);
    assert_eq!(model.bone_count, 3);
    assert_eq!(model.morph_count, 0);
    assert_eq!(model.last_frame_index, 30);

    let s = &model.sections;
    assert_eq!(s.initial_bone_keyframes_offset, 449);
    assert_eq!(s.bone_keyframe_count_offset, 623);
    assert_eq!(s.bone_keyframes_offset, 627);
    assert_eq!(s.bone_keyframes_end_offset, 751);

    assert_eq!(model.initial_bone_keyframe_summaries.len(), 3);
    for ibk in model.initial_bone_keyframe_summaries.iter() {
        assert_eq!(ibk.byte_length, 58);
        assert_eq!(ibk.payload_offset, ibk.offset + 12);
        assert_eq!(ibk.payload_byte_length, 46);
        assert_eq!(ibk.payload_bytes.len(), ibk.payload_byte_length);
        assert_eq!(&ibk.payload_bytes[..16], &ibk.interpolation[..]);
    }
    assert_eq!(model.initial_bone_keyframe_summaries[0].offset, 449);
    assert_eq!(model.initial_bone_keyframe_summaries[1].offset, 507);
    assert_eq!(model.initial_bone_keyframe_summaries[2].offset, 565);

    let imkf = &model.initial_model_keyframe;
    assert_eq!(imkf.offset, 755);
    assert_eq!(imkf.byte_length, 14);

    let link15 = model
        .bone_keyframe_summaries
        .iter()
        .find(|kf| kf.frame_index == 15)
        .expect("link_root frame 15 must be present");
    assert_eq!(link15.offset, 627);
    assert_eq!(link15.byte_length, 62);
    assert_eq!(link15.payload_offset, link15.offset + 16);
    assert_eq!(link15.payload_byte_length, 46);
    assert_eq!(link15.payload_bytes.len(), link15.payload_byte_length);
    assert_eq!(&link15.payload_bytes[..16], &link15.interpolation[..]);
    assert_eq!(link15.previous_keyframe_index, 0);
    assert_eq!(link15.next_keyframe_index, 0);
    assert_eq!(link15.translation, [0.05, 0.0, 0.0]);

    let eff30 = model
        .bone_keyframe_summaries
        .iter()
        .find(|kf| kf.frame_index == 30)
        .expect("effector frame 30 must be present");
    assert_eq!(eff30.offset, 689);
    assert_eq!(eff30.byte_length, 62);
    assert_eq!(eff30.payload_offset, eff30.offset + 16);
    assert_eq!(eff30.payload_byte_length, 46);
    assert_eq!(eff30.payload_bytes.len(), eff30.payload_byte_length);
    assert_eq!(&eff30.payload_bytes[..16], &eff30.interpolation[..]);
    assert_eq!(eff30.previous_keyframe_index, 1);
    assert_eq!(eff30.next_keyframe_index, 0);
    assert_eq!(eff30.translation, [0.0, 0.1, 0.0]);
}

#[test]
fn exports_pmm_scene_from_pmx_vmd_roundtrips_ik_multi_bone_nondefault() {
    let pmx = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
    let model = crate::pmx::parse_pmx_model(pmx).unwrap();

    let vmd = include_bytes!("../../fixtures/vmd/ik_multi_bone_nondefault.vmd");
    let motion = crate::vmd::parse_vmd_animation(vmd).unwrap();

    let report = export_pmm_scene_from_pmx_vmd(
        &model,
        &motion,
        // Low-level writer preserves the supplied path; CLI GUI smoke canonicalizes it.
        "..\\pmx\\ik_multi_axis_limit.pmx",
        &PmmSceneExportOptions::default(),
    );

    let reparsed = parse_pmm_manifest(&report.bytes).unwrap();
    let document = reparsed
        .document_summary
        .as_ref()
        .expect("exported PMM must contain document summary");
    assert_eq!(document.model_count, 1);
    assert_eq!(document.counts.models, 1);
    assert_eq!(document.counts.bones, 3);
    assert_eq!(document.counts.morphs, 0);

    let m = &document.models[0];
    assert_eq!(m.bone_count, 3);
    assert_eq!(m.morph_count, 0);
    assert_eq!(m.path, "..\\pmx\\ik_multi_axis_limit.pmx");

    // semantic parity for the non-default additional frames from VMD
    let link15 = m
        .bone_keyframe_summaries
        .iter()
        .find(|kf| kf.frame_index == 15)
        .expect("exported PMM must contain link_root frame 15");
    assert_eq!(link15.translation, [0.05, 0.0, 0.0]);
    assert_eq!(link15.orientation, [0.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        link15.interpolation,
        [
            10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120, 130, 140, 150, 160
        ]
    );
    assert!(!link15.physics_disabled);

    let eff30 = m
        .bone_keyframe_summaries
        .iter()
        .find(|kf| kf.frame_index == 30)
        .expect("exported PMM must contain effector frame 30");
    assert_eq!(eff30.translation, [0.0, 0.1, 0.0]);
    assert_eq!(eff30.orientation, [0.0, 0.0, 0.0, 1.0]);
    assert!(!eff30.physics_disabled);

    // exporter always emits per-bone initials (fallback identity when absent in VMD) + additionals
    assert_eq!(m.initial_bone_keyframes, 3);
    assert_eq!(m.bone_keyframes, 2);

    assert_eq!(m.constraint_bone_count, 1);
    assert_eq!(m.outside_parent_subject_bone_count, 2);
    assert_eq!(m.initial_model_keyframe.constraint_states, vec![true]);
    for outside_parent in &m.initial_model_keyframe.outside_parent_indices {
        assert_eq!(outside_parent.parent_model_index, -1);
        assert_eq!(outside_parent.parent_model_bone_index, 0);
    }
    assert_eq!(m.constraint_state_summaries.len(), 1);
    assert!(m.constraint_state_summaries[0].enabled);
    assert_eq!(m.outside_parent_state_summaries.len(), 2);
    assert_eq!(m.outside_parent_state_summaries[0].parent_model_index, 0);
    assert_eq!(
        m.outside_parent_state_summaries[0].parent_model_bone_index,
        0
    );
    assert_eq!(m.outside_parent_state_summaries[0].subject_bone_index, -1);
    assert_eq!(m.outside_parent_state_summaries[0].target_model_index, 0);

    let fixed_tracks_offset = m.path_offset + 256;
    assert_eq!(reparsed.source_bytes[fixed_tracks_offset], 2);
}

#[test]
fn pmm_export_model_structure_matches_pmx_bone_flags_and_display_frames() {
    let pmx = include_bytes!("../../fixtures/pmx/ik_multi_axis_limit.pmx");
    let model = crate::pmx::parse_pmx_model(pmx).unwrap();
    let structure = pmm_export_model_structure_from_pmx(&model);

    assert_eq!(structure.fixed_tracks, 2);
    assert_eq!(structure.constraint_bone_indices, vec![2]);
    assert_eq!(structure.outside_parent_subject_bone_indices, vec![-1, 2]);
    assert!(pmm_is_outside_parent_subject_bone(&model.skeleton.bones[2]));
    assert!(!pmm_is_outside_parent_subject_bone(
        &model.skeleton.bones[0]
    ));
    assert!(!pmm_is_outside_parent_subject_bone(
        &model.skeleton.bones[1]
    ));
}
