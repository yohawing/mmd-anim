//! Rigid-body shape and physics-mode codes shared by the PMD and PMX formats.

/// Returns the parsed shape name for a PMD/PMX rigid-body shape code.
pub(crate) fn shape_name(code: u8) -> &'static str {
    match code {
        0 => "sphere",
        1 => "box",
        2 => "capsule",
        _ => "unknown",
    }
}

/// Returns the PMD/PMX shape code for a parsed shape name; unknown names write a sphere.
pub(crate) fn shape_code(name: &str) -> u8 {
    match name {
        "box" => 1,
        "capsule" => 2,
        _ => 0,
    }
}

/// Returns the parsed physics-mode name for a PMD/PMX rigid-body mode code.
pub(crate) fn mode_name(code: u8) -> &'static str {
    match code {
        0 => "static",
        1 => "dynamic",
        2 => "dynamicBone",
        _ => "unknown",
    }
}

/// Returns the PMD/PMX mode code for a parsed mode name; unknown names write static.
pub(crate) fn mode_code(name: &str) -> u8 {
    match name {
        "dynamic" => 1,
        "dynamicBone" => 2,
        _ => 0,
    }
}
