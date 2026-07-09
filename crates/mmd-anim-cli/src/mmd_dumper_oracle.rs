use std::collections::BTreeSet;

use serde::Deserialize;
use thiserror::Error;

use crate::schema::SCHEMA_VERSION;

pub const DEFAULT_FOCUSED_IK_BONE_NAMES: &[&str] = &[
    "左足",
    "右足",
    "左ひざ",
    "右ひざ",
    "左足首",
    "右足首",
    "左つま先",
    "右つま先",
    "左足ＩＫ",
    "右足ＩＫ",
    "左つま先ＩＫ",
    "右つま先ＩＫ",
    "左足IK",
    "右足IK",
    "左つま先IK",
    "右つま先IK",
];

#[derive(Debug, Error)]
pub enum MmdDumperOracleParseError {
    #[error("invalid JSONL at line {line}: {source}")]
    Json {
        line: usize,
        #[source]
        source: serde_json::Error,
    },
    #[error("MMDDumper oracle JSONL is empty")]
    Empty,
    #[error("unsupported MMDDumper oracle schemaVersion {schema_version}")]
    UnsupportedSchema { schema_version: u32 },
    #[error(
        "MMDDumper oracle bone matrix must have 16 components: frame={frame} model={model} bone={bone}"
    )]
    InvalidBoneMatrix {
        frame: i32,
        model: usize,
        bone: usize,
    },
    #[error("invalid manifest JSON: {source}")]
    ManifestJson {
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "Unity runtime verification report has no matching caseResult for case={case_name:?} pmx={pmx_path:?}"
    )]
    UnityCaseNotFound {
        case_name: Option<String>,
        pmx_path: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct MmdDumperOracleDump {
    pub source: MmdDumperOracleSource,
    pub frames: Vec<MmdDumperOracleFrame>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GoldenIkBatchManifest {
    pub cases: Vec<GoldenIkBatchCase>,
}

impl GoldenIkBatchManifest {
    pub fn from_json_str(input: &str) -> Result<Self, MmdDumperOracleParseError> {
        serde_json::from_str(input)
            .map_err(|source| MmdDumperOracleParseError::ManifestJson { source })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GoldenIkBatchCase {
    pub name: String,
    pub pmx: String,
    pub vmd: String,
    pub frames: Vec<i32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct GoldenIkFixture {
    pub name: String,
    pub output: String,
    pub frames: Vec<i32>,
}

impl GoldenIkFixture {
    pub fn from_json_str(input: &str) -> Result<Self, MmdDumperOracleParseError> {
        serde_json::from_str(input)
            .map_err(|source| MmdDumperOracleParseError::ManifestJson { source })
    }
}

impl MmdDumperOracleDump {
    pub fn from_jsonl_str(
        input: &str,
        target_frames: Option<&[i32]>,
    ) -> Result<Self, MmdDumperOracleParseError> {
        let target_frames =
            target_frames.map(|frames| frames.iter().copied().collect::<BTreeSet<_>>());
        let mut source = None;
        let mut frames = Vec::new();

        for (line_index, line) in input.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let record: RawOracleRecord =
                serde_json::from_str(line).map_err(|source| MmdDumperOracleParseError::Json {
                    line: line_index + 1,
                    source,
                })?;
            if record.schema_version != SCHEMA_VERSION {
                return Err(MmdDumperOracleParseError::UnsupportedSchema {
                    schema_version: record.schema_version,
                });
            }

            source.get_or_insert_with(|| record.source.clone());
            let frame = record.frame.round() as i32;
            if target_frames
                .as_ref()
                .is_none_or(|target_frames| target_frames.contains(&frame))
            {
                frames.push(MmdDumperOracleFrame::from_raw(frame, record.models)?);
            }
        }

        Ok(Self {
            source: source.ok_or(MmdDumperOracleParseError::Empty)?,
            frames,
        })
    }

    pub fn find_frame(&self, frame: i32) -> Option<&MmdDumperOracleFrame> {
        self.frames
            .iter()
            .find(|candidate| candidate.frame == frame)
    }

    #[cfg(test)]
    pub fn from_unity_runtime_verification_json_str(
        input: &str,
        target_frames: Option<&[i32]>,
    ) -> Result<Self, MmdDumperOracleParseError> {
        Self::from_unity_runtime_verification_json_str_for_case(input, target_frames, None, None)
    }

    pub fn from_unity_runtime_verification_json_str_for_case(
        input: &str,
        target_frames: Option<&[i32]>,
        case_name: Option<&str>,
        pmx_path: Option<&str>,
    ) -> Result<Self, MmdDumperOracleParseError> {
        let target_frames =
            target_frames.map(|frames| frames.iter().copied().collect::<BTreeSet<_>>());
        let report: RawUnityRuntimeVerificationReport = serde_json::from_str(input)
            .map_err(|source| MmdDumperOracleParseError::Json { line: 1, source })?;
        if report.schema_version != SCHEMA_VERSION {
            return Err(MmdDumperOracleParseError::UnsupportedSchema {
                schema_version: report.schema_version,
            });
        }

        let mut frames = Vec::new();
        let mut matched_case = false;
        for case in report.case_results {
            if !unity_runtime_case_matches(&case, case_name, pmx_path) {
                continue;
            }
            matched_case = true;
            for frame in case.sampled_frames {
                let frame_number = frame.frame.round() as i32;
                if target_frames
                    .as_ref()
                    .is_some_and(|target_frames| !target_frames.contains(&frame_number))
                {
                    continue;
                }

                let mut bones = Vec::with_capacity(frame.bones.len());
                for (bone_index, bone) in frame.bones.into_iter().enumerate() {
                    let Ok(row_major) = bone.world_matrix.try_into() else {
                        return Err(MmdDumperOracleParseError::InvalidBoneMatrix {
                            frame: frame_number,
                            model: 0,
                            bone: bone_index,
                        });
                    };
                    bones.push(MmdDumperOracleBone {
                        index: bone.index,
                        name: bone.name,
                        world_matrix: unity_matrix_to_cols_array(row_major, &frame.matrix_layout),
                    });
                }

                frames.push(MmdDumperOracleFrame {
                    frame: frame_number,
                    models: vec![MmdDumperOracleModel {
                        index: 0,
                        name: case.name.clone(),
                        filename: case.pmx_path.clone(),
                        visible: true,
                        bones,
                        morphs: Vec::new(),
                    }],
                });
            }
        }

        if (case_name.is_some() || pmx_path.is_some()) && !matched_case {
            return Err(MmdDumperOracleParseError::UnityCaseNotFound {
                case_name: case_name.map(ToOwned::to_owned),
                pmx_path: pmx_path.map(ToOwned::to_owned),
            });
        }

        Ok(Self {
            source: MmdDumperOracleSource {
                mmd_version: report.unity_version,
                dumper_version: "unity-runtime-verification".to_owned(),
                project: Some("unity-mmd-loader".to_owned()),
            },
            frames,
        })
    }
}

fn unity_runtime_case_matches(
    case: &RawUnityRuntimeVerificationCase,
    case_name: Option<&str>,
    pmx_path: Option<&str>,
) -> bool {
    let case_name_matches = case_name.is_none_or(|case_name| case.name == case_name);
    let pmx_path_matches = pmx_path.is_none_or(|pmx_path| {
        normalize_runtime_path(&case.pmx_path) == normalize_runtime_path(pmx_path)
    });
    case_name_matches && pmx_path_matches
}

fn normalize_runtime_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MmdDumperOracleSource {
    #[serde(rename = "mmdVersion")]
    pub mmd_version: String,
    #[serde(rename = "dumperVersion")]
    pub dumper_version: String,
    #[serde(default)]
    pub project: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MmdDumperOracleFrame {
    pub frame: i32,
    pub models: Vec<MmdDumperOracleModel>,
}

impl MmdDumperOracleFrame {
    fn from_raw(
        frame: i32,
        models: Vec<RawOracleModel>,
    ) -> Result<Self, MmdDumperOracleParseError> {
        let mut parsed_models = Vec::with_capacity(models.len());
        for (model_index, model) in models.into_iter().enumerate() {
            parsed_models.push(MmdDumperOracleModel::from_raw(frame, model_index, model)?);
        }
        Ok(Self {
            frame,
            models: parsed_models,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MmdDumperOracleModel {
    pub index: i32,
    pub name: String,
    pub filename: String,
    pub visible: bool,
    pub bones: Vec<MmdDumperOracleBone>,
    pub morphs: Vec<MmdDumperOracleMorph>,
}

impl MmdDumperOracleModel {
    fn from_raw(
        frame: i32,
        model_index: usize,
        model: RawOracleModel,
    ) -> Result<Self, MmdDumperOracleParseError> {
        let mut bones = Vec::with_capacity(model.bones.len());
        for (bone_index, bone) in model.bones.into_iter().enumerate() {
            bones.push(MmdDumperOracleBone::from_raw(
                frame,
                model_index,
                bone_index,
                bone,
            )?);
        }
        Ok(Self {
            index: model.index,
            name: model.name,
            filename: model.filename,
            visible: model.visible,
            bones,
            morphs: model.morphs,
        })
    }

    pub fn find_bone(&self, name: &str) -> Option<&MmdDumperOracleBone> {
        self.bones.iter().find(|bone| bone.name == name)
    }

    pub fn focused_ik_bones<'a>(
        &'a self,
        focused_bone_names: &'a [&'a str],
    ) -> impl Iterator<Item = &'a MmdDumperOracleBone> + 'a {
        self.bones
            .iter()
            .filter(move |bone| focused_bone_names.contains(&bone.name.as_str()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MmdDumperOracleBone {
    pub index: i32,
    pub name: String,
    pub world_matrix: [f32; 16],
}

impl MmdDumperOracleBone {
    fn from_raw(
        frame: i32,
        model: usize,
        bone: usize,
        raw: RawOracleBone,
    ) -> Result<Self, MmdDumperOracleParseError> {
        let Ok(world_matrix) = raw.world_matrix.try_into() else {
            return Err(MmdDumperOracleParseError::InvalidBoneMatrix { frame, model, bone });
        };
        Ok(Self {
            index: raw.index,
            name: raw.name,
            world_matrix,
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MmdDumperOracleMorph {
    pub index: i32,
    pub name: String,
    pub weight: f32,
}

#[derive(Debug, Deserialize)]
struct RawOracleRecord {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    source: MmdDumperOracleSource,
    frame: f32,
    models: Vec<RawOracleModel>,
}

#[derive(Debug, Deserialize)]
struct RawOracleModel {
    index: i32,
    name: String,
    filename: String,
    visible: bool,
    bones: Vec<RawOracleBone>,
    #[serde(default)]
    morphs: Vec<MmdDumperOracleMorph>,
}

#[derive(Debug, Deserialize)]
struct RawOracleBone {
    index: i32,
    name: String,
    #[serde(rename = "worldMatrix")]
    world_matrix: Vec<f32>,
}

#[derive(Debug, Deserialize)]
struct RawUnityRuntimeVerificationReport {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "unityVersion")]
    unity_version: String,
    #[serde(rename = "caseResults")]
    case_results: Vec<RawUnityRuntimeVerificationCase>,
}

#[derive(Debug, Deserialize)]
struct RawUnityRuntimeVerificationCase {
    name: String,
    #[serde(rename = "pmxPath")]
    pmx_path: String,
    #[serde(rename = "sampledFrames")]
    sampled_frames: Vec<RawUnityRuntimeVerificationFrame>,
}

#[derive(Debug, Deserialize)]
struct RawUnityRuntimeVerificationFrame {
    frame: f32,
    #[serde(rename = "matrixLayout")]
    matrix_layout: Option<String>,
    bones: Vec<RawOracleBone>,
}

fn unity_matrix_to_cols_array(matrix: [f32; 16], matrix_layout: &Option<String>) -> [f32; 16] {
    if matrix_layout.as_deref() == Some("column-major") {
        return matrix;
    }

    [
        matrix[0], matrix[4], matrix[8], matrix[12], matrix[1], matrix[5], matrix[9], matrix[13],
        matrix[2], matrix[6], matrix[10], matrix[14], matrix[3], matrix[7], matrix[11], matrix[15],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── inline schema parse tests (always-on, no external asset required) ─────

    #[test]
    fn parses_minimal_inline_jsonl() {
        let jsonl = r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"1.0.0"},"frame":0.0,"models":[{"index":0,"name":"test_model","filename":"test.pmx","visible":true,"bones":[{"index":0,"name":"センター","worldMatrix":[1.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0, 0.0,0.0,1.0,0.0, 0.0,0.0,0.0,1.0]}],"morphs":[{"index":0,"name":"まばたき","weight":0.5}]}]}"#;

        let dump = MmdDumperOracleDump::from_jsonl_str(jsonl, None).unwrap();

        assert_eq!(dump.source.mmd_version, "9.32-x64");
        assert_eq!(dump.source.dumper_version, "1.0.0");
        assert_eq!(dump.frames.len(), 1);
        assert_eq!(dump.frames[0].frame, 0);
        assert_eq!(dump.frames[0].models.len(), 1);

        let model = &dump.frames[0].models[0];
        assert_eq!(model.index, 0);
        assert_eq!(model.name, "test_model");
        assert_eq!(model.filename, "test.pmx");
        assert!(model.visible);
        assert_eq!(model.bones.len(), 1);
        assert_eq!(model.bones[0].world_matrix[15], 1.0);
        assert_eq!(model.morphs.len(), 1);
        assert_eq!(model.morphs[0].weight, 0.5);
    }

    #[test]
    fn filters_target_frames_from_jsonl() {
        let jsonl = concat!(
            r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"1.0.0"},"frame":0.0,"models":[{"index":0,"name":"m","filename":"m.pmx","visible":true,"bones":[],"morphs":[]}]}"#,
            "\n",
            r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"1.0.0"},"frame":30.0,"models":[{"index":0,"name":"m","filename":"m.pmx","visible":true,"bones":[],"morphs":[]}]}"#,
        );

        let dump = MmdDumperOracleDump::from_jsonl_str(jsonl, Some(&[30])).unwrap();

        assert_eq!(dump.frames.len(), 1);
        assert_eq!(dump.frames[0].frame, 30);
        assert!(dump.find_frame(30).is_some());
        assert!(dump.find_frame(0).is_none());
    }

    #[test]
    fn parses_unity_runtime_verification_json_and_converts_row_major_matrices() {
        let json = r#"{
            "schemaVersion": 1,
            "unityVersion": "6000.4.8f1",
            "caseResults": [
                {
                    "name": "case-a",
                    "pmxPath": "model.pmx",
                    "sampledFrames": [
                        {
                            "frame": 60,
                            "matrixLayout": "row-major",
                            "bones": [
                                {
                                    "index": 3,
                                    "name": "スカート",
                                    "worldMatrix": [
                                        1.0, 0.0, 0.0, 7.0,
                                        0.0, 1.0, 0.0, 8.0,
                                        0.0, 0.0, 1.0, 9.0,
                                        0.0, 0.0, 0.0, 1.0
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let dump = MmdDumperOracleDump::from_unity_runtime_verification_json_str(json, Some(&[60]))
            .unwrap();

        assert_eq!(dump.source.dumper_version, "unity-runtime-verification");
        assert_eq!(dump.frames.len(), 1);
        let bone = &dump.frames[0].models[0].bones[0];
        assert_eq!(bone.index, 3);
        assert_eq!(bone.name, "スカート");
        assert_eq!(bone.world_matrix[12], 7.0);
        assert_eq!(bone.world_matrix[13], 8.0);
        assert_eq!(bone.world_matrix[14], 9.0);
    }

    #[test]
    fn filters_unity_runtime_verification_case_results_by_pmx_path() {
        let json = r#"{
            "schemaVersion": 1,
            "unityVersion": "6000.4.8f1",
            "caseResults": [
                {
                    "name": "case-a",
                    "pmxPath": "F:\\MMD\\model-a.pmx",
                    "sampledFrames": [
                        {
                            "frame": 60,
                            "matrixLayout": "row-major",
                            "bones": [
                                {
                                    "index": 1,
                                    "name": "A",
                                    "worldMatrix": [
                                        1.0, 0.0, 0.0, 1.0,
                                        0.0, 1.0, 0.0, 2.0,
                                        0.0, 0.0, 1.0, 3.0,
                                        0.0, 0.0, 0.0, 1.0
                                    ]
                                }
                            ]
                        }
                    ]
                },
                {
                    "name": "case-b",
                    "pmxPath": "f:/mmd/model-b.pmx",
                    "sampledFrames": [
                        {
                            "frame": 60,
                            "matrixLayout": "row-major",
                            "bones": [
                                {
                                    "index": 2,
                                    "name": "B",
                                    "worldMatrix": [
                                        1.0, 0.0, 0.0, 4.0,
                                        0.0, 1.0, 0.0, 5.0,
                                        0.0, 0.0, 1.0, 6.0,
                                        0.0, 0.0, 0.0, 1.0
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let dump = MmdDumperOracleDump::from_unity_runtime_verification_json_str_for_case(
            json,
            Some(&[60]),
            None,
            Some("F:\\MMD\\model-b.pmx"),
        )
        .unwrap();

        assert_eq!(dump.frames.len(), 1);
        let bone = &dump.frames[0].models[0].bones[0];
        assert_eq!(bone.index, 2);
        assert_eq!(bone.name, "B");
        assert_eq!(bone.world_matrix[12], 4.0);
        assert_eq!(bone.world_matrix[13], 5.0);
        assert_eq!(bone.world_matrix[14], 6.0);
    }

    #[test]
    fn filters_unity_runtime_verification_case_results_by_case_and_pmx_path() {
        let json = r#"{
            "schemaVersion": 1,
            "unityVersion": "6000.4.8f1",
            "caseResults": [
                {
                    "name": "case-a",
                    "pmxPath": "F:\\MMD\\shared.pmx",
                    "sampledFrames": [
                        {
                            "frame": 60,
                            "matrixLayout": "row-major",
                            "bones": [
                                {
                                    "index": 1,
                                    "name": "A",
                                    "worldMatrix": [
                                        1.0, 0.0, 0.0, 1.0,
                                        0.0, 1.0, 0.0, 2.0,
                                        0.0, 0.0, 1.0, 3.0,
                                        0.0, 0.0, 0.0, 1.0
                                    ]
                                }
                            ]
                        }
                    ]
                },
                {
                    "name": "case-b",
                    "pmxPath": "f:/mmd/shared.pmx",
                    "sampledFrames": [
                        {
                            "frame": 60,
                            "matrixLayout": "row-major",
                            "bones": [
                                {
                                    "index": 2,
                                    "name": "B",
                                    "worldMatrix": [
                                        1.0, 0.0, 0.0, 4.0,
                                        0.0, 1.0, 0.0, 5.0,
                                        0.0, 0.0, 1.0, 6.0,
                                        0.0, 0.0, 0.0, 1.0
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let dump = MmdDumperOracleDump::from_unity_runtime_verification_json_str_for_case(
            json,
            Some(&[60]),
            Some("case-b"),
            Some("F:\\MMD\\shared.pmx"),
        )
        .unwrap();

        assert_eq!(dump.frames.len(), 1);
        let bone = &dump.frames[0].models[0].bones[0];
        assert_eq!(bone.index, 2);
        assert_eq!(bone.name, "B");
        assert_eq!(bone.world_matrix[12], 4.0);
        assert_eq!(bone.world_matrix[13], 5.0);
        assert_eq!(bone.world_matrix[14], 6.0);
    }

    #[test]
    fn filters_unity_runtime_verification_case_results_by_case_name_without_pmx_path() {
        let json = r#"{
            "schemaVersion": 1,
            "unityVersion": "6000.4.8f1",
            "caseResults": [
                {
                    "name": "case-a",
                    "pmxPath": "F:\\UnityProject\\copied-a.pmx",
                    "sampledFrames": [
                        {
                            "frame": 60,
                            "matrixLayout": "row-major",
                            "bones": [
                                {
                                    "index": 1,
                                    "name": "A",
                                    "worldMatrix": [
                                        1.0, 0.0, 0.0, 1.0,
                                        0.0, 1.0, 0.0, 2.0,
                                        0.0, 0.0, 1.0, 3.0,
                                        0.0, 0.0, 0.0, 1.0
                                    ]
                                }
                            ]
                        }
                    ]
                },
                {
                    "name": "case-b",
                    "pmxPath": "F:\\UnityProject\\copied-b.pmx",
                    "sampledFrames": [
                        {
                            "frame": 60,
                            "matrixLayout": "row-major",
                            "bones": [
                                {
                                    "index": 2,
                                    "name": "B",
                                    "worldMatrix": [
                                        1.0, 0.0, 0.0, 4.0,
                                        0.0, 1.0, 0.0, 5.0,
                                        0.0, 0.0, 1.0, 6.0,
                                        0.0, 0.0, 0.0, 1.0
                                    ]
                                }
                            ]
                        }
                    ]
                }
            ]
        }"#;

        let dump = MmdDumperOracleDump::from_unity_runtime_verification_json_str_for_case(
            json,
            Some(&[60]),
            Some("case-b"),
            None,
        )
        .unwrap();

        assert_eq!(dump.frames.len(), 1);
        let bone = &dump.frames[0].models[0].bones[0];
        assert_eq!(bone.index, 2);
        assert_eq!(bone.name, "B");
        assert_eq!(bone.world_matrix[12], 4.0);
        assert_eq!(bone.world_matrix[13], 5.0);
        assert_eq!(bone.world_matrix[14], 6.0);
    }

    #[test]
    fn parses_manifest_with_inline_json() {
        let manifest_json = r#"{
            "schemaVersion": 1,
            "kind": "motion-numeric",
            "description": "test manifest for schema parse",
            "producer": {
                "tool": "MMDDumper",
                "runtime": "MikuMikuDance 9.32 x64",
                "command": "oracle-batch"
            },
            "defaults": {
                "outDir": "runs/test-motion-numeric",
                "timeoutMs": 180000,
                "dump": { "bones": true, "morphs": true, "rigidBodies": false },
                "focus": { "bones": ["センター", "右ひざ"], "morphs": ["まばたき"] },
                "comparison": {
                    "primary": "worldMatrix",
                    "secondary": ["localTransform", "morphWeight", "ikEnabled"],
                    "initialMode": "report-only"
                }
            },
            "cases": [
                {
                    "name": "test-case-a",
                    "kind": "motion-numeric",
                    "pmx": "model.pmx",
                    "vmd": "motion.vmd",
                    "frames": [0, 30],
                    "tags": ["inline"],
                    "notes": "extra manifest fields are intentionally ignored by this schema crate"
                },
                {
                    "name": "test-case-b",
                    "kind": "motion-numeric",
                    "pmx": "model_b.pmx",
                    "vmd": "motion_b.vmd",
                    "frames": [60]
                }
            ]
        }"#;

        let manifest = GoldenIkBatchManifest::from_json_str(manifest_json).unwrap();

        assert_eq!(manifest.cases.len(), 2);
        assert_eq!(manifest.cases[0].name, "test-case-a");
        assert_eq!(manifest.cases[0].pmx, "model.pmx");
        assert_eq!(manifest.cases[0].vmd, "motion.vmd");
        assert_eq!(manifest.cases[0].frames, vec![0, 30]);
        assert_eq!(manifest.cases[1].name, "test-case-b");
    }

    #[test]
    fn parses_fixture_with_inline_json() {
        let fixture_json = r#"{
            "name": "test-case",
            "output": "oracle.expected.jsonl",
            "frames": [0, 30, 60]
        }"#;

        let fixture = GoldenIkFixture::from_json_str(fixture_json).unwrap();

        assert_eq!(fixture.name, "test-case");
        assert_eq!(fixture.output, "oracle.expected.jsonl");
        assert_eq!(fixture.frames, vec![0, 30, 60]);
    }

    #[test]
    fn rejects_invalid_bone_matrix_length() {
        let jsonl = r#"{"schemaVersion":1,"source":{"mmdVersion":"9.32-x64","dumperVersion":"1.0.0"},"frame":0.0,"models":[{"index":0,"name":"m","filename":"m.pmx","visible":true,"bones":[{"index":0,"name":"bad","worldMatrix":[1.0,2.0,3.0]}],"morphs":[]}]}"#;

        let err = MmdDumperOracleDump::from_jsonl_str(jsonl, None).unwrap_err();

        assert!(
            matches!(err, MmdDumperOracleParseError::InvalidBoneMatrix { .. }),
            "expected InvalidBoneMatrix, got {:?}",
            err
        );
    }

    #[test]
    fn rejects_unsupported_schema_version() {
        let jsonl = r#"{"schemaVersion":99,"source":{"mmdVersion":"9.32-x64","dumperVersion":"1.0.0"},"frame":0.0,"models":[]}"#;

        let err = MmdDumperOracleDump::from_jsonl_str(jsonl, None).unwrap_err();

        assert!(
            matches!(
                err,
                MmdDumperOracleParseError::UnsupportedSchema { schema_version: 99 }
            ),
            "expected UnsupportedSchema(99), got {:?}",
            err
        );
    }

    #[test]
    fn exposes_three_mmd_loader_focused_ik_bone_contract() {
        assert!(DEFAULT_FOCUSED_IK_BONE_NAMES.contains(&"左ひざ"));
        assert!(DEFAULT_FOCUSED_IK_BONE_NAMES.contains(&"右足ＩＫ"));
        assert!(DEFAULT_FOCUSED_IK_BONE_NAMES.contains(&"左足IK"));
    }
}
