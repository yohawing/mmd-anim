#![cfg(feature = "fbx")]

use std::io::{Cursor, Seek, Write};

use fbxcel::{
    low::{v7400::ArrayAttributeEncoding, FbxVersion},
    writer::v7400::binary::{AttributesWriter, FbxFooter, Writer},
};

use crate::pmx::{PmxParsedBone, PmxParsedLocalAxis, PmxParsedMaterial, PmxParsedModel};

const ROOT_NODE_ID: i64 = 0;
const DOCUMENT_ID: i64 = 100;
const MODEL_ID: i64 = 200;
const GEOMETRY_ID: i64 = 300;
const MATERIAL_ID_BASE: i64 = 1000;
const BONE_MODEL_ID_BASE: i64 = 10_000;
const BONE_ATTR_ID_BASE: i64 = 20_000;
const SKIN_ID: i64 = 30_000;
const CLUSTER_ID_BASE: i64 = 40_000;
const POSE_ID: i64 = 50_000;

#[derive(Debug, Clone)]
pub struct FbxExportOptions {
    pub model_name: String,
    pub flip_z: bool,
}

impl Default for FbxExportOptions {
    fn default() -> Self {
        Self {
            model_name: "PMX Model".to_owned(),
            flip_z: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FbxExportError {
    #[error("PMX position buffer length must be divisible by 3, got {0}")]
    InvalidPositionBuffer(usize),
    #[error("PMX normal buffer length must be divisible by 3, got {0}")]
    InvalidNormalBuffer(usize),
    #[error("PMX UV buffer length must be divisible by 2, got {0}")]
    InvalidUvBuffer(usize),
    #[error("PMX index buffer length must be divisible by 3, got {0}")]
    InvalidIndexBuffer(usize),
    #[error("PMX index {index} references missing vertex {vertex} (vertex count {vertex_count})")]
    IndexOutOfRange {
        index: usize,
        vertex: u32,
        vertex_count: usize,
    },
    #[error("FBX writer error: {0}")]
    Writer(#[from] fbxcel::writer::v7400::binary::Error),
}

pub fn export_pmx_fbx_binary(
    model: &PmxParsedModel,
    options: &FbxExportOptions,
) -> Result<Vec<u8>, FbxExportError> {
    let mesh = MeshData::from_pmx(model, options)?;
    let sink = Cursor::new(Vec::new());
    let mut writer = Writer::new(sink, FbxVersion::V7_4)?;

    write_fbx_header_extension(&mut writer)?;
    write_top_level_fields(&mut writer)?;
    write_global_settings(&mut writer)?;
    write_documents(&mut writer)?;
    write_references(&mut writer)?;
    write_definitions(
        &mut writer,
        model.materials.len(),
        model.skeleton.bones.len(),
    )?;
    write_objects(&mut writer, model, options, &mesh)?;
    write_connections(&mut writer, model.materials.len(), &model.skeleton.bones)?;

    let footer_code: [u8; 16] = [
        0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26,
        0x7e,
    ];
    let footer = FbxFooter {
        unknown1: Some(&footer_code),
        ..Default::default()
    };
    let sink = writer.finalize_and_flush(&footer)?;
    Ok(sink.into_inner())
}

struct MeshData {
    vertices: Vec<f64>,
    normals: Vec<f64>,
    uvs: Vec<f64>,
    polygon_vertex_indices: Vec<i32>,
    polygon_uv_indices: Vec<i32>,
    polygon_material_indices: Vec<i32>,
}

impl MeshData {
    fn from_pmx(
        model: &PmxParsedModel,
        options: &FbxExportOptions,
    ) -> Result<Self, FbxExportError> {
        let vertex_count = model.geometry.positions.len() / 3;
        if model.geometry.positions.len() % 3 != 0 {
            return Err(FbxExportError::InvalidPositionBuffer(
                model.geometry.positions.len(),
            ));
        }
        if model.geometry.normals.len() % 3 != 0 {
            return Err(FbxExportError::InvalidNormalBuffer(
                model.geometry.normals.len(),
            ));
        }
        if model.geometry.uvs.len() % 2 != 0 {
            return Err(FbxExportError::InvalidUvBuffer(model.geometry.uvs.len()));
        }
        if model.geometry.indices.len() % 3 != 0 {
            return Err(FbxExportError::InvalidIndexBuffer(
                model.geometry.indices.len(),
            ));
        }

        let z_sign = if options.flip_z { -1.0 } else { 1.0 };
        let mut vertices = Vec::with_capacity(model.geometry.positions.len());
        for position in model.geometry.positions.chunks_exact(3) {
            vertices.push(position[0] as f64);
            vertices.push(position[1] as f64);
            vertices.push(position[2] as f64 * z_sign);
        }

        let mut normals = Vec::with_capacity(model.geometry.normals.len());
        for normal in model.geometry.normals.chunks_exact(3) {
            normals.push(normal[0] as f64);
            normals.push(normal[1] as f64);
            normals.push(normal[2] as f64 * z_sign);
        }

        let mut uvs = Vec::with_capacity(model.geometry.uvs.len());
        for uv in model.geometry.uvs.chunks_exact(2) {
            uvs.push(uv[0] as f64);
            uvs.push((1.0 - uv[1]) as f64);
        }

        let mut polygon_vertex_indices = Vec::with_capacity(model.geometry.indices.len());
        let mut polygon_uv_indices = Vec::with_capacity(model.geometry.indices.len());
        let mut polygon_material_indices =
            Vec::with_capacity(model.geometry.indices.len().saturating_div(3));
        for (triangle_index, triangle) in model.geometry.indices.chunks_exact(3).enumerate() {
            for (local_index, vertex_index) in triangle_indices_for_handedness(triangle, options)
                .into_iter()
                .enumerate()
            {
                if vertex_index as usize >= vertex_count {
                    return Err(FbxExportError::IndexOutOfRange {
                        index: triangle_index * 3 + local_index,
                        vertex: vertex_index,
                        vertex_count,
                    });
                }
                let raw = vertex_index as i32;
                if local_index == 2 {
                    polygon_vertex_indices.push(-raw - 1);
                } else {
                    polygon_vertex_indices.push(raw);
                }
                polygon_uv_indices.push(raw);
            }
            polygon_material_indices.push(material_index_for_triangle(model, triangle_index));
        }

        Ok(Self {
            vertices,
            normals,
            uvs,
            polygon_vertex_indices,
            polygon_uv_indices,
            polygon_material_indices,
        })
    }
}

fn triangle_indices_for_handedness(triangle: &[u32], options: &FbxExportOptions) -> [u32; 3] {
    if options.flip_z {
        [triangle[0], triangle[2], triangle[1]]
    } else {
        [triangle[0], triangle[1], triangle[2]]
    }
}

fn material_index_for_triangle(model: &PmxParsedModel, triangle_index: usize) -> i32 {
    let index_start = triangle_index * 3;
    model
        .geometry
        .material_groups
        .iter()
        .find(|group| index_start >= group.start && index_start < group.start + group.count)
        .map(|group| group.material_index as i32)
        .unwrap_or(0)
}

fn write_fbx_header_extension<W: Write + Seek>(
    writer: &mut Writer<W>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "FBXHeaderExtension", |_| Ok(()))?;
    write_i32_node(writer, "FBXHeaderVersion", 1003)?;
    write_i32_node(writer, "FBXVersion", 7400)?;
    write_i32_node(writer, "EncryptionType", 0)?;
    begin_node(writer, "CreationTimeStamp", |_| Ok(()))?;
    write_i32_node(writer, "Version", 1000)?;
    write_i32_node(writer, "Year", 2026)?;
    write_i32_node(writer, "Month", 6)?;
    write_i32_node(writer, "Day", 25)?;
    write_i32_node(writer, "Hour", 0)?;
    write_i32_node(writer, "Minute", 0)?;
    write_i32_node(writer, "Second", 0)?;
    write_i32_node(writer, "Millisecond", 0)?;
    writer.close_node()?;
    write_string_node(writer, "Creator", "mmd-anim fbx exporter spike")?;
    writer.close_node()?;
    Ok(())
}

fn write_top_level_fields<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "FileId", |attrs| {
        attrs.append_binary_direct(&[
            0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b,
            0xfc, 0xf1,
        ])?;
        Ok(())
    })?;
    writer.close_node()?;
    write_string_node(writer, "CreationTime", "1970-01-01 10:00:00:000")?;
    write_string_node(writer, "Creator", "mmd-anim fbx exporter")?;
    Ok(())
}

fn write_global_settings<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "GlobalSettings", |_| Ok(()))?;
    write_i32_node(writer, "Version", 1000)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_i32(writer, "UpAxis", "int", "Integer", "", 1)?;
    write_property_i32(writer, "UpAxisSign", "int", "Integer", "", 1)?;
    write_property_i32(writer, "FrontAxis", "int", "Integer", "", 2)?;
    write_property_i32(writer, "FrontAxisSign", "int", "Integer", "", 1)?;
    write_property_i32(writer, "CoordAxis", "int", "Integer", "", 0)?;
    write_property_i32(writer, "CoordAxisSign", "int", "Integer", "", 1)?;
    write_property_f64(writer, "UnitScaleFactor", "double", "Number", "", 1.0)?;
    write_property_f64(
        writer,
        "OriginalUnitScaleFactor",
        "double",
        "Number",
        "",
        1.0,
    )?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_documents<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "Documents", |_| Ok(()))?;
    write_i32_node(writer, "Count", 1)?;
    begin_node(writer, "Document", |attrs| {
        attrs.append_i64(DOCUMENT_ID)?;
        attrs.append_string_direct("")?;
        attrs.append_string_direct("Scene")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_compound(writer, "SourceObject", "object", "", "")?;
    write_property_string(writer, "ActiveAnimStackName", "KString", "", "")?;
    writer.close_node()?;
    write_i64_node(writer, "RootNode", ROOT_NODE_ID)?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_references<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "References", |_| Ok(()))?;
    writer.close_node()?;
    Ok(())
}

fn write_definitions<W: Write + Seek>(
    writer: &mut Writer<W>,
    material_count: usize,
    bone_count: usize,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Definitions", |_| Ok(()))?;
    write_i32_node(writer, "Version", 100)?;
    write_i32_node(
        writer,
        "Count",
        (5 + material_count + bone_count * 3) as i32,
    )?;

    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("GlobalSettings")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    writer.close_node()?;

    write_model_object_type(writer, bone_count as i32)?;
    write_geometry_object_type(writer)?;
    write_material_object_type(writer, material_count as i32)?;
    write_node_attribute_object_type(writer, bone_count as i32)?;
    write_deformer_object_type(writer, bone_count as i32)?;
    write_pose_object_type(writer)?;
    writer.close_node()?;
    Ok(())
}

fn write_model_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    bone_count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Model")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1 + bone_count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxNode")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_vec3(
        writer,
        "Lcl Translation",
        "Lcl Translation",
        "",
        "A",
        [0.0; 3],
    )?;
    write_property_vec3(writer, "Lcl Rotation", "Lcl Rotation", "", "A", [0.0; 3])?;
    write_property_vec3(writer, "Lcl Scaling", "Lcl Scaling", "", "A", [1.0; 3])?;
    write_property_f64(writer, "Visibility", "Visibility", "", "A", 1.0)?;
    write_property_i32(writer, "Visibility Inheritance", "bool", "", "", 1)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_node_attribute_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("NodeAttribute")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxSkeleton")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_color(writer, "Color", "ColorRGB", "Color", "", [0.8, 0.8, 0.8])?;
    write_property_f64(writer, "Size", "double", "Number", "", 33.333333333333336)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_geometry_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Geometry")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxMesh")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_color(writer, "Color", "ColorRGB", "Color", "", [0.8, 0.8, 0.8])?;
    write_property_vec3(writer, "BBoxMin", "Vector3D", "Vector", "", [0.0; 3])?;
    write_property_vec3(writer, "BBoxMax", "Vector3D", "Vector", "", [0.0; 3])?;
    write_property_i32(writer, "Primary Visibility", "bool", "", "", 1)?;
    write_property_i32(writer, "Casts Shadows", "bool", "", "", 1)?;
    write_property_i32(writer, "Receive Shadows", "bool", "", "", 1)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_material_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Material")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", count)?;
    begin_node(writer, "PropertyTemplate", |attrs| {
        attrs.append_string_direct("FbxSurfacePhong")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_string(writer, "ShadingModel", "KString", "", "Phong")?;
    write_property_i32(writer, "MultiLayer", "bool", "", "", 0)?;
    write_property_color(writer, "EmissiveColor", "Color", "", "A", [0.0, 0.0, 0.0])?;
    write_property_f64(writer, "EmissiveFactor", "Number", "", "A", 1.0)?;
    write_property_color(writer, "AmbientColor", "Color", "", "A", [0.2, 0.2, 0.2])?;
    write_property_f64(writer, "AmbientFactor", "Number", "", "A", 1.0)?;
    write_property_color(writer, "DiffuseColor", "Color", "", "A", [0.8, 0.8, 0.8])?;
    write_property_f64(writer, "DiffuseFactor", "Number", "", "A", 1.0)?;
    write_property_color(
        writer,
        "TransparentColor",
        "Color",
        "",
        "A",
        [0.0, 0.0, 0.0],
    )?;
    write_property_f64(writer, "TransparencyFactor", "Number", "", "A", 0.0)?;
    write_property_f64(writer, "Opacity", "Number", "", "A", 1.0)?;
    write_property_color(writer, "SpecularColor", "Color", "", "A", [0.2, 0.2, 0.2])?;
    write_property_f64(writer, "SpecularFactor", "Number", "", "A", 1.0)?;
    write_property_f64(writer, "Shininess", "Number", "", "A", 20.0)?;
    write_property_f64(writer, "ShininessExponent", "Number", "", "A", 20.0)?;
    write_property_color(writer, "ReflectionColor", "Color", "", "A", [0.0, 0.0, 0.0])?;
    write_property_f64(writer, "ReflectionFactor", "Number", "", "A", 1.0)?;
    writer.close_node()?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_deformer_object_type<W: Write + Seek>(
    writer: &mut Writer<W>,
    bone_count: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Deformer")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1 + bone_count)?;
    writer.close_node()?;
    Ok(())
}

fn write_pose_object_type<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "ObjectType", |attrs| {
        attrs.append_string_direct("Pose")?;
        Ok(())
    })?;
    write_i32_node(writer, "Count", 1)?;
    writer.close_node()?;
    Ok(())
}

fn write_objects<W: Write + Seek>(
    writer: &mut Writer<W>,
    model: &PmxParsedModel,
    options: &FbxExportOptions,
    mesh: &MeshData,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Objects", |_| Ok(()))?;
    write_geometry(writer, mesh)?;
    write_model(writer, options)?;
    write_skeleton(writer, &model.skeleton.bones, options)?;
    write_skin_deformers(writer, model, options)?;
    write_bind_pose(writer, &model.skeleton.bones, options)?;
    for (index, material) in model.materials.iter().enumerate() {
        write_material(writer, material, MATERIAL_ID_BASE + index as i64)?;
    }
    writer.close_node()?;
    Ok(())
}

fn write_geometry<W: Write + Seek>(
    writer: &mut Writer<W>,
    mesh: &MeshData,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Geometry", |attrs| {
        attrs.append_i64(GEOMETRY_ID)?;
        attrs.append_string_direct("\x00\x01Geometry")?;
        attrs.append_string_direct("Mesh")?;
        Ok(())
    })?;
    write_i32_node(writer, "GeometryVersion", 124)?;
    write_arr_f64_node(writer, "Vertices", &mesh.vertices)?;
    write_arr_i32_node(writer, "PolygonVertexIndex", &mesh.polygon_vertex_indices)?;

    begin_node(writer, "LayerElementNormal", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_string_node(writer, "Name", "")?;
    write_string_node(writer, "MappingInformationType", "ByVertice")?;
    write_string_node(writer, "ReferenceInformationType", "Direct")?;
    write_arr_f64_node(writer, "Normals", &mesh.normals)?;
    writer.close_node()?;

    begin_node(writer, "LayerElementUV", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_string_node(writer, "Name", "UVSet")?;
    write_string_node(writer, "MappingInformationType", "ByPolygonVertex")?;
    write_string_node(writer, "ReferenceInformationType", "IndexToDirect")?;
    write_arr_f64_node(writer, "UV", &mesh.uvs)?;
    write_arr_i32_node(writer, "UVIndex", &mesh.polygon_uv_indices)?;
    writer.close_node()?;

    begin_node(writer, "LayerElementMaterial", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_string_node(writer, "Name", "")?;
    write_string_node(writer, "MappingInformationType", "ByPolygon")?;
    write_string_node(writer, "ReferenceInformationType", "IndexToDirect")?;
    write_arr_i32_node(writer, "Materials", &mesh.polygon_material_indices)?;
    writer.close_node()?;

    begin_node(writer, "Layer", |attrs| {
        attrs.append_i32(0)?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 100)?;
    write_layer_element(writer, "LayerElementNormal", 0)?;
    write_layer_element(writer, "LayerElementUV", 0)?;
    write_layer_element(writer, "LayerElementMaterial", 0)?;
    writer.close_node()?;

    writer.close_node()?;
    Ok(())
}

fn write_layer_element<W: Write + Seek>(
    writer: &mut Writer<W>,
    element_type: &str,
    typed_index: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "LayerElement", |_| Ok(()))?;
    write_string_node(writer, "Type", element_type)?;
    write_i32_node(writer, "TypedIndex", typed_index)?;
    writer.close_node()?;
    Ok(())
}

fn write_model<W: Write + Seek>(
    writer: &mut Writer<W>,
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01Model", options.model_name);
    begin_node(writer, "Model", |attrs| {
        attrs.append_i64(MODEL_ID)?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("Mesh")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 232)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_vec3(
        writer,
        "Lcl Translation",
        "Lcl Translation",
        "",
        "A",
        [0.0; 3],
    )?;
    write_property_vec3(writer, "Lcl Rotation", "Lcl Rotation", "", "A", [0.0; 3])?;
    write_property_vec3(writer, "Lcl Scaling", "Lcl Scaling", "", "A", [1.0; 3])?;
    write_property_i32(writer, "DefaultAttributeIndex", "int", "Integer", "", 0)?;
    writer.close_node()?;
    write_bool_node(writer, "Shading", true)?;
    write_string_node(writer, "Culling", "CullingOff")?;
    writer.close_node()?;
    Ok(())
}

fn write_skeleton<W: Write + Seek>(
    writer: &mut Writer<W>,
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    for (index, bone) in bones.iter().enumerate() {
        write_bone_node_attribute(writer, index, bone)?;
        write_bone_model(writer, index, bone, bones, options)?;
    }
    Ok(())
}

fn write_bone_node_attribute<W: Write + Seek>(
    writer: &mut Writer<W>,
    index: usize,
    bone: &PmxParsedBone,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01NodeAttribute", bone_name(bone));
    begin_node(writer, "NodeAttribute", |attrs| {
        attrs.append_i64(bone_attr_id(index))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("LimbNode")?;
        Ok(())
    })?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_f64(writer, "Size", "double", "Number", "", 33.333333333333336)?;
    writer.close_node()?;
    write_string_node(writer, "TypeFlags", "Skeleton")?;
    writer.close_node()?;
    Ok(())
}

fn write_bone_model<W: Write + Seek>(
    writer: &mut Writer<W>,
    index: usize,
    bone: &PmxParsedBone,
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    let name = format!("{}\x00\x01Model", bone_name(bone));
    begin_node(writer, "Model", |attrs| {
        attrs.append_i64(bone_model_id(index))?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("LimbNode")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 232)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_i32(writer, "RotationActive", "bool", "", "", 1)?;
    if let Some(local_axis) = bone.local_axis.as_ref() {
        write_property_vec3(
            writer,
            "PreRotation",
            "Vector3D",
            "Vector",
            "",
            prerotation_from_local_axis(local_axis, options),
        )?;
    }
    write_property_i32(writer, "InheritType", "enum", "", "", 1)?;
    write_property_vec3(writer, "ScalingMax", "Vector3D", "Vector", "", [0.0; 3])?;
    write_property_i32(writer, "DefaultAttributeIndex", "int", "Integer", "", 0)?;
    write_property_vec3(
        writer,
        "Lcl Translation",
        "Lcl Translation",
        "",
        "A+",
        bone_local_translation(bone, bones, options),
    )?;
    writer.close_node()?;
    write_bool_node(writer, "Shading", true)?;
    write_string_node(writer, "Culling", "CullingOff")?;
    writer.close_node()?;
    Ok(())
}

fn bone_local_translation(
    bone: &PmxParsedBone,
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> [f64; 3] {
    let position = converted_bone_position(bone, options);
    if bone.parent_index >= 0 {
        if let Some(parent) = bones.get(bone.parent_index as usize) {
            let parent_position = converted_bone_position(parent, options);
            return [
                position[0] - parent_position[0],
                position[1] - parent_position[1],
                position[2] - parent_position[2],
            ];
        }
    }
    position
}

fn converted_bone_position(bone: &PmxParsedBone, options: &FbxExportOptions) -> [f64; 3] {
    let z_sign = if options.flip_z { -1.0 } else { 1.0 };
    [
        bone.position[0] as f64,
        bone.position[1] as f64,
        bone.position[2] as f64 * z_sign,
    ]
}

fn prerotation_from_local_axis(
    local_axis: &PmxParsedLocalAxis,
    options: &FbxExportOptions,
) -> [f64; 3] {
    let Some(lx) = normalize3([
        local_axis.x[0] as f64,
        local_axis.x[1] as f64,
        local_axis.x[2] as f64,
    ]) else {
        return [0.0; 3];
    };
    let Some(lz_initial) = normalize3([
        local_axis.z[0] as f64,
        local_axis.z[1] as f64,
        local_axis.z[2] as f64,
    ]) else {
        return [0.0; 3];
    };
    let Some(ly) = normalize3(cross3(lz_initial, lx)) else {
        return [0.0; 3];
    };
    let lz = cross3(lx, ly);

    let r_pmx = [
        [lx[0], ly[0], lz[0]],
        [lx[1], ly[1], lz[1]],
        [lx[2], ly[2], lz[2]],
    ];
    let r_fbx = if options.flip_z {
        [
            [r_pmx[0][0], r_pmx[0][1], -r_pmx[0][2]],
            [r_pmx[1][0], r_pmx[1][1], -r_pmx[1][2]],
            [-r_pmx[2][0], -r_pmx[2][1], r_pmx[2][2]],
        ]
    } else {
        r_pmx
    };

    let beta = r_fbx[0][2].clamp(-1.0, 1.0).asin();
    let (alpha, gamma) = if beta.cos().abs() < 1e-6 {
        (r_fbx[1][0].atan2(r_fbx[1][1]), 0.0)
    } else {
        (
            (-r_fbx[1][2]).atan2(r_fbx[2][2]),
            (-r_fbx[0][1]).atan2(r_fbx[0][0]),
        )
    };

    [alpha.to_degrees(), beta.to_degrees(), gamma.to_degrees()]
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize3(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = vec3_length(v);
    if length == 0.0 {
        None
    } else {
        Some([v[0] / length, v[1] / length, v[2] / length])
    }
}

fn vec3_length(v: [f64; 3]) -> f64 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn identity_matrix() -> [f64; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn bone_world_transform(bone: &PmxParsedBone, options: &FbxExportOptions) -> [f64; 16] {
    let position = converted_bone_position(bone, options);
    [
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        position[0],
        position[1],
        position[2],
        1.0,
    ]
}

fn bone_name(bone: &PmxParsedBone) -> &str {
    if bone.english_name.is_empty() {
        &bone.name
    } else {
        &bone.english_name
    }
}

fn write_material<W: Write + Seek>(
    writer: &mut Writer<W>,
    material: &PmxParsedMaterial,
    id: i64,
) -> Result<(), FbxExportError> {
    let material_name = if material.name.is_empty() {
        material.english_name.as_str()
    } else {
        material.name.as_str()
    };
    let name = format!("{}\x00\x01Material", material_name);
    begin_node(writer, "Material", |attrs| {
        attrs.append_i64(id)?;
        attrs.append_string_direct(&name)?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 102)?;
    write_string_node(writer, "ShadingModel", "phong")?;
    write_i32_node(writer, "MultiLayer", 0)?;
    begin_node(writer, "Properties70", |_| Ok(()))?;
    write_property_color(
        writer,
        "DiffuseColor",
        "Color",
        "",
        "A",
        [
            material.diffuse[0] as f64,
            material.diffuse[1] as f64,
            material.diffuse[2] as f64,
        ],
    )?;
    write_property_f64(writer, "DiffuseFactor", "double", "Number", "", 1.0)?;
    write_property_color(
        writer,
        "SpecularColor",
        "Color",
        "",
        "A",
        [
            material.specular[0] as f64,
            material.specular[1] as f64,
            material.specular[2] as f64,
        ],
    )?;
    write_property_f64(
        writer,
        "SpecularFactor",
        "double",
        "Number",
        "",
        material.specular_power as f64,
    )?;
    write_property_color(
        writer,
        "AmbientColor",
        "Color",
        "",
        "A",
        [
            material.ambient[0] as f64,
            material.ambient[1] as f64,
            material.ambient[2] as f64,
        ],
    )?;
    write_property_f64(
        writer,
        "TransparencyFactor",
        "double",
        "Number",
        "",
        (1.0 - material.diffuse[3]).clamp(0.0, 1.0) as f64,
    )?;
    writer.close_node()?;
    writer.close_node()?;
    Ok(())
}

fn write_skin_deformers<W: Write + Seek>(
    writer: &mut Writer<W>,
    model: &PmxParsedModel,
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    write_skin_deformer(writer)?;
    let vertex_count = model.geometry.positions.len() / 3;
    for (index, bone) in model.skeleton.bones.iter().enumerate() {
        write_cluster_deformer(
            writer,
            index,
            bone,
            &model.geometry.skin_indices,
            &model.geometry.skin_weights,
            vertex_count,
            options,
        )?;
    }
    Ok(())
}

fn write_skin_deformer<W: Write + Seek>(writer: &mut Writer<W>) -> Result<(), FbxExportError> {
    begin_node(writer, "Deformer", |attrs| {
        attrs.append_i64(SKIN_ID)?;
        attrs.append_string_direct("\x00\x01Deformer")?;
        attrs.append_string_direct("Skin")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 101)?;
    write_f64_node(writer, "Link_DeformAcuracy", 50.0)?;
    write_string_node(writer, "SkinningType", "Linear")?;
    writer.close_node()?;
    Ok(())
}

fn write_cluster_deformer<W: Write + Seek>(
    writer: &mut Writer<W>,
    index: usize,
    bone: &PmxParsedBone,
    skin_indices: &[u32],
    skin_weights: &[f32],
    vertex_count: usize,
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    let (indices, weights) =
        collect_bone_skin_data(index, skin_indices, skin_weights, vertex_count);
    begin_node(writer, "Deformer", |attrs| {
        attrs.append_i64(cluster_id(index))?;
        attrs.append_string_direct("\x00\x01SubDeformer")?;
        attrs.append_string_direct("Cluster")?;
        Ok(())
    })?;
    write_i32_node(writer, "Version", 100)?;
    begin_node(writer, "UserData", |attrs| {
        attrs.append_string_direct("")?;
        attrs.append_string_direct("")?;
        Ok(())
    })?;
    writer.close_node()?;
    if !indices.is_empty() {
        write_arr_i32_node(writer, "Indexes", &indices)?;
        write_arr_f64_node(writer, "Weights", &weights)?;
    }
    write_arr_f64_node(writer, "Transform", &identity_matrix())?;
    write_arr_f64_node(
        writer,
        "TransformLink",
        &bone_world_transform(bone, options),
    )?;
    writer.close_node()?;
    Ok(())
}

fn collect_bone_skin_data(
    bone_index: usize,
    skin_indices: &[u32],
    skin_weights: &[f32],
    vertex_count: usize,
) -> (Vec<i32>, Vec<f64>) {
    let mut indices = Vec::new();
    let mut weights = Vec::new();
    for vertex_index in 0..vertex_count {
        for slot in 0..4 {
            let skin_offset = vertex_index * 4 + slot;
            if skin_indices[skin_offset] as usize == bone_index && skin_weights[skin_offset] > 0.0 {
                indices.push(vertex_index as i32);
                weights.push(skin_weights[skin_offset] as f64);
            }
        }
    }
    (indices, weights)
}

fn write_bind_pose<W: Write + Seek>(
    writer: &mut Writer<W>,
    bones: &[PmxParsedBone],
    options: &FbxExportOptions,
) -> Result<(), FbxExportError> {
    begin_node(writer, "Pose", |attrs| {
        attrs.append_i64(POSE_ID)?;
        attrs.append_string_direct("BindPose\x00\x01Pose")?;
        attrs.append_string_direct("BindPose")?;
        Ok(())
    })?;
    write_string_node(writer, "Type", "BindPose")?;
    write_i32_node(writer, "Version", 100)?;
    write_i32_node(writer, "NbPoseNodes", (bones.len() + 1) as i32)?;
    write_pose_node(writer, MODEL_ID, &identity_matrix())?;
    for (index, bone) in bones.iter().enumerate() {
        write_pose_node(
            writer,
            bone_model_id(index),
            &bone_world_transform(bone, options),
        )?;
    }
    writer.close_node()?;
    Ok(())
}

fn write_pose_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    node_id: i64,
    matrix: &[f64; 16],
) -> Result<(), FbxExportError> {
    begin_node(writer, "PoseNode", |_| Ok(()))?;
    write_i64_node(writer, "Node", node_id)?;
    write_arr_f64_node(writer, "Matrix", matrix)?;
    writer.close_node()?;
    Ok(())
}

fn write_connections<W: Write + Seek>(
    writer: &mut Writer<W>,
    material_count: usize,
    bones: &[PmxParsedBone],
) -> Result<(), FbxExportError> {
    begin_node(writer, "Connections", |_| Ok(()))?;
    write_oo_connection(writer, MODEL_ID, ROOT_NODE_ID)?;
    write_oo_connection(writer, GEOMETRY_ID, MODEL_ID)?;
    write_oo_connection(writer, SKIN_ID, GEOMETRY_ID)?;
    for index in 0..material_count {
        write_oo_connection(writer, MATERIAL_ID_BASE + index as i64, MODEL_ID)?;
    }
    for (index, bone) in bones.iter().enumerate() {
        let model_id = bone_model_id(index);
        write_oo_connection(writer, bone_attr_id(index), model_id)?;
        write_oo_connection(writer, cluster_id(index), SKIN_ID)?;
        write_oo_connection(writer, model_id, cluster_id(index))?;
        let parent_id = if bone.parent_index >= 0 {
            bone_model_id(bone.parent_index as usize)
        } else {
            ROOT_NODE_ID
        };
        write_oo_connection(writer, model_id, parent_id)?;
    }
    writer.close_node()?;
    Ok(())
}

fn bone_model_id(index: usize) -> i64 {
    BONE_MODEL_ID_BASE + index as i64
}

fn bone_attr_id(index: usize) -> i64 {
    BONE_ATTR_ID_BASE + index as i64
}

fn cluster_id(index: usize) -> i64 {
    CLUSTER_ID_BASE + index as i64
}

fn write_oo_connection<W: Write + Seek>(
    writer: &mut Writer<W>,
    child_id: i64,
    parent_id: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, "C", |attrs| {
        attrs.append_string_direct("OO")?;
        attrs.append_i64(child_id)?;
        attrs.append_i64(parent_id)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn begin_node<W, F>(
    writer: &mut Writer<W>,
    name: &str,
    append_attrs: F,
) -> Result<(), FbxExportError>
where
    W: Write + Seek,
    F: FnOnce(&mut AttributesWriter<'_, W>) -> Result<(), FbxExportError>,
{
    let mut attrs = writer.new_node(name)?;
    append_attrs(&mut attrs)?;
    drop(attrs);
    Ok(())
}

fn write_bool_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: bool,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_bool(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_i32_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_i32(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_i64_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: i64,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_i64(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_f64_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: f64,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_f64(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_string_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    value: &str,
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_string_direct(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_arr_i32_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    values: &[i32],
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_arr_i32_from_iter(None::<ArrayAttributeEncoding>, values.iter().copied())?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_arr_f64_node<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    values: &[f64],
) -> Result<(), FbxExportError> {
    begin_node(writer, name, |attrs| {
        attrs.append_arr_f64_from_iter(None::<ArrayAttributeEncoding>, values.iter().copied())?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_i32<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: i32,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        attrs.append_i32(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_f64<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: f64,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        attrs.append_f64(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_compound<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_string<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    value: &str,
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct("")?;
        attrs.append_string_direct(value)?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_vec3<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: [f64; 3],
) -> Result<(), FbxExportError> {
    begin_node(writer, "P", |attrs| {
        attrs.append_string_direct(name)?;
        attrs.append_string_direct(type_name)?;
        attrs.append_string_direct(label)?;
        attrs.append_string_direct(flags)?;
        attrs.append_f64(value[0])?;
        attrs.append_f64(value[1])?;
        attrs.append_f64(value[2])?;
        Ok(())
    })?;
    writer.close_node()?;
    Ok(())
}

fn write_property_color<W: Write + Seek>(
    writer: &mut Writer<W>,
    name: &str,
    type_name: &str,
    label: &str,
    flags: &str,
    value: [f64; 3],
) -> Result<(), FbxExportError> {
    write_property_vec3(writer, name, type_name, label, flags, value)
}
