use serde::{Deserialize, Serialize};

use crate::error::ImportError;
use crate::sjis::{decode_sjis, encode_sjis};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryParsedManifest {
    pub format: String,
    pub byte_length: usize,
    pub text: bool,
    pub header: String,
    pub mesh_count: usize,
    pub material_count: usize,
    pub mesh_summaries: Vec<AccessoryMeshSummary>,
    pub materials: Vec<AccessoryMaterial>,
    pub vac_settings: Option<AccessoryVacSettings>,
    pub texture_references: Vec<String>,
    pub diagnostics: Vec<AccessoryDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryMeshSummary {
    pub vertex_count: usize,
    pub face_count: usize,
    pub positions: Vec<[f32; 3]>,
    pub face_indices: Vec<Vec<u32>>,
    pub normals: Vec<[f32; 3]>,
    pub normal_face_indices: Vec<Vec<u32>>,
    pub texture_coordinates: Vec<[f32; 2]>,
    pub vertex_colors: Vec<AccessoryVertexColor>,
    pub material_indices: Vec<u32>,
    pub material_start_index: usize,
    pub material_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryVertexColor {
    pub vertex_index: u32,
    pub color: [f32; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryMaterial {
    pub name: Option<String>,
    pub face_color: Option<[f32; 4]>,
    pub power: Option<f32>,
    pub specular_color: Option<[f32; 3]>,
    pub emissive_color: Option<[f32; 3]>,
    pub texture_references: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessoryVacSettings {
    pub raw_lines: Vec<String>,
    pub x_file: Option<String>,
    pub scale: Option<f32>,
    pub position: Option<[f32; 3]>,
    pub rotation: Option<[f32; 3]>,
    pub numeric_values: Vec<f32>,
    pub attachment_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessoryDiagnostic {
    pub level: String,
    pub code: String,
    pub message: String,
}

pub fn export_accessory_manifest(manifest: &AccessoryParsedManifest) -> Vec<u8> {
    match manifest.format.as_str() {
        "vac" => export_vac_manifest(manifest),
        _ => export_x_manifest(manifest),
    }
}

pub fn parse_accessory_manifest(
    data: &[u8],
    file_name: Option<&str>,
) -> Result<AccessoryParsedManifest, ImportError> {
    let extension = file_name
        .and_then(|name| {
            name.rsplit_once('.')
                .map(|(_, ext)| ext.to_ascii_lowercase())
        })
        .unwrap_or_default();
    if extension == "vac" {
        return Ok(parse_vac_manifest(data));
    }
    if !data.starts_with(b"xof ") {
        return Err(ImportError::InvalidMagic { format: "X" });
    }
    let header = std::str::from_utf8(&data[..data.len().min(16)])
        .unwrap_or("")
        .trim_end_matches('\0')
        .to_owned();
    let text = header.contains("txt");
    let body = if text {
        String::from_utf8_lossy(data).into_owned()
    } else {
        decode_sjis(data)
    };
    let mesh_summaries = if text {
        parse_x_mesh_summaries(&body)
    } else {
        Vec::new()
    };
    let materials = if text {
        parse_x_materials(&body)
    } else {
        Vec::new()
    };
    let diagnostics = if text {
        text_x_diagnostics(&mesh_summaries, &materials)
    } else {
        vec![AccessoryDiagnostic {
            level: "warning".to_owned(),
            code: "X_BINARY_LAYOUT_NOT_EXPANDED".to_owned(),
            message: "Binary DirectX .x payload is identified but not fully decoded yet."
                .to_owned(),
        }]
    };
    Ok(AccessoryParsedManifest {
        format: "x".to_owned(),
        byte_length: data.len(),
        text,
        header,
        mesh_count: if text {
            count_x_blocks(&body, "Mesh")
        } else {
            0
        },
        material_count: if text {
            count_x_blocks(&body, "Material")
        } else {
            0
        },
        mesh_summaries,
        materials,
        vac_settings: None,
        texture_references: extract_texture_references(&body),
        diagnostics,
    })
}

fn text_x_diagnostics(
    mesh_summaries: &[AccessoryMeshSummary],
    materials: &[AccessoryMaterial],
) -> Vec<AccessoryDiagnostic> {
    let mut diagnostics = Vec::new();
    if mesh_summaries.len() > 1
        && !materials.is_empty()
        && mesh_summaries.iter().any(|mesh| mesh.material_count == 0)
    {
        diagnostics.push(AccessoryDiagnostic {
            level: "warning".to_owned(),
            code: "X_MULTI_MESH_MATERIAL_EXPORT_PARTIAL".to_owned(),
            message: "Text .x contains multiple Mesh blocks and Material blocks; parser keeps global material DTOs, but the current exporter slice does not preserve per-mesh material ownership.".to_owned(),
        });
    }
    diagnostics
}

fn parse_vac_manifest(data: &[u8]) -> AccessoryParsedManifest {
    let decoded = decode_sjis(data);
    let lines = decoded
        .lines()
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let texture_references = lines
        .iter()
        .filter(|line| line.to_ascii_lowercase().ends_with(".x"))
        .cloned()
        .collect::<Vec<_>>();
    AccessoryParsedManifest {
        format: "vac".to_owned(),
        byte_length: data.len(),
        text: true,
        header: lines.first().cloned().unwrap_or_default(),
        mesh_count: 0,
        material_count: 0,
        mesh_summaries: Vec::new(),
        materials: Vec::new(),
        vac_settings: Some(parse_vac_settings(&lines, &texture_references)),
        texture_references,
        diagnostics: vec![AccessoryDiagnostic {
            level: "warning".to_owned(),
            code: "VAC_ACCESSORY_WRAPPER".to_owned(),
            message: "VAC is parsed as an accessory wrapper manifest; the referenced .x file should be parsed separately.".to_owned(),
        }],
    }
}

fn export_x_manifest(manifest: &AccessoryParsedManifest) -> Vec<u8> {
    let mut text = if manifest.header.starts_with("xof ") {
        format!("{}\n", manifest.header)
    } else {
        String::from("xof 0303txt 0032\n")
    };
    if !manifest.mesh_summaries.is_empty() {
        export_x_meshes(&mut text, manifest);
        return text.into_bytes();
    }
    for texture in &manifest.texture_references {
        text.push_str("TextureFilename {\n  \"");
        text.push_str(&escape_x_string(texture));
        text.push_str("\";\n}\n");
    }
    text.into_bytes()
}

fn export_x_meshes(text: &mut String, manifest: &AccessoryParsedManifest) {
    for mesh in &manifest.mesh_summaries {
        text.push_str("Mesh {\n");
        text.push_str(&format!("  {};\n", mesh.positions.len()));
        for (index, position) in mesh.positions.iter().enumerate() {
            let suffix = if index + 1 == mesh.positions.len() {
                ";;"
            } else {
                ","
            };
            text.push_str(&format!(
                "  {};{};{};{}\n",
                format_x_float(position[0]),
                format_x_float(position[1]),
                format_x_float(position[2]),
                suffix
            ));
        }
        text.push_str(&format!("  {};\n", mesh.face_indices.len()));
        for (index, face) in mesh.face_indices.iter().enumerate() {
            let suffix = if index + 1 == mesh.face_indices.len() {
                ";;"
            } else {
                ","
            };
            let indices = face
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            text.push_str(&format!("  {};{};{}\n", face.len(), indices, suffix));
        }
        if !mesh.normals.is_empty() {
            export_x_mesh_normals(text, mesh);
        }
        if !mesh.texture_coordinates.is_empty() {
            export_x_mesh_texture_coords(text, mesh);
        }
        if !mesh.vertex_colors.is_empty() {
            export_x_mesh_vertex_colors(text, mesh);
        }
        if !mesh.material_indices.is_empty() || !manifest.materials.is_empty() {
            export_x_mesh_material_list(text, mesh, &manifest.materials);
        }
        text.push_str("}\n");
    }
    let material_textures = manifest
        .materials
        .iter()
        .flat_map(|material| material.texture_references.iter())
        .collect::<Vec<_>>();
    for texture in &manifest.texture_references {
        if material_textures
            .iter()
            .any(|material_texture| material_texture.eq_ignore_ascii_case(texture))
        {
            continue;
        }
        text.push_str("TextureFilename {\n  \"");
        text.push_str(&escape_x_string(texture));
        text.push_str("\";\n}\n");
    }
}

fn export_x_mesh_normals(text: &mut String, mesh: &AccessoryMeshSummary) {
    text.push_str("  MeshNormals {\n");
    text.push_str(&format!("    {};\n", mesh.normals.len()));
    for (index, normal) in mesh.normals.iter().enumerate() {
        let suffix = if index + 1 == mesh.normals.len() {
            ";;"
        } else {
            ","
        };
        text.push_str(&format!(
            "    {};{};{};{}\n",
            format_x_float(normal[0]),
            format_x_float(normal[1]),
            format_x_float(normal[2]),
            suffix
        ));
    }
    text.push_str(&format!("    {};\n", mesh.normal_face_indices.len()));
    for (index, face) in mesh.normal_face_indices.iter().enumerate() {
        let suffix = if index + 1 == mesh.normal_face_indices.len() {
            ";;"
        } else {
            ","
        };
        let indices = face
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        text.push_str(&format!("    {};{};{}\n", face.len(), indices, suffix));
    }
    text.push_str("  }\n");
}

fn export_x_mesh_texture_coords(text: &mut String, mesh: &AccessoryMeshSummary) {
    text.push_str("  MeshTextureCoords {\n");
    text.push_str(&format!("    {};\n", mesh.texture_coordinates.len()));
    for (index, uv) in mesh.texture_coordinates.iter().enumerate() {
        let suffix = if index + 1 == mesh.texture_coordinates.len() {
            ";;"
        } else {
            ","
        };
        text.push_str(&format!(
            "    {};{};{}\n",
            format_x_float(uv[0]),
            format_x_float(uv[1]),
            suffix
        ));
    }
    text.push_str("  }\n");
}

fn export_x_mesh_vertex_colors(text: &mut String, mesh: &AccessoryMeshSummary) {
    text.push_str("  MeshVertexColors {\n");
    text.push_str(&format!("    {};\n", mesh.vertex_colors.len()));
    for (index, color) in mesh.vertex_colors.iter().enumerate() {
        let suffix = if index + 1 == mesh.vertex_colors.len() {
            ";;"
        } else {
            ","
        };
        text.push_str(&format!(
            "    {};{};{};{};{};{}\n",
            color.vertex_index,
            format_x_float(color.color[0]),
            format_x_float(color.color[1]),
            format_x_float(color.color[2]),
            format_x_float(color.color[3]),
            suffix
        ));
    }
    text.push_str("  }\n");
}

fn export_x_mesh_material_list(
    text: &mut String,
    mesh: &AccessoryMeshSummary,
    materials: &[AccessoryMaterial],
) {
    let max_material_index = mesh.material_indices.iter().copied().max().unwrap_or(0) as usize;
    let material_start = mesh.material_start_index.min(materials.len());
    let owned_material_count = mesh
        .material_count
        .min(materials.len().saturating_sub(material_start));
    let material_count = owned_material_count.max(max_material_index + 1).max(1);
    text.push_str("  MeshMaterialList {\n");
    text.push_str(&format!("    {};\n", material_count));
    text.push_str(&format!("    {};\n", mesh.face_indices.len()));
    for face_index in 0..mesh.face_indices.len() {
        let material_index = mesh.material_indices.get(face_index).copied().unwrap_or(0);
        let suffix = if face_index + 1 == mesh.face_indices.len() {
            ";"
        } else {
            ","
        };
        text.push_str(&format!("    {}{}\n", material_index, suffix));
    }
    for index in 0..material_count {
        let fallback;
        let material = if let Some(material) = materials.get(material_start + index) {
            material
        } else {
            fallback = default_accessory_material();
            &fallback
        };
        export_x_material(text, material);
    }
    text.push_str("  }\n");
}

fn default_accessory_material() -> AccessoryMaterial {
    AccessoryMaterial {
        name: None,
        face_color: Some([1.0, 1.0, 1.0, 1.0]),
        power: Some(0.0),
        specular_color: Some([0.0, 0.0, 0.0]),
        emissive_color: Some([0.0, 0.0, 0.0]),
        texture_references: Vec::new(),
    }
}

fn export_x_material(text: &mut String, material: &AccessoryMaterial) {
    text.push_str("    Material");
    if let Some(name) = &material.name {
        text.push(' ');
        text.push_str(name);
    }
    text.push_str(" {\n");
    let face_color = material.face_color.unwrap_or([1.0, 1.0, 1.0, 1.0]);
    text.push_str(&format!(
        "      {};{};{};{};;\n",
        format_x_float(face_color[0]),
        format_x_float(face_color[1]),
        format_x_float(face_color[2]),
        format_x_float(face_color[3])
    ));
    text.push_str(&format!(
        "      {};\n",
        format_x_float(material.power.unwrap_or(0.0))
    ));
    let specular = material.specular_color.unwrap_or([0.0, 0.0, 0.0]);
    text.push_str(&format!(
        "      {};{};{};;\n",
        format_x_float(specular[0]),
        format_x_float(specular[1]),
        format_x_float(specular[2])
    ));
    let emissive = material.emissive_color.unwrap_or([0.0, 0.0, 0.0]);
    text.push_str(&format!(
        "      {};{};{};;\n",
        format_x_float(emissive[0]),
        format_x_float(emissive[1]),
        format_x_float(emissive[2])
    ));
    for texture in &material.texture_references {
        text.push_str("      TextureFilename { \"");
        text.push_str(&escape_x_string(texture));
        text.push_str("\"; }\n");
    }
    text.push_str("    }\n");
}

fn format_x_float(value: f32) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        value.to_string()
    }
}

fn export_vac_manifest(manifest: &AccessoryParsedManifest) -> Vec<u8> {
    let mut text = String::new();
    if let Some(settings) = &manifest.vac_settings
        && !settings.raw_lines.is_empty()
    {
        for line in &settings.raw_lines {
            text.push_str(line);
            text.push('\n');
        }
        return encode_sjis(&text);
    }
    if manifest.header.is_empty() {
        text.push_str("accessory\n");
    } else {
        text.push_str(&manifest.header);
        text.push('\n');
    }
    if let Some(settings) = &manifest.vac_settings {
        if let Some(x_file) = settings
            .x_file
            .as_ref()
            .or_else(|| manifest.texture_references.first())
            && x_file != &manifest.header
        {
            text.push_str(x_file);
            text.push('\n');
        }
        if let Some(scale) = settings.scale {
            text.push_str(&format_x_float(scale));
            text.push('\n');
        }
        if let Some(position) = settings.position {
            push_vac_vec3(&mut text, position);
        }
        if let Some(rotation) = settings.rotation {
            push_vac_vec3(&mut text, rotation);
        }
        if let Some(target) = &settings.attachment_target {
            text.push_str(target);
            text.push('\n');
        }
        return encode_sjis(&text);
    }
    for reference in &manifest.texture_references {
        if reference != &manifest.header {
            text.push_str(reference);
            text.push('\n');
        }
    }
    encode_sjis(&text)
}

fn push_vac_vec3(text: &mut String, value: [f32; 3]) {
    text.push_str(&format!(
        "{},{},{}\n",
        format_x_float(value[0]),
        format_x_float(value[1]),
        format_x_float(value[2])
    ));
}

fn parse_vac_settings(lines: &[String], texture_references: &[String]) -> AccessoryVacSettings {
    let x_file = texture_references.first().cloned();
    let first_x_index = lines
        .iter()
        .position(|line| line.to_ascii_lowercase().ends_with(".x"));
    let numeric_values = lines
        .iter()
        .flat_map(|line| parse_vac_numeric_values(line))
        .collect::<Vec<_>>();
    let attachment_target = first_x_index.and_then(|index| {
        lines
            .iter()
            .skip(index + 1)
            .find(|line| {
                !line.to_ascii_lowercase().ends_with(".x")
                    && !is_vac_numeric_line(line)
                    && !is_vac_comment_line(line)
            })
            .cloned()
    });

    AccessoryVacSettings {
        raw_lines: lines.to_vec(),
        x_file,
        scale: first_x_index
            .and_then(|index| lines.get(index + 1))
            .and_then(|line| parse_vac_scalar(line)),
        position: first_x_index
            .and_then(|index| lines.get(index + 2))
            .and_then(|line| parse_vac_vec3(line)),
        rotation: first_x_index
            .and_then(|index| lines.get(index + 3))
            .and_then(|line| parse_vac_vec3(line)),
        numeric_values,
        attachment_target,
    }
}

fn is_vac_comment_line(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

fn is_vac_numeric_line(line: &str) -> bool {
    !parse_vac_numeric_values(line).is_empty()
}

fn parse_vac_scalar(line: &str) -> Option<f32> {
    let values = parse_vac_numeric_values(line);
    if values.len() == 1 {
        Some(values[0])
    } else {
        None
    }
}

fn parse_vac_vec3(line: &str) -> Option<[f32; 3]> {
    let values = parse_vac_numeric_values(line);
    if values.len() == 3 {
        Some([values[0], values[1], values[2]])
    } else {
        None
    }
}

fn parse_vac_numeric_values(line: &str) -> Vec<f32> {
    if is_vac_comment_line(line) {
        return Vec::new();
    }
    let values = line
        .split(',')
        .map(str::trim)
        .map(str::parse::<f32>)
        .collect::<Result<Vec<_>, _>>();
    values.unwrap_or_default()
}

fn escape_x_string(value: &str) -> String {
    value.replace('"', "\\\"")
}

fn count_x_blocks(text: &str, keyword: &str) -> usize {
    text.lines()
        .map(str::trim_start)
        .filter(|line| !line.starts_with("template "))
        .filter(|line| {
            let Some(rest) = line.strip_prefix(keyword) else {
                return false;
            };
            rest.chars()
                .next()
                .is_some_and(|ch| ch.is_whitespace() || ch == '{')
        })
        .count()
}

fn parse_x_mesh_summaries(text: &str) -> Vec<AccessoryMeshSummary> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut summaries = Vec::new();
    let mut index = 0usize;
    let mut material_start_index = 0usize;
    while index < lines.len() {
        let line = lines[index].trim_start();
        if line.starts_with("template ") || !is_x_block_start(line, "Mesh") {
            index += 1;
            continue;
        }
        let (block, next_index) = collect_x_block(&lines, index);
        index = next_index;
        if let Some(mut summary) = parse_x_mesh_summary_block(&block) {
            summary.material_start_index = material_start_index;
            material_start_index += summary.material_count;
            summaries.push(summary);
        }
    }
    summaries
}

fn parse_x_mesh_summary_block(block: &str) -> Option<AccessoryMeshSummary> {
    let lines = block.lines().collect::<Vec<_>>();
    let (vertex_count, mut index) = next_usize_line(&lines, 1)?;
    let mut positions = Vec::with_capacity(vertex_count);
    for _ in 0..vertex_count {
        index = next_non_empty_after(&lines, index);
        if index >= lines.len() {
            break;
        }
        if let Some(pos) = parse_x_position(lines[index]) {
            positions.push(pos);
        }
        index += 1;
    }
    let (face_count, next_index) = next_usize_line(&lines, index)?;
    index = next_index;
    let mut face_indices = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        index = next_non_empty_after(&lines, index);
        if index >= lines.len() {
            break;
        }
        if let Some(fi) = parse_x_face_indices(lines[index]) {
            face_indices.push(fi);
        }
        index += 1;
    }
    let mut normals = Vec::new();
    let mut normal_face_indices = Vec::new();
    let mut texture_coordinates = Vec::new();
    let mut vertex_colors = Vec::new();
    let mut material_indices = Vec::new();
    let mut material_count = 0usize;
    while index < lines.len() {
        let line = lines[index].trim_start();
        if is_x_block_start(line, "MeshNormals") {
            let (parsed_normals, parsed_faces, next_index) = parse_x_mesh_normals(&lines, index);
            normals = parsed_normals;
            normal_face_indices = parsed_faces;
            index = next_index;
            continue;
        }
        if is_x_block_start(line, "MeshTextureCoords") {
            let (coords, next_index) = parse_x_mesh_texture_coords(&lines, index);
            texture_coordinates = coords;
            index = next_index;
            continue;
        }
        if is_x_block_start(line, "MeshVertexColors") {
            let (colors, next_index) = parse_x_mesh_vertex_colors(&lines, index);
            vertex_colors = colors;
            index = next_index;
            continue;
        }
        if is_x_block_start(line, "MeshMaterialList") {
            let (count, indices, next_index) = parse_x_mesh_material_list(&lines, index);
            material_count = count;
            material_indices = indices;
            index = next_index;
            continue;
        }
        index += 1;
    }
    Some(AccessoryMeshSummary {
        vertex_count,
        face_count,
        positions,
        face_indices,
        normals,
        normal_face_indices,
        texture_coordinates,
        vertex_colors,
        material_indices,
        material_start_index: 0,
        material_count,
    })
}

fn parse_x_position(line: &str) -> Option<[f32; 3]> {
    // Format: "x;y;z;[,|;]"
    let parts: Vec<&str> = line.trim().split(';').collect();
    if parts.len() < 3 {
        return None;
    }
    let x = parts[0].trim().parse::<f32>().ok()?;
    let y = parts[1].trim().parse::<f32>().ok()?;
    let z = parts[2].trim().parse::<f32>().ok()?;
    Some([x, y, z])
}

fn parse_x_face_indices(line: &str) -> Option<Vec<u32>> {
    // Format: "<nVerts>;<i0>,<i1>,...;<,|;>"
    let (count_part, rest) = line.trim().split_once(';')?;
    let _count: usize = count_part.trim().parse().ok()?;
    let indices_str = rest.split(';').next().unwrap_or("").trim();
    let indices = indices_str
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();
    Some(indices)
}

fn parse_x_mesh_normals(lines: &[&str], start: usize) -> (Vec<[f32; 3]>, Vec<Vec<u32>>, usize) {
    let (block, next_index) = collect_x_block(lines, start);
    let block_lines = block.lines().collect::<Vec<_>>();
    let Some((normal_count, mut index)) = next_usize_line(&block_lines, 1) else {
        return (Vec::new(), Vec::new(), next_index);
    };
    let mut normals = Vec::with_capacity(normal_count);
    for _ in 0..normal_count {
        index = next_non_empty_after(&block_lines, index);
        if index >= block_lines.len() {
            break;
        }
        if let Some(normal) = parse_x_position(block_lines[index]) {
            normals.push(normal);
        }
        index += 1;
    }
    let Some((face_count, next_line)) = next_usize_line(&block_lines, index) else {
        return (normals, Vec::new(), next_index);
    };
    index = next_line;
    let mut face_indices = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        index = next_non_empty_after(&block_lines, index);
        if index >= block_lines.len() {
            break;
        }
        if let Some(face) = parse_x_face_indices(block_lines[index]) {
            face_indices.push(face);
        }
        index += 1;
    }
    (normals, face_indices, next_index)
}

fn parse_x_mesh_texture_coords(lines: &[&str], start: usize) -> (Vec<[f32; 2]>, usize) {
    let (block, next_index) = collect_x_block(lines, start);
    let block_lines = block.lines().collect::<Vec<_>>();
    let Some((coord_count, mut index)) = next_usize_line(&block_lines, 1) else {
        return (Vec::new(), next_index);
    };
    let mut coords = Vec::with_capacity(coord_count);
    for _ in 0..coord_count {
        index = next_non_empty_after(&block_lines, index);
        if index >= block_lines.len() {
            break;
        }
        if let Some(coord) = parse_x_texture_coordinate(block_lines[index]) {
            coords.push(coord);
        }
        index += 1;
    }
    (coords, next_index)
}

fn parse_x_mesh_vertex_colors(lines: &[&str], start: usize) -> (Vec<AccessoryVertexColor>, usize) {
    let (block, next_index) = collect_x_block(lines, start);
    let block_lines = block.lines().collect::<Vec<_>>();
    let Some((color_count, mut index)) = next_usize_line(&block_lines, 1) else {
        return (Vec::new(), next_index);
    };
    let mut colors = Vec::with_capacity(color_count);
    for _ in 0..color_count {
        index = next_non_empty_after(&block_lines, index);
        if index >= block_lines.len() {
            break;
        }
        if let Some(color) = parse_x_vertex_color(block_lines[index]) {
            colors.push(color);
        }
        index += 1;
    }
    (colors, next_index)
}

fn parse_x_vertex_color(line: &str) -> Option<AccessoryVertexColor> {
    // Format: "vertexIndex;r;g;b;a;[,|;;]"
    let parts: Vec<&str> = line.trim().split(';').collect();
    if parts.len() < 5 {
        return None;
    }
    let vertex_index = parts[0].trim().parse::<u32>().ok()?;
    let r = parts[1].trim().parse::<f32>().ok()?;
    let g = parts[2].trim().parse::<f32>().ok()?;
    let b = parts[3].trim().parse::<f32>().ok()?;
    let a = parts[4].trim().parse::<f32>().ok()?;
    Some(AccessoryVertexColor {
        vertex_index,
        color: [r, g, b, a],
    })
}

fn parse_x_texture_coordinate(line: &str) -> Option<[f32; 2]> {
    let parts: Vec<&str> = line.trim().split(';').collect();
    if parts.len() < 2 {
        return None;
    }
    let u = parts[0].trim().parse::<f32>().ok()?;
    let v = parts[1].trim().parse::<f32>().ok()?;
    Some([u, v])
}

fn parse_x_mesh_material_list(lines: &[&str], start: usize) -> (usize, Vec<u32>, usize) {
    let (block, next_index) = collect_x_block(lines, start);
    let block_lines = block.lines().collect::<Vec<_>>();
    let Some((material_count, index)) = next_usize_line(&block_lines, 1) else {
        return (0, Vec::new(), next_index);
    };
    let Some((face_count, mut index)) = next_usize_line(&block_lines, index) else {
        return (material_count, Vec::new(), next_index);
    };
    let mut indices = Vec::with_capacity(face_count);
    for _ in 0..face_count {
        let Some((material_index, next_line)) = next_usize_line(&block_lines, index) else {
            break;
        };
        indices.push(material_index as u32);
        index = next_line;
    }
    (material_count, indices, next_index)
}

fn parse_x_materials(text: &str) -> Vec<AccessoryMaterial> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut materials = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index].trim_start();
        if line.starts_with("template ") || !is_x_block_start(line, "Material") {
            index += 1;
            continue;
        }
        let (block, next_index) = collect_x_block(&lines, index);
        index = next_index;
        if let Some(material) = parse_x_material_block(&block) {
            materials.push(material);
        }
    }
    materials
}

fn collect_x_block(lines: &[&str], start: usize) -> (String, usize) {
    let mut block = String::new();
    let mut depth = 0i32;
    let mut saw_open = false;
    let mut index = start;
    while index < lines.len() {
        let line = lines[index];
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    saw_open = true;
                }
                '}' => {
                    depth -= 1;
                }
                _ => {}
            }
        }
        block.push_str(line);
        block.push('\n');
        index += 1;
        if saw_open && depth <= 0 {
            break;
        }
    }
    (block, index)
}

fn parse_x_material_block(block: &str) -> Option<AccessoryMaterial> {
    let header = block
        .split_once('{')
        .map(|(header, _)| header.trim())
        .unwrap_or_default();
    let name = header
        .strip_prefix("Material")
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned);
    let content = block.split_once('{')?.1.rsplit_once('}')?.0;
    let numeric_values = parse_x_numeric_values(content);

    Some(AccessoryMaterial {
        name,
        face_color: numeric_values
            .get(0..4)
            .and_then(|values| values.try_into().ok()),
        power: numeric_values.get(4).copied(),
        specular_color: numeric_values
            .get(5..8)
            .and_then(|values| values.try_into().ok()),
        emissive_color: numeric_values
            .get(8..11)
            .and_then(|values| values.try_into().ok()),
        texture_references: extract_texture_references(content),
    })
}

fn parse_x_numeric_values(text: &str) -> Vec<f32> {
    text.split(|ch: char| ch.is_whitespace() || ch == ';' || ch == ',' || ch == '{' || ch == '}')
        .filter_map(|part| part.trim().parse::<f32>().ok())
        .collect()
}

fn next_usize_line(lines: &[&str], mut index: usize) -> Option<(usize, usize)> {
    index = next_non_empty_after(lines, index);
    if index >= lines.len() {
        return None;
    }
    parse_leading_usize(lines[index]).map(|value| (value, index + 1))
}

fn next_non_empty_after(lines: &[&str], mut index: usize) -> usize {
    while index < lines.len() && lines[index].trim().is_empty() {
        index += 1;
    }
    index
}

fn parse_leading_usize(line: &str) -> Option<usize> {
    line.trim()
        .split_once([';', ','])
        .map(|(value, _)| value.trim())
        .and_then(|value| value.parse().ok())
}

fn is_x_block_start(line: &str, keyword: &str) -> bool {
    let Some(rest) = line.strip_prefix(keyword) else {
        return false;
    };
    rest.chars()
        .next()
        .is_some_and(|ch| ch.is_whitespace() || ch == '{')
}

fn extract_texture_references(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut unquoted_text = String::with_capacity(text.len());
    let mut quoted = String::new();
    let mut in_quote = false;
    let mut quote = '\0';
    let mut escaped = false;
    for ch in text.chars() {
        if in_quote {
            unquoted_text.push(' ');
            if escaped {
                if ch == quote {
                    quoted.push(ch);
                } else {
                    quoted.push('\\');
                    quoted.push(ch);
                }
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                add_texture_reference(&mut values, &quoted);
                quoted.clear();
                in_quote = false;
            } else {
                quoted.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            unquoted_text.push(' ');
            in_quote = true;
            quote = ch;
            quoted.clear();
            escaped = false;
        } else {
            unquoted_text.push(ch);
        }
    }

    for token in unquoted_text.split(|c: char| c.is_whitespace() || c == ';' || c == ',') {
        let cleaned = token.trim_matches(|c: char| c == '"' || c == '\'');
        add_texture_reference(&mut values, cleaned);
    }
    values
}

fn add_texture_reference(values: &mut Vec<String>, candidate: &str) {
    let lower = candidate.to_ascii_lowercase();
    if [".bmp", ".png", ".jpg", ".jpeg", ".tga", ".dds"]
        .iter()
        .any(|ext| lower.ends_with(ext))
        && !values
            .iter()
            .any(|value: &String| value.eq_ignore_ascii_case(candidate))
    {
        values.push(candidate.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn exports_text_x_manifest_texture_references() {
        let data = br#"xof 0303txt 0032
TextureFilename { "tex/main.png"; }
TextureFilename { "tex/sub.tga"; }
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        let exported = export_accessory_manifest(&parsed);
        let reparsed = parse_accessory_manifest(&exported, Some("stage.x")).unwrap();

        assert_eq!(reparsed.format, "x");
        assert!(reparsed.text);
        assert_eq!(reparsed.header, "xof 0303txt 0032");
        assert_eq!(parsed.mesh_count, 0);
        assert_eq!(parsed.material_count, 0);
        assert_eq!(reparsed.texture_references, parsed.texture_references);
        assert!(reparsed.diagnostics.is_empty());
    }

    #[test]
    fn accessory_mesh_summary_json_schema_is_stable() {
        let summary = AccessoryMeshSummary {
            vertex_count: 1,
            face_count: 1,
            positions: vec![[0.0, 0.0, 0.0]],
            face_indices: vec![vec![0u32]],
            normals: Vec::new(),
            normal_face_indices: Vec::new(),
            texture_coordinates: Vec::new(),
            vertex_colors: Vec::new(),
            material_indices: vec![0],
            material_start_index: 0,
            material_count: 1,
        };
        let keys = json_keys(&serde_json::to_value(&summary).unwrap());
        assert_eq!(
            keys,
            vec![
                "faceCount",
                "faceIndices",
                "materialCount",
                "materialIndices",
                "materialStartIndex",
                "normalFaceIndices",
                "normals",
                "positions",
                "textureCoordinates",
                "vertexColors",
                "vertexCount",
            ]
        );
    }

    #[test]
    fn accessory_vertex_color_json_schema_is_stable() {
        let vertex_color = AccessoryVertexColor {
            vertex_index: 2,
            color: [1.0, 0.5, 0.25, 1.0],
        };
        let keys = json_keys(&serde_json::to_value(&vertex_color).unwrap());
        assert_eq!(keys, vec!["color", "vertexIndex"]);
    }

    #[test]
    fn accessory_material_json_schema_is_stable() {
        let material = AccessoryMaterial {
            name: Some("mat".to_owned()),
            face_color: Some([1.0, 0.5, 0.25, 1.0]),
            power: Some(32.0),
            specular_color: Some([0.1, 0.2, 0.3]),
            emissive_color: Some([0.0, 0.0, 0.0]),
            texture_references: vec!["tex.png".to_owned()],
        };
        let keys = json_keys(&serde_json::to_value(&material).unwrap());
        assert_eq!(
            keys,
            vec![
                "emissiveColor",
                "faceColor",
                "name",
                "power",
                "specularColor",
                "textureReferences",
            ]
        );
    }

    #[test]
    fn accessory_vac_settings_json_schema_is_stable() {
        let settings = AccessoryVacSettings {
            raw_lines: vec!["sample".to_owned(), "model.x".to_owned()],
            x_file: Some("model.x".to_owned()),
            scale: Some(1.0),
            position: Some([0.0, 1.0, 2.0]),
            rotation: Some([10.0, 20.0, 30.0]),
            numeric_values: vec![1.0, 0.0, 1.0, 2.0, 10.0, 20.0, 30.0],
            attachment_target: Some("右手首".to_owned()),
        };
        let keys = json_keys(&serde_json::to_value(&settings).unwrap());
        assert_eq!(
            keys,
            vec![
                "attachmentTarget",
                "numericValues",
                "position",
                "rawLines",
                "rotation",
                "scale",
                "xFile",
            ]
        );
    }

    #[test]
    fn parses_text_x_mesh_vertex_positions() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0.0;0.0;0.0;,
  1.0;0.0;0.0;,
  0.0;1.0;0.0;;
  1;
  3;0,1,2;;
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        assert_eq!(parsed.mesh_summaries.len(), 1);
        let summary = &parsed.mesh_summaries[0];
        assert_eq!(summary.vertex_count, 3);
        assert_eq!(summary.positions.len(), 3);
        assert_eq!(summary.positions[0], [0.0f32, 0.0, 0.0]);
        assert_eq!(summary.positions[1], [1.0f32, 0.0, 0.0]);
        assert_eq!(summary.positions[2], [0.0f32, 1.0, 0.0]);
    }

    #[test]
    fn parses_text_x_mesh_face_indices() {
        let data = br#"xof 0303txt 0032
Mesh {
  4;
  0;0;0;,
  1;0;0;,
  1;1;0;,
  0;1;0;;
  2;
  3;0,1,2;;
  3;0,2,3;;
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        assert_eq!(parsed.mesh_summaries.len(), 1);
        let summary = &parsed.mesh_summaries[0];
        assert_eq!(summary.face_count, 2);
        assert_eq!(summary.face_indices.len(), 2);
        assert_eq!(summary.face_indices[0], vec![0u32, 1, 2]);
        assert_eq!(summary.face_indices[1], vec![0u32, 2, 3]);
    }

    #[test]
    fn parses_text_x_mesh_texture_coordinates() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshNormals {
    1;
    0;0;1;;
    1;
    3;0,0,0;;
  }
  MeshTextureCoords {
    3;
    0;0;,
    1;0;,
    0;1;;
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert_eq!(parsed.mesh_summaries.len(), 1);
        assert_eq!(
            parsed.mesh_summaries[0].texture_coordinates,
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
        );
    }

    #[test]
    fn parses_text_x_mesh_normals() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshNormals {
    1;
    0;0;1;;
    1;
    3;0,0,0;;
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert_eq!(parsed.mesh_summaries.len(), 1);
        assert_eq!(parsed.mesh_summaries[0].normals, vec![[0.0, 0.0, 1.0]]);
        assert_eq!(
            parsed.mesh_summaries[0].normal_face_indices,
            vec![vec![0, 0, 0]]
        );
    }

    #[test]
    fn parses_text_x_mesh_material_indices() {
        let data = br#"xof 0303txt 0032
Mesh {
  4;
  0;0;0;,
  1;0;0;,
  1;1;0;,
  0;1;0;;
  2;
  3;0,1,2;,
  3;0,2,3;;
  MeshMaterialList {
    2;
    2;
    0,
    1;
    Material { 1;1;1;1;; 5; 0;0;0;; 0;0;0;; }
    Material { 0;0;0;1;; 5; 0;0;0;; 0;0;0;; }
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        assert_eq!(parsed.mesh_summaries.len(), 1);
        assert_eq!(parsed.mesh_summaries[0].material_indices, vec![0, 1]);
        assert_eq!(parsed.materials.len(), 2);
    }

    #[test]
    fn parses_text_x_mesh_material_indices_after_child_blocks() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshNormals {
    1;
    0;0;1;;
    1;
    3;0,0,0;;
  }
  MeshMaterialList {
    1;
    1;
    0;
    Material { 1;1;1;1;; 5; 0;0;0;; 0;0;0;; }
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert_eq!(parsed.mesh_summaries.len(), 1);
        assert_eq!(parsed.mesh_summaries[0].material_indices, vec![0]);
        assert_eq!(parsed.materials.len(), 1);
    }

    #[test]
    fn parses_text_x_mesh_and_material_counts() {
        let data = br#"xof 0303txt 0032
template Mesh { <template-guid> }
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshMaterialList {
    2;
    1;
    0;
    Material { 1.0;1.0;1.0;1.0;; 1.0; 0.0;0.0;0.0;; 0.0;0.0;0.0;; }
    Material namedMat { 0.5;0.5;0.5;1.0;; 1.0; 0.0;0.0;0.0;; 0.0;0.0;0.0;; }
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert_eq!(parsed.mesh_count, 1);
        assert_eq!(parsed.material_count, 2);
        assert_eq!(parsed.mesh_summaries.len(), 1);
        assert_eq!(parsed.mesh_summaries[0].vertex_count, 3);
        assert_eq!(parsed.mesh_summaries[0].face_count, 1);
        assert_eq!(parsed.mesh_summaries[0].material_indices, vec![0]);
        assert_eq!(parsed.materials.len(), 2);
        assert_eq!(parsed.materials[0].name, None);
        assert_eq!(parsed.materials[0].face_color, Some([1.0, 1.0, 1.0, 1.0]));
        assert_eq!(parsed.materials[1].name, Some("namedMat".to_owned()));
        assert_eq!(parsed.materials[1].face_color, Some([0.5, 0.5, 0.5, 1.0]));
    }

    #[test]
    fn exports_text_x_mesh_material_roundtrip() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshNormals {
    1;
    0;0;1;;
    1;
    3;0,0,0;;
  }
  MeshTextureCoords {
    3;
    0;0;,
    1;0;,
    0;1;;
  }
  MeshVertexColors {
    3;
    2;1.0;0.5;0.25;1.0;,
    0;0.0;1.0;0.0;0.75;,
    1;0.0;0.0;1.0;0.5;;
  }
  MeshMaterialList {
    1;
    1;
    0;
    Material namedMat {
      0.5;0.25;0.75;1.0;;
      8.0;
      0.1;0.2;0.3;;
      0.0;0.0;0.0;;
      TextureFilename { "mesh.png"; }
    }
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        let exported = export_accessory_manifest(&parsed);
        let reparsed = parse_accessory_manifest(&exported, Some("stage.x")).unwrap();

        assert_eq!(reparsed.mesh_count, 1);
        assert_eq!(reparsed.material_count, 1);
        assert_eq!(reparsed.mesh_summaries.len(), 1);
        assert_eq!(
            reparsed.mesh_summaries[0].positions,
            parsed.mesh_summaries[0].positions
        );
        assert_eq!(
            reparsed.mesh_summaries[0].face_indices,
            parsed.mesh_summaries[0].face_indices
        );
        assert_eq!(reparsed.mesh_summaries[0].normals, vec![[0.0, 0.0, 1.0]]);
        assert_eq!(
            reparsed.mesh_summaries[0].normal_face_indices,
            vec![vec![0, 0, 0]]
        );
        assert_eq!(
            reparsed.mesh_summaries[0].texture_coordinates,
            vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]
        );
        assert_eq!(
            reparsed.mesh_summaries[0].vertex_colors,
            parsed.mesh_summaries[0].vertex_colors
        );
        assert_eq!(reparsed.mesh_summaries[0].vertex_colors[0].vertex_index, 2);
        assert_eq!(
            reparsed.mesh_summaries[0].vertex_colors[0].color,
            [1.0, 0.5, 0.25, 1.0]
        );
        assert_eq!(reparsed.mesh_summaries[0].material_indices, vec![0]);
        assert_eq!(reparsed.materials.len(), 1);
        assert_eq!(reparsed.materials[0].name, Some("namedMat".to_owned()));
        assert_eq!(
            reparsed.materials[0].face_color,
            Some([0.5, 0.25, 0.75, 1.0])
        );
        assert_eq!(reparsed.materials[0].power, Some(8.0));
        assert_eq!(reparsed.materials[0].texture_references, vec!["mesh.png"]);
        assert_eq!(reparsed.texture_references, vec!["mesh.png"]);
    }

    #[test]
    fn tracks_text_x_multi_mesh_material_ownership() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshMaterialList {
    1;
    1;
    0;
    Material { 1;1;1;1;; 5; 0;0;0;; 0;0;0;; }
  }
}
Mesh {
  3;
  0;0;1;,
  1;0;1;,
  0;1;1;;
  1;
  3;0,2,1;;
  MeshMaterialList {
    1;
    1;
    0;
    Material { 0;0;0;1;; 5; 0;0;0;; 0;0;0;; }
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert_eq!(parsed.mesh_summaries.len(), 2);
        assert_eq!(parsed.materials.len(), 2);
        assert!(parsed.diagnostics.is_empty());
        assert_eq!(parsed.mesh_summaries[0].material_start_index, 0);
        assert_eq!(parsed.mesh_summaries[0].material_count, 1);
        assert_eq!(parsed.mesh_summaries[1].material_start_index, 1);
        assert_eq!(parsed.mesh_summaries[1].material_count, 1);
    }

    #[test]
    fn exports_text_x_multiple_mesh_material_indices_roundtrip() {
        let manifest = AccessoryParsedManifest {
            format: "x".to_owned(),
            byte_length: 0,
            text: true,
            header: "xof 0303txt 0032".to_owned(),
            mesh_count: 2,
            material_count: 1,
            mesh_summaries: vec![
                AccessoryMeshSummary {
                    vertex_count: 3,
                    face_count: 1,
                    positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    face_indices: vec![vec![0, 1, 2]],
                    normals: vec![[0.0, 0.0, 1.0]],
                    normal_face_indices: vec![vec![0, 0, 0]],
                    texture_coordinates: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
                    vertex_colors: Vec::new(),
                    material_indices: vec![0],
                    material_start_index: 0,
                    material_count: 1,
                },
                AccessoryMeshSummary {
                    vertex_count: 3,
                    face_count: 1,
                    positions: vec![[0.0, 0.0, 1.0], [1.0, 0.0, 1.0], [0.0, 1.0, 1.0]],
                    face_indices: vec![vec![0, 2, 1]],
                    normals: vec![[0.0, 0.0, -1.0]],
                    normal_face_indices: vec![vec![0, 0, 0]],
                    texture_coordinates: vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0]],
                    vertex_colors: Vec::new(),
                    material_indices: vec![0],
                    material_start_index: 1,
                    material_count: 1,
                },
            ],
            materials: vec![
                AccessoryMaterial {
                    name: Some("sharedMat".to_owned()),
                    face_color: Some([1.0, 1.0, 1.0, 1.0]),
                    power: Some(4.0),
                    specular_color: Some([0.0, 0.0, 0.0]),
                    emissive_color: Some([0.0, 0.0, 0.0]),
                    texture_references: Vec::new(),
                },
                AccessoryMaterial {
                    name: Some("secondMat".to_owned()),
                    face_color: Some([0.5, 0.5, 0.5, 1.0]),
                    power: Some(2.0),
                    specular_color: Some([0.0, 0.0, 0.0]),
                    emissive_color: Some([0.0, 0.0, 0.0]),
                    texture_references: Vec::new(),
                },
            ],
            vac_settings: None,
            texture_references: Vec::new(),
            diagnostics: Vec::new(),
        };

        let exported = export_accessory_manifest(&manifest);
        let reparsed = parse_accessory_manifest(&exported, Some("stage.x")).unwrap();

        assert_eq!(reparsed.mesh_summaries.len(), 2);
        assert_eq!(reparsed.mesh_summaries[0].material_indices, vec![0]);
        assert_eq!(reparsed.mesh_summaries[1].material_indices, vec![0]);
        assert_eq!(reparsed.mesh_summaries[0].material_start_index, 0);
        assert_eq!(reparsed.mesh_summaries[0].material_count, 1);
        assert_eq!(reparsed.mesh_summaries[1].material_start_index, 1);
        assert_eq!(reparsed.mesh_summaries[1].material_count, 1);
        assert_eq!(reparsed.mesh_summaries[1].face_indices, vec![vec![0, 2, 1]]);
        assert_eq!(reparsed.mesh_summaries[0].normals, vec![[0.0, 0.0, 1.0]]);
        assert_eq!(reparsed.mesh_summaries[1].normals, vec![[0.0, 0.0, -1.0]]);
        assert_eq!(
            reparsed.mesh_summaries[0].normal_face_indices,
            vec![vec![0, 0, 0]]
        );
        assert_eq!(
            reparsed.mesh_summaries[1].texture_coordinates,
            vec![[0.0, 0.0], [0.0, 1.0], [1.0, 0.0]]
        );
        assert_eq!(reparsed.materials.len(), 2);
        assert_eq!(reparsed.materials[0].name, Some("sharedMat".to_owned()));
        assert_eq!(reparsed.materials[1].name, Some("secondMat".to_owned()));
    }

    #[test]
    fn parses_text_x_material_texture_reference() {
        let data = br#"xof 0303txt 0032
Material screenMat {
  1.0;0.5;0.25;1.0;;
  32.0;
  0.1;0.2;0.3;;
  0.0;0.0;0.0;;
  TextureFilename { "screen.png"; }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert_eq!(parsed.material_count, 1);
        assert_eq!(parsed.materials.len(), 1);
        assert_eq!(parsed.materials[0].name, Some("screenMat".to_owned()));
        assert_eq!(parsed.materials[0].face_color, Some([1.0, 0.5, 0.25, 1.0]));
        assert_eq!(parsed.materials[0].power, Some(32.0));
        assert_eq!(parsed.materials[0].specular_color, Some([0.1, 0.2, 0.3]));
        assert_eq!(parsed.materials[0].emissive_color, Some([0.0, 0.0, 0.0]));
        assert_eq!(parsed.materials[0].texture_references, vec!["screen.png"]);
    }

    #[test]
    fn parses_single_line_x_material() {
        let data = br#"xof 0303txt 0032
Material namedMat { 0.5;0.5;0.5;1.0;; 8.0; 0.0;0.0;0.0;; 0.1;0.2;0.3;; }
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert_eq!(parsed.materials.len(), 1);
        assert_eq!(parsed.materials[0].name, Some("namedMat".to_owned()));
        assert_eq!(parsed.materials[0].power, Some(8.0));
        assert_eq!(parsed.materials[0].emissive_color, Some([0.1, 0.2, 0.3]));
    }

    #[test]
    fn parses_text_x_mesh_vertex_colors() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshVertexColors {
    2;
    2;1.0;1.0;1.0;1.0;,
    0;0.5;0.5;0.5;1.0;;
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        assert_eq!(parsed.mesh_summaries.len(), 1);
        let summary = &parsed.mesh_summaries[0];
        assert_eq!(summary.vertex_colors.len(), 2);
        assert_eq!(summary.vertex_colors[0].vertex_index, 2);
        assert_eq!(summary.vertex_colors[0].color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(summary.vertex_colors[1].vertex_index, 0);
        assert_eq!(summary.vertex_colors[1].color, [0.5, 0.5, 0.5, 1.0]);
    }

    #[test]
    fn roundtrip_text_x_mesh_vertex_colors() {
        let data = br#"xof 0303txt 0032
Mesh {
  3;
  0;0;0;,
  1;0;0;,
  0;1;0;;
  1;
  3;0,1,2;;
  MeshVertexColors {
    3;
    2;1.0;0.5;0.25;1.0;,
    0;0.0;1.0;0.0;0.75;,
    1;0.0;0.0;1.0;0.5;;
  }
}
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        let exported = export_accessory_manifest(&parsed);
        let reparsed = parse_accessory_manifest(&exported, Some("stage.x")).unwrap();

        assert_eq!(reparsed.mesh_summaries.len(), 1);
        assert_eq!(
            reparsed.mesh_summaries[0].vertex_colors,
            parsed.mesh_summaries[0].vertex_colors
        );
        assert_eq!(reparsed.mesh_summaries[0].vertex_colors.len(), 3);
        assert_eq!(reparsed.mesh_summaries[0].vertex_colors[0].vertex_index, 2);
        assert_eq!(
            reparsed.mesh_summaries[0].vertex_colors[0].color,
            [1.0, 0.5, 0.25, 1.0]
        );
        assert_eq!(reparsed.mesh_summaries[0].vertex_colors[1].vertex_index, 0);
        assert_eq!(
            reparsed.mesh_summaries[0].vertex_colors[1].color,
            [0.0, 1.0, 0.0, 0.75]
        );
        assert_eq!(reparsed.mesh_summaries[0].vertex_colors[2].vertex_index, 1);
        assert_eq!(
            reparsed.mesh_summaries[0].vertex_colors[2].color,
            [0.0, 0.0, 1.0, 0.5]
        );
    }

    #[test]
    fn binary_x_is_diagnostic_only() {
        let data = b"xof 0303bin 0032\x01\x02\x03\x04";
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();

        assert!(!parsed.text);
        assert_eq!(parsed.mesh_count, 0);
        assert_eq!(parsed.material_count, 0);
        assert!(parsed.mesh_summaries.is_empty());
        assert!(parsed.materials.is_empty());
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].code, "X_BINARY_LAYOUT_NOT_EXPANDED");
    }

    #[test]
    fn exports_text_x_manifest_preserves_windows_texture_paths() {
        let data = br#"xof 0302txt 0064
TextureFilename { "D:\\MikuMikuDance_v932x64\\UserFile\\Accessory\\screen.png"; }
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        let exported = export_accessory_manifest(&parsed);
        let reparsed = parse_accessory_manifest(&exported, Some("stage.x")).unwrap();

        assert_eq!(reparsed.header, "xof 0302txt 0064");
        assert_eq!(reparsed.texture_references, parsed.texture_references);
    }

    #[test]
    fn exports_text_x_manifest_preserves_quoted_texture_paths_with_separators() {
        let data = br#"xof 0303txt 0032
TextureFilename { "C:\My Files\tex,main;01.png"; }
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        let exported = export_accessory_manifest(&parsed);
        let reparsed = parse_accessory_manifest(&exported, Some("stage.x")).unwrap();

        assert_eq!(
            parsed.texture_references,
            vec!["C:\\My Files\\tex,main;01.png"]
        );
        assert_eq!(reparsed.texture_references, parsed.texture_references);
    }

    #[test]
    fn exports_vac_manifest_references() {
        let data = "sample accessory\r\nmodel.x\r\n".as_bytes();
        let parsed = parse_accessory_manifest(data, Some("model.vac")).unwrap();
        let exported = export_accessory_manifest(&parsed);
        let reparsed = parse_accessory_manifest(&exported, Some("model.vac")).unwrap();

        assert_eq!(reparsed.format, "vac");
        assert_eq!(reparsed.header, "sample accessory");
        assert_eq!(reparsed.texture_references, vec!["model.x"]);
        assert_eq!(reparsed.diagnostics[0].code, "VAC_ACCESSORY_WRAPPER");
    }

    #[test]
    fn exports_vac_manifest_preserves_raw_display_and_attachment_lines() {
        let data =
            encode_sjis("sample accessory\r\nmodel.x\r\n1.5\r\n0,1,2\r\n10,20,30\r\n右手首\r\n");
        let parsed = parse_accessory_manifest(&data, Some("model.vac")).unwrap();
        let settings = parsed.vac_settings.as_ref().unwrap();

        assert_eq!(settings.x_file, Some("model.x".to_owned()));
        assert_eq!(settings.scale, Some(1.5));
        assert_eq!(settings.position, Some([0.0, 1.0, 2.0]));
        assert_eq!(settings.rotation, Some([10.0, 20.0, 30.0]));
        assert_eq!(
            settings.numeric_values,
            vec![1.5, 0.0, 1.0, 2.0, 10.0, 20.0, 30.0]
        );
        assert_eq!(settings.attachment_target, Some("右手首".to_owned()));

        let exported = export_accessory_manifest(&parsed);
        let reparsed = parse_accessory_manifest(&exported, Some("model.vac")).unwrap();

        assert_eq!(reparsed.texture_references, vec!["model.x"]);
        assert_eq!(
            reparsed.vac_settings.unwrap().raw_lines,
            vec![
                "sample accessory",
                "model.x",
                "1.5",
                "0,1,2",
                "10,20,30",
                "右手首",
            ]
        );
    }

    #[test]
    fn exports_vac_manifest_from_semantic_settings_without_raw_lines() {
        let manifest = AccessoryParsedManifest {
            format: "vac".to_owned(),
            byte_length: 0,
            text: true,
            header: "sample accessory".to_owned(),
            mesh_count: 0,
            material_count: 0,
            mesh_summaries: Vec::new(),
            materials: Vec::new(),
            vac_settings: Some(AccessoryVacSettings {
                raw_lines: Vec::new(),
                x_file: Some("model.x".to_owned()),
                scale: Some(1.5),
                position: Some([0.0, 1.0, 2.0]),
                rotation: Some([10.0, 20.0, 30.0]),
                numeric_values: Vec::new(),
                attachment_target: Some("右手首".to_owned()),
            }),
            texture_references: Vec::new(),
            diagnostics: Vec::new(),
        };

        let exported = export_accessory_manifest(&manifest);
        let reparsed = parse_accessory_manifest(&exported, Some("model.vac")).unwrap();
        let settings = reparsed.vac_settings.as_ref().unwrap();

        assert_eq!(reparsed.header, "sample accessory");
        assert_eq!(reparsed.texture_references, vec!["model.x"]);
        assert_eq!(settings.scale, Some(1.5));
        assert_eq!(settings.position, Some([0.0, 1.0, 2.0]));
        assert_eq!(settings.rotation, Some([10.0, 20.0, 30.0]));
        assert_eq!(settings.attachment_target, Some("右手首".to_owned()));
    }

    #[test]
    fn parses_real_mmd_vac_scale_position_rotation_layout() {
        let data = encode_sjis(
            "ネギ(右手)\r\nnegi.x\r\n1.0\r\n-0.5,-1.0,0.00\r\n0.0,0.0,0.0\r\n右手首\r\n",
        );
        let parsed = parse_accessory_manifest(&data, Some("negi.vac")).unwrap();
        let settings = parsed.vac_settings.as_ref().unwrap();

        assert_eq!(parsed.header, "ネギ(右手)");
        assert_eq!(settings.x_file, Some("negi.x".to_owned()));
        assert_eq!(settings.scale, Some(1.0));
        assert_eq!(settings.position, Some([-0.5, -1.0, 0.0]));
        assert_eq!(settings.rotation, Some([0.0, 0.0, 0.0]));
        assert_eq!(settings.attachment_target, Some("右手首".to_owned()));
        assert_eq!(
            settings.numeric_values,
            vec![1.0, -0.5, -1.0, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn vac_attachment_target_ignores_comment_lines_after_numeric_fields() {
        let data = encode_sjis(
            "sample accessory\r\nmodel.x\r\n1.0\r\n0,1,2\r\n10,20,30\r\n// comment\r\n右手首\r\n",
        );
        let parsed = parse_accessory_manifest(&data, Some("model.vac")).unwrap();
        let settings = parsed.vac_settings.as_ref().unwrap();

        assert_eq!(settings.scale, Some(1.0));
        assert_eq!(settings.position, Some([0.0, 1.0, 2.0]));
        assert_eq!(settings.rotation, Some([10.0, 20.0, 30.0]));
        assert_eq!(settings.attachment_target, Some("右手首".to_owned()));
    }

    #[test]
    fn accessory_manifest_json_top_level_schema_is_stable() {
        let data = br#"xof 0303txt 0032
TextureFilename { "tex/main.png"; }
"#;
        let parsed = parse_accessory_manifest(data, Some("stage.x")).unwrap();
        let keys = json_keys(&serde_json::to_value(&parsed).unwrap());

        assert_eq!(
            keys,
            vec![
                "byteLength",
                "diagnostics",
                "format",
                "header",
                "materialCount",
                "materials",
                "meshCount",
                "meshSummaries",
                "text",
                "textureReferences",
                "vacSettings",
            ]
        );
    }
}
