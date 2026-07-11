use std::{
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use mmd_anim_format::MmdFormatKind;

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

pub(crate) fn format_kind_label(kind: MmdFormatKind) -> &'static str {
    match kind {
        MmdFormatKind::Pmd => "PMD",
        MmdFormatKind::Pmx => "PMX",
        MmdFormatKind::Vmd => "VMD",
        MmdFormatKind::Vpd => "VPD",
        MmdFormatKind::Pmm => "PMM",
        MmdFormatKind::Nmd => "NMD",
        MmdFormatKind::X => "X",
        MmdFormatKind::Vac => "VAC",
        MmdFormatKind::Unknown => "unknown",
    }
}

pub(crate) fn unsupported_format_error(
    command: &str,
    path: &Path,
    kind: MmdFormatKind,
) -> Box<dyn std::error::Error> {
    anyhow!(
        "{command}: unsupported or unrecognized file format (detected={}): {}",
        format_kind_label(kind),
        path.display()
    )
    .into()
}

pub(crate) fn unsupported_format_usage_message(
    command: &str,
    path: &Path,
    kind: MmdFormatKind,
    hint: &str,
) -> String {
    format!(
        "{command}: unsupported or unrecognized file format (detected={}): {}; {hint}",
        format_kind_label(kind),
        path.display()
    )
}

pub(crate) fn unsupported_format_operation_error(
    command: &str,
    path: &Path,
    kind: MmdFormatKind,
    operation: &str,
) -> Box<dyn std::error::Error> {
    anyhow!(
        "{command}: {operation} is not supported for {} file: {}",
        format_kind_label(kind),
        path.display()
    )
    .into()
}

pub(crate) fn parse_failure_error(
    command: &str,
    path: &Path,
    kind: MmdFormatKind,
    error: impl std::fmt::Display,
) -> Box<dyn std::error::Error> {
    anyhow!(
        "{command}: failed to parse {} file {}: {error}",
        format_kind_label(kind),
        path.display()
    )
    .into()
}

pub(crate) fn import_failure_error(
    command: &str,
    path: &Path,
    kind: MmdFormatKind,
    error: impl std::fmt::Display,
) -> Box<dyn std::error::Error> {
    anyhow!(
        "{command}: failed to import {} file {}: {error}",
        format_kind_label(kind),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn unsupported_format_error_includes_command_path_and_detected_kind() {
        let path = Path::new("assets/mystery.bin");
        let error = unsupported_format_error("inspect", path, MmdFormatKind::Unknown);
        let message = error.to_string();
        assert!(message.contains("inspect:"));
        assert!(message.contains("assets/mystery.bin"));
        assert!(message.contains("detected=unknown"));
    }

    #[test]
    fn parse_failure_error_includes_command_path_and_format() {
        let path = Path::new("model.pmx");
        let error = parse_failure_error("inspect", path, MmdFormatKind::Pmx, "bad header");
        let message = error.to_string();
        assert!(message.contains("inspect:"));
        assert!(message.contains("failed to parse PMX file model.pmx"));
        assert!(message.contains("bad header"));
    }

    #[test]
    fn import_failure_error_includes_command_path_and_format() {
        let path = Path::new("motion.vmd");
        let error = import_failure_error("import", path, MmdFormatKind::Vmd, "truncated frame");
        let message = error.to_string();
        assert!(message.contains("import:"));
        assert!(message.contains("failed to import VMD file motion.vmd"));
        assert!(message.contains("truncated frame"));
    }
}
