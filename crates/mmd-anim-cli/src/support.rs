use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};

pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| {
        anyhow!(
            "failed to read {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
    })
}

pub(crate) fn read_text_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|error| {
        anyhow!(
            "failed to read {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
    })
}

pub(crate) fn write_file(path: &Path, data: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, data).map_err(|error| {
        anyhow!(
            "failed to write {}: {}",
            path.display(),
            io_error_label(error.kind())
        )
    })
}

pub(crate) fn diagnostics_suffix(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!(" diagnostics={count}")
    }
}

pub(crate) fn unsupported_format_error(path: &Path) -> Box<dyn std::error::Error> {
    anyhow!(
        "unsupported or unrecognized file format: {}",
        path.display()
    )
    .into()
}

pub(crate) fn format_cli_error(error: &(dyn std::error::Error + 'static)) -> String {
    if let Some(io_error) = error.downcast_ref::<io::Error>() {
        return format!("I/O error: {}", io_error_label(io_error.kind()));
    }
    error.to_string()
}

pub(crate) fn io_error_label(kind: io::ErrorKind) -> &'static str {
    match kind {
        io::ErrorKind::NotFound => "file not found",
        io::ErrorKind::PermissionDenied => "permission denied",
        io::ErrorKind::InvalidData => "invalid data",
        io::ErrorKind::UnexpectedEof => "unexpected end of file",
        io::ErrorKind::AlreadyExists => "already exists",
        io::ErrorKind::WouldBlock => "operation would block",
        io::ErrorKind::TimedOut => "operation timed out",
        io::ErrorKind::Interrupted => "operation interrupted",
        _ => "I/O error",
    }
}

pub(crate) fn resolve_maybe_absolute(root: &Path, path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

pub(crate) fn translation_checksum(matrices: &[glam::Mat4]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for m in matrices {
        hash ^= m.w_axis.x.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= m.w_axis.y.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
        hash ^= m.w_axis.z.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

pub(crate) fn f32_checksum(values: &[f32]) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for value in values {
        hash ^= value.to_bits();
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

pub(crate) fn copy_world_matrices_to_f32(matrices: &[glam::Mat4], out: &mut [f32]) {
    debug_assert!(out.len() >= matrices.len() * 16);
    for (index, matrix) in matrices.iter().enumerate() {
        let offset = index * 16;
        out[offset..offset + 16].copy_from_slice(&matrix.to_cols_array());
    }
}
