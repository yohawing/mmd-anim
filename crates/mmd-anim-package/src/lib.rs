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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmdPackageCompression {
    None,
    ZstdV1,
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

/// Authenticated package with lazy, per-entry decoding.
pub struct MmdPackage {
    bytes: Arc<[u8]>,
    key: Zeroizing<[u8; 32]>,
    header: MmdPackageHeader,
    manifest: MmdPackageManifest,
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
        let manifest = parse_manifest(&manifest_value, &limits)?;
        validate_layout(&manifest.entries, bytes.len(), manifest_end, &limits)?;
        validate_defaults(&manifest_value, &manifest)?;

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

    /// Authenticates and decodes one entry by its stable ID.
    pub fn read_entry(&self, id: u32) -> Result<Vec<u8>> {
        let index = *self
            .entries_by_id
            .get(&id)
            .ok_or(MmdPackageError::EntryNotFound(id))?;
        self.read_index(index)
    }

    /// Authenticates and decodes one entry by its exact, case-sensitive path.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let index = *self
            .entries_by_path
            .get(path)
            .ok_or_else(|| MmdPackageError::PathNotFound(path.to_owned()))?;
        self.read_index(index)
    }

    fn read_index(&self, index: usize) -> Result<Vec<u8>> {
        let entry = &self.manifest.entries[index];
        if !is_known_codec(&entry.codec) {
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

        match entry.compression {
            MmdPackageCompression::None => {
                require_decoded_size(plaintext.len(), entry.decoded_size)?;
                Ok(plaintext)
            }
            MmdPackageCompression::ZstdV1 => decode_zstd(&plaintext, entry.decoded_size),
        }
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
    let compression = match field(strict_json::required_str(object, "compression"))? {
        "none" => MmdPackageCompression::None,
        "zstd-v1" => MmdPackageCompression::ZstdV1,
        other => return invalid_manifest(format!("unsupported compression {other:?}")),
    };

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

fn validate_defaults(root: &Value, manifest: &MmdPackageManifest) -> Result<()> {
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
        validate_texture_bindings(binding, manifest)?;
    }
    if matching != 1 {
        return invalid_manifest("defaultModelEntryId must have exactly one modelBindings entry");
    }
    Ok(())
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
) -> Result<()> {
    let textures = field(strict_json::required(binding, "textureBindings"))?
        .as_array()
        .ok_or_else(|| {
            MmdPackageError::InvalidManifest("textureBindings must be an array".into())
        })?;
    let mut indices = HashSet::with_capacity(textures.len());
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
    }
    Ok(())
}

fn parse_kind(value: &str) -> Result<MmdPackageEntryKind> {
    match value {
        "model" => Ok(MmdPackageEntryKind::Model),
        "motion" => Ok(MmdPackageEntryKind::Motion),
        "texture" => Ok(MmdPackageEntryKind::Texture),
        "metadata" => Ok(MmdPackageEntryKind::Metadata),
        "audio" => Ok(MmdPackageEntryKind::Audio),
        "binary" => Ok(MmdPackageEntryKind::Binary),
        other => invalid_manifest(format!("unknown entry kind {other:?}")),
    }
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
