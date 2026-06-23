use std::{collections::BTreeMap, fs, path::Path, process::ExitCode};

pub(crate) fn rig_inspect(
    pmx_path: &Path,
    use_json: bool,
    show_bones: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let data = fs::read(pmx_path)
        .map_err(|e| format!("failed to read {}: {}", pmx_path.display(), e))?;
    let parsed = mmd_anim_format::parse_pmx_model(&data)?;
    let bones = &parsed.skeleton.bones;
    let bone_count = bones.len();

    let ik_bones: Vec<(usize, &mmd_anim_format::pmx::PmxParsedBone)> = bones
        .iter()
        .enumerate()
        .filter(|(_, b)| b.ik.is_some())
        .collect();

    let grant_bones: Vec<(usize, &mmd_anim_format::pmx::PmxParsedBone)> = bones
        .iter()
        .enumerate()
        .filter(|(_, b)| b.append_transform.is_some())
        .collect();

    let mut layer_distribution = BTreeMap::<i32, usize>::new();
    for bone in bones {
        *layer_distribution.entry(bone.layer).or_default() += 1;
    }

    if use_json {
        print_json(
            &parsed,
            bones,
            &ik_bones,
            &grant_bones,
            &layer_distribution,
            show_bones,
        )?;
    } else {
        print_human(
            &parsed,
            bones,
            bone_count,
            &ik_bones,
            &grant_bones,
            &layer_distribution,
            show_bones,
        );
    }

    Ok(ExitCode::SUCCESS)
}

fn print_json(
    parsed: &mmd_anim_format::pmx::PmxParsedModel,
    bones: &[mmd_anim_format::pmx::PmxParsedBone],
    ik_bones: &[(usize, &mmd_anim_format::pmx::PmxParsedBone)],
    grant_bones: &[(usize, &mmd_anim_format::pmx::PmxParsedBone)],
    layer_distribution: &BTreeMap<i32, usize>,
    show_bones: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let ik_chains: Vec<serde_json::Value> = ik_bones
        .iter()
        .map(|(idx, bone)| {
            let ik = bone.ik.as_ref().unwrap();
            let links: Vec<serde_json::Value> = ik
                .links
                .iter()
                .map(|link| {
                    let link_name = bones
                        .get(link.bone_index as usize)
                        .map(|b| b.name.as_str())
                        .unwrap_or("<unknown>");
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
            let target_name = bones
                .get(ik.target_index as usize)
                .map(|b| b.name.as_str())
                .unwrap_or("<unknown>");
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

    let grants: Vec<serde_json::Value> = grant_bones
        .iter()
        .map(|(idx, bone)| {
            let append = bone.append_transform.as_ref().unwrap();
            let source_name = bones
                .get(append.parent_index as usize)
                .map(|b| b.name.as_str())
                .unwrap_or("<unknown>");
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
        "ikChainCount": ik_bones.len(),
        "grantCount": grant_bones.len(),
        "ikChains": ik_chains,
        "grants": grants,
        "deformLayers": layers,
    });

    if show_bones {
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

fn print_human(
    parsed: &mmd_anim_format::pmx::PmxParsedModel,
    bones: &[mmd_anim_format::pmx::PmxParsedBone],
    bone_count: usize,
    ik_bones: &[(usize, &mmd_anim_format::pmx::PmxParsedBone)],
    grant_bones: &[(usize, &mmd_anim_format::pmx::PmxParsedBone)],
    layer_distribution: &BTreeMap<i32, usize>,
    show_bones: bool,
) {
    println!(
        "rig-inspect: model={} bones={} ikChains={} grants={}",
        parsed.metadata.name,
        bone_count,
        ik_bones.len(),
        grant_bones.len(),
    );

    let layer_str = layer_distribution
        .iter()
        .map(|(layer, count)| format!("{layer}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    println!("deform layers: {layer_str}");

    if !ik_bones.is_empty() {
        println!("\nIK chains ({}):", ik_bones.len());
        for (idx, bone) in ik_bones {
            let ik = bone.ik.as_ref().unwrap();
            let target_name = bones
                .get(ik.target_index as usize)
                .map(|b| b.name.as_str())
                .unwrap_or("<unknown>");
            println!(
                "  [{idx}] {} -> target={} links={} iterations={} limitAngle={:.6}",
                bone.name,
                target_name,
                ik.links.len(),
                ik.loop_count,
                ik.limit_angle,
            );
        }
    }

    if !grant_bones.is_empty() {
        println!("\ngrants ({}):", grant_bones.len());
        for (idx, bone) in grant_bones {
            let append = bone.append_transform.as_ref().unwrap();
            let source_name = bones
                .get(append.parent_index as usize)
                .map(|b| b.name.as_str())
                .unwrap_or("<unknown>");
            let mut affects = Vec::new();
            if bone.flags.append_rotate {
                affects.push("rotation");
            }
            if bone.flags.append_translate {
                affects.push("translation");
            }
            let affects_str = if affects.is_empty() {
                "none".to_owned()
            } else {
                affects.join("+")
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

    if show_bones {
        println!("\nbones ({bone_count}):");
        for (idx, bone) in bones.iter().enumerate() {
            let parent_str = if bone.parent_index < 0 {
                "root".to_owned()
            } else {
                format!("{}", bone.parent_index)
            };
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
                parent_str,
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
        assert!(err.contains("nonexistent.pmx"), "error should contain path: {err}");
    }
}
