use encoding_rs::SHIFT_JIS;

pub(crate) fn decode_sjis(bytes: &[u8]) -> String {
    let (decoded, _, _) = SHIFT_JIS.decode(bytes);
    decoded.into_owned()
}

pub(crate) fn decode_sjis_fixed_trimmed(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    decode_sjis(&bytes[..end]).trim().to_owned()
}

pub(crate) fn decode_sjis_trim_nul(bytes: &[u8]) -> String {
    decode_sjis(bytes).trim_end_matches('\0').to_owned()
}

pub(crate) fn encode_sjis(text: &str) -> Vec<u8> {
    let (encoded, _, _) = SHIFT_JIS.encode(text);
    encoded.into_owned()
}

pub(crate) fn encode_sjis_prefix_fit(text: &str, len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len);
    for ch in text.chars() {
        let mut buf = [0u8; 4];
        let encoded = encode_sjis(ch.encode_utf8(&mut buf));
        if bytes.len() + encoded.len() > len {
            break;
        }
        bytes.extend_from_slice(&encoded);
    }
    bytes
}
