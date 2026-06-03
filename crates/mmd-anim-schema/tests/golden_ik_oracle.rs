// External asset compare tests.
// These require a local MMDDumper checkout and are NOT the always-on
// inline schema parse tests.  The always-on tests live in
// `crates/mmd-anim-schema/src/mmd_dumper_oracle.rs`.

use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    path::PathBuf,
};

use mmd_anim_schema::{
    DEFAULT_FOCUSED_IK_BONE_NAMES, GoldenIkBatchManifest, GoldenIkFixture, MmdDumperOracleDump,
};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap()
}

fn external_golden_root() -> Option<PathBuf> {
    let root = workspace_root().join("MMDDumper/out/golden-ik-oracle");
    let manifest_path = root.join("oracle-batch.json");
    if !manifest_path.exists() {
        return None;
    }
    let manifest =
        GoldenIkBatchManifest::from_json_str(&fs::read_to_string(&manifest_path).ok()?).ok()?;
    for case in manifest.cases {
        let case_root = root.join(&case.name);
        let fixture_path = case_root.join("fixture.json");
        let fixture =
            GoldenIkFixture::from_json_str(&fs::read_to_string(&fixture_path).ok()?).ok()?;
        if !resolve_oracle_path(&case_root, &fixture.output).exists() {
            return None;
        }
    }
    Some(root)
}

#[test]
fn parses_first_record_from_each_golden_ik_oracle_case() {
    let Some(root) = external_golden_root() else {
        eprintln!("skipping external MMDDumper fixture smoke: oracle-batch.json not found");
        return;
    };
    let manifest = GoldenIkBatchManifest::from_json_str(
        &fs::read_to_string(root.join("oracle-batch.json")).unwrap(),
    )
    .unwrap();

    for case in manifest.cases {
        let case_root = root.join(&case.name);
        let fixture = GoldenIkFixture::from_json_str(
            &fs::read_to_string(case_root.join("fixture.json")).unwrap(),
        )
        .unwrap();
        let oracle_path = PathBuf::from(&fixture.output);
        let oracle_path = if oracle_path.is_absolute() {
            oracle_path
        } else {
            case_root.join(oracle_path)
        };
        let first_line = BufReader::new(File::open(oracle_path).unwrap())
            .lines()
            .map(|line| line.unwrap())
            .find(|line| !line.trim().is_empty())
            .unwrap();
        let dump = MmdDumperOracleDump::from_jsonl_str(&first_line, None).unwrap();
        let first_model = &dump.frames[0].models[0];

        assert!(
            !first_model.bones.is_empty(),
            "{} should contain bone matrices",
            case.name
        );
        assert!(
            first_model
                .focused_ik_bones(DEFAULT_FOCUSED_IK_BONE_NAMES)
                .next()
                .is_some(),
            "{} should contain at least one focused IK bone",
            case.name
        );
    }
}

#[test]
fn parses_selected_frames_from_each_golden_ik_oracle_case() {
    let Some(root) = external_golden_root() else {
        eprintln!("skipping external MMDDumper fixture smoke: oracle-batch.json not found");
        return;
    };
    let manifest = GoldenIkBatchManifest::from_json_str(
        &fs::read_to_string(root.join("oracle-batch.json")).unwrap(),
    )
    .unwrap();

    for case in manifest.cases {
        let case_root = root.join(&case.name);
        let fixture = GoldenIkFixture::from_json_str(
            &fs::read_to_string(case_root.join("fixture.json")).unwrap(),
        )
        .unwrap();
        let oracle_path = resolve_oracle_path(&case_root, &fixture.output);
        let frames = if fixture.frames.is_empty() {
            case.frames.as_slice()
        } else {
            fixture.frames.as_slice()
        };
        let dump = MmdDumperOracleDump::from_jsonl_str(
            &fs::read_to_string(oracle_path).unwrap(),
            Some(frames),
        )
        .unwrap();

        assert_eq!(
            dump.frames.len(),
            frames.len(),
            "{} should contain every selected frame",
            case.name
        );

        for frame in &dump.frames {
            let first_model = &frame.models[0];
            assert!(
                first_model
                    .focused_ik_bones(DEFAULT_FOCUSED_IK_BONE_NAMES)
                    .next()
                    .is_some(),
                "{} frame {} should contain at least one focused IK bone",
                case.name,
                frame.frame
            );
        }
    }
}

fn resolve_oracle_path(case_root: &std::path::Path, output: &str) -> PathBuf {
    let oracle_path = PathBuf::from(output);
    if oracle_path.is_absolute() {
        oracle_path
    } else {
        case_root.join(oracle_path)
    }
}
