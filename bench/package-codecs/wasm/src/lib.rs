use package_codecs_core::{
    check_boundaries, compress, decompress, decrypt, encrypt, BoundaryChecks, PipelineSizes,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance)]
    fn now() -> f64;
}

#[derive(Debug, Serialize)]
struct WasmResult {
    sizes: PipelineSizes,
    checks: BoundaryChecks,
    timings: WasmTimings,
}

#[derive(Debug, Serialize)]
struct WasmTimings {
    compress_ms: f64,
    encrypt_ms: f64,
    decrypt_ms: f64,
    decompress_ms: f64,
    pipeline_ms: f64,
    pipeline_mib_per_s: f64,
}

fn mib_per_second(bytes: usize, elapsed_ms: f64) -> f64 {
    if elapsed_ms <= 0.0 {
        return 0.0;
    }
    bytes as f64 / (1024.0 * 1024.0) / (elapsed_ms / 1000.0)
}

#[wasm_bindgen]
pub fn run_case(input: &[u8], case_id: &str) -> Result<String, JsValue> {
    let aad = format!("mmdpack-phase0/{case_id}");
    let pipeline_start = now();

    let start = now();
    let compressed = compress(input).map_err(|error| JsValue::from_str(&error))?;
    let compress_ms = now() - start;

    let start = now();
    let encrypted =
        encrypt(compressed, aad.as_bytes()).map_err(|error| JsValue::from_str(&error))?;
    let encrypt_ms = now() - start;

    let start = now();
    let decrypted =
        decrypt(&encrypted, aad.as_bytes()).map_err(|error| JsValue::from_str(&error))?;
    let decrypt_ms = now() - start;

    let start = now();
    let restored =
        decompress(&decrypted, input.len()).map_err(|error| JsValue::from_str(&error))?;
    let decompress_ms = now() - start;
    let pipeline_ms = now() - pipeline_start;

    let mut checks = check_boundaries(&encrypted, aad.as_bytes());
    checks.round_trip = restored == input;
    let result = WasmResult {
        sizes: PipelineSizes {
            input_bytes: input.len(),
            compressed_bytes: encrypted.ciphertext().len().saturating_sub(16),
            ciphertext_bytes: encrypted.ciphertext().len(),
        },
        checks,
        timings: WasmTimings {
            compress_ms,
            encrypt_ms,
            decrypt_ms,
            decompress_ms,
            pipeline_ms,
            pipeline_mib_per_s: mib_per_second(input.len(), pipeline_ms),
        },
    };
    serde_json::to_string(&result).map_err(|error| JsValue::from_str(&error.to_string()))
}
