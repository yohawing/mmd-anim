use std::collections::HashMap;

use encoding_rs::SHIFT_JIS;
use glam::Vec3A;
use mmd_anim_runtime::{
    BoneIndex, BoneInit, IkLinkInit, IkSolverInit, ModelArena, MorphIndex, MorphInit,
    MorphOffsetSpan, VertexMorphOffset,
};
use serde::{Deserialize, Serialize};

use crate::error::ImportError;
use crate::normalize::normalize_vmd_name;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedModel {
    pub metadata: PmdParsedMetadata,
    pub geometry: PmdParsedGeometry,
    pub materials: Vec<PmdParsedMaterial>,
    pub toon_textures: Vec<String>,
    pub toon_texture_bytes: Vec<Vec<u8>>,
    pub skeleton: PmdParsedSkeleton,
    pub morphs: Vec<PmdParsedMorph>,
    pub display_frames: Vec<PmdParsedDisplayFrame>,
    pub rigid_bodies: Vec<PmdParsedRigidBody>,
    pub joints: Vec<PmdParsedJoint>,
    pub diagnostics: Vec<PmdParserDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedMetadata {
    pub format: String,
    pub version: f32,
    pub encoding: String,
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub english_name: String,
    pub english_name_bytes: Vec<u8>,
    pub comment: String,
    pub comment_bytes: Vec<u8>,
    pub english_comment: String,
    pub english_comment_bytes: Vec<u8>,
    pub counts: PmdParsedCounts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedCounts {
    pub vertices: usize,
    pub faces: usize,
    pub materials: usize,
    pub bones: usize,
    pub ik: usize,
    pub morphs: usize,
    pub display_frames: usize,
    pub rigid_bodies: usize,
    pub joints: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedGeometry {
    pub vertices: Vec<PmdParsedVertex>,
    pub indices: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bone_indices: [i32; 2],
    pub bone_weight: u8,
    pub edge_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedMaterial {
    pub diffuse: [f32; 4],
    pub specular_power: f32,
    pub specular: [f32; 3],
    pub ambient: [f32; 3],
    pub toon_index: u8,
    pub edge_enabled: bool,
    pub face_count: u32,
    pub texture_name: String,
    pub texture_name_bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmdParsedSkeleton {
    pub bones: Vec<PmdParsedBone>,
    pub ik: Vec<PmdParsedIk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedBone {
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub english_name: String,
    pub english_name_bytes: Vec<u8>,
    pub parent_index: i32,
    pub tail_index: i32,
    pub bone_type: u8,
    pub ik_index: i32,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedIk {
    pub bone_index: u16,
    pub target_bone_index: u16,
    pub loop_count: u16,
    pub limit_angle: f32,
    pub links: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedMorph {
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub english_name: String,
    pub english_name_bytes: Vec<u8>,
    #[serde(rename = "type")]
    pub kind: String,
    pub vertex_count: usize,
    pub vertex_offsets: Vec<PmdParsedMorphVertexOffset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedMorphVertexOffset {
    pub vertex_index: u32,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedDisplayFrame {
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub english_name: String,
    pub english_name_bytes: Vec<u8>,
    pub frames: Vec<PmdParsedDisplayFrameElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmdParsedDisplayFrameElement {
    #[serde(rename = "type")]
    pub kind: String,
    pub index: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmdParsedRigidBody {
    pub name: String,
    pub name_bytes: Vec<u8>,
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
pub struct PmdParsedJoint {
    pub name: String,
    pub name_bytes: Vec<u8>,
    pub rigid_body_index_a: u32,
    pub rigid_body_index_b: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub translation_min: [f32; 3],
    pub translation_max: [f32; 3],
    pub rotation_min: [f32; 3],
    pub rotation_max: [f32; 3],
    pub spring_translation: [f32; 3],
    pub spring_rotation: [f32; 3],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmdParserDiagnostic {
    pub level: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub struct PmdRuntimeImport {
    pub model: ModelArena,
    pub bone_names: Vec<String>,
    pub bone_name_to_index: HashMap<Vec<u8>, BoneIndex>,
    pub morph_name_to_index: HashMap<Vec<u8>, MorphIndex>,
    pub ik_solver_bone_name_to_index: HashMap<Vec<u8>, usize>,
    pub diagnostics: Vec<PmdParserDiagnostic>,
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

    fn peek_u32_at(&self, pos: usize) -> Option<u32> {
        let bytes = self.data.get(pos..pos + 4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read(&mut self, n: usize) -> Result<&'a [u8], ImportError> {
        if self.remaining() < n {
            return Err(ImportError::UnexpectedEof(n - self.remaining()));
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn skip(&mut self, n: usize) -> Result<(), ImportError> {
        self.read(n).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8, ImportError> {
        Ok(self.read(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ImportError> {
        let b = self.read(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32, ImportError> {
        let b = self.read(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, ImportError> {
        let b = self.read(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn vec3(&mut self) -> Result<[f32; 3], ImportError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn fixed_text_raw(&mut self, n: usize) -> Result<(String, Vec<u8>), ImportError> {
        let bytes = self.read(n)?;
        Ok((decode_sjis_fixed(bytes), bytes.to_vec()))
    }
}

pub fn parse_pmd_model(data: &[u8]) -> Result<PmdParsedModel, ImportError> {
    let mut r = Reader::new(data);
    if r.read(3)? != b"Pmd" {
        return Err(ImportError::InvalidMagic { format: "PMD" });
    }
    let version = r.f32()?;
    let (name, name_bytes) = r.fixed_text_raw(20)?;
    let (comment, comment_bytes) = r.fixed_text_raw(256)?;

    let vertex_count = r.u32()? as usize;
    let mut vertices = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        vertices.push(PmdParsedVertex {
            position: r.vec3()?,
            normal: r.vec3()?,
            uv: [r.f32()?, r.f32()?],
            bone_indices: [normalize_index(r.u16()?), normalize_index(r.u16()?)],
            bone_weight: r.u8()?,
            edge_enabled: r.u8()? == 0,
        });
    }
    let index_count = r.u32()? as usize;
    let mut indices = Vec::with_capacity(index_count);
    for _ in 0..index_count {
        indices.push(r.u16()?);
    }

    let material_count = r.u32()? as usize;
    let mut materials = Vec::with_capacity(material_count);
    for _ in 0..material_count {
        let diffuse = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
        let specular_power = r.f32()?;
        let specular = [r.f32()?, r.f32()?, r.f32()?];
        let ambient = [r.f32()?, r.f32()?, r.f32()?];
        let toon_index = r.u8()?;
        let edge_enabled = r.u8()? != 0;
        let face_count = r.u32()? / 3;
        let (texture_name, texture_name_bytes) = r.fixed_text_raw(20)?;
        materials.push(PmdParsedMaterial {
            diffuse,
            specular_power,
            specular,
            ambient,
            toon_index,
            edge_enabled,
            face_count,
            texture_name,
            texture_name_bytes,
        });
    }

    let bone_count = r.u16()? as usize;
    let mut bones = Vec::with_capacity(bone_count);
    for _ in 0..bone_count {
        let (name, name_bytes) = r.fixed_text_raw(20)?;
        bones.push(PmdParsedBone {
            name,
            name_bytes,
            english_name: String::new(),
            english_name_bytes: Vec::new(),
            parent_index: normalize_index(r.u16()?),
            tail_index: normalize_index(r.u16()?),
            bone_type: r.u8()?,
            ik_index: normalize_index(r.u16()?),
            position: r.vec3()?,
        });
    }

    let ik_count = r.u16()? as usize;
    let mut ik = Vec::with_capacity(ik_count);
    for _ in 0..ik_count {
        let bone_index = r.u16()?;
        let target_bone_index = r.u16()?;
        let link_count = r.u8()? as usize;
        let loop_count = r.u16()?;
        let limit_angle = r.f32()?;
        let mut links = Vec::with_capacity(link_count);
        for _ in 0..link_count {
            links.push(r.u16()?);
        }
        ik.push(PmdParsedIk {
            bone_index,
            target_bone_index,
            loop_count,
            limit_angle,
            links,
        });
    }

    let morph_count = r.u16()? as usize;
    let mut morphs = Vec::with_capacity(morph_count);
    for _ in 0..morph_count {
        let (name, name_bytes) = r.fixed_text_raw(20)?;
        let vertex_count = r.u32()? as usize;
        let morph_type = r.u8()?;
        let mut vertex_offsets = Vec::with_capacity(vertex_count);
        for _ in 0..vertex_count {
            vertex_offsets.push(PmdParsedMorphVertexOffset {
                vertex_index: r.u32()?,
                position: r.vec3()?,
            });
        }
        morphs.push(PmdParsedMorph {
            name,
            name_bytes,
            english_name: String::new(),
            english_name_bytes: Vec::new(),
            kind: if morph_type == 0 { "base" } else { "vertex" }.to_owned(),
            vertex_count,
            vertex_offsets,
        });
    }

    let mut display_frames = Vec::new();
    let mut bone_display_name_count = 0usize;
    let mut bone_display_frame_start = 0usize;
    if r.remaining() >= 1 {
        let morph_display_count = r.u8()? as usize;
        for _ in 0..morph_display_count {
            let index = r.u16()?;
            display_frames.push(PmdParsedDisplayFrame {
                name: morphs
                    .get(index as usize)
                    .map(|m| m.name.clone())
                    .unwrap_or_default(),
                name_bytes: morphs
                    .get(index as usize)
                    .map(|m| m.name_bytes.clone())
                    .unwrap_or_default(),
                english_name: String::new(),
                english_name_bytes: Vec::new(),
                frames: vec![PmdParsedDisplayFrameElement {
                    kind: "morph".to_owned(),
                    index,
                }],
            });
        }
    }
    if r.remaining() >= 1 {
        bone_display_name_count = r.u8()? as usize;
        bone_display_frame_start = display_frames.len();
        for _ in 0..bone_display_name_count {
            let (name, name_bytes) = r.fixed_text_raw(50)?;
            display_frames.push(PmdParsedDisplayFrame {
                name,
                name_bytes,
                english_name: String::new(),
                english_name_bytes: Vec::new(),
                frames: Vec::new(),
            });
        }
        if r.remaining() >= 4 {
            let bone_display_count = r.u32()? as usize;
            for _ in 0..bone_display_count {
                let bone_index = r.u16()?;
                let frame_index = r.u8()? as usize;
                if let Some(frame) =
                    display_frames.get_mut(bone_display_frame_start + frame_index.saturating_sub(1))
                {
                    frame.frames.push(PmdParsedDisplayFrameElement {
                        kind: "bone".to_owned(),
                        index: bone_index,
                    });
                }
            }
        }
    }

    let mut english_name = String::new();
    let mut english_comment = String::new();
    let mut english_name_bytes = Vec::new();
    let mut english_comment_bytes = Vec::new();
    if r.remaining() >= 1 && r.u8()? != 0 {
        (english_name, english_name_bytes) = r.fixed_text_raw(20)?;
        (english_comment, english_comment_bytes) = r.fixed_text_raw(256)?;
        for bone in &mut bones {
            (bone.english_name, bone.english_name_bytes) = r.fixed_text_raw(20)?;
        }
        for morph in morphs.iter_mut().skip(1) {
            (morph.english_name, morph.english_name_bytes) = r.fixed_text_raw(20)?;
        }
        for index in 0..bone_display_name_count {
            if let Some(frame) = display_frames.get_mut(bone_display_frame_start + index) {
                (frame.english_name, frame.english_name_bytes) = r.fixed_text_raw(50)?;
            } else {
                r.skip(50)?;
            }
        }
    }

    let mut toon_textures = Vec::new();
    let mut toon_texture_bytes = Vec::new();
    if r.remaining() >= 1000 && !is_plausible_pmd_physics_tail(&r) {
        toon_textures.reserve(10);
        toon_texture_bytes.reserve(10);
        for _ in 0..10 {
            let (toon_texture, bytes) = r.fixed_text_raw(100)?;
            toon_textures.push(toon_texture);
            toon_texture_bytes.push(bytes);
        }
    }

    let rigid_bodies = if r.remaining() >= 4 {
        read_rigid_bodies(&mut r)?
    } else {
        Vec::new()
    };
    let joints = if r.remaining() >= 4 {
        read_joints(&mut r)?
    } else {
        Vec::new()
    };

    Ok(PmdParsedModel {
        metadata: PmdParsedMetadata {
            format: "pmd".to_owned(),
            version,
            encoding: "shift-jis".to_owned(),
            name,
            name_bytes,
            english_name,
            english_name_bytes,
            comment,
            comment_bytes,
            english_comment,
            english_comment_bytes,
            counts: PmdParsedCounts {
                vertices: vertex_count,
                faces: index_count / 3,
                materials: materials.len(),
                bones: bones.len(),
                ik: ik.len(),
                morphs: morphs.len(),
                display_frames: display_frames.len(),
                rigid_bodies: rigid_bodies.len(),
                joints: joints.len(),
            },
        },
        geometry: PmdParsedGeometry { vertices, indices },
        materials,
        toon_textures,
        toon_texture_bytes,
        skeleton: PmdParsedSkeleton { bones, ik },
        morphs,
        display_frames,
        rigid_bodies,
        joints,
        diagnostics: Vec::new(),
    })
}

pub fn import_pmd_runtime(data: &[u8]) -> Result<PmdRuntimeImport, ImportError> {
    let parsed = parse_pmd_model(data)?;
    build_pmd_runtime_from_parsed(parsed)
}

pub fn export_pmd_model(model: &PmdParsedModel) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"Pmd");
    write_f32(&mut out, model.metadata.version);
    write_fixed_text(
        &mut out,
        &model.metadata.name,
        &model.metadata.name_bytes,
        20,
    );
    write_fixed_text(
        &mut out,
        &model.metadata.comment,
        &model.metadata.comment_bytes,
        256,
    );

    write_u32(&mut out, model.geometry.vertices.len() as u32);
    for vertex in &model.geometry.vertices {
        write_vec3(&mut out, vertex.position);
        write_vec3(&mut out, vertex.normal);
        write_f32(&mut out, vertex.uv[0]);
        write_f32(&mut out, vertex.uv[1]);
        write_index_u16(&mut out, vertex.bone_indices[0]);
        write_index_u16(&mut out, vertex.bone_indices[1]);
        out.push(vertex.bone_weight);
        out.push(if vertex.edge_enabled { 0 } else { 1 });
    }

    write_u32(&mut out, model.geometry.indices.len() as u32);
    for &index in &model.geometry.indices {
        write_u16(&mut out, index);
    }

    write_u32(&mut out, model.materials.len() as u32);
    for material in &model.materials {
        for value in material.diffuse {
            write_f32(&mut out, value);
        }
        write_f32(&mut out, material.specular_power);
        write_vec3(&mut out, material.specular);
        write_vec3(&mut out, material.ambient);
        out.push(material.toon_index);
        out.push(u8::from(material.edge_enabled));
        write_u32(&mut out, material.face_count.saturating_mul(3));
        write_fixed_text(
            &mut out,
            &material.texture_name,
            &material.texture_name_bytes,
            20,
        );
    }

    write_u16(&mut out, model.skeleton.bones.len() as u16);
    for bone in &model.skeleton.bones {
        write_fixed_text(&mut out, &bone.name, &bone.name_bytes, 20);
        write_index_u16(&mut out, bone.parent_index);
        write_index_u16(&mut out, bone.tail_index);
        out.push(bone.bone_type);
        write_index_u16(&mut out, bone.ik_index);
        write_vec3(&mut out, bone.position);
    }

    write_u16(&mut out, model.skeleton.ik.len() as u16);
    for ik in &model.skeleton.ik {
        write_u16(&mut out, ik.bone_index);
        write_u16(&mut out, ik.target_bone_index);
        out.push(ik.links.len() as u8);
        write_u16(&mut out, ik.loop_count);
        write_f32(&mut out, ik.limit_angle);
        for &link in &ik.links {
            write_u16(&mut out, link);
        }
    }

    write_u16(&mut out, model.morphs.len() as u16);
    for morph in &model.morphs {
        write_fixed_text(&mut out, &morph.name, &morph.name_bytes, 20);
        write_u32(&mut out, morph.vertex_offsets.len() as u32);
        out.push(match morph.kind.as_str() {
            "base" => 0,
            _ => 1,
        });
        for offset in &morph.vertex_offsets {
            write_u32(&mut out, offset.vertex_index);
            write_vec3(&mut out, offset.position);
        }
    }

    let morph_display_indices: Vec<u16> = model
        .display_frames
        .iter()
        .filter_map(|frame| {
            frame
                .frames
                .first()
                .filter(|entry| entry.kind == "morph")
                .map(|entry| entry.index)
        })
        .collect();
    let bone_display_frames: Vec<&PmdParsedDisplayFrame> = model
        .display_frames
        .iter()
        .filter(|frame| {
            frame
                .frames
                .first()
                .is_none_or(|entry| entry.kind != "morph")
        })
        .collect();
    let has_english = should_export_pmd_english(model, &bone_display_frames);
    let has_toon = should_export_pmd_toon(model);
    let has_physics = !model.rigid_bodies.is_empty() || !model.joints.is_empty();
    let has_display = !morph_display_indices.is_empty() || !bone_display_frames.is_empty();

    if has_display || has_english || has_toon || has_physics {
        out.push(morph_display_indices.len() as u8);
        for index in morph_display_indices {
            write_u16(&mut out, index);
        }

        out.push(bone_display_frames.len() as u8);
        for frame in &bone_display_frames {
            write_fixed_text(&mut out, &frame.name, &frame.name_bytes, 50);
        }

        let bone_display_count: usize = bone_display_frames
            .iter()
            .map(|frame| {
                frame
                    .frames
                    .iter()
                    .filter(|entry| entry.kind == "bone")
                    .count()
            })
            .sum();
        write_u32(&mut out, bone_display_count as u32);
        for (frame_index, frame) in bone_display_frames.iter().enumerate() {
            for entry in frame.frames.iter().filter(|entry| entry.kind == "bone") {
                write_u16(&mut out, entry.index);
                out.push((frame_index + 1) as u8);
            }
        }

        out.push(u8::from(has_english));
        if has_english {
            write_fixed_text(
                &mut out,
                &model.metadata.english_name,
                &model.metadata.english_name_bytes,
                20,
            );
            write_fixed_text(
                &mut out,
                &model.metadata.english_comment,
                &model.metadata.english_comment_bytes,
                256,
            );
            for bone in &model.skeleton.bones {
                write_fixed_text(&mut out, &bone.english_name, &bone.english_name_bytes, 20);
            }
            for morph in model.morphs.iter().skip(1) {
                write_fixed_text(&mut out, &morph.english_name, &morph.english_name_bytes, 20);
            }
            for frame in &bone_display_frames {
                write_fixed_text(&mut out, &frame.english_name, &frame.english_name_bytes, 50);
            }
        }
    }

    if has_toon {
        for index in 0..10 {
            let raw = model
                .toon_texture_bytes
                .get(index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let text = model
                .toon_textures
                .get(index)
                .map(String::as_str)
                .unwrap_or("");
            write_fixed_text(&mut out, text, raw, 100);
        }
    }

    if has_physics {
        write_u32(&mut out, model.rigid_bodies.len() as u32);
        for body in &model.rigid_bodies {
            write_fixed_text(&mut out, &body.name, &body.name_bytes, 20);
            write_index_u16(&mut out, body.bone_index);
            out.push(body.group);
            write_u16(&mut out, body.mask);
            out.push(match body.shape.as_str() {
                "sphere" => 0,
                "box" => 1,
                "capsule" => 2,
                _ => 0,
            });
            write_vec3(&mut out, body.size);
            write_vec3(&mut out, body.position);
            write_vec3(&mut out, body.rotation);
            write_f32(&mut out, body.mass);
            write_f32(&mut out, body.linear_damping);
            write_f32(&mut out, body.angular_damping);
            write_f32(&mut out, body.restitution);
            write_f32(&mut out, body.friction);
            out.push(match body.mode.as_str() {
                "static" => 0,
                "dynamic" => 1,
                "dynamicBone" => 2,
                _ => 0,
            });
        }

        write_u32(&mut out, model.joints.len() as u32);
        for joint in &model.joints {
            write_fixed_text(&mut out, &joint.name, &joint.name_bytes, 20);
            write_u32(&mut out, joint.rigid_body_index_a);
            write_u32(&mut out, joint.rigid_body_index_b);
            write_vec3(&mut out, joint.position);
            write_vec3(&mut out, joint.rotation);
            write_vec3(&mut out, joint.translation_min);
            write_vec3(&mut out, joint.translation_max);
            write_vec3(&mut out, joint.rotation_min);
            write_vec3(&mut out, joint.rotation_max);
            write_vec3(&mut out, joint.spring_translation);
            write_vec3(&mut out, joint.spring_rotation);
        }
    }

    out
}

fn build_pmd_runtime_from_parsed(parsed: PmdParsedModel) -> Result<PmdRuntimeImport, ImportError> {
    let bones = parsed
        .skeleton
        .bones
        .iter()
        .map(|bone| {
            BoneInit::new(
                if bone.parent_index < 0 {
                    None
                } else {
                    Some(BoneIndex(bone.parent_index as u32))
                },
                Vec3A::new(bone.position[0], bone.position[1], bone.position[2]),
            )
        })
        .collect::<Vec<_>>();

    let ik_solvers = parsed
        .skeleton
        .ik
        .iter()
        .map(|ik| IkSolverInit {
            ik_bone: BoneIndex(u32::from(ik.bone_index)),
            target_bone: BoneIndex(u32::from(ik.target_bone_index)),
            links: ik
                .links
                .iter()
                .map(|&link| IkLinkInit::new(BoneIndex(u32::from(link))))
                .collect(),
            iteration_count: u32::from(ik.loop_count),
            limit_angle: ik.limit_angle,
        })
        .collect::<Vec<_>>();

    let (vertex_offsets, vertex_spans, vertex_diagnostics) =
        build_pmd_vertex_morph_offsets(&parsed.morphs, parsed.metadata.counts.vertices);
    let morph_count = parsed.morphs.len();
    let morph = MorphInit {
        morph_count: morph_count as u32,
        vertex_offsets,
        vertex_spans,
        bone_spans: vec![Default::default(); morph_count],
        group_spans: vec![Default::default(); morph_count],
        ..MorphInit::default()
    };
    let model = ModelArena::new_with_morphs(bones, ik_solvers, Vec::new(), morph)
        .map_err(ImportError::ModelBuildFailed)?;

    let mut bone_name_to_index = HashMap::with_capacity(parsed.skeleton.bones.len() * 2);
    for (index, bone) in parsed.skeleton.bones.iter().enumerate() {
        insert_sjis_name_keys(&mut bone_name_to_index, &bone.name, BoneIndex(index as u32));
    }

    let mut morph_name_to_index = HashMap::with_capacity(parsed.morphs.len() * 2);
    for (index, morph) in parsed.morphs.iter().enumerate() {
        insert_sjis_name_keys(
            &mut morph_name_to_index,
            &morph.name,
            MorphIndex(index as u32),
        );
    }

    let mut ik_solver_bone_name_to_index = HashMap::with_capacity(parsed.skeleton.ik.len() * 2);
    for (solver_index, ik) in parsed.skeleton.ik.iter().enumerate() {
        if let Some(bone) = parsed.skeleton.bones.get(ik.bone_index as usize) {
            insert_sjis_name_keys(&mut ik_solver_bone_name_to_index, &bone.name, solver_index);
        }
    }

    let mut diagnostics = parsed.diagnostics;
    diagnostics.extend(vertex_diagnostics);
    diagnostics.push(PmdParserDiagnostic {
        level: "warning".to_owned(),
        code: "PMD_RUNTIME_PARTIAL".to_owned(),
        message: "PMD runtime import builds bones, IK solvers, morph slots, and vertex morph offsets; material/physics runtime parity is not implemented yet.".to_owned(),
    });

    Ok(PmdRuntimeImport {
        model,
        bone_names: parsed
            .skeleton
            .bones
            .iter()
            .map(|bone| bone.name.clone())
            .collect(),
        bone_name_to_index,
        morph_name_to_index,
        ik_solver_bone_name_to_index,
        diagnostics,
    })
}

fn build_pmd_vertex_morph_offsets(
    morphs: &[PmdParsedMorph],
    vertex_count: usize,
) -> (
    Vec<VertexMorphOffset>,
    Vec<MorphOffsetSpan>,
    Vec<PmdParserDiagnostic>,
) {
    let mut offsets = Vec::new();
    let mut spans = vec![MorphOffsetSpan::default(); morphs.len()];
    let mut diagnostics = Vec::new();
    let base_vertices = morphs.first().filter(|morph| morph.kind == "base");

    for (morph_index, morph) in morphs.iter().enumerate() {
        if morph.kind != "vertex" {
            continue;
        }
        let start = offsets.len() as u32;
        for vertex in &morph.vertex_offsets {
            let Some(base) = base_vertices.and_then(|base| {
                base.vertex_offsets
                    .get(vertex.vertex_index as usize)
                    .map(|base| base.vertex_index)
            }) else {
                diagnostics.push(PmdParserDiagnostic {
                    level: "warning".to_owned(),
                    code: "PMD_VERTEX_MORPH_BASE_INDEX_MISSING".to_owned(),
                    message: format!(
                        "PMD morph {:?} references missing base morph vertex {}",
                        morph.name, vertex.vertex_index
                    ),
                });
                continue;
            };
            if base as usize >= vertex_count {
                diagnostics.push(PmdParserDiagnostic {
                    level: "warning".to_owned(),
                    code: "PMD_VERTEX_MORPH_VERTEX_INDEX_OUT_OF_RANGE".to_owned(),
                    message: format!(
                        "PMD morph {:?} resolved vertex index {} outside vertex count {}",
                        morph.name, base, vertex_count
                    ),
                });
                continue;
            }
            offsets.push(VertexMorphOffset {
                vertex_index: base,
                position_offset: Vec3A::new(
                    vertex.position[0],
                    vertex.position[1],
                    vertex.position[2],
                ),
            });
        }
        spans[morph_index] = MorphOffsetSpan {
            start,
            count: offsets.len() as u32 - start,
        };
    }

    (offsets, spans, diagnostics)
}

fn insert_sjis_name_keys<T: Copy>(map: &mut HashMap<Vec<u8>, T>, name: &str, value: T) {
    let (encoded, _, _) = SHIFT_JIS.encode(name);
    let normalized = normalize_vmd_name(encoded.as_ref());
    if !normalized.is_empty() {
        map.insert(normalized, value);
    }
    if !name.is_empty() {
        map.insert(name.as_bytes().to_vec(), value);
    }
}

fn read_rigid_bodies(r: &mut Reader<'_>) -> Result<Vec<PmdParsedRigidBody>, ImportError> {
    let count = r.u32()? as usize;
    let mut bodies = Vec::with_capacity(count);
    for _ in 0..count {
        let (name, name_bytes) = r.fixed_text_raw(20)?;
        bodies.push(PmdParsedRigidBody {
            name,
            name_bytes,
            bone_index: normalize_index(r.u16()?),
            group: r.u8()?,
            mask: r.u16()?,
            shape: match r.u8()? {
                0 => "sphere",
                1 => "box",
                2 => "capsule",
                _ => "unknown",
            }
            .to_owned(),
            size: r.vec3()?,
            position: r.vec3()?,
            rotation: r.vec3()?,
            mass: r.f32()?,
            linear_damping: r.f32()?,
            angular_damping: r.f32()?,
            restitution: r.f32()?,
            friction: r.f32()?,
            mode: match r.u8()? {
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

fn read_joints(r: &mut Reader<'_>) -> Result<Vec<PmdParsedJoint>, ImportError> {
    let count = r.u32()? as usize;
    let mut joints = Vec::with_capacity(count);
    for _ in 0..count {
        let (name, name_bytes) = r.fixed_text_raw(20)?;
        joints.push(PmdParsedJoint {
            name,
            name_bytes,
            rigid_body_index_a: r.u32()?,
            rigid_body_index_b: r.u32()?,
            position: r.vec3()?,
            rotation: r.vec3()?,
            translation_min: r.vec3()?,
            translation_max: r.vec3()?,
            rotation_min: r.vec3()?,
            rotation_max: r.vec3()?,
            spring_translation: r.vec3()?,
            spring_rotation: r.vec3()?,
        });
    }
    Ok(joints)
}

fn is_plausible_pmd_physics_tail(r: &Reader<'_>) -> bool {
    let start = r.pos;
    let Some(rigid_body_count) = r.peek_u32_at(start).map(|count| count as usize) else {
        return false;
    };
    let Some(rigid_body_bytes) = rigid_body_count.checked_mul(83) else {
        return false;
    };
    let joint_count_offset = start + 4 + rigid_body_bytes;
    if joint_count_offset == r.data.len() {
        return true;
    }
    let Some(joint_count) = r
        .peek_u32_at(joint_count_offset)
        .map(|count| count as usize)
    else {
        return false;
    };
    let Some(joint_bytes) = joint_count.checked_mul(124) else {
        return false;
    };
    joint_count_offset + 4 + joint_bytes == r.data.len()
}

fn normalize_index(index: u16) -> i32 {
    if index == 0xffff { -1 } else { index as i32 }
}

fn write_fixed_text(out: &mut Vec<u8>, text: &str, raw: &[u8], len: usize) {
    let mut bytes = vec![0u8; len];
    if !raw.is_empty() {
        let copy_len = raw.len().min(len);
        bytes[..copy_len].copy_from_slice(&raw[..copy_len]);
    } else {
        let mut cursor = 0;
        for ch in text.chars() {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            let (encoded, _, _) = SHIFT_JIS.encode(s);
            let encoded = encoded.as_ref();
            if cursor + encoded.len() > len {
                break;
            }
            bytes[cursor..cursor + encoded.len()].copy_from_slice(encoded);
            cursor += encoded.len();
        }
    }
    out.extend_from_slice(&bytes);
}

fn write_index_u16(out: &mut Vec<u8>, index: i32) {
    write_u16(out, if index < 0 { 0xffff } else { index as u16 });
}

fn write_vec3(out: &mut Vec<u8>, values: [f32; 3]) {
    for value in values {
        write_f32(out, value);
    }
}

fn write_f32(out: &mut Vec<u8>, value: f32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn should_export_pmd_english(
    model: &PmdParsedModel,
    bone_display_frames: &[&PmdParsedDisplayFrame],
) -> bool {
    !model.metadata.english_name.is_empty()
        || !model.metadata.english_name_bytes.is_empty()
        || !model.metadata.english_comment.is_empty()
        || !model.metadata.english_comment_bytes.is_empty()
        || model
            .skeleton
            .bones
            .iter()
            .any(|bone| !bone.english_name.is_empty() || !bone.english_name_bytes.is_empty())
        || model
            .morphs
            .iter()
            .skip(1)
            .any(|morph| !morph.english_name.is_empty() || !morph.english_name_bytes.is_empty())
        || bone_display_frames
            .iter()
            .any(|frame| !frame.english_name.is_empty() || !frame.english_name_bytes.is_empty())
}

fn should_export_pmd_toon(model: &PmdParsedModel) -> bool {
    model.toon_texture_bytes.len() == 10 || model.toon_textures.len() == 10
}

fn decode_sjis_fixed(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let (decoded, _, _) = SHIFT_JIS.decode(&bytes[..end]);
    decoded.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_fixed(out: &mut Vec<u8>, value: &[u8], len: usize) {
        let mut bytes = vec![0u8; len];
        bytes[..value.len().min(len)].copy_from_slice(&value[..value.len().min(len)]);
        out.extend_from_slice(&bytes);
    }

    fn push_bone(
        out: &mut Vec<u8>,
        name: &[u8],
        parent: u16,
        tail: u16,
        bone_type: u8,
        ik_index: u16,
        position: [f32; 3],
    ) {
        push_fixed(out, name, 20);
        out.extend_from_slice(&parent.to_le_bytes());
        out.extend_from_slice(&tail.to_le_bytes());
        out.push(bone_type);
        out.extend_from_slice(&ik_index.to_le_bytes());
        for value in position {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn minimal_pmd_with_ik() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"Pmd");
        out.extend_from_slice(&1.0f32.to_le_bytes());
        push_fixed(&mut out, b"model", 20);
        push_fixed(&mut out, b"comment", 256);
        out.extend_from_slice(&0u32.to_le_bytes()); // vertices
        out.extend_from_slice(&0u32.to_le_bytes()); // indices
        out.extend_from_slice(&0u32.to_le_bytes()); // materials

        out.extend_from_slice(&2u16.to_le_bytes());
        push_bone(&mut out, b"root", 0xffff, 1, 0, 0xffff, [0.0, 0.0, 0.0]);
        push_bone(&mut out, b"legIK", 0, 0, 2, 0, [0.0, 1.0, 0.0]);

        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // IK bone
        out.extend_from_slice(&0u16.to_le_bytes()); // target bone
        out.push(1); // links
        out.extend_from_slice(&40u16.to_le_bytes());
        out.extend_from_slice(&1.0f32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());

        out.extend_from_slice(&1u16.to_le_bytes());
        push_fixed(&mut out, b"base", 20);
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(0);
        out
    }

    fn insert_empty_vertices(data: &mut Vec<u8>, vertex_count: u32) {
        let vertex_count_pos = 3 + 4 + 20 + 256;
        data[vertex_count_pos..vertex_count_pos + 4].copy_from_slice(&vertex_count.to_le_bytes());
        let insert_pos = vertex_count_pos + 4;
        data.splice(
            insert_pos..insert_pos,
            vec![0u8; vertex_count as usize * 38],
        );
    }

    fn insert_vertex_and_indices(data: &mut Vec<u8>, vertex: &[u8], indices: &[u16]) {
        let vertex_count_pos = 3 + 4 + 20 + 256;
        data[vertex_count_pos..vertex_count_pos + 4].copy_from_slice(&1u32.to_le_bytes());
        let vertex_insert_pos = vertex_count_pos + 4;
        data.splice(vertex_insert_pos..vertex_insert_pos, vertex.iter().copied());
        let index_count_pos = vertex_insert_pos + vertex.len();
        data[index_count_pos..index_count_pos + 4]
            .copy_from_slice(&(indices.len() as u32).to_le_bytes());
        let index_insert_pos = index_count_pos + 4;
        let mut index_bytes = Vec::new();
        for index in indices {
            index_bytes.extend_from_slice(&index.to_le_bytes());
        }
        data.splice(index_insert_pos..index_insert_pos, index_bytes);
    }

    fn insert_material(data: &mut Vec<u8>, material: &[u8]) {
        let vertex_count_pos = 3 + 4 + 20 + 256;
        let vertex_count = u32::from_le_bytes(
            data[vertex_count_pos..vertex_count_pos + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let index_count_pos = vertex_count_pos + 4 + vertex_count * 38;
        let index_count = u32::from_le_bytes(
            data[index_count_pos..index_count_pos + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let material_count_pos = index_count_pos + 4 + index_count * 2;
        data[material_count_pos..material_count_pos + 4].copy_from_slice(&1u32.to_le_bytes());
        data.splice(
            material_count_pos + 4..material_count_pos + 4,
            material.iter().copied(),
        );
    }

    fn sample_vertex(edge_byte: u8) -> Vec<u8> {
        let mut vertex = Vec::new();
        for value in [1.0f32, 2.0, 3.0] {
            vertex.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0.0f32, 1.0, 0.0] {
            vertex.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0.25f32, 0.75] {
            vertex.extend_from_slice(&value.to_le_bytes());
        }
        vertex.extend_from_slice(&0u16.to_le_bytes());
        vertex.extend_from_slice(&1u16.to_le_bytes());
        vertex.push(80);
        vertex.push(edge_byte);
        vertex
    }

    fn sample_material(edge_byte: u8) -> Vec<u8> {
        let mut material = Vec::new();
        for value in [0.1f32, 0.2, 0.3, 0.4] {
            material.extend_from_slice(&value.to_le_bytes());
        }
        material.extend_from_slice(&5.0f32.to_le_bytes());
        for value in [0.5f32, 0.6, 0.7] {
            material.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0.8f32, 0.9, 1.0] {
            material.extend_from_slice(&value.to_le_bytes());
        }
        material.push(3);
        material.push(edge_byte);
        material.extend_from_slice(&3u32.to_le_bytes());
        push_fixed(&mut material, b"tex.bmp", 20);
        material
    }

    fn push_vec3(out: &mut Vec<u8>, values: [f32; 3]) {
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
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
    fn pmd_model_json_top_level_schema_is_stable() {
        let parsed = parse_pmd_model(&minimal_pmd_with_ik()).unwrap();
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
                "toonTextureBytes",
                "toonTextures",
            ]
        );
    }

    #[test]
    fn imports_pmd_runtime_skeleton_and_name_maps() {
        let imported = import_pmd_runtime(&minimal_pmd_with_ik()).unwrap();

        assert_eq!(imported.model.bone_count(), 2);
        assert_eq!(imported.model.ik_count(), 1);
        assert_eq!(imported.model.morph_count(), 1);
        assert_eq!(imported.bone_names, vec!["root", "legIK"]);
        assert_eq!(
            imported.bone_name_to_index.get(b"legIK".as_slice()),
            Some(&BoneIndex(1))
        );
        assert_eq!(
            imported.morph_name_to_index.get(b"base".as_slice()),
            Some(&MorphIndex(0))
        );
        assert_eq!(
            imported
                .ik_solver_bone_name_to_index
                .get(b"legIK".as_slice()),
            Some(&0)
        );
        assert_eq!(imported.diagnostics[0].code, "PMD_RUNTIME_PARTIAL");
    }

    #[test]
    fn exports_minimal_pmd_roundtrip() {
        let data = minimal_pmd_with_ik();
        let parsed = parse_pmd_model(&data).unwrap();
        let exported = export_pmd_model(&parsed);
        let reparsed = parse_pmd_model(&exported).unwrap();

        assert_eq!(exported, data);
        assert_eq!(reparsed.metadata.name, parsed.metadata.name);
        assert_eq!(reparsed.skeleton.bones.len(), 2);
        assert_eq!(reparsed.skeleton.ik.len(), 1);
        assert_eq!(reparsed.morphs.len(), 1);
    }

    #[test]
    fn exports_pmd_json_dto_roundtrip() {
        let data = minimal_pmd_with_ik();
        let parsed = parse_pmd_model(&data).unwrap();
        let json = serde_json::to_string(&parsed).unwrap();
        let from_json: PmdParsedModel = serde_json::from_str(&json).unwrap();
        let exported = export_pmd_model(&from_json);
        let reparsed = parse_pmd_model(&exported).unwrap();

        assert_eq!(exported, data);
        assert_eq!(reparsed.metadata.name, parsed.metadata.name);
        assert_eq!(reparsed.skeleton.bones.len(), 2);
        assert_eq!(reparsed.skeleton.ik.len(), 1);
        assert_eq!(reparsed.morphs.len(), 1);
    }

    #[test]
    fn exports_pmd_english_block_roundtrip() {
        let mut data = minimal_pmd_with_ik();
        data.push(0); // morph display count
        data.push(1); // bone display frame name count
        push_fixed(&mut data, b"frame", 50);
        data.extend_from_slice(&1u32.to_le_bytes()); // bone display count
        data.extend_from_slice(&1u16.to_le_bytes());
        data.push(1);
        data.push(1); // english block enabled
        push_fixed(&mut data, b"model-en", 20);
        push_fixed(&mut data, b"comment-en", 256);
        push_fixed(&mut data, b"root-en", 20);
        push_fixed(&mut data, b"legIK-en", 20);
        push_fixed(&mut data, b"frame-en", 50);

        let parsed = parse_pmd_model(&data).unwrap();
        let exported = export_pmd_model(&parsed);
        let reparsed = parse_pmd_model(&exported).unwrap();

        assert_eq!(exported, data);
        assert_eq!(reparsed.metadata.english_name, "model-en");
        assert_eq!(reparsed.skeleton.bones[1].english_name, "legIK-en");
        assert_eq!(reparsed.display_frames[0].english_name, "frame-en");
        assert_eq!(reparsed.display_frames[0].frames[0].kind, "bone");
        assert_eq!(reparsed.display_frames[0].frames[0].index, 1);
    }

    #[test]
    fn exports_pmd_toon_texture_block_roundtrip() {
        let mut data = minimal_pmd_with_ik();
        data.push(0); // morph display count
        data.push(0); // bone display frame name count
        data.extend_from_slice(&0u32.to_le_bytes()); // bone display count
        data.push(0); // english block disabled
        for i in 0..10 {
            push_fixed(&mut data, format!("toon{i}.bmp").as_bytes(), 100);
        }

        let parsed = parse_pmd_model(&data).unwrap();
        let exported = export_pmd_model(&parsed);
        let reparsed = parse_pmd_model(&exported).unwrap();

        assert_eq!(exported, data);
        assert_eq!(reparsed.toon_textures.len(), 10);
        assert_eq!(reparsed.toon_textures[9], "toon9.bmp");
    }

    #[test]
    fn exports_pmd_physics_tail_roundtrip() {
        let mut data = minimal_pmd_with_ik();
        data.push(0); // morph display count
        data.push(0); // bone display frame name count
        data.extend_from_slice(&0u32.to_le_bytes()); // bone display count
        data.push(0); // english block disabled

        data.extend_from_slice(&1u32.to_le_bytes()); // rigid bodies
        push_fixed(&mut data, b"body", 20);
        data.extend_from_slice(&1u16.to_le_bytes());
        data.push(2);
        data.extend_from_slice(&3u16.to_le_bytes());
        data.push(1);
        push_vec3(&mut data, [1.0, 2.0, 3.0]);
        push_vec3(&mut data, [4.0, 5.0, 6.0]);
        push_vec3(&mut data, [0.1, 0.2, 0.3]);
        for value in [10.0f32, 0.4, 0.5, 0.6, 0.7] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.push(2);

        data.extend_from_slice(&1u32.to_le_bytes()); // joints
        push_fixed(&mut data, b"joint", 20);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        for values in [
            [1.0, 1.1, 1.2],
            [2.0, 2.1, 2.2],
            [-1.0, -1.1, -1.2],
            [1.0, 1.1, 1.2],
            [-0.1, -0.2, -0.3],
            [0.1, 0.2, 0.3],
            [3.0, 3.1, 3.2],
            [4.0, 4.1, 4.2],
        ] {
            push_vec3(&mut data, values);
        }

        let parsed = parse_pmd_model(&data).unwrap();
        let exported = export_pmd_model(&parsed);
        let reparsed = parse_pmd_model(&exported).unwrap();

        assert_eq!(exported, data);
        assert_eq!(reparsed.rigid_bodies[0].mode, "dynamicBone");
        assert_eq!(reparsed.joints[0].spring_rotation, [4.0, 4.1, 4.2]);
    }

    #[test]
    fn exports_pmd_edge_enabled_byte_polarity() {
        let mut data = minimal_pmd_with_ik();
        insert_vertex_and_indices(&mut data, &sample_vertex(0), &[0, 0, 0]);
        insert_material(&mut data, &sample_material(1));

        let parsed = parse_pmd_model(&data).unwrap();
        let exported = export_pmd_model(&parsed);
        let reparsed = parse_pmd_model(&exported).unwrap();

        assert_eq!(exported, data);
        assert!(reparsed.geometry.vertices[0].edge_enabled);
        assert!(reparsed.materials[0].edge_enabled);
    }

    #[test]
    fn pmd_fixed_text_fallback_does_not_split_shift_jis_character() {
        let mut out = Vec::new();

        write_fixed_text(&mut out, "abcd\u{3042}", &[], 5);

        assert_eq!(out, vec![b'a', b'b', b'c', b'd', 0]);
        assert_eq!(decode_sjis_fixed(&out), "abcd");
    }

    #[test]
    fn parses_pmd_morph_vertex_offsets() {
        let mut data = minimal_pmd_with_ik();
        let morph_tail_len = 2 + 20 + 4 + 1;
        data.truncate(data.len() - morph_tail_len);
        data.extend_from_slice(&1u16.to_le_bytes());
        push_fixed(&mut data, b"smile", 20);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.push(1);
        data.extend_from_slice(&7u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());

        let parsed = parse_pmd_model(&data).unwrap();

        assert_eq!(parsed.morphs.len(), 1);
        assert_eq!(parsed.morphs[0].name, "smile");
        assert_eq!(parsed.morphs[0].vertex_count, 1);
        assert_eq!(parsed.morphs[0].vertex_offsets[0].vertex_index, 7);
        assert_eq!(parsed.morphs[0].vertex_offsets[0].position, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn parses_pmd_geometry_vertices_and_indices() {
        let mut data = minimal_pmd_with_ik();
        let mut vertex = Vec::new();
        for value in [1.0f32, 2.0, 3.0] {
            vertex.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0.0f32, 1.0, 0.0] {
            vertex.extend_from_slice(&value.to_le_bytes());
        }
        for value in [0.25f32, 0.75] {
            vertex.extend_from_slice(&value.to_le_bytes());
        }
        vertex.extend_from_slice(&0u16.to_le_bytes());
        vertex.extend_from_slice(&1u16.to_le_bytes());
        vertex.push(80);
        vertex.push(0);
        insert_vertex_and_indices(&mut data, &vertex, &[0, 0, 0]);

        let parsed = parse_pmd_model(&data).unwrap();

        assert_eq!(parsed.geometry.vertices.len(), 1);
        assert_eq!(parsed.geometry.vertices[0].position, [1.0, 2.0, 3.0]);
        assert_eq!(parsed.geometry.vertices[0].normal, [0.0, 1.0, 0.0]);
        assert_eq!(parsed.geometry.vertices[0].uv, [0.25, 0.75]);
        assert_eq!(parsed.geometry.vertices[0].bone_indices, [0, 1]);
        assert_eq!(parsed.geometry.vertices[0].bone_weight, 80);
        assert!(parsed.geometry.vertices[0].edge_enabled);
        assert_eq!(parsed.geometry.indices, vec![0, 0, 0]);
    }

    #[test]
    fn parses_pmd_physics_tail_details() {
        let mut data = minimal_pmd_with_ik();
        data.push(0); // morph display count
        data.push(0); // bone display frame name count
        data.extend_from_slice(&0u32.to_le_bytes()); // bone display count
        data.push(0); // english block disabled

        data.extend_from_slice(&1u32.to_le_bytes()); // rigid bodies
        push_fixed(&mut data, b"body", 20);
        data.extend_from_slice(&1u16.to_le_bytes());
        data.push(2);
        data.extend_from_slice(&3u16.to_le_bytes());
        data.push(1);
        push_vec3(&mut data, [1.0, 2.0, 3.0]);
        push_vec3(&mut data, [4.0, 5.0, 6.0]);
        push_vec3(&mut data, [0.1, 0.2, 0.3]);
        for value in [10.0f32, 0.4, 0.5, 0.6, 0.7] {
            data.extend_from_slice(&value.to_le_bytes());
        }
        data.push(2);

        data.extend_from_slice(&1u32.to_le_bytes()); // joints
        push_fixed(&mut data, b"joint", 20);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        for values in [
            [1.0, 1.1, 1.2],
            [2.0, 2.1, 2.2],
            [-1.0, -1.1, -1.2],
            [1.0, 1.1, 1.2],
            [-0.1, -0.2, -0.3],
            [0.1, 0.2, 0.3],
            [3.0, 3.1, 3.2],
            [4.0, 4.1, 4.2],
        ] {
            push_vec3(&mut data, values);
        }

        let parsed = parse_pmd_model(&data).unwrap();

        assert_eq!(parsed.rigid_bodies.len(), 1);
        assert_eq!(parsed.rigid_bodies[0].shape, "box");
        assert_eq!(parsed.rigid_bodies[0].size, [1.0, 2.0, 3.0]);
        assert_eq!(parsed.rigid_bodies[0].position, [4.0, 5.0, 6.0]);
        assert_eq!(parsed.rigid_bodies[0].mode, "dynamicBone");
        assert_eq!(parsed.joints.len(), 1);
        assert_eq!(parsed.joints[0].position, [1.0, 1.1, 1.2]);
        assert_eq!(parsed.joints[0].spring_rotation, [4.0, 4.1, 4.2]);
    }

    #[test]
    fn parses_pmd_toon_texture_block() {
        let mut data = minimal_pmd_with_ik();
        data.push(0); // morph display count
        data.push(0); // bone display frame name count
        data.extend_from_slice(&0u32.to_le_bytes()); // bone display count
        data.push(0); // english block disabled
        for i in 0..10 {
            push_fixed(&mut data, format!("toon{i}.bmp").as_bytes(), 100);
        }

        let parsed = parse_pmd_model(&data).unwrap();

        assert_eq!(parsed.toon_textures.len(), 10);
        assert_eq!(parsed.toon_textures[0], "toon0.bmp");
        assert_eq!(parsed.toon_textures[9], "toon9.bmp");
    }

    #[test]
    fn preserves_pmd_fixed_text_raw_bytes() {
        let mut data = minimal_pmd_with_ik();
        data.push(0); // morph display count
        data.push(1); // bone display frame name count
        push_fixed(&mut data, b"frame", 50);
        data.extend_from_slice(&0u32.to_le_bytes()); // bone display count
        data.push(1); // english block enabled
        push_fixed(&mut data, b"model-en", 20);
        push_fixed(&mut data, b"comment-en", 256);
        push_fixed(&mut data, b"root-en", 20);
        push_fixed(&mut data, b"legIK-en", 20);
        push_fixed(&mut data, b"frame-en", 50);
        for i in 0..10 {
            push_fixed(&mut data, format!("toon{i}.bmp").as_bytes(), 100);
        }

        let parsed = parse_pmd_model(&data).unwrap();

        assert_eq!(parsed.metadata.name, "model");
        assert_eq!(parsed.metadata.name_bytes.len(), 20);
        assert_eq!(&parsed.metadata.name_bytes[..5], b"model");
        assert_eq!(parsed.metadata.english_name, "model-en");
        assert_eq!(parsed.metadata.english_name_bytes.len(), 20);
        assert_eq!(parsed.skeleton.bones[1].name, "legIK");
        assert_eq!(parsed.skeleton.bones[1].name_bytes.len(), 20);
        assert_eq!(parsed.skeleton.bones[1].english_name, "legIK-en");
        assert_eq!(parsed.display_frames[0].name, "frame");
        assert_eq!(parsed.display_frames[0].name_bytes.len(), 50);
        assert_eq!(parsed.display_frames[0].english_name, "frame-en");
        assert_eq!(parsed.toon_texture_bytes.len(), 10);
        assert_eq!(parsed.toon_texture_bytes[0].len(), 100);
    }

    #[test]
    fn serializes_pmd_ik_fields_as_camel_case() {
        let ik = PmdParsedIk {
            bone_index: 1,
            target_bone_index: 2,
            loop_count: 3,
            limit_angle: 4.0,
            links: vec![5],
        };

        let value = serde_json::to_value(ik).unwrap();

        assert_eq!(value["boneIndex"], 1);
        assert_eq!(value["targetBoneIndex"], 2);
        assert_eq!(value["loopCount"], 3);
        assert_eq!(value["limitAngle"], 4.0);
        assert!(value.get("bone_index").is_none());
    }

    #[test]
    fn imports_pmd_vertex_morph_offsets_through_base_morph_indices() {
        let mut data = minimal_pmd_with_ik();
        insert_empty_vertices(&mut data, 8);
        let morph_tail_len = 2 + 20 + 4 + 1;
        data.truncate(data.len() - morph_tail_len);
        data.extend_from_slice(&2u16.to_le_bytes());

        push_fixed(&mut data, b"base", 20);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.push(0);
        data.extend_from_slice(&7u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());

        push_fixed(&mut data, b"smile", 20);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.push(1);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());

        let imported = import_pmd_runtime(&data).unwrap();

        assert_eq!(imported.model.morph_count(), 2);
        assert_eq!(
            imported.model.vertex_morph_spans()[1],
            MorphOffsetSpan { start: 0, count: 1 }
        );
        assert_eq!(
            imported.model.vertex_morph_offsets()[0],
            VertexMorphOffset {
                vertex_index: 7,
                position_offset: Vec3A::new(1.0, 2.0, 3.0),
            }
        );
        assert_eq!(
            imported.morph_name_to_index.get(b"smile".as_slice()),
            Some(&MorphIndex(1))
        );
        assert_eq!(imported.diagnostics[0].code, "PMD_RUNTIME_PARTIAL");
    }

    #[test]
    fn skips_pmd_vertex_morph_offsets_outside_vertex_count() {
        let mut data = minimal_pmd_with_ik();
        insert_empty_vertices(&mut data, 1);
        let morph_tail_len = 2 + 20 + 4 + 1;
        data.truncate(data.len() - morph_tail_len);
        data.extend_from_slice(&2u16.to_le_bytes());

        push_fixed(&mut data, b"base", 20);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.push(0);
        data.extend_from_slice(&7u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());

        push_fixed(&mut data, b"smile", 20);
        data.extend_from_slice(&1u32.to_le_bytes());
        data.push(1);
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());

        let imported = import_pmd_runtime(&data).unwrap();

        assert!(imported.model.vertex_morph_offsets().is_empty());
        assert_eq!(
            imported.diagnostics[0].code,
            "PMD_VERTEX_MORPH_VERTEX_INDEX_OUT_OF_RANGE"
        );
        assert_eq!(imported.diagnostics[1].code, "PMD_RUNTIME_PARTIAL");
    }
}
