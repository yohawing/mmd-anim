use super::*;

fn bone_frame(frame: u32) -> VmdParsedBoneFrame {
    VmdParsedBoneFrame {
        bone_name: "bone".to_owned(),
        bone_name_bytes: b"bone".to_vec(),
        frame,
        translation: [frame as f32, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        interpolation: vmd_interpolation_block(mmd_anim_runtime::VmdBoneInterpolation::LINEAR)
            .to_vec(),
    }
}

#[test]
fn track_gate_uses_candidate_only_when_it_is_strictly_smaller() {
    fn key(frame: &VmdParsedBoneFrame) -> (u32, &[u8]) {
        (frame.frame, frame.bone_name_bytes.as_slice())
    }

    let source = vec![bone_frame(0), bone_frame(1), bone_frame(2)];
    let smaller = prefer_smaller_tracks(&source, vec![bone_frame(0), bone_frame(2)], key);
    assert_eq!(smaller.len(), 2);
    let mut equal_candidate = source.clone();
    equal_candidate[1].translation = [99.0, 0.0, 0.0];
    let equal = prefer_smaller_tracks(&source, equal_candidate, key);
    assert_eq!(equal.len(), source.len());
    assert_eq!(equal[1].translation, source[1].translation);
}

#[test]
fn morph_track_gate_groups_normalized_names_and_sorts_selected_frames() {
    let frame = |name: &[u8], frame, weight| VmdParsedMorphFrame {
        morph_name: String::from_utf8_lossy(name).into_owned(),
        morph_name_bytes: name.to_vec(),
        frame,
        weight,
    };
    let source = vec![
        frame(b"b", 2, 0.2),
        frame(b"a\0", 1, 0.1),
        frame(b"a", 0, 0.0),
        frame(b"b", 0, 0.0),
    ];
    let candidate = vec![
        frame(b"a", 0, 0.5),
        frame(b"b", 2, 0.9),
        frame(b"b", 0, 0.9),
    ];

    let selected = prefer_smaller_tracks(&source, candidate, |frame| {
        (frame.frame, frame.morph_name_bytes.as_slice())
    });
    let actual: Vec<_> = selected
        .iter()
        .map(|frame| (frame.frame, frame.morph_name_bytes.as_slice(), frame.weight))
        .collect();
    assert_eq!(
        actual,
        vec![(0, b"a".as_slice(), 0.5), (0, b"b", 0.0), (2, b"b", 0.2)]
    );
}

#[test]
fn linear_interpolation_is_repeated_in_all_vmd_blocks() {
    let block = vmd_interpolation_block(mmd_anim_runtime::VmdBoneInterpolation::LINEAR);
    for chunk in block.chunks_exact(16) {
        assert_eq!(chunk, &block[..16]);
    }
    assert_eq!(&block[..4], &[20, 20, 20, 20]);
    assert_eq!(&block[8..12], &[107, 107, 107, 107]);
}

#[test]
fn rejects_the_same_file_through_an_alias_path() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mmd-anim-reduce-vmd-{unique}"));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("motion.vmd");
    std::fs::write(&input, b"fixture").unwrap();
    let alias = dir.join(".").join("motion.vmd");
    assert!(ensure_distinct_paths(&input, &alias).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}
