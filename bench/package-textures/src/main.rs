use mmdpack_texture_probe::*;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Instant,
};

#[derive(Debug, Serialize, Deserialize)]
struct NativeReport {
    schema: u32,
    lane: String,
    status: String,
    run_id: String,
    manifest_sha256: String,
    lock_sha256: String,
    source_digest: String,
    basisu_sha256: String,
    config: TextureConfig,
    cases: Vec<NativeCase>,
    failures: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct NativeCase {
    id: String,
    source_sha256: String,
    source_bytes: u64,
    width: u32,
    height: u32,
    class: String,
    color_space: String,
    encode_no_zstd_ms: f64,
    encode_zstd_ms: f64,
    candidate_a: Candidate,
    candidate_b: Candidate,
    uastc_levels_equal: bool,
    native_a: NativeDecode,
    native_b: NativeDecode,
    quality: String,
}
#[derive(Debug, Serialize, Deserialize)]
struct Candidate {
    ktx2_bytes: u64,
    ktx2_sha256: String,
    internal_compressed_bytes: u64,
    raw_metadata_bytes: u64,
    raw_blocks_bytes: u64,
    raw_zstd_bytes: u64,
    raw_payload_bytes: u64,
    reconstructed_ktx2_bytes: u64,
    reconstructed_ktx2_sha256: String,
    level_hashes: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
struct NativeDecode {
    ok: bool,
    elapsed_ms: f64,
    output_bytes: u64,
    mip_hashes: Vec<String>,
    mip_dimensions: Vec<[u32; 2]>,
    error: Option<String>,
}
type DdsMetrics = (u64, Vec<String>, Vec<[u32; 2]>);

fn arg(args: &[String], name: &str) -> Result<String, String> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].clone())
        .ok_or_else(|| format!("missing {name}"))
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
fn read_manifest_once(path: &Path) -> Result<(Vec<u8>, Manifest, String), String> {
    let bytes = fs::read(path).map_err(|e| format!("manifest read failed: {e}"))?;
    let hash = sha256_hex(&bytes);
    let manifest =
        serde_json::from_slice(&bytes).map_err(|e| format!("manifest parse failed: {e}"))?;
    Ok((bytes, manifest, hash))
}
fn checked_command(exe: &Path, args: &[String]) -> Result<(String, f64), String> {
    let start = Instant::now();
    let out = Command::new(exe)
        .args(args)
        .output()
        .map_err(|e| format!("external command failed to start: {e}"))?;
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    if !out.status.success() {
        return Err(format!(
            "external command exited {}: {}",
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok((String::from_utf8_lossy(&out.stdout).into_owned(), elapsed))
}
fn dds_observation(path: &Path, width: u32, height: u32) -> Result<DdsMetrics, String> {
    let b = fs::read(path).map_err(|e| e.to_string())?;
    if b.len() < 128 || &b[..4] != b"DDS " {
        return Err("native output is not DDS".into());
    }
    let mip_count = u32::from_le_bytes(b[28..32].try_into().unwrap()).max(1);
    let mut at = 128usize;
    let mut hashes = Vec::new();
    let mut dims = Vec::new();
    for i in 0..mip_count {
        if i >= u32::BITS {
            return Err("DDS mip index exceeds supported shift range".into());
        }
        let w = (width >> i).max(1);
        let h = (height >> i).max(1);
        let len = usize::try_from(w)
            .ok()
            .and_then(|w| usize::try_from(h).ok().and_then(|h| w.checked_mul(h)))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| format!("DDS mip {i} size overflow"))?;
        let end = at
            .checked_add(len)
            .ok_or_else(|| format!("DDS mip {i} range overflow"))?;
        if end > b.len() {
            return Err(format!("DDS mip {i} is truncated"));
        }
        hashes.push(sha256_hex(&b[at..end]));
        dims.push([w, h]);
        at = end;
    }
    Ok((b.len() as u64, hashes, dims))
}
fn native_decode(exe: &Path, input: &Path, out: &Path, width: u32, height: u32) -> NativeDecode {
    let args = vec![
        "-export_dds".into(),
        "RGBA32".into(),
        "-output_file".into(),
        out.display().to_string(),
        input.display().to_string(),
    ];
    match checked_command(exe, &args) {
        Ok((_, ms)) => match dds_observation(out, width, height) {
            Ok((bytes, h, d)) => NativeDecode {
                ok: true,
                elapsed_ms: ms,
                output_bytes: bytes,
                mip_hashes: h,
                mip_dimensions: d,
                error: None,
            },
            Err(e) => NativeDecode {
                ok: false,
                elapsed_ms: ms,
                output_bytes: 0,
                mip_hashes: Vec::new(),
                mip_dimensions: Vec::new(),
                error: Some(e),
            },
        },
        Err(e) => NativeDecode {
            ok: false,
            elapsed_ms: 0.0,
            output_bytes: 0,
            mip_hashes: Vec::new(),
            mip_dimensions: Vec::new(),
            error: Some(e),
        },
    }
}
fn run(args: &[String]) -> Result<(), String> {
    let manifest_path = PathBuf::from(arg(args, "--manifest")?);
    let basis = PathBuf::from(arg(args, "--basisu")?);
    let run_dir = PathBuf::from(arg(args, "--run-dir")?);
    let out = PathBuf::from(arg(args, "--output")?);
    let run_id = arg(args, "--run-id")?;
    let expected_manifest = arg(args, "--expected-manifest-sha256")?;
    let expected_lock = arg(args, "--expected-lock-sha256")?;
    let source_digest = arg(args, "--source-digest")?;
    let (_, manifest, manifest_hash) = read_manifest_once(&manifest_path)?;
    validate_manifest(&manifest)?;
    if manifest_hash != expected_manifest {
        return Err("manifest changed before native lane".into());
    }
    let lock_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock");
    let lock_hash = sha256_hex(&fs::read(&lock_path).map_err(|e| e.to_string())?);
    if lock_hash != expected_lock {
        return Err("Cargo.lock does not match expected hash".into());
    }
    fs::create_dir_all(&run_dir).map_err(|e| e.to_string())?;
    let mut report = NativeReport {
        schema: SCHEMA,
        lane: "native".into(),
        status: "ok".into(),
        run_id,
        manifest_sha256: manifest_hash,
        lock_sha256: lock_hash,
        source_digest,
        basisu_sha256: sha256_hex(&fs::read(&basis).map_err(|e| e.to_string())?),
        config: TextureConfig {
            uastc_level: 2,
            rdo_lambda: "1.0".into(),
            zstd_level: 3,
            mipmaps: true,
            color_space: "per-case".into(),
            orientation: "rd".into(),
            profile: "UASTC_LDR_4x4".into(),
        },
        cases: Vec::new(),
        failures: Vec::new(),
    };
    for case in &manifest.cases {
        let result = (|| {
            let source = fs::read(&case.path).map_err(|e| e.to_string())?;
            let (w, h) = png_dimensions(&source)?;
            if [w, h] != [case.width, case.height] {
                return Err(format!("case {} manifest dimensions drift", case.id));
            }
            let dir = safe_case_dir(&run_dir, &case.id)?;
            let a_path = dir.join("candidate-a.ktx2");
            let b_path = dir.join("candidate-b.ktx2");
            let color_flag = if case.color_space == "linear" {
                "-linear"
            } else {
                "-srgb"
            };
            let aa = vec![
                "-uastc".into(),
                "-uastc_level".into(),
                "2".into(),
                "-uastc_rdo_l".into(),
                "1.0".into(),
                color_flag.into(),
                "-mipmap".into(),
                "-ktx2_no_zstandard".into(),
                "-output_file".into(),
                a_path.display().to_string(),
                case.path.clone(),
            ];
            let (_, a_ms) = checked_command(&basis, &aa)?;
            let bb = vec![
                "-uastc".into(),
                "-uastc_level".into(),
                "2".into(),
                "-uastc_rdo_l".into(),
                "1.0".into(),
                color_flag.into(),
                "-mipmap".into(),
                "-ktx2".into(),
                "-ktx2_zstandard_level".into(),
                "3".into(),
                "-output_file".into(),
                b_path.display().to_string(),
                case.path.clone(),
            ];
            let (_, b_ms) = checked_command(&basis, &bb)?;
            let a = fs::read(&a_path).map_err(|e| e.to_string())?;
            let b = fs::read(&b_path).map_err(|e| e.to_string())?;
            let pa = parse_ktx2(&a, false)?;
            let pb = parse_ktx2(&b, true)?;
            let expected_transfer = if case.color_space == "linear" { 1 } else { 2 };
            if pa.dfd.get(14).copied() != Some(expected_transfer)
                || pb.dfd.get(14).copied() != Some(expected_transfer)
            {
                return Err(format!("case {} encoded color-space DFD drift", case.id));
            }
            let al = pa.levels_for(&a)?;
            let bl = pb.levels_for(&b)?;
            equal_levels(&al, &bl)?;
            let meta = make_probe_metadata(&pa, &al, case);
            let meta_bytes = serde_json::to_vec(&meta).map_err(|e| e.to_string())?;
            let raw_zstd = encode_raw_blocks(&al, 3)?;
            let raw_path = dir.join("candidate-a.raw.zst");
            let meta_path = dir.join("candidate-a.raw.json");
            fs::write(&raw_path, &raw_zstd).map_err(|e| e.to_string())?;
            fs::write(&meta_path, &meta_bytes).map_err(|e| e.to_string())?;
            let reconstructed = reconstruct_ktx2(&meta, &raw_zstd)?;
            let rec_path = dir.join("candidate-a-reconstructed.ktx2");
            fs::write(&rec_path, &reconstructed).map_err(|e| e.to_string())?;
            let ra = native_decode(&basis, &rec_path, &dir.join("native-a.dds"), w, h);
            let rb = native_decode(&basis, &b_path, &dir.join("native-b.dds"), w, h);
            let ok = ra.ok
                && rb.ok
                && ra.mip_hashes == rb.mip_hashes
                && ra.mip_dimensions == rb.mip_dimensions;
            if !ok {
                return Err(format!(
                    "case {} native transcode mismatch/failure",
                    case.id
                ));
            }
            let raw_blocks_bytes = al.iter().try_fold(0usize, |total, level| {
                total
                    .checked_add(level.len())
                    .ok_or_else(|| "raw block size overflow".to_string())
            })?;
            let raw_payload_bytes = meta_bytes
                .len()
                .checked_add(raw_zstd.len())
                .ok_or_else(|| "raw payload size overflow".to_string())?;
            Ok(NativeCase{id:case.id.clone(),source_sha256:sha256_hex(&source),source_bytes:source.len() as u64,width:w,height:h,class:case.class.clone(),color_space:case.color_space.clone(),encode_no_zstd_ms:a_ms,encode_zstd_ms:b_ms,candidate_a:Candidate{ktx2_bytes:a.len() as u64,ktx2_sha256:sha256_hex(&a),internal_compressed_bytes:raw_blocks_bytes as u64,raw_metadata_bytes:meta_bytes.len() as u64,raw_blocks_bytes:raw_blocks_bytes as u64,raw_zstd_bytes:raw_zstd.len() as u64,raw_payload_bytes:raw_payload_bytes as u64,reconstructed_ktx2_bytes:reconstructed.len() as u64,reconstructed_ktx2_sha256:sha256_hex(&reconstructed),level_hashes:al.iter().map(|x|sha256_hex(x)).collect()},candidate_b:Candidate{ktx2_bytes:b.len() as u64,ktx2_sha256:sha256_hex(&b),internal_compressed_bytes:pb.levels.iter().try_fold(0u64, |total, x| total.checked_add(x.length)).ok_or_else(|| "KTX2 compressed size overflow".to_string())?,raw_metadata_bytes:0,raw_blocks_bytes:0,raw_zstd_bytes:0,raw_payload_bytes:0,reconstructed_ktx2_bytes:0,reconstructed_ktx2_sha256:String::new(),level_hashes:bl.iter().map(|x|sha256_hex(x)).collect()},uastc_levels_equal:true,native_a:ra,native_b:rb,quality:"relative candidate quality: equivalent UASTC input (all mip bytes match); absolute source PNG quality metric: unavailable and out of scope for this bounded probe".into()})
        })();
        match result {
            Ok(c) => report.cases.push(c),
            Err(e) => report.failures.push(format!("{}: {e}", case.id)),
        }
    }
    if !report.failures.is_empty() {
        report.status = "failed".into();
        return Err("native campaign failed; no report candidate published".into());
    }
    write_json(&out, &report)
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = if args.get(1).map(String::as_str) == Some("run") {
        run(&args[2..])
    } else {
        Err("usage: mmdpack-texture-probe run ...".into())
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
