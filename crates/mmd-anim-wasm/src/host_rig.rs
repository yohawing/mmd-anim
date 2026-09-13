use glam::{Mat4, Quat, Vec3A};
use js_sys::{Float32Array, Function, Promise, Uint8Array};
use mmd_anim_runtime::{BoneIndex, HostPoseView, HostRigDefinition, IkSolveOptions};
use wasm_bindgen::{JsCast, prelude::*};

use super::{WasmMmdModel, WasmMmdRuntimeInstance};

/// Model-bound retargeted pose evaluator. Call free() when no longer needed.
/// Models, rigs and instances may be released in any order after evaluation.
#[wasm_bindgen]
pub struct WasmMmdHostRig {
    rig: HostRigDefinition,
    positions: Vec<Vec3A>,
    rotations: Vec<Quat>,
    scales: Vec<Vec3A>,
    before_world_matrices: Vec<f32>,
    physics_world_matrices: Vec<f32>,
    physics_world_matrix_mask: Vec<u8>,
    physics_writeback: Vec<Option<Mat4>>,
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
        count
            .checked_mul(16)
            .ok_or_else(|| JsValue::from_str("host rig matrix buffer size overflow"))?;
        Ok(Self {
            rig,
            positions: vec![Vec3A::ZERO; count],
            rotations: vec![Quat::IDENTITY; count],
            scales: vec![Vec3A::ONE; count],
            before_world_matrices: Vec::new(),
            physics_world_matrices: Vec::new(),
            physics_world_matrix_mask: Vec::new(),
            physics_writeback: Vec::new(),
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
        self.copy_pose_inputs(positions_xyz, rotations_xyzw, scales_xyz)?;
        let options = ik_options(ik_tolerance, ik_max_iterations_cap)?;
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
                options,
            )
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        instance.refresh_caches();
        Ok(())
    }

    /// Evaluate one backend-neutral RuntimeRig physics frame. The synchronous
    /// callback receives `(beforeWorldMatrices, physicsWorldMatrices, mask)`.
    /// It must fill model-space column-major physics matrices, set non-zero mask
    /// bytes for selected bones, and return `true`. Promise results are rejected.
    /// Calling back into `instance` while this method is active is unsupported.
    #[wasm_bindgen(js_name = evaluateWithExternalPhysics)]
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_with_external_physics(
        &mut self,
        instance: &mut WasmMmdRuntimeInstance,
        positions_xyz: &[f32],
        rotations_xyzw: &[f32],
        scales_xyz: &[f32],
        morph_weights: &[f32],
        ik_enabled: &[u8],
        ik_tolerance: f32,
        ik_max_iterations_cap: u32,
        callback: &Function,
    ) -> Result<(), JsValue> {
        self.copy_pose_inputs(positions_xyz, rotations_xyzw, scales_xyz)?;
        let options = ik_options(ik_tolerance, ik_max_iterations_cap)?;
        let bone_count = self.positions.len();
        let matrix_len = bone_count
            .checked_mul(16)
            .ok_or_else(|| JsValue::from_str("host rig matrix buffer size overflow"))?;
        self.before_world_matrices.resize(matrix_len, 0.0);
        self.physics_world_matrices.resize(matrix_len, 0.0);
        self.physics_world_matrix_mask.resize(bone_count, 0);
        self.physics_writeback.resize(bone_count, None);
        let mut evaluation = instance
            .runtime
            .begin_host_rig_evaluation(
                &self.rig,
                &HostPoseView {
                    local_position_offsets: &self.positions,
                    local_rotations: &self.rotations,
                    local_scales: &self.scales,
                    morph_weights,
                    ik_enabled,
                },
                options,
            )
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        evaluation.evaluate_before_physics_with_ik_options(options);

        let outcome = (|| -> Result<(), JsValue> {
            flatten_matrices_into_slice(
                &mut self.before_world_matrices,
                evaluation.runtime_mut().world_matrices(),
            );
            let before = Float32Array::from(self.before_world_matrices.as_slice());
            let physics = Float32Array::from(self.before_world_matrices.as_slice());
            let mask_len = u32::try_from(self.physics_world_matrix_mask.len())
                .map_err(|_| JsValue::from_str("host rig bone count exceeds TypedArray limits"))?;
            let mask = Uint8Array::new_with_length(mask_len);
            let result = callback.call3(
                &JsValue::UNDEFINED,
                before.as_ref(),
                physics.as_ref(),
                mask.as_ref(),
            )?;
            if result.is_instance_of::<Promise>() {
                return Err(JsValue::from_str(
                    "external physics callback must be synchronous",
                ));
            }
            if result.as_bool() != Some(true) {
                return Err(JsValue::from_str(
                    "external physics callback must return true",
                ));
            }

            if physics.length() as usize != matrix_len || mask.length() != mask_len {
                return Err(JsValue::from_str(
                    "external physics callback detached an output buffer",
                ));
            }
            if bone_count != 0 {
                physics.copy_to(&mut self.physics_world_matrices);
                mask.copy_to(&mut self.physics_world_matrix_mask);
            }
            self.physics_writeback.fill(None);
            for bone_index in 0..self.physics_writeback.len() {
                if self.physics_world_matrix_mask[bone_index] == 0 {
                    continue;
                }
                let start = bone_index * 16;
                let raw = <[f32; 16]>::try_from(&self.physics_world_matrices[start..start + 16])
                    .expect("host rig matrix buffer length is fixed at construction");
                if raw.iter().any(|value| !value.is_finite()) {
                    return Err(JsValue::from_str(
                        "external physics callback wrote a non-finite matrix",
                    ));
                }
                self.physics_writeback[bone_index] = Some(Mat4::from_cols_array(&raw));
            }
            evaluation
                .runtime_mut()
                .apply_physics_world_matrices(&self.physics_writeback);
            evaluation.evaluate_after_physics_with_ik_options(options);
            Ok(())
        })();
        drop(evaluation);
        instance.refresh_caches();
        outcome
    }

    fn copy_pose_inputs(
        &mut self,
        positions_xyz: &[f32],
        rotations_xyzw: &[f32],
        scales_xyz: &[f32],
    ) -> Result<(), JsValue> {
        let n = self.positions.len();
        let position_len = n
            .checked_mul(3)
            .ok_or_else(|| JsValue::from_str("host rig pose count overflow"))?;
        let rotation_len = n
            .checked_mul(4)
            .ok_or_else(|| JsValue::from_str("host rig pose count overflow"))?;
        if positions_xyz.len() != position_len
            || rotations_xyzw.len() != rotation_len
            || scales_xyz.len() != position_len
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
        Ok(())
    }
}

fn flatten_matrices_into_slice(dst: &mut [f32], matrices: &[Mat4]) {
    debug_assert_eq!(dst.len(), matrices.len() * 16);
    for (matrix_index, matrix) in matrices.iter().enumerate() {
        dst[matrix_index * 16..matrix_index * 16 + 16].copy_from_slice(&matrix.to_cols_array());
    }
}

fn ik_options(ik_tolerance: f32, ik_max_iterations_cap: u32) -> Result<IkSolveOptions, JsValue> {
    if !ik_tolerance.is_finite() || ik_tolerance < 0.0 {
        return Err(JsValue::from_str(
            "ikTolerance must be non-negative and finite",
        ));
    }
    Ok(IkSolveOptions {
        tolerance: ik_tolerance,
        max_iterations_cap: (ik_max_iterations_cap != 0).then_some(ik_max_iterations_cap),
    })
}
