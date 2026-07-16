use std::{collections::BTreeMap, path::Path, process::ExitCode};

use mmd_anim_format::pmx::PmxParsedModel;

use crate::{parse_failure_error, read_file};

/// Number of skin weight/index slots per vertex in PMX BDEF4/QDEF/SDEF/BDEF2 layouts.
const SKIN_SLOTS_PER_VERTEX: usize = 4;

pub(crate) fn skin_check(
    model_path: &Path,
    use_json: bool,
    tolerance: f32,
    distance_threshold: f32,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err("skin-check --tolerance must be a non-negative finite number".into());
    }
    if !distance_threshold.is_finite() || distance_threshold < 0.0 {
        return Err("skin-check --distance-threshold must be a non-negative finite number".into());
    }

    let data = read_file(model_path)?;
    let parsed = mmd_anim_format::parse_pmx_model(&data).map_err(|error| {
        parse_failure_error(
            "skin-check",
            model_path,
            mmd_anim_format::MmdFormatKind::Pmx,
            error,
        )
    })?;

    let report = analyze_skin(&parsed, tolerance, distance_threshold);

    if use_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report.to_json(&parsed, tolerance, distance_threshold))?
        );
    } else {
        report.print_human(&parsed, tolerance, distance_threshold);
    }

    Ok(ExitCode::SUCCESS)
}

struct WeightSumAnomaly {
    vertex: usize,
    mode: String,
    sum: f32,
    indices: [u32; 4],
    weights: [f32; 4],
}

struct DuplicateBoneAnomaly {
    vertex: usize,
    mode: String,
    bone_index: u32,
    slots: Vec<(usize, f32)>,
}

struct NegativeIndexAnomaly {
    vertex: usize,
    mode: String,
    slot: usize,
    weight: f32,
    distance: f32,
    vertex_position: [f32; 3],
    bone0_position: [f32; 3],
}

struct SkinCheckReport {
    vertex_count: usize,
    weight_sum: Vec<WeightSumAnomaly>,
    duplicate_bone: Vec<DuplicateBoneAnomaly>,
    negative_index: Vec<NegativeIndexAnomaly>,
}

fn bone_name(parsed: &PmxParsedModel, index: u32) -> &str {
    parsed
        .skeleton
        .bones
        .get(index as usize)
        .map(|bone| bone.name.as_str())
        .unwrap_or("<out-of-range>")
}

fn analyze_skin(
    parsed: &PmxParsedModel,
    tolerance: f32,
    distance_threshold: f32,
) -> SkinCheckReport {
    let geometry = &parsed.geometry;
    let vertex_count = geometry.positions.len() / 3;
    let modes = &geometry.sdef.skinning_modes;
    let bone0_position = parsed.skeleton.bones.first().map(|bone| bone.position);

    let mut weight_sum = Vec::new();
    let mut duplicate_bone = Vec::new();
    let mut negative_index = Vec::new();

    for vertex in 0..vertex_count {
        let base = vertex * SKIN_SLOTS_PER_VERTEX;
        if base + SKIN_SLOTS_PER_VERTEX > geometry.skin_indices.len()
            || base + SKIN_SLOTS_PER_VERTEX > geometry.skin_weights.len()
        {
            continue;
        }
        let indices: [u32; 4] = geometry.skin_indices[base..base + 4].try_into().unwrap();
        let weights: [f32; 4] = geometry.skin_weights[base..base + 4].try_into().unwrap();
        let mode = modes
            .get(vertex)
            .cloned()
            .unwrap_or_else(|| "unknown".to_owned());

        let is_bdef4_or_qdef = mode == "bdef4" || mode == "qdef";

        if is_bdef4_or_qdef {
            let sum: f32 = weights.iter().sum();
            if (sum - 1.0).abs() > tolerance {
                weight_sum.push(WeightSumAnomaly {
                    vertex,
                    mode: mode.clone(),
                    sum,
                    indices,
                    weights,
                });
            }

            let mut grouped: BTreeMap<u32, Vec<(usize, f32)>> = BTreeMap::new();
            for (slot, (&bone_index, &weight)) in indices.iter().zip(weights.iter()).enumerate() {
                if weight > 0.0 {
                    grouped.entry(bone_index).or_default().push((slot, weight));
                }
            }
            for (bone_index, slots) in grouped {
                if slots.len() > 1 {
                    duplicate_bone.push(DuplicateBoneAnomaly {
                        vertex,
                        mode: mode.clone(),
                        bone_index,
                        slots,
                    });
                }
            }
        }

        if let Some(bone0_pos) = bone0_position {
            let vertex_position = [
                geometry.positions[vertex * 3],
                geometry.positions[vertex * 3 + 1],
                geometry.positions[vertex * 3 + 2],
            ];
            let dx = vertex_position[0] - bone0_pos[0];
            let dy = vertex_position[1] - bone0_pos[1];
            let dz = vertex_position[2] - bone0_pos[2];
            let distance = (dx * dx + dy * dy + dz * dz).sqrt();
            if distance > distance_threshold {
                // Slot 0 is always the primary slot; a zero bone index there is
                // ordinary (it may legitimately reference bone 0). Slots 1..4 are
                // "non-primary" and a zero index with positive weight there is
                // consistent with a -1 (unbound) bone index having been clamped to
                // 0 by normalize_nonnegative_index during parsing.
                for slot in 1..SKIN_SLOTS_PER_VERTEX {
                    if indices[slot] == 0 && weights[slot] > 0.0 {
                        negative_index.push(NegativeIndexAnomaly {
                            vertex,
                            mode: mode.clone(),
                            slot,
                            weight: weights[slot],
                            distance,
                            vertex_position,
                            bone0_position: bone0_pos,
                        });
                    }
                }
            }
        }
    }

    SkinCheckReport {
        vertex_count,
        weight_sum,
        duplicate_bone,
        negative_index,
    }
}

impl SkinCheckReport {
    fn print_human(&self, parsed: &PmxParsedModel, tolerance: f32, distance_threshold: f32) {
        println!(
            "skin-check: model={} vertices={} weightSumIssues={} duplicateBoneIndexIssues={} negativeIndexCandidates={} tolerance={:.6} distanceThreshold={:.6}",
            parsed.metadata.name,
            self.vertex_count,
            self.weight_sum.len(),
            self.duplicate_bone.len(),
            self.negative_index.len(),
            tolerance,
            distance_threshold,
        );

        if self.weight_sum.is_empty()
            && self.duplicate_bone.is_empty()
            && self.negative_index.is_empty()
        {
            println!("no skin anomalies found");
            return;
        }

        if !self.weight_sum.is_empty() {
            println!("\n[weight-sum] ({} vertices)", self.weight_sum.len());
            for anomaly in &self.weight_sum {
                let bone_names: Vec<&str> = anomaly
                    .indices
                    .iter()
                    .map(|&index| bone_name(parsed, index))
                    .collect();
                println!(
                    "  [weight-sum] vertex={} mode={} sum={:.6} indices={:?} boneNames={:?} weights=[{:.4},{:.4},{:.4},{:.4}]",
                    anomaly.vertex,
                    anomaly.mode,
                    anomaly.sum,
                    anomaly.indices,
                    bone_names,
                    anomaly.weights[0],
                    anomaly.weights[1],
                    anomaly.weights[2],
                    anomaly.weights[3],
                );
            }
        }

        if !self.duplicate_bone.is_empty() {
            println!(
                "\n[duplicate-bone] ({} vertices)",
                self.duplicate_bone.len()
            );
            for anomaly in &self.duplicate_bone {
                let slots_str = anomaly
                    .slots
                    .iter()
                    .map(|(slot, weight)| format!("{slot}:{weight:.4}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                println!(
                    "  [duplicate-bone] vertex={} mode={} boneIndex={} boneName={} slots=[{}]",
                    anomaly.vertex,
                    anomaly.mode,
                    anomaly.bone_index,
                    bone_name(parsed, anomaly.bone_index),
                    slots_str,
                );
            }
        }

        if !self.negative_index.is_empty() {
            println!(
                "\n[negative-index-candidate] ({} vertices)",
                self.negative_index.len()
            );
            for anomaly in &self.negative_index {
                println!(
                    "  [negative-index-candidate] vertex={} mode={} slot={} weight={:.4} distance={:.4} vertexPos=({:.4},{:.4},{:.4}) bone0Pos=({:.4},{:.4},{:.4}) bone0Name={}",
                    anomaly.vertex,
                    anomaly.mode,
                    anomaly.slot,
                    anomaly.weight,
                    anomaly.distance,
                    anomaly.vertex_position[0],
                    anomaly.vertex_position[1],
                    anomaly.vertex_position[2],
                    anomaly.bone0_position[0],
                    anomaly.bone0_position[1],
                    anomaly.bone0_position[2],
                    bone_name(parsed, 0),
                );
            }
        }
    }

    fn to_json(
        &self,
        parsed: &PmxParsedModel,
        tolerance: f32,
        distance_threshold: f32,
    ) -> serde_json::Value {
        let weight_sum: Vec<serde_json::Value> = self
            .weight_sum
            .iter()
            .map(|anomaly| {
                let bone_names: Vec<&str> = anomaly
                    .indices
                    .iter()
                    .map(|&index| bone_name(parsed, index))
                    .collect();
                serde_json::json!({
                    "vertex": anomaly.vertex,
                    "mode": anomaly.mode,
                    "sum": anomaly.sum,
                    "boneIndices": anomaly.indices,
                    "boneNames": bone_names,
                    "weights": anomaly.weights,
                })
            })
            .collect();

        let duplicate_bone: Vec<serde_json::Value> = self
            .duplicate_bone
            .iter()
            .map(|anomaly| {
                let slots: Vec<serde_json::Value> = anomaly
                    .slots
                    .iter()
                    .map(|(slot, weight)| serde_json::json!({ "slot": slot, "weight": weight }))
                    .collect();
                serde_json::json!({
                    "vertex": anomaly.vertex,
                    "mode": anomaly.mode,
                    "boneIndex": anomaly.bone_index,
                    "boneName": bone_name(parsed, anomaly.bone_index),
                    "slots": slots,
                })
            })
            .collect();

        let negative_index: Vec<serde_json::Value> = self
            .negative_index
            .iter()
            .map(|anomaly| {
                serde_json::json!({
                    "vertex": anomaly.vertex,
                    "mode": anomaly.mode,
                    "slot": anomaly.slot,
                    "weight": anomaly.weight,
                    "distance": anomaly.distance,
                    "vertexPosition": anomaly.vertex_position,
                    "bone0Position": anomaly.bone0_position,
                    "bone0Name": bone_name(parsed, 0),
                })
            })
            .collect();

        serde_json::json!({
            "modelName": parsed.metadata.name,
            "vertexCount": self.vertex_count,
            "tolerance": tolerance,
            "distanceThreshold": distance_threshold,
            "weightSumIssueCount": self.weight_sum.len(),
            "duplicateBoneIndexIssueCount": self.duplicate_bone.len(),
            "negativeIndexCandidateCount": self.negative_index.len(),
            "weightSumIssues": weight_sum,
            "duplicateBoneIndexIssues": duplicate_bone,
            "negativeIndexCandidates": negative_index,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skin_check_returns_error_for_missing_file() {
        let result = skin_check(Path::new("nonexistent.pmx"), false, 0.001, 1.0);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("nonexistent.pmx"),
            "error should contain path: {err}"
        );
    }

    #[test]
    fn skin_check_rejects_negative_tolerance() {
        let result = skin_check(Path::new("nonexistent.pmx"), false, -1.0, 1.0);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("tolerance"),
            "error should mention tolerance: {err}"
        );
    }

    #[test]
    fn skin_check_rejects_negative_distance_threshold() {
        let result = skin_check(Path::new("nonexistent.pmx"), false, 0.001, -1.0);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("distance-threshold"),
            "error should mention distance-threshold: {err}"
        );
    }
}
