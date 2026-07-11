use mmd_anim_format::{PmxParsedModel, PmxPartsDescriptor, PmxPartsInput};

pub(crate) fn build_test_pmx_model(descriptor: PmxPartsDescriptor) -> PmxParsedModel {
    let positions_xyz = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let normals_xyz = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let uvs_xy = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let indices = [0, 1, 2];
    mmd_anim_format::build_pmx_model_from_parts(PmxPartsInput {
        descriptor,
        positions_xyz: &positions_xyz,
        normals_xyz: &normals_xyz,
        uvs_xy: &uvs_xy,
        indices: &indices,
        skin_indices: &[],
        skin_weights: &[],
        edge_scale: &[],
    })
    .unwrap()
}
