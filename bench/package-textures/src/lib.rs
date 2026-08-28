use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::convert::TryInto;

pub const SCHEMA: u32 = 1;
pub const UASTC_DFD_MODEL: u8 = 166;
const MAX_PROBE_RAW_BYTES: usize = 256 * 1024 * 1024;
const MAX_PROBE_METADATA_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Manifest {
    pub schema: u32,
    pub cases: Vec<TextureCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextureCase {
    pub id: String,
    pub path: String,
    pub class: String,
    pub color_space: String,
    pub mipmaps: bool,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TextureConfig {
    pub uastc_level: u8,
    pub rdo_lambda: String,
    pub zstd_level: i32,
    pub mipmaps: bool,
    pub color_space: String,
    pub orientation: String,
    pub profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MipInfo {
    pub width: u32,
    pub height: u32,
    pub length: usize,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProbeMetadata {
    pub schema: u32,
    pub width: u32,
    pub height: u32,
    pub mip_levels: Vec<MipInfo>,
    pub has_alpha: bool,
    pub color_space: String,
    pub orientation: String,
    pub profile: String,
    pub dfd_hex: String,
    pub kvd_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParsedKtx2 {
    pub width: u32,
    pub height: u32,
    pub level_count: u32,
    pub supercompression: u32,
    pub dfd: Vec<u8>,
    pub kvd: Vec<u8>,
    pub levels: Vec<KtxLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KtxLevel {
    pub offset: u64,
    pub length: u64,
    pub uncompressed_length: u64,
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema != SCHEMA {
        return Err(format!("unsupported manifest schema {}", manifest.schema));
    }
    if !(10..=20).contains(&manifest.cases.len()) {
        return Err(format!(
            "manifest must contain 10-20 cases, got {}",
            manifest.cases.len()
        ));
    }
    let classes = ["toon", "alpha_cutout", "diffuse", "normal"];
    let colors = ["srgb", "linear"];
    let mut ids = std::collections::HashSet::new();
    for case in &manifest.cases {
        if !is_safe_case_id(&case.id) || !ids.insert(&case.id) {
            return Err(format!(
                "case IDs must be safe, non-empty, and unique: {}",
                case.id
            ));
        }
        if case.path.trim().is_empty() {
            return Err(format!("case {} has an empty path", case.id));
        }
        if !classes.contains(&case.class.as_str()) {
            return Err(format!("case {} has unsupported class", case.id));
        }
        if !colors.contains(&case.color_space.as_str()) {
            return Err(format!("case {} has unsupported color space", case.id));
        }
        if !case.mipmaps || case.width == 0 || case.height == 0 {
            return Err(format!("case {} must request non-empty mipmaps", case.id));
        }
        let mip_count = u32::BITS - case.width.max(case.height).leading_zeros();
        let raw_total = (0..usize::try_from(mip_count)
            .map_err(|_| "mip count does not fit usize")?)
            .try_fold(0usize, |total, level| {
                total
                    .checked_add(expected_mip_bytes(case.width, case.height, level)?)
                    .ok_or_else(|| "manifest raw UASTC size overflow".to_string())
            })?;
        if raw_total > MAX_PROBE_RAW_BYTES {
            return Err(format!("case {} exceeds bounded raw UASTC size", case.id));
        }
    }
    Ok(())
}

pub fn is_safe_case_id(id: &str) -> bool {
    if id.is_empty() || id == "." || id == ".." {
        return false;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

pub fn safe_case_dir(run_dir: &std::path::Path, id: &str) -> Result<std::path::PathBuf, String> {
    if !is_safe_case_id(id) {
        return Err("case ID is not a safe path slug".into());
    }
    let root = run_dir
        .canonicalize()
        .map_err(|e| format!("run directory resolution failed: {e}"))?;
    let dir = root.join(id);
    if !dir.starts_with(&root) {
        return Err("case directory escaped run directory".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("case directory creation failed: {e}"))?;
    let resolved = dir
        .canonicalize()
        .map_err(|e| format!("case directory resolution failed: {e}"))?;
    if !resolved.starts_with(&root) {
        return Err("resolved case directory escaped run directory".into());
    }
    Ok(resolved)
}

pub fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), String> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err("only PNG inputs with an IHDR are supported by this bounded probe".into());
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    if width == 0 || height == 0 {
        return Err("PNG dimensions must be non-zero".into());
    }
    Ok((width, height))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let end = at
        .checked_add(4)
        .ok_or_else(|| "KTX2 integer offset overflow".to_string())?;
    bytes
        .get(at..end)
        .and_then(|b| b.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| "truncated KTX2 integer".into())
}
fn read_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    let end = at
        .checked_add(8)
        .ok_or_else(|| "KTX2 integer offset overflow".to_string())?;
    bytes
        .get(at..end)
        .and_then(|b| b.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| "truncated KTX2 integer".into())
}
fn checked_range(
    bytes: &[u8],
    offset: u64,
    length: u64,
    label: &str,
) -> Result<std::ops::Range<usize>, String> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| format!("{label} range overflow"))?;
    if end > bytes.len() as u64 || offset > usize::MAX as u64 || end > usize::MAX as u64 {
        return Err(format!("{label} range is outside the KTX2 bytes"));
    }
    Ok(offset as usize..end as usize)
}

pub fn parse_ktx2(bytes: &[u8], allow_zstd: bool) -> Result<ParsedKtx2, String> {
    const IDENT: &[u8; 12] = b"\xABKTX 20\xBB\r\n\x1A\n";
    if bytes.len() < 80 || &bytes[..12] != IDENT {
        return Err("invalid or truncated KTX2 identifier/header".into());
    }
    let vk_format = read_u32(bytes, 12)?;
    let type_size = read_u32(bytes, 16)?;
    let width = read_u32(bytes, 20)?;
    let height = read_u32(bytes, 24)?;
    let depth = read_u32(bytes, 28)?;
    let layers = read_u32(bytes, 32)?;
    let faces = read_u32(bytes, 36)?;
    let level_count = read_u32(bytes, 40)?;
    let supercompression = read_u32(bytes, 44)?;
    let dfd_offset = read_u32(bytes, 48)? as u64;
    let dfd_length = read_u32(bytes, 52)? as u64;
    let kvd_offset = read_u32(bytes, 56)? as u64;
    let kvd_length = read_u32(bytes, 60)? as u64;
    if vk_format != 0
        || type_size != 1
        || width == 0
        || height == 0
        || depth != 0
        || layers != 0
        || faces != 1
        || level_count == 0
    {
        return Err("KTX2 is not a single 2D UASTC texture".into());
    }
    if supercompression != 0 && !(allow_zstd && supercompression == 2) {
        return Err(format!(
            "unsupported KTX2 supercompression scheme {supercompression}"
        ));
    }
    let level_count_usize =
        usize::try_from(level_count).map_err(|_| "level count does not fit usize")?;
    let max_dim = width.max(height);
    let max_levels = u32::BITS - max_dim.leading_zeros();
    if level_count > max_levels {
        return Err(format!(
            "level count {level_count} exceeds dimension-derived maximum {max_levels}"
        ));
    }
    let index_bytes = level_count_usize
        .checked_mul(24)
        .ok_or_else(|| "KTX2 level index size overflow".to_string())?;
    let index_end = 80usize
        .checked_add(index_bytes)
        .ok_or_else(|| "KTX2 level index overflow".to_string())?;
    let dfd_range = checked_range(bytes, dfd_offset, dfd_length, "DFD")?;
    let dfd_bounds = (dfd_range.start, dfd_range.end);
    if dfd_range.start < index_end {
        return Err("DFD overlaps KTX2 header or level index".into());
    }
    let dfd = bytes[dfd_range].to_vec();
    if dfd.len() < 16
        || usize::try_from(read_u32(&dfd, 0)?).map_err(|_| "DFD length does not fit usize")?
            != dfd.len()
        || dfd[12] != UASTC_DFD_MODEL
    {
        return Err("KTX2 DFD is not a complete UASTC LDR descriptor".into());
    }
    let kvd_bounds = if kvd_length == 0 {
        None
    } else {
        let range = checked_range(bytes, kvd_offset, kvd_length, "KVD")?;
        if range.start < index_end || range.start < dfd_bounds.1 && range.end > dfd_bounds.0 {
            return Err("KVD overlaps KTX2 header, level index, or DFD".into());
        }
        Some((range.start, range.end))
    };
    let kvd = if let Some((start, end)) = kvd_bounds {
        bytes[start..end].to_vec()
    } else {
        Vec::new()
    };
    if index_end > bytes.len() {
        return Err("truncated KTX2 level index".into());
    }
    let max_file_levels = bytes.len().saturating_sub(80) / 24;
    if level_count_usize > max_file_levels {
        return Err("KTX2 level index exceeds file size".into());
    }
    let mut levels = Vec::with_capacity(level_count_usize);
    let mut ranges = Vec::with_capacity(level_count_usize);
    for i in 0..level_count_usize {
        let at = 80usize
            .checked_add(
                i.checked_mul(24)
                    .ok_or_else(|| "KTX2 level index offset overflow".to_string())?,
            )
            .ok_or_else(|| "KTX2 level index offset overflow".to_string())?;
        let offset = read_u64(bytes, at)?;
        let length_at = at
            .checked_add(8)
            .ok_or_else(|| "KTX2 level length offset overflow".to_string())?;
        let uncompressed_at = at
            .checked_add(16)
            .ok_or_else(|| "KTX2 level length offset overflow".to_string())?;
        let length = read_u64(bytes, length_at)?;
        let uncompressed_length = read_u64(bytes, uncompressed_at)?;
        if length == 0 || uncompressed_length == 0 {
            return Err(format!("mip {i} has an empty level"));
        }
        let range = checked_range(bytes, offset, length, &format!("mip {i}"))?;
        if range.start < index_end
            || range.start < dfd_bounds.1 && range.end > dfd_bounds.0
            || kvd_bounds.is_some_and(|(start, end)| range.start < end && range.end > start)
        {
            return Err(format!("mip {i} overlaps KTX2 metadata or index"));
        }
        ranges.push((range.start, range.end));
        levels.push(KtxLevel {
            offset,
            length,
            uncompressed_length,
        });
    }
    ranges.sort_unstable();
    for pair in ranges.windows(2) {
        if pair[0].1 > pair[1].0 {
            return Err("KTX2 level ranges overlap".into());
        }
    }
    let parsed = ParsedKtx2 {
        width,
        height,
        level_count,
        supercompression,
        dfd,
        kvd,
        levels,
    };
    let _ = parsed.levels_for(bytes)?;
    Ok(parsed)
}

impl ParsedKtx2 {
    pub fn levels_for(&self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let expected_levels =
            usize::try_from(self.level_count).map_err(|_| "KTX2 level count does not fit usize")?;
        let max_levels = u32::BITS - self.width.max(self.height).leading_zeros();
        if expected_levels == 0 || self.level_count > max_levels {
            return Err("KTX2 level count exceeds dimension-derived maximum".into());
        }
        if self.levels.len() != expected_levels {
            return Err("KTX2 level count does not match level index".into());
        }
        if self.supercompression != 0 && self.supercompression != 2 {
            return Err("unsupported KTX2 supercompression scheme".into());
        }
        let mut expected_lengths = Vec::with_capacity(expected_levels);
        let mut aggregate = 0usize;
        for (i, level) in self.levels.iter().enumerate() {
            if level.length == 0 || level.uncompressed_length == 0 {
                return Err(format!("mip {i} has an empty level"));
            }
            let expected = expected_mip_bytes(self.width, self.height, i)?;
            let expected_u64 =
                u64::try_from(expected).map_err(|_| "expected UASTC mip size does not fit u64")?;
            if level.uncompressed_length != expected_u64 {
                return Err(format!(
                    "mip {i} declares {} bytes, expected {expected}",
                    level.uncompressed_length
                ));
            }
            if self.supercompression == 0 && level.length != expected_u64 {
                return Err(format!("mip {i} uncompressed length mismatch"));
            }
            checked_range(bytes, level.offset, level.length, &format!("mip {i}"))?;
            aggregate = aggregate
                .checked_add(expected)
                .ok_or_else(|| "KTX2 aggregate UASTC size overflow".to_string())?;
            if aggregate > MAX_PROBE_RAW_BYTES {
                return Err("KTX2 aggregate UASTC size exceeds bounded size".into());
            }
            expected_lengths.push(expected);
        }
        let mut out = Vec::with_capacity(expected_levels);
        for ((i, level), expected) in self.levels.iter().enumerate().zip(expected_lengths) {
            let range = checked_range(bytes, level.offset, level.length, &format!("mip {i}"))?;
            let data = &bytes[range];
            let raw = if self.supercompression == 2 {
                zstd::bulk::decompress(data, expected)
                    .map_err(|e| format!("mip {i} zstd decode failed: {e}"))?
            } else {
                data.to_vec()
            };
            if raw.len() != expected {
                return Err(format!(
                    "mip {i} has {} bytes, expected {expected}",
                    raw.len()
                ));
            }
            out.push(raw);
        }
        Ok(out)
    }
}

fn expected_mip_bytes(width: u32, height: u32, level: usize) -> Result<usize, String> {
    if level >= u32::BITS as usize {
        return Err("mip index would require a shift of 32 or more".into());
    }
    let shift = u32::try_from(level).map_err(|_| "mip index does not fit u32")?;
    let mw = width.checked_shr(shift).unwrap_or(0).max(1);
    let mh = height.checked_shr(shift).unwrap_or(0).max(1);
    let blocks_w = usize::try_from(mw.div_ceil(4)).map_err(|_| "mip width does not fit usize")?;
    let blocks_h = usize::try_from(mh.div_ceil(4)).map_err(|_| "mip height does not fit usize")?;
    blocks_w
        .checked_mul(blocks_h)
        .and_then(|v| v.checked_mul(16))
        .ok_or_else(|| "expected UASTC mip size overflow".into())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn from_hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("metadata hex has odd length".into());
    }
    if text.len() / 2 > MAX_PROBE_METADATA_BYTES {
        return Err("probe metadata exceeds bounded size".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16)
                .map_err(|_| "metadata hex is invalid".to_string())
        })
        .collect()
}

pub fn make_probe_metadata(
    parsed: &ParsedKtx2,
    levels: &[Vec<u8>],
    case: &TextureCase,
) -> ProbeMetadata {
    ProbeMetadata {
        schema: SCHEMA,
        width: parsed.width,
        height: parsed.height,
        mip_levels: levels
            .iter()
            .enumerate()
            .map(|(i, level)| MipInfo {
                width: parsed
                    .width
                    .checked_shr(u32::try_from(i).unwrap_or(u32::MAX))
                    .unwrap_or(0)
                    .max(1),
                height: parsed
                    .height
                    .checked_shr(u32::try_from(i).unwrap_or(u32::MAX))
                    .unwrap_or(0)
                    .max(1),
                length: level.len(),
                sha256: sha256_hex(level),
            })
            .collect(),
        has_alpha: case.class == "alpha_cutout",
        color_space: case.color_space.clone(),
        orientation: "rd".into(),
        profile: "UASTC_LDR_4x4".into(),
        dfd_hex: to_hex(&parsed.dfd),
        // KVD contains tool-specific writer strings in this probe. Orientation
        // is explicit above, so retaining KVD would count non-essential bytes.
        kvd_hex: String::new(),
    }
}

pub fn encode_raw_blocks(levels: &[Vec<u8>], zstd_level: i32) -> Result<Vec<u8>, String> {
    let total = levels.iter().try_fold(0usize, |total, level| {
        total
            .checked_add(level.len())
            .ok_or_else(|| "raw UASTC block size overflow".to_string())
    })?;
    if total > MAX_PROBE_RAW_BYTES {
        return Err("raw UASTC blocks exceed bounded size".into());
    }
    let mut all = Vec::new();
    all.try_reserve(total)
        .map_err(|_| "raw UASTC block allocation failed".to_string())?;
    for level in levels {
        all.extend_from_slice(level);
    }
    zstd::bulk::compress(&all, zstd_level).map_err(|e| format!("raw UASTC zstd encode failed: {e}"))
}

pub fn decode_raw_blocks(
    metadata: &ProbeMetadata,
    compressed: &[u8],
) -> Result<Vec<Vec<u8>>, String> {
    if metadata.schema != SCHEMA || metadata.width == 0 || metadata.height == 0 {
        return Err("invalid probe metadata dimensions/schema".into());
    }
    let max_levels = u32::BITS - metadata.width.max(metadata.height).leading_zeros();
    let level_count = u32::try_from(metadata.mip_levels.len())
        .map_err(|_| "probe metadata mip count does not fit u32")?;
    if level_count == 0 || level_count > max_levels {
        return Err("probe metadata mip count exceeds dimension-derived maximum".into());
    }
    let total = metadata.mip_levels.iter().try_fold(0usize, |total, mip| {
        total
            .checked_add(mip.length)
            .ok_or_else(|| "probe metadata total length overflow".to_string())
    })?;
    if total > MAX_PROBE_RAW_BYTES {
        return Err("probe metadata raw blocks exceed bounded size".into());
    }
    for (i, mip) in metadata.mip_levels.iter().enumerate() {
        let expected = expected_mip_bytes(metadata.width, metadata.height, i)?;
        if mip.width
            != metadata
                .width
                .checked_shr(u32::try_from(i).map_err(|_| "mip index does not fit u32")?)
                .unwrap_or(0)
                .max(1)
            || mip.height
                != metadata
                    .height
                    .checked_shr(u32::try_from(i).map_err(|_| "mip index does not fit u32")?)
                    .unwrap_or(0)
                    .max(1)
            || mip.length != expected
        {
            return Err(format!(
                "probe metadata mip {i} dimensions or length mismatch"
            ));
        }
    }
    let all = zstd::bulk::decompress(compressed, total)
        .map_err(|e| format!("raw UASTC zstd decode failed: {e}"))?;
    if all.len() != total {
        return Err("raw UASTC decoded length mismatch".into());
    }
    let mut at = 0usize;
    let mut levels = Vec::new();
    for (i, mip) in metadata.mip_levels.iter().enumerate() {
        let end = at
            .checked_add(mip.length)
            .ok_or_else(|| format!("raw UASTC mip {i} range overflow"))?;
        if end > all.len() {
            return Err(format!("raw UASTC mip {i} range is outside decoded bytes"));
        }
        let level = all[at..end].to_vec();
        if sha256_hex(&level) != mip.sha256 {
            return Err(format!("raw UASTC mip {i} hash mismatch"));
        }
        levels.push(level);
        at = end;
    }
    Ok(levels)
}

fn put_u32(out: &mut Vec<u8>, x: u32) {
    out.extend_from_slice(&x.to_le_bytes());
}
fn put_u64(out: &mut Vec<u8>, x: u64) {
    out.extend_from_slice(&x.to_le_bytes());
}
fn align8(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(8) {
        out.push(0);
    }
}

pub fn reconstruct_ktx2(metadata: &ProbeMetadata, compressed: &[u8]) -> Result<Vec<u8>, String> {
    let dfd = from_hex(&metadata.dfd_hex)?;
    let kvd = from_hex(&metadata.kvd_hex)?;
    if dfd.len() > u32::MAX as usize || kvd.len() > u32::MAX as usize {
        return Err("probe metadata section length does not fit KTX2 header".into());
    }
    let metadata_total = dfd
        .len()
        .checked_add(kvd.len())
        .ok_or_else(|| "probe metadata total size overflow".to_string())?;
    if metadata_total > MAX_PROBE_METADATA_BYTES {
        return Err("probe metadata total exceeds bounded size".into());
    }
    let levels = decode_raw_blocks(metadata, compressed)?;
    let n = u32::try_from(levels.len()).map_err(|_| "mip count does not fit KTX2 header")?;
    let index_bytes = usize::try_from(n)
        .map_err(|_| "mip count does not fit usize")?
        .checked_mul(24)
        .ok_or_else(|| "KTX2 level index size overflow".to_string())?;
    let index_end = 80usize
        .checked_add(index_bytes)
        .ok_or_else(|| "KTX2 level index overflow".to_string())?;
    let dfd_offset_usize = index_end
        .checked_add(7)
        .ok_or_else(|| "DFD offset overflow".to_string())?
        / 8
        * 8;
    let dfd_offset = u64::try_from(dfd_offset_usize).map_err(|_| "DFD offset does not fit u64")?;
    let body_capacity = levels.iter().try_fold(dfd.len(), |size, level| {
        let aligned = size
            .checked_add(7)
            .ok_or_else(|| "KTX2 body size overflow".to_string())?
            / 8
            * 8;
        aligned
            .checked_add(level.len())
            .ok_or_else(|| "KTX2 body size overflow".to_string())
    })?;
    let mut body = Vec::new();
    body.try_reserve(body_capacity)
        .map_err(|_| "KTX2 body allocation failed".to_string())?;
    body.extend_from_slice(&dfd);
    align8(&mut body);
    let mut kvd_offset = 0u64;
    if !kvd.is_empty() {
        kvd_offset = dfd_offset
            .checked_add(u64::try_from(body.len()).map_err(|_| "KVD offset does not fit u64")?)
            .ok_or_else(|| "KVD offset overflow".to_string())?;
        body.extend_from_slice(&kvd);
        align8(&mut body);
    }
    let mut actual_level_offsets = Vec::new();
    actual_level_offsets
        .try_reserve(levels.len())
        .map_err(|_| "KTX2 level index allocation failed".to_string())?;
    for level in &levels {
        align8(&mut body);
        let offset = dfd_offset
            .checked_add(u64::try_from(body.len()).map_err(|_| "level offset does not fit u64")?)
            .ok_or_else(|| "level offset overflow".to_string())?;
        let length = u64::try_from(level.len()).map_err(|_| "level length does not fit u64")?;
        actual_level_offsets.push((offset, length));
        body.extend_from_slice(level);
    }
    let mut header = Vec::with_capacity(80);
    header.extend_from_slice(b"\xABKTX 20\xBB\r\n\x1A\n");
    put_u32(&mut header, 0);
    put_u32(&mut header, 1);
    put_u32(&mut header, metadata.width);
    put_u32(&mut header, metadata.height);
    put_u32(&mut header, 0);
    put_u32(&mut header, 0);
    put_u32(&mut header, 1);
    put_u32(&mut header, n);
    put_u32(&mut header, 0);
    put_u32(
        &mut header,
        u32::try_from(dfd_offset).map_err(|_| "DFD offset does not fit KTX2 header")?,
    );
    put_u32(
        &mut header,
        u32::try_from(dfd.len()).map_err(|_| "DFD length does not fit KTX2 header")?,
    );
    put_u32(
        &mut header,
        u32::try_from(kvd_offset).map_err(|_| "KVD offset does not fit KTX2 header")?,
    );
    put_u32(
        &mut header,
        u32::try_from(kvd.len()).map_err(|_| "KVD length does not fit KTX2 header")?,
    );
    put_u64(&mut header, 0);
    put_u64(&mut header, 0);
    let mut rebuilt = header;
    for (offset, len) in &actual_level_offsets {
        put_u64(&mut rebuilt, *offset);
        put_u64(&mut rebuilt, *len);
        put_u64(&mut rebuilt, *len);
    }
    while rebuilt.len() < dfd_offset_usize {
        rebuilt.push(0);
    }
    rebuilt.extend_from_slice(&body);
    Ok(rebuilt)
}

pub fn equal_levels(left: &[Vec<u8>], right: &[Vec<u8>]) -> Result<(), String> {
    if left.len() != right.len() {
        return Err("mip count drift".into());
    }
    for (i, (a, b)) in left.iter().zip(right).enumerate() {
        if a != b {
            return Err(format!("mip {i} UASTC bytes drift"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn case(id: &str) -> TextureCase {
        TextureCase {
            id: id.into(),
            path: "fixture.png".into(),
            class: "diffuse".into(),
            color_space: "srgb".into(),
            mipmaps: true,
            width: 4,
            height: 4,
        }
    }
    #[test]
    fn manifest_boundaries_are_rejected() {
        let mut m = Manifest {
            schema: SCHEMA,
            cases: Vec::new(),
        };
        assert!(validate_manifest(&m).is_err());
        m.cases = (0..10).map(|_| case("same")).collect();
        assert!(validate_manifest(&m).is_err());
        m.cases = (0..10).map(|i| case(&i.to_string())).collect();
        m.cases[0].class = "bad".into();
        assert!(validate_manifest(&m).is_err());
        m.cases[0].class = "diffuse".into();
        m.cases[0].width = u32::MAX;
        m.cases[0].height = u32::MAX;
        assert!(validate_manifest(&m).is_err());
        for invalid in [".", "..", "_bad", "a/b", "a\\b", "a space"] {
            m.cases = (0..10).map(|i| case(&format!("case-{i}"))).collect();
            m.cases[0].id = invalid.into();
            assert!(validate_manifest(&m).is_err(), "{invalid}");
        }
    }
    #[test]
    fn levels_equal_rejects_drift() {
        assert!(equal_levels(&[vec![1, 2]], &[vec![1, 3]]).is_err());
    }
    #[test]
    fn raw_probe_roundtrip_reconstructs_valid_ktx2() {
        let mut dfd = vec![0u8; 44];
        dfd[..4].copy_from_slice(&44u32.to_le_bytes());
        dfd[8] = 2;
        dfd[10] = 40;
        dfd[12] = UASTC_DFD_MODEL;
        dfd[13] = 1;
        dfd[14] = 2;
        dfd[16] = 3;
        dfd[17] = 3;
        dfd[20] = 16;
        let levels = vec![vec![7u8; 16], vec![8u8; 16]];
        let parsed = ParsedKtx2 {
            width: 4,
            height: 4,
            level_count: 2,
            supercompression: 0,
            dfd: dfd.clone(),
            kvd: Vec::new(),
            levels: Vec::new(),
        };
        let meta = make_probe_metadata(&parsed, &levels, &case("x"));
        let compressed = encode_raw_blocks(&levels, 3).unwrap();
        let rebuilt = reconstruct_ktx2(&meta, &compressed).unwrap();
        let reparsed = parse_ktx2(&rebuilt, false).unwrap();
        assert_eq!(reparsed.levels_for(&rebuilt).unwrap(), levels);
    }
    #[test]
    fn truncated_ktx2_is_rejected() {
        assert!(parse_ktx2(b"\xABKTX 20", false).is_err());
    }
    #[test]
    fn level_overlapping_header_is_rejected() {
        let mut dfd = vec![0u8; 44];
        dfd[..4].copy_from_slice(&44u32.to_le_bytes());
        dfd[8] = 2;
        dfd[10] = 40;
        dfd[12] = UASTC_DFD_MODEL;
        dfd[13] = 1;
        dfd[14] = 2;
        dfd[16] = 3;
        dfd[17] = 3;
        dfd[20] = 16;
        let levels = vec![vec![7u8; 16]];
        let parsed = ParsedKtx2 {
            width: 4,
            height: 4,
            level_count: 1,
            supercompression: 0,
            dfd,
            kvd: Vec::new(),
            levels: Vec::new(),
        };
        let meta = make_probe_metadata(&parsed, &levels, &case("x"));
        let compressed = encode_raw_blocks(&levels, 3).unwrap();
        let mut rebuilt = reconstruct_ktx2(&meta, &compressed).unwrap();
        rebuilt[80..88].copy_from_slice(&0u64.to_le_bytes());
        assert!(parse_ktx2(&rebuilt, false).is_err());
    }

    #[test]
    fn extreme_level_count_and_uncompressed_length_are_rejected() {
        let mut dfd = vec![0u8; 44];
        dfd[..4].copy_from_slice(&44u32.to_le_bytes());
        dfd[8] = 2;
        dfd[10] = 40;
        dfd[12] = UASTC_DFD_MODEL;
        dfd[13] = 1;
        dfd[14] = 2;
        dfd[16] = 3;
        dfd[17] = 3;
        dfd[20] = 16;
        let levels = vec![vec![7u8; 16]];
        let parsed = ParsedKtx2 {
            width: 1,
            height: 1,
            level_count: 1,
            supercompression: 0,
            dfd,
            kvd: Vec::new(),
            levels: Vec::new(),
        };
        let meta = make_probe_metadata(&parsed, &levels, &case("x"));
        let compressed = encode_raw_blocks(&levels, 3).unwrap();
        let rebuilt = reconstruct_ktx2(&meta, &compressed).unwrap();
        let mut too_many = rebuilt.clone();
        too_many[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_ktx2(&too_many, false).is_err());

        let mut huge = rebuilt;
        huge[80 + 16..80 + 24].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(parse_ktx2(&huge, false).is_err());
    }

    #[test]
    fn raw_metadata_total_overflow_is_rejected_before_decode() {
        let metadata = ProbeMetadata {
            schema: SCHEMA,
            width: 4,
            height: 4,
            mip_levels: vec![
                MipInfo {
                    width: 4,
                    height: 4,
                    length: usize::MAX,
                    sha256: String::new(),
                },
                MipInfo {
                    width: 2,
                    height: 2,
                    length: usize::MAX,
                    sha256: String::new(),
                },
            ],
            has_alpha: false,
            color_space: "srgb".into(),
            orientation: "rd".into(),
            profile: "UASTC_LDR_4x4".into(),
            dfd_hex: String::new(),
            kvd_hex: String::new(),
        };
        assert!(decode_raw_blocks(&metadata, &[]).is_err());
    }

    #[test]
    fn huge_dimensions_and_metadata_are_rejected() {
        assert!(expected_mip_bytes(u32::MAX, u32::MAX, 0).is_err());
        let metadata = ProbeMetadata {
            schema: SCHEMA,
            width: 4,
            height: 4,
            mip_levels: vec![MipInfo {
                width: 4,
                height: 4,
                length: 16,
                sha256: sha256_hex(&[0; 16]),
            }],
            has_alpha: false,
            color_space: "srgb".into(),
            orientation: "rd".into(),
            profile: "UASTC_LDR_4x4".into(),
            dfd_hex: "00".repeat(MAX_PROBE_METADATA_BYTES + 1),
            kvd_hex: String::new(),
        };
        assert!(reconstruct_ktx2(&metadata, &[]).is_err());
    }

    #[test]
    fn ktx_levels_preflight_aggregate_before_decompression() {
        let parsed = ParsedKtx2 {
            width: 16_384,
            height: 16_384,
            level_count: 2,
            supercompression: 2,
            dfd: Vec::new(),
            kvd: Vec::new(),
            levels: vec![
                KtxLevel {
                    offset: 0,
                    length: 1,
                    uncompressed_length: 268_435_456,
                },
                KtxLevel {
                    offset: 1,
                    length: 1,
                    uncompressed_length: 67_108_864,
                },
            ],
        };
        let error = parsed.levels_for(&[0, 0]).unwrap_err();
        assert!(error.contains("aggregate UASTC size exceeds bounded size"));
    }
}
