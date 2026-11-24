use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use mmd_anim_format::MmdFormatKind;
use mmd_anim_runtime::{IkSolverRuntimeStats, MorphIndex};

pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| file_operation_error("read", path, error))
}

pub(crate) fn read_text_file(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|error| file_operation_error("read", path, error))
}

pub(crate) fn write_file(path: &Path, data: impl AsRef<[u8]>) -> Result<()> {
    fs::write(path, data).map_err(|error| file_operation_error("write", path, error))
}

fn file_operation_error(operation: &str, path: &Path, error: io::Error) -> anyhow::Error {
    anyhow!(
        "failed to {operation} {}: {}",
        path.display(),
        io_error_label(error.kind())
    )
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
        "{command}: {operation} is not supported for file (detected={}): {}",
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
        "{command}: failed to parse file (format={}): {}: {error}",
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
        "{command}: failed to import file (format={}): {}: {error}",
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

/// Number of morph weight slots needed to cover every registered morph index.
pub(crate) fn registered_morph_count<K>(morph_name_to_index: &HashMap<K, MorphIndex>) -> usize {
    morph_name_to_index
        .values()
        .map(|index| index.as_usize() + 1)
        .max()
        .unwrap_or(0)
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct IkRuntimeStatsSummary {
    pub solver_evaluations: u64,
    pub configured_iterations: u64,
    pub executed_iterations: u64,
    pub skipped_iterations: u64,
    pub skipped_ratio: f64,
    pub tolerance_precheck_breaks: u64,
    pub tolerance_post_iteration_breaks: u64,
    pub rollback_breaks: u64,
    pub max_iteration_exhaustions: u64,
    pub link_visits: u64,
    pub link_steps: u64,
}

pub(crate) fn summarize_ik_runtime_stats(stats: &[IkSolverRuntimeStats]) -> IkRuntimeStatsSummary {
    let sum = |field: fn(&IkSolverRuntimeStats) -> u64| stats.iter().map(field).sum::<u64>();
    let solver_evaluations = sum(|stats| stats.solver_evaluations);
    let configured_iterations = sum(|stats| stats.configured_iterations);
    let executed_iterations = sum(|stats| stats.executed_iterations);
    let skipped_iterations = configured_iterations.saturating_sub(executed_iterations);
    let skipped_ratio = if configured_iterations == 0 {
        0.0
    } else {
        skipped_iterations as f64 / configured_iterations as f64
    };

    IkRuntimeStatsSummary {
        solver_evaluations,
        configured_iterations,
        executed_iterations,
        skipped_iterations,
        skipped_ratio,
        tolerance_precheck_breaks: sum(|stats| stats.tolerance_precheck_breaks),
        tolerance_post_iteration_breaks: sum(|stats| stats.tolerance_post_iteration_breaks),
        rollback_breaks: sum(|stats| stats.rollback_breaks),
        max_iteration_exhaustions: sum(|stats| stats.max_iteration_exhaustions),
        link_visits: sum(|stats| stats.link_visits),
        link_steps: sum(|stats| stats.link_steps),
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
        assert!(message.contains("failed to parse file (format=PMX): model.pmx"));
        assert!(!message.contains("detected="));
        assert!(message.contains("bad header"));
    }

    #[test]
    fn import_failure_error_includes_command_path_and_format() {
        let path = Path::new("motion.vmd");
        let error = import_failure_error("import", path, MmdFormatKind::Vmd, "truncated frame");
        let message = error.to_string();
        assert!(message.contains("import:"));
        assert!(message.contains("failed to import file (format=VMD): motion.vmd"));
        assert!(!message.contains("detected="));
        assert!(message.contains("truncated frame"));
    }
}
