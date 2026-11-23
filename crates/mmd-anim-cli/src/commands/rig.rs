use std::{collections::BTreeMap, fs, path::Path, process::ExitCode};

use mmd_anim_format::pmx::{PmxParsedAppendTransform, PmxParsedBone, PmxParsedIk, PmxParsedModel};

/// Rig facts shared by the JSON and human-readable `rig-inspect` reports.
struct RigInspection<'a> {
    parsed: &'a PmxParsedModel,
    ik_chains: Vec<(usize, &'a PmxParsedBone, &'a PmxParsedIk)>,
    grants: Vec<(usize, &'a PmxParsedBone, &'a PmxParsedAppendTransform)>,
    layer_distribution: BTreeMap<i32, usize>,
    show_bones: bool,
}

pub(crate) fn rig_inspect(
    pmx_path: &Path,
    use_json: bool,
    show_bones: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data =
        fs::read(pmx_path).map_err(|e| format!("failed to read {}: {}", pmx_path.display(), e))?;
    let parsed = mmd_anim_format::parse_pmx_model(&data).map_err(|e| {
        format!(
            "rig-inspect requires a PMX model file: {}: {}",
            pmx_path.display(),
            e
        )
    })?;
    let bones = &parsed.skeleton.bones;

    let ik_chains = bones
        .iter()
        .enumerate()
        .filter_map(|(index, bone)| Some((index, bone, bone.ik.as_ref()?)))
        .collect();
    let grants = bones
        .iter()
        .enumerate()
        .filter_map(|(index, bone)| Some((index, bone, bone.append_transform.as_ref()?)))
        .collect();

    let mut layer_distribution = BTreeMap::<i32, usize>::new();
    for bone in bones {
        *layer_distribution.entry(bone.layer).or_default() += 1;
    }

    let inspection = RigInspection {
        parsed: &parsed,
        ik_chains,
        grants,
        layer_distribution,
        show_bones,
    };
    if use_json {
        print_json(&inspection)?;
    } else {
        print_human(&inspection);
    }

    Ok(ExitCode::SUCCESS)
}

fn bone_name(bones: &[PmxParsedBone], index: i32) -> &str {
    bones
        .get(index as usize)
        .map(|bone| bone.name.as_str())
        .unwrap_or("<unknown>")
}

fn print_json(inspection: &RigInspection<'_>) -> Result<(), Box<dyn std::error::Error>> {
    let RigInspection {
        parsed,
        ik_chains,
        grants,
        layer_distribution,
        show_bones,
    } = inspection;
    let bones = &parsed.skeleton.bones;
    let ik_chain_values: Vec<serde_json::Value> = ik_chains
        .iter()
        .map(|(idx, bone, ik)| {
            let links: Vec<serde_json::Value> = ik
                .links
                .iter()
                .map(|link| {
                    let link_name = bone_name(bones, link.bone_index);
                    let mut obj = serde_json::json!({
                        "boneIndex": link.bone_index,
                        "boneName": link_name,
                    });
                    if let Some(limits) = &link.limits {
                        obj["limits"] = serde_json::json!({
                            "lower": limits.lower,
                            "upper": limits.upper,
                        });
                    }
                    obj
                })
                .collect();
            let target_name = bone_name(bones, ik.target_index);
            serde_json::json!({
                "boneIndex": idx,
                "boneName": bone.name,
                "targetIndex": ik.target_index,
                "targetName": target_name,
                "loopCount": ik.loop_count,
                "limitAngle": ik.limit_angle,
                "linkCount": ik.links.len(),
                "links": links,
            })
        })
        .collect();

    let grant_values: Vec<serde_json::Value> = grants
        .iter()
        .map(|(idx, bone, append)| {
            let source_name = bone_name(bones, append.parent_index);
            serde_json::json!({
                "boneIndex": idx,
                "boneName": bone.name,
                "sourceIndex": append.parent_index,
                "sourceName": source_name,
                "weight": append.weight,
                "affectRotation": bone.flags.append_rotate,
                "affectTranslation": bone.flags.append_translate,
                "local": bone.flags.append_local,
            })
        })
        .collect();

    let layers: serde_json::Value = layer_distribution
        .iter()
        .map(|(layer, count)| (layer.to_string(), serde_json::json!(count)))
        .collect::<serde_json::Map<String, serde_json::Value>>()
        .into();

    let mut root = serde_json::json!({
        "modelName": parsed.metadata.name,
        "boneCount": bones.len(),
        "ikChainCount": ik_chains.len(),
        "grantCount": grants.len(),
        "ikChains": ik_chain_values,
        "grants": grant_values,
        "deformLayers": layers,
    });

    if *show_bones {
        let bone_list: Vec<serde_json::Value> = bones
            .iter()
            .enumerate()
            .map(|(idx, bone)| {
                serde_json::json!({
                    "index": idx,
                    "name": bone.name,
                    "englishName": bone.english_name,
                    "parentIndex": bone.parent_index,
                    "layer": bone.layer,
                    "position": bone.position,
                    "flags": {
                        "rotatable": bone.flags.rotatable,
                        "translatable": bone.flags.translatable,
                        "visible": bone.flags.visible,
                        "enabled": bone.flags.enabled,
                        "ik": bone.flags.ik,
                        "appendRotate": bone.flags.append_rotate,
                        "appendTranslate": bone.flags.append_translate,
                        "fixedAxis": bone.flags.fixed_axis,
                        "localAxis": bone.flags.local_axis,
                        "transformAfterPhysics": bone.flags.transform_after_physics,
                    },
                })
            })
            .collect();
        root["bones"] = serde_json::json!(bone_list);
    }

    println!("{}", serde_json::to_string_pretty(&root)?);
    Ok(())
}

fn print_human(inspection: &RigInspection<'_>) {
    let RigInspection {
        parsed,
        ik_chains,
        grants,
        layer_distribution,
        show_bones,
    } = inspection;
    let bones = &parsed.skeleton.bones;
    let bone_count = bones.len();
    println!(
        "rig-inspect: model={} bones={} ikChains={} grantCount={}",
        parsed.metadata.name,
        bone_count,
        ik_chains.len(),
        grants.len(),
    );

    let layer_str = layer_distribution
        .iter()
        .map(|(layer, count)| format!("{layer}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    println!("deform layers: {layer_str}");

    if !ik_chains.is_empty() {
        println!("\nIK chains ({}):", ik_chains.len());
        for (idx, bone, ik) in ik_chains {
            let target_name = bone_name(bones, ik.target_index);
            println!(
                "  [{idx}] {} -> target={} links={} loopCount={} limitAngle={}",
                bone.name,
                target_name,
                ik.links.len(),
                ik.loop_count,
                ik.limit_angle,
            );
        }
    }

    if !grants.is_empty() {
        println!("\ngrants ({}):", grants.len());
        for (idx, bone, append) in grants {
            let source_name = bone_name(bones, append.parent_index);
            let affects_str = match (bone.flags.append_rotate, bone.flags.append_translate) {
                (true, true) => "rotation+translation",
                (true, false) => "rotation",
                (false, true) => "translation",
                (false, false) => "none",
            };
            println!(
                "  [{idx}] {} <- source={} weight={:.4} affects={}{}",
                bone.name,
                source_name,
                append.weight,
                affects_str,
                if bone.flags.append_local {
                    " (local)"
                } else {
                    ""
                },
            );
        }
    }

    if *show_bones {
        println!("\nbones ({bone_count}):");
        for (idx, bone) in bones.iter().enumerate() {
            let mut flags = Vec::new();
            if bone.flags.ik {
                flags.push("IK");
            }
            if bone.flags.append_rotate || bone.flags.append_translate {
                flags.push("grant");
            }
            if bone.flags.fixed_axis {
                flags.push("fixedAxis");
            }
            if bone.flags.transform_after_physics {
                flags.push("afterPhysics");
            }
            if !bone.flags.visible {
                flags.push("hidden");
            }
            let flags_str = if flags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", flags.join(","))
            };
            println!(
                "  [{idx}] {} parent={} layer={} pos=({:.4},{:.4},{:.4}){}",
                bone.name,
                bone.parent_index.max(-1),
                bone.layer,
                bone.position[0],
                bone.position[1],
                bone.position[2],
                flags_str,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rig_inspect_returns_error_for_missing_file() {
        let result = rig_inspect(Path::new("nonexistent.pmx"), false, false);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("nonexistent.pmx"),
            "error should contain path: {err}"
        );
    }
}
