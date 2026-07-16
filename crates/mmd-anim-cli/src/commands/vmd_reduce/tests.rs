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
    let source = vec![bone_frame(0), bone_frame(1), bone_frame(2)];
    let smaller = prefer_smaller_bone_tracks(&source, vec![bone_frame(0), bone_frame(2)]);
    assert_eq!(smaller.len(), 2);
    let mut equal_candidate = source.clone();
    equal_candidate[1].translation = [99.0, 0.0, 0.0];
    let equal = prefer_smaller_bone_tracks(&source, equal_candidate);
    assert_eq!(equal.len(), source.len());
    assert_eq!(equal[1].translation, source[1].translation);
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
