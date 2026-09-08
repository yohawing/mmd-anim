use glam::{Quat, Vec3A};
use mmd_anim_runtime::{BoneIndex, HostPoseView, HostRigDefinition, IkSolveOptions};
use wasm_bindgen::prelude::*;

use super::{WasmMmdModel, WasmMmdRuntimeInstance};

/// Model-bound retargeted pose evaluator. Call free() when no longer needed.
/// Models, rigs and instances may be released in any order after evaluation.
#[wasm_bindgen]
pub struct WasmMmdHostRig {
    rig: HostRigDefinition,
    positions: Vec<Vec3A>,
    rotations: Vec<Quat>,
    scales: Vec<Vec3A>,
}

#[wasm_bindgen]
impl WasmMmdHostRig {
    /// Lists contain MMD bone indices, not Humanoid slot indices. IK is disabled
    /// unless its PMX controller is listed in goalBones and enabled per frame.
    #[wasm_bindgen(constructor)]
    pub fn new(
        model: &WasmMmdModel,
        driven_bones: &[u32],
        goal_bones: &[u32],
    ) -> Result<WasmMmdHostRig, JsValue> {
        let driven: Vec<_> = driven_bones.iter().copied().map(BoneIndex).collect();
        let goals: Vec<_> = goal_bones.iter().copied().map(BoneIndex).collect();
        let rig = HostRigDefinition::new(model.model.clone(), &driven, &goals)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let count = model.model.bone_count();
        Ok(Self {
            rig,
            positions: vec![Vec3A::ZERO; count],
            rotations: vec![Quat::IDENTITY; count],
            scales: vec![Vec3A::ONE; count],
        })
    }

    /// Input is fresh pre-morph MMD local offsets/quaternions/scales, not the
    /// previous output pose. Scales must be positive and uniform per bone.
    /// Driven bones retain post-morph FK world transforms; other bones evaluate
    /// Append/IK through both phases. Invalid input throws without changing
    /// instance outputs. cap=0 retains authored IK iterations. Read results with
    /// the instance's existing world/skinning/morph copy or view methods.
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate(
        &mut self,
        instance: &mut WasmMmdRuntimeInstance,
        positions_xyz: &[f32],
        rotations_xyzw: &[f32],
        scales_xyz: &[f32],
        morph_weights: &[f32],
        ik_enabled: &[u8],
        ik_tolerance: f32,
        ik_max_iterations_cap: u32,
    ) -> Result<(), JsValue> {
        let n = self.positions.len();
        if positions_xyz.len() != n * 3
            || rotations_xyzw.len() != n * 4
            || scales_xyz.len() != n * 3
        {
            return Err(JsValue::from_str(
                "host rig pose arrays must match model bone count",
            ));
        }
        for (dst, src) in self.positions.iter_mut().zip(positions_xyz.chunks_exact(3)) {
            *dst = Vec3A::new(src[0], src[1], src[2]);
        }
        for (dst, src) in self
            .rotations
            .iter_mut()
            .zip(rotations_xyzw.chunks_exact(4))
        {
            *dst = Quat::from_xyzw(src[0], src[1], src[2], src[3]);
        }
        for (dst, src) in self.scales.iter_mut().zip(scales_xyz.chunks_exact(3)) {
            *dst = Vec3A::new(src[0], src[1], src[2]);
        }
        instance
            .runtime
            .evaluate_host_rig_pose(
                &self.rig,
                &HostPoseView {
                    local_position_offsets: &self.positions,
                    local_rotations: &self.rotations,
                    local_scales: &self.scales,
                    morph_weights,
                    ik_enabled,
                },
                IkSolveOptions {
                    tolerance: ik_tolerance,
                    max_iterations_cap: (ik_max_iterations_cap != 0)
                        .then_some(ik_max_iterations_cap),
                },
            )
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        instance.refresh_caches();
        Ok(())
    }
}
