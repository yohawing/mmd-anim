//! Reads skin cluster (per-bone deformer) data back out of arbitrary FBX 7.x
//! binary files and diffs two such files against each other.
//!
//! This is intentionally independent from the exporter's own node-id scheme
//! (`CLUSTER_ID_BASE`, `SKIN_ID`, ...): it walks `Objects/Deformer` nodes of
//! kind `Cluster` and resolves the owning bone by following the `OO`
//! connection from the cluster to its `Model` node. That makes it usable on
//! FBX files produced by other tools too, e.g. a Maya import/export
//! roundtrip of our own export, which assigns its own arbitrary object IDs.

use std::{collections::HashMap, io::Cursor};

use fbxcel::{
    low::v7400::AttributeValue,
    tree::{any::AnyTree, v7400::{NodeHandle, Tree}},
};

/// One parsed FBX skin cluster (per-bone deformer).
#[derive(Debug, Clone)]
pub struct FbxSkinClusterData {
    pub cluster_id: i64,
    pub bone_name: String,
    /// Control-point (vertex) indices influenced by this cluster.
    pub indices: Vec<i32>,
    /// Weights parallel to `indices`.
    pub weights: Vec<f64>,
    /// Cluster `Transform` (inverse bind matrix), row-major 4x4, if present.
    pub transform: Option<[f64; 16]>,
    /// Cluster `TransformLink` (bind matrix), row-major 4x4, if present.
    pub transform_link: Option<[f64; 16]>,
}

#[derive(Debug, thiserror::Error)]
pub enum FbxSkinReadError {
    #[error("failed to parse FBX tree: {0}")]
    Parse(String),
    #[error("FBX tree version is not supported (expected FBX 7.x binary)")]
    UnsupportedVersion,
}

/// Reads all skin cluster (per-bone deformer) data out of an arbitrary FBX
/// binary file's bytes.
pub fn read_fbx_skin_clusters(bytes: &[u8]) -> Result<Vec<FbxSkinClusterData>, FbxSkinReadError> {
    let tree = load_v7400_tree(bytes)?;
    let root = tree.root();
    let Some(objects) = root.first_child_by_name("Objects") else {
        return Ok(Vec::new());
    };

    let mut model_names: HashMap<i64, String> = HashMap::new();
    for model in objects.children_by_name("Model") {
        let attrs = model.attributes();
        let id = attrs.first().and_then(AttributeValue::get_i64);
        let raw_name = attrs.get(1).and_then(AttributeValue::get_string);
        if let (Some(id), Some(raw_name)) = (id, raw_name) {
            model_names.insert(id, fbx_object_name(raw_name).to_owned());
        }
    }

    // Cluster (parent) -> Model (child) via an "OO" connection, matching the
    // convention this crate's own exporter writes: `C,"OO",<modelId>,<clusterId>`.
    let mut cluster_to_model: HashMap<i64, i64> = HashMap::new();
    if let Some(connections) = root.first_child_by_name("Connections") {
        for connection in connections.children_by_name("C") {
            let attrs = connection.attributes();
            if attrs.first().and_then(AttributeValue::get_string) != Some("OO") {
                continue;
            }
            let Some(child_id) = attrs.get(1).and_then(AttributeValue::get_i64) else {
                continue;
            };
            let Some(parent_id) = attrs.get(2).and_then(AttributeValue::get_i64) else {
                continue;
            };
            if model_names.contains_key(&child_id) {
                cluster_to_model.entry(parent_id).or_insert(child_id);
            }
        }
    }

    let mut clusters = Vec::new();
    for deformer in objects.children_by_name("Deformer") {
        let attrs = deformer.attributes();
        if attrs.get(2).and_then(AttributeValue::get_string) != Some("Cluster") {
            continue;
        }
        let Some(cluster_id) = attrs.first().and_then(AttributeValue::get_i64) else {
            continue;
        };
        let bone_name = cluster_to_model
            .get(&cluster_id)
            .and_then(|model_id| model_names.get(model_id))
            .cloned()
            .unwrap_or_else(|| format!("<unresolved-cluster-{cluster_id}>"));
        let indices = arr_i32_child(deformer, "Indexes").unwrap_or_default();
        let weights = arr_f64_child(deformer, "Weights").unwrap_or_default();
        let transform =
            arr_f64_child(deformer, "Transform").and_then(|values| <[f64; 16]>::try_from(values.as_slice()).ok());
        let transform_link = arr_f64_child(deformer, "TransformLink")
            .and_then(|values| <[f64; 16]>::try_from(values.as_slice()).ok());
        clusters.push(FbxSkinClusterData {
            cluster_id,
            bone_name,
            indices,
            weights,
            transform,
            transform_link,
        });
    }

    Ok(clusters)
}

fn arr_i32_child(node: NodeHandle<'_>, name: &str) -> Option<Vec<i32>> {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_arr_i32)
        .map(|values| values.to_vec())
}

fn arr_f64_child(node: NodeHandle<'_>, name: &str) -> Option<Vec<f64>> {
    node.first_child_by_name(name)
        .and_then(|child| child.attributes().first())
        .and_then(AttributeValue::get_arr_f64)
        .map(|values| values.to_vec())
}

/// FBX binary object names are stored as `"<name>\u{0}\u{1}<class>"`. Splits
/// off the class suffix and returns just the object's own name.
fn fbx_object_name(raw: &str) -> &str {
    raw.split('\u{0}').next().unwrap_or(raw)
}

fn load_v7400_tree(bytes: &[u8]) -> Result<Tree, FbxSkinReadError> {
    match AnyTree::from_seekable_reader(Cursor::new(bytes))
        .map_err(|error| FbxSkinReadError::Parse(error.to_string()))?
    {
        AnyTree::V7400(_version, tree, _footer) => Ok(tree),
        _ => Err(FbxSkinReadError::UnsupportedVersion),
    }
}

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct FbxSkinDiffOptions {
    /// Minimum absolute weight delta for a shared vertex to be reported as changed.
    pub weight_epsilon: f64,
    /// Minimum absolute per-component delta for a Transform/TransformLink matrix
    /// to be reported as differing.
    pub matrix_epsilon: f64,
}

impl Default for FbxSkinDiffOptions {
    fn default() -> Self {
        Self {
            weight_epsilon: 0.001,
            matrix_epsilon: 1.0e-4,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FbxSkinVertexWeight {
    pub vertex_index: i32,
    pub weight: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct FbxSkinWeightDiff {
    pub vertex_index: i32,
    pub weight_a: f64,
    pub weight_b: f64,
}

impl FbxSkinWeightDiff {
    pub fn delta(&self) -> f64 {
        self.weight_b - self.weight_a
    }
}

#[derive(Debug, Clone, Default)]
pub struct FbxSkinBoneDiff {
    pub bone_name: String,
    pub in_a: bool,
    pub in_b: bool,
    pub vertex_count_a: usize,
    pub vertex_count_b: usize,
    /// Vertices influenced in B but not in A, with their B weight.
    pub added_vertices: Vec<FbxSkinVertexWeight>,
    /// Vertices influenced in A but not in B, with their A weight.
    pub removed_vertices: Vec<FbxSkinVertexWeight>,
    /// Vertices influenced in both, whose weight differs by more than
    /// `FbxSkinDiffOptions::weight_epsilon`.
    pub changed_weights: Vec<FbxSkinWeightDiff>,
    pub transform_a: Option<[f64; 16]>,
    pub transform_b: Option<[f64; 16]>,
    pub transform_link_a: Option<[f64; 16]>,
    pub transform_link_b: Option<[f64; 16]>,
    pub transform_differs: bool,
    pub transform_link_differs: bool,
    pub transform_max_abs_delta: Option<f64>,
    pub transform_link_max_abs_delta: Option<f64>,
}

impl FbxSkinBoneDiff {
    pub fn has_differences(&self) -> bool {
        !self.in_a
            || !self.in_b
            || !self.added_vertices.is_empty()
            || !self.removed_vertices.is_empty()
            || !self.changed_weights.is_empty()
            || self.transform_differs
            || self.transform_link_differs
    }
}

#[derive(Debug, Clone, Default)]
pub struct FbxSkinDiffReport {
    /// Sorted by bone name.
    pub bones: Vec<FbxSkinBoneDiff>,
}

impl FbxSkinDiffReport {
    pub fn differing_bones(&self) -> impl Iterator<Item = &FbxSkinBoneDiff> {
        self.bones.iter().filter(|bone| bone.has_differences())
    }

    pub fn difference_count(&self) -> usize {
        self.differing_bones().count()
    }
}

/// Diffs two sets of skin clusters, matching bones by name (not by FBX object
/// id, since different exporters/tools assign their own arbitrary ids).
///
/// If a bone name repeats within one side's cluster list, the first cluster
/// wins; this mirrors PMX/FBX skeletons where bone names are expected unique.
pub fn diff_fbx_skin_clusters(
    clusters_a: &[FbxSkinClusterData],
    clusters_b: &[FbxSkinClusterData],
    options: FbxSkinDiffOptions,
) -> FbxSkinDiffReport {
    let by_name_a = index_by_bone_name(clusters_a);
    let by_name_b = index_by_bone_name(clusters_b);

    let mut bone_names: Vec<&String> = by_name_a.keys().chain(by_name_b.keys()).copied().collect();
    bone_names.sort();
    bone_names.dedup();

    let bones = bone_names
        .into_iter()
        .map(|bone_name| {
            diff_one_bone(
                bone_name,
                by_name_a.get(bone_name).copied(),
                by_name_b.get(bone_name).copied(),
                options,
            )
        })
        .collect();

    FbxSkinDiffReport { bones }
}

fn index_by_bone_name(clusters: &[FbxSkinClusterData]) -> HashMap<&String, &FbxSkinClusterData> {
    let mut map = HashMap::with_capacity(clusters.len());
    for cluster in clusters {
        map.entry(&cluster.bone_name).or_insert(cluster);
    }
    map
}

fn diff_one_bone(
    bone_name: &str,
    cluster_a: Option<&FbxSkinClusterData>,
    cluster_b: Option<&FbxSkinClusterData>,
    options: FbxSkinDiffOptions,
) -> FbxSkinBoneDiff {
    let weights_a = cluster_a.map(vertex_weight_map).unwrap_or_default();
    let weights_b = cluster_b.map(vertex_weight_map).unwrap_or_default();

    let mut added: Vec<FbxSkinVertexWeight> = weights_b
        .iter()
        .filter(|(vertex, _)| !weights_a.contains_key(*vertex))
        .map(|(vertex, weight)| FbxSkinVertexWeight {
            vertex_index: *vertex,
            weight: *weight,
        })
        .collect();
    added.sort_unstable_by_key(|entry| entry.vertex_index);

    let mut removed: Vec<FbxSkinVertexWeight> = weights_a
        .iter()
        .filter(|(vertex, _)| !weights_b.contains_key(*vertex))
        .map(|(vertex, weight)| FbxSkinVertexWeight {
            vertex_index: *vertex,
            weight: *weight,
        })
        .collect();
    removed.sort_unstable_by_key(|entry| entry.vertex_index);

    let mut changed: Vec<FbxSkinWeightDiff> = weights_a
        .iter()
        .filter_map(|(vertex, weight_a)| {
            let weight_b = weights_b.get(vertex)?;
            if (weight_a - weight_b).abs() > options.weight_epsilon {
                Some(FbxSkinWeightDiff {
                    vertex_index: *vertex,
                    weight_a: *weight_a,
                    weight_b: *weight_b,
                })
            } else {
                None
            }
        })
        .collect();
    changed.sort_unstable_by_key(|entry| entry.vertex_index);

    let transform_a = cluster_a.and_then(|cluster| cluster.transform);
    let transform_b = cluster_b.and_then(|cluster| cluster.transform);
    let transform_link_a = cluster_a.and_then(|cluster| cluster.transform_link);
    let transform_link_b = cluster_b.and_then(|cluster| cluster.transform_link);
    let transform_max_abs_delta = matrix_max_abs_delta(transform_a, transform_b);
    let transform_link_max_abs_delta = matrix_max_abs_delta(transform_link_a, transform_link_b);

    FbxSkinBoneDiff {
        bone_name: bone_name.to_owned(),
        in_a: cluster_a.is_some(),
        in_b: cluster_b.is_some(),
        vertex_count_a: weights_a.len(),
        vertex_count_b: weights_b.len(),
        added_vertices: added,
        removed_vertices: removed,
        changed_weights: changed,
        transform_a,
        transform_b,
        transform_link_a,
        transform_link_b,
        transform_differs: matrix_differs(transform_max_abs_delta, transform_a, transform_b, options.matrix_epsilon),
        transform_link_differs: matrix_differs(
            transform_link_max_abs_delta,
            transform_link_a,
            transform_link_b,
            options.matrix_epsilon,
        ),
        transform_max_abs_delta,
        transform_link_max_abs_delta,
    }
}

fn vertex_weight_map(cluster: &FbxSkinClusterData) -> HashMap<i32, f64> {
    let mut map = HashMap::with_capacity(cluster.indices.len());
    for (position, vertex) in cluster.indices.iter().enumerate() {
        let weight = cluster.weights.get(position).copied().unwrap_or(0.0);
        map.insert(*vertex, weight);
    }
    map
}

fn matrix_max_abs_delta(a: Option<[f64; 16]>, b: Option<[f64; 16]>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => a
            .iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).abs())
            .fold(None, |max, delta| Some(max.map_or(delta, |max: f64| max.max(delta)))),
        _ => None,
    }
}

fn matrix_differs(
    max_abs_delta: Option<f64>,
    a: Option<[f64; 16]>,
    b: Option<[f64; 16]>,
    epsilon: f64,
) -> bool {
    match (a, b) {
        (Some(_), Some(_)) => max_abs_delta.is_some_and(|delta| delta > epsilon),
        (None, None) => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cluster(bone_name: &str, indices: &[i32], weights: &[f64]) -> FbxSkinClusterData {
        FbxSkinClusterData {
            cluster_id: 0,
            bone_name: bone_name.to_owned(),
            indices: indices.to_vec(),
            weights: weights.to_vec(),
            transform: Some(identity()),
            transform_link: Some(identity()),
        }
    }

    fn identity() -> [f64; 16] {
        let mut matrix = [0.0; 16];
        matrix[0] = 1.0;
        matrix[5] = 1.0;
        matrix[10] = 1.0;
        matrix[15] = 1.0;
        matrix
    }

    #[test]
    fn fbx_object_name_strips_class_suffix() {
        assert_eq!(fbx_object_name("LeftArm\u{0}\u{1}Model"), "LeftArm");
        assert_eq!(fbx_object_name("LeftArm"), "LeftArm");
    }

    #[test]
    fn diff_reports_added_removed_and_changed_weights() {
        let a = vec![cluster("Bone", &[0, 1, 2], &[1.0, 0.5, 0.25])];
        let b = vec![cluster("Bone", &[0, 1, 3], &[1.0, 0.6, 0.75])];

        let report = diff_fbx_skin_clusters(&a, &b, FbxSkinDiffOptions::default());
        assert_eq!(report.bones.len(), 1);
        let bone = &report.bones[0];
        assert!(bone.in_a && bone.in_b);
        assert_eq!(bone.vertex_count_a, 3);
        assert_eq!(bone.vertex_count_b, 3);
        assert_eq!(bone.removed_vertices.len(), 1);
        assert_eq!(bone.removed_vertices[0].vertex_index, 2);
        assert_eq!(bone.added_vertices.len(), 1);
        assert_eq!(bone.added_vertices[0].vertex_index, 3);
        assert_eq!(bone.changed_weights.len(), 1);
        assert_eq!(bone.changed_weights[0].vertex_index, 1);
        assert!(bone.has_differences());
    }

    #[test]
    fn diff_ignores_weight_changes_below_epsilon() {
        let a = vec![cluster("Bone", &[0], &[0.5])];
        let b = vec![cluster("Bone", &[0], &[0.5005])];
        let report = diff_fbx_skin_clusters(
            &a,
            &b,
            FbxSkinDiffOptions {
                weight_epsilon: 0.001,
                matrix_epsilon: 1.0e-4,
            },
        );
        assert!(!report.bones[0].has_differences());
    }

    #[test]
    fn diff_reports_bones_present_in_only_one_side() {
        let a = vec![cluster("OnlyA", &[0], &[1.0])];
        let b = vec![cluster("OnlyB", &[0], &[1.0])];
        let report = diff_fbx_skin_clusters(&a, &b, FbxSkinDiffOptions::default());
        assert_eq!(report.bones.len(), 2);
        let only_a = report.bones.iter().find(|bone| bone.bone_name == "OnlyA").unwrap();
        assert!(only_a.in_a && !only_a.in_b);
        assert!(only_a.has_differences());
        let only_b = report.bones.iter().find(|bone| bone.bone_name == "OnlyB").unwrap();
        assert!(!only_b.in_a && only_b.in_b);
        assert!(only_b.has_differences());
    }

    #[test]
    fn diff_reports_transform_matrix_delta() {
        let mut moved = identity();
        moved[12] = 1.0;
        let a = FbxSkinClusterData {
            transform_link: Some(identity()),
            ..cluster("Bone", &[0], &[1.0])
        };
        let b = FbxSkinClusterData {
            transform_link: Some(moved),
            ..cluster("Bone", &[0], &[1.0])
        };
        let report = diff_fbx_skin_clusters(&[a], &[b], FbxSkinDiffOptions::default());
        let bone = &report.bones[0];
        assert!(bone.transform_link_differs);
        assert!(!bone.transform_differs);
        assert_eq!(bone.transform_link_max_abs_delta, Some(1.0));
    }
}
