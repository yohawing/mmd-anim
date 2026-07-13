use std::{path::Path, process::ExitCode};

use mmd_anim_format::fbx::{
    FbxSkinBoneDiff, FbxSkinDiffOptions, FbxSkinDiffReport, diff_fbx_skin_clusters,
    read_fbx_skin_clusters,
};
use serde_json::json;

use crate::read_file;

pub(crate) struct DiffFbxSkinOptions {
    pub weight_epsilon: f64,
    pub matrix_epsilon: f64,
    pub summary_only: bool,
    pub use_json: bool,
}

impl Default for DiffFbxSkinOptions {
    fn default() -> Self {
        let defaults = FbxSkinDiffOptions::default();
        Self {
            weight_epsilon: defaults.weight_epsilon,
            matrix_epsilon: defaults.matrix_epsilon,
            summary_only: false,
            use_json: false,
        }
    }
}

/// Compares the skin cluster (per-bone deformer) data between two FBX files
/// and reports differences. Bones are matched by name, not FBX object id, so
/// this also works when comparing our export against a Maya import/export
/// roundtrip of that same file (Maya reassigns object ids on export).
pub(crate) fn diff_fbx_skin(
    path_a: &Path,
    path_b: &Path,
    options: DiffFbxSkinOptions,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let bytes_a = read_file(path_a)?;
    let bytes_b = read_file(path_b)?;
    let clusters_a = read_fbx_skin_clusters(&bytes_a).map_err(|error| {
        format!(
            "failed to read FBX skin clusters from {}: {error}",
            path_a.display()
        )
    })?;
    let clusters_b = read_fbx_skin_clusters(&bytes_b).map_err(|error| {
        format!(
            "failed to read FBX skin clusters from {}: {error}",
            path_b.display()
        )
    })?;

    let diff_options = FbxSkinDiffOptions {
        weight_epsilon: options.weight_epsilon,
        matrix_epsilon: options.matrix_epsilon,
    };
    let report = diff_fbx_skin_clusters(&clusters_a, &clusters_b, diff_options);
    let difference_count = report.difference_count();

    if options.use_json {
        let value = diff_report_json(
            path_a,
            path_b,
            clusters_a.len(),
            clusters_b.len(),
            &options,
            &report,
        );
        println!("{}", serde_json::to_string(&value)?);
    } else {
        print_diff_report_text(path_a, path_b, clusters_a.len(), clusters_b.len(), &options, &report);
    }

    if difference_count == 0 {
        Ok(ExitCode::SUCCESS)
    } else {
        Err(format!(
            "fbx-skin-diff: {difference_count} bone(s) differ between {} and {}",
            path_a.display(),
            path_b.display()
        )
        .into())
    }
}

fn print_diff_report_text(
    path_a: &Path,
    path_b: &Path,
    cluster_count_a: usize,
    cluster_count_b: usize,
    options: &DiffFbxSkinOptions,
    report: &FbxSkinDiffReport,
) {
    println!(
        "fbx-skin-diff: a={} b={} clustersA={} clustersB={} weightEpsilon={} matrixEpsilon={} diffBones={}",
        path_a.display(),
        path_b.display(),
        cluster_count_a,
        cluster_count_b,
        options.weight_epsilon,
        options.matrix_epsilon,
        report.difference_count(),
    );

    for bone in &report.bones {
        let has_diff = bone.has_differences();
        if options.summary_only && !has_diff {
            continue;
        }

        if !bone.in_a {
            println!(
                "bone={} status=only-in-b vertsB={}",
                bone.bone_name, bone.vertex_count_b
            );
            continue;
        }
        if !bone.in_b {
            println!(
                "bone={} status=only-in-a vertsA={}",
                bone.bone_name, bone.vertex_count_a
            );
            continue;
        }

        println!(
            "bone={} status={} vertsA={} vertsB={} vertDelta={} added={} removed={} changedWeights={} transformDiffers={} transformLinkDiffers={} transformMaxAbsDelta={} transformLinkMaxAbsDelta={}",
            bone.bone_name,
            if has_diff { "diff" } else { "ok" },
            bone.vertex_count_a,
            bone.vertex_count_b,
            bone.vertex_count_b as i64 - bone.vertex_count_a as i64,
            bone.added_vertices.len(),
            bone.removed_vertices.len(),
            bone.changed_weights.len(),
            bone.transform_differs,
            bone.transform_link_differs,
            format_optional_f64(bone.transform_max_abs_delta),
            format_optional_f64(bone.transform_link_max_abs_delta),
        );

        if options.summary_only {
            continue;
        }
        for vertex in &bone.added_vertices {
            println!(
                "  vertex-diff bone={} vertex={} kind=added weightB={:.6}",
                bone.bone_name, vertex.vertex_index, vertex.weight
            );
        }
        for vertex in &bone.removed_vertices {
            println!(
                "  vertex-diff bone={} vertex={} kind=removed weightA={:.6}",
                bone.bone_name, vertex.vertex_index, vertex.weight
            );
        }
        for weight in &bone.changed_weights {
            println!(
                "  vertex-diff bone={} vertex={} kind=weight-changed weightA={:.6} weightB={:.6} delta={:.6}",
                bone.bone_name,
                weight.vertex_index,
                weight.weight_a,
                weight.weight_b,
                weight.delta()
            );
        }
    }
}

fn format_optional_f64(value: Option<f64>) -> String {
    match value {
        Some(value) => format!("{value:.9}"),
        None => "-".to_owned(),
    }
}

fn diff_report_json(
    path_a: &Path,
    path_b: &Path,
    cluster_count_a: usize,
    cluster_count_b: usize,
    options: &DiffFbxSkinOptions,
    report: &FbxSkinDiffReport,
) -> serde_json::Value {
    let bones = report
        .bones
        .iter()
        .map(bone_diff_json)
        .collect::<Vec<_>>();
    json!({
        "status": if report.difference_count() == 0 { "ok" } else { "diff" },
        "command": "fbx-skin-diff",
        "a": path_a.display().to_string(),
        "b": path_b.display().to_string(),
        "clustersA": cluster_count_a,
        "clustersB": cluster_count_b,
        "weightEpsilon": options.weight_epsilon,
        "matrixEpsilon": options.matrix_epsilon,
        "diffBoneCount": report.difference_count(),
        "bones": bones,
    })
}

fn bone_diff_json(bone: &FbxSkinBoneDiff) -> serde_json::Value {
    json!({
        "boneName": bone.bone_name,
        "inA": bone.in_a,
        "inB": bone.in_b,
        "vertexCountA": bone.vertex_count_a,
        "vertexCountB": bone.vertex_count_b,
        "vertexCountDelta": bone.vertex_count_b as i64 - bone.vertex_count_a as i64,
        "addedVertices": bone.added_vertices.iter().map(|vertex| json!({
            "vertex": vertex.vertex_index,
            "weightB": vertex.weight,
        })).collect::<Vec<_>>(),
        "removedVertices": bone.removed_vertices.iter().map(|vertex| json!({
            "vertex": vertex.vertex_index,
            "weightA": vertex.weight,
        })).collect::<Vec<_>>(),
        "changedWeights": bone.changed_weights.iter().map(|weight| json!({
            "vertex": weight.vertex_index,
            "weightA": weight.weight_a,
            "weightB": weight.weight_b,
            "delta": weight.delta(),
        })).collect::<Vec<_>>(),
        "transformDiffers": bone.transform_differs,
        "transformLinkDiffers": bone.transform_link_differs,
        "transformMaxAbsDelta": bone.transform_max_abs_delta,
        "transformLinkMaxAbsDelta": bone.transform_link_max_abs_delta,
        "hasDifferences": bone.has_differences(),
    })
}
