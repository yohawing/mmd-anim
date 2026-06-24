use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MmdFormatKind {
    Pmd,
    Pmx,
    Vmd,
    Vpd,
    Pmm,
    Nmd,
    X,
    Vac,
    Unknown,
}

pub fn sniff(data: &[u8]) -> Option<MmdFormatKind> {
    if data.starts_with(b"PMX ") {
        return Some(MmdFormatKind::Pmx);
    }
    if data.starts_with(b"Pmd") {
        return Some(MmdFormatKind::Pmd);
    }
    if data.starts_with(b"Vocaloid Motion Data") {
        return Some(MmdFormatKind::Vmd);
    }
    if data.starts_with(b"Polygon Movie maker ") {
        return Some(MmdFormatKind::Pmm);
    }
    None
}

pub fn detect_mmd_format(data: &[u8], file_name: Option<&str>) -> MmdFormatKind {
    if let Some(kind) = sniff(data) {
        return kind;
    }
    if data.starts_with(b"Vocaloid Pose Data") {
        return MmdFormatKind::Vpd;
    }
    if data.starts_with(b"xof ") {
        return MmdFormatKind::X;
    }
    if looks_like_nmd(data, file_name) {
        return MmdFormatKind::Nmd;
    }
    match extension(file_name).as_deref() {
        Some("x") => MmdFormatKind::X,
        Some("vac") => MmdFormatKind::Vac,
        Some("nmd") => MmdFormatKind::Nmd,
        Some("pmd") => MmdFormatKind::Pmd,
        Some("pmx") => MmdFormatKind::Pmx,
        Some("vmd") => MmdFormatKind::Vmd,
        Some("vpd") => MmdFormatKind::Vpd,
        Some("pmm") => MmdFormatKind::Pmm,
        _ => MmdFormatKind::Unknown,
    }
}

fn extension(file_name: Option<&str>) -> Option<String> {
    file_name?
        .rsplit_once('.')
        .map(|(_, ext)| ext.trim().to_ascii_lowercase())
}

fn looks_like_nmd(data: &[u8], file_name: Option<&str>) -> bool {
    matches!(extension(file_name).as_deref(), Some("nmd"))
        || data.starts_with(b"NMD")
        || data.starts_with(b"Nanoem Motion Data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_core_mmd_formats_from_magic_bytes() {
        assert_eq!(detect_mmd_format(b"PMX test", None), MmdFormatKind::Pmx);
        assert_eq!(detect_mmd_format(b"Pmd\x00", None), MmdFormatKind::Pmd);
        assert_eq!(
            detect_mmd_format(b"Vocaloid Motion Data 0002", None),
            MmdFormatKind::Vmd
        );
        assert_eq!(
            detect_mmd_format(b"Vocaloid Pose Data file", None),
            MmdFormatKind::Vpd
        );
        assert_eq!(
            detect_mmd_format(b"Polygon Movie maker 0002", None),
            MmdFormatKind::Pmm
        );
        assert_eq!(
            detect_mmd_format(b"xof 0303txt 0032", None),
            MmdFormatKind::X
        );
        assert_eq!(
            detect_mmd_format(b"Nanoem Motion Data", None),
            MmdFormatKind::Nmd
        );
    }

    #[test]
    fn sniffs_core_binary_formats_from_magic_bytes() {
        assert_eq!(sniff(b"PMX test"), Some(MmdFormatKind::Pmx));
        assert_eq!(sniff(b"Pmd\x00"), Some(MmdFormatKind::Pmd));
        assert_eq!(
            sniff(b"Vocaloid Motion Data 0002\0\0\0\0\0model"),
            Some(MmdFormatKind::Vmd)
        );
        assert_eq!(
            sniff(b"Polygon Movie maker 0002\0"),
            Some(MmdFormatKind::Pmm)
        );
        assert_eq!(sniff(b"Vocaloid Pose Data file"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn falls_back_to_case_insensitive_extension() {
        assert_eq!(
            detect_mmd_format(b"", Some("motion.NMD")),
            MmdFormatKind::Nmd
        );
        assert_eq!(
            detect_mmd_format(b"", Some("accessory.VAC")),
            MmdFormatKind::Vac
        );
        assert_eq!(
            detect_mmd_format(b"", Some("model.PMD")),
            MmdFormatKind::Pmd
        );
        assert_eq!(
            detect_mmd_format(b"", Some("unknown.bin")),
            MmdFormatKind::Unknown
        );
    }
}
