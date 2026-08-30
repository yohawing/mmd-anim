use std::{
    error::Error,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::Arc,
};

use anyhow::anyhow;
use clap::Subcommand;
use mmd_anim_package::{
    MMDPACK_HEADER_LEN, MmdPackage, MmdPackageCompression, MmdPackageEntryKind, MmdPackageHeader,
    MmdPackageLimits, MmdPackageVerifyOptions,
};

type CommandResult = Result<ExitCode, Box<dyn Error>>;

#[derive(Subcommand)]
pub(crate) enum PackageCommand {
    /// Read and print only the fixed package header.
    Header {
        /// Path to the MMDPACK file.
        package: PathBuf,
    },
    /// Authenticate a package and print its manifest entry table.
    Inspect {
        /// Path to the MMDPACK file.
        package: PathBuf,
        /// Path to the raw 32-byte package key.
        #[arg(long, value_name = "FILE")]
        key_file: PathBuf,
    },
    /// Authenticate and verify package entries and PMX texture bindings.
    Verify {
        /// Path to the MMDPACK file.
        package: PathBuf,
        /// Path to the raw 32-byte package key.
        #[arg(long, value_name = "FILE")]
        key_file: PathBuf,
        /// Reject entries whose codec is not known by this package reader.
        #[arg(long)]
        strict_codecs: bool,
    },
}

pub(crate) fn dispatch(command: PackageCommand) -> CommandResult {
    match command {
        PackageCommand::Header { package } => print_header(&package),
        PackageCommand::Inspect { package, key_file } => print_manifest(&package, &key_file),
        PackageCommand::Verify {
            package,
            key_file,
            strict_codecs,
        } => verify_package(&package, &key_file, strict_codecs),
    }
}

fn print_header(path: &Path) -> CommandResult {
    let header = read_header(path)?;
    println!("version: {}.{}", header.major, header.minor);
    println!("package_id: {}", format_hex(&header.package_id));
    println!("nonce_prefix: {}", format_hex(&header.nonce_prefix));
    println!("manifest_cipher_size: {}", header.manifest_cipher_size);
    Ok(ExitCode::SUCCESS)
}

fn print_manifest(package_path: &Path, key_path: &Path) -> CommandResult {
    let package = open_package(package_path, key_path)?;
    let manifest = package.manifest();
    println!("schema: {}", manifest.schema);
    println!(
        "default_model_entry_id: {}",
        manifest.default_model_entry_id
    );
    match manifest.default_motion_entry_id {
        Some(id) => println!("default_motion_entry_id: {id}"),
        None => println!("default_motion_entry_id: none"),
    }
    println!("entries: {}", manifest.entries.len());
    for entry in &manifest.entries {
        println!(
            "entry id={} path={:?} kind={} codec={} compression={} offset={} cipher_size={} decoded_size={}",
            entry.id,
            entry.path,
            kind_name(entry.kind),
            entry.codec,
            compression_name(entry.compression),
            entry.offset,
            entry.cipher_size,
            entry.decoded_size,
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn verify_package(package_path: &Path, key_path: &Path, strict_codecs: bool) -> CommandResult {
    let package = open_package(package_path, key_path)?;
    let mut pmx_bindings_verified = 0;
    let report = package
        .verify_with::<Box<dyn Error>, _>(
            MmdPackageVerifyOptions { strict_codecs },
            |entry, model| {
                if entry.kind != MmdPackageEntryKind::Model || entry.codec != "pmx" {
                    return Ok(());
                }
                if package.model_binding(entry.id).is_none() {
                    return Ok(());
                }
                let textures = mmd_anim_format::pmx::parse_pmx_texture_table(model).map_err(
                    |error| {
                        command_error(format!(
                            "package verify model entry {} ({}): failed to read PMX texture table: {error}",
                            entry.id, entry.path
                        ))
                    },
                )?;
                package
                    .validate_texture_bindings_against_table(entry.id, textures.len())
                    .map_err(|error| {
                        command_error(format!(
                            "package verify model entry {} ({}): {error}",
                            entry.id, entry.path
                        ))
                    })?;
                pmx_bindings_verified += 1;
                Ok(())
            },
        )
        .map_err(|error| {
            command_error(format!(
                "package verify {}: {error}",
                package_path.display()
            ))
        })?;

    println!("entries_verified: {}", report.authenticated_entry_count);
    println!("total_decoded_bytes: {}", report.total_decoded_bytes);
    println!("pmx_bindings_verified: {pmx_bindings_verified}");
    println!(
        "unknown_codec_count: {}",
        report.unknown_codec_entry_ids.len()
    );
    if report.unknown_codec_entry_ids.is_empty() {
        println!("unknown_codecs: none");
    } else {
        let ids = report
            .unknown_codec_entry_ids
            .iter()
            .map(|id| {
                package
                    .manifest()
                    .entries
                    .iter()
                    .find(|entry| entry.id == *id)
                    .map(|entry| format!("{id}:{}", entry.codec))
                    .unwrap_or_else(|| id.to_string())
            })
            .collect::<Vec<_>>()
            .join(",");
        println!("unknown_codecs: {ids}");
    }
    println!("texture_payload_validation: manifest metadata only; KTX2 payload/DFD deferred");
    Ok(ExitCode::SUCCESS)
}

fn read_header(path: &Path) -> CommandResultHeader {
    let mut file = File::open(path).map_err(|error| io_error("header", path, error))?;
    let mut bytes = [0_u8; MMDPACK_HEADER_LEN];
    file.read_exact(&mut bytes)
        .map_err(|error| io_error("header", path, error))?;
    MmdPackageHeader::parse_prefix(&bytes)
        .map_err(|error| command_error(format!("package header {}: {error}", path.display())))
}

type CommandResultHeader = Result<MmdPackageHeader, Box<dyn Error>>;

fn open_package(package_path: &Path, key_path: &Path) -> CommandResultPackage {
    let key = read_key_file(key_path)?;
    let limits = MmdPackageLimits::default();
    let bytes = read_package_bytes(package_path, limits.max_package_bytes)?;
    MmdPackage::open_bytes(Arc::from(bytes), key, limits)
        .map_err(|error| command_error(format!("package open {}: {error}", package_path.display())))
}

type CommandResultPackage = Result<MmdPackage, Box<dyn Error>>;

fn read_package_bytes(path: &Path, max_package_bytes: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    let file = File::open(path).map_err(|error| io_error("package", path, error))?;
    let metadata_len = file
        .metadata()
        .map_err(|error| io_error("package", path, error))?
        .len();
    if metadata_len > max_package_bytes {
        return Err(limit_error(metadata_len, max_package_bytes));
    }

    let mut bytes = Vec::new();
    file.take(max_package_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("package", path, error))?;
    let actual_len = bytes.len() as u64;
    if actual_len > max_package_bytes {
        return Err(limit_error(actual_len, max_package_bytes));
    }
    Ok(bytes)
}

fn read_key_file(path: &Path) -> Result<[u8; 32], Box<dyn Error>> {
    let mut file = File::open(path).map_err(|error| io_error("key file", path, error))?;
    let mut key = [0_u8; 32];
    if let Err(error) = file.read_exact(&mut key) {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            return Err(command_error(format!(
                "key file {} must contain exactly 32 raw bytes",
                path.display()
            )));
        }
        return Err(io_error("key file", path, error));
    }
    let mut extra = [0_u8; 1];
    if file
        .read(&mut extra)
        .map_err(|error| io_error("key file", path, error))?
        != 0
    {
        return Err(command_error(format!(
            "key file {} must contain exactly 32 raw bytes",
            path.display()
        )));
    }
    Ok(key)
}

fn command_error(message: impl Into<String>) -> Box<dyn Error> {
    anyhow!(message.into()).into()
}

fn limit_error(actual: u64, limit: u64) -> Box<dyn Error> {
    command_error(
        mmd_anim_package::MmdPackageError::LimitExceeded {
            what: "package bytes",
            actual,
            limit,
        }
        .to_string(),
    )
}

fn io_error(operation: &str, path: &Path, error: io::Error) -> Box<dyn Error> {
    command_error(format!(
        "package {operation} {}: {}",
        path.display(),
        crate::io_error_label(error.kind())
    ))
}

fn format_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn kind_name(kind: MmdPackageEntryKind) -> &'static str {
    match kind {
        MmdPackageEntryKind::Model => "model",
        MmdPackageEntryKind::Motion => "motion",
        MmdPackageEntryKind::Texture => "texture",
        MmdPackageEntryKind::Metadata => "metadata",
        MmdPackageEntryKind::Audio => "audio",
        MmdPackageEntryKind::Binary => "binary",
    }
}

fn compression_name(compression: MmdPackageCompression) -> &'static str {
    match compression {
        MmdPackageCompression::None => "none",
        MmdPackageCompression::ZstdV1 => "zstd-v1",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mmd_anim_package::{
        MmdModelBinding, MmdPackagePackCompression, MmdPackagePackEntry, MmdPackagePackInput,
        MmdPackagePacker,
    };
    use std::fs;

    #[test]
    fn raw_key_file_reader_rejects_short_and_long_files() {
        let short = tempfile_path("short-key");
        fs::write(&short, [0_u8; 31]).unwrap();
        assert!(read_key_file(&short).is_err());
        let _ = fs::remove_file(&short);

        let long = tempfile_path("long-key");
        fs::write(&long, [0_u8; 33]).unwrap();
        assert!(read_key_file(&long).is_err());
        let _ = fs::remove_file(&long);
    }

    #[test]
    fn bounded_package_reader_rejects_an_oversized_sparse_file() {
        let path = tempfile_path("oversized-package");
        let file = File::create(&path).unwrap();
        file.set_len(8).unwrap();

        let error = read_package_bytes(&path, 4).unwrap_err().to_string();
        assert!(error.contains("package bytes exceeds the configured limit"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn hex_format_is_lowercase_and_fixed_width() {
        assert_eq!(format_hex(&[0x00, 0x0a, 0xff]), "000aff");
    }

    #[test]
    fn verify_command_accepts_a_minimal_authenticated_pmx_package() {
        let packed = MmdPackagePacker::pack(
            MmdPackagePackInput {
                default_model_entry_id: 1,
                default_motion_entry_id: None,
                entries: vec![MmdPackagePackEntry {
                    id: 1,
                    path: "model/model.pmx".into(),
                    kind: MmdPackageEntryKind::Model,
                    codec: "pmx".into(),
                    compression: MmdPackagePackCompression::None,
                    decoded: minimal_pmx_prefix(),
                    media_type: None,
                    motion: None,
                    texture: None,
                }],
                model_bindings: vec![MmdModelBinding {
                    model_entry_id: 1,
                    texture_bindings: Vec::new(),
                }],
            },
            MmdPackageLimits::default(),
        )
        .unwrap();
        let package_path = tempfile_path("verify-package");
        let key_path = tempfile_path("verify-key");
        fs::write(&package_path, packed.bytes()).unwrap();
        fs::write(&key_path, packed.key()).unwrap();

        assert_eq!(
            verify_package(&package_path, &key_path, false).unwrap(),
            ExitCode::SUCCESS
        );
        let _ = fs::remove_file(package_path);
        let _ = fs::remove_file(key_path);
    }

    #[test]
    fn verify_command_allows_an_unbound_non_default_pmx_model() {
        let packed = MmdPackagePacker::pack(
            MmdPackagePackInput {
                default_model_entry_id: 1,
                default_motion_entry_id: None,
                entries: vec![
                    MmdPackagePackEntry {
                        id: 1,
                        path: "model/default.pmx".into(),
                        kind: MmdPackageEntryKind::Model,
                        codec: "pmx".into(),
                        compression: MmdPackagePackCompression::None,
                        decoded: minimal_pmx_prefix(),
                        media_type: None,
                        motion: None,
                        texture: None,
                    },
                    MmdPackagePackEntry {
                        id: 2,
                        path: "model/optional.pmx".into(),
                        kind: MmdPackageEntryKind::Model,
                        codec: "pmx".into(),
                        compression: MmdPackagePackCompression::None,
                        decoded: minimal_pmx_prefix(),
                        media_type: None,
                        motion: None,
                        texture: None,
                    },
                ],
                model_bindings: vec![MmdModelBinding {
                    model_entry_id: 1,
                    texture_bindings: Vec::new(),
                }],
            },
            MmdPackageLimits::default(),
        )
        .unwrap();
        let package_path = tempfile_path("unbound-model-package");
        let key_path = tempfile_path("unbound-model-key");
        fs::write(&package_path, packed.bytes()).unwrap();
        fs::write(&key_path, packed.key()).unwrap();

        assert_eq!(
            verify_package(&package_path, &key_path, false).unwrap(),
            ExitCode::SUCCESS
        );
        let _ = fs::remove_file(package_path);
        let _ = fs::remove_file(key_path);
    }

    fn minimal_pmx_prefix() -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"PMX ");
        bytes.extend_from_slice(&2.0_f32.to_le_bytes());
        bytes.extend_from_slice(&[8, 1, 0, 4, 1, 1, 4, 1, 1]);
        bytes.extend(std::iter::repeat_n(0_u8, 16));
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes
    }

    fn tempfile_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "mmd-anim-package-command-{name}-{}",
            std::process::id()
        ))
    }
}
