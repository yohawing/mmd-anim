use serde::{Deserialize, Serialize};

use crate::error::ImportError;
use crate::sjis::{decode_sjis, encode_sjis};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VpdParsedPose {
    #[serde(default = "default_vpd_format", skip_deserializing)]
    pub format: &'static str,
    pub model_file: String,
    pub bone_count: usize,
    pub bones: Vec<VpdBonePose>,
    #[serde(default, skip_deserializing)]
    pub diagnostics: Vec<VpdDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VpdBonePose {
    pub name: String,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Debug, Clone, Serialize)]
pub struct VpdDiagnostic {
    pub level: &'static str,
    pub code: &'static str,
    pub message: String,
}

fn default_vpd_format() -> &'static str {
    "vpd"
}

pub fn parse_vpd_pose(data: &[u8]) -> Result<VpdParsedPose, ImportError> {
    let text = decode_sjis(data);
    if !text.starts_with("Vocaloid Pose Data file") {
        return Err(ImportError::InvalidMagic { format: "VPD" });
    }
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let _header = lines.next();
    let model_file = strip_comment(lines.next().ok_or(ImportError::UnsupportedFormat {
        format: "VPD",
        detail: "missing model file",
    })?)
    .trim_end_matches(';')
    .trim()
    .to_owned();
    let count_line = lines.next().ok_or(ImportError::UnsupportedFormat {
        format: "VPD",
        detail: "missing bone count",
    })?;
    let declared_count = strip_comment(count_line)
        .trim_end_matches(';')
        .parse::<usize>()
        .map_err(|_| ImportError::UnsupportedFormat {
            format: "VPD",
            detail: "invalid bone count",
        })?;
    let mut bones = Vec::new();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("Bone") {
            let Some(name) = rest
                .split_once('{')
                .map(|(_, name)| name)
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
            else {
                return Err(ImportError::UnsupportedFormat {
                    format: "VPD",
                    detail: "invalid bone name",
                });
            };
            let translation =
                parse_f32_tuple3(lines.next().ok_or(ImportError::UnsupportedFormat {
                    format: "VPD",
                    detail: "missing bone translation",
                })?)
                .ok_or(ImportError::UnsupportedFormat {
                    format: "VPD",
                    detail: "invalid bone translation",
                })?;
            let rotation =
                lines
                    .next()
                    .and_then(parse_f32_tuple4)
                    .ok_or(ImportError::UnsupportedFormat {
                        format: "VPD",
                        detail: "invalid bone rotation",
                    })?;
            let closing = lines.next().ok_or(ImportError::UnsupportedFormat {
                format: "VPD",
                detail: "missing bone terminator",
            })?;
            if strip_comment(closing) != "}" {
                return Err(ImportError::UnsupportedFormat {
                    format: "VPD",
                    detail: "invalid bone terminator",
                });
            }
            bones.push(VpdBonePose {
                name,
                translation,
                rotation,
            });
        }
    }
    let parsed_bone_count = bones.len();
    Ok(VpdParsedPose {
        format: "vpd",
        model_file,
        bone_count: parsed_bone_count,
        bones,
        diagnostics: if declared_count != parsed_bone_count {
            vec![VpdDiagnostic {
                level: "warning",
                code: "VPD_DECLARED_COUNT_MISMATCH",
                message: format!(
                    "declared bone count {declared_count}, parsed {}",
                    parsed_bone_count
                ),
            }]
        } else {
            Vec::new()
        },
    })
}

pub fn export_vpd_pose(pose: &VpdParsedPose) -> Vec<u8> {
    let mut text = String::new();
    text.push_str("Vocaloid Pose Data file\r\n\r\n");
    text.push_str(&format!("{};\t\t// parent file name\r\n", pose.model_file));
    text.push_str(&format!("{};\t\t// bone count\r\n\r\n", pose.bones.len()));
    for (index, bone) in pose.bones.iter().enumerate() {
        text.push_str(&format!("Bone{index}{{{}\r\n", bone.name));
        text.push_str(&format!(
            "  {:.6},{:.6},{:.6};\t\t// trans x,y,z\r\n",
            bone.translation[0], bone.translation[1], bone.translation[2]
        ));
        text.push_str(&format!(
            "  {:.6},{:.6},{:.6},{:.6};\t\t// Quaternion x,y,z,w\r\n",
            bone.rotation[0], bone.rotation[1], bone.rotation[2], bone.rotation[3]
        ));
        text.push_str("}\r\n\r\n");
    }
    encode_sjis(&text)
}

fn parse_f32_tuple3(line: &str) -> Option<[f32; 3]> {
    parse_numbers(line).and_then(|values| values.try_into().ok())
}

fn parse_f32_tuple4(line: &str) -> Option<[f32; 4]> {
    parse_numbers(line).and_then(|values| values.try_into().ok())
}

fn parse_numbers(line: &str) -> Option<Vec<f32>> {
    strip_comment(line)
        .trim_matches(|c: char| c == ';' || c == '{' || c == '}')
        .split(',')
        .map(|part| {
            part.trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
        })
        .collect()
}

fn strip_comment(line: &str) -> &str {
    line.split_once("//")
        .map(|(value, _)| value)
        .unwrap_or(line)
        .trim()
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn exports_parsed_vpd_pose_for_roundtrip() {
        let source = "Vocaloid Pose Data file\r\n\r\nmiku.osm;\r\n1;\r\n\r\nBone0{左親指１\r\n  1.000000,2.000000,3.000000;\r\n  0.100000,0.200000,0.300000,0.400000;\r\n}\r\n";
        let encoded = encode_sjis(source);
        let parsed = parse_vpd_pose(&encoded).unwrap();
        let exported = export_vpd_pose(&parsed);
        let reparsed = parse_vpd_pose(&exported).unwrap();

        assert_eq!(parsed.model_file, reparsed.model_file);
        assert_eq!(parsed.bone_count, reparsed.bone_count);
        assert_eq!(parsed.bones[0].name, reparsed.bones[0].name);
        assert_eq!(parsed.bones[0].translation, reparsed.bones[0].translation);
        assert_eq!(parsed.bones[0].rotation, reparsed.bones[0].rotation);
    }

    #[test]
    fn vpd_pose_json_top_level_schema_is_stable() {
        let source = "Vocaloid Pose Data file\r\n\r\nmiku.osm;\r\n0;\r\n";
        let encoded = encode_sjis(source);
        let parsed = parse_vpd_pose(&encoded).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed).unwrap());

        assert_eq!(
            keys,
            vec!["boneCount", "bones", "diagnostics", "format", "modelFile"]
        );
    }

    #[test]
    fn rejects_truncated_or_malformed_bone_records() {
        for source in [
            "Vocaloid Pose Data file\r\n",
            "Vocaloid Pose Data file\r\n\r\nmodel.pmx;\r\n",
            "Vocaloid Pose Data file\r\n\r\nmodel.pmx;\r\n1;\r\nBone0{左腕\r\n",
            "Vocaloid Pose Data file\r\n\r\nmodel.pmx;\r\n1;\r\nBone0{左腕\r\n0,0,0;\r\n0,0,0,1;\r\n",
            "Vocaloid Pose Data file\r\n\r\nmodel.pmx;\r\n1;\r\nBone0{左腕\r\ninvalid;\r\n0,0,0,1;\r\n}\r\n",
            "Vocaloid Pose Data file\r\n\r\nmodel.pmx;\r\n1;\r\nBone0{左腕\r\nNaN,0,0;\r\n0,0,0,1;\r\n}\r\n",
        ] {
            assert!(parse_vpd_pose(&encode_sjis(source)).is_err(), "{source:?}");
        }
    }
}
