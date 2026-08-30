use std::{
    collections::HashSet,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::anyhow;
use clap::Subcommand;
use mmd_anim_package::{
    MMDPACK_HEADER_LEN, MmdModelBinding, MmdPackage, MmdPackageCompression, MmdPackageEntryKind,
    MmdPackageHeader, MmdPackageLimits, MmdPackageMotionMetadata, MmdPackageMotionRole,
    MmdPackagePackCompression, MmdPackagePackEntry, MmdPackagePackInput, MmdPackagePacker,
    MmdPackageVerifyOptions, MmdTextureBinding,
};
use serde::Deserialize;
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

type CommandResult = Result<ExitCode, Box<dyn Error>>;

const MAX_PACK_CONFIG_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackConfig {
    default_model_entry_id: u32,
    #[serde(default)]
    default_motion_entry_id: Option<u32>,
    entries: Vec<PackEntryConfig>,
    model_bindings: Vec<PackModelBindingConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackEntryConfig {
    id: u32,
    path: String,
    kind: String,
    codec: String,
    compression: String,
    #[serde(default)]
    media_type: Option<String>,
    #[serde(default)]
    motion: Option<PackMotionConfig>,
    #[serde(default)]
    texture: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackMotionConfig {
    role: String,
    #[serde(default)]
    target_model_entry_id: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackModelBindingConfig {
    model_entry_id: u32,
    texture_bindings: Vec<PackTextureBindingConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PackTextureBindingConfig {
    texture_index: u32,
    entry_id: u32,
}

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
    #[command(
        long_about = "Stage codec-ready payloads from input-dir into an encrypted MMDPACK package.
This Phase 1 command does not decode PNG/JPEG images, generate mipmaps, or
encode UASTC/KTX2. The strict config defaults to input-dir/mmdpack.json when
--config is omitted."
    )]
    Pack {
        /// Directory containing the source assets.
        input_dir: PathBuf,
        /// Optional strict config path; defaults to <input-dir>/mmdpack.json.
        #[arg(long, value_name = "FILE")]
        config: Option<PathBuf>,
        /// Destination package path; it must not already exist.
        #[arg(short = 'o', long, value_name = "FILE")]
        output: PathBuf,
        /// Destination for the raw 32-byte package key; it must not exist.
        #[arg(long, value_name = "FILE")]
        key_out: PathBuf,
    },
    #[command(
        long_about = "Verify a package and write each decoded codec payload unchanged to a new directory.
Raw UASTC and KTX2 payloads are restored as-is; this command does not convert
them to PNG."
    )]
    Unpack {
        /// Path to the MMDPACK file.
        package: PathBuf,
        /// Destination directory; it must not already exist.
        #[arg(short = 'o', long, value_name = "DIR")]
        output_dir: PathBuf,
        /// Path to the raw 32-byte package key.
        #[arg(long, value_name = "FILE")]
        key_file: PathBuf,
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
        PackageCommand::Pack {
            input_dir,
            config,
            output,
            key_out,
        } => pack_package(&input_dir, config.as_deref(), &output, &key_out),
        PackageCommand::Unpack {
            package,
            output_dir,
            key_file,
        } => unpack_package(&package, &output_dir, &key_file),
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
                if validate_pmx_binding(&package, entry, model, "verify")? {
                    pmx_bindings_verified += 1;
                }
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

fn pack_package(
    input_dir: &Path,
    config_path: Option<&Path>,
    output: &Path,
    key_out: &Path,
) -> CommandResult {
    ensure_new_destinations(output, key_out)?;
    let root = fs::canonicalize(input_dir)
        .map_err(|error| io_error("input directory", input_dir, error))?;
    if !root.is_dir() {
        return Err(command_error(format!(
            "pack input is not a directory: {}",
            input_dir.display()
        )));
    }
    let config_path = config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join("mmdpack.json"));
    let config = read_pack_config(&config_path)?;
    validate_logical_paths(
        config.entries.iter().map(|entry| entry.path.as_str()),
        "pack",
    )?;
    let limits = MmdPackageLimits::default();
    let input = build_pack_input(&root, config, &limits)?;
    validate_pack_model_binding_ranges(&input)?;
    let entry_count = input.entries.len();
    let packed = MmdPackagePacker::pack(input, limits)
        .map_err(|error| command_error(format!("package pack {}: {error}", input_dir.display())))?;
    publish_pack_artifacts(&packed, output, key_out)?;
    println!("package: {}", output.display());
    println!("key: {}", key_out.display());
    println!("entries_packed: {entry_count}");
    Ok(ExitCode::SUCCESS)
}

fn unpack_package(package_path: &Path, output_dir: &Path, key_path: &Path) -> CommandResult {
    reject_existing_output_dir(output_dir)?;
    let package = open_package(package_path, key_path)?;
    validate_unpack_paths(&package.manifest().entries)?;

    let staging_dir = create_unpack_staging(output_dir)?;
    let mut pmx_bindings_verified = 0;
    let report = match package.verify_with::<Box<dyn Error>, _>(
        MmdPackageVerifyOptions {
            strict_codecs: true,
        },
        |entry, model| {
            if validate_pmx_binding(&package, entry, model, "unpack")? {
                pmx_bindings_verified += 1;
            }
            write_unpacked_entry(&staging_dir, entry, model)?;
            Ok(())
        },
    ) {
        Ok(report) => report,
        Err(error) => {
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(command_error(format!(
                "package unpack verify {}: {error}",
                package_path.display()
            )));
        }
    };
    publish_unpack_staging(&staging_dir, output_dir)?;
    println!("output: {}", output_dir.display());
    println!("entries_verified: {}", report.authenticated_entry_count);
    println!("entries_unpacked: {}", package.manifest().entries.len());
    println!("pmx_bindings_verified: {pmx_bindings_verified}");
    Ok(ExitCode::SUCCESS)
}

fn read_pack_config(path: &Path) -> Result<PackConfig, Box<dyn Error>> {
    read_pack_config_with_limit(path, MAX_PACK_CONFIG_BYTES)
}

fn read_pack_config_with_limit(path: &Path, max_bytes: u64) -> Result<PackConfig, Box<dyn Error>> {
    let bytes = read_bounded_file(path, max_bytes, "pack config", "pack config")?;
    serde_json::from_slice(&bytes).map_err(|error| {
        command_error(format!(
            "pack config {} is invalid: {error}",
            path.display()
        ))
    })
}

fn build_pack_input(
    root: &Path,
    config: PackConfig,
    limits: &MmdPackageLimits,
) -> Result<MmdPackagePackInput, Box<dyn Error>> {
    if config.entries.len() > limits.max_entries {
        return Err(limit_error(
            "entry count",
            config.entries.len() as u64,
            limits.max_entries as u64,
        ));
    }
    let mut total_decoded = 0_u64;
    let mut entries = Vec::with_capacity(config.entries.len());
    for entry in config.entries {
        validate_relative_path(&entry.path)?;
        let kind = parse_entry_kind(&entry.kind)?;
        let compression = parse_pack_compression(&entry.compression)?;
        let motion = entry
            .motion
            .as_ref()
            .map(|motion| {
                Ok::<_, Box<dyn Error>>(MmdPackageMotionMetadata {
                    role: parse_motion_role(&motion.role)?,
                    target_model_entry_id: motion.target_model_entry_id,
                })
            })
            .transpose()?;
        let source = root.join(&entry.path);
        let canonical_source =
            fs::canonicalize(&source).map_err(|error| io_error("pack source", &source, error))?;
        if !canonical_source.starts_with(root) {
            return Err(command_error(format!(
                "pack source escapes input directory: {}",
                entry.path
            )));
        }
        if !canonical_source.is_file() {
            return Err(command_error(format!(
                "pack source is not a file: {}",
                entry.path
            )));
        }

        let remaining_total = limits.max_total_decoded_bytes.saturating_sub(total_decoded);
        let read_limit = limits.max_entry_decoded_bytes.min(remaining_total);
        let limit_name = if limits.max_entry_decoded_bytes <= remaining_total {
            "entry decoded bytes"
        } else {
            "total decoded bytes"
        };
        let decoded = read_bounded_file(&canonical_source, read_limit, limit_name, "pack source")?;
        total_decoded = total_decoded
            .checked_add(decoded.len() as u64)
            .ok_or_else(|| command_error("total decoded bytes overflow"))?;

        entries.push(MmdPackagePackEntry {
            id: entry.id,
            path: entry.path,
            kind,
            codec: entry.codec,
            compression,
            decoded,
            media_type: entry.media_type,
            motion,
            texture: entry.texture,
        });
    }
    let model_bindings = config
        .model_bindings
        .into_iter()
        .map(|binding| MmdModelBinding {
            model_entry_id: binding.model_entry_id,
            texture_bindings: binding
                .texture_bindings
                .into_iter()
                .map(|texture| MmdTextureBinding {
                    texture_index: texture.texture_index,
                    entry_id: texture.entry_id,
                })
                .collect(),
        })
        .collect();
    Ok(MmdPackagePackInput {
        default_model_entry_id: config.default_model_entry_id,
        default_motion_entry_id: config.default_motion_entry_id,
        entries,
        model_bindings,
    })
}

fn validate_pack_model_binding_ranges(input: &MmdPackagePackInput) -> Result<(), Box<dyn Error>> {
    for binding in &input.model_bindings {
        let Some(model) = input.entries.iter().find(|entry| {
            entry.id == binding.model_entry_id
                && entry.kind == MmdPackageEntryKind::Model
                && entry.codec == "pmx"
        }) else {
            continue;
        };
        let texture_count = mmd_anim_format::pmx::parse_pmx_texture_table(&model.decoded)
            .map_err(|error| {
                command_error(format!(
                    "package pack model entry {}: failed to read PMX texture table: {error}",
                    model.id
                ))
            })?
            .len();
        for texture in &binding.texture_bindings {
            if usize::try_from(texture.texture_index).map_or(true, |index| index >= texture_count) {
                return Err(command_error(format!(
                    "package pack model entry {}: textureIndex {} is outside the PMX texture table ({} entries)",
                    model.id, texture.texture_index, texture_count
                )));
            }
        }
    }
    Ok(())
}

fn validate_pmx_binding(
    package: &MmdPackage,
    entry: &mmd_anim_package::MmdPackageEntry,
    model: &[u8],
    operation: &str,
) -> Result<bool, Box<dyn Error>> {
    if entry.kind != MmdPackageEntryKind::Model || entry.codec != "pmx" {
        return Ok(false);
    }
    if package.model_binding(entry.id).is_none() {
        return Ok(false);
    }
    let textures = mmd_anim_format::pmx::parse_pmx_texture_table(model).map_err(|error| {
        command_error(format!(
            "package {operation} model entry {} ({}): failed to read PMX texture table: {error}",
            entry.id, entry.path
        ))
    })?;
    package
        .validate_texture_bindings_against_table(entry.id, textures.len())
        .map_err(|error| {
            command_error(format!(
                "package {operation} model entry {} ({}): {error}",
                entry.id, entry.path
            ))
        })?;
    Ok(true)
}

fn parse_entry_kind(value: &str) -> Result<MmdPackageEntryKind, Box<dyn Error>> {
    match value {
        "model" => Ok(MmdPackageEntryKind::Model),
        "motion" => Ok(MmdPackageEntryKind::Motion),
        "texture" => Ok(MmdPackageEntryKind::Texture),
        "metadata" => Ok(MmdPackageEntryKind::Metadata),
        "audio" => Ok(MmdPackageEntryKind::Audio),
        "binary" => Ok(MmdPackageEntryKind::Binary),
        _ => Err(command_error(format!(
            "unknown package entry kind: {value:?}"
        ))),
    }
}

fn parse_pack_compression(value: &str) -> Result<MmdPackagePackCompression, Box<dyn Error>> {
    match value {
        "none" => Ok(MmdPackagePackCompression::None),
        "zstd-v1" => Ok(MmdPackagePackCompression::ZstdV1),
        "auto-zstd-v1" => Ok(MmdPackagePackCompression::AutoZstdV1),
        _ => Err(command_error(format!(
            "unknown package compression: {value:?}"
        ))),
    }
}

fn parse_motion_role(value: &str) -> Result<MmdPackageMotionRole, Box<dyn Error>> {
    match value {
        "model" => Ok(MmdPackageMotionRole::Model),
        "scene" => Ok(MmdPackageMotionRole::Scene),
        "mixed" => Ok(MmdPackageMotionRole::Mixed),
        _ => Err(command_error(format!("unknown motion role: {value:?}"))),
    }
}

fn validate_relative_path(path: &str) -> Result<(), Box<dyn Error>> {
    if path.is_empty()
        || Path::new(path).is_absolute()
        || is_windows_absolute(path)
        || path.contains('\\')
    {
        return Err(command_error(format!(
            "package path is not relative: {path:?}"
        )));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(command_error(format!(
                "package path contains an invalid segment: {path:?}"
            )));
        }
        if segment.chars().any(char::is_control) {
            return Err(command_error(format!(
                "package path contains a control character: {path:?}"
            )));
        }
        #[cfg(windows)]
        validate_windows_segment(segment)?;
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn validate_windows_segment(segment: &str) -> Result<(), Box<dyn Error>> {
    if segment.is_empty()
        || segment.ends_with('.')
        || segment.ends_with(' ')
        || segment.chars().any(|character| {
            character.is_ascii_control()
                || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
        })
    {
        return Err(command_error(format!(
            "package path segment is not Windows-safe: {segment:?}"
        )));
    }
    let stem = segment.split('.').next().unwrap_or_default();
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0');
    if reserved {
        return Err(command_error(format!(
            "package path segment is a reserved Windows name: {segment:?}"
        )));
    }
    Ok(())
}

fn is_windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with("\\\\")
        || (bytes.len() >= 3 && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\'))
}

fn validate_unpack_paths(
    entries: &[mmd_anim_package::MmdPackageEntry],
) -> Result<(), Box<dyn Error>> {
    validate_logical_paths(entries.iter().map(|entry| entry.path.as_str()), "unpack")
}

fn validate_logical_paths<'a>(
    paths: impl IntoIterator<Item = &'a str>,
    operation: &str,
) -> Result<(), Box<dyn Error>> {
    let mut full_keys = HashSet::new();
    let mut path_keys = Vec::new();
    for path in paths {
        validate_relative_path(path)?;
        let key = collision_key(path);
        if !full_keys.insert(key.clone()) {
            return Err(command_error(format!(
                "{operation} paths collide after case/NFD normalization: {path:?}"
            )));
        }
        path_keys.push((path, key));
    }
    for (path, key) in path_keys {
        let mut prefix = String::new();
        let segments: Vec<_> = key.split('/').collect();
        for segment in segments.iter().take(segments.len().saturating_sub(1)) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(segment);
            if full_keys.contains(&prefix) {
                return Err(command_error(format!(
                    "{operation} paths have a file/directory prefix collision: {path:?}"
                )));
            }
        }
    }
    Ok(())
}

fn collision_key(path: &str) -> String {
    path.split('/')
        .map(|segment| segment.nfd().collect::<String>().to_lowercase())
        .collect::<Vec<_>>()
        .join("/")
}

fn read_bounded_file(
    path: &Path,
    limit: u64,
    what: &'static str,
    operation: &'static str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let file = File::open(path).map_err(|error| io_error(operation, path, error))?;
    let metadata_len = file
        .metadata()
        .map_err(|error| io_error(operation, path, error))?
        .len();
    if metadata_len > limit {
        return Err(limit_error(what, metadata_len, limit));
    }
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| io_error(operation, path, error))?;
    let actual_len = bytes.len() as u64;
    if actual_len > limit {
        return Err(limit_error(what, actual_len, limit));
    }
    Ok(bytes)
}

fn ensure_new_destinations(output: &Path, key_out: &Path) -> Result<(), Box<dyn Error>> {
    let output_identity = destination_identity(output)?;
    let key_identity = destination_identity(key_out)?;
    if same_destination(&output_identity, &key_identity) {
        return Err(command_error(
            "package output and key-out must be different files",
        ));
    }
    reject_existing_destination(output, "package output")?;
    reject_existing_destination(key_out, "package key-out")?;
    Ok(())
}

fn destination_identity(path: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        command_error(format!("destination has no file name: {}", path.display()))
    })?;
    let parent =
        fs::canonicalize(parent).map_err(|error| io_error("destination parent", parent, error))?;
    Ok(parent.join(file_name))
}

fn same_destination(left: &Path, right: &Path) -> bool {
    collision_key(&left.to_string_lossy().replace('\\', "/"))
        == collision_key(&right.to_string_lossy().replace('\\', "/"))
}

fn reject_existing_destination(path: &Path, label: &str) -> Result<(), Box<dyn Error>> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(command_error(format!(
            "{label} already exists: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(label, path, error)),
    }
}

fn reject_existing_output_dir(path: &Path) -> Result<(), Box<dyn Error>> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(command_error(format!(
            "unpack output directory already exists: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("unpack output", path, error)),
    }
}

fn write_unpacked_entry(
    staging_dir: &Path,
    entry: &mmd_anim_package::MmdPackageEntry,
    decoded: &[u8],
) -> Result<(), Box<dyn Error>> {
    let destination = join_logical_path(staging_dir, &entry.path);
    let parent = destination.parent().unwrap_or(staging_dir);
    fs::create_dir_all(parent).map_err(|error| io_error("unpack directory", parent, error))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&destination)
        .map_err(|error| io_error("unpack file", &destination, error))?;
    file.write_all(decoded)
        .map_err(|error| io_error("unpack file", &destination, error))?;
    file.sync_all()
        .map_err(|error| io_error("unpack file", &destination, error))?;
    Ok(())
}

fn create_unpack_staging(output_dir: &Path) -> Result<PathBuf, Box<dyn Error>> {
    static NEXT_STAGING_ID: AtomicU64 = AtomicU64::new(0);
    let parent = destination_parent(output_dir);
    for _ in 0..128 {
        let id = NEXT_STAGING_ID.fetch_add(1, Ordering::Relaxed);
        let staging = parent.join(format!(".mmd-anim-unpack-{}-{id}.tmp", std::process::id()));
        match fs::create_dir(&staging) {
            Ok(()) => return Ok(staging),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error("unpack staging", &staging, error)),
        }
    }
    Err(command_error(
        "could not allocate a unique unpack staging directory",
    ))
}

fn publish_unpack_staging(staging_dir: &Path, output_dir: &Path) -> CommandResult {
    if let Err(error) = fs::create_dir(output_dir) {
        let _ = fs::remove_dir_all(staging_dir);
        return Err(io_error("unpack output", output_dir, error));
    }

    let result = (|| -> Result<(), Box<dyn Error>> {
        for child in fs::read_dir(staging_dir)
            .map_err(|error| io_error("unpack staging", staging_dir, error))?
        {
            let child = child.map_err(|error| io_error("unpack staging", staging_dir, error))?;
            let destination = output_dir.join(child.file_name());
            fs::rename(child.path(), &destination)
                .map_err(|error| io_error("unpack output", &destination, error))?;
        }
        fs::remove_dir(staging_dir)
            .map_err(|error| io_error("unpack staging", staging_dir, error))?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_dir_all(staging_dir);
        let _ = fs::remove_dir_all(output_dir);
        return Err(error);
    }
    Ok(ExitCode::SUCCESS)
}

fn join_logical_path(root: &Path, path: &str) -> PathBuf {
    path.split('/')
        .fold(root.to_path_buf(), |path, segment| path.join(segment))
}

fn publish_pack_artifacts(
    packed: &mmd_anim_package::MmdPackedPackage,
    output: &Path,
    key_out: &Path,
) -> CommandResult {
    let output_parent = destination_parent(output);
    let key_parent = destination_parent(key_out);
    let (output_temp, mut output_file) = create_temp_file(output_parent, "package")?;
    if let Err(error) = output_file.write_all(packed.bytes()) {
        let _ = fs::remove_file(&output_temp);
        return Err(io_error("package output", output, error));
    }
    if let Err(error) = output_file.sync_all() {
        let _ = fs::remove_file(&output_temp);
        return Err(io_error("package output", output, error));
    }
    drop(output_file);

    let (key_temp, mut key_file) = match create_temp_file(key_parent, "key") {
        Ok(value) => value,
        Err(error) => {
            let _ = fs::remove_file(&output_temp);
            return Err(error);
        }
    };
    if let Err(error) = key_file.write_all(packed.key()) {
        let _ = fs::remove_file(&output_temp);
        let _ = fs::remove_file(&key_temp);
        return Err(io_error("package key-out", key_out, error));
    }
    if let Err(error) = key_file.sync_all() {
        let _ = fs::remove_file(&output_temp);
        let _ = fs::remove_file(&key_temp);
        return Err(io_error("package key-out", key_out, error));
    }
    drop(key_file);

    if let Err(error) = publish_temp(&output_temp, output) {
        let _ = fs::remove_file(&output_temp);
        let _ = fs::remove_file(&key_temp);
        return Err(error);
    }
    if let Err(error) = publish_temp(&key_temp, key_out) {
        let _ = fs::remove_file(&key_temp);
        let _ = fs::remove_file(output);
        return Err(error);
    }
    Ok(ExitCode::SUCCESS)
}

fn create_temp_file(parent: &Path, label: &str) -> Result<(PathBuf, File), Box<dyn Error>> {
    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
    for _ in 0..128 {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".mmd-anim-{label}-{}-{id}.tmp", std::process::id()));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                if label == "key" {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Err(error) = file.set_permissions(fs::Permissions::from_mode(0o600))
                        {
                            let _ = fs::remove_file(&path);
                            return Err(io_error("package key-out", &path, error));
                        }
                    }
                }
                return Ok((path, file));
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error("temporary output", &path, error)),
        }
    }
    Err(command_error(
        "could not allocate a unique package temporary file",
    ))
}

fn publish_temp(temp: &Path, target: &Path) -> Result<(), Box<dyn Error>> {
    fs::hard_link(temp, target).map_err(|error| io_error("publish package", target, error))?;
    if let Err(error) = fs::remove_file(temp) {
        let _ = fs::remove_file(target);
        return Err(io_error("publish package", temp, error));
    }
    Ok(())
}

fn destination_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
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
    read_bounded_file(path, max_package_bytes, "package bytes", "package")
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

fn limit_error(what: &'static str, actual: u64, limit: u64) -> Box<dyn Error> {
    command_error(
        mmd_anim_package::MmdPackageError::LimitExceeded {
            what,
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
    use serde_json::json;
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

    #[test]
    fn strict_pack_config_rejects_unknown_fields() {
        let result = serde_json::from_str::<PackConfig>(
            r#"{
                "defaultModelEntryId": 1,
                "entries": [],
                "modelBindings": [],
                "unexpected": true
            }"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn pack_config_reader_rejects_oversized_input_before_json_parse() {
        let path = tempfile_path("oversized-config");
        fs::write(&path, b"12345").unwrap();
        let error = read_pack_config_with_limit(&path, 4)
            .unwrap_err()
            .to_string();
        assert!(error.contains("pack config exceeds the configured limit"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn windows_path_segments_reject_reserved_and_unsafe_names_on_all_hosts() {
        for segment in [
            "CON.txt",
            "PRN",
            "AUX.png",
            "NUL",
            "CLOCK$.txt",
            "COM1.obj",
            "LPT9.dat",
            "trailing. ",
            "trailing.",
            "bad:name",
        ] {
            assert!(
                validate_windows_segment(segment).is_err(),
                "segment should be rejected: {segment:?}"
            );
        }
        for segment in ["model.pmx", "folder", "日本語.bin"] {
            validate_windows_segment(segment).unwrap();
        }
    }

    #[test]
    fn unpack_path_collision_check_handles_case_and_nfd() {
        let entries = vec![
            test_manifest_entry("Cafe\u{301}/diffuse.bin", 1),
            test_manifest_entry("café/diffuse.bin", 2),
        ];
        let error = validate_unpack_paths(&entries).unwrap_err().to_string();
        assert!(error.contains("case/NFD"), "{error}");
    }

    #[test]
    fn destination_alias_check_folds_unicode_case() {
        let lower = destination_identity(Path::new("é-package.mmdpack")).unwrap();
        let upper = destination_identity(Path::new("É-package.mmdpack")).unwrap();
        assert!(same_destination(&lower, &upper));
    }

    #[test]
    fn pack_rejects_traversal_paths_before_source_access() {
        let root = tempfile_path("traversal-input");
        fs::create_dir_all(&root).unwrap();
        let result = build_pack_input(
            &root,
            PackConfig {
                default_model_entry_id: 1,
                default_motion_entry_id: None,
                entries: vec![PackEntryConfig {
                    id: 1,
                    path: "../outside.pmx".into(),
                    kind: "model".into(),
                    codec: "pmx".into(),
                    compression: "none".into(),
                    media_type: None,
                    motion: None,
                    texture: None,
                }],
                model_bindings: Vec::new(),
            },
            &MmdPackageLimits::default(),
        );
        assert!(result.is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn pack_rejects_pmx_texture_binding_out_of_range_before_publish() {
        let root = tempfile_path("out-of-range-input");
        fs::create_dir_all(root.join("model")).unwrap();
        fs::write(root.join("model/model.pmx"), minimal_pmx_prefix()).unwrap();
        let config = json!({
            "defaultModelEntryId": 1,
            "entries": [{
                "id": 1,
                "path": "model/model.pmx",
                "kind": "model",
                "codec": "pmx",
                "compression": "none"
            }],
            "modelBindings": [{
                "modelEntryId": 1,
                "textureBindings": [{"textureIndex": 0, "entryId": 1}]
            }]
        });
        fs::write(
            root.join("mmdpack.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let package_path = tempfile_path("out-of-range-package");
        let key_path = tempfile_path("out-of-range-key");

        let error = pack_package(&root, None, &package_path, &key_path)
            .unwrap_err()
            .to_string();
        assert!(error.contains("textureIndex 0 is outside the PMX texture table"));
        assert!(!package_path.exists());
        assert!(!key_path.exists());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(package_path);
        let _ = fs::remove_file(key_path);
    }

    #[test]
    fn pack_and_unpack_roundtrip_minimal_pmx_without_external_assets() {
        let root = tempfile_path("roundtrip-input");
        fs::create_dir_all(root.join("model")).unwrap();
        let model = minimal_pmx_prefix();
        fs::write(root.join("model/model.pmx"), &model).unwrap();
        let config = json!({
            "defaultModelEntryId": 1,
            "entries": [{
                "id": 1,
                "path": "model/model.pmx",
                "kind": "model",
                "codec": "pmx",
                "compression": "none"
            }],
            "modelBindings": [{
                "modelEntryId": 1,
                "textureBindings": []
            }]
        });
        fs::write(
            root.join("mmdpack.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let package_path = tempfile_path("roundtrip.mmdpack");
        let key_path = tempfile_path("roundtrip.key");
        let output_dir = tempfile_path("roundtrip-output");

        pack_package(&root, None, &package_path, &key_path).unwrap();
        assert_eq!(fs::read(&key_path).unwrap().len(), 32);
        assert_eq!(read_header(&package_path).unwrap().major, 1);
        verify_package(&package_path, &key_path, true).unwrap();
        unpack_package(&package_path, &output_dir, &key_path).unwrap();
        assert_eq!(fs::read(output_dir.join("model/model.pmx")).unwrap(), model);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(package_path);
        let _ = fs::remove_file(key_path);
        let _ = fs::remove_dir_all(output_dir);
    }

    #[test]
    fn pack_rejects_existing_output_and_key_without_overwriting() {
        let root = tempfile_path("existing-input");
        fs::create_dir_all(root.join("model")).unwrap();
        fs::write(root.join("model/model.pmx"), minimal_pmx_prefix()).unwrap();
        let config = json!({
            "defaultModelEntryId": 1,
            "entries": [{
                "id": 1,
                "path": "model/model.pmx",
                "kind": "model",
                "codec": "pmx",
                "compression": "none"
            }],
            "modelBindings": [{"modelEntryId": 1, "textureBindings": []}]
        });
        fs::write(
            root.join("mmdpack.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        let package_path = tempfile_path("existing-package");
        let key_path = tempfile_path("existing-key");
        fs::write(&package_path, b"keep-package").unwrap();
        fs::write(&key_path, b"keep-key").unwrap();

        assert!(pack_package(&root, None, &package_path, &key_path).is_err());
        assert_eq!(fs::read(&package_path).unwrap(), b"keep-package");
        assert_eq!(fs::read(&key_path).unwrap(), b"keep-key");

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_file(package_path);
        let _ = fs::remove_file(key_path);
    }

    fn test_manifest_entry(path: &str, id: u32) -> mmd_anim_package::MmdPackageEntry {
        mmd_anim_package::MmdPackageEntry {
            id,
            path: path.to_owned(),
            kind: MmdPackageEntryKind::Binary,
            codec: "opaque".to_owned(),
            compression: mmd_anim_package::MmdPackageCompression::None,
            offset: 0,
            cipher_size: 16,
            decoded_size: 0,
        }
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
