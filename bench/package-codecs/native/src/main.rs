use package_codecs_core::{
    check_boundaries, compress, decompress, decrypt, encrypt, BoundaryChecks, AES_TAG_BYTES,
    ZSTD_LEVEL,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
    time::{SystemTime, UNIX_EPOCH},
};

const REPORT_SCHEMA: &str = "mmdpack-phase0-benchmark/1";
const AES_BACKEND: &str = "RustCrypto aes-gcm 0.10.3 (AES-256-GCM)";
const ZSTD_BACKEND: &str = "zstd crate 0.13.3 / zstd 1.5.7";

#[derive(Debug, Deserialize)]
struct Campaign {
    schema: String,
    description: String,
    cases: Vec<CampaignCase>,
}

#[derive(Debug, Deserialize)]
struct CampaignCase {
    id: String,
    kind: String,
    size_class: String,
    path_label: String,
    path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TimingReport {
    pub compress_ms: f64,
    pub encrypt_ms: f64,
    pub decrypt_ms: f64,
    pub decompress_ms: f64,
    pub pipeline_ms: f64,
    pub pipeline_mib_per_s: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CaseReport {
    pub id: String,
    pub kind: String,
    pub size_class: String,
    pub path_label: String,
    pub input_bytes: usize,
    pub compressed_bytes: usize,
    pub ciphertext_bytes: usize,
    pub compressed_ratio: f64,
    pub sha256: String,
    pub timings: TimingReport,
    pub checks: BoundaryChecks,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunReport {
    pub schema: String,
    pub campaign_schema: String,
    pub description: String,
    pub run_id: String,
    pub measured_at_utc: String,
    pub environment: EnvironmentReport,
    pub config: ConfigReport,
    pub cases: Vec<CaseReport>,
    pub failures: Vec<FailureReport>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvironmentReport {
    pub platform: String,
    pub rustc: String,
    pub wasm_runner: String,
    pub native_runner: String,
    pub campaign_manifest_sha256: String,
    pub cargo_lock_sha256: String,
    pub wasm_pack: String,
    pub harness_source_digest: String,
    pub msrv_statement: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ConfigReport {
    pub compression: String,
    pub zstd_level: i32,
    pub encryption: String,
    pub key_bytes: usize,
    pub nonce_bytes: usize,
    pub tag_bytes: usize,
    pub aad: String,
    pub key_policy: String,
    pub timing_policy: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FailureReport {
    pub id: String,
    pub path_label: String,
    pub error: String,
}

#[derive(Debug, Clone)]
struct RunContext {
    run_id: String,
    measured_at_utc: String,
    campaign_manifest_sha256: String,
    cargo_lock_sha256: String,
    wasm_pack: String,
    harness_source_digest: String,
}

fn usage() {
    eprintln!(
        "usage: package-codecs-native --manifest PATH --raw-output PATH [metadata options]\n       package-codecs-native --render-only --native-json PATH --wasm-json PATH --report PATH"
    );
}

fn arg_value(args: &[String], name: &str, default: Option<&str>) -> Result<PathBuf, String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| PathBuf::from(&pair[1]))
        .or_else(|| default.map(PathBuf::from))
        .ok_or_else(|| format!("missing {name}"))
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

fn platform() -> String {
    format!("{}-{}", env::consts::OS, env::consts::ARCH)
}

fn default_measured_at() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("unix-seconds:{seconds}")
}

fn sha256_hex(input: &[u8]) -> String {
    Sha256::digest(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_campaign(campaign: &Campaign) -> Result<(), String> {
    if campaign.schema != "mmdpack-phase0-campaign/1" {
        return Err(format!("unsupported campaign schema: {}", campaign.schema));
    }
    if campaign.description.trim().is_empty() {
        return Err("campaign description must not be empty".to_string());
    }
    if !(10..=20).contains(&campaign.cases.len()) {
        return Err(format!(
            "campaign must contain 10-20 cases, got {}",
            campaign.cases.len()
        ));
    }
    let mut ids = std::collections::HashSet::with_capacity(campaign.cases.len());
    for case in &campaign.cases {
        if case.id.trim().is_empty() {
            return Err("campaign case id must not be empty".to_string());
        }
        if !ids.insert(case.id.clone()) {
            return Err(format!("duplicate campaign case id: {}", case.id));
        }
        if !matches!(case.kind.as_str(), "pmx" | "vmd") {
            return Err(format!(
                "unsupported case kind for {}: {}",
                case.id, case.kind
            ));
        }
        if !matches!(case.size_class.as_str(), "small" | "medium" | "large") {
            return Err(format!(
                "unsupported size_class for {}: {}",
                case.id, case.size_class
            ));
        }
        if case.path.trim().is_empty() || case.path_label.trim().is_empty() {
            return Err(format!("path and path_label are required for {}", case.id));
        }
    }
    Ok(())
}

fn arg_text(args: &[String], name: &str) -> Option<String> {
    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn expected_hash(args: &[String], name: &str, actual: &str) -> Result<String, String> {
    let expected = arg_text(args, name).ok_or_else(|| format!("missing {name}"))?;
    if expected != actual {
        return Err(format!("{name} does not match bytes read for this run"));
    }
    Ok(actual.to_string())
}

fn context_from_args(
    args: &[String],
    manifest_hash: &str,
    cargo_lock_hash: &str,
) -> Result<RunContext, String> {
    Ok(RunContext {
        run_id: arg_value(args, "--run-id", Some("manual-local"))?
            .to_string_lossy()
            .into_owned(),
        measured_at_utc: arg_value(args, "--measured-at", None)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|_| default_measured_at()),
        campaign_manifest_sha256: expected_hash(args, "--manifest-sha256", manifest_hash)?,
        cargo_lock_sha256: expected_hash(args, "--cargo-lock-sha256", cargo_lock_hash)?,
        wasm_pack: args
            .windows(2)
            .find(|pair| pair[0] == "--wasm-pack")
            .map(|pair| pair[1].clone())
            .unwrap_or_else(|| "not-provided".to_string()),
        harness_source_digest: args
            .windows(2)
            .find(|pair| pair[0] == "--harness-source-digest")
            .map(|pair| pair[1].clone())
            .unwrap_or_else(|| "untracked local harness; digest not supplied".to_string()),
    })
}

fn mib_per_second(bytes: usize, elapsed_ms: f64) -> f64 {
    if elapsed_ms <= 0.0 {
        return 0.0;
    }
    bytes as f64 / (1024.0 * 1024.0) / (elapsed_ms / 1000.0)
}

fn measure_case(case: &CampaignCase) -> Result<CaseReport, String> {
    let input = fs::read(&case.path).map_err(|error| format!("read {}: {error}", case.path))?;
    let aad = format!("mmdpack-phase0/{}", case.id);
    let pipeline_start = Instant::now();

    let start = Instant::now();
    let compressed = compress(&input)?;
    let compress_ms = start.elapsed().as_secs_f64() * 1000.0;

    let start = Instant::now();
    let encrypted = encrypt(compressed, aad.as_bytes())?;
    let encrypt_ms = start.elapsed().as_secs_f64() * 1000.0;

    let start = Instant::now();
    let decrypted = decrypt(&encrypted, aad.as_bytes())?;
    let decrypt_ms = start.elapsed().as_secs_f64() * 1000.0;

    let start = Instant::now();
    let restored = decompress(&decrypted, input.len())?;
    let decompress_ms = start.elapsed().as_secs_f64() * 1000.0;
    let pipeline_ms = pipeline_start.elapsed().as_secs_f64() * 1000.0;

    let mut checks = check_boundaries(&encrypted, aad.as_bytes());
    checks.round_trip = restored == input;
    if !checks.round_trip {
        return Err("decompressed bytes differ from input".to_string());
    }
    if !(checks.wrong_key_rejected && checks.tamper_rejected && checks.truncation_rejected) {
        return Err(format!("boundary check failed: {checks:?}"));
    }
    let compressed_bytes = encrypted.ciphertext().len() - AES_TAG_BYTES;
    Ok(CaseReport {
        id: case.id.clone(),
        kind: case.kind.clone(),
        size_class: case.size_class.clone(),
        path_label: case.path_label.clone(),
        input_bytes: input.len(),
        compressed_bytes,
        ciphertext_bytes: encrypted.ciphertext().len(),
        compressed_ratio: compressed_bytes as f64 / input.len().max(1) as f64,
        sha256: sha256_hex(&input),
        timings: TimingReport {
            compress_ms,
            encrypt_ms,
            decrypt_ms,
            decompress_ms,
            pipeline_ms,
            pipeline_mib_per_s: mib_per_second(input.len(), pipeline_ms),
        },
        checks,
    })
}

fn run_native(campaign: &Campaign, context: &RunContext) -> RunReport {
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    for case in &campaign.cases {
        match measure_case(case) {
            Ok(report) => cases.push(report),
            Err(error) => failures.push(FailureReport {
                id: case.id.clone(),
                path_label: case.path_label.clone(),
                error,
            }),
        }
    }
    RunReport {
        schema: REPORT_SCHEMA.to_string(),
        campaign_schema: campaign.schema.clone(),
        description: campaign.description.clone(),
        run_id: context.run_id.clone(),
        measured_at_utc: context.measured_at_utc.clone(),
        environment: EnvironmentReport {
            platform: platform(),
            rustc: rustc_version(),
            wasm_runner: "not-run (native JSON lane)".to_string(),
            native_runner: "package-codecs-native 0.1.0".to_string(),
            campaign_manifest_sha256: context.campaign_manifest_sha256.clone(),
            cargo_lock_sha256: context.cargo_lock_sha256.clone(),
            wasm_pack: context.wasm_pack.clone(),
            harness_source_digest: context.harness_source_digest.clone(),
            msrv_statement: format!(
                "MSRV 1.87 build not verified on this host; measured with {}",
                rustc_version()
            ),
        },
        config: ConfigReport {
            compression: ZSTD_BACKEND.to_string(),
            zstd_level: ZSTD_LEVEL,
            encryption: AES_BACKEND.to_string(),
            key_bytes: 32,
            nonce_bytes: 12,
            tag_bytes: AES_TAG_BYTES,
            aad: "mmdpack-phase0/<case id>".to_string(),
            key_policy: "fresh random key and nonce per encrypt call; never serialized".to_string(),
            timing_policy: "single measured iteration; OS/file caches uncontrolled; no warmup or repeat; timings are directional".to_string(),
        },
        cases,
        failures,
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(value).map_err(|error| format!("serialize: {error}"))?;
    atomic_write(path, &json)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("output path has no valid file name: {}", path.display()))?;
    let temporary = path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = File::create(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        file.write_all(bytes)
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("flush {}: {error}", temporary.display()))?;
        fs::rename(&temporary, path).map_err(|error| format!("publish {}: {error}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?,
    )
    .map_err(|error| format!("parse {}: {error}", path.display()))
}

pub fn validate_comparable(native: &RunReport, wasm: &RunReport) -> Result<(), String> {
    if native.schema != wasm.schema {
        return Err("Native/WASM report schema mismatch".to_string());
    }
    if native.campaign_schema != wasm.campaign_schema {
        return Err("Native/WASM campaign schema mismatch".to_string());
    }
    if native.run_id != wasm.run_id || native.measured_at_utc != wasm.measured_at_utc {
        return Err("Native/WASM run metadata mismatch".to_string());
    }
    if native.environment.campaign_manifest_sha256 != wasm.environment.campaign_manifest_sha256
        || native.environment.cargo_lock_sha256 != wasm.environment.cargo_lock_sha256
        || native.environment.harness_source_digest != wasm.environment.harness_source_digest
    {
        return Err("Native/WASM reproducibility metadata mismatch".to_string());
    }
    if native.config != wasm.config {
        return Err("Native/WASM codec/security configuration mismatch".to_string());
    }
    if !native.failures.is_empty() || !wasm.failures.is_empty() {
        return Err("Native/WASM campaign contains failures".to_string());
    }
    if native.cases.len() != wasm.cases.len() {
        return Err("Native/WASM case count mismatch".to_string());
    }
    for (index, (native_case, wasm_case)) in native.cases.iter().zip(&wasm.cases).enumerate() {
        if native_case.id != wasm_case.id {
            return Err(format!("Native/WASM case order mismatch at index {index}"));
        }
        if native_case.sha256 != wasm_case.sha256
            || native_case.input_bytes != wasm_case.input_bytes
            || native_case.compressed_bytes != wasm_case.compressed_bytes
            || native_case.ciphertext_bytes != wasm_case.ciphertext_bytes
        {
            return Err(format!(
                "Native/WASM case size/hash mismatch: {}",
                native_case.id
            ));
        }
        if !all_checks_pass(&native_case.checks) || !all_checks_pass(&wasm_case.checks) {
            return Err(format!(
                "Native/WASM boundary check failed: {}",
                native_case.id
            ));
        }
    }
    Ok(())
}

fn all_checks_pass(checks: &BoundaryChecks) -> bool {
    checks.round_trip
        && checks.wrong_key_rejected
        && checks.tamper_rejected
        && checks.truncation_rejected
}

fn escaped_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn shared_label(value: &str) -> String {
    let is_windows_absolute = value.as_bytes().get(1) == Some(&b':');
    if is_windows_absolute || value.starts_with('/') || value.starts_with('\\') {
        "<local asset>".to_string()
    } else {
        escaped_cell(value)
    }
}

pub fn render_markdown(native: &RunReport, wasm: Option<&RunReport>) -> String {
    let mut markdown = String::new();
    markdown.push_str("# MMDPACK Phase 0: compression and authenticated-encryption benchmark\n\n");
    markdown.push_str("> Measurement evidence only. The `.mmdpack` V1 format and backend choices remain unfrozen.\n\n");
    markdown.push_str("## Scope and configuration\n\n");
    markdown.push_str(&format!("{}\n\n", shared_label(&native.description)));
    markdown.push_str("- Pipeline: `input -> Zstandard -> AES-256-GCM -> AES-256-GCM decrypt -> Zstandard decompress`\n");
    markdown.push_str(&format!(
        "- Compression: `{}` at level `{}`\n",
        native.config.compression, native.config.zstd_level
    ));
    markdown.push_str(&format!(
        "- Encryption: `{}`; key={} bytes, nonce={} bytes, tag={} bytes\n",
        native.config.encryption,
        native.config.key_bytes,
        native.config.nonce_bytes,
        native.config.tag_bytes
    ));
    markdown.push_str(&format!("- AAD: `{}`\n", native.config.aad));
    markdown.push_str(&format!("- Key policy: {}\n", native.config.key_policy));
    markdown.push_str(&format!(
        "- Timing policy: {}\n\n",
        native.config.timing_policy
    ));
    markdown.push_str(&format!("- Run ID: `{}`\n", shared_label(&native.run_id)));
    markdown.push_str(&format!(
        "- Measured at (UTC): `{}`\n",
        shared_label(&native.measured_at_utc)
    ));
    markdown.push_str(&format!(
        "- Campaign manifest SHA-256: `{}`\n",
        native.environment.campaign_manifest_sha256
    ));
    markdown.push_str(&format!(
        "- Standalone Cargo.lock SHA-256: `{}`\n",
        native.environment.cargo_lock_sha256
    ));
    markdown.push_str(&format!(
        "- wasm-pack: `{}`\n",
        shared_label(&native.environment.wasm_pack)
    ));
    markdown.push_str(&format!(
        "- Harness source digest: `{}`\n",
        native.environment.harness_source_digest
    ));
    markdown.push_str(&format!(
        "- MSRV: {}\n\n",
        native.environment.msrv_statement
    ));
    markdown.push_str("## Reproducibility metadata\n\n");
    markdown.push_str("| Lane | Platform | Toolchain | Runner |\n|---|---|---|---|\n");
    markdown.push_str(&format!(
        "| Native | `{}` | `{}` | `{}` |\n",
        native.environment.platform, native.environment.rustc, native.environment.native_runner
    ));
    if let Some(wasm) = wasm {
        markdown.push_str(&format!(
            "| WASM/Node | `{}` | `{}` | `{}` |\n",
            wasm.environment.platform, wasm.environment.rustc, wasm.environment.wasm_runner
        ));
    } else {
        markdown.push_str("| WASM/Node | not run | not run | not run |\n");
    }
    markdown.push_str("\nGenerated raw JSON is local-only under `.ai/mmdpack/`; cryptographic keys/nonces and asset payloads are never written.\n\n");
    markdown.push_str("## Native results\n\n");
    render_results(&mut markdown, native);
    if let Some(wasm) = wasm {
        markdown.push_str("## WASM/Node results\n\n");
        render_results(&mut markdown, wasm);
    }
    markdown.push_str("## Memory and copy observations\n\n");
    markdown.push_str("- Native RSS, WASM linear-memory peak, JS heap peak, and largest allocation were not measured by this small harness; no unsupported values are claimed.\n");
    markdown.push_str("- JS↔WASM copy counts and byte totals were not instrumented; the Node lane passes one `Uint8Array` per case and records this as an observation, not a measurement.\n");
    markdown.push_str("- Package-layer buffer lifetime and `loadPackage()`/`openPackage()` behavior are outside this Phase 0 codec probe.\n\n");
    markdown.push_str("## Bounded conclusion\n\n");
    markdown.push_str("This campaign validates the draft operation order and fail-closed authentication boundaries on the selected cases. It is insufficient to freeze a V1 backend or memory gate. Native/WASM adoption should remain provisional until a larger, separately reviewed campaign compares alternative backends and records reliable memory data.\n");
    markdown
}

fn render_results(markdown: &mut String, report: &RunReport) {
    markdown.push_str("| ID | Kind | Class | Input | Zstd | Ratio | Ciphertext | SHA-256 | Compress ms | Encrypt ms | Decrypt ms | Decompress ms | Pipeline ms | MiB/s | Round-trip | Wrong key | Tamper | Truncation |\n");
    markdown.push_str(
        "|---|---|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---|---|---|---|\n",
    );
    for case in &report.cases {
        markdown.push_str(&format!(
            "| `{}` | {} | {} | {} | {} | {:.4} | {} | `{}` | {:.3} | {:.3} | {:.3} | {:.3} | {:.3} | {:.2} | {} | {} | {} | {} |\n",
            shared_label(&case.id), case.kind, case.size_class, case.input_bytes,
            case.compressed_bytes, case.compressed_ratio, case.ciphertext_bytes,
            case.sha256, case.timings.compress_ms, case.timings.encrypt_ms,
            case.timings.decrypt_ms, case.timings.decompress_ms,
            case.timings.pipeline_ms, case.timings.pipeline_mib_per_s,
            case.checks.round_trip, case.checks.wrong_key_rejected,
            case.checks.tamper_rejected, case.checks.truncation_rejected
        ));
    }
    if !report.failures.is_empty() {
        markdown.push_str("\nFailures/skips:\n\n");
        for failure in &report.failures {
            markdown.push_str(&format!(
                "- `{}` ({}): measurement failed; inspect ignored raw JSON\n",
                shared_label(&failure.id),
                shared_label(&failure.path_label)
            ));
        }
    } else {
        markdown.push_str("Failures/skips: none.\n");
    }
    markdown.push('\n');
}

fn main() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        usage();
        return Ok(());
    }
    let render_only = args.iter().any(|arg| arg == "--render-only");
    if render_only {
        let native_path = arg_value(&args, "--native-json", None)?;
        let wasm_path = arg_value(&args, "--wasm-json", None)?;
        let report_path = arg_value(&args, "--report", None)?;
        let native: RunReport = read_json(&native_path)?;
        let wasm: RunReport = read_json(&wasm_path)?;
        validate_comparable(&native, &wasm)?;
        atomic_write(
            &report_path,
            render_markdown(&native, Some(&wasm)).as_bytes(),
        )?;
        println!("report published candidate={}", report_path.display());
        return Ok(());
    }

    let manifest_path = arg_value(
        &args,
        "--manifest",
        Some("../../../.ai/mmdpack/campaign.json"),
    )?;
    let raw_path = arg_value(
        &args,
        "--raw-output",
        Some("../../../.ai/mmdpack/native.json"),
    )?;
    let manifest_bytes = fs::read(&manifest_path)
        .map_err(|error| format!("read manifest {}: {error}", manifest_path.display()))?;
    let manifest_hash = sha256_hex(&manifest_bytes);
    let lock_path = arg_value(&args, "--cargo-lock", Some("Cargo.lock"))?;
    let lock_bytes = fs::read(&lock_path)
        .map_err(|error| format!("read lock {}: {error}", lock_path.display()))?;
    let lock_hash = sha256_hex(&lock_bytes);
    expected_hash(&args, "--manifest-sha256", &manifest_hash)?;
    expected_hash(&args, "--cargo-lock-sha256", &lock_hash)?;
    let campaign: Campaign = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("parse manifest {}: {error}", manifest_path.display()))?;
    validate_campaign(&campaign)?;
    let context = context_from_args(&args, &manifest_hash, &lock_hash)?;
    let native = run_native(&campaign, &context);
    write_json(&raw_path, &native)?;
    if !native.failures.is_empty() {
        return Err(format!(
            "campaign measurement failed for {} case(s); report not published",
            native.failures.len()
        ));
    }
    println!(
        "native cases={} failures={} raw={}",
        native.cases.len(),
        native.failures.len(),
        raw_path.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_campaign() -> Campaign {
        Campaign {
            schema: "mmdpack-phase0-campaign/1".to_string(),
            description: "fixture campaign".to_string(),
            cases: (0..10)
                .map(|index| CampaignCase {
                    id: format!("case-{index}"),
                    kind: if index < 5 { "pmx" } else { "vmd" }.to_string(),
                    size_class: match index % 3 {
                        0 => "small",
                        1 => "medium",
                        _ => "large",
                    }
                    .to_string(),
                    path_label: format!("fixture/{index}"),
                    path: format!("fixture/{index}"),
                })
                .collect(),
        }
    }

    fn fixture() -> RunReport {
        RunReport {
            schema: REPORT_SCHEMA.to_string(),
            campaign_schema: "phase0/1".to_string(),
            description: "deterministic fixture".to_string(),
            run_id: "fixture-run".to_string(),
            measured_at_utc: "fixture-time".to_string(),
            environment: EnvironmentReport {
                platform: "test".to_string(),
                rustc: "test".to_string(),
                wasm_runner: "test".to_string(),
                native_runner: "test".to_string(),
                campaign_manifest_sha256: "manifest".to_string(),
                cargo_lock_sha256: "lock".to_string(),
                wasm_pack: "wasm-pack".to_string(),
                harness_source_digest: "source".to_string(),
                msrv_statement: "msrv".to_string(),
            },
            config: ConfigReport {
                compression: ZSTD_BACKEND.to_string(),
                zstd_level: 3,
                encryption: AES_BACKEND.to_string(),
                key_bytes: 32,
                nonce_bytes: 12,
                tag_bytes: 16,
                aad: "test".to_string(),
                key_policy: "never serialized".to_string(),
                timing_policy: "fixture".to_string(),
            },
            cases: vec![CaseReport {
                id: "case|one".to_string(),
                kind: "pmx".to_string(),
                size_class: "small".to_string(),
                path_label: "fixture".to_string(),
                input_bytes: 100,
                compressed_bytes: 50,
                ciphertext_bytes: 66,
                compressed_ratio: 0.5,
                sha256: "abc".to_string(),
                timings: TimingReport {
                    compress_ms: 1.0,
                    encrypt_ms: 1.0,
                    decrypt_ms: 1.0,
                    decompress_ms: 1.0,
                    pipeline_ms: 4.0,
                    pipeline_mib_per_s: 0.02,
                },
                checks: BoundaryChecks {
                    round_trip: true,
                    wrong_key_rejected: true,
                    tamper_rejected: true,
                    truncation_rejected: true,
                },
            }],
            failures: Vec::new(),
        }
    }

    #[test]
    fn markdown_render_is_deterministic_and_escapes_cells() {
        let report = fixture();
        assert_eq!(
            render_markdown(&report, None),
            render_markdown(&report, None)
        );
        assert!(render_markdown(&report, None).contains("case\\|one"));
        assert!(render_markdown(&report, None)
            .contains("V1 format and backend choices remain unfrozen"));
    }

    #[test]
    fn campaign_validation_rejects_empty_campaign() {
        let mut campaign = valid_campaign();
        campaign.cases.clear();
        assert!(validate_campaign(&campaign).is_err());
    }

    #[test]
    fn campaign_validation_rejects_duplicate_ids() {
        let mut campaign = valid_campaign();
        campaign.cases[1].id = campaign.cases[0].id.clone();
        assert!(validate_campaign(&campaign)
            .expect_err("duplicate must fail")
            .contains("duplicate"));
    }

    #[test]
    fn campaign_validation_rejects_unsupported_kind_and_size() {
        let mut campaign = valid_campaign();
        campaign.cases[0].kind = "txt".to_string();
        assert!(validate_campaign(&campaign)
            .expect_err("kind must fail")
            .contains("unsupported case kind"));
        let mut campaign = valid_campaign();
        campaign.cases[0].size_class = "huge".to_string();
        assert!(validate_campaign(&campaign)
            .expect_err("size must fail")
            .contains("unsupported size_class"));
    }

    #[test]
    fn failed_case_is_collected_for_nonzero_cli_exit() {
        let campaign = valid_campaign();
        let context = RunContext {
            run_id: "test".to_string(),
            measured_at_utc: "test".to_string(),
            campaign_manifest_sha256: "manifest".to_string(),
            cargo_lock_sha256: "lock".to_string(),
            wasm_pack: "wasm-pack".to_string(),
            harness_source_digest: "source".to_string(),
        };
        let report = run_native(&campaign, &context);
        assert_eq!(report.cases.len(), 0);
        assert_eq!(report.failures.len(), 10);
    }

    #[test]
    fn comparability_rejects_native_wasm_drift() {
        let native = fixture();
        let mut wasm = fixture();
        wasm.cases[0].sha256 = "drift".to_string();
        assert!(validate_comparable(&native, &wasm)
            .expect_err("hash drift must fail")
            .contains("size/hash"));
    }

    #[test]
    fn atomic_write_replaces_complete_output() {
        let directory =
            std::env::temp_dir().join(format!("package-codecs-atomic-{}", std::process::id()));
        fs::create_dir_all(&directory).expect("test directory");
        let output = directory.join("report.md");
        atomic_write(&output, b"first").expect("first publish");
        atomic_write(&output, b"second").expect("replacement publish");
        assert_eq!(fs::read(&output).expect("read output"), b"second");
        assert!(!output
            .with_file_name(format!(".report.md.{}.tmp", std::process::id()))
            .exists());
        fs::remove_dir_all(directory).expect("cleanup test directory");
    }

    #[test]
    fn failure_markdown_does_not_publish_local_paths_or_error_text() {
        let mut report = fixture();
        report.cases.clear();
        report.failures.push(FailureReport {
            id: "bad-case".to_string(),
            path_label: r"F:\MMD\secret.pmx".to_string(),
            error: r"read F:\MMD\secret.pmx: access denied".to_string(),
        });
        let markdown = render_markdown(&report, None);
        assert!(markdown.contains("<local asset>"));
        assert!(!markdown.contains("secret.pmx"));
        assert!(!markdown.contains("access denied"));
    }
}
