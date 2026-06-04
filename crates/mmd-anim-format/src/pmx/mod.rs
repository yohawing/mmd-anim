use std::collections::HashMap;

use glam::{Mat4, Quat, Vec3A};
use serde::{Deserialize, Serialize};

use mmd_anim_runtime::{
    AppendTransformInit, BoneIndex, BoneInit, BoneMorphOffset, GroupMorphOffset, IkAngleLimit,
    IkLinkInit, IkSolverInit, ModelArena, MorphIndex, MorphInit, MorphOffsetSpan,
};

use crate::error::ImportError;

const PMX_MAGIC: [u8; 4] = [0x50, 0x4D, 0x58, 0x20];

const BONE_FLAG_TAIL_INDEX: u16 = 0x0001;
const BONE_FLAG_IK: u16 = 0x0020;
const BONE_FLAG_LOCAL_APPEND: u16 = 0x0080;
const BONE_FLAG_APPEND_ROTATE: u16 = 0x0100;
const BONE_FLAG_APPEND_TRANSLATE: u16 = 0x0200;
const BONE_FLAG_FIXED_AXIS: u16 = 0x0400;
const BONE_FLAG_LOCAL_AXIS: u16 = 0x0800;
const BONE_FLAG_EXTERNAL_PARENT: u16 = 0x2000;

fn decode_utf16le_lossy(bytes: &[u8]) -> String {
    let end = bytes.len().saturating_sub(bytes.len() % 2);
    let units = bytes[..end]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .take_while(|&code| code != 0);
    std::char::decode_utf16(units)
        .map(|item| item.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Utf16Le = 0,
    Utf8 = 1,
}

#[derive(Debug, Clone)]
pub struct PmxHeader {
    pub version: f32,
    pub encoding: TextEncoding,
    pub extra_uv_count: u8,
    pub vertex_index_size: u8,
    pub texture_index_size: u8,
    pub material_index_size: u8,
    pub bone_index_size: u8,
    pub morph_index_size: u8,
    pub rigidbody_index_size: u8,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn require(&self, n: usize) -> Result<(), ImportError> {
        if self.remaining() >= n {
            Ok(())
        } else {
            Err(ImportError::UnexpectedEof(
                n.saturating_sub(self.remaining()),
            ))
        }
    }

    fn require_record_bytes(&self, count: usize, record_size: usize) -> Result<(), ImportError> {
        let bytes = count
            .checked_mul(record_size)
            .ok_or(ImportError::SectionOverflow)?;
        self.require(bytes)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], ImportError> {
        self.require(n)?;
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, ImportError> {
        Ok(self.read_bytes(1)?[0])
    }

    fn read_u16_le(&mut self) -> Result<u16, ImportError> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn read_i32_le(&mut self) -> Result<i32, ImportError> {
        let b = self.read_bytes(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_f32_le(&mut self) -> Result<f32, ImportError> {
        let b = self.read_bytes(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn read_vec3(&mut self) -> Result<Vec3A, ImportError> {
        Ok(Vec3A::new(
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
        ))
    }

    fn read_vec2_array(&mut self) -> Result<[f32; 2], ImportError> {
        Ok([self.read_f32_le()?, self.read_f32_le()?])
    }

    fn read_vec3_array(&mut self) -> Result<[f32; 3], ImportError> {
        Ok([
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
        ])
    }

    fn read_vec4_array(&mut self) -> Result<[f32; 4], ImportError> {
        Ok([
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
            self.read_f32_le()?,
        ])
    }

    fn read_sized_index(&mut self, size: u8) -> Result<i32, ImportError> {
        match size {
            1 => Ok(self.read_u8()? as i8 as i32),
            2 => Ok({
                let b = self.read_bytes(2)?;
                i16::from_le_bytes([b[0], b[1]]) as i32
            }),
            4 => self.read_i32_le(),
            _ => Err(ImportError::InvalidIndexSize(size)),
        }
    }

    fn read_index(&mut self, header: &PmxHeader) -> Result<i32, ImportError> {
        self.read_sized_index(header.bone_index_size)
    }

    fn read_vertex_index(&mut self, size: u8) -> Result<u32, ImportError> {
        match size {
            1 => Ok(self.read_u8()? as u32),
            2 => Ok(self.read_u16_le()? as u32),
            4 => {
                let raw = self.read_i32_le()?;
                if raw < 0 {
                    Err(ImportError::SectionOverflow)
                } else {
                    Ok(raw as u32)
                }
            }
            _ => Err(ImportError::InvalidIndexSize(size)),
        }
    }

    fn skip(&mut self, n: usize) -> Result<(), ImportError> {
        self.require(n)?;
        self.pos += n;
        Ok(())
    }

    fn read_string(&mut self, encoding: TextEncoding) -> Result<String, ImportError> {
        let len = self.read_i32_le()?;
        if len <= 0 {
            return Ok(String::new());
        }
        let bytes = self.read_bytes(len as usize)?;
        match encoding {
            TextEncoding::Utf8 => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
            }
            TextEncoding::Utf16Le => Ok(decode_utf16le_lossy(bytes)),
        }
    }

    fn read_string_owned(
        &mut self,
        encoding: TextEncoding,
    ) -> Result<(Vec<u8>, String), ImportError> {
        let len = self.read_i32_le()?;
        if len <= 0 {
            return Ok((Vec::new(), String::new()));
        }
        let bytes = self.read_bytes(len as usize)?.to_vec();
        let decoded = match encoding {
            TextEncoding::Utf8 => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                String::from_utf8_lossy(&bytes[..end]).into_owned()
            }
            TextEncoding::Utf16Le => decode_utf16le_lossy(&bytes),
        };
        Ok((bytes, decoded))
    }

    fn read_bone_index(&mut self, header: &PmxHeader) -> Result<Option<BoneIndex>, ImportError> {
        let raw = self.read_sized_index(header.bone_index_size)?;
        if raw < 0 {
            Ok(None)
        } else {
            Ok(Some(BoneIndex(raw as u32)))
        }
    }
}

pub fn read_header(data: &[u8]) -> Result<(PmxHeader, usize), ImportError> {
    let mut r = Reader::new(data);

    let magic = r.read_bytes(4)?;
    if magic != PMX_MAGIC {
        return Err(ImportError::InvalidPmxMagic);
    }

    let version = r.read_f32_le()?;
    if !(2.0..=2.2).contains(&version) {
        return Err(ImportError::UnsupportedPmxVersion(version));
    }

    let data_count = r.read_u8()?;
    if data_count < 8 {
        return Err(ImportError::InvalidEncoding(data_count));
    }

    let encoding_byte = r.read_u8()?;
    let encoding = match encoding_byte {
        0 => TextEncoding::Utf16Le,
        1 => TextEncoding::Utf8,
        _ => return Err(ImportError::InvalidEncoding(encoding_byte)),
    };

    let extra_uv_count = r.read_u8()?;
    let vertex_index_size = r.read_u8()?;
    let texture_index_size = r.read_u8()?;
    let material_index_size = r.read_u8()?;
    let bone_index_size = r.read_u8()?;
    let morph_index_size = r.read_u8()?;
    let rigidbody_index_size = r.read_u8()?;
    for size in [
        vertex_index_size,
        texture_index_size,
        material_index_size,
        bone_index_size,
        morph_index_size,
        rigidbody_index_size,
    ] {
        if !matches!(size, 1 | 2 | 4) {
            return Err(ImportError::InvalidIndexSize(size));
        }
    }
    if data_count > 8 {
        r.skip((data_count - 8) as usize)?;
    }

    let header = PmxHeader {
        version,
        encoding,
        extra_uv_count,
        vertex_index_size,
        texture_index_size,
        material_index_size,
        bone_index_size,
        morph_index_size,
        rigidbody_index_size,
    };

    Ok((header, r.pos))
}

pub fn skip_model_info(data: &[u8], header: &PmxHeader, pos: usize) -> Result<usize, ImportError> {
    let mut r = Reader { data, pos };
    let _model_name = r.read_string(header.encoding)?;
    let _english_name = r.read_string(header.encoding)?;
    let _comment = r.read_string(header.encoding)?;
    let _english_comment = r.read_string(header.encoding)?;
    Ok(r.pos)
}

pub fn skip_vertices(data: &[u8], header: &PmxHeader, pos: usize) -> Result<usize, ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    let count = count as usize;

    for _ in 0..count {
        r.skip(12)?;
        r.skip(12)?;
        r.skip(8)?;
        r.skip(header.extra_uv_count as usize * 16)?;
        let vertex_deform_type = r.read_u8()?;

        let bone_indices: usize = match vertex_deform_type {
            0 => 1,
            1 => 2,
            2 | 4 => 4,
            3 => 2,
            _ => return Err(ImportError::SectionOverflow),
        };
        for _ in 0..bone_indices {
            r.read_sized_index(header.bone_index_size)?;
        }

        let weight_count: usize = match vertex_deform_type {
            0 => 0,
            1 => 1,
            2 | 4 => 4,
            3 => 1,
            _ => return Err(ImportError::SectionOverflow),
        };
        r.skip(weight_count * 4)?;

        if vertex_deform_type == 3 {
            r.skip(36)?;
        }

        r.skip(4)?;
    }

    Ok(r.pos)
}

pub fn skip_faces(data: &[u8], vertex_index_size: u8, pos: usize) -> Result<usize, ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    let count = count as usize;
    let bytes = count
        .checked_mul(vertex_index_size as usize)
        .ok_or(ImportError::SectionOverflow)?;
    r.skip(bytes)?;
    Ok(r.pos)
}

pub fn skip_textures(
    data: &[u8],
    encoding: TextEncoding,
    pos: usize,
) -> Result<usize, ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    for _ in 0..count {
        let _path = r.read_string(encoding)?;
    }
    Ok(r.pos)
}

pub fn skip_materials(data: &[u8], header: &PmxHeader, pos: usize) -> Result<usize, ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    for _ in 0..count {
        let _name = r.read_string(header.encoding)?;
        let _english_name = r.read_string(header.encoding)?;
        r.skip(16)?;
        r.skip(16)?;
        r.skip(12)?;
        r.skip(1)?;
        r.skip(16)?;
        r.skip(4)?;
        let _texture = r.read_sized_index(header.texture_index_size)?;
        let _sphere_tex = r.read_sized_index(header.texture_index_size)?;
        r.skip(1)?;
        let _toon = r.read_u8()?;
        if _toon == 0 {
            r.read_sized_index(header.texture_index_size)?;
        } else {
            r.skip(1)?;
        }
        let _memo = r.read_string(header.encoding)?;
        let _face_count = r.read_i32_le()?;
    }
    Ok(r.pos)
}

pub fn skip_morphs(data: &[u8], header: &PmxHeader, pos: usize) -> Result<usize, ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    for _ in 0..count {
        let _name = r.read_string(header.encoding)?;
        let _english_name = r.read_string(header.encoding)?;
        r.skip(1)?;
        let _morph_type = r.read_u8()?;
        let _offset_count = r.read_i32_le()?;
        match _morph_type {
            0 => {
                for _ in 0.._offset_count {
                    r.read_sized_index(header.morph_index_size)?;
                    r.skip(4)?;
                }
            }
            1 => {
                for _ in 0.._offset_count {
                    r.read_sized_index(header.vertex_index_size)?;
                    r.skip(12)?;
                }
            }
            2 => {
                for _ in 0.._offset_count {
                    r.read_sized_index(header.bone_index_size)?;
                    r.skip(12)?;
                    r.skip(16)?;
                }
            }
            3..=7 => {
                for _ in 0.._offset_count {
                    r.read_sized_index(header.vertex_index_size)?;
                    r.skip(16)?;
                }
            }
            8 => {
                for _ in 0.._offset_count {
                    r.read_sized_index(header.material_index_size)?;
                    r.skip(1)?;
                    r.skip(16)?;
                    r.skip(12)?;
                    r.skip(4)?;
                    r.skip(12)?;
                    r.skip(16)?;
                    r.skip(4)?;
                    r.skip(16)?;
                    r.skip(16)?;
                    r.skip(16)?;
                }
            }
            9 => {
                for _ in 0.._offset_count {
                    r.read_sized_index(header.rigidbody_index_size)?;
                    r.skip(1)?;
                    r.skip(12)?;
                    r.skip(12)?;
                }
            }
            _ => return Err(ImportError::SectionOverflow),
        }
    }
    Ok(r.pos)
}

#[derive(Debug, Clone)]
pub struct PmxMorphNames {
    pub name_bytes: Vec<Vec<u8>>,
    pub names: Vec<String>,
}

pub fn read_morph_names(
    data: &[u8],
    header: &PmxHeader,
    pos: usize,
) -> Result<(PmxMorphNames, usize), ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    let count = count as usize;
    r.require_record_bytes(count, 1)?;

    let mut name_bytes = Vec::with_capacity(count);
    let mut names = Vec::with_capacity(count);

    for _ in 0..count {
        let (raw, name) = r.read_string_owned(header.encoding)?;
        let _english_name = r.read_string(header.encoding)?;
        name_bytes.push(raw);
        names.push(name);

        r.skip(1)?;
        let morph_type = r.read_u8()?;
        let offset_count = r.read_i32_le()?;
        if offset_count < 0 {
            return Err(ImportError::SectionOverflow);
        }
        let offset_count = offset_count as usize;

        let per_offset = match morph_type {
            0 => header.morph_index_size as usize + 4,
            1 => header.vertex_index_size as usize + 12,
            2 => header.bone_index_size as usize + 12 + 16,
            3..=7 => header.vertex_index_size as usize + 16,
            8 => header.material_index_size as usize + 1 + 16 + 12 + 4 + 12 + 16 + 4 + 16 + 16 + 16,
            9 => header.rigidbody_index_size as usize + 1 + 12 + 12,
            _ => return Err(ImportError::SectionOverflow),
        };
        r.skip(
            offset_count
                .checked_mul(per_offset)
                .ok_or(ImportError::SectionOverflow)?,
        )?;
    }

    Ok((PmxMorphNames { name_bytes, names }, r.pos))
}

/// Read morph names and extract bone/group offset payloads.
///
/// Returns names (same layout as [`read_morph_names`]) plus a [`MorphInit`]
/// suitable for [`ModelArena::new_with_morphs`]. Unsupported morph types
/// (vertex, UV, material, flip) have their payload skipped.
pub fn read_morph_offsets(
    data: &[u8],
    header: &PmxHeader,
    pos: usize,
) -> Result<(PmxMorphNames, MorphInit, usize), ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    let count = count as usize;
    r.require_record_bytes(count, 1)?;

    let mut name_bytes = Vec::with_capacity(count);
    let mut names = Vec::with_capacity(count);

    let mut bone_offsets_all: Vec<BoneMorphOffset> = Vec::new();
    let mut bone_spans: Vec<MorphOffsetSpan> = Vec::with_capacity(count);
    let mut group_offsets_all: Vec<GroupMorphOffset> = Vec::new();
    let mut group_spans: Vec<MorphOffsetSpan> = Vec::with_capacity(count);

    for _ in 0..count {
        let (raw, name) = r.read_string_owned(header.encoding)?;
        let _english_name = r.read_string(header.encoding)?;
        name_bytes.push(raw);
        names.push(name);

        r.skip(1)?;
        let morph_type = r.read_u8()?;
        let offset_count = r.read_i32_le()?;
        if offset_count < 0 {
            return Err(ImportError::SectionOverflow);
        }
        let offset_count = offset_count as usize;

        match morph_type {
            0 => {
                let start = group_offsets_all.len() as u32;
                for _ in 0..offset_count {
                    let child_raw = r.read_sized_index(header.morph_index_size)?;
                    if child_raw < 0 {
                        return Err(ImportError::SectionOverflow);
                    }
                    let ratio = r.read_f32_le()?;
                    group_offsets_all.push(GroupMorphOffset {
                        child_morph: MorphIndex(child_raw as u32),
                        ratio,
                    });
                }
                group_spans.push(MorphOffsetSpan {
                    start,
                    count: offset_count as u32,
                });
                bone_spans.push(MorphOffsetSpan::default());
            }
            2 => {
                let start = bone_offsets_all.len() as u32;
                for _ in 0..offset_count {
                    let bone_raw = r.read_sized_index(header.bone_index_size)?;
                    if bone_raw < 0 {
                        return Err(ImportError::SectionOverflow);
                    }
                    let pos = r.read_vec3()?;
                    let qx = r.read_f32_le()?;
                    let qy = r.read_f32_le()?;
                    let qz = r.read_f32_le()?;
                    let qw = r.read_f32_le()?;
                    bone_offsets_all.push(BoneMorphOffset {
                        target_bone: BoneIndex(bone_raw as u32),
                        position_offset: pos,
                        rotation_offset: Quat::from_xyzw(qx, qy, qz, qw),
                    });
                }
                bone_spans.push(MorphOffsetSpan {
                    start,
                    count: offset_count as u32,
                });
                group_spans.push(MorphOffsetSpan::default());
            }
            _ => {
                let per_offset = match morph_type {
                    1 => header.vertex_index_size as usize + 12,
                    3..=7 => header.vertex_index_size as usize + 16,
                    8 => {
                        header.material_index_size as usize
                            + 1
                            + 16
                            + 12
                            + 4
                            + 12
                            + 16
                            + 4
                            + 16
                            + 16
                            + 16
                    }
                    9 => header.rigidbody_index_size as usize + 1 + 12 + 12,
                    _ => return Err(ImportError::SectionOverflow),
                };
                r.skip(
                    offset_count
                        .checked_mul(per_offset)
                        .ok_or(ImportError::SectionOverflow)?,
                )?;
                bone_spans.push(MorphOffsetSpan::default());
                group_spans.push(MorphOffsetSpan::default());
            }
        }
    }

    Ok((
        PmxMorphNames { name_bytes, names },
        MorphInit {
            morph_count: count as u32,
            vertex_spans: vec![MorphOffsetSpan::default(); count],
            bone_offsets: bone_offsets_all,
            bone_spans,
            group_offsets: group_offsets_all,
            group_spans,
            ..MorphInit::default()
        },
        r.pos,
    ))
}

#[derive(Debug, Clone)]
pub struct PmxBoneImport {
    pub bones: Vec<BoneInit>,
    pub ik_solvers: Vec<IkSolverInit>,
    pub append_transforms: Vec<AppendTransformInit>,
    pub bone_name_bytes: Vec<Vec<u8>>,
    pub bone_names: Vec<String>,
}

pub fn read_bones(
    data: &[u8],
    header: &PmxHeader,
    pos: usize,
) -> Result<(PmxBoneImport, usize), ImportError> {
    let mut r = Reader { data, pos };
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    let count = count as usize;
    r.require_record_bytes(count, 1)?;

    let mut bones = Vec::with_capacity(count);
    let mut ik_solvers = Vec::new();
    let mut append_transforms = Vec::new();

    let mut bone_name_bytes = Vec::with_capacity(count);
    let mut bone_names = Vec::with_capacity(count);

    let mut absolute_positions: Vec<Vec3A> = Vec::with_capacity(count);

    for _bone_i in 0..count {
        let (name_bytes, name) = r.read_string_owned(header.encoding)?;
        let _english_name = r.read_string(header.encoding)?;
        let position = r.read_vec3()?;
        absolute_positions.push(position);
        let parent = r.read_bone_index(header)?;
        let transform_order = r.read_i32_le()?;
        let flags = r.read_u16_le()?;

        if flags & BONE_FLAG_TAIL_INDEX != 0 {
            r.read_index(header)?;
        } else {
            r.read_vec3()?;
        }

        let has_append =
            flags & BONE_FLAG_APPEND_ROTATE != 0 || flags & BONE_FLAG_APPEND_TRANSLATE != 0;

        if has_append {
            let append_parent = r.read_bone_index(header)?;
            let append_ratio = r.read_f32_le()?;

            if let Some(source) = append_parent {
                let target_idx = bones.len();
                append_transforms.push(
                    AppendTransformInit::new(BoneIndex(target_idx as u32), source, append_ratio)
                        .with_rotation_if(flags & BONE_FLAG_APPEND_ROTATE != 0)
                        .with_translation_if(flags & BONE_FLAG_APPEND_TRANSLATE != 0)
                        .with_local_if(flags & BONE_FLAG_LOCAL_APPEND != 0),
                );
            }
        }

        let fixed_axis = if flags & BONE_FLAG_FIXED_AXIS != 0 {
            Some(r.read_vec3()?)
        } else {
            None
        };

        if flags & BONE_FLAG_LOCAL_AXIS != 0 {
            r.read_vec3()?;
            r.read_vec3()?;
        }

        if flags & BONE_FLAG_EXTERNAL_PARENT != 0 {
            r.read_i32_le()?;
        }

        if flags & BONE_FLAG_IK != 0 {
            let ik_target = r.read_bone_index(header)?;
            let ik_loop_count = r.read_i32_le()?;
            let ik_limit_angle = r.read_f32_le()?;
            let link_count = r.read_i32_le()?;
            if link_count < 0 {
                return Err(ImportError::SectionOverflow);
            }
            r.require_record_bytes(link_count as usize, header.bone_index_size as usize + 1)?;

            let mut links = Vec::with_capacity(link_count as usize);
            for _ in 0..link_count {
                let link_bone = r.read_bone_index(header)?.unwrap_or(BoneIndex(0));
                let has_limit = r.read_u8()?;
                let angle_limit = if has_limit != 0 {
                    let min = r.read_vec3()?;
                    let max = r.read_vec3()?;
                    Some(IkAngleLimit::new(min, max))
                } else {
                    None
                };
                links.push(match angle_limit {
                    Some(lim) => IkLinkInit::new(link_bone).with_angle_limit(lim),
                    None => IkLinkInit::new(link_bone),
                });
            }

            let ik_bone = BoneIndex(bones.len() as u32);
            if let Some(target) = ik_target {
                ik_solvers.push(IkSolverInit {
                    ik_bone,
                    target_bone: target,
                    links,
                    iteration_count: ik_loop_count as u32,
                    limit_angle: ik_limit_angle,
                });
            }
        }

        bone_name_bytes.push(name_bytes);
        bone_names.push(name);

        bones.push(BoneInit {
            parent,
            rest_position: position,
            inverse_bind_matrix: Mat4::from_translation((-position).into()),
            transform_order,
            fixed_axis,
        });
    }

    for bone in bones.iter_mut() {
        if let Some(parent_idx) = bone.parent
            && let Some(parent_abs) = absolute_positions.get(parent_idx.as_usize())
        {
            bone.rest_position -= parent_abs;
        }
    }

    Ok((
        PmxBoneImport {
            bones,
            ik_solvers,
            append_transforms,
            bone_name_bytes,
            bone_names,
        },
        r.pos,
    ))
}

pub fn import_pmx_model(data: &[u8]) -> Result<PmxBoneImport, ImportError> {
    let (header, pos) = read_header(data)?;
    let pos = skip_model_info(data, &header, pos)?;
    let pos = skip_vertices(data, &header, pos)?;
    let pos = skip_faces(data, header.vertex_index_size, pos)?;
    let pos = skip_textures(data, header.encoding, pos)?;
    let pos = skip_materials(data, &header, pos)?;
    let (bones, _pos) = read_bones(data, &header, pos)?;
    Ok(bones)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedModel {
    pub metadata: PmxParsedMetadata,
    pub geometry: PmxParsedGeometry,
    pub materials: Vec<PmxParsedMaterial>,
    pub skeleton: PmxParsedSkeleton,
    pub morphs: Vec<PmxParsedMorph>,
    pub display_frames: Vec<PmxParsedDisplayFrame>,
    pub rigid_bodies: Vec<PmxParsedRigidBody>,
    pub joints: Vec<PmxParsedJoint>,
    pub soft_bodies: Vec<PmxParsedSoftBody>,
    pub diagnostics: Vec<PmxParserDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedMetadata {
    #[serde(default = "default_pmx_format")]
    pub format: String,
    pub version: f32,
    pub encoding: String,
    pub name: String,
    pub english_name: String,
    pub comment: String,
    pub english_comment: String,
    pub counts: PmxParsedCounts,
    pub index_sizes: PmxParsedIndexSizes,
    pub additional_uv_count: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedCounts {
    pub vertices: usize,
    pub faces: usize,
    pub materials: usize,
    pub bones: usize,
    pub morphs: usize,
    pub display_frames: usize,
    pub rigid_bodies: usize,
    pub joints: usize,
    pub soft_bodies: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmxParsedIndexSizes {
    pub vertex: u8,
    pub texture: u8,
    pub material: u8,
    pub bone: u8,
    pub morph: u8,
    #[serde(rename = "rigidBody")]
    pub rigid_body: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedGeometry {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    pub uvs: Vec<f32>,
    pub additional_uvs: Vec<Vec<f32>>,
    pub indices: Vec<u32>,
    pub skin_indices: Vec<u32>,
    pub skin_weights: Vec<f32>,
    pub edge_scale: Vec<f32>,
    pub material_groups: Vec<PmxParsedMaterialGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedMaterialGroup {
    pub start: usize,
    pub count: usize,
    pub material_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedMaterial {
    pub name: String,
    pub english_name: String,
    pub texture_path: String,
    pub sphere_texture_path: String,
    pub sphere_mode: String,
    pub toon_texture_path: String,
    pub shared_toon_index: Option<u8>,
    pub diffuse: [f32; 4],
    pub specular: [f32; 3],
    pub specular_power: f32,
    pub ambient: [f32; 3],
    pub edge_color: [f32; 4],
    pub edge_size: f32,
    pub flags: PmxParsedMaterialFlags,
    pub face_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedMaterialFlags {
    pub double_sided: bool,
    pub ground_shadow: bool,
    pub self_shadow_map: bool,
    pub self_shadow: bool,
    pub edge: bool,
    pub vertex_color: bool,
    pub point_draw: bool,
    pub line_draw: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmxParsedSkeleton {
    pub bones: Vec<PmxParsedBone>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedBone {
    pub name: String,
    pub english_name: String,
    pub parent_index: i32,
    pub layer: i32,
    pub position: [f32; 3],
    pub tail_index: i32,
    pub tail_position: Option<[f32; 3]>,
    pub flags: PmxParsedBoneFlags,
    pub append_transform: Option<PmxParsedAppendTransform>,
    pub fixed_axis: Option<[f32; 3]>,
    pub local_axis: Option<PmxParsedLocalAxis>,
    pub external_parent_key: Option<i32>,
    pub ik: Option<PmxParsedIk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedBoneFlags {
    pub indexed_tail: bool,
    pub rotatable: bool,
    pub translatable: bool,
    pub visible: bool,
    pub enabled: bool,
    pub ik: bool,
    pub append_local: bool,
    pub append_rotate: bool,
    pub append_translate: bool,
    pub fixed_axis: bool,
    pub local_axis: bool,
    pub transform_after_physics: bool,
    pub external_parent_transform: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedAppendTransform {
    pub parent_index: i32,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmxParsedLocalAxis {
    pub x: [f32; 3],
    pub z: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedIk {
    pub target_index: i32,
    pub loop_count: i32,
    pub limit_angle: f32,
    pub links: Vec<PmxParsedIkLink>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedIkLink {
    pub bone_index: i32,
    pub limits: Option<PmxParsedIkLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmxParsedIkLimit {
    pub lower: [f32; 3],
    pub upper: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedMorph {
    pub name: String,
    pub english_name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub vertex_offsets: Vec<PmxParsedVertexMorphOffset>,
    pub group_offsets: Vec<PmxParsedGroupMorphOffset>,
    pub bone_offsets: Vec<PmxParsedBoneMorphOffset>,
    pub uv_offsets: Vec<PmxParsedUvMorphOffset>,
    pub additional_uv_offsets: Vec<PmxParsedAdditionalUvMorphOffset>,
    pub material_offsets: Vec<PmxParsedMaterialMorphOffset>,
    pub flip_offsets: Vec<PmxParsedGroupMorphOffset>,
    pub impulse_offsets: Vec<PmxParsedImpulseMorphOffset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedVertexMorphOffset {
    pub vertex_index: u32,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedGroupMorphOffset {
    pub morph_index: i32,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedBoneMorphOffset {
    pub bone_index: i32,
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedUvMorphOffset {
    pub vertex_index: u32,
    pub uv: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedAdditionalUvMorphOffset {
    pub vertex_index: u32,
    pub uv_index: u8,
    pub uv: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedMaterialMorphOffset {
    pub material_index: i32,
    pub operation: String,
    pub diffuse: [f32; 4],
    pub specular: [f32; 3],
    pub specular_power: f32,
    pub ambient: [f32; 3],
    pub edge_color: [f32; 4],
    pub edge_size: f32,
    pub texture_factor: [f32; 4],
    pub sphere_texture_factor: [f32; 4],
    pub toon_texture_factor: [f32; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedImpulseMorphOffset {
    pub rigid_body_index: i32,
    pub local: bool,
    pub velocity: [f32; 3],
    pub torque: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedDisplayFrame {
    pub name: String,
    pub english_name: String,
    pub special: bool,
    pub frames: Vec<PmxParsedDisplayFrameElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmxParsedDisplayFrameElement {
    #[serde(rename = "type")]
    pub kind: String,
    pub index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedRigidBody {
    pub name: String,
    pub english_name: String,
    pub bone_index: i32,
    pub group: u8,
    pub mask: u16,
    pub shape: String,
    pub size: [f32; 3],
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub mass: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub restitution: f32,
    pub friction: f32,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedJoint {
    pub name: String,
    pub english_name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub rigid_body_index_a: i32,
    pub rigid_body_index_b: i32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub translation_lower_limit: [f32; 3],
    pub translation_upper_limit: [f32; 3],
    pub rotation_lower_limit: [f32; 3],
    pub rotation_upper_limit: [f32; 3],
    pub spring_translation_factor: [f32; 3],
    pub spring_rotation_factor: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxParsedSoftBody {
    pub name: String,
    pub english_name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub material_index: i32,
    pub collision_group: u8,
    pub collision_mask: u16,
    pub flags: u8,
    pub bending_constraints_distance: i32,
    pub cluster_count: i32,
    pub total_mass: f32,
    pub collision_margin: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmxParserDiagnostic {
    pub level: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsDescriptor {
    #[serde(default = "default_pmx_parts_version")]
    pub version: f32,
    #[serde(default = "default_pmx_parts_encoding")]
    pub encoding: String,
    #[serde(default = "default_pmx_parts_name")]
    pub name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub english_comment: String,
    #[serde(default)]
    pub material_name: String,
    #[serde(default)]
    pub english_material_name: String,
    #[serde(default)]
    pub materials: Vec<PmxPartsMaterialDescriptor>,
    #[serde(default)]
    pub bones: Vec<PmxPartsBoneDescriptor>,
    #[serde(default)]
    pub morphs: Vec<PmxPartsMorphDescriptor>,
    #[serde(default)]
    pub display_frames: Vec<PmxPartsDisplayFrameDescriptor>,
    #[serde(default)]
    pub rigid_bodies: Vec<PmxPartsRigidBodyDescriptor>,
    #[serde(default)]
    pub joints: Vec<PmxPartsJointDescriptor>,
    #[serde(default)]
    pub index_sizes: PmxPartsIndexSizes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsMaterialDescriptor {
    #[serde(default = "default_pmx_parts_material_name")]
    pub name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default)]
    pub texture_path: String,
    #[serde(default)]
    pub sphere_texture_path: String,
    #[serde(default)]
    pub sphere_mode: String,
    #[serde(default)]
    pub toon_texture_path: String,
    #[serde(default = "default_pmx_parts_shared_toon_index")]
    pub shared_toon_index: Option<u8>,
    #[serde(default = "default_pmx_parts_diffuse")]
    pub diffuse: [f32; 4],
    #[serde(default)]
    pub specular: [f32; 3],
    #[serde(default = "default_pmx_parts_specular_power")]
    pub specular_power: f32,
    #[serde(default = "default_pmx_parts_ambient")]
    pub ambient: [f32; 3],
    #[serde(default = "default_pmx_parts_edge_color")]
    pub edge_color: [f32; 4],
    #[serde(default = "default_pmx_parts_edge_size")]
    pub edge_size: f32,
    #[serde(default)]
    pub flags: PmxPartsMaterialFlags,
    #[serde(default)]
    pub face_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsMaterialFlags {
    #[serde(default = "default_true")]
    pub double_sided: bool,
    #[serde(default = "default_true")]
    pub ground_shadow: bool,
    #[serde(default = "default_true")]
    pub self_shadow_map: bool,
    #[serde(default = "default_true")]
    pub self_shadow: bool,
    #[serde(default)]
    pub edge: bool,
    #[serde(default)]
    pub vertex_color: bool,
    #[serde(default)]
    pub point_draw: bool,
    #[serde(default)]
    pub line_draw: bool,
}

impl Default for PmxPartsMaterialFlags {
    fn default() -> Self {
        Self {
            double_sided: true,
            ground_shadow: true,
            self_shadow_map: true,
            self_shadow: true,
            edge: false,
            vertex_color: false,
            point_draw: false,
            line_draw: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsBoneDescriptor {
    #[serde(default = "default_pmx_parts_root_bone_name")]
    pub name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default = "default_negative_index")]
    pub parent_index: i32,
    #[serde(default)]
    pub layer: i32,
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default = "default_negative_index")]
    pub tail_index: i32,
    #[serde(default)]
    pub tail_position: Option<[f32; 3]>,
    #[serde(default)]
    pub rotatable: bool,
    #[serde(default)]
    pub translatable: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsMorphDescriptor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default = "default_pmx_parts_morph_kind", alias = "type")]
    pub kind: String,
    #[serde(default)]
    pub vertex_offsets: Vec<PmxPartsVertexMorphOffset>,
    #[serde(default)]
    pub group_offsets: Vec<PmxPartsGroupMorphOffset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsVertexMorphOffset {
    pub vertex_index: u32,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsGroupMorphOffset {
    pub morph_index: i32,
    pub weight: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsDisplayFrameDescriptor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default)]
    pub special: bool,
    #[serde(default)]
    pub frames: Vec<PmxPartsDisplayFrameItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsDisplayFrameItem {
    #[serde(default = "default_pmx_parts_display_frame_kind")]
    pub kind: String,
    #[serde(default)]
    pub index: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsRigidBodyDescriptor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(default = "default_negative_index")]
    pub bone_index: i32,
    #[serde(default)]
    pub group: u8,
    #[serde(default)]
    pub mask: u16,
    #[serde(default = "default_pmx_parts_rigid_body_shape")]
    pub shape: String,
    #[serde(default = "default_unit_vec3")]
    pub size: [f32; 3],
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default = "default_pmx_parts_mass")]
    pub mass: f32,
    #[serde(default)]
    pub linear_damping: f32,
    #[serde(default)]
    pub angular_damping: f32,
    #[serde(default)]
    pub restitution: f32,
    #[serde(default)]
    pub friction: f32,
    #[serde(default = "default_pmx_parts_rigid_body_mode")]
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsJointDescriptor {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub english_name: String,
    #[serde(
        default = "default_pmx_parts_joint_kind",
        rename = "type",
        alias = "kind"
    )]
    pub kind: String,
    #[serde(default = "default_negative_index")]
    pub rigid_body_index_a: i32,
    #[serde(default = "default_negative_index")]
    pub rigid_body_index_b: i32,
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default)]
    pub rotation: [f32; 3],
    #[serde(default)]
    pub translation_lower_limit: [f32; 3],
    #[serde(default)]
    pub translation_upper_limit: [f32; 3],
    #[serde(default)]
    pub rotation_lower_limit: [f32; 3],
    #[serde(default)]
    pub rotation_upper_limit: [f32; 3],
    #[serde(default)]
    pub spring_translation_factor: [f32; 3],
    #[serde(default)]
    pub spring_rotation_factor: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmxPartsIndexSizes {
    #[serde(default = "default_pmx_vertex_index_size")]
    pub vertex: u8,
    #[serde(default = "default_pmx_small_index_size")]
    pub texture: u8,
    #[serde(default = "default_pmx_small_index_size")]
    pub material: u8,
    #[serde(default = "default_pmx_small_index_size")]
    pub bone: u8,
    #[serde(default = "default_pmx_small_index_size")]
    pub morph: u8,
    #[serde(default = "default_pmx_small_index_size")]
    pub rigid_body: u8,
}

impl Default for PmxPartsIndexSizes {
    fn default() -> Self {
        Self {
            vertex: default_pmx_vertex_index_size(),
            texture: default_pmx_small_index_size(),
            material: default_pmx_small_index_size(),
            bone: default_pmx_small_index_size(),
            morph: default_pmx_small_index_size(),
            rigid_body: default_pmx_small_index_size(),
        }
    }
}

pub struct PmxPartsInput<'a> {
    pub descriptor: PmxPartsDescriptor,
    pub positions_xyz: &'a [f32],
    pub normals_xyz: &'a [f32],
    pub uvs_xy: &'a [f32],
    pub indices: &'a [u32],
    pub skin_indices: &'a [u32],
    pub skin_weights: &'a [f32],
    pub edge_scale: &'a [f32],
}

fn default_pmx_format() -> String {
    "pmx".to_owned()
}

fn default_pmx_parts_version() -> f32 {
    2.0
}

fn default_pmx_parts_encoding() -> String {
    "utf-8".to_owned()
}

fn default_pmx_parts_name() -> String {
    "model".to_owned()
}

fn default_pmx_parts_root_bone_name() -> String {
    "root".to_owned()
}

fn default_negative_index() -> i32 {
    -1
}

fn default_pmx_parts_display_frame_kind() -> String {
    "bone".to_owned()
}

fn default_pmx_parts_morph_kind() -> String {
    "vertex".to_owned()
}

fn default_pmx_parts_rigid_body_shape() -> String {
    "sphere".to_owned()
}

fn default_pmx_parts_rigid_body_mode() -> String {
    "static".to_owned()
}

fn default_pmx_parts_joint_kind() -> String {
    "generic6dofSpring".to_owned()
}

fn default_unit_vec3() -> [f32; 3] {
    [1.0, 1.0, 1.0]
}

fn default_pmx_parts_mass() -> f32 {
    1.0
}

fn default_pmx_parts_material_name() -> String {
    "material".to_owned()
}

fn default_pmx_parts_shared_toon_index() -> Option<u8> {
    Some(0)
}

fn default_pmx_parts_diffuse() -> [f32; 4] {
    [0.8, 0.8, 0.8, 1.0]
}

fn default_pmx_parts_specular_power() -> f32 {
    1.0
}

fn default_pmx_parts_ambient() -> [f32; 3] {
    [0.2, 0.2, 0.2]
}

fn default_pmx_parts_edge_color() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}

fn default_pmx_parts_edge_size() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

fn default_pmx_vertex_index_size() -> u8 {
    4
}

fn default_pmx_small_index_size() -> u8 {
    1
}

pub fn build_pmx_model_from_parts(input: PmxPartsInput<'_>) -> Result<PmxParsedModel, String> {
    let vertex_count = validate_pmx_parts_geometry(
        input.positions_xyz,
        input.normals_xyz,
        input.uvs_xy,
        input.indices,
        input.skin_indices,
        input.skin_weights,
        input.edge_scale,
    )?;
    validate_pmx_parts_descriptor(
        &input.descriptor,
        vertex_count,
        input.indices,
        input.skin_indices,
    )?;

    let skin_indices = if input.skin_indices.is_empty() {
        vec![0; vertex_count * 4]
    } else {
        input.skin_indices.to_vec()
    };
    let skin_weights = if input.skin_weights.is_empty() {
        default_pmx_skin_weights(vertex_count)
    } else {
        input.skin_weights.to_vec()
    };
    let edge_scale = if input.edge_scale.is_empty() {
        vec![1.0; vertex_count]
    } else {
        input.edge_scale.to_vec()
    };
    let face_count = input.indices.len() / 3;
    let (materials, material_groups) = build_pmx_parts_materials(&input.descriptor, face_count);
    let bones = build_pmx_parts_bones(&input.descriptor);
    let morphs = build_pmx_parts_morphs(&input.descriptor);
    let display_frames = build_pmx_parts_display_frames(&input.descriptor);
    let rigid_bodies = build_pmx_parts_rigid_bodies(&input.descriptor);
    let joints = build_pmx_parts_joints(&input.descriptor);

    let model = PmxParsedModel {
        metadata: PmxParsedMetadata {
            format: "pmx".to_owned(),
            version: input.descriptor.version,
            encoding: input.descriptor.encoding,
            name: input.descriptor.name,
            english_name: input.descriptor.english_name,
            comment: input.descriptor.comment,
            english_comment: input.descriptor.english_comment,
            counts: PmxParsedCounts {
                vertices: vertex_count,
                faces: face_count,
                materials: materials.len(),
                bones: bones.len(),
                morphs: morphs.len(),
                display_frames: display_frames.len(),
                rigid_bodies: rigid_bodies.len(),
                joints: joints.len(),
                soft_bodies: 0,
            },
            index_sizes: PmxParsedIndexSizes {
                vertex: input.descriptor.index_sizes.vertex,
                texture: input.descriptor.index_sizes.texture,
                material: input.descriptor.index_sizes.material,
                bone: input.descriptor.index_sizes.bone,
                morph: input.descriptor.index_sizes.morph,
                rigid_body: input.descriptor.index_sizes.rigid_body,
            },
            additional_uv_count: 0,
        },
        geometry: PmxParsedGeometry {
            positions: input.positions_xyz.to_vec(),
            normals: input.normals_xyz.to_vec(),
            uvs: input.uvs_xy.to_vec(),
            additional_uvs: Vec::new(),
            indices: input.indices.to_vec(),
            skin_indices,
            skin_weights,
            edge_scale,
            material_groups,
        },
        materials,
        skeleton: PmxParsedSkeleton { bones },
        morphs,
        display_frames,
        rigid_bodies,
        joints,
        soft_bodies: Vec::new(),
        diagnostics: Vec::new(),
    };
    validate_pmx_export_model(&model)?;
    Ok(model)
}

pub fn validate_pmx_export_model(model: &PmxParsedModel) -> Result<(), String> {
    let geometry = &model.geometry;
    if !geometry.positions.len().is_multiple_of(3) {
        return Err(format!(
            "PMX export positions length must be divisible by 3: {}",
            geometry.positions.len()
        ));
    }
    let vertex_count = geometry.positions.len() / 3;
    if geometry.normals.len() != vertex_count * 3 {
        return Err(format!(
            "PMX export normals length mismatch: expected {}, got {}",
            vertex_count * 3,
            geometry.normals.len()
        ));
    }
    if geometry.uvs.len() != vertex_count * 2 {
        return Err(format!(
            "PMX export uvs length mismatch: expected {}, got {}",
            vertex_count * 2,
            geometry.uvs.len()
        ));
    }
    if geometry.additional_uvs.len() != model.metadata.additional_uv_count as usize {
        return Err(format!(
            "PMX export additionalUvs set count mismatch: expected {}, got {}",
            model.metadata.additional_uv_count,
            geometry.additional_uvs.len()
        ));
    }
    for (index, values) in geometry.additional_uvs.iter().enumerate() {
        if values.len() != vertex_count * 4 {
            return Err(format!(
                "PMX export additionalUvs[{}] length mismatch: expected {}, got {}",
                index,
                vertex_count * 4,
                values.len()
            ));
        }
    }
    if !geometry.indices.len().is_multiple_of(3) {
        return Err(format!(
            "PMX export indices length must be divisible by 3: {}",
            geometry.indices.len()
        ));
    }
    let expected_skin_len = vertex_count * 4;
    if geometry.skin_indices.is_empty() != geometry.skin_weights.is_empty() {
        return Err(
            "PMX export skinIndices and skinWeights must be both empty or both provided".to_owned(),
        );
    }
    if !geometry.skin_indices.is_empty() && geometry.skin_indices.len() != expected_skin_len {
        return Err(format!(
            "PMX export skinIndices length mismatch: expected {}, got {}",
            expected_skin_len,
            geometry.skin_indices.len()
        ));
    }
    if !geometry.skin_weights.is_empty() && geometry.skin_weights.len() != expected_skin_len {
        return Err(format!(
            "PMX export skinWeights length mismatch: expected {}, got {}",
            expected_skin_len,
            geometry.skin_weights.len()
        ));
    }
    Ok(())
}

fn validate_pmx_parts_geometry(
    positions_xyz: &[f32],
    normals_xyz: &[f32],
    uvs_xy: &[f32],
    indices: &[u32],
    skin_indices: &[u32],
    skin_weights: &[f32],
    edge_scale: &[f32],
) -> Result<usize, String> {
    if !positions_xyz.len().is_multiple_of(3) {
        return Err(format!(
            "positions_xyz must contain vertex_count * 3 values, got {}",
            positions_xyz.len()
        ));
    }
    let vertex_count = positions_xyz.len() / 3;
    if vertex_count == 0 {
        return Err("PMX parts export requires at least one vertex".to_owned());
    }
    if normals_xyz.len() != vertex_count * 3 {
        return Err(format!(
            "normals_xyz must contain vertex_count * 3 values, expected {}, got {}",
            vertex_count * 3,
            normals_xyz.len()
        ));
    }
    if uvs_xy.len() != vertex_count * 2 {
        return Err(format!(
            "uvs_xy must contain vertex_count * 2 values, expected {}, got {}",
            vertex_count * 2,
            uvs_xy.len()
        ));
    }
    if !indices.len().is_multiple_of(3) {
        return Err(format!(
            "indices must contain triangle indices and be divisible by 3, got {}",
            indices.len()
        ));
    }
    if let Some(&index) = indices
        .iter()
        .find(|&&index| index as usize >= vertex_count)
    {
        return Err(format!(
            "indices contains out-of-range vertex index {index} for vertex_count {vertex_count}"
        ));
    }
    if !skin_indices.is_empty() && skin_indices.len() != vertex_count * 4 {
        return Err(format!(
            "skin_indices must contain vertex_count * 4 values when provided, expected {}, got {}",
            vertex_count * 4,
            skin_indices.len()
        ));
    }
    if !skin_weights.is_empty() && skin_weights.len() != vertex_count * 4 {
        return Err(format!(
            "skin_weights must contain vertex_count * 4 values when provided, expected {}, got {}",
            vertex_count * 4,
            skin_weights.len()
        ));
    }
    if skin_indices.is_empty() != skin_weights.is_empty() {
        return Err(
            "skin_indices and skin_weights must either both be provided or both be empty"
                .to_owned(),
        );
    }
    if !edge_scale.is_empty() && edge_scale.len() != vertex_count {
        return Err(format!(
            "edge_scale must contain vertex_count values when provided, expected {}, got {}",
            vertex_count,
            edge_scale.len()
        ));
    }
    Ok(vertex_count)
}

fn validate_pmx_parts_descriptor(
    descriptor: &PmxPartsDescriptor,
    vertex_count: usize,
    indices: &[u32],
    skin_indices: &[u32],
) -> Result<(), String> {
    if descriptor.version < 2.0 {
        return Err(format!(
            "PMX parts export supports PMX 2.0 or newer, got {}",
            descriptor.version
        ));
    }
    if descriptor.encoding != "utf-8" && descriptor.encoding != "utf-16-le" {
        return Err(format!(
            "PMX parts export encoding must be utf-8 or utf-16-le, got {}",
            descriptor.encoding
        ));
    }
    validate_pmx_index_size("vertex", descriptor.index_sizes.vertex)?;
    validate_pmx_index_size("texture", descriptor.index_sizes.texture)?;
    validate_pmx_index_size("material", descriptor.index_sizes.material)?;
    validate_pmx_index_size("bone", descriptor.index_sizes.bone)?;
    validate_pmx_index_size("morph", descriptor.index_sizes.morph)?;
    validate_pmx_index_size("rigidBody", descriptor.index_sizes.rigid_body)?;
    if indices.len() / 3 > i32::MAX as usize {
        return Err(format!(
            "face count {} exceeds PMX i32 material face count limit",
            indices.len() / 3
        ));
    }
    if !pmx_vertex_index_size_can_hold(descriptor.index_sizes.vertex, vertex_count) {
        return Err(format!(
            "vertex index size {} cannot hold vertex_count {}",
            descriptor.index_sizes.vertex, vertex_count
        ));
    }
    if !indices.is_empty()
        && !pmx_vertex_index_size_can_hold(
            descriptor.index_sizes.vertex,
            indices.iter().copied().max().unwrap_or(0) as usize + 1,
        )
    {
        return Err(format!(
            "vertex index size {} cannot hold max index {}",
            descriptor.index_sizes.vertex,
            indices.iter().copied().max().unwrap_or(0)
        ));
    }
    validate_pmx_parts_materials(descriptor, indices.len() / 3)?;
    validate_pmx_parts_bones(descriptor)?;
    validate_pmx_parts_morphs(descriptor, vertex_count)?;
    validate_pmx_parts_skin_indices(descriptor, skin_indices)?;
    validate_pmx_parts_display_frames(descriptor)?;
    validate_pmx_parts_physics(descriptor)?;
    Ok(())
}

fn validate_pmx_parts_materials(
    descriptor: &PmxPartsDescriptor,
    face_count: usize,
) -> Result<(), String> {
    if descriptor.materials.is_empty() {
        return Ok(());
    }
    if !pmx_signed_index_size_can_hold_count(
        descriptor.index_sizes.material,
        descriptor.materials.len(),
    ) {
        return Err(format!(
            "material index size {} cannot hold material count {}",
            descriptor.index_sizes.material,
            descriptor.materials.len()
        ));
    }
    let mut total = 0usize;
    for (index, material) in descriptor.materials.iter().enumerate() {
        if material.face_count < 0 {
            return Err(format!(
                "materials[{index}].faceCount must be non-negative, got {}",
                material.face_count
            ));
        }
        total += material.face_count as usize;
    }
    if total != face_count {
        return Err(format!(
            "materials faceCount sum must match index face count, expected {}, got {}",
            face_count, total
        ));
    }
    Ok(())
}

fn validate_pmx_parts_bones(descriptor: &PmxPartsDescriptor) -> Result<(), String> {
    let bone_count = descriptor.bones.len().max(1);
    if !pmx_signed_index_size_can_hold_count(descriptor.index_sizes.bone, bone_count) {
        return Err(format!(
            "bone index size {} cannot hold bone count {}",
            descriptor.index_sizes.bone, bone_count
        ));
    }
    for (index, bone) in descriptor.bones.iter().enumerate() {
        if bone.parent_index < -1
            || (bone.parent_index != -1 && bone.parent_index as usize >= bone_count)
        {
            return Err(format!(
                "bones[{index}].parentIndex must be -1 or a valid bone index, got {}",
                bone.parent_index
            ));
        }
        if bone.tail_index < -1 || (bone.tail_index != -1 && bone.tail_index as usize >= bone_count)
        {
            return Err(format!(
                "bones[{index}].tailIndex must be -1 or a valid bone index, got {}",
                bone.tail_index
            ));
        }
    }
    Ok(())
}

fn validate_pmx_parts_skin_indices(
    descriptor: &PmxPartsDescriptor,
    skin_indices: &[u32],
) -> Result<(), String> {
    if skin_indices.is_empty() {
        return Ok(());
    }
    let bone_count = descriptor.bones.len().max(1);
    if let Some(&index) = skin_indices
        .iter()
        .find(|&&index| index as usize >= bone_count)
    {
        return Err(format!(
            "skin_indices contains out-of-range bone index {index} for bone count {bone_count}"
        ));
    }
    Ok(())
}

fn validate_pmx_parts_morphs(
    descriptor: &PmxPartsDescriptor,
    vertex_count: usize,
) -> Result<(), String> {
    if !pmx_signed_index_size_can_hold_count(descriptor.index_sizes.morph, descriptor.morphs.len())
    {
        return Err(format!(
            "morph index size {} cannot hold morph count {}",
            descriptor.index_sizes.morph,
            descriptor.morphs.len()
        ));
    }
    for (morph_index, morph) in descriptor.morphs.iter().enumerate() {
        match morph.kind.as_str() {
            "vertex" => {
                for (offset_index, offset) in morph.vertex_offsets.iter().enumerate() {
                    if offset.vertex_index as usize >= vertex_count {
                        return Err(format!(
                            "morphs[{morph_index}].vertexOffsets[{offset_index}].vertexIndex out of range: {} for vertex_count {vertex_count}",
                            offset.vertex_index
                        ));
                    }
                }
            }
            "group" => {
                for (offset_index, offset) in morph.group_offsets.iter().enumerate() {
                    if offset.morph_index < 0
                        || offset.morph_index as usize >= descriptor.morphs.len()
                    {
                        return Err(format!(
                            "morphs[{morph_index}].groupOffsets[{offset_index}].morphIndex out of range: {}",
                            offset.morph_index
                        ));
                    }
                }
            }
            other => {
                return Err(format!(
                    "morphs[{morph_index}].kind must be vertex or group, got {other}"
                ));
            }
        }
    }
    Ok(())
}

fn validate_pmx_parts_display_frames(descriptor: &PmxPartsDescriptor) -> Result<(), String> {
    let bone_count = descriptor.bones.len().max(1);
    let morph_count = descriptor.morphs.len();
    for (frame_index, frame) in descriptor.display_frames.iter().enumerate() {
        for (item_index, item) in frame.frames.iter().enumerate() {
            match item.kind.as_str() {
                "bone" => {
                    if item.index < 0 || item.index as usize >= bone_count {
                        return Err(format!(
                            "displayFrames[{frame_index}].frames[{item_index}].index must reference an existing bone, got {}",
                            item.index
                        ));
                    }
                }
                "morph" => {
                    if item.index < 0 || item.index as usize >= morph_count {
                        return Err(format!(
                            "displayFrames[{frame_index}].frames[{item_index}].index must reference an existing morph, got {}",
                            item.index
                        ));
                    }
                }
                other => {
                    return Err(format!(
                        "displayFrames[{frame_index}].frames[{item_index}].kind must be bone or morph, got {other}"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_pmx_parts_physics(descriptor: &PmxPartsDescriptor) -> Result<(), String> {
    let bone_count = descriptor.bones.len().max(1);
    if !pmx_signed_index_size_can_hold_count(
        descriptor.index_sizes.rigid_body,
        descriptor.rigid_bodies.len(),
    ) {
        return Err(format!(
            "rigidBody index size {} cannot hold rigid body count {}",
            descriptor.index_sizes.rigid_body,
            descriptor.rigid_bodies.len()
        ));
    }
    for (index, body) in descriptor.rigid_bodies.iter().enumerate() {
        if body.bone_index < -1 || (body.bone_index != -1 && body.bone_index as usize >= bone_count)
        {
            return Err(format!(
                "rigidBodies[{index}].boneIndex must be -1 or a valid bone index, got {}",
                body.bone_index
            ));
        }
        if !matches!(body.shape.as_str(), "sphere" | "box" | "capsule") {
            return Err(format!(
                "rigidBodies[{index}].shape must be sphere, box, or capsule, got {}",
                body.shape
            ));
        }
        if !matches!(body.mode.as_str(), "static" | "dynamic" | "dynamicBone") {
            return Err(format!(
                "rigidBodies[{index}].mode must be static, dynamic, or dynamicBone, got {}",
                body.mode
            ));
        }
    }
    for (index, joint) in descriptor.joints.iter().enumerate() {
        validate_pmx_parts_rigid_body_ref(
            "rigidBodyIndexA",
            index,
            joint.rigid_body_index_a,
            descriptor.rigid_bodies.len(),
        )?;
        validate_pmx_parts_rigid_body_ref(
            "rigidBodyIndexB",
            index,
            joint.rigid_body_index_b,
            descriptor.rigid_bodies.len(),
        )?;
        if !matches!(
            joint.kind.as_str(),
            "generic6dofSpring" | "generic6dof" | "point2point" | "coneTwist" | "slider" | "hinge"
        ) {
            return Err(format!(
                "joints[{index}].kind has unsupported PMX joint type {}",
                joint.kind
            ));
        }
    }
    Ok(())
}

fn validate_pmx_parts_rigid_body_ref(
    field: &str,
    joint_index: usize,
    value: i32,
    rigid_body_count: usize,
) -> Result<(), String> {
    if value < -1 || (value != -1 && value as usize >= rigid_body_count) {
        Err(format!(
            "joints[{joint_index}].{field} must be -1 or a valid rigid body index, got {value}"
        ))
    } else {
        Ok(())
    }
}

fn validate_pmx_index_size(name: &str, value: u8) -> Result<(), String> {
    if matches!(value, 1 | 2 | 4) {
        Ok(())
    } else {
        Err(format!(
            "PMX {name} index size must be 1, 2, or 4, got {value}"
        ))
    }
}

fn pmx_vertex_index_size_can_hold(size: u8, count: usize) -> bool {
    match size {
        1 => count <= u8::MAX as usize + 1,
        2 => count <= u16::MAX as usize + 1,
        4 => true,
        _ => false,
    }
}

fn pmx_signed_index_size_can_hold_count(size: u8, count: usize) -> bool {
    match size {
        1 => count <= i8::MAX as usize + 1,
        2 => count <= i16::MAX as usize + 1,
        4 => true,
        _ => false,
    }
}

fn default_pmx_skin_weights(vertex_count: usize) -> Vec<f32> {
    let mut weights = vec![0.0; vertex_count * 4];
    for vertex in 0..vertex_count {
        weights[vertex * 4] = 1.0;
    }
    weights
}

fn build_pmx_parts_materials(
    descriptor: &PmxPartsDescriptor,
    face_count: usize,
) -> (Vec<PmxParsedMaterial>, Vec<PmxParsedMaterialGroup>) {
    if face_count == 0 {
        return (Vec::new(), Vec::new());
    }
    if descriptor.materials.is_empty() {
        return (
            vec![default_pmx_parts_material(
                &descriptor.material_name,
                &descriptor.english_material_name,
                face_count as i32,
            )],
            vec![PmxParsedMaterialGroup {
                start: 0,
                count: face_count * 3,
                material_index: 0,
            }],
        );
    }

    let mut start = 0usize;
    let mut materials = Vec::with_capacity(descriptor.materials.len());
    let mut groups = Vec::with_capacity(descriptor.materials.len());
    for (index, material) in descriptor.materials.iter().enumerate() {
        let index_count = material.face_count as usize * 3;
        materials.push(pmx_parts_material_from_descriptor(material));
        groups.push(PmxParsedMaterialGroup {
            start,
            count: index_count,
            material_index: index,
        });
        start += index_count;
    }
    (materials, groups)
}

fn pmx_parts_material_from_descriptor(material: &PmxPartsMaterialDescriptor) -> PmxParsedMaterial {
    PmxParsedMaterial {
        name: material.name.clone(),
        english_name: material.english_name.clone(),
        texture_path: material.texture_path.clone(),
        sphere_texture_path: material.sphere_texture_path.clone(),
        sphere_mode: if material.sphere_mode.is_empty() {
            "none".to_owned()
        } else {
            material.sphere_mode.clone()
        },
        toon_texture_path: material.toon_texture_path.clone(),
        shared_toon_index: material.shared_toon_index,
        diffuse: material.diffuse,
        specular: material.specular,
        specular_power: material.specular_power,
        ambient: material.ambient,
        edge_color: material.edge_color,
        edge_size: material.edge_size,
        flags: PmxParsedMaterialFlags {
            double_sided: material.flags.double_sided,
            ground_shadow: material.flags.ground_shadow,
            self_shadow_map: material.flags.self_shadow_map,
            self_shadow: material.flags.self_shadow,
            edge: material.flags.edge,
            vertex_color: material.flags.vertex_color,
            point_draw: material.flags.point_draw,
            line_draw: material.flags.line_draw,
        },
        face_count: material.face_count,
    }
}

fn build_pmx_parts_bones(descriptor: &PmxPartsDescriptor) -> Vec<PmxParsedBone> {
    if descriptor.bones.is_empty() {
        return vec![default_pmx_parts_root_bone()];
    }
    descriptor
        .bones
        .iter()
        .map(pmx_parts_bone_from_descriptor)
        .collect()
}

fn build_pmx_parts_display_frames(descriptor: &PmxPartsDescriptor) -> Vec<PmxParsedDisplayFrame> {
    descriptor
        .display_frames
        .iter()
        .map(|frame| PmxParsedDisplayFrame {
            name: frame.name.clone(),
            english_name: frame.english_name.clone(),
            special: frame.special,
            frames: frame
                .frames
                .iter()
                .map(|item| PmxParsedDisplayFrameElement {
                    kind: item.kind.clone(),
                    index: item.index,
                })
                .collect(),
        })
        .collect()
}

fn build_pmx_parts_morphs(descriptor: &PmxPartsDescriptor) -> Vec<PmxParsedMorph> {
    descriptor
        .morphs
        .iter()
        .map(pmx_parts_morph_from_descriptor)
        .collect()
}

fn build_pmx_parts_rigid_bodies(descriptor: &PmxPartsDescriptor) -> Vec<PmxParsedRigidBody> {
    descriptor
        .rigid_bodies
        .iter()
        .map(|body| PmxParsedRigidBody {
            name: body.name.clone(),
            english_name: body.english_name.clone(),
            bone_index: body.bone_index,
            group: body.group,
            mask: body.mask,
            shape: body.shape.clone(),
            size: body.size,
            position: body.position,
            rotation: body.rotation,
            mass: body.mass,
            linear_damping: body.linear_damping,
            angular_damping: body.angular_damping,
            restitution: body.restitution,
            friction: body.friction,
            mode: body.mode.clone(),
        })
        .collect()
}

fn build_pmx_parts_joints(descriptor: &PmxPartsDescriptor) -> Vec<PmxParsedJoint> {
    descriptor
        .joints
        .iter()
        .map(|joint| PmxParsedJoint {
            name: joint.name.clone(),
            english_name: joint.english_name.clone(),
            kind: joint.kind.clone(),
            rigid_body_index_a: joint.rigid_body_index_a,
            rigid_body_index_b: joint.rigid_body_index_b,
            position: joint.position,
            rotation: joint.rotation,
            translation_lower_limit: joint.translation_lower_limit,
            translation_upper_limit: joint.translation_upper_limit,
            rotation_lower_limit: joint.rotation_lower_limit,
            rotation_upper_limit: joint.rotation_upper_limit,
            spring_translation_factor: joint.spring_translation_factor,
            spring_rotation_factor: joint.spring_rotation_factor,
        })
        .collect()
}

fn pmx_parts_morph_from_descriptor(morph: &PmxPartsMorphDescriptor) -> PmxParsedMorph {
    let mut parsed = PmxParsedMorph {
        name: morph.name.clone(),
        english_name: morph.english_name.clone(),
        kind: morph.kind.clone(),
        vertex_offsets: Vec::new(),
        group_offsets: Vec::new(),
        bone_offsets: Vec::new(),
        uv_offsets: Vec::new(),
        additional_uv_offsets: Vec::new(),
        material_offsets: Vec::new(),
        flip_offsets: Vec::new(),
        impulse_offsets: Vec::new(),
    };
    match morph.kind.as_str() {
        "vertex" => {
            parsed.vertex_offsets = morph
                .vertex_offsets
                .iter()
                .map(|offset| PmxParsedVertexMorphOffset {
                    vertex_index: offset.vertex_index,
                    position: offset.position,
                })
                .collect();
        }
        "group" => {
            parsed.group_offsets = morph
                .group_offsets
                .iter()
                .map(|offset| PmxParsedGroupMorphOffset {
                    morph_index: offset.morph_index,
                    weight: offset.weight,
                })
                .collect();
        }
        _ => unreachable!("PmxPartsMorphDescriptor was validated before build"),
    }
    parsed
}

fn pmx_parts_bone_from_descriptor(bone: &PmxPartsBoneDescriptor) -> PmxParsedBone {
    let tail_position = if bone.tail_index >= 0 {
        None
    } else {
        Some(bone.tail_position.unwrap_or([0.0, 1.0, 0.0]))
    };
    PmxParsedBone {
        name: bone.name.clone(),
        english_name: bone.english_name.clone(),
        parent_index: bone.parent_index,
        layer: bone.layer,
        position: bone.position,
        tail_index: bone.tail_index,
        tail_position,
        flags: PmxParsedBoneFlags {
            indexed_tail: bone.tail_index >= 0,
            rotatable: bone.rotatable,
            translatable: bone.translatable,
            visible: bone.visible,
            enabled: bone.enabled,
            ik: false,
            append_local: false,
            append_rotate: false,
            append_translate: false,
            fixed_axis: false,
            local_axis: false,
            external_parent_transform: false,
            transform_after_physics: false,
        },
        append_transform: None,
        fixed_axis: None,
        local_axis: None,
        external_parent_key: None,
        ik: None,
    }
}

fn default_pmx_parts_material(
    name: &str,
    english_name: &str,
    face_count: i32,
) -> PmxParsedMaterial {
    PmxParsedMaterial {
        name: if name.is_empty() {
            "material".to_owned()
        } else {
            name.to_owned()
        },
        english_name: english_name.to_owned(),
        texture_path: String::new(),
        sphere_texture_path: String::new(),
        sphere_mode: "none".to_owned(),
        toon_texture_path: String::new(),
        shared_toon_index: Some(0),
        diffuse: [0.8, 0.8, 0.8, 1.0],
        specular: [0.0, 0.0, 0.0],
        specular_power: 1.0,
        ambient: [0.2, 0.2, 0.2],
        edge_color: [0.0, 0.0, 0.0, 1.0],
        edge_size: 1.0,
        flags: PmxParsedMaterialFlags {
            double_sided: true,
            ground_shadow: true,
            self_shadow_map: true,
            self_shadow: true,
            edge: false,
            vertex_color: false,
            point_draw: false,
            line_draw: false,
        },
        face_count,
    }
}

fn default_pmx_parts_root_bone() -> PmxParsedBone {
    PmxParsedBone {
        name: "root".to_owned(),
        english_name: "root".to_owned(),
        parent_index: -1,
        layer: 0,
        position: [0.0, 0.0, 0.0],
        tail_index: -1,
        tail_position: Some([0.0, 1.0, 0.0]),
        flags: PmxParsedBoneFlags {
            indexed_tail: false,
            rotatable: true,
            translatable: true,
            visible: true,
            enabled: true,
            ik: false,
            append_local: false,
            append_rotate: false,
            append_translate: false,
            fixed_axis: false,
            local_axis: false,
            transform_after_physics: false,
            external_parent_transform: false,
        },
        append_transform: None,
        fixed_axis: None,
        local_axis: None,
        external_parent_key: None,
        ik: None,
    }
}

pub fn parse_pmx_model(data: &[u8]) -> Result<PmxParsedModel, ImportError> {
    let (header, pos) = read_header(data)?;
    let mut r = Reader { data, pos };
    let name = r.read_string(header.encoding)?;
    let english_name = r.read_string(header.encoding)?;
    let comment = r.read_string(header.encoding)?;
    let english_comment = r.read_string(header.encoding)?;

    let vertex_count = read_section_count_with_min_record(&mut r, 12 + 12 + 8 + 1 + 4)?;
    let geometry = read_parsed_geometry(&mut r, &header, vertex_count)?;
    let index_count = geometry.indices.len();
    let textures = read_parsed_textures(&mut r, header.encoding)?;
    let materials = read_parsed_materials(&mut r, &header, &textures)?;
    let material_groups = build_material_groups(&materials);
    let bone_count = read_section_count_with_min_record(&mut r, 4 + 4 + 12)?;
    let skeleton = PmxParsedSkeleton {
        bones: read_parsed_bones(&mut r, &header, bone_count)?,
    };
    let morphs = read_parsed_morphs(&mut r, &header)?;
    let display_frames = read_parsed_display_frames(&mut r, &header)?;
    let rigid_bodies = if r.remaining() >= 4 {
        read_parsed_rigid_bodies(&mut r, &header)?
    } else {
        Vec::new()
    };
    let joints = if r.remaining() >= 4 {
        read_parsed_joints(&mut r, &header)?
    } else {
        Vec::new()
    };
    let soft_bodies = if header.version >= 2.05 && r.remaining() >= 4 {
        read_parsed_soft_bodies(&mut r, &header)?
    } else {
        Vec::new()
    };

    let mut geometry = geometry;
    geometry.material_groups = material_groups;
    let mut diagnostics = Vec::new();
    if !soft_bodies.is_empty() {
        diagnostics.push(PmxParserDiagnostic {
            level: "warning".to_owned(),
            code: "PMX_SOFT_BODY_UNSUPPORTED".to_owned(),
            message: format!(
                "{} PMX soft bodies are parsed only as header records by mmd-anim.",
                soft_bodies.len()
            ),
        });
    }
    if r.remaining() > 0 {
        diagnostics.push(PmxParserDiagnostic {
            level: "warning".to_owned(),
            code: "PMX_TRAILING_DATA_UNPARSED".to_owned(),
            message: format!("{} trailing PMX bytes were left unparsed.", r.remaining()),
        });
    }

    Ok(PmxParsedModel {
        metadata: PmxParsedMetadata {
            format: "pmx".to_owned(),
            version: header.version,
            encoding: match header.encoding {
                TextEncoding::Utf8 => "utf-8".to_owned(),
                TextEncoding::Utf16Le => "utf-16-le".to_owned(),
            },
            name,
            english_name,
            comment,
            english_comment,
            counts: PmxParsedCounts {
                vertices: vertex_count,
                faces: index_count / 3,
                materials: materials.len(),
                bones: skeleton.bones.len(),
                morphs: morphs.len(),
                display_frames: display_frames.len(),
                rigid_bodies: rigid_bodies.len(),
                joints: joints.len(),
                soft_bodies: soft_bodies.len(),
            },
            index_sizes: PmxParsedIndexSizes {
                vertex: header.vertex_index_size,
                texture: header.texture_index_size,
                material: header.material_index_size,
                bone: header.bone_index_size,
                morph: header.morph_index_size,
                rigid_body: header.rigidbody_index_size,
            },
            additional_uv_count: header.extra_uv_count,
        },
        geometry,
        materials,
        skeleton,
        morphs,
        display_frames,
        rigid_bodies,
        joints,
        soft_bodies,
        diagnostics,
    })
}

pub fn export_pmx_model(model: &PmxParsedModel) -> Vec<u8> {
    let encoding = pmx_text_encoding(&model.metadata);
    let mut out = Vec::new();
    out.extend_from_slice(&PMX_MAGIC);
    write_f32(&mut out, model.metadata.version);
    out.push(8);
    out.push(encoding as u8);
    out.push(model.metadata.additional_uv_count);
    out.push(model.metadata.index_sizes.vertex);
    out.push(model.metadata.index_sizes.texture);
    out.push(model.metadata.index_sizes.material);
    out.push(model.metadata.index_sizes.bone);
    out.push(model.metadata.index_sizes.morph);
    out.push(model.metadata.index_sizes.rigid_body);

    write_pmx_string(&mut out, &model.metadata.name, encoding);
    write_pmx_string(&mut out, &model.metadata.english_name, encoding);
    write_pmx_string(&mut out, &model.metadata.comment, encoding);
    write_pmx_string(&mut out, &model.metadata.english_comment, encoding);

    let vertex_count = model.geometry.positions.len() / 3;
    write_i32(&mut out, vertex_count as i32);
    for index in 0..vertex_count {
        write_f32_slice(
            &mut out,
            &model.geometry.positions[index * 3..index * 3 + 3],
        );
        write_f32_slice(&mut out, &model.geometry.normals[index * 3..index * 3 + 3]);
        write_f32_slice(&mut out, &model.geometry.uvs[index * 2..index * 2 + 2]);
        for uv in &model.geometry.additional_uvs {
            write_f32_slice(&mut out, &uv[index * 4..index * 4 + 4]);
        }
        out.push(2); // BDEF4 preserves the parsed skin index/weight arrays.
        for slot in 0..4 {
            write_sized_index(
                &mut out,
                model
                    .geometry
                    .skin_indices
                    .get(index * 4 + slot)
                    .copied()
                    .unwrap_or(0) as i32,
                model.metadata.index_sizes.bone,
            );
        }
        for slot in 0..4 {
            write_f32(
                &mut out,
                model
                    .geometry
                    .skin_weights
                    .get(index * 4 + slot)
                    .copied()
                    .unwrap_or(if slot == 0 { 1.0 } else { 0.0 }),
            );
        }
        write_f32(
            &mut out,
            model.geometry.edge_scale.get(index).copied().unwrap_or(1.0),
        );
    }

    write_i32(&mut out, model.geometry.indices.len() as i32);
    for &index in &model.geometry.indices {
        write_vertex_index(&mut out, index, model.metadata.index_sizes.vertex);
    }

    let textures = collect_pmx_textures(&model.materials);
    write_i32(&mut out, textures.len() as i32);
    for texture in &textures {
        write_pmx_string(&mut out, texture, encoding);
    }

    write_i32(&mut out, model.materials.len() as i32);
    for material in &model.materials {
        write_pmx_string(&mut out, &material.name, encoding);
        write_pmx_string(&mut out, &material.english_name, encoding);
        write_f32_slice(&mut out, &material.diffuse);
        write_f32_slice(&mut out, &material.specular);
        write_f32(&mut out, material.specular_power);
        write_f32_slice(&mut out, &material.ambient);
        out.push(material_flag_bits(&material.flags));
        write_f32_slice(&mut out, &material.edge_color);
        write_f32(&mut out, material.edge_size);
        write_sized_index(
            &mut out,
            texture_index(&textures, &material.texture_path),
            model.metadata.index_sizes.texture,
        );
        write_sized_index(
            &mut out,
            texture_index(&textures, &material.sphere_texture_path),
            model.metadata.index_sizes.texture,
        );
        out.push(match material.sphere_mode.as_str() {
            "multiply" => 1,
            "add" => 2,
            "subTexture" => 3,
            _ => 0,
        });
        if let Some(shared) = material.shared_toon_index {
            out.push(1);
            out.push(shared);
        } else {
            out.push(0);
            write_sized_index(
                &mut out,
                texture_index(&textures, &material.toon_texture_path),
                model.metadata.index_sizes.texture,
            );
        }
        write_pmx_string(&mut out, "", encoding);
        write_i32(&mut out, material.face_count.saturating_mul(3));
    }

    write_i32(&mut out, model.skeleton.bones.len() as i32);
    for bone in &model.skeleton.bones {
        write_pmx_string(&mut out, &bone.name, encoding);
        write_pmx_string(&mut out, &bone.english_name, encoding);
        write_f32_slice(&mut out, &bone.position);
        write_sized_index(&mut out, bone.parent_index, model.metadata.index_sizes.bone);
        write_i32(&mut out, bone.layer);
        let bits = bone_flag_bits(&bone.flags);
        write_u16(&mut out, bits);
        if bone.flags.indexed_tail {
            write_sized_index(&mut out, bone.tail_index, model.metadata.index_sizes.bone);
        } else {
            write_f32_slice(&mut out, &bone.tail_position.unwrap_or([0.0; 3]));
        }
        if bone.flags.append_rotate || bone.flags.append_translate {
            let append = bone.append_transform.as_ref();
            write_sized_index(
                &mut out,
                append.map(|value| value.parent_index).unwrap_or(-1),
                model.metadata.index_sizes.bone,
            );
            write_f32(&mut out, append.map(|value| value.weight).unwrap_or(0.0));
        }
        if bone.flags.fixed_axis {
            write_f32_slice(&mut out, &bone.fixed_axis.unwrap_or([0.0; 3]));
        }
        if bone.flags.local_axis {
            if let Some(axis) = &bone.local_axis {
                write_f32_slice(&mut out, &axis.x);
                write_f32_slice(&mut out, &axis.z);
            } else {
                write_f32_slice(&mut out, &[1.0, 0.0, 0.0]);
                write_f32_slice(&mut out, &[0.0, 0.0, 1.0]);
            }
        }
        if bone.flags.external_parent_transform {
            write_i32(&mut out, bone.external_parent_key.unwrap_or(0));
        }
        if bone.flags.ik {
            if let Some(ik) = &bone.ik {
                write_sized_index(&mut out, ik.target_index, model.metadata.index_sizes.bone);
                write_i32(&mut out, ik.loop_count);
                write_f32(&mut out, ik.limit_angle);
                write_i32(&mut out, ik.links.len() as i32);
                for link in &ik.links {
                    write_sized_index(&mut out, link.bone_index, model.metadata.index_sizes.bone);
                    out.push(u8::from(link.limits.is_some()));
                    if let Some(limits) = &link.limits {
                        write_f32_slice(&mut out, &limits.lower);
                        write_f32_slice(&mut out, &limits.upper);
                    }
                }
            } else {
                write_sized_index(&mut out, -1, model.metadata.index_sizes.bone);
                write_i32(&mut out, 0);
                write_f32(&mut out, 0.0);
                write_i32(&mut out, 0);
            }
        }
    }

    write_i32(&mut out, model.morphs.len() as i32);
    for morph in &model.morphs {
        write_pmx_string(&mut out, &morph.name, encoding);
        write_pmx_string(&mut out, &morph.english_name, encoding);
        out.push(0);
        let morph_type = morph_type_byte(morph);
        out.push(morph_type);
        write_i32(&mut out, morph_offset_count(morph, morph_type) as i32);
        match morph_type {
            0 => write_group_morph_offsets(&mut out, &morph.group_offsets, model),
            1 => {
                for offset in &morph.vertex_offsets {
                    write_vertex_index(
                        &mut out,
                        offset.vertex_index,
                        model.metadata.index_sizes.vertex,
                    );
                    write_f32_slice(&mut out, &offset.position);
                }
            }
            2 => {
                for offset in &morph.bone_offsets {
                    write_sized_index(&mut out, offset.bone_index, model.metadata.index_sizes.bone);
                    write_f32_slice(&mut out, &offset.translation);
                    write_f32_slice(&mut out, &offset.rotation);
                }
            }
            3 => write_uv_morph_offsets(&mut out, &morph.uv_offsets, model),
            4..=7 => {
                for offset in &morph.additional_uv_offsets {
                    write_vertex_index(
                        &mut out,
                        offset.vertex_index,
                        model.metadata.index_sizes.vertex,
                    );
                    write_f32_slice(&mut out, &offset.uv);
                }
            }
            8 => {
                for offset in &morph.material_offsets {
                    write_sized_index(
                        &mut out,
                        offset.material_index,
                        model.metadata.index_sizes.material,
                    );
                    out.push(if offset.operation == "multiply" { 0 } else { 1 });
                    write_f32_slice(&mut out, &offset.diffuse);
                    write_f32_slice(&mut out, &offset.specular);
                    write_f32(&mut out, offset.specular_power);
                    write_f32_slice(&mut out, &offset.ambient);
                    write_f32_slice(&mut out, &offset.edge_color);
                    write_f32(&mut out, offset.edge_size);
                    write_f32_slice(&mut out, &offset.texture_factor);
                    write_f32_slice(&mut out, &offset.sphere_texture_factor);
                    write_f32_slice(&mut out, &offset.toon_texture_factor);
                }
            }
            9 => write_group_morph_offsets(&mut out, &morph.flip_offsets, model),
            10 => {
                for offset in &morph.impulse_offsets {
                    write_sized_index(
                        &mut out,
                        offset.rigid_body_index,
                        model.metadata.index_sizes.rigid_body,
                    );
                    out.push(u8::from(offset.local));
                    write_f32_slice(&mut out, &offset.velocity);
                    write_f32_slice(&mut out, &offset.torque);
                }
            }
            _ => {}
        }
    }

    write_i32(&mut out, model.display_frames.len() as i32);
    for frame in &model.display_frames {
        write_pmx_string(&mut out, &frame.name, encoding);
        write_pmx_string(&mut out, &frame.english_name, encoding);
        out.push(u8::from(frame.special));
        write_i32(&mut out, frame.frames.len() as i32);
        for item in &frame.frames {
            if item.kind == "morph" {
                out.push(1);
                write_sized_index(&mut out, item.index, model.metadata.index_sizes.morph);
            } else {
                out.push(0);
                write_sized_index(&mut out, item.index, model.metadata.index_sizes.bone);
            }
        }
    }

    write_i32(&mut out, model.rigid_bodies.len() as i32);
    for body in &model.rigid_bodies {
        write_pmx_string(&mut out, &body.name, encoding);
        write_pmx_string(&mut out, &body.english_name, encoding);
        write_sized_index(&mut out, body.bone_index, model.metadata.index_sizes.bone);
        out.push(body.group);
        write_u16(&mut out, body.mask);
        out.push(match body.shape.as_str() {
            "box" => 1,
            "capsule" => 2,
            _ => 0,
        });
        write_f32_slice(&mut out, &body.size);
        write_f32_slice(&mut out, &body.position);
        write_f32_slice(&mut out, &body.rotation);
        write_f32(&mut out, body.mass);
        write_f32(&mut out, body.linear_damping);
        write_f32(&mut out, body.angular_damping);
        write_f32(&mut out, body.restitution);
        write_f32(&mut out, body.friction);
        out.push(match body.mode.as_str() {
            "dynamic" => 1,
            "dynamicBone" => 2,
            _ => 0,
        });
    }

    write_i32(&mut out, model.joints.len() as i32);
    for joint in &model.joints {
        write_pmx_string(&mut out, &joint.name, encoding);
        write_pmx_string(&mut out, &joint.english_name, encoding);
        out.push(match joint.kind.as_str() {
            "generic6dof" => 1,
            "point2point" => 2,
            "coneTwist" => 3,
            "slider" => 4,
            "hinge" => 5,
            _ => 0,
        });
        write_sized_index(
            &mut out,
            joint.rigid_body_index_a,
            model.metadata.index_sizes.rigid_body,
        );
        write_sized_index(
            &mut out,
            joint.rigid_body_index_b,
            model.metadata.index_sizes.rigid_body,
        );
        write_f32_slice(&mut out, &joint.position);
        write_f32_slice(&mut out, &joint.rotation);
        write_f32_slice(&mut out, &joint.translation_lower_limit);
        write_f32_slice(&mut out, &joint.translation_upper_limit);
        write_f32_slice(&mut out, &joint.rotation_lower_limit);
        write_f32_slice(&mut out, &joint.rotation_upper_limit);
        write_f32_slice(&mut out, &joint.spring_translation_factor);
        write_f32_slice(&mut out, &joint.spring_rotation_factor);
    }

    if model.metadata.version >= 2.05 {
        write_i32(&mut out, model.soft_bodies.len() as i32);
        for soft_body in &model.soft_bodies {
            write_pmx_string(&mut out, &soft_body.name, encoding);
            write_pmx_string(&mut out, &soft_body.english_name, encoding);
            out.push(if soft_body.kind == "rope" { 1 } else { 0 });
            write_sized_index(
                &mut out,
                soft_body.material_index,
                model.metadata.index_sizes.material,
            );
            out.push(soft_body.collision_group);
            write_u16(&mut out, soft_body.collision_mask);
            out.push(soft_body.flags);
            write_i32(&mut out, soft_body.bending_constraints_distance);
            write_i32(&mut out, soft_body.cluster_count);
            write_f32(&mut out, soft_body.total_mass);
            write_f32(&mut out, soft_body.collision_margin);
            out.extend_from_slice(&[0u8; 4 + 12 * 4 + 6 * 4 + 4 * 4 + 3 * 4]);
            write_i32(&mut out, 0);
            write_i32(&mut out, 0);
        }
    }

    out
}

fn read_section_count(r: &mut Reader<'_>) -> Result<usize, ImportError> {
    let count = r.read_i32_le()?;
    if count < 0 {
        return Err(ImportError::SectionOverflow);
    }
    Ok(count as usize)
}

fn read_section_count_with_min_record(
    r: &mut Reader<'_>,
    min_record_size: usize,
) -> Result<usize, ImportError> {
    let count = read_section_count(r)?;
    r.require_record_bytes(count, min_record_size)?;
    Ok(count)
}

fn read_parsed_geometry(
    r: &mut Reader<'_>,
    header: &PmxHeader,
    vertex_count: usize,
) -> Result<PmxParsedGeometry, ImportError> {
    r.require_record_bytes(vertex_count, 12 + 12 + 8 + 1 + 4)?;
    let position_capacity = vertex_count
        .checked_mul(3)
        .ok_or(ImportError::SectionOverflow)?;
    let uv_capacity = vertex_count
        .checked_mul(2)
        .ok_or(ImportError::SectionOverflow)?;
    let skin_capacity = vertex_count
        .checked_mul(4)
        .ok_or(ImportError::SectionOverflow)?;

    let mut positions = Vec::with_capacity(position_capacity);
    let mut normals = Vec::with_capacity(position_capacity);
    let mut uvs = Vec::with_capacity(uv_capacity);
    let mut additional_uvs =
        vec![Vec::with_capacity(skin_capacity); header.extra_uv_count as usize];
    let mut skin_indices = Vec::with_capacity(skin_capacity);
    let mut skin_weights = Vec::with_capacity(skin_capacity);
    let mut edge_scale = Vec::with_capacity(vertex_count);

    for _ in 0..vertex_count {
        positions.extend_from_slice(&r.read_vec3_array()?);
        normals.extend_from_slice(&r.read_vec3_array()?);
        uvs.extend_from_slice(&r.read_vec2_array()?);
        for uv in additional_uvs.iter_mut() {
            uv.extend_from_slice(&r.read_vec4_array()?);
        }
        let weight_type = r.read_u8()?;
        let mut indices = [0u32; 4];
        let mut weights = [0.0f32; 4];
        match weight_type {
            0 => {
                indices[0] =
                    normalize_nonnegative_index(r.read_sized_index(header.bone_index_size)?);
                weights[0] = 1.0;
            }
            1 | 3 => {
                indices[0] =
                    normalize_nonnegative_index(r.read_sized_index(header.bone_index_size)?);
                indices[1] =
                    normalize_nonnegative_index(r.read_sized_index(header.bone_index_size)?);
                let w = r.read_f32_le()?;
                weights[0] = w;
                weights[1] = 1.0 - w;
                if weight_type == 3 {
                    r.skip(36)?;
                }
            }
            2 | 4 => {
                for index in &mut indices {
                    *index =
                        normalize_nonnegative_index(r.read_sized_index(header.bone_index_size)?);
                }
                for weight in &mut weights {
                    *weight = r.read_f32_le()?;
                }
            }
            _ => return Err(ImportError::SectionOverflow),
        }
        skin_indices.extend_from_slice(&indices);
        skin_weights.extend_from_slice(&weights);
        edge_scale.push(r.read_f32_le()?);
    }

    let index_count = read_section_count_with_min_record(r, header.vertex_index_size as usize)?;
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(r.read_vertex_index(header.vertex_index_size)?);
    }

    Ok(PmxParsedGeometry {
        positions,
        normals,
        uvs,
        additional_uvs,
        indices,
        skin_indices,
        skin_weights,
        edge_scale,
        material_groups: Vec::new(),
    })
}

fn normalize_nonnegative_index(index: i32) -> u32 {
    if index < 0 { 0 } else { index as u32 }
}

fn read_parsed_textures(
    r: &mut Reader<'_>,
    encoding: TextEncoding,
) -> Result<Vec<String>, ImportError> {
    let count = read_section_count_with_min_record(r, 4)?;
    let mut textures = Vec::with_capacity(count);
    for _ in 0..count {
        textures.push(r.read_string(encoding)?);
    }
    Ok(textures)
}

fn read_parsed_materials(
    r: &mut Reader<'_>,
    header: &PmxHeader,
    textures: &[String],
) -> Result<Vec<PmxParsedMaterial>, ImportError> {
    let count = read_section_count_with_min_record(r, 4 + 4 + 16 + 16 + 12 + 1 + 16 + 4)?;
    let mut materials = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.read_string(header.encoding)?;
        let english_name = r.read_string(header.encoding)?;
        let diffuse = r.read_vec4_array()?;
        let specular = r.read_vec3_array()?;
        let specular_power = r.read_f32_le()?;
        let ambient = r.read_vec3_array()?;
        let flag_bits = r.read_u8()?;
        let edge_color = r.read_vec4_array()?;
        let edge_size = r.read_f32_le()?;
        let texture_index = r.read_sized_index(header.texture_index_size)?;
        let sphere_texture_index = r.read_sized_index(header.texture_index_size)?;
        let sphere_mode_raw = r.read_u8()?;
        let toon_flag = r.read_u8()?;
        let (toon_texture_path, shared_toon_index) = if toon_flag == 0 {
            let toon_texture_index = r.read_sized_index(header.texture_index_size)?;
            (texture_path(textures, toon_texture_index), None)
        } else {
            (String::new(), Some(r.read_u8()?))
        };
        let _memo = r.read_string(header.encoding)?;
        let face_count = r.read_i32_le()? / 3;
        materials.push(PmxParsedMaterial {
            name,
            english_name,
            texture_path: texture_path(textures, texture_index),
            sphere_texture_path: texture_path(textures, sphere_texture_index),
            sphere_mode: match sphere_mode_raw {
                1 => "multiply",
                2 => "add",
                3 => "subTexture",
                _ => "none",
            }
            .to_owned(),
            toon_texture_path,
            shared_toon_index,
            diffuse,
            specular,
            specular_power,
            ambient,
            edge_color,
            edge_size,
            flags: PmxParsedMaterialFlags {
                double_sided: flag_bits & 0x01 != 0,
                ground_shadow: flag_bits & 0x02 != 0,
                self_shadow_map: flag_bits & 0x04 != 0,
                self_shadow: flag_bits & 0x08 != 0,
                edge: flag_bits & 0x10 != 0,
                vertex_color: flag_bits & 0x20 != 0,
                point_draw: flag_bits & 0x40 != 0,
                line_draw: flag_bits & 0x80 != 0,
            },
            face_count,
        });
    }
    Ok(materials)
}

fn texture_path(textures: &[String], index: i32) -> String {
    if index < 0 {
        String::new()
    } else {
        textures.get(index as usize).cloned().unwrap_or_default()
    }
}

fn build_material_groups(materials: &[PmxParsedMaterial]) -> Vec<PmxParsedMaterialGroup> {
    let mut start = 0usize;
    materials
        .iter()
        .enumerate()
        .map(|(material_index, material)| {
            let count = (material.face_count.max(0) as usize) * 3;
            let group = PmxParsedMaterialGroup {
                start,
                count,
                material_index,
            };
            start += count;
            group
        })
        .collect()
}

fn pmx_text_encoding(metadata: &PmxParsedMetadata) -> TextEncoding {
    match metadata.encoding.as_str() {
        "utf-16-le" => TextEncoding::Utf16Le,
        _ => TextEncoding::Utf8,
    }
}

fn write_pmx_string(out: &mut Vec<u8>, text: &str, encoding: TextEncoding) {
    match encoding {
        TextEncoding::Utf8 => {
            write_i32(out, text.len() as i32);
            out.extend_from_slice(text.as_bytes());
        }
        TextEncoding::Utf16Le => {
            let bytes = text
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>();
            write_i32(out, bytes.len() as i32);
            out.extend_from_slice(&bytes);
        }
    }
}

fn write_f32_slice(out: &mut Vec<u8>, values: &[f32]) {
    for &value in values {
        write_f32(out, value);
    }
}

fn write_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_sized_index(out: &mut Vec<u8>, value: i32, size: u8) {
    match size {
        1 => out.push(value as i8 as u8),
        2 => out.extend_from_slice(&(value as i16).to_le_bytes()),
        4 => out.extend_from_slice(&value.to_le_bytes()),
        _ => {}
    }
}

fn write_vertex_index(out: &mut Vec<u8>, value: u32, size: u8) {
    match size {
        1 => out.push(value as u8),
        2 => out.extend_from_slice(&(value as u16).to_le_bytes()),
        4 => out.extend_from_slice(&(value as i32).to_le_bytes()),
        _ => {}
    }
}

fn material_flag_bits(flags: &PmxParsedMaterialFlags) -> u8 {
    u8::from(flags.double_sided)
        | (u8::from(flags.ground_shadow) << 1)
        | (u8::from(flags.self_shadow_map) << 2)
        | (u8::from(flags.self_shadow) << 3)
        | (u8::from(flags.edge) << 4)
        | (u8::from(flags.vertex_color) << 5)
        | (u8::from(flags.point_draw) << 6)
        | (u8::from(flags.line_draw) << 7)
}

fn bone_flag_bits(flags: &PmxParsedBoneFlags) -> u16 {
    u16::from(flags.indexed_tail)
        | (u16::from(flags.rotatable) << 1)
        | (u16::from(flags.translatable) << 2)
        | (u16::from(flags.visible) << 3)
        | (u16::from(flags.enabled) << 4)
        | if flags.ik { BONE_FLAG_IK } else { 0 }
        | if flags.append_local {
            BONE_FLAG_LOCAL_APPEND
        } else {
            0
        }
        | if flags.append_rotate {
            BONE_FLAG_APPEND_ROTATE
        } else {
            0
        }
        | if flags.append_translate {
            BONE_FLAG_APPEND_TRANSLATE
        } else {
            0
        }
        | if flags.fixed_axis {
            BONE_FLAG_FIXED_AXIS
        } else {
            0
        }
        | if flags.local_axis {
            BONE_FLAG_LOCAL_AXIS
        } else {
            0
        }
        | (u16::from(flags.transform_after_physics) << 12)
        | if flags.external_parent_transform {
            BONE_FLAG_EXTERNAL_PARENT
        } else {
            0
        }
}

fn collect_pmx_textures(materials: &[PmxParsedMaterial]) -> Vec<String> {
    let mut textures = Vec::new();
    for texture in materials.iter().flat_map(|material| {
        [
            material.texture_path.as_str(),
            material.sphere_texture_path.as_str(),
            material.toon_texture_path.as_str(),
        ]
    }) {
        if !texture.is_empty() && !textures.iter().any(|existing| existing == texture) {
            textures.push(texture.to_owned());
        }
    }
    textures
}

fn texture_index(textures: &[String], texture: &str) -> i32 {
    if texture.is_empty() {
        -1
    } else {
        textures
            .iter()
            .position(|candidate| candidate == texture)
            .map(|index| index as i32)
            .unwrap_or(-1)
    }
}

fn morph_type_byte(morph: &PmxParsedMorph) -> u8 {
    match morph.kind.as_str() {
        "group" => 0,
        "vertex" => 1,
        "bone" => 2,
        "uv" => 3,
        "additionalUv" => morph
            .additional_uv_offsets
            .first()
            .map(|offset| offset.uv_index.saturating_add(4).min(7))
            .unwrap_or(4),
        "material" => 8,
        "flip" => 9,
        "impulse" => 10,
        _ => 0,
    }
}

fn morph_offset_count(morph: &PmxParsedMorph, morph_type: u8) -> usize {
    match morph_type {
        0 => morph.group_offsets.len(),
        1 => morph.vertex_offsets.len(),
        2 => morph.bone_offsets.len(),
        3 => morph.uv_offsets.len(),
        4..=7 => morph.additional_uv_offsets.len(),
        8 => morph.material_offsets.len(),
        9 => morph.flip_offsets.len(),
        10 => morph.impulse_offsets.len(),
        _ => 0,
    }
}

fn write_group_morph_offsets(
    out: &mut Vec<u8>,
    offsets: &[PmxParsedGroupMorphOffset],
    model: &PmxParsedModel,
) {
    for offset in offsets {
        write_sized_index(out, offset.morph_index, model.metadata.index_sizes.morph);
        write_f32(out, offset.weight);
    }
}

fn write_uv_morph_offsets(
    out: &mut Vec<u8>,
    offsets: &[PmxParsedUvMorphOffset],
    model: &PmxParsedModel,
) {
    for offset in offsets {
        write_vertex_index(out, offset.vertex_index, model.metadata.index_sizes.vertex);
        write_f32_slice(out, &offset.uv);
    }
}

fn read_parsed_bones(
    r: &mut Reader<'_>,
    header: &PmxHeader,
    count: usize,
) -> Result<Vec<PmxParsedBone>, ImportError> {
    r.require_record_bytes(count, 4 + 4 + 12)?;
    let mut bones = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.read_string(header.encoding)?;
        let english_name = r.read_string(header.encoding)?;
        let position = r.read_vec3_array()?;
        let parent_index = r.read_sized_index(header.bone_index_size)?;
        let layer = r.read_i32_le()?;
        let bits = r.read_u16_le()?;
        let flags = parsed_bone_flags(bits);
        let (tail_index, tail_position) = if flags.indexed_tail {
            (r.read_sized_index(header.bone_index_size)?, None)
        } else {
            (-1, Some(r.read_vec3_array()?))
        };
        let append_transform = if flags.append_rotate || flags.append_translate {
            Some(PmxParsedAppendTransform {
                parent_index: r.read_sized_index(header.bone_index_size)?,
                weight: r.read_f32_le()?,
            })
        } else {
            None
        };
        let fixed_axis = if flags.fixed_axis {
            Some(r.read_vec3_array()?)
        } else {
            None
        };
        let local_axis = if flags.local_axis {
            Some(PmxParsedLocalAxis {
                x: r.read_vec3_array()?,
                z: r.read_vec3_array()?,
            })
        } else {
            None
        };
        let external_parent_key = if flags.external_parent_transform {
            Some(r.read_i32_le()?)
        } else {
            None
        };
        let ik = if flags.ik {
            let target_index = r.read_sized_index(header.bone_index_size)?;
            let loop_count = r.read_i32_le()?;
            let limit_angle = r.read_f32_le()?;
            let link_count =
                read_section_count_with_min_record(r, header.bone_index_size as usize + 1)?;
            let mut links = Vec::with_capacity(link_count);
            for _ in 0..link_count {
                let bone_index = r.read_sized_index(header.bone_index_size)?;
                let has_limit = r.read_u8()? != 0;
                links.push(PmxParsedIkLink {
                    bone_index,
                    limits: if has_limit {
                        Some(PmxParsedIkLimit {
                            lower: r.read_vec3_array()?,
                            upper: r.read_vec3_array()?,
                        })
                    } else {
                        None
                    },
                });
            }
            Some(PmxParsedIk {
                target_index,
                loop_count,
                limit_angle,
                links,
            })
        } else {
            None
        };
        bones.push(PmxParsedBone {
            name,
            english_name,
            parent_index,
            layer,
            position,
            tail_index,
            tail_position,
            flags,
            append_transform,
            fixed_axis,
            local_axis,
            external_parent_key,
            ik,
        });
    }
    Ok(bones)
}

fn parsed_bone_flags(bits: u16) -> PmxParsedBoneFlags {
    PmxParsedBoneFlags {
        indexed_tail: bits & BONE_FLAG_TAIL_INDEX != 0,
        rotatable: bits & 0x0002 != 0,
        translatable: bits & 0x0004 != 0,
        visible: bits & 0x0008 != 0,
        enabled: bits & 0x0010 != 0,
        ik: bits & BONE_FLAG_IK != 0,
        append_local: bits & BONE_FLAG_LOCAL_APPEND != 0,
        append_rotate: bits & BONE_FLAG_APPEND_ROTATE != 0,
        append_translate: bits & BONE_FLAG_APPEND_TRANSLATE != 0,
        fixed_axis: bits & BONE_FLAG_FIXED_AXIS != 0,
        local_axis: bits & BONE_FLAG_LOCAL_AXIS != 0,
        transform_after_physics: bits & 0x1000 != 0,
        external_parent_transform: bits & BONE_FLAG_EXTERNAL_PARENT != 0,
    }
}

fn read_parsed_morphs(
    r: &mut Reader<'_>,
    header: &PmxHeader,
) -> Result<Vec<PmxParsedMorph>, ImportError> {
    let count = read_section_count_with_min_record(r, 4 + 4 + 1 + 1 + 4)?;
    let mut morphs = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.read_string(header.encoding)?;
        let english_name = r.read_string(header.encoding)?;
        let _panel = r.read_u8()?;
        let morph_type = r.read_u8()?;
        let min_offset_size = match morph_type {
            0 => header.morph_index_size as usize + 4,
            1 => header.vertex_index_size as usize + 12,
            2 => header.bone_index_size as usize + 12 + 16,
            3..=7 => header.vertex_index_size as usize + 16,
            8 => header.material_index_size as usize + 1 + 16 + 12 + 4 + 12 + 16 + 4 + 16 + 16 + 16,
            9 => header.morph_index_size as usize + 4,
            10 => header.rigidbody_index_size as usize + 1 + 12 + 12,
            _ => return Err(ImportError::SectionOverflow),
        };
        let offset_count = read_section_count_with_min_record(r, min_offset_size)?;
        let mut morph = PmxParsedMorph {
            name,
            english_name,
            kind: match morph_type {
                0 => "group",
                1 => "vertex",
                2 => "bone",
                3 => "uv",
                4..=7 => "additionalUv",
                8 => "material",
                9 => "flip",
                10 => "impulse",
                _ => "unknown",
            }
            .to_owned(),
            vertex_offsets: Vec::new(),
            group_offsets: Vec::new(),
            bone_offsets: Vec::new(),
            uv_offsets: Vec::new(),
            additional_uv_offsets: Vec::new(),
            material_offsets: Vec::new(),
            flip_offsets: Vec::new(),
            impulse_offsets: Vec::new(),
        };
        for _ in 0..offset_count {
            match morph_type {
                0 => morph.group_offsets.push(PmxParsedGroupMorphOffset {
                    morph_index: r.read_sized_index(header.morph_index_size)?,
                    weight: r.read_f32_le()?,
                }),
                1 => morph.vertex_offsets.push(PmxParsedVertexMorphOffset {
                    vertex_index: r.read_vertex_index(header.vertex_index_size)?,
                    position: r.read_vec3_array()?,
                }),
                2 => morph.bone_offsets.push(PmxParsedBoneMorphOffset {
                    bone_index: r.read_sized_index(header.bone_index_size)?,
                    translation: r.read_vec3_array()?,
                    rotation: r.read_vec4_array()?,
                }),
                3 => morph.uv_offsets.push(PmxParsedUvMorphOffset {
                    vertex_index: r.read_vertex_index(header.vertex_index_size)?,
                    uv: r.read_vec4_array()?,
                }),
                4..=7 => morph
                    .additional_uv_offsets
                    .push(PmxParsedAdditionalUvMorphOffset {
                        vertex_index: r.read_vertex_index(header.vertex_index_size)?,
                        uv_index: morph_type - 4,
                        uv: r.read_vec4_array()?,
                    }),
                8 => morph.material_offsets.push(PmxParsedMaterialMorphOffset {
                    material_index: r.read_sized_index(header.material_index_size)?,
                    operation: if r.read_u8()? == 0 {
                        "multiply".to_owned()
                    } else {
                        "add".to_owned()
                    },
                    diffuse: r.read_vec4_array()?,
                    specular: r.read_vec3_array()?,
                    specular_power: r.read_f32_le()?,
                    ambient: r.read_vec3_array()?,
                    edge_color: r.read_vec4_array()?,
                    edge_size: r.read_f32_le()?,
                    texture_factor: r.read_vec4_array()?,
                    sphere_texture_factor: r.read_vec4_array()?,
                    toon_texture_factor: r.read_vec4_array()?,
                }),
                9 => morph.flip_offsets.push(PmxParsedGroupMorphOffset {
                    morph_index: r.read_sized_index(header.morph_index_size)?,
                    weight: r.read_f32_le()?,
                }),
                10 => morph.impulse_offsets.push(PmxParsedImpulseMorphOffset {
                    rigid_body_index: r.read_sized_index(header.rigidbody_index_size)?,
                    local: r.read_u8()? != 0,
                    velocity: r.read_vec3_array()?,
                    torque: r.read_vec3_array()?,
                }),
                _ => return Err(ImportError::SectionOverflow),
            }
        }
        morphs.push(morph);
    }
    Ok(morphs)
}

fn read_parsed_display_frames(
    r: &mut Reader<'_>,
    header: &PmxHeader,
) -> Result<Vec<PmxParsedDisplayFrame>, ImportError> {
    let count = read_section_count_with_min_record(r, 4 + 4 + 1 + 4)?;
    let mut frames = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.read_string(header.encoding)?;
        let english_name = r.read_string(header.encoding)?;
        let special = r.read_u8()? == 1;
        let item_count = read_section_count_with_min_record(r, 1)?;
        let mut items = Vec::with_capacity(item_count);
        for _ in 0..item_count {
            let kind = r.read_u8()?;
            let index = match kind {
                0 => r.read_sized_index(header.bone_index_size)?,
                1 => r.read_sized_index(header.morph_index_size)?,
                _ => return Err(ImportError::SectionOverflow),
            };
            items.push(PmxParsedDisplayFrameElement {
                kind: if kind == 0 {
                    "bone".to_owned()
                } else {
                    "morph".to_owned()
                },
                index,
            });
        }
        frames.push(PmxParsedDisplayFrame {
            name,
            english_name,
            special,
            frames: items,
        });
    }
    Ok(frames)
}

fn read_parsed_rigid_bodies(
    r: &mut Reader<'_>,
    header: &PmxHeader,
) -> Result<Vec<PmxParsedRigidBody>, ImportError> {
    let count = read_section_count_with_min_record(
        r,
        4 + 4 + header.bone_index_size as usize + 1 + 2 + 1 + 12 + 12 + 12 + 4 + 4 + 4 + 4 + 4 + 1,
    )?;
    let mut bodies = Vec::with_capacity(count);
    for _ in 0..count {
        bodies.push(PmxParsedRigidBody {
            name: r.read_string(header.encoding)?,
            english_name: r.read_string(header.encoding)?,
            bone_index: r.read_sized_index(header.bone_index_size)?,
            group: r.read_u8()?,
            mask: r.read_u16_le()?,
            shape: match r.read_u8()? {
                0 => "sphere",
                1 => "box",
                2 => "capsule",
                _ => "unknown",
            }
            .to_owned(),
            size: r.read_vec3_array()?,
            position: r.read_vec3_array()?,
            rotation: r.read_vec3_array()?,
            mass: r.read_f32_le()?,
            linear_damping: r.read_f32_le()?,
            angular_damping: r.read_f32_le()?,
            restitution: r.read_f32_le()?,
            friction: r.read_f32_le()?,
            mode: match r.read_u8()? {
                0 => "static",
                1 => "dynamic",
                2 => "dynamicBone",
                _ => "unknown",
            }
            .to_owned(),
        });
    }
    Ok(bodies)
}

fn read_parsed_joints(
    r: &mut Reader<'_>,
    header: &PmxHeader,
) -> Result<Vec<PmxParsedJoint>, ImportError> {
    let count = read_section_count_with_min_record(
        r,
        4 + 4 + 1 + header.rigidbody_index_size as usize * 2 + 12 * 8,
    )?;
    let mut joints = Vec::with_capacity(count);
    for _ in 0..count {
        joints.push(PmxParsedJoint {
            name: r.read_string(header.encoding)?,
            english_name: r.read_string(header.encoding)?,
            kind: match r.read_u8()? {
                0 => "generic6dofSpring",
                1 => "generic6dof",
                2 => "point2point",
                3 => "coneTwist",
                4 => "slider",
                5 => "hinge",
                _ => "unknown",
            }
            .to_owned(),
            rigid_body_index_a: r.read_sized_index(header.rigidbody_index_size)?,
            rigid_body_index_b: r.read_sized_index(header.rigidbody_index_size)?,
            position: r.read_vec3_array()?,
            rotation: r.read_vec3_array()?,
            translation_lower_limit: r.read_vec3_array()?,
            translation_upper_limit: r.read_vec3_array()?,
            rotation_lower_limit: r.read_vec3_array()?,
            rotation_upper_limit: r.read_vec3_array()?,
            spring_translation_factor: r.read_vec3_array()?,
            spring_rotation_factor: r.read_vec3_array()?,
        });
    }
    Ok(joints)
}

fn read_parsed_soft_bodies(
    r: &mut Reader<'_>,
    header: &PmxHeader,
) -> Result<Vec<PmxParsedSoftBody>, ImportError> {
    let count = read_section_count_with_min_record(
        r,
        4 + 4
            + 1
            + header.material_index_size as usize
            + 1
            + 2
            + 1
            + 4
            + 4
            + 4
            + 4
            + 4
            + 12 * 4
            + 6 * 4
            + 4 * 4
            + 3 * 4
            + 4
            + 4,
    )?;
    let mut soft_bodies = Vec::with_capacity(count);
    for _ in 0..count {
        let name = r.read_string(header.encoding)?;
        let english_name = r.read_string(header.encoding)?;
        let kind = match r.read_u8()? {
            0 => "triMesh",
            1 => "rope",
            _ => "unknown",
        }
        .to_owned();
        let material_index = r.read_sized_index(header.material_index_size)?;
        let collision_group = r.read_u8()?;
        let collision_mask = r.read_u16_le()?;
        let flags = r.read_u8()?;
        let bending_constraints_distance = r.read_i32_le()?;
        let cluster_count = r.read_i32_le()?;
        let total_mass = r.read_f32_le()?;
        let collision_margin = r.read_f32_le()?;
        r.skip(4 + 12 * 4 + 6 * 4 + 4 * 4 + 3 * 4)?;
        let anchor_count = read_section_count_with_min_record(
            r,
            header.rigidbody_index_size as usize + header.vertex_index_size as usize + 1,
        )?;
        for _ in 0..anchor_count {
            r.read_sized_index(header.rigidbody_index_size)?;
            r.read_vertex_index(header.vertex_index_size)?;
            r.skip(1)?;
        }
        let pin_count = read_section_count_with_min_record(r, header.vertex_index_size as usize)?;
        for _ in 0..pin_count {
            r.read_vertex_index(header.vertex_index_size)?;
        }
        soft_bodies.push(PmxParsedSoftBody {
            name,
            english_name,
            kind,
            material_index,
            collision_group,
            collision_mask,
            flags,
            bending_constraints_distance,
            cluster_count,
            total_mass,
            collision_margin,
        });
    }
    Ok(soft_bodies)
}

#[derive(Debug)]
pub struct PmxRuntimeImport {
    pub model: ModelArena,
    pub bone_names: Vec<String>,
    pub bone_name_to_index: HashMap<Vec<u8>, BoneIndex>,
    pub morph_name_to_index: HashMap<Vec<u8>, MorphIndex>,
    pub ik_solver_bone_name_to_index: HashMap<Vec<u8>, usize>,
}

pub fn import_pmx_runtime(data: &[u8]) -> Result<PmxRuntimeImport, ImportError> {
    let (header, pos) = read_header(data)?;
    let pos = skip_model_info(data, &header, pos)?;
    let pos = skip_vertices(data, &header, pos)?;
    let pos = skip_faces(data, header.vertex_index_size, pos)?;
    let pos = skip_textures(data, header.encoding, pos)?;
    let pos = skip_materials(data, &header, pos)?;
    let (bone_import, pos) = read_bones(data, &header, pos)?;
    let (morph_names, morph_init, _pos) = read_morph_offsets(data, &header, pos)?;

    let mut bone_name_to_index = HashMap::with_capacity(bone_import.bone_name_bytes.len() * 2);
    for (i, bytes) in bone_import.bone_name_bytes.iter().enumerate() {
        let index = BoneIndex(i as u32);
        bone_name_to_index.insert(bytes.clone(), index);
        let decoded = bone_import.bone_names[i].as_bytes();
        if decoded != bytes.as_slice() && !decoded.is_empty() {
            bone_name_to_index.insert(decoded.to_vec(), index);
        }
    }

    let mut morph_name_to_index = HashMap::with_capacity(morph_names.name_bytes.len() * 2);
    for (i, bytes) in morph_names.name_bytes.iter().enumerate() {
        let index = MorphIndex(i as u32);
        morph_name_to_index.insert(bytes.clone(), index);
        let decoded = morph_names.names[i].as_bytes();
        if decoded != bytes.as_slice() && !decoded.is_empty() {
            morph_name_to_index.insert(decoded.to_vec(), index);
        }
    }

    let mut ik_solver_bone_name_to_index = HashMap::with_capacity(bone_import.ik_solvers.len() * 2);
    for (solver_idx, solver) in bone_import.ik_solvers.iter().enumerate() {
        let bone_idx = solver.ik_bone.as_usize();
        if bone_idx < bone_import.bone_name_bytes.len() {
            let bytes = &bone_import.bone_name_bytes[bone_idx];
            ik_solver_bone_name_to_index.insert(bytes.clone(), solver_idx);
            let decoded = bone_import.bone_names[bone_idx].as_bytes();
            if decoded != bytes.as_slice() && !decoded.is_empty() {
                ik_solver_bone_name_to_index.insert(decoded.to_vec(), solver_idx);
            }
        }
    }

    let model = ModelArena::new_with_morphs(
        bone_import.bones,
        bone_import.ik_solvers,
        bone_import.append_transforms,
        morph_init,
    )
    .map_err(ImportError::ModelBuildFailed)?;

    Ok(PmxRuntimeImport {
        model,
        bone_names: bone_import.bone_names,
        bone_name_to_index,
        morph_name_to_index,
        ik_solver_bone_name_to_index,
    })
}

trait AppendTransformInitExt {
    fn with_rotation_if(self, on: bool) -> Self;
    fn with_translation_if(self, on: bool) -> Self;
    fn with_local_if(self, on: bool) -> Self;
}

impl AppendTransformInitExt for AppendTransformInit {
    fn with_rotation_if(mut self, on: bool) -> Self {
        if on {
            self.affect_rotation = true;
        }
        self
    }

    fn with_translation_if(mut self, on: bool) -> Self {
        if on {
            self.affect_translation = true;
        }
        self
    }

    fn with_local_if(mut self, on: bool) -> Self {
        if on {
            self.local = true;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use mmd_anim_runtime::RuntimeInstance;
    use std::sync::Arc;

    fn build_small_pmx_header_bytes(bone_index_size: u8, encoding: TextEncoding) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"PMX ");
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.push(8);
        buf.push(encoding as u8);
        buf.push(0);
        buf.push(4);
        buf.push(1);
        buf.push(1);
        buf.push(bone_index_size);
        buf.push(1);
        buf.push(1);
        buf
    }

    fn build_empty_model_info(_encoding: TextEncoding) -> Vec<u8> {
        let mut buf = Vec::new();
        for _ in 0..4 {
            buf.extend_from_slice(&0i32.to_le_bytes());
        }
        buf
    }

    fn build_empty_vertex_section() -> Vec<u8> {
        vec![0u8, 0, 0, 0]
    }

    fn build_empty_face_section() -> Vec<u8> {
        vec![0u8, 0, 0, 0]
    }

    fn build_empty_texture_section() -> Vec<u8> {
        vec![0u8, 0, 0, 0]
    }

    fn build_empty_material_section() -> Vec<u8> {
        vec![0u8, 0, 0, 0]
    }

    #[test]
    fn rejects_impossible_pmx_vertex_count_before_allocation() {
        let mut buf = build_small_pmx_header_bytes(1, TextEncoding::Utf8);
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&i32::MAX.to_le_bytes());

        assert!(matches!(
            parse_pmx_model(&buf),
            Err(ImportError::UnexpectedEof(_))
        ));
        assert!(matches!(
            import_pmx_runtime(&buf),
            Err(ImportError::UnexpectedEof(_))
        ));
    }

    #[test]
    fn rejects_invalid_pmx_index_sizes_in_header() {
        let buf = build_small_pmx_header_bytes(3, TextEncoding::Utf8);

        assert!(matches!(
            read_header(&buf),
            Err(ImportError::InvalidIndexSize(3))
        ));
    }

    fn build_bone_name_bytes(name: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as i32).to_le_bytes());
        buf.extend_from_slice(name_bytes);
        buf
    }

    fn build_bone_section_header(count: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&count.to_le_bytes());
        buf
    }

    fn build_morph_name_bytes(name: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        let name_bytes = name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as i32).to_le_bytes());
        buf.extend_from_slice(name_bytes);
        buf
    }

    fn empty_pmx_flags() -> PmxParsedMaterialFlags {
        PmxParsedMaterialFlags {
            double_sided: false,
            ground_shadow: false,
            self_shadow_map: false,
            self_shadow: false,
            edge: false,
            vertex_color: false,
            point_draw: false,
            line_draw: false,
        }
    }

    fn basic_bone_flags(indexed_tail: bool) -> PmxParsedBoneFlags {
        PmxParsedBoneFlags {
            indexed_tail,
            rotatable: true,
            translatable: false,
            visible: true,
            enabled: true,
            ik: false,
            append_local: false,
            append_rotate: false,
            append_translate: false,
            fixed_axis: false,
            local_axis: false,
            transform_after_physics: false,
            external_parent_transform: false,
        }
    }

    fn parsed_pmx_fixture() -> PmxParsedModel {
        PmxParsedModel {
            metadata: PmxParsedMetadata {
                format: "pmx".to_owned(),
                version: 2.0,
                encoding: "utf-8".to_owned(),
                name: "model".to_owned(),
                english_name: "model-en".to_owned(),
                comment: "comment".to_owned(),
                english_comment: "comment-en".to_owned(),
                counts: PmxParsedCounts {
                    vertices: 1,
                    faces: 1,
                    materials: 1,
                    bones: 1,
                    morphs: 1,
                    display_frames: 1,
                    rigid_bodies: 1,
                    joints: 1,
                    soft_bodies: 0,
                },
                index_sizes: PmxParsedIndexSizes {
                    vertex: 4,
                    texture: 1,
                    material: 1,
                    bone: 2,
                    morph: 1,
                    rigid_body: 1,
                },
                additional_uv_count: 1,
            },
            geometry: PmxParsedGeometry {
                positions: vec![1.0, 2.0, 3.0],
                normals: vec![0.0, 1.0, 0.0],
                uvs: vec![0.25, 0.75],
                additional_uvs: vec![vec![1.0, 2.0, 3.0, 4.0]],
                indices: vec![0, 0, 0],
                skin_indices: vec![0, 0, 0, 0],
                skin_weights: vec![1.0, 0.0, 0.0, 0.0],
                edge_scale: vec![1.25],
                material_groups: vec![PmxParsedMaterialGroup {
                    start: 0,
                    count: 3,
                    material_index: 0,
                }],
            },
            materials: vec![PmxParsedMaterial {
                name: "mat".to_owned(),
                english_name: "mat-en".to_owned(),
                texture_path: "tex.png".to_owned(),
                sphere_texture_path: "sphere.spa".to_owned(),
                sphere_mode: "add".to_owned(),
                toon_texture_path: "toon.bmp".to_owned(),
                shared_toon_index: None,
                diffuse: [0.1, 0.2, 0.3, 0.4],
                specular: [0.5, 0.6, 0.7],
                specular_power: 8.0,
                ambient: [0.8, 0.9, 1.0],
                edge_color: [0.0, 0.1, 0.2, 0.3],
                edge_size: 1.5,
                flags: PmxParsedMaterialFlags {
                    double_sided: true,
                    edge: true,
                    ..empty_pmx_flags()
                },
                face_count: 1,
            }],
            skeleton: PmxParsedSkeleton {
                bones: vec![PmxParsedBone {
                    name: "Root".to_owned(),
                    english_name: "RootEn".to_owned(),
                    parent_index: -1,
                    layer: 0,
                    position: [0.0, 1.0, 2.0],
                    tail_index: -1,
                    tail_position: Some([0.0, 2.0, 0.0]),
                    flags: basic_bone_flags(false),
                    append_transform: None,
                    fixed_axis: None,
                    local_axis: None,
                    external_parent_key: None,
                    ik: None,
                }],
            },
            morphs: vec![PmxParsedMorph {
                name: "Smile".to_owned(),
                english_name: "SmileEn".to_owned(),
                kind: "vertex".to_owned(),
                vertex_offsets: vec![PmxParsedVertexMorphOffset {
                    vertex_index: 0,
                    position: [0.1, 0.2, 0.3],
                }],
                group_offsets: Vec::new(),
                bone_offsets: Vec::new(),
                uv_offsets: Vec::new(),
                additional_uv_offsets: Vec::new(),
                material_offsets: Vec::new(),
                flip_offsets: Vec::new(),
                impulse_offsets: Vec::new(),
            }],
            display_frames: vec![PmxParsedDisplayFrame {
                name: "RootFrame".to_owned(),
                english_name: "RootFrameEn".to_owned(),
                special: true,
                frames: vec![
                    PmxParsedDisplayFrameElement {
                        kind: "bone".to_owned(),
                        index: 0,
                    },
                    PmxParsedDisplayFrameElement {
                        kind: "morph".to_owned(),
                        index: 0,
                    },
                ],
            }],
            rigid_bodies: vec![PmxParsedRigidBody {
                name: "body".to_owned(),
                english_name: "body-en".to_owned(),
                bone_index: 0,
                group: 1,
                mask: 2,
                shape: "box".to_owned(),
                size: [1.0, 2.0, 3.0],
                position: [4.0, 5.0, 6.0],
                rotation: [0.1, 0.2, 0.3],
                mass: 10.0,
                linear_damping: 0.4,
                angular_damping: 0.5,
                restitution: 0.6,
                friction: 0.7,
                mode: "dynamicBone".to_owned(),
            }],
            joints: vec![PmxParsedJoint {
                name: "joint".to_owned(),
                english_name: "joint-en".to_owned(),
                kind: "generic6dofSpring".to_owned(),
                rigid_body_index_a: 0,
                rigid_body_index_b: -1,
                position: [1.0, 1.1, 1.2],
                rotation: [2.0, 2.1, 2.2],
                translation_lower_limit: [-1.0, -1.1, -1.2],
                translation_upper_limit: [1.0, 1.1, 1.2],
                rotation_lower_limit: [-0.1, -0.2, -0.3],
                rotation_upper_limit: [0.1, 0.2, 0.3],
                spring_translation_factor: [3.0, 3.1, 3.2],
                spring_rotation_factor: [4.0, 4.1, 4.2],
            }],
            soft_bodies: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn assert_pmx_roundtrip_eq(left: &PmxParsedModel, right: &PmxParsedModel) {
        assert_eq!(
            serde_json::to_value(left).unwrap(),
            serde_json::to_value(right).unwrap()
        );
    }

    fn json_keys(value: &serde_json::Value) -> Vec<String> {
        let mut keys = value
            .as_object()
            .unwrap()
            .keys()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        keys.sort();
        keys
    }

    #[test]
    fn pmx_model_json_top_level_schema_is_stable() {
        let parsed = parsed_pmx_fixture();
        let keys = json_keys(&serde_json::to_value(&parsed).unwrap());

        assert_eq!(
            keys,
            vec![
                "diagnostics",
                "displayFrames",
                "geometry",
                "joints",
                "materials",
                "metadata",
                "morphs",
                "rigidBodies",
                "skeleton",
                "softBodies",
            ]
        );
    }

    #[test]
    fn parses_pmx_header() {
        let header_bytes = build_small_pmx_header_bytes(4, TextEncoding::Utf8);
        let (header, pos) = read_header(&header_bytes).unwrap();

        assert_eq!(header.version, 2.0);
        assert_eq!(header.encoding, TextEncoding::Utf8);
        assert_eq!(header.bone_index_size, 4);
        assert_eq!(header.vertex_index_size, 4);
        assert!(pos > 0);
    }

    #[test]
    fn pmx_bone_flag_values_match_reference_layout() {
        assert_eq!(BONE_FLAG_TAIL_INDEX, 0x0001);
        assert_eq!(BONE_FLAG_IK, 0x0020);
        assert_eq!(BONE_FLAG_LOCAL_APPEND, 0x0080);
        assert_eq!(BONE_FLAG_APPEND_ROTATE, 0x0100);
        assert_eq!(BONE_FLAG_APPEND_TRANSLATE, 0x0200);
        assert_eq!(BONE_FLAG_FIXED_AXIS, 0x0400);
        assert_eq!(BONE_FLAG_LOCAL_AXIS, 0x0800);
        assert_eq!(BONE_FLAG_EXTERNAL_PARENT, 0x2000);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut header_bytes = build_small_pmx_header_bytes(4, TextEncoding::Utf8);
        header_bytes[0] = 0xFF;
        assert_eq!(
            read_header(&header_bytes).unwrap_err(),
            ImportError::InvalidPmxMagic
        );
    }

    #[test]
    fn reads_single_bone_no_parent() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(4, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(1));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i32).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bones.len(), 1);
        assert_eq!(result.bone_names.len(), 1);
        assert_eq!(result.bone_names[0], "Root");
        assert_eq!(&result.bone_name_bytes[0][..], b"Root");
        assert_eq!(result.bones[0].parent, None);
        assert!((result.bones[0].rest_position.x - 0.0).abs() < 0.001);
        assert!((result.bones[0].rest_position.y - 1.0).abs() < 0.001);
        assert!((result.bones[0].rest_position.z - 2.0).abs() < 0.001);
        assert!(result.ik_solvers.is_empty());
        assert!(result.append_transforms.is_empty());
    }

    #[test]
    fn reads_bone_with_parent() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Child"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bones.len(), 2);
        assert_eq!(result.bone_names[0], "Root");
        assert_eq!(result.bone_names[1], "Child");
        assert_eq!(result.bones[0].parent, None);
        assert_eq!(result.bones[1].parent, Some(BoneIndex(0)));
    }

    #[test]
    fn reads_bone_with_append_transform() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("AppendBone"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(
            &(BONE_FLAG_LOCAL_APPEND | BONE_FLAG_APPEND_ROTATE | BONE_FLAG_APPEND_TRANSLATE)
                .to_le_bytes(),
        );
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0.5f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bones.len(), 2);
        assert_eq!(result.append_transforms.len(), 1);
        assert_eq!(result.append_transforms[0].target_bone, BoneIndex(1));
        assert_eq!(result.append_transforms[0].source_bone, BoneIndex(0));
        assert!((result.append_transforms[0].ratio - 0.5).abs() < 0.001);
        assert!(result.append_transforms[0].affect_rotation);
        assert!(result.append_transforms[0].affect_translation);
        assert!(result.append_transforms[0].local);
    }

    #[test]
    fn reads_ik_bone_with_links_and_angle_limits() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(3));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("IKLink"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("IKBone"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&4.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&(BONE_FLAG_TAIL_INDEX | BONE_FLAG_IK).to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&10i32.to_le_bytes());
        buf.extend_from_slice(&0.5f32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.push(1u8);
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bones.len(), 3);
        assert_eq!(result.ik_solvers.len(), 1);
        assert_eq!(result.ik_solvers[0].ik_bone, BoneIndex(2));
        assert_eq!(result.ik_solvers[0].target_bone, BoneIndex(1));
        assert_eq!(result.ik_solvers[0].links.len(), 1);
        assert_eq!(result.ik_solvers[0].links[0].bone, BoneIndex(1));
        assert!(result.ik_solvers[0].links[0].angle_limit.is_some());
        assert_eq!(result.ik_solvers[0].iteration_count, 10);
        assert!((result.ik_solvers[0].limit_angle - 0.5).abs() < 0.001);
    }

    #[test]
    fn handles_utf16le_encoding() {
        let header_bytes = build_small_pmx_header_bytes(4, TextEncoding::Utf16Le);
        let (header, _) = read_header(&header_bytes).unwrap();
        assert_eq!(header.encoding, TextEncoding::Utf16Le);
    }

    #[test]
    fn decodes_utf16le_surrogate_pairs() {
        let bytes = [0x3D, 0xD8, 0x00, 0xDE, 0x00, 0x00];
        assert_eq!(decode_utf16le_lossy(&bytes), "\u{1F600}");
    }

    #[test]
    fn reports_eof_on_truncated_data() {
        let buf = vec![0x50, 0x4D, 0x58]; // incomplete magic
        assert!(matches!(
            read_header(&buf).unwrap_err(),
            ImportError::UnexpectedEof(_)
        ));
    }

    #[test]
    fn bone_names_retained_and_mapped_to_indices() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Hips"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Spine"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bone_name_bytes.len(), 2);
        assert_eq!(&result.bone_name_bytes[0][..], b"Hips");
        assert_eq!(&result.bone_name_bytes[1][..], b"Spine");
        assert_eq!(result.bone_names[0], "Hips");
        assert_eq!(result.bone_names[1], "Spine");

        let bone_map: HashMap<Vec<u8>, BoneIndex> = result
            .bone_name_bytes
            .iter()
            .enumerate()
            .map(|(i, b)| (b.clone(), BoneIndex(i as u32)))
            .collect();

        assert_eq!(bone_map.get(b"Hips".as_slice()), Some(&BoneIndex(0)));
        assert_eq!(bone_map.get(b"Spine".as_slice()), Some(&BoneIndex(1)));
        assert_eq!(bone_map.get(b"Unknown".as_slice()), None);
    }

    #[test]
    fn ik_solver_name_mapped_to_solver_index() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(3));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("IKLink"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("LeftLegIK"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&4.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&(BONE_FLAG_TAIL_INDEX | BONE_FLAG_IK).to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&10i32.to_le_bytes());
        buf.extend_from_slice(&0.5f32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.push(1u8);
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());

        let bone_import = import_pmx_model(&buf).unwrap();
        assert_eq!(bone_import.ik_solvers.len(), 1);
        assert_eq!(bone_import.ik_solvers[0].ik_bone, BoneIndex(2));
        assert_eq!(
            bone_import.bone_names[bone_import.ik_solvers[0].ik_bone.as_usize()],
            "LeftLegIK"
        );

        let ik_name_map: HashMap<Vec<u8>, usize> = bone_import
            .ik_solvers
            .iter()
            .enumerate()
            .filter_map(|(solver_idx, solver)| {
                let bone_idx = solver.ik_bone.as_usize();
                if bone_idx < bone_import.bone_name_bytes.len() {
                    Some((bone_import.bone_name_bytes[bone_idx].clone(), solver_idx))
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(ik_name_map.get(b"LeftLegIK".as_slice()), Some(&0));
    }

    #[test]
    fn reads_morph_names_without_payloads() {
        let (header, pos) =
            read_header(&build_small_pmx_header_bytes(4, TextEncoding::Utf8)).unwrap();

        let mut buf = vec![0u8; pos];
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&build_morph_name_bytes("Smile"));
        buf.extend_from_slice(&build_morph_name_bytes(""));
        buf.push(4u8);
        buf.push(0u8);
        buf.extend_from_slice(&0i32.to_le_bytes());

        let (morphs, new_pos) = read_morph_names(&buf[..], &header, pos).unwrap();
        assert_eq!(morphs.name_bytes.len(), 1);
        assert_eq!(&morphs.name_bytes[0][..], b"Smile");
        assert_eq!(morphs.names[0], "Smile");
        assert!(new_pos > pos);
    }

    #[test]
    fn reads_morph_type_zero_with_vertex_group_offsets() {
        let (header, pos) =
            read_header(&build_small_pmx_header_bytes(4, TextEncoding::Utf8)).unwrap();
        assert_eq!(header.vertex_index_size, 4);
        assert_eq!(header.morph_index_size, 1);

        let mut buf = vec![0u8; pos];
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&build_morph_name_bytes("Blink"));
        buf.extend_from_slice(&build_morph_name_bytes(""));
        buf.push(0u8);
        buf.push(0u8);
        buf.extend_from_slice(&2i32.to_le_bytes());
        buf.push(0u8);
        buf.extend_from_slice(&[0u8; 4]);
        buf.push(1u8);
        buf.extend_from_slice(&[0u8; 4]);

        let (morphs, new_pos) = read_morph_names(&buf[..], &header, pos).unwrap();
        assert_eq!(morphs.name_bytes.len(), 1);
        assert_eq!(&morphs.name_bytes[0][..], b"Blink");
        assert_eq!(morphs.names[0], "Blink");
        assert_eq!(new_pos, buf.len());
    }

    #[test]
    fn runtime_import_builds_model_arena_with_name_lookup() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Hips"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Spine"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());

        let rt = import_pmx_runtime(&buf).unwrap();
        assert_eq!(rt.model.bone_count(), 2);
        assert_eq!(rt.model.parent_index(BoneIndex(1)), Some(BoneIndex(0)));
        assert_eq!(
            rt.bone_name_to_index.get(b"Hips".as_slice()),
            Some(&BoneIndex(0))
        );
        assert_eq!(
            rt.bone_name_to_index.get(b"Spine".as_slice()),
            Some(&BoneIndex(1))
        );
        assert!(rt.morph_name_to_index.is_empty());
        assert!(rt.ik_solver_bone_name_to_index.is_empty());
    }

    #[test]
    fn parse_pmx_model_exposes_three_loader_shaped_sections() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Child"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&0i32.to_le_bytes()); // morphs
        buf.extend_from_slice(&1i32.to_le_bytes()); // display frames
        buf.extend_from_slice(&build_bone_name_bytes("RootFrame"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.push(1);
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.push(0);
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes()); // rigid bodies
        buf.extend_from_slice(&0i32.to_le_bytes()); // joints

        let parsed = parse_pmx_model(&buf).unwrap();
        assert_eq!(parsed.metadata.format, "pmx");
        assert_eq!(parsed.metadata.counts.bones, 2);
        assert_eq!(parsed.skeleton.bones[0].name, "Root");
        assert_eq!(parsed.skeleton.bones[1].parent_index, 0);
        assert_eq!(parsed.display_frames.len(), 1);
        assert_eq!(parsed.display_frames[0].frames[0].kind, "bone");
        assert_eq!(parsed.display_frames[0].frames[0].index, 1);
        assert!(parsed.diagnostics.is_empty());
    }

    #[test]
    fn exports_pmx_semantic_roundtrip() {
        let parsed = parsed_pmx_fixture();
        let exported = export_pmx_model(&parsed);
        let reparsed = parse_pmx_model(&exported).unwrap();

        assert_pmx_roundtrip_eq(&parsed, &reparsed);
    }

    #[test]
    fn exports_pmx_json_dto_semantic_roundtrip() {
        let parsed = parsed_pmx_fixture();
        let json = serde_json::to_string(&parsed).unwrap();
        let from_json: PmxParsedModel = serde_json::from_str(&json).unwrap();
        let exported = export_pmx_model(&from_json);
        let reparsed = parse_pmx_model(&exported).unwrap();

        assert_pmx_roundtrip_eq(&parsed, &reparsed);
    }

    #[test]
    fn builds_pmx_parts_with_material_descriptors() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(serde_json::json!({
            "name": "material-parts",
            "materials": [
                {
                    "name": "red",
                    "englishName": "red-en",
                    "diffuse": [1.0, 0.0, 0.0, 1.0],
                    "ambient": [0.5, 0.0, 0.0],
                    "flags": { "edge": true },
                    "faceCount": 1
                },
                {
                    "name": "blue",
                    "diffuse": [0.0, 0.0, 1.0, 1.0],
                    "faceCount": 1
                }
            ],
            "bones": [
                { "name": "root", "englishName": "root-en", "tailIndex": 1 },
                { "name": "child", "parentIndex": 0, "position": [0.0, 1.0, 0.0], "rotatable": true }
            ],
            "morphs": [
                {
                    "name": "raise",
                    "englishName": "raise-en",
                    "kind": "vertex",
                    "vertexOffsets": [{ "vertexIndex": 0, "position": [0.0, 0.1, 0.0] }]
                },
                {
                    "name": "combo",
                    "kind": "group",
                    "groupOffsets": [{ "morphIndex": 0, "weight": 0.5 }]
                }
            ],
            "displayFrames": [
                {
                    "name": "Root",
                    "englishName": "Root-en",
                    "special": true,
                    "frames": [
                        { "kind": "bone", "index": 0 },
                        { "kind": "bone", "index": 1 },
                        { "kind": "morph", "index": 0 }
                    ]
                }
            ],
            "rigidBodies": [
                {
                    "name": "body",
                    "englishName": "body-en",
                    "boneIndex": 1,
                    "group": 2,
                    "mask": 3,
                    "shape": "box",
                    "size": [1.0, 2.0, 3.0],
                    "position": [0.0, 1.0, 0.0],
                    "rotation": [0.1, 0.2, 0.3],
                    "mass": 2.0,
                    "linearDamping": 0.4,
                    "angularDamping": 0.5,
                    "restitution": 0.6,
                    "friction": 0.7,
                    "mode": "dynamicBone"
                }
            ],
            "joints": [
                {
                    "name": "joint",
                    "type": "generic6dofSpring",
                    "rigidBodyIndexA": 0,
                    "rigidBodyIndexB": -1,
                    "position": [0.0, 1.0, 0.0],
                    "translationLowerLimit": [-1.0, -1.0, -1.0],
                    "translationUpperLimit": [1.0, 1.0, 1.0],
                    "springTranslationFactor": [0.1, 0.2, 0.3]
                }
            ],
            "indexSizes": {
                "vertex": 1,
                "material": 1,
                "texture": 1,
                "bone": 1,
                "morph": 1,
                "rigidBody": 1
            }
        }))
        .unwrap();
        let model = build_pmx_model_from_parts(PmxPartsInput {
            descriptor,
            positions_xyz: &[
                0.0, 0.0, 0.0, //
                1.0, 0.0, 0.0, //
                1.0, 1.0, 0.0, //
                0.0, 1.0, 0.0,
            ],
            normals_xyz: &[
                0.0, 0.0, 1.0, //
                0.0, 0.0, 1.0, //
                0.0, 0.0, 1.0, //
                0.0, 0.0, 1.0,
            ],
            uvs_xy: &[0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0],
            indices: &[0, 1, 2, 0, 2, 3],
            skin_indices: &[0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0],
            skin_weights: &[
                0.75, 0.25, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 0.5, 0.5, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
            ],
            edge_scale: &[],
        })
        .unwrap();
        let exported = export_pmx_model(&model);
        let reparsed = parse_pmx_model(&exported).unwrap();

        assert_eq!(reparsed.metadata.name, "material-parts");
        assert_eq!(reparsed.metadata.counts.materials, 2);
        assert_eq!(reparsed.materials[0].name, "red");
        assert_eq!(reparsed.materials[0].english_name, "red-en");
        assert_eq!(reparsed.materials[0].face_count, 1);
        assert!(reparsed.materials[0].flags.edge);
        assert_eq!(reparsed.materials[1].name, "blue");
        assert_eq!(reparsed.materials[1].face_count, 1);
        assert_eq!(reparsed.geometry.material_groups.len(), 2);
        assert_eq!(reparsed.geometry.material_groups[0].start, 0);
        assert_eq!(reparsed.geometry.material_groups[0].count, 3);
        assert_eq!(reparsed.geometry.material_groups[1].start, 3);
        assert_eq!(reparsed.geometry.material_groups[1].count, 3);
        assert_eq!(reparsed.metadata.counts.bones, 2);
        assert_eq!(reparsed.skeleton.bones[0].name, "root");
        assert_eq!(reparsed.skeleton.bones[0].english_name, "root-en");
        assert_eq!(reparsed.skeleton.bones[0].tail_index, 1);
        assert!(reparsed.skeleton.bones[0].flags.indexed_tail);
        assert_eq!(reparsed.skeleton.bones[1].name, "child");
        assert_eq!(reparsed.skeleton.bones[1].parent_index, 0);
        assert!(reparsed.skeleton.bones[1].flags.rotatable);
        assert_eq!(reparsed.geometry.skin_indices[1], 1);
        assert_eq!(reparsed.geometry.skin_weights[1], 0.25);
        assert_eq!(reparsed.metadata.counts.morphs, 2);
        assert_eq!(reparsed.morphs[0].name, "raise");
        assert_eq!(reparsed.morphs[0].english_name, "raise-en");
        assert_eq!(reparsed.morphs[0].kind, "vertex");
        assert_eq!(reparsed.morphs[0].vertex_offsets[0].vertex_index, 0);
        assert_eq!(
            reparsed.morphs[0].vertex_offsets[0].position,
            [0.0, 0.1, 0.0]
        );
        assert_eq!(reparsed.morphs[1].name, "combo");
        assert_eq!(reparsed.morphs[1].kind, "group");
        assert_eq!(reparsed.morphs[1].group_offsets[0].morph_index, 0);
        assert_eq!(reparsed.morphs[1].group_offsets[0].weight, 0.5);
        assert_eq!(reparsed.display_frames.len(), 1);
        assert_eq!(reparsed.display_frames[0].name, "Root");
        assert_eq!(reparsed.display_frames[0].english_name, "Root-en");
        assert!(reparsed.display_frames[0].special);
        assert_eq!(reparsed.display_frames[0].frames.len(), 3);
        assert_eq!(reparsed.display_frames[0].frames[1].kind, "bone");
        assert_eq!(reparsed.display_frames[0].frames[1].index, 1);
        assert_eq!(reparsed.display_frames[0].frames[2].kind, "morph");
        assert_eq!(reparsed.display_frames[0].frames[2].index, 0);
        assert_eq!(reparsed.rigid_bodies.len(), 1);
        assert_eq!(reparsed.rigid_bodies[0].name, "body");
        assert_eq!(reparsed.rigid_bodies[0].english_name, "body-en");
        assert_eq!(reparsed.rigid_bodies[0].bone_index, 1);
        assert_eq!(reparsed.rigid_bodies[0].group, 2);
        assert_eq!(reparsed.rigid_bodies[0].mask, 3);
        assert_eq!(reparsed.rigid_bodies[0].shape, "box");
        assert_eq!(reparsed.rigid_bodies[0].mode, "dynamicBone");
        assert_eq!(reparsed.joints.len(), 1);
        assert_eq!(reparsed.joints[0].name, "joint");
        assert_eq!(reparsed.joints[0].kind, "generic6dofSpring");
        assert_eq!(reparsed.joints[0].rigid_body_index_a, 0);
        assert_eq!(reparsed.joints[0].rigid_body_index_b, -1);
    }

    #[test]
    fn rejects_pmx_parts_material_face_count_mismatch() {
        let descriptor: PmxPartsDescriptor = serde_json::from_value(serde_json::json!({
            "materials": [{ "name": "bad", "faceCount": 2 }]
        }))
        .unwrap();
        let error = build_pmx_model_from_parts(PmxPartsInput {
            descriptor,
            positions_xyz: &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals_xyz: &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            uvs_xy: &[0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            indices: &[0, 1, 2],
            skin_indices: &[],
            skin_weights: &[],
            edge_scale: &[],
        })
        .unwrap_err();

        assert!(error.contains("materials faceCount sum"));
    }

    #[test]
    fn rejects_pmx_export_model_partial_skin_arrays() {
        let mut parsed = parsed_pmx_fixture();
        parsed.geometry.skin_weights.clear();

        let error = validate_pmx_export_model(&parsed).unwrap_err();

        assert!(error.contains("skinIndices and skinWeights"));
    }

    // Synthetic PMX 2.2 gate — no real PMX 2.2 fixture exists in the scan set yet.
    // Verifies that version=2.2 survives export_pmx_model -> parse_pmx_model unchanged.
    #[test]
    fn exports_pmx_22_synthetic_roundtrip() {
        let mut parsed = parsed_pmx_fixture();
        parsed.metadata.version = 2.2;

        let exported = export_pmx_model(&parsed);
        let reparsed = parse_pmx_model(&exported).unwrap();

        assert_eq!(
            reparsed.metadata.version, 2.2,
            "PMX 2.2 version must survive the export-parse cycle"
        );
        assert_pmx_roundtrip_eq(&parsed, &reparsed);
    }

    #[test]
    fn exports_pmx_soft_body_header_fields_with_diagnostic_for_pmx_21_and_22() {
        for version in [2.1, 2.2] {
            let mut parsed = parsed_pmx_fixture();
            parsed.metadata.version = version;
            parsed.soft_bodies = vec![PmxParsedSoftBody {
                name: "soft".to_owned(),
                english_name: "soft-en".to_owned(),
                kind: "rope".to_owned(),
                material_index: 0,
                collision_group: 1,
                collision_mask: 2,
                flags: 3,
                bending_constraints_distance: 4,
                cluster_count: 5,
                total_mass: 6.0,
                collision_margin: 0.7,
            }];
            let exported = export_pmx_model(&parsed);
            let reparsed = parse_pmx_model(&exported).unwrap();

            assert_eq!(reparsed.metadata.version, version);
            assert_eq!(reparsed.metadata.counts.soft_bodies, 1);
            assert_eq!(reparsed.soft_bodies.len(), 1);
            assert_eq!(reparsed.soft_bodies[0].name, "soft");
            assert_eq!(reparsed.soft_bodies[0].english_name, "soft-en");
            assert_eq!(reparsed.soft_bodies[0].kind, "rope");
            assert_eq!(reparsed.soft_bodies[0].material_index, 0);
            assert_eq!(reparsed.soft_bodies[0].collision_group, 1);
            assert_eq!(reparsed.soft_bodies[0].collision_mask, 2);
            assert_eq!(reparsed.soft_bodies[0].flags, 3);
            assert_eq!(reparsed.soft_bodies[0].bending_constraints_distance, 4);
            assert_eq!(reparsed.soft_bodies[0].cluster_count, 5);
            assert_eq!(reparsed.soft_bodies[0].total_mass, 6.0);
            assert_eq!(reparsed.soft_bodies[0].collision_margin, 0.7);
            assert_eq!(reparsed.diagnostics.len(), 1);
            assert_eq!(reparsed.diagnostics[0].code, "PMX_SOFT_BODY_UNSUPPORTED");
        }
    }

    #[test]
    fn exports_parsed_pmx_skeleton_display_roundtrip() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Child"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&0i32.to_le_bytes()); // morphs
        buf.extend_from_slice(&1i32.to_le_bytes()); // display frames
        buf.extend_from_slice(&build_bone_name_bytes("RootFrame"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.push(1);
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.push(0);
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes()); // rigid bodies
        buf.extend_from_slice(&0i32.to_le_bytes()); // joints

        let parsed = parse_pmx_model(&buf).unwrap();
        let exported = export_pmx_model(&parsed);
        let reparsed = parse_pmx_model(&exported).unwrap();

        assert_pmx_roundtrip_eq(&parsed, &reparsed);
    }

    #[test]
    fn runtime_import_ik_solver_name_lookup() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(3));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("IKLink"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("LeftLegIK"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&4.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&(BONE_FLAG_TAIL_INDEX | BONE_FLAG_IK).to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.extend_from_slice(&10i32.to_le_bytes());
        buf.extend_from_slice(&0.5f32.to_le_bytes());
        buf.extend_from_slice(&1i32.to_le_bytes());
        buf.extend_from_slice(&1i16.to_le_bytes());
        buf.push(1u8);
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&(-1.0f32).to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());

        let rt = import_pmx_runtime(&buf).unwrap();
        assert_eq!(rt.model.bone_count(), 3);
        assert_eq!(rt.model.ik_count(), 1);
        assert_eq!(
            rt.ik_solver_bone_name_to_index.get(b"LeftLegIK".as_slice()),
            Some(&0)
        );
        assert_eq!(
            rt.bone_name_to_index.get(b"LeftLegIK".as_slice()),
            Some(&BoneIndex(2))
        );
        assert_eq!(
            rt.ik_solver_bone_name_to_index.get(b"NoSuchIK".as_slice()),
            None
        );
    }

    #[test]
    fn converts_absolute_pmx_positions_to_local_rest_positions() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Parent"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Child"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&3.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bones.len(), 2);
        assert_eq!(result.bones[0].parent, None);
        assert!((result.bones[0].rest_position.x - 1.0).abs() < 0.001);
        assert!((result.bones[0].rest_position.y - 2.0).abs() < 0.001);
        assert!((result.bones[0].rest_position.z - 0.0).abs() < 0.001);
        assert_eq!(result.bones[1].parent, Some(BoneIndex(0)));
        assert!((result.bones[1].rest_position.x - 0.0).abs() < 0.001);
        assert!((result.bones[1].rest_position.y - 1.0).abs() < 0.001);
        assert!((result.bones[1].rest_position.z - 0.0).abs() < 0.001);
    }

    #[test]
    fn root_inverse_bind_cancels_rest_world() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(1));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&3.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        let ibm = result.bones[0].inverse_bind_matrix;
        let rest_world = Mat4::from_translation(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(rest_world * ibm, Mat4::IDENTITY);
    }

    #[test]
    fn reads_pmx_fixed_axis_bone_descriptor() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(1));

        buf.extend_from_slice(&build_bone_name_bytes("Fixed"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&BONE_FLAG_FIXED_AXIS.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bones[0].fixed_axis, Some(Vec3A::Y));
    }

    #[test]
    fn child_inverse_bind_uses_absolute_not_local_position() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Root"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Child"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&5.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        let result = import_pmx_model(&buf).unwrap();
        assert_eq!(result.bones[1].parent, Some(BoneIndex(0)));
        assert_eq!(result.bones[1].rest_position, Vec3A::new(4.0, 0.0, 0.0));
        let ibm = result.bones[1].inverse_bind_matrix;
        let rest_world = Mat4::from_translation(Vec3::new(5.0, 0.0, 0.0));
        assert_eq!(rest_world * ibm, Mat4::IDENTITY);
    }

    #[test]
    fn runtime_rest_pose_skinning_matrices_are_identity() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&build_small_pmx_header_bytes(2, TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_model_info(TextEncoding::Utf8));
        buf.extend_from_slice(&build_empty_vertex_section());
        buf.extend_from_slice(&build_empty_face_section());
        buf.extend_from_slice(&build_empty_texture_section());
        buf.extend_from_slice(&build_empty_material_section());
        buf.extend_from_slice(&build_bone_section_header(2));

        buf.extend_from_slice(&build_bone_name_bytes("Parent"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&2.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&(-1i16).to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&build_bone_name_bytes("Child"));
        buf.extend_from_slice(&build_bone_name_bytes(""));
        buf.extend_from_slice(&1.0f32.to_le_bytes());
        buf.extend_from_slice(&3.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0i16.to_le_bytes());
        buf.extend_from_slice(&0i32.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());
        buf.extend_from_slice(&0.0f32.to_le_bytes());

        buf.extend_from_slice(&0i32.to_le_bytes());

        let rt = import_pmx_runtime(&buf).unwrap();
        let model = Arc::new(rt.model);
        let mut instance = RuntimeInstance::new(model);
        instance.evaluate_current_pose();
        let matrices = instance.skinning_matrices();
        assert_eq!(matrices.len(), 2);
        assert_eq!(matrices[0], Mat4::IDENTITY);
        assert_eq!(matrices[1], Mat4::IDENTITY);
    }
}
