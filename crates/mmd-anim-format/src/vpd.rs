use encoding_rs::SHIFT_JIS;
use serde::{Deserialize, Serialize};

use crate::error::ImportError;

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
    let (decoded, _, _) = SHIFT_JIS.decode(data);
    let text = decoded.into_owned();
    if !text.starts_with("Vocaloid Pose Data file") {
        return Err(ImportError::InvalidMagic { format: "VPD" });
    }
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let _header = lines.next();
    let model_file = strip_comment(lines.next().unwrap_or(""))
        .trim_end_matches(';')
        .trim()
        .to_owned();
    let count_line = lines.next().unwrap_or("0;");
    let declared_count = count_line
        .trim_end_matches(';')
        .parse::<usize>()
        .unwrap_or(0);
    let mut bones = Vec::new();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("Bone") {
            let Some(name) = rest
                .split_once('{')
                .map(|(_, name)| name)
                .map(|name| name.trim().to_owned())
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            let translation = lines.next().map(parse_f32_tuple3).unwrap_or([0.0; 3]);
            let rotation = lines
                .next()
                .map(parse_f32_tuple4)
                .unwrap_or([0.0, 0.0, 0.0, 1.0]);
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
        diagnostics: if declared_count != 0 && declared_count != parsed_bone_count {
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
    let (encoded, _, _) = SHIFT_JIS.encode(&text);
    encoded.into_owned()
}

fn parse_f32_tuple3(line: &str) -> [f32; 3] {
    let values = parse_numbers(line);
    [
        values.first().copied().unwrap_or(0.0),
        values.get(1).copied().unwrap_or(0.0),
        values.get(2).copied().unwrap_or(0.0),
    ]
}

fn parse_f32_tuple4(line: &str) -> [f32; 4] {
    let values = parse_numbers(line);
    [
        values.first().copied().unwrap_or(0.0),
        values.get(1).copied().unwrap_or(0.0),
        values.get(2).copied().unwrap_or(0.0),
        values.get(3).copied().unwrap_or(1.0),
    ]
}

fn parse_numbers(line: &str) -> Vec<f32> {
    strip_comment(line)
        .trim_matches(|c: char| c == ';' || c == '{' || c == '}')
        .split(',')
        .filter_map(|part| part.trim().parse::<f32>().ok())
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
        let (encoded, _, _) = SHIFT_JIS.encode(source);
        let parsed = parse_vpd_pose(encoded.as_ref()).unwrap();
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
        let (encoded, _, _) = SHIFT_JIS.encode(source);
        let parsed = parse_vpd_pose(encoded.as_ref()).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed).unwrap());

        assert_eq!(
            keys,
            vec!["boneCount", "bones", "diagnostics", "format", "modelFile"]
        );
    }
}
