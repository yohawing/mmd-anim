use crate::sjis::decode_sjis;

pub fn normalize_vmd_name(bytes: &[u8]) -> Vec<u8> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    decode_sjis(&bytes[..end]).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_name_preserved() {
        assert_eq!(normalize_vmd_name(b"BoneA"), b"BoneA".to_vec());
    }

    #[test]
    fn japanese_name_decoded_to_utf8() {
        let sjis: &[u8] = &[0x83, 0x65, 0x83, 0x58, 0x83, 0x67];
        let utf8: &[u8] = &[0xE3, 0x83, 0x86, 0xE3, 0x82, 0xB9, 0xE3, 0x83, 0x88];
        assert_eq!(normalize_vmd_name(sjis), utf8.to_vec());
    }

    #[test]
    fn empty_name_stays_empty() {
        assert_eq!(normalize_vmd_name(b""), b"".to_vec());
    }

    #[test]
    fn trailing_nul_is_trimmed() {
        assert_eq!(normalize_vmd_name(b"BoneA\0\0"), b"BoneA".to_vec());
    }
}
