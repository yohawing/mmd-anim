use package_backends_core::{
    aes_decrypt_rustcrypto, aes_encrypt_rustcrypto, campaign_material, conformance_material,
    sha256_hex, zstd_decode_baseline, zstd_decode_ruzstd, zstd_encode, AES_TAG_BYTES,
    MAX_WINDOW_LOG, TEST_VECTOR_INPUT, ZSTD_LEVEL,
};
use ring::aead::{self, Aad, LessSafeKey, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

const REPORT_SCHEMA: &str = "mmdpack-phase0-backends/1";
const WARMUP: usize = 1;
const REPEATS: usize = 5;
const RING_BACKEND: &str = "ring 0.17.14 AES-256-GCM";
const RUSTCRYPTO_BACKEND: &str = "RustCrypto aes-gcm 0.10.3 AES-256-GCM";

#[derive(Clone, Debug, Deserialize)]
struct Campaign {
    schema: String,
    description: String,
    cases: Vec<CampaignCase>,
}

#[derive(Clone, Debug, Deserialize)]
struct CampaignCase {
    id: String,
    kind: String,
    size_class: String,
    path_label: String,
    path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Timing {
    pub warmup: usize,
    pub repeats: usize,
    pub samples_ms: Vec<f64>,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub throughput_mib_s: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AesChecks {
    pub round_trip: bool,
    pub wrong_key_rejected: bool,
    pub wrong_aad_rejected: bool,
    pub tamper_rejected: bool,
    pub truncation_rejected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AesBackendReport {
    pub backend: String,
    pub wire_bytes: usize,
    pub wire_sha256: String,
    pub plaintext_sha256: String,
    pub encrypt: Timing,
    pub decrypt: Timing,
    pub checks: AesChecks,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AesReport {
    pub rustcrypto: AesBackendReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ring: Option<AesBackendReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webcrypto: Option<AesBackendReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConformanceBackendReport {
    pub backend: String,
    pub wire_bytes: usize,
    pub wire_sha256: String,
    pub plaintext_sha256: String,
    pub checks: AesChecks,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AesConformanceReport {
    pub vector_id: String,
    pub input_bytes: usize,
    pub input_sha256: String,
    pub rustcrypto: ConformanceBackendReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ring: Option<ConformanceBackendReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webcrypto: Option<ConformanceBackendReport>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZstdChecks {
    pub round_trip: bool,
    pub size_limit_rejected: bool,
    pub truncation_rejected: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZstdBackendReport {
    pub backend: String,
    pub decoded_bytes: usize,
    pub decoded_sha256: String,
    pub decode: Timing,
    pub checks: ZstdChecks,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ZstdReport {
    pub libzstd: ZstdBackendReport,
    pub ruzstd: ZstdBackendReport,
    pub decoded_equal: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaseReport {
    pub id: String,
    pub kind: String,
    pub size_class: String,
    pub path_label: String,
    pub input_bytes: usize,
    pub input_sha256: String,
    pub compressed_bytes: usize,
    pub compressed_sha256: String,
    pub aes: AesReport,
    pub zstd: ZstdReport,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnvironmentReport {
    pub platform: String,
    pub rustc: String,
    pub native_runner: String,
    pub native_binary_bytes: u64,
    pub campaign_manifest_sha256: String,
    pub cargo_lock_sha256: String,
    pub wasm_pack: String,
    pub harness_source_digest: String,
    pub msrv_statement: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigReport {
    pub aes_wire: String,
    pub aes_reference: String,
    pub aes_candidate: String,
    pub zstd_encoder: String,
    pub zstd_level: i32,
    pub zstd_reference_decoder: String,
    pub zstd_candidate_decoder: String,
    pub zstd_frame_policy: String,
    pub max_window_bytes: u64,
    pub max_decoded_bytes: usize,
    pub vector_policy: String,
    pub timing_policy: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub schema: String,
    pub lane: String,
    pub campaign_schema: String,
    pub description: String,
    pub status: String,
    pub run_id: String,
    pub measured_at_utc: String,
    pub environment: EnvironmentReport,
    pub config: ConfigReport,
    pub aes_conformance: AesConformanceReport,
    pub cases: Vec<CaseReport>,
    pub failures: Vec<String>,
    pub observable_copies: CopyObservation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CopyObservation {
    pub input_calls: usize,
    pub input_bytes: usize,
    pub js_wasm_copy_count: Option<usize>,
    pub js_wasm_copy_bytes: Option<usize>,
    pub note: String,
}

#[derive(Clone, Debug)]
struct Context {
    run_id: String,
    measured_at_utc: String,
    manifest_sha256: String,
    lock_sha256: String,
    source_digest: String,
    wasm_pack: String,
}

fn arg(args: &[String], name: &str) -> Result<String, String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
        .ok_or_else(|| format!("missing {name}"))
}

fn hex_sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn safe_label(label: &str) -> bool {
    !label.is_empty()
        && !label.contains(['\r', '\n', '|', '`'])
        && !label.chars().any(char::is_control)
}

fn validate_campaign(campaign: &Campaign) -> Result<(), String> {
    if campaign.schema != "mmdpack-phase0-campaign/1" {
        return Err(format!("unsupported campaign schema: {}", campaign.schema));
    }
    if campaign.description.trim().is_empty() || !(10..=20).contains(&campaign.cases.len()) {
        return Err("campaign must have a description and 10-20 cases".to_string());
    }
    let mut ids = HashSet::new();
    for case in &campaign.cases {
        if !safe_id(&case.id) || !ids.insert(case.id.clone()) {
            return Err(format!(
                "invalid or duplicate campaign case id: {}",
                case.id
            ));
        }
        if !matches!(case.kind.as_str(), "pmx" | "vmd")
            || !matches!(case.size_class.as_str(), "small" | "medium" | "large")
        {
            return Err(format!("unsupported campaign class for {}", case.id));
        }
        if !safe_label(&case.path_label) || case.path.trim().is_empty() {
            return Err(format!("invalid campaign path metadata for {}", case.id));
        }
    }
    Ok(())
}

fn context(args: &[String], manifest: &str, lock: &str) -> Result<Context, String> {
    let expected_manifest = arg(args, "--expected-manifest-sha256")?;
    let expected_lock = arg(args, "--expected-lock-sha256")?;
    if expected_manifest != manifest || expected_lock != lock {
        return Err("manifest or Cargo.lock hash does not match the bytes used".to_string());
    }
    Ok(Context {
        run_id: arg(args, "--run-id")?,
        measured_at_utc: arg(args, "--measured-at")?,
        manifest_sha256: manifest.to_string(),
        lock_sha256: lock.to_string(),
        source_digest: arg(args, "--harness-source-digest")?,
        wasm_pack: arg(args, "--wasm-pack").unwrap_or_else(|_| "not-run".to_string()),
    })
}

fn campaign_key() -> Result<[u8; 32], String> {
    let encoded = env::var("MMDPACK_BACKENDS_CAMPAIGN_KEY_HEX")
        .map_err(|_| "campaign key environment value is missing".to_string())?;
    if encoded.len() != 64 {
        return Err("campaign key environment value must contain 64 hex characters".to_string());
    }
    let mut key = [0_u8; 32];
    for (index, slot) in key.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16)
            .map_err(|_| "campaign key environment value is not hexadecimal".to_string())?;
    }
    Ok(key)
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| "unavailable".to_string())
}

fn throughput(bytes: usize, millis: f64) -> f64 {
    if millis <= 0.0 {
        0.0
    } else {
        bytes as f64 / (1024.0 * 1024.0) / (millis / 1000.0)
    }
}

fn measure<T, F>(bytes: usize, mut operation: F) -> Result<(T, Timing), String>
where
    T: Clone,
    F: FnMut(usize) -> Result<T, String>,
{
    for iteration in 0..WARMUP {
        operation(iteration)?;
    }
    let mut samples = Vec::with_capacity(REPEATS);
    let mut last = None;
    for iteration in 0..REPEATS {
        let start = Instant::now();
        let output = operation(WARMUP + iteration)?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        last = Some(output);
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f64::total_cmp);
    let p50 = sorted[REPEATS / 2];
    let p95 = sorted[REPEATS - 1];
    Ok((
        last.expect("REPEATS is non-zero"),
        Timing {
            warmup: WARMUP,
            repeats: REPEATS,
            samples_ms: samples,
            p50_ms: p50,
            p95_ms: p95,
            throughput_mib_s: throughput(bytes, p50),
        },
    ))
}

fn ring_key(key: &[u8; 32]) -> Result<LessSafeKey, String> {
    let unbound = UnboundKey::new(&aead::AES_256_GCM, key)
        .map_err(|_| "ring AES-GCM key initialization failed".to_string())?;
    Ok(LessSafeKey::new(unbound))
}

fn aes_encrypt_ring(
    input: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    let cipher = ring_key(key)?;
    let mut wire = input.to_vec();
    cipher
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(*nonce),
            Aad::from(aad),
            &mut wire,
        )
        .map_err(|_| "ring AES-GCM encryption failed".to_string())?;
    Ok(wire)
}

fn aes_decrypt_ring(
    wire: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    if wire.len() < AES_TAG_BYTES {
        return Err("ring AES-GCM wire payload is shorter than its tag".to_string());
    }
    let cipher = ring_key(key)?;
    let mut input = wire.to_vec();
    cipher
        .open_in_place(
            Nonce::assume_unique_for_key(*nonce),
            Aad::from(aad),
            &mut input,
        )
        .map(|plaintext| plaintext.to_vec())
        .map_err(|_| "ring AES-GCM authentication failed".to_string())
}

fn decrypt_for_backend(
    ring: bool,
    wire: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    if ring {
        aes_decrypt_ring(wire, key, nonce, aad)
    } else {
        aes_decrypt_rustcrypto(wire, key, nonce, aad)
    }
}

fn encrypt_for_backend(
    ring: bool,
    input: &[u8],
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    if ring {
        aes_encrypt_ring(input, key, nonce, aad)
    } else {
        aes_encrypt_rustcrypto(input, key, nonce, aad)
    }
}

fn conformance_backend_report(
    backend: &str,
    ring: bool,
) -> Result<ConformanceBackendReport, String> {
    let (key, nonce, aad) = conformance_material();
    let wire = encrypt_for_backend(ring, TEST_VECTOR_INPUT, &key, &nonce, &aad)?;
    let plaintext = decrypt_for_backend(ring, &wire, &key, &nonce, &aad)?;
    let mut wrong_key = key;
    wrong_key[0] ^= 1;
    let wrong_key_rejected = decrypt_for_backend(ring, &wire, &wrong_key, &nonce, &aad).is_err();
    let mut wrong_aad = aad.clone();
    wrong_aad.push(1);
    let wrong_aad_rejected = decrypt_for_backend(ring, &wire, &key, &nonce, &wrong_aad).is_err();
    let mut tampered = wire.clone();
    tampered[0] ^= 1;
    let tamper_rejected = decrypt_for_backend(ring, &tampered, &key, &nonce, &aad).is_err();
    let truncation_rejected =
        decrypt_for_backend(ring, &wire[..wire.len() - 1], &key, &nonce, &aad).is_err();
    let checks = AesChecks {
        round_trip: plaintext == TEST_VECTOR_INPUT,
        wrong_key_rejected,
        wrong_aad_rejected,
        tamper_rejected,
        truncation_rejected,
    };
    if !checks.round_trip
        || !checks.wrong_key_rejected
        || !checks.wrong_aad_rejected
        || !checks.tamper_rejected
        || !checks.truncation_rejected
    {
        return Err(format!(
            "AES conformance boundary check failed for {backend}"
        ));
    }
    let expected_wire_bytes = TEST_VECTOR_INPUT
        .len()
        .checked_add(AES_TAG_BYTES)
        .ok_or_else(|| "AES conformance wire size overflow".to_string())?;
    if wire.len() != expected_wire_bytes {
        return Err(format!("AES conformance wire size failed for {backend}"));
    }
    Ok(ConformanceBackendReport {
        backend: backend.to_string(),
        wire_bytes: wire.len(),
        wire_sha256: sha256_hex(&wire),
        plaintext_sha256: sha256_hex(&plaintext),
        checks,
    })
}

fn native_conformance() -> Result<AesConformanceReport, String> {
    let rustcrypto = conformance_backend_report(RUSTCRYPTO_BACKEND, false)?;
    let ring = conformance_backend_report(RING_BACKEND, true)?;
    if rustcrypto.wire_bytes != ring.wire_bytes
        || rustcrypto.wire_sha256 != ring.wire_sha256
        || rustcrypto.plaintext_sha256 != ring.plaintext_sha256
    {
        return Err("Native AES conformance wire or plaintext drift detected".to_string());
    }
    Ok(AesConformanceReport {
        vector_id: "aes-gcm-phase0-fixed-v1".to_string(),
        input_bytes: TEST_VECTOR_INPUT.len(),
        input_sha256: sha256_hex(TEST_VECTOR_INPUT),
        rustcrypto,
        ring: Some(ring),
        webcrypto: None,
    })
}

fn aes_backend_report(
    backend: &str,
    compressed: &[u8],
    id: &str,
    ring: bool,
    run_key: &[u8; 32],
) -> Result<AesBackendReport, String> {
    let mut last_nonce = [0_u8; 12];
    let mut last_aad = Vec::new();
    let (performance_wire, encrypt_timing) = measure(compressed.len(), |iteration| {
        let domain = format!("native/{backend}/encrypt/{iteration}");
        let (key, nonce, aad) = campaign_material(run_key, id, &domain);
        last_nonce = nonce;
        last_aad = aad.clone();
        encrypt_for_backend(ring, compressed, &key, &nonce, &aad)
    })?;
    let (plaintext, decrypt_timing) = measure(compressed.len(), |_| {
        decrypt_for_backend(ring, &performance_wire, run_key, &last_nonce, &last_aad)
    })?;
    let mut wrong_key = *run_key;
    wrong_key[0] ^= 1;
    let wrong_key_rejected =
        decrypt_for_backend(ring, &performance_wire, &wrong_key, &last_nonce, &last_aad).is_err();
    let mut wrong_aad = last_aad.clone();
    wrong_aad.push(1);
    let wrong_aad_rejected =
        decrypt_for_backend(ring, &performance_wire, run_key, &last_nonce, &wrong_aad).is_err();
    let mut tampered = performance_wire.clone();
    tampered[0] ^= 1;
    let tamper_rejected =
        decrypt_for_backend(ring, &tampered, run_key, &last_nonce, &last_aad).is_err();
    let truncation_rejected = decrypt_for_backend(
        ring,
        &performance_wire[..performance_wire.len() - 1],
        run_key,
        &last_nonce,
        &last_aad,
    )
    .is_err();
    let checks = AesChecks {
        round_trip: plaintext == compressed,
        wrong_key_rejected,
        wrong_aad_rejected,
        tamper_rejected,
        truncation_rejected,
    };
    if !checks.round_trip
        || !checks.wrong_key_rejected
        || !checks.wrong_aad_rejected
        || !checks.tamper_rejected
        || !checks.truncation_rejected
    {
        return Err(format!("AES boundary check failed for {backend}"));
    }
    let expected_wire_bytes = compressed
        .len()
        .checked_add(AES_TAG_BYTES)
        .ok_or_else(|| "AES wire size overflow".to_string())?;
    if performance_wire.len() != expected_wire_bytes {
        return Err(format!("AES wire size failed for {backend}"));
    }
    Ok(AesBackendReport {
        backend: backend.to_string(),
        wire_bytes: performance_wire.len(),
        wire_sha256: sha256_hex(&performance_wire),
        plaintext_sha256: sha256_hex(&plaintext),
        encrypt: encrypt_timing,
        decrypt: decrypt_timing,
        checks,
    })
}

fn zstd_backend_report(
    backend: &str,
    frame: &[u8],
    input: &[u8],
    ruzstd: bool,
) -> Result<ZstdBackendReport, String> {
    let decode = |_| {
        if ruzstd {
            zstd_decode_ruzstd(frame, input.len())
        } else {
            zstd_decode_baseline(frame, input.len())
        }
    };
    let (decoded, decode_timing) = measure(input.len(), decode)?;
    let size_limit_rejected = if ruzstd {
        zstd_decode_ruzstd(frame, input.len().saturating_sub(1)).is_err()
    } else {
        zstd_decode_baseline(frame, input.len().saturating_sub(1)).is_err()
    };
    let truncation_rejected = if ruzstd {
        zstd_decode_ruzstd(&frame[..frame.len().saturating_sub(1)], input.len()).is_err()
    } else {
        zstd_decode_baseline(&frame[..frame.len().saturating_sub(1)], input.len()).is_err()
    };
    let checks = ZstdChecks {
        round_trip: decoded == input,
        size_limit_rejected,
        truncation_rejected,
    };
    if !checks.round_trip || !checks.size_limit_rejected || !checks.truncation_rejected {
        return Err(format!("Zstandard boundary check failed for {backend}"));
    }
    Ok(ZstdBackendReport {
        backend: backend.to_string(),
        decoded_bytes: decoded.len(),
        decoded_sha256: sha256_hex(&decoded),
        decode: decode_timing,
        checks,
    })
}

fn case_directory(run_dir: &Path, id: &str) -> Result<PathBuf, String> {
    if !safe_id(id) {
        return Err(format!("unsafe case id: {id}"));
    }
    let root =
        fs::canonicalize(run_dir).map_err(|error| format!("canonicalize run dir: {error}"))?;
    let cases = root.join("cases");
    fs::create_dir_all(&cases).map_err(|error| format!("create case directory: {error}"))?;
    let candidate = cases.join(id);
    fs::create_dir_all(&candidate).map_err(|error| format!("create case directory: {error}"))?;
    let resolved = fs::canonicalize(&candidate)
        .map_err(|error| format!("canonicalize case directory: {error}"))?;
    if !resolved.starts_with(&root) {
        return Err("case directory escaped run directory".to_string());
    }
    Ok(resolved)
}

fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "output has no valid filename".to_string())?;
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = File::create(&tmp).map_err(|error| format!("create output: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("write output: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync output: {error}"))?;
        fs::rename(&tmp, path).map_err(|error| format!("publish output: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn measure_case(
    case: &CampaignCase,
    run_dir: &Path,
    run_key: &[u8; 32],
) -> Result<CaseReport, String> {
    let input = fs::read(&case.path).map_err(|error| format!("read {}: {error}", case.path))?;
    if input.is_empty() {
        return Err("input is empty".to_string());
    }
    let frame = zstd_encode(&input)?;
    let directory = case_directory(run_dir, &case.id)?;
    atomic_bytes(&directory.join("frame.zst"), &frame)?;
    let rustcrypto = aes_backend_report(RUSTCRYPTO_BACKEND, &frame, &case.id, false, run_key)?;
    let ring = aes_backend_report(RING_BACKEND, &frame, &case.id, true, run_key)?;
    let libzstd = zstd_backend_report("zstd 0.13.3 / libzstd 1.5.7", &frame, &input, false)?;
    let ruzstd = zstd_backend_report("ruzstd 0.8.3", &frame, &input, true)?;
    let aes = AesReport {
        rustcrypto,
        ring: Some(ring),
        webcrypto: None,
    };
    let zstd = ZstdReport {
        decoded_equal: libzstd.decoded_sha256 == ruzstd.decoded_sha256,
        libzstd,
        ruzstd,
    };
    if !zstd.decoded_equal {
        return Err("backend output drift detected".to_string());
    }
    Ok(CaseReport {
        id: case.id.clone(),
        kind: case.kind.clone(),
        size_class: case.size_class.clone(),
        path_label: case.path_label.clone(),
        input_bytes: input.len(),
        input_sha256: sha256_hex(&input),
        compressed_bytes: frame.len(),
        compressed_sha256: sha256_hex(&frame),
        aes,
        zstd,
    })
}

fn run(args: &[String]) -> Result<(), String> {
    let manifest_path = PathBuf::from(arg(args, "--manifest")?);
    let lock_path = PathBuf::from(arg(args, "--cargo-lock")?);
    let run_dir = PathBuf::from(arg(args, "--run-dir")?);
    let output_path = PathBuf::from(arg(args, "--output")?);
    fs::create_dir_all(&run_dir).map_err(|error| format!("create run directory: {error}"))?;
    let manifest_bytes =
        fs::read(&manifest_path).map_err(|error| format!("read manifest: {error}"))?;
    let manifest_hash = sha256_hex(&manifest_bytes);
    let lock_hash = hex_sha256_file(&lock_path)?;
    let campaign: Campaign = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("parse manifest: {error}"))?;
    validate_campaign(&campaign)?;
    let context = context(args, &manifest_hash, &lock_hash)?;
    let run_key = campaign_key()?;
    let aes_conformance = native_conformance()?;
    let mut cases = Vec::with_capacity(campaign.cases.len());
    for case in &campaign.cases {
        cases.push(
            measure_case(case, &run_dir, &run_key)
                .map_err(|error| format!("case {} failed: {error}", case.id))?,
        );
    }
    let input_bytes = cases.iter().map(|case| case.input_bytes).sum();
    let report = RunReport {
        schema: REPORT_SCHEMA.to_string(),
        lane: "native".to_string(),
        campaign_schema: campaign.schema,
        description: campaign.description,
        status: "ok".to_string(),
        run_id: context.run_id,
        measured_at_utc: context.measured_at_utc,
        environment: EnvironmentReport {
            platform: format!("{}-{}", env::consts::OS, env::consts::ARCH),
            rustc: rustc_version(),
            native_runner: "package-backends-native 0.1.0".to_string(),
            native_binary_bytes: env::current_exe()
                .ok()
                .and_then(|path| fs::metadata(path).ok())
                .map_or(0, |metadata| metadata.len()),
            campaign_manifest_sha256: context.manifest_sha256,
            cargo_lock_sha256: context.lock_sha256,
            wasm_pack: context.wasm_pack,
            harness_source_digest: context.source_digest,
            msrv_statement: format!("MSRV 1.87 build not verified on this host; measured with {}", rustc_version()),
        },
        config: ConfigReport {
            aes_wire: "AES-256-GCM fixed profile (ciphertext || 16-byte tag)".to_string(),
            aes_reference: "AES-256-GCM fixed profile (ciphertext || 16-byte tag)".to_string(),
            aes_candidate: "Native RustCrypto/ring or WASM RustCrypto/Node WebCrypto".to_string(),
            zstd_encoder: "zstd crate 0.13.3 / libzstd 1.5.7, level 3 (one frame per case)".to_string(),
            zstd_level: ZSTD_LEVEL,
            zstd_reference_decoder: "zstd crate 0.13.3 / libzstd 1.5.7".to_string(),
            zstd_candidate_decoder: "ruzstd 0.8.3".to_string(),
            zstd_frame_policy: "single frame; declared content size; no dictionary/checksum; window <= 64 MiB; decoded <= 128 MiB".to_string(),
            max_window_bytes: 1_u64 << MAX_WINDOW_LOG,
            max_decoded_bytes: package_backends_core::MAX_DECODED_BYTES,
            vector_policy: "campaign uses a fresh run-scoped 32-byte key from an ephemeral environment value; every warmup/repeat encryption has a backend/domain/iteration-unique nonce and AAD; one fixed public key/nonce/AAD vector is conformance-only and excluded from campaign performance; no secrets serialized".to_string(),
            timing_policy: "one warmup plus five measured iterations; OS/file caches uncontrolled; p50/p95 are directional".to_string(),
        },
        aes_conformance,
        cases,
        failures: Vec::new(),
        observable_copies: CopyObservation {
            input_calls: campaign.cases.len(),
            input_bytes,
            js_wasm_copy_count: None,
            js_wasm_copy_bytes: None,
            note: "Native file reads are counted; heap/RSS/copy peaks are unavailable".to_string(),
        },
    };
    let bytes =
        serde_json::to_vec_pretty(&report).map_err(|error| format!("serialize report: {error}"))?;
    atomic_bytes(&output_path, &bytes)?;
    println!(
        "native backend lane complete: cases={} output={}",
        report.cases.len(),
        output_path.display()
    );
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if let Err(error) = run(&args) {
        eprintln!("package-backends-native: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campaign_validation_rejects_unsafe_ids_and_invalid_size() {
        let case = |id: &str| CampaignCase {
            id: id.to_string(),
            kind: "pmx".to_string(),
            size_class: "small".to_string(),
            path_label: "fixture/a.pmx".to_string(),
            path: "fixture/a.pmx".to_string(),
        };
        let mut campaign = Campaign {
            schema: "mmdpack-phase0-campaign/1".to_string(),
            description: "fixture".to_string(),
            cases: (0..10).map(|i| case(&format!("case-{i}"))).collect(),
        };
        campaign.cases[0] = case("../escape");
        assert!(validate_campaign(&campaign).is_err());
        campaign.cases[0] = case("case-0");
        campaign.cases[1] = case("case-0");
        assert!(validate_campaign(&campaign).is_err());
    }

    #[test]
    fn campaign_validation_rejects_empty_and_unsupported_cases() {
        let mut campaign = Campaign {
            schema: "mmdpack-phase0-campaign/1".to_string(),
            description: "fixture".to_string(),
            cases: Vec::new(),
        };
        assert!(validate_campaign(&campaign).is_err());
        let case = CampaignCase {
            id: "case-0".to_string(),
            kind: "txt".to_string(),
            size_class: "small".to_string(),
            path_label: "fixture/a".to_string(),
            path: "fixture/a".to_string(),
        };
        campaign.cases = (0..10)
            .map(|index| CampaignCase {
                id: format!("case-{index}"),
                ..case.clone()
            })
            .collect();
        assert!(validate_campaign(&campaign).is_err());
        campaign.cases[0].kind = "pmx".to_string();
        campaign.cases[0].size_class = "huge".to_string();
        assert!(validate_campaign(&campaign).is_err());
    }

    #[test]
    fn case_directory_rejects_traversal_and_stays_contained() {
        let root =
            std::env::temp_dir().join(format!("package-backends-case-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        assert!(case_directory(&root, "../escape").is_err());
        let resolved = case_directory(&root, "safe-case").unwrap();
        assert!(resolved.starts_with(fs::canonicalize(&root).unwrap()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn atomic_output_replaces_only_complete_file() {
        let root =
            std::env::temp_dir().join(format!("package-backends-atomic-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("report.json");
        atomic_bytes(&output, b"first").unwrap();
        atomic_bytes(&output, b"second").unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"second");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn timing_has_warmup_and_distribution() {
        let (_, timing) = measure(1024, |_| Ok::<_, String>(vec![1_u8; 1024])).unwrap();
        assert_eq!(timing.warmup, 1);
        assert_eq!(timing.repeats, 5);
        assert_eq!(timing.samples_ms.len(), 5);
        assert!(timing.p95_ms >= timing.p50_ms);
    }

    #[test]
    fn conformance_vector_has_native_wire_parity_and_boundaries() {
        let report = native_conformance().unwrap();
        let ring = report.ring.as_ref().unwrap();
        assert_eq!(report.rustcrypto.wire_bytes, ring.wire_bytes);
        assert_eq!(report.rustcrypto.wire_sha256, ring.wire_sha256);
        assert_eq!(report.rustcrypto.plaintext_sha256, report.input_sha256);
        assert!(report.rustcrypto.checks.wrong_aad_rejected);
        assert!(ring.checks.wrong_aad_rejected);
    }
}
