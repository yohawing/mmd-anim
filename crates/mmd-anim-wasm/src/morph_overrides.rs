use super::{IkSolveOptions, WasmMmdClip, WasmMmdRuntimeInstance};
use wasm_bindgen::prelude::*;

fn ik_options(tolerance: Option<f32>, cap: u32) -> Result<IkSolveOptions, JsValue> {
    let tolerance = tolerance.unwrap_or_else(|| IkSolveOptions::default().tolerance);
    if !tolerance.is_finite() || tolerance < 0.0 {
        return Err(JsValue::from_str(
            "ikTolerance must be non-negative and finite",
        ));
    }
    Ok(IkSolveOptions {
        tolerance,
        max_iterations_cap: (cap != 0).then_some(cap),
    })
}

#[wasm_bindgen]
impl WasmMmdRuntimeInstance {
    /// Override direct morph weights before group expansion and Append/IK.
    #[wasm_bindgen(js_name = evaluateClipFrameWithMorphOverrides)]
    #[allow(clippy::too_many_arguments)]
    pub fn evaluate_clip_frame_with_morph_overrides(
        &mut self,
        clip: &WasmMmdClip,
        frame: f32,
        indices: &[u32],
        weights: &[f32],
        ik_tolerance: Option<f32>,
        ik_max_iterations_cap: u32,
        ik_enabled: bool,
    ) -> Result<(), JsValue> {
        let options = ik_options(ik_tolerance, ik_max_iterations_cap)?;
        self.runtime
            .evaluate_with_morph_overrides(
                Some(&clip.clip),
                frame,
                indices,
                weights,
                options,
                ik_enabled,
            )
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        self.refresh_caches();
        Ok(())
    }

    /// Rebuild the rest pose before applying overrides; repeated calls never accumulate.
    #[wasm_bindgen(js_name = evaluateRestPoseWithMorphOverrides)]
    pub fn evaluate_rest_pose_with_morph_overrides(
        &mut self,
        indices: &[u32],
        weights: &[f32],
        ik_tolerance: Option<f32>,
        ik_max_iterations_cap: u32,
        ik_enabled: bool,
    ) -> Result<(), JsValue> {
        let options = ik_options(ik_tolerance, ik_max_iterations_cap)?;
        self.runtime
            .evaluate_with_morph_overrides(None, 0.0, indices, weights, options, ik_enabled)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        self.refresh_caches();
        Ok(())
    }
}
