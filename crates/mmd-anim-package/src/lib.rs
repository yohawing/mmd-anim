//! Experimental bounded reader for the draft `.mmdpack` container.
//!
//! The binary format is still a draft. This crate intentionally exposes only
//! a low-level native reader and must not be treated as a frozen V1 contract.

mod error;
mod pack;
mod strict_json;

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use std::sync::Arc;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

pub use error::MmdPackageError;
use error::Result;
pub use pack::{
    MmdModelBinding, MmdPackageMotionMetadata, MmdPackageMotionRole, MmdPackagePackCompression,
    MmdPackagePackEntry, MmdPackagePackError, MmdPackagePackInput, MmdPackagePacker,
    MmdPackedPackage, MmdTextureBinding,
};

pub const MMDPACK_HEADER_LEN: usize = 64;
const GCM_TAG_LEN: u64 = 16;
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
const ENTRY_AAD_PREFIX: &[u8; 10] = b"MMDP-AAD-1";
// Draft metadata budgets are intentionally private until the package format
// and its public configuration contract are frozen.
const MAX_MANIFEST_DEPTH: usize = 16;
const MAX_MANIFEST_NODES: usize = 65_536;
const MAX_MANIFEST_ARRAY_ITEMS: usize = 8_192;
const MAX_MANIFEST_OBJECT_FIELDS: usize = 8_192;
const MAX_METADATA_STRING_BYTES: usize = 64 * 1024;
const MAX_LICENSES: usize = 4_096;
const MAX_CREDITS: usize = 4_096;
const MAX_LICENSE_REFS_PER_ENTRY: usize = 64;
const MAX_TEXTURE_DIMENSION: u32 = 16_384;
const MAX_TEXTURE_MIP_COUNT: usize = 32;
const KTX2_HEADER_LEN: usize = 80;
const KTX2_LEVEL_INDEX_ENTRY_LEN: usize = 24;
const KTX2_ZSTD_SUPERCOMPRESSION: u32 = 2;
const KTX2_DFD_SIZE: usize = 44;
const KTX2_DFD_BLOCK_SIZE: usize = 40;
const KTX2_MAX_KVD_BYTES: u64 = 16 * 1024 * 1024;
const KTX2_IDENTIFIER: &[u8; 12] = b"\xabKTX 20\xbb\r\n\x1a\n";

#[derive(Clone, Debug)]
struct Ktx2TextureMetadata {
    width: u32,
    height: u32,
    mip_count: usize,
    color_space: String,
    channel_model: String,
    swizzle: String,
}

/// Parsed fixed header for the current draft wire format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MmdPackageHeader {
    pub major: u16,
    pub minor: u16,
    pub package_id: [u8; 16],
    pub nonce_prefix: [u8; 8],
    pub manifest_cipher_size: u64,
}

impl MmdPackageHeader {
    /// Parses and validates the fixed 64-byte prefix without decrypting data.
    pub fn parse_prefix(bytes: &[u8]) -> Result<Self> {
        let header = bytes
            .get(..MMDPACK_HEADER_LEN)
            .ok_or(MmdPackageError::HeaderTooShort)?;
        if &header[..8] != b"MMDPACK\0" {
            return Err(MmdPackageError::InvalidMagic);
        }

        let major = u16::from_le_bytes(header[8..10].try_into().unwrap());
        let minor = u16::from_le_bytes(header[10..12].try_into().unwrap());
        if major != 1 || minor != 0 {
            return Err(MmdPackageError::UnsupportedVersion { major, minor });
        }

        let flags = u32::from_le_bytes(header[12..16].try_into().unwrap());
        if flags != 0 {
            return Err(MmdPackageError::UnsupportedFlags(flags));
        }
        if header[48..64].iter().any(|byte| *byte != 0) {
            return Err(MmdPackageError::NonZeroReserved);
        }

        Ok(Self {
            major,
            minor,
            package_id: header[16..32].try_into().unwrap(),
            nonce_prefix: header[32..40].try_into().unwrap(),
            manifest_cipher_size: u64::from_le_bytes(header[40..48].try_into().unwrap()),
        })
    }
}

/// Resource limits checked before entry-sized allocations are made.
#[derive(Clone, Debug)]
pub struct MmdPackageLimits {
    pub max_package_bytes: u64,
    pub max_manifest_cipher_bytes: u64,
    pub max_entries: usize,
    pub max_entry_cipher_bytes: u64,
    pub max_entry_decoded_bytes: u64,
    pub max_total_decoded_bytes: u64,
    pub max_path_bytes: usize,
}

impl Default for MmdPackageLimits {
    fn default() -> Self {
        Self {
            max_package_bytes: 2 * 1024 * 1024 * 1024,
            max_manifest_cipher_bytes: 8 * 1024 * 1024,
            max_entries: 4096,
            max_entry_cipher_bytes: 512 * 1024 * 1024,
            max_entry_decoded_bytes: 512 * 1024 * 1024,
            max_total_decoded_bytes: 2 * 1024 * 1024 * 1024,
            max_path_bytes: 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmdPackageEntryKind {
    Model,
    Motion,
    Texture,
    Metadata,
    Audio,
    Binary,
}

impl MmdPackageEntryKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Motion => "motion",
            Self::Texture => "texture",
            Self::Metadata => "metadata",
            Self::Audio => "audio",
            Self::Binary => "binary",
        }
    }

    pub fn from_token(value: &str) -> Option<Self> {
        [
            Self::Model,
            Self::Motion,
            Self::Texture,
            Self::Metadata,
            Self::Audio,
            Self::Binary,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmdPackageCompression {
    None,
    ZstdV1,
}

impl MmdPackageCompression {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ZstdV1 => "zstd-v1",
        }
    }

    pub fn from_token(value: &str) -> Option<Self> {
        [Self::None, Self::ZstdV1]
            .into_iter()
            .find(|compression| compression.as_str() == value)
    }
}

/// Manifest fields required by the low-level reader.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MmdPackageManifest {
    pub schema: String,
    pub default_model_entry_id: u32,
    pub default_motion_entry_id: Option<u32>,
    pub entries: Vec<MmdPackageEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MmdPackageEntry {
    pub id: u32,
    pub path: String,
    pub kind: MmdPackageEntryKind,
    pub codec: String,
    pub compression: MmdPackageCompression,
    pub offset: u64,
    pub cipher_size: u64,
    pub decoded_size: u64,
}

/// Controls package-layer verification. Known codec payload checks, including
/// the private KTX2 profile, run while entries are decoded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MmdPackageVerifyOptions {
    /// Reject manifest entries whose codec token is not known to this crate.
    pub strict_codecs: bool,
}

/// Successful package-layer verification summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MmdPackageVerifyReport {
    pub entry_count: usize,
    pub total_decoded_bytes: u64,
    /// Unknown codecs were still authenticated and decompressed when strict
    /// codec checking was disabled.
    pub unknown_codec_entry_ids: Vec<u32>,
}

/// Authenticated package with lazy, per-entry decoding.
pub struct MmdPackage {
    bytes: Arc<[u8]>,
    key: Zeroizing<[u8; 32]>,
    header: MmdPackageHeader,
    manifest: MmdPackageManifest,
    model_bindings: Vec<MmdModelBinding>,
    ktx2_metadata: HashMap<u32, Ktx2TextureMetadata>,
    payload_base: usize,
    entries_by_id: HashMap<u32, usize>,
    entries_by_path: HashMap<String, usize>,
}

impl MmdPackage {
    /// Opens an in-memory package and authenticates its manifest.
    ///
    /// Entry ciphertext remains undecoded until [`Self::read_entry`] or
    /// [`Self::read`] is called.
    pub fn open_bytes(bytes: Arc<[u8]>, key: [u8; 32], limits: MmdPackageLimits) -> Result<Self> {
        check_limit(
            "package bytes",
            bytes.len() as u64,
            limits.max_package_bytes,
        )?;
        let header = MmdPackageHeader::parse_prefix(&bytes)?;
        check_limit(
            "manifest ciphertext bytes",
            header.manifest_cipher_size,
            limits.max_manifest_cipher_bytes,
        )?;
        if header.manifest_cipher_size < GCM_TAG_LEN {
            return Err(MmdPackageError::InvalidManifest(
                "manifest ciphertext is shorter than the GCM tag".into(),
            ));
        }

        let manifest_end_u64 = (MMDPACK_HEADER_LEN as u64)
            .checked_add(header.manifest_cipher_size)
            .ok_or(MmdPackageError::IntegerOverflow("manifest end"))?;
        let manifest_end = usize::try_from(manifest_end_u64)
            .map_err(|_| MmdPackageError::IntegerOverflow("manifest end"))?;
        let manifest_ciphertext = bytes
            .get(MMDPACK_HEADER_LEN..manifest_end)
            .ok_or(MmdPackageError::Truncated("manifest ciphertext"))?;

        let cipher = Aes256Gcm::new_from_slice(&key).expect("AES-256 key length is fixed");
        let manifest_plaintext = cipher
            .decrypt(
                Nonce::from_slice(&manifest_nonce(&header.nonce_prefix)),
                Payload {
                    msg: manifest_ciphertext,
                    aad: &bytes[..MMDPACK_HEADER_LEN],
                },
            )
            .map_err(|_| MmdPackageError::AuthenticationFailed("manifest"))?;
        if manifest_plaintext.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(MmdPackageError::InvalidJson(
                "UTF-8 BOM is not allowed".into(),
            ));
        }

        let manifest_value = strict_json::parse(&manifest_plaintext)
            .map_err(|error| MmdPackageError::InvalidJson(error.to_string()))?;
        validate_json_budgets(&manifest_value)?;
        let manifest = parse_manifest(&manifest_value, &limits)?;
        validate_layout(&manifest.entries, bytes.len(), manifest_end, &limits)?;
        let model_bindings = validate_defaults(&manifest_value, &manifest)?;
        let ktx2_metadata = validate_entry_metadata(&manifest_value, &manifest)?;

        let entries_by_id = strict_json::index_by_id(&manifest.entries, |entry| entry.id);
        let entries_by_path = manifest
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.path.clone(), index))
            .collect();

        Ok(Self {
            bytes,
            key: Zeroizing::new(key),
            header,
            manifest,
            model_bindings,
            ktx2_metadata,
            payload_base: manifest_end,
            entries_by_id,
            entries_by_path,
        })
    }

    pub fn header(&self) -> &MmdPackageHeader {
        &self.header
    }

    pub fn manifest(&self) -> &MmdPackageManifest {
        &self.manifest
    }

    /// Returns the optional binding record for a model entry.
    pub fn model_binding(&self, model_entry_id: u32) -> Option<&MmdModelBinding> {
        self.model_bindings
            .iter()
            .find(|binding| binding.model_entry_id == model_entry_id)
    }

    /// Checks the stored texture bindings against a PMX texture-table count.
    ///
    /// PMX texture indices are zero-based. This method only checks indices
    /// that are explicitly bound; unbound PMX texture slots are permitted.
    /// Built-in toon slots are not part of the PMX texture table and therefore
    /// are intentionally not considered here.
    pub fn validate_texture_bindings_against_table(
        &self,
        model_entry_id: u32,
        texture_table_len: usize,
    ) -> Result<()> {
        let binding = self
            .model_binding(model_entry_id)
            .ok_or(MmdPackageError::ModelBindingNotFound(model_entry_id))?;
        for texture in &binding.texture_bindings {
            let in_range = usize::try_from(texture.texture_index)
                .map(|index| index < texture_table_len)
                .unwrap_or(false);
            if !in_range {
                return Err(MmdPackageError::TextureIndexOutOfRange {
                    model_entry_id,
                    texture_index: texture.texture_index,
                    texture_table_len,
                });
            }
        }
        Ok(())
    }

    /// Authenticates and decodes one entry by its stable ID.
    pub fn read_entry(&self, id: u32) -> Result<Vec<u8>> {
        let index = *self
            .entries_by_id
            .get(&id)
            .ok_or(MmdPackageError::EntryNotFound(id))?;
        self.decode_index(index, true)
    }

    /// Authenticates and decodes one entry by its exact, case-sensitive path.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let index = *self
            .entries_by_path
            .get(path)
            .ok_or_else(|| MmdPackageError::PathNotFound(path.to_owned()))?;
        self.decode_index(index, true)
    }

    /// Authenticates and decodes every entry one at a time without retaining
    /// decoded payloads between entries.
    pub fn verify(&self, options: MmdPackageVerifyOptions) -> Result<MmdPackageVerifyReport> {
        self.verify_with(options, |_, _| Ok::<(), MmdPackageError>(()))
    }

    /// Authenticates and decodes every entry once, invoking `visitor` with the
    /// borrowed decoded payload before moving to the next entry. The visitor
    /// must not retain the payload; this keeps verification allocation-bounded.
    ///
    /// Visitor errors and package errors share the caller-selected error type.
    /// `E` must be constructible from [`MmdPackageError`].
    pub fn verify_with<E, F>(
        &self,
        options: MmdPackageVerifyOptions,
        mut visitor: F,
    ) -> std::result::Result<MmdPackageVerifyReport, E>
    where
        E: From<MmdPackageError>,
        F: FnMut(&MmdPackageEntry, &[u8]) -> std::result::Result<(), E>,
    {
        let mut total_decoded_bytes = 0_u64;
        let mut unknown_codec_entry_ids = Vec::new();
        for (index, entry) in self.manifest.entries.iter().enumerate() {
            let known = is_known_codec(&entry.codec);
            if !known {
                if options.strict_codecs {
                    return Err(MmdPackageError::UnsupportedCodec(entry.codec.clone()).into());
                }
                unknown_codec_entry_ids.push(entry.id);
            }
            let decoded = self.decode_index(index, false).map_err(E::from)?;
            total_decoded_bytes = total_decoded_bytes
                .checked_add(decoded.len() as u64)
                .ok_or(MmdPackageError::IntegerOverflow("verified decoded bytes"))
                .map_err(E::from)?;
            visitor(entry, &decoded)?;
        }
        Ok(MmdPackageVerifyReport {
            entry_count: self.manifest.entries.len(),
            total_decoded_bytes,
            unknown_codec_entry_ids,
        })
    }

    fn decode_index(&self, index: usize, require_known_codec: bool) -> Result<Vec<u8>> {
        let entry = &self.manifest.entries[index];
        if require_known_codec && !is_known_codec(&entry.codec) {
            return Err(MmdPackageError::UnsupportedCodec(entry.codec.clone()));
        }

        let start = self
            .payload_base
            .checked_add(usize_from_u64(entry.offset, "entry start")?)
            .ok_or(MmdPackageError::IntegerOverflow("entry start"))?;
        let end = start
            .checked_add(usize_from_u64(entry.cipher_size, "entry end")?)
            .ok_or(MmdPackageError::IntegerOverflow("entry end"))?;
        let ciphertext = self
            .bytes
            .get(start..end)
            .ok_or(MmdPackageError::Truncated("entry ciphertext"))?;

        let cipher =
            Aes256Gcm::new_from_slice(self.key.as_ref()).expect("AES-256 key length is fixed");
        let plaintext = cipher
            .decrypt(
                Nonce::from_slice(&entry_nonce(&self.header.nonce_prefix, entry.id)),
                Payload {
                    msg: ciphertext,
                    aad: &entry_aad(&self.header.package_id, entry),
                },
            )
            .map_err(|_| MmdPackageError::AuthenticationFailed("entry"))?;

        let decoded = match entry.compression {
            MmdPackageCompression::None => {
                require_decoded_size(plaintext.len(), entry.decoded_size)?;
                plaintext
            }
            MmdPackageCompression::ZstdV1 => decode_zstd(&plaintext, entry.decoded_size)?,
        };
        if entry.codec == "ktx2-uastc-v1" {
            let metadata = self
                .ktx2_metadata
                .get(&entry.id)
                .expect("validated KTX2 metadata is present");
            validate_ktx2_payload(&decoded, entry.compression, metadata)?;
        }
        Ok(decoded)
    }
}

fn parse_manifest(value: &Value, limits: &MmdPackageLimits) -> Result<MmdPackageManifest> {
    let object = strict_json::object(value)
        .ok_or_else(|| MmdPackageError::InvalidManifest("root must be an object".into()))?;
    let schema = field(strict_json::required_str(object, "schema"))?;
    if schema != "mmdpack/1" {
        return Err(MmdPackageError::InvalidManifest(format!(
            "unsupported schema {schema:?}"
        )));
    }

    let default_model_entry_id = to_entry_id(field(strict_json::required_u64(
        object,
        "defaultModelEntryId",
    ))?)?;
    let default_motion_entry_id = field(strict_json::optional_u64(object, "defaultMotionEntryId"))?
        .map(to_entry_id)
        .transpose()?;
    let entries_value = field(strict_json::required(object, "entries"))?;
    let entries_array = entries_value
        .as_array()
        .ok_or_else(|| MmdPackageError::InvalidManifest("entries must be an array".into()))?;
    check_limit(
        "entry count",
        entries_array.len() as u64,
        limits.max_entries as u64,
    )?;

    let mut entries = Vec::with_capacity(entries_array.len());
    let mut ids = HashSet::with_capacity(entries_array.len());
    let mut paths = HashSet::with_capacity(entries_array.len());
    let mut folded_paths = HashSet::with_capacity(entries_array.len());
    for value in entries_array {
        let entry = parse_entry(value, limits)?;
        if !ids.insert(entry.id) {
            return invalid_manifest(format!("duplicate entry id {}", entry.id));
        }
        if !paths.insert(entry.path.clone()) {
            return invalid_manifest(format!("duplicate entry path {:?}", entry.path));
        }
        if !folded_paths.insert(entry.path.to_ascii_lowercase()) {
            return invalid_manifest(format!(
                "ASCII case-insensitive path collision at {:?}",
                entry.path
            ));
        }
        entries.push(entry);
    }

    Ok(MmdPackageManifest {
        schema: schema.to_owned(),
        default_model_entry_id,
        default_motion_entry_id,
        entries,
    })
}

fn parse_entry(value: &Value, limits: &MmdPackageLimits) -> Result<MmdPackageEntry> {
    let object = strict_json::object(value)
        .ok_or_else(|| MmdPackageError::InvalidManifest("entry must be an object".into()))?;
    let id = to_entry_id(field(strict_json::required_u64(object, "id"))?)?;
    let path = field(strict_json::required_str(object, "path"))?;
    validate_path(path, limits.max_path_bytes)?;
    let kind = parse_kind(field(strict_json::required_str(object, "kind"))?)?;
    let codec = field(strict_json::required_str(object, "codec"))?;
    validate_codec_token(codec)?;
    validate_known_kind_codec(kind, codec)?;
    let compression_token = field(strict_json::required_str(object, "compression"))?;
    let compression = MmdPackageCompression::from_token(compression_token).ok_or_else(|| {
        MmdPackageError::InvalidManifest(format!("unsupported compression {compression_token:?}"))
    })?;

    Ok(MmdPackageEntry {
        id,
        path: path.to_owned(),
        kind,
        codec: codec.to_owned(),
        compression,
        offset: field(strict_json::required_u64(object, "offset"))?,
        cipher_size: field(strict_json::required_u64(object, "cipherSize"))?,
        decoded_size: field(strict_json::required_u64(object, "decodedSize"))?,
    })
}

fn validate_json_budgets(value: &Value) -> Result<()> {
    let mut nodes = 0_usize;
    validate_json_value(value, 0, &mut nodes)
}

fn validate_json_value(value: &Value, depth: usize, nodes: &mut usize) -> Result<()> {
    check_limit("manifest depth", depth as u64, MAX_MANIFEST_DEPTH as u64)?;
    *nodes = nodes
        .checked_add(1)
        .ok_or(MmdPackageError::IntegerOverflow("manifest node count"))?;
    check_limit("manifest nodes", *nodes as u64, MAX_MANIFEST_NODES as u64)?;

    match value {
        Value::String(string) => check_limit(
            "metadata string bytes",
            string.len() as u64,
            MAX_METADATA_STRING_BYTES as u64,
        ),
        Value::Array(values) => {
            check_limit(
                "manifest array items",
                values.len() as u64,
                MAX_MANIFEST_ARRAY_ITEMS as u64,
            )?;
            let child_depth = depth
                .checked_add(1)
                .ok_or(MmdPackageError::IntegerOverflow("manifest depth"))?;
            for value in values {
                validate_json_value(value, child_depth, nodes)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            check_limit(
                "manifest object fields",
                object.len() as u64,
                MAX_MANIFEST_OBJECT_FIELDS as u64,
            )?;
            let child_depth = depth
                .checked_add(1)
                .ok_or(MmdPackageError::IntegerOverflow("manifest depth"))?;
            for (key, value) in object {
                check_limit(
                    "metadata string bytes",
                    key.len() as u64,
                    MAX_METADATA_STRING_BYTES as u64,
                )?;
                validate_json_value(value, child_depth, nodes)?;
            }
            Ok(())
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn validate_entry_metadata(
    root: &Value,
    manifest: &MmdPackageManifest,
) -> Result<HashMap<u32, Ktx2TextureMetadata>> {
    let root_object = strict_json::object(root).expect("manifest root was checked");
    let license_keys = validate_licenses(root_object)?;
    validate_credits(root_object)?;
    let mut ktx2_metadata = HashMap::new();

    for entry in &manifest.entries {
        let object = manifest_entry_object(root, entry.id)?;
        match entry.kind {
            MmdPackageEntryKind::Motion => {
                let motion = object.get("motion").ok_or_else(|| {
                    MmdPackageError::InvalidManifest(format!(
                        "motion entry {} requires motion metadata",
                        entry.id
                    ))
                })?;
                validate_motion_metadata(motion, manifest)?;
            }
            _ if object.contains_key("motion") => {
                return invalid_manifest(format!(
                    "non-motion entry {} cannot contain motion metadata",
                    entry.id
                ));
            }
            _ => {}
        }

        if let Some(texture) = object.get("texture") {
            if entry.kind != MmdPackageEntryKind::Texture {
                return invalid_manifest(format!(
                    "non-texture entry {} cannot contain texture metadata",
                    entry.id
                ));
            }
            if entry.codec == "ktx2-uastc-v1" {
                let metadata = parse_ktx2_metadata(texture, entry.id)?;
                ktx2_metadata.insert(entry.id, metadata);
            } else {
                validate_texture_metadata(texture, entry)?;
            }
        } else if entry.kind == MmdPackageEntryKind::Texture {
            return invalid_manifest(format!(
                "texture entry {} requires texture metadata",
                entry.id
            ));
        }

        let media_type = object.get("mediaType");
        if let Some(media_type) = media_type {
            let media_type = media_type.as_str().ok_or_else(|| {
                MmdPackageError::InvalidManifest(format!(
                    "mediaType for entry {} must be a string",
                    entry.id
                ))
            })?;
            if media_type.is_empty() {
                return invalid_manifest(format!(
                    "mediaType for entry {} must not be empty",
                    entry.id
                ));
            }
        }
        if entry.kind == MmdPackageEntryKind::Audio && media_type.is_none() {
            return invalid_manifest(format!("audio entry {} requires mediaType", entry.id));
        }

        validate_license_refs(object, entry.id, license_keys.as_ref())?;
    }
    Ok(ktx2_metadata)
}

fn validate_licenses(root: &Map<String, Value>) -> Result<Option<HashSet<String>>> {
    let Some(value) = root.get("licenses") else {
        return Ok(None);
    };
    let licenses = value
        .as_object()
        .ok_or_else(|| MmdPackageError::InvalidManifest("licenses must be an object".into()))?;
    check_limit("license count", licenses.len() as u64, MAX_LICENSES as u64)?;
    let mut keys = HashSet::with_capacity(licenses.len());
    for (key, metadata) in licenses {
        if key.is_empty() {
            return invalid_manifest("license key must not be empty");
        }
        validate_license_metadata(metadata, key)?;
        keys.insert(key.clone());
    }
    Ok(Some(keys))
}

fn validate_credits(root: &Map<String, Value>) -> Result<()> {
    let Some(value) = root.get("credits") else {
        return Ok(());
    };
    let credits = value
        .as_array()
        .ok_or_else(|| MmdPackageError::InvalidManifest("credits must be an array".into()))?;
    check_limit("credit count", credits.len() as u64, MAX_CREDITS as u64)?;
    for credit in credits {
        validate_credit_metadata(credit)?;
    }
    Ok(())
}

fn validate_license_metadata(value: &Value, key: &str) -> Result<()> {
    let object = value.as_object().ok_or_else(|| {
        MmdPackageError::InvalidManifest(format!("license {key:?} must be an object"))
    })?;
    field(strict_json::required_str(object, "name"))?;
    for field_name in ["url", "text", "attribution", "notes"] {
        if let Some(value) = object.get(field_name) {
            value.as_str().ok_or_else(|| {
                MmdPackageError::InvalidManifest(format!(
                    "license {key:?} field {field_name:?} must be a string"
                ))
            })?;
        }
    }
    Ok(())
}

fn validate_credit_metadata(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| MmdPackageError::InvalidManifest("credit entry must be an object".into()))?;
    let subject = field(strict_json::required_str(object, "subject"))?;
    if !matches!(
        subject,
        "model"
            | "texture"
            | "motion"
            | "choreography"
            | "stage"
            | "accessory"
            | "music-composition"
            | "sound-recording"
            | "performer"
            | "other"
    ) {
        return invalid_manifest(format!("unsupported credit subject {subject:?}"));
    }
    field(strict_json::required_str(object, "name"))?;
    for field_name in ["author", "url"] {
        if let Some(value) = object.get(field_name) {
            value.as_str().ok_or_else(|| {
                MmdPackageError::InvalidManifest(format!(
                    "credit field {field_name:?} must be a string"
                ))
            })?;
        }
    }
    Ok(())
}

fn validate_license_refs(
    object: &Map<String, Value>,
    entry_id: u32,
    license_keys: Option<&HashSet<String>>,
) -> Result<()> {
    let Some(value) = object.get("licenseRefs") else {
        return Ok(());
    };
    let refs = value.as_array().ok_or_else(|| {
        MmdPackageError::InvalidManifest(format!(
            "licenseRefs for entry {entry_id} must be an array"
        ))
    })?;
    check_limit(
        "license references",
        refs.len() as u64,
        MAX_LICENSE_REFS_PER_ENTRY as u64,
    )?;
    let mut seen = HashSet::with_capacity(refs.len());
    for value in refs {
        let reference = value.as_str().ok_or_else(|| {
            MmdPackageError::InvalidManifest(format!(
                "licenseRefs for entry {entry_id} must contain strings"
            ))
        })?;
        if !seen.insert(reference) {
            return invalid_manifest(format!(
                "duplicate license reference {reference:?} for entry {entry_id}"
            ));
        }
        if !license_keys.is_some_and(|keys| keys.contains(reference)) {
            return invalid_manifest(format!(
                "license reference {reference:?} for entry {entry_id} is not declared"
            ));
        }
    }
    Ok(())
}

fn validate_motion_metadata(value: &Value, manifest: &MmdPackageManifest) -> Result<()> {
    let object = value.as_object().ok_or_else(|| {
        MmdPackageError::InvalidManifest("motion metadata must be an object".into())
    })?;
    let role = field(strict_json::required_str(object, "role"))?;
    if !matches!(role, "model" | "scene" | "mixed") {
        return invalid_manifest(format!("unsupported motion role {role:?}"));
    }
    let target = field(strict_json::optional_u64(object, "targetModelEntryId"))?
        .map(to_entry_id)
        .transpose()?;
    if role == "model" && target.is_none() {
        return invalid_manifest("model motion metadata requires targetModelEntryId");
    }
    if let Some(target) = target {
        let target_entry = manifest
            .entries
            .iter()
            .find(|entry| entry.id == target)
            .ok_or_else(|| {
                MmdPackageError::InvalidManifest(format!(
                    "motion targetModelEntryId references missing entry {target}"
                ))
            })?;
        if target_entry.kind != MmdPackageEntryKind::Model || target_entry.codec != "pmx" {
            return invalid_manifest("motion targetModelEntryId must reference a model/pmx entry");
        }
    }
    Ok(())
}

fn validate_texture_metadata(value: &Value, entry: &MmdPackageEntry) -> Result<()> {
    let object = value.as_object().ok_or_else(|| {
        MmdPackageError::InvalidManifest(format!(
            "texture metadata for entry {} must be an object",
            entry.id
        ))
    })?;
    match entry.codec.as_str() {
        "uastc-ldr-4x4-v1" => validate_raw_uastc_metadata(object, entry),
        "ktx2-uastc-v1" => parse_ktx2_metadata(value, entry.id).map(|_| ()),
        _ => Ok(()),
    }
}

fn validate_texture_summary(object: &Map<String, Value>) -> Result<(u32, u32, usize)> {
    let width = texture_dimension(object, "width")?;
    let height = texture_dimension(object, "height")?;
    let mip_count = field(strict_json::required_u64(object, "mipCount"))?;
    check_limit("texture mip count", mip_count, MAX_TEXTURE_MIP_COUNT as u64)?;
    let mip_count = usize::try_from(mip_count)
        .map_err(|_| MmdPackageError::IntegerOverflow("texture mip count"))?;
    if mip_count == 0 {
        return invalid_manifest("texture mipCount must be at least one");
    }
    texture_enum(object, "colorSpace", &["srgb", "linear"])?;
    texture_enum(object, "usage", &["color", "normal", "data", "toon"])?;
    texture_enum(object, "channelModel", &["r", "rg", "rgb", "rgba"])?;
    let swizzle = field(strict_json::required_str(object, "swizzle"))?;
    if swizzle.len() != 4
        || !swizzle
            .bytes()
            .all(|byte| matches!(byte, b'r' | b'g' | b'b' | b'a' | b'0' | b'1'))
    {
        return invalid_manifest("texture swizzle must be exactly four r/g/b/a/0/1 characters");
    }
    texture_enum(object, "alphaMode", &["straight"])?;
    texture_enum(object, "origin", &["top-left"])?;
    Ok((width, height, mip_count))
}

fn validate_raw_uastc_metadata(object: &Map<String, Value>, entry: &MmdPackageEntry) -> Result<()> {
    let (width, height, mip_count) = validate_texture_summary(object)?;
    texture_enum(object, "blockOrder", &["row-major-top-left"])?;
    let mips = field(strict_json::required(object, "mips"))?
        .as_array()
        .ok_or_else(|| MmdPackageError::InvalidManifest("texture mips must be an array".into()))?;
    check_limit(
        "texture mip count",
        mips.len() as u64,
        MAX_TEXTURE_MIP_COUNT as u64,
    )?;
    if mips.len() != mip_count {
        return invalid_manifest(format!(
            "texture mips length {} does not match mipCount {mip_count}",
            mips.len()
        ));
    }

    let mut expected_offset = 0_u64;
    let mut previous_width = width;
    let mut previous_height = height;
    for (level, mip) in mips.iter().enumerate() {
        let mip = mip.as_object().ok_or_else(|| {
            MmdPackageError::InvalidManifest("texture mip must be an object".into())
        })?;
        let mip_width = texture_dimension(mip, "width")?;
        let mip_height = texture_dimension(mip, "height")?;
        let expected_width = if level == 0 {
            width
        } else {
            (previous_width / 2).max(1)
        };
        let expected_height = if level == 0 {
            height
        } else {
            (previous_height / 2).max(1)
        };
        if mip_width != expected_width || mip_height != expected_height {
            return invalid_manifest(format!(
                "texture mip {level} dimensions {mip_width}x{mip_height} do not match expected {expected_width}x{expected_height}"
            ));
        }
        let blocks_width = (mip_width as u64).div_ceil(4);
        let blocks_height = (mip_height as u64).div_ceil(4);
        let expected_size = blocks_width
            .checked_mul(blocks_height)
            .and_then(|size| size.checked_mul(16))
            .ok_or(MmdPackageError::IntegerOverflow("raw UASTC mip size"))?;
        let offset = field(strict_json::required_u64(mip, "offset"))?;
        let size = field(strict_json::required_u64(mip, "size"))?;
        if offset != expected_offset {
            return invalid_manifest(format!(
                "texture mip {level} offset {offset} does not match contiguous offset {expected_offset}"
            ));
        }
        if size != expected_size {
            return invalid_manifest(format!(
                "texture mip {level} size {size} does not match expected raw UASTC size {expected_size}"
            ));
        }
        expected_offset = expected_offset
            .checked_add(size)
            .ok_or(MmdPackageError::IntegerOverflow("raw UASTC mip coverage"))?;
        previous_width = mip_width;
        previous_height = mip_height;
    }
    if expected_offset != entry.decoded_size {
        return invalid_manifest(format!(
            "raw UASTC mip coverage {expected_offset} does not match decodedSize {}",
            entry.decoded_size
        ));
    }
    Ok(())
}

fn parse_ktx2_metadata(value: &Value, entry_id: u32) -> Result<Ktx2TextureMetadata> {
    let object = value.as_object().ok_or_else(|| {
        MmdPackageError::InvalidManifest(format!(
            "texture metadata for entry {} must be an object",
            entry_id
        ))
    })?;
    let (width, height, mip_count) = validate_texture_summary(object)?;
    Ok(Ktx2TextureMetadata {
        width,
        height,
        mip_count,
        color_space: field(strict_json::required_str(object, "colorSpace"))?.to_owned(),
        channel_model: field(strict_json::required_str(object, "channelModel"))?.to_owned(),
        swizzle: field(strict_json::required_str(object, "swizzle"))?.to_owned(),
    })
}

fn validate_ktx2_payload(
    payload: &[u8],
    entry_compression: MmdPackageCompression,
    metadata: &Ktx2TextureMetadata,
) -> Result<()> {
    if payload.len() < KTX2_HEADER_LEN {
        return invalid_ktx2("payload is shorter than the KTX2 header");
    }
    if payload[..12] != *KTX2_IDENTIFIER {
        return invalid_ktx2("invalid KTX2 identifier");
    }

    let vk_format = ktx2_u32(payload, 12, "vkFormat")?;
    let type_size = ktx2_u32(payload, 16, "typeSize")?;
    let width = ktx2_u32(payload, 20, "pixelWidth")?;
    let height = ktx2_u32(payload, 24, "pixelHeight")?;
    let depth = ktx2_u32(payload, 28, "pixelDepth")?;
    let layer_count = ktx2_u32(payload, 32, "layerCount")?;
    let face_count = ktx2_u32(payload, 36, "faceCount")?;
    let level_count = ktx2_u32(payload, 40, "levelCount")?;
    let supercompression = ktx2_u32(payload, 44, "supercompressionScheme")?;
    let dfd_offset = ktx2_u32(payload, 48, "dfdByteOffset")? as u64;
    let dfd_length = ktx2_u32(payload, 52, "dfdByteLength")? as u64;
    let kvd_offset = ktx2_u32(payload, 56, "kvdByteOffset")? as u64;
    let kvd_length = ktx2_u32(payload, 60, "kvdByteLength")? as u64;
    let sgd_offset = ktx2_u64(payload, 64, "sgdByteOffset")?;
    let sgd_length = ktx2_u64(payload, 72, "sgdByteLength")?;

    if vk_format != 0 {
        return invalid_ktx2(format!(
            "UASTC KTX2 vkFormat must be undefined, got {vk_format}"
        ));
    }
    if type_size != 1 {
        return invalid_ktx2(format!("UASTC KTX2 typeSize must be 1, got {type_size}"));
    }
    if width != metadata.width || height != metadata.height {
        return invalid_ktx2(format!(
            "KTX2 dimensions {width}x{height} do not match manifest {}x{}",
            metadata.width, metadata.height
        ));
    }
    if depth != 0 {
        return invalid_ktx2(format!("2D KTX2 pixelDepth must be 0, got {depth}"));
    }
    if layer_count > 1 {
        return invalid_ktx2(format!(
            "KTX2 layerCount must be 0 or 1 for one effective layer, got {layer_count}"
        ));
    }
    if face_count != 1 {
        return invalid_ktx2(format!("2D KTX2 faceCount must be 1, got {face_count}"));
    }
    if level_count == 0 || level_count as usize != metadata.mip_count {
        return invalid_ktx2(format!(
            "KTX2 levelCount {level_count} does not match manifest mipCount {}",
            metadata.mip_count
        ));
    }
    if level_count as usize > MAX_TEXTURE_MIP_COUNT {
        return invalid_ktx2(format!(
            "KTX2 levelCount {level_count} exceeds the supported limit"
        ));
    }
    let mut max_dimension = width.max(height);
    let mut max_level_count = 1_u32;
    while max_dimension > 1 {
        max_dimension = (max_dimension / 2).max(1);
        max_level_count += 1;
    }
    if level_count > max_level_count {
        return invalid_ktx2(format!(
            "KTX2 levelCount {level_count} exceeds the mip chain for {width}x{height}"
        ));
    }
    if supercompression != 0 && supercompression != KTX2_ZSTD_SUPERCOMPRESSION {
        return invalid_ktx2(format!(
            "unsupported KTX2 supercompression scheme {supercompression}"
        ));
    }
    if supercompression == KTX2_ZSTD_SUPERCOMPRESSION
        && entry_compression != MmdPackageCompression::None
    {
        return invalid_ktx2("KTX2 internal Zstd requires MMDPACK entry compression none");
    }
    if sgd_length != 0 || sgd_offset != 0 {
        return invalid_ktx2("KTX2 UASTC must not contain supercompression global data");
    }

    let level_count = level_count as usize;
    let index_end = KTX2_HEADER_LEN
        .checked_add(
            level_count
                .checked_mul(KTX2_LEVEL_INDEX_ENTRY_LEN)
                .ok_or(MmdPackageError::IntegerOverflow("KTX2 level index"))?,
        )
        .ok_or(MmdPackageError::IntegerOverflow("KTX2 level index"))?;
    let mut ranges = vec![(0_u64, index_end as u64)];
    if !dfd_offset.is_multiple_of(4) {
        return invalid_ktx2("KTX2 dfdByteOffset must be 4-byte aligned");
    }
    add_ktx2_range(&mut ranges, payload, dfd_offset, dfd_length, "KTX2 DFD")?;
    if dfd_length != KTX2_DFD_SIZE as u64 {
        return invalid_ktx2(format!(
            "KTX2 UASTC DFD must be {KTX2_DFD_SIZE} bytes, got {dfd_length}"
        ));
    }
    if kvd_length != 0 {
        check_limit("KTX2 key/value bytes", kvd_length, KTX2_MAX_KVD_BYTES)?;
        if !kvd_offset.is_multiple_of(4) {
            return invalid_ktx2("KTX2 kvdByteOffset must be 4-byte aligned");
        }
        add_ktx2_range(
            &mut ranges,
            payload,
            kvd_offset,
            kvd_length,
            "KTX2 key/value data",
        )?;
    } else if kvd_offset != 0 {
        return invalid_ktx2("KTX2 kvdByteOffset must be zero when kvdByteLength is zero");
    }

    let dfd = ktx2_range(payload, dfd_offset, dfd_length, "KTX2 DFD")?;
    validate_ktx2_dfd(dfd, supercompression, metadata)?;
    let (swizzle, _orientation) = if kvd_length == 0 {
        ("rgba".to_owned(), "rd".to_owned())
    } else {
        parse_ktx2_kvd(ktx2_range(
            payload,
            kvd_offset,
            kvd_length,
            "KTX2 key/value data",
        )?)?
    };
    if swizzle != metadata.swizzle {
        return invalid_ktx2(format!(
            "KTX2 swizzle {swizzle:?} does not match manifest {:?}",
            metadata.swizzle
        ));
    }

    let mut mip_width = width;
    let mut mip_height = height;
    let mut levels = Vec::with_capacity(level_count);
    for level in 0..level_count {
        let level_index_offset = KTX2_HEADER_LEN
            .checked_add(
                level
                    .checked_mul(KTX2_LEVEL_INDEX_ENTRY_LEN)
                    .ok_or(MmdPackageError::IntegerOverflow("KTX2 level index offset"))?,
            )
            .ok_or(MmdPackageError::IntegerOverflow("KTX2 level index offset"))?;
        let byte_offset = ktx2_u64(payload, level_index_offset, "KTX2 level byteOffset")?;
        let byte_length = ktx2_u64(payload, level_index_offset + 8, "KTX2 level byteLength")?;
        let uncompressed_byte_length = ktx2_u64(
            payload,
            level_index_offset + 16,
            "KTX2 level uncompressedByteLength",
        )?;
        let expected_size = (mip_width as u64)
            .div_ceil(4)
            .checked_mul((mip_height as u64).div_ceil(4))
            .and_then(|size| size.checked_mul(16))
            .ok_or(MmdPackageError::IntegerOverflow("KTX2 UASTC mip size"))?;
        if uncompressed_byte_length != expected_size {
            return invalid_ktx2(format!(
                "KTX2 mip {level} uncompressed size {uncompressed_byte_length} does not match expected UASTC size {expected_size}"
            ));
        }
        if byte_length == 0 {
            return invalid_ktx2(format!("KTX2 mip {level} has an empty level"));
        }
        if supercompression == 0 && byte_length != uncompressed_byte_length {
            return invalid_ktx2(format!(
                "KTX2 mip {level} byteLength {byte_length} does not match uncompressedByteLength {uncompressed_byte_length}"
            ));
        }
        if supercompression == 0 && byte_offset % 16 != 0 {
            return invalid_ktx2(format!(
                "KTX2 uncompressed mip {level} offset {byte_offset} is not 16-byte aligned"
            ));
        }
        add_ktx2_range(
            &mut ranges,
            payload,
            byte_offset,
            byte_length,
            "KTX2 level data",
        )?;
        levels.push((byte_offset, byte_length, uncompressed_byte_length));
        mip_width = (mip_width / 2).max(1);
        mip_height = (mip_height / 2).max(1);
    }

    if supercompression == KTX2_ZSTD_SUPERCOMPRESSION {
        for (level, (byte_offset, byte_length, uncompressed_byte_length)) in
            levels.into_iter().enumerate()
        {
            let compressed = ktx2_range(payload, byte_offset, byte_length, "KTX2 level data")?;
            validate_ktx2_zstd_level(compressed, uncompressed_byte_length).map_err(|error| {
                match error {
                    MmdPackageError::InvalidKtx2(message) => {
                        MmdPackageError::InvalidKtx2(format!("mip {level}: {message}"))
                    }
                    other => other,
                }
            })?;
        }
    }
    Ok(())
}

fn validate_ktx2_dfd(
    dfd: &[u8],
    supercompression: u32,
    metadata: &Ktx2TextureMetadata,
) -> Result<()> {
    if ktx2_u32(dfd, 0, "DFD totalSize")? as usize != KTX2_DFD_SIZE {
        return invalid_ktx2("KTX2 DFD totalSize does not match its fixed UASTC block");
    }
    if ktx2_u32(dfd, 4, "DFD descriptor type")? != 0 {
        return invalid_ktx2("KTX2 UASTC DFD must be a vendor-neutral basic descriptor");
    }
    if ktx2_u16(dfd, 8, "DFD version")? != 2
        || ktx2_u16(dfd, 10, "DFD descriptor block size")? != KTX2_DFD_BLOCK_SIZE as u16
    {
        return invalid_ktx2("KTX2 UASTC DFD has an unsupported descriptor version or size");
    }
    if dfd[12] != 166 {
        return invalid_ktx2(format!(
            "KTX2 DFD color model must be UASTC (166), got {}",
            dfd[12]
        ));
    }
    if dfd[13] != 0 && dfd[13] != 1 {
        return invalid_ktx2(format!(
            "KTX2 UASTC DFD color primaries {} are outside this profile",
            dfd[13]
        ));
    }
    let expected_transfer = match metadata.color_space.as_str() {
        "linear" => 1,
        "srgb" => 2,
        _ => unreachable!("texture summary validates colorSpace"),
    };
    if dfd[14] != expected_transfer {
        return invalid_ktx2(format!(
            "KTX2 DFD transfer function {} does not match manifest {}",
            dfd[14], metadata.color_space
        ));
    }
    if dfd[15] & 1 != 0 {
        return invalid_ktx2("KTX2 UASTC DFD marks alpha as premultiplied");
    }
    if dfd[16..20] != [3, 3, 0, 0] {
        return invalid_ktx2("KTX2 UASTC DFD must describe 4x4 texel blocks");
    }
    if dfd[21..28].iter().any(|byte| *byte != 0)
        || (supercompression == 0 && dfd[20] != 16)
        || (supercompression == KTX2_ZSTD_SUPERCOMPRESSION && dfd[20] != 0 && dfd[20] != 16)
    {
        return invalid_ktx2("KTX2 UASTC DFD has an invalid bytesPlane layout");
    }

    let sample = &dfd[28..44];
    let sample_word = u32::from_le_bytes(sample[..4].try_into().unwrap());
    let bit_offset = sample_word as u16;
    let bit_length = ((sample_word >> 16) & 0xff) as u8;
    let channel_type = (sample_word >> 24) as u8;
    let channel_id = channel_type & 0x0f;
    if bit_offset != 0
        || bit_length != 127
        || channel_type & 0x80 != 0
        || sample[4..8].iter().any(|byte| *byte != 0)
        || sample[8..12].iter().any(|byte| *byte != 0)
        || sample[12..16] != [0xff; 4]
    {
        return invalid_ktx2("KTX2 UASTC DFD sample does not describe one LDR 128-bit block");
    }
    let dfd_channel_model = match channel_id {
        0 => "rgb",
        3 => "rgba",
        4 => "r",
        5 | 6 => "rg",
        other => {
            return invalid_ktx2(format!("unsupported KTX2 UASTC DFD channel id {other}"));
        }
    };
    if dfd_channel_model != metadata.channel_model {
        return invalid_ktx2(format!(
            "KTX2 DFD channel model {dfd_channel_model:?} does not match manifest {:?}",
            metadata.channel_model
        ));
    }
    Ok(())
}

fn parse_ktx2_kvd(kvd: &[u8]) -> Result<(String, String)> {
    let mut cursor = 0_usize;
    let mut seen_keys: HashSet<&[u8]> = HashSet::new();
    let mut swizzle = "rgba".to_owned();
    let mut orientation = "rd".to_owned();
    while cursor < kvd.len() {
        let length_end = cursor
            .checked_add(4)
            .ok_or(MmdPackageError::IntegerOverflow("KTX2 key/value length"))?;
        let length = usize::try_from(ktx2_u32(kvd, cursor, "keyAndValueByteLength")?)
            .map_err(|_| MmdPackageError::IntegerOverflow("KTX2 key/value length"))?;
        let value_start = length_end;
        let value_end = value_start
            .checked_add(length)
            .ok_or(MmdPackageError::IntegerOverflow("KTX2 key/value end"))?;
        if length < 2 || value_end > kvd.len() {
            return invalid_ktx2("KTX2 key/value pair is truncated or empty");
        }
        let pair = &kvd[value_start..value_end];
        let nul = pair
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| MmdPackageError::InvalidKtx2("KTX2 key is not NUL-terminated".into()))?;
        if nul == 0 {
            return invalid_ktx2("KTX2 key must not be empty");
        }
        let key = &pair[..nul];
        if !seen_keys.insert(key) {
            return invalid_ktx2("KTX2 key/value keys must be unique");
        }
        let value = &pair[nul + 1..];
        let padding = (4 - (length % 4)) % 4;
        let padded_end = value_end
            .checked_add(padding)
            .ok_or(MmdPackageError::IntegerOverflow("KTX2 key/value padding"))?;
        if padded_end > kvd.len() || kvd[value_end..padded_end].iter().any(|byte| *byte != 0) {
            return invalid_ktx2("KTX2 key/value padding is truncated or nonzero");
        }

        match key {
            b"KTXswizzle" => {
                swizzle = ktx2_metadata_string(value, "KTXswizzle")?;
                if swizzle.len() != 4
                    || !swizzle
                        .bytes()
                        .all(|byte| matches!(byte, b'r' | b'g' | b'b' | b'a' | b'0' | b'1'))
                {
                    return invalid_ktx2("KTXswizzle must contain four r/g/b/a/0/1 characters");
                }
            }
            b"KTXorientation" => {
                orientation = ktx2_metadata_string(value, "KTXorientation")?;
                if orientation != "rd" {
                    return invalid_ktx2(format!(
                        "KTXorientation {orientation:?} is not top-left rd"
                    ));
                }
            }
            _ => {}
        }
        cursor = padded_end;
    }
    Ok((swizzle, orientation))
}

fn ktx2_metadata_string(value: &[u8], name: &'static str) -> Result<String> {
    let value = value
        .strip_suffix(&[0])
        .ok_or_else(|| MmdPackageError::InvalidKtx2(format!("{name} must be NUL-terminated")))?;
    std::str::from_utf8(value)
        .map(str::to_owned)
        .map_err(|_| MmdPackageError::InvalidKtx2(format!("{name} must be UTF-8")))
}

fn validate_ktx2_zstd_level(compressed: &[u8], expected: u64) -> Result<()> {
    let frame_size = zstd::zstd_safe::find_frame_compressed_size(compressed)
        .map_err(|code| MmdPackageError::InvalidKtx2(format!("invalid KTX2 Zstd frame: {code}")))?;
    if frame_size != compressed.len() {
        return invalid_ktx2("KTX2 Zstd level must contain exactly one frame");
    }
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(compressed))
        .map_err(|error| MmdPackageError::InvalidKtx2(error.to_string()))?;
    decoder
        .window_log_max(26)
        .map_err(|error| MmdPackageError::InvalidKtx2(error.to_string()))?;
    let mut limited = decoder.take(expected.saturating_add(1));
    let total = std::io::copy(&mut limited, &mut std::io::sink())
        .map_err(|error| MmdPackageError::InvalidKtx2(error.to_string()))?;
    if total != expected {
        return invalid_ktx2(format!(
            "KTX2 Zstd level expanded to {total} bytes, expected {expected}"
        ));
    }
    Ok(())
}

fn add_ktx2_range(
    ranges: &mut Vec<(u64, u64)>,
    payload: &[u8],
    offset: u64,
    length: u64,
    name: &'static str,
) -> Result<()> {
    let end = offset
        .checked_add(length)
        .ok_or(MmdPackageError::IntegerOverflow("KTX2 section end"))?;
    ktx2_range(payload, offset, length, name)?;
    if length != 0
        && ranges
            .iter()
            .any(|(start, previous_end)| offset < *previous_end && *start < end)
    {
        return invalid_ktx2(format!("KTX2 {name} overlaps another declared section"));
    }
    if length != 0 {
        ranges.push((offset, end));
    }
    Ok(())
}

fn ktx2_range<'a>(
    payload: &'a [u8],
    offset: u64,
    length: u64,
    name: &'static str,
) -> Result<&'a [u8]> {
    let start = usize::try_from(offset)
        .map_err(|_| MmdPackageError::IntegerOverflow("KTX2 section start"))?;
    let length = usize::try_from(length)
        .map_err(|_| MmdPackageError::IntegerOverflow("KTX2 section length"))?;
    let end = start
        .checked_add(length)
        .ok_or(MmdPackageError::IntegerOverflow("KTX2 section end"))?;
    payload
        .get(start..end)
        .ok_or_else(|| MmdPackageError::InvalidKtx2(format!("KTX2 {name} is outside the payload")))
}

fn ktx2_u16(bytes: &[u8], offset: usize, name: &'static str) -> Result<u16> {
    let end = offset
        .checked_add(2)
        .ok_or(MmdPackageError::IntegerOverflow("KTX2 field end"))?;
    let bytes = bytes
        .get(offset..end)
        .ok_or_else(|| MmdPackageError::InvalidKtx2(format!("KTX2 {name} is truncated")))?;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
}

fn ktx2_u32(bytes: &[u8], offset: usize, name: &'static str) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(MmdPackageError::IntegerOverflow("KTX2 field end"))?;
    let bytes = bytes
        .get(offset..end)
        .ok_or_else(|| MmdPackageError::InvalidKtx2(format!("KTX2 {name} is truncated")))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn ktx2_u64(bytes: &[u8], offset: usize, name: &'static str) -> Result<u64> {
    let end = offset
        .checked_add(8)
        .ok_or(MmdPackageError::IntegerOverflow("KTX2 field end"))?;
    let bytes = bytes
        .get(offset..end)
        .ok_or_else(|| MmdPackageError::InvalidKtx2(format!("KTX2 {name} is truncated")))?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

fn invalid_ktx2<T>(message: impl Into<String>) -> Result<T> {
    Err(MmdPackageError::InvalidKtx2(message.into()))
}

fn texture_dimension(object: &Map<String, Value>, field_name: &'static str) -> Result<u32> {
    let value = field(strict_json::required_u64(object, field_name))?;
    check_limit("texture dimension", value, MAX_TEXTURE_DIMENSION as u64)?;
    let value =
        u32::try_from(value).map_err(|_| MmdPackageError::IntegerOverflow("texture dimension"))?;
    if value == 0 {
        return invalid_manifest(format!("texture {field_name} must be at least one"));
    }
    Ok(value)
}

fn texture_enum(
    object: &Map<String, Value>,
    field_name: &'static str,
    allowed: &[&str],
) -> Result<()> {
    let value = field(strict_json::required_str(object, field_name))?;
    if allowed.contains(&value) {
        Ok(())
    } else {
        invalid_manifest(format!("unsupported texture {field_name} {value:?}"))
    }
}

fn validate_layout(
    entries: &[MmdPackageEntry],
    package_len: usize,
    payload_base: usize,
    limits: &MmdPackageLimits,
) -> Result<()> {
    let mut order: Vec<_> = entries.iter().collect();
    order.sort_unstable_by_key(|entry| entry.offset);
    let mut expected_offset = 0_u64;
    let mut total_decoded = 0_u64;
    for entry in order {
        if entry.offset != expected_offset {
            return invalid_manifest(format!(
                "entry {} begins at {}, expected contiguous offset {}",
                entry.id, entry.offset, expected_offset
            ));
        }
        if entry.cipher_size < GCM_TAG_LEN {
            return invalid_manifest(format!(
                "entry {} ciphertext is shorter than the GCM tag",
                entry.id
            ));
        }
        check_limit(
            "entry ciphertext bytes",
            entry.cipher_size,
            limits.max_entry_cipher_bytes,
        )?;
        check_limit(
            "entry decoded bytes",
            entry.decoded_size,
            limits.max_entry_decoded_bytes,
        )?;
        expected_offset = expected_offset
            .checked_add(entry.cipher_size)
            .ok_or(MmdPackageError::IntegerOverflow("payload coverage"))?;
        total_decoded = total_decoded
            .checked_add(entry.decoded_size)
            .ok_or(MmdPackageError::IntegerOverflow("total decoded bytes"))?;
        check_limit(
            "total decoded bytes",
            total_decoded,
            limits.max_total_decoded_bytes,
        )?;
    }

    let actual_payload_len = package_len
        .checked_sub(payload_base)
        .ok_or(MmdPackageError::Truncated("payload base"))? as u64;
    if expected_offset != actual_payload_len {
        return invalid_manifest(format!(
            "entry table covers {expected_offset} payload bytes, file contains {actual_payload_len}"
        ));
    }
    Ok(())
}

fn validate_defaults(root: &Value, manifest: &MmdPackageManifest) -> Result<Vec<MmdModelBinding>> {
    let default_model = manifest
        .entries
        .iter()
        .find(|entry| entry.id == manifest.default_model_entry_id)
        .ok_or_else(|| {
            MmdPackageError::InvalidManifest("defaultModelEntryId does not exist".into())
        })?;
    if default_model.kind != MmdPackageEntryKind::Model || default_model.codec != "pmx" {
        return invalid_manifest("defaultModelEntryId must reference a model/pmx entry");
    }

    if let Some(id) = manifest.default_motion_entry_id {
        let motion = manifest
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or_else(|| {
                MmdPackageError::InvalidManifest("defaultMotionEntryId does not exist".into())
            })?;
        if motion.kind != MmdPackageEntryKind::Motion || motion.codec != "vmd" {
            return invalid_manifest("defaultMotionEntryId must reference a motion/vmd entry");
        }
        let motion_object = manifest_entry_object(root, id)?;
        let metadata = field(strict_json::required(motion_object, "motion"))?
            .as_object()
            .ok_or_else(|| {
                MmdPackageError::InvalidManifest("motion metadata must be an object".into())
            })?;
        if field(strict_json::required_str(metadata, "role"))? != "model" {
            return invalid_manifest("default motion role must be model");
        }
        let target = to_entry_id(field(strict_json::required_u64(
            metadata,
            "targetModelEntryId",
        ))?)?;
        if target != manifest.default_model_entry_id {
            return invalid_manifest("default motion must target defaultModelEntryId");
        }
    }

    let root = strict_json::object(root).expect("manifest root was checked");
    let bindings = field(strict_json::required(root, "modelBindings"))?
        .as_array()
        .ok_or_else(|| MmdPackageError::InvalidManifest("modelBindings must be an array".into()))?;
    let mut bound_models = HashSet::with_capacity(bindings.len());
    let mut model_bindings = Vec::with_capacity(bindings.len());
    let mut matching = 0;
    for binding in bindings {
        let binding = binding.as_object().ok_or_else(|| {
            MmdPackageError::InvalidManifest("model binding must be an object".into())
        })?;
        let model_id = to_entry_id(field(strict_json::required_u64(binding, "modelEntryId"))?)?;
        if !bound_models.insert(model_id) {
            return invalid_manifest(format!("duplicate model binding for entry {model_id}"));
        }
        let model = manifest
            .entries
            .iter()
            .find(|entry| entry.id == model_id)
            .ok_or_else(|| {
                MmdPackageError::InvalidManifest(format!(
                    "model binding references missing entry {model_id}"
                ))
            })?;
        if model.kind != MmdPackageEntryKind::Model || model.codec != "pmx" {
            return invalid_manifest(format!("model binding entry {model_id} must be model/pmx"));
        }
        if model_id == manifest.default_model_entry_id {
            matching += 1;
        }
        let texture_bindings = validate_texture_bindings(binding, manifest)?;
        model_bindings.push(MmdModelBinding {
            model_entry_id: model_id,
            texture_bindings,
        });
    }
    if matching != 1 {
        return invalid_manifest("defaultModelEntryId must have exactly one modelBindings entry");
    }
    Ok(model_bindings)
}

fn manifest_entry_object(root: &Value, id: u32) -> Result<&Map<String, Value>> {
    let root = strict_json::object(root).expect("manifest root was checked");
    let entries = field(strict_json::required(root, "entries"))?
        .as_array()
        .expect("entries were checked");
    entries
        .iter()
        .find_map(|value| {
            let object = value.as_object()?;
            (object.get("id")?.as_u64() == Some(id as u64)).then_some(object)
        })
        .ok_or_else(|| MmdPackageError::InvalidManifest(format!("entry {id} does not exist")))
}

fn validate_texture_bindings(
    binding: &Map<String, Value>,
    manifest: &MmdPackageManifest,
) -> Result<Vec<MmdTextureBinding>> {
    let textures = field(strict_json::required(binding, "textureBindings"))?
        .as_array()
        .ok_or_else(|| {
            MmdPackageError::InvalidManifest("textureBindings must be an array".into())
        })?;
    let mut indices = HashSet::with_capacity(textures.len());
    let mut parsed = Vec::with_capacity(textures.len());
    for texture in textures {
        let texture = texture.as_object().ok_or_else(|| {
            MmdPackageError::InvalidManifest("texture binding must be an object".into())
        })?;
        let index = u32::try_from(field(strict_json::required_u64(texture, "textureIndex"))?)
            .map_err(|_| MmdPackageError::InvalidManifest("textureIndex exceeds u32".into()))?;
        if !indices.insert(index) {
            return invalid_manifest(format!("duplicate textureIndex {index}"));
        }
        let entry_id = to_entry_id(field(strict_json::required_u64(texture, "entryId"))?)?;
        let entry = manifest
            .entries
            .iter()
            .find(|entry| entry.id == entry_id)
            .ok_or_else(|| {
                MmdPackageError::InvalidManifest(format!(
                    "texture binding references missing entry {entry_id}"
                ))
            })?;
        if entry.kind != MmdPackageEntryKind::Texture {
            return invalid_manifest(format!(
                "texture binding entry {entry_id} must have kind texture"
            ));
        }
        parsed.push(MmdTextureBinding {
            texture_index: index,
            entry_id,
        });
    }
    Ok(parsed)
}

fn parse_kind(value: &str) -> Result<MmdPackageEntryKind> {
    MmdPackageEntryKind::from_token(value)
        .ok_or_else(|| MmdPackageError::InvalidManifest(format!("unknown entry kind {value:?}")))
}

fn validate_known_kind_codec(kind: MmdPackageEntryKind, codec: &str) -> Result<()> {
    let valid = match codec {
        "pmx" => kind == MmdPackageEntryKind::Model,
        "vmd" => kind == MmdPackageEntryKind::Motion,
        "json" | "utf8" => kind == MmdPackageEntryKind::Metadata,
        "opaque" => matches!(
            kind,
            MmdPackageEntryKind::Audio | MmdPackageEntryKind::Binary
        ),
        "uastc-ldr-4x4-v1" | "ktx2-uastc-v1" => kind == MmdPackageEntryKind::Texture,
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        invalid_manifest(format!("codec {codec:?} is invalid for kind {kind:?}"))
    }
}

fn is_known_codec(codec: &str) -> bool {
    matches!(
        codec,
        "pmx" | "vmd" | "json" | "utf8" | "opaque" | "uastc-ldr-4x4-v1" | "ktx2-uastc-v1"
    )
}

fn validate_codec_token(codec: &str) -> Result<()> {
    if codec.is_empty()
        || codec.len() > 63
        || !codec.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        return invalid_manifest("codec must be a 1..63 byte lowercase ASCII token");
    }
    Ok(())
}

fn validate_path(path: &str, max_bytes: usize) -> Result<()> {
    if path.is_empty()
        || path.len() > max_bytes
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || !path.nfc().eq(path.chars())
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return invalid_manifest(format!(
            "path {path:?} is not normalized or exceeds its limit"
        ));
    }
    Ok(())
}

fn decode_zstd(compressed: &[u8], decoded_size: u64) -> Result<Vec<u8>> {
    if compressed.get(..4) != Some(&ZSTD_MAGIC) {
        return Err(MmdPackageError::InvalidZstd(
            "standard frame magic is required".into(),
        ));
    }
    let descriptor = *compressed
        .get(4)
        .ok_or_else(|| MmdPackageError::InvalidZstd("frame header is truncated".into()))?;
    if descriptor & 0x04 != 0 {
        return Err(MmdPackageError::InvalidZstd(
            "content checksum must be disabled".into(),
        ));
    }
    if descriptor & 0x03 != 0 {
        return Err(MmdPackageError::InvalidZstd(
            "dictionary IDs are not allowed".into(),
        ));
    }
    let frame_size = zstd::zstd_safe::find_frame_compressed_size(compressed).map_err(|code| {
        MmdPackageError::InvalidZstd(format!("invalid frame: error code {code}"))
    })?;
    if frame_size != compressed.len() {
        return Err(MmdPackageError::InvalidZstd(
            "exactly one frame with no trailing bytes is required".into(),
        ));
    }
    let content_size = zstd::zstd_safe::get_frame_content_size(compressed)
        .map_err(|_| MmdPackageError::InvalidZstd("invalid frame content size".into()))?
        .ok_or_else(|| MmdPackageError::InvalidZstd("content size is required".into()))?;
    if content_size != decoded_size {
        return Err(MmdPackageError::InvalidZstd(format!(
            "frame content size {content_size} does not match decodedSize {decoded_size}"
        )));
    }

    let capacity = usize_from_u64(decoded_size, "decoded buffer")?;
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(compressed))?;
    decoder.window_log_max(26)?;
    let mut limited = decoder.single_frame().take(decoded_size.saturating_add(1));
    let mut decoded = Vec::with_capacity(capacity);
    limited
        .read_to_end(&mut decoded)
        .map_err(|error| MmdPackageError::InvalidZstd(error.to_string()))?;
    require_decoded_size(decoded.len(), decoded_size)?;
    Ok(decoded)
}

fn require_decoded_size(actual: usize, expected: u64) -> Result<()> {
    if actual as u64 != expected {
        return invalid_manifest(format!(
            "decoded payload size {actual} does not match decodedSize {expected}"
        ));
    }
    Ok(())
}

fn manifest_nonce(prefix: &[u8; 8]) -> [u8; 12] {
    let mut nonce = [0_u8; 12];
    nonce[..8].copy_from_slice(prefix);
    nonce
}

fn entry_nonce(prefix: &[u8; 8], id: u32) -> [u8; 12] {
    let mut nonce = manifest_nonce(prefix);
    nonce[8..].copy_from_slice(&id.to_le_bytes());
    nonce
}

fn entry_aad(package_id: &[u8; 16], entry: &MmdPackageEntry) -> [u8; 38] {
    let mut aad = [0_u8; 38];
    aad[..10].copy_from_slice(ENTRY_AAD_PREFIX);
    aad[10..26].copy_from_slice(package_id);
    aad[26..30].copy_from_slice(&entry.id.to_le_bytes());
    aad[30..38].copy_from_slice(&entry.decoded_size.to_le_bytes());
    aad
}

fn check_limit(what: &'static str, actual: u64, limit: u64) -> Result<()> {
    if actual > limit {
        Err(MmdPackageError::LimitExceeded {
            what,
            actual,
            limit,
        })
    } else {
        Ok(())
    }
}

fn to_entry_id(value: u64) -> Result<u32> {
    let id = u32::try_from(value)
        .map_err(|_| MmdPackageError::InvalidManifest("entry id exceeds u32".into()))?;
    if id == 0 {
        return invalid_manifest("entry id 0 is reserved for the manifest");
    }
    Ok(id)
}

fn usize_from_u64(value: u64, what: &'static str) -> Result<usize> {
    usize::try_from(value).map_err(|_| MmdPackageError::IntegerOverflow(what))
}

fn field<T>(value: std::result::Result<T, String>) -> Result<T> {
    value.map_err(MmdPackageError::InvalidManifest)
}

fn invalid_manifest<T>(message: impl Into<String>) -> Result<T> {
    Err(MmdPackageError::InvalidManifest(message.into()))
}

impl From<std::io::Error> for MmdPackageError {
    fn from(error: std::io::Error) -> Self {
        Self::InvalidZstd(error.to_string())
    }
}
