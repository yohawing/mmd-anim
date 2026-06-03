use mmd_anim_runtime::ModelBuildError;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ImportError {
    #[error("unexpected end of data (need {0} more bytes)")]
    UnexpectedEof(usize),
    #[error("invalid PMX magic bytes")]
    InvalidPmxMagic,
    #[error("unsupported PMX version {0}")]
    UnsupportedPmxVersion(f32),
    #[error("invalid text encoding byte {0}")]
    InvalidEncoding(u8),
    #[error("unsupported index size {0}")]
    InvalidIndexSize(u8),
    #[error("PMX section overflow")]
    SectionOverflow,
    #[error("invalid VMD magic bytes")]
    InvalidVmdMagic,
    #[error("VMD model name is not valid Shift-JIS")]
    InvalidVmdModelName,
    #[error("invalid {format} magic bytes")]
    InvalidMagic { format: &'static str },
    #[error("unsupported {format} parser feature: {detail}")]
    UnsupportedFormat {
        format: &'static str,
        detail: &'static str,
    },
    #[error("model build failed: {0}")]
    ModelBuildFailed(ModelBuildError),
}
