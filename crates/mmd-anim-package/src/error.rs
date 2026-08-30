use thiserror::Error;

/// Errors returned by the experimental MMDPACK reader.
#[derive(Debug, Error)]
pub enum MmdPackageError {
    #[error("package is shorter than the fixed 64-byte header")]
    HeaderTooShort,
    #[error("invalid MMDPACK magic")]
    InvalidMagic,
    #[error("unsupported MMDPACK version {major}.{minor}")]
    UnsupportedVersion { major: u16, minor: u16 },
    #[error("header flags must be zero, got {0:#x}")]
    UnsupportedFlags(u32),
    #[error("reserved header bytes must be zero")]
    NonZeroReserved,
    #[error("{what} exceeds the configured limit: {actual} > {limit}")]
    LimitExceeded {
        what: &'static str,
        actual: u64,
        limit: u64,
    },
    #[error("integer overflow while computing {0}")]
    IntegerOverflow(&'static str),
    #[error("package is truncated while reading {0}")]
    Truncated(&'static str),
    #[error("authentication failed for {0}")]
    AuthenticationFailed(&'static str),
    #[error("manifest is not valid strict JSON: {0}")]
    InvalidJson(String),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("entry id {0} was not found")]
    EntryNotFound(u32),
    #[error("entry path {0:?} was not found")]
    PathNotFound(String),
    #[error("model entry {0} has no modelBindings record")]
    ModelBindingNotFound(u32),
    #[error(
        "texture binding index {texture_index} for model entry {model_entry_id} is outside the PMX texture table of {texture_table_len} entries"
    )]
    TextureIndexOutOfRange {
        model_entry_id: u32,
        texture_index: u32,
        texture_table_len: usize,
    },
    #[error("codec {0:?} is not supported by this reader")]
    UnsupportedCodec(String),
    #[error("invalid zstd-v1 payload: {0}")]
    InvalidZstd(String),
}

pub(crate) type Result<T> = std::result::Result<T, MmdPackageError>;
