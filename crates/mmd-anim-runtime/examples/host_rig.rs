//! Run with: cargo run -p mmd-anim-runtime --example host_rig
use std::sync::Arc;

use glam::{Quat, Vec3A};
use mmd_anim_runtime::{
    AppendTransformInit, BoneIndex, BoneInit, HostPoseView, HostRigDefinition, IkSolveOptions,
    ModelArena, RuntimeInstance,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // source -> helper -> primary; helper inherits another half of source's
    // rotation. The retargeted primary must not rotate twice as a consequence.
    let model = Arc::new(ModelArena::new_full(
        vec![
            BoneInit::new(None, Vec3A::ZERO),
            BoneInit::new(Some(BoneIndex(0)), Vec3A::Y),
            BoneInit::new(Some(BoneIndex(1)), Vec3A::Y),
        ],
        vec![],
        vec![AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 0.5).with_rotation()],
    )?);
    let rig = HostRigDefinition::new(model.clone(), &[BoneIndex(0), BoneIndex(2)], &[])?;
    let mut runtime = RuntimeInstance::new(model);
    // The host builds these base arrays from its retarget output, not from
    // the display skeleton after the previous MMD evaluation.
    let positions = [Vec3A::ZERO; 3];
    let rotations = [
        Quat::from_rotation_z(0.4),
        Quat::IDENTITY,
        Quat::from_rotation_z(0.2),
    ];
    let scales = [Vec3A::ONE; 3];
    let pose = HostPoseView {
        local_position_offsets: &positions,
        local_rotations: &rotations,
        local_scales: &scales,
        morph_weights: &[],
        ik_enabled: &[],
    };
    runtime.evaluate_host_rig_pose(&rig, &pose, IkSolveOptions::default())?;
    for (index, world) in runtime.world_matrices().iter().enumerate() {
        println!("bone {index}: position {:?}", world.w_axis.truncate());
    }
    Ok(())
}
