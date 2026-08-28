use package_backends_core::{
    aes_decrypt_rustcrypto, aes_encrypt_rustcrypto, campaign_material, conformance_material,
    zstd_decode_baseline, zstd_decode_ruzstd, AES_KEY_BYTES, AES_NONCE_BYTES, TEST_VECTOR_INPUT,
};
use wasm_bindgen::prelude::*;

fn js_error(error: String) -> JsValue {
    JsValue::from_str(&error)
}

fn key_bytes(key: &[u8]) -> Result<[u8; AES_KEY_BYTES], JsValue> {
    key.try_into()
        .map_err(|_| js_error("AES-GCM key must contain 32 bytes".to_string()))
}

fn nonce_bytes(nonce: &[u8]) -> Result<[u8; AES_NONCE_BYTES], JsValue> {
    nonce
        .try_into()
        .map_err(|_| js_error("AES-GCM nonce must contain 12 bytes".to_string()))
}

/// Fixed public vector entry point. This is conformance/unit-test material,
/// never the campaign or production key path.
#[wasm_bindgen]
pub fn rustcrypto_encrypt_conformance() -> Result<Vec<u8>, JsValue> {
    let (key, nonce, aad) = conformance_material();
    aes_encrypt_rustcrypto(TEST_VECTOR_INPUT, &key, &nonce, &aad).map_err(js_error)
}

#[wasm_bindgen]
pub fn rustcrypto_decrypt_explicit(
    wire: &[u8],
    key: &[u8],
    nonce: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, JsValue> {
    let key = key_bytes(key)?;
    let nonce = nonce_bytes(nonce)?;
    aes_decrypt_rustcrypto(wire, &key, &nonce, aad).map_err(js_error)
}

#[wasm_bindgen]
pub fn rustcrypto_encrypt_campaign(
    input: &[u8],
    key: &[u8],
    case_id: &str,
    domain: &str,
) -> Result<Vec<u8>, JsValue> {
    let key = key_bytes(key)?;
    let (key, nonce, aad) = campaign_material(&key, case_id, domain);
    aes_encrypt_rustcrypto(input, &key, &nonce, &aad).map_err(js_error)
}

#[wasm_bindgen]
pub fn rustcrypto_decrypt_campaign(
    wire: &[u8],
    key: &[u8],
    case_id: &str,
    domain: &str,
) -> Result<Vec<u8>, JsValue> {
    let key = key_bytes(key)?;
    let (key, nonce, aad) = campaign_material(&key, case_id, domain);
    aes_decrypt_rustcrypto(wire, &key, &nonce, &aad).map_err(js_error)
}

#[wasm_bindgen]
pub fn zstd_decode_libzstd(frame: &[u8], expected_size: usize) -> Result<Vec<u8>, JsValue> {
    zstd_decode_baseline(frame, expected_size).map_err(js_error)
}

#[wasm_bindgen]
pub fn zstd_decode_ruzstd_wasm(frame: &[u8], expected_size: usize) -> Result<Vec<u8>, JsValue> {
    zstd_decode_ruzstd(frame, expected_size).map_err(js_error)
}
