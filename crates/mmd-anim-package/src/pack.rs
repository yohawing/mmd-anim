use std::sync::Arc;

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use rand_core::{OsRng, RngCore};
use serde_json::{Map, Value};
use thiserror::Error;
use zeroize::Zeroizing;

use super::{
    GCM_TAG_LEN, MMDPACK_HEADER_LEN, MmdPackage, MmdPackageCompression, MmdPackageEntry,
    MmdPackageEntryKind, MmdPackageError, MmdPackageLimits, check_limit, entry_aad, entry_nonce,
    manifest_nonce, parse_ktx2_metadata, validate_ktx2_payload,
};

#[derive(Debug, Error)]
pub enum MmdPackagePackError {
    #[error("operating-system randomness is unavailable: {0}")]
    RandomnessUnavailable(String),
    #[error("package creation failed: {0}")]
    PackingFailed(String),
    #[error(transparent)]
    Package(#[from] MmdPackageError),
}

type PackResult<T> = std::result::Result<T, MmdPackagePackError>;

/// Compression requested for one already codec-encoded payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmdPackagePackCompression {
    None,
    ZstdV1,
    /// Uses Zstd only when its complete frame is smaller than the input.
    AutoZstdV1,
}

impl MmdPackagePackCompression {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ZstdV1 => "zstd-v1",
            Self::AutoZstdV1 => "auto-zstd-v1",
        }
    }

    pub fn from_token(value: &str) -> Option<Self> {
        [Self::None, Self::ZstdV1, Self::AutoZstdV1]
            .into_iter()
            .find(|compression| compression.as_str() == value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MmdPackageMotionMetadata {
    pub role: MmdPackageMotionRole,
    pub target_model_entry_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MmdPackageMotionRole {
    Model,
    Scene,
    Mixed,
}

impl MmdPackageMotionRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Scene => "scene",
            Self::Mixed => "mixed",
        }
    }

    pub fn from_token(value: &str) -> Option<Self> {
        [Self::Model, Self::Scene, Self::Mixed]
            .into_iter()
            .find(|role| role.as_str() == value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MmdTextureBinding {
    pub texture_index: u32,
    pub entry_id: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MmdModelBinding {
    pub model_entry_id: u32,
    pub texture_bindings: Vec<MmdTextureBinding>,
}

/// One payload passed to the low-level packer after codec encoding.
#[derive(Clone, Debug)]
pub struct MmdPackagePackEntry {
    pub id: u32,
    pub path: String,
    pub kind: MmdPackageEntryKind,
    pub codec: String,
    pub compression: MmdPackagePackCompression,
    pub decoded: Vec<u8>,
    pub media_type: Option<String>,
    pub motion: Option<MmdPackageMotionMetadata>,
    /// Draft codec-specific texture metadata. Package-layer shape validation
    /// applies to known UASTC/KTX2 profiles.
    pub texture: Option<Value>,
}

/// Complete low-level input. PMX parsing and texture-index discovery are not
/// performed by this type.
#[derive(Clone, Debug)]
pub struct MmdPackagePackInput {
    pub default_model_entry_id: u32,
    pub default_motion_entry_id: Option<u32>,
    pub entries: Vec<MmdPackagePackEntry>,
    pub model_bindings: Vec<MmdModelBinding>,
}

/// Newly encrypted bytes and their newly generated content-encryption key.
pub struct MmdPackedPackage {
    bytes: Arc<[u8]>,
    key: Zeroizing<[u8; 32]>,
    package_id: [u8; 16],
}

impl MmdPackedPackage {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn key(&self) -> &[u8; 32] {
        &self.key
    }

    pub fn package_id(&self) -> [u8; 16] {
        self.package_id
    }

    /// Transfers the package bytes and a copy of the key to the caller.
    /// The caller becomes responsible for clearing the returned key.
    pub fn into_parts(self) -> (Arc<[u8]>, [u8; 32]) {
        let key = *self.key;
        (self.bytes, key)
    }
}

pub struct MmdPackagePacker;

impl MmdPackagePacker {
    /// Creates a package with fresh OS-generated key, UUIDv4 package ID, and
    /// nonce prefix, then re-opens it with the reader before returning it.
    pub fn pack(
        input: MmdPackagePackInput,
        limits: MmdPackageLimits,
    ) -> PackResult<MmdPackedPackage> {
        let mut random = OsRng;
        Self::pack_with_rng(input, limits, &mut random)
    }

    fn pack_with_rng<R: RngCore>(
        mut input: MmdPackagePackInput,
        limits: MmdPackageLimits,
        random: &mut R,
    ) -> PackResult<MmdPackedPackage> {
        validate_input_sizes(&input, &limits)?;
        validate_input_metadata(&input)?;
        let mut key = Zeroizing::new([0_u8; 32]);
        let mut package_id = [0_u8; 16];
        let mut nonce_prefix = [0_u8; 8];
        random
            .try_fill_bytes(key.as_mut())
            .map_err(|error| MmdPackagePackError::RandomnessUnavailable(error.to_string()))?;
        random
            .try_fill_bytes(&mut package_id)
            .map_err(|error| MmdPackagePackError::RandomnessUnavailable(error.to_string()))?;
        random
            .try_fill_bytes(&mut nonce_prefix)
            .map_err(|error| MmdPackagePackError::RandomnessUnavailable(error.to_string()))?;
        package_id[6] = (package_id[6] & 0x0f) | 0x40;
        package_id[8] = (package_id[8] & 0x3f) | 0x80;

        let (entries, encoded) = encode_entries(&mut input.entries)?;
        for entry in &entries {
            check_limit(
                "entry ciphertext bytes",
                entry.cipher_size,
                limits.max_entry_cipher_bytes,
            )?;
        }
        let manifest = build_manifest(&input, &entries)?;
        let manifest_plaintext = serde_json::to_vec(&manifest)
            .map_err(|error| MmdPackagePackError::PackingFailed(error.to_string()))?;
        let manifest_cipher_size = (manifest_plaintext.len() as u64)
            .checked_add(GCM_TAG_LEN)
            .ok_or(MmdPackageError::IntegerOverflow("manifest cipher size"))?;
        check_limit(
            "manifest ciphertext bytes",
            manifest_cipher_size,
            limits.max_manifest_cipher_bytes,
        )?;
        let header = build_header(package_id, nonce_prefix, manifest_cipher_size);
        let cipher = Aes256Gcm::new_from_slice(key.as_ref()).expect("AES-256 key length is fixed");
        let manifest_ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&manifest_nonce(&nonce_prefix)),
                Payload {
                    msg: &manifest_plaintext,
                    aad: &header,
                },
            )
            .map_err(|_| MmdPackagePackError::PackingFailed("manifest encryption failed".into()))?;

        let payload_size =
            entries.iter().try_fold(0_usize, |total, entry| {
                total
                    .checked_add(usize::try_from(entry.cipher_size).map_err(|_| {
                        MmdPackageError::IntegerOverflow("packed entry cipher size")
                    })?)
                    .ok_or(MmdPackageError::IntegerOverflow("packed payload size"))
            })?;
        let capacity = MMDPACK_HEADER_LEN
            .checked_add(manifest_ciphertext.len())
            .and_then(|size| size.checked_add(payload_size))
            .ok_or(MmdPackageError::IntegerOverflow("packed package size"))?;
        check_limit("package bytes", capacity as u64, limits.max_package_bytes)?;
        let mut bytes = Vec::with_capacity(capacity);
        bytes.extend_from_slice(&header);
        bytes.extend_from_slice(&manifest_ciphertext);
        for (entry, encoded) in entries.iter().zip(encoded) {
            let ciphertext = cipher
                .encrypt(
                    Nonce::from_slice(&entry_nonce(&nonce_prefix, entry.id)),
                    Payload {
                        msg: &encoded,
                        aad: &entry_aad(&package_id, entry),
                    },
                )
                .map_err(|_| {
                    MmdPackagePackError::PackingFailed("entry encryption failed".into())
                })?;
            bytes.extend_from_slice(&ciphertext);
        }

        let bytes: Arc<[u8]> = bytes.into();
        MmdPackage::open_bytes(bytes.clone(), *key, limits)?;
        Ok(MmdPackedPackage {
            bytes,
            key,
            package_id,
        })
    }
}

fn validate_input_sizes(input: &MmdPackagePackInput, limits: &MmdPackageLimits) -> PackResult<()> {
    check_limit(
        "entry count",
        input.entries.len() as u64,
        limits.max_entries as u64,
    )?;
    let mut total = 0_u64;
    for entry in &input.entries {
        let decoded_size = entry.decoded.len() as u64;
        check_limit(
            "entry decoded bytes",
            decoded_size,
            limits.max_entry_decoded_bytes,
        )?;
        total = total
            .checked_add(decoded_size)
            .ok_or(MmdPackageError::IntegerOverflow("total decoded bytes"))?;
        check_limit("total decoded bytes", total, limits.max_total_decoded_bytes)?;
    }
    Ok(())
}

fn validate_input_metadata(input: &MmdPackagePackInput) -> PackResult<()> {
    for entry in &input.entries {
        match entry.kind {
            MmdPackageEntryKind::Motion if entry.motion.is_none() => {
                return Err(MmdPackagePackError::PackingFailed(format!(
                    "motion entry {} requires motion metadata",
                    entry.id
                )));
            }
            MmdPackageEntryKind::Texture if entry.texture.is_none() => {
                return Err(MmdPackagePackError::PackingFailed(format!(
                    "texture entry {} requires texture metadata",
                    entry.id
                )));
            }
            MmdPackageEntryKind::Audio if entry.media_type.as_deref().is_none_or(str::is_empty) => {
                return Err(MmdPackagePackError::PackingFailed(format!(
                    "audio entry {} requires mediaType",
                    entry.id
                )));
            }
            _ => {}
        }
        if entry.codec == "ktx2-uastc-v1" {
            if entry.kind != MmdPackageEntryKind::Texture {
                return Err(MmdPackagePackError::PackingFailed(format!(
                    "codec ktx2-uastc-v1 requires texture kind for entry {}",
                    entry.id
                )));
            }
            let texture = entry.texture.as_ref().ok_or_else(|| {
                MmdPackagePackError::PackingFailed(format!(
                    "texture entry {} requires texture metadata",
                    entry.id
                ))
            })?;
            parse_ktx2_metadata(texture, entry.id)?;
        }
        if entry.kind != MmdPackageEntryKind::Motion && entry.motion.is_some() {
            return Err(MmdPackagePackError::PackingFailed(format!(
                "non-motion entry {} cannot contain motion metadata",
                entry.id
            )));
        }
        if entry.kind != MmdPackageEntryKind::Texture && entry.texture.is_some() {
            return Err(MmdPackagePackError::PackingFailed(format!(
                "non-texture entry {} cannot contain texture metadata",
                entry.id
            )));
        }
    }
    Ok(())
}

fn encode_entries(
    inputs: &mut [MmdPackagePackEntry],
) -> PackResult<(Vec<MmdPackageEntry>, Vec<Vec<u8>>)> {
    let mut offset = 0_u64;
    let mut entries = Vec::with_capacity(inputs.len());
    let mut encoded_entries = Vec::with_capacity(inputs.len());
    for input in inputs {
        let decoded_size = input.decoded.len() as u64;
        let compressed = match input.compression {
            MmdPackagePackCompression::None => None,
            MmdPackagePackCompression::ZstdV1 => {
                let compressed = compress_zstd(&input.decoded)?;
                Some(compressed)
            }
            MmdPackagePackCompression::AutoZstdV1 => {
                let compressed = compress_zstd(&input.decoded)?;
                if compressed.len() < input.decoded.len() {
                    Some(compressed)
                } else {
                    None
                }
            }
        };
        let compression = if compressed.is_some() {
            MmdPackageCompression::ZstdV1
        } else {
            MmdPackageCompression::None
        };
        if input.codec == "ktx2-uastc-v1" {
            let texture = input.texture.as_ref().ok_or_else(|| {
                MmdPackagePackError::PackingFailed(format!(
                    "texture entry {} requires texture metadata",
                    input.id
                ))
            })?;
            let metadata = parse_ktx2_metadata(texture, input.id)?;
            validate_ktx2_payload(&input.decoded, compression, &metadata)?;
        }
        let encoded = if let Some(compressed) = compressed {
            input.decoded.clear();
            compressed
        } else {
            std::mem::take(&mut input.decoded)
        };
        let cipher_size = (encoded.len() as u64)
            .checked_add(GCM_TAG_LEN)
            .ok_or(MmdPackageError::IntegerOverflow("entry cipher size"))?;
        let entry = MmdPackageEntry {
            id: input.id,
            path: input.path.clone(),
            kind: input.kind,
            codec: input.codec.clone(),
            compression,
            offset,
            cipher_size,
            decoded_size,
        };
        offset = offset
            .checked_add(cipher_size)
            .ok_or(MmdPackageError::IntegerOverflow("entry offset"))?;
        entries.push(entry);
        encoded_entries.push(encoded);
    }
    Ok((entries, encoded_entries))
}

fn build_manifest(input: &MmdPackagePackInput, entries: &[MmdPackageEntry]) -> PackResult<Value> {
    let mut root = Map::new();
    root.insert("schema".into(), Value::String("mmdpack/1".into()));
    root.insert(
        "defaultModelEntryId".into(),
        Value::from(input.default_model_entry_id),
    );
    if let Some(id) = input.default_motion_entry_id {
        root.insert("defaultMotionEntryId".into(), Value::from(id));
    }

    if entries.len() != input.entries.len() {
        return Err(MmdPackagePackError::PackingFailed(
            "manifest entries do not match pack input entries".into(),
        ));
    }
    let mut manifest_entries = Vec::with_capacity(entries.len());
    for (entry, input_entry) in entries.iter().zip(&input.entries) {
        let mut value = Map::new();
        value.insert("id".into(), Value::from(entry.id));
        value.insert("path".into(), Value::String(entry.path.clone()));
        value.insert("kind".into(), Value::String(entry.kind.as_str().into()));
        value.insert("codec".into(), Value::String(entry.codec.clone()));
        value.insert(
            "compression".into(),
            Value::String(entry.compression.as_str().into()),
        );
        value.insert("offset".into(), Value::from(entry.offset));
        value.insert("cipherSize".into(), Value::from(entry.cipher_size));
        value.insert("decodedSize".into(), Value::from(entry.decoded_size));
        if let Some(media_type) = &input_entry.media_type {
            value.insert("mediaType".into(), Value::String(media_type.clone()));
        }
        if let Some(motion) = &input_entry.motion {
            let mut metadata = Map::new();
            metadata.insert("role".into(), Value::String(motion.role.as_str().into()));
            if let Some(id) = motion.target_model_entry_id {
                metadata.insert("targetModelEntryId".into(), Value::from(id));
            }
            value.insert("motion".into(), Value::Object(metadata));
        }
        if let Some(texture) = &input_entry.texture {
            value.insert("texture".into(), texture.clone());
        }
        manifest_entries.push(Value::Object(value));
    }
    root.insert("entries".into(), Value::Array(manifest_entries));
    root.insert(
        "modelBindings".into(),
        Value::Array(
            input
                .model_bindings
                .iter()
                .map(|binding| {
                    let mut value = Map::new();
                    value.insert("modelEntryId".into(), Value::from(binding.model_entry_id));
                    value.insert(
                        "textureBindings".into(),
                        Value::Array(
                            binding
                                .texture_bindings
                                .iter()
                                .map(|texture| {
                                    let mut value = Map::new();
                                    value.insert(
                                        "textureIndex".into(),
                                        Value::from(texture.texture_index),
                                    );
                                    value.insert("entryId".into(), Value::from(texture.entry_id));
                                    Value::Object(value)
                                })
                                .collect(),
                        ),
                    );
                    Value::Object(value)
                })
                .collect(),
        ),
    );
    Ok(Value::Object(root))
}

fn compress_zstd(decoded: &[u8]) -> PackResult<Vec<u8>> {
    zstd::bulk::compress(decoded, 3)
        .map_err(|error| MmdPackagePackError::PackingFailed(error.to_string()))
}

fn build_header(
    package_id: [u8; 16],
    nonce_prefix: [u8; 8],
    manifest_cipher_size: u64,
) -> [u8; MMDPACK_HEADER_LEN] {
    let mut header = [0_u8; MMDPACK_HEADER_LEN];
    header[..8].copy_from_slice(b"MMDPACK\0");
    header[8..10].copy_from_slice(&1_u16.to_le_bytes());
    header[16..32].copy_from_slice(&package_id);
    header[32..40].copy_from_slice(&nonce_prefix);
    header[40..48].copy_from_slice(&manifest_cipher_size.to_le_bytes());
    header
}

#[cfg(test)]
mod tests {
    use rand_core::impls;

    use super::*;

    struct FixedRandom(u8);

    impl RngCore for FixedRandom {
        fn next_u32(&mut self) -> u32 {
            impls::next_u32_via_fill(self)
        }

        fn next_u64(&mut self) -> u64 {
            impls::next_u64_via_fill(self)
        }

        fn fill_bytes(&mut self, dest: &mut [u8]) {
            for byte in dest {
                *byte = self.0;
                self.0 = self.0.wrapping_add(1);
            }
        }

        fn try_fill_bytes(&mut self, dest: &mut [u8]) -> std::result::Result<(), rand_core::Error> {
            self.fill_bytes(dest);
            Ok(())
        }
    }

    #[test]
    fn deterministic_rng_still_sets_uuid_v4_bits() {
        let packed = MmdPackagePacker::pack_with_rng(
            minimal_input(MmdPackagePackCompression::None, b"PMX".to_vec()),
            MmdPackageLimits::default(),
            &mut FixedRandom(0),
        )
        .unwrap();
        assert_eq!(packed.package_id()[6] >> 4, 4);
        assert_eq!(packed.package_id()[8] >> 6, 2);
    }

    #[test]
    fn encoding_takes_owned_uncompressed_payload() {
        let mut entries = minimal_input(MmdPackagePackCompression::None, vec![3; 1024]).entries;
        let (_, encoded) = encode_entries(&mut entries).unwrap();
        assert!(entries[0].decoded.is_empty());
        assert_eq!(encoded[0], vec![3; 1024]);
    }

    fn minimal_input(
        compression: MmdPackagePackCompression,
        decoded: Vec<u8>,
    ) -> MmdPackagePackInput {
        MmdPackagePackInput {
            default_model_entry_id: 1,
            default_motion_entry_id: None,
            entries: vec![MmdPackagePackEntry {
                id: 1,
                path: "model/model.pmx".into(),
                kind: MmdPackageEntryKind::Model,
                codec: "pmx".into(),
                compression,
                decoded,
                media_type: None,
                motion: None,
                texture: None,
            }],
            model_bindings: vec![MmdModelBinding {
                model_entry_id: 1,
                texture_bindings: vec![],
            }],
        }
    }
}
